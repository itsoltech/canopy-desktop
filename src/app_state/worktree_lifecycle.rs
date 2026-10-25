//! The app side of remote worktree Stop, Remove and Merge: the same process, editor and
//! chat rules as the local worktree dialog, run by their owners on the app thread. The
//! remote service reaches them through a bounded channel; it never holds app state.
use super::AppState;
use canopy_desktop::{
    remote::{
        proto::{GitWorktreeIntent, WorktreeAction},
        resources::WorktreeLifecycle,
    },
    state::workspace::{PaneId, WorkspaceId},
};
use futures_channel::oneshot;
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use tonic::Status;

/// Requests waiting for the app thread; more are refused, never queued unbounded.
const PENDING: usize = 16;

enum Request {
    Preflight(
        PathBuf,
        GitWorktreeIntent,
        oneshot::Sender<Result<Vec<String>, String>>,
    ),
    Apply(
        PathBuf,
        GitWorktreeIntent,
        oneshot::Sender<Result<(), String>>,
    ),
    Finished(PathBuf, bool, oneshot::Sender<Result<(), String>>),
}

struct Lifecycle(async_channel::Sender<Request>);

/// What Remove or Merge paused for its Git mutation, released when it finished.
struct Paused {
    workspace: WorkspaceId,
    paths: Vec<PathBuf>,
    panes: Vec<PaneId>,
}

/// Starts the app-side owner; the remote Git service gets it with `with_lifecycle`.
pub fn install(cx: &mut App) -> Arc<dyn WorktreeLifecycle> {
    let (sender, requests) = async_channel::bounded(PENDING);
    cx.spawn(async move |cx| {
        let mut paused: HashMap<PathBuf, Paused> = HashMap::new();
        while let Ok(request) = requests.recv().await {
            match request {
                Request::Preflight(root, intent, reply) => {
                    let _ = reply.send(full_preflight(&root, &intent, cx).await);
                }
                Request::Apply(root, intent, reply) => {
                    let result = match workspace_of(&root, cx).await {
                        Some(id) => apply(id, root.clone(), &intent, &mut paused, cx).await,
                        None => Err("This worktree is not open on the host.".into()),
                    };
                    let _ = reply.send(result);
                }
                Request::Finished(root, removed, reply) => {
                    let result = match paused.remove(&root) {
                        Some(paused) => cx.update(|cx| finished(paused, root, removed, cx)),
                        None => Ok(()),
                    };
                    let _ = reply.send(result);
                }
            }
        }
    })
    .detach();
    Arc::new(Lifecycle(sender))
}

/// Source checks, then (Merge) the target branch's checkout: its editors, processes,
/// cleanup failures and other devices' unsaved edits block the merge like locally.
async fn full_preflight(
    root: &std::path::Path,
    intent: &GitWorktreeIntent,
    cx: &mut AsyncApp,
) -> Result<Vec<String>, String> {
    let id = workspace_of(root, cx)
        .await
        .ok_or("This worktree is not open on the host.")?;
    let mut impacts = dirty_elsewhere(root, intent, cx).await?;
    impacts.extend(cx.update(|cx| preflight(id, intent, cx))?);
    if let Some((target, target_root)) = merge_target(id, intent, cx).await? {
        let branch = intent.target_branch.clone().unwrap_or_default();
        cx.update(|cx| target_ready(target, &branch, cx))?;
        // The merge writes into the target checkout: unsaved edits there are lost
        // only with the same explicit acknowledgement.
        let dirty = dirty_elsewhere(&target_root, intent, cx).await?;
        impacts.extend(
            dirty
                .into_iter()
                .map(|d| format!("Target branch '{branch}': {d}")),
        );
    }
    Ok(impacts)
}

/// The open checkout of the merge's target branch, when it has one.
async fn merge_target(
    source: WorkspaceId,
    intent: &GitWorktreeIntent,
    cx: &mut AsyncApp,
) -> Result<Option<(WorkspaceId, PathBuf)>, String> {
    if intent.action != WorktreeAction::MergeWorktree as i32 {
        return Ok(None);
    }
    let Some(branch) = intent.target_branch.clone() else {
        return Ok(None);
    };
    let path = cx.update(|cx| {
        let app = cx.global::<AppState>();
        let projects = app.projects.read(cx);
        let project = projects
            .catalog
            .items
            .iter()
            .find(|p| p.workspace == source)?;
        let repository = project
            .repository_path
            .clone()
            .unwrap_or_else(|| project.path.clone());
        app.git
            .read(cx)
            .repository(&repository)?
            .worktrees
            .iter()
            .find(|w| w.branch_name() == Some(branch.as_str()) && w.available)
            .map(|w| w.path.clone())
    });
    let Some(path) = path else {
        return Ok(None);
    };
    let canonical = cx
        .background_spawn(async move { std::fs::canonicalize(&path).unwrap_or(path) })
        .await;
    Ok(workspace_of(&canonical, cx).await.map(|id| (id, canonical)))
}

/// The target checkout can take a merge now (the local rules of the worktree dialog).
fn target_ready(target: WorkspaceId, branch: &str, cx: &mut App) -> Result<(), String> {
    let app = cx.global::<AppState>().clone();
    if app.editors.read(cx).busy(&panes_of(target, cx), cx) {
        return Err(format!(
            "Save or close unsaved editor files in target branch '{branch}' before merging."
        ));
    }
    if app.workspace_running(target, cx) {
        return Err(format!(
            "Stop processes in target branch '{branch}' before merging."
        ));
    }
    if let Some(error) = app.terminals.read(cx).workspace_cleanup_error(target, cx) {
        return Err(format!(
            "Resolve the previous terminal cleanup failure in target branch '{branch}' before merging: {error}"
        ));
    }
    Ok(())
}

/// The open project whose checkout is `root` (canonical), matched off the app thread.
async fn workspace_of(root: &std::path::Path, cx: &mut AsyncApp) -> Option<WorkspaceId> {
    let candidates: Vec<(WorkspaceId, PathBuf)> = cx.update(|cx| {
        cx.global::<AppState>()
            .projects
            .read(cx)
            .catalog
            .items
            .iter()
            .map(|p| {
                (
                    p.workspace,
                    p.worktree_path.clone().unwrap_or_else(|| p.path.clone()),
                )
            })
            .collect()
    });
    let root = root.to_owned();
    cx.background_spawn(async move {
        candidates.into_iter().find_map(|(id, path)| {
            (std::fs::canonicalize(&path).unwrap_or(path) == root).then_some(id)
        })
    })
    .await
}

/// Unsaved buffers other devices reported for files of this checkout, also of files
/// gone from disk and of devices offline now (APP-04). Removing or merging it loses
/// them unless the user acknowledged that; Stop keeps the files and only lists them.
/// A list too long to read whole refuses the operation (never a partial answer).
async fn dirty_elsewhere(
    root: &std::path::Path,
    intent: &GitWorktreeIntent,
    cx: &mut AsyncApp,
) -> Result<Vec<String>, String> {
    let Some(client) = cx.update(|cx| cx.global::<AppState>().settings.read(cx).client()) else {
        return Err("Host storage is unavailable.".into());
    };
    let root = root
        .to_str()
        .ok_or("This worktree's path can't be checked for unsaved edits.")?
        .to_owned();
    let presence = client.runtime_root_presence(root).await.map_err(|error| match error {
        canopy_desktop::settings::Error::ResourceLimit => {
            "Too many devices report unsaved edits in this worktree to list them; resolve some first."
                .to_owned()
        }
        other => format!("Unsaved edits on other devices couldn't be checked: {other}"),
    })?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let found: Vec<String> = presence
        .iter()
        .filter(|(_, p)| p.device_id() != canopy_desktop::host::LOCAL_PRINCIPAL)
        .map(|(file, p)| {
            let minutes = now.saturating_sub(p.last_seen()) / 60;
            format!(
                "{} has unsaved edits in {} (last seen {}).",
                p.device_name(),
                file.relative_path(),
                if minutes == 0 {
                    "just now".to_owned()
                } else {
                    format!("{minutes} min ago")
                }
            )
        })
        .collect();
    let stop = intent.action == WorktreeAction::StopWorktree as i32;
    let active = cx.update(|cx| {
        let remote = cx.global::<AppState>().remote.read(cx);
        presence
            .iter()
            .any(|(_, p)| remote.device_online(p.device_id()))
    });
    if active && !stop {
        return Err("A connected device has unsaved edits in this worktree. Save or discard them on that device before changing the worktree.".into());
    }
    if !found.is_empty() && !stop && !intent.acknowledge_offline_dirty {
        return Err(format!(
            "Other devices have unsaved edits in this worktree. {}",
            found.join(" ")
        ));
    }
    Ok(found)
}

fn panes_of(id: WorkspaceId, cx: &App) -> Vec<PaneId> {
    cx.global::<AppState>()
        .projects
        .read(cx)
        .runtime_workspaces(cx)
        .iter()
        .filter(|w| w.id == id)
        .flat_map(|w| w.all_panes())
        .map(|p| p.id)
        .collect()
}

/// What the operation affects, or why it can't run now.
fn preflight(
    id: WorkspaceId,
    intent: &GitWorktreeIntent,
    cx: &mut App,
) -> Result<Vec<String>, String> {
    let app = cx.global::<AppState>().clone();
    let stop = intent.action == WorktreeAction::StopWorktree as i32;
    if let Some(error) = app.terminals.read(cx).workspace_cleanup_error(id, cx) {
        return Err(format!(
            "Resolve the previous terminal cleanup failure first: {error}"
        ));
    }
    if !stop && app.editors.read(cx).busy(&panes_of(id, cx), cx) {
        return Err("Unsaved editor files on the host must be saved or closed first.".into());
    }
    let mut impacts = vec![];
    let chats = app.chats.read(cx);
    for summary in chats.chat_summaries(cx) {
        let in_workspace = chats.workspace_of_chat(&summary.chat) == Some(id);
        if !in_workspace {
            continue;
        }
        let title = summary.title.clone().unwrap_or_else(|| "Chat".into());
        let mut parts = vec![];
        if summary.working {
            parts.push("the agent is working".to_owned());
        }
        if summary.unsent > 0 {
            parts.push(format!("{} queued message(s)", summary.unsent));
        }
        if summary.observers > 0 {
            parts.push(format!("{} connected device(s)", summary.observers));
        }
        let effect = if stop {
            "stops; its conversation stays"
        } else {
            "is deleted with its conversation"
        };
        impacts.push(if parts.is_empty() {
            format!("Chat “{title}” {effect}.")
        } else {
            format!("Chat “{title}” {effect} ({}).", parts.join(", "))
        });
    }
    if app.terminals.read(cx).workspace_running(id, cx) {
        impacts.push("Running terminals in this worktree are stopped.".into());
    }
    Ok(impacts)
}

/// Stop: every process of the worktree ends (confirmed). Remove and Merge: the same,
/// then files and editors let go of the checkout until `finished`.
async fn apply(
    id: WorkspaceId,
    root: PathBuf,
    intent: &GitWorktreeIntent,
    paused: &mut HashMap<PathBuf, Paused>,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let stop = cx.update(|cx| {
        let app = cx.global::<AppState>().clone();
        app.workspace_running(id, cx)
            .then(|| app.stop_workspace_processes(id, cx))
    });
    if let Some(stop) = stop {
        stop.await
            .map_err(|error| format!("Could not confirm process cleanup: {error}"))?;
    }
    if intent.action == WorktreeAction::StopWorktree as i32 {
        return Ok(());
    }
    let target = merge_target(id, intent, cx).await?;
    cx.update(|cx| {
        let app = cx.global::<AppState>().clone();
        let mut panes = panes_of(id, cx);
        if app.editors.read(cx).busy(&panes, cx) {
            return Err(
                "Unsaved editor files on the host must be saved or closed first.".to_owned(),
            );
        }
        let mut paths = vec![root.clone()];
        // The target checkout is written by the merge: checked again, then paused too.
        if let Some((target, target_root)) = target {
            target_ready(
                target,
                intent.target_branch.as_deref().unwrap_or_default(),
                cx,
            )?;
            panes.extend(panes_of(target, cx));
            paths.push(target_root);
        }
        super::git::pause_cleanup_handles(&app, &paths, &panes, cx);
        paused.insert(
            root,
            Paused {
                workspace: id,
                paths,
                panes,
            },
        );
        Ok(())
    })
}

/// The Git mutation ended: handles resume; a removed checkout closes its project and
/// deletes its chats as the preflight listed (no second prompt on the host).
fn finished(paused: Paused, root: PathBuf, removed: bool, cx: &mut App) -> Result<(), String> {
    let app = cx.global::<AppState>().clone();
    let removed_paths = if removed {
        HashSet::from([root])
    } else {
        HashSet::new()
    };
    super::git::resume_cleanup_handles(&app, &paused.paths, &paused.panes, &removed_paths, cx);
    if removed {
        app.projects.update(cx, |projects, cx| {
            projects.close_removed(paused.workspace, cx)
        })?;
        app.chats.update(cx, |chats, cx| {
            if chats.sync_model(cx) {
                cx.notify();
            }
        });
    }
    Ok(())
}

fn status(error: String) -> Status {
    if error.contains("not open") {
        Status::not_found(error)
    } else {
        Status::failed_precondition(error)
    }
}

impl Lifecycle {
    async fn ask<T>(
        &self,
        request: impl FnOnce(oneshot::Sender<Result<T, String>>) -> Request,
    ) -> Result<T, Status> {
        let (reply, answer) = oneshot::channel();
        self.0.try_send(request(reply)).map_err(|_| {
            Status::resource_exhausted("The host is busy with other worktree operations.")
        })?;
        answer
            .await
            .map_err(|_| Status::unavailable("Canopy is closing."))?
            .map_err(status)
    }
}

#[tonic::async_trait]
impl WorktreeLifecycle for Lifecycle {
    async fn preflight(
        &self,
        root: PathBuf,
        intent: GitWorktreeIntent,
    ) -> Result<Vec<String>, Status> {
        self.ask(|reply| Request::Preflight(root, intent, reply))
            .await
    }
    async fn apply(&self, root: PathBuf, intent: GitWorktreeIntent) -> Result<(), Status> {
        self.ask(|reply| Request::Apply(root, intent, reply)).await
    }
    async fn finished(&self, root: PathBuf, removed: bool) -> Result<(), Status> {
        self.ask(|reply| Request::Finished(root, removed, reply))
            .await
    }
}
