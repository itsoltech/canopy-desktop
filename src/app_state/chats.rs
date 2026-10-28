//! Registry of chats: one runtime per chat pane, alive without any view (hidden tab,
//! turn diff, remote command), and the views that present them in this window. Agent
//! processes never enter SQLite; the pane keeps only its account, choices and provider
//! session id.
use super::AppState;
use crate::ui::chat::{ChatView, NEW_CHAT_TITLE, TurnDiffView};
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
// The remote bridge's typed chat operations; the gRPC server is their caller.
#[cfg_attr(not(test), allow(dead_code))]
mod commands;
mod runtime;
#[allow(unused_imports, reason = "the remote bridge's typed chat API")]
pub use commands::{ChatSummary, DeletePreflight, ForkChat, ForkReplacement, NewChat};
pub(crate) use runtime::Recovering;
pub use runtime::backend_of;
pub use runtime::{
    AgentOption, Archived, ChatNotice, ChatNoticeKind, ChatRuntime, ChatRuntimeEvent,
    ChatStateChanged, LineState, QueueLine,
};
#[allow(unused_imports, reason = "the remote bridge's typed chat API")]
pub use runtime::{
    ChatCommand, ChatSnapshot, DecisionAnswer, EditPreview, FilesPreview, PendingDecision,
    RemoteKey, SnapshotEpoch,
};
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

/// `/resume` typed in a chat: the workspace lists closed chats. `replace`: the empty
/// chat it was typed in, which the chosen one takes the place of.
#[derive(Clone, Copy)]
pub struct ChatResumeRequested {
    pub replace: Option<PaneId>,
}
impl EventEmitter<ChatResumeRequested> for Chats {}

/// "Restore chat…" on a conversation kept without its agent and account.
pub struct ChatRecoveryRequested(pub String);
impl EventEmitter<ChatRecoveryRequested> for Chats {}

/// Session tokens of the page service (`preview_html`), reachable without `Chats`.
pub struct PageTokens(pub canopy_desktop::chat::pages::Registry);
impl Global for PageTokens {}

/// Closing panes whose chats would lose something: their conversation (closing deletes
/// a chat), or messages still queued (also on quit).
#[derive(Clone)]
pub struct ChatGuardRequest {
    pub ids: Vec<PaneId>,
    /// Catalog chats without a pane that the action also deletes (a project's chats
    /// created remotely, LIFE-10).
    pub chats: Vec<String>,
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
    /// Remote clients watching these chats.
    observers: usize,
}
impl EventEmitter<ChatGuardRequest> for Chats {}

/// A closed pane whose agent process is still stopping.
struct Retiring {
    workspace: WorkspaceId,
    pane: canopy_desktop::state::workspace::Pane,
    folder: Option<std::path::PathBuf>,
    runtime: Option<Entity<ChatRuntime>>,
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    _watch: Task<()>,
}

/// A host catalog chat that no pane of this window's layout shows: created remotely,
/// or left without a tab. It runs, is guarded and cleaned up without a view (OWN-07).
#[derive(Clone, Debug, PartialEq)]
pub struct DetachedChat {
    pub chat: String,
    /// This window's presentation key for it (never sent to another device).
    pub id: PaneId,
    pub workspace: WorkspaceId,
    pub tool: String,
    pub title: String,
}

pub struct Chats {
    /// Projections are not host-owned chats: no local history, process or delete.
    client_runtimes: HashMap<String, Entity<ChatRuntime>>,
    client_panes: HashMap<PaneId, String>,
    client_events: HashMap<String, Subscription>,
    detaching: HashSet<PaneId>,
    /// Every chat runtime by the chat's UUID (`ChatSettings.id`): the only owner. Panes
    /// and remote observers are presentations of it; it outlives them (OWN-01).
    runtimes: HashMap<String, Entity<ChatRuntime>>,
    /// Local presentation: the chat each pane (or detached chat's key) shows.
    panes: HashMap<PaneId, String>,
    /// Catalog chats without a pane in the layout, by chat UUID.
    detached: HashMap<String, DetachedChat>,
    /// Workspaces open at the last sync: a detached chat goes with its project only
    /// when that project closes while Canopy runs (never at startup).
    open_workspaces: HashSet<WorkspaceId>,
    /// Presentations of shown chats; a hidden tab keeps its view.
    pub views: HashMap<PaneId, Entity<ChatView>>,
    /// Turn-diff panes, the resource they were built from and whether it had data.
    pub turn_diffs: HashMap<PaneId, (String, bool, Entity<TurnDiffView>)>,
    pub error: Option<String>,
    /// The workspace each chat belongs to (its pane's, else its catalog record's).
    workspaces: HashMap<String, WorkspaceId>,
    events: HashMap<String, Subscription>,
    view_events: HashMap<PaneId, Vec<Subscription>>,
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
    /// The runtime pane `id` (or a detached chat's key) presents, when it has one.
    pub fn runtime(&self, id: PaneId) -> Option<&Entity<ChatRuntime>> {
        if let Some(key) = self.client_panes.get(&id) {
            return self.client_runtimes.get(key);
        }
        self.runtimes.get(self.panes.get(&id)?)
    }
    /// The runtime of chat `chat` (its UUID), with or without a pane.
    #[cfg_attr(not(test), allow(dead_code, reason = "called by the remote bridge"))]
    pub fn runtime_for_chat(&self, chat: &str) -> Option<&Entity<ChatRuntime>> {
        self.runtimes.get(chat)
    }
    /// Panes of this window that show a chat with a runtime (the quit guard's chats).
    pub fn chat_ids(&self) -> impl Iterator<Item = &PaneId> {
        self.known.keys().filter(|pane| {
            self.panes
                .get(pane)
                .is_some_and(|c| self.runtimes.contains_key(c))
        })
    }
    /// Catalog chats shown by no pane, by their workspace.
    pub fn detached(&self) -> impl Iterator<Item = &DetachedChat> {
        self.detached.values()
    }
    /// The workspace chat `chat` belongs to: its pane's, else its catalog record's.
    pub fn workspace_of_chat(&self, chat: &str) -> Option<WorkspaceId> {
        self.workspaces
            .get(chat)
            .copied()
            .or_else(|| self.detached.get(chat).map(|d| d.workspace))
            .or_else(|| {
                self.known.values().find_map(|(pane, workspace)| {
                    (pane.metadata.chat.as_ref()?.id.as_deref() == Some(chat)).then_some(*workspace)
                })
            })
    }
    /// Detached chats of `workspaces` (what closing those projects also deletes).
    pub fn detached_in(&self, workspaces: &[WorkspaceId]) -> Vec<String> {
        let mut chats: Vec<_> = self
            .detached
            .values()
            .filter(|d| workspaces.contains(&d.workspace))
            .map(|d| d.chat.clone())
            .collect();
        chats.sort();
        chats
    }

    fn at_risk(&self, ids: &[PaneId], chats: &[String], cx: &App) -> AtRisk {
        let mut risk = AtRisk::default();
        let panes: HashMap<PaneId, Option<String>> = cx
            .global::<AppState>()
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .flat_map(|w| w.all_panes())
            .filter(|p| p.metadata.kind == PaneKind::Chat)
            .map(|p| (p.id, p.metadata.chat.as_ref().and_then(|c| c.id.clone())))
            .collect();
        let mut seen = HashSet::new();
        let targets = ids
            .iter()
            .filter_map(|id| {
                let chat = self
                    .panes
                    .get(id)
                    .cloned()
                    .or_else(|| panes.get(id)?.clone());
                match chat {
                    Some(chat) => Some(Some(chat)),
                    // A chat pane from before saved history: a conversation to lose.
                    None => panes.contains_key(id).then_some(None),
                }
            })
            .chain(chats.iter().map(|chat| Some(chat.clone())));
        for chat in targets {
            if let Some(chat) = &chat
                && !seen.insert(chat.clone())
            {
                continue;
            }
            let Some(runtime) = chat.as_ref().and_then(|chat| self.runtimes.get(chat)) else {
                risk.chats += 1;
                continue;
            };
            let runtime = runtime.read(cx);
            if runtime.has_conversation() || runtime.is_working() || runtime.history_uncertain() {
                risk.chats += 1;
            }
            risk.working += usize::from(runtime.is_working());
            risk.unsent += runtime.unsent();
            risk.commands += runtime.running_commands();
            risk.observers += runtime.observers();
        }
        risk
    }

    /// Runs `action` now, or after the user agreed to what it loses. `deleting`: the
    /// chats close for good (tab, pane, project), so their conversations are deleted;
    /// otherwise (quit) nothing is at stake: queued messages stay with the chat.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn guard(
        ids: Vec<PaneId>,
        action: impl Fn(&mut App) + 'static,
        deleting: bool,
        cx: &mut App,
    ) {
        Self::guard_chats(ids, vec![], action, deleting, cx)
    }

    /// `guard` that also covers catalog chats without a pane (`chats`).
    pub fn guard_chats(
        ids: Vec<PaneId>,
        chats: Vec<String>,
        action: impl Fn(&mut App) + 'static,
        deleting: bool,
        cx: &mut App,
    ) {
        let registry = cx.global::<AppState>().chats.clone();
        // Register metadata before applying a close, even if tools have not loaded
        // and this restored conversation has never had a ChatView.
        registry.update(cx, |registry, cx| {
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
                    registry
                        .known
                        .entry(pane.id)
                        .or_insert((pane, workspace.id));
                }
            }
        });
        if deleting && let Some(error) = registry.read(cx).deletion_blocker(&ids, cx) {
            registry.update(cx, |registry, cx| {
                registry.error = Some(error);
                cx.notify();
            });
            return;
        }
        // Queued messages are durable host state: quitting keeps them; only deleting
        // chats asks first.
        let risk = registry.read(cx).at_risk(&ids, &chats, cx);
        if deleting && (risk.chats > 0 || risk.unsent > 0) {
            registry.update(cx, |_, cx| {
                cx.emit(ChatGuardRequest {
                    ids,
                    chats,
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
        let chats = request.chats.clone();
        let action = request.action.clone();
        let risk = cx
            .global::<AppState>()
            .chats
            .read(cx)
            .at_risk(&ids, &chats, cx);
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        let title_of = |cx: &App| {
            let app = cx.global::<AppState>();
            app.projects
                .read(cx)
                .runtime_workspaces(cx)
                .iter()
                .find_map(|w| {
                    ids.iter()
                        .find_map(|id| w.chat_title(*id).map(str::to_owned))
                })
                .or_else(|| {
                    let registry = app.chats.read(cx);
                    chats
                        .iter()
                        .find_map(|chat| registry.detached.get(chat).map(|d| d.title.clone()))
                })
        };
        let (title, mut detail, button) = if risk.chats > 0 {
            (
                if risk.chats == 1 {
                    title_of(cx)
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
            let count = ids.len() + chats.len();
            (
                if count == 1 {
                    "Delete this chat?".to_owned()
                } else {
                    format!("Delete {count} chats?")
                },
                vec![],
                "Delete chat",
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
                "{} not delivered to the agent; they won't be sent.",
                plural(risk.unsent, "queued message is", "queued messages are")
            ));
        }
        if risk.commands > 0 {
            detail.push(format!(
                "{} will be stopped.",
                plural(risk.commands, "running ! command", "running ! commands")
            ));
        }
        if risk.observers > 0 {
            detail.push(format!(
                "{} will see it deleted.",
                plural(risk.observers, "connected device", "connected devices")
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
        let registry = cx.global::<AppState>().chats.clone();
        let deleting = request.deleting;
        window
            .spawn(cx, async move |cx| {
                if answer.await != Ok(1) {
                    return;
                }
                let _ = cx.update(|_, cx| {
                    if deleting && let Some(error) = registry.read(cx).deletion_blocker(&ids, cx) {
                        registry.update(cx, |registry, cx| {
                            registry.error = Some(error);
                            cx.notify();
                        });
                        return;
                    }
                    action(cx);
                });
            })
            .detach();
    }

    pub fn new() -> Self {
        Self {
            client_runtimes: HashMap::new(),
            client_panes: HashMap::new(),
            client_events: HashMap::new(),
            detaching: HashSet::new(),
            runtimes: HashMap::new(),
            panes: HashMap::new(),
            detached: HashMap::new(),
            open_workspaces: HashSet::new(),
            views: HashMap::new(),
            view_events: HashMap::new(),
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

    /// Chat panes of the layout with their workspace (titles as the sidebar shows).
    fn chat_panes(
        cx: &App,
    ) -> HashMap<PaneId, (canopy_desktop::state::workspace::Pane, WorkspaceId)> {
        cx.global::<AppState>()
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .flat_map(|workspace| {
                workspace
                    .all_panes()
                    .into_iter()
                    .filter(|pane| {
                        pane.metadata.kind == PaneKind::Chat && pane.metadata.remote.is_none()
                    })
                    .map(move |mut pane| {
                        pane.metadata.title = workspace.chat_title(pane.id).map(str::to_owned);
                        (pane.id, (pane, workspace.id))
                    })
            })
            .collect()
    }

    /// Follows the layout without a window: panes gone from it delete their chats
    /// (explicit Delete semantics); detached catalog chats are listed and kept, and
    /// go only with a project closed meanwhile. Remote-only runtimes never depend on
    /// a pane.
    pub fn sync_model(&mut self, cx: &mut Context<Self>) -> bool {
        self.retiring.retain(|r| {
            !r.result
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(Result::is_ok)
        });
        let app = cx.global::<AppState>().clone();
        if self.stopping || app.settings.read(cx).quitting || !app.projects.read(cx).ready {
            return false;
        }
        let chat_panes = Self::chat_panes(cx);
        let removed: Vec<PaneId> = self
            .known
            .keys()
            .filter(|id| !chat_panes.contains_key(id))
            .copied()
            .collect();
        let mut changed = !removed.is_empty();
        for id in removed {
            self.view_events.remove(&id);
            self.views.remove(&id);
            let chat = self.panes.remove(&id);
            if let Some((pane, workspace)) = self.known.remove(&id) {
                if self.detaching.remove(&id) {
                    if let Some(chat) = chat {
                        self.workspaces.insert(chat.clone(), workspace);
                        self.detached.insert(
                            chat.clone(),
                            DetachedChat {
                                chat,
                                id,
                                workspace,
                                tool: pane.tool.clone(),
                                title: pane
                                    .metadata
                                    .title
                                    .clone()
                                    .unwrap_or_else(|| NEW_CHAT_TITLE.into()),
                            },
                        );
                    }
                    continue;
                }
                let runtime = chat.as_ref().and_then(|chat| self.drop_runtime(chat));
                self.retire(workspace, pane, runtime, cx);
            }
        }
        // Panes showing a chat that already runs (opened from the sidebar, or created
        // remotely and shown here now) present that runtime: never a second one.
        for (id, (pane, workspace)) in &chat_panes {
            let chat = pane.metadata.chat.as_ref().and_then(|c| c.id.clone());
            if let Some(chat) = chat {
                if let Some(detached) = self.detached.remove(&chat) {
                    self.panes.remove(&detached.id);
                    changed = true;
                }
                if let Some(runtime) = self.runtimes.get(&chat)
                    && self.panes.get(id) != Some(&chat)
                {
                    let pane = pane.clone();
                    runtime.update(cx, |runtime, cx| runtime.attach_pane(pane, cx));
                }
                if self.runtimes.contains_key(&chat) {
                    self.panes.insert(*id, chat.clone());
                    self.workspaces.insert(chat, *workspace);
                }
            }
        }
        self.known = chat_panes.clone();
        changed |= self.sync_detached(&chat_panes, cx);
        changed
    }
    /// Closing a tab removes a presentation. Explicit Delete does not use this.
    pub fn detach_views(&mut self, panes: &[PaneId]) {
        self.detaching.extend(panes.iter().copied());
    }

    /// Lists catalog chats without a pane; deletes those whose project closed since
    /// the last sync.
    fn sync_detached(
        &mut self,
        chat_panes: &HashMap<PaneId, (canopy_desktop::state::workspace::Pane, WorkspaceId)>,
        cx: &mut Context<Self>,
    ) -> bool {
        let app = cx.global::<AppState>().clone();
        let projects = app.projects.read(cx);
        let scratch = projects.scratch_id(cx);
        let open: HashSet<WorkspaceId> = projects
            .runtime_workspaces(cx)
            .iter()
            .map(|w| w.id)
            .collect();
        let shown: HashSet<String> = chat_panes
            .iter()
            .filter_map(|(id, (pane, _))| {
                pane.metadata
                    .chat
                    .as_ref()
                    .and_then(|c| c.id.clone())
                    .or_else(|| self.panes.get(id).cloned())
            })
            .collect();
        let settings = app.settings.read(cx);
        let mut changed = false;
        let mut closed = vec![];
        let mut found = HashMap::new();
        for record in settings.chat_records() {
            if record.recovery_reason().is_some() || shown.contains(record.id()) {
                continue;
            }
            let workspace = record
                .workspace()
                .and_then(|root| serde_json::from_str::<WorkspaceId>(root).ok())
                .unwrap_or(scratch);
            if !open.contains(&workspace) && self.open_workspaces.contains(&workspace) {
                // Its project closed while Canopy ran: the chat goes with it.
                let pane = canopy_desktop::state::workspace::Pane {
                    id: PaneId::new(),
                    tool: record.agent().to_owned(),
                    metadata: record.metadata().clone(),
                };
                closed.push((record.id().to_owned(), pane, workspace));
                continue;
            }
            found.insert(record.id().to_owned(), (record.clone(), workspace));
        }
        // Runtimes created here before their record was read back.
        for (chat, runtime) in &self.runtimes {
            if shown.contains(chat) || found.contains_key(chat) {
                continue;
            }
            let workspace = self.workspaces.get(chat).copied().unwrap_or(scratch);
            if self.open_workspaces.contains(&workspace) && !open.contains(&workspace) {
                closed.push((chat.clone(), runtime.read(cx).pane_ref().clone(), workspace));
                continue;
            }
            if let Some(record) = settings.chat_record(chat) {
                found.insert(chat.clone(), (record.clone(), workspace));
            }
        }
        let before = self.detached.len();
        self.detached.retain(|chat, _| found.contains_key(chat));
        changed |= before != self.detached.len();
        self.panes
            .retain(|pane, chat| chat_panes.contains_key(pane) || found.contains_key(chat));
        for (chat, (record, workspace)) in found {
            let runtime = self.runtimes.get(&chat);
            let title = runtime
                .and_then(|r| r.read(cx).pane_ref().metadata.title.clone())
                .or_else(|| record.metadata().title.clone())
                .unwrap_or_else(|| NEW_CHAT_TITLE.to_owned());
            let id = self
                .detached
                .get(&chat)
                .map(|d| d.id)
                .or_else(|| runtime.map(|r| r.read(cx).pane_ref().id))
                .unwrap_or_default();
            let entry = DetachedChat {
                chat: chat.clone(),
                id,
                workspace,
                tool: record.agent().to_owned(),
                title,
            };
            if self.detached.get(&chat) != Some(&entry) {
                self.detached.insert(chat.clone(), entry);
                changed = true;
            }
            self.panes.insert(id, chat.clone());
            self.workspaces.insert(chat, workspace);
        }
        for (chat, mut pane, workspace) in closed {
            if let Some(detached) = self.detached.remove(&chat) {
                pane.id = detached.id;
            }
            self.panes.remove(&pane.id);
            let runtime = self.drop_runtime(&chat);
            self.retire(workspace, pane, runtime, cx);
            changed = true;
        }
        self.open_workspaces = open;
        changed
    }

    /// Takes chat `chat`'s runtime out of the registry (it is being deleted).
    fn drop_runtime(&mut self, chat: &str) -> Option<Entity<ChatRuntime>> {
        self.events.remove(chat);
        self.workspaces.remove(chat);
        self.runtimes.remove(chat)
    }

    /// Views need a Window (the composer), so the workspace view drives this.
    pub fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        let mut changed = self.sync_model(cx);
        if self.stopping || app.settings.read(cx).quitting || !app.projects.read(cx).ready {
            return;
        }
        let chat_panes = self.known.clone();
        let workspace = app.workspace.read(cx);
        let active = workspace.activation_plan();
        let focused = workspace.active().map(|tab| tab.focused);
        self.sync_clients(&active, focused, window, cx);
        let ready = self.agents_ready
            && app.tools.read(cx).ready
            && app.tools.read(cx).environment.is_some()
            && !app.git.read(cx).busy;
        // A shown turn diff needs its chat's history, even while the chat's own tab
        // isn't shown: that chat's runtime exists without starting its agent.
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
            let runtime = match self.runtime(*id) {
                Some(runtime) => runtime.clone(),
                None => {
                    let Some((pane, workspace)) = chat_panes.get(id).filter(|_| ready) else {
                        continue;
                    };
                    changed = true;
                    let runtime = self.adopt(pane.clone(), cx);
                    if let Some(chat) = self.panes.get(id).cloned() {
                        self.workspaces.insert(chat, *workspace);
                    }
                    runtime
                }
            };
            if start && ready && runtime.read(cx).dormant() {
                runtime.update(cx, |runtime, cx| runtime.start(cx));
            }
            if start && !self.views.contains_key(id) {
                let view = cx.new(|cx| ChatView::with_runtime(runtime, window, cx));
                self.view_events.insert(
                    *id,
                    vec![
                        // A fork goes on where the user picks; the workspace asks.
                        cx.subscribe(&view, |_, _, request: &ChatForkRequested, cx| {
                            cx.emit(ChatForkRequested(request.0.clone()))
                        }),
                        cx.subscribe(&view, |_, _, request: &ChatResumeRequested, cx| {
                            cx.emit(*request)
                        }),
                    ],
                );
                self.views.insert(*id, view);
                changed = true;
            }
        }
        for (pane, chat) in &self.panes {
            let Some(runtime) = self.runtimes.get(chat) else {
                continue;
            };
            if let Some((pane, _)) = chat_panes.get(pane)
                && let Some(title) = &pane.metadata.title
            {
                runtime.update(cx, |runtime, cx| {
                    runtime.set_title(title, pane.metadata.chat_title_state, cx)
                });
            }
        }
        for (chat, runtime) in &self.runtimes {
            // A view, even of a hidden tab, keeps the process (LIFE-09).
            let presented = self
                .views
                .keys()
                .any(|pane| self.panes.get(pane) == Some(chat));
            runtime.update(cx, |runtime, cx| runtime.set_presented(presented, cx));
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

    fn sync_clients(
        &mut self,
        active: &[canopy_desktop::state::workspace::Pane],
        focused: Option<PaneId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panes: Vec<_> = cx
            .global::<AppState>()
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .flat_map(|w| w.all_panes())
            .filter(|p| p.metadata.remote.is_some())
            .collect();
        let keep: HashSet<_> = panes.iter().map(|p| p.id).collect();
        let gone: Vec<_> = self
            .client_panes
            .keys()
            .filter(|p| !keep.contains(p))
            .copied()
            .collect();
        for id in gone {
            self.client_panes.remove(&id);
            self.views.remove(&id);
            self.view_events.remove(&id);
        }
        self.client_runtimes
            .retain(|key, _| self.client_panes.values().any(|value| value == key));
        self.client_events
            .retain(|key, _| self.client_runtimes.contains_key(key));
        for pane in panes {
            let key = pane.metadata.remote.as_ref().unwrap().key();
            self.client_panes.insert(pane.id, key.clone());
            let visible = active.iter().any(|p| p.id == pane.id);
            if !visible && !self.views.contains_key(&pane.id) {
                continue;
            }
            let runtime = self
                .client_runtimes
                .entry(key.clone())
                .or_insert_with(|| cx.new(|cx| ChatRuntime::new(pane.clone(), cx)))
                .clone();
            self.client_events.entry(key).or_insert_with(|| {
                cx.subscribe(&runtime, |_, _, _: &ChatStateChanged, cx| cx.notify())
            });
            let view = self
                .views
                .entry(pane.id)
                .or_insert_with(|| cx.new(|cx| ChatView::with_runtime(runtime, window, cx)))
                .clone();
            self.view_events.entry(pane.id).or_insert_with(|| {
                vec![
                    cx.subscribe(
                        &view,
                        |_, _, event: &crate::ui::chat::ClientForkRequested, cx| {
                            cx.emit(event.clone())
                        },
                    ),
                    cx.subscribe(
                        &view,
                        |_, _, event: &crate::ui::chat::ClientFileRequested, cx| {
                            cx.emit(event.clone())
                        },
                    ),
                ]
            });
            view.update(cx, |v, cx| {
                v.set_active(visible, visible && focused == Some(pane.id), cx)
            });
        }
    }

    /// The runtime of chat pane `id`, created (with its saved history, without its
    /// agent) when the pane exists and the registry runs. Needs no window.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn ensure_runtime(
        &mut self,
        id: PaneId,
        cx: &mut Context<Self>,
    ) -> Option<Entity<ChatRuntime>> {
        if let Some(runtime) = self.runtime(id) {
            return Some(runtime.clone());
        }
        let app = cx.global::<AppState>().clone();
        if self.stopping || app.settings.read(cx).quitting || !app.projects.read(cx).ready {
            return None;
        }
        let (pane, workspace) = Self::chat_panes(cx).remove(&id)?;
        self.known.entry(id).or_insert((pane.clone(), workspace));
        let runtime = self.adopt(pane, cx);
        if let Some(chat) = self.panes.get(&id).cloned() {
            self.workspaces.insert(chat, workspace);
        }
        Some(runtime)
    }

    /// The runtime of chat `chat` (its UUID), created without any pane, view or window
    /// when the host catalog has the chat: a remote Start/Send runs it here without a
    /// local tab. The same chat never gets a second runtime.
    #[cfg_attr(not(test), allow(dead_code, reason = "called by the remote bridge"))]
    pub fn ensure_runtime_for_chat(
        &mut self,
        chat: &str,
        cx: &mut Context<Self>,
    ) -> Result<Entity<ChatRuntime>, String> {
        if let Some(runtime) = self.runtimes.get(chat) {
            return Ok(runtime.clone());
        }
        let app = cx.global::<AppState>().clone();
        if self.stopping || app.settings.read(cx).quitting {
            return Err("Canopy is quitting.".into());
        }
        if !app.projects.read(cx).ready {
            return Err("The workspace is still being restored.".into());
        }
        // A pane shows it: that pane's runtime (its layout metadata is current).
        if let Some((id, _)) = Self::chat_panes(cx).into_iter().find(|(_, (pane, _))| {
            pane.metadata.chat.as_ref().and_then(|c| c.id.as_deref()) == Some(chat)
        }) {
            return self
                .ensure_runtime(id, cx)
                .ok_or_else(|| "The chat is unavailable.".to_owned());
        }
        let record = app
            .settings
            .read(cx)
            .chat_record(chat)
            .cloned()
            .ok_or_else(|| "This chat is not in the host catalog.".to_owned())?;
        if let Some(reason) = record.recovery_reason() {
            return Err(format!(
                "{reason}. Choose this chat's agent and account first."
            ));
        }
        if self
            .retiring
            .iter()
            .any(|r| r.pane.metadata.chat.as_ref().and_then(|c| c.id.as_deref()) == Some(chat))
        {
            return Err("This chat is being deleted.".into());
        }
        let scratch = app.projects.read(cx).scratch_id(cx);
        let workspace = record
            .workspace()
            .and_then(|root| serde_json::from_str::<WorkspaceId>(root).ok())
            .unwrap_or(scratch);
        let id = self.detached.get(chat).map(|d| d.id).unwrap_or_default();
        let pane = canopy_desktop::state::workspace::Pane {
            id,
            tool: record.agent().to_owned(),
            metadata: record.metadata().clone(),
        };
        let root = record.workspace().map(str::to_owned);
        let runtime = cx.new(|cx| {
            let mut runtime = ChatRuntime::new(pane, cx);
            runtime.set_catalog_root(root);
            runtime
        });
        self.register(chat.to_owned(), runtime.clone(), cx);
        self.panes.insert(id, chat.to_owned());
        self.workspaces.insert(chat.to_owned(), workspace);
        self.detached
            .entry(chat.to_owned())
            .or_insert(DetachedChat {
                chat: chat.to_owned(),
                id,
                workspace,
                tool: record.agent().to_owned(),
                title: record
                    .metadata()
                    .title
                    .clone()
                    .unwrap_or_else(|| NEW_CHAT_TITLE.to_owned()),
            });
        cx.notify();
        Ok(runtime)
    }

    fn register(&mut self, chat: String, runtime: Entity<ChatRuntime>, cx: &mut Context<Self>) {
        self.events.insert(
            chat.clone(),
            cx.subscribe(&runtime, |_, _, _: &ChatStateChanged, cx| cx.notify()),
        );
        self.runtimes.insert(chat, runtime);
    }

    /// The runtime for `pane`, created for it when its chat has none.
    pub(crate) fn adopt(
        &mut self,
        pane: canopy_desktop::state::workspace::Pane,
        cx: &mut Context<Self>,
    ) -> Entity<ChatRuntime> {
        let id = pane.id;
        if let Some(runtime) = self.runtime(id) {
            return runtime.clone();
        }
        let chat = pane.metadata.chat.as_ref().and_then(|c| c.id.clone());
        if let Some(runtime) = chat
            .as_ref()
            .and_then(|chat| self.runtimes.get(chat))
            .cloned()
        {
            runtime.update(cx, |runtime, cx| runtime.attach_pane(pane, cx));
            self.panes.insert(id, chat.expect("checked"));
            return runtime;
        }
        let runtime = cx.new(|cx| ChatRuntime::new(pane, cx));
        // A chat from before saved history gets its id from the runtime.
        let chat = runtime.read(cx).chat_id().expect("a runtime has a chat id");
        self.detached.remove(&chat);
        self.register(chat.clone(), runtime.clone(), cx);
        self.panes.insert(id, chat);
        cx.notify();
        runtime
    }

    /// Shows a catalog chat no pane shows in a tab of its project (or Private chats):
    /// the tab presents the runtime it already has, with its current launch state.
    pub fn open_detached(&mut self, chat: &str, cx: &mut Context<Self>) {
        if Self::unavailable(cx).is_some() || self.opening.is_some() {
            return;
        }
        let Some(detached) = self.detached.get(chat).cloned() else {
            return;
        };
        let app = cx.global::<AppState>().clone();
        let (tool, mut metadata) = match self.runtimes.get(chat) {
            Some(runtime) => {
                let runtime = runtime.read(cx);
                let mut metadata = runtime.pane_ref().metadata.clone();
                metadata.chat = Some(Box::new(runtime.settings().clone()));
                (runtime.pane_ref().tool.clone(), metadata)
            }
            None => match app.settings.read(cx).chat_record(chat) {
                Some(record) => (record.agent().to_owned(), record.metadata().clone()),
                None => return,
            },
        };
        metadata.kind = PaneKind::Chat;
        metadata.title = Some(detached.title.clone());
        let target = (detached.workspace != app.projects.read(cx).scratch_id(cx))
            .then_some(detached.workspace);
        self.place_titled(target, &tool, metadata, &detached.title, cx);
    }

    /// Delete on a catalog chat no pane shows: the same confirmation and cleanup as
    /// closing its tab.
    pub fn request_delete_detached(chat: String, cx: &mut App) {
        let chats = vec![chat.clone()];
        Self::guard_chats(
            vec![],
            chats,
            move |cx| {
                let registry = cx.global::<AppState>().chats.clone();
                registry.update(cx, |registry, cx| registry.delete_detached(&chat, cx));
            },
            true,
            cx,
        );
    }

    /// Deletes a catalog chat no pane shows (host Delete or a remote Delete command):
    /// its runtime stops, then history, record and private folder go.
    pub fn delete_detached(&mut self, chat: &str, cx: &mut Context<Self>) {
        let Some(detached) = self.detached.get(chat).cloned() else {
            return;
        };
        let pane = match self.runtimes.get(chat) {
            Some(runtime) => {
                let runtime = runtime.read(cx);
                let mut pane = runtime.pane_ref().clone();
                pane.metadata.chat = Some(Box::new(runtime.settings().clone()));
                pane
            }
            None => match cx.global::<AppState>().settings.read(cx).chat_record(chat) {
                Some(record) => canopy_desktop::state::workspace::Pane {
                    id: detached.id,
                    tool: record.agent().to_owned(),
                    metadata: record.metadata().clone(),
                },
                None => return,
            },
        };
        if let Some(cwd) = pane.metadata.cwd.as_deref()
            && detached.workspace == cx.global::<AppState>().projects.read(cx).scratch_id(cx)
            && let Some(error) = self.directory_blocker(cwd, &[detached.id], cx)
        {
            self.error = Some(error);
            cx.notify();
            return;
        }
        self.detached.remove(chat);
        self.panes.remove(&detached.id);
        let runtime = self.drop_runtime(chat);
        self.retire(detached.workspace, pane, runtime, cx);
        cx.notify();
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
                .runtime(target.chat)
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

    /// New chat follows the selected checkout; outside a project it gets a private folder.
    pub fn open_current(&mut self, cx: &mut Context<Self>) {
        let target = cx
            .global::<AppState>()
            .projects
            .read(cx)
            .catalog
            .active
            .map_or(ChatTarget::NoProject, ChatTarget::Project);
        self.open_new(target, cx);
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
        let profile_mode = chosen.and_then(|p| profile_mode(provider, p));
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
            agent_options: Default::default(),
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
                    request.provider.tool(),
                    metadata,
                    &title,
                    cx,
                );
            });
        }));
    }

    /// Opens a closed chat again (DEC-094): in its project when that is open, else
    /// where the user is, in its own folder; its history and agent session come back.
    /// `replace`: an empty chat it takes the place of.
    pub fn reopen(
        &mut self,
        closed: canopy_desktop::chat::closed::ClosedChat,
        replace: Option<PaneId>,
        cx: &mut Context<Self>,
    ) {
        if Self::unavailable(cx).is_some() || self.opening.is_some() {
            return;
        }
        let Some(chat) = closed.chat_id().map(str::to_owned) else {
            return;
        };
        // Any chat agent still configured: Claude, Codex or an ACP agent.
        if runtime::backend_of(&closed.tool, cx).is_none() {
            return;
        }
        let tool = closed.tool.clone();
        let app = cx.global::<AppState>().clone();
        let (target, active) = {
            let projects = app.projects.read(cx);
            let catalog = &projects.catalog;
            let target = match &closed.project {
                None => None,
                Some(_) => catalog
                    .items
                    .iter()
                    .find(|project| {
                        closed.in_project(Some(
                            project.worktree_path.as_deref().unwrap_or(&project.path),
                        ))
                    })
                    .map(|project| project.workspace)
                    .or(catalog.active),
            };
            (target, catalog.active)
        };
        app.chat_history
            .update(cx, |history, cx| history.reopened(&chat, cx));
        let mut metadata = closed.metadata.clone();
        metadata.title = Some(closed.title.clone());
        self.place_titled(target, &tool, metadata, &closed.title, cx);
        // The empty chat `/resume` was typed in gives way (it holds nothing to keep).
        if let Some(pane) = replace
            && target == active
            && self
                .runtime(pane)
                .is_some_and(|runtime| !runtime.read(cx).has_conversation())
        {
            app.workspace.update(cx, |workspace, cx| {
                if let Some(tab) = workspace.tab_of(pane)
                    && workspace.close_pane(tab, pane).is_ok()
                {
                    cx.notify();
                }
            });
        }
    }

    /// Deletes a closed chat for good: its history, attachments and "No project" folder.
    pub fn forget(
        &mut self,
        closed: &canopy_desktop::chat::closed::ClosedChat,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = closed.chat_id().map(str::to_owned) else {
            return;
        };
        let delete = cx
            .global::<AppState>()
            .chat_history
            .clone()
            .update(cx, |history, cx| history.delete(&chat, cx));
        cx.spawn(async move |this, cx| {
            if let Err(error) = delete.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
        if let Some(cwd) = closed.metadata.cwd.clone() {
            cx.background_spawn(async move {
                if let Ok(data) = canopy_desktop::platform::directories::data_dir()
                    && canopy_desktop::chat::scratch::owns(&data, &cwd)
                {
                    let _ = canopy_desktop::chat::scratch::remove(&data, &cwd);
                }
            })
            .detach();
        }
    }

    fn place(
        &mut self,
        target: Option<WorkspaceId>,
        provider: Provider,
        metadata: PaneMetadata,
        cx: &mut Context<Self>,
    ) {
        self.place_titled(target, provider.tool(), metadata, NEW_CHAT_TITLE, cx);
    }

    fn place_titled(
        &mut self,
        target: Option<WorkspaceId>,
        tool: &str,
        metadata: PaneMetadata,
        title: &str,
        cx: &mut Context<Self>,
    ) {
        let projects = cx.global::<AppState>().projects.clone();
        let failed = metadata.clone();
        let result = projects.update(cx, |projects, cx| {
            projects.open_chat(target, title, tool, metadata, cx)
        });
        if result.is_err() && target.is_none() {
            let workspace = projects.read(cx).scratch_id(cx);
            self.retire(
                workspace,
                canopy_desktop::state::workspace::Pane {
                    id: PaneId::new(),
                    tool: tool.to_owned(),
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
            .runtimes
            .iter()
            .filter(|(_, runtime)| runtime.read(cx).is_live())
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
        let runtimes: Vec<_> = self
            .runtimes
            .iter()
            .filter(|(chat, _)| self.workspaces.get(*chat) == Some(&id))
            .map(|(_, runtime)| runtime.clone())
            .chain(
                self.retiring
                    .iter()
                    .filter(|retiring| retiring.workspace == id)
                    .filter_map(|retiring| retiring.runtime.clone()),
            )
            .collect();
        let close = self.close_all(runtimes, false, cx);
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
        let runtimes: Vec<_> = self
            .runtimes
            .values()
            .cloned()
            .chain(
                self.retiring
                    .iter()
                    .filter_map(|retiring| retiring.runtime.clone()),
            )
            .collect();
        let close = self.close_all(runtimes, true, cx);
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
                    this.view_events.clear();
                    this.runtimes.clear();
                    this.panes.clear();
                    this.detached.clear();
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

    /// Joins each runtime's single in-flight close; the first error is reported.
    fn close_all(
        &mut self,
        runtimes: Vec<Entity<ChatRuntime>>,
        for_good: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let tasks: Vec<_> = runtimes
            .iter()
            .map(|runtime| {
                runtime.update(cx, |runtime, cx| {
                    if for_good {
                        runtime.close(cx)
                    } else {
                        runtime.shutdown(cx)
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
impl EventEmitter<crate::ui::chat::ClientForkRequested> for Chats {}
impl EventEmitter<crate::ui::chat::ClientFileRequested> for Chats {}

/// The mode an account's profile starts its chats in.
fn profile_mode(
    provider: Provider,
    profile: &canopy_desktop::state::tools::Profile,
) -> Option<Mode> {
    match provider {
        Provider::Claude => Mode::from_claude(&profile.settings.permission_mode),
        Provider::Codex => Some(Mode::from_codex(
            &profile.settings.approval_mode,
            &profile.settings.sandbox,
            profile.settings.full_auto,
            profile.settings.bypass_approvals,
        )),
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

#[cfg(test)]
mod tests {
    use super::AppState;
    use canopy_desktop::{
        state::workspace::{PaneKind, Workspace},
        terminal::environment::ShellEnvironment,
    };
    use gpui_kit::TestAppContext;
    use std::sync::Arc;

    #[gpui_kit::test]
    async fn a_chat_runtime_restores_and_guards_its_conversation_without_a_view(
        cx: &mut TestAppContext,
    ) {
        use canopy_desktop::chat::{config::ChatSettings, session::ChatSession};
        use canopy_desktop::settings::{ChatHistoryBatch, SettingsClient};
        use canopy_desktop::state::workspace::PaneMetadata;
        cx.background_executor.allow_parking();
        let folder = tempfile::tempdir().unwrap();
        let client = SettingsClient::create(&folder.path().join("settings.db"))
            .await
            .unwrap();
        let chat = ChatSettings::new_id();
        let (mut session, _) = ChatSession::with_settings(ChatSettings::default());
        session.note("Saved before the restart".into());
        let item = serde_json::to_string(&session.transcript().items()[0]).unwrap();
        client
            .save_chat_history(vec![ChatHistoryBatch {
                chat: chat.clone(),
                epochs: vec![(0, "{}".into()), (1, "{}".into())],
                items: vec![(0, 1, item.clone()), (1, 1, item)],
            }])
            .await
            .unwrap();
        let mut workspace = Workspace::new();
        let tab = workspace.open("Chat", "claude");
        let pane = workspace.active().unwrap().focused;
        workspace
            .set_pane_metadata(
                tab,
                pane,
                PaneMetadata {
                    kind: PaneKind::Chat,
                    chat: Some(Box::new(ChatSettings {
                        id: Some(chat),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap();
        let (app, runtime) = cx.update(|cx| {
            let app = AppState::install_test(workspace, cx);
            app.projects.update(cx, |projects, _| projects.ready = true);
            app.chat_history
                .update(cx, |history, cx| history.initialize(client.clone(), cx));
            let runtime = app
                .chats
                .update(cx, |chats, cx| chats.ensure_runtime(pane, cx))
                .expect("a chat pane gets a runtime without any window");
            (app, runtime)
        });
        for _ in 0..500 {
            cx.run_until_parked();
            if !cx.update(|cx| runtime.read(cx).history_uncertain()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let asked = std::rc::Rc::new(std::cell::Cell::new(false));
        cx.update(|cx| {
            let chats = app.chats.read(cx);
            assert!(chats.views.is_empty());
            let same = chats.runtime(pane).unwrap();
            assert_eq!(same.entity_id(), runtime.entity_id());
            let runtime = runtime.read(cx);
            assert!(!runtime.history_uncertain());
            // The earlier session is put away; the last one waits for the first start.
            assert_eq!(runtime.archived().len(), 1);
            assert!(runtime.dormant() && !runtime.is_live());
            assert!(runtime.has_conversation());
            assert_eq!(chats.at_risk(&[pane], &[], cx).chats, 1);
            let ran = asked.clone();
            super::Chats::guard(vec![pane], move |_| ran.set(true), true, cx);
        });
        assert!(
            !asked.get(),
            "deleting a conversation shown by no view still asks first"
        );
        client.shutdown().await.unwrap();
    }

    /// Waits for the runtime's background database work.
    async fn settle(cx: &mut TestAppContext, done: impl Fn(&mut gpui_kit::App) -> bool) {
        for _ in 0..500 {
            cx.run_until_parked();
            if cx.update(|cx| done(cx)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the runtime didn't settle");
    }

    #[gpui_kit::test]
    async fn the_queue_is_host_state_that_quit_keeps_and_a_restart_holds(cx: &mut TestAppContext) {
        use crate::app_state::LineState;
        use canopy_desktop::chat::{config::ChatSettings, session::Draft};
        use canopy_desktop::settings::{Access, SettingsClient};
        use canopy_desktop::state::workspace::PaneMetadata;
        cx.background_executor.allow_parking();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let chat = ChatSettings::new_id();
        let mut workspace = Workspace::new();
        let tab = workspace.open("Chat", "claude");
        let pane = workspace.active().unwrap().focused;
        workspace
            .set_pane_metadata(
                tab,
                pane,
                PaneMetadata {
                    kind: PaneKind::Chat,
                    chat: Some(Box::new(ChatSettings {
                        id: Some(chat.clone()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap();
        let runtime = cx.update(|cx| {
            let app = AppState::install_test(workspace, cx);
            app.projects.update(cx, |projects, _| projects.ready = true);
            app.settings.update(cx, |settings, _| {
                settings.install_test_client(client.clone(), host)
            });
            app.chat_history
                .update(cx, |history, cx| history.initialize(client.clone(), cx));
            app.chats
                .update(cx, |chats, cx| chats.ensure_runtime(pane, cx))
                .unwrap()
        });
        settle(cx, |cx| runtime.read(cx).host_queue().is_some()).await;
        // The agent can't start here (no shell environment): messages wait durably.
        for text in ["first", "second"] {
            runtime.update(cx, |runtime, cx| runtime.send_draft(Draft::text(text), cx));
            cx.run_until_parked();
        }
        settle(cx, |cx| runtime.read(cx).queue_lines().len() == 2).await;
        let lines = cx.update(|cx| runtime.read(cx).queue_lines());
        assert!(lines.iter().all(|line| line.state == LineState::Queued));
        assert_eq!(cx.update(|cx| runtime.read(cx).unsent()), 2);
        // Reordering is revision-checked host state.
        let (first, second) = (lines[0].id.clone(), lines[1].id.clone());
        runtime.update(cx, |runtime, cx| {
            runtime.move_queued(second.clone(), first.clone(), cx)
        });
        settle(cx, |cx| {
            runtime
                .read(cx)
                .queue_lines()
                .first()
                .map(|line| line.id.clone())
                == Some(second.clone())
        })
        .await;
        // Closing a tab detaches the view; the shared chat and durable queue remain.
        cx.update(|cx| {
            let app = cx.global::<AppState>().clone();
            app.chats.update(cx, |chats, _| chats.detach_views(&[pane]));
            app.workspace.update(cx, |workspace, cx| {
                workspace.close(tab).unwrap();
                cx.notify();
            });
            app.chats.update(cx, |chats, cx| {
                chats.sync_model(cx);
            });
            let chats = app.chats.read(cx);
            assert_eq!(
                chats.runtime_for_chat(&chat).unwrap().entity_id(),
                runtime.entity_id()
            );
            assert!(chats.detached().any(|entry| entry.chat == chat));
            assert!(chats.retiring.is_empty());
        });
        // Quitting closes chats without deleting them: nothing to confirm or discard.
        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        cx.update(|cx| {
            let ran = closed.clone();
            super::Chats::guard(vec![pane], move |_| ran.set(true), false, cx);
        });
        assert!(closed.get(), "quit keeps queued messages without asking");
        client.shutdown().await.unwrap();

        // The next start of Canopy holds the queue until an explicit Resume.
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        client.initialize_runtime().await.unwrap();
        let queue = client.load_runtime_queue(chat).await.unwrap();
        assert!(queue.paused());
        let order: Vec<_> = queue.entries().iter().map(|e| e.id().to_owned()).collect();
        assert_eq!(order, vec![second, first]);
        assert!(
            queue
                .entries()
                .iter()
                .all(|e| e.state() == canopy_desktop::host::QueueState::Queued)
        );
        client.shutdown().await.unwrap();
    }

    #[gpui_kit::test]
    async fn a_chat_created_without_a_pane_runs_by_its_uuid_and_survives_layout_sync(
        cx: &mut TestAppContext,
    ) {
        use canopy_desktop::chat::{config::ChatSettings, session::Draft};
        use canopy_desktop::host::ChatRecord;
        use canopy_desktop::settings::{Access, SettingsClient};
        use canopy_desktop::state::workspace::PaneMetadata;
        cx.background_executor.allow_parking();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let chat = ChatSettings::new_id();
        // As a remote Create leaves it: a catalog record, no pane, no window.
        let record = ChatRecord::new(
            chat.clone(),
            None,
            "claude".into(),
            PaneMetadata {
                kind: PaneKind::Chat,
                cwd: Some(folder.path().to_owned()),
                profile_id: Some("claude-default".into()),
                chat: Some(Box::new(ChatSettings {
                    id: Some(chat.clone()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )
        .unwrap();
        client.save_runtime_chat(record.clone()).await.unwrap();
        let (app, runtime) = cx.update(|cx| {
            let app = AppState::install_test(Workspace::new(), cx);
            app.projects.update(cx, |projects, _| projects.ready = true);
            app.settings.update(cx, |settings, _| {
                settings.install_test_client(client.clone(), host);
                settings.record_saved(record);
            });
            app.chat_history
                .update(cx, |history, cx| history.initialize(client.clone(), cx));
            let runtime = app
                .chats
                .update(cx, |chats, cx| chats.ensure_runtime_for_chat(&chat, cx))
                .expect("a catalog chat gets a runtime without a pane");
            (app, runtime)
        });
        settle(cx, |cx| runtime.read(cx).host_queue().is_some()).await;
        // One after another: the first Send goes on with a new chat's held queue.
        for (count, text) in ["first", "second"].into_iter().enumerate() {
            runtime.update(cx, |runtime, cx| runtime.send_draft(Draft::text(text), cx));
            settle(cx, |cx| {
                let runtime = runtime.read(cx);
                runtime.queue_lines().len() == count + 1 && !runtime.queue_paused()
            })
            .await;
        }
        let ids: Vec<_> = cx.update(|cx| {
            runtime
                .read(cx)
                .queue_lines()
                .into_iter()
                .map(|line| line.id)
                .collect()
        });
        cx.update(|cx| {
            // The layout has no pane for it: syncing keeps it, as the same entity.
            app.chats.update(cx, |chats, cx| {
                chats.sync_model(cx);
                chats.sync_model(cx);
            });
            let chats = app.chats.read(cx);
            assert!(chats.views.is_empty());
            let same = chats
                .runtime_for_chat(&chat)
                .expect("layout sync keeps a chat no pane shows");
            assert_eq!(same.entity_id(), runtime.entity_id());
            let scratch = app.projects.read(cx).scratch_id(cx);
            assert_eq!(chats.detached_in(&[scratch]), vec![chat.clone()]);
        });
        // Steer keeps the accepted message: it moves first under its own ID.
        runtime.update(cx, |runtime, cx| runtime.steer_queued(ids[1].clone(), cx));
        settle(cx, |cx| {
            runtime
                .read(cx)
                .queue_lines()
                .first()
                .map(|line| line.id.clone())
                == Some(ids[1].clone())
        })
        .await;
        let lines = cx.update(|cx| runtime.read(cx).queue_lines());
        assert_eq!(
            lines.len(),
            2,
            "steering never removes a message from the queue"
        );
        assert_eq!(lines[1].id, ids[0]);
        // Closing its project's chats asks first, naming the remote-only chat.
        let asked = std::rc::Rc::new(std::cell::Cell::new(false));
        cx.update(|cx| {
            let scratch = app.projects.read(cx).scratch_id(cx);
            let chats = app.chats.read(cx).detached_in(&[scratch]);
            assert_eq!(app.chats.read(cx).at_risk(&[], &chats, cx).unsent, 2);
            let ran = asked.clone();
            super::Chats::guard_chats(vec![], chats, move |_| ran.set(true), true, cx);
        });
        assert!(
            !asked.get(),
            "deleting a remote-only chat with queued messages asks first"
        );
        client.shutdown().await.unwrap();

        // The next start knows the chat by the same ID, with its queue held.
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        client.initialize_runtime().await.unwrap();
        let chats = client.load_runtime_chats().await.unwrap();
        assert!(chats.iter().any(|record| record.id() == chat));
        let queue = client.load_runtime_queue(chat).await.unwrap();
        assert!(queue.paused());
        let order: Vec<_> = queue.entries().iter().map(|e| e.id().to_owned()).collect();
        assert_eq!(order, vec![ids[1].clone(), ids[0].clone()]);
        client.shutdown().await.unwrap();
    }

    #[gpui_kit::test]
    fn new_chat_follows_the_selected_checkout_without_creating_a_private_chat(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            let app = AppState::install_test(Workspace::new(), cx);
            app.tools.update(cx, |tools, _| {
                tools.ready = true;
                tools.environment = Some(Ok(Arc::new(ShellEnvironment {
                    shell: "/bin/sh".into(),
                    vars: Default::default(),
                })));
            });
            app.projects.update(cx, |projects, _| projects.ready = true);
            let scratch = app.workspace.read(cx).id;
            for checkout in ["/repo", "/worktrees/feature-sidebar"] {
                let path = std::path::PathBuf::from(checkout);
                app.projects.update(cx, |projects, cx| {
                    projects.open_worktree("/repo".into(), path.clone(), cx)
                });
                let target = app.projects.read(cx).catalog.active.unwrap();
                app.chats.update(cx, |chats, cx| chats.open_current(cx));
                let workspace = app.workspace.read(cx);
                assert_eq!(workspace.id, target);
                assert_ne!(workspace.id, scratch);
                let panes = workspace.all_panes();
                assert_eq!(panes.len(), 1);
                assert_eq!(panes[0].metadata.kind, PaneKind::Chat);
                assert_eq!(panes[0].metadata.cwd.as_ref(), Some(&path));
                assert!(
                    app.projects
                        .read(cx)
                        .scratch_workspace(cx)
                        .tabs()
                        .is_empty()
                );
                assert!(!app.chats.read(cx).is_opening());
                assert!(app.chats.read(cx).error.is_none());
            }
        });
    }
}
