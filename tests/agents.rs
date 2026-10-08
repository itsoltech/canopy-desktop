use canopy_desktop::{
    agents::{
        Event, Status, launch,
        relay::{Registration, Relay, forward_event},
    },
    state::{
        tools::ToolCatalog,
        workspace::{PaneMetadata, Workspace},
    },
    terminal::environment::ShellEnvironment,
};
use std::{
    collections::HashMap,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn send(reg: &Registration, token: &str, session: &str) -> Result<(), String> {
    let mut registration = reg.clone();
    registration.token = token.into();
    forward_event(
        &registration,
        &serde_json::json!({"hook_event_name":"SessionStart","session_id":session}),
    )
}
fn receive(relay: &Relay) -> Event {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(event) = relay.events.try_recv() {
            return event;
        }
        assert!(Instant::now() < deadline, "missing event");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn relay_authenticates_routes_and_rejects_retired_runs() {
    let relay = Relay::start().unwrap();
    let a = relay.register();
    let b = relay.register();
    assert_ne!(a.run, b.run);
    assert_ne!(a.token, b.token);
    assert!(send(&a, "wrong", "forged").is_err());
    send(&a, &a.token, "session-a").unwrap();
    let event = receive(&relay);
    assert_eq!(event.run, a.run);
    assert_eq!(event.session.as_deref(), Some("session-a"));
    relay.remove(&a.run);
    assert!(send(&a, &a.token, "late").is_err());
    send(&b, &b.token, "session-b").unwrap();
    assert_eq!(receive(&relay).run, b.run);
    let endpoint = a.endpoint.clone();
    relay.shutdown();
    if let canopy_desktop::agents::relay::Endpoint::UnixSocket(socket) = endpoint {
        assert!(!socket.exists());
    }
}

#[test]
fn relay_accepts_parallel_reconnects_and_rejects_oversized_events() {
    let relay = Relay::start().unwrap();
    let registration = relay.register();
    let joins: Vec<_> = (0..4)
        .map(|index| {
            let registration = registration.clone();
            std::thread::spawn(move || {
                forward_event(
                    &registration,
                    &serde_json::json!({
                        "hook_event_name": "SessionStart",
                        "session_id": format!("parallel-{index}")
                    }),
                )
            })
        })
        .collect();
    for join in joins {
        join.join().unwrap().unwrap();
    }
    let mut sessions: Vec<_> = (0..4).map(|_| receive(&relay).session.unwrap()).collect();
    sessions.sort();
    assert_eq!(
        sessions,
        ["parallel-0", "parallel-1", "parallel-2", "parallel-3"]
    );
    assert!(
        forward_event(
            &registration,
            &serde_json::json!({"payload": "x".repeat(1_048_576)})
        )
        .is_err()
    );
    relay.shutdown();
}

#[test]
fn packaged_hook_helper_forwards_stdin_without_stdout_protocol() {
    launch::validate_helper(std::path::Path::new(env!(
        "CARGO_BIN_EXE_canopy-agent-hook"
    )))
    .unwrap();
    let relay = Relay::start().unwrap();
    let registration = relay.register();
    let mut env = ShellEnvironment {
        shell: std::env::current_exe().unwrap(),
        vars: HashMap::new(),
    };
    launch::environment(&mut env, &registration);
    let mut child = Command::new(env!("CARGO_BIN_EXE_canopy-agent-hook"))
        .envs(env.vars)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::json!({
                "hook_event_name": "SessionStart",
                "session_id": "helper-session"
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(receive(&relay).session.as_deref(), Some("helper-session"));
    relay.shutdown();
}

#[cfg(windows)]
#[test]
fn codex_windows_command_executes_the_helper_and_keeps_provider_stdout_empty() {
    use std::os::windows::process::CommandExt;

    let helper = std::path::Path::new(env!("CARGO_BIN_EXE_canopy-agent-hook"));
    let relay = Relay::start().unwrap();
    let registration = relay.register();
    let mut catalog = ToolCatalog::default();
    let mut workspace = Workspace::new();
    workspace.open("Codex", "codex");
    catalog.bind_default(&mut workspace);
    let mut pane = workspace.activation_plan()[0].clone();
    launch::augment(&mut catalog, &mut pane, helper).unwrap();
    let profile = &catalog.get("codex").unwrap().profiles[0];
    let settings: serde_json::Value =
        serde_json::from_str(&profile.settings.settings_json).unwrap();
    let command = settings["hooks"]["Stop"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["hooks"][0]["commandWindows"]
        .as_str()
        .unwrap();
    let mut environment = ShellEnvironment {
        shell: std::env::current_exe().unwrap(),
        vars: HashMap::new(),
    };
    launch::environment(&mut environment, &registration);
    for shell in ["cmd", "powershell"] {
        let mut invocation = if shell == "cmd" {
            let mut command_line = Command::new(std::env::var_os("ComSpec").expect("ComSpec"));
            command_line.args(["/D", "/S", "/C"]);
            // Match Codex 0.154.0's command runner rather than std's argv quoting.
            command_line.raw_arg(format!(r#""{command}""#));
            command_line
        } else {
            let mut command_line = Command::new("powershell.exe");
            command_line.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"]);
            command_line.arg(command);
            command_line
        };
        let mut child = invocation
            .envs(&environment.vars)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                serde_json::json!({
                    "hook_event_name": "Stop",
                    "session_id": format!("codex-stop-{shell}")
                })
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{shell} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty(), "{shell}");
        assert!(output.stderr.is_empty(), "{shell}");
        let event = receive(&relay);
        assert_eq!(event.run, registration.run);
        assert_eq!(
            event.session.as_deref(),
            Some(format!("codex-stop-{shell}").as_str())
        );
    }
    relay.shutdown();
}

#[test]
fn both_providers_get_private_hook_overlays_and_exact_resume_args() {
    let executable = std::env::current_exe().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let env = ShellEnvironment {
        shell: executable.clone(),
        vars: HashMap::new(),
    };
    let relay = Relay::start().unwrap();
    for tool in ["claude", "codex"] {
        let mut catalog = ToolCatalog::default();
        let definition = catalog.tools.iter_mut().find(|t| t.id == tool).unwrap();
        definition.executable = executable.to_string_lossy().into_owned();
        definition.profiles[0].settings.settings_json =
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-hook"}]}]}}"#
                .into();
        let original = catalog.clone();
        let mut workspace = Workspace::new();
        let tab = workspace.open(tool, tool);
        workspace
            .set_pane_metadata(
                tab,
                workspace.active().unwrap().focused,
                PaneMetadata {
                    cwd: Some(cwd.clone()),
                    resume_id: Some("550e8400-e29b-41d4-a716-446655440000".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut pane = workspace.all_panes().remove(0);
        launch::augment(
            &mut catalog,
            &mut pane,
            std::path::Path::new("/tmp/Canopy's App/canopy"),
        )
        .unwrap();
        let spec = catalog.launch(&pane, &env).unwrap();
        let settings: serde_json::Value = serde_json::from_str(
            &catalog.get(tool).unwrap().profiles[0]
                .settings
                .settings_json,
        )
        .unwrap();
        assert_eq!(
            settings["hooks"]["SessionStart"].as_array().unwrap().len(),
            2
        );
        let command = settings["hooks"]["SessionStart"][1]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        if cfg!(windows) {
            assert_eq!(command, "\"/tmp/Canopy's App/canopy\"");
        } else {
            assert_eq!(
                shell_words::split(command).unwrap(),
                ["/tmp/Canopy's App/canopy", "--agent-hook"]
            );
        }
        assert!(
            original.get(tool).unwrap().profiles[0]
                .settings
                .settings_json
                .contains("user-hook")
        );
        if tool == "codex" {
            assert_eq!(
                &spec.arguments[..2],
                &["resume", "550e8400-e29b-41d4-a716-446655440000"]
            );
        } else {
            assert!(
                spec.arguments
                    .windows(2)
                    .any(|args| args == ["--resume", "550e8400-e29b-41d4-a716-446655440000"])
            );
        }
        let a = relay.register();
        let b = relay.register();
        let mut ea = env.clone();
        let mut eb = env.clone();
        launch::environment(&mut ea, &a);
        launch::environment(&mut eb, &b);
        assert_ne!(ea.vars["CANOPY_AGENT_RUN"], eb.vars["CANOPY_AGENT_RUN"]);
        assert_eq!(ea.vars["CANOPY_AGENT_ENDPOINT_KIND"], a.endpoint.kind());
        assert_eq!(ea.vars["CANOPY_AGENT_ENDPOINT"], a.endpoint.address());
    }
    relay.shutdown();
}
#[test]
fn unknown_and_subagent_events_do_not_invent_idle_state() {
    assert_eq!(Status::Working.event("Unknown"), Status::Working);
    assert_eq!(Status::Working.event("SubagentStop"), Status::Working);
    let event = Event::from_json(
        "run".into(),
        &serde_json::json!({"hook_event_name":"SessionStart","session_id":"--last","agent_id":"child"}),
    );
    assert!(event.session.is_none());
    assert!(event.subagent);
}

#[test]
fn question_events_pause_until_answer_and_do_not_scan_terminal_text() {
    let waiting = Event::from_json(
        "run".into(),
        &serde_json::json!({"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"Which option?"}]}}),
    );
    assert_eq!(waiting.status(Status::Working), Status::Waiting);
    assert_eq!(waiting.question.as_deref(), Some("Which option?"));
    let answered = Event::from_json(
        "run".into(),
        &serde_json::json!({"hook_event_name":"PostToolUse","tool_name":"AskUserQuestion"}),
    );
    assert_eq!(answered.status(Status::Waiting), Status::Working);
    let codex = Event::from_json(
        "run".into(),
        &serde_json::json!({"hook_event_name":"PreToolUse", "tool_name":"request_user_input", "tool_input":{"questions":[{"question":"What should we work on?"}]}}),
    );
    assert_eq!(codex.status(Status::Working), Status::Waiting);
    assert_eq!(codex.question.as_deref(), Some("What should we work on?"));
    let answer = Event::from_json(
        "run".into(),
        &serde_json::json!({"hook_event_name":"PostToolUse", "tool_name":"request_user_input"}),
    );
    assert_eq!(answer.status(Status::Waiting), Status::Working);
}

#[test]
fn unrelated_tool_completion_does_not_clear_a_pending_question() {
    let mut attention = canopy_desktop::agents::Attention::default();
    let event = |name, tool, id| {
        Event::from_json(
            "run".into(),
            &serde_json::json!({"hook_event_name":name,"tool_name":tool,"tool_use_id":id}),
        )
    };
    attention.apply(&event("PreToolUse", "request_user_input", "question"));
    assert!(attention.waiting());
    attention.apply(&event("PostToolUse", "shell", "other"));
    assert!(attention.waiting());
    attention.apply(&event("PostToolUse", "request_user_input", "question"));
    assert!(!attention.waiting());
}

#[test]
fn pane_session_binding_rejects_stale_updates_and_restores_lazily_from_sqlite() {
    use canopy_desktop::{
        settings::SettingsClient,
        state::{layout::LayoutState, projects::Projects, session::SessionSnapshot},
    };
    futures_lite::future::block_on(async {
        let mut projects = Projects::default();
        let id = projects.open("/tmp".into());
        let mut workspace = Workspace::new();
        workspace.id = id;
        let a = workspace.open("A", "codex");
        let source_a = workspace.all_panes()[0].clone();
        let b = workspace.open("B", "claude");
        workspace.set_default_cwd("/tmp".into());
        let source_a = workspace
            .all_panes()
            .into_iter()
            .find(|p| p.id == source_a.id)
            .unwrap();
        let source_b = workspace.active().unwrap().root.first().clone();
        assert!(
            workspace
                .bind_agent_session(&source_a, "codex-session")
                .unwrap()
        );
        assert!(
            workspace
                .bind_agent_session(&source_a, "late-session")
                .is_err()
        );
        workspace
            .bind_agent_session(&source_b, "claude-session")
            .unwrap();
        workspace.activate(a).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let saved = SessionSnapshot {
            projects,
            workspaces: vec![workspace],
            layout: LayoutState::default(),
        };
        client.save_session(saved.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(path, canopy_desktop::settings::Access::ReadOnly)
            .await
            .unwrap();
        let mut restored = client.load_session().await.unwrap().unwrap();
        assert_eq!(restored, saved);
        assert_eq!(restored.activation_plan().len(), 1);
        assert_eq!(
            restored.activation_plan()[0].metadata.resume_id.as_deref(),
            Some("codex-session")
        );
        restored.workspaces[0].activate(b).unwrap();
        assert_eq!(
            restored.activation_plan()[0].metadata.resume_id.as_deref(),
            Some("claude-session")
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn notch_notifications_require_an_unseen_significant_transition() {
    use canopy_desktop::agents::should_notify;
    for next in [
        Status::Waiting,
        Status::Idle,
        Status::Failed,
        Status::Exited,
    ] {
        assert!(should_notify(Status::Working, next, false));
        assert!(!should_notify(Status::Working, next, true));
    }
    assert!(!should_notify(Status::Starting, Status::Idle, false));
    assert!(!should_notify(Status::Working, Status::Working, false));
    assert!(!should_notify(Status::Idle, Status::Working, false));
    assert!(!should_notify(Status::Waiting, Status::Waiting, false));
}

#[test]
fn every_split_pane_is_visible_only_in_the_active_window_and_tab() {
    use canopy_desktop::{agents::pane_visible, state::workspace::Axis};
    let mut workspace = Workspace::new();
    let tab = workspace.open("Agents", "codex");
    let first = workspace.active().unwrap().focused;
    let second = workspace
        .split(tab, first, Axis::Horizontal, "claude")
        .unwrap();
    assert!(pane_visible(true, Some(workspace.id), &workspace, first));
    assert!(pane_visible(true, Some(workspace.id), &workspace, second));
    assert!(!pane_visible(false, Some(workspace.id), &workspace, first));
    assert!(!pane_visible(
        true,
        Some(Workspace::new().id),
        &workspace,
        first
    ));
    workspace.open("Other tab", "shell");
    assert!(!pane_visible(true, Some(workspace.id), &workspace, second));
}

#[test]
fn agent_navigation_selects_exact_tab_and_split_before_lazy_start() {
    use canopy_desktop::{
        state::workspace::{Axis, PaneId},
        terminal::lifecycle,
    };
    let mut workspace = Workspace::new();
    let agents = workspace.open("Agents", "codex");
    let first = workspace.active().unwrap().focused;
    let target = workspace
        .split(agents, first, Axis::Horizontal, "claude")
        .unwrap();
    workspace.open("Sleeping shell", "shell");
    let sleeping = workspace.active().unwrap().focused;
    workspace.activate_pane(target).unwrap();
    assert_eq!(workspace.active().unwrap().id, agents);
    assert_eq!(workspace.active().unwrap().focused, target);
    let plan = lifecycle::reconcile(
        &[workspace.clone()],
        Some(workspace.id),
        &Default::default(),
    );
    assert_eq!(plan.focused, Some(target));
    assert!(!plan.start.iter().any(|pane| pane.id == sleeping));
    let before = workspace.clone();
    assert!(workspace.activate_pane(PaneId::new()).is_err());
    assert_eq!(workspace, before);
    workspace.close(agents).unwrap();
    assert!(workspace.activate_pane(target).is_err());
}

#[test]
fn claude_declined_question_clears_both_question_and_permission_waits() {
    use canopy_desktop::agents::Attention;
    let event = |name: &str, id: Option<&str>| {
        Event::from_json(
            "run".into(),
            &serde_json::json!({
                "hook_event_name":name, "tool_name":"AskUserQuestion", "tool_use_id":id,
                "tool_input":{"questions":[{"question":"Choose?"}]}
            }),
        )
    };
    for completion in ["PermissionDenied", "PostToolUse", "PostToolUseFailure"] {
        let mut attention = Attention::default();
        attention.apply(&event("PreToolUse", Some("question-1")));
        attention.apply(&event("PermissionRequest", None));
        assert!(attention.waiting());
        let mut done = event(completion, Some("question-1"));
        done.interrupted = completion == "PostToolUseFailure";
        attention.apply(&done);
        assert!(!attention.waiting(), "{completion}");
        assert!(attention.question().is_none());
        assert_eq!(
            done.status(Status::Waiting),
            if completion == "PostToolUse" {
                Status::Working
            } else {
                Status::Idle
            }
        );
    }
}

#[test]
fn claude_idle_notification_recovers_waiting_without_turning_unrelated_notifications_into_alerts() {
    use canopy_desktop::agents::Attention;
    let event = |kind: &str| {
        Event::from_json(
            "run".into(),
            &serde_json::json!({
                "hook_event_name":"Notification", "notification_type":kind
            }),
        )
    };
    let mut attention = Attention::default();
    attention.apply(&Event::from_json(
        "run".into(),
        &serde_json::json!({
            "hook_event_name":"PreToolUse", "tool_name":"AskUserQuestion", "tool_use_id":"q"
        }),
    ));
    attention.apply(&event("auth_success"));
    assert!(attention.waiting());
    attention.apply(&event("idle_prompt"));
    assert!(!attention.waiting());
    assert_eq!(event("idle_prompt").status(Status::Waiting), Status::Idle);
    assert_eq!(event("auth_success").status(Status::Idle), Status::Idle);
    let failure = Event::from_json(
        "run".into(),
        &serde_json::json!({
            "hook_event_name":"PostToolUseFailure", "is_interrupt":false
        }),
    );
    assert_eq!(failure.status(Status::Working), Status::Failed);
}

#[test]
fn claude_rejection_requires_exact_session_call_and_structured_result() {
    use canopy_desktop::agents::transcript::{QuestionWatch, read_rejection, rejected_question};
    let result = serde_json::json!({"sessionId":"session", "isSidechain":false,
        "message":{"content":[{"type":"tool_result", "tool_use_id":"question", "is_error":true,
        "content":"The user doesn't want to proceed with this tool use. The tool use was rejected."}]}});
    let bytes = serde_json::to_vec(&result).unwrap();
    assert!(rejected_question(&bytes, "session", "question"));
    assert!(!rejected_question(&bytes, "other", "question"));
    assert!(!rejected_question(&bytes, "session", "other"));
    let mut other = result.clone();
    other["isSidechain"] = true.into();
    assert!(!rejected_question(
        &serde_json::to_vec(&other).unwrap(),
        "session",
        "question"
    ));
    other = result.clone();
    other["message"]["content"][0]["content"] = "ordinary execution failure".into();
    assert!(!rejected_question(
        &serde_json::to_vec(&other).unwrap(),
        "session",
        "question"
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    std::fs::write(&path, b"{}").unwrap();
    let _watch = QuestionWatch::new(&path).unwrap();
    assert!(!read_rejection(&path, "session", "question").unwrap());
    std::fs::write(&path, bytes).unwrap();
    assert!(read_rejection(&path, "session", "question").unwrap());
}
