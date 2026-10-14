//! Codex `app-server` sessions replayed from a recording (codex-cli 0.161.0): approvals
//! in Ask mode, a patch and a shell write, an image, a subagent and plain replies.
use canopy_desktop::chat::{
    codex::wire::{self, Message},
    config::{ChatSettings, Mode},
    presentation,
    session::{ChatSession, CodexStart, Status},
    tools::{self, ToolView},
    transcript::{Body, Decision, Outcome, PermissionKind, ToolState},
};
use serde_json::{Value, json};

const RECORDING: &str = include_str!("fixtures/chat/codex-0.161/session.jsonl");
/// A turn that leaves `sleep 30; echo done` running (a unified exec session) and ends;
/// the command's output and end come after it.
const BACKGROUND: &str = include_str!("fixtures/chat/codex-0.161/background.jsonl");

fn message(value: Value) -> Message {
    wire::parse_line(value.to_string().as_bytes())
        .unwrap()
        .unwrap()
}

/// A session with its thread open (`thread` from the recording).
fn opened(mode: Mode) -> (ChatSession, Vec<Value>) {
    opened_with(mode, RECORDING)
}

fn opened_with(mode: Mode, recording: &str) -> (ChatSession, Vec<Value>) {
    let settings = ChatSettings {
        mode,
        ..Default::default()
    };
    let (mut session, lines) = ChatSession::codex(
        settings,
        CodexStart {
            cwd: "/tmp/canopy-codex".into(),
            images: Some("/data/chat".into()),
            ..Default::default()
        },
    );
    assert_eq!(lines[0]["method"], "initialize");
    let id = lines[0]["id"].as_str().unwrap().to_owned();
    let lines = session.on_codex(message(json!({"id": id, "result": {"userAgent": "x"}})));
    assert_eq!(lines[0]["method"], "initialized");
    assert_eq!(lines[1]["method"], "thread/start");
    assert_eq!(lines[1]["params"]["approvalPolicy"], mode.codex().0);
    let thread = lines[1]["id"].as_str().unwrap().to_owned();
    let thread_id = recording
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|v| v["params"]["thread"]["id"].as_str().map(str::to_owned))
        .next()
        .unwrap();
    session.on_codex(message(
        json!({"id": thread, "result": {"thread": {"id": thread_id}, "model": "gpt-6.1-sol"}}),
    ));
    assert_eq!(session.session_id(), Some(thread_id.as_str()));
    assert_eq!(*session.status(), Status::Ready);
    (session, lines)
}

#[test]
fn a_recorded_session_builds_the_conversation() {
    let (mut session, _) = opened(Mode::Ask);
    let lines = session.submit("Create a.txt and b.txt");
    assert_eq!(lines[0]["method"], "turn/start");
    assert_eq!(lines[0]["params"]["approvalPolicy"], "untrusted");
    assert_eq!(*session.status(), Status::Working);
    let mut replies = vec![];
    for line in RECORDING.lines() {
        let Some(message) = wire::parse_line(line.as_bytes()).unwrap() else {
            continue;
        };
        // Responses answered the recorder's own requests.
        if matches!(message, Message::Response { .. } | Message::Error { .. }) {
            continue;
        }
        replies.extend(session.on_codex(message));
        if *session.status() == Status::NeedsDecision {
            let item = session.pending_decisions()[0];
            replies.extend(session.decide(item, true));
        }
    }
    // Each approval was answered with the request's own id.
    let answers: Vec<&Value> = replies
        .iter()
        .filter(|line| line.get("result").is_some())
        .collect();
    assert_eq!(answers.len(), 2);
    assert!(answers.iter().all(|a| a["result"]["decision"] == "accept"));

    let items = session.transcript().items();
    let tool = |name: &'static str| {
        items.iter().filter_map(move |item| match &item.body {
            Body::Tool(tool) if tool.name == name => Some((item, tool)),
            _ => None,
        })
    };
    let commands: Vec<&str> = tool("Bash")
        .filter(|(item, _)| item.parent.is_none())
        .map(|(_, t)| t.input["command"].as_str().unwrap())
        .collect();
    assert_eq!(commands, ["echo b > b.txt"]);
    let (_, write) = tool("Write").next().expect("patch");
    assert_eq!(write.state, ToolState::Succeeded);
    let ToolView::Change(change) = tools::view(write) else {
        panic!("change view")
    };
    assert!(change.created && change.path.ends_with("a.txt"));
    let permissions: Vec<_> = items
        .iter()
        .filter_map(|item| match &item.body {
            Body::Permission(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(permissions.len(), 2);
    assert!(permissions.iter().all(|p| p.decision == Decision::Allowed));
    assert!(matches!(
        tools::proposal(&permissions[0].tool, &permissions[0].input),
        tools::Proposal::Change(_)
    ));
    // The subagent: a card, its command and reply under it.
    let (_, agent) = tool("Agent").next().expect("subagent card");
    assert!(presentation::is_subagent(agent));
    assert_eq!(agent.state, ToolState::Succeeded);
    let children = items
        .iter()
        .filter(|item| item.parent.as_deref() == Some(agent.call_id.as_str()))
        .count();
    assert!(children >= 2, "the subagent's work is under its card");
    let texts: Vec<&str> = items
        .iter()
        .filter(|item| item.parent.is_none())
        .filter_map(|item| match &item.body {
            Body::Text(block) => Some(block.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"Green"));
    let ends: Vec<_> = items
        .iter()
        .filter_map(|item| match &item.body {
            Body::TurnEnd(end) => Some(end),
            _ => None,
        })
        .collect();
    assert!(ends.len() >= 3);
    assert!(
        ends.iter()
            .all(|end| end.outcome == Outcome::Completed && end.resume_at.is_some())
    );
    assert!(ends[0].usage.is_some());
    assert!(session.context().is_some_and(|c| c.max > 0));
    assert!(session.rate_limit().is_some());
    assert_eq!(*session.status(), Status::Ready);
}

#[test]
fn a_command_running_past_its_turn_ends_in_its_own_card() {
    let (mut session, _) = opened_with(Mode::Auto, BACKGROUND);
    session.submit("Start sleep 30 in the background");
    let commands = |session: &ChatSession| -> Vec<(ToolState, bool, Option<String>)> {
        session
            .transcript()
            .items()
            .iter()
            .filter_map(|item| match &item.body {
                Body::Tool(tool) if tool.name == "Bash" => Some((
                    tool.state,
                    tool.background,
                    tool.output.as_ref().map(|o| o.text.clone()),
                )),
                _ => None,
            })
            .collect()
    };
    let mut turn_ended = false;
    for line in BACKGROUND.lines() {
        session.on_codex(wire::parse_line(line.as_bytes()).unwrap().unwrap());
        if line.contains("\"turn/completed\"") {
            turn_ended = true;
            assert_eq!(*session.status(), Status::Ready);
            assert_eq!(commands(&session), [(ToolState::Running, true, None)]);
        }
    }
    assert!(turn_ended);
    assert_eq!(
        commands(&session),
        [(ToolState::Succeeded, true, Some("done\n".to_owned()))]
    );
}

#[test]
fn deny_with_a_note_declines_and_tells_the_agent() {
    let (mut session, _) = opened(Mode::Ask);
    session.submit("go");
    session.on_codex(message(json!({"method":"turn/started","params":{"threadId": session.session_id(), "turn":{"id":"turn-1"}}})));
    session.on_codex(message(
        json!({"id": 7, "method": "item/commandExecution/requestApproval", "params": {
        "threadId": session.session_id(), "turnId": "turn-1", "itemId": "exec-1",
        "command": "/bin/zsh -lc 'rm -rf build'", "cwd": "/tmp/canopy-codex", "reason": "clean"}}),
    ));
    assert_eq!(*session.status(), Status::NeedsDecision);
    let item = session.pending_decisions()[0];
    let lines = session.respond(
        item,
        canopy_desktop::chat::session::Response::Deny(Some("keep build".into())),
    );
    assert_eq!(
        lines[0],
        json!({"id": 7, "result": {"decision": "decline"}})
    );
    assert_eq!(lines[1]["method"], "turn/steer");
    assert_eq!(lines[1]["params"]["expectedTurnId"], "turn-1");
    assert!(
        lines[1]["params"]["input"][0]["text"]
            .as_str()
            .unwrap()
            .contains("keep build")
    );
}

#[test]
fn questions_answer_by_id_and_stop_interrupts_the_turn() {
    let (mut session, _) = opened(Mode::Auto);
    session.submit("go");
    let thread = session.session_id().unwrap().to_owned();
    session.on_codex(message(
        json!({"method":"turn/started","params":{"threadId": thread, "turn":{"id":"turn-1"}}}),
    ));
    session.on_codex(message(json!({"id": "q1", "method": "item/tool/requestUserInput", "params": {
        "threadId": thread, "turnId": "turn-1", "itemId": "tool-1",
        "questions": [{"id": "style", "header": "Style", "question": "Tabs or spaces?", "isOther": false, "isSecret": false,
            "options": [{"label": "Tabs", "description": ""}, {"label": "Spaces", "description": ""}]}]}})));
    let item = session.pending_decisions()[0];
    let permission = session
        .transcript()
        .items()
        .iter()
        .filter_map(|i| match &i.body {
            Body::Permission(p) => Some(p),
            _ => None,
        })
        .next()
        .unwrap();
    assert_eq!(permission.kind, PermissionKind::Question);
    assert_eq!(
        presentation::questions(permission)[0].question,
        "Tabs or spaces?"
    );
    let lines = session.respond(
        item,
        canopy_desktop::chat::session::Response::Answer(vec!["Spaces".into()]),
    );
    assert_eq!(
        lines[0],
        json!({"id": "q1", "result": {"answers": {"style": {"answers": ["Spaces"]}}}})
    );
    let lines = session.interrupt();
    assert_eq!(lines[0]["method"], "turn/interrupt");
    assert_eq!(
        lines[0]["params"],
        json!({"threadId": thread, "turnId": "turn-1"})
    );
    session.on_codex(message(json!({"method":"turn/completed","params":{"threadId": thread, "turn":{"id":"turn-1","status":"interrupted"}}})));
    assert!(
        session
            .transcript()
            .items()
            .iter()
            .any(|i| matches!(&i.body,
        Body::TurnEnd(end) if end.outcome == Outcome::Interrupted))
    );
    assert_eq!(*session.status(), Status::Ready);
}

#[test]
fn a_resume_and_a_fork_reopen_the_saved_thread() {
    // A saved thread's active goal is paused before it opens: Codex would otherwise
    // start working on it by itself (live-verified with 0.161).
    let active = json!({"goal": {"threadId": "thread-9", "objective": "tests pass",
        "status": "active", "tokensUsed": 10, "timeUsedSeconds": 3,
        "createdAt": 1, "updatedAt": 2}});
    for (fork, method, goal) in [
        (None, "thread/resume", active),
        (Some("turn-1"), "thread/fork", json!({"goal": null})),
    ] {
        let (mut session, lines) = ChatSession::codex(
            ChatSettings::default(),
            CodexStart {
                cwd: "/tmp/x".into(),
                resume: Some("thread-9".into()),
                fork_at: fork.map(str::to_owned),
                ..Default::default()
            },
        );
        let id = lines[0]["id"].as_str().unwrap().to_owned();
        let lines = session.on_codex(message(json!({"id": id, "result": {}})));
        assert_eq!(lines[1]["method"], "thread/goal/get");
        assert_eq!(lines[1]["params"]["threadId"], "thread-9");
        let asked = lines[1]["id"].as_str().unwrap().to_owned();
        let mut lines = session.on_codex(message(json!({"id": asked, "result": goal})));
        if lines[0]["method"] == "thread/goal/set" {
            assert_eq!(lines[0]["params"]["status"], "paused");
            let paused = lines[0]["id"].as_str().unwrap().to_owned();
            lines = session.on_codex(message(json!({"id": paused, "result": {}})));
        }
        assert_eq!(lines[0]["method"], method);
        assert_eq!(lines[0]["params"]["threadId"], "thread-9");
        // A message typed while the thread opens waits for it.
        assert!(session.submit("hello").is_empty());
        let thread = lines[0]["id"].as_str().unwrap().to_owned();
        let sent = session.on_codex(message(
            json!({"id": thread, "result": {"thread": {"id": "thread-10"}}}),
        ));
        assert_eq!(sent.last().unwrap()["method"], "turn/start");
        assert_eq!(sent.last().unwrap()["params"]["threadId"], "thread-10");
    }
}

#[test]
fn canopy_page_tools_are_approved_and_other_servers_are_not() {
    let (mut session, _) = opened(Mode::Auto);
    session.submit("Show me a page");
    let ask = |server: &str| {
        json!({"id": 41, "method": "mcpServer/elicitation/request", "params": {
            "threadId": "t", "turnId": "u", "serverName": server, "mode": "form",
            "_meta": {"codex_approval_kind": "mcp_tool_call"}
        }})
    };
    let lines = session.on_codex(message(ask("canopy")));
    assert_eq!(lines[0]["id"], 41);
    assert_eq!(lines[0]["result"]["action"], "accept");
    let before = session.transcript().items().len();
    let lines = session.on_codex(message(ask("someone-else")));
    assert_eq!(lines[0]["result"]["action"], "cancel");
    assert!(
        session.transcript().items().len() > before,
        "a notice says it went unanswered"
    );
}
