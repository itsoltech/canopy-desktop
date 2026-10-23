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
    sync::Arc,
};
mod cleanup;
/// Read-only projection for commands invoked from inside a chat registry update.
#[derive(Default)]
pub(crate) struct ChatFolders(HashMap<PaneId, std::path::PathBuf>);
impl Global for ChatFolders {}
impl ChatFolders {
    pub(crate) fn contains(&self, path: &std::path::Path) -> bool {
        self.0.values().any(|dir| path.starts_with(dir))
    }
}

/// Where a new chat opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatTarget {
    /// The "No project" space, in a private folder of its own.
    NoProject,
    Project(WorkspaceId),
}

/// A chat's conversation up to an agent reply, to go on in a new chat (DEC-092/093).
#[derive(Clone)]
pub struct ForkRequest {
    /// The chat forked (`ChatSettings.id`).
    pub source: String,
    pub cut: canopy_desktop::chat::fork::Cut,
    pub provider: Provider,
    pub profile: Option<String>,
    /// Its choices (model, effort, mode); the new chat gets its own id.
    pub settings: ChatSettings,
    /// The forked part's agent session, branched off at `cut`.
    pub session: String,
    pub title: String,
}

/// Fork asked under a reply: the workspace asks where the new chat goes.
pub struct ChatForkRequested(pub ForkRequest);
impl EventEmitter<ChatForkRequested> for Chats {}

/// Session tokens of the page service (`preview_html`), reachable without `Chats`.
pub struct PageTokens(pub canopy_desktop::chat::pages::Registry);
impl Global for PageTokens {}

/// Closing panes whose chats would lose something: their conversation (closing deletes
/// a chat), or messages still queued (also on quit).
#[derive(Clone)]
pub struct ChatGuardRequest {
    pub ids: Vec<PaneId>,
    pub action: std::rc::Rc<dyn Fn(&mut App)>,
    /// The chats are closed for good: their conversations are deleted.
    pub deleting: bool,
}

/// What closing some chats would lose.
#[derive(Default)]
struct AtRisk {
    /// Chats with a conversation (or an agent at work).
    chats: usize,
    working: usize,
    unsent: usize,
    commands: usize,
}
impl EventEmitter<ChatGuardRequest> for Chats {}

/// A closed pane whose agent process is still stopping.
struct Retiring {
    workspace: WorkspaceId,
    pane: canopy_desktop::state::workspace::Pane,
    folder: Option<std::path::PathBuf>,
    view: Option<Entity<ChatView>>,
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    _watch: Task<()>,
}

pub struct Chats {
    pub views: HashMap<PaneId, Entity<ChatView>>,
    /// Turn-diff panes, the resource they were built from and whether it had data.
    pub turn_diffs: HashMap<PaneId, (String, bool, Entity<TurnDiffView>)>,
    pub error: Option<String>,
    workspaces: HashMap<PaneId, WorkspaceId>,
    events: HashMap<PaneId, [Subscription; 2]>,
    retiring: Vec<Retiring>,
    known: HashMap<PaneId, (canopy_desktop::state::workspace::Pane, WorkspaceId)>,
    retired_folders: HashSet<std::path::PathBuf>,
    stopping: bool,
    agents_ready: bool,
    opening: Option<Task<()>>,
    /// Renders pages for agents' `preview_html` (DEC-077); `None` where unsupported.
    pages: Option<canopy_desktop::chat::pages::PageService>,
    _previews: Option<Task<()>>,
}

impl Chats {
    pub fn any_unsent(&self, cx: &App) -> bool {
        self.views.values().any(|v| v.read(cx).has_unsent())
    }
    fn at_risk(&self, ids: &[PaneId], cx: &App) -> AtRisk {
        let mut risk = AtRisk::default();
        let panes: HashSet<_> = cx
            .global::<AppState>()
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .flat_map(|w| w.all_panes())
            .filter(|p| p.metadata.kind == PaneKind::Chat)
            .map(|p| p.id)
            .collect();
        for id in ids.iter().copied().collect::<HashSet<_>>() {
            let Some(view) = self.views.get(&id) else {
                risk.chats += usize::from(panes.contains(&id));
                continue;
            };
            let view = view.read(cx);
            if view.has_conversation() || view.is_working() || view.history_uncertain() {
                risk.chats += 1;
            }
            risk.working += usize::from(view.is_working());
            risk.unsent += view.unsent();
            risk.commands += view.running_commands();
        }
        risk
    }

    /// Runs `action` now, or after the user agreed to what it loses. `deleting`: the
    /// chats close for good (tab, pane, project), so their conversations are deleted;
    /// otherwise (quit) only queued messages are at stake.
    pub fn guard(
        ids: Vec<PaneId>,
        action: impl Fn(&mut App) + 'static,
        deleting: bool,
        cx: &mut App,
    ) {
        let chats = cx.global::<AppState>().chats.clone();
        // Register metadata before applying a close, even if tools have not loaded
        // and this restored conversation has never had a ChatView.
        chats.update(cx, |chats, cx| {
            for workspace in cx
                .global::<AppState>()
                .projects
                .read(cx)
                .runtime_workspaces(cx)
            {
                for pane in workspace
                    .all_panes()
                    .into_iter()
                    .filter(|p| p.metadata.kind == PaneKind::Chat)
                {
                    chats.known.entry(pane.id).or_insert((pane, workspace.id));
                }
            }
        });
        if deleting && let Some(error) = chats.read(cx).deletion_blocker(&ids, cx) {
            chats.update(cx, |chats, cx| {
                chats.error = Some(error);
                cx.notify();
            });
            return;
        }
        let risk = chats.read(cx).at_risk(&ids, cx);
        if risk.unsent > 0 || deleting && risk.chats > 0 {
            chats.update(cx, |_, cx| {
                cx.emit(ChatGuardRequest {
                    ids,
                    action: std::rc::Rc::new(action),
                    deleting,
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
        let risk = cx.global::<AppState>().chats.read(cx).at_risk(&ids, cx);
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        let (title, mut detail, button) = if request.deleting && risk.chats > 0 {
            (
                if risk.chats == 1 {
                    cx.global::<AppState>()
                        .projects
                        .read(cx)
                        .runtime_workspaces(cx)
                        .iter()
                        .find_map(|w| ids.iter().find_map(|id| w.chat_title(*id)))
                        .map(|title| format!("Delete “{title}”?"))
                        .unwrap_or_else(|| "Delete this chat?".to_owned())
                } else {
                    format!("Delete {} chats?", risk.chats)
                },
                vec![if risk.chats == 1 {
                    "Closing deletes the conversation and its attachments. This can't be undone."
                        .to_owned()
                } else {
                    "Closing deletes their conversations and attachments. This can't be undone."
                        .to_owned()
                }],
                "Delete chat",
            )
        } else {
            (
                format!(
                    "Discard {}?",
                    plural(risk.unsent, "queued message", "queued messages")
                ),
                vec![],
                "Discard and close",
            )
        };
        if risk.working > 0 {
            detail.push(if risk.working == 1 {
                "The agent is still working and will be stopped.".to_owned()
            } else {
                format!(
                    "{} agents are still working and will be stopped.",
                    risk.working
                )
            });
        }
        if risk.unsent > 0 {
            detail.push(format!(
                "{} not sent to the agent yet.",
                plural(risk.unsent, "queued message is", "queued messages are")
            ));
        }
        if risk.commands > 0 {
            detail.push(format!(
                "{} will be stopped.",
                plural(risk.commands, "running ! command", "running ! commands")
            ));
        }
        let detail = detail.join(" ");
        let answer = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(&detail)
                .filter(|detail| !detail.is_empty())
                .map(|d| d.as_str()),
            &[PromptButton::cancel("Cancel"), PromptButton::ok(button)],
            cx,
        );
        let chats = cx.global::<AppState>().chats.clone();
        let deleting = request.deleting;
        window
            .spawn(cx, async move |cx| {
                if answer.await != Ok(1) {
                    return;
                }
                let _ = cx.update(|_, cx| {
                    if deleting && let Some(error) = chats.read(cx).deletion_blocker(&ids, cx) {
                        chats.update(cx, |chats, cx| {
                            chats.error = Some(error);
                            cx.notify();
                        });
                        return;
                    }
                    for id in ids.iter().filter(|_| !deleting) {
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
            known: HashMap::new(),
            retired_folders: HashSet::new(),
            stopping: false,
            agents_ready: false,
            opening: None,
            pages: None,
            _previews: None,
        }
    }

    /// Starts the page service; a failure only leaves agents without page previews. Its
    /// token registry is a global: chats start and close from inside `Chats` updates.
    pub fn start_pages(&mut self, cx: &mut Context<Self>) {
        match canopy_desktop::chat::pages::PageService::start() {
            Ok((service, requests)) => {
                cx.set_global(PageTokens(service.registry()));
                self.pages = Some(service);
                self._previews = Some(crate::ui::chat::serve_previews(requests, cx));
            }
            Err(error) => eprintln!("Page previews unavailable: {error}"),
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
        self.retiring.retain(|r| {
            !r.result
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(Result::is_ok)
        });
        let app = cx.global::<AppState>().clone();
        if self.stopping || app.settings.read(cx).quitting || !app.projects.read(cx).ready {
            return;
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
                        .map(move |mut pane| {
                            pane.metadata.title = workspace.chat_title(pane.id).map(str::to_owned);
                            (pane.id, (pane, workspace.id))
                        })
                })
                .collect();
        let owner: HashMap<PaneId, WorkspaceId> = chat_panes
            .iter()
            .map(|(id, (_, workspace))| (*id, *workspace))
            .collect();
        let removed: Vec<PaneId> = self
            .known
            .keys()
            .filter(|id| !owner.contains_key(id))
            .copied()
            .collect();
        let mut changed = !removed.is_empty();
        for id in removed {
            self.events.remove(&id);
            self.workspaces.remove(&id);
            if let Some((pane, workspace)) = self.known.remove(&id) {
                let view = self.views.remove(&id);
                self.retire(workspace, pane, view, cx);
            }
        }
        self.known = chat_panes.clone();
        self.workspaces = owner;
        let workspace = app.workspace.read(cx);
        let active = workspace.activation_plan();
        let focused = workspace.active().map(|tab| tab.focused);
        let ready = self.agents_ready
            && app.tools.read(cx).ready
            && app.tools.read(cx).environment.is_some()
            && !app.git.read(cx).busy;
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
                [
                    cx.subscribe(&view, |_, _, _: &ChatStateChanged, cx| cx.notify()),
                    // A fork goes on where the user picks; the workspace asks.
                    cx.subscribe(&view, |_, _, request: &ChatForkRequested, cx| {
                        cx.emit(ChatForkRequested(request.0.clone()))
                    }),
                ],
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
                if let Some((pane, _)) = chat_panes.get(id)
                    && let Some(title) = &pane.metadata.title
                {
                    view.set_title(title, pane.metadata.chat_title_state, cx);
                }
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
            chat_title_state: Some(canopy_desktop::state::workspace::ChatTitleState::Placeholder),
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

    /// Opens a fork of a conversation in `target`: its copy is saved first, then the
    /// tab opens and its agent branches the session off at the reply.
    pub fn fork(&mut self, target: ChatTarget, request: ForkRequest, cx: &mut Context<Self>) {
        if Self::unavailable(cx).is_some() || self.opening.is_some() {
            return;
        }
        let project = match target {
            ChatTarget::Project(id) => match project_cwd(id, cx) {
                Some(cwd) => Some((id, cwd)),
                None => {
                    self.error = Some("This project is no longer open.".into());
                    cx.notify();
                    return;
                }
            },
            ChatTarget::NoProject => None,
        };
        let id = ChatSettings::new_id();
        let mut settings = request.settings.clone();
        settings.id = Some(id.clone());
        // Permissions remembered for a chat stay with it.
        settings.allowed.clear();
        let copy = cx
            .global::<AppState>()
            .chat_history
            .clone()
            .update(cx, |history, cx| {
                history.fork(&request.source, &id, request.cut.clone(), cx)
            });
        self.opening = Some(cx.spawn(async move |this, cx| {
            let copied = copy.await;
            let cwd = match &project {
                Some((_, cwd)) => Ok(cwd.clone()),
                None => {
                    cx.background_executor()
                        .spawn(async {
                            let data = canopy_desktop::platform::directories::data_dir()?;
                            canopy_desktop::chat::scratch::create(&data)
                        })
                        .await
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.opening = None;
                let cwd = match (copied, cwd) {
                    (Ok(()), Ok(cwd)) => cwd,
                    (Err(error), _) => {
                        this.error = Some(format!("Could not fork the chat: {error}"));
                        cx.notify();
                        return;
                    }
                    (_, Err(error)) => {
                        this.error = Some(format!("Could not create the chat folder: {error}"));
                        cx.notify();
                        return;
                    }
                };
                let title = format!("{} (fork)", request.title);
                let metadata = PaneMetadata {
                    cwd: Some(cwd),
                    profile_id: request.profile.clone(),
                    // The session to branch off; the new one replaces it once it has an id.
                    resume_id: Some(request.session.clone()),
                    title: Some(title.clone()),
                    kind: PaneKind::Chat,
                    chat: Some(Box::new(settings)),
                    ..Default::default()
                };
                this.place_titled(
                    project.map(|(id, _)| id),
                    request.provider,
                    metadata,
                    &title,
                    cx,
                );
            });
        }));
    }

    fn place(
        &mut self,
        target: Option<WorkspaceId>,
        provider: Provider,
        metadata: PaneMetadata,
        cx: &mut Context<Self>,
    ) {
        self.place_titled(target, provider, metadata, NEW_CHAT_TITLE, cx);
    }

    fn place_titled(
        &mut self,
        target: Option<WorkspaceId>,
        provider: Provider,
        metadata: PaneMetadata,
        title: &str,
        cx: &mut Context<Self>,
    ) {
        let projects = cx.global::<AppState>().projects.clone();
        let failed = metadata.clone();
        let result = projects.update(cx, |projects, cx| {
            projects.open_chat(target, title, provider.tool(), metadata, cx)
        });
        if result.is_err() && target.is_none() {
            let workspace = projects.read(cx).scratch_id(cx);
            self.retire(
                workspace,
                canopy_desktop::state::workspace::Pane {
                    id: PaneId::new(),
                    tool: provider.tool().to_owned(),
                    metadata: failed,
                },
                None,
                cx,
            );
        }
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
        if app.projects.read(cx).busy {
            return Some("Waiting for a project operation.");
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
                .filter(|r| {
                    !r.result
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_ref()
                        .is_some_and(Result::is_ok)
                })
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
                    .filter_map(|retiring| retiring.view.clone()),
            )
            .collect();
        let close = self.close_all(views, false, cx);
        let cleanup = self.pending_cleanup(Some(id), cx);
        cx.spawn(async move |_, _| {
            let mut result = close.await;
            for job in cleanup {
                let closed = job.await;
                if result.is_ok() {
                    result = closed;
                }
            }
            result
        })
    }

    /// Quit: stop every chat process. An error keeps the application open.
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.stopping = true;
        for (id, _) in self.cleanup_errors() {
            self.retry_cleanup(id, cx);
        }
        let views: Vec<_> = self
            .views
            .values()
            .cloned()
            .chain(
                self.retiring
                    .iter()
                    .filter_map(|retiring| retiring.view.clone()),
            )
            .collect();
        let close = self.close_all(views, true, cx);
        let cleanup = self.pending_cleanup(None, cx);
        cx.spawn(async move |this, cx| {
            let mut result = close.await;
            for job in cleanup {
                let closed = job.await;
                if result.is_ok() {
                    result = closed;
                }
            }
            let _ = this.update(cx, |this, cx| {
                this.stopping = false;
                if result.is_ok() {
                    this.views.clear();
                    this.events.clear();
                    this.workspaces.clear();
                    this.known.clear();
                    this.retired_folders.clear();
                    this.retiring.clear();
                    if cx.has_global::<ChatFolders>() {
                        let folders = cx.global_mut::<ChatFolders>();
                        folders.0.clear();
                    }
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
