//! Folds Codex `app-server` messages into the provider-neutral transcript. Codex items
//! take the shapes the chat already draws (DEC-058): commands as `Bash`, patches as
//! `Edit`/`Write` with a Claude-like structured patch, searches as `WebSearch`, MCP calls
//! as `mcp__server__tool`, subagents as `Agent` with their thread's items under them.
//! Pure: no I/O, so recorded sessions replay deterministically in tests.
use super::wire::{self, Message, id_text};
use crate::chat::claude::adapter::{CliState, Signal};
use crate::chat::claude::wire::ContextUsage;
use crate::chat::transcript::{
    Body, Decision, Notice, NoticeKind, Outcome, Permission, PermissionKind, Remember, Resolution,
    TextBlock, ToolCall, ToolOutput, ToolState, Transcript, TurnEnd, Usage,
};
use serde_json::{Value, json};
use std::collections::HashMap;

/// The account's usage limit (ChatGPT plans), for the context indicator.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RateLimit {
    pub used_percent: f64,
    pub window_mins: Option<u64>,
    /// Unix seconds.
    pub resets_at: Option<u64>,
}

#[derive(Default)]
pub struct Adapter {
    /// The conversation's thread; others are its subagents.
    pub thread: Option<String>,
    /// `skills/changed` arrived: the skills list is asked for again.
    pub skills_changed: bool,
    /// The running turn of the main thread.
    pub turn: Option<String>,
    /// Codex item id → transcript indices (a patch becomes one row per file).
    items: HashMap<String, Vec<usize>>,
    /// Subagent thread → its card.
    subagents: HashMap<String, usize>,
    /// Subagent thread → its latest reply (the card's result).
    replies: HashMap<String, String>,
    /// Waiting requests: id as text → (id as sent, transcript index).
    permissions: HashMap<String, (Value, usize)>,
    /// The turn's plan, one row updated in place.
    plan: Option<usize>,
    usage: Option<Usage>,
    pub context: Option<ContextUsage>,
    pub rate: Option<RateLimit>,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

impl Adapter {
    /// Request ids of pending requests, oldest first.
    pub fn pending_permissions(&self) -> Vec<String> {
        let mut pending: Vec<_> = self.permissions.iter().collect();
        pending.sort_by_key(|(_, (_, index))| *index);
        pending.into_iter().map(|(id, _)| id.clone()).collect()
    }

    /// The id a request was sent with, to answer it.
    pub fn request_id(&self, request: &str) -> Option<Value> {
        self.permissions.get(request).map(|(id, _)| id.clone())
    }

    pub fn resolve_with(
        &mut self,
        request: &str,
        decision: Decision,
        resolution: Option<Resolution>,
        transcript: &mut Transcript,
    ) -> bool {
        let Some((_, index)) = self.permissions.remove(request) else {
            return false;
        };
        transcript.update(index, |body| {
            if let Body::Permission(permission) = body {
                permission.decision = decision;
                permission.resolution = resolution;
            }
        });
        true
    }

    /// Closes everything still open (interrupt, failure, exit).
    pub fn close_turn(&mut self, transcript: &mut Transcript, outcome: &Outcome) {
        for request in self.pending_permissions() {
            self.resolve_with(&request, Decision::Cancelled, None, transcript);
        }
        let cancel = *outcome != Outcome::Completed;
        let open: Vec<usize> = self
            .items
            .drain()
            .flat_map(|(_, indices)| indices)
            .collect();
        for index in open {
            transcript.update(index, |body| match body {
                Body::Text(block) | Body::Thinking(block) => block.streaming = false,
                Body::Tool(tool) if cancel && !tool.state.finished() => {
                    tool.state = ToolState::Cancelled
                }
                _ => {}
            });
        }
        if cancel {
            for index in self.subagents.values() {
                transcript.update(*index, |body| {
                    if let Body::Tool(tool) = body
                        && !tool.state.finished()
                    {
                        tool.state = ToolState::Cancelled;
                    }
                });
            }
        }
        self.plan = None;
        self.turn = None;
    }

    pub fn apply(&mut self, message: Message, transcript: &mut Transcript) -> Vec<Signal> {
        match message {
            Message::Response { id, result } => vec![Signal::ControlAck {
                request_id: id,
                error: None,
                response: Some(result),
            }],
            Message::Error { id, message } => vec![Signal::ControlAck {
                request_id: id,
                error: Some(message),
                response: None,
            }],
            Message::Notification { method, params } => {
                self.notification(&method, &params, transcript)
            }
            Message::Request { id, method, params } => {
                self.request(id, &method, &params, transcript)
            }
        }
    }

    /// `Some(None)` for the main thread, `Some(Some(card))` for a subagent's, `None` for
    /// threads this chat doesn't show.
    fn route(&self, params: &Value) -> Option<Option<String>> {
        let thread = text(params, "threadId")?;
        match &self.thread {
            None => Some(None),
            Some(main) if main == thread => Some(None),
            Some(_) => self
                .subagents
                .contains_key(thread)
                .then(|| Some(thread.to_owned())),
        }
    }

    fn notification(&mut self, method: &str, params: &Value, t: &mut Transcript) -> Vec<Signal> {
        match method {
            "skills/changed" => {
                self.skills_changed = true;
                return vec![];
            }
            "account/rateLimits/updated" => {
                self.rate = rate_limit(&params["rateLimits"]).or(self.rate);
                return vec![];
            }
            "serverRequest/resolved" => {
                let request = params.get("requestId").map(id_text).unwrap_or_default();
                return if self.resolve_with(&request, Decision::Cancelled, None, t) {
                    vec![Signal::PermissionWithdrawn]
                } else {
                    vec![]
                };
            }
            _ => {}
        }
        // A subagent's items may come before its activity: its card is made on sight.
        if method.starts_with("item/")
            && self.route(params).is_none()
            && let (Some(_), Some(thread)) = (&self.thread, text(params, "threadId"))
        {
            let thread = thread.to_owned();
            let card = tool(
                &thread,
                "Agent",
                json!({"description": "Subagent"}),
                ToolState::Running,
            );
            let index = t.push(None, Body::Tool(card));
            self.subagents.insert(thread, index);
        }
        let Some(parent) = self.route(params) else {
            return vec![];
        };
        let main = parent.is_none();
        match method {
            "thread/status/changed" if main => match params["status"]["type"].as_str() {
                Some("idle") => vec![Signal::State(CliState::Idle)],
                _ => vec![],
            },
            // A turn of our own is open already; one Codex starts itself opens here.
            "turn/started" if main => {
                self.turn = params["turn"]["id"].as_str().map(str::to_owned);
                vec![Signal::State(CliState::Running)]
            }
            "turn/completed" if main => {
                let turn = &params["turn"];
                let outcome = match turn["status"].as_str() {
                    Some("interrupted") => Outcome::Interrupted,
                    Some("failed") => Outcome::Failed(
                        turn["error"]["message"]
                            .as_str()
                            .unwrap_or("The turn failed.")
                            .to_owned(),
                    ),
                    _ => Outcome::Completed,
                };
                self.close_turn(t, &outcome);
                t.push(
                    None,
                    Body::TurnEnd(TurnEnd {
                        outcome,
                        usage: self.usage.take(),
                        cost_usd: None,
                        duration_ms: turn["durationMs"].as_u64(),
                        resume_at: turn["id"].as_str().map(str::to_owned),
                    }),
                );
                vec![Signal::TurnFinished]
            }
            "item/started" | "item/completed" => {
                self.item(&params["item"], parent, method == "item/completed", t);
                vec![]
            }
            "item/agentMessage/delta" | "item/reasoning/summaryTextDelta" => {
                let delta = text(params, "delta").unwrap_or_default().to_owned();
                self.each(params, t, |body| {
                    if let Body::Text(block) | Body::Thinking(block) = body {
                        block.text.push_str(&delta);
                    }
                });
                vec![]
            }
            "item/reasoning/summaryPartAdded" => {
                self.each(params, t, |body| {
                    if let Body::Thinking(block) = body
                        && !block.text.is_empty()
                    {
                        block.text.push_str("\n\n");
                    }
                });
                vec![]
            }
            "item/commandExecution/outputDelta" => {
                let delta = text(params, "delta").unwrap_or_default().to_owned();
                self.each(params, t, |body| {
                    if let Body::Tool(tool) = body {
                        tool.output
                            .get_or_insert_with(|| ToolOutput {
                                text: String::new(),
                                is_error: false,
                                structured: None,
                            })
                            .text
                            .push_str(&delta);
                    }
                });
                vec![]
            }
            "turn/plan/updated" if main => {
                let todos: Vec<Value> = params["plan"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|step| {
                        json!({
                            "content": step["step"],
                            "status": match step["status"].as_str() {
                                Some("inProgress") => "in_progress",
                                Some("completed") => "completed",
                                _ => "pending",
                            },
                        })
                    })
                    .collect();
                let input = json!({"todos": todos, "explanation": params["explanation"]});
                match self.plan {
                    Some(index) => t.update(index, |body| {
                        if let Body::Tool(tool) = body {
                            tool.input = input;
                        }
                    }),
                    None => {
                        self.plan = Some(t.push(
                            None,
                            Body::Tool(tool("plan", "TodoWrite", input, ToolState::Succeeded)),
                        ))
                    }
                }
                vec![]
            }
            "thread/tokenUsage/updated" if main => {
                let usage = &params["tokenUsage"];
                let last = &usage["last"];
                let count = |key: &str| last[key].as_u64().unwrap_or(0);
                // `last` is one model call; a turn adds up its calls.
                let total = self.usage.get_or_insert_with(Usage::default);
                total.input_tokens +=
                    count("inputTokens").saturating_sub(count("cachedInputTokens"));
                total.output_tokens += count("outputTokens");
                total.cache_read_input_tokens += count("cachedInputTokens");
                total.cache_creation_input_tokens += count("cacheWriteInputTokens");
                if let Some(max) = usage["modelContextWindow"].as_u64().filter(|max| *max > 0) {
                    self.context = Some(ContextUsage {
                        used: count("inputTokens") + count("outputTokens"),
                        max,
                        buffer: 0,
                    });
                }
                vec![]
            }
            "error" if main => {
                let error = &params["error"];
                let message = error["message"]
                    .as_str()
                    .unwrap_or("Codex reported an error.");
                let info = error["codexErrorInfo"].clone();
                let limited = ["usageLimitExceeded", "rateLimitExceeded"]
                    .iter()
                    .any(|kind| info == json!(kind) || info.get(*kind).is_some());
                let (kind, until) = if limited {
                    (
                        NoticeKind::RateLimit,
                        self.rate.and_then(|rate| rate.resets_at),
                    )
                } else if params["willRetry"].as_bool() == Some(true) {
                    (NoticeKind::ApiRetry, None)
                } else {
                    (NoticeKind::Error, None)
                };
                t.push(
                    None,
                    Body::Notice(Notice {
                        kind,
                        text: message.to_owned(),
                        until,
                    }),
                );
                vec![]
            }
            _ => vec![],
        }
    }

    /// Applies `f` to every row of the item named in `params`.
    fn each(&self, params: &Value, t: &mut Transcript, mut f: impl FnMut(&mut Body)) {
        let Some(indices) = text(params, "itemId").and_then(|id| self.items.get(id)) else {
            return;
        };
        for index in indices.clone() {
            t.update(index, &mut f);
        }
    }

    fn item(&mut self, item: &Value, parent: Option<String>, done: bool, t: &mut Transcript) {
        let Some(id) = text(item, "id").map(str::to_owned) else {
            return;
        };
        let known = self.items.get(&id).cloned();
        match text(item, "type").unwrap_or_default() {
            "userMessage" => {
                // Ours is in the conversation already; a subagent's is its task.
                if let (Some(thread), true) = (&parent, done) {
                    let prompt: String = item["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|part| text(part, "text"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Some(&card) = self.subagents.get(thread) {
                        t.update(card, |body| {
                            if let Body::Tool(tool) = body
                                && tool.input.get("prompt").is_none()
                            {
                                tool.input["prompt"] = json!(prompt);
                            }
                        });
                    }
                }
            }
            kind @ ("agentMessage" | "reasoning" | "plan") => {
                let content = match kind {
                    "reasoning" => item["summary"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                    _ => text(item, "text").unwrap_or_default().to_owned(),
                };
                if let (Some(thread), true, "agentMessage") = (&parent, done, kind) {
                    self.replies.insert(thread.clone(), content.clone());
                }
                match known {
                    Some(indices) => {
                        for index in indices {
                            t.update(index, |body| {
                                if let Body::Text(block) | Body::Thinking(block) = body {
                                    // Deltas already built it; the final text is authoritative.
                                    if done && !content.is_empty() {
                                        block.text = content.clone();
                                    }
                                    block.streaming = !done;
                                }
                            });
                        }
                    }
                    None => {
                        let block = TextBlock {
                            text: content,
                            streaming: !done,
                        };
                        let body = if kind == "reasoning" {
                            Body::Thinking(block)
                        } else {
                            Body::Text(block)
                        };
                        let index = t.push(parent, body);
                        self.items.insert(id, vec![index]);
                    }
                }
            }
            "commandExecution" => {
                let mut call = tool(
                    &id,
                    "Bash",
                    json!({"command": wire::command_text(text(item, "command").unwrap_or_default())}),
                    ToolState::Running,
                );
                if done {
                    let status = text(item, "status").unwrap_or_default();
                    let exit = item["exitCode"].as_i64();
                    let failed = status == "failed"
                        || status == "declined"
                        || exit.is_some_and(|code| code != 0);
                    let mut output = text(item, "aggregatedOutput")
                        .unwrap_or_default()
                        .to_owned();
                    if status == "declined" && output.is_empty() {
                        output = "Declined.".into();
                    }
                    call.state = if failed {
                        ToolState::Failed
                    } else {
                        ToolState::Succeeded
                    };
                    // Without a final output, the streamed one stands.
                    if output.is_empty()
                        && let Some(streamed) = self
                            .items
                            .get(&id)
                            .and_then(|indices| indices.first())
                            .and_then(|&index| match &t.get(index)?.body {
                                Body::Tool(tool) => tool.output.as_ref().map(|o| o.text.clone()),
                                _ => None,
                            })
                    {
                        output = streamed;
                    }
                    call.output = Some(ToolOutput {
                        text: output,
                        is_error: failed,
                        structured: None,
                    });
                    call.elapsed_seconds = item["durationMs"].as_f64().map(|ms| ms / 1000.);
                }
                self.upsert(id, parent, vec![call], t);
            }
            "fileChange" => {
                let status = text(item, "status").unwrap_or_default();
                let state = match (done, status) {
                    (false, _) => ToolState::Running,
                    (true, "completed") => ToolState::Succeeded,
                    (true, _) => ToolState::Failed,
                };
                let calls = item["changes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(n, change)| {
                        let path = text(change, "path").unwrap_or_default();
                        let kind = change["kind"]["type"].as_str().unwrap_or("update");
                        let mut call = tool(
                            &format!("{id}#{n}"),
                            if kind == "add" { "Write" } else { "Edit" },
                            json!({"file_path": path}),
                            state,
                        );
                        call.output = Some(ToolOutput {
                            text: if state == ToolState::Failed {
                                "The change wasn't applied.".into()
                            } else {
                                String::new()
                            },
                            is_error: state == ToolState::Failed,
                            structured: Some(patch_result(
                                path,
                                kind,
                                text(change, "diff").unwrap_or_default(),
                            )),
                        });
                        call
                    })
                    .collect();
                self.upsert(id, parent, calls, t);
            }
            "webSearch" => {
                let mut call = tool(
                    &id,
                    "WebSearch",
                    json!({"query": text(item, "query").unwrap_or_default()}),
                    if done {
                        ToolState::Succeeded
                    } else {
                        ToolState::Running
                    },
                );
                if done {
                    call.output = Some(ToolOutput {
                        text: search_results(&item["results"]),
                        is_error: false,
                        structured: None,
                    });
                }
                self.upsert(id, parent, vec![call], t);
            }
            "mcpToolCall" => {
                let name = format!(
                    "mcp__{}__{}",
                    text(item, "server").unwrap_or("server"),
                    text(item, "tool").unwrap_or("tool")
                );
                let mut call = tool(&id, &name, item["arguments"].clone(), ToolState::Running);
                if done {
                    let error = item["error"]["message"].as_str();
                    call.state = match text(item, "status") {
                        Some("failed") => ToolState::Failed,
                        _ if error.is_some() => ToolState::Failed,
                        _ => ToolState::Succeeded,
                    };
                    call.output = Some(ToolOutput {
                        text: error
                            .map(str::to_owned)
                            .unwrap_or_else(|| content_text(&item["result"]["content"])),
                        is_error: call.state == ToolState::Failed,
                        structured: None,
                    });
                    call.elapsed_seconds = item["durationMs"].as_f64().map(|ms| ms / 1000.);
                }
                self.upsert(id, parent, vec![call], t);
            }
            "imageView" => {
                let call = tool(
                    &id,
                    "ViewImage",
                    json!({"path": item["path"]}),
                    ToolState::Succeeded,
                );
                self.upsert(id, parent, vec![call], t);
            }
            "subAgentActivity" => self.subagent(item, parent, t),
            "contextCompaction" if done && parent.is_none() => {
                t.push(
                    None,
                    Body::Notice(Notice {
                        kind: NoticeKind::Compacted,
                        text: "Conversation compacted".into(),
                        until: None,
                    }),
                );
            }
            // Spawning/waiting calls are the cards' business; the rest has no row.
            _ => {}
        }
    }

    /// Adds the item's rows, or replaces their content (keeping streamed output).
    fn upsert(
        &mut self,
        id: String,
        parent: Option<String>,
        calls: Vec<ToolCall>,
        t: &mut Transcript,
    ) {
        let mut indices = self.items.get(&id).cloned().unwrap_or_default();
        for (n, call) in calls.into_iter().enumerate() {
            match indices.get(n) {
                Some(&index) => t.update(index, |body| {
                    if let Body::Tool(tool) = body {
                        let streamed = tool.output.take();
                        *tool = ToolCall {
                            output: call.output.or(streamed),
                            ..call
                        };
                    }
                }),
                None => indices.push(t.push(parent.clone(), Body::Tool(call))),
            }
        }
        self.items.insert(id, indices);
    }

    fn subagent(&mut self, item: &Value, parent: Option<String>, t: &mut Transcript) {
        let Some(thread) = text(item, "agentThreadId").map(str::to_owned) else {
            return;
        };
        let state = match text(item, "kind") {
            Some("completed") => ToolState::Succeeded,
            Some("interrupted") => ToolState::Cancelled,
            _ => ToolState::Running,
        };
        let reply = self.replies.get(&thread).cloned();
        match self.subagents.get(&thread) {
            Some(&index) => t.update(index, |body| {
                if let Body::Tool(tool) = body {
                    // A later "interacted" doesn't reopen a finished subagent.
                    if !tool.state.finished() || state != ToolState::Running {
                        tool.state = state;
                    }
                    if state.finished() {
                        tool.output = Some(ToolOutput {
                            text: reply.unwrap_or_default(),
                            is_error: false,
                            structured: None,
                        });
                    }
                }
            }),
            None => {
                let name = text(item, "agentPath")
                    .and_then(|path| path.rsplit('/').next())
                    .map(|name| name.replace('_', " "))
                    .unwrap_or_else(|| "Subagent".into());
                let card = tool(&thread, "Agent", json!({"description": name}), state);
                let index = t.push(parent, Body::Tool(card));
                self.subagents.insert(thread, index);
            }
        }
    }

    fn request(
        &mut self,
        id: Value,
        method: &str,
        params: &Value,
        t: &mut Transcript,
    ) -> Vec<Signal> {
        let item = text(params, "itemId").map(str::to_owned);
        let (tool, input, kind, remember) = match method {
            "item/commandExecution/requestApproval" => (
                "Bash",
                json!({
                    "command": wire::command_text(text(params, "command").unwrap_or_default()),
                    "cwd": params["cwd"],
                    "description": params["reason"],
                    "decisions": params["availableDecisions"],
                }),
                PermissionKind::Tool,
                offers(params, "acceptForSession").then_some(Remember::Session),
            ),
            "item/fileChange/requestApproval" => {
                let change = item
                    .as_ref()
                    .and_then(|item| self.items.get(item))
                    .and_then(|indices| indices.first())
                    .and_then(|&index| match &t.get(index)?.body {
                        Body::Tool(tool) => Some((
                            tool.input.clone(),
                            tool.output.as_ref().and_then(|o| o.structured.clone()),
                            tool.name.clone(),
                        )),
                        _ => None,
                    });
                let (input, result, name) = change.unwrap_or((json!({}), None, "Edit".into()));
                let files = item
                    .as_ref()
                    .and_then(|item| self.items.get(item))
                    .map_or(0, Vec::len);
                (
                    "apply_patch",
                    json!({
                        "file_path": input["file_path"],
                        "result": result,
                        "files": files,
                        "write": name == "Write",
                        "description": params["reason"],
                        "decisions": params["availableDecisions"],
                    }),
                    PermissionKind::Tool,
                    offers(params, "acceptForSession").then_some(Remember::Session),
                )
            }
            "item/permissions/requestApproval" => (
                "Permissions",
                json!({"description": params["reason"], "permissions": params["permissions"]}),
                PermissionKind::Tool,
                Some(Remember::Session),
            ),
            "item/tool/requestUserInput" => {
                let questions: Vec<Value> = params["questions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|q| {
                        json!({
                            "id": q["id"],
                            "question": q["question"],
                            "header": q["header"],
                            "multiSelect": false,
                            "options": q["options"].as_array().map(|options| options.iter().map(|o| json!({
                                "label": o["label"], "description": o["description"],
                            })).collect::<Vec<_>>()).unwrap_or_default(),
                        })
                    })
                    .collect();
                (
                    "AskUserQuestion",
                    json!({"questions": questions}),
                    PermissionKind::Question,
                    None,
                )
            }
            "mcpServer/elicitation/request" => {
                t.push(
                    None,
                    Body::Notice(Notice {
                        kind: NoticeKind::Diagnostic,
                        text: format!(
                            "{} asked for input, which Canopy can't answer yet.",
                            text(params, "serverName").unwrap_or("An MCP server")
                        ),
                        until: None,
                    }),
                );
                return vec![Signal::Reply(wire::reply(&id, json!({"action": "cancel"})))];
            }
            _ => return vec![Signal::Reply(wire::refuse(&id, "Not supported by Canopy."))],
        };
        let request = id_text(&id);
        let index = t.push(
            None,
            Body::Permission(Permission {
                request_id: request.clone(),
                call_id: item,
                tool: tool.into(),
                input,
                title: None,
                description: text(params, "reason").map(str::to_owned),
                kind,
                decision: Decision::Pending,
                remember,
                resolution: None,
            }),
        );
        self.permissions.insert(request, (id, index));
        vec![Signal::PermissionRequested]
    }
}

/// Whether a request offers `decision` (all of them when it doesn't say).
pub fn offers(params: &Value, decision: &str) -> bool {
    let available = params
        .get("availableDecisions")
        .or_else(|| params.get("decisions"))
        .and_then(Value::as_array);
    available.is_none_or(|list| list.iter().any(|d| d.as_str() == Some(decision)))
}

fn tool(id: &str, name: &str, input: Value, state: ToolState) -> ToolCall {
    ToolCall {
        call_id: id.into(),
        name: name.into(),
        input,
        input_partial: String::new(),
        state,
        output: None,
        elapsed_seconds: None,
        activity: None,
        background: false,
    }
}

fn rate_limit(limits: &Value) -> Option<RateLimit> {
    let primary = limits.get("primary").filter(|p| p.is_object())?;
    Some(RateLimit {
        used_percent: primary["usedPercent"].as_f64()?,
        window_mins: primary["windowDurationMins"].as_u64(),
        resets_at: primary["resetsAt"].as_u64(),
    })
}

/// The rate limit in an `account/rateLimits/read` result.
pub fn rate_limit_result(result: &Value) -> Option<RateLimit> {
    rate_limit(&result["rateLimits"])
}

fn content_text(content: &Value) -> String {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| text(part, "text"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn search_results(results: &Value) -> String {
    results
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|result| {
            let title = text(result, "title").unwrap_or_default();
            let url = text(result, "url").unwrap_or_default();
            (!title.is_empty() || !url.is_empty())
                .then(|| format!("{title} {url}").trim().to_owned())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A Codex file change as the structured result the chat's change views read
/// (Claude Code's Edit/Write shape): a new file's content or the patch's hunks.
pub fn patch_result(path: &str, kind: &str, diff: &str) -> Value {
    match kind {
        "add" => json!({"type": "create", "filePath": path, "content": diff}),
        "delete" => {
            let lines: Vec<String> = diff.lines().map(|line| format!("-{line}")).collect();
            json!({"type": "update", "filePath": path, "structuredPatch": [{
                "oldStart": 1, "oldLines": lines.len(), "newStart": 0, "newLines": 0, "lines": lines,
            }]})
        }
        _ => json!({"type": "update", "filePath": path, "structuredPatch": hunks(diff)}),
    }
}

/// Hunks of a unified diff (file headers skipped).
fn hunks(diff: &str) -> Vec<Value> {
    let mut hunks: Vec<Value> = vec![];
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("@@ ") {
            let mut ranges = header.split_whitespace();
            let range = |part: Option<&str>, sign: char| -> (u64, u64) {
                let part = part.and_then(|p| p.strip_prefix(sign)).unwrap_or("0");
                let mut numbers = part.split(',');
                let start = numbers.next().and_then(|n| n.parse().ok()).unwrap_or(0);
                let count = numbers.next().and_then(|n| n.parse().ok()).unwrap_or(1);
                (start, count)
            };
            let (old_start, old_lines) = range(ranges.next(), '-');
            let (new_start, new_lines) = range(ranges.next(), '+');
            hunks.push(json!({
                "oldStart": old_start, "oldLines": old_lines,
                "newStart": new_start, "newLines": new_lines, "lines": [],
            }));
        } else if let Some(hunk) = hunks.last_mut()
            && matches!(line.chars().next(), Some(' ' | '+' | '-' | '\\') | None)
        {
            let line = if line.is_empty() { " " } else { line };
            hunk["lines"]
                .as_array_mut()
                .expect("lines")
                .push(json!(line));
        }
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::tools::{ToolView, view};

    #[test]
    fn patches_become_change_views() {
        let update = patch_result("/r/a.rs", "update", "@@ -1,2 +1,2 @@\n-old\n+new\n keep\n");
        let call = ToolCall {
            output: Some(ToolOutput {
                text: String::new(),
                is_error: false,
                structured: Some(update),
            }),
            ..tool(
                "x",
                "Edit",
                json!({"file_path": "/r/a.rs"}),
                ToolState::Succeeded,
            )
        };
        let ToolView::Change(change) = view(&call) else {
            panic!("change view")
        };
        assert_eq!(
            (change.added, change.removed, change.created),
            (1, 1, false)
        );
        let created = ToolCall {
            output: Some(ToolOutput {
                text: String::new(),
                is_error: false,
                structured: Some(patch_result("/r/b.txt", "add", "x\ny\n")),
            }),
            ..tool(
                "y",
                "Write",
                json!({"file_path": "/r/b.txt"}),
                ToolState::Succeeded,
            )
        };
        let ToolView::Change(change) = view(&created) else {
            panic!("change view")
        };
        assert_eq!((change.added, change.created), (2, true));
    }
}
