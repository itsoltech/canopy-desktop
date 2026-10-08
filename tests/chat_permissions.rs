//! Answers to permission, question and plan requests, against hand-written frames
//! shaped like Claude Code 2.1.294 `can_use_tool` requests (see docs/chat.md).
use canopy_desktop::chat::{
    claude::wire,
    session::{ChatSession, DENIED_MESSAGE, Response, Status},
    transcript::{Body, Decision, ItemId, Resolution},
};
use serde_json::{Value, json};

fn feed(session: &mut ChatSession, value: Value) -> Vec<Value> {
    session.on_message(
        wire::parse_line(value.to_string().as_bytes())
            .unwrap()
            .unwrap(),
    )
}

fn working() -> ChatSession {
    let (mut session, _) = ChatSession::new();
    feed(
        &mut session,
        json!({"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}),
    );
    feed(
        &mut session,
        json!({"type":"system","subtype":"init","session_id":"s1"}),
    );
    session.submit("go");
    session
}

fn request(
    session: &mut ChatSession,
    id: &str,
    tool: &str,
    input: Value,
    suggestions: Value,
) -> ItemId {
    feed(
        session,
        json!({"type":"control_request","request_id":id,"request":{
            "subtype":"can_use_tool","tool_name":tool,"input":input,
            "tool_use_id":format!("toolu_{id}"),"permission_suggestions":suggestions,
        }}),
    );
    *session.pending_decisions().last().unwrap()
}

fn reply(lines: &[Value]) -> &Value {
    &lines[0]["response"]["response"]
}

fn resolution(session: &ChatSession, id: ItemId) -> Option<Resolution> {
    session
        .transcript()
        .items()
        .iter()
        .find_map(|item| match &item.body {
            Body::Permission(p) if item.id == id => p.resolution.clone(),
            _ => None,
        })
}

#[test]
fn deny_tells_the_agent_what_to_do_instead() {
    let mut session = working();
    let id = request(
        &mut session,
        "r1",
        "Bash",
        json!({"command":"rm -rf build"}),
        json!([]),
    );
    assert_eq!(*session.status(), Status::NeedsDecision);
    let lines = session.respond(id, Response::Deny(Some("  use cargo clean  ".into())));
    assert_eq!(reply(&lines)["behavior"], "deny");
    assert_eq!(
        reply(&lines)["message"],
        "The user denied this in Canopy and said: use cargo clean"
    );
    assert_eq!(
        resolution(&session, id),
        Some(Resolution::Denied(Some("use cargo clean".into())))
    );
    assert_eq!(*session.status(), Status::Working);
    assert!(
        session.respond(id, Response::Allow).is_empty(),
        "answered once"
    );

    let id = request(
        &mut session,
        "r2",
        "Bash",
        json!({"command":"ls"}),
        json!([]),
    );
    let lines = session.respond(id, Response::Deny(Some("   ".into())));
    assert_eq!(reply(&lines)["message"], DENIED_MESSAGE);
}

#[test]
fn allow_for_this_chat_needs_something_to_remember() {
    let mut session = working();
    let id = request(
        &mut session,
        "r1",
        "WebFetch",
        json!({"url":"https://example.com"}),
        json!([]),
    );
    assert!(session.respond(id, Response::AllowForChat).is_empty());
    let suggestions = json!([
        {"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"npm test *"}],"behavior":"allow","destination":"localSettings"},
        {"type":"addDirectories","directories":["/tmp/x"],"destination":"session"},
        {"type":"setMode","mode":"acceptEdits","destination":"session"}
    ]);
    let id = request(
        &mut session,
        "r2",
        "Bash",
        json!({"command":"npm test -- a"}),
        suggestions,
    );
    let lines = session.respond(id, Response::AllowForChat);
    let updates = reply(&lines)["updatedPermissions"].as_array().unwrap();
    assert_eq!(
        updates.len(),
        1,
        "only the rule; no directory grant or mode switch"
    );
    assert_eq!(updates[0]["destination"], "session");
    assert_eq!(updates[0]["rules"][0]["ruleContent"], "npm test *");
    assert_eq!(session.take_remembered(), ["Bash(npm test *)"]);
    assert!(session.take_remembered().is_empty());
}

#[test]
fn answers_must_fit_the_request() {
    let mut session = working();
    let questions = json!({"questions":[
        {"question":"Color?","header":"Color","options":[{"label":"red","description":""}],"multiSelect":false},
        {"question":"Why?","header":"","options":[],"multiSelect":false}
    ]});
    let id = request(&mut session, "q", "AskUserQuestion", questions, json!([]));
    assert!(
        session.respond(id, Response::Allow).is_empty(),
        "questions need answers"
    );
    assert!(
        session
            .respond(
                id,
                Response::ApprovePlan {
                    accept_edits: false
                }
            )
            .is_empty()
    );
    assert!(
        session
            .respond(id, Response::Answer(vec![" ".into(), "".into()]))
            .is_empty()
    );
    let lines = session.respond(id, Response::Answer(vec!["red".into(), "because".into()]));
    assert_eq!(
        reply(&lines)["updatedInput"]["answers"],
        json!({"Color?": "red", "Why?": "because"})
    );
}

#[test]
fn keep_planning_sends_the_feedback() {
    let mut session = working();
    let id = request(
        &mut session,
        "p",
        "ExitPlanMode",
        json!({"plan":"# Plan"}),
        json!([]),
    );
    assert!(
        session.respond(id, Response::Allow).is_empty(),
        "plans are approved, not allowed"
    );
    let lines = session.respond(id, Response::KeepPlanning(Some("add tests".into())));
    assert_eq!(reply(&lines)["behavior"], "deny");
    assert_eq!(
        reply(&lines)["message"],
        "The user wants to keep planning. Feedback: add tests"
    );
}

#[test]
fn several_requests_wait_in_order_and_stop_dismisses_them() {
    let mut session = working();
    let first = request(&mut session, "a", "Bash", json!({"command":"a"}), json!([]));
    let second = request(
        &mut session,
        "b",
        "AskUserQuestion",
        json!({"questions":[]}),
        json!([]),
    );
    let third = request(
        &mut session,
        "c",
        "ExitPlanMode",
        json!({"plan":"p"}),
        json!([]),
    );
    assert_eq!(session.pending_decisions(), [first, second, third]);
    session.respond(first, Response::Allow);
    assert_eq!(session.pending_decisions(), [second, third]);
    assert_eq!(*session.status(), Status::NeedsDecision);
    session.interrupt();
    assert!(session.pending_decisions().is_empty());
    assert!(session.transcript().items().iter().all(|item| !matches!(
        &item.body,
        Body::Permission(p) if p.decision == Decision::Pending
    )));
}
