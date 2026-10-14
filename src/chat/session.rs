//! Session controller: owns the transcript and decides what to write to the agent.
//! No I/O — the owner forwards process events in and writes the returned lines out.
use super::provider::Provider;
use super::{
    claude::{
        adapter::{Adapter, CliState, InitInfo, Signal},
        wire::{self, ContextUsage, Inbound, Lifecycle, Priority},
    },
    config::{ChatSettings, Effort, Mode, ModelInfo, valid_rule},
    goal::{EndReason, Goal},
    presentation,
    process::{Event, Exit},
    transcript::{
        Attachment, Body, Decision, Delivery, Item, ItemId, Notice, NoticeKind, Outcome,
        PermissionKind, Remember, Resolution, Rule, Transcript, TurnEnd, TurnId, UserMessage,
    },
};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

mod codex_engine;
mod goal_engine;
pub use codex_engine::CodexStart;

/// A line from either agent's process.
#[derive(Debug)]
pub enum Incoming {
    Claude(Inbound),
    Codex(super::codex::wire::Message),
}
pub fn parse_claude(line: &[u8]) -> Result<Option<Incoming>, String> {
    Ok(wire::parse_line(line)?.map(Incoming::Claude))
}
pub fn parse_codex(line: &[u8]) -> Result<Option<Incoming>, String> {
    Ok(super::codex::wire::parse_line(line)?.map(Incoming::Codex))
}

const DIAGNOSTICS: usize = 50;
pub const DENIED_MESSAGE: &str = "Denied by the user in Canopy.";

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Starting,
    Ready,
    Working,
    NeedsDecision,
    Interrupting,
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
    Failed(String),
}
impl Status {
    pub fn ended(&self) -> bool {
        matches!(self, Self::Exited { .. } | Self::Failed(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QueuedId(pub u64);

/// The user's answer to a pending request: a permission, questions or a plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    Allow,
    /// Allow and remember the request's `remember` for the rest of the chat.
    AllowForChat,
    /// Deny, optionally telling the agent what to do instead.
    Deny(Option<String>),
    /// One answer per question, in the order asked; empty ones are left out.
    Answer(Vec<String>),
    ApprovePlan {
        accept_edits: bool,
    },
    KeepPlanning(Option<String>),
}

fn clean(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Restoring files from before a user message (the CLI's file checkpoints).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rewind {
    Preview(wire::RewindPreview),
    /// The dry run failed: files can't be offered for restoring.
    Unavailable(String),
    Done,
    Failed(String),
}

/// What came before this session's first message, for editing it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Base {
    /// A new conversation: editing the first message starts over.
    #[default]
    Fresh,
    /// A resumed or forked conversation: its first message continues history this
    /// session doesn't hold, so it can't be edited.
    Continued,
}

/// What Resume sends after Stop, so the agent finishes the interrupted work before the
/// queue continues.
pub const RESUME_PROMPT: &str = "Continue where you left off.";

/// A message as written in the composer: text and what goes along with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub text: String,
    pub attachments: Vec<Attachment>,
    /// Context sent before the text; only messages saved from an agent switch (withdrawn,
    /// DEC-087) carry one, and Retry or Edit sends it again.
    pub context: Option<String>,
    /// Sets the chat's goal: the text is its condition.
    pub goal: bool,
}
impl Draft {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
    fn trimmed(self) -> Option<Self> {
        let text = self.text.trim().to_owned();
        (!text.is_empty() || !self.attachments.is_empty()).then_some(Self {
            text,
            attachments: self.attachments,
            context: self.context,
            goal: self.goal,
        })
    }
}

/// A message held in Canopy until the agent finishes, or being steered into its turn.
#[derive(Clone, Debug, PartialEq)]
pub struct Queued {
    pub id: QueuedId,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub context: Option<String>,
    /// Uuid of the Steer already written to the CLI; it can no longer be edited.
    pub steering: Option<String>,
    /// Sets the chat's goal once sent (never steered).
    pub goal: bool,
}

pub struct ChatSession {
    transcript: Transcript,
    adapter: Adapter,
    status: Status,
    info: InitInfo,
    initialize_id: String,
    interrupt_id: Option<String>,
    initialized: bool,
    /// Lines written only after the CLI acknowledged `initialize`.
    held: Vec<Value>,
    next_request: u64,
    interrupting: bool,
    /// The CLI is handling a turn (from start until its `result`).
    turn_open: bool,
    /// The owner asked the process to stop; its exit status is then expected.
    closing: bool,
    background_tasks: usize,
    diagnostics: VecDeque<String>,
    /// What the agent runs with; changes apply from the next turn.
    settings: ChatSettings,
    models: Vec<ModelInfo>,
    /// Commands and skills the agent offers (`/` in the composer).
    commands: Vec<crate::chat::config::CommandInfo>,
    /// In-flight setting changes and the value to restore if the CLI refuses them.
    pending_settings: HashMap<String, Previous>,
    /// Context window use after the last turn, and the request asking for it.
    context: Option<ContextUsage>,
    context_request: Option<String>,
    /// File restores asked for (request id → item, dry run) and the latest outcome.
    rewinds: HashMap<String, (ItemId, bool)>,
    rewind: Option<(ItemId, Rewind)>,
    /// `stop_task` requests in flight, to report a refusal.
    task_stops: HashSet<String>,
    base: Base,
    /// Oldest first. Sent one at a time once the agent has finished everything.
    queue: VecDeque<Queued>,
    next_queued: u64,
    /// Stop holds the queue until the user sends or steers again.
    hold_queue: bool,
    /// Stop cut work short and nothing was sent since: Resume first asks to finish it.
    stopped: bool,
    /// Rules remembered for the chat, not yet taken by the view.
    remembered: Vec<String>,
    /// Our message uuids the CLI has not finished with (`command_lifecycle`).
    outstanding: HashSet<String>,
    lifecycle_seen: bool,
    /// The CLI reported idle: no turn, held result or background agent is running.
    cli_idle: bool,
    provider: Provider,
    /// Codex protocol state (unused for Claude).
    codex: codex_engine::CodexEngine,
    /// The chat's goal (`/goal`) while it is set.
    goal: Option<Goal>,
    /// The goal changed since the owner last saved it.
    goal_changed: bool,
    /// Why the goal's next removal happens, when not the user's Clear.
    goal_clearing: Option<EndReason>,
    /// Claude finished a goal turn on its own: the status asked after it says whether
    /// the goal ended.
    goal_settling: bool,
    /// Codex: the goal continues after the turn that ended; the session stays working
    /// until Codex starts the next one (or the owner lets it go, `expire_goal_hold`).
    goal_hold: bool,
    /// Commands Canopy sends on its own (Claude `/goal …`), oldest first; they go before
    /// the queue and leave no message in the conversation.
    quiet: VecDeque<String>,
    quiet_turns: HashSet<TurnId>,
}

#[derive(Clone, Debug)]
enum Previous {
    Model(Option<String>),
    Effort(Option<Effort>),
    Mode(Mode),
}
impl Previous {
    fn noun(&self) -> &'static str {
        match self {
            Self::Model(_) => "model",
            Self::Effort(_) => "reasoning effort",
            Self::Mode(_) => "work mode",
        }
    }
}

impl Queued {
    fn draft(self) -> Draft {
        Draft {
            text: self.text,
            attachments: self.attachments,
            context: self.context,
            goal: self.goal,
        }
    }
}

impl ChatSession {
    /// Returns the session and the lines to write immediately.
    pub fn new() -> (Self, Vec<Value>) {
        Self::with_settings(ChatSettings::default())
    }

    /// `settings` must match what the process was launched with.
    pub fn with_settings(settings: ChatSettings) -> (Self, Vec<Value>) {
        let mut session = Self::base(settings, Provider::Claude);
        session.initialize_id = session.request_id();
        let init = wire::initialize(&session.initialize_id);
        (session, vec![init])
    }

    fn base(settings: ChatSettings, provider: Provider) -> Self {
        Self {
            transcript: Transcript::default(),
            adapter: Adapter::default(),
            status: Status::Starting,
            info: InitInfo::default(),
            initialize_id: String::new(),
            interrupt_id: None,
            initialized: false,
            held: vec![],
            next_request: 0,
            interrupting: false,
            turn_open: false,
            closing: false,
            background_tasks: 0,
            diagnostics: VecDeque::new(),
            settings,
            models: vec![],
            commands: vec![],
            pending_settings: HashMap::new(),
            context: None,
            context_request: None,
            rewinds: HashMap::new(),
            rewind: None,
            task_stops: HashSet::new(),
            base: Base::Fresh,
            queue: VecDeque::new(),
            next_queued: 0,
            hold_queue: false,
            stopped: false,
            remembered: vec![],
            outstanding: HashSet::new(),
            lifecycle_seen: false,
            cli_idle: true,
            provider,
            codex: Default::default(),
            goal: None,
            goal_changed: false,
            goal_clearing: None,
            goal_settling: false,
            goal_hold: false,
            quiet: VecDeque::new(),
            quiet_turns: HashSet::new(),
        }
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// Requests waiting for the user, as the provider names them, oldest first.
    fn pending_requests(&self) -> Vec<String> {
        match self.provider {
            Provider::Claude => self.adapter.pending_permissions(),
            Provider::Codex => self.codex.adapter.pending_permissions(),
        }
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }
    /// The conversation so far, handed over when this session is replaced.
    pub fn take_transcript(&mut self) -> Transcript {
        std::mem::take(&mut self.transcript)
    }
    /// A note from Canopy itself in the conversation (e.g. history it couldn't read).
    pub fn note(&mut self, text: String) {
        self.transcript.push(
            None,
            Body::Notice(Notice {
                kind: NoticeKind::Diagnostic,
                text,
                until: None,
            }),
        );
    }
    pub fn take_changes(&mut self) -> Vec<super::transcript::Change> {
        self.transcript.take_changes()
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn info(&self) -> &InitInfo {
        &self.info
    }
    pub fn session_id(&self) -> Option<&str> {
        self.info.session_id.as_deref()
    }
    pub fn settings(&self) -> &ChatSettings {
        &self.settings
    }
    /// Models the account offers; empty until the CLI answered `initialize`.
    pub fn models(&self) -> &[ModelInfo] {
        &self.models
    }
    pub fn commands(&self) -> &[crate::chat::config::CommandInfo] {
        &self.commands
    }

    pub fn set_model(&mut self, model: Option<String>) -> Vec<Value> {
        if self.status.ended() || self.settings.model == model {
            return vec![];
        }
        if self.provider == Provider::Codex {
            // Codex takes it with the next turn.
            self.settings.model = model;
            return vec![];
        }
        let previous = Previous::Model(std::mem::replace(&mut self.settings.model, model));
        let line = wire::set_model(&self.track(previous), self.settings.model.as_deref());
        self.outgoing(vec![line])
    }
    pub fn set_effort(&mut self, effort: Option<Effort>) -> Vec<Value> {
        if self.status.ended() || self.settings.effort == effort {
            return vec![];
        }
        if self.provider == Provider::Codex {
            self.settings.effort = effort;
            return vec![];
        }
        let previous = Previous::Effort(std::mem::replace(&mut self.settings.effort, effort));
        let line = wire::set_effort(&self.track(previous), effort);
        self.outgoing(vec![line])
    }
    pub fn set_mode(&mut self, mode: Mode) -> Vec<Value> {
        if self.status.ended() || self.settings.mode == mode {
            return vec![];
        }
        if self.provider == Provider::Codex {
            self.settings.mode = mode;
            return vec![];
        }
        let previous = Previous::Mode(std::mem::replace(&mut self.settings.mode, mode));
        let line = wire::set_permission_mode(&self.track(previous), mode);
        self.outgoing(vec![line])
    }

    fn track(&mut self, previous: Previous) -> String {
        let id = self.request_id();
        self.pending_settings.insert(id.clone(), previous);
        id
    }

    /// Subagents or shells still running after their turn ended.
    pub fn background_tasks(&self) -> usize {
        self.background_tasks
    }

    /// Call before closing the process: the exit that follows is not a failure.
    pub fn close_requested(&mut self) {
        self.closing = true;
    }

    pub fn diagnostics(&self) -> impl Iterator<Item = &str> {
        self.diagnostics.iter().map(String::as_str)
    }

    pub fn queue(&self) -> &VecDeque<Queued> {
        &self.queue
    }
    /// Stop paused automatic sending of the queue.
    pub fn queue_held(&self) -> bool {
        self.hold_queue && self.queue.iter().any(|q| q.steering.is_none())
    }

    /// Lets the queue held by Stop go on. Work cut short by Stop comes first: the agent
    /// is asked to finish it (`RESUME_PROMPT`) before the oldest queued message. Each
    /// goes as soon as the agent can take a turn (right away when it is idle, else once
    /// the Stop has finished).
    pub fn resume_queue(&mut self) -> Vec<Value> {
        if !self.hold_queue || self.status.ended() {
            return vec![];
        }
        self.hold_queue = false;
        if self.stopped {
            self.stopped = false;
            self.next_queued += 1;
            self.queue.push_front(Queued {
                id: QueuedId(self.next_queued),
                text: RESUME_PROMPT.to_owned(),
                attachments: vec![],
                context: None,
                steering: None,
                goal: false,
            });
        }
        self.flush()
    }

    /// Sends a user message, or keeps it in the queue while the agent is busy.
    pub fn submit(&mut self, text: &str) -> Vec<Value> {
        self.submit_draft(Draft::text(text))
    }
    /// `submit` with attachments (image data loaded).
    pub fn submit_draft(&mut self, draft: Draft) -> Vec<Value> {
        let Some(draft) = draft.trimmed() else {
            return vec![];
        };
        if self.status.ended() {
            return vec![];
        }
        self.hold_queue = false;
        // Canopy's own commands go first (`quiet`).
        if self.queue.is_empty() && self.quiet.is_empty() && self.accepts_turn() {
            return self.send(draft);
        }
        self.enqueue(draft);
        self.flush()
    }

    /// A `!` command starts: its card goes into the conversation, in a turn of its own.
    pub fn begin_shell(&mut self, command: &str) -> ItemId {
        let turn = self.transcript.allocate_turn();
        let index = self.transcript.push_in(
            turn,
            None,
            Body::Shell(crate::chat::transcript::ShellRun {
                command: command.to_owned(),
                running: true,
                ..Default::default()
            }),
        );
        self.transcript.items()[index].id
    }

    /// More output of the `!` command `id`.
    pub fn shell_output(&mut self, id: ItemId, chunk: &str) {
        if let Some(index) = self.transcript.position(id) {
            self.transcript.update(index, |body| {
                if let Body::Shell(run) = body {
                    crate::chat::shell::append(&mut run.output, chunk, crate::chat::shell::KEEP);
                }
            });
        }
    }

    /// The `!` command `id` ended: its result goes to the agent as a message (queued
    /// while the agent works).
    pub fn end_shell(
        &mut self,
        id: ItemId,
        exit: Option<i32>,
        stopped: bool,
        elapsed_ms: u64,
    ) -> Vec<Value> {
        let Some(index) = self.transcript.position(id) else {
            return vec![];
        };
        let mut result = None;
        self.transcript.update(index, |body| {
            if let Body::Shell(run) = body {
                run.running = false;
                run.exit = exit;
                run.stopped = stopped;
                run.elapsed_ms = Some(elapsed_ms);
                run.reported = true;
                result = Some(Attachment::Shell {
                    command: run.command.clone(),
                    exit,
                    stopped,
                    output: crate::chat::shell::excerpt(&run.output, crate::chat::shell::REPORT),
                    label: None,
                });
            }
        });
        match result {
            Some(result) => self.submit_draft(Draft {
                attachments: vec![result],
                ..Default::default()
            }),
            None => vec![],
        }
    }

    /// Shows saved history before the session's first event: the conversation it
    /// resumes. Ids and turns continue after it.
    pub fn restore_history(&mut self, items: Vec<Item>) {
        self.restore_history_after(items, 0);
    }
    /// `restore_history`, with new item ids also after `floor`.
    pub fn restore_history_after(&mut self, items: Vec<Item>, floor: u64) {
        self.transcript = Transcript::restore_after(items, floor);
    }

    /// Sends a queued message now: the agent reads it after the current tool.
    pub fn steer(&mut self, id: QueuedId) -> Vec<Value> {
        let Some(index) = self
            .queue
            .iter()
            .position(|q| q.id == id && q.steering.is_none())
        else {
            return vec![];
        };
        if self.status.ended() || self.interrupting {
            return vec![];
        }
        self.hold_queue = false;
        if self.accepts_turn() {
            let queued = self.queue.remove(index).expect("queued message");
            return self.send(queued.draft());
        }
        if self.provider == Provider::Codex {
            let draft = self.queue[index].clone().draft();
            let Some((id, line)) = self.codex_steer(&draft) else {
                // No running turn to steer into: it goes as the next message.
                return vec![];
            };
            self.queue[index].steering = Some(id);
            return self.outgoing(vec![line]);
        }
        let uuid = uuid::Uuid::new_v4().to_string();
        let line = wire::user_message(
            &self.queue[index].text,
            &self.queue[index].attachments,
            self.queue[index].context.as_deref(),
            &uuid,
            Priority::Next,
        );
        self.outstanding.insert(uuid.clone());
        if self.tracks_lifecycle() {
            // The bubble appears where the CLI reads it (`started`).
            self.queue[index].steering = Some(uuid);
        } else {
            let queued = self.queue.remove(index).expect("queued message");
            self.push_steered(queued.draft());
        }
        self.outgoing(vec![line])
    }

    /// Steers a message that was never queued (⌘⏎ in the composer).
    pub fn steer_text(&mut self, text: &str) -> Vec<Value> {
        self.steer_draft(Draft::text(text))
    }
    pub fn steer_draft(&mut self, draft: Draft) -> Vec<Value> {
        let Some(draft) = draft.trimmed() else {
            return vec![];
        };
        if self.status.ended() {
            return vec![];
        }
        let id = self.enqueue(draft);
        self.steer(id)
    }

    /// Keeps a composer draft displaced by Edit, without sending anything.
    pub fn enqueue_draft(&mut self, draft: Draft) {
        if let Some(draft) = draft.trimmed() {
            self.enqueue(draft);
        }
    }

    /// Moves a waiting message in front of `before` (or to the end); steers in flight
    /// stay where they are.
    pub fn move_queued(&mut self, id: QueuedId, before: Option<QueuedId>) -> bool {
        if Some(id) == before {
            return false;
        }
        let Some(from) = self
            .queue
            .iter()
            .position(|q| q.id == id && q.steering.is_none())
        else {
            return false;
        };
        let moved = self.queue.remove(from).expect("queued message");
        let at = before
            .and_then(|before| self.queue.iter().position(|q| q.id == before))
            .unwrap_or(self.queue.len());
        self.queue.insert(at, moved);
        true
    }

    /// Takes a waiting message back for editing.
    pub fn edit(&mut self, id: QueuedId) -> Option<Draft> {
        let index = self
            .queue
            .iter()
            .position(|q| q.id == id && q.steering.is_none())?;
        self.queue.remove(index).map(|q| q.draft())
    }
    pub fn remove(&mut self, id: QueuedId) -> bool {
        self.edit(id).is_some()
    }

    /// Messages not yet written to the CLI.
    pub fn unsent(&self) -> usize {
        self.queue.iter().filter(|q| q.steering.is_none()).count()
    }
    /// Drops messages not yet written to the CLI; steers in flight still show when read.
    pub fn discard_unsent(&mut self) {
        self.queue.retain(|q| q.steering.is_some());
    }

    /// Unsent messages for a replacement session (Restart); nothing is lost.
    pub fn take_queue(&mut self) -> Vec<Draft> {
        self.queue.drain(..).map(|q| q.draft()).collect()
    }
    /// Restores messages taken from a previous session; they wait for the user.
    pub fn restore_queue(&mut self, drafts: Vec<Draft>) {
        for draft in drafts {
            self.enqueue(draft);
        }
        self.hold_queue = !self.queue.is_empty();
    }

    /// Whether one background task (`call`) can be stopped on its own: Claude only, while
    /// it runs; Codex has no request for it.
    pub fn can_stop_background(&self, call: &str) -> bool {
        self.provider == Provider::Claude
            && self.initialized
            && !self.status.ended()
            && self.adapter.background_task(call).is_some()
    }

    /// Stops one background task; its card ends when the CLI reports it stopped.
    pub fn stop_background(&mut self, call: &str) -> Vec<Value> {
        if !self.can_stop_background(call) {
            return vec![];
        }
        let task = self
            .adapter
            .background_task(call)
            .expect("checked above")
            .to_owned();
        let id = self.request_id();
        self.task_stops.insert(id.clone());
        self.outgoing(vec![wire::stop_task(&id, &task)])
    }

    /// Stops the current turn. Pending permissions are denied so the CLI is not left waiting.
    pub fn interrupt(&mut self) -> Vec<Value> {
        // After its turn the agent may still wait for background subagents; interrupt
        // stops them too (live-verified: they end with `task_notification stopped`).
        // Only the CLI's own state says it is still busy: a stale task count must not
        // leave the session interrupting with nothing to end it.
        let waiting = !self.cli_idle;
        if self.goal_hold {
            // Between Codex's goal turns: pausing the goal is the whole stop.
            let (lines, _) = self.codex_stop_goal();
            // Held first: nothing waiting goes out as the hold ends.
            self.hold_queue = true;
            self.stopped = true;
            let _ = self.expire_goal_hold();
            return self.outgoing(lines);
        }
        if !self.initialized || self.interrupting || !(self.turn_open || waiting) {
            return vec![];
        }
        self.interrupting = true;
        if self.turn_open {
            self.status = Status::Interrupting;
        }
        if self.provider == Provider::Codex {
            let (mut lines, cancelled) = self.codex_stop_goal();
            if !cancelled {
                lines.extend(self.codex_interrupt());
            }
            self.hold_queue = true;
            self.stopped = true;
            return lines;
        }
        let mut lines = vec![];
        for request in self.adapter.pending_permissions() {
            if let Some(line) = self.answer(&request, Decision::Cancelled) {
                lines.push(line);
            }
        }
        let id = self.request_id();
        let cancel_queued = self
            .info
            .capabilities
            .iter()
            .any(|c| c == wire::CANCEL_QUEUED_CAPABILITY);
        lines.push(wire::interrupt(&id, cancel_queued));
        self.interrupt_id = Some(id);
        self.hold_queue = true;
        self.stopped = true;
        lines
    }

    /// Allows or denies a permission item; repeated decisions are ignored.
    pub fn decide(&mut self, item: ItemId, allow: bool) -> Vec<Value> {
        self.respond(
            item,
            if allow {
                Response::Allow
            } else {
                Response::Deny(None)
            },
        )
    }

    /// Answers a pending request; answers that don't fit its kind, and repeated ones,
    /// are ignored.
    pub fn respond(&mut self, item: ItemId, response: Response) -> Vec<Value> {
        if self.provider == Provider::Codex {
            return self.codex_respond(item, response);
        }
        let Some(permission) = self.transcript.position(item).and_then(|index| {
            match &self.transcript.items()[index].body {
                Body::Permission(p) if p.decision == Decision::Pending => Some(p.clone()),
                _ => None,
            }
        }) else {
            return vec![];
        };
        let request = permission.request_id.as_str();
        let call = permission.call_id.as_deref();
        let input = &permission.input;
        let kind = permission.kind;
        let (decision, resolution, line) = match response {
            Response::Allow if kind == PermissionKind::Tool => (
                Decision::Allowed,
                Resolution::Allowed,
                wire::allow(request, call, input),
            ),
            Response::AllowForChat if kind == PermissionKind::Tool => {
                // Only rules the chat can keep are sent, so a restart behaves the same.
                let remember = match permission.remember.clone() {
                    Some(Remember::Rules(rules)) => {
                        let rules: Vec<Rule> = rules
                            .into_iter()
                            .filter(|rule| valid_rule(&rule.text()))
                            .collect();
                        if rules.is_empty() {
                            return vec![];
                        }
                        Remember::Rules(rules)
                    }
                    Some(Remember::AcceptEdits) => Remember::AcceptEdits,
                    Some(Remember::Session) | None => return vec![],
                };
                let line = wire::allow_updating(
                    request,
                    call,
                    input,
                    wire::session_permissions(&remember),
                );
                (Decision::Allowed, Resolution::Remembered(remember), line)
            }
            Response::Deny(note) => {
                let note = clean(note);
                let message = match &note {
                    Some(note) => format!("The user denied this in Canopy and said: {note}"),
                    None => DENIED_MESSAGE.to_owned(),
                };
                (
                    Decision::Denied,
                    Resolution::Denied(note),
                    wire::deny(request, call, &message),
                )
            }
            Response::Answer(answers) if kind == PermissionKind::Question => {
                let pairs: Vec<(String, String)> = presentation::questions(&permission)
                    .into_iter()
                    .zip(answers)
                    .filter_map(|(question, answer)| {
                        Some((question.question, clean(Some(answer))?))
                    })
                    .collect();
                if pairs.is_empty() {
                    return vec![];
                }
                let line = wire::answer_questions(request, call, input, &pairs);
                (Decision::Allowed, Resolution::Answered(pairs), line)
            }
            Response::ApprovePlan { accept_edits } if kind == PermissionKind::Plan => (
                Decision::Allowed,
                Resolution::PlanApproved { accept_edits },
                wire::approve_plan(request, call, input, accept_edits),
            ),
            Response::KeepPlanning(note) if kind == PermissionKind::Plan => {
                let note = clean(note);
                let message = match &note {
                    Some(note) => format!("The user wants to keep planning. Feedback: {note}"),
                    None => "The user wants to keep planning.".to_owned(),
                };
                (
                    Decision::Denied,
                    Resolution::KeepPlanning(note),
                    wire::deny(request, call, &message),
                )
            }
            _ => return vec![],
        };
        let rules = match &resolution {
            Resolution::Remembered(Remember::Rules(rules)) => {
                rules.iter().map(Rule::text).collect()
            }
            _ => vec![],
        };
        if !self
            .adapter
            .resolve_with(request, decision, Some(resolution), &mut self.transcript)
        {
            return vec![];
        }
        self.remembered.extend(rules);
        if self.status == Status::NeedsDecision && self.pending_requests().is_empty() {
            self.status = Status::Working;
        }
        vec![line]
    }

    /// Context window use after the last finished turn.
    pub fn context(&self) -> Option<ContextUsage> {
        match self.provider {
            Provider::Claude => self.context,
            Provider::Codex => self.codex.adapter.context,
        }
    }

    /// The account pays per token (an API key), so the turn's cost is an amount owed.
    /// A subscription login reports `none` (or `oauth`).
    pub fn api_billing(&self) -> bool {
        if self.provider == Provider::Codex {
            return self.codex.account.as_deref() == Some("apiKey");
        }
        self.info
            .api_key_source
            .as_deref()
            .is_some_and(|source| !matches!(source, "none" | "oauth"))
    }

    /// Set at start: whether this session continues an earlier conversation.
    pub fn set_base(&mut self, base: Base) {
        self.base = base;
    }

    fn request_context(&mut self) -> Vec<Value> {
        // A closing session asks nothing more of its process; Codex reports it unasked.
        if self.provider == Provider::Codex
            || !self.initialized
            || self.closing
            || self.status.ended()
            || self.context_request.is_some()
        {
            return vec![];
        }
        let id = self.request_id();
        self.context_request = Some(id.clone());
        vec![wire::get_context_usage(&id)]
    }

    /// The last message that started a turn, to send again (Retry).
    pub fn retry_draft(&self) -> Option<Draft> {
        self.transcript
            .items()
            .iter()
            .rev()
            .find_map(|item| match &item.body {
                Body::User(message)
                    if item.parent.is_none() && message.delivery == Delivery::Sent =>
                {
                    Some(Draft {
                        text: message.text.clone(),
                        attachments: message.attachments.clone(),
                        context: message.context.clone(),
                        goal: message.goal,
                    })
                }
                _ => None,
            })
    }

    /// Where an edit of the user message `item` continues from: the last message of
    /// the turn before it (`None` for the first message: a fresh conversation). `None`
    /// when the item can't be edited (not a sent message of this session).
    pub fn fork_point(&self, item: ItemId) -> Option<Option<String>> {
        let index = self.transcript.position(item)?;
        let Body::User(message) = &self.transcript.items()[index].body else {
            return None;
        };
        if message.delivery != Delivery::Sent || self.transcript.items()[index].parent.is_some() {
            return None;
        }
        let previous = self.transcript.items()[..index]
            .iter()
            .rev()
            .find_map(|item| match &item.body {
                Body::TurnEnd(end) => end.resume_at.clone(),
                _ => None,
            });
        match (previous, &self.base) {
            (Some(at), _) => Some(Some(at)),
            (None, Base::Fresh) => Some(None),
            // History before this session's first turn isn't known here.
            (None, Base::Continued) => None,
        }
    }

    /// Asks what restoring files to before the user message `item` would change
    /// (`dry_run`), or restores them. The outcome arrives in `rewind`.
    pub fn rewind_files(&mut self, item: ItemId, dry_run: bool) -> Vec<Value> {
        let Some(uuid) = self.transcript.position(item).and_then(|index| {
            match &self.transcript.items()[index].body {
                Body::User(message) => message.uuid.clone(),
                _ => None,
            }
        }) else {
            return vec![];
        };
        // Codex keeps no file checkpoints (DEC-055).
        if !self.initialized || self.status.ended() || self.provider == Provider::Codex {
            return vec![];
        }
        let id = self.request_id();
        self.rewinds.insert(id.clone(), (item, dry_run));
        self.rewind = None;
        vec![wire::rewind_files(&id, &uuid, dry_run)]
    }

    /// The latest file restore outcome.
    pub fn rewind(&self) -> Option<&(ItemId, Rewind)> {
        self.rewind.as_ref()
    }

    /// Requests waiting for the user, oldest first.
    pub fn pending_decisions(&self) -> Vec<ItemId> {
        self.pending_requests()
            .iter()
            .filter_map(|request| {
                self.transcript
                    .items()
                    .iter()
                    .rev()
                    .find_map(|item| match &item.body {
                        Body::Permission(p) if &p.request_id == request => Some(item.id),
                        _ => None,
                    })
            })
            .collect()
    }

    /// Rules "Allow for this chat" added since the last call, to keep with the chat.
    pub fn take_remembered(&mut self) -> Vec<String> {
        std::mem::take(&mut self.remembered)
    }

    /// A process event of either provider.
    pub fn on_incoming(&mut self, event: Event<Incoming>) -> Vec<Value> {
        match event {
            Event::Message(Incoming::Claude(message)) => self.on_message(message),
            Event::Message(Incoming::Codex(message)) => self.on_codex(message),
            Event::Diagnostic(text) => self.on_event(Event::Diagnostic(text)),
            Event::Exited(exit) => self.on_event(Event::Exited(exit)),
        }
    }

    pub fn on_event(&mut self, event: Event<Inbound>) -> Vec<Value> {
        match event {
            Event::Message(message) => self.on_message(message),
            Event::Diagnostic(text) => {
                if self.diagnostics.len() == DIAGNOSTICS {
                    self.diagnostics.pop_front();
                }
                self.diagnostics.push_back(text);
                vec![]
            }
            Event::Exited(exit) => {
                self.on_exit(exit);
                vec![]
            }
        }
    }

    pub fn on_message(&mut self, message: Inbound) -> Vec<Value> {
        // Output racing a reported exit (a descendant held stdout) has no live session.
        if self.status.ended() {
            return vec![];
        }
        if !self.turn_open && self.initialized && Adapter::opens_turn(&message) {
            let turn = self.transcript.allocate_turn();
            self.start_turn(turn);
        }
        let signals = self
            .adapter
            .apply(message, &mut self.transcript, self.interrupting);
        let mut lines = vec![];
        for signal in signals {
            match signal {
                Signal::ControlAck {
                    request_id,
                    error,
                    response,
                } if request_id == self.initialize_id => {
                    match error {
                        None => {
                            self.initialized = true;
                            if let Some(response) = &response {
                                self.models = wire::models(response);
                                self.commands = wire::commands(response);
                                if !self.info.skills.is_empty() {
                                    crate::chat::config::mark_skills(
                                        &mut self.commands,
                                        &self.info.skills,
                                    );
                                }
                                // A change queued before initialize is newer than the launch value.
                                let queued_mode = self
                                    .pending_settings
                                    .values()
                                    .any(|p| matches!(p, Previous::Mode(_)));
                                if let Some(mode) = response
                                    .get("current_permission_mode")
                                    .and_then(Value::as_str)
                                    .and_then(Mode::from_claude)
                                    .filter(|_| !queued_mode)
                                {
                                    self.settings.mode = mode;
                                }
                            }
                            if self.status == Status::Starting {
                                self.status = if self.turn_open {
                                    Status::Working
                                } else {
                                    Status::Ready
                                };
                            }
                            lines.append(&mut self.held);
                        }
                        Some(error) => {
                            self.status = Status::Failed(format!(
                                "Claude Code could not start the session: {error}"
                            ));
                        }
                    }
                }
                Signal::ControlAck {
                    request_id,
                    response,
                    ..
                } if self.context_request.as_deref() == Some(request_id.as_str()) => {
                    self.context_request = None;
                    if let Some(usage) = response.as_ref().and_then(wire::context_usage) {
                        self.context = Some(usage);
                    }
                }
                Signal::ControlAck {
                    request_id,
                    error,
                    response,
                } if self.rewinds.contains_key(&request_id) => {
                    let (item, dry_run) = self.rewinds.remove(&request_id).expect("rewind");
                    let outcome = match (error, response) {
                        (Some(error), _) if dry_run => Rewind::Unavailable(error),
                        (Some(error), _) => Rewind::Failed(error),
                        (None, Some(response)) if dry_run => {
                            Rewind::Preview(wire::rewind_preview(&response))
                        }
                        (None, Some(response))
                            if response["canRewind"].as_bool() == Some(false) =>
                        {
                            Rewind::Failed(
                                response["error"]
                                    .as_str()
                                    .unwrap_or("These files can no longer be restored.")
                                    .to_owned(),
                            )
                        }
                        (None, _) => Rewind::Done,
                    };
                    self.rewind = Some((item, outcome));
                }
                Signal::ControlAck {
                    request_id, error, ..
                } if self.pending_settings.contains_key(&request_id) => {
                    let previous = self.pending_settings.remove(&request_id);
                    if let (Some(error), Some(previous)) = (error, previous) {
                        let noun = previous.noun();
                        match previous {
                            Previous::Model(model) => self.settings.model = model,
                            Previous::Effort(effort) => self.settings.effort = effort,
                            Previous::Mode(mode) => self.settings.mode = mode,
                        }
                        self.transcript.push(
                            None,
                            Body::Notice(Notice {
                                kind: NoticeKind::Error,
                                text: format!("Couldn't change the {noun}: {error}"),
                                until: None,
                            }),
                        );
                    }
                }
                Signal::PermissionMode(mode) => {
                    if let Some(mode) = Mode::from_claude(&mode) {
                        self.settings.mode = mode;
                    }
                }
                Signal::ControlAck {
                    request_id,
                    error: Some(error),
                    ..
                } if self.interrupt_id.as_deref() == Some(request_id.as_str()) => {
                    self.interrupting = false;
                    if self.status == Status::Interrupting {
                        self.status = Status::Working;
                    }
                    self.transcript.push(
                        None,
                        Body::Notice(Notice {
                            kind: NoticeKind::Error,
                            text: format!("Claude Code could not stop the turn: {error}"),
                            until: None,
                        }),
                    );
                }
                Signal::ControlAck {
                    request_id, error, ..
                } if self.task_stops.remove(&request_id) => {
                    if let Some(error) = error {
                        self.transcript.push(
                            None,
                            Body::Notice(Notice {
                                kind: NoticeKind::Error,
                                text: format!("Couldn't stop the background task: {error}"),
                                until: None,
                            }),
                        );
                    }
                }
                Signal::ControlAck { .. } => {}
                Signal::Init(info) => {
                    if !info.skills.is_empty() {
                        crate::chat::config::mark_skills(&mut self.commands, &info.skills);
                    }
                    let session = info.session_id.clone().or(self.info.session_id.take());
                    self.info = InitInfo {
                        session_id: session,
                        ..info
                    };
                }
                Signal::SessionId(id) => self.info.session_id = Some(id),
                Signal::State(CliState::Running) => {
                    self.cli_idle = false;
                    if !self.turn_open {
                        // The agent resumed on its own, e.g. to report a finished
                        // background subagent.
                        let turn = self.transcript.allocate_turn();
                        self.start_turn(turn);
                    }
                    if matches!(self.status, Status::Ready | Status::Starting) {
                        self.status = Status::Working;
                    }
                }
                // requires_action precedes the can_use_tool request; only an actual
                // answerable item puts the session into NeedsDecision.
                Signal::State(CliState::RequiresAction) => {}
                Signal::PermissionRequested => {
                    if !self.interrupting {
                        self.status = Status::NeedsDecision;
                    }
                }
                Signal::PermissionWithdrawn => {
                    if self.status == Status::NeedsDecision
                        && self.adapter.pending_permissions().is_empty()
                    {
                        self.status = Status::Working;
                    }
                }
                Signal::State(CliState::Idle) => {
                    lines.extend(self.request_context());
                    // Authoritative: everything we sent was handled.
                    self.cli_idle = true;
                    // Idle fires only after background subagents have exited.
                    self.background_tasks = 0;
                    self.outstanding.clear();
                    // A steer the CLI never reported as read waits again instead of
                    // blocking the queue.
                    for queued in &mut self.queue {
                        queued.steering = None;
                    }
                    self.turn_open = false;
                    self.interrupting = false;
                    if !self.status.ended() {
                        self.status = Status::Ready;
                    }
                }
                Signal::TurnFinished => {
                    self.hide_quiet_end();
                    self.turn_open = false;
                    self.interrupting = false;
                    if !self.status.ended() {
                        self.status = Status::Ready;
                    }
                }
                Signal::BackgroundTasks(count) => self.background_tasks = count,
                Signal::Reply(line) => lines.push(line),
                Signal::Lifecycle { uuid, state } => self.on_lifecycle(uuid, state),
                Signal::Goal(event) => lines.extend(self.on_goal(event)),
            }
        }
        lines.extend(self.flush());
        lines
    }

    fn on_lifecycle(&mut self, uuid: String, state: Lifecycle) {
        self.lifecycle_seen = true;
        let steering = self
            .queue
            .iter()
            .position(|q| q.steering.as_deref() == Some(uuid.as_str()));
        match (state, steering) {
            (Lifecycle::Started, Some(index)) => {
                let queued = self.queue.remove(index).expect("steered message");
                self.push_steered(queued.draft());
            }
            // Dropped before the agent read it (Stop, cancelled turn): it waits again.
            (state, Some(index)) if state.terminal() => self.queue[index].steering = None,
            _ => {}
        }
        if state.terminal() {
            self.outstanding.remove(&uuid);
        }
    }

    fn on_exit(&mut self, exit: Exit) {
        let clean = self.closing || (exit.code == Some(0) && self.initialized && !self.turn_open);
        let reason = match (exit.code, exit.signal) {
            (Some(code), _) => format!("exit code {code}"),
            (None, Some(signal)) => format!("signal {signal}"),
            (None, None) => "unknown status".into(),
        };
        if self.turn_open {
            let outcome = if self.interrupting || self.closing {
                Outcome::Interrupted
            } else {
                Outcome::Failed(format!(
                    "{} exited before finishing the turn ({reason}).",
                    self.provider.name()
                ))
            };
            match self.provider {
                Provider::Claude => self.adapter.close_turn(&mut self.transcript, &outcome),
                Provider::Codex => self
                    .codex
                    .adapter
                    .close_turn(&mut self.transcript, &outcome),
            }
            self.transcript.push(
                None,
                Body::TurnEnd(TurnEnd {
                    outcome,
                    usage: None,
                    cost_usd: None,
                    duration_ms: None,
                    resume_at: None,
                    quiet: false,
                }),
            );
            self.turn_open = false;
        } else if self.provider == Provider::Claude {
            self.adapter
                .close_turn(&mut self.transcript, &Outcome::Interrupted);
        } else {
            self.codex
                .adapter
                .close_turn(&mut self.transcript, &Outcome::Interrupted);
        }
        self.adapter.cancel_background(&mut self.transcript);
        self.background_tasks = 0;
        self.outstanding.clear();
        self.cli_idle = true;
        for queued in &mut self.queue {
            queued.steering = None;
        }
        if !clean && !exit.stderr.is_empty() {
            self.transcript.push(
                None,
                Body::Notice(Notice {
                    kind: NoticeKind::Diagnostic,
                    text: exit.stderr.clone(),
                    until: None,
                }),
            );
        }
        if matches!(self.status, Status::Failed(_)) {
            return;
        }
        self.status = if clean {
            Status::Exited {
                code: exit.code,
                signal: exit.signal,
            }
        } else if !self.initialized {
            Status::Failed(format!(
                "{} stopped before the session started ({reason}).",
                self.provider.name()
            ))
        } else {
            Status::Failed(format!(
                "{} session ended unexpectedly ({reason}).",
                self.provider.name()
            ))
        };
    }

    fn answer(&mut self, request: &str, decision: Decision) -> Option<Value> {
        let index = self.transcript.rposition(
            |item| matches!(&item.body, Body::Permission(p) if p.request_id == request),
        )?;
        let Body::Permission(permission) = &self.transcript.items()[index].body else {
            return None;
        };
        let call = permission.call_id.clone();
        let input = permission.input.clone();
        if !self
            .adapter
            .resolve(request, decision, &mut self.transcript)
        {
            return None;
        }
        Some(match decision {
            Decision::Allowed => wire::allow(request, call.as_deref(), &input),
            Decision::Cancelled => wire::deny(request, call.as_deref(), "Interrupted by the user."),
            _ => wire::deny(request, call.as_deref(), DENIED_MESSAGE),
        })
    }

    fn start_turn(&mut self, turn: super::transcript::TurnId) {
        self.transcript.set_current_turn(turn);
        self.turn_open = true;
        // An unanswered context request doesn't block the next one.
        self.context_request = None;
        if !self.status.ended() && self.initialized {
            self.status = Status::Working;
        }
    }

    /// Nothing runs and nothing we sent is pending: a message would start a fresh turn.
    fn accepts_turn(&self) -> bool {
        !self.turn_open
            && !self.interrupting
            && !self.goal_hold
            && self.cli_idle
            && (!self.tracks_lifecycle() || self.outstanding.is_empty())
            && self.pending_requests().is_empty()
            // Codex takes a turn only once its thread is open.
            && (self.provider == Provider::Claude || self.codex_thread().is_some())
    }

    fn tracks_lifecycle(&self) -> bool {
        self.lifecycle_seen
            || self
                .info
                .capabilities
                .iter()
                .any(|c| c == wire::LIFECYCLE_CAPABILITY)
    }

    fn enqueue(&mut self, draft: Draft) -> QueuedId {
        self.next_queued += 1;
        let id = QueuedId(self.next_queued);
        self.queue.push_back(Queued {
            id,
            text: draft.text,
            attachments: draft.attachments,
            context: draft.context,
            steering: None,
            goal: draft.goal,
        });
        id
    }

    /// Starts a turn with `draft`.
    fn send(&mut self, draft: Draft) -> Vec<Value> {
        // Codex needs its thread; `accepts_turn` waits for it, this keeps it so.
        if self.provider == Provider::Codex && self.codex_thread().is_none() {
            return vec![];
        }
        self.stopped = false;
        let turn = self.transcript.allocate_turn();
        let uuid = uuid::Uuid::new_v4().to_string();
        if draft.goal {
            self.start_goal(&draft.text);
        }
        let line = match self.provider {
            Provider::Claude if draft.goal => wire::user_message(
                &format!("/{} {}", super::goal::COMMAND, draft.text),
                &[],
                draft.context.as_deref(),
                &uuid,
                Priority::Later,
            ),
            Provider::Claude => wire::user_message(
                &draft.text,
                &draft.attachments,
                draft.context.as_deref(),
                &uuid,
                Priority::Later,
            ),
            Provider::Codex if draft.goal => {
                self.codex_goal_send(&draft).pop().unwrap_or(Value::Null)
            }
            Provider::Codex => self.codex_send(&draft, &uuid).pop().unwrap_or(Value::Null),
        };
        self.transcript.push_in(
            turn,
            None,
            Body::User(UserMessage {
                text: draft.text,
                delivery: Delivery::Sent,
                uuid: Some(uuid.clone()),
                attachments: draft.attachments,
                context: draft.context,
                goal: draft.goal,
            }),
        );
        self.start_turn(turn);
        self.outstanding.insert(uuid);
        self.outgoing(vec![line])
    }

    /// Shows a steered message where the agent read it: inside the running turn, or
    /// as a new turn when the CLI ran it after the previous one ended.
    fn push_steered(&mut self, draft: Draft) {
        if self.turn_open {
            self.transcript.push(
                None,
                Body::User(UserMessage {
                    text: draft.text,
                    delivery: Delivery::Steered,
                    uuid: None,
                    attachments: draft.attachments,
                    context: draft.context.clone(),
                    goal: false,
                }),
            );
        } else {
            let turn = self.transcript.allocate_turn();
            self.transcript.push_in(
                turn,
                None,
                Body::User(UserMessage {
                    text: draft.text,
                    delivery: Delivery::Sent,
                    uuid: None,
                    attachments: draft.attachments,
                    context: draft.context.clone(),
                    goal: false,
                }),
            );
            self.start_turn(turn);
        }
    }

    /// Sends the oldest waiting message once the agent has finished everything.
    fn flush(&mut self) -> Vec<Value> {
        if let Some(lines) = self.flush_quiet() {
            return lines;
        }
        if let Some(lines) = self.steer_into_goal() {
            return lines;
        }
        if self.hold_queue
            || !self.initialized
            || self.status.ended()
            || self.queue.iter().any(|q| q.steering.is_some())
            || !self.accepts_turn()
        {
            return vec![];
        }
        match self.queue.pop_front() {
            Some(queued) => self.send(queued.draft()),
            None => vec![],
        }
    }

    fn outgoing(&mut self, lines: Vec<Value>) -> Vec<Value> {
        if self.initialized {
            lines
        } else {
            self.held.extend(lines);
            vec![]
        }
    }

    fn request_id(&mut self) -> String {
        self.next_request += 1;
        format!("canopy-{}", self.next_request)
    }
}
