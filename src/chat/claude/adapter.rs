//! Folds Claude stream-json messages into the provider-neutral transcript.
//! Pure: no I/O, so recorded sessions replay deterministically in tests.
use super::wire::{self, ApiEvent, Block, Delta, Inbound, System};
use crate::chat::presentation::HANDBACK_TOOL;
use crate::chat::transcript::{
    Body, Decision, Notice, NoticeKind, Outcome, Permission, PermissionKind, Resolution, TextBlock,
    ToolCall, ToolOutput, ToolState, Transcript, TurnEnd, Usage,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InitInfo {
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    pub api_key_source: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliState {
    Running,
    Idle,
    RequiresAction,
}

/// What the session controller must react to after a message was applied.
#[derive(Debug, PartialEq)]
pub enum Signal {
    ControlAck {
        request_id: String,
        error: Option<String>,
        /// Success payload (e.g. models and permission mode for `initialize`).
        response: Option<Value>,
    },
    Init(InitInfo),
    SessionId(String),
    State(CliState),
    PermissionRequested,
    PermissionWithdrawn,
    TurnFinished,
    /// Number of non-ambient background tasks the CLI is tracking.
    BackgroundTasks(usize),
    /// The CLI changed the permission mode itself (e.g. leaving plan mode).
    PermissionMode(String),
    /// A reply that must be written back to the CLI.
    Reply(Value),
    /// Progress of one of our user messages.
    Lifecycle {
        uuid: String,
        state: wire::Lifecycle,
    },
}

#[derive(Default)]
pub struct Adapter {
    /// (message id, content block index) → transcript index, for the current turn.
    blocks: HashMap<(String, usize), usize>,
    /// Message currently streaming in each thread (main or a subagent's tool call).
    streaming: HashMap<Option<String>, String>,
    /// Messages whose content arrived through stream events; snapshots only confirm them.
    streamed: HashSet<String>,
    tools: HashMap<String, usize>,
    /// Background tool calls outlive their turn until the task notification arrives.
    background: HashMap<String, usize>,
    /// Tool call → the call that spawned it (subagent output), for background ancestry.
    parents: HashMap<String, Option<String>>,
    /// Subagent call → the report it handed back (`SubagentHandback`).
    handbacks: HashMap<String, String>,
    permissions: HashMap<String, usize>,
    /// The main thread's latest message uuid, kept with the turn's end.
    last_message: Option<String>,
}

impl Adapter {
    /// Main-thread output that starts a model response. Outside an open turn this is
    /// the agent resuming on its own (e.g. after a background subagent reported).
    pub fn opens_turn(message: &Inbound) -> bool {
        match message {
            Inbound::StreamEvent(event) => {
                event.parent_tool_use_id.is_none()
                    && matches!(event.event, ApiEvent::MessageStart { .. })
            }
            Inbound::Assistant(message) => message.parent_tool_use_id.is_none(),
            _ => false,
        }
    }

    pub fn apply(
        &mut self,
        message: Inbound,
        transcript: &mut Transcript,
        interrupted: bool,
    ) -> Vec<Signal> {
        match message {
            Inbound::ControlResponse { response } => vec![Signal::ControlAck {
                error: (response.subtype == "error")
                    .then(|| response.error.unwrap_or_else(|| "Request failed.".into())),
                request_id: response.request_id,
                response: response.response,
            }],
            Inbound::ControlRequest {
                request_id,
                request,
            } => self.control_request(request_id, &request, transcript),
            Inbound::ControlCancelRequest { request_id } => {
                if self.resolve(&request_id, Decision::Cancelled, transcript) {
                    vec![Signal::PermissionWithdrawn]
                } else {
                    vec![]
                }
            }
            Inbound::System(system) => self.system(system, transcript),
            Inbound::StreamEvent(event) => {
                self.stream(event.event, event.parent_tool_use_id, transcript);
                vec![]
            }
            Inbound::Assistant(message) => {
                self.assistant(message, transcript);
                vec![]
            }
            Inbound::User(message) => {
                self.tool_results(&message, transcript);
                vec![]
            }
            Inbound::Result(result) => self.result(result, transcript, interrupted),
            Inbound::RateLimitEvent(event) => {
                let info = event.rate_limit_info;
                let text = match info.status.as_deref() {
                    Some("allowed_warning") => "Approaching the Claude usage limit",
                    Some("rejected") => "Claude usage limit reached",
                    _ => return vec![],
                };
                // Seconds since the epoch (milliseconds tolerated).
                let until = info
                    .resets_at
                    .as_ref()
                    .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
                    .map(|at| if at > 10_000_000_000 { at / 1000 } else { at });
                notice_until(transcript, None, NoticeKind::RateLimit, text.into(), until);
                vec![]
            }
            Inbound::ToolProgress(progress) => {
                if let Some(index) = self.tool_index(&progress.tool_use_id) {
                    transcript.update(index, |body| {
                        if let Body::Tool(tool) = body {
                            tool.elapsed_seconds = progress.elapsed_time_seconds;
                        }
                    });
                }
                vec![]
            }
            Inbound::CommandLifecycle {
                command_uuid,
                state,
            } => vec![Signal::Lifecycle {
                uuid: command_uuid,
                state,
            }],
            Inbound::Unknown => vec![],
        }
    }

    /// Records a decision for a pending permission; false when it was not pending.
    pub fn resolve(&mut self, request_id: &str, decision: Decision, t: &mut Transcript) -> bool {
        self.resolve_with(request_id, decision, None, t)
    }

    pub fn resolve_with(
        &mut self,
        request_id: &str,
        decision: Decision,
        resolution: Option<Resolution>,
        t: &mut Transcript,
    ) -> bool {
        let Some(index) = self.permissions.remove(request_id) else {
            return false;
        };
        t.update(index, |body| {
            if let Body::Permission(permission) = body {
                permission.decision = decision;
                permission.resolution = resolution;
            }
        });
        true
    }

    /// The process is gone: background calls and their subagents' tools never report back.
    pub fn cancel_background(&mut self, transcript: &mut Transcript) {
        let open: Vec<usize> = self
            .background
            .drain()
            .map(|(_, index)| index)
            .chain(self.tools.drain().map(|(_, index)| index))
            .chain(self.blocks.drain().map(|(_, index)| index))
            .collect();
        for index in open {
            transcript.update(index, |body| match body {
                Body::Tool(tool) if !tool.state.finished() => {
                    finish_input(tool);
                    tool.state = ToolState::Cancelled;
                    tool.activity = None;
                }
                Body::Text(block) | Body::Thinking(block) => block.streaming = false,
                _ => {}
            });
        }
        self.streaming.clear();
        self.streamed.clear();
        self.parents.clear();
        self.handbacks.clear();
    }

    /// Output of a subagent running in the background, which outlives the main turn.
    fn in_background(&self, parent: Option<&str>) -> bool {
        let mut parent = parent;
        // Bounded: a malformed parent cycle must not hang the UI.
        for _ in 0..64 {
            let Some(call) = parent else {
                return false;
            };
            if self.background.contains_key(call) {
                return true;
            }
            parent = self.parents.get(call).and_then(Option::as_deref);
        }
        false
    }

    /// Request ids of pending permissions, oldest first.
    pub fn pending_permissions(&self) -> Vec<String> {
        let mut pending: Vec<_> = self.permissions.iter().collect();
        pending.sort_by_key(|(_, index)| **index);
        pending.into_iter().map(|(id, _)| id.clone()).collect()
    }

    /// Closes everything still open in the current turn (interrupt, failure, exit).
    pub fn close_turn(&mut self, transcript: &mut Transcript, outcome: &Outcome) {
        for request in self.pending_permissions() {
            self.resolve(&request, Decision::Cancelled, transcript);
        }
        // Background subagents keep working after the main turn: their items stay open.
        let keep = |index: &usize| {
            self.background.values().any(|b| b == index)
                || transcript
                    .get(*index)
                    .is_some_and(|item| self.in_background(item.parent.as_deref()))
        };
        let (kept, open): (HashSet<usize>, HashSet<usize>) = self
            .blocks
            .values()
            .chain(self.tools.values())
            .copied()
            .partition(|index| keep(index));
        let cancel = *outcome != Outcome::Completed;
        for index in open {
            transcript.update(index, |body| match body {
                Body::Text(block) | Body::Thinking(block) => block.streaming = false,
                Body::Tool(tool) => {
                    finish_input(tool);
                    if cancel && !tool.state.finished() {
                        tool.state = ToolState::Cancelled;
                    }
                }
                _ => {}
            });
        }
        self.blocks.retain(|_, index| kept.contains(index));
        self.tools.retain(|_, index| kept.contains(index));
        let streaming: Vec<_> = self
            .streaming
            .keys()
            .filter(|parent| !self.in_background(parent.as_deref()))
            .cloned()
            .collect();
        for parent in streaming {
            if let Some(message) = self.streaming.remove(&parent) {
                self.streamed.remove(&message);
            }
        }
    }

    fn control_request(
        &mut self,
        request_id: String,
        request: &Value,
        transcript: &mut Transcript,
    ) -> Vec<Signal> {
        let Some(prompt) = wire::can_use_tool(request) else {
            return vec![Signal::Reply(wire::unsupported(
                &request_id,
                wire::request_subtype(request),
            ))];
        };
        let kind = match prompt.tool_name.as_str() {
            "AskUserQuestion" => PermissionKind::Question,
            "ExitPlanMode" => PermissionKind::Plan,
            _ => PermissionKind::Tool,
        };
        let remember = wire::remember(&prompt.permission_suggestions);
        let index = transcript.push(
            None,
            Body::Permission(Permission {
                request_id: request_id.clone(),
                call_id: prompt.tool_use_id,
                tool: prompt.tool_name,
                input: prompt.input,
                title: prompt.title.or(prompt.display_name),
                description: prompt.description.or(prompt.decision_reason),
                kind,
                decision: Decision::Pending,
                remember,
                resolution: None,
            }),
        );
        self.permissions.insert(request_id, index);
        vec![Signal::PermissionRequested]
    }

    fn system(&mut self, system: System, transcript: &mut Transcript) -> Vec<Signal> {
        match system {
            System::Init(init) => vec![Signal::Init(InitInfo {
                capabilities: init.capability_names(),
                session_id: init.session_id,
                model: init.model,
                permission_mode: init.permission_mode,
                version: init.claude_code_version,
                api_key_source: init.api_key_source,
            })],
            System::SessionStateChanged { state, session_id } => {
                let mut signals: Vec<_> = session_id.into_iter().map(Signal::SessionId).collect();
                match state.as_str() {
                    "running" => signals.push(Signal::State(CliState::Running)),
                    "idle" => signals.push(Signal::State(CliState::Idle)),
                    "requires_action" => signals.push(Signal::State(CliState::RequiresAction)),
                    _ => {}
                }
                signals
            }
            System::Status {
                permission_mode: Some(mode),
            } => vec![Signal::PermissionMode(mode)],
            System::Status { .. } => vec![],
            System::CompactBoundary { compact_metadata } => {
                let metadata = compact_metadata.unwrap_or_default();
                let mut text = "Conversation compacted".to_owned();
                if let Some(before) = metadata["pre_tokens"].as_u64() {
                    text.push_str(&format!(
                        " · {} tokens summarized",
                        crate::chat::presentation::tokens(before)
                    ));
                }
                match metadata["trigger"].as_str() {
                    Some("auto") => text.push_str(" (automatically, near the context limit)"),
                    Some("manual") => text.push_str(" (on request)"),
                    _ => {}
                }
                notice(transcript, None, NoticeKind::Compacted, text);
                vec![]
            }
            System::TaskStarted {
                tool_use_id: Some(call),
                is_backgrounded: Some(true),
                ambient,
            } if ambient != Some(true) => {
                if let Some(&index) = self.tools.get(&call) {
                    self.background.insert(call, index);
                    transcript.update(index, |body| {
                        if let Body::Tool(tool) = body {
                            tool.background = true;
                        }
                    });
                }
                vec![]
            }
            System::TaskStarted { .. } => vec![],
            System::TaskProgress {
                tool_use_id: Some(call),
                description: Some(description),
            } => {
                if let Some(index) = self.tool_index(&call) {
                    transcript.update(index, |body| {
                        if let Body::Tool(tool) = body {
                            tool.activity = Some(description);
                        }
                    });
                }
                vec![]
            }
            System::TaskProgress { .. } => vec![],
            System::TaskNotification {
                tool_use_id,
                status,
                summary,
            } => {
                let Some((call, index)) = tool_use_id
                    .and_then(|call| self.background.remove(&call).map(|index| (call, index)))
                else {
                    return vec![];
                };
                self.close_children(&call, transcript);
                // The handed-back report, not the CLI's note that it was delivered elsewhere.
                let summary = self.handbacks.remove(&call).or(summary);
                transcript.update(index, |body| {
                    if let Body::Tool(tool) = body {
                        tool.state = match status.as_str() {
                            "completed" => ToolState::Succeeded,
                            "failed" => ToolState::Failed,
                            _ => ToolState::Cancelled,
                        };
                        tool.activity = None;
                        if let Some(summary) = summary {
                            tool.output = Some(ToolOutput {
                                is_error: status == "failed",
                                text: summary,
                                structured: None,
                            });
                        }
                    }
                });
                vec![]
            }
            System::BackgroundTasksChanged { tasks } => {
                let active = tasks
                    .iter()
                    .filter(|task| task.get("ambient").and_then(Value::as_bool) != Some(true))
                    .count();
                vec![Signal::BackgroundTasks(active)]
            }
            System::ApiRetry {
                attempt,
                max_retries,
                error_status,
                retry_delay_ms,
            } => {
                let mut text = match (attempt, max_retries) {
                    (Some(a), Some(m)) => format!("Retrying the API request (attempt {a} of {m})"),
                    _ => "Retrying the API request".into(),
                };
                if let Some(status) = error_status {
                    text.push_str(&format!(" after HTTP {status}"));
                }
                let until = retry_delay_ms.map(|ms| now_seconds() + ms.div_ceil(1000));
                notice_until(transcript, None, NoticeKind::ApiRetry, text, until);
                vec![]
            }
            System::Other => vec![],
        }
    }

    fn stream(&mut self, event: ApiEvent, parent: Option<String>, transcript: &mut Transcript) {
        match event {
            ApiEvent::MessageStart { message } => {
                self.streamed.insert(message.id.clone());
                self.streaming.insert(parent, message.id);
            }
            ApiEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                let Some(message) = self.streaming.get(&parent).cloned() else {
                    return;
                };
                if let Some(item) = self.block_item(content_block, parent, true, transcript) {
                    self.blocks.insert((message, index), item);
                }
            }
            ApiEvent::ContentBlockDelta { index, delta } => {
                let Some(&item) = self.streaming_block(&parent, index) else {
                    return;
                };
                transcript.update(item, |body| match (body, delta) {
                    (Body::Text(block), Delta::TextDelta { text }) => block.text.push_str(&text),
                    (Body::Thinking(block), Delta::ThinkingDelta { thinking }) => {
                        block.text.push_str(&thinking)
                    }
                    (Body::Tool(tool), Delta::InputJsonDelta { partial_json }) => {
                        tool.input_partial.push_str(&partial_json)
                    }
                    _ => {}
                });
            }
            ApiEvent::ContentBlockStop { index } => {
                let Some(&item) = self.streaming_block(&parent, index) else {
                    return;
                };
                transcript.update(item, |body| match body {
                    Body::Text(block) | Body::Thinking(block) => block.streaming = false,
                    Body::Tool(tool) => finish_input(tool),
                    _ => {}
                });
            }
            ApiEvent::Other => {}
        }
    }

    fn tool_index(&self, call: &str) -> Option<usize> {
        self.tools
            .get(call)
            .or_else(|| self.background.get(call))
            .copied()
    }

    fn streaming_block(&self, parent: &Option<String>, index: usize) -> Option<&usize> {
        let message = self.streaming.get(parent)?;
        self.blocks.get(&(message.clone(), index))
    }

    fn assistant(&mut self, message: wire::AssistantMessage, transcript: &mut Transcript) {
        if message.parent_tool_use_id.is_none() && message.uuid.is_some() {
            self.last_message = message.uuid.clone();
        }
        let streamed = self.streamed.contains(&message.message.id);
        for block in message.message.content {
            match block {
                // Complete input replaces whatever the stream assembled.
                Block::ToolUse { id, name, input } => match self.tools.get(&id) {
                    Some(&index) => transcript.update(index, |body| {
                        if let Body::Tool(tool) = body {
                            tool.input = input;
                            tool.input_partial.clear();
                            if tool.state == ToolState::Streaming {
                                tool.state = ToolState::Running;
                            }
                        }
                    }),
                    None => {
                        let block = Block::ToolUse { id, name, input };
                        self.block_item(
                            block,
                            message.parent_tool_use_id.clone(),
                            false,
                            transcript,
                        );
                    }
                },
                // Streamed text is already complete; snapshots of unstreamed
                // messages (typically subagent turns) are appended once.
                Block::Text { text } | Block::Thinking { thinking: text } if text.is_empty() => {}
                block if !streamed => {
                    self.block_item(block, message.parent_tool_use_id.clone(), false, transcript);
                }
                _ => {}
            }
        }
        if let Some(error) = message.error.as_ref().and_then(Value::as_str) {
            notice(
                transcript,
                message.parent_tool_use_id,
                NoticeKind::Error,
                format!("Claude reported an error: {error}."),
            );
        }
    }

    fn block_item(
        &mut self,
        block: Block,
        parent: Option<String>,
        streaming: bool,
        transcript: &mut Transcript,
    ) -> Option<usize> {
        let body = match block {
            Block::Text { text } => Body::Text(TextBlock { text, streaming }),
            Block::Thinking { thinking } => Body::Thinking(TextBlock {
                text: thinking,
                streaming,
            }),
            Block::ToolUse { id, name, input } => {
                if let Some(&index) = self.tools.get(&id) {
                    return Some(index);
                }
                let index = transcript.push(
                    parent,
                    Body::Tool(ToolCall {
                        call_id: id.clone(),
                        name,
                        input,
                        input_partial: String::new(),
                        state: if streaming {
                            ToolState::Streaming
                        } else {
                            ToolState::Running
                        },
                        output: None,
                        elapsed_seconds: None,
                        activity: None,
                        background: false,
                    }),
                );
                self.parents
                    .insert(id.clone(), parent_of(transcript, index));
                self.tools.insert(id, index);
                return Some(index);
            }
            Block::Other => return None,
        };
        Some(transcript.push(parent, body))
    }

    fn tool_results(&mut self, message: &wire::UserMessage, transcript: &mut Transcript) {
        let results = message.tool_results();
        let structured = (results.len() == 1)
            .then(|| message.tool_use_result.clone())
            .flatten();
        for result in results {
            let Some(&index) = self.tools.get(result.tool_use_id) else {
                continue;
            };
            if self.background.contains_key(result.tool_use_id) {
                // The result is a launch placeholder; the task notification ends the call.
                transcript.update(index, |body| {
                    if let Body::Tool(tool) = body {
                        finish_input(tool);
                    }
                });
                continue;
            }
            self.record_handback(index, transcript);
            // A subagent's report arrives through its handback, not in this result.
            let text = self
                .handbacks
                .remove(result.tool_use_id)
                .unwrap_or(result.text);
            transcript.update(index, |body| {
                if let Body::Tool(tool) = body {
                    finish_input(tool);
                    tool.state = if result.is_error {
                        ToolState::Failed
                    } else {
                        ToolState::Succeeded
                    };
                    tool.output = Some(ToolOutput {
                        text,
                        is_error: result.is_error,
                        structured: structured.clone(),
                    });
                }
            });
        }
    }

    /// `SubagentHandback({message})` carries the subagent's final report to its caller.
    fn record_handback(&mut self, index: usize, transcript: &mut Transcript) {
        let Some(item) = transcript.get(index) else {
            return;
        };
        let (Some(parent), Body::Tool(tool)) = (&item.parent, &item.body) else {
            return;
        };
        if tool.name != HANDBACK_TOOL {
            return;
        }
        let Some(message) = tool.input.get("message").and_then(Value::as_str) else {
            return;
        };
        let (parent, message) = (parent.clone(), message.to_owned());
        // The caller may already have finished with a placeholder result.
        if let Some(&caller) = self
            .tools
            .get(&parent)
            .or_else(|| self.background.get(&parent))
            && transcript
                .get(caller)
                .is_some_and(|item| matches!(&item.body, Body::Tool(t) if t.state.finished()))
        {
            transcript.update(caller, |body| {
                if let Body::Tool(tool) = body {
                    tool.output = Some(ToolOutput {
                        text: message.clone(),
                        is_error: false,
                        structured: None,
                    });
                }
            });
            return;
        }
        self.handbacks.insert(parent, message);
    }

    /// A subagent ended: anything of its own still open will never report back.
    fn close_children(&mut self, call: &str, transcript: &mut Transcript) {
        let children: Vec<usize> = self
            .tools
            .values()
            .chain(self.blocks.values())
            .copied()
            // A nested background subagent ends with its own notification.
            .filter(|index| !self.background.values().any(|b| b == index))
            .filter(|&index| {
                let mut parent = transcript.get(index).and_then(|item| item.parent.clone());
                for _ in 0..64 {
                    match parent {
                        Some(p) if p == call => return true,
                        Some(p) => parent = self.parents.get(&p).cloned().flatten(),
                        None => return false,
                    }
                }
                false
            })
            .collect();
        for index in &children {
            // A handback whose result has not arrived yet still carries the report.
            if let Some(Body::Tool(tool)) = transcript.get(*index).map(|item| &item.body)
                && tool.name == HANDBACK_TOOL
                && let Some(message) = tool.input.get("message").and_then(Value::as_str)
            {
                self.handbacks
                    .entry(call.to_owned())
                    .or_insert_with(|| message.to_owned());
            }
            transcript.update(*index, |body| match body {
                Body::Tool(tool) if tool.name == HANDBACK_TOOL && !tool.state.finished() => {
                    finish_input(tool);
                    tool.state = ToolState::Succeeded;
                }
                Body::Tool(tool) if !tool.state.finished() => {
                    finish_input(tool);
                    tool.state = ToolState::Cancelled;
                }
                Body::Text(block) | Body::Thinking(block) => block.streaming = false,
                _ => {}
            });
        }
        self.tools.retain(|_, index| !children.contains(index));
        self.blocks.retain(|_, index| !children.contains(index));
    }

    fn result(
        &mut self,
        result: wire::ResultMessage,
        transcript: &mut Transcript,
        interrupted: bool,
    ) -> Vec<Signal> {
        let errors = || {
            let text: Vec<String> = result
                .errors
                .iter()
                .map(|e| e.as_str().map_or_else(|| e.to_string(), str::to_owned))
                .collect();
            if text.is_empty() {
                result
                    .result
                    .clone()
                    .unwrap_or_else(|| "The turn failed.".into())
            } else {
                text.join("\n")
            }
        };
        let outcome = match result.subtype.as_str() {
            "success" if !result.is_error => Outcome::Completed,
            _ if interrupted => Outcome::Interrupted,
            "error_max_turns" => Outcome::Limit("Reached the maximum number of turns.".into()),
            "error_max_budget_usd" => Outcome::Limit("Reached the cost budget.".into()),
            _ => Outcome::Failed(errors()),
        };
        self.close_turn(transcript, &outcome);
        transcript.push(
            None,
            Body::TurnEnd(TurnEnd {
                outcome,
                usage: result.usage.map(|u| Usage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                    cache_read_input_tokens: u.cache_read_input_tokens,
                    cache_creation_input_tokens: u.cache_creation_input_tokens,
                }),
                cost_usd: result.total_cost_usd,
                duration_ms: result.duration_ms,
                resume_at: self.last_message.clone(),
            }),
        );
        let mut signals: Vec<_> = result
            .session_id
            .into_iter()
            .map(Signal::SessionId)
            .collect();
        signals.push(Signal::TurnFinished);
        signals
    }
}

fn finish_input(tool: &mut ToolCall) {
    if !tool.input_partial.is_empty()
        && let Ok(input) = serde_json::from_str(&tool.input_partial)
    {
        tool.input = input;
        tool.input_partial.clear();
    }
    if tool.state == ToolState::Streaming {
        tool.state = ToolState::Running;
    }
}

fn notice(transcript: &mut Transcript, parent: Option<String>, kind: NoticeKind, text: String) {
    notice_until(transcript, parent, kind, text, None);
}

fn notice_until(
    transcript: &mut Transcript,
    parent: Option<String>,
    kind: NoticeKind,
    text: String,
    until: Option<u64>,
) {
    transcript.push(parent, Body::Notice(Notice { kind, text, until }));
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn parent_of(transcript: &Transcript, index: usize) -> Option<String> {
    transcript.get(index).and_then(|item| item.parent.clone())
}
