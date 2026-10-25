//! The Codex side of `ChatSession`: what goes to `codex app-server` and how its
//! answers move the session. The transcript, queue and decisions are shared.
use super::{ChatSession, DENIED_MESSAGE, Draft, Response, Status, clean};
use crate::chat::backend::{CliState, Signal};
use crate::chat::codex::{
    adapter::{self, Adapter},
    wire::{self, Message, ThreadOptions, TurnOptions},
};
use crate::chat::presentation;
use crate::chat::transcript::{
    Body, Decision, ItemId, Notice, NoticeKind, PermissionKind, Remember, Resolution,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

/// Where a Codex conversation starts: a new thread, a saved one, or a saved one cut
/// after a turn (an edited message).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodexStart {
    pub cwd: PathBuf,
    pub resume: Option<String>,
    pub fork_at: Option<String>,
    /// The chat's attachment folder: images go to Codex by path.
    pub images: Option<PathBuf>,
    /// The goal the conversation carried ends (an edited message): cleared once the
    /// thread is open.
    pub clear_goal: bool,
}

#[derive(Default)]
pub(super) struct CodexEngine {
    pub adapter: Adapter,
    pub start: CodexStart,
    thread_request: Option<String>,
    models_request: Option<String>,
    skills_request: Option<String>,
    catalog_failed: bool,
    account_request: Option<String>,
    rate_request: Option<String>,
    /// Our `turn/start` in flight, to report its failure.
    pub(super) turn_request: Option<String>,
    /// Setting a goal: the steps still to send and the one awaiting its answer.
    pub(super) goal_steps: std::collections::VecDeque<super::goal_engine::GoalStep>,
    pub(super) goal_step: Option<String>,
    /// Before a saved thread opens its goal is paused, or Codex would start working on
    /// it by itself: the goal asked for, the pause, and the open request waiting.
    restore_goal: Option<String>,
    restore_pause: Option<String>,
    thread_line: Option<Value>,
    /// The thread's goal is being cleared (`CodexStart::clear_goal`): its reports until
    /// then are not this chat's goal.
    pub(super) discarding_goal: bool,
    discard_request: Option<String>,
    /// Steers in flight: request id → the queued message's steering id.
    pub(super) steers: HashMap<String, String>,
    /// `account/read`: "chatgpt" or "apiKey".
    pub account: Option<String>,
    /// Stop came before the turn's id: it's sent once the id is known.
    interrupt_waiting: bool,
}

impl ChatSession {
    pub(super) fn codex_catalog_pending(&self) -> bool {
        self.codex.models_request.is_some()
            || self.codex.skills_request.is_some()
            || self.codex.catalog_failed
    }

    /// A session talking to `codex app-server`; returns the lines to write immediately.
    pub fn codex(
        settings: crate::chat::config::ChatSettings,
        start: CodexStart,
    ) -> (Self, Vec<Value>) {
        let mut session = Self::base(settings, crate::chat::provider::Provider::Codex);
        session.codex.start = start;
        session.initialize_id = session.request_id();
        let line = wire::initialize(&session.initialize_id);
        (session, vec![line])
    }

    fn thread_options(&self) -> ThreadOptions<'_> {
        ThreadOptions {
            cwd: &self.codex.start.cwd,
            mode: self.settings.mode,
            model: self.settings.model.as_deref(),
        }
    }

    /// The account's usage limit (ChatGPT plans).
    pub fn rate_limit(&self) -> Option<adapter::RateLimit> {
        self.codex.adapter.rate
    }

    pub(super) fn codex_thread(&self) -> Option<&str> {
        self.codex.adapter.thread.as_deref()
    }

    pub(super) fn codex_send(&mut self, draft: &Draft, uuid: &str) -> Vec<Value> {
        let Some(thread) = self.codex.adapter.thread.clone() else {
            return vec![];
        };
        let _ = uuid;
        let id = self.request_id();
        self.codex.turn_request = Some(id.clone());
        let input = wire::input(
            &draft.text,
            &draft.attachments,
            draft.context.as_deref(),
            self.codex.start.images.as_deref(),
        );
        vec![wire::turn_start(
            &id,
            &thread,
            input,
            &TurnOptions {
                mode: self.settings.mode,
                model: self.settings.model.as_deref(),
                effort: self.settings.effort,
            },
        )]
    }

    /// Writes `draft` into the running turn; `None` when there's no turn to steer.
    pub(super) fn codex_steer(&mut self, draft: &Draft) -> Option<(String, Value)> {
        let thread = self.codex.adapter.thread.clone()?;
        let turn = self.codex.adapter.turn.clone()?;
        let id = self.request_id();
        let input = wire::input(
            &draft.text,
            &draft.attachments,
            draft.context.as_deref(),
            self.codex.start.images.as_deref(),
        );
        self.codex.steers.insert(id.clone(), id.clone());
        Some((id.clone(), wire::turn_steer(&id, &thread, &turn, input)))
    }

    pub(super) fn codex_interrupt(&mut self) -> Vec<Value> {
        let mut lines = vec![];
        for request in self.codex.adapter.pending_permissions() {
            if let Some(id) = self.codex.adapter.request_id(&request) {
                lines.push(wire::reply(
                    &id,
                    cancel_answer(&self.permission_tool(&request)),
                ));
            }
            self.codex.adapter.resolve_with(
                &request,
                Decision::Cancelled,
                None,
                &mut self.transcript,
            );
        }
        match self.codex_interrupt_line() {
            Some(line) => lines.push(line),
            // The turn's id comes with the answer to `turn/start`.
            None => self.codex.interrupt_waiting = true,
        }
        lines
    }

    fn codex_interrupt_line(&mut self) -> Option<Value> {
        let thread = self.codex.adapter.thread.clone()?;
        let turn = self.codex.adapter.turn.clone()?;
        let id = self.request_id();
        self.interrupt_id = Some(id.clone());
        Some(wire::turn_interrupt(&id, &thread, &turn))
    }

    fn permission_tool(&self, request: &str) -> String {
        self.transcript
            .items()
            .iter()
            .rev()
            .find_map(|item| match &item.body {
                Body::Permission(p) if p.request_id == request => Some(p.tool.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub(super) fn codex_respond(&mut self, item: ItemId, response: Response) -> Vec<Value> {
        let Some(permission) = self.transcript.position(item).and_then(|index| {
            match &self.transcript.items()[index].body {
                Body::Permission(p) if p.decision == Decision::Pending => Some(p.clone()),
                _ => None,
            }
        }) else {
            return vec![];
        };
        let Some(id) = self.codex.adapter.request_id(&permission.request_id) else {
            return vec![];
        };
        let tool = permission.tool.as_str();
        let mut follow = None;
        let (decision, resolution, result) = match (response, permission.kind) {
            (Response::Allow, PermissionKind::Tool) => (
                Decision::Allowed,
                Resolution::Allowed,
                allow_answer(tool, &permission.input, false),
            ),
            (Response::AllowForChat, PermissionKind::Tool)
                if adapter::offers(&permission.input, "acceptForSession") =>
            {
                (
                    Decision::Allowed,
                    Resolution::Remembered(Remember::Session),
                    allow_answer(tool, &permission.input, true),
                )
            }
            (Response::Deny(note), _) => {
                let note = clean(note);
                // Codex has no message with a refusal: what to do instead follows as input.
                follow = note
                    .as_ref()
                    .map(|note| format!("The user declined that and said: {note}"));
                // Without "decline" on offer, refusing cancels the turn.
                let decline = adapter::offers(&permission.input, "decline");
                let result = match permission.kind {
                    PermissionKind::Question => json!({"answers": {}}),
                    _ => cancel_or_decline(tool, !decline),
                };
                if !decline {
                    follow = None;
                }
                (Decision::Denied, Resolution::Denied(note), result)
            }
            (Response::Answer(answers), PermissionKind::Question) => {
                let questions = presentation::questions(&permission);
                let ids: Vec<String> = permission.input["questions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|q| q["id"].as_str().unwrap_or_default().to_owned())
                    .collect();
                let mut pairs = vec![];
                let mut map = serde_json::Map::new();
                for ((question, answer), id) in questions.into_iter().zip(answers).zip(ids) {
                    let Some(answer) = clean(Some(answer)) else {
                        continue;
                    };
                    map.insert(id, json!({"answers": [answer.clone()]}));
                    pairs.push((question.question, answer));
                }
                if pairs.is_empty() {
                    return vec![];
                }
                (
                    Decision::Allowed,
                    Resolution::Answered(pairs),
                    json!({"answers": map}),
                )
            }
            _ => return vec![],
        };
        if !self.codex.adapter.resolve_with(
            &permission.request_id,
            decision,
            Some(resolution),
            &mut self.transcript,
        ) {
            return vec![];
        }
        if self.status == Status::NeedsDecision
            && self.codex.adapter.pending_permissions().is_empty()
        {
            self.status = Status::Working;
        }
        let mut lines = vec![wire::reply(&id, result)];
        if let Some(text) = follow
            && let Some((_, line)) = self.codex_steer(&Draft::text(text))
        {
            lines.push(line);
        }
        let _ = DENIED_MESSAGE;
        lines
    }

    pub fn on_codex(&mut self, message: Message) -> Vec<Value> {
        if self.status.ended() {
            return vec![];
        }
        let signals = self.codex.adapter.apply(message, &mut self.transcript);
        // A goal message starts its turn without `turn/start`: its first event is evidence.
        if self.awaiting.is_some()
            && self.awaiting_steer.is_none()
            && self.codex.turn_request.is_none()
            && signals
                .iter()
                .any(|signal| !matches!(signal, Signal::ControlAck { .. }))
        {
            self.receipt(None, crate::chat::backend::Receipt::Delivered);
        }
        let mut lines = vec![];
        if std::mem::take(&mut self.codex.adapter.skills_changed) && self.initialized {
            let id = self.request_id();
            lines.push(wire::skills_list(&id, &self.codex.start.cwd, true));
            self.codex.skills_request = Some(id);
        }
        for signal in signals {
            match signal {
                Signal::ControlAck {
                    request_id,
                    error,
                    response,
                } => lines.extend(self.codex_ack(&request_id, error, response)),
                Signal::State(CliState::Running) => {
                    self.cli_idle = false;
                    if !self.turn_open {
                        // Codex continued on its own: toward its goal, or a subagent
                        // reported back.
                        let turn = self.transcript.allocate_turn();
                        self.start_turn(turn);
                        self.goal_turn_started();
                    }
                }
                // The turn ends with `turn/completed`; idle only says nothing else runs.
                Signal::State(_) => self.cli_idle = true,
                Signal::PermissionRequested if !self.interrupting => {
                    self.status = Status::NeedsDecision;
                }
                Signal::PermissionWithdrawn
                    if self.status == Status::NeedsDecision
                        && self.codex.adapter.pending_permissions().is_empty() =>
                {
                    self.status = Status::Working;
                }
                Signal::TurnFinished => {
                    let completed = self.transcript.items().last().is_some_and(|item| {
                        matches!(&item.body, Body::TurnEnd(end)
                            if end.outcome == crate::chat::transcript::Outcome::Completed)
                    });
                    let interrupted = self.interrupting;
                    self.turn_open = false;
                    self.interrupting = false;
                    self.cli_idle = true;
                    // A steer the turn never took waits again.
                    for queued in &mut self.queue {
                        queued.steering = None;
                    }
                    if !self.status.ended() {
                        self.status = Status::Ready;
                    }
                    self.hold_for_goal(completed && !interrupted);
                }
                Signal::Reply(line) => lines.push(line),
                Signal::Goal(event) => lines.extend(self.on_goal(event)),
                _ => {}
            }
        }
        lines.extend(self.flush());
        lines
    }

    fn codex_ack(
        &mut self,
        request: &str,
        error: Option<String>,
        response: Option<Value>,
    ) -> Vec<Value> {
        let result = response.unwrap_or(Value::Null);
        let mut lines = vec![];
        if self.codex.discard_request.as_deref() == Some(request) {
            // Cleared (or there was none): later reports are the chat's own again.
            self.codex.discard_request = None;
            self.codex.discarding_goal = false;
            return lines;
        }
        if self.codex.goal_step.as_deref() == Some(request) {
            let setting = matches!(
                self.codex.goal_steps.front(),
                Some(super::goal_engine::GoalStep::Turn(_))
            );
            if let (true, Some(error)) = (setting, &error) {
                // Codex took no goal (an older Codex): the message still goes as a turn.
                self.codex
                    .goal_steps
                    .retain(|step| !matches!(step, super::goal_engine::GoalStep::Activate));
                self.goal = None;
                self.goal_changed = true;
                self.note(format!("Codex couldn't set the goal: {error}"));
            }
            lines.extend(self.next_goal_step());
        }
        if self.codex.restore_goal.as_deref() == Some(request) {
            self.codex.restore_goal = None;
            let active =
                error.is_none() && wire::goal(&result).is_some_and(|goal| goal.status == "active");
            match (active, self.codex.start.resume.clone()) {
                (true, Some(thread)) => {
                    let id = self.request_id();
                    lines.push(wire::goal_set(&id, &thread, None, Some("paused")));
                    self.codex.restore_pause = Some(id);
                }
                _ => lines.extend(self.codex.thread_line.take()),
            }
            return lines;
        }
        if self.codex.restore_pause.as_deref() == Some(request) {
            self.codex.restore_pause = None;
            lines.extend(self.codex.thread_line.take());
            return lines;
        }
        if request == self.initialize_id {
            if let Some(error) = error {
                self.status = Status::Failed(format!("Codex could not start: {error}"));
                return lines;
            }
            lines.push(wire::initialized());
            let id = self.request_id();
            let options = self.thread_options();
            let line = match (&self.codex.start.resume, &self.codex.start.fork_at) {
                (Some(thread), Some(turn)) => wire::thread_fork(&id, thread, turn, &options),
                (Some(thread), None) => wire::thread_resume(&id, thread, &options),
                (None, _) => wire::thread_start(&id, &options),
            };
            self.codex.thread_request = Some(id);
            match self.codex.start.resume.clone() {
                // A saved thread may hold an active goal: paused first (`restore_goal`).
                Some(thread) => {
                    let id = self.request_id();
                    lines.push(wire::goal_get(&id, &thread));
                    self.codex.restore_goal = Some(id);
                    self.codex.thread_line = Some(line);
                }
                None => lines.push(line),
            }
            let id = self.request_id();
            lines.push(wire::skills_list(&id, &self.codex.start.cwd, false));
            self.codex.skills_request = Some(id);
            for (slot, build) in [
                (0, wire::model_list as fn(&str) -> Value),
                (1, wire::account_read),
                (2, wire::rate_limits_read),
            ] {
                let id = self.request_id();
                lines.push(build(&id));
                let slot = match slot {
                    0 => &mut self.codex.models_request,
                    1 => &mut self.codex.account_request,
                    _ => &mut self.codex.rate_request,
                };
                *slot = Some(id);
            }
            return lines;
        }
        if self.codex.thread_request.as_deref() == Some(request) {
            self.codex.thread_request = None;
            let Some(thread) = result["thread"]["id"].as_str().filter(|_| error.is_none()) else {
                let resumed = self.codex.start.resume.is_some();
                self.status = Status::Failed(format!(
                    "Codex couldn't {} the conversation: {}",
                    if resumed { "continue" } else { "open" },
                    error.unwrap_or_else(|| "no thread".into())
                ));
                return lines;
            };
            self.codex.adapter.thread = Some(thread.to_owned());
            self.info.session_id = Some(thread.to_owned());
            if std::mem::take(&mut self.codex.start.clear_goal) {
                let id = self.request_id();
                lines.push(wire::goal_clear(&id, thread));
                self.codex.discarding_goal = true;
                self.codex.discard_request = Some(id);
            }
            if let Some(model) = result["model"].as_str() {
                self.info.model = Some(model.to_owned());
            }
            self.initialized = true;
            if self.status == Status::Starting {
                self.status = if self.turn_open {
                    Status::Working
                } else {
                    Status::Ready
                };
            }
            lines.append(&mut self.held);
            return lines;
        }
        if self.codex.models_request.as_deref() == Some(request) {
            self.codex.models_request = None;
            self.codex.catalog_failed |= error.is_some();
            self.models = wire::models(&result);
            return lines;
        }
        if self.codex.skills_request.as_deref() == Some(request) {
            self.codex.skills_request = None;
            self.codex.catalog_failed |= error.is_some();
            if error.is_none() {
                self.commands = wire::skills(&result);
            }
            return lines;
        }
        if self.codex.account_request.as_deref() == Some(request) {
            self.codex.account_request = None;
            self.codex.account = result["account"]["type"].as_str().map(str::to_owned);
            return lines;
        }
        if self.codex.rate_request.as_deref() == Some(request) {
            self.codex.rate_request = None;
            if let Some(rate) = adapter::rate_limit_result(&result) {
                self.codex.adapter.rate = Some(rate);
            }
            return lines;
        }
        if let Some(steering) = self.codex.steers.remove(request) {
            if self.awaiting.as_deref() == Some(steering.as_str())
                && let Some(draft) = self.awaiting_steer.take()
            {
                if error.is_none() {
                    self.push_steered(draft);
                    self.receipt(Some(&steering), crate::chat::backend::Receipt::Delivered);
                } else {
                    self.receipt(Some(&steering), crate::chat::backend::Receipt::Refused);
                }
                return lines;
            }
            let index = self
                .queue
                .iter()
                .position(|q| q.steering.as_deref() == Some(steering.as_str()));
            match (error, index) {
                (None, Some(index)) => {
                    let queued = self.queue.remove(index).expect("steered message");
                    self.push_steered(queued.draft());
                }
                (Some(_), Some(index)) => self.queue[index].steering = None,
                _ => {}
            }
            return lines;
        }
        if error.is_none() && self.codex.turn_request.as_deref() == Some(request) {
            self.codex.turn_request = None;
            if self.awaiting_steer.is_none() {
                self.receipt(None, crate::chat::backend::Receipt::Delivered);
            }
            if let Some(turn) = result["turn"]["id"].as_str() {
                self.codex.adapter.turn = Some(turn.to_owned());
            }
            if std::mem::take(&mut self.codex.interrupt_waiting)
                && let Some(line) = self.codex_interrupt_line()
            {
                lines.push(line);
            }
            return lines;
        }
        if let Some(error) = error {
            if self.codex.turn_request.as_deref() == Some(request) {
                self.codex.turn_request = None;
                if self.awaiting_steer.is_none() {
                    self.receipt(None, crate::chat::backend::Receipt::Refused);
                }
                self.transcript.push(
                    None,
                    Body::Notice(Notice {
                        kind: NoticeKind::Error,
                        text: format!("Codex couldn't start the turn: {error}"),
                        until: None,
                    }),
                );
                self.codex.adapter.close_turn(
                    &mut self.transcript,
                    &crate::chat::transcript::Outcome::Interrupted,
                );
                self.turn_open = false;
                if !self.status.ended() {
                    self.status = Status::Ready;
                }
            } else if self.interrupt_id.as_deref() == Some(request) {
                self.interrupting = false;
                if self.status == Status::Interrupting {
                    self.status = Status::Working;
                }
                self.transcript.push(
                    None,
                    Body::Notice(Notice {
                        kind: NoticeKind::Error,
                        text: format!("Codex could not stop the turn: {error}"),
                        until: None,
                    }),
                );
            }
        }
        lines
    }
}

/// The approval for `tool`; `session` keeps it until the agent restarts.
fn allow_answer(tool: &str, input: &Value, session: bool) -> Value {
    match tool {
        "Permissions" => json!({
            "permissions": input["permissions"],
            "scope": if session { "session" } else { "turn" },
        }),
        _ => json!({"decision": if session { "acceptForSession" } else { "accept" }}),
    }
}

fn cancel_or_decline(tool: &str, cancel: bool) -> Value {
    match tool {
        "Permissions" => json!({"permissions": {}, "scope": "turn"}),
        _ => json!({"decision": if cancel { "cancel" } else { "decline" }}),
    }
}

fn cancel_answer(tool: &str) -> Value {
    match tool {
        "AskUserQuestion" => json!({"answers": {}}),
        other => cancel_or_decline(other, true),
    }
}
