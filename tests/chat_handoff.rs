//! Stage 8: one conversation, several agents. The conversation so far goes to the agent
//! switched to with the next message (both wires), each saved session records who ran
//! it, and a chat pane moves to another agent with the session to resume.
use canopy_desktop::chat::{
    claude::wire as claude_wire,
    codex::wire as codex_wire,
    config::{ChatSettings, Mode},
    handoff::{self, Part},
    history::{self, AgentRef, Boundary, EpochMeta},
    session::{ChatSession, Draft},
    transcript::{Body, Transcript},
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

fn user_messages(transcript: &Transcript) -> Vec<(String, Option<String>)> {
    transcript
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::User(message) => Some((message.text.clone(), message.context.clone())),
            _ => None,
        })
        .collect()
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
fn the_waiting_context_goes_with_the_next_message_only() {
    let mut session = ready();
    session.set_handoff(Some("<canopy-context>KIWI</canopy-context>".into()));
    let lines = session.submit("Which word?");
    let sent = lines
        .iter()
        .find(|line| line["type"] == "user")
        .expect("user message");
    assert_eq!(
        sent["message"]["content"][0]["text"],
        "<canopy-context>KIWI</canopy-context>"
    );
    assert_eq!(session.handoff(), None);
    finish(&mut session, "KIWI");
    let lines = session.submit("Thanks");
    let sent = lines.iter().find(|line| line["type"] == "user").unwrap();
    assert_eq!(sent["message"]["content"].as_array().unwrap().len(), 1);
    assert_eq!(
        user_messages(session.transcript()),
        vec![
            (
                "Which word?".into(),
                Some("<canopy-context>KIWI</canopy-context>".into())
            ),
            ("Thanks".into(), None),
        ]
    );
}

#[test]
fn a_queued_message_takes_the_context_and_retry_sends_it_again() {
    let mut session = ready();
    // Restored after a switch: waits for Resume, then goes with the context.
    session.restore_queue(vec![Draft::text("Continue")]);
    session.set_handoff(Some("ctx".into()));
    assert!(session.queue_held());
    let lines = session.resume_queue();
    let sent = lines.iter().find(|line| line["type"] == "user").unwrap();
    assert_eq!(sent["message"]["content"][0]["text"], "ctx");
    finish(&mut session, "done");
    let retry = session.retry_draft().expect("retry");
    assert_eq!(retry.context.as_deref(), Some("ctx"));
    // An explicit context of a message wins over a waiting one.
    session.set_handoff(Some("newer".into()));
    let lines = session.submit_draft(Draft {
        text: "again".into(),
        context: Some("own".into()),
        ..Default::default()
    });
    let sent = lines.iter().find(|line| line["type"] == "user").unwrap();
    assert_eq!(sent["message"]["content"][0]["text"], "own");
    assert_eq!(session.handoff(), Some("newer"));
}

#[test]
fn codex_sends_the_context_in_its_turn_input() {
    let (mut session, lines) = ChatSession::codex(
        ChatSettings {
            mode: Mode::Auto,
            ..Default::default()
        },
        Default::default(),
    );
    let id = lines[0]["id"].as_str().unwrap().to_owned();
    let lines = session.on_codex(
        codex_wire::parse_line(
            json!({"id": id, "result": {"userAgent": "x"}})
                .to_string()
                .as_bytes(),
        )
        .unwrap()
        .unwrap(),
    );
    let thread = lines[1]["id"].as_str().unwrap().to_owned();
    session.on_codex(
        codex_wire::parse_line(
            json!({"id": thread, "result": {"thread": {"id": "t1"}}})
                .to_string()
                .as_bytes(),
        )
        .unwrap()
        .unwrap(),
    );
    session.set_handoff(Some("from Claude".into()));
    let lines = session.submit("Which word?");
    let start = lines
        .iter()
        .find(|line| line["method"] == "turn/start")
        .expect("turn/start");
    assert_eq!(start["params"]["input"][0]["text"], "from Claude");
    assert_eq!(start["params"]["input"][1]["text"], "Which word?");
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
        before: Some(Boundary::switched_to(
            canopy_desktop::chat::provider::Provider::Codex,
        )),
        continued: false,
        agent: Some(agent.clone()),
    };
    let rows = ChatHistoryRows {
        epochs: vec![
            // Saved before agents were recorded.
            (0, r#"{"before":null,"continued":false}"#.into()),
            (1, history::meta_json(&meta)),
        ],
        ..Default::default()
    };
    let saved = history::parse(rows);
    assert_eq!(saved.epochs.len(), 2);
    assert_eq!(saved.epochs[0].meta.agent, None);
    assert_eq!(saved.epochs[1].meta, meta);
    assert_eq!(Boundary::SwitchedToCodex.label(), "Switched to Codex");
    // No agent written for histories without one: the stored JSON stays as it was.
    assert!(!history::meta_json(&EpochMeta::default()).contains("agent"));
}

#[test]
fn a_return_gets_only_what_happened_since() {
    let mut claude = ready();
    claude.submit("Remember KIWI");
    finish(&mut claude, "Noted KIWI.");
    let mut codex = ready();
    codex.submit("Create a.txt");
    finish(&mut codex, "Created a.txt.");
    let since = handoff::summary(
        &[Part {
            agent: "Codex",
            transcript: codex.transcript(),
        }],
        "Claude Code",
        true,
    )
    .unwrap();
    assert!(since.contains("Created a.txt.") && !since.contains("KIWI"));
    let full = handoff::summary(
        &[
            Part {
                agent: "Claude Code",
                transcript: claude.transcript(),
            },
            Part {
                agent: "Codex",
                transcript: codex.transcript(),
            },
        ],
        "Claude Code",
        false,
    )
    .unwrap();
    assert!(full.contains("### Claude Code\nNoted KIWI.") && full.contains("Created a.txt."));
}

#[test]
fn a_chat_pane_moves_to_another_agent_with_its_session() {
    let mut workspace = Workspace::new();
    let tab = workspace.open("chat", "claude");
    let pane = workspace.active().unwrap().focused;
    workspace
        .set_pane_metadata(
            tab,
            pane,
            PaneMetadata {
                kind: PaneKind::Chat,
                cwd: Some("/repo".into()),
                profile_id: Some("claude-default".into()),
                resume_id: Some("s1".into()),
                ..Default::default()
            },
        )
        .unwrap();
    // Before a switch the account can't change under a saved session.
    assert!(
        workspace
            .set_chat_account(pane, "codex", "codex-default")
            .is_err()
    );
    assert!(
        workspace
            .switch_chat_agent(pane, "codex", "codex-default", None)
            .unwrap()
    );
    let moved = workspace.all_panes()[0].clone();
    assert_eq!(moved.tool, "codex");
    assert_eq!(moved.metadata.profile_id.as_deref(), Some("codex-default"));
    assert_eq!(moved.metadata.resume_id, None);
    assert!(
        !workspace
            .switch_chat_agent(pane, "codex", "codex-default", None)
            .unwrap()
    );
    workspace
        .switch_chat_agent(pane, "claude", "claude-default", Some("s1"))
        .unwrap();
    assert_eq!(
        workspace.all_panes()[0].metadata.resume_id.as_deref(),
        Some("s1")
    );
    assert!(
        workspace
            .switch_chat_agent(pane, "shell", "x", None)
            .is_err()
    );
}
