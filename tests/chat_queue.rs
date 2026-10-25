//! Queue and Steer: messages typed while the agent works, against hand-written frames
//! shaped like Claude Code 2.1.294 output (see docs/chat.md).
use canopy_desktop::chat::{
    claude::wire,
    session::{ChatSession, Status},
    transcript::{Body, Delivery, TurnId},
};
use serde_json::Value;

fn feed(session: &mut ChatSession, line: &str) -> Vec<Value> {
    session.on_message(wire::parse_line(line.as_bytes()).unwrap().unwrap())
}

fn ready(capabilities: &[&str]) -> ChatSession {
    let (mut session, _) = ChatSession::new();
    feed(
        &mut session,
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}"#,
    );
    let init = serde_json::json!({
        "type": "system", "subtype": "init", "session_id": "s1", "capabilities": capabilities,
    });
    feed(&mut session, &init.to_string());
    session
}

fn state(session: &mut ChatSession, state: &str) -> Vec<Value> {
    feed(
        session,
        &format!(r#"{{"type":"system","subtype":"session_state_changed","state":"{state}"}}"#),
    )
}

fn lifecycle(session: &mut ChatSession, uuid: &str, state: &str) -> Vec<Value> {
    feed(
        session,
        &format!(r#"{{"type":"command_lifecycle","command_uuid":"{uuid}","state":"{state}"}}"#),
    )
}

fn result(session: &mut ChatSession) -> Vec<Value> {
    feed(
        session,
        r#"{"type":"result","subtype":"success","is_error":false,"result":"ok"}"#,
    )
}

fn user_lines(lines: &[Value]) -> Vec<(String, String, String)> {
    lines
        .iter()
        .filter(|line| line["type"] == "user")
        .map(|line| {
            (
                line["message"]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                line["priority"].as_str().unwrap().to_owned(),
                line["uuid"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn bubbles(session: &ChatSession) -> Vec<(TurnId, String, Delivery)> {
    session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::User(m) => Some((item.turn, m.text.clone(), m.delivery)),
            _ => None,
        })
        .collect()
}

fn queued_texts(session: &ChatSession) -> Vec<String> {
    session.queue().iter().map(|q| q.text.clone()).collect()
}

#[test]
fn messages_typed_while_working_wait_and_go_one_turn_at_a_time() {
    let mut session = ready(&[]);
    let sent = user_lines(&session.submit("first"));
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1, "later", "the priority is always explicit");
    state(&mut session, "running");

    assert!(session.submit("second").is_empty());
    assert!(session.submit("third").is_empty());
    assert_eq!(queued_texts(&session), ["second", "third"]);
    assert_eq!(
        bubbles(&session).len(),
        1,
        "queued messages are not in the chat yet"
    );

    // A result alone is not the end: background work may still run until idle.
    assert!(result(&mut session).is_empty());
    let next = user_lines(&state(&mut session, "idle"));
    assert_eq!(next.len(), 1);
    assert_eq!(
        (next[0].0.as_str(), next[0].1.as_str()),
        ("second", "later")
    );
    assert_eq!(queued_texts(&session), ["third"]);
    assert_eq!(*session.status(), Status::Working);

    state(&mut session, "running");
    result(&mut session);
    let last = user_lines(&state(&mut session, "idle"));
    assert_eq!(last[0].0, "third");
    assert!(session.queue().is_empty());
    let turns: Vec<_> = bubbles(&session).into_iter().map(|b| (b.0, b.1)).collect();
    assert_eq!(
        turns,
        [
            (TurnId(1), "first".into()),
            (TurnId(2), "second".into()),
            (TurnId(3), "third".into())
        ]
    );
}

#[test]
fn steer_is_shown_where_the_agent_read_it() {
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    let first = user_lines(&session.submit("first"))[0].2.clone();
    lifecycle(&mut session, &first, "queued");
    lifecycle(&mut session, &first, "started");
    state(&mut session, "running");
    session.submit("also check the tests");
    let id = session.queue()[0].id;
    let steer = user_lines(&session.steer(id));
    assert_eq!(steer[0].1, "next");
    assert!(session.queue()[0].steering.is_some(), "Sending… until read");
    assert_eq!(bubbles(&session).len(), 1);

    lifecycle(&mut session, &steer[0].2, "queued");
    lifecycle(&mut session, &steer[0].2, "started");
    assert!(session.queue().is_empty());
    assert_eq!(
        bubbles(&session),
        [
            (TurnId(1), "first".into(), Delivery::Sent),
            (TurnId(1), "also check the tests".into(), Delivery::Steered)
        ]
    );
    lifecycle(&mut session, &steer[0].2, "completed");
    result(&mut session);
    lifecycle(&mut session, &first, "completed");
    assert!(user_lines(&state(&mut session, "idle")).is_empty());
    assert_eq!(*session.status(), Status::Ready);
}

#[test]
fn steer_read_after_the_turn_ended_starts_a_new_turn() {
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    session.submit("first");
    state(&mut session, "running");
    let steer = user_lines(&session.steer_text("one more thing"))[0]
        .2
        .clone();
    lifecycle(&mut session, &steer, "queued");
    result(&mut session);
    lifecycle(&mut session, &steer, "started");
    assert_eq!(
        bubbles(&session)[1],
        (TurnId(2), "one more thing".into(), Delivery::Sent)
    );
    assert_eq!(*session.status(), Status::Working);
}

#[test]
fn without_lifecycle_frames_a_steer_appears_immediately() {
    let mut session = ready(&[]);
    session.submit("first");
    state(&mut session, "running");
    let lines = session.steer_text("faster please");
    assert_eq!(user_lines(&lines)[0].1, "next");
    assert!(session.queue().is_empty());
    assert_eq!(bubbles(&session)[1].2, Delivery::Steered);
}

#[test]
fn stop_keeps_the_queue_until_the_user_sends_again() {
    let mut session = ready(&[wire::CANCEL_QUEUED_CAPABILITY]);
    session.submit("first");
    state(&mut session, "running");
    session.submit("second");
    let stop = session.interrupt();
    let interrupt = stop.last().unwrap();
    assert_eq!(interrupt["request"]["subtype"], "interrupt");
    assert_eq!(interrupt["request"]["cancel_queued"], true);
    feed(
        &mut session,
        r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#,
    );
    assert!(
        user_lines(&state(&mut session, "idle")).is_empty(),
        "Stop holds the queue"
    );
    assert!(session.queue_held());

    let sent = user_lines(&session.submit("third"));
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "second", "the oldest message goes first");
    assert_eq!(queued_texts(&session), ["third"]);
}

#[test]
fn resume_sends_the_queue_held_by_stop() {
    let mut session = ready(&[]);
    session.submit("first");
    state(&mut session, "running");
    session.submit("second");
    session.submit("third");
    session.interrupt();
    assert!(
        session.resume_queue().is_empty(),
        "waits until the Stop has finished"
    );
    feed(
        &mut session,
        r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#,
    );
    let sent = user_lines(&state(&mut session, "idle"));
    assert_eq!(
        sent[0].0,
        canopy_desktop::chat::session::RESUME_PROMPT,
        "the stopped work is finished first"
    );
    assert!(!session.queue_held());
    state(&mut session, "running");
    assert!(result(&mut session).is_empty());
    let next = user_lines(&state(&mut session, "idle"));
    assert_eq!(next[0].0, "second", "then the queue goes on");

    let mut stopped = ready(&[]);
    stopped.submit("first");
    state(&mut stopped, "running");
    stopped.submit("later");
    stopped.interrupt();
    result(&mut stopped);
    state(&mut stopped, "idle");
    assert!(stopped.queue_held());
    let sent = user_lines(&stopped.resume_queue());
    assert_eq!(
        sent[0].0,
        canopy_desktop::chat::session::RESUME_PROMPT,
        "an idle agent gets it right away"
    );

    // Sending something after Stop already moved on: Resume only releases the queue.
    let mut moved = ready(&[]);
    moved.submit("first");
    state(&mut moved, "running");
    moved.submit("queued");
    moved.interrupt();
    result(&mut moved);
    state(&mut moved, "idle");
    let id = moved.queue()[0].id;
    assert_eq!(user_lines(&moved.steer(id))[0].0, "queued");
    assert!(moved.resume_queue().is_empty(), "nothing held any more");
}

#[test]
fn a_steer_dropped_before_it_was_read_waits_again() {
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    session.submit("first");
    state(&mut session, "running");
    let steer = user_lines(&session.steer_text("wait"))[0].2.clone();
    lifecycle(&mut session, &steer, "queued");
    lifecycle(&mut session, &steer, "cancelled");
    assert_eq!(queued_texts(&session), ["wait"]);
    assert!(session.queue()[0].steering.is_none());
    assert_eq!(bubbles(&session).len(), 1);
}

#[test]
fn only_waiting_messages_can_be_edited_or_removed() {
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    session.submit("first");
    state(&mut session, "running");
    session.submit("a");
    session.submit("b");
    let (a, b) = (session.queue()[0].id, session.queue()[1].id);
    session.steer(a);
    assert_eq!(
        session.edit(a),
        None,
        "a steer in flight is no longer editable"
    );
    assert!(!session.remove(a));
    assert_eq!(session.edit(b).map(|d| d.text).as_deref(), Some("b"));
    assert_eq!(queued_texts(&session), ["a"]);
}

#[test]
fn a_restarted_session_keeps_the_queue_on_hold() {
    let mut old = ready(&[]);
    old.submit("first");
    state(&mut old, "running");
    old.submit("later message");
    let mut session = ready(&[]);
    session.restore_queue(old.take_queue());
    assert!(session.queue_held());
    assert!(user_lines(&state(&mut session, "idle")).is_empty());
    let sent = user_lines(&session.steer(session.queue()[0].id));
    assert_eq!(sent[0].0, "later message");
}

#[test]
fn first_message_before_initialize_is_held_not_queued() {
    let (mut session, _) = ChatSession::new();
    assert!(session.submit("hi").is_empty(), "held until initialize");
    assert!(session.queue().is_empty());
    let lines = feed(
        &mut session,
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}"#,
    );
    assert_eq!(user_lines(&lines)[0].0, "hi");
    assert_eq!(*session.status(), Status::Working);
}

#[test]
fn idle_releases_a_steer_the_cli_never_reported() {
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    session.submit("first");
    state(&mut session, "running");
    session.steer_text("lost?");
    result(&mut session);
    // Never reported as read: it goes out again as its own turn instead of blocking.
    let sent = user_lines(&state(&mut session, "idle"));
    assert_eq!((sent[0].0.as_str(), sent[0].1.as_str()), ("lost?", "later"));
    assert!(session.queue().is_empty());
}

#[test]
fn discarding_keeps_steers_already_written() {
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    session.submit("first");
    state(&mut session, "running");
    session.submit("waiting");
    let steer = user_lines(&session.steer_text("in flight"))[0].2.clone();
    assert_eq!(session.unsent(), 1);
    session.discard_unsent();
    assert_eq!(queued_texts(&session), ["in flight"]);
    lifecycle(&mut session, &steer, "started");
    assert_eq!(
        bubbles(&session)[1].1,
        "in flight",
        "what the agent read is shown"
    );
}

#[test]
fn steer_waits_while_stop_is_in_flight() {
    let mut session = ready(&[]);
    session.submit("first");
    state(&mut session, "running");
    session.interrupt();
    assert!(session.steer_text("after stop").is_empty());
    assert_eq!(queued_texts(&session), ["after stop"]);
}

#[test]
fn stop_after_the_turn_stops_background_subagents() {
    let mut session = ready(&[]);
    session.submit("first");
    state(&mut session, "running");
    feed(
        &mut session,
        r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"id":"a"}]}"#,
    );
    result(&mut session);
    assert_eq!(*session.status(), Status::Ready);
    let stop = session.interrupt();
    assert_eq!(stop.last().unwrap()["request"]["subtype"], "interrupt");
    assert_eq!(
        *session.status(),
        Status::Ready,
        "no turn to show as stopping"
    );
    state(&mut session, "idle");
    assert!(session.interrupt().is_empty(), "nothing left to stop");
}

#[test]
fn queued_messages_can_be_reordered_but_not_steers_in_flight() {
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    session.submit("first");
    state(&mut session, "running");
    for text in ["a", "b", "c"] {
        session.submit(text);
    }
    let ids: Vec<_> = session.queue().iter().map(|q| q.id).collect();
    assert!(session.move_queued(ids[2], Some(ids[0])));
    assert_eq!(queued_texts(&session), ["c", "a", "b"]);
    assert!(session.move_queued(ids[2], None));
    assert_eq!(queued_texts(&session), ["a", "b", "c"]);
    session.steer(ids[0]);
    assert!(
        !session.move_queued(ids[0], None),
        "a steer in flight stays put"
    );
}

#[test]
fn accepted_dispatch_preserves_host_message_id_and_does_not_create_another_queue() {
    use canopy_desktop::chat::session::Draft;
    let mut session = ready(&[]);
    let id = "7ba2f1b0-ddb6-4f6a-aac1-c8aff5919cec";
    let lines = session
        .submit_accepted(Draft::text("accepted"), id)
        .unwrap();
    assert_eq!(user_lines(&lines)[0].2, id);
    assert!(
        session
            .submit_accepted(Draft::text("wait in host queue"), "next")
            .is_err()
    );
    assert!(session.queue().is_empty());
    let users = session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::User(user) => Some(user),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].uuid.as_deref(), Some(id));
}

#[test]
fn a_host_queued_message_is_delivered_only_on_the_cli_lifecycle() {
    use canopy_desktop::chat::backend::Receipt;
    use canopy_desktop::chat::session::Draft;
    let mut session = ready(&[canopy_desktop::chat::claude::wire::LIFECYCLE_CAPABILITY]);
    session.use_host_queue();
    assert!(session.ready_for_message());
    let lines = session
        .submit_accepted(Draft::text("first"), "host-message-1")
        .unwrap();
    assert_eq!(
        user_lines(&lines)[0].2,
        "host-message-1",
        "the host ID goes out"
    );
    assert!(
        session.take_receipts().is_empty(),
        "written is not delivered"
    );
    assert!(!session.ready_for_message());
    assert!(
        session
            .submit_accepted(Draft::text("second"), "host-message-2")
            .is_err(),
        "a busy session never queues an accepted message itself"
    );
    lifecycle(&mut session, "host-message-1", "queued");
    assert!(session.take_receipts().is_empty());
    lifecycle(&mut session, "host-message-1", "started");
    assert_eq!(
        session.take_receipts(),
        vec![("host-message-1".to_owned(), Receipt::Delivered)]
    );
    result(&mut session);
    state(&mut session, "idle");

    // Discarded before the CLI read it: refused, not delivered.
    session
        .submit_accepted(Draft::text("dropped"), "host-message-2")
        .unwrap();
    lifecycle(&mut session, "host-message-2", "discarded");
    assert_eq!(
        session.take_receipts(),
        vec![("host-message-2".to_owned(), Receipt::Refused)]
    );
}

#[test]
fn with_the_host_queue_an_unread_steer_returns_instead_of_going_out_alone() {
    let mut session = ready(&[canopy_desktop::chat::claude::wire::LIFECYCLE_CAPABILITY]);
    session.use_host_queue();
    session
        .submit_accepted(
            canopy_desktop::chat::session::Draft::text("first"),
            "host-message-1",
        )
        .unwrap();
    let steer = user_lines(&session.steer_text("also this"))[0].2.clone();
    lifecycle(&mut session, &steer, "cancelled");
    let lines = result(&mut session);
    assert!(
        user_lines(&lines).is_empty(),
        "nothing is resent by the session"
    );
    let unread = session.take_unread_steers();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].text, "also this");
}

#[test]
fn an_accepted_steer_is_evidenced_under_its_host_message_id() {
    use canopy_desktop::chat::{backend::Receipt, session::Draft};
    let mut session = ready(&[wire::LIFECYCLE_CAPABILITY]);
    session.use_host_queue();
    let first = "11111111-1111-4111-8111-111111111111";
    session
        .submit_accepted(Draft::text("first"), first)
        .unwrap();
    lifecycle(&mut session, first, "started");
    state(&mut session, "running");
    assert_eq!(
        session.take_receipts(),
        [(first.to_owned(), Receipt::Delivered)]
    );
    assert!(session.can_steer());

    // The durable message's own ID goes to the agent; nothing is queued in the session.
    let steer = "22222222-2222-4222-8222-222222222222";
    let lines = user_lines(
        &session
            .steer_accepted(Draft::text("check tests"), steer)
            .unwrap(),
    );
    assert_eq!(lines, [("check tests".into(), "next".into(), steer.into())]);
    assert!(session.queue().is_empty());
    assert!(!session.can_steer(), "one steer in flight at a time");
    assert_eq!(
        bubbles(&session).len(),
        1,
        "shown only once the agent reads it"
    );
    lifecycle(&mut session, steer, "started");
    assert_eq!(
        session.take_receipts(),
        [(steer.to_owned(), Receipt::Delivered)]
    );
    assert_eq!(
        bubbles(&session)[1],
        (TurnId(1), "check tests".into(), Delivery::Steered)
    );

    // Refused before it was read: the host settles it as failed, no bubble.
    let refused = "33333333-3333-4333-8333-333333333333";
    session
        .steer_accepted(Draft::text("never read"), refused)
        .unwrap();
    lifecycle(&mut session, refused, "cancelled");
    assert_eq!(
        session.take_receipts(),
        [(refused.to_owned(), Receipt::Refused)]
    );
    assert_eq!(bubbles(&session).len(), 2);
}
