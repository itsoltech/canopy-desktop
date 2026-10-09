//! Runtime registry of chat panes. Agent processes never enter SQLite; the pane
//! keeps only its account, choices and provider session id.
use super::AppState;
use crate::ui::chat::{ChatStateChanged, ChatView, NEW_CHAT_TITLE, TurnDiffView};
use canopy_desktop::chat::provider::Provider;
use canopy_desktop::{
    chat::config::{ChatSettings, Mode},
    state::workspace::{PaneId, PaneKind, PaneMetadata, WorkspaceId},
};
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Where a new chat opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatTarget {
    /// The "No project" space, in a private folder of its own.
    NoProject,
    Project(WorkspaceId),
}

/// Closing panes whose chats still hold unsent queued messages.
pub struct ChatGuardRequest {
    pub ids: Vec<PaneId>,
    pub action: std::rc::Rc<dyn Fn(&mut App)>,
}
impl EventEmitter<ChatGuardRequest> for Chats {}

/// A closed pane whose agent process is still stopping.
struct Retiring {
    workspace: WorkspaceId,
    view: Entity<ChatView>,
    done: Arc<AtomicBool>,
    _watch: Task<()>,
}

pub struct Chats {
    pub views: HashMap<PaneId, Entity<ChatView>>,
    /// Turn-diff panes, the resource they were built from and whether it had data.
    pub turn_diffs: HashMap<PaneId, (String, bool, Entity<TurnDiffView>)>,
    pub error: Option<String>,
    workspaces: HashMap<PaneId, WorkspaceId>,
    events: HashMap<PaneId, Subscription>,
    retiring: Vec<Retiring>,
    stopping: bool,
    agents_ready: bool,
    /// The space last shown once everything was ready (None = "No project"); entering
    /// an empty "No project" space starts a chat, closing its last tab does not.
    last_space: Option<Option<WorkspaceId>>,
    opening: Option<Task<()>>,
}

impl Chats {
    pub fn unsent(&self, ids: &[PaneId], cx: &App) -> bool {
        ids.iter()
            .any(|id| self.views.get(id).is_some_and(|v| v.read(cx).has_unsent()))
    }
    pub fn any_unsent(&self, cx: &App) -> bool {
        self.views.values().any(|v| v.read(cx).has_unsent())
    }
    fn unsent_count(&self, ids: &[PaneId], cx: &App) -> usize {
        ids.iter()
            .filter_map(|id| self.views.get(id))
            .map(|v| v.read(cx).unsent())
            .sum()
    }

    /// Runs `action` now, or after the user agreed to drop the queued messages.
    pub fn guard(ids: Vec<PaneId>, action: impl Fn(&mut App) + 'static, cx: &mut App) {
        let chats = cx.global::<AppState>().chats.clone();
        if chats.read(cx).unsent(&ids, cx) {
            chats.update(cx, |_, cx| {
                cx.emit(ChatGuardRequest {
                    ids,
                    action: std::rc::Rc::new(action),
                })
            });
        } else {
            action(cx);
        }
    }

    pub fn confirm(request: &ChatGuardRequest, window: &mut Window, cx: &mut App) {
        if window.has_active_prompt() {
            return;
        }
        let ids = request.ids.clone();
        let action = request.action.clone();
        let count = cx
            .global::<AppState>()
            .chats
            .read(cx)
            .unsent_count(&ids, cx);
        let title = if count == 1 {
            "Discard 1 queued message?".to_owned()
        } else {
            format!("Discard {count} queued messages?")
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &title,
            Some("Messages waiting in the chat queue haven't been sent to the agent."),
            &[
                PromptButton::cancel("Cancel"),
                PromptButton::ok("Discard and close"),
            ],
            cx,
        );
        let chats = cx.global::<AppState>().chats.clone();
        window
            .spawn(cx, async move |cx| {
                if answer.await != Ok(1) {
                    return;
                }
                let _ = cx.update(|_, cx| {
                    for id in &ids {
                        if let Some(view) = chats.read(cx).views.get(id).cloned() {
                            view.update(cx, |view, cx| view.discard_queue(cx));
                        }
                    }
                    action(cx);
                });
            })
            .detach();
    }

    pub fn new() -> Self {
        Self {
            views: HashMap::new(),
            turn_diffs: HashMap::new(),
            error: None,
            workspaces: HashMap::new(),
            events: HashMap::new(),
            retiring: vec![],
            stopping: false,
            agents_ready: false,
            last_space: None,
            opening: None,
        }
    }

    /// Chats register hook runs, so they start only once the agent relay is up.
    pub fn agents_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ready = cx.global::<AppState>().agents.read(cx).ready;
        if ready != self.agents_ready {
            self.agents_ready = ready;
            self.sync(window, cx);
        }
    }

    /// Views need a Window (the composer), so the workspace view drives this.
    pub fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.retiring
            .retain(|retiring| !retiring.done.load(Ordering::Acquire));
        let app = cx.global::<AppState>().clone();
        if self.stopping
            || app.settings.read(cx).quitting
            || !app.projects.read(cx).ready
            || !app.tools.read(cx).ready
        {
            return;
        }
        let space = app.projects.read(cx).catalog.active;
        if self.agents_ready && Self::unavailable(cx).is_none() && self.last_space != Some(space) {
            self.last_space = Some(space);
            if space.is_none() && app.workspace.read(cx).tabs().is_empty() {
                self.open_new(ChatTarget::NoProject, cx);
            }
        }
        let chat_panes: HashMap<PaneId, (canopy_desktop::state::workspace::Pane, WorkspaceId)> =
            app.projects
                .read(cx)
                .runtime_workspaces(cx)
                .iter()
                .flat_map(|workspace| {
                    workspace
                        .all_panes()
                        .into_iter()
                        .filter(|pane| pane.metadata.kind == PaneKind::Chat)
                        .map(move |pane| (pane.id, (pane, workspace.id)))
                })
                .collect();
        let owner: HashMap<PaneId, WorkspaceId> = chat_panes
            .iter()
            .map(|(id, (_, workspace))| (*id, *workspace))
            .collect();
        let removed: Vec<PaneId> = self
            .views
            .keys()
            .filter(|id| !owner.contains_key(id))
            .copied()
            .collect();
        let mut changed = !removed.is_empty();
        for id in removed {
            self.events.remove(&id);
            let workspace = self.workspaces.remove(&id);
            if let (Some(view), Some(workspace)) = (self.views.remove(&id), workspace) {
                self.retire(workspace, view, cx);
            }
        }
        let workspace = app.workspace.read(cx);
        let active = workspace.activation_plan();
        let focused = workspace.active().map(|tab| tab.focused);
        let ready =
            self.agents_ready && app.tools.read(cx).environment.is_some() && !app.git.read(cx).busy;
        // A shown turn diff needs its chat's history, even while the chat's own tab
        // isn't shown: that chat is created without starting its agent.
        let diffed: Vec<PaneId> = active
            .iter()
            .filter(|p| p.metadata.kind == PaneKind::TurnDiff)
            .filter_map(|p| {
                serde_json::from_str::<canopy_desktop::chat::tools::TurnDiffTarget>(
                    p.metadata.resource.as_deref()?,
                )
                .ok()
            })
            .map(|target| target.chat)
            .collect();
        let shown: Vec<PaneId> = active
            .iter()
            .filter(|p| p.metadata.kind == PaneKind::Chat)
            .map(|p| p.id)
            .collect();
        for id in shown.iter().chain(&diffed) {
            let start = shown.contains(id);
            if let Some(view) = self.views.get(id) {
                if start && view.read(cx).dormant() && ready {
                    view.update(cx, |view, cx| view.start(cx));
                }
                continue;
            }
            if !ready {
                continue;
            }
            let Some((pane, workspace)) = chat_panes.get(id) else {
                continue;
            };
            let view = cx.new(|cx| ChatView::new(pane.clone(), window, cx));
            self.events.insert(
                *id,
                cx.subscribe(&view, |_, _, _: &ChatStateChanged, cx| cx.notify()),
            );
            if start {
                view.update(cx, |view, cx| view.start(cx));
            }
            self.workspaces.insert(*id, *workspace);
            self.views.insert(*id, view);
            changed = true;
        }
        for (id, view) in &self.views {
            let visible = active.iter().any(|pane| pane.id == *id);
            view.update(cx, |view, cx| {
                view.set_active(visible, visible && focused == Some(*id), cx)
            });
        }
        changed |= self.sync_turn_diffs(&app, &active, cx);
        if changed {
            cx.notify();
        }
    }

    /// Turn diffs read the chat's live transcript; without it they say so.
    fn sync_turn_diffs(
        &mut self,
        app: &AppState,
        active: &[canopy_desktop::state::workspace::Pane],
        cx: &mut Context<Self>,
    ) -> bool {
        let alive: HashSet<PaneId> = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .flat_map(|workspace| workspace.all_panes())
            .filter(|pane| pane.metadata.kind == PaneKind::TurnDiff)
            .map(|pane| pane.id)
            .collect();
        let before = self.turn_diffs.len();
        self.turn_diffs.retain(|id, _| alive.contains(id));
        let mut changed = before != self.turn_diffs.len();
        for pane in active
            .iter()
            .filter(|p| p.metadata.kind == PaneKind::TurnDiff)
        {
            let resource = pane.metadata.resource.clone().unwrap_or_default();
            // An unavailable diff is retried: the chat may not have started yet.
            if self
                .turn_diffs
                .get(&pane.id)
                .is_some_and(|(built, found, _)| *built == resource && *found)
            {
                continue;
            }
            let Ok(target) =
                serde_json::from_str::<canopy_desktop::chat::tools::TurnDiffTarget>(&resource)
            else {
                continue;
            };
            let files = self
                .views
                .get(&target.chat)
                .and_then(|chat| chat.read(cx).turn_changes(target.session, target.turn));
            let found = files.is_some();
            if !found
                && self
                    .turn_diffs
                    .get(&pane.id)
                    .is_some_and(|(built, _, _)| *built == resource)
            {
                continue;
            }
            let cwd = pane.metadata.cwd.clone();
            let view = cx.new(|_| TurnDiffView::new(&target, files, cwd));
            self.turn_diffs.insert(pane.id, (resource, found, view));
            changed = true;
        }
        changed
    }

    fn retire(&mut self, workspace: WorkspaceId, view: Entity<ChatView>, cx: &mut Context<Self>) {
        let close = view.update(cx, |view, cx| view.close(cx));
        let cwd = view.read(cx).cwd();
        // Closing a chat deletes it: its saved conversation and attachments go too.
        if let Some(chat) = view.read(cx).chat_id() {
            cx.global::<AppState>()
                .chat_history
                .clone()
                .update(cx, |history, cx| history.delete(&chat, cx));
        }
        let done = Arc::new(AtomicBool::new(false));
        let finished = done.clone();
        let watch = cx.spawn(async move |this, cx| {
            let mut result = close.await;
            // A chat without a project owns its private folder; it goes with the tab,
            // and only after the agent process is confirmed gone.
            if result.is_ok()
                && let Some(cwd) = cwd
            {
                result = cx
                    .background_executor()
                    .spawn(async move {
                        let data = canopy_desktop::platform::directories::data_dir()?;
                        if canopy_desktop::chat::scratch::owns(&data, &cwd) {
                            canopy_desktop::chat::scratch::remove(&data, &cwd).map(drop)
                        } else {
                            Ok(())
                        }
                    })
                    .await;
            }
            finished.store(true, Ordering::Release);
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = result {
                    this.error = Some(error);
                }
                cx.notify();
            });
        });
        self.retiring.push(Retiring {
            workspace,
            view,
            done,
            _watch: watch,
        });
    }

    /// Opens a new, empty chat tab in `target` with the last choices and shows it.
    pub fn open_new(&mut self, target: ChatTarget, cx: &mut Context<Self>) {
        if Self::unavailable(cx).is_some() || self.opening.is_some() {
            return;
        }
        let app = cx.global::<AppState>().clone();
        let defaults = app.settings.read(cx).values.chat.clone();
        let provider = Self::provider(cx);
        let Some(tool) = app.tools.read(cx).catalog.get(provider.tool()).cloned() else {
            return;
        };
        let profile = defaults
            .profile
            .clone()
            .filter(|id| tool.profiles.iter().any(|p| &p.id == id))
            .or(tool.default_profile.clone());
        let chosen = profile
            .as_ref()
            .and_then(|id| tool.profiles.iter().find(|p| &p.id == id));
        let profile_mode = chosen.and_then(|p| match provider {
            Provider::Claude => Mode::from_claude(&p.settings.permission_mode),
            Provider::Codex => Some(Mode::from_codex(
                &p.settings.approval_mode,
                &p.settings.sandbox,
                p.settings.full_auto,
                p.settings.bypass_approvals,
            )),
        });
        // The last choice in the other agent's terms; its model and effort don't apply.
        let same = defaults
            .profile
            .as_ref()
            .is_some_and(|id| tool.profiles.iter().any(|p| &p.id == id));
        let mode = match (defaults.mode, provider) {
            (Some(mode), Provider::Claude) => Some(mode.for_claude()),
            (Some(mode), Provider::Codex) => Some(mode.for_codex()),
            (None, _) => None,
        };
        // The profile's own model, unless the last choice for this account is newer.
        let profile_model = chosen
            .map(|p| p.model.clone())
            .filter(|model| !model.trim().is_empty());
        let settings = ChatSettings {
            model: defaults.model.filter(|_| same).or(profile_model),
            effort: defaults.effort.filter(|_| same),
            mode: mode.or(profile_mode).unwrap_or(match provider {
                Provider::Claude => Mode::Ask,
                Provider::Codex => Mode::Auto,
            }),
            // New chats start without remembered permissions.
            allowed: vec![],
            id: Some(ChatSettings::new_id()),
        };
        let metadata = move |cwd| PaneMetadata {
            cwd: Some(cwd),
            profile_id: profile,
            title: Some(NEW_CHAT_TITLE.into()),
            kind: PaneKind::Chat,
            chat: Some(Box::new(settings)),
            ..Default::default()
        };
        match target {
            ChatTarget::Project(id) => {
                let Some(cwd) = project_cwd(id, cx) else {
                    self.error = Some("This project is no longer open.".into());
                    cx.notify();
                    return;
                };
                self.place(Some(id), provider, metadata(cwd), cx);
            }
            ChatTarget::NoProject => {
                // The private folder is created off the UI thread, then the tab opens.
                self.opening = Some(cx.spawn(async move |this, cx| {
                    let folder = cx
                        .background_executor()
                        .spawn(async {
                            let data = canopy_desktop::platform::directories::data_dir()?;
                            canopy_desktop::chat::scratch::create(&data)
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.opening = None;
                        match folder {
                            Ok(cwd) => this.place(None, provider, metadata(cwd), cx),
                            Err(error) => {
                                this.error =
                                    Some(format!("Could not create the chat folder: {error}"));
                                cx.notify();
                            }
                        }
                    });
                }));
            }
        }
    }

    fn place(
        &mut self,
        target: Option<WorkspaceId>,
        provider: Provider,
        metadata: PaneMetadata,
        cx: &mut Context<Self>,
    ) {
        let projects = cx.global::<AppState>().projects.clone();
        let result = projects.update(cx, |projects, cx| {
            projects.open_chat(target, NEW_CHAT_TITLE, provider.tool(), metadata, cx)
        });
        self.error = result.err();
        cx.notify();
    }

    /// The agent of a new chat: the last account's (Claude unless it was Codex's).
    pub fn provider(cx: &App) -> Provider {
        let app = cx.global::<AppState>();
        let tools = app.tools.read(cx);
        let last = app.settings.read(cx).values.chat.profile.clone();
        let usable = |provider: Provider| {
            tools
                .catalog
                .get(provider.tool())
                .is_some_and(|tool| tool.enabled)
                && !matches!(tools.availability.get(provider.tool()), Some(Err(_)))
        };
        let owns_last = |provider: &Provider| {
            tools.catalog.get(provider.tool()).is_some_and(|tool| {
                last.as_ref()
                    .is_some_and(|id| tool.profiles.iter().any(|p| &p.id == id))
            })
        };
        // The last account's agent; the other one when that can't run (Claude names
        // the problem when neither can).
        let preferred = Provider::ALL
            .into_iter()
            .rfind(owns_last)
            .unwrap_or(Provider::Claude);
        if usable(preferred) {
            return preferred;
        }
        Provider::ALL
            .into_iter()
            .rfind(|provider| usable(*provider))
            .unwrap_or(Provider::Claude)
    }

    /// Why "New chat" is unavailable, if it is.
    pub fn unavailable(cx: &App) -> Option<&'static str> {
        let app = cx.global::<AppState>();
        let tools = app.tools.read(cx);
        if app.settings.read(cx).quitting {
            return Some("Canopy is quitting.");
        }
        if !tools.ready || tools.environment.is_none() {
            return Some("Loading tools…");
        }
        let provider = Self::provider(cx);
        let tool = tools.catalog.get(provider.tool())?;
        if !tool.enabled {
            return Some(match provider {
                Provider::Claude => "Claude Code is disabled in Preferences.",
                Provider::Codex => "Codex is disabled in Preferences.",
            });
        }
        if matches!(tools.availability.get(provider.tool()), Some(Err(_))) {
            return Some(match provider {
                Provider::Claude => "Claude Code is not installed.",
                Provider::Codex => "Codex is not installed.",
            });
        }
        if app.git.read(cx).busy {
            return Some("Waiting for a Git operation.");
        }
        if !app.projects.read(cx).ready {
            return Some("Restoring workspace…");
        }
        None
    }

    pub fn running_workspaces(&self, cx: &App) -> HashSet<WorkspaceId> {
        let mut running: HashSet<_> = self
            .views
            .iter()
            .filter(|(_, view)| view.read(cx).is_live())
            .filter_map(|(id, _)| self.workspaces.get(id).copied())
            .collect();
        running.extend(
            self.retiring
                .iter()
                .filter(|retiring| !retiring.done.load(Ordering::Acquire))
                .map(|retiring| retiring.workspace),
        );
        running
    }
    pub fn workspace_running(&self, id: WorkspaceId, cx: &App) -> bool {
        self.running_workspaces(cx).contains(&id)
    }

    /// Stops every chat of one workspace; layouts and transcripts stay.
    pub fn stop_workspace(
        &mut self,
        id: WorkspaceId,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let views: Vec<_> = self
            .views
            .iter()
            .filter(|(pane, _)| self.workspaces.get(pane) == Some(&id))
            .map(|(_, view)| view.clone())
            .chain(
                self.retiring
                    .iter()
                    .filter(|retiring| retiring.workspace == id)
                    .map(|retiring| retiring.view.clone()),
            )
            .collect();
        self.close_all(views, false, cx)
    }

    /// Quit: stop every chat process. An error keeps the application open.
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.stopping = true;
        let views: Vec<_> = self
            .views
            .values()
            .cloned()
            .chain(self.retiring.iter().map(|retiring| retiring.view.clone()))
            .collect();
        let close = self.close_all(views, true, cx);
        cx.spawn(async move |this, cx| {
            let result = close.await;
            let _ = this.update(cx, |this, cx| {
                this.stopping = false;
                if result.is_ok() {
                    this.views.clear();
                    this.events.clear();
                    this.workspaces.clear();
                    this.retiring.clear();
                }
                cx.notify();
            });
            result
        })
    }

    /// Joins each view's single in-flight close; the first error is reported.
    fn close_all(
        &mut self,
        views: Vec<Entity<ChatView>>,
        for_good: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let tasks: Vec<_> = views
            .iter()
            .map(|view| {
                view.update(cx, |view, cx| {
                    if for_good {
                        view.close(cx)
                    } else {
                        view.shutdown(cx)
                    }
                })
            })
            .collect();
        cx.spawn(async move |this, cx| {
            let mut result = Ok(());
            for task in tasks {
                let closed = task.await;
                if result.is_ok() {
                    result = closed;
                }
            }
            let _ = this.update(cx, |this, cx| {
                this.error = result.as_ref().err().cloned();
                cx.notify();
            });
            result
        })
    }
}

/// The directory of an open project or worktree.
fn project_cwd(id: WorkspaceId, cx: &App) -> Option<std::path::PathBuf> {
    cx.global::<AppState>()
        .projects
        .read(cx)
        .catalog
        .items
        .iter()
        .find(|project| project.workspace == id)
        .map(|project| {
            project
                .worktree_path
                .clone()
                .unwrap_or_else(|| project.path.clone())
        })
}
