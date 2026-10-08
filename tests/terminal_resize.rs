#![cfg(unix)]
use canopy_desktop::terminal::{
    environment::ShellEnvironment,
    session::{LaunchSpec, Session, Size, Status},
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const PROBE: &str = "CANOPY_TEST_PTY_RESIZE_PROBE";
static RESIZED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_resize(_: libc::c_int) {
    RESIZED.store(true, Ordering::Relaxed);
}

fn signal_mask() -> libc::sigset_t {
    unsafe {
        let mut mask = std::mem::zeroed();
        assert_eq!(
            libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut mask),
            0
        );
        mask
    }
}

fn blocked(mask: &libc::sigset_t, signal: libc::c_int) -> bool {
    unsafe { libc::sigismember(mask, signal) == 1 }
}

/// Reproduce the mask inherited from a macOS dispatch worker, on this thread only.
struct WorkerMask(libc::sigset_t);
impl WorkerMask {
    fn new() -> Self {
        unsafe {
            let mut blocked = std::mem::zeroed();
            libc::sigemptyset(&mut blocked);
            libc::sigaddset(&mut blocked, libc::SIGWINCH);
            libc::sigaddset(&mut blocked, libc::SIGTERM);
            let mut old = std::mem::zeroed();
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, &mut old),
                0
            );
            Self(old)
        }
    }
}
impl Drop for WorkerMask {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut());
        }
    }
}

fn wait_for(session: &Session, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = session
            .frame()
            .map(|frame| {
                frame
                    .cells
                    .iter()
                    .map(|cell| cell.text.as_str())
                    .collect::<String>()
            })
            .unwrap_or_default();
        if output.contains(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "missing {expected:?} in {output:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn worker_spawn_delivers_resize_signals_and_restores_its_own_mask() {
    let _worker_mask = WorkerMask::new();
    let before = signal_mask();
    let environment = ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: HashMap::from([(PROBE.into(), "1".into())]),
    };
    let spec = LaunchSpec {
        program: std::env::current_exe().unwrap(),
        arguments: vec![
            "--exact".into(),
            "resize_probe_child".into(),
            "--nocapture".into(),
        ],
        cwd: std::env::temp_dir(),
    };
    let session = Session::start(spec, &environment, Size::bounded(80, 24)).unwrap();
    let after = signal_mask();
    for signal in [libc::SIGWINCH, libc::SIGTERM] {
        assert!(blocked(&before, signal));
        assert!(blocked(&after, signal), "changed the caller's mask");
    }
    wait_for(&session, "READY 24 80");
    for (columns, rows) in [(115, 45), (151, 45), (179, 45), (100, 32), (80, 24)] {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !session.resize(Size::bounded(columns, rows)) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        // No input or extra repaint signal: the kernel's SIGWINCH must wake the TUI.
        wait_for(&session, &format!("WINCH {rows} {columns}"));
        assert_eq!(session.frame().unwrap().size, Size::bounded(columns, rows));
    }
    session.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(session.status(), Status::Running) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        Session::start(
            LaunchSpec {
                program: "/missing/canopy-resize-test".into(),
                arguments: vec![],
                cwd: std::env::temp_dir(),
            },
            &environment,
            Size::bounded(80, 24)
        )
        .is_err()
    );
    let after_error = signal_mask();
    for signal in [libc::SIGWINCH, libc::SIGTERM] {
        assert!(
            blocked(&after_error, signal),
            "spawn error changed the caller's mask"
        );
    }
}

/// Run the test executable itself as a TUI: a shell could reset the inherited
/// signal mask and hide the bug affecting directly launched native tools.
#[test]
fn resize_probe_child() {
    if std::env::var_os(PROBE).is_none() {
        return;
    }
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_resize as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut action.sa_mask);
        assert_eq!(
            libc::sigaction(libc::SIGWINCH, &action, std::ptr::null_mut()),
            0
        );
    }
    let report = |prefix| unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        assert_eq!(
            libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size),
            0
        );
        println!("{prefix} {} {}", size.ws_row, size.ws_col);
    };
    report("READY");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if RESIZED.swap(false, Ordering::Relaxed) {
            report("WINCH");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
