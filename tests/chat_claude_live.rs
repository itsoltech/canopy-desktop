#![cfg(unix)]
//! Live Claude Code checks against the installed CLI. Opt-in and paid (tiny haiku turns):
//!
//! ```sh
//! CANOPY_LIVE_CLAUDE=1 cargo test --locked --test chat_claude_live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `CANOPY_RECORD_FIXTURES=<dir>` also writes each scenario's redacted stdout as
//! `<dir>/<scenario>.jsonl` (account data, home and temp paths removed).
//! Every scenario runs in a fresh temp directory with user/project settings disabled.
use canopy_desktop::{
    agents::{launch as hook_launch, relay::Relay},
    chat::{
        claude::wire::{self, Inbound},
        config::{ChatSettings, Effort, Mode},
        launch::{HookConfig, prepare_claude},
        process::{Event, Process},
        session::{ChatSession, Status},
        transcript::{Body, Decision, Delivery, Outcome, ToolState},
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

const TURN: Duration = Duration::from_secs(120);

fn enabled() -> bool {
    if std::env::var_os("CANOPY_LIVE_CLAUDE").is_none() {
        eprintln!("skipped: set CANOPY_LIVE_CLAUDE=1");
        return false;
    }
    true
}

fn parse(line: &[u8]) -> Result<Option<(String, Inbound)>, String> {
    Ok(
        wire::parse_line(line)?
            .map(|message| (String::from_utf8_lossy(line).into_owned(), message)),
    )
}

struct Live {
    name: &'static str,
    dir: tempfile::TempDir,
    process: Process,
    events: async_channel::Receiver<Event<(String, Inbound)>>,
    session: ChatSession,
    /// Redacted stdout, appended as it arrives so a failing scenario still leaves evidence.
    recorder: Option<std::fs::File>,
    cwd: PathBuf,
    started: Instant,
}

impl Live {
    fn start(name: &'static str, dir: Option<tempfile::TempDir>, resume: Option<&str>) -> Self {
        Self::start_with(name, dir, resume, None)
    }

    fn start_model(name: &'static str, model: &str) -> Self {
        Self::start_full(name, None, None, None, model)
    }

    fn start_with(
        name: &'static str,
        dir: Option<tempfile::TempDir>,
        resume: Option<&str>,
        hooks: Option<HookConfig<'_>>,
    ) -> Self {
        Self::start_full(name, dir, resume, hooks, "haiku")
    }

    fn start_full(
        name: &'static str,
        dir: Option<tempfile::TempDir>,
        resume: Option<&str>,
        hooks: Option<HookConfig<'_>>,
        model: &str,
    ) -> Self {
        let dir = dir.unwrap_or_else(|| tempfile::tempdir().unwrap());
        std::fs::write(dir.path().join("note.txt"), "canopy fixture note\n").unwrap();
        let env = ShellEnvironment::load().expect("login shell environment");
        let pane = Pane {
            id: PaneId::new(),
            tool: "claude".into(),
            metadata: PaneMetadata {
                cwd: Some(std::fs::canonicalize(dir.path()).unwrap()),
                arguments: vec!["--model".into(), model.into(), "--setting-sources=".into()],
                resume_id: resume.map(str::to_owned),
                ..Default::default()
            },
        };
        let launch = prepare_claude(
            &ToolCatalog::default(),
            &pane,
            &env,
            &ChatSettings::default(),
            hooks,
        )
        .unwrap();
        let (process, events) =
            Process::start(launch.spec, launch.files, parse, wire::MAX_LINE_BYTES)
                .expect("claude starts");
        let (session, lines) = ChatSession::new();
        let recorder = std::env::var_os("CANOPY_RECORD_FIXTURES").map(|dir| {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::File::create(dir.join(format!("{name}.jsonl"))).unwrap()
        });
        let cwd = std::fs::canonicalize(dir.path()).unwrap();
        let live = Self {
            name,
            dir,
            process,
            events,
            session,
            recorder,
            cwd,
            started: Instant::now(),
        };
        live.write(lines);
        live
    }

    fn write(&self, lines: Vec<Value>) {
        for line in lines {
            self.process.send(&line).unwrap();
        }
    }

    fn submit(&mut self, text: &str) {
        let lines = self.session.submit(text);
        self.write(lines);
    }

    /// Applies process events until `done` holds; fails on timeout.
    fn pump(&mut self, timeout: Duration, done: impl Fn(&ChatSession) -> bool) {
        let deadline = Instant::now() + timeout;
        while !done(&self.session) {
            assert!(
                Instant::now() < deadline,
                "{}: timed out in status {:?}\n{:#?}",
                self.name,
                self.session.status(),
                self.session.transcript().items()
            );
            let Ok(event) = self.events.try_recv() else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            let event = match event {
                Event::Message((raw, message)) => {
                    if let Some(file) = &mut self.recorder {
                        use std::io::Write;
                        writeln!(file, "{}", redact(&raw, &self.cwd)).unwrap();
                    }
                    Event::Message(message)
                }
                Event::Diagnostic(text) => Event::Diagnostic(text),
                Event::Exited(exit) => Event::Exited(exit),
            };
            let lines = self.session.on_event(event);
            self.write(lines);
        }
    }

    fn turn_ended(&self) -> bool {
        self.session
            .transcript()
            .items()
            .last()
            .is_some_and(|item| matches!(item.body, Body::TurnEnd(_)))
    }

    fn pending_permission(&self) -> Option<(canopy_desktop::chat::transcript::ItemId, String)> {
        self.session
            .transcript()
            .items()
            .iter()
            .find_map(|item| match &item.body {
                Body::Permission(p) if p.decision == Decision::Pending => {
                    Some((item.id, p.tool.clone()))
                }
                _ => None,
            })
    }

    fn assistant_text(&self) -> String {
        self.session
            .transcript()
            .items()
            .iter()
            .filter_map(|item| match &item.body {
                Body::Text(block) if item.parent.is_none() => Some(block.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn finish(mut self) -> String {
        self.session.close_requested();
        self.process.close().unwrap();
        self.pump(Duration::from_secs(10), |s| s.status().ended());
        assert!(
            matches!(self.session.status(), Status::Exited { .. }),
            "{}: closing is not a failure: {:?}",
            self.name,
            self.session.status()
        );
        let session = self.session.session_id().unwrap_or_default().to_owned();
        eprintln!(
            "{}: {:?} in {:.1}s, session {session}, diagnostics {:?}",
            self.name,
            self.session.status(),
            self.started.elapsed().as_secs_f64(),
            self.session.diagnostics().collect::<Vec<_>>()
        );
        session
    }
}

/// Keeps only protocol-relevant data in recorded fixtures.
fn redact(line: &str, cwd: &Path) -> String {
    let mut value: Value = serde_json::from_str(line).unwrap();
    if value["type"] == "control_response" {
        if let Some(response) = value["response"]["response"].as_object_mut() {
            response.retain(|key, _| {
                matches!(
                    key.as_str(),
                    "current_permission_mode" | "session_state" | "models"
                )
            });
        }
        if let Some(response) = value["response"].as_object_mut() {
            response.remove("pending_permission_requests");
            response.remove("pending_user_dialog_requests");
        }
    }
    if value["type"] == "system" && value["subtype"] == "init" {
        value.as_object_mut().unwrap().retain(|key, _| {
            matches!(
                key.as_str(),
                "type"
                    | "subtype"
                    | "session_id"
                    | "uuid"
                    | "model"
                    | "permissionMode"
                    | "claude_code_version"
                    | "capabilities"
                    | "cwd"
                    | "tools"
            )
        });
    }
    let mut text = value.to_string();
    let home = std::env::var("HOME").unwrap_or_default();
    for (from, to) in [
        (
            cwd.to_string_lossy().into_owned(),
            "/tmp/canopy-fixture".to_owned(),
        ),
        (home, "/Users/user".to_owned()),
    ] {
        if !from.is_empty() {
            text = text.replace(&from, &to);
        }
    }
    text
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn simple_reply_streams_and_ends_the_turn() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("simple", None, None);
    live.pump(TURN, |s| *s.status() == Status::Ready);
    live.submit("Reply with exactly: ok");
    live.pump(TURN, |s| {
        *s.status() == Status::Ready && s.transcript().items().len() > 1
    });
    assert!(live.turn_ended());
    assert!(live.assistant_text().to_lowercase().contains("ok"));
    assert!(matches!(
        live.session.transcript().items()[0].body,
        Body::User(ref m) if m.delivery == Delivery::Sent
    ));
    live.finish();
}

fn tool_permission(name: &'static str, prompt: &str, allow: bool) {
    let mut live = Live::start(name, None, None);
    live.submit(prompt);
    live.pump(TURN, |s| *s.status() == Status::NeedsDecision);
    let (item, tool) = live.pending_permission().expect("permission request");
    eprintln!("{name}: permission for {tool}");
    let lines = live.session.decide(item, allow);
    assert_eq!(lines.len(), 1);
    assert!(
        live.session.decide(item, allow).is_empty(),
        "second decision is ignored"
    );
    live.write(lines);
    live.pump(TURN, |s| {
        *s.status() == Status::Ready && s.transcript().items().len() > 2
    });
    let tool = live
        .session
        .transcript()
        .items()
        .iter()
        .find_map(|item| match &item.body {
            Body::Tool(call) if call.name == tool => Some(call.clone()),
            _ => None,
        })
        .expect("tool call");
    eprintln!(
        "{name}: tool {:?} output {:?}",
        tool.state,
        tool.output.as_ref().map(|o| &o.text)
    );
    if allow {
        assert_eq!(tool.state, ToolState::Succeeded);
    } else {
        assert_eq!(tool.state, ToolState::Failed);
    }
    eprintln!("{name}: reply {:?}", live.assistant_text());
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn bash_allow_runs_the_tool() {
    if enabled() {
        tool_permission(
            "bash-allow",
            "Use the Bash tool to run exactly `touch canopy-allow.txt`, then reply with: done",
            true,
        );
    }
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn bash_deny_reports_a_failed_tool() {
    if enabled() {
        tool_permission(
            "bash-deny",
            "Use the Bash tool to run exactly `mkdir canopy-deny`. If it is denied, reply with: denied",
            false,
        );
    }
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn write_allow_reports_a_structured_result() {
    if !enabled() {
        return;
    }
    tool_permission(
        "write-allow",
        "Use the Write tool to create created.txt containing the single word hello. Then reply: done",
        true,
    );
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn interrupt_keeps_partial_text_and_marks_the_turn() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("interrupt", None, None);
    live.submit("Count from 1 to 300, one number per line, no other text.");
    live.pump(TURN, |s| {
        s.transcript()
            .items()
            .iter()
            .any(|item| matches!(&item.body, Body::Text(b) if b.text.len() > 20))
    });
    let lines = live.session.interrupt();
    assert!(!lines.is_empty());
    live.write(lines);
    live.pump(TURN, |s| *s.status() == Status::Ready);
    let end = live
        .session
        .transcript()
        .items()
        .iter()
        .rev()
        .find_map(|item| match &item.body {
            Body::TurnEnd(end) => Some(end.outcome.clone()),
            _ => None,
        });
    eprintln!(
        "interrupt: outcome {end:?}, partial {} chars",
        live.assistant_text().len()
    );
    assert_eq!(end, Some(Outcome::Interrupted));
    assert!(!live.assistant_text().is_empty());
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn queued_message_waits_for_the_running_turn() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("queued", None, None);
    live.submit("Reply with exactly: first");
    live.pump(TURN, |s| *s.status() == Status::Working);
    live.submit("Reply with exactly: second");
    assert_eq!(live.session.queue().len(), 1, "held in Canopy");
    live.pump(TURN, |s| {
        *s.status() == Status::Ready
            && s.queue().is_empty()
            && s.transcript()
                .items()
                .iter()
                .filter(|item| matches!(item.body, Body::TurnEnd(_)))
                .count()
                >= 2
    });
    let users: Vec<_> = live
        .session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::User(m) => Some((item.turn.0, m.text.clone(), m.delivery)),
            _ => None,
        })
        .collect();
    eprintln!("queued: users {users:?}, reply {:?}", live.assistant_text());
    assert_eq!(users.len(), 2);
    assert_ne!(
        users[0].0, users[1].0,
        "the queued message has its own turn"
    );
    assert!(users.iter().all(|u| u.2 == Delivery::Sent));
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn steer_is_read_after_the_running_tool() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("steer", None, None);
    live.pump(TURN, |s| *s.status() == Status::Ready);
    let lines = live.session.set_mode(Mode::Yolo);
    live.write(lines);
    live.submit(
        "Use the Bash tool to run exactly `sleep 4; echo first`. Then reply with its output and follow any later instruction from me.",
    );
    let running = |s: &ChatSession| {
        s.transcript().items().iter().any(
            |item| matches!(&item.body, Body::Tool(t) if t.name == "Bash" && t.state == ToolState::Running),
        )
    };
    live.pump(TURN, running);
    let lines = live
        .session
        .steer_text("Also include the word STEER in your reply.");
    live.write(lines);
    live.pump(TURN, |s| {
        *s.status() == Status::Ready
            && s.queue().is_empty()
            && s.transcript()
                .items()
                .iter()
                .any(|item| matches!(item.body, Body::TurnEnd(_)))
    });
    let items = live.session.transcript().items();
    let steered = items
        .iter()
        .position(|item| matches!(&item.body, Body::User(m) if m.delivery == Delivery::Steered));
    let bash = items
        .iter()
        .position(|item| matches!(&item.body, Body::Tool(t) if t.name == "Bash"));
    let turns: std::collections::HashSet<_> = items.iter().map(|item| item.turn).collect();
    eprintln!(
        "steer: steered at {steered:?}, bash at {bash:?}, turns {}, reply {:?}",
        turns.len(),
        live.assistant_text()
    );
    assert!(
        steered > bash,
        "the steer is shown after the tool it followed"
    );
    assert_eq!(turns.len(), 1, "one turn: the steer was folded in");
    assert!(live.assistant_text().contains("STEER"));
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn subagent_output_is_nested_under_its_tool_call() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("subagent", None, None);
    live.submit(
        "Use the Agent tool with a general-purpose subagent to read note.txt and report its contents. Wait for its result, then reply with the contents only.",
    );
    let agent_done = |s: &ChatSession| {
        s.transcript().items().iter().any(
            |item| matches!(&item.body, Body::Tool(t) if t.name == "Agent" && t.state.finished()),
        )
    };
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
        live.pump(TURN, |s| {
            s.status() == &Status::NeedsDecision || (s.status() == &Status::Ready && agent_done(s))
        });
        match live.pending_permission() {
            Some((item, _)) => {
                let lines = live.session.decide(item, true);
                live.write(lines);
            }
            None if agent_done(&live.session) => break,
            None => assert!(Instant::now() < deadline, "subagent did not finish"),
        }
    }
    // The agent resumes on its own after the background subagent reports.
    live.pump(TURN, |s| {
        *s.status() == Status::Ready
            && s.transcript()
                .items()
                .iter()
                .filter(|item| matches!(item.body, Body::TurnEnd(_)))
                .count()
                >= 2
    });
    let nested = live
        .session
        .transcript()
        .items()
        .iter()
        .filter(|item| item.parent.is_some())
        .count();
    eprintln!(
        "subagent: nested items {nested}, reply {:?}",
        live.assistant_text()
    );
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn resume_continues_the_exact_session() {
    if !enabled() {
        return;
    }
    let mut first = Live::start("resume-first", None, None);
    first.submit("Remember the word: pelican. Reply with exactly: noted");
    first.pump(TURN, |s| {
        *s.status() == Status::Ready && s.transcript().items().len() > 1
    });
    let (first, session) = first.finish_keep();
    assert!(!session.is_empty());
    let first_dir = first.dir;
    let mut second = Live::start("resume-second", Some(first_dir), Some(&session));
    second.submit("Which word did I ask you to remember? Reply with the word only.");
    second.pump(TURN, |s| {
        *s.status() == Status::Ready && s.transcript().items().len() > 1
    });
    assert_eq!(second.session.session_id(), Some(session.as_str()));
    assert!(second.assistant_text().to_lowercase().contains("pelican"));
    second.finish();
}

impl Live {
    /// Like `finish`, but returns the live state so the directory can be reused.
    fn finish_keep(mut self) -> (Self, String) {
        self.session.close_requested();
        self.process.close().unwrap();
        self.pump(Duration::from_secs(10), |s| s.status().ended());
        let session = self.session.session_id().unwrap_or_default().to_owned();
        (self, session)
    }
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn canopy_hooks_arrive_alongside_the_protocol() {
    if !enabled() {
        return;
    }
    let relay = Relay::start().unwrap();
    let registration = relay.register();
    let helper = PathBuf::from(env!("CARGO_BIN_EXE_canopy-agent-hook"));
    hook_launch::validate_helper(&helper).unwrap();
    let started = Instant::now();
    let mut live = Live::start_with(
        "hooks",
        None,
        None,
        Some(HookConfig {
            helper: &helper,
            registration: &registration,
        }),
    );
    live.pump(TURN, |s| *s.status() == Status::Ready);
    std::thread::sleep(Duration::from_secs(2));
    let before: Vec<_> = std::iter::from_fn(|| relay.events.try_recv().ok())
        .map(|e| e.name)
        .collect();
    let submitted = started.elapsed();
    live.submit("Reply with exactly: ok");
    live.pump(TURN, |s| {
        *s.status() == Status::Ready && s.transcript().items().len() > 1
    });
    std::thread::sleep(Duration::from_secs(2));
    let after: Vec<_> = std::iter::from_fn(|| relay.events.try_recv().ok())
        .map(|e| (e.name, e.session))
        .collect();
    eprintln!(
        "hooks: before first message (ready, +2s): {before:?}; first message at {:.1}s; after: {after:?}",
        submitted.as_secs_f64()
    );
    let session = live.session.session_id().map(str::to_owned);
    assert!(after.iter().all(|(_, s)| s.is_none() || *s == session));
    assert!(!after.is_empty(), "hooks arrive after the first message");
    live.finish();
    relay.shutdown();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn models_and_settings_change_during_the_session() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("settings", None, None);
    live.pump(TURN, |s| *s.status() == Status::Ready);
    let models = live.session.models().to_vec();
    eprintln!(
        "settings: models {:?}",
        models
            .iter()
            .map(|m| (&m.value, &m.display_name, &m.effort_levels, m.supports_auto))
            .collect::<Vec<_>>()
    );
    assert!(!models.is_empty(), "initialize lists models");
    let effort_model = models.iter().find(|m| m.supports_effort()).cloned();
    let mut lines = live.session.set_mode(Mode::Yolo);
    if let Some(model) = &effort_model {
        lines.extend(live.session.set_model(Some(model.value.clone())));
        lines.extend(
            live.session
                .set_effort(model.effort_levels.first().copied()),
        );
    }
    live.write(lines);
    live.submit("Use the Bash tool to run exactly `mkdir canopy-yolo`, then reply with: done");
    live.pump(TURN, |s| {
        *s.status() == Status::Ready && s.transcript().items().len() > 2
    });
    let notices: Vec<_> = live
        .session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Notice(n) => Some(n.text.clone()),
            _ => None,
        })
        .collect();
    eprintln!(
        "settings: notices {notices:?}, settings {:?}",
        live.session.settings()
    );
    assert!(notices.is_empty(), "every setting was accepted");
    assert!(
        live.pending_permission().is_none()
            && !live
                .session
                .transcript()
                .items()
                .iter()
                .any(|item| matches!(item.body, Body::Permission(_))),
        "Yolo runs Bash without a prompt"
    );
    assert!(live.dir.path().join("canopy-yolo").is_dir());
    assert_eq!(live.session.settings().mode, Mode::Yolo);
    let _ = Effort::Max;
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn parallel_background_subagents_finish_cleanly() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("parallel-subagents", None, None);
    live.pump(TURN, |s| *s.status() == Status::Ready);
    let lines = live.session.set_mode(Mode::Yolo);
    live.write(lines);
    live.submit(
        "Launch TWO Explore subagents in parallel in the background (run_in_background true). A: run `ls -la /etc | head -20` with Bash, then read /etc/shells and report its line count. B: list /usr/share with Bash, read /usr/share/dict/README and report its first line. Wait for both, then reply with one line per agent.",
    );
    let done = |s: &ChatSession| {
        let items = s.transcript().items();
        let agents: Vec<_> = items
            .iter()
            .filter_map(|item| match &item.body {
                Body::Tool(t) if t.name == "Agent" => Some(t),
                _ => None,
            })
            .collect();
        *s.status() == Status::Ready
            && s.background_tasks() == 0
            && agents.len() >= 2
            && agents.iter().all(|t| t.state.finished())
    };
    live.pump(Duration::from_secs(300), done);
    let items = live.session.transcript().items();
    let open: Vec<_> = items
        .iter()
        .filter_map(|item| match &item.body {
            Body::Tool(t) if !t.state.finished() => Some(t.name.clone()),
            _ => None,
        })
        .collect();
    let reports: Vec<_> = items
        .iter()
        .filter_map(|item| match &item.body {
            Body::Tool(t) if t.name == "Agent" => t.output.as_ref().map(|o| o.text.clone()),
            _ => None,
        })
        .collect();
    eprintln!("parallel: open tools {open:?}, reports {reports:?}");
    assert!(open.is_empty(), "no subagent tool is left running");
    assert!(
        reports
            .iter()
            .all(|r| !r.trim().is_empty() && !r.contains("SubagentHandback")),
        "reports are the subagents' own text"
    );
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn tool_results_carry_patches_and_contents() {
    if !enabled() {
        return;
    }
    let mut live = Live::start("tools", None, None);
    std::fs::write(
        live.dir.path().join("a.txt"),
        "line one\nline two\nline three\n",
    )
    .unwrap();
    live.pump(TURN, |s| *s.status() == Status::Ready);
    let lines = live.session.set_mode(Mode::Yolo);
    live.write(lines);
    live.submit(
        "Do these steps one tool call at a time, without explaining: 1) Read a.txt. 2) Edit a.txt replacing 'line two' with 'line 2'. 3) Write a new file b.txt containing two lines. 4) Write a.txt again with the single line 'rewritten'. 5) Bash: run `ls`. 6) Bash: run `ls missing-dir` (it fails). Then reply 'done'.",
    );
    live.pump(TURN, |s| {
        *s.status() == Status::Ready
            && s.transcript()
                .items()
                .iter()
                .any(|item| matches!(item.body, Body::TurnEnd(_)))
    });
    let tools: Vec<_> = live
        .session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Tool(t) => Some((
                t.name.clone(),
                t.output
                    .as_ref()
                    .and_then(|o| o.structured.clone())
                    .is_some(),
            )),
            _ => None,
        })
        .collect();
    eprintln!("tools: {tools:?}");
    for name in ["Read", "Edit", "Write", "Bash"] {
        assert!(
            tools.iter().any(|(n, structured)| n == name && *structured),
            "{name} result is structured"
        );
    }
    live.finish();
}

#[test]
#[ignore = "live Claude Code; set CANOPY_LIVE_CLAUDE=1"]
fn thinking_summaries_stream_into_the_transcript() {
    if !enabled() {
        return;
    }
    let mut live = Live::start_model("thinking", "opus");
    live.submit(
        "Carefully reason step by step: how many distinct ways can you tile a 2x10 board with 1x2 dominoes? Reply with the number only.",
    );
    live.pump(TURN, |s| {
        *s.status() == Status::Ready
            && s.transcript()
                .items()
                .iter()
                .any(|item| matches!(item.body, Body::TurnEnd(_)))
    });
    let thinking: String = live
        .session
        .transcript()
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Thinking(block) => Some(block.text.as_str()),
            _ => None,
        })
        .collect();
    eprintln!("thinking: {thinking:?}, reply {:?}", live.assistant_text());
    assert!(
        !thinking.trim().is_empty(),
        "a summary of the reasoning arrives"
    );
    live.finish();
}
