//! `!` commands from the composer (DEC-072): one shell command the user runs, in the
//! chat's folder with their shell and environment. Its output streams back while it
//! runs; Stop ends its whole process group (a Job Object on Windows). No GPUI here.
use std::{
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// Output kept for the card: this much from the start and from the end.
pub const KEEP: usize = 32 * 1024;
/// Output the agent gets: this much from the start and from the end.
pub const REPORT: usize = 5 * 1024;
/// After the shell exits, output still arriving from tools it left running is not
/// waited for longer than this.
const DRAIN: Duration = Duration::from_millis(500);
/// Stop asks politely first, then kills the group after this.
const GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq)]
pub enum ShellEvent {
    Output(String),
    Exit {
        code: Option<i32>,
        elapsed: Duration,
    },
}

/// A running command; dropping it doesn't stop it (call `stop`).
#[derive(Clone)]
pub struct Running {
    shared: Arc<Shared>,
}

struct Shared {
    /// Process group signalled on Unix; Windows uses the Job Object.
    #[cfg_attr(windows, allow(dead_code))]
    pid: u32,
    /// Set once the child is reaped; signals are sent only before, under this lock.
    exited: Mutex<bool>,
    /// The Job Object holding the command and everything it started.
    #[cfg(windows)]
    job: crate::terminal::windows_job::ProcessJob,
}

impl Running {
    /// Runs `command` with `shell -c` in `cwd`, with exactly the environment `vars`.
    #[cfg(unix)]
    pub fn start(
        command: &str,
        cwd: &Path,
        shell: &Path,
        vars: &std::collections::HashMap<String, String>,
    ) -> Result<(Self, async_channel::Receiver<ShellEvent>), String> {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new(shell);
        child
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .env_clear()
            .envs(vars)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0);
        // SAFETY: only async-signal-safe libc calls between fork and exec; GPUI threads
        // may block signals, the command must start with none blocked.
        unsafe {
            child.pre_exec(|| {
                let mut empty = std::mem::zeroed();
                libc::sigemptyset(&mut empty);
                libc::pthread_sigmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
                Ok(())
            });
        }
        let started = Instant::now();
        let child = child
            .spawn()
            .map_err(|error| format!("Couldn't run the command: {error}"))?;
        let shared = Arc::new(Shared {
            pid: child.id(),
            exited: Mutex::new(false),
        });
        let receiver = stream(child, shared.clone(), started);
        Ok((Self { shared }, receiver))
    }

    /// Runs `command` with the user's shell in `cwd`, inside a Job Object created
    /// before the command runs (it starts suspended), so Stop and cleanup reach every
    /// process it starts.
    #[cfg(windows)]
    pub fn start(
        command: &str,
        cwd: &Path,
        shell: &Path,
        vars: &std::collections::HashMap<String, String>,
    ) -> Result<(Self, async_channel::Receiver<ShellEvent>), String> {
        use crate::terminal::environment::{ShellKind, shell_kind};
        use std::os::windows::process::CommandExt;
        let mut child = std::process::Command::new(shell);
        match shell_kind(shell) {
            ShellKind::PowerShell => {
                child.args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    command,
                ]);
            }
            ShellKind::Cmd => {
                child.raw_arg(format!("/d /s /c \"{command}\""));
            }
            ShellKind::Posix => {
                child.arg("-c").arg(command);
            }
            ShellKind::Other => {
                return Err(format!(
                    "Commands can't run with the shell {}.",
                    shell.display()
                ));
            }
        }
        windows::spawn(child, cwd, vars)
    }

    /// Runs `program` with `arguments` directly (no shell), inside its own Job Object:
    /// an agent's terminal on Windows, where argv is not shell syntax.
    #[cfg(windows)]
    pub fn start_program(
        program: &Path,
        arguments: &[String],
        cwd: &Path,
        vars: &std::collections::HashMap<String, String>,
    ) -> Result<(Self, async_channel::Receiver<ShellEvent>), String> {
        let mut child = std::process::Command::new(program);
        child.args(arguments);
        windows::spawn(child, cwd, vars)
    }
}

/// Streams a started command's output and reports its exit (after its pipes drain,
/// bounded by `DRAIN`).
fn stream(
    mut child: std::process::Child,
    shared: Arc<Shared>,
    started: Instant,
) -> async_channel::Receiver<ShellEvent> {
    {
        let (events, receiver) = async_channel::unbounded();
        let (done, drained) = std::sync::mpsc::channel::<()>();
        let mut readers = 0;
        let pipes: [Option<Box<dyn Read + Send>>; 2] = [
            child
                .stdout
                .take()
                .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        ];
        for mut pipe in pipes.into_iter().flatten() {
            readers += 1;
            let events = events.clone();
            let done = done.clone();
            thread::spawn(move || {
                let mut buffer = [0u8; 8192];
                let mut pending = Vec::new();
                loop {
                    match pipe.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            pending.extend_from_slice(&buffer[..read]);
                            // Whole characters only; a split one waits for the rest.
                            let valid = match std::str::from_utf8(&pending) {
                                Ok(_) => pending.len(),
                                Err(error) if error.error_len().is_none() => error.valid_up_to(),
                                Err(_) => pending.len(),
                            };
                            let chunk: Vec<u8> = pending.drain(..valid).collect();
                            if !chunk.is_empty()
                                && events
                                    .send_blocking(ShellEvent::Output(
                                        String::from_utf8_lossy(&chunk).into_owned(),
                                    ))
                                    .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
                if !pending.is_empty() {
                    let _ = events.send_blocking(ShellEvent::Output(
                        String::from_utf8_lossy(&pending).into_owned(),
                    ));
                }
                let _ = done.send(());
            });
        }
        drop(done);
        let waiter = shared.clone();
        thread::spawn(move || {
            let status = reap(&mut child, &waiter);
            let deadline = Instant::now() + DRAIN;
            for _ in 0..readers {
                let left = deadline.saturating_duration_since(Instant::now());
                if drained.recv_timeout(left).is_err() {
                    break;
                }
            }
            let _ = events.send_blocking(ShellEvent::Exit {
                code: status.ok().and_then(|status| status.code()),
                elapsed: started.elapsed(),
            });
        });
        receiver
    }
}

impl Running {
    #[cfg(not(any(unix, windows)))]
    pub fn start(
        _: &str,
        _: &Path,
        _: &Path,
        _: &std::collections::HashMap<String, String>,
    ) -> Result<(Self, async_channel::Receiver<ShellEvent>), String> {
        Err("Shell commands from the chat aren't supported on this platform.".into())
    }

    /// Ends the command and everything it started: SIGTERM, then SIGKILL.
    pub fn stop(&self) {
        if *self.shared.exited.lock().unwrap_or_else(|e| e.into_inner()) {
            return;
        }
        signal(&self.shared, false);
        let shared = self.shared.clone();
        thread::spawn(move || {
            thread::sleep(GRACE);
            signal(&shared, true);
        });
    }

    /// Stops and confirms release off the UI thread. A reaped leader is never
    /// signalled again: surviving background children make cleanup conservative.
    pub fn close(&self) -> Result<(), String> {
        self.stop();
        let deadline = Instant::now() + GRACE + Duration::from_secs(2);
        loop {
            let exited = *self.shared.exited.lock().unwrap_or_else(|e| e.into_inner());
            #[cfg(windows)]
            let released = exited && self.job_released();
            #[cfg(not(windows))]
            let released = exited && group_released(self.shared.pid);
            if released {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("A shell command may still have background processes. Its folder was kept; retry cleanup after they exit.".into());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(unix)]
fn group_released(pid: u32) -> bool {
    // A probe only: never signal a group after its leader has been reaped.
    (unsafe { libc::killpg(pid as libc::pid_t, 0) == -1 })
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}
#[cfg(not(any(unix, windows)))]
fn group_released(_: u32) -> bool {
    true
}
#[cfg(windows)]
impl Running {
    /// Windows confirms release when the command's Job Object is empty.
    fn job_released(&self) -> bool {
        self.shared.job.terminate_and_wait().is_ok()
    }
}

#[cfg(unix)]
fn signal(shared: &Shared, kill: bool) {
    let exited = shared.exited.lock().unwrap_or_else(|e| e.into_inner());
    if *exited {
        return;
    }
    // SAFETY: plain syscall on the child's own group (process_group(0)); the waiter
    // reaps only under this lock, so the id is still ours.
    unsafe {
        libc::killpg(
            shared.pid as libc::pid_t,
            if kill { libc::SIGKILL } else { libc::SIGTERM },
        );
    }
}
#[cfg(windows)]
fn signal(shared: &Shared, _: bool) {
    // No polite stop for console-less processes: the job ends the whole tree.
    let _ = shared.job.request_termination();
}
#[cfg(not(any(unix, windows)))]
fn signal(_: &Shared, _: bool) {}

/// Waits for the exit without reaping, then reaps under the lock `signal` takes.
#[cfg(unix)]
fn reap(
    child: &mut std::process::Child,
    shared: &Shared,
) -> std::io::Result<std::process::ExitStatus> {
    loop {
        // SAFETY: siginfo is plain data written by the kernel; WNOWAIT keeps the zombie.
        let result = unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            libc::waitid(
                libc::P_PID,
                shared.pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if result == 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
        {
            break;
        }
    }
    let mut exited = shared.exited.lock().unwrap_or_else(|e| e.into_inner());
    let status = child.wait();
    *exited = true;
    status
}
#[cfg(not(unix))]
fn reap(
    child: &mut std::process::Child,
    shared: &Shared,
) -> std::io::Result<std::process::ExitStatus> {
    let status = child.wait();
    *shared.exited.lock().unwrap_or_else(|e| e.into_inner()) = true;
    status
}

#[cfg(windows)]
mod windows {
    use super::{Running, Shared, ShellEvent, stream};
    use crate::terminal::windows_job::ProcessJob;
    use std::{
        os::windows::{io::AsRawHandle, process::CommandExt},
        path::Path,
        sync::{Arc, Mutex},
        time::Instant,
    };
    use windows_sys::Win32::{
        Foundation::HANDLE,
        System::{
            JobObjects::AssignProcessToJobObject,
            Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
        },
    };

    #[link(name = "ntdll", kind = "raw-dylib")]
    unsafe extern "system" {
        /// Resumes every thread of a process created suspended.
        fn NtResumeProcess(process: HANDLE) -> i32;
    }

    /// Starts `child` suspended, puts it in a new job before it runs any code, then
    /// lets it run: nothing it starts can escape the job.
    pub(super) fn spawn(
        mut child: std::process::Command,
        cwd: &Path,
        vars: &std::collections::HashMap<String, String>,
    ) -> Result<(Running, async_channel::Receiver<ShellEvent>), String> {
        let job =
            ProcessJob::new().map_err(|error| format!("Couldn't run the command: {error}"))?;
        child
            .current_dir(cwd)
            .env_clear()
            .envs(vars)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let started = Instant::now();
        let mut child = child
            .spawn()
            .map_err(|error| format!("Couldn't run the command: {error}"))?;
        let handle = child.as_raw_handle() as HANDLE;
        // SAFETY: the job handle is owned and valid; the child handle stays valid while
        // `child` lives. The suspended child has run no code yet.
        let assigned = unsafe { AssignProcessToJobObject(job.raw() as HANDLE, handle) } != 0;
        // SAFETY: as above; resuming a process we created suspended.
        let resumed = assigned && unsafe { NtResumeProcess(handle) } >= 0;
        if !resumed {
            let error = std::io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Couldn't run the command in its own job: {error}"));
        }
        let shared = Arc::new(Shared {
            pid: child.id(),
            exited: Mutex::new(false),
            job,
        });
        let receiver = stream(child, shared.clone(), started);
        Ok((Running { shared }, receiver))
    }
}

/// Appends `chunk` to `output`, keeping its first and last `limit` bytes with a note
/// where the middle was left out.
pub fn append(output: &mut String, chunk: &str, limit: usize) {
    output.push_str(chunk);
    if output.len() > 3 * limit {
        *output = excerpt(output, limit);
    }
}

/// `output` with at most `limit` bytes from its start and its end.
pub fn excerpt(output: &str, limit: usize) -> String {
    const NOTE: &str = "\n… (output shortened) …\n";
    if output.len() <= 2 * limit + NOTE.len() {
        return output.to_owned();
    }
    // An earlier excerpt keeps its note: cut around it, not again inside the head.
    let (head, tail) = match output.find(NOTE) {
        Some(at) => (&output[..at], &output[at + NOTE.len()..]),
        None => (output, output),
    };
    let mut start = limit.min(head.len());
    while !head.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = tail.len().saturating_sub(limit);
    while !tail.is_char_boundary(end) {
        end += 1;
    }
    format!("{}{NOTE}{}", &head[..start], &tail[end..])
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn run(command: &str) -> (String, Option<i32>) {
        let vars = std::env::vars().collect();
        let (_running, events) =
            Running::start(command, Path::new("/tmp"), Path::new("/bin/sh"), &vars).unwrap();
        let mut output = String::new();
        loop {
            match events.recv_blocking().unwrap() {
                ShellEvent::Output(chunk) => output.push_str(&chunk),
                ShellEvent::Exit { code, .. } => return (output, code),
            }
        }
    }

    #[test]
    fn commands_report_output_and_exit_code() {
        let (output, code) = run("echo hi; echo oops >&2; exit 3");
        assert!(
            output.contains("hi\n") && output.contains("oops\n"),
            "{output}"
        );
        assert_eq!(code, Some(3));
        let (output, _) = run("pwd");
        assert!(output.trim_end().ends_with("tmp"), "{output}");
    }

    #[test]
    fn stop_ends_the_whole_group() {
        let vars = std::env::vars().collect();
        let (running, events) = Running::start(
            "sleep 30 & sleep 30",
            Path::new("/tmp"),
            Path::new("/bin/sh"),
            &vars,
        )
        .unwrap();
        thread::sleep(Duration::from_millis(200));
        let started = Instant::now();
        running.stop();
        loop {
            if let ShellEvent::Exit { code, .. } = events.recv_blocking().unwrap() {
                assert_eq!(code, None);
                break;
            }
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn natural_shell_exit_does_not_confirm_release_of_background_children() {
        let folder = tempfile::tempdir().unwrap();
        let (running, events) = Running::start(
            "sleep 30 & echo $! > child.pid",
            folder.path(),
            Path::new("/bin/sh"),
            &std::env::vars().collect(),
        )
        .unwrap();
        while !matches!(events.recv_blocking().unwrap(), ShellEvent::Exit { .. }) {}
        let pid: i32 = std::fs::read_to_string(folder.path().join("child.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let result = running.close();
        // This is a known child of the test, still bounded by its 30s sleep.
        let alive = unsafe { libc::kill(pid, 0) == 0 };
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        assert!(
            result.is_err(),
            "reaping the shell is not release of its group"
        );
        assert!(
            alive,
            "cleanup must not signal a reaped process group's old identity"
        );
    }

    #[test]
    fn long_output_keeps_its_start_and_end() {
        let mut output = String::new();
        for line in 0..10_000 {
            append(&mut output, &format!("line {line}\n"), 100);
        }
        let kept = excerpt(&output, 100);
        assert!(kept.starts_with("line 0\n"), "{kept}");
        assert!(kept.ends_with("line 9999\n"), "{kept}");
        assert!(kept.contains("(output shortened)") && kept.len() < 400);
        assert_eq!(excerpt("short", 100), "short");
    }
}
