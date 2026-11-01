//! Runtime-only registry. Pane IDs survive drag/drop; processes never enter SQLite.
use super::AppState;
use crate::ui::terminal::TerminalView;
use canopy_desktop::{
    state::workspace::{PaneId, WorkspaceId},
    terminal::lifecycle,
};
use gpui_kit::*;
use std::{collections::HashMap, sync::Arc};
struct WorkspaceStop {
    task: Task<()>,
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
}
/// A terminal a remote device opened. It has no pane and changes no host layout;
/// it belongs to its workspace and ends with it, on Close or on Quit.
struct DetachedTerminal {
    workspace: WorkspaceId,
    view: Entity<TerminalView>,
    _events: Subscription,
}
struct RetiringTerminal {
    workspace: WorkspaceId,
    pane: PaneId,
    view: Entity<TerminalView>,
    _task: Task<()>,
    done: Arc<std::sync::atomic::AtomicBool>,
}
pub struct Terminals {
    pub views: HashMap<PaneId, Entity<TerminalView>>,
    pub error: Option<String>,
    cleanup_errors: HashMap<PaneId, String>,
    workspaces: HashMap<PaneId, WorkspaceId>,
    events: HashMap<PaneId, Subscription>,
    observers: Vec<Subscription>,
    stopping: bool,
    agents_ready: bool,
    stop_jobs: HashMap<WorkspaceId, WorkspaceStop>,
    retiring: Vec<RetiringTerminal>,
    detached: HashMap<String, DetachedTerminal>,
    close_jobs: HashMap<PaneId, (Vec<PaneId>, Task<()>)>,
}
impl Terminals {
    /// Admit explicit local terminal closes before changing layout or stopping
    /// processes. Quit and cleanup of an already admitted operation stay available
    /// even at the inbox limit.
    pub(crate) fn close_local_panes(
        &mut self,
        panes: Vec<PaneId>,
        close: impl FnOnce(&mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        let views: Vec<_> = panes
            .iter()
            .filter_map(|p| self.views.get(p).cloned())
            .collect();
        let Some(key) = panes.first().copied() else {
            close(cx);
            return;
        };
        if views.is_empty() {
            close(cx);
            return;
        }
        if self.stopping
            || self
                .close_jobs
                .values()
                .any(|(closing, _)| panes.iter().any(|pane| closing.contains(pane)))
        {
            return;
        }
        let commands = views
            .iter()
            .map(|view| {
                let view = view.read(cx);
                Ok((
                    view.local_resource(cx)?,
                    canopy_desktop::command_bus::Command::Catalog(
                        canopy_desktop::command_bus::CatalogCommand::CloseTerminal {
                            terminal: view.terminal_id().into(),
                            plan: uuid::Uuid::new_v4().to_string(),
                        },
                    ),
                ))
            })
            .collect::<Result<Vec<_>, String>>();
        let commands = match commands {
            Ok(c) => c,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.close_jobs.insert(
            key,
            (
                panes,
                cx.spawn(async move |this, cx| {
                    let result = async {
                        let inputs = super::file_ops::LocalResource::begin_set(commands).await?;
                        let tasks = this
                            .update(cx, |_, cx| {
                                close(cx);
                                views
                                    .iter()
                                    .map(|view| view.update(cx, |view, cx| view.shutdown(cx)))
                                    .collect::<Vec<_>>()
                            })
                            .map_err(|_| "Terminal owner stopped.".to_owned());
                        let outcome = match tasks {
                            Err(error) => Err(error),
                            Ok(tasks) => {
                                let mut result = Ok(());
                                for task in tasks {
                                    if let Err(error) = task.await {
                                        result = Err(error);
                                    }
                                }
                                result
                            }
                        };
                        super::file_ops::LocalResource::finish_set(inputs, outcome).await
                    }
                    .await;
                    let _ = this.update(cx, |this, cx| {
                        this.close_jobs.remove(&key);
                        if let Err(error) = result {
                            this.error = Some(error);
                        }
                        cx.notify();
                    });
                }),
            ),
        );
    }
    /// Every terminal view with its workspace: pane terminals and device-opened ones.
    pub fn all(&self) -> Vec<(WorkspaceId, Entity<TerminalView>)> {
        self.views
            .iter()
            .filter_map(|(pane, view)| Some((*self.workspaces.get(pane)?, view.clone())))
            .chain(
                self.detached
                    .values()
                    .map(|terminal| (terminal.workspace, terminal.view.clone())),
            )
            .collect()
    }
    /// The view with remote identity `id`.
    pub fn find(&self, id: &str, cx: &App) -> Option<Entity<TerminalView>> {
        self.detached
            .get(id)
            .map(|terminal| terminal.view.clone())
            .or_else(|| {
                self.views
                    .values()
                    .find(|view| view.read(cx).terminal_id() == id)
                    .cloned()
            })
    }
    pub fn is_detached(&self, id: &str) -> bool {
        self.detached.contains_key(id)
    }
    fn live_views(&self) -> impl Iterator<Item = &Entity<TerminalView>> {
        self.views
            .values()
            .chain(self.detached.values().map(|terminal| &terminal.view))
            .chain(self.retiring.iter().map(|r| &r.view))
    }
    /// Starts a terminal for a device in `cwd` of `workspace`; the same `id` again
    /// returns the existing one. The process obeys the same limits as pane terminals.
    #[expect(
        clippy::too_many_arguments,
        reason = "Detached terminal creation needs its identity, workspace, launch selection, input holder, and GPUI owner context."
    )]
    pub fn open_detached(
        &mut self,
        id: String,
        workspace: WorkspaceId,
        cwd: std::path::PathBuf,
        tool: String,
        profile: Option<String>,
        holder: canopy_desktop::terminal::control::Holder,
        cx: &mut Context<Self>,
    ) -> Result<Entity<TerminalView>, String> {
        if let Some(terminal) = self.detached.get(&id) {
            return Ok(terminal.view.clone());
        }
        let app = cx.global::<AppState>().clone();
        if self.stopping || app.settings.read(cx).quitting {
            return Err("The host is closing terminals.".into());
        }
        if app.git.read(cx).busy {
            return Err("A Git operation is running on the host. Try again shortly.".into());
        }
        if self.workspace_stopping(workspace) {
            return Err("Terminals of this workspace are stopping.".into());
        }
        if self.views.len() + self.detached.len() >= 64 {
            return Err("The limit of 64 started terminals has been reached.".into());
        }
        let env = app
            .tools
            .read(cx)
            .environment
            .clone()
            .ok_or("The shell environment is still loading. Try again shortly.")??;
        let pane = canopy_desktop::state::workspace::Pane {
            id: PaneId::new(),
            tool,
            metadata: canopy_desktop::state::workspace::PaneMetadata {
                cwd: Some(cwd),
                profile_id: profile,
                ..Default::default()
            },
        };
        let pane_id = pane.id;
        let view = cx.new(|cx| TerminalView::new(pane, Ok(env), cx).for_device(id.clone(), holder));
        let events = cx.subscribe(
            &view,
            move |this, view, _: &crate::ui::terminal::TerminalStateChanged, cx| {
                if let Some(result) = view.read(cx).cleanup_result() {
                    this.record_cleanup_result(pane_id, result);
                }
                cx.notify();
            },
        );
        // Not painted: the device renders it, the host shows no pane for it.
        view.update(cx, |view, cx| {
            view.set_active(false, false, cx);
            view.start(cx);
        });
        self.detached.insert(
            id,
            DetachedTerminal {
                workspace,
                view: view.clone(),
                _events: events,
            },
        );
        cx.notify();
        Ok(view)
    }
    /// Stops a device-opened terminal (confirmed cleanup) and forgets it.
    pub fn close_detached(&mut self, id: &str, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        let Some(terminal) = self.detached.remove(id) else {
            return Task::ready(Err("The terminal is no longer available.".into()));
        };
        let workspace = terminal.workspace;
        self.retire(workspace, terminal.view, cx)
    }
    /// Moves a view to the retiring owners until its cleanup is confirmed.
    fn retire(
        &mut self,
        workspace: WorkspaceId,
        view: Entity<TerminalView>,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let cleanup = view.update(cx, |view, cx| view.shutdown(cx));
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed = done.clone();
        let pane = view.read(cx).pane_id();
        let (finished, result) = async_channel::bounded(1);
        let task = cx.spawn(async move |this, cx| {
            let outcome = cleanup.await;
            let _ = this.update(cx, |this, cx| {
                this.record_cleanup_result(pane, outcome.clone());
                cx.notify();
            });
            completed.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = finished.try_send(outcome);
        });
        self.retiring.push(RetiringTerminal {
            workspace,
            pane,
            view,
            _task: task,
            done,
        });
        cx.notify();
        cx.spawn(async move |_, _| {
            result
                .recv()
                .await
                .map_err(|_| "Terminal cleanup result was lost.".to_owned())?
        })
    }
    pub fn folder_references(&self, cx: &App) -> Vec<std::path::PathBuf> {
        self.live_views()
            .filter_map(|view| {
                let view = view.read(cx);
                (view.is_live() || view.cleanup_error().is_some())
                    .then(|| view.cwd().map(std::path::Path::to_path_buf))
                    .flatten()
            })
            .collect()
    }
    pub fn uses_directory(&self, dir: &std::path::Path, cx: &App) -> bool {
        self.live_views().any(|view| {
            let view = view.read(cx);
            (view.is_live() || view.cleanup_error().is_some())
                && view.cwd().is_some_and(|cwd| cwd.starts_with(dir))
        })
    }
    pub fn new() -> Self {
        Self {
            views: HashMap::new(),
            error: None,
            cleanup_errors: HashMap::new(),
            workspaces: HashMap::new(),
            events: HashMap::new(),
            observers: vec![],
            stopping: false,
            agents_ready: false,
            stop_jobs: HashMap::new(),
            retiring: vec![],
            detached: HashMap::new(),
            close_jobs: HashMap::new(),
        }
    }
    pub fn bind(&mut self, app: &AppState, cx: &mut Context<Self>) {
        self.observers = vec![
            cx.observe(&app.agents, |this, state, cx| {
                let ready = state.read(cx).ready;
                if ready != this.agents_ready {
                    this.agents_ready = ready;
                    this.sync(cx);
                }
            }),
            cx.observe(&app.git, |this, _, cx| this.sync(cx)),
            cx.observe(&app.tools, |this, _, cx| this.sync(cx)),
            cx.observe(&app.workspace, |this, _, cx| this.sync(cx)),
            cx.observe(&app.projects, |this, _, cx| this.sync(cx)),
        ];
    }
    fn record_cleanup_result(&mut self, pane: PaneId, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.cleanup_errors.remove(&pane);
                self.error = self.cleanup_errors.values().next().cloned();
            }
            Err(error) => {
                self.cleanup_errors.insert(pane, error.clone());
                self.error = Some(error);
            }
        }
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        if self.stopping {
            return;
        }
        self.retiring.retain(|terminal| {
            !terminal.done.load(std::sync::atomic::Ordering::SeqCst)
                || terminal.view.read(cx).cleanup_error().is_some()
        });
        let app = cx.global::<AppState>().clone();
        if app.settings.read(cx).quitting
            || !app.projects.read(cx).ready
            || !app.tools.read(cx).ready
        {
            return;
        }
        let all = app.projects.read(cx).runtime_workspaces(cx);
        let started = self.views.keys().copied().collect();
        // The global workspace is the active space: a project or "No project".
        let active = Some(app.workspace.read(cx).id);
        let plan = lifecycle::reconcile(&all, active, &started);
        for id in &plan.remove {
            self.events.remove(id);
            if let Some(view) = self.views.remove(id) {
                let workspace = self
                    .workspaces
                    .remove(id)
                    .expect("started terminal has a workspace owner");
                self.retire(workspace, view, cx).detach();
            }
        }
        // A closed project ends the terminals devices opened in it.
        let gone: Vec<_> = self
            .detached
            .iter()
            .filter(|(_, terminal)| !all.iter().any(|w| w.id == terminal.workspace))
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            self.close_detached(&id, cx).detach();
        }
        if !app.git.read(cx).busy
            && let Some(env) = app.tools.read(cx).environment.clone()
        {
            for pane in &plan.start {
                if matches!(pane.tool.as_str(), "claude" | "codex") && !app.agents.read(cx).ready {
                    continue;
                }
                let env = if self.views.len() < 64 {
                    env.clone()
                } else {
                    Err("The limit of 64 started terminal panes has been reached.".into())
                };
                let view = cx.new(|cx| TerminalView::new(pane.clone(), env.clone(), cx));
                let pane_id = pane.id;
                let event = cx.subscribe(
                    &view,
                    move |this, view, _: &crate::ui::terminal::TerminalStateChanged, cx| {
                        if let Some(result) = view.read(cx).cleanup_result() {
                            this.record_cleanup_result(pane_id, result);
                        }
                        cx.notify();
                    },
                );
                self.events.insert(pane.id, event);
                if env.is_ok() {
                    view.update(cx, |view, cx| view.start(cx));
                }
                let workspace = all
                    .iter()
                    .find(|workspace| workspace.all_panes().iter().any(|p| p.id == pane.id))
                    .map(|workspace| workspace.id)
                    .expect("planned terminal belongs to a workspace");
                self.workspaces.insert(pane.id, workspace);
                self.views.insert(pane.id, view);
            }
        }
        for (id, view) in &self.views {
            let visible = plan.visible.contains(id);
            view.update(cx, |view, cx| {
                view.set_active(visible, visible && plan.focused == Some(*id), cx)
            });
        }
        cx.notify();
    }
    pub fn running_workspaces(&self, cx: &App) -> std::collections::HashSet<WorkspaceId> {
        let mut running: std::collections::HashSet<_> = cx
            .global::<AppState>()
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .into_iter()
            .filter(|w| {
                w.all_panes()
                    .iter()
                    .any(|p| self.views.get(&p.id).is_some_and(|v| v.read(cx).is_live()))
            })
            .map(|w| w.id)
            .collect();
        running.extend(
            self.detached
                .values()
                .filter(|terminal| terminal.view.read(cx).is_live())
                .map(|terminal| terminal.workspace),
        );
        running.extend(
            self.retiring
                .iter()
                .filter(|terminal| {
                    !terminal.done.load(std::sync::atomic::Ordering::SeqCst)
                        || terminal.view.read(cx).cleanup_error().is_some()
                })
                .map(|terminal| terminal.workspace),
        );
        running
    }
    pub fn workspace_stopping(&self, id: WorkspaceId) -> bool {
        self.stop_jobs
            .get(&id)
            .is_some_and(|job| !job.completion.is_closed())
            || self.retiring.iter().any(|terminal| {
                terminal.workspace == id && !terminal.done.load(std::sync::atomic::Ordering::SeqCst)
            })
    }
    pub fn workspace_cleanup_error(&self, id: WorkspaceId, cx: &App) -> Option<String> {
        self.retiring
            .iter()
            .filter(|terminal| terminal.workspace == id)
            .find_map(|terminal| {
                self.cleanup_errors
                    .get(&terminal.pane)
                    .cloned()
                    .or_else(|| terminal.view.read(cx).cleanup_error())
            })
            .or_else(|| {
                cx.global::<AppState>()
                    .projects
                    .read(cx)
                    .runtime_workspaces(cx)
                    .into_iter()
                    .find(|workspace| workspace.id == id)
                    .and_then(|workspace| {
                        self.detached
                            .values()
                            .filter(|terminal| terminal.workspace == workspace.id)
                            .find_map(|terminal| terminal.view.read(cx).cleanup_error())
                            .or_else(|| {
                                workspace.all_panes().iter().find_map(|pane| {
                                    self.cleanup_errors.get(&pane.id).cloned().or_else(|| {
                                        self.views
                                            .get(&pane.id)
                                            .and_then(|view| view.read(cx).cleanup_error())
                                    })
                                })
                            })
                    })
            })
    }
    pub fn request_stop_workspace(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        if self.stopping || self.workspace_stopping(id) {
            return;
        }
        self.stop_jobs.retain(|_, job| !job.completion.is_closed());
        let stop = self.stop_workspace(id, cx);
        let (finished, completion) = async_channel::bounded::<()>(1);
        let stop_result = Arc::new(std::sync::Mutex::new(None));
        let shared_result = stop_result.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = stop.await;
            *shared_result.lock().unwrap() = Some(result.clone());
            drop(finished);
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = result {
                    this.error = Some(error);
                }
                cx.notify();
            });
        });
        self.stop_jobs.insert(
            id,
            WorkspaceStop {
                task,
                completion,
                result: stop_result,
            },
        );
        cx.notify();
    }
    pub fn stop_workspace(
        &mut self,
        id: canopy_desktop::state::workspace::WorkspaceId,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        if let Some(job) = self.stop_jobs.get(&id)
            && !job.completion.is_closed()
        {
            let completion = job.completion.clone();
            let result = job.result.clone();
            return cx.spawn(async move |_, _| {
                let _ = completion.recv().await;
                result
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or("Terminal cleanup result was lost.".to_owned())?
            });
        }
        let app = cx.global::<AppState>();
        let panes = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .into_iter()
            .find(|w| w.id == id)
            .map(|w| w.all_panes())
            .unwrap_or_default();
        let mut views: Vec<_> = panes
            .iter()
            .filter_map(|p| {
                self.views
                    .get(&p.id)
                    .filter(|view| {
                        view.read(cx).is_live()
                            || view.read(cx).cleanup_error().is_some()
                            || self.cleanup_errors.contains_key(&p.id)
                    })
                    .cloned()
            })
            .collect();
        views.extend(
            self.detached
                .values()
                .filter(|terminal| {
                    terminal.workspace == id
                        && (terminal.view.read(cx).is_live()
                            || terminal.view.read(cx).cleanup_error().is_some())
                })
                .map(|terminal| terminal.view.clone()),
        );
        for terminal in self
            .retiring
            .iter()
            .filter(|terminal| terminal.workspace == id)
        {
            if !views
                .iter()
                .any(|view| view.read(cx).pane_id() == terminal.pane)
            {
                views.push(terminal.view.clone());
            }
        }
        let pane_ids: Vec<_> = views.iter().map(|view| view.read(cx).pane_id()).collect();
        let tasks: Vec<_> = views
            .iter()
            .map(|view| view.update(cx, |view, cx| view.shutdown(cx)))
            .collect();
        cx.spawn(async move |this, cx| {
            let mut result = Ok(());
            for (view, task) in views.iter().zip(tasks) {
                let cleanup = task.await;
                match &cleanup {
                    Ok(()) => view.update(cx, |view, cx| view.mark_stopped(cx)),
                    Err(error) => {
                        view.update(cx, |view, cx| view.mark_stop_failed(error.clone(), cx))
                    }
                }
                if result.is_ok() {
                    result = cleanup;
                }
            }
            if let Err(error) = &result {
                let error = error.clone();
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error);
                    cx.notify();
                });
            } else {
                let _ = this.update(cx, |this, cx| {
                    for pane in &pane_ids {
                        this.cleanup_errors.remove(pane);
                    }
                    this.retiring
                        .retain(|terminal| !pane_ids.contains(&terminal.pane));
                    this.error = this.cleanup_errors.values().next().cloned();
                    cx.notify();
                });
            }
            result
        })
    }
    pub fn workspace_running(
        &self,
        id: canopy_desktop::state::workspace::WorkspaceId,
        cx: &App,
    ) -> bool {
        if self.retiring.iter().any(|terminal| {
            terminal.workspace == id
                && (!terminal.done.load(std::sync::atomic::Ordering::SeqCst)
                    || terminal.view.read(cx).cleanup_error().is_some())
        }) || self
            .detached
            .values()
            .any(|terminal| terminal.workspace == id && terminal.view.read(cx).is_live())
        {
            return true;
        }
        let app = cx.global::<AppState>();
        app.projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .find(|w| w.id == id)
            .is_some_and(|w| {
                w.all_panes()
                    .iter()
                    .any(|p| self.views.get(&p.id).is_some_and(|v| v.read(cx).is_live()))
            })
    }
    pub fn running_count(&self, tool: &str, profile: Option<&str>, cx: &App) -> usize {
        self.views
            .values()
            .chain(self.detached.values().map(|terminal| &terminal.view))
            .filter(|view| {
                let view = view.read(cx);
                view.tool_id() == tool
                    && profile.is_none_or(|id| view.profile_id() == Some(id))
                    && view.is_running()
            })
            .count()
    }
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.stopping = true;
        let mut views: Vec<_> = self
            .views
            .values()
            .chain(self.detached.values().map(|terminal| &terminal.view))
            .cloned()
            .collect();
        for terminal in &self.retiring {
            if !views
                .iter()
                .any(|view| view.read(cx).pane_id() == terminal.pane)
            {
                views.push(terminal.view.clone());
            }
        }
        let tasks: Vec<_> = views
            .iter()
            .map(|view| view.update(cx, |view, cx| view.shutdown(cx)))
            .collect();
        let stop_jobs: Vec<_> = self.stop_jobs.drain().map(|(_, job)| job).collect();
        let close_jobs: Vec<_> = self.close_jobs.drain().map(|(_, (_, job))| job).collect();
        cx.spawn(async move |this, cx| {
            let mut result = Ok(());
            for job in close_jobs {
                job.await;
            }
            for (view, task) in views.iter().zip(tasks) {
                let cleanup = task.await;
                match &cleanup {
                    Ok(()) => view.update(cx, |view, cx| view.mark_stopped(cx)),
                    Err(error) => {
                        view.update(cx, |view, cx| view.mark_stop_failed(error.clone(), cx))
                    }
                }
                if result.is_ok() {
                    result = cleanup;
                }
            }
            for job in stop_jobs {
                job.task.await;
                let _ = job.completion.recv().await;
                let cleanup = job
                    .result
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| Err("Terminal cleanup result was lost.".to_owned()));
                if result.is_ok() {
                    result = cleanup;
                }
            }
            let state = result.clone();
            let _ = this.update(cx, |this, cx| {
                this.stopping = false;
                match state {
                    Ok(()) => {
                        this.views.clear();
                        this.detached.clear();
                        this.events.clear();
                        this.cleanup_errors.clear();
                        this.workspaces.clear();
                        this.retiring.clear();
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Terminals;
    use canopy_desktop::state::workspace::PaneId;

    #[test]
    fn cleanup_failures_are_scoped_and_cleared_only_by_confirmed_success() {
        let mut terminals = Terminals::new();
        let failed = PaneId::new();
        let unrelated = PaneId::new();
        terminals.record_cleanup_result(failed, Err("cleanup failed".into()));
        assert_eq!(
            terminals.cleanup_errors.get(&failed).map(String::as_str),
            Some("cleanup failed")
        );
        assert!(!terminals.cleanup_errors.contains_key(&unrelated));
        terminals.record_cleanup_result(unrelated, Ok(()));
        assert!(terminals.cleanup_errors.contains_key(&failed));
        terminals.record_cleanup_result(failed, Ok(()));
        assert!(terminals.cleanup_errors.is_empty());
        assert!(terminals.error.is_none());
    }
}
