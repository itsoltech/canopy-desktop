use super::AppState;
use canopy_desktop::{
    files::{
        self, CancelGuard, Decorations, DirectoryListing, Dirty, Index, TreeWatch, WatchPaths,
    },
    state::workspace::{PaneKind, PaneMetadata},
};
use gpui_kit::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
#[derive(Clone)]
struct Request {
    directories: BTreeSet<PathBuf>,
    revision: u64,
    force: u64,
    cancel: Arc<AtomicBool>,
}
struct Runtime {
    request: Arc<Mutex<Request>>,
    wake: async_channel::Sender<()>,
    _cancel: CancelGuard,
}
pub struct FilesState {
    pub root: Option<PathBuf>,
    pub index: Arc<Index>,
    pub error: Option<String>,
    pub loading: bool,
    runtime: Option<Runtime>,
    task: Option<Task<()>>,
    observers: Vec<Subscription>,
    creation: Option<Task<()>>,
    creating: bool,
    quitting: bool,
    paused: bool,
}
impl FilesState {
    pub fn new() -> Self {
        Self {
            root: None,
            index: Arc::new(Index::default()),
            error: None,
            loading: false,
            runtime: None,
            task: None,
            observers: vec![],
            creation: None,
            creating: false,
            quitting: false,
            paused: false,
        }
    }
    pub fn bind(&mut self, cx: &mut Context<Self>) {
        let projects = cx.global::<AppState>().projects.clone();
        self.observers
            .push(cx.observe(&projects, |s, _, cx| s.sync(cx)));
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        let root = cx
            .global::<AppState>()
            .projects
            .read(cx)
            .catalog
            .current()
            .map(|p| p.worktree_path.as_ref().unwrap_or(&p.path).clone());
        if root == self.root {
            return;
        }
        self.runtime = None;
        self.task = None;
        self.paused = false;
        self.root = root;
        self.index = Arc::new(Index::default());
        self.start(cx);
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.quitting || self.paused {
            return;
        }
        if let Some(runtime) = &self.runtime
            && !runtime.wake.is_closed()
        {
            self.schedule(None, true, cx);
        } else {
            self.start(cx);
        }
    }
    pub fn expanded(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.quitting || self.paused {
            return;
        }
        let mut wanted = BTreeSet::from([PathBuf::new()]);
        for path in paths {
            if path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                wanted.insert(path);
            }
        }
        if wanted.len() > files::MAX_OPEN_DIRECTORIES {
            let error =
                Some("Too many expanded folders. Collapse a folder before opening another.".into());
            if self.error != error {
                self.error = error;
                cx.notify();
            }
            return;
        }
        if self
            .runtime
            .as_ref()
            .is_some_and(|r| r.request.lock().unwrap().directories == wanted)
        {
            return;
        }
        self.schedule(Some(wanted), false, cx);
    }
    fn schedule(&mut self, wanted: Option<BTreeSet<PathBuf>>, force: bool, cx: &mut Context<Self>) {
        let Some(runtime) = &mut self.runtime else {
            return;
        };
        let next = CancelGuard::default();
        runtime._cancel = next;
        let mut request = runtime.request.lock().unwrap();
        if let Some(wanted) = wanted {
            request.directories = wanted;
        }
        request.revision += 1;
        if force {
            request.force += 1;
        }
        request.cancel = runtime._cancel.0.clone();
        let mut index = (*self.index).clone();
        index.loading_directories = request
            .directories
            .difference(&index.loaded_directories)
            .cloned()
            .collect();
        self.index = Arc::new(index);
        self.error = None;
        let _ = runtime.wake.try_send(());
        cx.notify();
    }
    fn publish(&mut self, index: Index, error: Option<String>, cx: &mut Context<Self>) {
        let changed = self.loading || *self.index != index || self.error != error;
        self.loading = false;
        if changed {
            self.index = Arc::new(index);
            self.error = error;
            cx.notify();
        }
    }
    fn start(&mut self, cx: &mut Context<Self>) {
        self.runtime = None;
        self.task = None;
        self.error = None;
        if self.paused {
            self.loading = false;
            cx.notify();
            return;
        }
        let Some(root) = self.root.clone() else {
            self.loading = false;
            cx.notify();
            return;
        };
        let Some(client) = cx.global::<AppState>().git.read(cx).client() else {
            self.loading = false;
            self.error = Some("File worker is unavailable.".into());
            cx.notify();
            return;
        };
        let (wake, incoming) = async_channel::bounded(1);
        let cancel = CancelGuard::default();
        let control = Arc::new(Mutex::new(Request {
            directories: BTreeSet::from([PathBuf::new()]),
            revision: 0,
            force: 0,
            cancel: cancel.0.clone(),
        }));
        self.runtime = Some(Runtime {
            request: control.clone(),
            wake,
            _cancel: cancel,
        });
        self.loading = true;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let work_root = root.clone();
            let canonical = cx
                .background_executor()
                .spawn(async move { work_root.canonicalize().map_err(|e| e.to_string()) })
                .await;
            let canonical = match canonical {
                Ok(path) => path,
                Err(e) => {
                    let _ = this.update(cx, |s, cx| s.publish(Index::default(), Some(e), cx));
                    return;
                }
            };
            let mut cache = BTreeMap::<PathBuf, DirectoryListing>::new();
            let mut errors = BTreeMap::new();
            let mut decoration = Decorations::default();
            let mut git_error = None;
            let mut watcher: Option<TreeWatch> = None;
            let mut dirty = Dirty {
                tree: true,
                git: true,
                ..Default::default()
            };
            let mut last_force = 0;
            'refresh: loop {
                while incoming.try_recv().is_ok() {}
                let request = control.lock().unwrap().clone();
                if request.force != last_force {
                    last_force = request.force;
                    dirty = Dirty {
                        tree: true,
                        git: true,
                        ..Default::default()
                    };
                    watcher = None;
                }
                cache.retain(|path, _| request.directories.contains(path));
                errors.retain(|path, _| request.directories.contains(path));
                if dirty.tree {
                    if dirty.directories.is_empty() {
                        cache.clear();
                        errors.clear();
                    } else {
                        for path in &dirty.directories {
                            if let Ok(path) = path.strip_prefix(&canonical) {
                                cache.remove(path);
                                errors.remove(path);
                            }
                        }
                    }
                    dirty.tree = false;
                    dirty.directories.clear();
                }
                let paths = watch_paths(&canonical, &request.directories, &cache, &decoration);
                let watch_root = canonical.clone();
                let configured = cx
                    .background_executor()
                    .spawn(async move { configure_watch(watcher, &watch_root, paths) })
                    .await;
                watcher = configured.0;
                let mut watch_error = configured.1;
                if request.cancel.load(Ordering::Relaxed) {
                    continue 'refresh;
                }
                let mut directories = request.directories.iter().cloned().collect::<Vec<_>>();
                directories.sort_by_key(|p| (p.components().count(), p.clone()));
                for path in directories {
                    if !path.as_os_str().is_empty()
                        && !cache
                            .get(path.parent().unwrap_or(Path::new("")))
                            .is_some_and(|parent| {
                                parent.entries.iter().any(|e| e.path == path && e.directory)
                            })
                    {
                        cache.remove(&path);
                        errors.remove(&path);
                        continue;
                    }
                    if cache.contains_key(&path) || errors.contains_key(&path) {
                        continue;
                    }
                    let used = cache.values().map(|d| d.entries.len()).sum::<usize>();
                    let budget = files::MAX_VISIBLE_ENTRIES.saturating_sub(used);
                    let result = client
                        .file_directory(
                            canonical.clone(),
                            path.clone(),
                            budget,
                            request.cancel.clone(),
                        )
                        .await;
                    if request.cancel.load(Ordering::Relaxed) {
                        continue 'refresh;
                    }
                    match result {
                        Ok(list) => {
                            cache.insert(path, list);
                        }
                        Err(e) => {
                            errors.insert(path, e);
                        }
                    }
                }
                if request.cancel.load(Ordering::Relaxed) {
                    continue 'refresh;
                }
                let index = files::snapshot(&cache, &decoration, BTreeSet::new(), errors.clone());
                if this
                    .update(cx, |s, cx| {
                        if s.root.as_ref() == Some(&root) {
                            s.publish(index, watch_error.clone().or(git_error.clone()), cx);
                        }
                    })
                    .is_err()
                {
                    return;
                }
                let paths = watch_paths(
                    &canonical,
                    &cache.keys().cloned().collect(),
                    &cache,
                    &decoration,
                );
                let watch_root = canonical.clone();
                let configured = cx
                    .background_executor()
                    .spawn(async move { configure_watch(watcher, &watch_root, paths) })
                    .await;
                watcher = configured.0;
                watch_error = configured.1;
                if request.cancel.load(Ordering::Relaxed) {
                    continue 'refresh;
                }
                let error = watch_error.clone().or(git_error.clone());
                if this
                    .update(cx, |s, cx| {
                        if s.root.as_ref() == Some(&root) && s.error != error {
                            s.error = error;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    return;
                }
                if dirty.git {
                    dirty.git = false;
                    match client.file_decorations(canonical.clone()).await {
                        Ok(value) => {
                            decoration = value;
                            git_error = None;
                        }
                        Err(e) => git_error = Some(format!("Git status unavailable: {e}")),
                    }
                    if request.cancel.load(Ordering::Relaxed) {
                        continue 'refresh;
                    }
                    let paths = watch_paths(
                        &canonical,
                        &cache.keys().cloned().collect(),
                        &cache,
                        &decoration,
                    );
                    let watch_root = canonical.clone();
                    let configured = cx
                        .background_executor()
                        .spawn(async move { configure_watch(watcher, &watch_root, paths) })
                        .await;
                    watcher = configured.0;
                    watch_error = configured.1;
                    if request.cancel.load(Ordering::Relaxed) {
                        continue 'refresh;
                    }
                    let index =
                        files::snapshot(&cache, &decoration, BTreeSet::new(), errors.clone());
                    if this
                        .update(cx, |s, cx| {
                            if s.root.as_ref() == Some(&root) {
                                s.publish(index, watch_error.clone().or(git_error.clone()), cx);
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                let watch_rx = watcher.as_ref().map(|w| w.changes.clone());
                let event = futures_lite::future::race(
                    async { incoming.recv().await.map(|_| false) },
                    async {
                        if let Some(rx) = watch_rx {
                            rx.recv().await.map(|_| true)
                        } else {
                            std::future::pending().await
                        }
                    },
                )
                .await;
                match event {
                    Err(_) => return,
                    Ok(false) => {}
                    Ok(true) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(250))
                            .await;
                        if let Some(w) = &watcher {
                            while w.changes.try_recv().is_ok() {}
                            dirty = w.take_dirty();
                        }
                    }
                }
            }
        }));
    }
    pub fn create_file(&mut self, relative: PathBuf, cx: &mut Context<Self>) {
        if self.creating || self.quitting {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        self.creating = true;
        self.creation = Some(cx.spawn(async move |this, cx| {
            let (base, path) = (root.clone(), relative.clone());
            let result = cx
                .background_executor()
                .spawn(async move { files::create(&base, &path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.creating = false;
                if this.root.as_ref() != Some(&root) {
                    return;
                }
                match result {
                    Ok(()) => {
                        this.refresh(cx);
                        this.open(&relative, cx);
                    }
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            });
        }));
    }
    pub fn begin_quit(&mut self) -> Option<Task<()>> {
        self.quitting = true;
        self.runtime = None;
        self.task = None;
        self.creation.take()
    }
    pub fn pause_for_cleanup(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if !self
            .root
            .as_ref()
            .is_some_and(|root| paths.iter().any(|path| path == root))
        {
            return;
        }
        self.paused = true;
        self.runtime = None;
        self.task = None;
        self.loading = false;
        cx.notify();
    }
    pub fn resume_after_cleanup(
        &mut self,
        paths: &[PathBuf],
        removed: &std::collections::HashSet<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        if !paths.iter().any(|path| path == root) {
            return;
        }
        if removed.contains(root) {
            return;
        }
        self.paused = false;
        self.start(cx);
    }
    pub fn cancel_quit(&mut self, cx: &mut Context<Self>) {
        self.quitting = false;
        self.refresh(cx);
    }
    pub fn open(&self, relative: &Path, cx: &mut App) {
        if self.quitting || cx.global::<AppState>().settings.read(cx).quitting {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let resource = relative.to_string_lossy().to_string();
        let app = cx.global::<AppState>().clone();
        app.workspace.update(cx, |workspace, cx| {
            let existing = workspace
                .all_panes()
                .iter()
                .find(|p| {
                    matches!(
                        p.metadata.kind,
                        PaneKind::Editor | PaneKind::Image | PaneKind::Font | PaneKind::Video
                    ) && p.metadata.cwd.as_ref() == Some(&root)
                        && p.metadata.resource.as_deref() == Some(&resource)
                })
                .map(|p| p.id);
            if let Some(pane) = existing {
                let _ = workspace.activate_pane(pane);
            } else {
                let title = relative
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let tab = workspace.open(&title, "editor");
                let pane = workspace.active().unwrap().focused;
                let _ = workspace.set_pane_metadata(
                    tab,
                    pane,
                    PaneMetadata {
                        kind: if files::is_font(relative) {
                            PaneKind::Font
                        } else if files::is_video(relative) {
                            PaneKind::Video
                        } else if files::is_image(relative) {
                            PaneKind::Image
                        } else {
                            PaneKind::Editor
                        },
                        cwd: Some(root),
                        resource: Some(resource),
                        title: Some(title),
                        ..Default::default()
                    },
                );
            }
            cx.notify();
        });
    }
}

fn watch_paths(
    root: &Path,
    listed: &BTreeSet<PathBuf>,
    cache: &BTreeMap<PathBuf, DirectoryListing>,
    decoration: &Decorations,
) -> WatchPaths {
    let git = cache
        .values()
        .flat_map(|d| [d.git_directory.clone(), d.git_common_directory.clone()])
        .chain([
            decoration.git_directory.clone(),
            decoration.git_common_directory.clone(),
        ])
        .flatten()
        .collect();
    WatchPaths {
        listed: listed.iter().map(|p| root.join(p)).collect(),
        tracked: decoration
            .tracked_directories
            .iter()
            .map(|p| root.join(p))
            .collect(),
        untracked: decoration
            .untracked_directories
            .iter()
            .map(|p| root.join(p))
            .collect(),
        ignored: cache
            .values()
            .flat_map(|d| d.entries.iter())
            .filter(|e| e.ignored)
            .map(|e| root.join(&e.path))
            .collect(),
        git,
    }
}
fn configure_watch(
    watcher: Option<TreeWatch>,
    root: &Path,
    mut paths: WatchPaths,
) -> (Option<TreeWatch>, Option<String>) {
    let safe = |path: &PathBuf| {
        path.strip_prefix(root).is_ok_and(|p| {
            if p.as_os_str().is_empty() {
                root.is_dir()
            } else {
                files::resolve(root, p).is_ok_and(|path| path.is_dir())
            }
        })
    };
    paths.listed.retain(safe);
    #[cfg(not(target_os = "macos"))]
    {
        paths.tracked.retain(safe);
        paths.untracked.retain(safe);
    }
    let mut watcher = match watcher.map(Ok).unwrap_or_else(TreeWatch::new) {
        Ok(w) => w,
        Err(e) => return (None, Some(format!("File watcher unavailable: {e}"))),
    };
    let result = watcher.set(paths).err().or_else(|| watcher.error());
    (
        Some(watcher),
        result.map(|e| format!("File watcher unavailable: {e}")),
    )
}
