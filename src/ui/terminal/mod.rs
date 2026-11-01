//! Native GPUI terminal surface. The PTY/emulator live in the library service.
mod cursor;
mod input;
mod lifecycle;
mod painter;
mod view;
mod viewport;
use canopy_desktop::{
    state::workspace::Pane,
    terminal::{
        control::{Holder, Lease, Run},
        environment::ShellEnvironment,
        session::{Frame, Mode, Session, Size, Status},
    },
};
use gpui_kit::*;
use std::sync::Arc;

actions!(
    terminal_actions,
    [
        CopyTerminal,
        PasteTerminal,
        TerminalTab,
        TerminalBacktab,
        TerminalShiftEnter
    ]
);
/// Copy/paste come from the command keymap (Settings → Shortcuts); Tab,
/// Shift+Tab and Shift+Enter always go to the terminal.
pub(crate) fn init(cx: &mut App) {
    crate::ui::keymap::init(cx);
}
pub struct TerminalStateChanged;
impl EventEmitter<TerminalStateChanged> for TerminalView {}
struct CleanupOperation {
    generation: u64,
    _task: Task<()>,
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
}
pub struct TerminalView {
    pane: Pane,
    /// Remote identity of this terminal for its whole life; never a pane index.
    terminal_id: String,
    agent_run: Option<String>,
    env: Option<Arc<ShellEnvironment>>,
    session: Option<Arc<Session>>,
    /// The current run and its single input/resize owner (local or a device).
    run: Option<Arc<Run>>,
    /// Owner granted to the next run: whoever starts it.
    next_holder: Holder,
    /// Lease as of the last refresh; render reads this copy.
    lease: Option<Lease>,
    lease_timer: Option<Task<()>>,
    cleanup: Option<CleanupOperation>,
    frame: Option<Frame>,
    cursor: cursor::CursorStabilizer,
    cursor_task: Option<Task<()>>,
    status: Option<Status>,
    error: Option<String>,
    input_error: Option<String>,
    starting: bool,
    stopping: bool,
    generation: u64,
    focus: FocusHandle,
    visible: bool,
    focused: bool,
    needs_focus: bool,
    bounds: Bounds<Pixels>,
    columns: Size,
    anchor: canopy_desktop::terminal::geometry::GridAnchor,
    /// Cell of the terminal font the grid was last fitted with.
    cell: canopy_desktop::terminal::geometry::CellSize,
    settle_deadline: std::time::Instant,
    settle_task: Option<Task<()>>,
    preedit: String,
    wheel: f32,
    /// Link under the pointer, underlined; a click on it opens it.
    link: Option<canopy_desktop::terminal::links::Link>,
    /// Link the left button went down on, and the cell; opens if released there.
    pressed_link: Option<(canopy_desktop::terminal::links::Link, (usize, usize))>,
    task: Option<Task<()>>,
    input_control: Option<Task<()>>,
    pump: Option<Task<()>>,
}
impl TerminalView {
    /// Real terminal painter with deterministic cells and no process, for native UI review.
    #[cfg(test)]
    pub(super) fn visual_fixture(pane: Pane, text: &str, cx: &mut Context<Self>) -> Self {
        use alacritty_terminal::{
            term::cell::Flags,
            vte::ansi::{CursorShape, Rgb},
        };
        use canopy_desktop::terminal::session::Cell;
        let mut view = Self::new(pane, Err(String::new()), cx);
        view.error = None;
        let lines: Vec<_> = text.lines().collect();
        let cells = lines
            .iter()
            .enumerate()
            .flat_map(|(row, line)| {
                line.chars().enumerate().map(move |(column, ch)| Cell {
                    row,
                    column,
                    text: ch.to_string(),
                    width: 1,
                    fg: if line.starts_with('$') {
                        Rgb {
                            r: 89,
                            g: 211,
                            b: 178,
                        }
                    } else {
                        Rgb {
                            r: 210,
                            g: 216,
                            b: 227,
                        }
                    },
                    bg: Rgb {
                        r: 25,
                        g: 28,
                        b: 34,
                    },
                    flags: Flags::empty(),
                    selected: false,
                })
            })
            .collect();
        view.frame = Some(Frame {
            cells,
            cursor: (0, lines.len()),
            cursor_shape: CursorShape::Block,
            mode: Mode::SHOW_CURSOR,
            offset: 0,
            size: Size::bounded(90, 32),
            revision: 1,
        });
        view
    }
    pub fn pane_id(&self) -> canopy_desktop::state::workspace::PaneId {
        self.pane.id
    }
    pub fn pane(&self) -> &Pane {
        &self.pane
    }
    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }
    /// A terminal created by a remote device: its ID and first input owner.
    pub fn for_device(mut self, terminal_id: String, holder: Holder) -> Self {
        self.terminal_id = terminal_id;
        self.next_holder = holder;
        self
    }
    pub fn run(&self) -> Option<Arc<Run>> {
        self.run.clone()
    }
    pub fn is_starting(&self) -> bool {
        self.starting
    }
    /// Why the last start failed, if it did.
    pub fn start_error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn cwd(&self) -> Option<&std::path::Path> {
        self.pane.metadata.cwd.as_deref()
    }
    pub fn tool_id(&self) -> &str {
        &self.pane.tool
    }
    pub fn profile_id(&self) -> Option<&str> {
        self.pane.metadata.profile_id.as_deref()
    }
    pub fn mark_stopped(&mut self, cx: &mut Context<Self>) {
        self.starting = false;
        self.stopping = false;
        self.status = Some(Status::Exited {
            code: None,
            signal: None,
        });
        self.error = None;
        cx.emit(TerminalStateChanged);
        cx.notify();
    }
    pub fn mark_stop_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.starting = false;
        self.stopping = false;
        self.status = Some(Status::Failed(error.clone()));
        self.error = Some(error);
        cx.emit(TerminalStateChanged);
        cx.notify();
    }
    pub fn is_live(&self) -> bool {
        self.stopping || self.starting || self.is_running()
    }
    pub fn cleanup_error(&self) -> Option<String> {
        self.cleanup_result().and_then(Result::err)
    }
    pub fn cleanup_result(&self) -> Option<Result<(), String>> {
        self.cleanup
            .as_ref()
            .and_then(|cleanup| cleanup.result.lock().unwrap().clone())
            .or_else(|| {
                self.session
                    .as_ref()
                    .and_then(|session| session.cleanup_result())
            })
    }
    pub fn is_running(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|s| matches!(s, Status::Running))
    }
    pub fn new(
        pane: Pane,
        env: Result<Arc<ShellEnvironment>, String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (env, error) = match env {
            Ok(env) => (Some(env), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            pane,
            terminal_id: uuid::Uuid::new_v4().to_string(),
            agent_run: None,
            env,
            session: None,
            run: None,
            next_holder: Holder::Local,
            lease: None,
            lease_timer: None,
            cleanup: None,
            frame: None,
            cursor: Default::default(),
            cursor_task: None,
            status: None,
            error,
            input_error: None,
            starting: false,
            stopping: false,
            generation: 0,
            focus: cx.focus_handle(),
            visible: true,
            focused: false,
            needs_focus: false,
            bounds: Bounds::default(),
            columns: Size::bounded(80, 24),
            anchor: Default::default(),
            cell: Default::default(),
            settle_deadline: std::time::Instant::now(),
            settle_task: None,
            preedit: String::new(),
            wheel: 0.,
            link: None,
            pressed_link: None,
            task: None,
            input_control: None,
            pump: None,
        }
    }
}
