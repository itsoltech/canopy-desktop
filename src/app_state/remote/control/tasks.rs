use super::*;
use canopy_desktop::{
    integrations::{self as i, filters::TaskFilter},
    remote::tasks::TasksHost,
    state::workspace::WorkspaceId,
};
#[tonic::async_trait]
impl TasksHost for ControlBridge {
    async fn save_filter(&self, filter: TaskFilter) -> Result<(), Status> {
        self.request(|reply| Op::Filter(Some(filter), String::new(), reply))
            .await
    }
    async fn remove_filter(&self, id: String) -> Result<(), Status> {
        self.request(|reply| Op::Filter(None, id, reply)).await
    }
    async fn task_to_worktree(
        &self,
        workspace: proto::WorkspaceRef,
        task: i::TaskRef,
        branch: String,
    ) -> Result<String, Status> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| busy("Remote runtime is unavailable."))?;
        self.request(|reply| Op::Worktree(workspace, task, branch, runtime, reply))
            .await
    }
}
pub(super) fn filter(
    filter: Option<TaskFilter>,
    id: String,
    cx: &mut App,
) -> Result<Committed, Status> {
    let owner = cx.global::<AppState>().integrations.clone();
    owner.update(cx, |state, cx| {
        let config = match filter {
            Some(filter) => state.config.save_filter(filter).map_err(invalid)?,
            None => state.config.remove_filter(&id),
        };
        let (tx, rx) = oneshot::channel();
        state
            .save_config_committed(config, "Task filter", None, None, Some(tx), cx)
            .map_err(busy)?;
        Ok(rx)
    })
}
pub(super) fn worktree(
    workspace: proto::WorkspaceRef,
    task: i::TaskRef,
    branch: String,
    runtime: tokio::runtime::Handle,
    locks: ResourceLocks,
    reply: Reply<String>,
    cx: &mut App,
) {
    let app = cx.global::<AppState>().clone();
    let context = (|| {
        if app.settings.read(cx).host_id() != Some(workspace.host_id.as_str()) {
            return Err(invalid("Workspace belongs to another host."));
        }
        let id: WorkspaceId = serde_json::from_str(&workspace.workspace_id).map_err(invalid)?;
        let projects = app.projects.read(cx);
        if !projects.ready || projects.busy || projects.quitting {
            return Err(busy("Projects are busy."));
        }
        let project = projects
            .catalog
            .items
            .iter()
            .find(|p| p.workspace == id)
            .ok_or_else(|| Status::not_found("Workspace was removed."))?;
        let path = project
            .worktree_path
            .clone()
            .unwrap_or_else(|| project.path.clone());
        let git = app
            .git
            .read(cx)
            .client()
            .ok_or_else(|| busy("Git is starting."))?;
        if !task.valid() {
            return Err(invalid("Invalid task reference."));
        }
        Ok((path, git))
    })();
    let (path, git) = match context {
        Ok(context) => context,
        Err(error) => {
            let _ = reply.send(Err(error));
            return;
        }
    };
    // Resource work runs on Tokio (GitClient's worker uses spawn_blocking), while
    // publication goes back through the live feature owners.
    let create = runtime.spawn(async move {
        let root = std::fs::canonicalize(&path)
            .map_err(|_| Status::not_found("Workspace folder is unavailable."))?;
        let _gate = locks.acquire(&root).await?;
        let repository = root.clone();
        let created = git
            .run_resource_operation(move || {
                let destination = canopy_desktop::git::propose_worktree_path(&root)?;
                canopy_desktop::git::create_with_metadata(&canopy_desktop::git::CreateWorktree {
                    repository: root,
                    destination,
                    branch,
                    new_branch: true,
                    base: "HEAD".into(),
                })
            })
            .await
            .map_err(|_| Status::unknown("Worktree outcome requires reconciliation."))?
            .map_err(|_| {
                Status::unknown(
                    "Worktree creation did not complete; inspect the host before retrying.",
                )
            })?;
        Ok::<_, Status>((repository, created, _gate))
    });
    cx.spawn(async move |cx| {
        let result = async {
            let (repository, created, _gate) = create
                .await
                .map_err(|_| Status::unknown("Worktree worker stopped."))??;
            let path = created.path.clone();
            let (id, catalog_done) = cx.update(|cx| {
                app.projects.update(cx, |state, cx| {
                    let id = state
                        .open_worktree_without_selecting(repository, created, cx)
                        .map_err(|_| {
                            Status::unknown("Worktree exists but its catalog entry was not saved.")
                        })?;
                    Ok::<_, Status>((id, state.committed(cx)))
                })
            })?;
            catalog_done
                .await
                .map_err(|_| Status::unknown("Worktree catalog save was interrupted."))?
                .map_err(|_| {
                    Status::unknown("Worktree exists but its catalog entry could not be committed.")
                })?;
            let link_done = cx.update(|cx| {
                app.integrations.update(cx, |state, cx| {
                    let mut config = state.config.clone();
                    let links = config.links.entry(path).or_default();
                    if let Some(old) = links.iter_mut().find(|old| old.same_task(&task)) {
                        *old = task;
                    } else {
                        links.push(task);
                    }
                    let (tx, rx) = oneshot::channel();
                    state
                        .save_config_committed(config, "Task link", None, None, Some(tx), cx)
                        .map_err(|_| {
                            Status::unknown("Worktree exists; task link could not be started.")
                        })?;
                    Ok::<_, Status>(rx)
                })
            })?;
            link_done
                .await
                .map_err(|_| Status::unknown("Worktree exists; task link outcome is uncertain."))?
                .map_err(|_| {
                    Status::unknown("Worktree exists; task link could not be committed.")
                })?;
            serde_json::to_string(&id)
                .map_err(|_| Status::unknown("Created workspace identity could not be returned."))
        }
        .await;
        let _ = reply.send(result);
    })
    .detach();
}
