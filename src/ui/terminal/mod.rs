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
pub(crate) fn init(cx: &mut App) {
    #[cfg(not(target_os = "windows"))]
    cx.bind_keys([
        KeyBinding::new("cmd-c", CopyTerminal, Some("Terminal")),
        KeyBinding::new("cmd-v", PasteTerminal, Some("Terminal")),
        KeyBinding::new("tab", TerminalTab, Some("Terminal")),
        KeyBinding::new("shift-tab", TerminalBacktab, Some("Terminal")),
        KeyBinding::new("shift-enter", TerminalShiftEnter, Some("Terminal")),
    ]);
    #[cfg(target_os = "windows")]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-c", CopyTerminal, Some("Terminal")),
        KeyBinding::new("ctrl-shift-v", PasteTerminal, Some("Terminal")),
        KeyBinding::new("tab", TerminalTab, Some("Terminal")),
        KeyBinding::new("shift-tab", TerminalBacktab, Some("Terminal")),
        KeyBinding::new("shift-enter", TerminalShiftEnter, Some("Terminal")),
    ]);
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
    agent_run: Option<String>,
    env: Option<Arc<ShellEnvironment>>,
    session: Option<Arc<Session>>,
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
    settle_deadline: std::time::Instant,
    settle_task: Option<Task<()>>,
    preedit: String,
    wheel: f32,
    task: Option<Task<()>>,
    pump: Option<Task<()>>,
}
impl TerminalView {
    pub fn pane_id(&self) -> canopy_desktop::state::workspace::PaneId {
        self.pane.id
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
            agent_run: None,
            env,
            session: None,
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
            settle_deadline: std::time::Instant::now(),
            settle_task: None,
            preedit: String::new(),
            wheel: 0.,
            task: None,
            pump: None,
        }
    }
}
