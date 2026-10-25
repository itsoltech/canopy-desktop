//! Claude Code stream-json wire format (`--input-format/--output-format stream-json`).
//!
//! The protocol is internal to Claude Code and evolves with the CLI. Every struct
//! tolerates unknown fields, every tagged enum has a catch-all, and optional data
//! defaults instead of failing the line. Shapes follow CLI 2.1.294 /
//! `@anthropic-ai/claude-agent-sdk` 0.3.293.
pub use crate::chat::backend::{ContextUsage, MessageLifecycle as Lifecycle};
use crate::chat::config::{CommandInfo, Effort, Mode, ModelInfo};
use crate::chat::provider::Provider;
use crate::chat::transcript::{Attachment, Remember, Rule};
use serde::Deserialize;
use serde_json::{Value, json};

/// Lines beyond this size are dropped with a diagnostic instead of being buffered.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inbound {
    ControlRequest {
        request_id: String,
        /// Kept raw: unsupported subtypes still need their name for the reply.
        request: Value,
    },
    ControlResponse {
        response: ControlResponse,
    },
    ControlCancelRequest {
        request_id: String,
    },
    System(System),
    StreamEvent(StreamEvent),
    Assistant(AssistantMessage),
    User(UserMessage),
    Result(ResultMessage),
    RateLimitEvent(RateLimitEvent),
    ToolProgress(ToolProgress),
    /// Progress of a user message we sent (`msg_lifecycle_v1`), keyed by its uuid.
    CommandLifecycle {
        command_uuid: String,
        state: Lifecycle,
    },
    #[serde(other)]
    Unknown,
}

/// How the CLI schedules a user message sent while it may be busy. Always explicit:
/// without it the CLI picks `next` or `later` from a server-side rollout flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// Read after the current tool result, inside the running turn (Steer).
    Next,
    /// Handled as its own turn once the current one is over.
    Later,
}
impl Priority {
    fn key(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Later => "later",
        }
    }
}

pub const LIFECYCLE_CAPABILITY: &str = "msg_lifecycle_v1";
pub const CANCEL_QUEUED_CAPABILITY: &str = "interrupt_cancel_queued_v1";

/// Parses one stdout line. Blank and keep-alive lines yield `None`.
pub fn parse_line(line: &[u8]) -> Result<Option<Inbound>, String> {
    let trimmed = line.trim_ascii();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match serde_json::from_slice::<Inbound>(trimmed) {
        Ok(Inbound::Unknown) => {
            // keep_alive / transcript_mirror carry nothing for the transcript.
            Ok(None)
        }
        Ok(message) => Ok(Some(message)),
        Err(error) => Err(format!(
            "Ignored unreadable Claude output ({} bytes): {error}",
            trimmed.len()
        )),
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CanUseTool {
    pub tool_name: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub decision_reason: Option<String>,
    /// Ways to remember the decision (`addRules`, `setMode`, `addDirectories`).
    #[serde(default)]
    pub permission_suggestions: Vec<Value>,
}

/// What "Allow for this chat" can remember from the CLI's suggestions: its allow rules,
/// else switching to accepting edits. Directory grants are not offered.
pub fn remember(suggestions: &[Value]) -> Option<Remember> {
    let rules: Vec<Rule> = suggestions
        .iter()
        .filter(|s| s["type"] == "addRules" && s["behavior"] == "allow")
        .filter_map(|s| s["rules"].as_array())
        .flatten()
        .filter_map(|rule| {
            Some(Rule {
                tool: rule["toolName"]
                    .as_str()
                    .filter(|t| !t.is_empty())?
                    .to_owned(),
                content: rule["ruleContent"]
                    .as_str()
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned),
            })
        })
        .collect();
    if !rules.is_empty() {
        return Some(Remember::Rules(rules));
    }
    suggestions
        .iter()
        .any(|s| s["type"] == "setMode" && s["mode"] == "acceptEdits")
        .then_some(Remember::AcceptEdits)
}

/// `updatedPermissions` for remembering. Always scoped to the running session: the CLI
/// would otherwise write them into the project's or user's settings files (DEC-039).
pub fn session_permissions(remember: &Remember) -> Value {
    match remember {
        Remember::Rules(rules) => json!([{
            "type": "addRules",
            "rules": rules
                .iter()
                .map(|rule| match &rule.content {
                    Some(content) => json!({"toolName": rule.tool, "ruleContent": content}),
                    None => json!({"toolName": rule.tool}),
                })
                .collect::<Vec<_>>(),
            "behavior": "allow",
            "destination": "session",
        }]),
        Remember::AcceptEdits => accept_edits(),
        // Codex only: nothing for Claude to remember.
        Remember::Session => json!([]),
    }
}

fn accept_edits() -> Value {
    json!([{"type": "setMode", "mode": "acceptEdits", "destination": "session"}])
}

/// Allows a call with permission updates (session-scoped, see `session_permissions`).
pub fn allow_updating(
    request_id: &str,
    tool_use_id: Option<&str>,
    input: &Value,
    permissions: Value,
) -> Value {
    success(
        request_id,
        with_tool_use(
            json!({"behavior": "allow", "updatedInput": input, "updatedPermissions": permissions}),
            tool_use_id,
        ),
    )
}

/// Approves a plan (ExitPlanMode); with `accept_edits` the agent then edits without asking.
pub fn approve_plan(
    request_id: &str,
    tool_use_id: Option<&str>,
    input: &Value,
    accept: bool,
) -> Value {
    if accept {
        allow_updating(request_id, tool_use_id, input, accept_edits())
    } else {
        allow(request_id, tool_use_id, input)
    }
}

/// Answers AskUserQuestion: its input plus `answers` (question → answer text; several
/// choices joined with ", ").
pub fn answer_questions(
    request_id: &str,
    tool_use_id: Option<&str>,
    input: &Value,
    answers: &[(String, String)],
) -> Value {
    let mut input = input.clone();
    if let Some(object) = input.as_object_mut() {
        object.insert(
            "answers".into(),
            answers
                .iter()
                .map(|(question, answer)| (question.clone(), Value::from(answer.as_str())))
                .collect::<serde_json::Map<_, _>>()
                .into(),
        );
    }
    allow(request_id, tool_use_id, &input)
}

#[derive(Clone, Debug, Deserialize)]
pub struct ControlResponse {
    pub subtype: String,
    pub request_id: String,
    #[serde(default)]
    pub response: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum System {
    Init(Init),
    SessionStateChanged {
        state: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    CompactBoundary {
        #[serde(default)]
        compact_metadata: Option<Value>,
    },
    Status {
        #[serde(default, rename = "permissionMode")]
        permission_mode: Option<String>,
    },
    TaskStarted {
        #[serde(default)]
        task_id: Option<String>,
        #[serde(default)]
        tool_use_id: Option<String>,
        #[serde(default)]
        is_backgrounded: Option<bool>,
        #[serde(default)]
        ambient: Option<bool>,
    },
    TaskProgress {
        #[serde(default)]
        tool_use_id: Option<String>,
        #[serde(default)]
        description: Option<String>,
    },
    TaskNotification {
        #[serde(default)]
        tool_use_id: Option<String>,
        status: String,
        #[serde(default)]
        summary: Option<String>,
    },
    BackgroundTasksChanged {
        #[serde(default)]
        tasks: Vec<Value>,
    },
    ApiRetry {
        #[serde(default)]
        attempt: Option<u32>,
        #[serde(default)]
        max_retries: Option<u32>,
        #[serde(default)]
        error_status: Option<u16>,
        #[serde(default)]
        retry_delay_ms: Option<u64>,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Init {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default, rename = "permissionMode")]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub claude_code_version: Option<String>,
    /// `none` for a subscription login; otherwise the source of an API key.
    #[serde(default, rename = "apiKeySource")]
    pub api_key_source: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<Value>,
    #[serde(default)]
    pub skills: Vec<String>,
}
impl Init {
    pub fn capability_names(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct StreamEvent {
    pub event: ApiEvent,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
}

/// Raw Anthropic Messages API stream event.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApiEvent {
    MessageStart {
        message: MessageHeader,
    },
    ContentBlockStart {
        index: usize,
        content_block: Block,
    },
    ContentBlockDelta {
        index: usize,
        delta: Delta,
    },
    ContentBlockStop {
        index: usize,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MessageHeader {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text {
        #[serde(default)]
        text: String,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    InputJsonDelta {
        partial_json: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AssistantMessage {
    pub message: ApiMessage,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub error: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApiMessage {
    pub id: String,
    #[serde(default)]
    pub content: Vec<Block>,
    /// `<synthetic>` for the CLI's own replies (local commands such as `/goal`).
    #[serde(default)]
    pub model: Option<String>,
}
impl ApiMessage {
    pub fn synthetic(&self) -> bool {
        self.model.as_deref() == Some("<synthetic>")
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UserMessage {
    pub message: UserContent,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub tool_use_result: Option<Value>,
    /// Written by the CLI itself (e.g. a Stop hook's feedback), not by us.
    #[serde(default, rename = "isSynthetic")]
    pub is_synthetic: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UserContent {
    /// A string or an array of content blocks; tool results are extracted lazily.
    #[serde(default)]
    pub content: Value,
}

pub struct ToolResult<'a> {
    pub tool_use_id: &'a str,
    pub is_error: bool,
    pub text: String,
}
impl UserMessage {
    /// Its text (string content or text blocks).
    pub fn text(&self) -> String {
        content_text(Some(&self.message.content))
    }
    pub fn tool_results(&self) -> Vec<ToolResult<'_>> {
        let Some(blocks) = self.message.content.as_array() else {
            return vec![];
        };
        blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
            .filter_map(|block| {
                Some(ToolResult {
                    tool_use_id: block.get("tool_use_id")?.as_str()?,
                    is_error: block
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    text: content_text(block.get("content")),
                })
            })
            .collect()
    }
}

/// Flattens tool_result content (a string or text/image blocks) into display text.
fn content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => part.get("text").and_then(Value::as_str).map(str::to_owned),
                Some("image") => Some("[image]".into()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResultMessage {
    pub subtype: String,
    /// Why the run ended: `completed`, `aborted_streaming` (interrupted), …
    #[serde(default)]
    pub terminal_reason: Option<String>,
    /// Model requests in the run; 0 for a local command such as `/goal`.
    #[serde(default)]
    pub num_turns: Option<u64>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub usage: Option<ApiUsage>,
    #[serde(default)]
    pub errors: Vec<Value>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct ApiUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RateLimitEvent {
    #[serde(default)]
    pub rate_limit_info: RateLimitInfo,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RateLimitInfo {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "resetsAt")]
    pub resets_at: Option<Value>,
    #[serde(default, rename = "rateLimitType")]
    pub rate_limit_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ToolProgress {
    pub tool_use_id: String,
    #[serde(default)]
    pub elapsed_time_seconds: Option<f64>,
}

// Outbound messages. Each value is written as one NDJSON line.

pub fn initialize(request_id: &str) -> Value {
    // No systemPrompt: the CLI keeps its Claude Code preset, as the SDK preset does.
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": {"subtype": "initialize", "hooks": {}, "supportedDialogKinds": []},
    })
}

/// A user message: images as image blocks (their data must be loaded), quotes as quote
/// blocks in the text, files and folders listed after it for the agent to read.
pub fn user_message(
    text: &str,
    attachments: &[Attachment],
    context: Option<&str>,
    uuid: &str,
    priority: Priority,
) -> Value {
    let mut content: Vec<Value> = attachments
        .iter()
        .filter_map(|attachment| match attachment {
            Attachment::Image {
                media_type,
                data: Some(data),
                ..
            } => Some(json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": &**data},
            })),
            _ => None,
        })
        .collect();
    let paths: Vec<String> = attachments
        .iter()
        .filter_map(|attachment| match attachment {
            // A path already mentioned in the text isn't listed again.
            Attachment::Path { path, label }
                if label.as_deref().is_none_or(|label| !text.contains(label)) =>
            {
                Some(format!("- {}", path.display()))
            }
            _ => None,
        })
        .collect();
    let text = crate::chat::compose::expand(text, attachments, Provider::Claude).text;
    let text = if paths.is_empty() {
        text
    } else {
        let list = paths.join("\n");
        let header = "Attached files (read them as needed):";
        if text.is_empty() {
            format!("{header}\n{list}")
        } else {
            format!("{text}\n\n{header}\n{list}")
        }
    };
    // After a switch, the conversation so far comes first, apart from the message.
    if let Some(context) = context {
        content.insert(0, json!({"type": "text", "text": context}));
    }
    if !text.is_empty() {
        content.push(json!({"type": "text", "text": text}));
    }
    json!({
        "type": "user",
        "session_id": "",
        "message": {"role": "user", "content": content},
        "parent_tool_use_id": null,
        "uuid": uuid,
        "priority": priority.key(),
    })
}

pub fn set_model(request_id: &str, model: Option<&str>) -> Value {
    control(request_id, json!({"subtype": "set_model", "model": model}))
}

pub fn set_permission_mode(request_id: &str, mode: Mode) -> Value {
    control(
        request_id,
        json!({"subtype": "set_permission_mode", "mode": mode.claude()}),
    )
}

/// Effort has no dedicated request; it is a flag-settings overlay (`effortLevel`).
pub fn set_effort(request_id: &str, effort: Option<Effort>) -> Value {
    control(
        request_id,
        json!({
            "subtype": "apply_flag_settings",
            "settings": {"effortLevel": effort.map(Effort::key)},
        }),
    )
}

/// Stops one background task (a shell, subagent or monitor) the CLI runs.
pub fn stop_task(request_id: &str, task: &str) -> Value {
    control(request_id, json!({"subtype": "stop_task", "task_id": task}))
}

fn control(request_id: &str, request: Value) -> Value {
    json!({"type": "control_request", "request_id": request_id, "request": request})
}

/// Commands and skills listed in the `initialize` response.
pub fn commands(initialize: &Value) -> Vec<CommandInfo> {
    let Some(commands) = initialize.get("commands").and_then(Value::as_array) else {
        return vec![];
    };
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(600)
            .collect::<String>()
    };
    crate::chat::config::tidy_commands(
        commands
            .iter()
            .filter_map(|command| {
                let name = command.get("name")?.as_str()?.trim_start_matches('/');
                crate::chat::config::valid_command(name).then(|| CommandInfo {
                    name: name.to_owned(),
                    description: text(command, "description"),
                    hint: Some(text(command, "argumentHint")).filter(|hint| !hint.is_empty()),
                    path: None,
                    // Until `system/init` lists the skills: the user's, project and
                    // plugin entries are skills, built-in ones commands.
                    skill: command.get("builtin").and_then(Value::as_bool) != Some(true),
                })
            })
            .collect(),
    )
}

/// Models listed in the `initialize` response, in CLI order.
pub fn models(initialize: &Value) -> Vec<ModelInfo> {
    let Some(models) = initialize.get("models").and_then(Value::as_array) else {
        return vec![];
    };
    models
        .iter()
        .filter_map(|model| {
            let value = model.get("value")?.as_str()?;
            if !crate::chat::config::valid_model(value) {
                return None;
            }
            let text = |key: &str| {
                model
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(512)
                    .collect::<String>()
            };
            let supports_effort =
                model.get("supportsEffort").and_then(Value::as_bool) == Some(true);
            Some(ModelInfo {
                value: value.to_owned(),
                resolved: Some(text("resolvedModel")).filter(|name| !name.is_empty()),
                display_name: Some(text("displayName"))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| value.to_owned()),
                description: text("description"),
                effort_levels: if supports_effort {
                    model
                        .get("supportedEffortLevels")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|level| Effort::from_key(level.as_str()?))
                        // A Codex level; Claude doesn't take it.
                        .filter(|effort| *effort != Effort::Ultra)
                        .collect()
                } else {
                    vec![]
                },
                supports_auto: model.get("supportsAutoMode").and_then(Value::as_bool) == Some(true),
                ..Default::default()
            })
        })
        .take(64)
        .collect()
}

/// `cancel_queued` also drops our messages the CLI still holds (needs its capability).
pub fn interrupt(request_id: &str, cancel_queued: bool) -> Value {
    let request = if cancel_queued {
        json!({"subtype": "interrupt", "cancel_queued": true})
    } else {
        json!({"subtype": "interrupt"})
    };
    control(request_id, request)
}

pub fn allow(request_id: &str, tool_use_id: Option<&str>, input: &Value) -> Value {
    success(
        request_id,
        with_tool_use(
            json!({"behavior": "allow", "updatedInput": input}),
            tool_use_id,
        ),
    )
}

pub fn deny(request_id: &str, tool_use_id: Option<&str>, message: &str) -> Value {
    success(
        request_id,
        with_tool_use(
            json!({"behavior": "deny", "message": message, "interrupt": false}),
            tool_use_id,
        ),
    )
}

fn with_tool_use(mut response: Value, tool_use_id: Option<&str>) -> Value {
    if let Some(id) = tool_use_id {
        response["toolUseID"] = id.into();
    }
    response
}

/// Answer to a control request Canopy does not support (hook callbacks, SDK MCP, dialogs).
pub fn unsupported(request_id: &str, subtype: Option<&str>) -> Value {
    match subtype {
        Some("elicitation") => success(request_id, json!({"action": "decline"})),
        Some("request_user_dialog") => success(request_id, json!({"behavior": "cancelled"})),
        _ => json!({
            "type": "control_response",
            "response": {
                "subtype": "error",
                "request_id": request_id,
                "error": "Not supported by Canopy.",
            },
        }),
    }
}

fn success(request_id: &str, response: Value) -> Value {
    json!({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": request_id, "response": response},
    })
}

pub fn request_subtype(request: &Value) -> Option<&str> {
    request.get("subtype")?.as_str()
}

/// Asks how much of the model's context window the conversation uses.
pub fn get_context_usage(request_id: &str) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": {"subtype": "get_context_usage"},
    })
}

/// Restores files to their state before the user message `user_message_id`; a dry run
/// only reports what would change. Needs file checkpointing in the CLI's environment.
pub fn rewind_files(request_id: &str, user_message_id: &str, dry_run: bool) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": {"subtype": "rewind_files", "user_message_id": user_message_id, "dry_run": dry_run},
    })
}

pub fn context_usage(response: &Value) -> Option<ContextUsage> {
    let max = response["maxTokens"].as_u64().filter(|max| *max > 0)?;
    let buffer = response["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|category| category["kind"] == "buffer")
        .filter_map(|category| category["tokens"].as_u64())
        .sum();
    Some(ContextUsage {
        used: response["totalTokens"].as_u64()?,
        max,
        buffer,
    })
}

pub use crate::chat::backend::RewindPreview;

pub fn rewind_preview(response: &Value) -> RewindPreview {
    RewindPreview {
        possible: response["canRewind"].as_bool().unwrap_or(false),
        files: response["filesChanged"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        insertions: response["insertions"].as_u64().unwrap_or(0),
        deletions: response["deletions"].as_u64().unwrap_or(0),
    }
}

/// A permission prompt (`can_use_tool`), including AskUserQuestion.
pub fn can_use_tool(request: &Value) -> Option<CanUseTool> {
    (request_subtype(request) == Some("can_use_tool"))
        .then(|| serde_json::from_value(request.clone()).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Inbound {
        parse_line(json.as_bytes()).unwrap().unwrap()
    }

    #[test]
    fn unknown_types_and_fields_are_tolerated() {
        assert!(parse_line(br#"{"type":"keep_alive"}"#).unwrap().is_none());
        assert!(
            parse_line(br#"{"type":"brand_new","x":1}"#)
                .unwrap()
                .is_none()
        );
        assert!(parse_line(b"  \n").unwrap().is_none());
        assert!(matches!(
            parse(r#"{"type":"system","subtype":"brand_new","whatever":[1]}"#),
            Inbound::System(System::Other)
        ));
        assert!(matches!(
            parse(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"citations_delta","x":1}},"new_field":true}"#
            ),
            Inbound::StreamEvent(StreamEvent {
                event: ApiEvent::ContentBlockDelta {
                    delta: Delta::Other,
                    ..
                },
                ..
            })
        ));
        assert!(parse_line(b"not json").is_err());
    }

    #[test]
    fn result_tag_may_follow_other_fields() {
        let Inbound::Result(result) = parse(
            r#"{"duration_ms":10,"session_id":"s","total_cost_usd":0.5,"usage":{"input_tokens":2,"output_tokens":4},"type":"result","subtype":"success","is_error":false,"result":"ok"}"#,
        ) else {
            panic!("expected result");
        };
        assert_eq!(result.result.as_deref(), Some("ok"));
        assert_eq!(result.usage.unwrap().output_tokens, 4);
    }

    #[test]
    fn tool_results_flatten_string_and_block_content() {
        let Inbound::User(user) = parse(
            r#"{"type":"user","message":{"role":"user","content":[
                {"type":"tool_result","tool_use_id":"a","content":"plain"},
                {"type":"tool_result","tool_use_id":"b","is_error":true,"content":[{"type":"text","text":"x"},{"type":"text","text":"y"}]},
                {"type":"text","text":"ignored"}]}}"#,
        ) else {
            panic!("expected user");
        };
        let results = user.tool_results();
        assert_eq!(results.len(), 2);
        assert_eq!(
            (results[0].tool_use_id, results[0].text.as_str()),
            ("a", "plain")
        );
        assert!(results[1].is_error);
        assert_eq!(results[1].text, "x\ny");
    }

    #[test]
    fn settings_requests_and_models_follow_the_sdk_contract() {
        assert_eq!(
            set_model("r1", Some("opus"))["request"],
            json!({"subtype":"set_model","model":"opus"})
        );
        assert_eq!(set_model("r1", None)["request"]["model"], Value::Null);
        assert_eq!(
            set_permission_mode("r2", Mode::Yolo)["request"],
            json!({"subtype":"set_permission_mode","mode":"bypassPermissions"})
        );
        assert_eq!(
            set_effort("r3", Some(Effort::XHigh))["request"],
            json!({"subtype":"apply_flag_settings","settings":{"effortLevel":"xhigh"}})
        );
        let models = models(&json!({"models": [
            {"value": "default", "displayName": "Default (recommended)", "description": "d",
             "supportsEffort": true, "supportedEffortLevels": ["low", "high", "ultra"], "supportsAutoMode": true},
            {"value": "haiku", "displayName": "", "supportsEffort": false, "supportedEffortLevels": ["low"]},
            {"value": "--print"},
            {"displayName": "no value"}
        ]}));
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].effort_levels, [Effort::Low, Effort::High]);
        assert!(models[0].supports_auto);
        assert_eq!(models[1].display_name, "haiku");
        assert!(!models[1].supports_effort());
    }

    #[test]
    fn command_lifecycle_frames_parse_with_unknown_states() {
        let parse =
            |state: &str| {
                match parse_line(
            format!(
                r#"{{"type":"command_lifecycle","command_uuid":"u1","state":"{state}","uuid":"x"}}"#
            )
            .as_bytes(),
        ) {
            Ok(Some(Inbound::CommandLifecycle {
                command_uuid,
                state,
            })) => (command_uuid, state),
            other => panic!("{other:?}"),
        }
            };
        assert_eq!(parse("started"), ("u1".into(), Lifecycle::Started));
        assert!(parse("refused").1.terminal());
        assert_eq!(parse("paused").1, Lifecycle::Other);
        assert!(!Lifecycle::Queued.terminal());
    }

    #[test]
    fn initialize_lists_commands_and_skills() {
        let response = json!({"commands": [
            {"name": "review", "description": "Review a PR (user)", "argumentHint": "<pr>"},
            {"name": "compact", "description": "Compact the conversation", "argumentHint": ""},
            {"name": "bad name", "description": "spaces"},
            {"name": "review", "description": "duplicate"},
            {"description": "no name"}
        ]});
        let commands = commands(&response);
        assert_eq!(
            commands
                .iter()
                .map(|c| (c.name.as_str(), c.hint.as_deref()))
                .collect::<Vec<_>>(),
            [("compact", None), ("review", Some("<pr>"))]
        );
        assert!(super::commands(&json!({})).is_empty());
    }

    #[test]
    fn outbound_shapes_match_the_sdk_contract() {
        assert_eq!(
            user_message("hi", &[], None, "u1", Priority::Later),
            json!({"type":"user","session_id":"","message":{"role":"user","content":[{"type":"text","text":"hi"}]},"parent_tool_use_id":null,"uuid":"u1","priority":"later"})
        );
        assert_eq!(
            user_message("hi", &[], None, "u2", Priority::Next)["priority"],
            "next"
        );
        let attachments = [
            Attachment::Image {
                file: "a.png".into(),
                media_type: "image/png".into(),
                width: 1,
                height: 1,
                data: Some("QUJD".into()),
                label: None,
            },
            Attachment::Path {
                path: "/tmp/notes.txt".into(),
                label: None,
            },
        ];
        assert_eq!(
            user_message("look", &attachments, None, "u3", Priority::Later)["message"]["content"],
            json!([
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":"QUJD"}},
                {"type":"text","text":"look\n\nAttached files (read them as needed):\n- /tmp/notes.txt"}
            ])
        );
        // A quote is a quote block where its badge is.
        let quote = crate::chat::quotes::quote("KIWI is the word").unwrap();
        let text = format!("what is {} about?", quote.label().unwrap());
        assert_eq!(
            user_message(&text, &[quote], None, "u4", Priority::Later)["message"]["content"],
            json!([{"type":"text","text":"what is\n\n> KIWI is the word\n\nabout?"}])
        );
        assert_eq!(
            allow("r1", Some("toolu_1"), &json!({"command":"ls"})),
            json!({"type":"control_response","response":{"subtype":"success","request_id":"r1","response":{"behavior":"allow","updatedInput":{"command":"ls"},"toolUseID":"toolu_1"}}})
        );
        assert_eq!(
            deny("r2", None, "No")["response"]["response"],
            json!({"behavior":"deny","message":"No","interrupt":false})
        );
        assert_eq!(
            interrupt("r3", false),
            json!({"type":"control_request","request_id":"r3","request":{"subtype":"interrupt"}})
        );
        assert_eq!(interrupt("r3", true)["request"]["cancel_queued"], true);
        assert!(initialize("r0")["request"].get("systemPrompt").is_none());
        assert_eq!(
            unsupported("r4", Some("hook_callback"))["response"]["subtype"],
            "error"
        );
        assert_eq!(
            unsupported("r5", Some("elicitation"))["response"]["response"]["action"],
            "decline"
        );
    }
}
