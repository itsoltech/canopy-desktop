mod integrations;
mod task_drafts;
pub use integrations::{IntegrationsState, WriteFinished};
pub use task_drafts::DraftsState;
mod editors;
pub use editors::{Editors, GuardRequest};
mod files;
pub use files::FilesState;
mod agents;
pub use agents::{AgentSession, AgentsState, IntegrationHealth};
mod diffs;
pub use diffs::Diffs;
mod changes;
pub use changes::{ChangesState, UpstreamRequest};
mod git;
pub use git::{GitState, RemovalConfirmation};
mod projects;
mod tools;
pub use tools::{ToolsState, WorktreeAgent};
mod terminals;
pub use projects::{AgentFocusRequested, ProjectsState};
pub use terminals::Terminals;
// GPUI ownership and I/O coordination; domain operations live in the library.
use canopy_desktop::{
    settings::{Access, Change, Preferences, SettingsClient},
    state::{layout::LayoutState, workspace::Workspace},
};
use gpui_kit::*;

#[derive(Clone)]
pub struct AppState {
    pub settings: Entity<SettingsState>,
    pub projects: Entity<ProjectsState>,
    pub workspace: Entity<Workspace>,
    pub layout: Entity<LayoutState>,
    pub terminals: Entity<Terminals>,
    pub tools: Entity<ToolsState>,
    pub git: Entity<GitState>,
    pub changes: Entity<ChangesState>,
    pub diffs: Entity<Diffs>,
    pub agents: Entity<AgentsState>,
    pub integrations: Entity<IntegrationsState>,
    pub task_drafts: Entity<DraftsState>,
    pub files: Entity<FilesState>,
    pub editors: Entity<Editors>,
    pub toasts: Entity<crate::ui::toasts::ToastHost>,
    pub notch_status: Entity<NotchStatus>,
}

pub struct NotchStatus {
    pub error: Option<SharedString>,
}

impl NotchStatus {
    pub fn fail(&mut self, error: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.error = Some(error.into());
        cx.notify();
    }
    #[cfg(target_os = "windows")]
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
    }
}
impl Global for AppState {}
impl AppState {
    #[cfg(test)]
    pub fn install_test(workspace: Workspace, cx: &mut App) -> Self {
        let state = Self {
            workspace: cx.new(|_| workspace),
            projects: cx.new(|_| ProjectsState::new()),
            layout: cx.new(|_| LayoutState::default()),
            settings: cx.new(SettingsState::test),
            terminals: cx.new(|_| Terminals::new()),
            tools: cx.new(|_| ToolsState::new()),
            git: cx.new(|_| GitState::new()),
            changes: cx.new(|_| ChangesState::new()),
            agents: cx.new(|_| AgentsState::new()),
            integrations: cx.new(|_| IntegrationsState::new()),
            task_drafts: cx.new(|_| DraftsState::new()),
            files: cx.new(|_| FilesState::new()),
            editors: cx.new(|_| Editors::new()),
            diffs: cx.new(|_| Diffs::new()),
            toasts: cx.new(|_| crate::ui::toasts::ToastHost::new()),
            notch_status: cx.new(|_| NotchStatus { error: None }),
        };
        cx.set_global(state.clone());
        state
    }

    pub fn quit(cx: &mut App) {
        let app = cx.global::<Self>().clone();
        if app.settings.read(cx).quitting {
            return;
        }
        if app.editors.read(cx).any_dirty(cx) {
            let ids = app.editors.read(cx).views.keys().copied().collect();
            Editors::guard(ids, Self::quit, cx);
            return;
        }
        app.settings.update(cx, |s, cx| {
            s.quitting = true;
            cx.notify();
        });
        app.task_drafts.update(cx, |s, _| s.pause());
        let write = app.integrations.update(cx, |s, _| s.begin_quit());
        cx.spawn(async move |cx| {
            if let Some(write) = write {
                write.await;
            }
            let flush = app.task_drafts.update(cx, |s, cx| s.flush(cx));
            let result = flush.await;
            cx.update(|cx| {
                app.settings.update(cx, |s, cx| {
                    s.quitting = false;
                    cx.notify();
                });
                if let Err(error) = result {
                    app.task_drafts.update(cx, |s, cx| s.resume(cx));
                    app.integrations.update(cx, |s, cx| s.cancel_quit(cx));
                    app.projects.update(cx, |s, cx| {
                        s.error = Some(format!(
                            "Could not save task drafts before closing: {error}"
                        ));
                        cx.notify();
                    });
                } else if app.editors.read(cx).any_dirty(cx) {
                    app.task_drafts.update(cx, |s, cx| s.resume(cx));
                    app.integrations.update(cx, |s, cx| s.cancel_quit(cx));
                    let ids = app.editors.read(cx).views.keys().copied().collect();
                    Editors::guard(ids, Self::quit, cx);
                } else {
                    Self::finish_quit(cx);
                }
            });
        })
        .detach();
    }
    fn finish_quit(cx: &mut App) {
        let editors = cx.global::<Self>().editors.clone();
        if editors.read(cx).any_dirty(cx) {
            let ids = editors.read(cx).views.keys().copied().collect();
            Editors::guard(ids, Self::quit, cx);
            return;
        }
        let settings = cx.global::<Self>().settings.clone();
        let projects = cx.global::<Self>().projects.clone();
        let task = settings.update(cx, |state, cx| {
            if state.quitting {
                return None;
            }
            state.quitting = true;
            cx.notify();
            Some(state._task.take())
        });
        let Some(task) = task else {
            return;
        };
        let integrations = cx.global::<Self>().integrations.clone();
        let integration_task = integrations.update(cx, |state, _| state.begin_quit());
        let files = cx.global::<Self>().files.clone();
        let files_task = files.update(cx, |files, _| files.begin_quit());
        let tools = cx.global::<Self>().tools.clone();
        let tool_tasks = tools.update(cx, |state, _| state.shutdown_tasks());
        let git = cx.global::<Self>().git.clone();
        let git_task = git.update(cx, |git, _| git.begin_quit());
        let agents = cx.global::<Self>().agents.clone();
        agents.update(cx, |state, cx| state.flush(cx));
        let changes = cx.global::<Self>().changes.clone();
        let changes_task = changes.update(cx, |s, _| s.begin_quit());
        cx.spawn(async move |cx| {
            if let Some(task)=integration_task{task.await;}
            if let Some(task)=files_task{task.await;}
            if let Some(task)=changes_task{task.await;}
            if let Some(task)=git_task{task.await;}
            let project_task=projects.update(cx,|state,_|state.shutdown_task());
            for task in tool_tasks {task.await;}
            if let Some(task) = project_task {
                task.await;
            }
            if let Some(task) = task {
                task.await;
            }
            let writer=projects.update(cx,|state,_|state.writer.take());
            if let Some(writer)=writer {writer.await;}
            agents.update(cx, |state,cx|state.flush(cx));
            let final_save=projects.update(cx,|state,cx|state.final_snapshot(cx));
            if let Some((client,snapshot))=final_save && let Err(error)=client.save_session(snapshot).await {
                cx.update(|cx|cx.global::<Self>().task_drafts.clone().update(cx,|state,cx|state.resume(cx)));
                integrations.update(cx,|state,cx|state.cancel_quit(cx));
                files.update(cx,|files,cx|files.cancel_quit(cx));
                projects.update(cx,|state,cx|{state.quitting=false;state.error=Some(format!("Could not save layout before closing: {error}. Retry closing to save again."));cx.notify();});
                settings.update(cx,|state,cx|{state.quitting=false;cx.notify();});
                tools.update(cx,|state,cx|{state.quitting=false;cx.notify();});
                git.update(cx,|git,cx|git.cancel_quit(cx));
                changes.update(cx,|s,_|s.cancel_quit());
                return;
            }
            let terminals=cx.update(|cx|cx.global::<AppState>().terminals.clone());
            let stop=terminals.update(cx,|state,cx|state.shutdown(cx));
            if let Err(error) = stop.await {
                cx.update(|cx|cx.global::<Self>().task_drafts.clone().update(cx,|state,cx|state.resume(cx)));
                integrations.update(cx,|state,cx|state.cancel_quit(cx));
                files.update(cx,|files,cx|files.cancel_quit(cx));
                projects.update(cx,|state,cx|{state.quitting=false;state.error=Some(format!("Could not confirm terminal cleanup before closing: {error}. Canopy remains open."));cx.notify();});
                settings.update(cx,|state,cx|{state.quitting=false;cx.notify();});
                tools.update(cx,|state,cx|{state.quitting=false;cx.notify();});
                git.update(cx,|git,cx|git.cancel_quit(cx));
                changes.update(cx,|s,_|s.cancel_quit());
                return;
            }
            let agents = cx.update(|cx|cx.global::<AppState>().agents.clone());
            agents.update(cx, |state,cx|state.shutdown(cx)).await;
            let git=cx.update(|cx|cx.global::<AppState>().git.clone());
            let shutdown=git.update(cx,|git,cx|git.shutdown(cx));shutdown.await;
            let client = settings.update(cx, |state, _| state.client.take());
            if let Some(client) = client
                && let Err(error) = client.shutdown().await
            {
                eprintln!("Settings shutdown: {error}");
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
    pub fn init(cx: &mut App) {
        let workspace = cx.new(|_| Workspace::new());
        let state = Self {
            workspace,
            projects: cx.new(|_| ProjectsState::new()),
            layout: cx.new(|_| LayoutState::default()),
            settings: cx.new(SettingsState::new),
            terminals: cx.new(|_| Terminals::new()),
            tools: cx.new(|_| ToolsState::new()),
            git: cx.new(|_| GitState::new()),
            changes: cx.new(|_| ChangesState::new()),
            agents: cx.new(|_| AgentsState::new()),
            integrations: cx.new(|_| IntegrationsState::new()),
            task_drafts: cx.new(|_| DraftsState::new()),
            files: cx.new(|_| FilesState::new()),
            editors: cx.new(|_| Editors::new()),
            diffs: cx.new(|_| Diffs::new()),
            toasts: cx.new(|_| crate::ui::toasts::ToastHost::new()),
            notch_status: cx.new(|_| NotchStatus { error: None }),
        };
        cx.set_global(state.clone());
        state.files.update(cx, |files, cx| files.bind(cx));
        state.integrations.update(cx, |state, cx| state.bind(cx));
        state.agents.update(cx, |agents, cx| agents.bind(cx));
        state.diffs.update(cx, |diffs, cx| diffs.bind(&state, cx));
        state
            .changes
            .update(cx, |changes, cx| changes.bind(&state, cx));
        state.git.update(cx, |git, cx| git.bind(&state, cx));
        state
            .terminals
            .update(cx, |terminals, cx| terminals.bind(&state, cx));
        state
            .projects
            .update(cx, |projects, cx| projects.bind(&state, cx));
    }
}

pub struct SettingsState {
    pub values: Preferences,
    pub ready: bool,
    pub saving: bool,
    pub quitting: bool,
    pub error: Option<String>,
    pub warnings: Vec<canopy_desktop::settings::DecodeWarning>,
    client: Option<SettingsClient>,
    _task: Option<Task<()>>,
    _quit: Subscription,
}
impl SettingsState {
    #[cfg(test)]
    fn test(cx: &mut Context<Self>) -> Self {
        Self {
            values: Preferences::default(),
            ready: true,
            saving: false,
            quitting: false,
            error: None,
            warnings: vec![],
            client: None,
            _task: None,
            _quit: cx.on_app_quit(|_, _| async {}),
        }
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let task = cx.spawn(async |this, cx| {
            // Explicit opt-in for isolated development/GUI tests; the normal user DB is unchanged.
            let result = async {
                let path = cx
                    .background_executor()
                    .spawn(async {
                        let directory = canopy_desktop::platform::directories::data_dir()
                            .map_err(|_| canopy_desktop::settings::Error::DataDirectory)?;
                        canopy_desktop::platform::directories::ensure_private_dir(&directory)
                            .map_err(|_| canopy_desktop::settings::Error::DataDirectory)?;
                        Ok::<_, canopy_desktop::settings::Error>(directory.join("canopy.db"))
                    })
                    .await?;
                // Never fall back to the Electron source, nor overwrite a failed database.
                let client = match SettingsClient::open(&path, Access::ReadWrite).await {
                    Ok(client) => client,
                    Err(canopy_desktop::settings::Error::Sqlite(
                        rusqlite::Error::SqliteFailure(ref e, _),
                    )) if e.code == rusqlite::ErrorCode::CannotOpen => {
                        SettingsClient::create(&path).await?
                    }
                    Err(error) => return Err(error),
                };
                let snapshot = client.load().await?;
                Ok((client, snapshot))
            }
            .await;
            let mut project_init = None;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((client, snapshot)) => {
                        project_init =
                            Some((client.clone(), snapshot.preferences.reopen_last_workspace));
                        this.client = Some(client);
                        this.values = snapshot.preferences;
                        this.warnings = snapshot.warnings;
                        this.ready = true;
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
            if let Some((client, reopen)) = project_init {
                cx.update(|cx| {
                    let app = cx.global::<AppState>().clone();
                    app.task_drafts
                        .update(cx, |state, cx| state.initialize(client.clone(), cx));
                    app.integrations
                        .update(cx, |state, cx| state.initialize(client.clone(), cx));
                    app.tools
                        .update(cx, |state, cx| state.initialize(client, reopen, cx));
                });
            }
        });
        let quit = cx.on_app_quit(|this, _| {
            let task = this._task.take();
            let client = this.client.clone();
            async move {
                if let Some(task) = task {
                    task.await;
                }
                if let Some(client) = client
                    && let Err(error) = client.shutdown().await
                {
                    eprintln!("Settings shutdown: {error}");
                }
            }
        });
        Self {
            _quit: quit,
            values: Preferences::default(),
            ready: false,
            saving: false,
            quitting: false,
            error: None,
            warnings: vec![],
            client: None,
            _task: Some(task),
        }
    }
    /// Commit before publishing. One in-flight write prevents stale snapshots across windows.
    pub fn change(&mut self, change: Change, cx: &mut Context<Self>) {
        if !self.ready || self.saving || self.quitting {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.saving = true;
        self.error = None;
        cx.notify();
        self._task = Some(cx.spawn(async move |this, cx| {
            let result = client.apply(vec![change]).await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok(snapshot) => {
                        this.values = snapshot.preferences;
                        this.warnings = snapshot.warnings;
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }
}
