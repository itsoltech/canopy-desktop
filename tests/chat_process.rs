#![cfg(unix)]
//! The chat subprocess runner against a fake peer: line I/O, exit reporting,
//! bounded shutdown of a process that ignores stdin EOF and SIGTERM.
use canopy_desktop::chat::process::{Event, Exit, Process, Spec};
use std::time::{Duration, Instant};

fn parse(line: &[u8]) -> Result<Option<String>, String> {
    match line {
        b"" => Ok(None),
        b"bad" => Err("unreadable line".into()),
        line => Ok(Some(String::from_utf8_lossy(line).into_owned())),
    }
}

fn shell(script: &str, secrets: &[&str]) -> Spec {
    Spec {
        program: "/bin/sh".into(),
        arguments: vec!["-c".into(), script.into()],
        cwd: std::env::temp_dir(),
        environment: [("PATH".to_owned(), "/usr/bin:/bin".to_owned())].into(),
        secrets: secrets.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn next(events: &async_channel::Receiver<Event<String>>, timeout: Duration) -> Event<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(event) = events.try_recv() {
            return event;
        }
        assert!(Instant::now() < deadline, "no process event in time");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn exit(events: &async_channel::Receiver<Event<String>>) -> Exit {
    loop {
        if let Event::Exited(exit) = next(events, Duration::from_secs(10)) {
            return exit;
        }
    }
}

#[test]
fn lines_round_trip_and_stdin_eof_ends_the_process_cleanly() {
    let (process, events) = Process::start(
        shell(
            r#"while IFS= read -r line; do printf '%s\n' "$line"; done"#,
            &[],
        ),
        None,
        parse,
        1024,
    )
    .unwrap();
    process.send(&serde_json::json!({"n": 1})).unwrap();
    process.send(&serde_json::json!("two")).unwrap();
    for expected in [r#"{"n":1}"#, r#""two""#] {
        match next(&events, Duration::from_secs(5)) {
            Event::Message(line) => assert_eq!(line, expected),
            other => panic!("unexpected {other:?}"),
        }
    }
    process.close().unwrap();
    assert_eq!(exit(&events).code, Some(0));
    assert!(process.send(&serde_json::json!(3)).is_err());
}

#[test]
fn close_escalates_to_kill_for_a_process_ignoring_eof_and_sigterm() {
    let (process, events) = Process::start(
        shell("trap '' TERM; while :; do sleep 0.05; done", &[]),
        None,
        parse,
        1024,
    )
    .unwrap();
    let started = Instant::now();
    process
        .close_with(Duration::from_millis(100), Duration::from_millis(200))
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(process.has_exited());
    assert_eq!(exit(&events).signal, Some(libc::SIGKILL));
}

#[test]
fn diagnostics_and_stderr_tail_are_reported_without_secrets() {
    let (process, events) = Process::start(
        shell(
            "echo bad; echo ok; echo 'failed with sk-ant-0123456789' >&2; exit 3",
            &["sk-ant-0123456789"],
        ),
        None,
        parse,
        1024,
    )
    .unwrap();
    assert!(matches!(
        next(&events, Duration::from_secs(5)),
        Event::Diagnostic(_)
    ));
    assert!(matches!(next(&events, Duration::from_secs(5)), Event::Message(line) if line == "ok"));
    let exit = exit(&events);
    assert_eq!(exit.code, Some(3));
    assert_eq!(exit.stderr, "failed with [redacted]");
    drop(process);
}

#[test]
fn a_dropped_consumer_never_blocks_a_chatty_process() {
    let (process, events) = Process::start(
        shell(
            "i=0; while [ $i -lt 20000 ]; do echo line-$i; i=$((i+1)); done",
            &[],
        ),
        None,
        parse,
        1024,
    )
    .unwrap();
    drop(events);
    let started = Instant::now();
    process.close().unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn missing_program_and_working_directory_are_errors() {
    let mut spec = shell("true", &[]);
    spec.program = "/nonexistent/claude".into();
    assert!(Process::start(spec, None, parse, 1024).is_err());
    let mut spec = shell("true", &[]);
    spec.cwd = "/nonexistent/project".into();
    assert!(Process::start(spec, None, parse, 1024).is_err());
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks existence.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn first_line(events: &async_channel::Receiver<Event<String>>) -> i32 {
    match next(events, Duration::from_secs(5)) {
        Event::Message(line) => line.trim().parse().unwrap(),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn natural_exit_stops_tools_left_in_the_process_group() {
    let (_process, events) =
        Process::start(shell("sleep 30 & echo $!; exit 0", &[]), None, parse, 1024).unwrap();
    let orphan = first_line(&events);
    // The orphan holds stdout; the exit arrives only once the group sweep stopped it.
    assert_eq!(exit(&events).code, Some(0));
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(orphan) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(orphan));
}

#[test]
fn exit_is_reported_when_a_stubborn_descendant_keeps_stdout_open() {
    let (_process, events) = Process::start(
        shell("(trap '' TERM; sleep 30) & echo $!; exit 0", &[]),
        None,
        parse,
        1024,
    )
    .unwrap();
    let holder = first_line(&events);
    let started = Instant::now();
    assert_eq!(exit(&events).code, Some(0));
    assert!(started.elapsed() < Duration::from_secs(5));
    // SAFETY: test cleanup of the subshell group it started.
    unsafe {
        libc::kill(-holder, libc::SIGKILL);
        libc::kill(holder, libc::SIGKILL);
    }
}
