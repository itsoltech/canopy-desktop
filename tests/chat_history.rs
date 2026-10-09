//! A chat saved and read back (as after an app restart) continues where it was: the
//! same items, ids and turns after them, editable messages and Retry with attachments.
use canopy_desktop::chat::{
    claude::wire,
    history,
    session::{Base, ChatSession, Draft},
    transcript::{Attachment, Body, Delivery, ItemId},
};
use serde_json::{Value, json};

fn feed(session: &mut ChatSession, value: Value) {
    session.on_message(
        wire::parse_line(value.to_string().as_bytes())
            .unwrap()
            .unwrap(),
    );
}

fn ready() -> ChatSession {
    let (mut session, _) = ChatSession::new();
    feed(
        &mut session,
        json!({"type":"control_response","response":{"subtype":"success","request_id":"canopy-1","response":{}}}),
    );
    feed(
        &mut session,
        json!({"type":"system","subtype":"init","session_id":"s1","apiKeySource":"none"}),
    );
    session
}

fn turn(session: &mut ChatSession, draft: Draft, reply: &str) {
    session.submit_draft(draft);
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
        json!({"type":"result","subtype":"success","is_error":false,"result":"ok","duration_ms":10}),
    );
    feed(
        session,
        json!({"type":"system","subtype":"session_state_changed","state":"idle"}),
    );
}

fn image() -> Attachment {
    Attachment::Image {
        file: "a.png".into(),
        media_type: "image/png".into(),
        width: 4,
        height: 4,
        data: Some("QUJD".into()),
        label: Some("[a.png (1 KB)]".into()),
    }
}

/// Saved as Canopy stores it and read back.
fn reload(session: &ChatSession) -> Vec<canopy_desktop::chat::transcript::Item> {
    let rows = canopy_desktop::settings::ChatHistoryRows {
        epochs: vec![(0, history::meta_json(&Default::default()))],
        items: session
            .transcript()
            .items()
            .iter()
            .map(|item| (0, item.id.0, history::item_json(item).unwrap()))
            .collect(),
        skipped: 0,
        skipped_keys: vec![],
    };
    let mut saved = history::parse(rows);
    assert_eq!(saved.lost, 0);
    let mut epoch = saved.epochs.remove(0);
    assert_eq!(epoch.meta.base(), Base::Fresh);
    history::settle(&mut epoch.items);
    epoch.items
}

#[test]
fn a_reloaded_chat_continues_with_its_ids_edits_and_retry() {
    let mut first = ready();
    turn(&mut first, Draft::text("one"), "a1");
    turn(
        &mut first,
        Draft {
            text: "look".into(),
            attachments: vec![image()],
        },
        "a2",
    );
    let items = reload(&first);
    assert_eq!(items.len(), first.transcript().items().len());

    let mut second = ready();
    second.restore_history(items);
    let users: Vec<ItemId> = second
        .transcript()
        .items()
        .iter()
        .filter(|item| matches!(&item.body, Body::User(m) if m.delivery == Delivery::Sent))
        .map(|item| item.id)
        .collect();
    assert_eq!(
        second.fork_point(users[0]),
        Some(None),
        "the first message starts over"
    );
    assert_eq!(second.fork_point(users[1]), Some(Some("a1".into())));
    // Saved images are sent again from their file: the data is loaded before sending.
    let retry = second.retry_draft().unwrap();
    assert_eq!(retry.text, "look");
    assert!(canopy_desktop::chat::attachments::needs_data(
        &retry.attachments
    ));

    let last = second.transcript().items().last().unwrap().clone();
    turn(&mut second, Draft::text("three"), "a3");
    let index = second.transcript().position(last.id).unwrap() + 1;
    let added = &second.transcript().items()[index];
    assert!(
        added.id.0 > last.id.0 && added.turn.0 > last.turn.0,
        "ids and turns continue"
    );
}

#[test]
fn a_turn_cut_short_by_quitting_reloads_as_interrupted() {
    let mut first = ready();
    first.submit("long task");
    feed(
        &mut first,
        json!({"type":"system","subtype":"session_state_changed","state":"running"}),
    );
    feed(
        &mut first,
        json!({"type":"assistant","uuid":"a1","message":{"id":"m1","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"sleep 100"}}]}}),
    );
    let items = reload(&first);
    let ended = items.iter().any(|item| {
        matches!(&item.body, Body::TurnEnd(end)
            if end.outcome == canopy_desktop::chat::transcript::Outcome::Interrupted)
    });
    assert!(ended, "the open turn ends as interrupted");
    let tool_open = items
        .iter()
        .any(|item| matches!(&item.body, Body::Tool(call) if !call.state.finished()));
    assert!(!tool_open, "running tools are settled");
}
