//! Conversations saved while agents could be switched mid-conversation (withdrawn,
//! DEC-087) still load: each part keeps its agent, and a message sent with the
//! conversation so far sends it again on Retry. A chat's agent is fixed once it has a
//! session.
use canopy_desktop::chat::{
    claude::wire as claude_wire,
    codex::wire as codex_wire,
    config::ChatSettings,
    history::{self, AgentRef, Boundary, EpochMeta},
    session::{ChatSession, Draft},
};
use canopy_desktop::settings::ChatHistoryRows;
use canopy_desktop::state::workspace::{PaneKind, PaneMetadata, Workspace};
use serde_json::{Value, json};

fn feed(session: &mut ChatSession, value: Value) {
    session.on_message(
        claude_wire::parse_line(value.to_string().as_bytes())
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

fn finish(session: &mut ChatSession, reply: &str) {
    feed(
        session,
        json!({"type":"assistant","uuid":reply,"message":{"id":format!("m-{reply}"),"role":"assistant","content":[{"type":"text","text":reply}]}}),
    );
    feed(
        session,
        json!({"type":"result","subtype":"success","is_error":false,"result":reply,"duration_ms":10}),
    );
    feed(
        session,
        json!({"type":"system","subtype":"session_state_changed","state":"idle"}),
    );
}

fn user_line(lines: &[Value]) -> &Value {
    lines
        .iter()
        .find(|line| line["type"] == "user")
        .expect("user message")
}

#[test]
fn the_context_goes_first_and_apart_from_the_message() {
    let line = claude_wire::user_message(
        "Which word?",
        &[],
        Some("<canopy-context>earlier</canopy-context>"),
        "u1",
        claude_wire::Priority::Later,
    );
    let content = line["message"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(
        content[0]["text"],
        "<canopy-context>earlier</canopy-context>"
    );
    assert_eq!(content[1]["text"], "Which word?");

    let input = codex_wire::input("Which word?", &[], Some("ctx"), None);
    assert_eq!(
        input[0],
        json!({"type": "text", "text": "ctx", "text_elements": []})
    );
    assert_eq!(input[1]["text"], "Which word?");
    assert_eq!(codex_wire::input("hi", &[], None, None).len(), 1);
}

#[test]
fn a_message_with_its_context_sends_it_again_on_retry() {
    let mut session = ready();
    let lines = session.submit_draft(Draft {
        text: "Which word?".into(),
        context: Some("ctx".into()),
        ..Default::default()
    });
    assert_eq!(user_line(&lines)["message"]["content"][0]["text"], "ctx");
    finish(&mut session, "KIWI");
    let retry = session.retry_draft().expect("retry");
    assert_eq!(retry.context.as_deref(), Some("ctx"));
    // Nothing else carries it on.
    let lines = session.submit("Thanks");
    assert_eq!(
        user_line(&lines)["message"]["content"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn saved_sessions_know_their_agent_and_older_ones_still_load() {
    let agent = AgentRef {
        tool: "codex".into(),
        profile: Some("codex-default".into()),
        session: Some("t1".into()),
        settings: Some(ChatSettings {
            model: Some("gpt-6.1-sol".into()),
            ..Default::default()
        }),
    };
    let meta = EpochMeta {
        before: Some(Boundary::SwitchedToCodex),
        continued: false,
        agent: Some(agent.clone()),
        ..Default::default()
    };
    let rows = ChatHistoryRows {
        epochs: vec![
            // Saved before agents were recorded.
            (0, r#"{"before":null,"continued":false}"#.into()),
            (1, history::meta_json(&meta)),
            // Saved with the previous agent's goal offered after a switch.
            (
                2,
                r#"{"before":"SwitchedToClaude","continued":true,"previous_goal":"tests pass"}"#
                    .into(),
            ),
        ],
        ..Default::default()
    };
    let saved = history::parse(rows);
    assert_eq!(saved.epochs.len(), 3);
    assert_eq!(saved.epochs[0].meta.agent, None);
    assert_eq!(saved.epochs[1].meta, meta);
    assert_eq!(
        saved.epochs[2].meta.before,
        Some(Boundary::SwitchedToClaude)
    );
    assert_eq!(Boundary::SwitchedToCodex.label(), "Switched to Codex");
    // No agent written for histories without one: the stored JSON stays as it was.
    assert!(!history::meta_json(&EpochMeta::default()).contains("agent"));
}

#[test]
fn a_chat_with_a_session_keeps_its_agent() {
    let mut workspace = Workspace::new();
    let tab = workspace.open("chat", "claude");
    let pane = workspace.active().unwrap().focused;
    let chat = |resume: Option<&str>| PaneMetadata {
        kind: PaneKind::Chat,
        cwd: Some("/repo".into()),
        profile_id: Some("claude-default".into()),
        resume_id: resume.map(str::to_owned),
        ..Default::default()
    };
    workspace.set_pane_metadata(tab, pane, chat(None)).unwrap();
    // Before the first message the agent can still change.
    assert!(
        workspace
            .set_chat_account(pane, "codex", "codex-default")
            .unwrap()
    );
    workspace
        .set_pane_metadata(tab, pane, chat(Some("s1")))
        .unwrap();
    assert!(
        workspace
            .set_chat_account(pane, "codex", "codex-default")
            .is_err()
    );
}
