use super::AppState;
use canopy_desktop::{
    settings::SettingsClient,
    state::{
        projects::{Projects, canonical_directory, restore},
        session::SessionSnapshot,
        workspace::{PaneId, Workspace, WorkspaceId},
    },
};
use gpui_kit::*;
use std::collections::HashMap;
mod conversations;

pub struct AgentFocusRequested;
impl EventEmitter<AgentFocusRequested> for ProjectsState {}
pub struct ProjectsState {
    pub catalog: Projects,
    pub ready: bool,
    pub busy: bool,
    pub error: Option<String>,
    client: Option<SettingsClient>,
    sessions: HashMap<WorkspaceId, Workspace>,
    /// The "No project" space while a project is active; otherwise it is the
    /// global workspace entity.
    scratch: Workspace,
    task: Option<Task<()>>,
    picking: bool,
    pub(super) quitting: bool,
    enabled: bool,
    last_observed: Option<SessionSnapshot>,
    generation: u64,
    writing: bool,
    pub(super) writer: Option<Task<()>>,
    observers: Vec<Subscription>,
}
impl ProjectsState {
    pub fn clear_agent_session(
        &mut self,
        pane: canopy_desktop::state::workspace::PaneId,
        cx: &mut Context<Self>,
    ) {
        let active = cx.global::<AppState>().workspace.clone();
        active.update(cx, |workspace, cx| {
            if let Some((tab, mut metadata)) = workspace
                .tabs()
                .iter()
                .find_map(|t| t.root.find(pane).map(|p| (t.id, p.metadata.clone())))
            {
                metadata.resume_id = None;
                let _ = workspace.set_pane_metadata(tab, pane, metadata);
                cx.notify();
            }
        });
        self.changed(cx);
    }

    pub fn record_agent_session(
        &mut self,
        source: &canopy_desktop::state::workspace::Pane,
        session: &str,
        cx: &mut Context<Self>,
    ) {
        fn update(
            workspace: &mut Workspace,
            source: &canopy_desktop::state::workspace::Pane,
            session: &str,
        ) -> bool {
            workspace
                .bind_agent_session(source, session)
                .unwrap_or(false)
        }
        let active = cx.global::<AppState>().workspace.clone();
        active.update(cx, |workspace, cx| {
            if update(workspace, source, session) {
                cx.notify();
            }
        });
        for workspace in self
            .sessions
            .values_mut()
            .chain(std::iter::once(&mut self.scratch))
        {
            update(workspace, source, session);
        }
        self.changed(cx);
        cx.notify();
    }

    /// A chat starts over without its provider session (an edit of its first message).
    pub fn forget_chat_session(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        fn clear(workspace: &mut Workspace, pane: PaneId) -> bool {
            let Some((tab, mut metadata)) = workspace
                .tabs()
                .iter()
                .find_map(|t| t.root.find(pane).map(|p| (t.id, p.metadata.clone())))
            else {
                return false;
            };
            if metadata.resume_id.is_none() {
                return false;
            }
            metadata.resume_id = None;
            workspace.set_pane_metadata(tab, pane, metadata).is_ok()
        }
        let mut changed = false;
        cx.global::<AppState>()
            .workspace
            .clone()
            .update(cx, |workspace, cx| {
                if clear(workspace, pane) {
                    changed = true;
                    cx.notify();
                }
            });
        for workspace in self
            .sessions
            .values_mut()
            .chain(std::iter::once(&mut self.scratch))
        {
            changed |= clear(workspace, pane);
        }
        if changed {
            self.changed(cx);
            cx.notify();
        }
    }

    pub fn task_prompt_pasted(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        fn clear(workspace: &mut Workspace, pane: PaneId) {
            if let Some((tab, mut metadata)) = workspace
                .tabs()
                .iter()
                .find_map(|t| t.root.find(pane).map(|p| (t.id, p.metadata.clone())))
            {
                metadata.task_prompt = None;
                let _ = workspace.set_pane_metadata(tab, pane, metadata);
            }
        }
        cx.global::<AppState>()
            .workspace
            .clone()
            .update(cx, |w, cx| {
                clear(w, pane);
                cx.notify();
            });
        for workspace in self
            .sessions
            .values_mut()
            .chain(std::iter::once(&mut self.scratch))
        {
            clear(workspace, pane);
        }
        self.changed(cx);
        cx.notify();
    }

    pub fn new() -> Self {
        Self {
            catalog: Projects::default(),
            ready: false,
            busy: false,
            error: None,
            client: None,
            sessions: HashMap::new(),
            scratch: Workspace::new(),
            task: None,
            picking: false,
            quitting: false,
            enabled: false,
            last_observed: None,
            generation: 0,
            writing: false,
            writer: None,
            observers: vec![],
        }
    }
    pub fn shutdown_task(&mut self) -> Option<Task<()>> {
        self.quitting = true;
        let task = self.task.take();
        if self.picking {
            drop(task);
            None
        } else {
            task
        }
    }
    pub fn bind(&mut self, app: &AppState, cx: &mut Context<Self>) {
        self.observers = vec![
            cx.observe(&app.workspace, |this, workspace, cx| {
                this.changed(cx);
                let workspace = workspace.read(cx);
                let names: Vec<_> = workspace
                    .all_panes()
                    .into_iter()
                    .filter_map(|p| {
                        Some((
                            p.id,
                            workspace.chat_title(p.id)?.to_owned(),
                            p.metadata.chat_title_state,
                        ))
                    })
                    .collect();
                cx.defer(move |cx| {
                    for (id, name, state) in names {
                        Self::apply_chat_name(id, &name, state, cx);
                    }
                });
            }),
            cx.observe(&app.layout, |this, _, cx| this.changed(cx)),
        ];
    }
    pub fn initialize(&mut self, client: SettingsClient, reopen: bool, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        self.client = Some(client.clone());
        self.busy = true;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                if !reopen {
                    return Ok((Projects::default(), None, 0));
                }
                if let Some(session) = client.load_session().await? {
                    let (session, skipped) = cx
                        .background_executor()
                        .spawn(async move {
                            let skipped=session.projects.items.iter().filter(|p|
                                canonical_directory(p.worktree_path.as_ref().unwrap_or(&p.path)).is_err()).count();
                            (session, skipped)
                        })
                        .await;
                    return Ok((session.projects.clone(), Some(session), skipped));
                }
                let saved = client.load_projects().await?;
                let (catalog, skipped) = cx
                    .background_executor()
                    .spawn(async move { restore(saved) })
                    .await;
                Ok::<_, canopy_desktop::settings::Error>((catalog, None, skipped))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((catalog, session, skipped)) => {
                        let app = cx.global::<AppState>().clone();
                        let scratch = session
                            .as_ref()
                            .and_then(|s| s.scratch.clone())
                            .unwrap_or_else(|| app.workspace.read(cx).clone());
                        // The global entity holds "No project" until a project is published.
                        app.workspace.update(cx, |state, cx| {
                            *state = scratch;
                            cx.notify();
                        });
                        let restored = reopen && session.is_some();
                        if let Some(session) = session {
                            this.sessions =
                                session.workspaces.into_iter().map(|w| (w.id, w)).collect();
                            let layout = cx.global::<AppState>().layout.clone();
                            layout.update(cx, |state, cx| {
                                *state = session.layout;
                                cx.notify();
                            });
                        }
                        this.publish(catalog, cx);
                        this.ready = true;
                        this.enabled = reopen;
                        // Saved chats whose panes are gone (closed). Only against a restored
                        // layout: without one, nothing says which chats still exist.
                        let chats: Vec<String> = this
                            .runtime_workspaces(cx)
                            .iter()
                            .flat_map(|workspace| workspace.all_panes())
                            .filter_map(|pane| pane.metadata.chat.as_ref()?.id.clone())
                            .collect();
                        if restored {
                            cx.global::<AppState>()
                                .chat_history
                                .clone()
                                .update(cx, |history, cx| history.retain(chats, cx));
                        }
                        this.last_observed = Some(this.snapshot(cx));
                        if skipped > 0 {
                            this.error = Some(format!("{skipped} folder(s) are unavailable. Saved pane layouts are preserved."));
                        }
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }
    /// Every workspace with its live state, including "No project".
    pub fn runtime_workspaces(&self, cx: &App) -> Vec<Workspace> {
        let mut workspaces = self.snapshot(cx).workspaces;
        workspaces.push(self.scratch_workspace(cx));
        workspaces
    }
    pub(crate) fn scratch_workspace(&self, cx: &App) -> Workspace {
        if self.catalog.active.is_none() {
            cx.global::<AppState>().workspace.read(cx).clone()
        } else {
            self.scratch.clone()
        }
    }
    /// Identity of the "No project" space.
    pub fn scratch_id(&self, cx: &App) -> WorkspaceId {
        if self.catalog.active.is_none() {
            cx.global::<AppState>().workspace.read(cx).id
        } else {
            self.scratch.id
        }
    }
    fn snapshot(&self, cx: &App) -> SessionSnapshot {
        let app = cx.global::<AppState>();
        let workspaces = self
            .catalog
            .items
            .iter()
            .filter_map(|project| {
                if Some(project.workspace) == self.catalog.active {
                    Some(app.workspace.read(cx).clone())
                } else {
                    self.sessions.get(&project.workspace).cloned()
                }
            })
            .collect();
        let scratch = self.scratch_workspace(cx);
        SessionSnapshot {
            projects: self.catalog.clone(),
            workspaces,
            layout: app.layout.read(cx).clone(),
            scratch: (!scratch.tabs().is_empty()).then_some(scratch),
        }
    }
    pub(super) fn final_snapshot(&self, cx: &App) -> Option<(SettingsClient, SessionSnapshot)> {
        if !self.ready || !self.enabled {
            return None;
        }
        Some((self.client.clone()?, self.snapshot(cx)))
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        if !self.ready || !self.enabled || self.quitting {
            return;
        }
        let snapshot = self.snapshot(cx);
        if self.last_observed.as_ref() == Some(&snapshot) {
            return;
        }
        self.last_observed = Some(snapshot);
        self.generation += 1;
        if self.writing {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.writing = true;
        self.writer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                let Ok((generation, snapshot)) =
                    this.update(cx, |this, cx| (this.generation, this.snapshot(cx)))
                else {
                    return;
                };
                let result = client.save_session(snapshot).await;
                let again = this
                    .update(cx, |this, cx| {
                        match result {
                            Ok(()) => {
                                if this.generation != generation && !this.quitting {
                                    return true;
                                }
                                this.error = None;
                            }
                            Err(error) => {
                                this.error = Some(format!(
                                    "Layout not saved: {error}. Your current layout is still open."
                                ))
                            }
                        }
                        this.writing = false;
                        cx.notify();
                        false
                    })
                    .unwrap_or(false);
                if !again {
                    break;
                }
            }
        }));
    }
    fn available(&self) -> bool {
        self.ready && !self.busy && !self.quitting
    }
    pub fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.available() {
            return;
        }
        let parent = window.window_handle();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open folder".into()),
        });
        self.busy = true;
        self.picking = true;
        self.error = None;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let selection = paths.await;
            let _ = parent.update(cx, |_, window, _| window.activate_window());
            let path = match selection {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                _ => {
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        this.picking = false;
                        this.error = Some("Could not open the folder picker.".into());
                        cx.notify();
                    });
                    return;
                }
            };
            let Some(path) = path else {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    this.picking = false;
                    cx.notify();
                });
                return;
            };
            let _ = this.update(cx, |this, _| this.picking = false);
            let result = cx
                .background_executor()
                .spawn(async move { canopy_desktop::git::workspace_directory(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(path) => {
                        let mut next = this.catalog.clone();
                        next.open(path);
                        this.save(next, cx);
                    }
                    Err(_) => {
                        this.error = Some("This folder is unavailable or cannot be read.".into());
                        cx.notify();
                    }
                }
            });
        }));
    }
    pub fn focus_agent(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        cx.emit(AgentFocusRequested);
        let target = self
            .runtime_workspaces(cx)
            .into_iter()
            .find(|workspace| workspace.all_panes().iter().any(|p| p.id == pane))
            .map(|workspace| workspace.id);
        if let Some(id) = target {
            self.select_target(id, Some(pane), cx);
        }
    }
    /// Selecting a CLI agent activates its existing terminal tab in the work area.
    pub fn focus_agent_terminal(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        cx.emit(AgentFocusRequested);
        let target = self
            .runtime_workspaces(cx)
            .into_iter()
            .find(|w| {
                w.all_panes().iter().any(|p| {
                    p.id == pane
                        && p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
                        && canopy_desktop::chat::provider::Provider::from_tool(&p.tool).is_some()
                })
            })
            .map(|w| w.id);
        if let Some(id) = target {
            self.select_target(id, Some(pane), cx);
        }
    }
    pub fn select(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.select_target(id, None, cx);
    }
    /// Moves a chat pane (before its first message) to another account profile.
    pub fn set_chat_account(
        &mut self,
        pane: PaneId,
        tool: &str,
        profile: String,
        cx: &mut Context<Self>,
    ) {
        self.update_pane(cx, |workspace| {
            workspace
                .set_chat_account(pane, tool, &profile)
                .unwrap_or(false)
        });
    }
    /// Applies `update` to the active workspace and every cached one.
    fn update_pane(&mut self, cx: &mut Context<Self>, update: impl Fn(&mut Workspace) -> bool) {
        let active = cx.global::<AppState>().workspace.clone();
        let mut changed = false;
        active.update(cx, |workspace, cx| {
            if update(workspace) {
                changed = true;
                cx.notify();
            }
        });
        for workspace in self
            .sessions
            .values_mut()
            .chain(std::iter::once(&mut self.scratch))
        {
            changed |= update(workspace);
        }
        if changed {
            self.changed(cx);
            cx.notify();
        }
    }
    /// Persists a chat pane's choices and its first-message title, wherever the
    /// pane's workspace currently lives (active entity or cached session).
    pub fn update_chat(
        &mut self,
        pane: PaneId,
        settings: Option<canopy_desktop::chat::config::ChatSettings>,
        title: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        fn update(
            workspace: &mut Workspace,
            pane: PaneId,
            settings: &Option<canopy_desktop::chat::config::ChatSettings>,
            title: Option<&str>,
        ) -> bool {
            let mut changed = false;
            if let Some(settings) = settings {
                changed |= workspace
                    .set_chat_settings(pane, settings.clone())
                    .unwrap_or(false);
            }
            if let Some(title) = title {
                changed |= workspace
                    .retitle_placeholder(pane, crate::ui::chat::NEW_CHAT_TITLE, title)
                    .unwrap_or(false);
            }
            changed
        }
        let active = cx.global::<AppState>().workspace.clone();
        let mut changed = false;
        active.update(cx, |workspace, cx| {
            if update(workspace, pane, &settings, title) {
                changed = true;
                cx.notify();
            }
        });
        for workspace in self
            .sessions
            .values_mut()
            .chain(std::iter::once(&mut self.scratch))
        {
            changed |= update(workspace, pane, &settings, title);
        }
        if title.is_some() {
            self.sync_chat_name(pane, cx);
        }
        if changed {
            self.changed(cx);
            cx.notify();
        }
    }
    fn focus_selected(&self, pane: PaneId, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        app.workspace.update(cx, |workspace, cx| {
            if workspace.select_session(pane).is_ok() {
                cx.notify();
            }
        });
        if let Some(view) = app.terminals.read(cx).views.get(&pane).cloned() {
            view.update(cx, |view, cx| view.request_focus(cx));
        }
        if let Some(view) = app.chats.read(cx).views.get(&pane).cloned() {
            view.update(cx, |view, cx| view.request_focus(cx));
        }
    }
    /// Shows the "No project" space.
    pub fn select_none(&mut self, cx: &mut Context<Self>) {
        if !self.available() || self.catalog.active.is_none() {
            return;
        }
        let mut next = self.catalog.clone();
        next.select_none();
        self.save(next, cx);
    }

    /// Adds a chat tab to `target` (a project workspace, or "No project" for `None`)
    /// and shows that space.
    pub fn open_chat(
        &mut self,
        target: Option<WorkspaceId>,
        title: &str,
        tool: &str,
        metadata: canopy_desktop::state::workspace::PaneMetadata,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        fn add(
            workspace: &mut Workspace,
            title: &str,
            tool: &str,
            metadata: canopy_desktop::state::workspace::PaneMetadata,
        ) -> Result<(), String> {
            let tab = workspace.open_conversation(title, tool);
            let pane = workspace.active().expect("new tab").focused;
            if workspace.set_pane_metadata(tab, pane, metadata).is_err() {
                let _ = workspace.close(tab);
                return Err("The chat could not be created.".into());
            }
            Ok(())
        }
        if !self.available() {
            return Err("Projects are busy.".into());
        }
        let app = cx.global::<AppState>().clone();
        if metadata.cwd.as_deref().is_some_and(|cwd| {
            cx.try_global::<super::ChatFolders>()
                .is_some_and(|folders| folders.contains(cwd))
        }) {
            return Err("This chat folder is being removed.".into());
        }
        let active = app.workspace.read(cx).id;
        let target_id = target.unwrap_or_else(|| self.scratch_id(cx));
        if target_id == active {
            let mut result = Ok(());
            app.workspace.update(cx, |workspace, cx| {
                result = add(workspace, title, tool, metadata);
                cx.notify();
            });
            self.changed(cx);
            return result;
        }
        match target {
            None => {
                add(&mut self.scratch, title, tool, metadata)?;
                self.select_none(cx);
            }
            Some(id) => {
                let workspace = self
                    .sessions
                    .get_mut(&id)
                    .ok_or("This project is no longer open.")?;
                add(workspace, title, tool, metadata)?;
                self.select_target(id, None, cx);
            }
        }
        Ok(())
    }

    pub fn show_terminal(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.select_then(id, None, Some(super::ToolsState::show_terminal), cx);
    }
    pub fn create_chat_in(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.select_then(
            id,
            None,
            Some(|cx| {
                cx.global::<AppState>()
                    .chats
                    .clone()
                    .update(cx, |chats, cx| chats.open_current(cx));
            }),
            cx,
        );
    }
    fn selected_action(id: WorkspaceId, after: Option<fn(&mut App)>, cx: &mut Context<Self>) {
        if let Some(after) = after {
            cx.defer(move |cx| {
                let app = cx.global::<AppState>();
                if app.workspace.read(cx).id == id && !app.projects.read(cx).busy {
                    after(cx);
                }
            });
        }
    }
    fn select_target(&mut self, id: WorkspaceId, pane: Option<PaneId>, cx: &mut Context<Self>) {
        self.select_then(id, pane, None, cx);
    }
    fn select_then(
        &mut self,
        id: WorkspaceId,
        pane: Option<PaneId>,
        after: Option<fn(&mut App)>,
        cx: &mut Context<Self>,
    ) {
        if !self.available() {
            return;
        }
        if self.catalog.active == Some(id)
            || (self.catalog.active.is_none() && self.scratch_id(cx) == id)
        {
            if let Some(pane) = pane {
                self.focus_selected(pane, cx);
            }
            Self::selected_action(id, after, cx);
            return;
        }
        if self.scratch.id == id {
            if let Some(pane) = pane
                && self.scratch.select_session(pane).is_err()
            {
                return;
            }
            self.select_none(cx);
            if let Some(pane) = pane {
                self.focus_selected(pane, cx);
            }
            Self::selected_action(id, after, cx);
            return;
        }
        if !self.catalog.items.iter().any(|p| p.workspace == id) {
            return;
        }
        let mut next = self.catalog.clone();
        if !next.select(id) {
            return;
        }
        let selected = next.current().expect("selected project");
        let path = selected
            .worktree_path
            .as_ref()
            .unwrap_or(&selected.path)
            .clone();
        self.busy = true;
        self.error = None;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { canonical_directory(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(_) => {
                        // Select before publishing: observers must never start the old active tab.
                        if let Some(pane) = pane {
                            let Some(workspace) = this.sessions.get_mut(&id) else {
                                cx.notify();
                                return;
                            };
                            if workspace.select_session(pane).is_err() {
                                cx.notify();
                                return;
                            }
                        }
                        this.save(next, cx);
                        Self::selected_action(id, after, cx);
                        if let Some(pane) = pane {
                            this.focus_selected(pane, cx);
                        }
                    }
                    Err(_) => {
                        this.error = Some("This folder is unavailable or cannot be read.".into());
                        cx.notify();
                    }
                }
            });
        }));
    }
    pub fn open_worktree(
        &mut self,
        repository: std::path::PathBuf,
        path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.open_worktree_with_base(repository, path, None, cx);
    }
    pub fn open_worktree_with_base(
        &mut self,
        repository: std::path::PathBuf,
        path: std::path::PathBuf,
        base: Option<canopy_desktop::state::projects::WorktreeBase>,
        cx: &mut Context<Self>,
    ) {
        if !self.available() {
            return;
        }
        let mut next = self.catalog.clone();
        next.open_worktree_with_base(repository, path, base);
        self.save(next, cx);
    }
    pub fn close_repository(&mut self, repository: &std::path::Path, cx: &mut Context<Self>) {
        if !self.available() {
            return;
        }
        let next = self.catalog.clone();
        let ids: Vec<_> = next
            .items
            .iter()
            .filter(|p| p.repository_path.as_deref().unwrap_or(&p.path) == repository)
            .map(|p| p.workspace)
            .collect();
        let panes: Vec<_> = self
            .runtime_workspaces(cx)
            .iter()
            .filter(|w| ids.contains(&w.id))
            .flat_map(|w| w.all_panes())
            .map(|p| p.id)
            .collect();
        let target = cx.entity();
        cx.defer(move |cx| {
            super::Editors::guard_closing(
                panes.clone(),
                {
                    let target = target.clone();
                    let ids = ids.clone();
                    let panes = panes.clone();
                    move |cx| target.update(cx, |state, cx| state.close_confirmed(&ids, &panes, cx))
                },
                cx,
            )
        });
    }
    pub fn set_repository_roots(
        &mut self,
        roots: Vec<(std::path::PathBuf, std::path::PathBuf)>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for (path, root) in roots {
            if let Some(project) = self.catalog.items.iter_mut().find(|p| p.path == path)
                && project.repository_path.as_ref() != Some(&root)
            {
                project.repository_path = Some(root);
                changed = true;
            }
        }
        if changed {
            self.changed(cx);
            cx.notify();
        }
    }
    pub fn close(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        if !self.available() {
            return;
        }
        let panes: Vec<_> = self
            .runtime_workspaces(cx)
            .iter()
            .filter(|w| w.id == id)
            .flat_map(|w| w.all_panes())
            .map(|p| p.id)
            .collect();
        let target = cx.entity();
        cx.defer(move |cx| {
            super::Editors::guard_closing(
                panes.clone(),
                {
                    let panes = panes.clone();
                    move |cx| {
                        target.update(cx, |state, cx| state.close_confirmed(&[id], &panes, cx))
                    }
                },
                cx,
            )
        });
    }
    fn close_confirmed(&mut self, ids: &[WorkspaceId], panes: &[PaneId], cx: &mut Context<Self>) {
        if !self.available() {
            return;
        }
        let current: std::collections::HashSet<_> = self
            .runtime_workspaces(cx)
            .iter()
            .filter(|w| ids.contains(&w.id))
            .flat_map(|w| w.all_panes())
            .map(|p| p.id)
            .collect();
        if current != panes.iter().copied().collect() {
            self.error =
                Some("The project's tabs changed. Close the project again to review them.".into());
            cx.notify();
            return;
        }
        let mut next = self.catalog.clone();
        for id in ids {
            next.close(*id);
        }
        self.save(next, cx);
    }
    fn save(&mut self, next: Projects, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        if next.current().is_some_and(|p| {
            cx.try_global::<super::ChatFolders>()
                .is_some_and(|f| f.contains(&p.path))
        }) {
            self.error = Some(
                "This folder is being removed with a chat. Retry cleanup before opening it.".into(),
            );
            cx.notify();
            return;
        }
        self.publish(next, cx);
        self.enabled = true;
        self.error = None;
        self.changed(cx);
        cx.notify();
    }
    fn publish(&mut self, next: Projects, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        for project in &next.items {
            if self.catalog.active != Some(project.workspace)
                && !self.sessions.contains_key(&project.workspace)
            {
                let workspace = Workspace::empty(project.workspace);
                self.sessions.insert(project.workspace, workspace);
            }
        }
        if self.catalog.active != next.active {
            let current = app.workspace.read(cx).clone();
            match self.catalog.active {
                Some(old) => {
                    self.sessions.insert(old, current);
                }
                None => self.scratch = current,
            }
            let workspace = if let Some(id) = next.active {
                self.sessions
                    .remove(&id)
                    .unwrap_or_else(|| Workspace::empty(id))
            } else {
                self.scratch.clone()
            };
            app.workspace.update(cx, |state, cx| {
                *state = workspace;
                cx.notify();
            });
        }
        self.sessions
            .retain(|id, _| next.items.iter().any(|p| p.workspace == *id));
        self.catalog = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_desktop::state::workspace::{PaneKind, PaneMetadata};
    use core::prelude::v1::test;
    use gpui_kit::TestAppContext;

    fn titles(workspace: &Workspace) -> Vec<String> {
        workspace
            .tabs()
            .iter()
            .map(|tab| tab.title.clone())
            .collect()
    }

    #[gpui_kit::test]
    fn no_project_and_projects_keep_their_own_tabs(cx: &mut TestAppContext) {
        let folder = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(folder.path()).unwrap();
        let (app, project) = cx.update(|cx| {
            let mut scratch = Workspace::new();
            scratch.open("Scratch chat", "claude");
            let app = AppState::install_test(scratch, cx);
            let project = app.projects.update(cx, |projects, cx| {
                let mut next = projects.catalog.clone();
                let id = next.open(path.clone());
                next.select_none();
                let mut workspace = Workspace::empty(id);
                workspace.open("Project shell", "shell");
                projects.sessions.insert(id, workspace);
                projects.catalog = next;
                projects.ready = true;
                cx.notify();
                id
            });
            (app, project)
        });
        cx.update(|cx| {
            let projects = app.projects.read(cx);
            let ids: Vec<_> = projects
                .runtime_workspaces(cx)
                .iter()
                .map(|w| w.id)
                .collect();
            assert!(ids.contains(&project) && ids.contains(&projects.scratch_id(cx)));
            app.projects
                .update(cx, |projects, cx| projects.select_target(project, None, cx));
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(titles(app.workspace.read(cx)), ["Project shell"]);
            assert_eq!(titles(&app.projects.read(cx).scratch), ["Scratch chat"]);
            // A chat for "No project" goes to that space and shows it.
            app.projects.update(cx, |projects, cx| {
                projects
                    .open_chat(
                        None,
                        "New chat",
                        "claude",
                        PaneMetadata {
                            cwd: Some(path.clone()),
                            kind: PaneKind::Chat,
                            ..Default::default()
                        },
                        cx,
                    )
                    .unwrap()
            });
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(app.projects.read(cx).catalog.active, None);
            assert_eq!(titles(app.workspace.read(cx)), ["Scratch chat", "New chat"]);
            assert_eq!(
                titles(&app.projects.read(cx).sessions[&project]),
                ["Project shell"],
                "the project keeps its tabs"
            );
        });
    }

    #[gpui_kit::test]
    fn opening_a_folders_terminal_waits_for_selection_and_reuses_its_shell(
        cx: &mut TestAppContext,
    ) {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let (app, first, shell) = cx.update(|cx| {
            let app = AppState::install_test(Workspace::new(), cx);
            app.tools.update(cx, |tools, _| tools.ready = true);
            app.projects.update(cx, |projects, cx| {
                projects.ready = true;
                projects.open_worktree(a.path().into(), a.path().into(), cx);
            });
            let first = app.workspace.read(cx).id;
            let shell = app.workspace.update(cx, |w, _| {
                w.open("Terminal 1", "shell");
                w.active().unwrap().focused
            });
            app.projects.update(cx, |projects, cx| {
                projects.open_worktree(b.path().into(), b.path().into(), cx);
                projects.show_terminal(first, cx);
            });
            assert_ne!(app.workspace.read(cx).id, first);
            assert!(app.workspace.read(cx).tabs().is_empty());
            (app, first, shell)
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(app.workspace.read(cx).id, first);
            assert_eq!(app.workspace.read(cx).active().unwrap().focused, shell);
            assert_eq!(app.workspace.read(cx).tabs().len(), 1);
            assert!(app.projects.read(cx).error.is_none());
        });
    }
}
