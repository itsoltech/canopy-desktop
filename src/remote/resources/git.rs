use super::snapshot;
use super::*;
use crate::{
    git::{self, GitClient, changes, network},
    host::{FILES_GIT, READ},
    terminal::environment::ShellEnvironment,
};
use prost::Message;
use proto::git_preflight_request::Intent;
use std::collections::HashMap;
use std::{
    pin::Pin,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use tonic::{Request, Response};

/// App-owned process/editor coordination. The library never reaches into GPUI.
#[tonic::async_trait]
pub trait WorktreeLifecycle: Send + Sync {
    async fn preflight(
        &self,
        root: PathBuf,
        intent: proto::GitWorktreeIntent,
    ) -> Result<Vec<String>, Status>;
    async fn apply(&self, root: PathBuf, intent: proto::GitWorktreeIntent) -> Result<(), Status>;
    async fn finished(&self, root: PathBuf, removed: bool) -> Result<(), Status>;
}
#[derive(Clone)]
pub struct GitRpc {
    auth: RpcAuth,
    roots: WorkspaceRoots,
    git: GitClient,
    environment: ShellEnvironment,
    locks: ResourceLocks,
    plans: Arc<Mutex<HashMap<String, ApprovedPlan>>>,
    lifecycle: Option<Arc<dyn WorktreeLifecycle>>,
}
#[derive(Clone)]
struct ApprovedPlan {
    device: String,
    root: PathBuf,
    request: proto::GitPreflightRequest,
    expected: proto::GitPrecondition,
    expires: Instant,
    prepared: Prepared,
    destinations: Vec<PathBuf>,
    destination_snapshots: Vec<(PathBuf, proto::GitPrecondition)>,
}
#[derive(Clone)]
enum Prepared {
    Paths(Vec<changes::FileChange>),
    Commit,
    Network(network::Plan, network::Operation, network::Upstream),
    Create(git::CreateWorktree),
    Remove(
        git::RemoveWorktree,
        git::RemovalAction,
        git::WorktreeAnalysis,
        git::RemovalApproval,
    ),
    Stop,
}
impl GitRpc {
    pub fn new(
        auth: RpcAuth,
        roots: WorkspaceRoots,
        git: GitClient,
        environment: ShellEnvironment,
        locks: ResourceLocks,
    ) -> Self {
        Self {
            auth,
            roots,
            git,
            environment,
            locks,
            plans: Arc::default(),
            lifecycle: None,
        }
    }
    pub fn with_lifecycle(mut self, lifecycle: Arc<dyn WorktreeLifecycle>) -> Self {
        self.lifecycle = Some(lifecycle);
        self
    }
    async fn authorized<T>(
        &self,
        request: &Request<T>,
        workspace_ref: Option<&proto::WorkspaceRef>,
        scope: u32,
    ) -> Result<(DeviceGrant, PathBuf), Status> {
        let grant = self.auth.authenticate(request).await?;
        let root = workspace(&self.auth, &self.roots, &grant, workspace_ref, scope).await?;
        Ok((grant, root))
    }
    async fn check_dirty_paths(&self, root: &Path, intent: &Intent) -> Result<(), Status> {
        if let Intent::Paths(paths) = intent
            && paths.operation == proto::GitPathOperation::DiscardPaths as i32
        {
            for path in &paths.paths {
                validate_path(root, path)?;
                let identity = crate::host::FileIdentity::new(
                    WorkspaceRoots::root_string(root)?.into(),
                    path.clone(),
                )
                .ok_or_else(|| domain("Invalid file identity."))?;
                let warnings = self
                    .auth
                    .client()
                    .runtime_file_presence(identity)
                    .await
                    .map_err(storage_status)?;
                if !warnings.is_empty() {
                    return Err(Status::failed_precondition(format!(
                        "Dirty buffers must be resolved before discarding {}: {}",
                        path,
                        warnings
                            .iter()
                            .map(|p| format!("{} (last seen {})", p.device_name(), p.last_seen()))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        }
        Ok(())
    }
    async fn authorize_destinations(
        &self,
        grant: &DeviceGrant,
        root: &Path,
        intent: &Intent,
    ) -> Result<Vec<PathBuf>, Status> {
        let mut destinations = Vec::new();
        if let Intent::Worktree(worktree) = intent {
            if worktree.action == proto::WorktreeAction::CreateWorktree as i32 {
                let path = required(worktree.path.clone())?;
                if !Path::new(&path).is_absolute()
                    || Path::new(&path).components().any(|p| {
                        matches!(
                            p,
                            std::path::Component::ParentDir | std::path::Component::CurDir
                        )
                    })
                {
                    return Err(Status::invalid_argument(
                        "Worktree destination must be an authorized absolute path.",
                    ));
                }
                // Check before inspecting its parent or any metadata.
                self.auth.resource(grant.id(), FILES_GIT, &path).await?;
                let canonical = blocking(move || {
                    let path = PathBuf::from(path);
                    let parent = path
                        .parent()
                        .ok_or_else(|| domain("Destination has no parent."))?
                        .canonicalize()
                        .map_err(domain)?;
                    Ok(parent.join(
                        path.file_name()
                            .ok_or_else(|| domain("Destination has no name."))?,
                    ))
                })
                .await?;
                self.auth
                    .resource(
                        grant.id(),
                        FILES_GIT,
                        WorkspaceRoots::root_string(&canonical)?,
                    )
                    .await?;
                destinations.push(canonical);
            }
            if let Some(target) = &worktree.target_branch {
                let inspect_root = root.to_owned();
                let target = target.clone();
                let targets = blocking(move || {
                    let info = git::inspect(&inspect_root)
                        .map_err(domain)?
                        .ok_or_else(|| domain("Repository not found."))?;
                    Ok(info
                        .worktrees
                        .into_iter()
                        .filter(|w| w.branch_name() == Some(target.as_str()))
                        .map(|w| w.path)
                        .collect::<Vec<_>>())
                })
                .await?;
                for target in targets {
                    self.auth
                        .resource(grant.id(), FILES_GIT, WorkspaceRoots::root_string(&target)?)
                        .await?;
                    if target != root {
                        destinations.push(target);
                    }
                }
            }
        }
        destinations.sort();
        destinations.dedup();
        Ok(destinations)
    }
    async fn execute(
        &self,
        grant: DeviceGrant,
        meta: proto::CommandMeta,
        mut request: proto::GitPreflightRequest,
        expected: proto::GitPrecondition,
        method: &'static str,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let root = workspace(
            &self.auth,
            &self.roots,
            &grant,
            request.workspace.as_ref(),
            FILES_GIT,
        )
        .await?;
        normalize(&mut request)?;
        // The immutable client intent and expected snapshot, not the live plan,
        // fingerprint retry. A final receipt remains retrievable after plan expiry.
        let arguments = (request.encode_to_vec(), expected.encode_to_vec());
        let input = self.auth.input(
            &meta,
            &grant,
            method,
            request.workspace.as_ref().unwrap().workspace_id.clone(),
            serde_json::to_vec(&arguments).map_err(domain)?,
            WorkspaceRoots::root_string(&root)?.into(),
        )?;
        let service = self.clone();
        let result = mutation(
            self.auth.clone(),
            self.locks.clone(),
            root.clone(),
            input,
            move || async move {
                service
                    .auth
                    .resource(grant.id(), FILES_GIT, WorkspaceRoots::root_string(&root)?)
                    .await?;
                let id = expected.preflight_id.as_ref().ok_or_else(|| {
                    Status::failed_precondition("Review and approve a current Git preflight first.")
                })?;
                let plan = service
                    .plans
                    .lock()
                    .map_err(|_| Status::internal("Git plans unavailable."))?
                    .remove(id)
                    .ok_or_else(|| Status::aborted("Git preflight expired; review a new plan."))?;
                if plan.expires < Instant::now()
                    || plan.device != grant.id()
                    || plan.root != root
                    || plan.request != request
                    || plan.expected != expected
                {
                    return Err(Status::aborted(
                        "Git preflight does not match this operation.",
                    ));
                }
                let destinations = service
                    .authorize_destinations(&grant, &root, request.intent.as_ref().unwrap())
                    .await?;
                if destinations != plan.destinations {
                    return Err(Status::aborted(
                        "Affected worktree paths changed since preflight.",
                    ));
                }
                let _destination_guards = destinations
                    .iter()
                    .map(|path| {
                        service.locks.gate().try_reserve_root(path).ok_or_else(|| {
                            Status::aborted("An affected worktree is busy. Review a new preflight.")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let destination_snapshots = plan.destination_snapshots.clone();
                blocking(move || {
                    for (path, expected) in destination_snapshots {
                        snapshot::check(&path, &expected)?;
                    }
                    Ok(())
                })
                .await?;
                service
                    .check_dirty_paths(&root, request.intent.as_ref().unwrap())
                    .await?;
                let verify_root = root.clone();
                let verify_expected = expected.clone();
                blocking(move || snapshot::check(&verify_root, &verify_expected)).await?;
                let worktree = match request.intent.as_ref() {
                    Some(Intent::Worktree(intent))
                        if intent.action != proto::WorktreeAction::CreateWorktree as i32 =>
                    {
                        Some(intent.clone())
                    }
                    _ => None,
                };
                if let Some(intent) = &worktree {
                    let lifecycle = service.lifecycle.as_ref().ok_or_else(|| {
                        domain("needs_host_interaction: Worktree lifecycle owner is unavailable.")
                    })?;
                    lifecycle.preflight(root.clone(), intent.clone()).await?;
                    if let Err(error) = lifecycle.apply(root.clone(), intent.clone()).await {
                        let _ = lifecycle.finished(root.clone(), false).await;
                        return Err(error);
                    }
                }
                let apply_root = root.clone();
                let env = service.environment.clone();
                let apply_expected = expected.clone();
                let apply_destinations = plan.destination_snapshots.clone();
                let intent = request.intent.unwrap();
                let result = if matches!(&plan.prepared, Prepared::Stop) {
                    Ok(b"Stopped workspace processes.".to_vec())
                } else {
                    service
                        .git
                        .run_resource_operation(move || {
                            snapshot::check(&apply_root, &apply_expected)?;
                            for (path, expected) in &apply_destinations {
                                snapshot::check(path, expected)?;
                            }
                            apply(&apply_root, plan.prepared, intent, &apply_expected, &env)
                        })
                        .await
                        .map_err(Status::unknown)
                        .and_then(|result| result)
                };
                if worktree.is_some() {
                    let removed = !root.exists();
                    if let Err(error) = service
                        .lifecycle
                        .as_ref()
                        .unwrap()
                        .finished(root.clone(), removed)
                        .await
                        && let Ok(mut bytes) = result
                    {
                        bytes.extend(format!("\nCleanup warning: {}", error.message()).as_bytes());
                        return Ok(bytes);
                    }
                }
                result
            },
        )
        .await?;
        Ok(Response::new(result))
    }
}
#[tonic::async_trait]
impl proto::git_service_server::GitService for GitRpc {
    async fn preflight(
        &self,
        request: Request<proto::GitPreflightRequest>,
    ) -> Result<Response<proto::GitPlan>, Status> {
        let (grant, root) = self
            .authorized(&request, request.get_ref().workspace.as_ref(), FILES_GIT)
            .await?;
        let mut request = request.into_inner();
        normalize(&mut request)?;
        let _guard = self.locks.acquire(&root).await?;
        let destinations = self
            .authorize_destinations(&grant, &root, request.intent.as_ref().unwrap())
            .await?;
        let _destination_guards = destinations
            .iter()
            .map(|path| {
                self.locks
                    .gate()
                    .try_reserve_root(path)
                    .ok_or_else(|| Status::aborted("An affected worktree is busy."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let capture_destinations = destinations.clone();
        let destination_snapshots = blocking(move || {
            capture_destinations
                .into_iter()
                .filter(|path| path.exists())
                .map(|path| snapshot::capture(&path).map(|expected| (path, expected)))
                .collect::<Result<Vec<_>, _>>()
        })
        .await?;
        self.check_dirty_paths(&root, request.intent.as_ref().unwrap())
            .await?;
        let mut impacts = Vec::new();
        if let Some(Intent::Worktree(intent)) = &request.intent
            && intent.action != proto::WorktreeAction::CreateWorktree as i32
        {
            impacts.extend(
                self.lifecycle
                    .as_ref()
                    .ok_or_else(|| {
                        domain("needs_host_interaction: Worktree lifecycle owner is unavailable.")
                    })?
                    .preflight(root.clone(), intent.clone())
                    .await?,
            );
        }
        let capture_root = root.clone();
        let intent = request.intent.clone().unwrap();
        let (prepared, mut expected, mut details) = blocking(move || {
            let before = snapshot::capture(&capture_root)?;
            let (prepared, details) = prepare(&capture_root, &intent)?;
            snapshot::check(&capture_root, &before)?;
            Ok((prepared, before, details))
        })
        .await?;
        impacts.append(&mut details);
        let id = uuid::Uuid::new_v4().to_string();
        expected.preflight_id = Some(id.clone());
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| Status::internal("Git plans unavailable."))?;
        plans.retain(|_, p| p.expires > Instant::now());
        if plans.len() >= 128
            || plans
                .values()
                .map(|p| p.request.encoded_len())
                .sum::<usize>()
                + request.encoded_len()
                > 8 * 1024 * 1024
        {
            return Err(Status::resource_exhausted(
                "Too many pending Git preflights.",
            ));
        }
        plans.insert(
            id.clone(),
            ApprovedPlan {
                device: grant.id().into(),
                root,
                request: request.clone(),
                expected: expected.clone(),
                expires: Instant::now() + Duration::from_secs(300),
                prepared,
                destinations,
                destination_snapshots,
            },
        );
        Ok(Response::new(proto::GitPlan {
            workspace: request.workspace,
            plan_id: id,
            expected: Some(expected),
            impacts,
            can_execute: true,
            reason: String::new(),
            expires_unix: crate::artifacts::now() + 300,
        }))
    }
    async fn get_status(
        &self,
        request: Request<proto::WorkspaceRequest>,
    ) -> Result<Response<proto::GitStatus>, Status> {
        let (_, root) = self
            .authorized(&request, request.get_ref().workspace.as_ref(), READ)
            .await?;
        let workspace = request.into_inner().workspace;
        let result = blocking(move || {
            let expected = snapshot::capture(&root)?;
            let changes = changes::status(&root).map_err(domain)?;
            let repository = snapshot::repository(&root)?;
            let branch = repository
                .head()
                .ok()
                .and_then(|h| h.shorthand().ok().map(str::to_owned))
                .unwrap_or_default();
            let mut files = std::collections::BTreeMap::<String, proto::GitFile>::new();
            for file in changes.files {
                let path = file
                    .path
                    .to_str()
                    .ok_or_else(|| domain("Non-UTF8 Git path."))?
                    .to_owned();
                let row = files.entry(path.clone()).or_insert(proto::GitFile {
                    path,
                    index_status: String::new(),
                    worktree_status: String::new(),
                    previous_path: None,
                });
                if file.staged {
                    row.index_status = file.kind;
                } else {
                    row.worktree_status = file.kind;
                }
                row.previous_path = file.old_path.and_then(|p| p.to_str().map(str::to_owned));
            }
            snapshot::check(&root, &expected)?;
            let result = proto::GitStatus {
                workspace,
                head_oid: expected.head_oid.clone(),
                branch,
                files: files.into_values().collect(),
                operation: (repository.state() != git2::RepositoryState::Clean)
                    .then(|| format!("{:?}", repository.state())),
                expected: Some(expected),
            };
            if result.encoded_len() > crate::host::CONTROL_BYTES {
                return Err(Status::resource_exhausted(
                    "Git status exceeds the control frame limit.",
                ));
            }
            Ok(result)
        })
        .await?;
        Ok(Response::new(result))
    }
    type ReadDiffStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<proto::DiffChunk, Status>> + Send>>;
    async fn read_diff(
        &self,
        request: Request<proto::DiffRequest>,
    ) -> Result<Response<Self::ReadDiffStream>, Status> {
        let (grant, root) = self
            .authorized(&request, request.get_ref().workspace.as_ref(), READ)
            .await?;
        let request = request.into_inner();
        validate_path(&root, &request.path)?;
        let diff_root = root.clone();
        let diff = blocking(move || read_diff(&diff_root, &request)).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let auth = self.auth.clone();
        tokio::spawn(async move {
            let mut lines = Vec::new();
            let mut size = 0;
            for line in diff.lines {
                if let Err(error) = auth
                    .resource(grant.id(), READ, root.to_str().unwrap_or(""))
                    .await
                {
                    let _ = tx.send(Err(error)).await;
                    return;
                }
                let line = proto::DiffLine {
                    old_line: line.old,
                    new_line: line.new,
                    kind: line.kind.to_string(),
                    text: line.text,
                };
                if size + line.encoded_len() > 64 * 1024 && !lines.is_empty() {
                    if tx
                        .send(Ok(proto::DiffChunk {
                            lines: std::mem::take(&mut lines),
                            binary: diff.binary,
                            complete: false,
                            truncated: diff.truncated,
                        }))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    size = 0;
                }
                size += line.encoded_len();
                lines.push(line);
            }
            let _ = tx
                .send(Ok(proto::DiffChunk {
                    lines,
                    binary: diff.binary,
                    complete: true,
                    truncated: diff.truncated,
                }))
                .await;
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
    async fn read_history(
        &self,
        request: Request<proto::GitHistoryRequest>,
    ) -> Result<Response<proto::GitHistory>, Status> {
        let (_, root) = self
            .authorized(&request, request.get_ref().workspace.as_ref(), READ)
            .await?;
        let request = request.into_inner();
        Ok(Response::new(
            blocking(move || history(&root, request)).await?,
        ))
    }
    async fn list_refs(
        &self,
        request: Request<proto::WorkspaceRequest>,
    ) -> Result<Response<proto::GitRefs>, Status> {
        let (_, root) = self
            .authorized(&request, request.get_ref().workspace.as_ref(), READ)
            .await?;
        let refs = blocking(move || {
            let repo = snapshot::repository(&root)?;
            let mut refs = Vec::new();
            let mut size = 0;
            for reference in repo.references().map_err(domain)? {
                let reference = reference.map_err(domain)?;
                let name = reference.name().map_err(domain)?;
                let Some(oid) = reference.resolve().ok().and_then(|r| r.target()) else {
                    continue;
                };
                let item = proto::GitRef {
                    name: name.into(),
                    oid: oid.to_string(),
                    kind: if reference.is_tag() {
                        "tag"
                    } else if reference.is_remote() {
                        "remote"
                    } else {
                        "branch"
                    }
                    .into(),
                };
                size += item.encoded_len();
                if refs.len() >= 10_000 || size > crate::host::CONTROL_BYTES - 1024 {
                    return Err(Status::resource_exhausted(
                        "Too many refs for one response.",
                    ));
                }
                refs.push(item);
            }
            refs.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(proto::GitRefs { refs })
        })
        .await?;
        Ok(Response::new(refs))
    }
    async fn stage(
        &self,
        request: Request<proto::GitPathsRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        self.paths(request, proto::GitPathOperation::StagePaths, "git.stage")
            .await
    }
    async fn unstage(
        &self,
        request: Request<proto::GitPathsRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        self.paths(
            request,
            proto::GitPathOperation::UnstagePaths,
            "git.unstage",
        )
        .await
    }
    async fn discard(
        &self,
        request: Request<proto::GitPathsRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        self.paths(
            request,
            proto::GitPathOperation::DiscardPaths,
            "git.discard",
        )
        .await
    }
    async fn commit(
        &self,
        request: Request<proto::CommitRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        self.execute(
            grant,
            required(request.command)?,
            proto::GitPreflightRequest {
                workspace: request.workspace,
                intent: Some(Intent::Commit(proto::GitCommitIntent {
                    message: request.message,
                    sign: request.sign,
                })),
            },
            required(request.expected)?,
            "git.commit",
        )
        .await
    }
    async fn pull(
        &self,
        request: Request<proto::GitNetworkRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        self.network(request, proto::GitNetworkAction::PullRemote, "git.pull")
            .await
    }
    async fn push(
        &self,
        request: Request<proto::GitNetworkRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        self.network(request, proto::GitNetworkAction::PushRemote, "git.push")
            .await
    }
    async fn change_worktree(
        &self,
        request: Request<proto::WorktreeRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        let intent = proto::GitWorktreeIntent {
            action: request.action,
            branch: request.branch,
            path: request.path,
            start_point: request.start_point,
            target_branch: request.target_branch,
            delete_branch: request.delete_branch,
            discard_changes: request.discard_changes,
            delete_unmerged_branch: request.delete_unmerged_branch,
            acknowledge_offline_dirty: request.acknowledge_offline_dirty,
        };
        self.execute(
            grant,
            required(request.command)?,
            proto::GitPreflightRequest {
                workspace: request.workspace,
                intent: Some(Intent::Worktree(intent)),
            },
            required(request.expected)?,
            "git.worktree",
        )
        .await
    }
}
impl GitRpc {
    async fn paths(
        &self,
        request: Request<proto::GitPathsRequest>,
        operation: proto::GitPathOperation,
        method: &'static str,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        self.execute(
            grant,
            required(request.command)?,
            proto::GitPreflightRequest {
                workspace: request.workspace,
                intent: Some(Intent::Paths(proto::GitPathIntent {
                    operation: operation as i32,
                    paths: request.paths,
                })),
            },
            required(request.expected)?,
            method,
        )
        .await
    }
    async fn network(
        &self,
        request: Request<proto::GitNetworkRequest>,
        action: proto::GitNetworkAction,
        method: &'static str,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        self.execute(
            grant,
            required(request.command)?,
            proto::GitPreflightRequest {
                workspace: request.workspace,
                intent: Some(Intent::Network(proto::GitNetworkIntent {
                    action: action as i32,
                    remote: request.remote,
                    upstream_branch: request.upstream_branch,
                })),
            },
            required(request.expected)?,
            method,
        )
        .await
    }
}
fn normalize(request: &mut proto::GitPreflightRequest) -> Result<(), Status> {
    let intent = request
        .intent
        .as_mut()
        .ok_or_else(|| Status::invalid_argument("Git intent is required."))?;
    if let Intent::Paths(paths) = intent {
        paths.paths.sort();
        paths.paths.dedup();
        if paths.paths.is_empty() || paths.paths.len() > 10_000 {
            return Err(Status::invalid_argument(
                "Select between 1 and 10,000 paths.",
            ));
        }
    }
    if request.encoded_len() > crate::host::CONTROL_BYTES / 2 {
        return Err(Status::resource_exhausted("Git intent is too large."));
    }
    Ok(())
}
fn validate_path(root: &Path, path: &str) -> Result<(), Status> {
    let identity =
        crate::host::FileIdentity::new(WorkspaceRoots::root_string(root)?.into(), path.into())
            .ok_or_else(|| Status::invalid_argument("Invalid Git path."))?;
    if Path::new(identity.relative_path())
        .components()
        .any(|part| part.as_os_str() == ".git")
    {
        return Err(Status::permission_denied(
            "Git metadata cannot be selected.",
        ));
    }
    Ok(())
}
fn prepare(root: &Path, intent: &Intent) -> Result<(Prepared, Vec<String>), Status> {
    let repo = snapshot::repository(root)?;
    match intent {
        Intent::Paths(paths) => {
            let operation = proto::GitPathOperation::try_from(paths.operation).map_err(domain)?;
            if operation == proto::GitPathOperation::Unspecified {
                return Err(Status::invalid_argument("Path operation is required."));
            }
            let status = changes::status(root).map_err(domain)?;
            let staged = operation == proto::GitPathOperation::UnstagePaths;
            let mut files = Vec::new();
            for path in &paths.paths {
                validate_path(root, path)?;
                crate::files::resolve(root, Path::new(path)).map_err(domain)?;
                let file = status
                    .files
                    .iter()
                    .find(|f| f.path == Path::new(path) && f.staged == staged)
                    .ok_or_else(|| {
                        Status::aborted(format!("Selected change is no longer available: {path}"))
                    })?;
                if let Some(old) = &file.old_path {
                    crate::files::resolve(root, old).map_err(domain)?;
                }
                files.push(file.clone());
            }
            Ok((
                Prepared::Paths(files),
                paths
                    .paths
                    .iter()
                    .map(|p| format!("{operation:?}: {p}"))
                    .collect(),
            ))
        }
        Intent::Commit(commit) => {
            if commit.message.trim().is_empty() || commit.message.contains('\0') {
                return Err(Status::invalid_argument("A commit message is required."));
            }
            let signing = git::signing::info(&repo.config().map_err(domain)?).map_err(domain)?;
            if signing.enabled || commit.sign {
                return Err(domain(
                    "needs_host_interaction: Complete signing on the host; remote requests never open a pinentry prompt or silently create an unsigned commit.",
                ));
            }
            if repo.index().map_err(domain)?.has_conflicts() {
                return Err(domain("Resolve index conflicts before committing."));
            }
            Ok((Prepared::Commit,vec!["Commit the staged index; configured commit hooks run with bounded output and null stdin.".into()]))
        }
        Intent::Network(intent) => {
            let operation =
                match proto::GitNetworkAction::try_from(intent.action).map_err(domain)? {
                    proto::GitNetworkAction::PullRemote => network::Operation::Pull,
                    proto::GitNetworkAction::PushRemote => network::Operation::Push,
                    _ => return Err(Status::invalid_argument("Network operation is required.")),
                };
            let plan = network::prepare(root.to_owned()).map_err(domain)?;
            let upstream = match &intent.upstream_branch {
                Some(branch) => network::Upstream {
                    remote: intent.remote.clone(),
                    branch: branch.clone(),
                },
                None => plan
                    .upstream
                    .clone()
                    .filter(|u| u.remote == intent.remote)
                    .ok_or_else(|| domain("Select an upstream branch for this remote."))?,
            };
            let remote = repo.find_remote(&upstream.remote).map_err(domain)?;
            #[cfg(target_os = "macos")]
            if remote.url().map_err(domain)?.starts_with("https:") {
                return Err(domain(
                    "needs_host_interaction: HTTPS Keychain access must be completed on the host.",
                ));
            }
            let _ = remote;
            Ok((
                Prepared::Network(plan, operation, upstream),
                vec![format!(
                    "{operation:?} using configured remote {}. No force update.",
                    intent.remote
                )],
            ))
        }
        Intent::Worktree(intent) => {
            match proto::WorktreeAction::try_from(intent.action).map_err(domain)? {
                proto::WorktreeAction::CreateWorktree => {
                    let destination = PathBuf::from(required(intent.path.clone())?);
                    if destination.exists() {
                        return Err(Status::already_exists(
                            "Worktree destination already exists.",
                        ));
                    }
                    if intent.branch.is_empty()
                        || !git2::Reference::is_valid_name(&format!("refs/heads/{}", intent.branch))
                    {
                        return Err(Status::invalid_argument("Choose a valid branch name."));
                    }
                    let info = git::inspect(root)
                        .map_err(domain)?
                        .ok_or_else(|| domain("Repository unavailable."))?;
                    if info.worktrees.iter().any(|w| {
                        destination.starts_with(&w.path)
                            || w.branch_name() == Some(intent.branch.as_str())
                    }) {
                        return Err(domain(
                            "Destination is inside an existing worktree or branch is already checked out.",
                        ));
                    }
                    if let Some(base) = &intent.start_point {
                        if repo
                            .find_branch(&intent.branch, git2::BranchType::Local)
                            .is_ok()
                        {
                            return Err(Status::already_exists("New branch already exists."));
                        }
                        let base = if base == "HEAD" {
                            repo.head().map_err(domain)?
                        } else {
                            repo.find_branch(base, git2::BranchType::Local)
                                .map_err(domain)?
                                .into_reference()
                        };
                        base.peel_to_commit().map_err(domain)?;
                    } else {
                        repo.find_branch(&intent.branch, git2::BranchType::Local)
                            .map_err(domain)?;
                    }
                    let create = git::CreateWorktree {
                        repository: root.into(),
                        destination,
                        branch: intent.branch.clone(),
                        new_branch: intent.start_point.is_some(),
                        base: intent.start_point.clone().unwrap_or_default(),
                    };
                    Ok((
                        Prepared::Create(create),
                        vec![format!("Create worktree for branch {}.", intent.branch)],
                    ))
                }
                proto::WorktreeAction::StopWorktree => Ok((
                    Prepared::Stop,
                    vec!["Stop processes in this workspace; preserve files and drafts.".into()],
                )),
                action @ (proto::WorktreeAction::RemoveWorktree
                | proto::WorktreeAction::MergeWorktree) => {
                    if intent.path.as_deref().is_some_and(|p| Path::new(p) != root) {
                        return Err(Status::invalid_argument(
                            "Removal path must be the selected workspace.",
                        ));
                    }
                    if action == proto::WorktreeAction::MergeWorktree {
                        let signing =
                            git::signing::info(&repo.config().map_err(domain)?).map_err(domain)?;
                        if signing.enabled {
                            return Err(domain(
                                "needs_host_interaction: Signed worktree merges must be completed on the host.",
                            ));
                        }
                        if let Some(target) = &intent.target_branch {
                            let info = git::inspect(root)
                                .map_err(domain)?
                                .ok_or_else(|| domain("Repository unavailable."))?;
                            for worktree in info
                                .worktrees
                                .iter()
                                .filter(|w| w.branch_name() == Some(target.as_str()))
                            {
                                let target_repo =
                                    git2::Repository::open(&worktree.path).map_err(domain)?;
                                if git::signing::info(&target_repo.config().map_err(domain)?)
                                    .map_err(domain)?
                                    .enabled
                                {
                                    return Err(domain(
                                        "needs_host_interaction: The merge target requires host signing.",
                                    ));
                                }
                            }
                        }
                    }
                    let request = git::RemoveWorktree {
                        repository: root.into(),
                        path: root.into(),
                        kind: git::RemovalKind::Directory,
                    };
                    let target = intent.target_branch.clone();
                    let action = if action == proto::WorktreeAction::MergeWorktree {
                        git::RemovalAction::Merge {
                            target: required(target.clone())?,
                            delete_branch: intent.delete_branch,
                        }
                    } else if intent.delete_branch {
                        git::RemovalAction::DeleteBranch {
                            target: required(target.clone())?,
                        }
                    } else {
                        git::RemovalAction::KeepBranch
                    };
                    let expected =
                        git::analyze_worktree(&request, target.as_deref()).map_err(domain)?;
                    if expected.branch != intent.branch {
                        return Err(Status::aborted("Worktree branch changed."));
                    }
                    if expected.changed_entries > 0 && !intent.discard_changes {
                        return Err(domain(
                            "Explicit discard approval is required for changed, untracked or ignored files.",
                        ));
                    }
                    let approval = git::RemovalApproval {
                        discard_changes: intent.discard_changes,
                        delete_unmerged_branch: intent.delete_unmerged_branch,
                    };
                    Ok((
                        Prepared::Remove(request, action, expected.clone(), approval),
                        vec![format!(
                            "Remove worktree {}; {} changed entries, {} untracked/ignored entries.",
                            intent.branch, expected.changed_entries, expected.untracked_or_ignored
                        )],
                    ))
                }
                _ => Err(Status::invalid_argument("Worktree action is required.")),
            }
        }
    }
}
fn apply(
    root: &Path,
    prepared: Prepared,
    intent: Intent,
    expected: &proto::GitPrecondition,
    environment: &ShellEnvironment,
) -> Result<Vec<u8>, Status> {
    match (prepared, intent) {
        (Prepared::Paths(files), Intent::Paths(intent)) => {
            let mut current = expected.clone();
            for file in files {
                let edit =
                    match proto::GitPathOperation::try_from(intent.operation).map_err(domain)? {
                        proto::GitPathOperation::StagePaths => changes::Edit::Stage(Some(file)),
                        proto::GitPathOperation::UnstagePaths => changes::Edit::Unstage(Some(file)),
                        proto::GitPathOperation::DiscardPaths => changes::Edit::Discard(file),
                        _ => return Err(domain("Invalid path operation.")),
                    };
                changes::edit_checked(root, edit, &|| {
                    snapshot::check(root, &current).map_err(|e| e.to_string())
                })
                .map_err(domain)?;
                current = snapshot::capture(root)?;
            }
            Ok(Vec::new())
        }
        (Prepared::Commit, Intent::Commit(intent)) => {
            // Re-read configuration at execution; never let a changed signing
            // setting cause an invisible OS interaction.
            prepare(root, &Intent::Commit(intent.clone()))?;
            let result = changes::commit_with_hooks(
                root,
                &intent.message,
                (!expected.head_oid.is_empty()).then_some(expected.head_oid.as_str()),
                environment,
                &AtomicBool::new(false),
                &|_| {},
            )
            .map_err(domain)?;
            let mut output = format!(
                "{}{}",
                result.id,
                result.warning.map(|w| format!("\n{w}")).unwrap_or_default()
            );
            for hook in result.hooks {
                output.push_str(&format!("\n{}:\n{}", hook.name, hook.output));
            }
            if output.len() > crate::host::CONTROL_BYTES / 2 {
                let mut end = crate::host::CONTROL_BYTES / 2;
                while !output.is_char_boundary(end) {
                    end -= 1;
                }
                output.truncate(end);
                output.push_str("\n[Hook output truncated]");
            }
            Ok(output.into_bytes())
        }
        (Prepared::Network(plan, operation, upstream), Intent::Network(_)) => network::execute(
            plan,
            operation,
            Some(upstream),
            Arc::new(AtomicBool::new(false)),
        )
        .map(String::into_bytes)
        .map_err(Status::unknown),
        (Prepared::Create(request), Intent::Worktree(_)) => {
            if request.destination.exists() {
                return Err(Status::aborted("Worktree destination became occupied."));
            }
            git::create_with_metadata_exact(&request)
                .map(|r| r.path.to_string_lossy().as_bytes().to_vec())
                .map_err(domain)
        }
        (Prepared::Remove(request, action, analysis, approval), Intent::Worktree(intent)) => {
            prepare(root, &Intent::Worktree(intent))?;
            match git::execute_worktree_removal(
                &request,
                &action,
                &analysis,
                approval,
                Some(environment),
            )
            .map_err(domain)?
            {
                git::WorktreeRemovalOutcome::Finished(result) => {
                    if !result.worktree_removed && !result.merge_performed && !result.branch_deleted
                    {
                        return Err(domain(
                            result
                                .error
                                .unwrap_or_else(|| "Worktree operation was not applied.".into()),
                        ));
                    }
                    Ok(format!(
                        "removed={} merged={} branch_deleted={}\n{}\n{}",
                        result.worktree_removed,
                        result.merge_performed,
                        result.branch_deleted,
                        result.warning.unwrap_or_default(),
                        result.error.unwrap_or_default()
                    )
                    .into_bytes())
                }
                other => Err(Status::aborted(format!(
                    "Worktree approval changed: {other:?}"
                ))),
            }
        }
        (Prepared::Stop, Intent::Worktree(_)) => Ok(Vec::new()),
        _ => Err(Status::internal("Git preflight type mismatch.")),
    }
}
fn history(root: &Path, request: proto::GitHistoryRequest) -> Result<proto::GitHistory, Status> {
    let repo = snapshot::repository(root)?;
    let limit = if request.limit == 0 {
        50
    } else {
        request.limit.min(100)
    } as usize;
    let (anchor, offset) = match request.before_oid.as_deref() {
        None => (repo.head().ok().and_then(|h| h.target()), 0),
        Some(token) => {
            let (anchor, offset) = token
                .split_once(':')
                .ok_or_else(|| Status::invalid_argument("Invalid history continuation."))?;
            (
                Some(git2::Oid::from_str(anchor).map_err(domain)?),
                offset.parse::<usize>().map_err(domain)?,
            )
        }
    };
    let Some(anchor) = anchor else {
        return Ok(proto::GitHistory {
            commits: vec![],
            next_before_oid: None,
        });
    };
    if offset >= 10_000 {
        return Err(Status::resource_exhausted(
            "History is limited to 10,000 commits.",
        ));
    }
    let mut walk = repo.revwalk().map_err(domain)?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
        .map_err(domain)?;
    walk.push(anchor).map_err(domain)?;
    let mut commits = Vec::new();
    let mut more = false;
    let mut size = 0;
    for oid in walk.skip(offset).take(limit + 1) {
        let oid = oid.map_err(domain)?;
        if commits.len() == limit {
            more = true;
            break;
        }
        let commit = repo.find_commit(oid).map_err(domain)?;
        let entry = proto::GitCommit {
            oid: oid.to_string(),
            title: commit
                .summary()
                .map_err(domain)?
                .unwrap_or("")
                .chars()
                .take(4096)
                .collect(),
            author: commit
                .author()
                .name()
                .unwrap_or("")
                .chars()
                .take(1024)
                .collect(),
            time_unix: commit.time().seconds(),
            parents: commit
                .parent_ids()
                .map(|id| id.to_string())
                .take(256)
                .collect(),
        };
        size += entry.encoded_len();
        if size > crate::host::CONTROL_BYTES - 1024 {
            more = true;
            break;
        }
        commits.push(entry);
    }
    let next_before_oid = more.then(|| format!("{anchor}:{}", offset + commits.len()));
    Ok(proto::GitHistory {
        commits,
        next_before_oid,
    })
}
fn read_diff(root: &Path, request: &proto::DiffRequest) -> Result<changes::FileDiff, Status> {
    crate::files::resolve(root, Path::new(&request.path)).map_err(domain)?;
    if let Some(commit) = &request.commit {
        let repo = snapshot::repository(root)?;
        let commit = repo
            .find_commit(git2::Oid::from_str(commit).map_err(domain)?)
            .map_err(domain)?;
        let tree = commit.tree().map_err(domain)?;
        let parent = if commit.parent_count() > 0 {
            Some(commit.parent(0).map_err(domain)?.tree().map_err(domain)?)
        } else {
            None
        };
        let mut options = git2::DiffOptions::new();
        options
            .disable_pathspec_match(true)
            .pathspec(&request.path)
            .max_size(2 * 1024 * 1024);
        let diff = repo
            .diff_tree_to_tree(parent.as_ref(), Some(&tree), Some(&mut options))
            .map_err(domain)?;
        let mut lines = Vec::new();
        let mut bytes = 0;
        let mut truncated = false;
        diff.print(git2::DiffFormat::Patch, |_, _, line| {
            if lines.len() >= 20_000 || bytes + line.content().len() > 2 * 1024 * 1024 {
                truncated = true;
                return false;
            }
            bytes += line.content().len();
            let text = String::from_utf8_lossy(line.content());
            if text.len() > 16 * 1024 {
                truncated = true;
            }
            lines.push(changes::DiffLine {
                old: line.old_lineno(),
                new: line.new_lineno(),
                kind: line.origin(),
                text: text.chars().take(4096).collect(),
            });
            true
        })
        .or_else(|e| if truncated { Ok(()) } else { Err(e) })
        .map_err(domain)?;
        let binary = diff
            .deltas()
            .any(|d| d.old_file().is_binary() || d.new_file().is_binary());
        Ok(changes::FileDiff {
            lines,
            binary,
            truncated,
        })
    } else {
        let status = changes::status(root).map_err(domain)?;
        let file = status
            .files
            .into_iter()
            .find(|f| f.path == Path::new(&request.path) && f.staged == request.staged)
            .ok_or_else(|| Status::not_found("Git change not found."))?;
        changes::diff(root, &file).map_err(domain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn approved_stage_executes_once_and_stale_plan_preserves_index() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("repo");
        std::fs::create_dir(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("file.txt"), "one").unwrap();
        let (auth, grant) = super::super::tests::fixture(&root, &dir.path().join("db")).await;
        let roots = WorkspaceRoots::default();
        roots
            .replace(HashMap::from([("workspace".into(), root.clone())]))
            .unwrap();
        let worker = GitClient::start().unwrap();
        let service = GitRpc::new(
            auth.clone(),
            roots,
            worker.clone(),
            ShellEnvironment {
                shell: PathBuf::from("/bin/sh"),
                vars: HashMap::new(),
            },
            ResourceLocks::default(),
        );
        let request = proto::GitPreflightRequest {
            workspace: Some(proto::WorkspaceRef {
                host_id: auth.host_id().into(),
                workspace_id: "workspace".into(),
            }),
            intent: Some(Intent::Paths(proto::GitPathIntent {
                operation: proto::GitPathOperation::StagePaths as i32,
                paths: vec!["file.txt".into()],
            })),
        };
        let mut expected = snapshot::capture(&root).unwrap();
        let (prepared, _) = prepare(&root, request.intent.as_ref().unwrap()).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        expected.preflight_id = Some(id.clone());
        service.plans.lock().unwrap().insert(
            id,
            ApprovedPlan {
                device: grant.id().into(),
                root: root.clone(),
                request: request.clone(),
                expected: expected.clone(),
                expires: Instant::now() + Duration::from_secs(300),
                prepared,
                destinations: Vec::new(),
                destination_snapshots: Vec::new(),
            },
        );
        let meta = super::super::tests::meta(&auth);
        let result = service
            .execute(
                grant.clone(),
                meta.clone(),
                request.clone(),
                expected.clone(),
                "git.stage",
            )
            .await
            .unwrap()
            .into_inner();
        assert_eq!(result.state, proto::OperationState::Succeeded as i32);
        assert!(
            repo.index()
                .unwrap()
                .get_path(Path::new("file.txt"), 0)
                .is_some()
        );
        assert_eq!(
            service
                .execute(grant.clone(), meta, request.clone(), expected, "git.stage")
                .await
                .unwrap()
                .into_inner(),
            result
        );
        std::fs::write(root.join("file.txt"), "two").unwrap();
        let mut expected = snapshot::capture(&root).unwrap();
        let (prepared, _) = prepare(&root, request.intent.as_ref().unwrap()).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        expected.preflight_id = Some(id.clone());
        service.plans.lock().unwrap().insert(
            id,
            ApprovedPlan {
                device: grant.id().into(),
                root: root.clone(),
                request: request.clone(),
                expected: expected.clone(),
                expires: Instant::now() + Duration::from_secs(300),
                prepared,
                destinations: Vec::new(),
                destination_snapshots: Vec::new(),
            },
        );
        std::fs::write(root.join("file.txt"), "new external edit").unwrap();
        let before = repo
            .index()
            .unwrap()
            .get_path(Path::new("file.txt"), 0)
            .unwrap()
            .id;
        let failure = service
            .execute(
                grant,
                super::super::tests::meta(&auth),
                request,
                expected,
                "git.stage",
            )
            .await
            .unwrap()
            .into_inner();
        assert_eq!(failure.state, proto::OperationState::Failed as i32);
        assert_eq!(failure.error.unwrap().code, "conflict");
        assert_eq!(
            repo.index()
                .unwrap()
                .get_path(Path::new("file.txt"), 0)
                .unwrap()
                .id,
            before
        );
        worker.shutdown().await;
        auth.client().shutdown().await.unwrap();
    }
}
