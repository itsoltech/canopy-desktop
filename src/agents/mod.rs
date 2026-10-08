//! Agent hook transport and per-run identity, independent of UI and SQLite.
pub mod launch;
pub mod presentation;
pub mod relay;
pub mod transcript;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    #[serde(default)]
    pub transcript: Option<std::path::PathBuf>,
    pub run: String,
    pub subagent: bool,
    #[serde(default)]
    pub interrupted: bool,
    #[serde(default)]
    pub notification_type: Option<String>,
    pub session: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub permission: Option<String>,
    pub tool: Option<String>,
    pub tool_use_id: Option<String>,
    pub response: Option<String>,
    pub question: Option<String>,
}
impl Event {
    pub fn from_json(run: String, raw: &serde_json::Value) -> Self {
        let text = |key: &str, max: usize| {
            raw.get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.chars().take(max).collect::<String>())
        };
        Self {
            transcript: text("transcript_path", 4096).map(Into::into),
            run,
            interrupted: raw
                .get("is_interrupt")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            notification_type: text("notification_type", 80),
            subagent: raw
                .get("agent_id")
                .and_then(|v| v.as_str())
                .is_some_and(|v| !v.is_empty())
                || raw
                    .get("parent_session_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|v| !v.is_empty()),
            session: text("session_id", 128).filter(|s| valid_session(s)),
            name: text("hook_event_name", 80).unwrap_or_default(),
            model: text("model", 128),
            permission: text("permission_mode", 80),
            tool: text("tool_name", 128),
            tool_use_id: text("tool_use_id", 128).or_else(|| text("call_id", 128)),
            question: raw.get("tool_input").and_then(question_text),
            response: text("last_assistant_message", 4096),
        }
    }
}
fn question_text(input: &serde_json::Value) -> Option<String> {
    let decoded;
    let input = if let Some(json) = input.as_str() {
        decoded = serde_json::from_str::<serde_json::Value>(json).ok()?;
        &decoded
    } else {
        input
    };
    input
        .get("questions")?
        .as_array()?
        .first()?
        .get("question")?
        .as_str()
        .map(|s| s.chars().take(1024).collect())
}

impl Event {
    pub fn is_question_tool(&self) -> bool {
        self.tool
            .as_deref()
            .and_then(|name| name.rsplit('.').next())
            .is_some_and(|name| matches!(name, "request_user_input" | "AskUserQuestion"))
    }
    pub fn status(&self, previous: Status) -> Status {
        if self.subagent && matches!(self.name.as_str(), "Stop" | "SessionEnd") {
            return previous;
        }
        if (self.name == "PostToolUseFailure" && self.interrupted)
            || (self.name == "PermissionDenied" && self.is_question_tool())
        {
            return Status::Idle;
        }
        if self.name == "Notification" {
            return match self.notification_type.as_deref() {
                Some("idle_prompt" | "agent_completed") => Status::Idle,
                Some(
                    "permission_prompt"
                    | "elicitation_dialog"
                    | "agent_needs_input"
                    | "worker_permission_prompt",
                ) => Status::Waiting,
                _ => previous,
            };
        }
        if self.name == "PreToolUse" && self.is_question_tool() {
            Status::Waiting
        } else {
            previous.event(&self.name)
        }
    }
}

pub fn valid_session(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    Working,
    Waiting,
    Idle,
    Failed,
    Exited,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "Starting",
            Self::Working => "Working",
            Self::Waiting => "Needs attention",
            Self::Idle => "Idle",
            Self::Failed => "Error",
            Self::Exited => "Exited",
        }
    }
    pub fn event(self, name: &str) -> Self {
        match name {
            "SessionStart" => Self::Idle,
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "SubagentStart" | "PreCompact" => {
                Self::Working
            }
            "PermissionRequest" | "Notification" | "UserInputRequested" => Self::Waiting,
            "UserInputResolved" => Self::Working,
            "Stop" | "Interrupt" => Self::Idle,
            "PostToolUseFailure" | "StopFailure" | "PermissionDenied" => Self::Failed,
            "SessionEnd" => Self::Exited,
            _ => self,
        }
    }
}

/// Pending interactions survive unrelated/background tool completions.
#[derive(Clone, Default)]
pub struct Attention {
    pending: std::collections::BTreeMap<String, Option<String>>,
}
impl Attention {
    pub fn apply(&mut self, event: &Event) {
        let key = event
            .tool_use_id
            .as_deref()
            .or(event.tool.as_deref())
            .unwrap_or("permission");
        if (event.name == "PreToolUse" && event.is_question_tool())
            || event.name == "PermissionRequest"
        {
            if self.pending.len() < 32 {
                self.pending.insert(key.to_owned(), event.question.clone());
            }
        } else if matches!(
            event.name.as_str(),
            "PostToolUse" | "PostToolUseFailure" | "PermissionDenied"
        ) {
            self.pending.remove(key);
            // Claude PermissionRequest has no tool_use_id; its matching completion does.
            if let Some(tool) = &event.tool {
                self.pending.remove(tool);
            }
        } else if !event.subagent
            && ((event.name == "Notification"
                && matches!(
                    event.notification_type.as_deref(),
                    Some("idle_prompt" | "agent_completed")
                ))
                || matches!(
                    event.name.as_str(),
                    "UserPromptSubmit"
                        | "Stop"
                        | "StopFailure"
                        | "SessionEnd"
                        | "Interrupt"
                        | "SessionStart"
                ))
        {
            self.pending.clear();
        }
    }
    pub fn contains(&self, call: &str) -> bool {
        self.pending.contains_key(call)
    }
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn question(&self) -> Option<String> {
        self.pending.values().find_map(Clone::clone)
    }
}

/// Significant transitions only; startup/ordinary tool activity are not alerts.
pub fn should_notify(previous: Status, next: Status, visible: bool) -> bool {
    if visible || previous == next {
        return false;
    }
    match next {
        Status::Waiting | Status::Failed => true,
        Status::Idle => matches!(previous, Status::Working | Status::Waiting),
        Status::Exited => previous != Status::Starting,
        _ => false,
    }
}

pub fn pane_visible(
    window_active: bool,
    selected: Option<crate::state::workspace::WorkspaceId>,
    workspace: &crate::state::workspace::Workspace,
    pane: crate::state::workspace::PaneId,
) -> bool {
    window_active
        && selected == Some(workspace.id)
        && workspace
            .active()
            .is_some_and(|tab| tab.root.find(pane).is_some())
}
