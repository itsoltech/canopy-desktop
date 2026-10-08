use super::{TerminalView, painter};
use crate::app_state::AppState;
use canopy_desktop::terminal::{
    file_drop::{FileDropError, prepare_file_drop},
    input,
    session::{Mode, Status},
};
use gpui_kit::*;
use std::{ops::Range, path::PathBuf};
impl TerminalView {
    pub(super) fn keyboard_mode(&self) -> input::KeyboardMode {
        self.frame
            .as_ref()
            .map(|frame| input::KeyboardMode {
                app_cursor: frame.mode.contains(Mode::APP_CURSOR),
            })
            .unwrap_or_default()
    }

    pub(super) fn encoded_key(
        &self,
        key: &str,
        control: bool,
        alt: bool,
        shift: bool,
    ) -> Option<Vec<u8>> {
        input::key(key, control, alt, shift, self.keyboard_mode())
    }

    pub(super) fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let mods = event.keystroke.modifiers;
        if mods.platform {
            return;
        }
        // Preserve Option/AltGr-generated text (including Polish letters) and IME dead keys.
        if input::preserve_native_text_input(
            &event.keystroke.key,
            mods.control,
            mods.alt,
            event.keystroke.key_char.as_deref(),
            event.prefer_character_input,
            cfg!(target_os = "windows"),
        ) {
            return;
        }
        if let Some(bytes) =
            self.encoded_key(&event.keystroke.key, mods.control, mods.alt, mods.shift)
        {
            self.send(bytes, cx);
            cx.stop_propagation();
        }
    }
    fn cell_at(&self, point: Point<Pixels>) -> (usize, usize) {
        self.grid_geometry().cell_at(
            f32::from(point.x - self.bounds.origin.x),
            f32::from(point.y - self.bounds.origin.y),
        )
    }
    pub(super) fn mouse(
        &mut self,
        point: Point<Pixels>,
        button: u8,
        start: bool,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let (row, col) = self.cell_at(point);
        let mode = self.frame.as_ref().map(|f| f.mode).unwrap_or_default();
        if mode.intersects(Mode::MOUSE_MODE) && !shift {
            if button == 32 && !mode.intersects(Mode::MOUSE_DRAG | Mode::MOUSE_MOTION) {
                return;
            }
            self.report_mouse(col, row, button, cx);
        } else if button != 3 {
            if let Some(session) = &self.session {
                session.select(row.min(self.columns.rows - 1), col, start);
            }
            self.refresh(cx);
        }
    }
    fn report_mouse(&mut self, col: usize, row: usize, button: u8, cx: &mut Context<Self>) {
        let mode = self.frame.as_ref().map(|f| f.mode).unwrap_or_default();
        if mode.contains(Mode::SGR_MOUSE) {
            let end = if button == 3 { 'm' } else { 'M' };
            let button = if button == 3 { 0 } else { button };
            self.send(
                format!("\x1b[<{button};{};{}{end}", col + 1, row + 1).into_bytes(),
                cx,
            );
        } else if col < 223 && row < 223 {
            self.send(
                vec![27, b'[', b'M', 32 + button, 33 + col as u8, 33 + row as u8],
                cx,
            );
        }
    }
    pub(super) fn scroll(
        &mut self,
        lines: i32,
        point: Point<Pixels>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let mode = self.frame.as_ref().map(|f| f.mode).unwrap_or_default();
        if mode.intersects(Mode::MOUSE_MODE) && !shift {
            let (row, col) = self.cell_at(point);
            for _ in 0..lines.unsigned_abs().min(30) {
                self.report_mouse(col, row, if lines > 0 { 64 } else { 65 }, cx);
            }
        } else if mode.contains(Mode::ALT_SCREEN) && !shift {
            for _ in 0..lines.unsigned_abs().min(30) {
                self.send(
                    if lines > 0 {
                        b"\x1b[A".to_vec()
                    } else {
                        b"\x1b[B".to_vec()
                    },
                    cx,
                );
            }
        } else {
            if let Some(session) = &self.session {
                session.scroll(lines);
            }
            self.refresh(cx);
        }
    }
}
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let utf16: Vec<_> = self.preedit.encode_utf16().collect();
        let end = range.end.min(utf16.len());
        let start = range.start.min(end);
        *adjusted = Some(start..end);
        Some(String::from_utf16_lossy(&utf16[start..end]))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let len = self.preedit.encode_utf16().count();
        Some(UTF16Selection {
            range: len..len,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.preedit.is_empty()).then(|| 0..self.preedit.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.preedit.clear();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preedit.clear();
        self.send(text.as_bytes().to_vec(), cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preedit = text.to_owned();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let cursor = self
            .cursor
            .published()
            .map(|cursor| cursor.position)
            .or_else(|| self.frame.as_ref().map(|frame| frame.cursor))
            .unwrap_or_default();
        let geometry = self.grid_geometry();
        Some(Bounds::new(
            bounds.origin
                + point(
                    px(geometry.left + cursor.1 as f32 * painter::CELL_WIDTH),
                    px(geometry.top + cursor.0 as f32 * painter::CELL_HEIGHT),
                ),
            size(px(painter::CELL_WIDTH), px(painter::CELL_HEIGHT)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.status
            .as_ref()
            .is_some_and(|s| matches!(s, Status::Running))
    }
}

impl TerminalView {
    pub(super) fn send(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) -> bool {
        if let Some(session) = &self.session {
            let accepted = session.input(bytes);
            if accepted {
                self.note_cursor_input();
            }
            self.refresh(cx);
            accepted
        } else {
            false
        }
    }
    pub(super) fn copy(&self, cx: &mut Context<Self>) {
        if let Some(text) = self
            .session
            .as_ref()
            .and_then(|session| session.copy_selection())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
    pub(super) fn send_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let bracketed = self
            .frame
            .as_ref()
            .is_some_and(|f| f.mode.contains(Mode::BRACKETED_PASTE));
        let Some(bytes) = input::paste_bytes(text, bracketed) else {
            self.input_error = Some("Input is too large to send to this terminal.".into());
            cx.notify();
            return false;
        };
        self.send(bytes, cx)
    }
    pub(super) fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if self.send_text(&text, cx) {
            self.input_error = None;
        }
    }
    pub(super) fn can_accept_file_drop(&self) -> bool {
        !self.starting
            && !self.stopping
            && self
                .session
                .as_ref()
                .is_some_and(|session| matches!(session.status(), Status::Running))
    }
    pub(super) fn drop_files(
        &mut self,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_accept_file_drop() {
            return;
        }
        let shell = self
            .env
            .as_ref()
            .map(|environment| environment.shell_kind())
            .unwrap_or(canopy_desktop::terminal::environment::ShellKind::Other);
        let payload = match prepare_file_drop(paths, shell, cfg!(windows)) {
            Ok(payload) => payload,
            Err(FileDropError::Empty) => return,
            Err(error) => {
                self.input_error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.activate(window, cx);
        if self.can_accept_file_drop() && self.send_text(&payload, cx) {
            self.input_error = None;
        }
    }
    pub(super) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        let app = cx.global::<AppState>().clone();
        let pane = self.pane.id;
        app.workspace.update(cx, |state, cx| {
            let tab = state
                .tabs()
                .iter()
                .find(|t| t.root.find(pane).is_some())
                .map(|t| t.id);
            if let Some(tab) = tab {
                let _ = state.focus(tab, pane);
                cx.notify();
            }
        });
    }
}
