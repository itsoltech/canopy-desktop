#![cfg(unix)]
//! Live Codex checks against the installed CLI (`codex app-server`). Opt-in; uses the
//! signed-in account:
//!
//! ```sh
//! CANOPY_LIVE_CODEX=1 cargo test --locked --test chat_codex_live -- --ignored --nocapture --test-threads=1
//! ```
use canopy_desktop::{
    chat::{
        config::{ChatSettings, Effort, Mode},
        launch::prepare_codex,
        process::{Event, Process},
        session::{ChatSession, CodexStart, Incoming, Response, Status, parse_codex},
        transcript::{Body, Decision, ToolState},
    },
    state::{
        tools::ToolCatalog,
        workspace::{Pane, PaneId, PaneMetadata},
    },
    terminal::environment::ShellEnvironment,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const TURN: Duration = Duration::from_secs(180);

fn enabled() -> bool {
    if std::env::var_os("CANOPY_LIVE_CODEX").is_none() {
        eprintln!("skipped: set CANOPY_LIVE_CODEX=1");
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
    fn start(cwd: &Path, mode: Mode, resume: Option<&str>, fork_at: Option<&str>) -> Self {
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
        assert_eq!(launch.spec.arguments, ["app-server"]);
        let (process, events) =
            Process::start(launch.spec, launch.files, parse_codex, 16 * 1024 * 1024)
                .expect("codex starts");
        let (session, lines) = ChatSession::codex(
            ChatSettings {
                mode,
                effort: Some(Effort::Low),
                ..Default::default()
            },
            CodexStart {
                cwd: cwd.to_owned(),
                resume: resume.map(str::to_owned),
                fork_at: fork_at.map(str::to_owned),
                images: None,
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

    fn turn(&mut self, text: &str) {
        self.pump(|s| *s.status() == Status::Ready);
        let before = self.ends();
        let lines = self.session.submit(text);
        self.write(lines);
        self.pump(|s| ends(s) > before && *s.status() == Status::Ready);
    }

    fn ends(&self) -> usize {
        ends(&self.session)
    }

    fn reply(&self) -> String {
        self.session
            .transcript()
            .items()
            .iter()
            .rev()
            .filter(|item| item.parent.is_none())
            .filter_map(|item| match &item.body {
                Body::Text(block) => Some(block.text.clone()),
                _ => None,
            })
            .next()
            .unwrap_or_default()
    }

    fn finish(mut self) -> String {
        let thread = self.session.session_id().unwrap_or_default().to_owned();
        self.session.close_requested();
        self.process.close().unwrap();
        self.pump(|s| s.status().ended());
        thread
    }
}

fn ends(session: &ChatSession) -> usize {
    session
        .transcript()
        .items()
        .iter()
        .filter(|item| matches!(item.body, Body::TurnEnd(_)))
        .count()
}

fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(dir.path()).unwrap();
    (dir, cwd)
}

#[test]
#[ignore = "live Codex; set CANOPY_LIVE_CODEX=1"]
fn a_conversation_asks_resumes_and_forks() {
    if !enabled() {
        return;
    }
    let (_dir, cwd) = folder();
    let mut live = Live::start(&cwd, Mode::Ask, None, None);
    live.turn("Remember the word PLUM. Reply with exactly: noted");
    assert!(live.reply().to_lowercase().contains("noted"));
    // Ask mode: a write needs approval; it is declined with a note.
    live.pump(|s| *s.status() == Status::Ready);
    let lines = live
        .session
        .submit("Use apply_patch to create denied.txt containing x. Reply OK.");
    live.write(lines);
    live.pump(|s| *s.status() == Status::NeedsDecision);
    let item = live.session.pending_decisions()[0];
    let lines = live
        .session
        .respond(item, Response::Deny(Some("don't create files".into())));
    live.write(lines);
    live.pump(|s| ends(s) >= 2 && *s.status() == Status::Ready);
    assert!(!cwd.join("denied.txt").exists());
    let denied =
        live.session.transcript().items().iter().any(
            |item| matches!(&item.body, Body::Permission(p) if p.decision == Decision::Denied),
        );
    assert!(denied);
    let first_turn = live
        .session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::TurnEnd(end) => end.resume_at.clone(),
            _ => None,
        })
        .next()
        .unwrap();
    let thread = live.finish();
    assert!(!thread.is_empty());

    // A new process continues the saved thread.
    let mut resumed = Live::start(&cwd, Mode::Auto, Some(&thread), None);
    resumed.turn("Which word did I ask you to remember? Reply with the word only.");
    assert!(resumed.reply().to_uppercase().contains("PLUM"));
    assert_eq!(resumed.session.session_id(), Some(thread.as_str()));
    resumed.finish();

    // An edit forks after the first turn: what came later is gone.
    let mut forked = Live::start(&cwd, Mode::Auto, Some(&thread), Some(&first_turn));
    forked.turn("Did I ask you to create denied.txt in this conversation? Reply yes or no.");
    assert!(forked.reply().to_lowercase().contains("no"));
    assert_ne!(forked.session.session_id(), Some(thread.as_str()));
    forked.finish();
}

#[test]
#[ignore = "live Codex; set CANOPY_LIVE_CODEX=1"]
fn auto_mode_runs_commands_and_an_image_reaches_the_model() {
    if !enabled() {
        return;
    }
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    let (_dir, cwd) = folder();
    let images = cwd.join("images");
    let attachment = canopy_desktop::chat::attachments::prepare_image(
        &{
            let mut png = std::io::Cursor::new(vec![]);
            DynamicImage::ImageRgb8(RgbImage::from_pixel(32, 32, Rgb([220, 30, 30])))
                .write_to(&mut png, ImageFormat::Png)
                .unwrap();
            png.into_inner()
        },
        &images,
    )
    .unwrap();
    let mut live = Live::start(&cwd, Mode::Auto, None, None);
    live.turn("Run `ls` in the shell, then create notes.txt containing exactly v1 with apply_patch. Reply OK.");
    let items = live.session.transcript().items();
    assert!(items.iter().any(|item| matches!(&item.body,
        Body::Tool(t) if t.name == "Bash" && t.state == ToolState::Succeeded)));
    assert!(items.iter().any(|item| matches!(&item.body,
        Body::Tool(t) if (t.name == "Write" || t.name == "Edit") && t.state == ToolState::Succeeded)));
    assert_eq!(
        std::fs::read_to_string(cwd.join("notes.txt"))
            .unwrap()
            .trim(),
        "v1"
    );
    live.pump(|s| *s.status() == Status::Ready);
    // Images go by path from the chat's folder.
    let before = live.ends();
    let draft = canopy_desktop::chat::session::Draft {
        text: "What single color fills this image? One English word.".into(),
        attachments: vec![attachment],
        ..Default::default()
    };
    let lines = live.session.submit_draft(draft);
    // This session was opened without an image folder: send the path ourselves.
    let mut lines = lines;
    if let Some(input) = lines[0]["params"]["input"].as_array_mut()
        && input.iter().all(|part| part["type"] != "localImage")
        && let Some(file) = std::fs::read_dir(&images).unwrap().flatten().next()
    {
        input.push(serde_json::json!({"type": "localImage", "path": file.path()}));
    }
    live.write(lines);
    live.pump(|s| ends(s) > before && *s.status() == Status::Ready);
    let reply = live.reply().to_lowercase();
    eprintln!("image: {reply:?}");
    assert!(reply.contains("red"), "{reply:?}");
    assert!(live.session.context().is_some_and(|c| c.max > 0));
    live.finish();
}
