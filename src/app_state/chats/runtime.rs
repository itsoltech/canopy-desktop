//! One conversation's runtime: its agent session and process, event pump, queue,
//! decisions, generation, saved history and cleanup. It needs no `Window` or view: a
//! hidden tab, a turn diff or a remote command drive the same runtime, and every
//! `ChatView` of the chat only presents it (OWN-01…03).
use super::super::AppState;
use canopy_desktop::{
    agents::launch as hook_launch,
    chat::{
        acp::{self, AcpSession, HostCapabilities},
        agent::{BackendKind, Capabilities, OptionChoice, OptionEffect, SessionOption},
        backend_session::{self, BackendDecision, BackendIncoming, BackendSession},
        codex::wire as codex_wire,
        config::{ChatSettings, Effort, Mode},
        goal::EndReason,
        history::{AgentRef, Boundary, EpochMeta, Saved, SavedEpoch},
        launch::{HookConfig, fork_at, prepare_acp, prepare_claude, prepare_codex},
        presentation,
        process::{Event, Process},
        provider::Provider,
        session::{ChatSession, CodexStart, Draft, Response, Status},
        shell::{Running, ShellEvent},
        transcript::{Body, Change, ItemId, Notice, NoticeKind, PermissionKind, Transcript},
    },
    state::workspace::{ChatTitleState, Pane},
};
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

/// Something about the chat changed that its registry shows (status, liveness).
pub struct ChatStateChanged;

/// What presenting views follow, in order. Transcript indices in `Changed` belong to
/// session `serial`: the running one, or one already put away in `archived`.
pub enum ChatRuntimeEvent {
    Changed {
        serial: u64,
        changes: Rc<[Change]>,
    },
    /// The running session was replaced (Restart, Edit): it is the last `archived` entry.
    Archived,
    /// Saved sessions were read: `archived` holds them, before any running session.
    Restored,
    /// The account changed before the first message: the conversation starts over.
    Reset,
    /// A message taken from the user could not go out: back to the composer.
    DraftReturned(Draft),
    /// A device-owned composer version was durably accepted. A later draft wins.
    DraftConsumed(u64),
    EditFinished {
        revision: u64,
        result: Result<(), String>,
    },
    FileRequested(transport::ThreadFile),
    /// The process stops (close, restart): live presentations of it end.
    Stopping,
}

impl EventEmitter<ChatStateChanged> for ChatRuntime {}
impl EventEmitter<ChatRuntimeEvent> for ChatRuntime {}

/// A session replaced by Restart, Edit or New session (or restored from history),
/// kept whole so its diffs and subagent conversations still open.
pub struct Archived {
    /// Stable session number of the chat (saved history, turn diff targets).
    pub epoch: u64,
    pub transcript: Transcript,
    /// Who ran it (to come back to its session after a switch).
    pub agent: Option<AgentRef>,
    /// Separator under it ("Session restarted").
    pub label: &'static str,
    /// The runtime session it was, when it ran in this runtime.
    serial: Option<u64>,
}
impl Archived {
    /// The runtime session it was (`None`: restored from saved history).
    pub fn serial(&self) -> Option<u64> {
        self.serial
    }
}

/// The chat's saved conversation, read once when the runtime is created.
pub(super) enum History {
    Loading(#[allow(dead_code)] Task<()>),
    Loaded,
}

/// One in-flight shutdown shared by every caller (pane close, Stop worktree, quit).
struct Closing {
    generation: u64,
    _task: Task<()>,
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
}

/// A `!` command running for this chat (DEC-072).
struct Shell {
    running: Running,
    _pump: Task<()>,
}

mod host_services;
mod transport;
pub(crate) use transport::{
    Account, AttachmentSuggestion, ComposerClear, ComposerOperation, ContextSource,
    DraftPreparation, EditProgress, EditSubmission, HistoryPosition, MessageEdit, PendingOutcome,
    PendingRequest, Recovering, ThreadDestination, ThreadFile, ThreadPlace, ThreadTransport,
};
mod queue;
// The remote bridge's typed chat operations; the gRPC server is their caller.
#[cfg_attr(not(test), allow(dead_code))]
mod remote;
use host_services::HostTerminals;
pub use queue::{LineState, QueueLine};
pub use remote::{
    ChatCommand, ChatSnapshot, DecisionAnswer, EditPreview, FilesPreview, PendingDecision,
    RemoteKey, SnapshotEpoch,
};

/// How long a Codex goal waits for the next turn Codex starts by itself.
const GOAL_HOLD: std::time::Duration = std::time::Duration::from_secs(5);
/// An idle chat nobody presents gives its process back after this (LIFE-09).
const DORMANT_AFTER: std::time::Duration = std::time::Duration::from_secs(60);
/// Chat processes running at once; another start is refused, not queued (LIFE-09).
pub const MAX_CHAT_PROCESSES: usize = 32;
/// ACP lines may carry whole files and replayed turns.
const ACP_MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// Whether a composer notice reports a failure or only explains what happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatNoticeKind {
    /// Something failed: it stays until dismissed or replaced.
    Error,
    /// An explanation: it goes with the next message.
    Info,
}

/// Said under the composer: what couldn't be done, or why something waits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatNotice {
    pub kind: ChatNoticeKind,
    pub text: String,
}
impl ChatNotice {
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: ChatNoticeKind::Error,
            text: text.into(),
        }
    }
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            kind: ChatNoticeKind::Info,
            text: text.into(),
        }
    }
    /// What stays once a message is sent: errors, until dismissed or replaced.
    fn after_send(notice: Option<Self>) -> Option<Self> {
        notice.filter(|notice| notice.kind == ChatNoticeKind::Error)
    }
}

/// A new run identity. Seeded from the clock once per process, so a run ID of an
/// earlier Canopy process (a remote client's stale Interrupt, decision or Steer) never
/// names a run of this one; only equality is ever compared.
fn next_run() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: std::sync::OnceLock<AtomicU64> = std::sync::OnceLock::new();
    NEXT.get_or_init(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_micros() as u64);
        // Microseconds since 1970 leave far more room than any process uses.
        AtomicU64::new(now << 8)
    })
    .fetch_add(1, Ordering::Relaxed)
}

/// Chat runtimes whose process runs or starts, for the process limit. A global, not
/// `Chats`: starts run inside `Chats` updates.
#[derive(Default)]
pub struct LiveChats(std::cell::RefCell<HashSet<EntityId>>);
impl Global for LiveChats {}

/// Which protocol family `tool` speaks in a chat, from the tool catalog.
pub fn backend_of(tool: &str, cx: &App) -> Option<BackendKind> {
    match Provider::from_tool(tool) {
        Some(Provider::Claude) => Some(BackendKind::Claude),
        Some(Provider::Codex) => Some(BackendKind::Codex),
        None => cx
            .global::<AppState>()
            .tools
            .read(cx)
            .catalog
            .get(tool)
            .filter(|tool| tool.acp)
            .map(|_| BackendKind::Acp),
    }
}

pub struct ChatRuntime {
    transport: ThreadTransport,
    pane: Pane,
    settings: ChatSettings,
    session: Option<BackendSession>,
    /// Number of the session object in this runtime; views key transcript indices by it.
    serial: u64,
    /// Earlier sessions of the conversation, oldest first.
    archived: Vec<Arc<Archived>>,
    process: Option<Arc<Process>>,
    orphaned: Vec<Arc<Process>>,
    shell_resources: Vec<Running>,
    /// Lines produced before the process exists (initialize, early messages).
    outbox: Vec<serde_json::Value>,
    agent_run: Option<String>,
    generation: u64,
    /// Local dispatch to first new main-thread assistant text; no provider timestamps.
    first_text: Option<(std::time::Instant, u64, usize)>,
    starting: bool,
    error: Option<String>,
    start_task: Option<Task<()>>,
    pump: Option<Task<()>>,
    closing: Option<Closing>,
    /// Restored with a provider session: earlier messages exist but are not shown.
    resumed: bool,
    sent_any: bool,
    /// The registry closed this chat (or the app quits): never start again.
    closed: bool,
    /// An account switch or an edit is restarting the process; ignore further switches.
    switching: bool,
    /// Starts fork the session after this point (an edit, or a forked chat) until the
    /// new session has its own id.
    fork_at: Option<String>,
    /// Sent as soon as the next session starts (the edited message).
    send_on_start: Option<Draft>,
    /// The edited message sent at start, until the session is up (back on failure).
    edit_recovery: Option<Draft>,
    /// Remote commands waiting on what the session does next (an edit's start, a
    /// file restore's answer).
    remote_waits: remote::Waits,
    /// Working (true) or idle last told to the notch, for agents without hooks.
    reported_activity: Option<bool>,
    /// What the notch was last told this chat waits for.
    reported_decision: Option<String>,
    /// Whether the notch was last told the chat pursues its goal.
    reported_goal: Option<bool>,
    /// Lets a Codex goal hold go when Codex starts no further turn.
    goal_hold_timer: Option<Task<()>>,
    /// Ends of Claude goals already checked against its session file.
    goal_records: HashSet<ItemId>,
    /// Separator shown above the replaced session at the next start.
    boundary: Boundary,
    /// Number of the running session in the chat's saved history.
    epoch: u64,
    /// How the running session began and who runs it, as saved (DEC-065).
    epoch_meta: EpochMeta,
    history: History,
    /// The last saved session, continued by the first start.
    restored: Option<SavedEpoch>,
    /// Said in the conversation at the next start (history that couldn't be read).
    history_note: Option<String>,
    /// Restored items already saved: their insertion isn't written again.
    history_skip: usize,
    /// The saved history couldn't be read: nothing is written over it.
    history_off: bool,
    /// Ids of saved rows that couldn't be read (a newer version): new ones go after.
    item_floor: u64,
    /// `start` was asked for while the history was loading.
    start_waiting: bool,
    shells: HashMap<ItemId, Shell>,
    /// Commands the user (or closing) stopped, so the agent hears it.
    stopped_shells: HashSet<ItemId>,
    /// The running session's token for the page service (`preview_html`).
    page_token: Option<String>,
    /// Terminals an ACP agent created for its run; humans only watch them.
    terminals: HostTerminals,
    /// Host requests (files, terminals) of this run still being answered.
    host_busy: usize,
    /// A view shows this chat; without one an idle process may sleep (LIFE-09).
    presented: bool,
    sleep_timer: Option<Task<()>>,
    /// Said under the composer: errors until dismissed or replaced, explanations
    /// until the next message.
    notice: Option<ChatNotice>,
    /// The status the registry (sidebar, tabs) last heard.
    reported_status: &'static str,
    /// What remote clients last heard of the chat's state besides its status (goal
    /// accounting, options, decisions, notices): a change emits `ChatStateChanged`.
    reported_state: Option<remote::StateKey>,
    /// The chat's durable queue and catalog record.
    queue: queue::HostQueue,
    /// The host adopted this chat without its agent and account: nothing runs until
    /// the user chooses them (never a default account).
    recovery: Option<String>,
    /// The session (`serial`) whose saved ACP choices were read, and those still to
    /// offer it, one at a time.
    options_restored: u64,
    options_to_restore: Vec<(String, String)>,
    /// Created for a catalog chat no pane shows: its record's workspace (`None`:
    /// private). Cleared once a pane presents it.
    catalog_root: Option<Option<String>>,
    /// The pane the running agent was registered under (the notch keys runs by it).
    run_pane: canopy_desktop::state::workspace::PaneId,
    /// Remote clients subscribed to this chat; like views, they keep it awake.
    observers: usize,
    /// Saved model lists and device favorites notify through the same runtime
    /// observation as the executing environment's live catalog.
    _catalog_observation: Option<Subscription>,
}

impl ChatRuntime {
    pub fn new(pane: Pane, cx: &mut Context<Self>) -> Self {
        // However it goes, a dropped runtime no longer counts toward the process limit.
        let me = cx.entity_id();
        cx.on_release(move |_, cx| {
            if let Some(live) = cx.try_global::<LiveChats>() {
                live.0.borrow_mut().remove(&me);
            }
        })
        .detach();
        let pane_id = pane.id;
        let mut settings = pane.metadata.chat.as_deref().cloned().unwrap_or_default();
        let resumed = pane.metadata.resume_id.is_some();
        let (transport, history) = ThreadTransport::initialize(&pane, &mut settings, cx);
        let catalog_observation = ThreadTransport::observe_catalog(cx);
        Self {
            transport,
            pane,
            settings,
            session: None,
            serial: 0,
            archived: vec![],
            process: None,
            orphaned: vec![],
            shell_resources: vec![],
            outbox: vec![],
            agent_run: None,
            generation: next_run(),
            first_text: None,
            starting: false,
            error: None,
            start_task: None,
            pump: None,
            closing: None,
            resumed,
            sent_any: false,
            closed: false,
            switching: false,
            fork_at: None,
            send_on_start: None,
            edit_recovery: None,
            remote_waits: Default::default(),
            reported_activity: None,
            reported_decision: None,
            reported_goal: None,
            goal_hold_timer: None,
            goal_records: HashSet::new(),
            boundary: Boundary::Restarted,
            epoch: 0,
            epoch_meta: EpochMeta::default(),
            history,
            restored: None,
            history_note: None,
            history_skip: 0,
            history_off: false,
            item_floor: 0,
            start_waiting: false,
            shells: HashMap::new(),
            stopped_shells: HashSet::new(),
            page_token: None,
            terminals: HostTerminals::default(),
            host_busy: 0,
            presented: false,
            sleep_timer: None,
            notice: None,
            reported_status: "Not started",
            reported_state: None,
            queue: Default::default(),
            recovery: None,
            options_restored: 0,
            options_to_restore: vec![],
            catalog_root: None,
            run_pane: pane_id,
            observers: 0,
            _catalog_observation: catalog_observation,
        }
    }

    /// A catalog chat created without a pane (remote): its record's workspace.
    pub(in crate::app_state) fn set_catalog_root(&mut self, root: Option<String>) {
        self.catalog_root = Some(root);
    }
    /// A pane of the layout shows this chat now (opened from the sidebar, or a chat
    /// created remotely). The runtime keeps its own, current launch state; the pane
    /// becomes its local key. A running agent stays registered under its old key.
    fn attach_pane_local(&mut self, pane: Pane, cx: &mut Context<Self>) {
        self.catalog_root = None;
        self.pane.id = pane.id;
        if pane.metadata.title.is_some() {
            self.pane.metadata.title = pane.metadata.title;
            self.pane.metadata.chat_title_state = pane.metadata.chat_title_state;
        }
        if let Some(session) = self.pane.metadata.resume_id.clone()
            && pane.metadata.resume_id.as_deref() != Some(session.as_str())
        {
            let current = self.pane.clone();
            cx.global::<AppState>()
                .projects
                .clone()
                .update(cx, |projects, cx| {
                    projects.record_agent_session(&current, &session, cx)
                });
        }
        cx.emit(ChatStateChanged);
        cx.notify();
    }
    /// A remote client subscribed to this chat (LIFE-09: it keeps the process).
    #[cfg_attr(not(test), allow(dead_code, reason = "called by the remote bridge"))]
    pub fn observe(&mut self, cx: &mut Context<Self>) {
        self.observers += 1;
        self.schedule_sleep(cx);
    }
    /// A remote subscription ended (Detach, lost connection): nothing else changes.
    #[cfg_attr(not(test), allow(dead_code, reason = "called by the remote bridge"))]
    pub fn unobserve(&mut self, cx: &mut Context<Self>) {
        self.observers = self.observers.saturating_sub(1);
        self.schedule_sleep(cx);
    }
    pub fn observers(&self) -> usize {
        self.observers
    }

    // Reading.

    pub fn pane_ref(&self) -> &Pane {
        &self.pane
    }
    pub fn settings(&self) -> &ChatSettings {
        &self.settings
    }
    /// The running session, whatever protocol it speaks.
    pub fn backend(&self) -> Option<&BackendSession> {
        self.session.as_ref()
    }
    /// The Claude or Codex session, for their own extensions (goal, queue, rewind,
    /// `!` commands, models). `None` for other agents: those features are absent.
    pub fn native(&self) -> Option<&ChatSession> {
        match &self.session {
            Some(BackendSession::Native(session)) => Some(session),
            _ => None,
        }
    }
    fn native_mut(&mut self) -> Option<&mut ChatSession> {
        match &mut self.session {
            Some(BackendSession::Native(session)) => Some(session),
            _ => None,
        }
    }
    fn acp(&self) -> Option<&AcpSession> {
        match &self.session {
            Some(BackendSession::Acp(session)) => Some(session),
            _ => None,
        }
    }
    /// What the chat's agent can do. Before it starts: what its protocol family offers
    /// at most (an ACP agent tells only once initialized).
    fn local_capabilities(&self, cx: &App) -> Capabilities {
        match (&self.session, self.backend_kind(cx)) {
            (Some(session), _) => session.capabilities(),
            (None, Some(BackendKind::Acp) | None) => Capabilities::default(),
            (None, Some(kind)) => native_capabilities(kind == BackendKind::Claude),
        }
    }
    /// The protocol family of the chat's agent; `None` when its tool is gone.
    /// The agent's own options (ACP configuration, models and modes) with their
    /// opaque ids. Claude and Codex keep their dedicated model and mode controls.
    fn local_agent_options(&self) -> Vec<AgentOption> {
        let Some(session) = self.acp() else {
            return vec![];
        };
        let mut options: Vec<_> = session.options().iter().map(AgentOption::from).collect();
        let has = |category: &str| {
            session
                .options()
                .iter()
                .any(|option| option.category.as_deref() == Some(category))
        };
        let (model, mode) = (has("model"), has("mode"));
        if !model && !session.models().is_empty() {
            options.push(legacy_option(
                ACP_MODEL,
                "Model",
                "model",
                session.current_model(),
                session.models(),
            ));
        }
        if !mode && !session.modes().is_empty() {
            options.push(legacy_option(
                ACP_MODE,
                "Mode",
                "mode",
                session.current_mode(),
                session.modes(),
            ));
        }
        options
    }
    /// Output of a terminal the agent runs, for its tool card (watched, never typed
    /// into), and whether it still runs.
    pub fn terminal_output(&self, id: &str) -> Option<(&str, bool)> {
        self.terminals.output(id)
    }
    /// Requests waiting on the user, oldest first.
    fn local_pending_decisions(&self) -> Vec<ItemId> {
        match &self.session {
            Some(BackendSession::Native(session)) => session.pending_decisions(),
            Some(BackendSession::Acp(session)) => {
                let mut items: Vec<_> = session.permissions().map(|request| request.item).collect();
                items.sort_by_key(|item| item.0);
                items
            }
            None => vec![],
        }
    }
    /// Choices an ACP agent offered for decision `item`, ids untouched (AGT-11).
    fn local_decision_options(&self, item: ItemId) -> Option<Vec<acp::PermissionOption>> {
        self.acp()?
            .permissions()
            .find(|request| request.item == item)
            .map(|request| request.options.clone())
    }
    /// The account's models (Claude, Codex): as its agent lists them, else as last
    /// remembered (DEC-098).
    fn local_models(&self, cx: &App) -> Vec<canopy_desktop::chat::config::ModelInfo> {
        match self.native().map(ChatSession::models) {
            Some(models) if !models.is_empty() => models.to_vec(),
            _ if self.provider().is_none() => vec![],
            _ => cx
                .global::<AppState>()
                .model_catalog
                .read(cx)
                .models(&self.profile(cx))
                .map(<[_]>::to_vec)
                .unwrap_or_default(),
        }
    }
    /// The chosen model, else the account's default (Claude lists a `default` entry;
    /// Codex marks its default model).
    pub fn current_model(&self, cx: &App) -> Option<canopy_desktop::chat::config::ModelInfo> {
        let models = self.models(cx);
        match self.settings.model.as_deref() {
            Some(value) => models.into_iter().find(|m| m.value == value),
            None => {
                let default = models.iter().position(|m| m.value == "default");
                let index = default.or_else(|| models.iter().position(|m| m.default))?;
                models.into_iter().nth(index)
            }
        }
    }
    /// The model named under the chat's title in the sidebar: "Opus 5.5".
    pub fn model_label(&self, cx: &App) -> Option<String> {
        if self.acp().is_some() {
            return self.agent_options().into_iter().find_map(|option| {
                (option.category.as_deref() == Some("model"))
                    .then(|| option.choices.into_iter().find(|c| c.id == option.value))
                    .flatten()
                    .map(|choice| choice.label)
            });
        }
        let label = match self.current_model(cx) {
            Some(model) => model.label(),
            None => self.settings.model.clone()?,
        };
        let label = label
            .strip_prefix("Default (")
            .and_then(|l| l.strip_suffix(')'))
            .map_or(label.clone(), str::to_owned);
        (label != "Default").then_some(label)
    }
    /// The agent's display name: its tool's name in the catalog.
    fn local_agent_name(&self, cx: &App) -> String {
        if let Some(provider) = self.provider() {
            return provider.name().to_owned();
        }
        cx.global::<AppState>()
            .tools
            .read(cx)
            .catalog
            .get(&self.pane.tool)
            .map_or_else(|| self.pane.tool.clone(), |tool| tool.name.clone())
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
    pub fn archived(&self) -> &[Arc<Archived>] {
        &self.archived
    }
    /// The transcript that session `serial` has (or had, once put away).
    pub fn transcript_of(&self, serial: u64) -> Option<&Transcript> {
        if serial == self.serial
            && let Some(transcript) = self.transcript()
        {
            return Some(transcript);
        }
        self.archived
            .iter()
            .rev()
            .find(|archived| archived.serial == Some(serial))
            .map(|archived| &archived.transcript)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn starting(&self) -> bool {
        self.starting
    }
    pub fn resumed(&self) -> bool {
        self.resumed
    }
    pub fn cwd(&self) -> Option<std::path::PathBuf> {
        self.pane.metadata.cwd.clone()
    }
    /// The Claude or Codex agent the chat talks to; `None` for an ACP agent.
    /// Identifies the chat's saved history.
    pub fn chat_id(&self) -> Option<String> {
        self.settings.id.clone()
    }
    /// The chat's account: its profile id, else the agent's default profile.
    fn local_profile(&self, cx: &App) -> String {
        self.pane
            .metadata
            .profile_id
            .clone()
            .or_else(|| {
                let catalog = &cx.global::<AppState>().tools.read(cx).catalog;
                let tool = catalog.get(&self.pane.tool)?;
                // An ACP tool without profiles is its own single account.
                tool.default_profile
                    .clone()
                    .or_else(|| tool.profiles.is_empty().then(|| tool.id.clone()))
            })
            .unwrap_or_default()
    }
    pub fn history_uncertain(&self) -> bool {
        matches!(self.history, History::Loading(_)) || self.history_off
    }
    pub fn navigation_status(&self) -> &'static str {
        if self.history_uncertain() && !self.history_off {
            return "Loading";
        }
        if self.error.is_some() || self.history_off {
            return "Error";
        }
        if self.starting {
            return "Starting";
        }
        match self.status() {
            Some(Status::Starting) => "Starting",
            Some(Status::Working) => "Working",
            Some(Status::NeedsDecision) => "Needs attention",
            Some(Status::Interrupting) => "Stopping",
            Some(Status::Ready) => "Idle",
            Some(Status::Failed(_)) => "Error",
            Some(Status::Exited { .. }) => "Exited",
            None => "Not started",
        }
    }
    /// Created without starting its agent (a turn diff, a hidden or remote chat).
    /// A process is running or starting; Stop worktree has something to stop.
    pub fn is_live(&self) -> bool {
        self.starting || self.process.is_some() || !self.closing_done()
    }
    pub fn is_working(&self) -> bool {
        matches!(
            self.status(),
            Some(Status::Working | Status::NeedsDecision | Status::Interrupting)
        )
    }
    pub fn can_restart(&self) -> bool {
        !self.starting
            && self.closing_done()
            && (self.error.is_some() || self.status().is_some_and(Status::ended))
    }
    /// Starting or closing: Restart waits for it.
    fn closing_done(&self) -> bool {
        self.closing
            .as_ref()
            .is_none_or(|closing| closing.completion.is_closed())
    }
    /// Accepted messages not yet known to be delivered; they stay with the chat.
    pub fn unsent(&self) -> usize {
        self.pending_messages()
    }
    /// Something was said here (now, in an earlier or a saved session): closing loses it.
    pub fn has_conversation(&self) -> bool {
        self.archived
            .iter()
            .any(|archived| !archived.transcript.items().is_empty())
            || self
                .transcript()
                .is_some_and(|transcript| !transcript.items().is_empty())
            || self
                .restored
                .as_ref()
                .is_some_and(|restored| !restored.items.is_empty())
    }
    /// `!` commands still running.
    pub fn running_commands(&self) -> usize {
        self.shells.len()
    }
    /// The conversation has messages: its agent and account are fixed (DEC-087).
    fn local_account_locked(&self) -> bool {
        self.resumed
            || self.sent_any
            || self.has_conversation()
            || self.pane.metadata.resume_id.is_some()
    }
    /// Who runs the current session, as saved with it (DEC-065).
    pub fn agent_ref(&self) -> AgentRef {
        let mut settings = self.settings.clone();
        // The chat's id and rules stay with the chat, not with one agent.
        settings.id = None;
        settings.allowed.clear();
        AgentRef {
            tool: self.pane.tool.clone(),
            profile: self.pane.metadata.profile_id.clone(),
            session: self.pane.metadata.resume_id.clone(),
            settings: Some(settings),
        }
    }

    /// Files the agent changed in `turn` of session `session` (running or earlier).
    pub fn turn_changes(
        &self,
        session: u64,
        turn: u64,
    ) -> Option<Vec<canopy_desktop::chat::tools::FileChange>> {
        let restored;
        let transcript = if session == self.epoch {
            match (&self.session, &self.restored) {
                (Some(session), _) => session.transcript(),
                // Not started yet (shown for a turn diff): the saved session.
                (None, Some(saved)) => {
                    restored = Transcript::restore(saved.items.clone());
                    &restored
                }
                (None, None) => return None,
            }
        } else {
            &self
                .archived
                .iter()
                .find(|archived| archived.epoch == session)?
                .transcript
        };
        let turn = canopy_desktop::chat::transcript::TurnId(turn);
        transcript
            .items()
            .iter()
            .any(|item| item.turn == turn)
            .then(|| canopy_desktop::chat::tools::turn_changes(transcript, turn))
    }

    pub fn set_title(
        &mut self,
        title: &str,
        state: Option<ChatTitleState>,
        cx: &mut Context<Self>,
    ) {
        if self.pane.metadata.title.as_deref() != Some(title)
            || self.pane.metadata.chat_title_state != state
        {
            self.pane.metadata.title = Some(title.to_owned());
            self.pane.metadata.chat_title_state = state;
            cx.notify();
        }
    }

    // History.

    /// Puts the saved conversation in place: earlier sessions archived, the last one
    /// continued by the agent at start.
    fn restore_history(&mut self, saved: Result<Saved, String>, cx: &mut Context<Self>) {
        self.history = History::Loaded;
        let chat = self.settings.id.clone();
        let saved = match saved {
            Ok(saved) => saved,
            Err(error) => {
                // What is saved stays as it is; this run of the chat isn't saved.
                self.history_off = true;
                self.history_note = Some(format!(
                    "Earlier messages couldn't be loaded ({error}); this conversation isn't saved."
                ));
                Saved::default()
            }
        };
        self.item_floor = saved.floor.unwrap_or(0);
        if saved.lost > 0 {
            self.history_note = Some(format!(
                "{} saved {} couldn't be read.",
                saved.lost,
                if saved.lost == 1 { "entry" } else { "entries" }
            ));
        }
        let mut epochs = saved.epochs;
        // Sessions saved before agents could be switched ran the chat's first agent.
        let first = epochs
            .iter()
            .filter_map(|epoch| epoch.meta.agent.as_ref())
            .next()
            .map(|agent| AgentRef {
                tool: agent.tool.clone(),
                profile: agent.profile.clone(),
                ..Default::default()
            })
            .unwrap_or_else(|| AgentRef {
                tool: self.pane.tool.clone(),
                profile: self.pane.metadata.profile_id.clone(),
                ..Default::default()
            });
        let agent_of = |epoch: &SavedEpoch| Some(epoch.meta.agent.clone().unwrap_or(first.clone()));
        let last = epochs.pop();
        let history = cx.global::<AppState>().chat_history.clone();
        let settle = |epoch: &mut SavedEpoch, cx: &mut Context<Self>| {
            let changed = canopy_desktop::chat::history::settle(&mut epoch.items);
            if let Some(chat) = &chat
                && !changed.is_empty()
            {
                let items: Vec<_> = changed.iter().map(|&i| epoch.items[i].clone()).collect();
                let index = epoch.index;
                history.update(cx, |history, cx| {
                    history.record(chat, index, None, items, false, cx)
                });
            }
        };
        for position in 0..epochs.len() {
            let below = epochs
                .get(position + 1)
                .or(last.as_ref())
                .and_then(|next| next.meta.before)
                .unwrap_or(Boundary::Restarted);
            let epoch = &mut epochs[position];
            settle(epoch, cx);
            let transcript = Transcript::restore(std::mem::take(&mut epoch.items));
            let agent = agent_of(epoch);
            self.put_away(transcript, epoch.index, agent, below.label(), None);
        }
        if let Some(mut last) = last {
            settle(&mut last, cx);
            self.epoch = last.index;
            self.restored = Some(last);
            // The conversation is on screen; nothing is missing anymore.
            self.resumed = false;
        }
        // Sessions kept for a newer version: this run starts after them.
        if let Some(unread) = saved.last_epoch.filter(|&unread| unread >= self.epoch) {
            self.epoch = unread + 1;
            if let Some(last) = self.restored.take() {
                // Its rows stay saved, but it's not continued after a newer one.
                let agent = agent_of(&last);
                let transcript = Transcript::restore(last.items);
                self.put_away(
                    transcript,
                    last.index,
                    agent,
                    Boundary::Restarted.label(),
                    None,
                );
            }
        }
        cx.emit(ChatRuntimeEvent::Restored);
        // Sends wait for the record of the history part the next start continues.
        self.save_record(cx);
        if std::mem::take(&mut self.start_waiting) {
            self.start(cx);
        }
        cx.emit(ChatStateChanged);
        cx.notify();
    }

    fn put_away(
        &mut self,
        mut transcript: Transcript,
        epoch: u64,
        agent: Option<AgentRef>,
        label: &'static str,
        serial: Option<u64>,
    ) {
        // Views read a put-away session whole; its pending changes are not theirs.
        transcript.take_changes();
        self.archived.push(Arc::new(Archived {
            epoch,
            transcript,
            agent,
            label,
            serial,
        }));
    }

    // Lifecycle.

    /// Starts (or restarts) the agent with the pane's current account and choices.
    fn start_local(&mut self, cx: &mut Context<Self>) {
        if self.closed || self.starting || !self.closing_done() || self.process.is_some() {
            return;
        }
        if matches!(self.history, History::Loading(_)) {
            self.start_waiting = true;
            return;
        }
        if let Some(reason) = &self.recovery {
            self.error = Some(format!(
                "{reason}. Choose this chat's agent and account before it can run."
            ));
            cx.notify();
            return;
        }
        let app = cx.global::<AppState>().clone();
        // Fresh launch identity: resume id and account may have changed since creation.
        if let Some(pane) = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .into_iter()
            .flat_map(|workspace| workspace.all_panes())
            .find(|pane| pane.id == self.pane.id)
        {
            self.pane = pane;
        }
        let Some(env) = app.tools.read(cx).environment.clone() else {
            self.error = Some("The shell environment is still loading.".into());
            cx.notify();
            return;
        };
        let env = match env {
            Ok(env) => env,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(kind) = self.backend_kind(cx) else {
            self.error = Some("This chat's agent is no longer configured in Preferences.".into());
            cx.notify();
            return;
        };
        // LIFE-09: one more process past the limit is refused; Restart retries.
        let me = cx.entity_id();
        let live = cx
            .default_global::<LiveChats>()
            .0
            .borrow()
            .iter()
            .filter(|id| **id != me)
            .count();
        if live >= MAX_CHAT_PROCESSES {
            self.error = Some(format!(
                "{MAX_CHAT_PROCESSES} chats are already running. Stop one, then Restart this chat."
            ));
            cx.emit(ChatStateChanged);
            cx.notify();
            return;
        }
        let acp_cwd = if kind == BackendKind::Acp {
            match self
                .pane
                .metadata
                .cwd
                .clone()
                .filter(|cwd| cwd.is_absolute())
            {
                Some(cwd) => Some(cwd),
                None => {
                    self.error = Some("This chat has no folder for its agent.".into());
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        // Queued messages are durable; what the old run was delivering is settled first.
        self.abandon_dispatch(cx);
        let mut carried_goal = None;
        let replaced = self.session.is_some();
        if let Some(session) = &mut self.session {
            let transcript = match session {
                BackendSession::Native(native) => {
                    carried_goal = native.goal().cloned();
                    native.take_transcript()
                }
                BackendSession::Acp(acp) => Transcript::restore(acp.transcript().items().to_vec()),
            };
            let agent = self.epoch_meta.agent.clone();
            let serial = self.serial;
            self.put_away(
                transcript,
                self.epoch,
                agent,
                self.boundary.label(),
                Some(serial),
            );
            cx.emit(ChatRuntimeEvent::Archived);
        }
        let boundary = std::mem::replace(&mut self.boundary, Boundary::Restarted);
        // Forking needs the session it continues. An edit, or a forked chat (its point is
        // saved with its last part, DEC-093), branches off until the new session has an
        // id: a Restart before that branches again instead of going on in the source's.
        // Only Claude and Codex fork; other agents never get a fork point.
        let fork = self
            .fork_at
            .clone()
            .or_else(|| {
                self.restored
                    .as_ref()
                    .filter(|_| !replaced)
                    .and_then(|restored| restored.meta.fork_at.clone())
            })
            .filter(|_| self.pane.metadata.resume_id.is_some() && kind != BackendKind::Acp);
        self.fork_at = fork.clone();
        // Restart keeps the goal (it doesn't run on its own); an edit continues a
        // conversation whose goal ended here: the agent's copy is cleared.
        let carried_goal = carried_goal.filter(|_| boundary == Boundary::Restarted);
        let stale_goal = replaced
            && carried_goal.is_none()
            && (fork.is_some() || self.pane.metadata.resume_id.is_some());
        let continued = fork.is_some() || self.pane.metadata.resume_id.is_some();
        let restored = self.restored.take().filter(|_| !replaced);
        let new_epoch = restored.is_none();
        let meta = match &restored {
            // The saved session goes on: it keeps how it began.
            Some(epoch) => epoch.meta.clone(),
            None => EpochMeta {
                before: replaced.then_some(boundary),
                continued,
                agent: Some(self.agent_ref()),
                goal: carried_goal,
                fork_at: fork.clone(),
            },
        };
        let (session, lines) = match kind {
            BackendKind::Acp => {
                let mut items = restored
                    .as_ref()
                    .map(|e| e.items.clone())
                    .unwrap_or_default();
                let had_history = restored.is_some();
                if let Some(note) = self.history_note.take() {
                    // Said once, not saved: part of what the agent's session starts from.
                    let mut transcript = Transcript::restore(items);
                    transcript.push(
                        None,
                        Body::Notice(Notice {
                            kind: NoticeKind::Diagnostic,
                            text: note,
                            until: None,
                        }),
                    );
                    items = transcript.items().to_vec();
                }
                // Restoring reports every item inserted; they are saved already.
                self.history_skip = items.len();
                let checkpoint = had_history || !items.is_empty();
                let mut start = acp::Start::new(acp_cwd.expect("ACP folder")).with_host(
                    HostCapabilities::default()
                        .files(true, true)
                        .terminal(host_services::TERMINALS),
                );
                match self.pane.metadata.resume_id.clone() {
                    Some(id) => {
                        start = start.resume(id, items);
                        // Without saved history the agent's replay becomes it.
                        start.has_checkpoint = checkpoint;
                    }
                    None => start.checkpoint = items,
                }
                match AcpSession::new(start) {
                    Ok((session, lines)) => (BackendSession::from(session), lines),
                    Err(error) => {
                        self.error = Some(error);
                        cx.notify();
                        return;
                    }
                }
            }
            BackendKind::Claude | BackendKind::Codex => {
                let (mut session, mut lines) = if kind == BackendKind::Codex {
                    ChatSession::codex(
                        self.settings.clone(),
                        CodexStart {
                            cwd: self.pane.metadata.cwd.clone().unwrap_or_default(),
                            resume: self.pane.metadata.resume_id.clone(),
                            fork_at: fork.clone(),
                            images: self.settings.id.as_deref().and_then(|chat| {
                                let data =
                                    canopy_desktop::platform::directories::data_dir().ok()?;
                                Some(canopy_desktop::chat::history::attachments_dir(&data, chat))
                            }),
                            clear_goal: stale_goal,
                        },
                    )
                } else {
                    ChatSession::with_settings(self.settings.clone())
                };
                session.use_host_queue();
                session.set_base(meta.base());
                session.restore_goal(meta.goal.clone());
                if stale_goal {
                    lines.extend(session.clear_stale_goal());
                }
                if let Some(epoch) = &restored {
                    self.history_skip = epoch.items.len();
                    session.restore_history_after(epoch.items.clone(), self.item_floor);
                }
                if let Some(note) = self.history_note.take() {
                    // Said once, not saved.
                    self.history_skip = session.transcript().items().len() + 1;
                    session.note(note);
                }
                (BackendSession::from(session), lines)
            }
        };
        self.generation = next_run();
        self.closing = None;
        self.error = None;
        self.starting = true;
        if new_epoch {
            if replaced {
                self.epoch += 1;
            }
            // New sessions start at item 1 again: no unread rows to land on.
            self.item_floor = 0;
            if let Some(chat) = self.settings.id.as_ref().filter(|_| !self.history_off) {
                let epoch = self.epoch;
                app.chat_history.update(cx, |history, cx| {
                    history.record(chat, epoch, Some(meta.clone()), [], false, cx)
                });
            }
        }
        self.epoch_meta = meta;
        self.serial += 1;
        self.session = Some(session);
        if let Some(draft) = self.send_on_start.take() {
            match self.native() {
                // Sent through the queue as the new branch's first message once the
                // session is ready; given back if the start fails.
                Some(_) => self.edit_recovery = Some(draft),
                None => {
                    self.settle_edit(Err("The agent can't continue an edited message.".into()));
                    cx.emit(ChatRuntimeEvent::DraftReturned(draft))
                }
            }
        }
        self.outbox = lines;
        self.track_live(cx);
        // A saved session from before agents were recorded learns who runs it.
        self.record_meta(cx);
        // Restored history shows before the agent answers anything.
        self.after_change(cx);
        let registration = match app
            .agents
            .update(cx, |agents, cx| agents.register(self.pane.clone(), cx))
        {
            Ok(registration) => registration,
            Err(error) => {
                self.fail_start(error, cx);
                return;
            }
        };
        self.agent_run = Some(registration.run.clone());
        self.run_pane = self.pane.id;
        // A new run starts without anything reported to the notch.
        self.reported_decision = None;
        self.reported_activity = None;
        // Without hooks, the notch learns the chat is up from here.
        self.report_activity(false, cx);
        let catalog = app.tools.read(cx).catalog.clone();
        let pane = self.pane.clone();
        let settings = self.settings.clone();
        let generation = self.generation;
        // Canopy's page tools for Claude and Codex (DEC-076); a fresh token per process.
        // The registry is a global: this runs inside `Chats` updates.
        let tokens = cx
            .try_global::<crate::app_state::PageTokens>()
            .map(|tokens| tokens.0.clone());
        if let (Some(token), Some(tokens)) = (self.page_token.take(), &tokens) {
            tokens.remove(&token);
        }
        let pages = tokens
            .filter(|_| kind != BackendKind::Acp)
            .map(|tokens| tokens.register());
        self.page_token = pages.as_ref().map(|access| access.token.clone());
        self.start_task = Some(cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                    match kind {
                        BackendKind::Acp => {
                            let definition = catalog.agent_definition(&pane)?;
                            let cwd = pane.metadata.cwd.clone().unwrap_or_default();
                            let launch = prepare_acp(&definition, cwd, &env)?;
                            return Process::start(
                                launch.spec,
                                launch.files,
                                backend_session::parse_acp,
                                ACP_MAX_LINE_BYTES,
                            );
                        }
                        BackendKind::Codex => {
                            // Codex picks the conversation over the protocol; no hooks
                            // (DEC-059).
                            let mut launch = prepare_codex(&catalog, &pane, &env)?;
                            if let Some(access) = &pages {
                                canopy_desktop::chat::launch::add_page_tools(
                                    &mut launch,
                                    Provider::Codex,
                                    &executable,
                                    access,
                                )?;
                            }
                            return Process::start(
                                launch.spec,
                                launch.files,
                                backend_session::parse_codex,
                                codex_wire::MAX_LINE_BYTES,
                            );
                        }
                        BackendKind::Claude => {}
                    }
                    let helper = hook_launch::helper_path(&executable);
                    hook_launch::validate_helper(&helper)?;
                    let mut launch = prepare_claude(
                        &catalog,
                        &pane,
                        &env,
                        &settings,
                        Some(HookConfig {
                            helper: &helper,
                            registration: &registration,
                        }),
                    )?;
                    if let Some(at) = &fork {
                        fork_at(&mut launch, at);
                    }
                    if let Some(access) = &pages {
                        canopy_desktop::chat::launch::add_page_tools(
                            &mut launch,
                            Provider::Claude,
                            &executable,
                            access,
                        )?;
                    }
                    Process::start(
                        launch.spec,
                        launch.files,
                        backend_session::parse_claude,
                        canopy_desktop::chat::claude::wire::MAX_LINE_BYTES,
                    )
                })
                .await;
            let current = this
                .update(cx, |this, _| this.generation == generation)
                .unwrap_or(false);
            if !current {
                // Superseded (closed or restarted meanwhile): never adopt the process,
                // and finish closing it inside this task so shutdown can await it.
                if let Ok((process, _)) = started {
                    let process = Arc::new(process);
                    let closing = process.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move { closing.close() })
                        .await;
                    if let Err(error) = result {
                        let _ = this.update(cx, |this, cx| {
                            this.orphaned.push(process);
                            this.error = Some(error);
                            cx.notify();
                        });
                    }
                }
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.starting = false;
                match started {
                    Ok((process, events)) => {
                        if let (Some(host), Some(chat)) = (
                            cx.global::<AppState>().settings.read(cx).host_id(),
                            this.settings.id.as_deref(),
                        ) {
                            process.set_metrics_context(host, chat, generation);
                        }
                        let process = Arc::new(process);
                        for line in this.outbox.drain(..) {
                            let _ = process.send(&line);
                        }
                        this.process = Some(process);
                        this.pump = Some(this.pump(events, generation, cx));
                    }
                    Err(error) => this.fail_start(error, cx),
                }
                cx.emit(ChatStateChanged);
                cx.notify();
            });
        }));
        cx.emit(ChatStateChanged);
        cx.notify();
    }

    fn fail_start(&mut self, error: String, cx: &mut Context<Self>) {
        self.starting = false;
        self.error = Some(error.clone());
        if let Some(draft) = self.edit_recovery.take() {
            self.settle_edit(Err(format!(
                "The edited conversation couldn't start: {error}"
            )));
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
        }
        self.outbox.clear();
        // The session never started: end it so the composer stops accepting messages.
        self.end_session();
        self.after_change(cx);
        self.finish_agent(true, true, cx);
        self.track_live(cx);
        cx.emit(ChatStateChanged);
        cx.notify();
    }

    fn pump(
        &self,
        events: async_channel::Receiver<Event<BackendIncoming>>,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Ok(first) = events.recv().await {
                // Everything already waiting becomes one update and one frame.
                let mut batch = vec![first];
                while batch.len() < 512
                    && let Ok(event) = events.try_recv()
                {
                    batch.push(event);
                }
                let alive = this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return false;
                        }
                        this.apply(batch, cx);
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
    }

    fn apply(&mut self, batch: Vec<Event<BackendIncoming>>, cx: &mut Context<Self>) {
        let apply_started = std::time::Instant::now();
        let Some(session) = &mut self.session else {
            return;
        };
        let mut lines = vec![];
        let mut exited = false;
        for event in batch {
            exited |= matches!(event, Event::Exited(_));
            // A line of another protocol can't reach this session; nothing to apply.
            if let Ok(replies) = session.on_event(event) {
                lines.extend(replies);
            }
        }
        // Inspect only the new tail, retaining the last index for an initially
        // empty text block. Never drain transcript changes owned by fanout/history.
        if let Some((started, generation, from)) = self.first_text.take()
            && generation == self.generation
        {
            let items = session.transcript().items();
            let first_text = items.get(from..).unwrap_or_default().iter().any(|item| {
                item.parent.is_none()
                    && matches!(&item.body,
                    canopy_desktop::chat::transcript::Body::Text(text) if !text.text.is_empty())
            });
            if first_text {
                if let (Some(host), Some(chat)) = (
                    cx.global::<AppState>().settings.read(cx).host_id(),
                    self.settings.id.as_deref(),
                ) {
                    use canopy_desktop::remote::metrics::{Outcome, Stage, record};
                    record(
                        host,
                        chat,
                        None,
                        generation,
                        Stage::ProviderTtft,
                        started,
                        Outcome::Ok,
                    );
                }
            } else if !exited && matches!(session.status(), Status::Working | Status::NeedsDecision)
            {
                self.first_text =
                    Some((started, generation, items.len().saturating_sub(1).max(from)));
            }
        }
        self.write(lines);
        if exited {
            self.process = None;
            self.pump = None;
            // The run ended: its terminals and pending host work end with it.
            self.host_busy = 0;
            self.shell_resources.extend(self.terminals.release_all());
            let failed = matches!(self.status(), Some(Status::Failed(_)));
            self.finish_agent(failed, true, cx);
            self.track_live(cx);
            cx.emit(ChatStateChanged);
        } else {
            self.serve_host(cx);
        }
        self.after_change(cx);
        if let (Some(host), Some(chat)) = (
            cx.global::<AppState>().settings.read(cx).host_id(),
            self.settings.id.as_deref(),
        ) {
            use canopy_desktop::remote::metrics::{Outcome, Stage, record};
            record(
                host,
                chat,
                None,
                self.generation,
                Stage::Apply,
                apply_started,
                Outcome::Ok,
            );
        }
    }

    /// Ends the session as if its process exited (start failed, or it was stopped).
    fn end_session(&mut self) {
        let exit = || canopy_desktop::chat::process::Exit {
            code: None,
            signal: None,
            stderr: String::new(),
        };
        match &mut self.session {
            Some(BackendSession::Native(session)) => {
                session.close_requested();
                session.on_incoming(Event::Exited(exit()));
            }
            Some(BackendSession::Acp(session)) if !session.status().ended() => {
                session.on_event(Event::Exited(exit()));
            }
            _ => {}
        }
    }

    /// Records whether this chat's process runs or starts, for the process limit.
    fn track_live(&self, cx: &mut Context<Self>) {
        let (me, live) = (cx.entity_id(), self.is_live());
        let mut chats = cx.default_global::<LiveChats>().0.borrow_mut();
        if live {
            chats.insert(me);
        } else {
            chats.remove(&me);
        }
    }

    /// Settles what follows a session change once: saved history, the notch, the
    /// pane's choices and provider session; then tells the views which items changed.
    fn after_change(&mut self, cx: &mut Context<Self>) {
        self.settle_remote_waits();
        self.report_models(cx);
        self.settle_receipts(cx);
        self.restore_options();
        let Some(session) = &mut self.session else {
            return;
        };
        let changes = session.take_changes();
        let working = matches!(
            session.status(),
            Status::Working | Status::NeedsDecision | Status::Interrupting
        );
        // Claude and Codex choices live in the pane's chat settings.
        let (allowed, id) = (self.settings.allowed.clone(), self.settings.id.clone());
        let settings = self.native_mut().map(|session| {
            let mut settings = session.settings().clone();
            // Rules remembered for the chat live in its settings, not the session's.
            settings.allowed = allowed;
            settings.id = id;
            settings.allow(session.take_remembered());
            settings
        });
        self.save_history(&changes, cx);
        self.report_decision(cx);
        self.report_activity(working, cx);
        self.sync_goal(cx);
        if let Some(settings) = settings
            && settings != self.settings
        {
            // The CLI may change the mode itself (e.g. leaving plan mode).
            self.settings = settings;
            self.persist(None, cx);
        }
        self.save_session(cx);
        self.record_meta(cx);
        self.remember_options(cx);
        self.save_record(cx);
        self.dispatch(cx);
        self.schedule_sleep(cx);
        let status = self.navigation_status();
        let state = Some(self.state_key());
        if status != self.reported_status || state != self.reported_state {
            // The sidebar, tabs and remote clients follow the runtime, with or
            // without a view; goal accounting and option changes count too.
            self.reported_status = status;
            self.reported_state = state;
            cx.emit(ChatStateChanged);
        }
        cx.emit(ChatRuntimeEvent::Changed {
            serial: self.serial,
            changes: changes.into(),
        });
        cx.notify();
    }

    pub(super) fn write(&mut self, lines: Vec<serde_json::Value>) {
        match &self.process {
            Some(process) => {
                for line in lines {
                    let _ = process.send(&line);
                }
            }
            None => self.outbox.extend(lines),
        }
    }

    /// Runs a Claude or Codex operation: its lines go to the agent, its changes to the
    /// views. Other agents don't have it: nothing happens.
    fn act(
        &mut self,
        f: impl FnOnce(&mut ChatSession) -> Vec<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.native_mut() {
            let lines = f(session);
            self.write(lines);
            self.after_change(cx);
        }
    }

    /// Whether a view presents this chat (LIFE-09: only an unpresented one sleeps).
    fn set_presented_local(&mut self, presented: bool, cx: &mut Context<Self>) {
        if self.presented != presented {
            self.presented = presented;
            self.schedule_sleep(cx);
        }
    }

    /// Idle with nothing in flight, nobody watching, and a session the agent can resume
    /// later: its process may go (LIFE-09).
    fn can_sleep(&self) -> bool {
        !self.presented
            && self.observers == 0
            && !self.closed
            && !self.starting
            && !self.switching
            && self.process.is_some()
            && self.closing_done()
            && matches!(self.status(), Some(Status::Ready))
            && self.shells.is_empty()
            && !self.terminals.any_running()
            && self.host_busy == 0
            && self.send_on_start.is_none()
            && self.pane.metadata.resume_id.is_some()
            && self.backend().is_some_and(|session| {
                session.session_id().is_some() && session.capabilities().resume
            })
            && !self.queue_active()
            && self.native().is_none_or(|session| {
                session.background_tasks() == 0
                    && session.pending_decisions().is_empty()
                    && !session.pursuing_goal()
                    && !session.goal_hold()
            })
    }

    fn schedule_sleep(&mut self, cx: &mut Context<Self>) {
        if !self.can_sleep() {
            self.sleep_timer = None;
            return;
        }
        if self.sleep_timer.is_some() {
            return;
        }
        self.sleep_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DORMANT_AFTER).await;
            let _ = this.update(cx, |this, cx| {
                this.sleep_timer = None;
                if this.can_sleep() {
                    this.sleep(cx);
                }
            });
        }));
    }

    /// Stops the process and keeps the conversation as saved: the next Start or Send
    /// resumes the same session in the same history part.
    fn sleep(&mut self, cx: &mut Context<Self>) {
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            if stop.await.is_err() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if this.closed || this.starting || this.process.is_some() || this.error.is_some() {
                    return;
                }
                let Some(session) = this.session.take() else {
                    return;
                };
                this.restored = Some(SavedEpoch {
                    index: this.epoch,
                    meta: this.epoch_meta.clone(),
                    items: session.transcript().items().to_vec(),
                });
                cx.emit(ChatStateChanged);
                cx.notify();
            });
        })
        .detach();
    }

    fn restart_local(&mut self, cx: &mut Context<Self>) {
        if self.can_restart() {
            self.process = None;
            self.start(cx);
        }
    }

    fn finish_agent(&mut self, failed: bool, natural: bool, cx: &mut Context<Self>) {
        if let Some(run) = self.agent_run.take() {
            let pane = self.run_pane;
            cx.global::<AppState>()
                .agents
                .clone()
                .update(cx, |agents, cx| {
                    agents.finish(pane, &run, failed, natural, cx)
                });
        }
    }

    /// The chat is gone or the app quits: stop for good.
    pub fn close(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.closed = true;
        self.shutdown(cx)
    }

    /// Stops the agent process; every caller joins the same bounded close.
    fn shutdown_local(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        if let Some(closing) = &self.closing
            && closing.generation == self.generation
            && closing
                .result
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_none_or(Result::is_ok)
        {
            return wait(closing.completion.clone(), closing.result.clone(), cx);
        }
        self.generation = next_run();
        self.starting = false;
        self.stop_shells();
        cx.emit(ChatRuntimeEvent::Stopping);
        // A global, not `Chats`: closing runs inside a `Chats` update.
        if let Some(token) = self.page_token.take()
            && let Some(tokens) = cx.try_global::<crate::app_state::PageTokens>()
        {
            tokens.0.remove(&token);
        }
        self.pump = None;
        self.sleep_timer = None;
        self.host_busy = 0;
        let start = self.start_task.take();
        self.finish_agent(false, false, cx);
        if let Some(session) = self.native_mut() {
            session.close_requested();
        }
        self.shell_resources.extend(self.terminals.release_all());
        // Retain failed resources so a later cleanup can retry the same close.
        let process = self.process.clone();
        let shells: Vec<_> = self
            .shell_resources
            .iter()
            .cloned()
            .chain(self.shells.values().map(|shell| shell.running.clone()))
            .collect();
        let (finished, completion) = async_channel::bounded::<()>(1);
        let result = Arc::new(std::sync::Mutex::new(None));
        let shared = result.clone();
        let task = cx.spawn(async move |this, cx| {
            // A start in flight sees the new generation and closes its own process.
            if let Some(start) = start {
                start.await;
            }
            let mut processes = this
                .read_with(cx, |this, _| this.orphaned.clone())
                .unwrap_or_default();
            processes.extend(process);
            let closed = cx
                .background_executor()
                .spawn(async move {
                    let mut result = Ok(());
                    for process in processes {
                        let closed = process.close();
                        if result.is_ok() {
                            result = closed;
                        }
                    }
                    for shell in shells {
                        let closed = shell.close();
                        if result.is_ok() {
                            result = closed;
                        }
                    }
                    result
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if closed.is_ok() {
                    this.process = None;
                    this.orphaned.clear();
                    this.shell_resources.clear();
                }
                if this.status().is_some_and(|status| !status.ended()) {
                    this.end_session();
                }
                this.after_change(cx);
                this.track_live(cx);
                cx.emit(ChatStateChanged);
            });
            *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(closed);
            drop(finished);
        });
        self.closing = Some(Closing {
            generation: self.generation,
            _task: task,
            completion: completion.clone(),
            result: result.clone(),
        });
        cx.emit(ChatStateChanged);
        cx.notify();
        wait(completion, result, cx)
    }

    /// Switches the account (Claude or Codex) before the first message: a new process
    /// for the new profile, with `model` chosen (`None`: the account's default).
    fn set_account_local(
        &mut self,
        tool: String,
        profile: String,
        model: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.closed
            || self.switching
            || self.account_locked()
            || backend_of(&tool, cx).is_none()
            || (self.pane.metadata.profile_id.as_deref() == Some(&profile)
                && self.pane.tool == tool)
        {
            return;
        }
        self.switching = true;
        let provider = Provider::from_tool(&tool);
        if self.pane.tool != tool {
            // The same intent in the other agent's modes; efforts differ per model.
            match provider {
                Some(Provider::Claude) => self.settings.mode = self.settings.mode.for_claude(),
                Some(Provider::Codex) => self.settings.mode = self.settings.mode.for_codex(),
                // Other agents offer their own options once started.
                None => {}
            }
            self.settings.effort = None;
            self.pane.tool = tool.clone();
        }
        self.pane.metadata.profile_id = Some(profile.clone());
        // Models belong to the account: the one picked from its list, else its default.
        self.settings.model = model.filter(|_| provider.is_some());
        self.persist(Some((tool, profile)), cx);
        if provider.is_some() {
            self.remember(cx);
        }
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            let _ = stop.await;
            let _ = this.update(cx, |this, cx| {
                this.switching = false;
                if this.closed {
                    return;
                }
                this.session = None;
                this.archived.clear();
                cx.emit(ChatRuntimeEvent::Reset);
                this.start(cx);
            });
        })
        .detach();
    }

    /// Replaces the session with a branch that continues after `fork` (`None`: a new
    /// conversation) and sends `draft` there first. The draft comes back when the
    /// branch can't start.
    pub fn restart_edited(&mut self, fork: Option<String>, draft: Draft, cx: &mut Context<Self>) {
        // The branch continues from before the goal's end: the goal ends with the edit.
        if let Some(session) = self.native_mut().filter(|s| s.goal().is_some()) {
            session.drop_goal(EndReason::ClearedOnEdit);
            self.after_change(cx);
        }
        self.switching = true;
        self.fork_at = fork.clone();
        self.send_on_start = Some(draft);
        if fork.is_none() {
            // The first message itself changed: a new conversation.
            let pane = self.pane.id;
            self.pane.metadata.resume_id = None;
            cx.global::<AppState>()
                .projects
                .clone()
                .update(cx, |projects, cx| projects.forget_chat_session(pane, cx));
        }
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            let _ = stop.await;
            let _ = this.update(cx, |this, cx| {
                this.switching = false;
                if !this.closed {
                    this.boundary = Boundary::Edited;
                    this.start(cx);
                }
                // Not started (closing, already starting): nothing was sent.
                if let Some(draft) = this.send_on_start.take() {
                    this.fork_at = None;
                    this.boundary = Boundary::Restarted;
                    this.settle_edit(Err("The chat couldn't restart for the edit.".into()));
                    cx.emit(ChatRuntimeEvent::DraftReturned(draft));
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    // Conversation.

    /// The session can take messages now.
    /// A dormant chat accepts them too: they wait in the host queue, which starts it.
    fn accepts_messages_local(&self) -> bool {
        self.recovery.is_none()
            && (self.dormant() || self.session.as_ref().is_some_and(|s| !s.status().ended()))
    }
    /// Writes `draft` into the running turn (CHAT-03). It is accepted like any message
    /// first (durable ID, receipts) and goes first in line as a steer once `attempting`
    /// is recorded. With no turn to steer it is an ordinary message.
    fn steer_draft_local(&mut self, draft: Draft, cx: &mut Context<Self>) {
        if self
            .session
            .as_ref()
            .is_none_or(BackendSession::ready_for_message)
        {
            self.send_draft(draft, cx);
            return;
        }
        if !self.capabilities(cx).steer {
            // Queuing it instead is the user's choice, not a hidden Send (AGT-07).
            self.notice = Some(ChatNotice::info(format!(
                "{} can't take a message during its turn. Send queues it instead.",
                self.agent_name(cx)
            )));
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            cx.notify();
            return;
        }
        self.send_draft_as(draft, true, cx);
    }
    /// Stop: decisions are refused, the turn ends and the queue is held (CHAT-04).
    fn interrupt_local(&mut self, cx: &mut Context<Self>) {
        if let Err((reason, _)) = self.interrupt_checked(cx) {
            self.notice = Some(ChatNotice::error(reason));
            cx.notify();
        }
    }
    /// `interrupt`, saying whether the agent's input took it.
    pub(super) fn interrupt_checked(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), (String, bool)> {
        let Some(session) = &mut self.session else {
            return Ok(());
        };
        let working = matches!(
            session.status(),
            Status::Working | Status::NeedsDecision | Status::Interrupting
        ) || self
            .native()
            .is_some_and(|s| s.goal_hold() || s.background_tasks() > 0);
        let lines = self.session.as_mut().expect("session").interrupt();
        let written = self.write_dispatched(lines);
        if working {
            self.hold_queue(cx);
        }
        self.after_change(cx);
        written
    }
    fn respond_local(&mut self, item: ItemId, response: Response, cx: &mut Context<Self>) {
        self.act(|session| session.respond(item, response), cx);
    }
    /// Answers an ACP agent's request with the option it offered (`None`: cancel).
    fn respond_option_local(
        &mut self,
        item: ItemId,
        option: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &mut self.session else {
            return;
        };
        match session.respond(item, BackendDecision::OptionId(option)) {
            Ok(lines) => self.write(lines),
            Err(error) => self.notice = Some(ChatNotice::error(error)),
        }
        self.after_change(cx);
    }
    /// Chooses `value` for the agent's option `id`, as the agent offered them.
    fn set_agent_option_local(&mut self, id: &str, value: &str, cx: &mut Context<Self>) {
        let Some(BackendSession::Acp(session)) = &mut self.session else {
            return;
        };
        // The user's choice wins over one still being restored.
        self.options_to_restore.retain(|(option, _)| option != id);
        let sent = set_acp_option(session, id, value);
        match sent {
            Ok(lines) => self.write(lines),
            Err(error) => self.notice = Some(ChatNotice::error(error)),
        }
        self.after_change(cx);
    }
    /// Offers the chat's saved ACP choices to a ready session, one change at a time;
    /// values the session doesn't offer are skipped, refusals keep the agent's value.
    fn restore_options(&mut self) {
        let serial = self.serial;
        let Some(BackendSession::Acp(session)) = &self.session else {
            return;
        };
        if *session.status() != Status::Ready {
            return;
        }
        if self.options_restored != serial {
            self.options_restored = serial;
            let current = self.agent_options();
            self.options_to_restore = self
                .settings
                .agent_options
                .iter()
                .filter(|(id, value)| {
                    current.iter().any(|option| {
                        &option.id == *id
                            && option.value != **value
                            && option.choices.iter().any(|choice| &choice.id == *value)
                    })
                })
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect();
        }
        let Some(BackendSession::Acp(session)) = &mut self.session else {
            return;
        };
        if session.config_pending() || self.options_to_restore.is_empty() {
            return;
        }
        let (id, value) = self.options_to_restore.remove(0);
        if let Ok(lines) = set_acp_option(session, &id, &value) {
            self.write(lines);
        }
    }
    /// Keeps the ACP choices the agent confirmed with the chat, once restoring ended.
    fn remember_options(&mut self, cx: &mut Context<Self>) {
        let Some(BackendSession::Acp(session)) = &self.session else {
            return;
        };
        if self.options_restored != self.serial
            || !self.options_to_restore.is_empty()
            || session.config_pending()
        {
            return;
        }
        let chosen: std::collections::BTreeMap<_, _> = self
            .agent_options()
            .into_iter()
            .filter(|option| !option.value.is_empty())
            .map(|option| (option.id, option.value))
            .collect();
        if chosen != self.settings.agent_options {
            self.settings.agent_options = chosen;
            self.persist(None, cx);
        }
    }
    /// Something the chat couldn't do just now (a message refused, an option kept),
    /// or why something waits.
    pub fn notice(&self) -> Option<&ChatNotice> {
        self.notice.as_ref()
    }
    /// Says `notice` under the composer, in place of any other.
    pub fn show_notice(&mut self, notice: ChatNotice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        cx.notify();
    }
    /// The user closed the notice.
    pub fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        if self.notice.take().is_some() {
            cx.notify();
        }
    }
    fn stop_background_local(&mut self, call: &str, cx: &mut Context<Self>) {
        self.act(|session| session.stop_background(call), cx);
    }
    /// Said in the conversation, not to the agent.
    pub fn note(&mut self, text: String, cx: &mut Context<Self>) {
        match self.native_mut() {
            Some(session) => session.note(text),
            None => self.notice = Some(ChatNotice::info(text)),
        }
        self.after_change(cx);
    }
    /// Asks what restoring files to before `item` would change (`dry_run`), or restores
    /// them. False when the session can't now.
    pub fn rewind_files(&mut self, item: ItemId, dry_run: bool, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.native_mut() else {
            return false;
        };
        let lines = session.rewind_files(item, dry_run);
        let asked = !lines.is_empty();
        self.write(lines);
        cx.notify();
        asked
    }
    fn pause_goal_local(&mut self, cx: &mut Context<Self>) {
        self.act(|session| session.pause_goal(), cx);
    }
    fn resume_goal_local(&mut self, cx: &mut Context<Self>) {
        self.act(|session| session.resume_goal(), cx);
    }
    fn clear_goal_local(&mut self, cx: &mut Context<Self>) {
        self.act(|session| session.clear_goal(EndReason::ClearedByUser), cx);
    }

    // Choices.

    /// Forgets a rule remembered for this chat. The running process keeps it until Restart:
    /// the CLI cannot drop a session rule.
    pub fn remove_rule(&mut self, rule: &str, cx: &mut Context<Self>) {
        self.settings.allowed.retain(|allowed| allowed != rule);
        self.persist(None, cx);
        cx.notify();
    }
    fn set_model_local(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        self.change(|session| session.set_model(model), cx);
    }
    fn set_effort_local(&mut self, effort: Option<Effort>, cx: &mut Context<Self>) {
        self.change(|session| session.set_effort(effort), cx);
    }
    fn set_mode_local(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.change(|session| session.set_mode(mode), cx);
    }
    fn change(
        &mut self,
        f: impl FnOnce(&mut ChatSession) -> Vec<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.native_mut() else {
            return;
        };
        let lines = f(session);
        let settings = session.settings().clone();
        self.write(lines);
        if settings != self.settings {
            self.settings = settings;
            self.persist(None, cx);
            self.remember(cx);
        }
        self.after_change(cx);
    }

    fn persist_local(&mut self, account: Option<(String, String)>, cx: &mut Context<Self>) {
        let pane = self.pane.id;
        let settings = self.settings.clone();
        cx.global::<AppState>()
            .projects
            .clone()
            .update(cx, |projects, cx| {
                if let Some((tool, profile)) = account {
                    projects.set_chat_account(pane, &tool, profile, cx);
                }
                projects.update_chat(pane, Some(settings), None, cx)
            });
    }

    /// The latest choices become the defaults of the next new chat.
    fn remember(&self, cx: &mut Context<Self>) {
        use canopy_desktop::settings::{Change, PreferenceKey};
        let mut changes = vec![
            match &self.settings.model {
                Some(model) => Change::ChatModel(model.clone()),
                None => Change::Reset(PreferenceKey::ChatModel),
            },
            match self.settings.effort {
                Some(effort) => Change::ChatEffort(effort),
                None => Change::Reset(PreferenceKey::ChatEffort),
            },
        ];
        // Yolo applies only to the chat that chose it, never to the next new chat.
        if !self.settings.mode.unguarded() {
            changes.push(Change::ChatMode(self.settings.mode));
        }
        if let Some(profile) = &self.pane.metadata.profile_id {
            changes.push(Change::ChatProfile(profile.clone()));
        }
        cx.global::<AppState>()
            .settings
            .clone()
            .update(cx, |settings, cx| settings.remember(changes, cx));
    }

    // `!` commands.

    /// Runs `command` in the chat's folder; its card shows in the conversation and its
    /// result goes to the agent when it ends.
    pub fn run_shell(&mut self, command: &str, cx: &mut Context<Self>) -> Result<ItemId, String> {
        let command = command.trim();
        if command.is_empty() {
            return Err("Type a command to run.".into());
        }
        let cwd = self
            .cwd()
            .ok_or("This chat has no folder to run commands in.")?;
        let env = cx
            .global::<AppState>()
            .tools
            .read(cx)
            .environment
            .clone()
            .ok_or("The shell environment is still loading.")??;
        if self.session.is_some() && self.native().is_none() {
            return Err(format!(
                "{} doesn't take `!` command results.",
                self.agent_name(cx)
            ));
        }
        let session = self
            .native_mut()
            .filter(|session| !session.status().ended())
            .ok_or("The agent isn't running.")?;
        let (running, events) = Running::start(command, &cwd, &env.shell, &env.vars)?;
        let id = session.begin_shell(command);
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let ended = matches!(event, ShellEvent::Exit { .. });
                let alive = this
                    .update(cx, |this, cx| this.on_shell(id, event, cx))
                    .is_ok();
                if ended || !alive {
                    break;
                }
            }
        });
        self.shells.insert(
            id,
            Shell {
                running,
                _pump: pump,
            },
        );
        self.after_change(cx);
        Ok(id)
    }

    fn on_shell(&mut self, id: ItemId, event: ShellEvent, cx: &mut Context<Self>) {
        let Some(session) = self.native_mut() else {
            return;
        };
        match event {
            ShellEvent::Output(chunk) => session.shell_output(id, &chunk),
            ShellEvent::Exit { code, elapsed } => {
                let stopped = self.stopped_shells.remove(&id);
                if let Some(shell) = self.shells.remove(&id) {
                    // Exit describes the command, not proof that its entire group
                    // released cwd. Keep the handle for deletion/quit as well.
                    self.shell_resources.push(shell.running);
                }
                let session = self.native_mut().expect("session");
                let result = session.finish_shell(
                    id,
                    code,
                    stopped,
                    elapsed.as_millis().min(u64::MAX as u128) as u64,
                );
                // The result is a message like any other: accepted, then delivered.
                if let Some(draft) = result {
                    self.send_draft(draft, cx);
                }
            }
        }
        self.after_change(cx);
    }

    /// Stop on a running command's card: it ends, and the agent hears it was stopped.
    fn stop_shell_local(&mut self, id: ItemId, cx: &mut Context<Self>) {
        if let Some(shell) = self.shells.get(&id) {
            self.stopped_shells.insert(id);
            shell.running.stop();
            cx.notify();
        }
    }

    /// The chat closes or its agent restarts: running commands end with it.
    fn stop_shells(&mut self) {
        for (id, shell) in &self.shells {
            self.stopped_shells.insert(*id);
            shell.running.stop();
        }
    }

    // Reports outside the chat.

    /// Hands changed items to the chat's saved history.
    fn save_history(&mut self, changes: &[Change], cx: &mut Context<Self>) {
        let skip = std::mem::take(&mut self.history_skip);
        let (Some(chat), Some(session)) = (&self.settings.id, &self.session) else {
            return;
        };
        if self.history_off {
            return;
        }
        let transcript = session.transcript();
        let items: Vec<_> = changes
            .iter()
            .filter_map(|change| match *change {
                Change::Inserted(index) if index < skip => None,
                Change::Inserted(index) | Change::Updated(index) => transcript.get(index).cloned(),
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let urgent = items
            .iter()
            .any(|item| matches!(item.body, Body::TurnEnd(_)));
        let epoch = self.epoch;
        cx.global::<AppState>()
            .chat_history
            .clone()
            .update(cx, |history, cx| {
                history.record(chat, epoch, None, items, urgent, cx)
            });
    }

    /// Records the provider session for resume once the session has a message; until
    /// then the CLI has not written it, and the account may still change.
    fn save_session(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .session
            .as_ref()
            .filter(|session| {
                // An agent that can't load a session starts a new one at each start.
                session.capabilities().resume
                    && session
                        .transcript()
                        .items()
                        .iter()
                        .any(|item| matches!(item.body, Body::User(_)))
            })
            .and_then(BackendSession::session_id)
            .map(str::to_owned)
        else {
            return;
        };
        if self.pane.metadata.resume_id.as_deref() == Some(id.as_str()) {
            return;
        }
        let pane = self.pane.clone();
        cx.global::<AppState>()
            .projects
            .clone()
            .update(cx, |projects, cx| {
                projects.record_agent_session(&pane, &id, cx)
            });
        self.pane.metadata.resume_id = Some(id);
        // Branched: the new session is the one to go on in (`record_meta` saves it).
        self.fork_at = None;
        self.epoch_meta.fork_at = None;
    }

    /// Saves the running session's agent again once its session id or choices change.
    fn record_meta(&mut self, cx: &mut Context<Self>) {
        if self.history_off
            || self
                .session
                .as_ref()
                .is_none_or(|session| session.status().ended())
        {
            return;
        }
        let agent = Some(self.agent_ref());
        let goal = self.native().and_then(|s| s.goal()).cloned();
        if self.epoch_meta.agent == agent && self.epoch_meta.goal == goal {
            return;
        }
        self.epoch_meta.agent = agent;
        self.epoch_meta.goal = goal;
        let Some(chat) = self.settings.id.clone() else {
            return;
        };
        let (epoch, meta) = (self.epoch, self.epoch_meta.clone());
        cx.global::<AppState>()
            .chat_history
            .clone()
            .update(cx, |history, cx| {
                history.record(&chat, epoch, Some(meta), [], false, cx)
            });
    }

    /// The account's models as its agent lists them, for every picker (DEC-098).
    fn report_models(&self, cx: &mut Context<Self>) {
        let Some(models) = self.native().map(|s| s.models()) else {
            return;
        };
        let catalog = cx.global::<AppState>().model_catalog.clone();
        let profile = self.profile(cx);
        if models.is_empty() || catalog.read(cx).models(&profile) == Some(models) {
            return;
        }
        let models = models.to_vec();
        catalog.update(cx, |catalog, cx| catalog.set_models(&profile, models, cx));
    }

    /// Codex has no hooks in chat: its working/idle state goes to the notch from here.
    fn report_activity(&mut self, working: bool, cx: &mut Context<Self>) {
        // Claude reports through its hooks.
        if self.provider() == Some(Provider::Claude) || self.reported_activity == Some(working) {
            return;
        }
        let Some(run) = self.agent_run.clone() else {
            return;
        };
        self.reported_activity = Some(working);
        let pane = self.run_pane;
        cx.global::<AppState>()
            .agents
            .clone()
            .update(cx, |agents, cx| {
                agents.chat_activity(pane, &run, working, cx)
            });
    }

    /// Tells the notch what this chat waits for (its oldest request), so a chat asks
    /// for attention without any view.
    fn report_decision(&mut self, cx: &mut Context<Self>) {
        let agent = self.agent_name(cx);
        let decision = self.session.as_ref().and_then(|session| {
            let item = *self.pending_decisions().first()?;
            let permission = session
                .transcript()
                .items()
                .iter()
                .find_map(|i| match &i.body {
                    Body::Permission(p) if i.id == item => Some(p),
                    _ => None,
                })?;
            Some(match permission.kind {
                PermissionKind::Question => match presentation::questions(permission).first() {
                    Some(question) => format!("Question: {}", question.question),
                    None => format!("{agent} has a question"),
                },
                PermissionKind::Plan => "Plan ready for approval".to_owned(),
                PermissionKind::Tool => format!("Permission: {}", permission.tool),
            })
        });
        if decision == self.reported_decision {
            return;
        }
        let (Some(run), pane) = (self.agent_run.clone(), self.run_pane) else {
            return;
        };
        self.reported_decision = decision.clone();
        cx.global::<AppState>()
            .agents
            .clone()
            .update(cx, |agents, cx| {
                agents.set_decision(pane, &run, decision, cx)
            });
    }

    /// Mirrors the goal outside the chat: the notch shows "Goal" while it is pursued
    /// (and one alert at its end); Codex's wait for its next goal turn is bounded; a
    /// finished Claude goal is checked against its session's record.
    fn sync_goal(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.native() else {
            return;
        };
        let working = self.is_working();
        // The goal counts until the chat stops working after it.
        let pursuing = session.pursuing_goal() || (self.reported_goal == Some(true) && working);
        if self.reported_goal != Some(pursuing)
            && let Some(run) = self.agent_run.clone()
        {
            if self.reported_goal.is_some() || pursuing {
                let pane = self.run_pane;
                cx.global::<AppState>()
                    .agents
                    .clone()
                    .update(cx, |agents, cx| {
                        agents.chat_goal(pane, &run, pursuing, working, cx)
                    });
            }
            self.reported_goal = Some(pursuing);
        }
        let session = self.native().expect("session");
        if !session.goal_hold() {
            self.goal_hold_timer = None;
        } else if self.goal_hold_timer.is_none() {
            self.goal_hold_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(GOAL_HOLD).await;
                let _ = this.update(cx, |this, cx| {
                    this.goal_hold_timer = None;
                    if let Some(session) = this.native_mut() {
                        let lines = session.expire_goal_hold();
                        this.write(lines);
                    }
                    this.after_change(cx);
                });
            }));
        }
        self.check_goal_records(cx);
    }

    /// Claude's stream doesn't say whether a goal was met or judged impossible: its
    /// session file does. Read in the background; the card is corrected if it differs.
    fn check_goal_records(&mut self, cx: &mut Context<Self>) {
        if self.provider() != Some(Provider::Claude) {
            return;
        }
        let Some(session) = self.native() else {
            return;
        };
        let (Some(id), Some(cwd)) = (
            session.session_id().map(str::to_owned),
            self.pane.metadata.cwd.clone(),
        ) else {
            return;
        };
        let ends: Vec<_> = session
            .transcript()
            .items()
            .iter()
            .rev()
            .take(32)
            .filter_map(|item| match &item.body {
                Body::GoalEnd(end) if end.reason == EndReason::Achieved => {
                    Some((item.id, end.objective.clone()))
                }
                _ => None,
            })
            .filter(|(item, _)| !self.goal_records.contains(item))
            .collect();
        for (item, objective) in ends {
            self.goal_records.insert(item);
            let (id, cwd) = (id.clone(), cwd.clone());
            let read = cx.background_spawn(async move {
                canopy_desktop::chat::claude::goal_record::read(
                    &canopy_desktop::chat::claude::goal_record::config_dirs(),
                    &cwd,
                    &id,
                    &objective,
                )
            });
            cx.spawn(async move |this, cx| {
                let Some(record) = read.await else {
                    return;
                };
                let _ = this.update(cx, |this, cx| {
                    let reason = if record.met {
                        EndReason::Achieved
                    } else {
                        EndReason::Impossible
                    };
                    let note = (!record.met).then_some(record.reason).flatten();
                    if let Some(session) = this.native_mut() {
                        session.amend_goal_end(item, reason, note, record.seconds, record.tokens);
                    }
                    this.after_change(cx);
                });
            })
            .detach();
        }
    }
}

/// What Claude and Codex chats can do (rewind is Claude's).
fn set_acp_option(
    session: &mut AcpSession,
    id: &str,
    value: &str,
) -> Result<Vec<serde_json::Value>, String> {
    match id {
        ACP_MODEL => session.set_model(value),
        ACP_MODE => session.set_mode(value),
        _ => session.set_option(id, value),
    }
}

fn native_capabilities(claude: bool) -> Capabilities {
    let mut capabilities = Capabilities::default();
    capabilities.resume = true;
    capabilities.images = true;
    capabilities.embedded_context = true;
    capabilities.cancel = true;
    capabilities.steer = true;
    capabilities.fork = true;
    capabilities.rewind = claude;
    capabilities.goal = true;
    capabilities
}

/// Ids of the options an ACP agent offers outside `configOptions` (`session/set_model`,
/// `session/set_mode`); they can't collide with its own, which it names itself.
const ACP_MODEL: &str = "canopy:model";
const ACP_MODE: &str = "canopy:mode";

/// One of an agent's own options as the chat shows it: its id and values are the
/// agent's, never reinterpreted (AGT-04, AGT-11).
#[derive(Clone, Debug, PartialEq)]
pub struct AgentOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub value: String,
    pub choices: Vec<OptionChoice>,
    /// When a choice takes effect: at once, with the next turn, or after Restart.
    pub effect: OptionEffect,
}
impl From<&SessionOption> for AgentOption {
    fn from(option: &SessionOption) -> Self {
        Self {
            id: option.id.clone(),
            label: option.label.clone(),
            description: option.description.clone(),
            category: option.category.clone(),
            value: option.value.clone(),
            choices: option.choices.clone(),
            effect: option.effect,
        }
    }
}
fn legacy_option(
    id: &str,
    label: &str,
    category: &str,
    value: Option<&str>,
    choices: &[OptionChoice],
) -> AgentOption {
    AgentOption {
        id: id.into(),
        label: label.into(),
        description: None,
        category: Some(category.into()),
        value: value.unwrap_or_default().into(),
        choices: choices.to_vec(),
        effect: OptionEffect::Immediate,
    }
}

fn wait(
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    cx: &mut Context<ChatRuntime>,
) -> Task<Result<(), String>> {
    cx.spawn(async move |_, _| {
        let _ = completion.recv().await;
        result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| Err("Chat cleanup result was lost.".into()))
    })
}

#[cfg(test)]
mod notice_tests {
    use super::ChatNotice;
    use core::prelude::v1::test;

    #[test]
    fn a_sent_message_clears_explanations_but_not_errors() {
        let error = ChatNotice::error("Couldn't save this chat: database is locked");
        assert_eq!(
            ChatNotice::after_send(Some(error.clone())),
            Some(error),
            "an error stays until dismissed or replaced"
        );
        assert_eq!(
            ChatNotice::after_send(Some(ChatNotice::info("It waits in the queue."))),
            None
        );
    }
}
