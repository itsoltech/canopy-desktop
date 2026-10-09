#![cfg(unix)]
//! Live switch between agents in one conversation (stage 8): Claude Code → Codex with
//! the conversation so far → back to Claude's own session with what Codex did. Opt-in;
//! uses both signed-in accounts (a tiny haiku turn and low-effort Codex turns):
//!
//! ```sh
//! CANOPY_LIVE_CLAUDE=1 CANOPY_LIVE_CODEX=1 cargo test --locked --test chat_handoff_live -- --ignored --nocapture --test-threads=1
//! ```
use canopy_desktop::{
    chat::{
        config::{ChatSettings, Effort, Mode},
        handoff::{self, Part},
        launch::{prepare_claude, prepare_codex},
        process::{Event, Process},
        session::{Base, ChatSession, CodexStart, Incoming, Status, parse_claude, parse_codex},
        transcript::{Body, Outcome, Transcript},
    },
    state::{
        tools::ToolCatalog,
        workspace::{Pane, PaneId, PaneMetadata},
    },
    terminal::environment::ShellEnvironment,
};
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, Instant},
};

const TURN: Duration = Duration::from_secs(180);

fn enabled() -> bool {
    if std::env::var_os("CANOPY_LIVE_CLAUDE").is_none()
        || std::env::var_os("CANOPY_LIVE_CODEX").is_none()
    {
        eprintln!("skipped: set CANOPY_LIVE_CLAUDE=1 and CANOPY_LIVE_CODEX=1");
        return false;
    }
    true
}

struct Live {
    process: Process,
    events: async_channel::Receiver<Event<Incoming>>,
    session: ChatSession,
}

impl Live {
    fn claude(cwd: &Path, resume: Option<&str>) -> Self {
        let env = ShellEnvironment::load().expect("login shell environment");
        let pane = Pane {
            id: PaneId::new(),
            tool: "claude".into(),
            metadata: PaneMetadata {
                cwd: Some(cwd.to_owned()),
                arguments: vec![
                    "--model".into(),
                    "haiku".into(),
                    "--setting-sources=".into(),
                ],
                resume_id: resume.map(str::to_owned),
                ..Default::default()
            },
        };
        let settings = ChatSettings::default();
        let launch = prepare_claude(&ToolCatalog::default(), &pane, &env, &settings, None).unwrap();
        let (process, events) = Process::start(
            launch.spec,
            launch.files,
            parse_claude,
            canopy_desktop::chat::claude::wire::MAX_LINE_BYTES,
        )
        .expect("claude starts");
        let (mut session, lines) = ChatSession::with_settings(settings);
        if resume.is_some() {
            session.set_base(Base::Continued);
        }
        let live = Self {
            process,
            events,
            session,
        };
        live.write(lines);
        live
    }

    fn codex(cwd: &Path) -> Self {
        let env = ShellEnvironment::load().expect("login shell environment");
        let pane = Pane {
            id: PaneId::new(),
            tool: "codex".into(),
            metadata: PaneMetadata {
                cwd: Some(cwd.to_owned()),
                ..Default::default()
            },
        };
        let launch = prepare_codex(&ToolCatalog::default(), &pane, &env).unwrap();
        let (process, events) = Process::start(
            launch.spec,
            launch.files,
            parse_codex,
            canopy_desktop::chat::codex::wire::MAX_LINE_BYTES,
        )
        .expect("codex starts");
        let (session, lines) = ChatSession::codex(
            ChatSettings {
                mode: Mode::Auto,
                effort: Some(Effort::Low),
                ..Default::default()
            },
            CodexStart {
                cwd: cwd.to_owned(),
                ..Default::default()
            },
        );
        let live = Self {
            process,
            events,
            session,
        };
        live.write(lines);
        live
    }

    fn write(&self, lines: Vec<Value>) {
        for line in lines {
            self.process.send(&line).unwrap();
        }
    }

    fn pump(&mut self, done: impl Fn(&ChatSession) -> bool) {
        let deadline = Instant::now() + TURN;
        while !done(&self.session) {
            assert!(
                Instant::now() < deadline,
                "timed out in {:?}\n{:#?}",
                self.session.status(),
                self.session.transcript().items()
            );
            let Ok(event) = self.events.recv_blocking() else {
                panic!("events closed: {:?}", self.session.status());
            };
            let lines = self.session.on_incoming(event);
            self.write(lines);
        }
    }

    /// Sends `text` once the agent is ready and waits for the turn's end.
    fn turn(&mut self, text: &str) -> String {
        self.pump(|s| *s.status() == Status::Ready);
        let before = completed(&self.session);
        let lines = self.session.submit(text);
        self.write(lines);
        self.pump(|s| completed(s) > before && *s.status() == Status::Ready);
        self.session
            .transcript()
            .items()
            .iter()
            .rev()
            .filter(|item| item.parent.is_none())
            .find_map(|item| match &item.body {
                Body::Text(block) => Some(block.text.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn finish(mut self) -> String {
        let id = self.session.session_id().unwrap_or_default().to_owned();
        self.session.close_requested();
        self.process.close().unwrap();
        self.pump(|s| s.status().ended());
        id
    }
}

fn completed(session: &ChatSession) -> usize {
    session
        .transcript()
        .items()
        .iter()
        .filter(
            |item| matches!(&item.body, Body::TurnEnd(end) if end.outcome == Outcome::Completed),
        )
        .count()
}

#[test]
#[ignore = "live Claude Code and Codex; set CANOPY_LIVE_CLAUDE=1 CANOPY_LIVE_CODEX=1"]
fn claude_codex_and_back_keep_the_conversation() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(dir.path()).unwrap();

    let mut claude = Live::claude(&cwd, None);
    let reply = claude.turn("Remember the word KIWI. Reply with exactly: noted");
    assert!(reply.to_lowercase().contains("noted"), "{reply}");
    let claude_part = Transcript::restore(claude.session.transcript().items().to_vec());
    let claude_session = claude.finish();
    assert!(!claude_session.is_empty());

    // Codex starts fresh with everything so far.
    let mut codex = Live::codex(&cwd);
    let context = handoff::summary(
        &[Part {
            agent: "Claude Code",
            transcript: &claude_part,
        }],
        "Codex",
        false,
    )
    .expect("summary");
    codex.session.set_handoff(Some(context));
    let reply = codex.turn(
        "Which word did I ask you to remember? Also create the file codex.txt containing \
         exactly `codex` with apply_patch. Reply with the word only.",
    );
    assert!(reply.to_uppercase().contains("KIWI"), "{reply}");
    assert!(cwd.join("codex.txt").exists());
    let codex_part = Transcript::restore(codex.session.transcript().items().to_vec());
    codex.finish();

    // Claude continues its own session and learns only what Codex did.
    let mut claude = Live::claude(&cwd, Some(&claude_session));
    let since = handoff::summary(
        &[Part {
            agent: "Codex",
            transcript: &codex_part,
        }],
        "Claude Code",
        true,
    )
    .expect("delta");
    assert!(!since.contains("Remember the word KIWI"));
    claude.session.set_handoff(Some(since));
    let reply = claude.turn(
        "Which file did the other agent create, and which word did I ask you to remember? \
         Answer as: <file> <word>",
    );
    assert_eq!(claude.session.session_id(), Some(claude_session.as_str()));
    assert!(reply.contains("codex.txt"), "{reply}");
    assert!(reply.to_uppercase().contains("KIWI"), "{reply}");
    claude.finish();
}
