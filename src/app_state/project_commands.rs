//! Project catalog operations of the remote bridge: listing workspaces with their
//! canonical roots, opening a folder and closing a project after a preflight that is
//! checked again right before the effect. Each mutation has the durable command states;
//! none needs a window or changes what the host user is looking at.
use super::{AppState, chats::RemoteKey};
use canopy_desktop::{
    host::{CHAT, CommandInput, CommandReceipt, CommandState},
    state::workspace::{PaneId, WorkspaceId},
};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

/// One open project or worktree as a client lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceSummary {
    pub workspace: WorkspaceId,
    pub name: String,
    /// What device grants and file/Git operations are checked against.
    pub canonical_root: PathBuf,
    /// The repository a worktree belongs to (`None`: a plain folder or the main one).
    pub repository: Option<PathBuf>,
    /// Closed, but its chats' cleanup failed: it stays listed (and its authority
    /// granted) only so the close can be retried on the same target.
    pub closing: bool,
}

/// A project closed by a remote command whose chats' cleanup has not settled: the
/// catalog no longer has it, but its identity and checkout stay with this owner until
/// the cleanup succeeds, so a device can retry the close (never a false removal).
#[derive(Clone, Debug)]
struct Closing {
    name: String,
    checkout: PathBuf,
    repository: Option<PathBuf>,
}
#[derive(Default)]
struct ClosingProjects(HashMap<WorkspaceId, Closing>);
impl Global for ClosingProjects {}

/// A listed project: ID, name, checkout, repository and whether it is closing.
pub type Listed = (WorkspaceId, String, PathBuf, Option<PathBuf>, bool);

/// The catalog's projects and the closed ones whose cleanup still waits for a retry.
pub fn listed(cx: &App) -> Vec<Listed> {
    let mut listed: Vec<Listed> = catalog(cx)
        .into_iter()
        .map(|(id, name, path, repository)| (id, name, path, repository, false))
        .collect();
    if let Some(closing) = cx.try_global::<ClosingProjects>() {
        let chats = cx.global::<AppState>().chats.read(cx);
        for (id, project) in &closing.0 {
            if listed.iter().all(|(listed, ..)| listed != id)
                && !chats.unsettled_cleanup(*id).is_empty()
            {
                listed.push((
                    *id,
                    project.name.clone(),
                    project.checkout.clone(),
                    project.repository.clone(),
                    true,
                ));
            }
        }
    }
    listed
}

/// `resolve` for `listed`.
pub fn resolve_listed(projects: Vec<Listed>) -> Vec<WorkspaceSummary> {
    projects
        .into_iter()
        .filter_map(|(workspace, name, path, repository, closing)| {
            Some(WorkspaceSummary {
                workspace,
                name,
                canonical_root: std::fs::canonicalize(path).ok()?,
                repository,
                closing,
            })
        })
        .collect()
}

/// What closing a project loses now; the close is refused when this changed since.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectClosePreflight {
    pub workspace: WorkspaceId,
    pub panes: Vec<PaneId>,
    /// Its chats (with and without a tab), deleted with it.
    pub chats: Vec<String>,
    pub working: usize,
    pub unsent: usize,
    pub observers: usize,
    /// Unsaved editor files on the host: only the host user can save or discard them.
    pub dirty_editors: bool,
    pub running: bool,
}

fn reason(text: &str) -> Option<Vec<u8>> {
    serde_json::to_vec(&serde_json::json!({ "reason": text })).ok()
}

/// The catalog's projects with their checkout paths (not yet canonical).
pub fn catalog(cx: &App) -> Vec<(WorkspaceId, String, PathBuf, Option<PathBuf>)> {
    cx.global::<AppState>()
        .projects
        .read(cx)
        .catalog
        .items
        .iter()
        .map(|p| {
            (
                p.workspace,
                p.name.clone(),
                p.worktree_path.clone().unwrap_or_else(|| p.path.clone()),
                p.repository_path.clone(),
            )
        })
        .collect()
}

/// Canonical roots, read on a worker. A checkout gone from disk is left out (it can't
/// be granted or worked in).
pub fn resolve(
    projects: Vec<(WorkspaceId, String, PathBuf, Option<PathBuf>)>,
) -> Vec<WorkspaceSummary> {
    resolve_listed(
        projects
            .into_iter()
            .map(|(id, name, path, repository)| (id, name, path, repository, false))
            .collect(),
    )
}

/// Opens folder `path` (or the checkout containing it) as a project without
/// selecting it on the host. Result: the workspace ID as JSON.
pub fn open_project(
    key: RemoteKey,
    path: PathBuf,
    cx: &mut App,
) -> Task<Result<CommandReceipt, String>> {
    let app = cx.global::<AppState>().clone();
    let Some(client) = app.settings.read(cx).client() else {
        return Task::ready(Err("Host storage is unavailable.".into()));
    };
    cx.spawn(async move |cx| {
        let requested = path.clone();
        let folder = cx
            .background_spawn(async move { canopy_desktop::git::workspace_directory(&requested) })
            .await
            .map_err(|_| "This folder is unavailable or cannot be read.".to_owned())?;
        let root = folder
            .to_str()
            .ok_or("This folder's path can't be used.")?
            .to_owned();
        let input = command(key, "projects.open", root, &path)
            .ok_or("This folder can't be opened as asked.")?;
        let key = input.key().clone();
        let receipt = client
            .accept_runtime_command(input, CHAT)
            .await
            .map_err(|e| e.to_string())?;
        if receipt.state() != CommandState::Accepted {
            return Ok(receipt);
        }
        client
            .transition_runtime_command(
                key.clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        let opened = cx.update(|cx| {
            app.projects.update(cx, |projects, cx| {
                projects.open_without_selecting(folder, cx)
            })
        });
        let (state, result) = match opened {
            Ok(id) => {
                let committed = cx.update(|cx| {
                    app.projects
                        .update(cx, |projects, cx| projects.committed(cx))
                });
                match committed.await {
                    Ok(Ok(())) => (CommandState::Succeeded, serde_json::to_vec(&id).ok()),
                    _ => (
                        CommandState::DeliveryUnknown,
                        reason("The project opened, but its saved state could not be confirmed."),
                    ),
                }
            }
            Err(error) => (CommandState::Failed, reason(&error)),
        };
        client
            .transition_runtime_command(key, CommandState::Attempting, state, result)
            .await
            .map_err(|e| e.to_string())
    })
}

/// The inbox record of a project command under `root` (also its target).
fn command(
    key: RemoteKey,
    method: &str,
    root: String,
    arguments: &impl Serialize,
) -> Option<CommandInput> {
    let arguments = serde_json::to_vec(arguments).ok()?;
    let typed = Sha256::digest(&arguments).to_vec();
    let input = CommandInput::new(key.key, method.into(), root.clone(), arguments, typed)?;
    match key.fingerprint {
        Some(fingerprint) => input.with_fingerprint(fingerprint),
        None => Some(input),
    }?
    .with_root(root)
}

/// What closing project `id` loses now. A closed project whose cleanup failed has
/// only the chats still waiting for it (closing it again retries their cleanup).
pub fn close_preflight(id: WorkspaceId, cx: &App) -> Result<ProjectClosePreflight, String> {
    let app = cx.global::<AppState>();
    let projects = app.projects.read(cx);
    if !projects.catalog.items.iter().any(|p| p.workspace == id) {
        let closing = cx
            .try_global::<ClosingProjects>()
            .is_some_and(|c| c.0.contains_key(&id));
        let waiting = app.chats.read(cx).unsettled_cleanup(id);
        if !closing || waiting.is_empty() {
            return Err("resource_deleted".into());
        }
        return Ok(ProjectClosePreflight {
            workspace: id,
            panes: vec![],
            chats: waiting.into_iter().filter_map(|(_, chat)| chat).collect(),
            working: 0,
            unsent: 0,
            observers: 0,
            dirty_editors: false,
            running: false,
        });
    }
    let panes: Vec<PaneId> = projects
        .runtime_workspaces(cx)
        .iter()
        .filter(|w| w.id == id)
        .flat_map(|w| w.all_panes())
        .map(|p| p.id)
        .collect();
    let registry = app.chats.read(cx);
    let summaries: Vec<_> = registry
        .chat_summaries(cx)
        .into_iter()
        .filter(|s| registry.workspace_of_chat(&s.chat) == Some(id))
        .collect();
    Ok(ProjectClosePreflight {
        workspace: id,
        panes,
        chats: summaries.iter().map(|s| s.chat.clone()).collect(),
        working: summaries.iter().filter(|s| s.working).count(),
        unsent: summaries.iter().map(|s| s.unsent).sum(),
        observers: summaries.iter().map(|s| s.observers).sum(),
        dirty_editors: app.editors.read(cx).busy(
            &projects
                .runtime_workspaces(cx)
                .iter()
                .filter(|w| w.id == id)
                .flat_map(|w| w.all_panes())
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            cx,
        ),
        running: app.workspace_running(id, cx),
    })
}

/// Closes a project as the user confirmed in `expected`: refused when what it loses
/// changed or the host has unsaved editor files there. Succeeds only once its chats'
/// cleanup finished (LIFE-04). When the cleanup fails the command fails and the
/// project stays listed as closing; closing it again (a new preflight and command on
/// the same workspace) retries the cleanup instead of closing anything else.
pub fn close_project(
    key: RemoteKey,
    expected: ProjectClosePreflight,
    cx: &mut App,
) -> Task<Result<CommandReceipt, String>> {
    let app = cx.global::<AppState>().clone();
    let Some(client) = app.settings.read(cx).client() else {
        return Task::ready(Err("Host storage is unavailable.".into()));
    };
    let listed = catalog(cx)
        .into_iter()
        .find(|(id, ..)| *id == expected.workspace)
        .map(|(_, name, path, repository)| Closing {
            name,
            checkout: path,
            repository,
        });
    let retry = listed.is_none();
    let project = listed.or_else(|| {
        cx.try_global::<ClosingProjects>()
            .and_then(|c| c.0.get(&expected.workspace).cloned())
    });
    cx.spawn(async move |cx| {
        let project = project.ok_or("resource_deleted")?;
        let checkout = project.checkout.clone();
        let root = cx
            .background_spawn(async move { std::fs::canonicalize(checkout) })
            .await
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned))
            .ok_or("The project's folder is unavailable.")?;
        let input = command(key, "projects.close", root, &expected)
            .ok_or("This project can't be closed as asked.")?;
        let key = input.key().clone();
        let receipt = client
            .accept_runtime_command(input, CHAT)
            .await
            .map_err(|e| e.to_string())?;
        if receipt.state() != CommandState::Accepted {
            return Ok(receipt);
        }
        let current = cx.update(|cx| close_preflight(expected.workspace, cx));
        let refusal = match &current {
            Err(error) => Some(error.clone()),
            Ok(current) if current.dirty_editors => {
                Some("needs_host_interaction: unsaved editor files on the host".into())
            }
            Ok(current) if *current != expected => Some("conditions_changed".into()),
            Ok(_) => None,
        };
        if let Some(refusal) = refusal {
            return client
                .transition_runtime_command(
                    key,
                    CommandState::Accepted,
                    CommandState::Rejected,
                    reason(&refusal),
                )
                .await
                .map_err(|e| e.to_string());
        }
        client
            .transition_runtime_command(
                key.clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        let workspace = expected.workspace;
        let closing = cx.update(|cx| {
            cx.default_global::<ClosingProjects>()
                .0
                .insert(workspace, project.clone());
            if retry {
                return Ok(app.chats.update(cx, |chats, cx| {
                    chats.retry_cleanup_of(workspace, cx);
                    chats.cleanup_of(workspace, cx)
                }));
            }
            app.projects.update(cx, |projects, cx| {
                projects.close_removed(workspace, cx)?;
                // The close itself can be refused (tabs changed meanwhile).
                if projects
                    .catalog
                    .items
                    .iter()
                    .any(|p| p.workspace == workspace)
                {
                    return Err(projects
                        .error
                        .clone()
                        .unwrap_or_else(|| "The project could not be closed.".into()));
                }
                Ok(())
            })?;
            Ok::<_, String>(app.chats.update(cx, |chats, cx| {
                chats.sync_model(cx);
                chats.cleanup_of(workspace, cx)
            }))
        });
        let (state, result) = match closing {
            Err(error) => {
                cx.update(|cx| {
                    cx.default_global::<ClosingProjects>().0.remove(&workspace);
                });
                (CommandState::Failed, reason(&error))
            }
            Ok(cleanup) => {
                let mut outcome = Ok(());
                for job in cleanup {
                    let done = job.await;
                    if outcome.is_ok() {
                        outcome = done;
                    }
                }
                match outcome {
                    Ok(()) => {
                        cx.update(|cx| {
                            cx.default_global::<ClosingProjects>().0.remove(&workspace);
                        });
                        (CommandState::Succeeded, None)
                    }
                    // The project stays listed as closing: closing it again retries.
                    Err(error) => (
                        CommandState::Failed,
                        reason(&format!(
                            "cleanup_failed: {error} Close the project again to retry its cleanup."
                        )),
                    ),
                }
            }
        };
        client
            .transition_runtime_command(key, CommandState::Attempting, state, result)
            .await
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests;
