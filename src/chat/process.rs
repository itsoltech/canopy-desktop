//! Long-lived agent subprocess speaking newline-delimited JSON over pipes.
//!
//! Threads: a writer feeds stdin from a channel, a reader parses stdout lines
//! (backpressured through a bounded channel), a collector keeps a redacted stderr
//! tail, and a waiter reaps the child and reports the exit after stdout drains.
//! `close` always finishes in bounded time: stdin EOF → SIGTERM → SIGKILL (group).
use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const STDERR_TAIL_BYTES: usize = 8 * 1024;
/// Extra bytes kept so a secret crossing the tail boundary is still redacted whole.
const STDERR_REDACTION_MARGIN: usize = 4 * 1024;
const EVENT_BACKLOG: usize = 512;
pub const CLOSE_GRACE: Duration = Duration::from_secs(2);
pub const CLOSE_TERMINATE: Duration = Duration::from_secs(5);

pub struct Spec {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub cwd: PathBuf,
    /// The complete child environment; the host environment is not inherited.
    pub environment: HashMap<String, String>,
    /// Values masked in diagnostics and the stderr tail.
    pub secrets: Vec<String>,
}

#[derive(Debug)]
pub enum Event<T> {
    Message(T),
    Diagnostic(String),
    Exited(Exit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    /// Last bytes of stderr, secrets redacted.
    pub stderr: String,
}

/// Parses one stdout line (without the newline); `Ok(None)` skips it.
pub type Parser<T> = fn(&[u8]) -> Result<Option<T>, String>;

pub struct Process {
    input: Mutex<Option<async_channel::Sender<String>>>,
    shared: Arc<Shared>,
    _files: Option<tempfile::TempDir>,
}

struct Shared {
    pid: u32,
    exited: Mutex<bool>,
    changed: Condvar,
}

impl Process {
    pub fn start<T: Send + 'static>(
        spec: Spec,
        files: Option<tempfile::TempDir>,
        parse: Parser<T>,
        max_line: usize,
    ) -> Result<(Self, async_channel::Receiver<Event<T>>), String> {
        let cwd =
            std::fs::canonicalize(&spec.cwd).map_err(|_| "Working directory is unavailable.")?;
        if !cwd.is_dir() {
            return Err("Working directory is not a folder.".into());
        }
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.arguments)
            .current_dir(&cwd)
            .env_clear()
            .envs(&spec.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        platform::configure(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not start {}: {error}", spec.program.display()))?;
        let pid = child.id();
        let (stdin, stdout, stderr) =
            match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                (Some(stdin), Some(stdout), Some(stderr)) => (stdin, stdout, stderr),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Could not connect to the agent process.".into());
                }
            };
        let shared = Arc::new(Shared {
            pid,
            exited: Mutex::new(false),
            changed: Condvar::new(),
        });
        let secrets: Arc<[String]> = spec
            .secrets
            .into_iter()
            .filter(|secret| secret.len() >= 8)
            .collect();
        let (events, receiver) = async_channel::bounded(EVENT_BACKLOG);
        let (input, lines) = async_channel::unbounded::<String>();

        spawn_thread("canopy-chat-stdin", move || writer(stdin, lines))?;
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let collector_tail = tail.clone();
        let (stderr_done, stderr_finished) = mpsc::channel();
        spawn_thread("canopy-chat-stderr", move || {
            collect_tail(stderr, &collector_tail);
            let _ = stderr_done.send(());
        })?;
        let report = Arc::new(ExitReport {
            events: events.clone(),
            tail,
            secrets: secrets.clone(),
            sent: AtomicBool::new(false),
            stderr_finished: Mutex::new(stderr_finished),
        });
        let (status_tx, status_rx) = mpsc::channel();
        let (stdout_done, stdout_finished) = mpsc::channel();
        let reading = Arc::new(AtomicBool::new(false));
        let reader_reading = reading.clone();
        let reader_report = report.clone();
        spawn_thread("canopy-chat-stdout", move || {
            reader(stdout, &events, parse, max_line, &secrets, &reader_reading);
            let _ = stdout_done.send(());
            // Reported after the last stdout message, so the exit is always the final event.
            if let Ok(status) = status_rx.recv() {
                reader_report.send(status);
            }
        })?;
        let waiter_shared = shared.clone();
        spawn_thread("canopy-chat-wait", move || {
            let status = match platform::reap(&mut child, &waiter_shared) {
                Ok(status) => (status.code(), platform::signal(&status)),
                Err(_) => (None, None),
            };
            waiter_shared.changed.notify_all();
            let _ = status_tx.send(status);
            // A slow consumer only delays the reader. A reader stuck in read means a
            // process outside our control holds stdout open: report without it.
            loop {
                match stdout_finished.recv_timeout(CLOSE_GRACE) {
                    Err(mpsc::RecvTimeoutError::Timeout) if reading.load(Ordering::Acquire) => {
                        report.send(status);
                        return;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    _ => return,
                }
            }
        })?;

        Ok((
            Self {
                input: Mutex::new(Some(input)),
                shared,
                _files: files,
            },
            receiver,
        ))
    }

    /// Queues one NDJSON line; never blocks the caller.
    pub fn send(&self, message: &serde_json::Value) -> Result<(), String> {
        let line = format!("{message}\n");
        self.input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .ok_or("The agent session is closing.")?
            .try_send(line)
            .map_err(|_| "The agent process is no longer running.".into())
    }

    pub fn has_exited(&self) -> bool {
        *self.shared.exited.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Blocks until the process is gone. Call off the UI thread.
    pub fn close(&self) -> Result<(), String> {
        self.close_with(CLOSE_GRACE, CLOSE_TERMINATE)
    }

    pub fn close_with(&self, grace: Duration, terminate: Duration) -> Result<(), String> {
        let input = self.input.lock().unwrap_or_else(|e| e.into_inner()).take();
        shutdown(&self.shared, input, grace, terminate)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let input = self.input.lock().unwrap_or_else(|e| e.into_inner()).take();
        if input.is_none() && self.has_exited() {
            return;
        }
        // An owner that forgot to close must not leak a running agent.
        let shared = self.shared.clone();
        let _ = spawn_thread("canopy-chat-close", move || {
            let _ = shutdown(&shared, input, CLOSE_GRACE, CLOSE_TERMINATE);
        });
    }
}

fn shutdown(
    shared: &Shared,
    input: Option<async_channel::Sender<String>>,
    grace: Duration,
    terminate: Duration,
) -> Result<(), String> {
    // Dropping the sender ends the writer, which closes stdin: the CLI's clean exit path.
    drop(input);
    if wait_exit(shared, grace) {
        return Ok(());
    }
    platform::signal_group(shared, platform::Termination::Graceful);
    if wait_exit(shared, terminate) {
        return Ok(());
    }
    platform::signal_group(shared, platform::Termination::Forced);
    if wait_exit(shared, Duration::from_secs(2)) {
        Ok(())
    } else {
        Err("The agent process did not stop.".into())
    }
}

fn wait_exit(shared: &Shared, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let mut exited = shared.exited.lock().unwrap_or_else(|e| e.into_inner());
    while !*exited {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        exited = shared
            .changed
            .wait_timeout(exited, deadline - now)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
    true
}

fn spawn_thread(name: &str, f: impl FnOnce() + Send + 'static) -> Result<(), String> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .map(drop)
        .map_err(|_| "Could not start an agent session worker.".into())
}

fn writer(mut stdin: std::process::ChildStdin, lines: async_channel::Receiver<String>) {
    while let Ok(line) = lines.recv_blocking() {
        if stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.flush())
            .is_err()
        {
            break;
        }
    }
}

struct ExitReport<T> {
    events: async_channel::Sender<Event<T>>,
    tail: Arc<Mutex<VecDeque<u8>>>,
    secrets: Arc<[String]>,
    sent: AtomicBool,
    stderr_finished: Mutex<mpsc::Receiver<()>>,
}
impl<T> ExitReport<T> {
    /// Sends `Exited` once, whichever of reader or waiter gets here first.
    fn send(&self, (code, signal): (Option<i32>, Option<i32>)) {
        if self.sent.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self
            .stderr_finished
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(Duration::from_millis(500));
        let bytes: Vec<u8> = self
            .tail
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .copied()
            .collect();
        let stderr = tail_text(&redact(&String::from_utf8_lossy(&bytes), &self.secrets));
        let _ = self.events.send_blocking(Event::Exited(Exit {
            code,
            signal,
            stderr,
        }));
    }
}

/// The last `STDERR_TAIL_BYTES` of already redacted text, cut at a char boundary.
fn tail_text(text: &str) -> String {
    let mut start = text.len().saturating_sub(STDERR_TAIL_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].trim().to_owned()
}

fn reader<T>(
    stdout: std::process::ChildStdout,
    events: &async_channel::Sender<Event<T>>,
    parse: Parser<T>,
    max_line: usize,
    secrets: &[String],
    reading: &AtomicBool,
) {
    let mut stdout = BufReader::with_capacity(64 * 1024, stdout);
    let mut line = Vec::new();
    loop {
        line.clear();
        reading.store(true, Ordering::Release);
        let read = read_line(&mut stdout, &mut line, max_line);
        reading.store(false, Ordering::Release);
        let event = match read {
            Ok(Line::End) => return,
            Ok(Line::Oversized(size)) => Event::Diagnostic(format!(
                "Ignored agent output line larger than {} MiB ({size} bytes).",
                max_line / (1024 * 1024)
            )),
            Ok(Line::Complete) => match parse(&line) {
                Ok(Some(message)) => Event::Message(message),
                Ok(None) => continue,
                Err(error) => Event::Diagnostic(redact(&error, secrets)),
            },
            Err(_) => return,
        };
        // A dropped consumer still needs the pipe drained so the child never blocks.
        let _ = events.send_blocking(event);
    }
}

enum Line {
    Complete,
    Oversized(usize),
    End,
}

/// Reads up to the next newline, keeping at most `max` bytes in `line`.
fn read_line(reader: &mut impl BufRead, line: &mut Vec<u8>, max: usize) -> std::io::Result<Line> {
    let mut total = 0;
    loop {
        let buffer = match reader.fill_buf() {
            Ok(buffer) => buffer,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if buffer.is_empty() {
            return Ok(match total {
                0 => Line::End,
                _ if total > max => Line::Oversized(total),
                _ => Line::Complete,
            });
        }
        let (chunk, newline) = match buffer.iter().position(|b| *b == b'\n') {
            Some(at) => (&buffer[..at], Some(at)),
            None => (buffer, None),
        };
        if total + chunk.len() <= max {
            line.extend_from_slice(chunk);
        } else {
            line.clear();
        }
        total += chunk.len();
        let consumed = newline.map_or(chunk.len(), |at| at + 1);
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(if total > max {
                Line::Oversized(total)
            } else {
                Line::Complete
            });
        }
    }
}

fn collect_tail(mut stderr: std::process::ChildStderr, tail: &Mutex<VecDeque<u8>>) {
    let mut buffer = [0; 4096];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) => return,
            Ok(length) => {
                let mut tail = tail.lock().unwrap_or_else(|e| e.into_inner());
                tail.extend(&buffer[..length]);
                let excess = tail
                    .len()
                    .saturating_sub(STDERR_TAIL_BYTES + STDERR_REDACTION_MARGIN);
                tail.drain(..excess);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

fn redact(text: &str, secrets: &[String]) -> String {
    secrets.iter().fold(text.to_owned(), |text, secret| {
        text.replace(secret.as_str(), "[redacted]")
    })
}

#[cfg(unix)]
mod platform {
    use super::Shared;
    use std::os::unix::process::{CommandExt, ExitStatusExt};

    pub enum Termination {
        Graceful,
        Forced,
    }

    pub fn configure(command: &mut std::process::Command) {
        // Own process group: closing the session also stops tools the agent started.
        command.process_group(0);
        // SAFETY: only async-signal-safe libc calls between fork and exec. GPUI
        // background threads may block signals; the agent must start unmasked.
        unsafe {
            command.pre_exec(|| {
                let mut empty = std::mem::zeroed();
                libc::sigemptyset(&mut empty);
                libc::pthread_sigmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
                Ok(())
            });
        }
    }

    pub fn signal(status: &std::process::ExitStatus) -> Option<i32> {
        status.signal()
    }

    /// Observes the exit without reaping, then reaps under the lock: `signal_group`
    /// holds the same lock, so it never signals a pid that could be reused.
    pub fn reap(
        child: &mut std::process::Child,
        shared: &Shared,
    ) -> std::io::Result<std::process::ExitStatus> {
        await_exit(shared.pid);
        let mut exited = shared.exited.lock().unwrap_or_else(|e| e.into_inner());
        // The unreaped leader still reserves the group id: stop tools it left behind.
        // SAFETY: plain syscall on our own process group.
        unsafe {
            libc::killpg(shared.pid as libc::pid_t, libc::SIGTERM);
        }
        let status = child.wait();
        *exited = true;
        status
    }

    /// Blocks until the child exits, leaving it unreaped (WNOWAIT).
    fn await_exit(pid: u32) {
        loop {
            // SAFETY: siginfo is plain data written by the kernel; WNOWAIT keeps the zombie.
            let result = unsafe {
                let mut info: libc::siginfo_t = std::mem::zeroed();
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if result == 0
                || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return;
            }
        }
    }

    pub fn signal_group(shared: &Shared, termination: Termination) {
        let exited = shared.exited.lock().unwrap_or_else(|e| e.into_inner());
        if *exited {
            return;
        }
        let signal = match termination {
            Termination::Graceful => libc::SIGTERM,
            Termination::Forced => libc::SIGKILL,
        };
        // The waiter reaps only while holding this lock, so the pid is still ours.
        // SAFETY: plain syscall; the group id is the child's pid (process_group(0)).
        unsafe {
            libc::killpg(shared.pid as libc::pid_t, signal);
        }
        drop(exited);
    }
}

#[cfg(windows)]
mod platform {
    use super::Shared;
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{CREATE_NO_WINDOW, OpenProcess, PROCESS_TERMINATE, TerminateProcess},
    };

    pub enum Termination {
        Graceful,
        Forced,
    }

    pub fn configure(command: &mut std::process::Command) {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    pub fn signal(_: &std::process::ExitStatus) -> Option<i32> {
        None
    }

    /// The Child handle keeps the process object (and its pid) alive until dropped,
    /// so waiting outside the lock is safe and keeps `signal_group` unblocked.
    pub fn reap(
        child: &mut std::process::Child,
        shared: &Shared,
    ) -> std::io::Result<std::process::ExitStatus> {
        let status = child.wait();
        *shared.exited.lock().unwrap_or_else(|e| e.into_inner()) = true;
        status
    }

    /// Windows has no graceful signal for console-less children; both steps terminate.
    /// Unverified platform: only the direct child is stopped.
    pub fn signal_group(shared: &Shared, _: Termination) {
        let exited = shared.exited.lock().unwrap_or_else(|e| e.into_inner());
        if *exited {
            return;
        }
        // SAFETY: the handle is checked and closed; the pid is still unreaped (lock held).
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, shared.pid);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
        drop(exited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(input: &[u8], max: usize) -> Vec<Result<Vec<u8>, usize>> {
        let mut reader = std::io::BufReader::with_capacity(4, input);
        let mut out = vec![];
        let mut line = vec![];
        loop {
            line.clear();
            match read_line(&mut reader, &mut line, max).unwrap() {
                Line::End => return out,
                Line::Complete => out.push(Ok(line.clone())),
                Line::Oversized(size) => out.push(Err(size)),
            }
        }
    }

    #[test]
    fn lines_are_split_across_buffer_boundaries_and_capped() {
        assert_eq!(
            lines(b"abcdefgh\nij\n\nklmnopqrstu\nlast", 8),
            [
                Ok(b"abcdefgh".to_vec()),
                Ok(b"ij".to_vec()),
                Ok(vec![]),
                Err(11),
                Ok(b"last".to_vec()),
            ]
        );
    }

    #[test]
    fn secrets_are_redacted() {
        assert_eq!(
            redact("key sk-ant-123456 failed", &["sk-ant-123456".into()]),
            "key [redacted] failed"
        );
    }
}
