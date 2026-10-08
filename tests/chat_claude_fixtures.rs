//! Replays stdout recorded from Claude Code 2.1.294 (tests/chat_claude_live.rs with
//! CANOPY_RECORD_FIXTURES) through the session controller.
use canopy_desktop::chat::{
    claude::wire::{self, Inbound},
    process::{Event, Exit},
    session::{ChatSession, Status},
    transcript::{
        Body, Decision, Delivery, NoticeKind, Outcome, PermissionKind, ToolCall, ToolState, TurnId,
    },
};
use serde_json::Value;

const DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/chat/claude-2.1.294"
);
const ALL: [&str; 15] = [
    "settings",
    "simple",
    "bash-allow",
    "bash-deny",
    "write-allow",
    "interrupt",
    "queued",
    "steer",
    "subagent",
    "parallel-subagents",
    "tools",
    "thinking",
    "hooks",
    "resume-first",
    "resume-second",
];

fn lines(name: &str) -> Vec<String> {
    std::fs::read_to_string(format!("{DIR}/{name}.jsonl"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn message(line: &str) -> Inbound {
    wire::parse_line(line.as_bytes()).unwrap().unwrap()
}

/// Feeds recorded lines; `act` runs before each line and may answer prompts.
/// Recorded message uuids are mapped, in order, to the uuids this session wrote.
fn replay(
    name: &str,
    prompt: &str,
    mut act: impl FnMut(&mut ChatSession) -> Vec<Value>,
) -> (ChatSession, Vec<Value>) {
    let (mut session, mut written) = ChatSession::new();
    written.extend(session.submit(prompt));
    let mut uuids = std::collections::HashMap::<String, String>::new();
    for line in lines(name) {
        written.extend(act(&mut session));
        let mut value: Value = serde_json::from_str(&line).unwrap();
        if value["type"] == "command_lifecycle" {
            let recorded = value["command_uuid"].as_str().unwrap().to_owned();
            let ours: Vec<String> = written
                .iter()
                .filter(|line| line["type"] == "user")
                .map(|line| line["uuid"].as_str().unwrap().to_owned())
                .collect();
            let next = ours.get(uuids.len()).cloned();
            let mapped = uuids
                .entry(recorded.clone())
                .or_insert_with(|| next.unwrap_or(recorded));
            value["command_uuid"] = Value::String(mapped.clone());
        }
        written.extend(session.on_message(message(&value.to_string())));
    }
    written.extend(act(&mut session));
    (session, written)
}

fn tool<'a>(session: &'a ChatSession, name: &str) -> &'a ToolCall {
    session
        .transcript()
        .items()
        .iter()
        .find_map(|item| match &item.body {
            Body::Tool(call) if call.name == name && item.parent.is_none() => Some(call),
            _ => None,
        })
        .unwrap()
}

fn outcomes(session: &ChatSession) -> Vec<Outcome> {
    session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::TurnEnd(end) => Some(end.outcome.clone()),
            _ => None,
        })
        .collect()
}

fn main_text(session: &ChatSession, turn: TurnId) -> String {
    session
        .transcript()
        .items()
        .iter()
        .filter(|item| item.turn == turn && item.parent.is_none())
        .filter_map(|item| match &item.body {
            Body::Text(block) => Some(block.text.as_str()),
            _ => None,
        })
        .collect()
}

fn decide_pending(session: &mut ChatSession, allow: bool) -> Vec<Value> {
    let pending = session
        .transcript()
        .items()
        .iter()
        .find_map(|item| match &item.body {
            Body::Permission(p) if p.decision == Decision::Pending => Some(item.id),
            _ => None,
        });
    match pending {
        Some(id) if *session.status() == Status::NeedsDecision => session.decide(id, allow),
        _ => vec![],
    }
}

#[test]
fn every_recorded_line_parses_without_diagnostics() {
    for name in ALL {
        for line in lines(name) {
            assert!(
                wire::parse_line(line.as_bytes()).is_ok(),
                "{name}: unreadable line {line}"
            );
        }
    }
}

#[test]
fn streamed_text_matches_each_turn_result_without_duplication() {
    for name in [
        "simple",
        "bash-allow",
        "bash-deny",
        "write-allow",
        "queued",
        "hooks",
    ] {
        let (session, _) = replay(name, "prompt", |s| decide_pending(s, name != "bash-deny"));
        let results: Vec<String> = lines(name)
            .iter()
            .filter_map(|line| {
                let value: Value = serde_json::from_str(line).ok()?;
                (value["type"] == "result")
                    .then(|| value["result"].as_str().unwrap_or("").to_owned())
            })
            .collect();
        let turns: Vec<TurnId> = session
            .transcript()
            .items()
            .iter()
            .filter(|item| matches!(item.body, Body::TurnEnd(_)))
            .map(|item| item.turn)
            .collect();
        assert_eq!(turns.len(), results.len(), "{name}: one TurnEnd per result");
        for (turn, result) in turns.into_iter().zip(results) {
            assert_eq!(main_text(&session, turn), result, "{name}: turn text");
        }
        assert!(
            session
                .transcript()
                .items()
                .iter()
                .all(|item| match &item.body {
                    Body::Text(block) | Body::Thinking(block) => !block.streaming,
                    _ => true,
                })
        );
        assert_eq!(session.diagnostics().count(), 0);
    }
}

#[test]
fn simple_turn_reports_session_metadata_usage_and_status() {
    let (session, written) = replay("simple", "Reply with exactly: ok", |_| vec![]);
    assert_eq!(written[0]["request"]["subtype"], "initialize");
    assert_eq!(
        written[1]["type"], "user",
        "message waits for initialize ack"
    );
    assert_eq!(*session.status(), Status::Ready);
    let info = session.info();
    assert_eq!(info.version.as_deref(), Some("2.1.294"));
    assert!(
        info.capabilities
            .iter()
            .any(|c| c == "interrupt_receipt_v1")
    );
    assert!(
        session
            .session_id()
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
    );
    let items = session.transcript().items();
    assert!(matches!(&items[0].body, Body::User(m) if m.delivery == Delivery::Sent));
    let Body::TurnEnd(end) = &items.last().unwrap().body else {
        panic!("turn end last");
    };
    assert_eq!(end.outcome, Outcome::Completed);
    assert!(end.usage.unwrap().output_tokens > 0);
    assert!(end.cost_usd.unwrap() > 0.0);
}

#[test]
fn allowed_bash_runs_after_one_allow_reply() {
    let mut replies = vec![];
    let (session, _) = replay("bash-allow", "prompt", |s| {
        let lines = decide_pending(s, true);
        replies.extend(lines.clone());
        lines
    });
    assert_eq!(replies.len(), 1);
    let response = &replies[0]["response"]["response"];
    assert_eq!(response["behavior"], "allow");
    assert_eq!(
        response["updatedInput"]["command"],
        "touch canopy-allow.txt"
    );
    assert!(
        response["toolUseID"]
            .as_str()
            .unwrap()
            .starts_with("toolu_")
    );
    let bash = tool(&session, "Bash");
    assert_eq!(bash.state, ToolState::Succeeded);
    assert_eq!(bash.input["command"], "touch canopy-allow.txt");
    assert!(session.transcript().items().iter().any(|item| matches!(
        &item.body,
        Body::Permission(p) if p.kind == PermissionKind::Tool && p.decision == Decision::Allowed
    )));
}

#[test]
fn denied_bash_fails_with_the_canopy_message() {
    let (session, written) = replay("bash-deny", "prompt", |s| decide_pending(s, false));
    assert!(
        written
            .iter()
            .any(|line| line["response"]["response"]["behavior"] == "deny")
    );
    let bash = tool(&session, "Bash");
    assert_eq!(bash.state, ToolState::Failed);
    assert!(
        bash.output
            .as_ref()
            .unwrap()
            .text
            .contains("Denied by the user in Canopy")
    );
    assert_eq!(outcomes(&session), [Outcome::Completed]);
}

#[test]
fn write_result_keeps_the_structured_payload() {
    let (session, _) = replay("write-allow", "prompt", |s| decide_pending(s, true));
    let write = tool(&session, "Write");
    assert_eq!(write.state, ToolState::Succeeded);
    assert!(write.output.as_ref().unwrap().structured.is_some());
    assert!(
        write.input["file_path"]
            .as_str()
            .unwrap()
            .ends_with("created.txt")
    );
}

#[test]
fn interrupt_keeps_partial_text_and_ends_the_turn_interrupted() {
    let mut sent = false;
    let (mut session, written) = replay("interrupt", "Count", |s| {
        let streaming =
            s.transcript().items().iter().any(
                |item| matches!(&item.body, Body::Text(b) if b.streaming && b.text.len() > 20),
            );
        if streaming && !sent {
            sent = true;
            s.interrupt()
        } else {
            vec![]
        }
    });
    assert!(
        written
            .iter()
            .any(|line| line["request"]["subtype"] == "interrupt")
    );
    assert_eq!(outcomes(&session), [Outcome::Interrupted]);
    assert!(!main_text(&session, TurnId(1)).is_empty());
    session.close_requested();
    session.on_event(Event::Exited(Exit {
        code: Some(1),
        signal: None,
        stderr: String::new(),
    }));
    assert!(matches!(
        session.status(),
        Status::Exited { code: Some(1), .. }
    ));
}

#[test]
fn queued_message_becomes_its_own_turn() {
    let mut second = false;
    let (session, written) = replay("queued", "Reply with exactly: first", |s| {
        if !second && *s.status() == Status::Working {
            second = true;
            let lines = s.submit("Reply with exactly: second");
            assert!(lines.is_empty(), "held in Canopy while the agent works");
            assert_eq!(s.queue().len(), 1);
            return lines;
        }
        vec![]
    });
    let priorities: Vec<_> = written
        .iter()
        .filter(|line| line["type"] == "user")
        .map(|line| line["priority"].clone())
        .collect();
    assert_eq!(priorities, ["later", "later"]);
    assert!(session.queue().is_empty());
    let users: Vec<_> = session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::User(m) => Some((item.turn, m.delivery)),
            _ => None,
        })
        .collect();
    assert_eq!(
        users,
        [(TurnId(1), Delivery::Sent), (TurnId(2), Delivery::Sent)]
    );
    assert_eq!(main_text(&session, TurnId(1)).to_lowercase(), "first");
    assert_eq!(main_text(&session, TurnId(2)).to_lowercase(), "second");
}

#[test]
fn steer_is_folded_into_the_running_turn_after_the_tool() {
    let mut steered = false;
    let (session, written) = replay(
        "steer",
        "Use the Bash tool to run exactly `sleep 4; echo first`.",
        |s| {
            let running = s.transcript().items().iter().any(
                |item| matches!(&item.body, Body::Tool(t) if t.name == "Bash" && t.state == ToolState::Running),
            );
            if !steered && running {
                steered = true;
                let lines = s.steer_text("Also include the word STEER in your reply.");
                assert!(s.queue()[0].steering.is_some(), "Sending… until read");
                return lines;
            }
            vec![]
        },
    );
    assert!(steered);
    let priorities: Vec<_> = written
        .iter()
        .filter(|line| line["type"] == "user")
        .map(|line| line["priority"].clone())
        .collect();
    assert_eq!(priorities, ["later", "next"]);
    let items = session.transcript().items();
    let bash = items
        .iter()
        .position(|item| matches!(&item.body, Body::Tool(t) if t.name == "Bash"))
        .unwrap();
    let steer = items
        .iter()
        .position(|item| matches!(&item.body, Body::User(m) if m.delivery == Delivery::Steered))
        .unwrap();
    assert!(steer > bash);
    assert!(items.iter().all(|item| item.turn == TurnId(1)));
    assert!(main_text(&session, TurnId(1)).contains("STEER"));
    assert!(session.queue().is_empty());
    assert_eq!(*session.status(), Status::Ready);
}

#[test]
fn background_subagent_nests_output_and_resumes_the_agent() {
    let (session, _) = replay("subagent", "prompt", |s| decide_pending(s, true));
    let agent = tool(&session, "Agent");
    assert!(agent.background);
    assert_eq!(agent.state, ToolState::Succeeded);
    let summary = &agent.output.as_ref().unwrap().text;
    assert!(summary.contains("canopy fixture note"), "{summary}");
    assert!(
        !summary.contains("Async agent launched"),
        "placeholder is not shown"
    );
    let nested: Vec<_> = session
        .transcript()
        .items()
        .iter()
        .filter(|item| item.parent.as_deref() == Some(agent.call_id.as_str()))
        .collect();
    assert!(nested.iter().any(|item| matches!(
        &item.body,
        Body::Tool(t) if t.name == "Read" && t.state == ToolState::Succeeded
    )));
    let ends: Vec<_> = session
        .transcript()
        .items()
        .iter()
        .filter(|item| matches!(item.body, Body::TurnEnd(_)))
        .map(|item| item.turn)
        .collect();
    assert_eq!(ends.len(), 2);
    assert!(
        !session
            .transcript()
            .items()
            .iter()
            .any(|item| item.turn == ends[1] && matches!(item.body, Body::User(_))),
        "the second turn was started by the agent"
    );
    assert!(main_text(&session, ends[1]).contains("canopy fixture note"));
    assert_eq!(session.background_tasks(), 0);
    assert_eq!(*session.status(), Status::Ready);
}

#[test]
fn resumed_session_keeps_its_id() {
    let (first, _) = replay("resume-first", "prompt", |_| vec![]);
    let (second, _) = replay("resume-second", "prompt", |_| vec![]);
    assert!(first.session_id().is_some());
    assert_eq!(first.session_id(), second.session_id());
}

/// Synthetic: shapes from the SDK typings for events that are hard to trigger live.
#[test]
fn retries_limits_compaction_and_unknown_events_become_notices_or_are_ignored() {
    let (mut session, _) = ChatSession::new();
    for line in [
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}"#,
        r#"{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":500,"error_status":529,"error":"overloaded"}"#,
        r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}"#,
        r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"}}"#,
        r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto","pre_tokens":1}}"#,
        r#"{"type":"system","subtype":"something_new"}"#,
        r#"{"type":"control_request","request_id":"x1","request":{"subtype":"hook_callback","callback_id":"h"}}"#,
    ] {
        let replies = session.on_message(message(line));
        if line.contains("hook_callback") {
            assert_eq!(replies[0]["response"]["subtype"], "error");
            assert_eq!(replies[0]["response"]["request_id"], "x1");
        }
    }
    let kinds: Vec<_> = session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Notice(n) => Some(n.kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            NoticeKind::ApiRetry,
            NoticeKind::RateLimit,
            NoticeKind::Compacted
        ]
    );
    assert_eq!(*session.status(), Status::Ready);
}

#[test]
fn exit_mid_turn_fails_the_turn_and_cancels_pending_permissions() {
    let (mut session, _) = ChatSession::new();
    session.submit("hi");
    for line in [
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}"#,
        r#"{"type":"control_request","request_id":"p1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[]},"tool_use_id":"toolu_q"}}"#,
    ] {
        session.on_message(message(line));
    }
    assert_eq!(*session.status(), Status::NeedsDecision);
    session.on_event(Event::Exited(Exit {
        code: Some(1),
        signal: None,
        stderr: "boom".into(),
    }));
    assert!(matches!(session.status(), Status::Failed(_)));
    let items = session.transcript().items();
    assert!(items.iter().any(|item| matches!(
        &item.body,
        Body::Permission(p) if p.kind == PermissionKind::Question && p.decision == Decision::Cancelled
    )));
    assert!(items.iter().any(
        |item| matches!(&item.body, Body::TurnEnd(e) if matches!(e.outcome, Outcome::Failed(_)))
    ));
    assert!(
        items
            .iter()
            .any(|item| matches!(&item.body, Body::Notice(n) if n.text == "boom"))
    );
    assert!(
        session.submit("again").is_empty(),
        "an ended session accepts no input"
    );
}

#[test]
fn failed_initialize_is_reported() {
    let (mut session, _) = ChatSession::new();
    session.on_message(message(
        r#"{"type":"control_response","response":{"subtype":"error","request_id":"canopy-1","error":"bad flag"}}"#,
    ));
    assert_eq!(
        *session.status(),
        Status::Failed("Claude Code could not start the session: bad flag".into())
    );
}

#[test]
fn exit_cancels_background_calls_and_ignores_late_output() {
    let lines = lines("subagent");
    let (mut session, _) = ChatSession::new();
    session.submit("prompt");
    // Stop right after the first turn: the background subagent is still running.
    for line in &lines {
        session.on_message(message(line));
        if outcomes(&session).len() == 1 {
            break;
        }
    }
    assert_eq!(tool(&session, "Agent").state, ToolState::Running);
    session.close_requested();
    session.on_event(Event::Exited(Exit {
        code: Some(143),
        signal: None,
        stderr: String::new(),
    }));
    assert_eq!(tool(&session, "Agent").state, ToolState::Cancelled);
    let count = session.transcript().items().len();
    session.on_message(message(lines.last().unwrap()));
    assert_eq!(session.transcript().items().len(), count);
    assert!(matches!(session.status(), Status::Exited { .. }));
}

#[test]
fn rejected_interrupt_returns_to_working() {
    let (mut session, _) = ChatSession::new();
    session.submit("hi");
    session.on_message(message(
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}"#,
    ));
    let lines = session.interrupt();
    let id = lines.last().unwrap()["request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(*session.status(), Status::Interrupting);
    session.on_message(message(&format!(
        r#"{{"type":"control_response","response":{{"subtype":"error","request_id":"{id}","error":"busy"}}}}"#
    )));
    assert_eq!(*session.status(), Status::Working);
    assert!(
        session
            .transcript()
            .items()
            .iter()
            .any(|item| matches!(&item.body, Body::Notice(n) if n.kind == NoticeKind::Error))
    );
}

#[test]
fn settings_wait_for_initialize_and_revert_when_refused() {
    use canopy_desktop::chat::config::{ChatSettings, Effort, Mode};
    let (mut session, first) = ChatSession::with_settings(ChatSettings::default());
    assert_eq!(first.len(), 1);
    assert!(
        session.set_mode(Mode::Plan).is_empty(),
        "held until initialize"
    );
    assert!(
        session.set_mode(Mode::Plan).is_empty(),
        "unchanged value sends nothing"
    );
    let flushed = session.on_message(message(
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{
            "current_permission_mode":"default",
            "models":[{"value":"default","displayName":"Default","supportsEffort":true,"supportedEffortLevels":["low","high"]}]}}}"#,
    ));
    assert_eq!(flushed.len(), 1);
    assert_eq!(flushed[0]["request"]["subtype"], "set_permission_mode");
    assert_eq!(session.models().len(), 1);
    assert_eq!(
        session.settings().mode,
        Mode::Plan,
        "the user's newer choice wins"
    );
    let lines = session.set_effort(Some(Effort::High));
    let id = lines[0]["request_id"].as_str().unwrap().to_owned();
    session.on_message(message(&format!(
        r#"{{"type":"control_response","response":{{"subtype":"error","request_id":"{id}","error":"nope"}}}}"#
    )));
    assert_eq!(
        session.settings().effort,
        None,
        "refused change is reverted"
    );
    assert!(
        session.transcript().items().iter().any(
            |item| matches!(&item.body, Body::Notice(n) if n.text.contains("reasoning effort"))
        )
    );
    session.on_message(message(
        r#"{"type":"system","subtype":"status","status":null,"permissionMode":"acceptEdits"}"#,
    ));
    assert_eq!(
        session.settings().mode,
        Mode::AcceptEdits,
        "CLI-side mode changes are reflected"
    );
}

#[test]
fn recorded_models_and_setting_changes_are_accepted() {
    use canopy_desktop::chat::config::Mode;
    let lines = lines("settings");
    let (mut session, _) = ChatSession::new();
    session.on_message(message(&lines[0]));
    let models = session.models().to_vec();
    assert!(models.len() >= 2, "the CLI lists its models");
    assert!(models.iter().any(|m| m.value == "default"));
    let model = models.iter().find(|m| m.supports_effort()).unwrap().clone();
    // Same order as the live scenario: mode, model, effort (request ids canopy-2..4).
    assert!(!session.set_mode(Mode::Yolo).is_empty());
    assert!(!session.set_model(Some(model.value.clone())).is_empty());
    assert!(
        !session
            .set_effort(model.effort_levels.first().copied())
            .is_empty()
    );
    session.submit("prompt");
    for line in &lines[1..] {
        session.on_message(message(line));
    }
    assert_eq!(session.settings().mode, Mode::Yolo);
    assert_eq!(
        session.settings().model.as_deref(),
        Some(model.value.as_str())
    );
    assert!(
        !session
            .transcript()
            .items()
            .iter()
            .any(|item| matches!(&item.body, Body::Notice(_) | Body::Permission(_))),
        "every change was accepted and Yolo asked for nothing"
    );
    assert_eq!(tool(&session, "Bash").state, ToolState::Succeeded);
}

#[test]
fn background_subagent_tools_finish_after_the_main_turn() {
    // Subagent A's Bash starts before the main turn's result and finishes after it.
    let (session, _) = replay("parallel-subagents", "prompt", |_| vec![]);
    let items = session.transcript().items();
    let open: Vec<_> = items
        .iter()
        .filter_map(|item| match &item.body {
            Body::Tool(t) if !t.state.finished() => Some(t.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(open.is_empty(), "{open:?}");
    let agents = items
        .iter()
        .filter(|item| matches!(&item.body, Body::Tool(t) if t.name == "Agent" && t.state == ToolState::Succeeded))
        .count();
    assert_eq!(agents, 2);
}

#[test]
fn handback_becomes_the_subagent_report() {
    let (mut session, _) = ChatSession::new();
    session.submit("go");
    for line in [
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}"#,
        r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"agent1","name":"Agent","input":{"description":"Look","prompt":"Look around"}}]},"parent_tool_use_id":null}"#,
        r#"{"type":"system","subtype":"task_started","tool_use_id":"agent1","is_backgrounded":true}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"agent1","content":"Async agent launched"}]},"parent_tool_use_id":null}"#,
        r#"{"type":"result","subtype":"success","is_error":false}"#,
        r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"hb1","name":"SubagentHandback","input":{"message":"Found 3 files."}}]},"parent_tool_use_id":"agent1"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"hb1","content":"{\"success\":true}"}]},"parent_tool_use_id":"agent1"}"#,
        r#"{"type":"system","subtype":"task_notification","tool_use_id":"agent1","status":"completed","summary":"This agent's report was delivered to you as a message from \"x\" (its SubagentHandback call)."}"#,
    ] {
        session.on_message(message(line));
    }
    let agent = tool(&session, "Agent");
    assert_eq!(agent.state, ToolState::Succeeded);
    assert_eq!(agent.output.as_ref().unwrap().text, "Found 3 files.");
    let handback = session
        .transcript()
        .items()
        .iter()
        .find(|item| matches!(&item.body, Body::Tool(t) if t.name == "SubagentHandback"))
        .unwrap();
    assert!(!canopy_desktop::chat::presentation::visible(handback));
}

#[test]
fn handback_survives_a_notification_before_its_result() {
    let (mut session, _) = ChatSession::new();
    session.submit("go");
    for line in [
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}"#,
        r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"agent1","name":"Agent","input":{"description":"Look"}}]},"parent_tool_use_id":null}"#,
        r#"{"type":"system","subtype":"task_started","tool_use_id":"agent1","is_backgrounded":true}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"agent1","content":"launched"}]},"parent_tool_use_id":null}"#,
        r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"hb1","name":"SubagentHandback","input":{"message":"Report text"}}]},"parent_tool_use_id":"agent1"}"#,
        r#"{"type":"system","subtype":"task_notification","tool_use_id":"agent1","status":"completed","summary":"delivered elsewhere"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"hb1","content":"ok"}]},"parent_tool_use_id":"agent1"}"#,
    ] {
        session.on_message(message(line));
    }
    assert_eq!(
        tool(&session, "Agent").output.as_ref().unwrap().text,
        "Report text"
    );
}

#[test]
fn recorded_tools_become_views_and_turn_changes() {
    use canopy_desktop::chat::tools::{self, ToolView};
    let (session, _) = replay("tools", "prompt", |_| vec![]);
    let views: Vec<ToolView> = session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Tool(t) => Some(tools::view(t)),
            _ => None,
        })
        .collect();
    assert!(
        views
            .iter()
            .any(|v| matches!(v, ToolView::Read { total: 4, .. }))
    );
    assert!(
        views
            .iter()
            .any(|v| matches!(v, ToolView::Shell { failed: true, .. }))
    );
    assert!(views.iter().any(
        |v| matches!(v, ToolView::Shell { failed: false, stdout, .. } if stdout.contains("a.txt"))
    ));
    let changes = tools::turn_changes(session.transcript(), TurnId(1));
    let summary: Vec<_> = changes
        .iter()
        .map(|c| {
            (
                c.path.file_name().unwrap().to_string_lossy().into_owned(),
                c.created,
                c.added,
                c.removed,
            )
        })
        .collect();
    // a.txt: edit (+1 -1) then rewrite (+1 -3); b.txt created with two lines.
    assert_eq!(
        summary,
        [("a.txt".into(), false, 2, 4), ("b.txt".into(), true, 2, 0)]
    );
}

#[test]
fn recorded_thinking_streams_a_summary() {
    let (session, _) = replay("thinking", "prompt", |_| vec![]);
    let thinking: String = session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Thinking(block) => Some(block.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(!thinking.trim().is_empty());
    assert!(
        session
            .transcript()
            .items()
            .iter()
            .all(|item| !matches!(&item.body, Body::Thinking(b) if b.streaming))
    );
}
