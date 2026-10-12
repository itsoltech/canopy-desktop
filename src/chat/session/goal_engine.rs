//! The chat's goal (`/goal`) in `ChatSession`: setting, pausing, clearing and what
//! each agent reports about it. Claude runs a goal inside one long turn and answers
//! `/goal` commands itself; Codex keeps it on its thread and starts turn after turn.
use super::{ChatSession, Draft, Status};
use crate::chat::claude::wire::{self, Priority};
use crate::chat::codex::wire as codex_wire;
use crate::chat::goal::{self, EndReason, Goal, GoalEvent, GoalStatus};
use crate::chat::provider::Provider;
use crate::chat::transcript::{Body, GoalCheck, ItemId};
use serde_json::Value;

/// Codex's steps to set a goal, one at a time: each waits for the answer to the last.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum GoalStep {
    /// A new condition replaces the old goal (and its accounting). Paused until our first turn runs, so Codex starts none of its own first.
    SetPaused(String),
    /// The first turn carries the condition (built when it's due).
    Turn(Value),
    Activate,
}

use goal::now as now_seconds;

impl ChatSession {
    pub fn goal(&self) -> Option<&Goal> {
        self.goal.as_ref()
    }

    /// The agent is working toward the goal right now.
    pub fn pursuing_goal(&self) -> bool {
        self.goal
            .as_ref()
            .is_some_and(|goal| goal.status == GoalStatus::Pursuing)
            && matches!(
                self.status,
                Status::Working | Status::NeedsDecision | Status::Interrupting
            )
    }

    /// True once after each change of the goal: the owner saves it with the chat.
    pub fn take_goal_changed(&mut self) -> bool {
        std::mem::take(&mut self.goal_changed)
    }

    /// The goal saved with the chat, before the session's first event. It doesn't run
    /// on its own: Claude waits for the next message, Codex is paused before its thread
    /// opens (`CodexStart`) and reports it again.
    pub fn restore_goal(&mut self, goal: Option<Goal>) {
        self.goal = goal.map(|mut goal| {
            goal.status = match self.provider {
                Provider::Claude => GoalStatus::Set,
                Provider::Codex if goal.status == GoalStatus::Pursuing => GoalStatus::Paused,
                Provider::Codex => goal.status,
            };
            goal
        });
    }

    /// Sets the goal: `draft.text` is its condition. Starts its first turn right away,
    /// or after the current one.
    pub fn set_goal(&mut self, mut draft: Draft) -> Vec<Value> {
        draft.goal = true;
        draft.attachments.clear();
        self.submit_draft(draft)
    }

    /// Codex: no further goal turns; the one running finishes.
    pub fn pause_goal(&mut self) -> Vec<Value> {
        self.codex_goal_status("paused")
    }

    /// Codex: continues a paused or stopped goal (Codex starts the turn itself).
    pub fn resume_goal(&mut self) -> Vec<Value> {
        if !self
            .goal
            .as_ref()
            .is_some_and(|goal| goal.status.resumable())
        {
            return vec![];
        }
        self.stopped = false;
        self.codex_goal_status("active")
    }

    fn codex_goal_status(&mut self, status: &str) -> Vec<Value> {
        if self.provider != Provider::Codex || self.goal.is_none() || self.status.ended() {
            return vec![];
        }
        let Some(thread) = self.codex_thread().map(str::to_owned) else {
            return vec![];
        };
        let id = self.request_id();
        self.outgoing(vec![codex_wire::goal_set(&id, &thread, None, Some(status))])
    }

    /// Removes the goal; `reason` names why on its ending card. Claude's running turn
    /// is stopped first. Without a live agent it just ends here.
    pub fn clear_goal(&mut self, reason: EndReason) -> Vec<Value> {
        if self.goal.is_none() {
            return vec![];
        }
        if self.status.ended() || !self.initialized {
            self.end_goal(reason, None);
            return vec![];
        }
        self.goal_clearing = Some(reason);
        match self.provider {
            Provider::Codex => {
                let Some(thread) = self.codex_thread().map(str::to_owned) else {
                    self.end_goal(reason, None);
                    return vec![];
                };
                let id = self.request_id();
                self.outgoing(vec![codex_wire::goal_clear(&id, &thread)])
            }
            Provider::Claude => {
                let mut lines = if self.turn_open {
                    self.interrupt()
                } else {
                    vec![]
                };
                lines.extend(self.send_quiet("/goal clear"));
                lines
            }
        }
    }

    /// Ends the goal in Canopy only: its agent is going away (a switch).
    pub fn drop_goal(&mut self, reason: EndReason) {
        self.end_goal(reason, None);
    }

    /// Codex: the goal continues, but Codex started no turn for it after the last one.
    pub fn goal_hold(&self) -> bool {
        self.goal_hold
    }
    /// Lets a goal hold go: the session is ready for messages again.
    pub fn expire_goal_hold(&mut self) -> Vec<Value> {
        if !std::mem::take(&mut self.goal_hold) {
            return vec![];
        }
        if self.status == Status::Working && !self.turn_open {
            self.status = Status::Ready;
        }
        self.flush()
    }

    /// Corrects the card of a finished Claude goal with what its session recorded
    /// (whether it was met, and its numbers).
    pub fn amend_goal_end(
        &mut self,
        item: ItemId,
        reason: EndReason,
        note: Option<String>,
        seconds: Option<u64>,
        tokens: Option<u64>,
    ) {
        let Some(index) = self.transcript.position(item) else {
            return;
        };
        self.transcript.update(index, |body| {
            if let Body::GoalEnd(end) = body {
                end.reason = reason;
                if note.is_some() {
                    end.note = note;
                }
                end.seconds = seconds.or(end.seconds);
                end.tokens = tokens.or(end.tokens);
            }
        });
    }

    /// A command of Canopy's own (Claude): sent when the agent can take it, before the
    /// queue, without a message in the conversation.
    pub(super) fn send_quiet(&mut self, text: &str) -> Vec<Value> {
        if self.provider != Provider::Claude || self.status.ended() {
            return vec![];
        }
        self.quiet.push_back(text.to_owned());
        self.flush()
    }

    /// Sends the oldest quiet command when nothing runs.
    pub(super) fn flush_quiet(&mut self) -> Option<Vec<Value>> {
        if !self.initialized || self.status.ended() || !self.accepts_turn() {
            return None;
        }
        let text = self.quiet.pop_front()?;
        let turn = self.transcript.allocate_turn();
        let uuid = uuid::Uuid::new_v4().to_string();
        let line = wire::user_message(&text, &[], None, &uuid, Priority::Later);
        self.quiet_turns.insert(turn);
        self.start_turn(turn);
        self.outstanding.insert(uuid);
        Some(self.outgoing(vec![line]))
    }

    /// The turn that just ended carried a quiet command: its end isn't shown.
    pub(super) fn hide_quiet_end(&mut self) {
        let turn = self.transcript.current_turn();
        if !self.quiet_turns.remove(&turn) {
            return;
        }
        if let Some(index) = self
            .transcript
            .rposition(|item| item.turn == turn && matches!(item.body, Body::TurnEnd(_)))
        {
            self.transcript.update(index, |body| {
                if let Body::TurnEnd(end) = body {
                    end.quiet = true;
                }
            });
        }
    }

    fn quiet_turn(&self) -> bool {
        self.quiet_turns.contains(&self.transcript.current_turn())
    }

    /// The first goal message: a goal in Canopy until the agent confirms it.
    pub(super) fn start_goal(&mut self, objective: &str) {
        let mut goal = Goal::new(objective.to_owned(), GoalStatus::Pursuing, now_seconds());
        if self.provider == Provider::Codex {
            goal.turns = 1;
        }
        self.goal = Some(goal);
        self.goal_settling = false;
        self.goal_changed = true;
    }

    fn end_goal(&mut self, reason: EndReason, note: Option<String>) {
        let Some(goal) = self.goal.take() else {
            return;
        };
        self.goal_clearing = None;
        self.goal_settling = false;
        self.goal_changed = true;
        self.transcript
            .push(None, Body::GoalEnd(goal.end(reason, note, now_seconds())));
        self.release_goal_hold();
    }

    fn release_goal_hold(&mut self) {
        if std::mem::take(&mut self.goal_hold) && self.status == Status::Working && !self.turn_open
        {
            self.status = Status::Ready;
        }
    }

    fn goal_status(&mut self, status: GoalStatus) {
        if let Some(goal) = &mut self.goal
            && goal.status != status
        {
            goal.status = status;
            self.goal_changed = true;
        }
    }

    /// Codex finished a turn: while its goal is still pursued the next turn follows.
    pub(super) fn hold_for_goal(&mut self, completed: bool) {
        if completed
            && !self.interrupting
            && self
                .goal
                .as_ref()
                .is_some_and(|goal| goal.status == GoalStatus::Pursuing)
        {
            self.goal_hold = true;
            self.status = Status::Working;
        }
    }

    /// Codex started a turn of its own: one more turn toward the goal.
    pub(super) fn goal_turn_started(&mut self) {
        let held = std::mem::take(&mut self.goal_hold);
        let Some(goal) = &mut self.goal else {
            return;
        };
        if !held && goal.status != GoalStatus::Pursuing {
            // Resume: Codex continues the goal.
            if !goal.status.resumable() {
                return;
            }
            goal.status = GoalStatus::Pursuing;
        }
        goal.turns += 1;
        let n = goal.turns;
        self.goal_changed = true;
        self.transcript.push(None, Body::GoalTurn(n));
    }

    /// Codex: a goal draft sets the goal first; its turn and activation follow
    /// (`GoalStep`). Returns the first line.
    pub(super) fn codex_goal_send(&mut self, draft: &Draft) -> Vec<Value> {
        let uuid = String::new();
        let turn = self.codex_send(draft, &uuid);
        let Some(turn) = turn.into_iter().next() else {
            return vec![];
        };
        self.codex.goal_steps = [
            GoalStep::SetPaused(draft.text.clone()),
            GoalStep::Turn(turn),
            GoalStep::Activate,
        ]
        .into();
        self.next_goal_step()
    }

    /// Sends Codex's next step to set a goal (after the answer to the last).
    pub(super) fn next_goal_step(&mut self) -> Vec<Value> {
        self.codex.goal_step = None;
        let Some(thread) = self.codex_thread().map(str::to_owned) else {
            self.codex.goal_steps.clear();
            return vec![];
        };
        let Some(step) = self.codex.goal_steps.pop_front() else {
            return vec![];
        };
        let (id, line) = match step {
            GoalStep::SetPaused(objective) => {
                let id = self.request_id();
                let line = codex_wire::goal_set(&id, &thread, Some(&objective), Some("paused"));
                (id, line)
            }
            GoalStep::Turn(line) => match self.codex.turn_request.clone() {
                Some(id) => (id, line),
                None => return self.next_goal_step(),
            },
            GoalStep::Activate => {
                let id = self.request_id();
                (
                    id.clone(),
                    codex_wire::goal_set(&id, &thread, None, Some("active")),
                )
            }
        };
        self.codex.goal_step = Some(id);
        vec![line]
    }

    /// Stop with a Codex goal: pause it so Codex starts no further turn. A goal whose
    /// first turn wasn't sent yet ends that turn here; `true` then (nothing to interrupt).
    pub(super) fn codex_stop_goal(&mut self) -> (Vec<Value>, bool) {
        if self.provider != Provider::Codex {
            return (vec![], false);
        }
        let mut lines = vec![];
        let unsent = self
            .codex
            .goal_steps
            .iter()
            .any(|step| matches!(step, GoalStep::Turn(_)));
        // The goal stays paused: no activation after the turn.
        self.codex.goal_steps.clear();
        let pursued = self
            .goal
            .as_ref()
            .is_some_and(|goal| goal.status == GoalStatus::Pursuing);
        if pursued && let Some(thread) = self.codex_thread().map(str::to_owned) {
            let id = self.request_id();
            lines.push(codex_wire::goal_set(&id, &thread, None, Some("paused")));
            self.goal_status(GoalStatus::Paused);
        }
        if unsent && self.turn_open {
            self.codex.turn_request = None;
            self.codex.adapter.close_turn(
                &mut self.transcript,
                &crate::chat::transcript::Outcome::Interrupted,
            );
            self.transcript.push(
                None,
                Body::TurnEnd(crate::chat::transcript::TurnEnd {
                    outcome: crate::chat::transcript::Outcome::Interrupted,
                    usage: None,
                    cost_usd: None,
                    duration_ms: None,
                    resume_at: None,
                    quiet: false,
                }),
            );
            self.turn_open = false;
            self.interrupting = false;
            if !self.status.ended() {
                self.status = Status::Ready;
            }
            return (lines, true);
        }
        (lines, false)
    }

    /// Codex works toward the goal: a waiting message goes into its running turn
    /// instead of waiting for the goal to end.
    pub(super) fn steer_into_goal(&mut self) -> Option<Vec<Value>> {
        if self.provider != Provider::Codex
            || self.hold_queue
            || self.interrupting
            || !self.turn_open
            || self.codex.adapter.turn.is_none()
            || self.queue.iter().any(|q| q.steering.is_some())
            || !self
                .goal
                .as_ref()
                .is_some_and(|goal| goal.status == GoalStatus::Pursuing)
        {
            return None;
        }
        let index = self.queue.iter().position(|q| !q.goal)?;
        if index != 0 {
            return None;
        }
        let draft = self.queue[index].clone().draft();
        let (id, line) = self.codex_steer(&draft)?;
        self.queue[index].steering = Some(id);
        Some(self.outgoing(vec![line]))
    }

    /// A continued conversation (a switch back, an edit) may still hold a goal this chat
    /// no longer has: Claude restores it with the session, so it's cleared first.
    pub fn clear_stale_goal(&mut self) -> Vec<Value> {
        if self.provider != Provider::Claude || self.goal.is_some() {
            return vec![];
        }
        self.send_quiet("/goal clear")
    }

    /// Applies what the agent reported; returns lines to write.
    pub(super) fn on_goal(&mut self, event: GoalEvent) -> Vec<Value> {
        if self.codex.discarding_goal {
            if event == GoalEvent::CodexCleared {
                self.codex.discarding_goal = false;
            }
            if matches!(event, GoalEvent::Codex(_) | GoalEvent::CodexCleared) {
                return vec![];
            }
        }
        match event {
            GoalEvent::Set(objective) => {
                let status = if self.turn_open {
                    GoalStatus::Pursuing
                } else {
                    GoalStatus::Set
                };
                match &mut self.goal {
                    Some(goal) if goal.objective == objective => goal.status = status,
                    _ => self.goal = Some(Goal::new(objective, status, now_seconds())),
                }
                self.goal_changed = true;
            }
            GoalEvent::Active {
                objective,
                last_check,
            } => {
                let settling = std::mem::take(&mut self.goal_settling);
                let idle = settling || !self.turn_open || self.quiet_turn();
                let goal = self.goal.get_or_insert_with(|| {
                    Goal::new(objective.clone(), GoalStatus::Set, now_seconds())
                });
                goal.objective = objective;
                if last_check.is_some() {
                    goal.last_check = last_check;
                }
                // Asked after its turn: the goal is still set (the agent stopped without
                // meeting it, e.g. it waits for background work).
                if idle {
                    goal.status = GoalStatus::Set;
                }
                self.goal_changed = true;
            }
            GoalEvent::Feedback(text) => {
                let Some(goal) = &mut self.goal else {
                    return vec![];
                };
                let Some(reason) = goal::parse_feedback(&text, &goal.objective) else {
                    return vec![];
                };
                goal.checks += 1;
                goal.last_check = Some(reason.clone());
                goal.status = GoalStatus::Pursuing;
                let n = goal.checks;
                self.goal_changed = true;
                self.transcript
                    .push(None, Body::GoalCheck(GoalCheck { n, reason }));
            }
            GoalEvent::Cleared(Some(_)) => {
                let reason = self.goal_clearing.unwrap_or(EndReason::ClearedByUser);
                self.end_goal(reason, None);
            }
            GoalEvent::Cleared(None) => {
                if std::mem::take(&mut self.goal_settling) {
                    // Gone after its own turn: the condition held (or proved impossible;
                    // the owner corrects the card from the session's record).
                    self.end_goal(EndReason::Achieved, None);
                } else if let Some(reason) = self.goal_clearing {
                    self.end_goal(reason, None);
                } else if self.goal.take().is_some() {
                    // The agent has none (e.g. a restored chat whose session lost it).
                    self.goal_changed = true;
                }
            }
            GoalEvent::ClearedByError(text) => self.end_goal(EndReason::ClearedByError, Some(text)),
            GoalEvent::Note(text) => self.note(text),
            GoalEvent::TurnEnded { completed } => {
                let pursued = self
                    .goal
                    .as_ref()
                    .is_some_and(|goal| goal.status == GoalStatus::Pursuing);
                if pursued && !self.quiet_turn() {
                    if completed && self.goal_clearing.is_none() {
                        // Ran to its end on its own: ask whether the goal is still set.
                        self.goal_settling = true;
                        return self.send_quiet("/goal");
                    }
                    self.goal_status(GoalStatus::Set);
                }
            }
            GoalEvent::Codex(reported) => {
                let Some(status) = reported.status() else {
                    if self
                        .goal
                        .as_ref()
                        .is_some_and(|goal| goal.objective == reported.objective)
                    {
                        if let Some(goal) = &mut self.goal {
                            goal.tokens = Some(reported.tokens);
                            goal.seconds = Some(reported.seconds);
                        }
                        self.end_goal(EndReason::Achieved, None);
                    }
                    return vec![];
                };
                // Setting a goal passes through paused (`GoalStep`): it isn't paused.
                let status = if status == GoalStatus::Paused && self.codex.goal_step.is_some() {
                    GoalStatus::Pursuing
                } else {
                    status
                };
                let goal = match &mut self.goal {
                    Some(goal) if goal.objective == reported.objective => goal,
                    _ => {
                        let mut goal = Goal::new(reported.objective.clone(), status, now_seconds());
                        goal.turns = 1;
                        self.goal.insert(goal)
                    }
                };
                goal.status = status;
                goal.tokens = Some(reported.tokens);
                goal.seconds = Some(reported.seconds);
                goal.budget = reported.budget;
                self.goal_changed = true;
                if status != GoalStatus::Pursuing {
                    self.release_goal_hold();
                }
            }
            GoalEvent::CodexCleared => {
                let reason = self.goal_clearing.unwrap_or(EndReason::ClearedByUser);
                self.end_goal(reason, None);
            }
        }
        vec![]
    }
}
