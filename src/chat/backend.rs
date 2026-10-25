//! Shared results of agent protocols. Claude, Codex and future adapters use these
//! types; they do not belong to any one provider's wire format.
use super::goal::GoalEvent;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InitInfo {
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    pub api_key_source: Option<String>,
    pub skills: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliState {
    Running,
    Idle,
    RequiresAction,
}

/// Context use in tokens, independently of the protocol that supplied it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContextUsage {
    pub used: u64,
    pub max: u64,
    pub buffer: u64,
}

/// Progress of a submitted message. Unknown protocol states never imply delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageLifecycle {
    Queued,
    Started,
    Completed,
    Cancelled,
    Discarded,
    Refused,
    #[serde(other)]
    Other,
}
impl MessageLifecycle {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Discarded | Self::Refused
        )
    }
}

/// Backend evidence about one durably dispatched message. Writing to stdin is not
/// evidence: only the backend's acknowledgement or its first correlated turn event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receipt {
    Delivered,
    /// The backend answered that it did not take the message.
    Refused,
    /// The run ended before any evidence: the effect is uncertain.
    Unknown,
}

/// Adapter results consumed by the session controller. Protocol control replies
/// remain internal to the backend/session boundary, never part of the transcript.
#[derive(Debug, PartialEq)]
pub enum Signal {
    ControlAck {
        request_id: String,
        error: Option<String>,
        response: Option<Value>,
    },
    Init(InitInfo),
    SessionId(String),
    State(CliState),
    PermissionRequested,
    PermissionWithdrawn,
    TurnFinished,
    BackgroundTasks(usize),
    PermissionMode(String),
    Reply(Value),
    Lifecycle {
        uuid: String,
        state: MessageLifecycle,
    },
    Goal(GoalEvent),
}

/// What restoring files would change, from a `rewind_files` dry run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RewindPreview {
    pub possible: bool,
    pub files: Vec<String>,
    pub insertions: u64,
    pub deletions: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ToolKind {
    Shell,
    Read,
    Edit,
    Search,
    Web,
    Agent,
    Plan,
    #[default]
    Other,
}

pub fn tool_kind(name: &str) -> ToolKind {
    match name {
        "commandExecution" | "Bash" | "BashOutput" | "KillShell" | "Monitor" => ToolKind::Shell,
        "Read" | "NotebookRead" => ToolKind::Read,
        "fileChange" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => ToolKind::Edit,
        "Grep" | "Glob" | "LS" | "ToolSearch" => ToolKind::Search,
        "webSearch" | "WebFetch" | "WebSearch" => ToolKind::Web,
        "collabAgentToolCall" | "Agent" | "Task" | "SendMessage" | "TaskStop" => ToolKind::Agent,
        "plan" | "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet"
        | "ExitPlanMode" | "EnterPlanMode" => ToolKind::Plan,
        _ => ToolKind::Other,
    }
}
