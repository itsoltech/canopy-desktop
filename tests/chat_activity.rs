//! The panel over the composer (stage 12) replayed from Claude Code 2.1.296 recordings:
//! a shell left running in the background, stopped by its task id, and a task list
//! kept with `TaskCreate`/`TaskUpdate`.
use canopy_desktop::chat::{
    activity::{self, JobKind, TodoStatus},
    claude::wire,
    session::ChatSession,
    transcript::ToolState,
};
use serde_json::json;
use std::path::Path;

const BACKGROUND: &str = include_str!("fixtures/chat/claude-2.1.296/background.jsonl");
const TASKS: &str = include_str!("fixtures/chat/claude-2.1.296/tasks.jsonl");

fn started() -> ChatSession {
    let (mut session, _) = ChatSession::new();
    let ack = json!({"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}});
    session.on_message(
        wire::parse_line(ack.to_string().as_bytes())
            .unwrap()
            .unwrap(),
    );
    session
}

fn feed(session: &mut ChatSession, line: &str) {
    session.on_message(wire::parse_line(line.as_bytes()).unwrap().unwrap());
}

#[test]
fn a_background_shell_runs_past_its_turn_and_stops_by_its_task() {
    let mut session = started();
    let mut checked = false;
    for line in BACKGROUND.lines() {
        feed(&mut session, line);
        if line.contains("\"task_started\"") {
            let jobs = activity::jobs(session.transcript(), Path::new("/tmp"));
            assert_eq!(jobs.len(), 1);
            let job = &jobs[0];
            assert_eq!(
                (job.kind, job.state, job.label.as_str()),
                (JobKind::Shell, ToolState::Running, "sleep 20; echo done")
            );
            assert!(session.can_stop_background(&job.call));
            let stop = session.stop_background(&job.call);
            assert_eq!(
                stop[0]["request"],
                json!({"subtype": "stop_task", "task_id": "bcd3ditu9"})
            );
            checked = true;
        }
    }
    assert!(checked);
    let jobs = activity::jobs(session.transcript(), Path::new("/tmp"));
    assert_eq!(jobs[0].state, ToolState::Succeeded);
    assert!(!session.can_stop_background(&jobs[0].call));
    assert!(activity::todos(session.transcript()).is_none());
}

#[test]
fn created_tasks_and_their_updates_make_the_list() {
    let mut session = started();
    for line in TASKS.lines() {
        feed(&mut session, line);
    }
    let todos = activity::todos(session.transcript()).expect("a task list");
    let items: Vec<(&str, TodoStatus)> = todos
        .items
        .iter()
        .map(|todo| (todo.text.as_str(), todo.status))
        .collect();
    assert_eq!(
        items,
        [
            ("A", TodoStatus::Completed),
            ("B", TodoStatus::InProgress),
            ("C", TodoStatus::Pending),
        ]
    );
    assert_eq!(todos.current().map(|todo| todo.text.as_str()), Some("B"));
    assert!(activity::jobs(session.transcript(), Path::new("/tmp")).is_empty());
}
