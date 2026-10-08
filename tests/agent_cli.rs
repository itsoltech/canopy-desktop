//! Opt-in, small live-provider probes. No project files or user hook files are edited.
use canopy_desktop::{
    agents::{launch, relay::Relay},
    state::{
        tools::ToolCatalog,
        workspace::{PaneMetadata, Workspace},
    },
    terminal::environment::ShellEnvironment,
};
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn probe(tool: &str) {
    let env = ShellEnvironment::load().unwrap();
    let dir = tempfile::Builder::new()
        .prefix("canopy-agent-live-")
        .tempdir()
        .unwrap();
    let relay = Relay::start().unwrap();
    let mut session = None;
    for turn in 0..2 {
        let registration = relay.register();
        let mut catalog = ToolCatalog::default();
        let mut workspace = Workspace::new();
        let tab = workspace.open(tool, tool);
        workspace
            .set_pane_metadata(
                tab,
                workspace.active().unwrap().focused,
                PaneMetadata {
                    cwd: Some(dir.path().to_owned()),
                    resume_id: session.clone(),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut pane = workspace.all_panes().remove(0);
        launch::augment(
            &mut catalog,
            &mut pane,
            &std::env::var_os("CANOPY_AGENT_HELPER")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| env!("CARGO_BIN_EXE_canopy-desktop").into()),
        )
        .unwrap();
        let spec = catalog.launch(&pane, &env).unwrap();
        let mut hook_args = Vec::new();
        for pair in spec.arguments.windows(2) {
            if matches!(
                pair[0].as_str(),
                "-c" | "--config" | "--enable" | "--settings"
            ) {
                hook_args.extend_from_slice(pair);
            }
        }
        let mut command = Command::new(&spec.program);
        if tool == "claude" {
            command.args(["--print", "--tools", ""]);
            if let Some(id) = &session {
                command.args(["--resume", id]);
            }
        } else {
            command.args(["--sandbox", "read-only", "exec"]);
            if let Some(id) = &session {
                command.args(["resume", id]);
            }
            command.args(["--skip-git-repo-check", "--json"]);
        }
        command
            .args(hook_args)
            .arg("Reply exactly OK. Do not use any tools.")
            .current_dir(dir.path());
        let mut environment = env.clone();
        launch::environment(&mut environment, &registration);
        command
            .envs(&environment.vars)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.take(262144).read_to_end(&mut bytes);
            bytes
        });
        let err = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(262144).read_to_end(&mut bytes);
            bytes
        });
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut received = None;
        let status = loop {
            while let Ok(event) = relay.events.try_recv() {
                if event.run == registration.run && !event.subagent && event.session.is_some() {
                    received = event.session;
                }
            }
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() > deadline {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = out.join().unwrap();
        let errors = err.join().unwrap();
        while let Ok(event) = relay.events.try_recv() {
            if event.run == registration.run && !event.subagent && event.session.is_some() {
                received = event.session;
            }
        }
        relay.remove(&registration.run);
        if received.is_none() || !status.is_some_and(|s| s.success()) {
            let path = dir.keep();
            std::fs::write(path.join("stdout.log"), output).unwrap();
            std::fs::write(path.join("stderr.log"), errors).unwrap();
            relay.shutdown();
            panic!(
                "{tool} live probe turn {turn} did not complete with a bound root hook; logs retained at {}",
                path.display()
            );
        }
        if let Some(expected) = &session {
            assert_eq!(
                received.as_ref(),
                Some(expected),
                "resume changed session id"
            );
        }
        session = received;
    }
    relay.shutdown();
}
#[test]
#[ignore = "Starts two minimal Claude turns using existing login; checks real hooks and explicit resume"]
fn claude_hooks_and_resume() {
    probe("claude");
}
#[test]
#[ignore = "Starts two minimal Codex turns using existing login; hook trust must already be approved"]
fn codex_hooks_and_resume() {
    probe("codex");
}
