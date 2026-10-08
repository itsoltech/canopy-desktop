#![cfg(unix)]

use canopy_desktop::terminal::{
    environment::ShellEnvironment,
    session::{INPUT_LIMIT_BYTES, LaunchSpec, Session, Size, Status},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
fn env() -> ShellEnvironment {
    ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: HashMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("TERM".into(), "xterm-256color".into()),
        ]),
    }
}
fn session(program: &str, args: &[&str]) -> Session {
    Session::start(
        LaunchSpec {
            program: program.into(),
            arguments: args.iter().map(|s| s.to_string()).collect(),
            cwd: PathBuf::from("/tmp"),
        },
        &env(),
        Size::bounded(80, 24),
    )
    .unwrap()
}
fn wait(session: &Session) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(session.status(), Status::Running) {
        assert!(Instant::now() < deadline, "process did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn text(session: &Session) -> String {
    session
        .frame()
        .unwrap()
        .cells
        .iter()
        .map(|c| c.text.as_str())
        .collect::<String>()
}
#[test]
fn direct_executable_receives_literal_arguments_without_shell_interpolation() {
    let s = session("/usr/bin/printf", &["%s", "literal; $HOME"]);
    wait(&s);
    assert!(text(&s).contains("literal; $HOME"));
    assert_eq!(
        s.status(),
        Status::Exited {
            code: Some(0),
            signal: None
        }
    );
}
#[test]
fn ansi_unicode_and_final_output_are_drained_before_exit_status() {
    let s = session(
        "/bin/sh",
        &[
            "-c",
            r"printf '\033[31mRED\033[0m\r\nzażółć\r\nFINAL'; exit 7",
        ],
    );
    wait(&s);
    let frame = s.frame().unwrap();
    assert!(text(&s).contains("zażółć"));
    assert!(text(&s).contains("FINAL"));
    assert!(frame.cells.iter().any(|c| c.text == "R" && c.fg.r > c.fg.g));
    assert_eq!(
        s.status(),
        Status::Exited {
            code: Some(7),
            signal: None
        }
    );
}
#[test]
fn resizing_reaches_the_child_tty() {
    let s = session("/bin/sh", &["-c", "stty size; read answer; stty size"]);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !text(&s).contains("24 80") {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    while !s.resize(Size::bounded(100, 40)) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(s.input(b"ok\n".to_vec()));
    wait(&s);
    assert!(text(&s).contains("40 100"));
}

#[test]
fn input_reports_whether_the_pty_accepts_bytes() {
    let s = session(
        "/bin/sh",
        &["-c", "IFS= read -r line; printf 'GOT:%s' \"$line\""],
    );
    assert!(!s.input(vec![b'a'; INPUT_LIMIT_BYTES + 1]));
    assert!(s.input(b"payload\n".to_vec()));
    wait(&s);
    assert!(text(&s).contains("GOT:payload"));
    assert!(!s.input(b"late\n".to_vec()));
}
#[test]
fn close_reaps_a_process_which_ignores_hangup() {
    let s = session(
        "/bin/sh",
        &[
            "-c",
            "trap '' HUP TERM; printf READY; while :; do sleep 1; done",
        ],
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !text(&s).contains("READY") {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    s.stop();
    wait(&s);
}
#[test]
fn invalid_executable_and_directory_are_launch_errors() {
    assert!(
        Session::start(
            LaunchSpec {
                program: "/missing/canopy-program".into(),
                arguments: vec![],
                cwd: "/tmp".into()
            },
            &env(),
            Size::bounded(80, 24)
        )
        .is_err()
    );
    assert!(
        Session::start(
            LaunchSpec {
                program: "/bin/sh".into(),
                arguments: vec![],
                cwd: "/missing/canopy-directory".into()
            },
            &env(),
            Size::bounded(80, 24)
        )
        .is_err()
    );
}
#[test]
fn environment_probe_reads_the_login_shell_without_printing_values() {
    let vars = canopy_desktop::terminal::environment::probe(
        std::path::Path::new("/bin/zsh"),
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(vars.contains_key("PATH"));
    assert_eq!(vars["COLORTERM"], "truecolor");
}

#[test]
fn signal_termination_is_distinct_from_an_exit_code() {
    let s = session("/bin/sh", &["-c", "kill -TERM $$"]);
    wait(&s);
    assert_eq!(
        s.status(),
        Status::Exited {
            code: None,
            signal: Some(libc::SIGTERM)
        }
    );
}
#[test]
fn normal_exit_does_not_leave_ignored_hangup_children_running() {
    let s = session(
        "/bin/sh",
        &[
            "-c",
            "trap '' HUP; sleep 30 & printf 'CHILD=%s\n' $!; exit 0",
        ],
    );
    wait(&s);
    let output = text(&s);
    let pid: i32 = output
        .split("CHILD=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            Instant::now() < deadline,
            "child remains after terminal exit"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn macos_clear_discards_history_and_the_previous_visible_screen() {
    let s = session(
        "/bin/sh",
        &[
            "-c",
            "i=0; while [ $i -lt 80 ]; do printf 'OLD_%s\\r\\n' \"$i\"; i=$((i+1)); done; printf '\\033[3J\\033[H\\033[2JFRESH\\r\\n'",
        ],
    );
    wait(&s);
    s.scroll(10000);
    let frame = s.frame().unwrap();
    assert_eq!(frame.offset, 0);
    assert!(text(&s).contains("FRESH"));
    assert!(!text(&s).contains("OLD_"));
}
#[test]
fn a_normal_screen_redraw_does_not_erase_scrollback() {
    let s = session(
        "/bin/sh",
        &[
            "-c",
            "i=0; while [ $i -lt 80 ]; do printf 'KEEP_%s\\r\\n' \"$i\"; i=$((i+1)); done; printf '\\033[H\\033[2JREDRAW\\r\\n'",
        ],
    );
    wait(&s);
    s.scroll(10000);
    assert!(s.frame().unwrap().offset > 0);
    assert!(text(&s).contains("KEEP_"));
}

#[cfg(target_os = "macos")]
#[test]
fn system_clear_command_removes_all_scrollback() {
    let s = session(
        "/bin/sh",
        &[
            "-c",
            "i=0; while [ $i -lt 80 ]; do printf 'OLD_%s\\r\\n' \"$i\"; i=$((i+1)); done; /usr/bin/clear; printf 'NEW_PROMPT\\r\\n'",
        ],
    );
    wait(&s);
    s.scroll(10000);
    assert_eq!(s.frame().unwrap().offset, 0);
    assert!(text(&s).contains("NEW_PROMPT"));
    assert!(!text(&s).contains("OLD_"));
}
#[test]
fn clearing_alternate_screen_preserves_primary_scrollback() {
    let s = session(
        "/bin/sh",
        &[
            "-c",
            "i=0; while [ $i -lt 80 ]; do printf 'PRIMARY_%s\\r\\n' \"$i\"; i=$((i+1)); done; printf '\\033[?1049h\\033[3J\\033[H\\033[2JALT\\033[?1049l'",
        ],
    );
    wait(&s);
    s.scroll(10000);
    assert!(s.frame().unwrap().offset > 0);
    assert!(text(&s).contains("PRIMARY_"));
}
