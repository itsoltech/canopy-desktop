//! What a chat view shows for a transcript, independent of GPUI.
use super::{
    session::Status,
    transcript::{Body, Decision, Item, Outcome, Permission, ToolCall, ToolState, Transcript},
};
use serde_json::Value;
use std::path::Path;

/// Token counts as people read them: "840", "1.2k", "35k", "1.0M".
pub fn tokens(count: u64) -> String {
    match count {
        0..1_000 => count.to_string(),
        1_000..10_000 => format!("{:.1}k", count as f64 / 1_000.),
        10_000..1_000_000 => format!("{:.0}k", count as f64 / 1_000.),
        _ => format!("{:.1}M", count as f64 / 1_000_000.),
    }
}

/// One question of an AskUserQuestion request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub question: String,
    /// Short label ("Color"); may be empty.
    pub header: String,
    /// Choices as (label, description).
    pub options: Vec<(String, String)>,
    /// Several choices may be picked; their labels are sent joined with ", ".
    pub multi: bool,
}

/// The questions of a question request, tolerating missing fields (a question without
/// options is answered with text).
pub fn questions(permission: &Permission) -> Vec<Question> {
    let text = |value: &serde_json::Value| value.as_str().unwrap_or_default().trim().to_owned();
    permission.input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|question| {
            let text_of_question = text(&question["question"]);
            (!text_of_question.is_empty()).then(|| Question {
                question: text_of_question,
                header: text(&question["header"]),
                options: question["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|option| (text(&option["label"]), text(&option["description"])))
                    .filter(|(label, _)| !label.is_empty())
                    .collect(),
                multi: question["multiSelect"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// Whether an item gets a row. Successful turn ends add no noise, and thinking
/// is not displayed until the thinking stage.
pub fn visible(item: &Item) -> bool {
    match &item.body {
        // Thinking summaries show as a foldable row (empty blocks too: "Thought for Ns").
        Body::Thinking(_) => true,
        // Delivers the subagent's report, which its card and conversation already show.
        Body::Tool(tool) if tool.name == HANDBACK_TOOL => false,
        Body::TurnEnd(end) => end.outcome != Outcome::Completed,
        _ => true,
    }
}

/// Whether a finished call returned anything worth expanding. Claude Code reports an
/// empty result with placeholders such as "(empty)".
pub fn has_output(tool: &ToolCall) -> bool {
    tool.output.as_ref().is_some_and(|output| {
        let text = output.text.trim();
        !text.is_empty()
            && !matches!(
                text.to_ascii_lowercase().as_str(),
                "(empty)" | "(no content)" | "(no output)" | "(empty output)"
            )
    })
}

/// Claude Code tool a subagent calls with `{message}` to hand its report to the caller.
pub const HANDBACK_TOOL: &str = "SubagentHandback";

/// A tool call that runs a subagent; its output items carry the call id as parent.
pub fn is_subagent(tool: &ToolCall) -> bool {
    matches!(tool.name.as_str(), "Agent" | "Task")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentState {
    Working,
    Finished,
    Failed,
    Interrupted,
}
impl SubagentState {
    pub fn of(tool: &ToolCall) -> Self {
        match tool.state {
            ToolState::Streaming | ToolState::Running => Self::Working,
            ToolState::Succeeded => Self::Finished,
            ToolState::Failed => Self::Failed,
            ToolState::Cancelled => Self::Interrupted,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Finished => "Finished",
            Self::Failed => "Failed",
            Self::Interrupted => "Interrupted",
        }
    }
}

/// The task as the agent described it, falling back to the prompt and the agent type.
pub fn subagent_title(tool: &ToolCall) -> String {
    let text = |key: &str| {
        tool.input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    let title = text("description")
        .or_else(|| text("prompt"))
        .or_else(|| text("subagent_type"))
        .unwrap_or("Subagent");
    single_line(title, 120)
}

/// The prompt the subagent received: the first message of its conversation.
pub fn subagent_prompt(tool: &ToolCall) -> Option<&str> {
    tool.input
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|prompt| !prompt.trim().is_empty())
}

/// One line under the card title: what it is doing now, or how it ended.
pub fn subagent_preview(tool: &ToolCall, last_child: Option<&ToolCall>, cwd: &Path) -> String {
    match SubagentState::of(tool) {
        SubagentState::Working => tool
            .activity
            .clone()
            .or_else(|| {
                last_child.map(|child| {
                    let summary = tool_summary(child, cwd);
                    if summary.is_empty() {
                        child.name.clone()
                    } else {
                        format!("{} {summary}", child.name)
                    }
                })
            })
            .map(|text| single_line(&text, 160))
            .unwrap_or_default(),
        _ => tool
            .output
            .as_ref()
            .map(|output| single_line(output.text.trim(), 160))
            .unwrap_or_default(),
    }
}

/// The subagent of `call` is busy without writing: its view shows the thinking row.
pub fn thread_thinking(transcript: &Transcript, call: &str, tool: &ToolCall) -> bool {
    if tool.state.finished() {
        return false;
    }
    let last = transcript
        .items()
        .iter()
        .rev()
        .find(|item| item.parent.as_deref() == Some(call) && visible(item));
    !matches!(last.map(|item| &item.body), Some(Body::Text(block)) if block.streaming && !block.text.is_empty())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Shell,
    Read,
    Edit,
    Search,
    Web,
    Agent,
    Plan,
    Other,
}

pub fn tool_kind(name: &str) -> ToolKind {
    match name {
        "Bash" | "BashOutput" | "KillShell" | "Monitor" => ToolKind::Shell,
        "Read" | "NotebookRead" => ToolKind::Read,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => ToolKind::Edit,
        "Grep" | "Glob" | "LS" | "ToolSearch" => ToolKind::Search,
        "WebFetch" | "WebSearch" => ToolKind::Web,
        "Agent" | "Task" | "SendMessage" | "TaskStop" => ToolKind::Agent,
        "TodoWrite" | "ExitPlanMode" | "EnterPlanMode" => ToolKind::Plan,
        _ => ToolKind::Other,
    }
}

/// One-line description of a tool call: the command, path, pattern or task.
pub fn tool_summary(tool: &ToolCall, cwd: &Path) -> String {
    let input = &tool.input;
    let text = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_owned);
    let path = |key: &str| text(key).map(|value| relative(&value, cwd));
    let summary = match tool.name.as_str() {
        "Bash" => text("command"),
        "Read" | "Write" | "Edit" | "MultiEdit" => path("file_path"),
        "NotebookEdit" | "NotebookRead" => path("notebook_path"),
        "Grep" => text("pattern").map(|pattern| match path("path") {
            Some(scope) => format!("{pattern} in {scope}"),
            None => pattern,
        }),
        "Glob" => text("pattern"),
        "LS" => path("path"),
        "WebFetch" => text("url"),
        "WebSearch" => text("query"),
        "Agent" | "Task" => text("description").or_else(|| text("prompt")),
        "TodoWrite" => input
            .get("todos")
            .and_then(Value::as_array)
            .map(|todos| format!("{} items", todos.len())),
        "AskUserQuestion" => input
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|q| q.first())
            .and_then(|q| q.get("question"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    };
    let summary = summary.or_else(|| {
        let partial = tool.input_partial.trim();
        (!partial.is_empty()).then(|| partial.to_owned())
    });
    single_line(summary.as_deref().unwrap_or_default(), 160)
}

/// Path relative to the working directory when it lies inside it.
fn relative(path: &str, cwd: &Path) -> String {
    Path::new(path)
        .strip_prefix(cwd)
        .ok()
        .filter(|rest| !rest.as_os_str().is_empty())
        .map_or_else(
            || path.to_owned(),
            |rest| rest.to_string_lossy().into_owned(),
        )
}

/// First line, whitespace collapsed, truncated on a char boundary with an ellipsis.
pub fn single_line(text: &str, max_chars: usize) -> String {
    let line = text.lines().next().unwrap_or_default();
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let more = collapsed.chars().count() > max_chars || text.lines().nth(1).is_some();
    let mut out: String = collapsed.chars().take(max_chars).collect();
    if more {
        out = out.trim_end().to_owned();
        out.push('…');
    }
    out
}

/// The agent is busy but nothing is being written: show the thinking indicator.
pub fn thinking(transcript: &Transcript, status: &Status) -> bool {
    if !matches!(status, Status::Working) {
        return false;
    }
    let Some(last) = transcript
        .items()
        .iter()
        .rev()
        .find(|item| item.parent.is_none() && visible(item))
    else {
        return true;
    };
    match &last.body {
        Body::Text(block) => !block.streaming || block.text.is_empty(),
        // Its own row says "Thinking…" while it streams.
        Body::Thinking(block) => !block.streaming,
        Body::Permission(permission) => permission.decision != Decision::Pending,
        _ => true,
    }
}

/// A short tab title from the first message: about six words, at most 48 characters.
pub fn title(message: &str) -> String {
    let words: Vec<&str> = message.split_whitespace().collect();
    let mut title = single_line(&words[..words.len().min(6)].join(" "), 48);
    if words.len() > 6 && !title.ends_with('…') {
        title.push('…');
    }
    if title.is_empty() {
        "New chat".into()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::transcript::{
        Body, Delivery, Permission, PermissionKind, TextBlock, ToolState, TurnEnd, UserMessage,
    };
    use serde_json::json;

    fn tool(name: &str, input: Value) -> ToolCall {
        ToolCall {
            call_id: "t".into(),
            name: name.into(),
            input,
            input_partial: String::new(),
            state: ToolState::Running,
            output: None,
            elapsed_seconds: None,
            activity: None,
            background: false,
        }
    }

    #[test]
    fn subagent_cards_describe_the_task_and_its_progress() {
        let cwd = Path::new("/repo");
        let mut agent = tool(
            "Agent",
            json!({"description": "Read note", "prompt": "Read note.txt", "subagent_type": "general-purpose"}),
        );
        assert!(is_subagent(&agent) && !is_subagent(&tool("SendMessage", json!({}))));
        assert_eq!(subagent_title(&agent), "Read note");
        assert_eq!(subagent_prompt(&agent), Some("Read note.txt"));
        assert_eq!(subagent_title(&tool("Task", json!({}))), "Subagent");

        let read = tool("Read", json!({"file_path": "/repo/note.txt"}));
        assert_eq!(subagent_preview(&agent, Some(&read), cwd), "Read note.txt");
        agent.activity = Some("Reading note.txt".into());
        assert_eq!(
            subagent_preview(&agent, Some(&read), cwd),
            "Reading note.txt"
        );

        agent.state = ToolState::Succeeded;
        agent.output = Some(crate::chat::transcript::ToolOutput {
            text: "canopy fixture note\nsecond line".into(),
            is_error: false,
            structured: None,
        });
        assert_eq!(SubagentState::of(&agent), SubagentState::Finished);
        assert_eq!(
            subagent_preview(&agent, Some(&read), cwd),
            "canopy fixture note…"
        );
        agent.state = ToolState::Cancelled;
        assert_eq!(SubagentState::of(&agent).label(), "Interrupted");
    }

    #[test]
    fn a_subagent_thread_thinks_until_it_writes_or_finishes() {
        let mut transcript = Transcript::default();
        let mut agent = tool("Agent", json!({}));
        agent.call_id = "a1".into();
        assert!(thread_thinking(&transcript, "a1", &agent));
        let text = transcript.push(
            Some("a1".into()),
            Body::Text(TextBlock {
                text: "Reading".into(),
                streaming: true,
            }),
        );
        assert!(!thread_thinking(&transcript, "a1", &agent));
        transcript.update(text, |body| {
            if let Body::Text(block) = body {
                block.streaming = false;
            }
        });
        assert!(thread_thinking(&transcript, "a1", &agent));
        agent.state = ToolState::Succeeded;
        assert!(!thread_thinking(&transcript, "a1", &agent));
    }

    #[test]
    fn tool_summaries_name_the_command_path_or_pattern() {
        let cwd = Path::new("/repo");
        let cases = [
            (
                tool("Bash", json!({"command": "cargo test\n--all"})),
                "cargo test…",
            ),
            (
                tool("Read", json!({"file_path": "/repo/src/main.rs"})),
                "src/main.rs",
            ),
            (
                tool("Edit", json!({"file_path": "/elsewhere/x.rs"})),
                "/elsewhere/x.rs",
            ),
            (
                tool("Grep", json!({"pattern": "TODO", "path": "/repo/src"})),
                "TODO in src",
            ),
            (
                tool("WebSearch", json!({"query": "gpui list"})),
                "gpui list",
            ),
            (
                tool("Agent", json!({"description": "Read note"})),
                "Read note",
            ),
            (tool("TodoWrite", json!({"todos": [1, 2, 3]})), "3 items"),
            (tool("mcp__x__y", json!({})), ""),
        ];
        for (call, expected) in cases {
            assert_eq!(tool_summary(&call, cwd), expected, "{}", call.name);
        }
        let mut streaming = tool("Bash", json!({}));
        streaming.input_partial = r#"{"command": "ls"#.into();
        assert_eq!(tool_summary(&streaming, cwd), r#"{"command": "ls"#);
        assert_eq!(tool_kind("MultiEdit"), ToolKind::Edit);
    }

    #[test]
    fn completed_turns_have_no_rows_but_thinking_does() {
        let mut transcript = Transcript::default();
        transcript.push(
            None,
            Body::Thinking(TextBlock {
                text: "x".into(),
                streaming: false,
            }),
        );
        for outcome in [Outcome::Completed, Outcome::Interrupted] {
            transcript.push(
                None,
                Body::TurnEnd(TurnEnd {
                    outcome,
                    usage: None,
                    cost_usd: None,
                    duration_ms: None,
                    resume_at: None,
                }),
            );
        }
        let shown: Vec<bool> = transcript.items().iter().map(visible).collect();
        assert_eq!(shown, [true, false, true]);
    }

    #[test]
    fn thinking_shows_only_while_working_and_not_writing() {
        let mut transcript = Transcript::default();
        assert!(thinking(&transcript, &Status::Working));
        assert!(!thinking(&transcript, &Status::Ready));
        transcript.push(
            None,
            Body::User(UserMessage {
                text: "hi".into(),
                delivery: Delivery::Sent,
                uuid: None,
                attachments: vec![],
            }),
        );
        let text = transcript.push(
            None,
            Body::Text(TextBlock {
                text: String::new(),
                streaming: true,
            }),
        );
        assert!(
            thinking(&transcript, &Status::Working),
            "nothing written yet"
        );
        transcript.update(text, |body| {
            if let Body::Text(block) = body {
                block.text.push_str("Hel");
            }
        });
        assert!(!thinking(&transcript, &Status::Working));
        transcript.push(
            None,
            Body::Permission(Permission {
                request_id: "r".into(),
                call_id: None,
                tool: "Bash".into(),
                input: json!({}),
                title: None,
                description: None,
                kind: PermissionKind::Tool,
                decision: Decision::Pending,
                remember: None,
                resolution: None,
            }),
        );
        assert!(
            !thinking(&transcript, &Status::Working),
            "waiting on the user"
        );
    }

    #[test]
    fn placeholder_results_have_no_output() {
        let mut call = tool("Bash", json!({"command": "rm x"}));
        assert!(!has_output(&call));
        for (text, expected) in [
            ("(empty)", false),
            ("  (No content) ", false),
            ("", false),
            ("ok", true),
        ] {
            call.output = Some(crate::chat::transcript::ToolOutput {
                text: text.into(),
                is_error: false,
                structured: None,
            });
            assert_eq!(has_output(&call), expected, "{text}");
        }
    }

    #[test]
    fn titles() {
        assert_eq!(
            title("  Fix   the failing\nlogin test in auth module please "),
            "Fix the failing login test in…"
        );
        assert_eq!(title("   "), "New chat");
    }
}
