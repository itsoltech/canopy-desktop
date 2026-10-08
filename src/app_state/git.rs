use super::AppState;
use canopy_desktop::{
    git::{
        CreateWorktree, GitClient, RemovalAction, RemovalApproval, RemoveWorktree, RepositoryInfo,
        WorktreeAnalysis, WorktreeRemovalOutcome,
    },
    state::{projects::canonical_directory, workspace::PaneId},
};
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemovalConfirmation {
    Processes,
    Changes(usize),
    Branch(canopy_desktop::git::BranchAnalysis),
}
pub struct GitState {
    pub entries: HashMap<PathBuf, Result<Option<Arc<RepositoryInfo>>, String>>,
    pub error: Option<String>,
    pub watch_error: Option<String>,
    pub busy: bool,
    pub created_path: Option<PathBuf>,
    pub removal_confirmation: Option<RemovalConfirmation>,
    pub worktree_analysis: Option<WorktreeAnalysis>,
    pub removal_result: Option<canopy_desktop::git::RemovalResult>,
    client: Option<GitClient>,
    paths: HashSet<PathBuf>,
    observer: Option<Subscription>,
    events: Option<Task<()>>,
    task: Option<Task<()>>,
    quitting: bool,
}

fn pause_cleanup_handles(app: &AppState, paths: &[PathBuf], panes: &[PaneId], cx: &mut App) {
    app.files
        .update(cx, |files, cx| files.pause_for_cleanup(paths, cx));
    app.editors
        .update(cx, |editors, cx| editors.pause_for_cleanup(panes, cx));
}

fn resume_cleanup_handles(
    app: &AppState,
    paths: &[PathBuf],
    panes: &[PaneId],
    removed: &HashSet<PathBuf>,
    cx: &mut App,
) {
    app.files.update(cx, |files, cx| {
        files.resume_after_cleanup(paths, removed, cx)
    });
    app.editors
        .update(cx, |editors, cx| editors.resume_after_cleanup(panes, cx));
}

impl GitState {
    pub fn client(&self) -> Option<GitClient> {
        self.client.clone()
    }
    pub fn set_operation_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        cx.notify();
    }

    pub fn new() -> Self {
        match GitClient::start() {
            Ok(client) => Self {
                client: Some(client),
                entries: Default::default(),
                error: None,
                watch_error: None,
                busy: false,
                created_path: None,
                removal_confirmation: None,
                worktree_analysis: None,
                removal_result: None,
                paths: Default::default(),
                observer: None,
                events: None,
                task: None,
                quitting: false,
            },
            Err(error) => Self {
                client: None,
                entries: Default::default(),
                error: Some(error),
                watch_error: None,
                busy: false,
                created_path: None,
                removal_confirmation: None,
                worktree_analysis: None,
                removal_result: None,
                paths: Default::default(),
                observer: None,
                events: None,
                task: None,
                quitting: false,
            },
        }
    }
    pub fn bind(&mut self, app: &AppState, cx: &mut Context<Self>) {
        self.observer = Some(cx.observe(&app.projects, |this, projects, cx| {
            if this.quitting || !projects.read(cx).ready {
                return;
            }
            let paths: HashSet<_> = projects
                .read(cx)
                .catalog
                .items
                .iter()
                .map(|p| p.path.clone())
                .collect();
            if paths != this.paths {
                if let Some(client) = &this.client {
                    if let Err(error) = client.watch(paths.iter().cloned().collect()) {
                        this.error = Some(error);
                    } else {
                        this.paths = paths;
                    }
                }
                cx.notify();
            }
        }));
        if let Some(client) = self.client.clone() {
            self.events = Some(cx.spawn(async move |this, cx| {
                while client.updates.recv().await.is_ok() {
                    let snapshots = client.snapshots();
                    let watch_failed=client.stats.active_watch_failures.load(std::sync::atomic::Ordering::Relaxed)>0;
                    let _ = this.update(cx, |this, cx| {
                        this.entries = snapshots;
                        this.watch_error = watch_failed.then(|| "Some Git metadata watches are unavailable. Use Refresh after external Git changes.".into());
                        let roots = this
                            .entries
                            .iter()
                            .filter_map(|(path, e)| {
                                e.as_ref()
                                    .ok()
                                    .and_then(|i| i.as_ref())
                                    .map(|i| (path.clone(), i.root.clone()))
                            })
                            .collect();
                        let projects = cx.global::<AppState>().projects.clone();
                        projects.update(cx, |p, cx| p.set_repository_roots(roots, cx));
                        cx.notify();
                    });
                }
            }));
        }
    }
    pub fn repository(&self, path: &Path) -> Option<&Arc<RepositoryInfo>> {
        self.entries
            .get(path)
            .and_then(|r| r.as_ref().ok())
            .and_then(|r| r.as_ref())
            .or_else(|| {
                self.entries
                    .values()
                    .filter_map(|r| r.as_ref().ok().and_then(|r| r.as_ref()))
                    .find(|r| r.root == path)
            })
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.client
            && let Err(error) = client.refresh()
        {
            self.error = Some(error);
        }
        cx.notify();
    }
    pub fn create(
        &mut self,
        request: CreateWorktree,
        task: Option<canopy_desktop::integrations::TaskRef>,
        agent: Option<super::tools::WorktreeAgent>,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if self.busy || self.quitting {
            return Err("Git is busy.".into());
        }
        if task.is_some() && cx.global::<AppState>().integrations.read(cx).busy {
            return Err(
                "Wait for integration configuration to finish before creating this worktree."
                    .into(),
            );
        }
        let client = self.client.clone().ok_or("Git is unavailable.")?;
        let app = cx.global::<AppState>().clone();
        if !app.projects.read(cx).ready || app.projects.read(cx).busy {
            return Err("Projects are busy.".into());
        }
        if let Some(agent) = &agent {
            app.tools.read(cx).validate_worktree_agent(agent)?;
        }
        let account = if agent.is_some() {
            task.as_ref()
                .map(|task| {
                    app.integrations
                        .read(cx)
                        .config
                        .account_for(&task.project)
                        .cloned()
                        .ok_or("Connect this task provider first.")
                })
                .transpose()?
        } else {
            None
        };
        let http = cx.http_client();
        let expected_account = account.clone();
        let context_task = task.clone();
        let context_agent = agent.as_ref().map(|agent| agent.tool.clone());
        self.busy = true;
        self.created_path = None;
        self.error = None;
        let root = request.repository.clone();
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let context = match (account, context_task, context_agent) {
                    (Some(account), Some(task), Some(agent)) => Some(
                        cx.background_executor()
                            .spawn(async move {
                                canopy_desktop::integrations::task_context::prepare(
                                    http, account, task, agent,
                                )
                                .await
                            })
                            .await?,
                    ),
                    _ => None,
                };
                this.update(cx, |this, cx| {
                    let app = cx.global::<AppState>();
                    if this.quitting || app.settings.read(cx).quitting {
                        return Err("Worktree creation cancelled while quitting.".to_owned());
                    }
                    if app.projects.read(cx).busy {
                        return Err("Projects changed while preparing the task. Retry.".into());
                    }
                    if let Some(agent) = &agent {
                        app.tools.read(cx).validate_worktree_agent(agent)?;
                    }
                    if let (Some(expected), Some(task)) = (&expected_account, &task)
                        && app.integrations.read(cx).config.account_for(&task.project)
                            != Some(expected)
                    {
                        return Err(
                            "The task connection changed. Retry with the current connection."
                                .into(),
                        );
                    }
                    Ok(())
                })
                .map_err(|_| "Worktree creation was cancelled.".to_owned())??;
                client
                    .create(request)
                    .await
                    .map(|created| (created, context))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((created, context)) => {
                        let path = created.path;
                        this.created_path = Some(path.clone());
                        let projects = cx.global::<AppState>().projects.clone();
                        projects.update(cx, |p, cx| {
                            p.open_worktree_with_base(root, path.clone(), created.base, cx)
                        });
                        if let Some(task) = task {
                            let integrations = cx.global::<AppState>().integrations.clone();
                            if let Err(error) = integrations
                                .update(cx, |state, cx| state.link(path.clone(), task, cx))
                            {
                                this.error = Some(format!(
                                    "Worktree created, but task link could not be saved: {error}"
                                ));
                            }
                        }
                        if let Some(agent) = agent {
                            let tools = cx.global::<AppState>().tools.clone();
                            let prompt = context.as_ref().map(|v| v.prompt.clone());
                            let result = tools.update(cx, |tools, cx| {
                                tools.launch_worktree_agent(&agent, &path, prompt, cx)
                            });
                            match result {
                                Ok(()) => {
                                    if let Some(context) = context {
                                        context.retain();
                                    }
                                }
                                Err(error) => {
                                    this.error = Some(format!(
                                        "Worktree created, but agent could not start: {error}"
                                    ));
                                }
                            }
                        }
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn open(&mut self, root: PathBuf, path: PathBuf, cx: &mut Context<Self>) {
        if self.busy || self.quitting {
            return;
        }
        self.busy = true;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { canonical_directory(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(path) => {
                        let projects = cx.global::<AppState>().projects.clone();
                        projects.update(cx, |p, cx| p.open_worktree(root, path, cx));
                    }
                    Err(_) => this.error = Some("Worktree directory is unavailable.".into()),
                }
                cx.notify();
            });
        }));
    }
    pub fn analyze_removal(
        &mut self,
        request: RemoveWorktree,
        target: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if self.busy || self.quitting {
            return Err("Git is busy.".into());
        }
        let client = self.client.clone().ok_or("Git is unavailable.")?;
        self.busy = true;
        self.error = None;
        self.worktree_analysis = None;
        self.removal_result = None;
        self.removal_confirmation = None;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = client.analyze_worktree(request, target).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(analysis) => this.worktree_analysis = Some(analysis),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        Ok(())
    }

    pub fn execute_removal(
        &mut self,
        request: RemoveWorktree,
        action: RemovalAction,
        stop_processes: bool,
        approval: RemovalApproval,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if self.busy || self.quitting {
            return Err("Git is busy.".into());
        }
        let analysis = self
            .worktree_analysis
            .clone()
            .ok_or("Wait for worktree analysis.")?;
        let app = cx.global::<AppState>().clone();
        let workspace = app
            .projects
            .read(cx)
            .catalog
            .items
            .iter()
            .find(|project| project.path == request.path)
            .map(|project| project.workspace);
        if let Some(error) =
            workspace.and_then(|id| app.terminals.read(cx).workspace_cleanup_error(id, cx))
        {
            return Err(format!(
                "Resolve the previous terminal cleanup failure before removing a worktree: {error}"
            )
            .into());
        }
        let panes: Vec<_> = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .filter(|model| Some(model.id) == workspace)
            .flat_map(|model| model.all_panes())
            .map(|pane| pane.id)
            .collect();
        if app.editors.read(cx).busy(&panes, cx) {
            return Err("Save or close unsaved editor files before removing this worktree.".into());
        }
        let mut cleanup_paths = vec![request.path.clone()];
        let mut cleanup_panes = panes.clone();
        if workspace.is_some_and(|id| app.terminals.read(cx).workspace_running(id, cx))
            && !stop_processes
        {
            self.removal_confirmation = Some(RemovalConfirmation::Processes);
            self.error = None;
            cx.notify();
            return Ok(());
        }
        if let RemovalAction::Merge { target, .. } = &action
            && let Some(info) = self.repository(&request.repository)
            && let Some(target_path) = info
                .worktrees
                .iter()
                .find(|item| item.branch_name() == Some(target.as_str()) && item.available)
                .map(|item| item.path.clone())
        {
            cleanup_paths.push(target_path.clone());
            if let Some(target_workspace) = app
                .projects
                .read(cx)
                .catalog
                .items
                .iter()
                .find(|project| project.path == target_path)
                .map(|project| project.workspace)
            {
                let target_panes: Vec<_> = app
                    .projects
                    .read(cx)
                    .runtime_workspaces(cx)
                    .iter()
                    .filter(|model| model.id == target_workspace)
                    .flat_map(|model| model.all_panes())
                    .map(|pane| pane.id)
                    .collect();
                if app.editors.read(cx).busy(&target_panes, cx) {
                    return Err(format!(
                        "Save or close unsaved editor files in target branch '{target}' before merging."
                    )
                    .into());
                }
                if app
                    .terminals
                    .read(cx)
                    .workspace_running(target_workspace, cx)
                {
                    return Err(format!(
                        "Stop processes in target branch '{target}' before merging."
                    )
                    .into());
                }
                if let Some(error) = app
                    .terminals
                    .read(cx)
                    .workspace_cleanup_error(target_workspace, cx)
                {
                    return Err(format!(
                        "Resolve the previous terminal cleanup failure in target branch '{target}' before merging: {error}"
                    )
                    .into());
                }
                cleanup_panes.extend(target_panes);
            }
        }
        let env = app
            .tools
            .read(cx)
            .environment
            .clone()
            .and_then(Result::ok)
            .map(|env| (*env).clone());
        let client = self.client.clone().ok_or("Git is unavailable.")?;
        self.busy = true;
        self.error = None;
        self.removal_result = None;
        self.removal_confirmation = None;
        cx.notify();
        let stop = if stop_processes
            && workspace.is_some_and(|id| app.terminals.read(cx).workspace_running(id, cx))
        {
            workspace.map(|id| {
                app.terminals
                    .update(cx, |terminals, cx| terminals.stop_workspace(id, cx))
            })
        } else {
            None
        };
        self.task = Some(cx.spawn(async move |this, cx| {
            if let Some(stop) = stop
                && let Err(error) = stop.await
            {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    this.error = Some(format!(
                        "Could not confirm terminal process cleanup: {error}"
                    ));
                    cx.notify();
                });
                return;
            }
            if this
                .update(cx, |_, cx| {
                    pause_cleanup_handles(&app, &cleanup_paths, &cleanup_panes, cx)
                })
                .is_err()
            {
                return;
            }
            let removal_path = request.path.clone();
            let result = client.execute_worktree_removal(request, action, analysis, approval, env).await;
            let removed = matches!(
                &result,
                Ok(WorktreeRemovalOutcome::Finished(result)) if result.worktree_removed
            );
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(WorktreeRemovalOutcome::AnalysisChanged(analysis)) => {
                        this.worktree_analysis = Some(analysis);
                        this.error = Some("The worktree or branch state changed. Review the updated analysis before continuing.".into());
                    }
                    Ok(WorktreeRemovalOutcome::NeedsDiscardConfirmation(count)) => {
                        this.removal_confirmation = Some(RemovalConfirmation::Changes(count));
                    }
                    Ok(WorktreeRemovalOutcome::NeedsBranchConfirmation(analysis)) => {
                        this.removal_confirmation = Some(RemovalConfirmation::Branch(analysis));
                    }
                    Ok(WorktreeRemovalOutcome::Finished(result)) => {
                        if let Some(analysis) = result.retry_analysis.clone() {
                            this.worktree_analysis = Some(analysis);
                        }
                        if result.worktree_removed
                            && let Some(id) = workspace
                        {
                            cx.global::<AppState>().projects.clone().update(cx, |projects, cx| projects.close(id, cx));
                        }
                        if result.error.is_none() {
                            let message = if result.branch_deleted {
                                "Worktree and local branch removed"
                            } else if result.merge_performed {
                                "Branch merged and worktree removed"
                            } else {
                                "Worktree removed"
                            };
                            let message = result.warning.as_ref().map(|warning| format!("{message} · {warning}")).unwrap_or_else(|| message.into());
                            cx.global::<AppState>().toasts.clone().update(cx, |toasts, cx| toasts.show(message, cx));
                        }
                        this.error = result.error.clone();
                        this.removal_result = Some(result);
                    }
                    Err(error) => this.error = Some(error),
                }
                let removed_paths = if removed {
                    HashSet::from([removal_path])
                } else {
                    HashSet::new()
                };
                resume_cleanup_handles(
                    &app,
                    &cleanup_paths,
                    &cleanup_panes,
                    &removed_paths,
                    cx,
                );
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn retry_branch_deletion(
        &mut self,
        repository: PathBuf,
        analysis: canopy_desktop::git::BranchAnalysis,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if self.busy || self.quitting {
            return Err("Git is busy.".into());
        }
        let client = self.client.clone().ok_or("Git is unavailable.")?;
        self.busy = true;
        self.error = None;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = client
                .delete_branch_after_removal(repository, analysis)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        if let Some(result) = &mut this.removal_result {
                            result.branch_deleted = true;
                            result.error = None;
                            result.retry_analysis = None;
                        }
                        cx.global::<AppState>()
                            .toasts
                            .clone()
                            .update(cx, |toasts, cx| toasts.show("Local branch removed", cx));
                    }
                    Err(error) => {
                        if let Some(result) = &mut this.removal_result {
                            result.error = Some(error.clone());
                        }
                        this.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn remove(
        &mut self,
        request: RemoveWorktree,
        stop_processes: bool,
        discard_changes: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if self.busy || self.quitting {
            return Err("Git is busy.".into());
        }
        let app = cx.global::<AppState>().clone();
        let workspace = app
            .projects
            .read(cx)
            .catalog
            .items
            .iter()
            .find(|p| p.path == request.path)
            .map(|p| p.workspace);
        if let Some(error) =
            workspace.and_then(|id| app.terminals.read(cx).workspace_cleanup_error(id, cx))
        {
            return Err(format!(
                "Resolve the previous terminal cleanup failure before removing a worktree: {error}"
            )
            .into());
        }
        let panes: Vec<_> = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .filter(|w| Some(w.id) == workspace)
            .flat_map(|w| w.all_panes())
            .map(|p| p.id)
            .collect();
        if app.editors.read(cx).busy(&panes, cx) {
            return Err("Save or close unsaved editor files before removing this worktree.".into());
        }
        let cleanup_paths = vec![request.path.clone()];
        let cleanup_panes = panes.clone();
        self.removal_confirmation = None;
        self.removal_result = None;
        self.worktree_analysis = None;
        if workspace.is_some_and(|id| app.terminals.read(cx).workspace_running(id, cx))
            && !stop_processes
        {
            self.error = None;
            self.removal_confirmation = Some(RemovalConfirmation::Processes);
            cx.notify();
            return Ok(());
        }
        let client = self.client.clone().ok_or("Git is unavailable.")?;
        self.busy = true;
        self.error = None;
        cx.notify();
        let stop = if stop_processes {
            workspace.map(|id| app.terminals.update(cx, |t, cx| t.stop_workspace(id, cx)))
        } else {
            None
        };
        self.task = Some(cx.spawn(async move |this, cx| {
            if let Some(stop) = stop
                && let Err(error) = stop.await
            {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    this.error = Some(format!(
                        "Could not confirm terminal process cleanup: {error}"
                    ));
                    cx.notify();
                });
                return;
            }
            if this
                .update(cx, |_, cx| {
                    pause_cleanup_handles(&app, &cleanup_paths, &cleanup_panes, cx)
                })
                .is_err()
            {
                return;
            }
            let removal_path = request.path.clone();
            let result = client.remove(request, discard_changes).await;
            let removed = matches!(&result, Ok(canopy_desktop::git::RemovalOutcome::Removed));
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(canopy_desktop::git::RemovalOutcome::Removed) => {
                        if let Some(id) = workspace {
                            let projects = cx.global::<AppState>().projects.clone();
                            projects.update(cx, |p, cx| p.close(id, cx));
                        }
                    }
                    Ok(canopy_desktop::git::RemovalOutcome::NeedsDiscardConfirmation(count)) => {
                        this.removal_confirmation = Some(RemovalConfirmation::Changes(count))
                    }
                    Err(error) => this.error = Some(error),
                }
                let removed_paths = if removed {
                    HashSet::from([removal_path])
                } else {
                    HashSet::new()
                };
                resume_cleanup_handles(&app, &cleanup_paths, &cleanup_panes, &removed_paths, cx);
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn begin_quit(&mut self) -> Option<Task<()>> {
        self.quitting = true;
        self.task.take()
    }
    pub fn cancel_quit(&mut self, cx: &mut Context<Self>) {
        self.quitting = false;
        cx.notify();
    }
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.quitting = true;
        self.observer = None;
        self.events = None;
        let task = self.task.take();
        let client = self.client.take();
        cx.spawn(async move |_, cx| {
            if let Some(task) = task {
                task.await;
            }
            if let Some(client) = client {
                cx.background_executor()
                    .spawn(async move {
                        futures_lite::future::block_on(client.shutdown());
                    })
                    .await;
            }
        })
    }
}
