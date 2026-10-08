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

pub struct AgentFocusRequested;
impl EventEmitter<AgentFocusRequested> for ProjectsState {}
pub struct ProjectsState {
    pub catalog: Projects,
    pub ready: bool,
    pub busy: bool,
    pub error: Option<String>,
    client: Option<SettingsClient>,
    sessions: HashMap<WorkspaceId, Workspace>,
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
        for workspace in self.sessions.values_mut() {
            update(workspace, source, session);
        }
        self.changed(cx);
        cx.notify();
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
        for workspace in self.sessions.values_mut() {
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
            cx.observe(&app.workspace, |this, _, cx| this.changed(cx)),
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
    pub fn runtime_workspaces(&self, cx: &App) -> Vec<Workspace> {
        self.snapshot(cx).workspaces
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
        SessionSnapshot {
            projects: self.catalog.clone(),
            workspaces,
            layout: app.layout.read(cx).clone(),
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
    pub fn select(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.select_target(id, None, cx);
    }
    fn focus_selected(&self, pane: PaneId, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        app.workspace.update(cx, |workspace, cx| {
            if workspace.activate_pane(pane).is_ok() {
                cx.notify();
            }
        });
        if let Some(view) = app.terminals.read(cx).views.get(&pane).cloned() {
            view.update(cx, |view, cx| view.request_focus(cx));
        }
    }
    fn select_target(&mut self, id: WorkspaceId, pane: Option<PaneId>, cx: &mut Context<Self>) {
        if !self.available() {
            return;
        }
        if self.catalog.active == Some(id) {
            if let Some(pane) = pane {
                self.focus_selected(pane, cx);
            }
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
                            if workspace.activate_pane(pane).is_err() {
                                cx.notify();
                                return;
                            }
                        }
                        this.save(next, cx);
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
        let mut next = self.catalog.clone();
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
        let editors = cx.global::<AppState>().editors.clone();
        if editors.read(cx).busy(&panes, cx) {
            let repository = repository.to_owned();
            let target = cx.entity();
            super::Editors::guard(
                panes,
                move |cx| target.update(cx, |state, cx| state.close_repository(&repository, cx)),
                cx,
            );
            return;
        }
        for id in ids {
            next.close(id);
        }
        self.save(next, cx);
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
        let editors = cx.global::<AppState>().editors.clone();
        if editors.read(cx).busy(&panes, cx) {
            let target = cx.entity();
            super::Editors::guard(
                panes,
                move |cx| target.update(cx, |state, cx| state.close(id, cx)),
                cx,
            );
            return;
        }
        let mut next = self.catalog.clone();
        if next.close(id) {
            self.save(next, cx);
        }
    }
    fn save(&mut self, next: Projects, cx: &mut Context<Self>) {
        if self.quitting {
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
            if let Some(old) = self.catalog.active {
                self.sessions.insert(old, app.workspace.read(cx).clone());
            }
            let workspace = if let Some(id) = next.active {
                self.sessions
                    .remove(&id)
                    .unwrap_or_else(|| Workspace::empty(id))
            } else {
                Workspace::new()
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
