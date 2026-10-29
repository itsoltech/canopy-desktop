//! Alacritty owns PTY I/O and parsing. A supervisor drains/reaps off the UI thread.
use super::environment::ShellEnvironment;
pub use super::metered::InputRefused;
use super::metered::{InputBudget, Metered};
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
    vte::ansi::{Color, CursorShape, Rgb},
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
    budget: Arc<InputBudget>,
    /// Every projection reader (remote views) observes the revision; the local
    /// view keeps its single coalesced `wake` slot.
    observers: Arc<tokio::sync::watch::Sender<u64>>,
}
impl Events {
    /// The emulator's own replies; counted against the queue, never refused.
    fn reply(&self, bytes: Vec<u8>) {
        let Some(tx) = self.tx.get() else {
            return;
        };
        let len = bytes.len();
        if self.budget.reserve(len, false).is_ok() && tx.send(Msg::Input(bytes.into())).is_err() {
            self.budget.written(len);
        }
    }
    fn changed(&self) {
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        self.observers.send_replace(revision);
    }
}
impl EventListener for Events {
    fn send_event(&self, event: Event) {
        let renderable = matches!(event, Event::Wakeup);
        match event {
            Event::PtyWrite(text) => self.reply(text.into_bytes()),
            Event::ChildExit(status) => *self.exit.lock().unwrap() = Some(status),
            Event::Wakeup => self.changed(),
            Event::ColorRequest(index, format) => {
                self.reply(format(palette(index)).into_bytes());
            }
            Event::TextAreaSizeRequest(format) => {
                let size = self.size.lock().unwrap().winsize();
                self.reply(format(size).into_bytes());
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
/// A remote viewport and the reservation admitted before its allocation.
pub(crate) type AdmittedLiveFrame<B> = ((Frame, bool), B);
static LIVE_PTYS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub const INPUT_LIMIT_BYTES: usize = 1_048_576;
/// Alacritty keeps at most this many lines above the screen; older lines are lost.
pub const SCROLLBACK_LINES: usize = 10_000;
/// A read-only slice of the projection, independent of the local view's scroll.
#[derive(Clone)]
pub struct HistoryPage {
    pub frame: Frame,
    /// Lines currently retained above the screen.
    pub retained: usize,
    /// Older lines were already discarded by the scrollback limit.
    pub lost: bool,
    /// Offset of an older page, when lines above this page remain.
    pub previous: Option<usize>,
}
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
            budget: Arc::new(InputBudget::default()),
            observers: Arc::new(tokio::sync::watch::channel(0).0),
        };
        let budget = events.budget.clone();
        let observers = events.clone();
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
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
        let pty = Metered::new(pty, budget.clone());
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
                // Nothing is written after the I/O thread; return its queued bytes.
                budget.close();
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
                observers.changed();
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
    /// Queue bytes for the PTY. Success means queued within the bounded budget,
    /// not read or executed by the program. Input owners are checked by `Run`.
    pub fn queue_input(&self, bytes: Vec<u8>) -> Result<(), InputRefused> {
        if !matches!(self.status(), Status::Running) {
            return Err(InputRefused::NotRunning);
        }
        if bytes.len() > INPUT_LIMIT_BYTES {
            return Err(InputRefused::TooLarge);
        }
        let len = bytes.len();
        self.events.budget.reserve(len, true)?;
        if self.tx.send(Msg::Input(Cow::Owned(bytes))).is_err() {
            self.events.budget.written(len);
            return Err(InputRefused::NotRunning);
        }
        Ok(())
    }
    /// The local view follows its own typing: back to the bottom, selection cleared.
    pub fn reset_local_view(&self) {
        if let Some(mut term) = self.term.try_lock_unfair() {
            term.scroll_display(Scroll::Bottom);
            term.selection = None;
        }
    }
    /// Projection revision; advances on PTY output, resize and process end.
    pub fn revision(&self) -> u64 {
        self.events.revision.load(Ordering::Acquire)
    }
    /// A receiver per reader; values coalesce and never block the parser.
    pub fn observe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.events.observers.subscribe()
    }
    /// Wake the local view and every observer without a projection change.
    pub fn notify(&self) {
        let _ = self.events.wake.try_send(());
        self.events
            .observers
            .send_replace(self.events.revision.load(Ordering::Acquire));
    }
    pub fn size(&self) -> Size {
        *self.events.size.lock().unwrap()
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
        drop(term);
        let _ = self.tx.send(Msg::Resize(size.winsize()));
        self.events.changed();
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
    /// The OSC 8 hyperlink at a visible cell and the visible cells it covers.
    pub fn hyperlink_at(&self, row: usize, column: usize) -> Option<super::links::Link> {
        let term = self.term.try_lock_unfair()?;
        let grid = term.grid();
        let (rows, columns) = (grid.screen_lines(), grid.columns());
        if row >= rows || column >= columns {
            return None;
        }
        let offset = grid.display_offset() as i32;
        let at = |index: usize| {
            grid[Point::new(
                Line((index / columns) as i32 - offset),
                Column(index % columns),
            )]
            .hyperlink()
        };
        let index = row * columns + column;
        let link = at(index)?;
        let same = |index: usize| at(index).as_ref() == Some(&link);
        let (mut first, mut last) = (index, index);
        while first > 0 && same(first - 1) {
            first -= 1;
        }
        while last + 1 < rows * columns && same(last + 1) {
            last += 1;
        }
        Some(super::links::Link {
            url: link.uri().to_owned(),
            spans: super::links::spans((first..=last).map(|i| (i / columns, i % columns))),
        })
    }
    pub fn copy_selection(&self) -> Option<String> {
        self.term.try_lock_unfair()?.selection_to_string()
    }
    pub fn frame(&self) -> Option<Frame> {
        let term = self.term.try_lock_unfair()?;
        Some(project_frame(
            &term,
            None,
            self.events.revision.load(Ordering::Acquire),
        ))
    }
    /// Read a viewport without changing the local view's scroll or selection.
    /// The owner serializes this projection, not an arbitrary ANSI byte suffix.
    pub fn frame_at(&self, offset: usize) -> Option<Frame> {
        let term = self.term.try_lock_unfair()?;
        Some(project_frame(
            &term,
            Some(offset),
            self.events.revision.load(Ordering::Acquire),
        ))
    }
    /// The bottom viewport for a remote view and whether older scrollback was
    /// discarded, read under one lock without changing the local view.
    pub fn live_frame(&self) -> Option<(Frame, bool)> {
        let term = self.term.try_lock_unfair()?;
        let lost = term.grid().history_size() >= SCROLLBACK_LINES;
        Some((
            project_frame(&term, Some(0), self.events.revision.load(Ordering::Acquire)),
            lost,
        ))
    }
    /// Admit a remote projection under the grid lock, before allocating cells or
    /// text. The caller owns the returned reservation for the copied material.
    pub(crate) fn live_frame_bounded<B, E>(
        &self,
        admit: impl FnOnce(usize, usize) -> Result<B, E>,
    ) -> Result<Option<AdmittedLiveFrame<B>>, E> {
        let Some(term) = self.term.try_lock_unfair() else {
            return Ok(None);
        };
        let budget = admit_projection(&term, 0, term.grid().screen_lines(), admit)?;
        let lost = term.grid().history_size() >= SCROLLBACK_LINES;
        Ok(Some((
            (
                project_frame(&term, Some(0), self.events.revision.load(Ordering::Acquire)),
                lost,
            ),
            budget,
        )))
    }
    /// As `history_page`, but admission precedes all projection allocations.
    pub(crate) fn history_page_bounded<B, E>(
        &self,
        offset: usize,
        rows: usize,
        admit: impl FnOnce(usize, usize) -> Result<B, E>,
    ) -> Result<Option<(HistoryPage, B)>, E> {
        let Some(term) = self.term.try_lock_unfair() else {
            return Ok(None);
        };
        let (first, rows) = page_range(&term, offset, rows);
        let budget = admit_projection(&term, first, rows, admit)?;
        Ok(Some((
            project_page(
                &term,
                offset,
                rows,
                self.events.revision.load(Ordering::Acquire),
            ),
            budget,
        )))
    }
    /// `rows` lines ending `offset` lines above the bottom of the screen, read
    /// without changing the local view. Returns None while the parser holds the grid.
    pub fn history_page(&self, offset: usize, rows: usize) -> Option<HistoryPage> {
        let term = self.term.try_lock_unfair()?;
        Some(project_page(
            &term,
            offset,
            rows,
            self.events.revision.load(Ordering::Acquire),
        ))
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}
fn project_frame<T: EventListener>(
    term: &Term<T>,
    remote_offset: Option<usize>,
    revision: u64,
) -> Frame {
    let content = term.renderable_content();
    let grid = term.grid();
    let offset = remote_offset
        .unwrap_or(content.display_offset)
        .min(grid.history_size());
    let mut frame = project_lines(
        term,
        -(offset as i32),
        grid.screen_lines(),
        remote_offset.is_none(),
        revision,
    );
    frame.offset = offset;
    frame
}
fn project_page<T: EventListener>(
    term: &Term<T>,
    offset: usize,
    rows: usize,
    revision: u64,
) -> HistoryPage {
    let grid = term.grid();
    let retained = grid.history_size();
    let screen = grid.screen_lines();
    let (first, rows) = page_range(term, offset, rows);
    let last = first + rows as i32 - 1;
    let top = -(retained as i32);
    let mut frame = project_lines(term, first, (last - first + 1) as usize, false, revision);
    frame.offset = (screen as i32 - 1 - last) as usize;
    HistoryPage {
        frame,
        retained,
        lost: retained >= SCROLLBACK_LINES,
        previous: (first > top).then_some((screen as i32 - first) as usize),
    }
}
fn page_range<T: EventListener>(term: &Term<T>, offset: usize, rows: usize) -> (i32, usize) {
    let grid = term.grid();
    let top = -(grid.history_size() as i32);
    let screen = grid.screen_lines();
    let last = (screen as i32 - 1 - offset.min(grid.history_size() + screen - 1) as i32).max(top);
    let first = (last - rows.clamp(1, 200) as i32 + 1).max(top);
    (first, (last - first + 1) as usize)
}
/// Upper bound for String allocations made by project_lines (including Vec
/// growth for combining characters). No text is copied during admission.
fn admit_projection<T: EventListener, B, E>(
    term: &Term<T>,
    first: i32,
    rows: usize,
    admit: impl FnOnce(usize, usize) -> Result<B, E>,
) -> Result<B, E> {
    let grid = term.grid();
    let mut text_bytes = 0usize;
    for row in 0..rows {
        for column in 0..grid.columns() {
            let cell = &grid[Point::new(Line(first + row as i32), Column(column))];
            let characters = 1usize.saturating_add(cell.zerowidth().map_or(0, |extra| extra.len()));
            text_bytes =
                text_bytes.saturating_add(characters.saturating_mul(4).max(8).saturating_mul(2));
        }
    }
    admit(rows.saturating_mul(grid.columns()), text_bytes)
}
/// Cells of `rows` grid lines starting at `first` (negative lines are scrollback).
/// The cursor is placed relative to these lines and hidden when outside them.
fn project_lines<T: EventListener>(
    term: &Term<T>,
    first: i32,
    rows: usize,
    selection: bool,
    revision: u64,
) -> Frame {
    let content = term.renderable_content();
    let grid = term.grid();
    let size = Size {
        columns: grid.columns(),
        rows,
    };
    let mut cells = Vec::with_capacity(size.rows * size.columns);
    for row in 0..size.rows {
        for column in 0..size.columns {
            let point = Point::new(Line(first + row as i32), Column(column));
            let cell = &grid[point];
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
            cells.push(Cell {
                row,
                column,
                text,
                width: if cell.flags.contains(Flags::WIDE_CHAR) {
                    2
                } else {
                    1
                },
                fg,
                bg,
                flags: cell.flags,
                selected: selection
                    && content
                        .selection
                        .is_some_and(|selection| selection.contains(point)),
            });
        }
    }
    let cursor_row = content.cursor.point.line.0 - first;
    let mut mode = content.mode;
    if cursor_row < 0 || cursor_row >= rows as i32 {
        mode.remove(TermMode::SHOW_CURSOR);
    }
    Frame {
        cells,
        cursor: (cursor_row.max(0) as usize, content.cursor.point.column.0),
        cursor_shape: content.cursor.shape,
        mode,
        offset: 0,
        size,
        revision,
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
/// Default color `index` (ANSI, xterm cube/ramp or named) of the active theme
/// variant. Frames resolve it at projection, so a theme change re-projects.
pub fn palette(index: usize) -> Rgb {
    let hex = crate::appearance::active_terminal().color(index);
    Rgb {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use alacritty_terminal::vte::ansi::NamedColor;
    #[derive(Clone)]
    struct Sink;
    impl EventListener for Sink {
        fn send_event(&self, _: Event) {}
    }
    fn row(frame: &Frame, index: usize) -> String {
        frame
            .cells
            .iter()
            .filter(|cell| cell.row == index)
            .map(|cell| cell.text.as_str())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }
    #[test]
    fn remote_viewport_does_not_scroll_or_select_the_local_terminal() {
        let size = Size::bounded(8, 2);
        let mut term = Term::new(Config::default(), &size, Sink);
        let mut parser: alacritty_terminal::vte::ansi::Processor =
            alacritty_terminal::vte::ansi::Processor::new();
        parser.advance(&mut term, b"one\r\ntwo\r\nthree");
        term.scroll_display(Scroll::Top);
        let point = Point::new(Line(-1), Column(0));
        let mut selection = Selection::new(SelectionType::Simple, point, Side::Left);
        selection.update(Point::new(Line(-1), Column(2)), Side::Right);
        term.selection = Some(selection);
        let before = project_frame(&term, None, 4);
        assert_eq!(row(&before, 0), "one");
        let remote = project_frame(&term, Some(0), 4);
        assert_eq!(row(&remote, 0), "two");
        assert!(remote.cells.iter().all(|cell| !cell.selected));
        assert_eq!(project_frame(&term, None, 4).cells, before.cells);
        assert_eq!(term.grid().display_offset(), before.offset);
        assert_eq!(
            project_frame(&term, Some(usize::MAX), 4).offset,
            term.grid().history_size()
        );
    }
    #[test]
    fn projection_admission_accounts_for_combining_text_and_history_geometry() {
        let size = Size::bounded(80, 24);
        let mut term = Term::new(Config::default(), &size, Sink);
        let mut parser: alacritty_terminal::vte::ansi::Processor =
            alacritty_terminal::vte::ansi::Processor::new();
        let line = format!("e{}\r\n", "\u{301}".repeat(2000));
        for _ in 0..30 {
            parser.advance(&mut term, line.as_bytes());
        }
        let (first, rows) = page_range(&term, usize::MAX, 200);
        let (cells, text): (usize, usize) =
            admit_projection(&term, first, rows, |cells, text| Ok::<_, ()>((cells, text))).unwrap();
        let page = project_page(&term, usize::MAX, 200, 1);
        assert_eq!(cells, page.frame.cells.len());
        assert!(
            text >= page
                .frame
                .cells
                .iter()
                .map(|cell| cell.text.capacity())
                .sum::<usize>()
        );
        assert!(
            page.frame
                .cells
                .iter()
                .any(|cell| cell.text.chars().count() > 1000)
        );
        let refused = admit_projection(&term, first, rows, |_, _| Err::<(), _>("full"));
        assert_eq!(refused, Err("full"));
        // A later admitted read has identical contents: refusal never trims or
        // mutates the parser to fit its allocation budget.
        assert_eq!(
            project_page(&term, usize::MAX, 200, 1).frame.cells,
            page.frame.cells
        );
    }
    #[test]
    fn projection_preserves_alternate_screen_unicode_and_input_modes() {
        let size = Size::bounded(8, 2);
        let mut term = Term::new(Config::default(), &size, Sink);
        let mut parser: alacritty_terminal::vte::ansi::Processor =
            alacritty_terminal::vte::ansi::Processor::new();
        parser.advance(
            &mut term,
            "\u{1b}[?1049h\u{1b}[?2004h\u{1b}[31m界e\u{301}".as_bytes(),
        );
        let frame = project_frame(&term, Some(usize::MAX), 7);
        assert!(
            frame
                .mode
                .contains(TermMode::ALT_SCREEN | TermMode::BRACKETED_PASTE)
        );
        assert_eq!(frame.offset, 0);
        assert_eq!(frame.cells[0].text, "界");
        assert_eq!(frame.cells[0].width, 2);
        assert!(frame.cells[1].flags.contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(frame.cells[2].text, "e\u{301}");
        assert_eq!(frame.cells[0].fg, palette(NamedColor::Red as usize));
        assert_eq!(frame.cursor, (0, 3));
    }
}
