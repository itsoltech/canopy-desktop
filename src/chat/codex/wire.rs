//! Codex `app-server` wire format: JSON-RPC 2.0 without the `jsonrpc` field, one
//! message per line on stdio (live-verified with codex-cli 0.161.0).
//!
//! The protocol is marked experimental by Codex and changes between versions: messages
//! are kept as loose JSON and read field by field, so unknown shapes are skipped rather
//! than failing the line.
use crate::chat::config::{Effort, Mode, ModelInfo};
use crate::chat::transcript::Attachment;
use serde_json::{Value, json};
use std::path::Path;

/// Lines beyond this size are dropped with a diagnostic instead of being buffered.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The answer to one of our requests.
    Response {
        id: String,
        result: Value,
    },
    Error {
        id: String,
        message: String,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// The server asks something of us (approvals, questions); `id` is kept as sent.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
}

/// Request ids as text, whatever JSON type the server used.
pub fn id_text(id: &Value) -> String {
    match id {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub fn parse_line(line: &[u8]) -> Result<Option<Message>, String> {
    let line = std::str::from_utf8(line).map_err(|_| "Codex wrote a line that isn't UTF-8.")?;
    if line.trim().is_empty() {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_str(line).map_err(|error| format!("Unreadable Codex line: {error}"))?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    Ok(match (value.get("id"), method) {
        (Some(id), Some(method)) => Some(Message::Request {
            id: id.clone(),
            method,
            params,
        }),
        (None, Some(method)) => Some(Message::Notification { method, params }),
        (Some(id), None) => Some(match value.get("error") {
            Some(error) => Message::Error {
                id: id_text(id),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex reported an error.")
                    .to_owned(),
            },
            None => Message::Response {
                id: id_text(id),
                result: value.get("result").cloned().unwrap_or(Value::Null),
            },
        }),
        (None, None) => None,
    })
}

fn request(id: &str, method: &str, params: Value) -> Value {
    json!({"id": id, "method": method, "params": params})
}

/// The reply to a server request.
pub fn reply(id: &Value, result: Value) -> Value {
    json!({"id": id, "result": result})
}

/// A request we can't serve (dynamic tools, attestation…).
pub fn refuse(id: &Value, message: &str) -> Value {
    json!({"id": id, "error": {"code": -32601, "message": message}})
}

pub fn initialize(id: &str) -> Value {
    request(
        id,
        "initialize",
        json!({
            "clientInfo": {"name": "canopy", "title": "Canopy", "version": env!("CARGO_PKG_VERSION")},
            // Questions to the user (`item/tool/requestUserInput`) are experimental.
            "capabilities": {"experimentalApi": true},
        }),
    )
}

pub fn initialized() -> Value {
    json!({"method": "initialized"})
}

/// How a thread runs: its folder and what the agent may do.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadOptions<'a> {
    pub cwd: &'a Path,
    pub mode: Mode,
    pub model: Option<&'a str>,
}
impl ThreadOptions<'_> {
    fn params(&self) -> Value {
        let (approval, sandbox) = self.mode.codex();
        let mut params = json!({
            "cwd": self.cwd,
            "approvalPolicy": approval,
            "sandbox": sandbox,
        });
        if let Some(model) = self.model {
            params["model"] = json!(model);
        }
        params
    }
}

pub fn thread_start(id: &str, options: &ThreadOptions) -> Value {
    request(id, "thread/start", options.params())
}

/// Continues saved thread `thread`; its history isn't sent back (Canopy keeps its own).
pub fn thread_resume(id: &str, thread: &str, options: &ThreadOptions) -> Value {
    let mut params = options.params();
    params["threadId"] = json!(thread);
    params["excludeTurns"] = json!(true);
    request(id, "thread/resume", params)
}

/// A new thread holding `thread`'s history up to turn `last_turn` (an edited message).
pub fn thread_fork(id: &str, thread: &str, last_turn: &str, options: &ThreadOptions) -> Value {
    let mut params = options.params();
    params["threadId"] = json!(thread);
    params["lastTurnId"] = json!(last_turn);
    params["excludeTurns"] = json!(true);
    request(id, "thread/fork", params)
}

pub fn model_list(id: &str) -> Value {
    request(id, "model/list", json!({}))
}

pub fn account_read(id: &str) -> Value {
    request(id, "account/read", json!({}))
}

pub fn rate_limits_read(id: &str) -> Value {
    request(id, "account/rateLimits/read", json!({}))
}

/// What the agent may do from this turn on (`turn/start` overrides persist).
fn sandbox_policy(mode: Mode) -> Value {
    match mode.codex().1 {
        "read-only" => json!({"type": "readOnly"}),
        "danger-full-access" => json!({"type": "dangerFullAccess"}),
        _ => json!({"type": "workspaceWrite"}),
    }
}

/// A user message as Codex input: text, images by path (from the chat's attachment
/// folder `images`), files and folders as mentions.
pub fn input(text: &str, attachments: &[Attachment], images: Option<&Path>) -> Vec<Value> {
    let mut input = vec![];
    if !text.is_empty() {
        input.push(json!({"type": "text", "text": text, "text_elements": []}));
    }
    for attachment in attachments {
        match attachment {
            Attachment::Image { file, .. } => {
                if let Some(folder) = images {
                    input.push(json!({"type": "localImage", "path": folder.join(file)}));
                }
            }
            Attachment::Path { path, .. } => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                input.push(json!({"type": "mention", "name": name, "path": path}));
            }
        }
    }
    input
}

/// Choices that apply from this turn on.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnOptions<'a> {
    pub mode: Mode,
    pub model: Option<&'a str>,
    pub effort: Option<Effort>,
}

pub fn turn_start(id: &str, thread: &str, input: Vec<Value>, options: &TurnOptions) -> Value {
    let mut params = json!({
        "threadId": thread,
        "input": input,
        "approvalPolicy": options.mode.codex().0,
        "sandboxPolicy": sandbox_policy(options.mode),
        "summary": "auto",
    });
    if let Some(model) = options.model {
        params["model"] = json!(model);
    }
    if let Some(effort) = options.effort {
        params["effort"] = json!(effort.key());
    }
    request(id, "turn/start", params)
}

/// Sends input into the running turn `turn`; refused when that turn has ended.
pub fn turn_steer(id: &str, thread: &str, turn: &str, input: Vec<Value>) -> Value {
    request(
        id,
        "turn/steer",
        json!({"threadId": thread, "input": input, "expectedTurnId": turn}),
    )
}

pub fn turn_interrupt(id: &str, thread: &str, turn: &str) -> Value {
    request(
        id,
        "turn/interrupt",
        json!({"threadId": thread, "turnId": turn}),
    )
}

/// Models the account offers (`model/list`), default first.
pub fn models(result: &Value) -> Vec<ModelInfo> {
    let mut models: Vec<ModelInfo> = result
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|model| model.get("hidden").and_then(Value::as_bool) != Some(true))
        .filter_map(|model| {
            let value = model.get("id").and_then(Value::as_str)?.to_owned();
            let display = model
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(&value)
                .to_owned();
            Some(ModelInfo {
                resolved: None,
                display_name: display,
                description: model
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                effort_levels: model
                    .get("supportedReasoningEfforts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|e| Effort::from_key(e.get("reasoningEffort")?.as_str()?))
                    .collect(),
                supports_auto: false,
                default_effort: model
                    .get("defaultReasoningEffort")
                    .and_then(Value::as_str)
                    .and_then(Effort::from_key),
                default: model.get("isDefault").and_then(Value::as_bool) == Some(true),
                value,
            })
        })
        .collect();
    models.sort_by_key(|model| !model.default);
    models
}

/// The shell command without the login-shell wrapper Codex adds (`/bin/zsh -lc '…'`).
pub fn command_text(command: &str) -> String {
    for shell in [
        "/bin/zsh -lc ",
        "/bin/bash -lc ",
        "bash -lc ",
        "zsh -lc ",
        "sh -c ",
    ] {
        if let Some(rest) = command.strip_prefix(shell) {
            if let Ok(mut words) = shell_words::split(rest)
                && words.len() == 1
            {
                return words.remove(0);
            }
            return rest.to_owned();
        }
    }
    command.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_parse_into_responses_notifications_and_requests() {
        let parse = |line: &str| parse_line(line.as_bytes()).unwrap().unwrap();
        assert_eq!(
            parse(r#"{"id":"canopy-1","result":{"ok":true}}"#),
            Message::Response {
                id: "canopy-1".into(),
                result: json!({"ok": true})
            }
        );
        assert_eq!(
            parse(r#"{"id":2,"error":{"code":1,"message":"no"}}"#),
            Message::Error {
                id: "2".into(),
                message: "no".into()
            }
        );
        assert!(matches!(
            parse(r#"{"method":"turn/started","params":{}}"#),
            Message::Notification { .. }
        ));
        assert!(matches!(
            parse(r#"{"id":0,"method":"item/fileChange/requestApproval","params":{}}"#),
            Message::Request {
                id: Value::Number(_),
                ..
            }
        ));
        assert_eq!(parse_line(b"  ").unwrap(), None);
        assert!(parse_line(b"{").is_err());
    }

    #[test]
    fn requests_carry_the_chosen_mode_and_input() {
        let options = ThreadOptions {
            cwd: Path::new("/repo"),
            mode: Mode::ReadOnly,
            model: Some("gpt-6.1-sol"),
        };
        let start = thread_start("c1", &options);
        assert_eq!(start["params"]["approvalPolicy"], "on-request");
        assert_eq!(start["params"]["sandbox"], "read-only");
        assert_eq!(
            thread_fork("c2", "t", "turn-1", &options)["params"]["lastTurnId"],
            "turn-1"
        );
        let attachments = [
            Attachment::Image {
                file: "a.png".into(),
                media_type: "image/png".into(),
                width: 1,
                height: 1,
                data: None,
                label: None,
            },
            Attachment::Path {
                path: "/repo/notes.txt".into(),
                label: None,
            },
        ];
        let turn = turn_start(
            "c3",
            "t",
            input("hi", &attachments, Some(Path::new("/data/chat"))),
            &TurnOptions {
                mode: Mode::FullAccess,
                model: None,
                effort: Some(Effort::Ultra),
            },
        );
        assert_eq!(
            turn["params"]["input"],
            json!([
                {"type":"text","text":"hi","text_elements":[]},
                {"type":"localImage","path":"/data/chat/a.png"},
                {"type":"mention","name":"notes.txt","path":"/repo/notes.txt"}
            ])
        );
        assert_eq!(turn["params"]["approvalPolicy"], "never");
        assert_eq!(turn["params"]["sandboxPolicy"]["type"], "dangerFullAccess");
        assert_eq!(turn["params"]["effort"], "ultra");
        assert_eq!(command_text("/bin/zsh -lc 'ls -la'"), "ls -la");
        assert_eq!(
            command_text("/bin/zsh -lc \"printf 'v1' > notes.txt\""),
            "printf 'v1' > notes.txt"
        );
        assert_eq!(command_text("git status"), "git status");
    }

    #[test]
    fn models_come_default_first_with_their_efforts() {
        let models = models(&json!({"data": [
            {"id":"b","displayName":"B","supportedReasoningEfforts":[{"reasoningEffort":"low"}],"defaultReasoningEffort":"low","isDefault":false},
            {"id":"a","displayName":"A","supportedReasoningEfforts":[{"reasoningEffort":"high"},{"reasoningEffort":"ultra"}],"defaultReasoningEffort":"high","isDefault":true},
            {"id":"h","hidden":true}
        ]}));
        assert_eq!(
            models.iter().map(|m| m.value.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(models[0].effort_levels, [Effort::High, Effort::Ultra]);
        assert_eq!(models[0].default_effort, Some(Effort::High));
    }
}
