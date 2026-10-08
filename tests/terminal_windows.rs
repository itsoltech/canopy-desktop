#![cfg(windows)]

use canopy_desktop::terminal::{
    environment::{ShellEnvironment, ShellKind},
    session::{LaunchSpec, Session, Size, Status},
};
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::{CloseHandle, STILL_ACTIVE},
    System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

fn environment() -> ShellEnvironment {
    ShellEnvironment::load().expect("Windows shell environment")
}

fn start(
    env: &ShellEnvironment,
    program: impl Into<std::path::PathBuf>,
    args: Vec<String>,
) -> Session {
    Session::start(
        LaunchSpec {
            program: program.into(),
            arguments: args,
            cwd: std::env::current_dir().unwrap(),
        },
        env,
        Size::bounded(240, 40),
    )
    .unwrap()
}

fn text(session: &Session) -> String {
    session
        .frame()
        .unwrap()
        .cells
        .iter()
        .map(|cell| cell.text.as_str())
        .collect()
}

fn wait_for_text(session: &Session, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !text(session).contains(expected) {
        assert!(
            Instant::now() < deadline,
            "missing terminal output: {expected}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_closed(session: &Session) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.status() == Status::Running {
        assert!(Instant::now() < deadline, "terminal did not close");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn process_running(pid: u32) -> bool {
    // SAFETY: OpenProcess returns an owned handle or null; the queried handle is always closed.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let mut code = 0;
    // SAFETY: process is a live owned handle and code points to writable storage.
    let active =
        unsafe { GetExitCodeProcess(process, &mut code) } != 0 && code == STILL_ACTIVE as u32;
    // SAFETY: process is owned by this function.
    unsafe {
        CloseHandle(process);
    }
    active
}

#[test]
fn default_windows_shell_never_receives_a_posix_login_flag() {
    let env = environment();
    assert!(matches!(
        env.shell_kind(),
        ShellKind::PowerShell | ShellKind::Cmd | ShellKind::Other
    ));
    assert!(
        !env.default_shell_arguments()
            .iter()
            .any(|argument| argument == "-l")
    );
    assert!(env.vars.keys().any(|key| key.eq_ignore_ascii_case("PATH")));
}

#[test]
fn conpty_quotes_a_program_path_with_spaces() {
    let env = environment();
    let source = env.resolve("cmd.exe").unwrap();
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("Program Files").join("Canopy Test");
    std::fs::create_dir_all(&directory).unwrap();
    let program = directory.join("command copy.exe");
    std::fs::copy(source, &program).unwrap();
    let session = start(
        &env,
        program,
        vec![
            "/d".into(),
            "/s".into(),
            "/c".into(),
            "echo PROGRAM_PATH_OK".into(),
        ],
    );
    wait_closed(&session);
    assert!(text(&session).contains("PROGRAM_PATH_OK"));
    assert_eq!(
        session.status(),
        Status::Exited {
            code: Some(0),
            signal: None
        }
    );
}

#[test]
fn powershell_script_receives_literal_argv_env_final_output_and_exit_code() {
    let mut env = environment();
    env.vars.insert("CANOPY_W2_TEST".into(), "zażółć".into());
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("argument probe.ps1");
    std::fs::write(
        &script,
        r#"$json = ConvertTo-Json -Compress -InputObject @($args)
[Console]::WriteLine("ARGS=" + $json)
[Console]::WriteLine("ENV=" + $env:CANOPY_W2_TEST)
[Console]::WriteLine("FINAL")
exit 7
"#,
    )
    .unwrap();
    let values = vec![
        String::new(),
        "two words".into(),
        "quoted\"value".into(),
        "trail\\".into(),
        "&|<>^()".into(),
    ];
    let (program, arguments) = env.prepare_launch(script, values).unwrap();
    let session = start(&env, program, arguments);
    wait_closed(&session);
    let output = text(&session);
    assert!(output.contains(r#"ARGS=["","two words","quoted\"value","trail\\","&|<>^()"]"#));
    assert!(output.contains("ENV=zażółć"));
    assert!(output.contains("FINAL"));
    assert_eq!(
        session.status(),
        Status::Exited {
            code: Some(7),
            signal: None
        }
    );
}

#[test]
fn command_script_uses_cmd_without_interpreting_quoted_metacharacters() {
    let env = environment();
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("argument probe.cmd");
    std::fs::write(
        &script,
        "@echo off\r\nset \"CANOPY_ARG=%~1\"\r\nset CANOPY_ARG\r\nexit /b 7\r\n",
    )
    .unwrap();
    let (program, arguments) = env
        .prepare_launch(script, vec!["literal&value".into()])
        .unwrap();
    let session = start(&env, program, arguments);
    wait_closed(&session);
    assert!(text(&session).contains("CANOPY_ARG=literal&value"));
    assert_eq!(
        session.status(),
        Status::Exited {
            code: Some(7),
            signal: None
        }
    );
}

#[test]
fn resize_reaches_conpty() {
    let env = environment();
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("resize probe.ps1");
    std::fs::write(
        &script,
        "$size=$Host.UI.RawUI.WindowSize; Write-Output \"BEFORE=$($size.Width)x$($size.Height)\"; [Console]::ReadLine() > $null; $size=$Host.UI.RawUI.WindowSize; Write-Output \"AFTER=$($size.Width)x$($size.Height)\"",
    )
    .unwrap();
    let (program, arguments) = env.prepare_launch(script, vec![]).unwrap();
    let session = start(&env, program, arguments);
    wait_for_text(&session, "BEFORE=");
    while !session.resize(Size::bounded(100, 50)) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.input(b"\r\n".to_vec()));
    wait_closed(&session);
    assert!(text(&session).contains("AFTER=100x50"));
}

#[test]
fn repeated_stop_kills_each_terminal_job_and_its_descendants() {
    let env = environment();
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("child probe.ps1");
    std::fs::write(
        &script,
        "$child=Start-Process -PassThru -WindowStyle Hidden -FilePath $env:ComSpec -ArgumentList '/d','/s','/c','ping -t 127.0.0.1 >NUL'; Write-Output \"CHILD=$($child.Id)\"; [Console]::Out.Flush(); Start-Sleep -Seconds 30",
    )
    .unwrap();
    let mut sessions = Vec::new();
    for _ in 0..2 {
        let (program, arguments) = env.prepare_launch(script.clone(), vec![]).unwrap();
        let runtime_files = tempfile::tempdir().unwrap();
        let runtime_path = runtime_files.path().to_owned();
        std::fs::write(runtime_path.join("owned-by-session"), b"fixture").unwrap();
        let session = Session::start_with_config(
            LaunchSpec {
                program,
                arguments,
                cwd: std::env::current_dir().unwrap(),
            },
            &env,
            Size::bounded(240, 40),
            Some(runtime_files),
        )
        .unwrap();
        wait_for_text(&session, "CHILD=");
        let pid = text(&session)
            .split("CHILD=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<u32>()
            .unwrap();
        sessions.push((session, pid, runtime_path));
    }
    for (session, _, _) in &sessions {
        session.stop();
        session.stop();
    }
    for (session, pid, runtime_path) in sessions {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let result = futures_lite::future::block_on(session.wait_closed());
            result_tx.send((session, result)).unwrap();
        });
        let (session, cleanup) = result_rx
            .recv_timeout(Duration::from_secs(8))
            .expect("terminal cleanup exceeded its bounded deadline");
        waiter.join().unwrap();
        cleanup.unwrap();
        assert!(
            !process_running(pid),
            "session completion preceded descendant cleanup"
        );
        assert!(
            !runtime_path.exists(),
            "session completion preceded temporary-file cleanup"
        );
        assert!(!matches!(session.status(), Status::Running));
    }
}

#[test]
fn concurrent_stop_waiters_share_the_same_session_cleanup_result() {
    let env = environment();
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("shared cleanup.ps1");
    std::fs::write(&script, "Write-Output 'READY'; Start-Sleep -Seconds 30").unwrap();
    let (program, arguments) = env.prepare_launch(script, vec![]).unwrap();
    let session = std::sync::Arc::new(start(&env, program, arguments));
    wait_for_text(&session, "READY");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let waiters: Vec<_> = (0..2)
        .map(|_| {
            let session = session.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                session.stop();
                futures_lite::future::block_on(session.wait_closed())
            })
        })
        .collect();
    barrier.wait();
    for waiter in waiters {
        assert!(waiter.join().unwrap().is_ok());
    }
    assert_eq!(session.cleanup_result(), Some(Ok(())));
}
