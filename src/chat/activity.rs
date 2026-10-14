//! What the agent keeps track of beside the conversation (DEC-083…086): its task list
//! and the work it left running in the background. Read from the main thread of a
//! transcript, so it follows the conversation and comes back with saved history.
use super::presentation::{self, ToolKind};
use super::transcript::{Body, ToolCall, ToolState, Transcript};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}
impl TodoStatus {
    fn parse(value: Option<&str>) -> Option<Self> {
        match value? {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    /// The agent's id for it (`TaskCreate`); lists written whole have none.
    pub id: Option<String>,
    pub text: String,
    /// How the agent words it while working on it ("Running the tests").
    pub active: Option<String>,
    pub status: TodoStatus,
}
impl Todo {
    /// What the agent is doing, for an item in progress.
    pub fn doing(&self) -> &str {
        self.active.as_deref().unwrap_or(&self.text)
    }
}

/// The agent's task list as it stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Todos {
    pub items: Vec<Todo>,
    /// Position of the last item that changed the list: a newer list is a new one.
    pub version: usize,
}
impl Todos {
    pub fn done(&self) -> usize {
        self.items
            .iter()
            .filter(|todo| todo.status == TodoStatus::Completed)
            .count()
    }
    pub fn finished(&self) -> bool {
        self.done() == self.items.len()
    }
    pub fn current(&self) -> Option<&Todo> {
        self.items
            .iter()
            .find(|todo| todo.status == TodoStatus::InProgress)
    }
}

/// The agent's task list, from `TodoWrite` (Claude, and Codex's plan) or
/// `TaskCreate`/`TaskUpdate` (Claude); `None` when it never kept one.
pub fn todos(transcript: &Transcript) -> Option<Todos> {
    let mut todos: Option<Todos> = None;
    for (position, item) in transcript.items().iter().enumerate() {
        let Body::Tool(tool) = &item.body else {
            continue;
        };
        if item.parent.is_some() || matches!(tool.state, ToolState::Failed | ToolState::Streaming) {
            continue;
        }
        let list = todos.get_or_insert_with(Todos::default);
        let changed = match tool.name.as_str() {
            "TodoWrite" => {
                list.items = written(&tool.input);
                true
            }
            "TaskCreate" => created(tool).is_some_and(|todo| {
                list.items.push(todo);
                true
            }),
            "TaskUpdate" => updated(tool, &mut list.items),
            _ => false,
        };
        if changed {
            list.version = position + 1;
        }
    }
    todos.filter(|list| !list.items.is_empty())
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn written(input: &Value) -> Vec<Todo> {
    input
        .get("todos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|todo| {
            Some(Todo {
                id: None,
                text: text(todo, "content")?,
                active: text(todo, "activeForm"),
                status: TodoStatus::parse(todo.get("status").and_then(Value::as_str))
                    .unwrap_or(TodoStatus::Pending),
            })
        })
        .collect()
}

/// A created task, once the CLI gave it its id.
fn created(tool: &ToolCall) -> Option<Todo> {
    let output = tool.output.as_ref()?;
    let id = output
        .structured
        .as_ref()
        .and_then(|result| result.get("task"))
        .and_then(|task| match task.get("id")? {
            Value::String(id) => Some(id.clone()),
            Value::Number(id) => Some(id.to_string()),
            _ => None,
        })?;
    Some(Todo {
        id: Some(id),
        text: text(&tool.input, "subject")?,
        active: text(&tool.input, "activeForm"),
        status: TodoStatus::Pending,
    })
}

fn updated(tool: &ToolCall, items: &mut Vec<Todo>) -> bool {
    if tool.state != ToolState::Succeeded {
        return false;
    }
    let Some(id) = (match tool.input.get("taskId") {
        Some(Value::String(id)) => Some(id.clone()),
        Some(Value::Number(id)) => Some(id.to_string()),
        _ => None,
    }) else {
        return false;
    };
    let Some(index) = items
        .iter()
        .position(|todo| todo.id.as_deref() == Some(id.as_str()))
    else {
        return false;
    };
    if tool.input.get("status").and_then(Value::as_str) == Some("deleted") {
        items.remove(index);
        return true;
    }
    let todo = &mut items[index];
    if let Some(status) = TodoStatus::parse(tool.input.get("status").and_then(Value::as_str)) {
        todo.status = status;
    }
    if let Some(subject) = text(&tool.input, "subject") {
        todo.text = subject;
    }
    if let Some(active) = text(&tool.input, "activeForm") {
        todo.active = Some(active);
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Shell,
    Agent,
    Other,
}

/// A tool call the agent left running in the background.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub call: String,
    /// Its position in the transcript.
    pub index: usize,
    /// The command, or the subagent's task.
    pub label: String,
    pub kind: JobKind,
    pub state: ToolState,
    /// How long it ran, when the agent reported it.
    pub elapsed_seconds: Option<f64>,
}

/// The main thread's background calls, oldest first.
pub fn jobs(transcript: &Transcript, cwd: &Path) -> Vec<Job> {
    transcript
        .items()
        .iter()
        .enumerate()
        .filter(|(_, item)| item.parent.is_none())
        .filter_map(|(index, item)| match &item.body {
            Body::Tool(tool) if tool.background => Some(Job {
                call: tool.call_id.clone(),
                index,
                label: if presentation::is_subagent(tool) {
                    presentation::subagent_title(tool)
                } else {
                    presentation::tool_summary(tool, cwd)
                },
                kind: match presentation::tool_kind(&tool.name) {
                    ToolKind::Shell => JobKind::Shell,
                    _ if presentation::is_subagent(tool) => JobKind::Agent,
                    _ => JobKind::Other,
                },
                state: tool.state,
                elapsed_seconds: tool.elapsed_seconds,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::transcript::ToolOutput;
    use serde_json::json;

    fn tool(t: &mut Transcript, name: &str, input: Value, structured: Option<Value>) {
        t.push(
            None,
            Body::Tool(ToolCall {
                call_id: format!("call-{}", t.items().len()),
                name: name.into(),
                input,
                input_partial: String::new(),
                state: ToolState::Succeeded,
                output: Some(ToolOutput {
                    text: String::new(),
                    is_error: false,
                    structured,
                }),
                elapsed_seconds: None,
                activity: None,
                background: false,
            }),
        );
    }

    #[test]
    fn created_tasks_follow_their_updates() {
        let mut t = Transcript::default();
        for (id, subject) in [("1", "A"), ("2", "B"), ("3", "C")] {
            tool(
                &mut t,
                "TaskCreate",
                json!({"subject": subject, "description": "x"}),
                Some(json!({"task": {"id": id, "subject": subject}})),
            );
        }
        tool(
            &mut t,
            "TaskUpdate",
            json!({"taskId": "1", "status": "completed"}),
            None,
        );
        tool(
            &mut t,
            "TaskUpdate",
            json!({"taskId": "2", "status": "in_progress", "activeForm": "Doing B"}),
            None,
        );
        tool(
            &mut t,
            "TaskUpdate",
            json!({"taskId": "3", "status": "deleted"}),
            None,
        );
        let todos = todos(&t).unwrap();
        assert_eq!(todos.items.len(), 2);
        assert_eq!(todos.done(), 1);
        assert_eq!(todos.current().map(Todo::doing), Some("Doing B"));
        assert_eq!(todos.version, 6);
        assert!(!todos.finished());
    }

    #[test]
    fn a_written_list_replaces_the_previous_one() {
        let mut t = Transcript::default();
        tool(
            &mut t,
            "TodoWrite",
            json!({"todos": [{"content": "Old", "status": "pending"}]}),
            None,
        );
        tool(
            &mut t,
            "TodoWrite",
            json!({"todos": [
                {"content": "Read", "status": "completed"},
                {"content": "Test", "status": "completed"},
            ]}),
            None,
        );
        let todos = todos(&t).unwrap();
        assert_eq!(
            todos
                .items
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>(),
            ["Read", "Test"]
        );
        assert!(todos.finished());
        assert_eq!(todos.version, 2);
        assert_eq!(super::todos(&Transcript::default()), None);
    }
}
