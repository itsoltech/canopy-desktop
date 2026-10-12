//! Goals (`/goal`) against hand-written frames shaped like Claude Code 2.1.295 and
//! codex-cli 0.161.0 output (both live-checked; see docs/chat.md, "Etap 11").
use canopy_desktop::chat::{
    claude::wire,
    codex::wire as codex_wire,
    config::ChatSettings,
    goal::{EndReason, GoalStatus},
    session::{ChatSession, CodexStart, Draft, Status},
    transcript::{Body, GoalCheck},
};
use serde_json::{Value, json};

const CONDITION: &str = "counter.txt contains 3";

fn feed(session: &mut ChatSession, line: Value) -> Vec<Value> {
    session.on_message(
        wire::parse_line(line.to_string().as_bytes())
            .unwrap()
            .unwrap(),
    )
}

fn claude() -> ChatSession {
    let (mut session, _) = ChatSession::new();
    feed(
        &mut session,
        json!({"type": "control_response", "response": {
            "subtype": "success", "request_id": "canopy-1", "response": {}}}),
    );
    feed(
        &mut session,
        json!({"type": "system", "subtype": "init", "session_id": "s1"}),
    );
    session
}

/// The CLI's own reply (to `/goal`), not the model's.
fn synthetic(text: &str) -> Value {
    json!({"type": "assistant", "parent_tool_use_id": null, "uuid": "x",
        "message": {"id": "m", "model": "<synthetic>", "content": [{"type": "text", "text": text}]}})
}

fn feedback(reason: &str) -> Value {
    json!({"type": "user", "parent_tool_use_id": null, "isSynthetic": true,
        "message": {"role": "user", "content": [{"type": "text",
            "text": format!("Stop hook feedback:\n[{CONDITION}]: {reason}")}]}})
}

fn result(turns: u64, reason: &str) -> Value {
    json!({"type": "result", "subtype": "success", "is_error": false, "num_turns": turns,
        "terminal_reason": reason, "result": ""})
}

fn idle() -> Value {
    json!({"type": "system", "subtype": "session_state_changed", "state": "idle"})
}

fn running() -> Value {
    json!({"type": "system", "subtype": "session_state_changed", "state": "running"})
}

fn sent_texts(lines: &[Value]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line["type"] == "user")
        .map(|line| {
            line["message"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn bodies(session: &ChatSession) -> Vec<Body> {
    session
        .transcript()
        .items()
        .iter()
        .map(|item| item.body.clone())
        .collect()
}

fn goal_end(session: &ChatSession) -> Option<canopy_desktop::chat::goal::GoalEnd> {
    bodies(session).into_iter().find_map(|body| match body {
        Body::GoalEnd(end) => Some(end),
        _ => None,
    })
}

#[test]
fn a_claude_goal_runs_as_one_turn_with_checks_and_ends_achieved() {
    let mut session = claude();
    let sent = sent_texts(&session.set_goal(Draft::text(CONDITION)));
    assert_eq!(sent, [format!("/goal {CONDITION}")]);
    feed(&mut session, running());
    // The bubble shows the condition, marked as the goal.
    assert!(
        bodies(&session)
            .iter()
            .any(|body| matches!(body, Body::User(m) if m.goal && m.text == CONDITION))
    );

    feed(&mut session, synthetic(&format!("Goal set: {CONDITION}")));
    feed(&mut session, feedback("It holds 1, not 3."));
    feed(&mut session, feedback("It holds 2, not 3."));
    let goal = session.goal().expect("goal");
    assert_eq!((goal.status, goal.checks), (GoalStatus::Pursuing, 2));
    assert!(session.pursuing_goal());
    assert!(bodies(&session).contains(&Body::GoalCheck(GoalCheck {
        n: 2,
        reason: "It holds 2, not 3.".into(),
    })));
    // Claude's own goal text is goal state, not a reply.
    assert!(
        !bodies(&session)
            .iter()
            .any(|body| matches!(body, Body::Text(_)))
    );

    // The run ended on its own: whether the goal is still set is asked quietly.
    feed(&mut session, result(7, "completed"));
    let asked = sent_texts(&feed(&mut session, idle()));
    assert_eq!(asked, ["/goal"]);
    feed(&mut session, running());
    feed(
        &mut session,
        synthetic("No goal set. Usage: `/goal <condition>`"),
    );
    feed(&mut session, result(0, ""));

    assert!(session.goal().is_none());
    let end = goal_end(&session).expect("ending card");
    assert_eq!((end.reason, end.checks), (EndReason::Achieved, 2));
    // The quiet command left no message and no visible turn end behind.
    let users = bodies(&session)
        .into_iter()
        .filter(|body| matches!(body, Body::User(_)))
        .count();
    assert_eq!(users, 1);
    assert!(
        bodies(&session)
            .iter()
            .any(|body| matches!(body, Body::TurnEnd(end) if end.quiet))
    );
}

#[test]
fn stopping_a_claude_goal_keeps_it_set_and_clear_is_quiet() {
    let mut session = claude();
    session.set_goal(Draft::text(CONDITION));
    feed(&mut session, running());
    feed(&mut session, synthetic(&format!("Goal set: {CONDITION}")));
    feed(&mut session, feedback("It holds 1, not 3."));
    assert!(!session.interrupt().is_empty());
    feed(&mut session, result(5, "aborted_streaming"));
    feed(&mut session, idle());
    assert_eq!(session.goal().map(|g| g.status), Some(GoalStatus::Set));
    assert!(goal_end(&session).is_none());

    let sent = sent_texts(&session.clear_goal(EndReason::ClearedByUser));
    assert_eq!(sent, ["/goal clear"]);
    feed(&mut session, running());
    feed(
        &mut session,
        synthetic(&format!("Goal cleared: {CONDITION}")),
    );
    feed(&mut session, result(0, ""));
    assert!(session.goal().is_none());
    assert_eq!(
        goal_end(&session).map(|end| end.reason),
        Some(EndReason::ClearedByUser)
    );
}

#[test]
fn a_continued_claude_conversation_clears_a_goal_it_may_still_hold() {
    let mut session = claude();
    let sent = sent_texts(&session.clear_stale_goal());
    assert_eq!(sent, ["/goal clear"]);
    feed(&mut session, running());
    // The edited message waits for it.
    assert!(sent_texts(&session.submit("edited")).is_empty());
    feed(&mut session, synthetic("No goal set"));
    feed(&mut session, result(0, ""));
    assert_eq!(sent_texts(&feed(&mut session, idle())), ["edited"]);
    assert!(goal_end(&session).is_none());
}

fn codex_message(value: Value) -> codex_wire::Message {
    codex_wire::parse_line(value.to_string().as_bytes())
        .unwrap()
        .unwrap()
}

fn codex() -> ChatSession {
    let (mut session, lines) = ChatSession::codex(
        ChatSettings::default(),
        CodexStart {
            cwd: "/tmp/x".into(),
            ..Default::default()
        },
    );
    let id = lines[0]["id"].as_str().unwrap().to_owned();
    let lines = session.on_codex(codex_message(json!({"id": id, "result": {}})));
    let thread = lines[1]["id"].as_str().unwrap().to_owned();
    session.on_codex(codex_message(
        json!({"id": thread, "result": {"thread": {"id": "t"}}}),
    ));
    session
}

fn answer(session: &mut ChatSession, line: &Value, result: Value) -> Vec<Value> {
    session.on_codex(codex_message(json!({"id": line["id"], "result": result})))
}

fn notify(session: &mut ChatSession, method: &str, params: Value) -> Vec<Value> {
    session.on_codex(codex_message(json!({"method": method, "params": params})))
}

fn goal(status: &str, tokens: u64) -> Value {
    json!({"threadId": "t", "goal": {"threadId": "t", "objective": CONDITION,
        "status": status, "tokensUsed": tokens, "timeUsedSeconds": 40, "tokenBudget": null,
        "createdAt": 1, "updatedAt": 2}})
}

/// A goal set and running its first turn `u1`.
fn codex_goal() -> ChatSession {
    let mut session = codex();
    let lines = session.set_goal(Draft::text(CONDITION));
    // Paused until our turn runs, so Codex starts none of its own first.
    assert_eq!(lines[0]["method"], "thread/goal/set");
    assert_eq!(lines[0]["params"]["status"], "paused");
    assert_eq!(lines[0]["params"]["origin"], "user");
    let turn = answer(&mut session, &lines[0], goal("paused", 0));
    assert_eq!(turn[0]["method"], "turn/start");
    assert_eq!(turn[0]["params"]["input"][0]["text"], CONDITION);
    let activate = answer(&mut session, &turn[0], json!({"turn": {"id": "u1"}}));
    assert_eq!(activate[0]["params"]["status"], "active");
    answer(&mut session, &activate[0], goal("active", 0));
    notify(&mut session, "thread/goal/updated", goal("active", 0));
    notify(
        &mut session,
        "turn/started",
        json!({"threadId": "t", "turn": {"id": "u1"}}),
    );
    assert_eq!(session.goal().map(|g| g.status), Some(GoalStatus::Pursuing));
    session
}

fn completed(session: &mut ChatSession, turn: &str) -> Vec<Value> {
    notify(
        session,
        "turn/completed",
        json!({"threadId": "t", "turn": {"id": turn, "status": "completed"}}),
    )
}

#[test]
fn codex_goal_turns_are_one_piece_of_work_and_messages_steer_into_them() {
    let mut session = codex_goal();
    completed(&mut session, "u1");
    // Codex starts the next goal turn by itself: still working, nothing sent between.
    assert_eq!(*session.status(), Status::Working);
    assert!(session.goal_hold());
    assert!(session.submit("also update the docs").is_empty());

    let lines = notify(
        &mut session,
        "turn/started",
        json!({"threadId": "t", "turn": {"id": "u2"}}),
    );
    assert!(!session.goal_hold());
    assert_eq!(lines[0]["method"], "turn/steer");
    assert_eq!(lines[0]["params"]["expectedTurnId"], "u2");
    answer(&mut session, &lines[0], json!({"turnId": "u2"}));
    assert!(bodies(&session).contains(&Body::GoalTurn(2)));

    notify(&mut session, "thread/goal/updated", goal("complete", 1200));
    completed(&mut session, "u2");
    let end = goal_end(&session).expect("ending card");
    assert_eq!(
        (end.reason, end.turns, end.tokens),
        (EndReason::Achieved, 2, Some(1200))
    );
    assert!(!session.goal_hold());
    assert_eq!(*session.status(), Status::Ready);
}

#[test]
fn stop_pauses_a_codex_goal_before_interrupting_its_turn() {
    let mut session = codex_goal();
    let lines = session.interrupt();
    let methods: Vec<_> = lines.iter().map(|line| line["method"].clone()).collect();
    assert_eq!(methods, ["thread/goal/set", "turn/interrupt"]);
    assert_eq!(lines[0]["params"]["status"], "paused");
    assert_eq!(session.goal().map(|g| g.status), Some(GoalStatus::Paused));
}

#[test]
fn stop_between_codex_goal_turns_pauses_the_goal_and_frees_the_chat() {
    let mut session = codex_goal();
    completed(&mut session, "u1");
    assert!(session.goal_hold());
    let lines = session.interrupt();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["params"]["status"], "paused");
    assert!(!session.goal_hold());
    assert_eq!(*session.status(), Status::Ready);
    // Resume lets Codex continue it.
    let lines = session.resume_goal();
    assert_eq!(lines[0]["params"]["status"], "active");
}

#[test]
fn without_a_next_turn_the_codex_goal_hold_lets_go() {
    let mut session = codex_goal();
    completed(&mut session, "u1");
    session.submit("next");
    let lines = session.expire_goal_hold();
    assert_eq!(lines[0]["method"], "turn/start");
}

/// Recorded with tests/chat_claude_live.rs (`a_goal_runs_until_its_condition_holds`).
const RECORDED: &str = include_str!("fixtures/chat/claude-2.1.295/goal.jsonl");

#[test]
fn a_recorded_claude_goal_replays_to_its_end() {
    let (mut session, mut written) = ChatSession::new();
    written.extend(session.set_goal(Draft::text(
        "The file counter.txt in the current folder contains the number 2. Each turn may only \
         raise the number in it by exactly 1 (create it with 1 when it is missing); then end \
         the turn without doing anything else.",
    )));
    let mut uuids = std::collections::HashMap::<String, String>::new();
    for line in RECORDED.lines() {
        let mut value: Value = serde_json::from_str(line).unwrap();
        // Recorded message uuids stand for the ones this session wrote, in order.
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
        written.extend(feed(&mut session, value));
    }
    // The goal message and the quiet status question, nothing else.
    assert_eq!(
        sent_texts(&written)
            .iter()
            .map(|text| text.split(' ').next().unwrap())
            .collect::<Vec<_>>(),
        ["/goal", "/goal"]
    );
    assert!(session.goal().is_none());
    let end = goal_end(&session).expect("ending card");
    assert_eq!(end.reason, EndReason::Achieved);
    assert!(end.checks >= 1);
    assert!(
        !bodies(&session)
            .iter()
            .any(|body| matches!(body, Body::Text(block) if block.text.starts_with("Goal set:")))
    );
    assert_eq!(*session.status(), Status::Ready);
}
