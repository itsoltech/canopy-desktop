//! Turn metadata behind message actions, context use and edits, against hand-written
//! frames shaped like Claude Code 2.1.294 output (see docs/chat.md).
use canopy_desktop::chat::{
    claude::wire,
    session::{ChatSession, Rewind},
    transcript::{Body, ItemId, NoticeKind},
};
use serde_json::{Value, json};

fn feed(session: &mut ChatSession, value: Value) -> Vec<Value> {
    session.on_message(
        wire::parse_line(value.to_string().as_bytes())
            .unwrap()
            .unwrap(),
    )
}

fn ready(api_key_source: &str) -> ChatSession {
    let (mut session, _) = ChatSession::new();
    feed(
        &mut session,
        json!({"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}),
    );
    feed(
        &mut session,
        json!({"type":"system","subtype":"init","session_id":"s1","apiKeySource":api_key_source}),
    );
    session
}

/// One turn: the message, a reply with message uuid `reply`, its result and idle.
fn turn(session: &mut ChatSession, text: &str, reply: &str) -> Vec<Value> {
    let mut lines = session.submit(text);
    feed(
        session,
        json!({"type":"system","subtype":"session_state_changed","state":"running"}),
    );
    feed(
        session,
        json!({"type":"assistant","uuid":reply,"message":{"id":format!("m-{reply}"),"role":"assistant","content":[{"type":"text","text":"ok"}]}}),
    );
    feed(
        session,
        json!({"type":"result","subtype":"success","is_error":false,"result":"ok","duration_ms":1200}),
    );
    lines.extend(feed(
        session,
        json!({"type":"system","subtype":"session_state_changed","state":"idle"}),
    ));
    lines
}

fn users(session: &ChatSession) -> Vec<ItemId> {
    session
        .transcript()
        .items()
        .iter()
        .filter(|item| matches!(item.body, Body::User(_)))
        .map(|item| item.id)
        .collect()
}

fn control(lines: &[Value], subtype: &str) -> Option<Value> {
    lines
        .iter()
        .rfind(|line| line["request"]["subtype"] == subtype)
        .cloned()
}

#[test]
fn an_edit_forks_after_the_previous_turn() {
    let mut session = ready("none");
    turn(&mut session, "remember APPLE", "a1");
    turn(&mut session, "remember BANANA", "a2");
    let [first, second] = users(&session)[..] else {
        panic!("two messages");
    };
    assert_eq!(
        session.fork_point(first),
        Some(None),
        "a fresh conversation"
    );
    assert_eq!(session.fork_point(second), Some(Some("a1".into())));
    assert_eq!(
        session.retry_draft().map(|d| d.text).as_deref(),
        Some("remember BANANA")
    );
    assert!(!session.api_billing(), "a subscription login");
}

#[test]
fn context_use_is_asked_once_per_idle() {
    let mut session = ready("ANTHROPIC_API_KEY");
    assert!(session.api_billing());
    let lines = turn(&mut session, "hi", "a1");
    let request = control(&lines, "get_context_usage").expect("context request");
    assert!(
        control(
            &feed(
                &mut session,
                json!({"type":"system","subtype":"session_state_changed","state":"idle"}),
            ),
            "get_context_usage"
        )
        .is_none(),
        "one request in flight"
    );
    feed(
        &mut session,
        json!({"type":"control_response","response":{"subtype":"success","request_id":request["request_id"],"response":{
            "totalTokens":34782,"maxTokens":1000000,"percentage":3,
            "categories":[{"name":"Autocompact buffer","tokens":33000,"kind":"buffer"},{"name":"Messages","tokens":3353,"kind":"used"}]
        }}}),
    );
    let usage = session.context().expect("context");
    assert_eq!(
        (usage.used, usage.max, usage.buffer),
        (34782, 1000000, 33000)
    );
}

#[test]
fn files_are_previewed_then_restored_by_message_uuid() {
    let mut session = ready("none");
    let sent = turn(&mut session, "write v1", "a1");
    let uuid = sent.iter().find(|line| line["type"] == "user").unwrap()["uuid"].clone();
    let item = users(&session)[0];
    let lines = session.rewind_files(item, true);
    assert_eq!(lines[0]["request"]["user_message_id"], uuid);
    assert_eq!(lines[0]["request"]["dry_run"], true);
    feed(
        &mut session,
        json!({"type":"control_response","response":{"subtype":"success","request_id":lines[0]["request_id"],"response":{
            "canRewind":true,"filesChanged":["/w/note.txt"],"insertions":1,"deletions":1
        }}}),
    );
    let Some((at, Rewind::Preview(preview))) = session.rewind().cloned() else {
        panic!("preview");
    };
    assert_eq!((at, preview.files.len(), preview.possible), (item, 1, true));
    let lines = session.rewind_files(item, false);
    feed(
        &mut session,
        json!({"type":"control_response","response":{"subtype":"success","request_id":lines[0]["request_id"],"response":{"canRewind":true}}}),
    );
    assert_eq!(session.rewind().map(|r| &r.1), Some(&Rewind::Done));
}

#[test]
fn retries_and_compaction_read_clearly() {
    let mut session = ready("none");
    session.submit("go");
    feed(
        &mut session,
        json!({"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":5000,"error_status":529}),
    );
    feed(
        &mut session,
        json!({"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto","pre_tokens":180000}}),
    );
    let notices: Vec<_> = session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Notice(notice) => Some(notice.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(notices[0].kind, NoticeKind::ApiRetry);
    assert!(notices[0].until.is_some(), "a countdown to the retry");
    assert_eq!(notices[1].kind, NoticeKind::Compacted);
    assert!(notices[1].text.contains("180k tokens"));
}
