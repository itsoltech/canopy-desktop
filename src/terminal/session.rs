//! Alacritty owns PTY I/O and parsing. A supervisor drains/reaps off the UI thread.
use super::environment::ShellEnvironment;
pub use alacritty_terminal::term::TermMode as Mode;
use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
    sync::FairMutex,
    term::{Config, Term, TermMode, cell::Flags},
    tty,
    vte::ansi::{Color, CursorShape, NamedColor, Rgb},
};
use std::{
    borrow::Cow,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
    Failed(String),
}
#[derive(Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub cwd: PathBuf,
}
impl LaunchSpec {
    pub fn for_pane(
        pane: &crate::state::workspace::Pane,
        env: &ShellEnvironment,
    ) -> Result<Self, String> {
        let cwd = pane
            .metadata
            .cwd
            .clone()
            .ok_or("Pane has no working directory.")?;
        if pane.metadata.profile_id.is_some() || pane.metadata.resume_id.is_some() {
            return Err(
                "This pane needs a tool profile/resume adapter that is not implemented yet.".into(),
            );
        }
        let mut arguments = pane.metadata.arguments.clone();
        if pane.tool == "shell" && arguments.is_empty() {
            arguments = env.default_shell_arguments();
        }
        let (program, arguments) = env.prepare_launch(env.resolve(&pane.tool)?, arguments)?;
        Ok(Self {
            program,
            arguments,
            cwd,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub columns: usize,
    pub rows: usize,
}
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.columns
    }
}
impl Size {
    pub fn bounded(columns: usize, rows: usize) -> Self {
        Self {
            columns: columns.clamp(2, 500),
            rows: rows.clamp(1, 200),
        }
    }
    fn winsize(self) -> WindowSize {
        WindowSize {
            num_lines: self.rows as u16,
            num_cols: self.columns as u16,
            cell_width: 8,
            cell_height: 17,
        }
    }
}
#[derive(Clone)]
struct Events {
    wake: async_channel::Sender<()>,
    tx: Arc<OnceLock<EventLoopSender>>,
    exit: Arc<Mutex<Option<std::process::ExitStatus>>>,
    size: Arc<Mutex<Size>>,
    revision: Arc<AtomicU64>,
}
impl EventListener for Events {
    fn send_event(&self, event: Event) {
        let renderable = matches!(event, Event::Wakeup);
        match event {
            Event::PtyWrite(text) => {
                if let Some(tx) = self.tx.get() {
                    let _ = tx.send(Msg::Input(text.into_bytes().into()));
                }
            }
            Event::ChildExit(status) => *self.exit.lock().unwrap() = Some(status),
            Event::Wakeup => {
                self.revision.fetch_add(1, Ordering::Relaxed);
            }
            Event::ColorRequest(index, format) => {
                if let Some(tx) = self.tx.get() {
                    let _ = tx.send(Msg::Input(format(palette(index)).into_bytes().into()));
                }
            }
            Event::TextAreaSizeRequest(format) => {
                if let Some(tx) = self.tx.get() {
                    let _ = tx.send(Msg::Input(
                        format(self.size.lock().unwrap().winsize())
                            .into_bytes()
                            .into(),
                    ));
                }
            }
            // Clipboard access is user-initiated through copy/paste, not OSC requests.
            Event::ClipboardLoad(..) | Event::ClipboardStore(..) => {}
            _ => {}
        }
        if renderable {
            let _ = self.wake.try_send(());
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub row: usize,
    pub column: usize,
    pub text: String,
    pub width: usize,
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: Flags,
    pub selected: bool,
}
#[derive(Clone)]
pub struct Frame {
    pub cells: Vec<Cell>,
    pub cursor: (usize, usize),
    pub cursor_shape: CursorShape,
    pub mode: TermMode,
    pub offset: usize,
    pub size: Size,
    /// Advances only when the parser publishes a renderable PTY update.
    pub revision: u64,
}
static LIVE_PTYS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub const INPUT_LIMIT_BYTES: usize = 1_048_576;
struct PtyPermit;
impl PtyPermit {
    fn acquire() -> Result<Self, String> {
        LIVE_PTYS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < 64).then_some(n + 1)
            })
            .map(|_| Self)
            .map_err(|_| "The limit of 64 running terminal processes has been reached.".into())
    }
}
impl Drop for PtyPermit {
    fn drop(&mut self) {
        LIVE_PTYS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(windows)]
fn close_temporary_files(
    files: &Arc<Mutex<Option<tempfile::TempDir>>>,
    retained: &Arc<Mutex<Option<PathBuf>>>,
) -> Result<(), String> {
    let files = files.lock().unwrap().take();
    let Some(files) = files else {
        return Ok(());
    };
    let path = files.path().to_owned();
    if let Err(error) = files.close() {
        *retained.lock().unwrap() = Some(path);
        Err(format!(
            "Could not remove terminal configuration after cleanup: {error}"
        ))
    } else {
        Ok(())
    }
}

pub struct Session {
    term: Arc<FairMutex<Term<Events>>>,
    tx: EventLoopSender,
    status: Arc<Mutex<Status>>,
    #[cfg(windows)]
    process_status: Arc<Mutex<Status>>,
    stopped: AtomicBool,
    pub notifications: async_channel::Receiver<()>,
    cleanup_done: async_channel::Receiver<()>,
    cleanup_result: Arc<Mutex<Option<Result<(), String>>>>,
    #[cfg(windows)]
    retained_files: Arc<Mutex<Option<PathBuf>>>,
    events: Events,
    #[cfg(windows)]
    job: Arc<super::windows_job::ProcessJob>,
}
impl Session {
    pub fn start(spec: LaunchSpec, env: &ShellEnvironment, size: Size) -> Result<Self, String> {
        Self::start_with_config(spec, env, size, None)
    }
    pub fn start_with_config(
        spec: LaunchSpec,
        env: &ShellEnvironment,
        size: Size,
        files: Option<tempfile::TempDir>,
    ) -> Result<Self, String> {
        let permit = PtyPermit::acquire()?;
        let cwd =
            std::fs::canonicalize(&spec.cwd).map_err(|_| "Working directory is unavailable.")?;
        if !cwd.is_dir() {
            return Err("Working directory is not a folder.".into());
        }
        let (wake, notifications) = async_channel::bounded(1);
        let (cleanup_tx, cleanup_done) = async_channel::bounded::<()>(1);
        let cleanup_result = Arc::new(Mutex::new(None));
        let supervisor_cleanup_result = cleanup_result.clone();
        #[cfg(windows)]
        let retained_files = Arc::new(Mutex::new(None));
        #[cfg(windows)]
        let supervisor_retained_files = retained_files.clone();
        let exit = Arc::new(Mutex::new(None));
        let events = Events {
            wake: wake.clone(),
            tx: Arc::new(OnceLock::new()),
            exit: exit.clone(),
            size: Arc::new(Mutex::new(size)),
            revision: Arc::new(AtomicU64::new(0)),
        };
        let config = Config {
            scrolling_history: 10_000,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, events.clone())));
        #[cfg(windows)]
        let job = Arc::new(
            super::windows_job::ProcessJob::new()
                .map_err(|error| format!("Could not create terminal process job: {error}"))?,
        );
        #[cfg(windows)]
        let command_script = spec
            .program
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("cmd"))
            && spec.arguments.get(..4)
                == Some(&["/d".into(), "/s".into(), "/v:off".into(), "/c".into()]);
        let options = tty::Options {
            shell: Some(tty::Shell::new(
                spec.program.to_string_lossy().into_owned(),
                spec.arguments,
            )),
            working_directory: Some(cwd),
            drain_on_exit: true,
            env: env.vars.clone(),
            #[cfg(target_os = "windows")]
            escape_args: !command_script,
            #[cfg(target_os = "windows")]
            windows_job: Some(job.raw()),
        };
        let pty = super::spawn::pty(&options, size.winsize())
            .map_err(|e| format!("Could not start the program: {e}"))?;
        #[cfg(unix)]
        let (pid, control) = (
            pty.child().id() as i32,
            pty.file()
                .try_clone()
                .map_err(|_| "Could not retain PTY control.")?,
        );
        #[cfg(unix)]
        let pty = super::clear_scrollback::ClearPty::new(pty)
            .map_err(|_| "Could not initialize PTY output reader.")?;
        let io = EventLoop::new(term.clone(), events.clone(), pty, true, false)
            .map_err(|_| "Could not initialize PTY I/O.")?;
        let tx = io.channel();
        let _ = events.tx.set(tx.clone());
        let status = Arc::new(Mutex::new(Status::Running));
        #[cfg(windows)]
        let process_status = Arc::new(Mutex::new(Status::Running));
        let result_status = status.clone();
        #[cfg(windows)]
        let result_process_status = process_status.clone();
        let handle = io.spawn();
        let files = Arc::new(Mutex::new(files));
        let supervisor_files = files.clone();
        #[cfg(windows)]
        let supervisor_job = job.clone();
        std::thread::Builder::new()
            .name("canopy-pty-supervisor".into())
            .spawn(move || {
                let _permit = permit;
                let result = handle.join();
                let code = *exit.lock().unwrap();
                #[cfg(unix)]
                unsafe {
                    use std::os::fd::AsRawFd;
                    let foreground = libc::tcgetpgrp(control.as_raw_fd());
                    if foreground > 0
                        && foreground != libc::getpgrp()
                        && libc::getsid(foreground) == pid
                    {
                        libc::kill(-foreground, libc::SIGKILL);
                    }
                    // A tool may leave children in its original process group.
                    libc::kill(-pid, libc::SIGKILL);
                    if code.is_none() {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
                // Drop Alacritty PTY and reap the child before reporting completion.
                drop(result);
                #[cfg(unix)]
                drop(control);
                #[cfg(windows)]
                let mut cleanup = supervisor_job
                    .terminate_and_wait()
                    .map_err(|error| format!("Could not finish terminal process cleanup: {error}"));
                #[cfg(windows)]
                if cleanup.is_ok() {
                    cleanup = close_temporary_files(&supervisor_files, &supervisor_retained_files);
                }
                #[cfg(not(windows))]
                let cleanup: Result<(), String> = Ok(());
                #[cfg(windows)]
                drop(supervisor_job);
                #[cfg(windows)]
                match &cleanup {
                    Ok(()) => {}
                    Err(_) => {
                        if let Some(files) = supervisor_files.lock().unwrap().take() {
                            *supervisor_retained_files.lock().unwrap() = Some(files.keep());
                        }
                    }
                }
                #[cfg(not(windows))]
                drop(supervisor_files.lock().unwrap().take());
                let final_status = match code {
                    Some(status) => {
                        #[cfg(unix)]
                        let signal = {
                            use std::os::unix::process::ExitStatusExt;
                            status.signal()
                        };
                        #[cfg(not(unix))]
                        let signal = None;
                        Status::Exited {
                            code: status.code(),
                            signal,
                        }
                    }
                    None => Status::Failed("Terminal I/O stopped.".into()),
                };
                #[cfg(windows)]
                {
                    *result_process_status.lock().unwrap() = final_status.clone();
                }
                let final_status = if let Err(error) = &cleanup {
                    Status::Failed(error.clone())
                } else {
                    final_status
                };
                *supervisor_cleanup_result.lock().unwrap() = Some(cleanup);
                *result_status.lock().unwrap() = final_status;
                let _ = wake.try_send(());
                drop(cleanup_tx);
            })
            .map_err(|_| {
                let _ = tx.send(Msg::Shutdown);
                #[cfg(unix)]
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                    libc::kill(pid, libc::SIGKILL);
                }
                #[cfg(windows)]
                let cleanup = job.terminate_and_wait();
                #[cfg(windows)]
                if cleanup.is_err()
                    && let Some(files) = files.lock().unwrap().take()
                {
                    let _ = files.keep();
                } else {
                    drop(files.lock().unwrap().take());
                }
                #[cfg(not(windows))]
                drop(files.lock().unwrap().take());
                "Could not start PTY supervisor."
            })?;
        Ok(Self {
            term,
            tx,
            status,
            #[cfg(windows)]
            process_status,
            stopped: AtomicBool::new(false),
            notifications,
            cleanup_done,
            cleanup_result,
            #[cfg(windows)]
            retained_files,
            events,
            #[cfg(windows)]
            job,
        })
    }
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    pub fn input(&self, bytes: Vec<u8>) -> bool {
        if matches!(self.status(), Status::Running) && bytes.len() <= INPUT_LIMIT_BYTES {
            if let Some(mut term) = self.term.try_lock_unfair() {
                term.scroll_display(Scroll::Bottom);
                term.selection = None;
            }
            let _ = self.tx.send(Msg::Input(Cow::Owned(bytes)));
            true
        } else {
            false
        }
    }
    pub fn stop(&self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            #[cfg(windows)]
            let _ = self.job.request_termination();
            let _ = self.tx.send(Msg::Shutdown);
        }
    }
    pub async fn wait_closed(&self) -> Result<(), String> {
        let _ = self.cleanup_done.recv().await;
        self.cleanup_result()
            .ok_or("Terminal cleanup result was lost.".to_owned())?
    }
    pub fn cleanup_result(&self) -> Option<Result<(), String>> {
        self.cleanup_result.lock().unwrap().clone()
    }
    pub async fn retry_cleanup(&self) -> Result<(), String> {
        if let Some(Ok(())) = self.cleanup_result() {
            return Ok(());
        }
        #[cfg(windows)]
        {
            let job = self.job.clone();
            let result_state = self.cleanup_result.clone();
            let retained_files = self.retained_files.clone();
            let (tx, rx) = async_channel::bounded(1);
            std::thread::Builder::new()
                .name("canopy-pty-cleanup-retry".into())
                .spawn(move || {
                    let result = job
                        .terminate_and_wait()
                        .map_err(|error| {
                            format!("Could not finish terminal process cleanup: {error}")
                        })
                        .and_then(|()| {
                            let path = retained_files.lock().unwrap().take();
                            if let Some(path) = path
                                && let Err(error) = std::fs::remove_dir_all(&path)
                            {
                                *retained_files.lock().unwrap() = Some(path);
                                return Err(format!(
                                    "Could not remove terminal configuration after cleanup: {error}"
                                ));
                            }
                            Ok(())
                        });
                    *result_state.lock().unwrap() = Some(result.clone());
                    let _ = tx.send_blocking(result);
                })
                .map_err(|error| format!("Could not retry terminal process cleanup: {error}"))?;
            let result = rx
                .recv()
                .await
                .map_err(|_| "Terminal cleanup retry result was lost.".to_owned())?;
            if result.is_ok() {
                *self.status.lock().unwrap() = self.process_status.lock().unwrap().clone();
            }
            result
        }
        #[cfg(not(windows))]
        {
            self.wait_closed().await
        }
    }
    pub fn resize(&self, size: Size) -> bool {
        if size == *self.events.size.lock().unwrap() {
            return true;
        }
        let Some(mut term) = self.term.try_lock_unfair() else {
            return false;
        };
        term.resize(size);
        *self.events.size.lock().unwrap() = size;
        let _ = self.tx.send(Msg::Resize(size.winsize()));
        let _ = self.events.wake.try_send(());
        true
    }
    pub fn scroll(&self, lines: i32) {
        if let Some(mut term) = self.term.try_lock_unfair() {
            term.scroll_display(Scroll::Delta(lines));
        }
    }
    pub fn select(&self, row: usize, column: usize, start: bool) {
        let size = *self.events.size.lock().unwrap();
        if let Some(mut term) = self.term.try_lock_unfair() {
            let point = Point::new(
                Line(row as i32 - term.grid().display_offset() as i32),
                Column(column.min(size.columns - 1)),
            );
            if start {
                term.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
            } else if let Some(selection) = term.selection.as_mut() {
                selection.update(point, Side::Right);
            }
        }
    }
    pub fn copy_selection(&self) -> Option<String> {
        self.term.try_lock_unfair()?.selection_to_string()
    }
    pub fn frame(&self) -> Option<Frame> {
        let size = *self.events.size.lock().unwrap();
        let term = self.term.try_lock_unfair()?;
        let content = term.renderable_content();
        let cells = content
            .display_iter
            .map(|cell| {
                let row = (cell.point.line.0 + content.display_offset as i32).max(0) as usize;
                let mut text = cell.c.to_string();
                if let Some(extra) = cell.zerowidth() {
                    text.extend(extra);
                }
                let mut fg = resolve(cell.fg, content.colors);
                let mut bg = resolve(cell.bg, content.colors);
                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if cell.flags.contains(Flags::DIM) {
                    fg = Rgb {
                        r: fg.r / 2,
                        g: fg.g / 2,
                        b: fg.b / 2,
                    };
                }
                Cell {
                    row,
                    column: cell.point.column.0,
                    text,
                    width: if cell.flags.contains(Flags::WIDE_CHAR) {
                        2
                    } else {
                        1
                    },
                    fg,
                    bg,
                    flags: cell.flags,
                    selected: content.selection.is_some_and(|s| s.contains(cell.point)),
                }
            })
            .collect();
        Some(Frame {
            cells,
            cursor: (
                (content.cursor.point.line.0 + content.display_offset as i32).max(0) as usize,
                content.cursor.point.column.0,
            ),
            cursor_shape: content.cursor.shape,
            mode: content.mode,
            offset: content.display_offset,
            size,
            revision: self.events.revision.load(Ordering::Relaxed),
        })
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}
fn resolve(color: Color, colors: &alacritty_terminal::term::color::Colors) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => colors[i as usize].unwrap_or_else(|| palette(i as usize)),
        Color::Named(n) => colors[n].unwrap_or_else(|| palette(n as usize)),
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    fn wait_for_cleanup(session: Session) -> (Session, Result<(), String>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let result = futures_lite::future::block_on(session.wait_closed());
            tx.send((session, result)).unwrap();
        });
        let result = rx
            .recv_timeout(std::time::Duration::from_secs(8))
            .expect("cleanup result timed out");
        waiter.join().unwrap();
        result
    }

    #[test]
    fn natural_exit_cleanup_failure_is_retained_and_can_be_reverified() {
        let shell = std::env::var_os("ComSpec")
            .map(PathBuf::from)
            .expect("ComSpec");
        let environment = ShellEnvironment {
            shell: shell.clone(),
            vars: std::env::vars().collect(),
        };
        let files = tempfile::tempdir().unwrap();
        let files_path = files.path().to_owned();
        std::fs::write(files_path.join("retained"), b"fixture").unwrap();
        super::super::windows_job::force_next_cleanup_failure();
        let session = Session::start_with_config(
            LaunchSpec {
                program: shell,
                arguments: vec!["/d".into(), "/s".into(), "/c".into(), "exit 0".into()],
                cwd: std::env::current_dir().unwrap(),
            },
            &environment,
            Size::bounded(80, 24),
            Some(files),
        )
        .unwrap();
        let (session, result) = wait_for_cleanup(session);
        assert!(result.as_ref().is_err_and(|error| error.contains("forced")));
        assert!(files_path.exists());
        assert!(
            session
                .cleanup_result()
                .is_some_and(|result| result.is_err())
        );
        assert!(futures_lite::future::block_on(session.retry_cleanup()).is_ok());
        assert_eq!(session.cleanup_result(), Some(Ok(())));
        assert!(!files_path.exists());
        assert_eq!(
            session.status(),
            Status::Exited {
                code: Some(0),
                signal: None
            }
        );
    }

    #[test]
    fn locked_temporary_file_fails_initial_cleanup_and_succeeds_on_retry() {
        use std::os::windows::fs::OpenOptionsExt;

        let shell = std::env::var_os("ComSpec")
            .map(PathBuf::from)
            .expect("ComSpec");
        let environment = ShellEnvironment {
            shell: shell.clone(),
            vars: std::env::vars().collect(),
        };
        let files = tempfile::tempdir().unwrap();
        let files_path = files.path().to_owned();
        let locked_path = files_path.join("locked-config");
        std::fs::write(&locked_path, b"fixture").unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_path)
            .unwrap();
        let session = Session::start_with_config(
            LaunchSpec {
                program: shell,
                arguments: vec!["/d".into(), "/s".into(), "/c".into(), "exit 0".into()],
                cwd: std::env::current_dir().unwrap(),
            },
            &environment,
            Size::bounded(80, 24),
            Some(files),
        )
        .unwrap();
        let (session, result) = wait_for_cleanup(session);
        assert!(
            result
                .as_ref()
                .is_err_and(|error| error.contains("terminal configuration"))
        );
        assert!(files_path.exists());
        drop(locked);
        assert!(futures_lite::future::block_on(session.retry_cleanup()).is_ok());
        assert_eq!(session.cleanup_result(), Some(Ok(())));
        assert!(!files_path.exists());
    }
}
pub fn palette(index: usize) -> Rgb {
    let ansi = [
        0x1b1b1b, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xc8c8c8, 0x666666,
        0xff7a85, 0xb5e890, 0xffd68a, 0x80c7ff, 0xdf9aff, 0x75d5df, 0xffffff,
    ];
    let hex = if index < 16 {
        ansi[index]
    } else if index < 232 {
        let n = index - 16;
        let cube = |n: usize| if n == 0 { 0 } else { 55 + n * 40 };
        (cube(n / 36) << 16) | (cube((n / 6) % 6) << 8) | cube(n % 6)
    } else if index < 256 {
        let v = 8 + (index - 232) * 10;
        (v << 16) | (v << 8) | v
    } else if index == NamedColor::Background as usize {
        0x202020
    } else {
        0xc8c8c8
    };
    Rgb {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}
