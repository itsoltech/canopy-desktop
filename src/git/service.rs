//! One bounded worker, metadata-only native watches, coalesced snapshots.
use super::*;
use futures_channel::oneshot;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};
type Entry = Result<Option<Arc<RepositoryInfo>>>;
type Cache = HashMap<PathBuf, Entry>;
type ChangeCache = Arc<Mutex<HashMap<PathBuf, Result<Arc<changes::ChangesSnapshot>>>>>;
#[derive(Default)]
pub struct GitStats {
    pub scans: AtomicU64,
    pub events: AtomicU64,
    pub watch_failures: AtomicU64,
    /// Current failures, separate from the lifetime diagnostic counter.
    pub active_watch_failures: AtomicU64,
    pub status_scans: AtomicU64,
}
enum Command {
    TaskRepository(
        PathBuf,
        oneshot::Sender<Result<Option<crate::integrations::RepositoryContext>>>,
    ),
    Files(PathBuf, oneshot::Sender<Result<crate::files::Index>>),
    FileDirectory(
        PathBuf,
        PathBuf,
        usize,
        Arc<AtomicBool>,
        oneshot::Sender<Result<crate::files::DirectoryListing>>,
    ),
    FileDecorations(PathBuf, oneshot::Sender<Result<crate::files::Decorations>>),
    NetworkPlan(PathBuf, oneshot::Sender<Result<network::Plan>>),
    Network(
        network::Plan,
        network::Operation,
        Option<network::Upstream>,
        Arc<AtomicBool>,
        oneshot::Sender<Result<String>>,
    ),
    History(
        PathBuf,
        Option<Arc<history::HistorySnapshot>>,
        usize,
        oneshot::Sender<Result<history::HistoryPage>>,
    ),
    WatchChanges(Option<PathBuf>),
    ReadDiff(
        PathBuf,
        changes::FileChange,
        oneshot::Sender<Result<changes::FileDiff>>,
    ),
    Edit(PathBuf, changes::Edit, oneshot::Sender<Result<()>>),
    Commit(
        Box<CommitRequest>,
        oneshot::Sender<Result<changes::CommitOutcome>>,
    ),
    Watch(Vec<PathBuf>),
    Refresh,
    Wake,
    Create(CreateWorktree, oneshot::Sender<Result<CreatedWorktree>>),
    AnalyzeWorktree(
        RemoveWorktree,
        Option<String>,
        oneshot::Sender<Result<WorktreeAnalysis>>,
    ),
    ExecuteWorktreeRemoval(
        RemoveWorktree,
        RemovalAction,
        WorktreeAnalysis,
        RemovalApproval,
        Option<crate::terminal::environment::ShellEnvironment>,
        oneshot::Sender<Result<WorktreeRemovalOutcome>>,
    ),
    DeleteBranchAfterRemoval(PathBuf, BranchAnalysis, oneshot::Sender<Result<()>>),
    Remove(
        RemoveWorktree,
        bool,
        oneshot::Sender<Result<RemovalOutcome>>,
    ),
    Shutdown(oneshot::Sender<()>),
}
pub struct CommitRequest {
    pub path: PathBuf,
    pub message: String,
    pub head: Option<String>,
    pub env: crate::terminal::environment::ShellEnvironment,
    pub cancel: Arc<AtomicBool>,
    pub progress: Option<async_channel::Sender<String>>,
}
#[derive(Clone)]
pub struct GitClient {
    sender: SyncSender<Command>,
    cache: Arc<Mutex<Cache>>,
    changes: ChangeCache,
    pub updates: async_channel::Receiver<()>,
    pub stats: Arc<GitStats>,
}
struct Watch {
    dirty: Arc<AtomicBool>,
    paths: Vec<PathBuf>,
    failed: Arc<AtomicBool>,
    _watcher: Option<RecommendedWatcher>,
}

fn create_change_watch(
    path: PathBuf,
    sender: &SyncSender<Command>,
    cache: &ChangeCache,
    stats: &Arc<GitStats>,
    wake: &async_channel::Sender<()>,
) -> super::change_watch::ChangeWatch {
    let tx = sender.clone();
    let watch = super::change_watch::ChangeWatch::new(path.clone(), move || {
        let _ = tx.try_send(Command::Wake);
    });
    if watch.failed {
        stats.watch_failures.fetch_add(1, Ordering::Relaxed);
    }
    scan_changes(&path, cache, stats, wake);
    watch
}

fn suspend_change_watch(
    watch: &mut Option<super::change_watch::ChangeWatch>,
    path: &Path,
) -> Option<PathBuf> {
    if watch.as_ref().is_some_and(|watch| watch.path == path) {
        watch.take().map(|watch| watch.path)
    } else {
        None
    }
}

impl GitClient {
    pub fn start() -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(64);
        let (wake, updates) = async_channel::bounded(1);
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let stats = Arc::new(GitStats::default());
        let changes = Arc::new(Mutex::new(HashMap::new()));
        let change_cache = changes.clone();
        let shared = cache.clone();
        let worker_stats = stats.clone();
        let tx = sender.clone();
        std::thread::Builder::new()
            .name("canopy-git".into())
            .spawn(move || {
                let mut wanted = HashSet::<PathBuf>::new();
                let mut watches = HashMap::<PathBuf, Watch>::new();
                let mut deadline: Option<Instant> = None;
                let mut change_watch: Option<super::change_watch::ChangeWatch> = None;
                loop {
                    if let Some(watch) = &mut change_watch {
                        watch.schedule();
                        let failed = watch.failed;
                        if watch.due() {
                            if !failed && watch.failed {
                                worker_stats.watch_failures.fetch_add(1, Ordering::Relaxed);
                            }
                            scan_changes(&watch.path, &change_cache, &worker_stats, &wake);
                        }
                    }
                    let next_deadline = [deadline, change_watch.as_ref().and_then(|w| w.deadline)]
                        .into_iter()
                        .flatten()
                        .min();
                    let command = match next_deadline {
                        Some(at) => match receiver
                            .recv_timeout(at.saturating_duration_since(Instant::now()))
                        {
                            Ok(c) => Some(c),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(_) => break,
                        },
                        None => match receiver.recv() {
                            Ok(c) => Some(c),
                            Err(_) => break,
                        },
                    };
                    match command {
                        Some(Command::WatchChanges(path)) => {
                            if change_watch.as_ref().map(|w| &w.path) != path.as_ref() {
                                change_watch = path.map(|path| {
                                    create_change_watch(
                                        path,
                                        &tx,
                                        &change_cache,
                                        &worker_stats,
                                        &wake,
                                    )
                                });
                            }
                        }
                        Some(Command::NetworkPlan(path, reply)) => {
                            let _ = reply.send(network::prepare(path));
                        }
                        Some(Command::Network(plan, op, upstream, cancel, reply)) => {
                            let path = plan.path.clone();
                            let result = network::execute(plan, op, upstream, cancel);
                            scan_changes(&path, &change_cache, &worker_stats, &wake);
                            // Existing metadata watchers will coalesce ref changes.
                            let _ = reply.send(result);
                        }
                        Some(Command::History(path, head, offset, reply)) => {
                            let _ = reply.send(history::read(&path, head, offset));
                        }
                        Some(Command::TaskRepository(path, reply)) => {
                            let result = (|| {
                                let Some(info) = super::inspect(&path)? else {
                                    return Ok(None);
                                };
                                let repo =
                                    git2::Repository::discover(&path).map_err(|e| e.to_string())?;
                                let inferred = repo.find_remote("origin").ok().and_then(|r| {
                                    r.url().ok().and_then(crate::integrations::from_origin)
                                });
                                Ok(Some(crate::integrations::RepositoryContext {
                                    common: info.common,
                                    repository: info.root,
                                    inferred,
                                }))
                            })();
                            let _ = reply.send(result);
                        }
                        Some(Command::FileDirectory(root, path, budget, cancel, reply)) => {
                            if !reply.is_canceled() && !cancel.load(Ordering::Relaxed) {
                                let _ = reply.send(crate::files::list_directory(
                                    &root, &path, budget, &cancel,
                                ));
                            }
                        }
                        Some(Command::FileDecorations(root, reply)) => {
                            if !reply.is_canceled() {
                                let _ = reply.send(crate::files::decorations(&root));
                            }
                        }
                        Some(Command::Files(path, reply)) => {
                            if !reply.is_canceled() {
                                let _ = reply.send(crate::files::scan(&path));
                            }
                        }
                        Some(Command::ReadDiff(path, file, reply)) => {
                            let _ = reply.send(changes::diff(&path, &file));
                        }
                        Some(Command::Edit(path, edit, reply)) => {
                            let result = changes::edit(&path, edit);
                            scan_changes(&path, &change_cache, &worker_stats, &wake);
                            let _ = reply.send(result);
                        }
                        Some(Command::Commit(request, reply)) => {
                            let path = request.path.clone();
                            let report = |phase: &str| {
                                if let Some(tx) = &request.progress {
                                    let _ = tx.try_send(phase.to_owned());
                                }
                            };
                            let result = changes::commit_with_hooks(
                                &path,
                                &request.message,
                                request.head.as_deref(),
                                &request.env,
                                &request.cancel,
                                &report,
                            );
                            scan_changes(&path, &change_cache, &worker_stats, &wake);
                            let _ = reply.send(result);
                            for w in watches.values() {
                                w.dirty.store(true, Ordering::Release);
                            }
                            deadline = Some(Instant::now());
                        }
                        Some(Command::Watch(paths)) => {
                            wanted = paths.into_iter().collect();
                            shared.lock().unwrap().retain(|p, _| wanted.contains(p));
                            change_cache
                                .lock()
                                .unwrap()
                                .retain(|p, _| wanted.contains(p));
                            for path in &wanted {
                                if !shared.lock().unwrap().contains_key(path) {
                                    scan(path, &shared, &worker_stats);
                                }
                            }
                            sync_watches(&wanted, &shared, &mut watches, &tx, &worker_stats);
                            let _ = wake.try_send(());
                        }
                        Some(Command::Refresh) => {
                            if change_watch.as_ref().is_some_and(|watch| watch.failed)
                                && let Some(path) = change_watch.take().map(|watch| watch.path)
                            {
                                change_watch = Some(create_change_watch(
                                    path,
                                    &tx,
                                    &change_cache,
                                    &worker_stats,
                                    &wake,
                                ));
                            }
                            watches.retain(|_, watch| !watch.failed.load(Ordering::Acquire));
                            if let Some(w) = &change_watch {
                                scan_changes(&w.path, &change_cache, &worker_stats, &wake);
                            }
                            for watch in watches.values() {
                                watch.dirty.store(true, Ordering::Release);
                            }
                            deadline = Some(Instant::now());
                        }
                        Some(Command::Wake) => {
                            if deadline.is_none() {
                                deadline = Some(Instant::now() + Duration::from_millis(200));
                            }
                        }
                        Some(Command::Create(request, reply)) => {
                            let result = create_with_metadata(&request);
                            let _ = reply.send(result);
                            for w in watches.values() {
                                w.dirty.store(true, Ordering::Release);
                            }
                            deadline = Some(Instant::now());
                        }
                        Some(Command::AnalyzeWorktree(request, target, reply)) => {
                            let _ = reply.send(analyze_worktree(&request, target.as_deref()));
                        }
                        Some(Command::ExecuteWorktreeRemoval(
                            request,
                            action,
                            expected,
                            approval,
                            env,
                            reply,
                        )) => {
                            let mut cleanup_paths = vec![request.path.clone()];
                            if let RemovalAction::Merge { target, .. } = &action
                                && let Ok(Some(info)) = inspect(&request.repository)
                                && let Some(path) = info
                                    .worktrees
                                    .iter()
                                    .find(|worktree| {
                                        worktree.branch_name() == Some(target.as_str())
                                    })
                                    .map(|worktree| worktree.path.clone())
                            {
                                cleanup_paths.push(path);
                            }
                            let mut restart_watch = None;
                            for path in &cleanup_paths {
                                if let Some(path) = suspend_change_watch(&mut change_watch, path) {
                                    restart_watch = Some(path);
                                    break;
                                }
                            }
                            let result = execute_worktree_removal(
                                &request,
                                &action,
                                &expected,
                                approval,
                                env.as_ref(),
                            );
                            if let Some(path) = restart_watch.filter(|path| path.is_dir()) {
                                change_watch = Some(create_change_watch(
                                    path,
                                    &tx,
                                    &change_cache,
                                    &worker_stats,
                                    &wake,
                                ));
                            }
                            let _ = reply.send(result);
                            for w in watches.values() {
                                w.dirty.store(true, Ordering::Release);
                            }
                            deadline = Some(Instant::now());
                        }
                        Some(Command::DeleteBranchAfterRemoval(repository, analysis, reply)) => {
                            let _ = reply.send(delete_branch_after_removal(&repository, &analysis));
                            for w in watches.values() {
                                w.dirty.store(true, Ordering::Release);
                            }
                            deadline = Some(Instant::now());
                        }
                        Some(Command::Remove(request, discard, reply)) => {
                            let restart_watch =
                                suspend_change_watch(&mut change_watch, &request.path);
                            let result = remove_worktree(&request, discard);
                            if let Some(path) = restart_watch.filter(|path| path.is_dir()) {
                                change_watch = Some(create_change_watch(
                                    path,
                                    &tx,
                                    &change_cache,
                                    &worker_stats,
                                    &wake,
                                ));
                            }
                            let _ = reply.send(result);
                            for w in watches.values() {
                                w.dirty.store(true, Ordering::Release);
                            }
                            deadline = Some(Instant::now());
                        }
                        Some(Command::Shutdown(reply)) => {
                            watches.clear();
                            let _ = reply.send(());
                            break;
                        }
                        None => {
                            deadline = None;
                            let dirty: Vec<_> = watches
                                .iter()
                                .filter(|(_, w)| w.dirty.swap(false, Ordering::AcqRel))
                                .map(|(p, _)| p.clone())
                                .collect();
                            let mut changed = false;
                            let metadata_changed = !dirty.is_empty();
                            for root in dirty {
                                let aliases: Vec<_> = shared
                                    .lock()
                                    .unwrap()
                                    .iter()
                                    .filter(|(path, entry)| {
                                        **path == root
                                            || entry
                                                .as_ref()
                                                .ok()
                                                .and_then(|v| v.as_ref())
                                                .is_some_and(|info| info.root == root)
                                    })
                                    .map(|(p, _)| p.clone())
                                    .collect();
                                let result = scan_result(&root, &worker_stats);
                                for alias in aliases {
                                    let mut map = shared.lock().unwrap();
                                    if map.get(&alias) != Some(&result) {
                                        map.insert(alias, result.clone());
                                        changed = true;
                                    }
                                }
                            }
                            sync_watches(&wanted, &shared, &mut watches, &tx, &worker_stats);
                            if metadata_changed && let Some(w) = &change_watch {
                                scan_changes(&w.path, &change_cache, &worker_stats, &wake);
                            }
                            if changed {
                                let _ = wake.try_send(());
                            }
                        }
                    }
                    let failures = watches
                        .values()
                        .filter(|watch| watch.failed.load(Ordering::Acquire))
                        .count() as u64
                        + u64::from(change_watch.as_ref().is_some_and(|w| w.failed));
                    if worker_stats
                        .active_watch_failures
                        .swap(failures, Ordering::Relaxed)
                        != failures
                    {
                        let _ = wake.try_send(());
                    }
                    // A full queue can drop Wake; dirty flags remain authoritative.
                    if deadline.is_none()
                        && watches.values().any(|w| w.dirty.load(Ordering::Acquire))
                    {
                        deadline = Some(Instant::now() + Duration::from_millis(200));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender,
            cache,
            changes,
            updates,
            stats,
        })
    }
    pub fn changes_snapshot(&self, path: &Path) -> Option<Result<Arc<changes::ChangesSnapshot>>> {
        self.changes.lock().unwrap().get(path).cloned()
    }
    pub fn watch_changes(&self, path: Option<PathBuf>) -> Result<()> {
        self.send(Command::WatchChanges(path))
    }
    pub async fn network_plan(&self, path: PathBuf) -> Result<network::Plan> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::NetworkPlan(path, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn network(
        &self,
        plan: network::Plan,
        operation: network::Operation,
        upstream: Option<network::Upstream>,
        cancel: Arc<AtomicBool>,
    ) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Network(plan, operation, upstream, cancel, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn history(
        &self,
        path: PathBuf,
        head: Option<Arc<history::HistorySnapshot>>,
        offset: usize,
    ) -> Result<history::HistoryPage> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::History(path, head, offset, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn task_repository(
        &self,
        path: PathBuf,
    ) -> Result<Option<crate::integrations::RepositoryContext>> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::TaskRepository(path, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn file_directory(
        &self,
        root: PathBuf,
        path: PathBuf,
        budget: usize,
        cancel: Arc<AtomicBool>,
    ) -> Result<crate::files::DirectoryListing> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::FileDirectory(root, path, budget, cancel, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn file_decorations(&self, root: PathBuf) -> Result<crate::files::Decorations> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::FileDecorations(root, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn files(&self, path: PathBuf) -> Result<crate::files::Index> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Files(path, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn diff(
        &self,
        path: PathBuf,
        file: changes::FileChange,
    ) -> Result<changes::FileDiff> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::ReadDiff(path, file, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn edit(&self, path: PathBuf, edit: changes::Edit) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Edit(path, edit, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn commit(
        &self,
        path: PathBuf,
        message: String,
        head: Option<String>,
        env: crate::terminal::environment::ShellEnvironment,
        cancel: Arc<AtomicBool>,
    ) -> Result<String> {
        self.commit_with_hooks(CommitRequest {
            path,
            message,
            head,
            env,
            cancel,
            progress: None,
        })
        .await
        .map(|result| result.id.to_string())
    }
    pub async fn commit_with_hooks(
        &self,
        request: CommitRequest,
    ) -> Result<changes::CommitOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Commit(Box::new(request), tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub fn snapshots(&self) -> Cache {
        self.cache.lock().unwrap().clone()
    }
    pub fn watch(&self, paths: Vec<PathBuf>) -> Result<()> {
        if paths.len() > 128 {
            return Err("At most 128 working directories can be watched.".into());
        }
        self.send(Command::Watch(paths))
    }
    pub fn refresh(&self) -> Result<()> {
        self.send(Command::Refresh)
    }
    fn send(&self, c: Command) -> Result<()> {
        self.sender
            .try_send(c)
            .map_err(|_| "Git worker is busy or unavailable. Try again shortly.".into())
    }
    pub async fn create(&self, request: CreateWorktree) -> Result<CreatedWorktree> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Create(request, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn analyze_worktree(
        &self,
        request: RemoveWorktree,
        target: Option<String>,
    ) -> Result<WorktreeAnalysis> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::AnalyzeWorktree(request, target, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn execute_worktree_removal(
        &self,
        request: RemoveWorktree,
        action: RemovalAction,
        expected: WorktreeAnalysis,
        approval: RemovalApproval,
        env: Option<crate::terminal::environment::ShellEnvironment>,
    ) -> Result<WorktreeRemovalOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::ExecuteWorktreeRemoval(
            request, action, expected, approval, env, tx,
        ))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn delete_branch_after_removal(
        &self,
        repository: PathBuf,
        analysis: BranchAnalysis,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::DeleteBranchAfterRemoval(repository, analysis, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn remove(&self, request: RemoveWorktree, discard: bool) -> Result<RemovalOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Remove(request, discard, tx))?;
        rx.await.map_err(|_| "Git worker stopped.".to_owned())?
    }
    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        let sender = self.sender.clone(); // UI uses this only after pending mutations finish.
        if sender.send(Command::Shutdown(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}
fn scan_result(path: &Path, stats: &GitStats) -> Entry {
    stats.scans.fetch_add(1, Ordering::Relaxed);
    inspect(path).map(|v| v.map(Arc::new))
}
fn scan(path: &Path, cache: &Mutex<Cache>, stats: &GitStats) {
    let cached = repository_root(path).ok().flatten().and_then(|root| {
        cache
            .lock()
            .unwrap()
            .values()
            .find(|entry| {
                entry
                    .as_ref()
                    .ok()
                    .and_then(|v| v.as_ref())
                    .is_some_and(|info| info.root == root)
            })
            .cloned()
    });
    let result = cached.unwrap_or_else(|| scan_result(path, stats));
    cache.lock().unwrap().insert(path.into(), result);
}

fn sync_watches(
    wanted: &HashSet<PathBuf>,
    cache: &Mutex<Cache>,
    watches: &mut HashMap<PathBuf, Watch>,
    sender: &SyncSender<Command>,
    stats: &Arc<GitStats>,
) {
    let entries = cache.lock().unwrap();
    let mut roots = HashMap::new();
    for path in wanted {
        let info = entries
            .get(path)
            .and_then(|r| r.as_ref().ok())
            .and_then(|v| v.as_ref());
        let root = info.map(|i| i.root.clone()).unwrap_or_else(|| path.clone());
        roots.insert(root, info.map(|i| i.common.clone()));
    }
    drop(entries);
    watches.retain(|path, _| roots.contains_key(path));
    for (root, common) in roots {
        let mut paths = vec![common.clone().unwrap_or_else(|| root.clone())];
        if let Some(common) = &common {
            for name in ["refs", "worktrees"] {
                let path = common.join(name);
                if path.is_dir() {
                    paths.push(path);
                }
            }
        }
        if watches.get(&root).is_some_and(|w| w.paths == paths) {
            continue;
        }
        let dirty = Arc::new(AtomicBool::new(false));
        let flag = dirty.clone();
        let sender = sender.clone();
        let callback_stats = stats.clone();
        let watched_common = common.clone();
        let failed = Arc::new(AtomicBool::new(false));
        let callback_failed = failed.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if (event.is_err() || event.as_ref().is_ok_and(|event| event.need_rescan()))
                    && !callback_failed.swap(true, Ordering::AcqRel)
                {
                    callback_stats
                        .watch_failures
                        .fetch_add(1, Ordering::Relaxed);
                }
                let relevant = event
                    .as_ref()
                    .map(|e| {
                        !matches!(e.kind, notify::EventKind::Access(_))
                            && e.paths.iter().any(|p| {
                                if let Some(common) = &watched_common {
                                    p.strip_prefix(common).ok().is_some_and(|p| {
                                        matches!(
                                            p.components()
                                                .next()
                                                .map(|c| c.as_os_str().to_string_lossy())
                                                .as_deref(),
                                            Some(
                                                "HEAD"
                                                    | "index"
                                                    | "refs"
                                                    | "worktrees"
                                                    | "packed-refs"
                                                    | "config"
                                            )
                                        )
                                    })
                                } else {
                                    p.file_name().is_some_and(|n| n == ".git")
                                }
                            })
                    })
                    .unwrap_or(true);
                if relevant {
                    callback_stats.events.fetch_add(1, Ordering::Relaxed);
                    if !flag.swap(true, Ordering::AcqRel) {
                        let _ = sender.try_send(Command::Wake);
                    }
                }
            })
            .ok();
        if watcher.is_none() {
            failed.store(true, Ordering::Release);
        }
        if let Some(w) = &mut watcher {
            for (index, path) in paths.iter().enumerate() {
                if w.watch(
                    path,
                    if index == 0 {
                        RecursiveMode::NonRecursive
                    } else {
                        RecursiveMode::Recursive
                    },
                )
                .is_err()
                {
                    failed.store(true, Ordering::Release);
                    stats.watch_failures.fetch_add(1, Ordering::Relaxed);
                }
            }
        } else {
            stats.watch_failures.fetch_add(1, Ordering::Relaxed);
        }
        watches.insert(
            root,
            Watch {
                dirty,
                paths,
                failed,
                _watcher: watcher,
            },
        );
    }
}

fn scan_changes(
    path: &Path,
    cache: &Mutex<HashMap<PathBuf, Result<Arc<changes::ChangesSnapshot>>>>,
    stats: &GitStats,
    wake: &async_channel::Sender<()>,
) {
    let revision = stats.status_scans.fetch_add(1, Ordering::Relaxed) + 1;
    let result = changes::status(path).map(|mut s| {
        s.revision = revision;
        Arc::new(s)
    });
    let mut cache = cache.lock().unwrap();
    if cache.get(path) != Some(&result) {
        cache.insert(path.into(), result);
        let _ = wake.try_send(());
    }
}
