//! Session controller: owns the transcript and decides what to write to the agent.
//! No I/O — the owner forwards process events in and writes the returned lines out.
use super::{
    claude::{
        adapter::{Adapter, CliState, InitInfo, Signal},
        wire::{self, Inbound, Lifecycle, Priority},
    },
    config::{ChatSettings, Effort, Mode, ModelInfo},
    process::{Event, Exit},
    transcript::{
        Body, Decision, Delivery, ItemId, Notice, NoticeKind, Outcome, Transcript, TurnEnd,
        UserMessage,
    },
};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

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

/// What Resume sends after Stop, so the agent finishes the interrupted work before the
/// queue continues.
pub const RESUME_PROMPT: &str = "Continue where you left off.";

/// A message held in Canopy until the agent finishes, or being steered into its turn.
#[derive(Clone, Debug, PartialEq)]
pub struct Queued {
    pub id: QueuedId,
    pub text: String,
    /// Uuid of the Steer already written to the CLI; it can no longer be edited.
    pub steering: Option<String>,
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
    /// In-flight setting changes and the value to restore if the CLI refuses them.
    pending_settings: HashMap<String, Previous>,
    /// Oldest first. Sent one at a time once the agent has finished everything.
    queue: VecDeque<Queued>,
    next_queued: u64,
    /// Stop holds the queue until the user sends or steers again.
    hold_queue: bool,
    /// Stop cut work short and nothing was sent since: Resume first asks to finish it.
    stopped: bool,
    /// Our message uuids the CLI has not finished with (`command_lifecycle`).
    outstanding: HashSet<String>,
    lifecycle_seen: bool,
    /// The CLI reported idle: no turn, held result or background agent is running.
    cli_idle: bool,
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

impl ChatSession {
    /// Returns the session and the lines to write immediately.
    pub fn new() -> (Self, Vec<Value>) {
        Self::with_settings(ChatSettings::default())
    }

    /// `settings` must match what the process was launched with.
    pub fn with_settings(settings: ChatSettings) -> (Self, Vec<Value>) {
        let mut session = Self {
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
            pending_settings: HashMap::new(),
            queue: VecDeque::new(),
            next_queued: 0,
            hold_queue: false,
            stopped: false,
            outstanding: HashSet::new(),
            lifecycle_seen: false,
            cli_idle: true,
        };
        session.initialize_id = session.request_id();
        let init = wire::initialize(&session.initialize_id);
        (session, vec![init])
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
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

    pub fn set_model(&mut self, model: Option<String>) -> Vec<Value> {
        if self.status.ended() || self.settings.model == model {
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
        let previous = Previous::Effort(std::mem::replace(&mut self.settings.effort, effort));
        let line = wire::set_effort(&self.track(previous), effort);
        self.outgoing(vec![line])
    }
    pub fn set_mode(&mut self, mode: Mode) -> Vec<Value> {
        if self.status.ended() || self.settings.mode == mode {
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
                steering: None,
            });
        }
        self.flush()
    }

    /// Sends a user message, or keeps it in the queue while the agent is busy.
    pub fn submit(&mut self, text: &str) -> Vec<Value> {
        let text = text.trim();
        if text.is_empty() || self.status.ended() {
            return vec![];
        }
        self.hold_queue = false;
        if self.queue.is_empty() && self.accepts_turn() {
            return self.send(text.to_owned());
        }
        self.enqueue(text.to_owned());
        self.flush()
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
            return self.send(queued.text);
        }
        let uuid = uuid::Uuid::new_v4().to_string();
        let line = wire::user_message(&self.queue[index].text, &uuid, Priority::Next);
        self.outstanding.insert(uuid.clone());
        if self.tracks_lifecycle() {
            // The bubble appears where the CLI reads it (`started`).
            self.queue[index].steering = Some(uuid);
        } else {
            let queued = self.queue.remove(index).expect("queued message");
            self.push_steered(queued.text);
        }
        self.outgoing(vec![line])
    }

    /// Steers a message that was never queued (⌘⏎ in the composer).
    pub fn steer_text(&mut self, text: &str) -> Vec<Value> {
        let text = text.trim();
        if text.is_empty() || self.status.ended() {
            return vec![];
        }
        let id = self.enqueue(text.to_owned());
        self.steer(id)
    }

    /// Keeps a composer draft displaced by Edit, without sending anything.
    pub fn enqueue_draft(&mut self, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            self.enqueue(text.to_owned());
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
    pub fn edit(&mut self, id: QueuedId) -> Option<String> {
        let index = self
            .queue
            .iter()
            .position(|q| q.id == id && q.steering.is_none())?;
        self.queue.remove(index).map(|q| q.text)
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
    pub fn take_queue(&mut self) -> Vec<String> {
        self.queue.drain(..).map(|q| q.text).collect()
    }
    /// Restores messages taken from a previous session; they wait for the user.
    pub fn restore_queue(&mut self, texts: Vec<String>) {
        for text in texts {
            self.enqueue(text);
        }
        self.hold_queue = !self.queue.is_empty();
    }

    /// Stops the current turn. Pending permissions are denied so the CLI is not left waiting.
    pub fn interrupt(&mut self) -> Vec<Value> {
        // After its turn the agent may still wait for background subagents; interrupt
        // stops them too (live-verified: they end with `task_notification stopped`).
        // Only the CLI's own state says it is still busy: a stale task count must not
        // leave the session interrupting with nothing to end it.
        let waiting = !self.cli_idle;
        if !self.initialized || self.interrupting || !(self.turn_open || waiting) {
            return vec![];
        }
        self.interrupting = true;
        if self.turn_open {
            self.status = Status::Interrupting;
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

    /// Records the user's decision for a permission item; repeated decisions are ignored.
    pub fn decide(&mut self, item: ItemId, allow: bool) -> Vec<Value> {
        let Some(request) = self.transcript.position(item).and_then(|index| {
            match &self.transcript.items()[index].body {
                Body::Permission(p) if p.decision == Decision::Pending => {
                    Some(p.request_id.clone())
                }
                _ => None,
            }
        }) else {
            return vec![];
        };
        let decision = if allow {
            Decision::Allowed
        } else {
            Decision::Denied
        };
        let line = self.answer(&request, decision);
        if self.status == Status::NeedsDecision && self.adapter.pending_permissions().is_empty() {
            self.status = Status::Working;
        }
        line.into_iter().collect()
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
                        }),
                    );
                }
                Signal::ControlAck { .. } => {}
                Signal::Init(info) => {
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
                    self.turn_open = false;
                    self.interrupting = false;
                    if !self.status.ended() {
                        self.status = Status::Ready;
                    }
                }
                Signal::BackgroundTasks(count) => self.background_tasks = count,
                Signal::Reply(line) => lines.push(line),
                Signal::Lifecycle { uuid, state } => self.on_lifecycle(uuid, state),
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
                self.push_steered(queued.text);
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
                    "Claude Code exited before finishing the turn ({reason})."
                ))
            };
            self.adapter.close_turn(&mut self.transcript, &outcome);
            self.transcript.push(
                None,
                Body::TurnEnd(TurnEnd {
                    outcome,
                    usage: None,
                    cost_usd: None,
                    duration_ms: None,
                }),
            );
            self.turn_open = false;
        } else {
            self.adapter
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
                "Claude Code stopped before the session started ({reason})."
            ))
        } else {
            Status::Failed(format!(
                "Claude Code session ended unexpectedly ({reason})."
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
        if !self.status.ended() && self.initialized {
            self.status = Status::Working;
        }
    }

    /// Nothing runs and nothing we sent is pending: a message would start a fresh turn.
    fn accepts_turn(&self) -> bool {
        !self.turn_open
            && !self.interrupting
            && self.cli_idle
            && (!self.tracks_lifecycle() || self.outstanding.is_empty())
            && self.adapter.pending_permissions().is_empty()
    }

    fn tracks_lifecycle(&self) -> bool {
        self.lifecycle_seen
            || self
                .info
                .capabilities
                .iter()
                .any(|c| c == wire::LIFECYCLE_CAPABILITY)
    }

    fn enqueue(&mut self, text: String) -> QueuedId {
        self.next_queued += 1;
        let id = QueuedId(self.next_queued);
        self.queue.push_back(Queued {
            id,
            text,
            steering: None,
        });
        id
    }

    /// Starts a turn with `text`.
    fn send(&mut self, text: String) -> Vec<Value> {
        self.stopped = false;
        let turn = self.transcript.allocate_turn();
        let uuid = uuid::Uuid::new_v4().to_string();
        let line = wire::user_message(&text, &uuid, Priority::Later);
        self.transcript.push_in(
            turn,
            None,
            Body::User(UserMessage {
                text,
                delivery: Delivery::Sent,
            }),
        );
        self.start_turn(turn);
        self.outstanding.insert(uuid);
        self.outgoing(vec![line])
    }

    /// Shows a steered message where the agent read it: inside the running turn, or
    /// as a new turn when the CLI ran it after the previous one ended.
    fn push_steered(&mut self, text: String) {
        if self.turn_open {
            self.transcript.push(
                None,
                Body::User(UserMessage {
                    text,
                    delivery: Delivery::Steered,
                }),
            );
        } else {
            let turn = self.transcript.allocate_turn();
            self.transcript.push_in(
                turn,
                None,
                Body::User(UserMessage {
                    text,
                    delivery: Delivery::Sent,
                }),
            );
            self.start_turn(turn);
        }
    }

    /// Sends the oldest waiting message once the agent has finished everything.
    fn flush(&mut self) -> Vec<Value> {
        if self.hold_queue
            || !self.initialized
            || self.status.ended()
            || self.queue.iter().any(|q| q.steering.is_some())
            || !self.accepts_turn()
        {
            return vec![];
        }
        match self.queue.pop_front() {
            Some(queued) => self.send(queued.text),
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
