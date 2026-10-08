//! Suppress transient cursor positions from unsynchronized agent TUI redraw bursts.
use super::*;
use alacritty_terminal::vte::ansi::CursorShape;
use std::time::{Duration, Instant};

const QUIET_WINDOW: Duration = Duration::from_millis(80);
const INTERACTIVE_QUIET_WINDOW: Duration = Duration::from_millis(20);
const INTERACTION_WINDOW: Duration = Duration::from_millis(250);
const REDRAW_DAMAGE_THRESHOLD: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PaintedCursor {
    pub position: (usize, usize),
    pub shape: CursorShape,
    pub visible: bool,
}

impl PaintedCursor {
    fn from_frame(frame: &Frame) -> Self {
        Self {
            position: frame.cursor,
            shape: frame.cursor_shape,
            visible: frame.mode.contains(Mode::SHOW_CURSOR)
                && frame.cursor_shape != CursorShape::Hidden,
        }
    }
}

#[derive(Default)]
pub(super) struct CursorStabilizer {
    published: Option<PaintedCursor>,
    pending: Option<PaintedCursor>,
    deadline: Option<Instant>,
    revision: Option<u64>,
    interactive_until: Option<Instant>,
}

impl CursorStabilizer {
    pub fn published(&self) -> Option<PaintedCursor> {
        self.published
    }

    fn observe(
        &mut self,
        frame: &Frame,
        redraw: bool,
        now: Instant,
        stabilize: bool,
    ) -> Option<Instant> {
        let current = PaintedCursor::from_frame(frame);
        if !stabilize {
            self.revision = Some(frame.revision);
            self.pending = None;
            self.deadline = None;
            self.published = Some(current);
            return None;
        }
        if self.revision == Some(frame.revision) {
            return self.deadline;
        }
        self.revision = Some(frame.revision);
        let bursting = self.deadline.is_some();
        if !redraw && !bursting {
            self.published = Some(current);
            self.pending = None;
            return None;
        }
        if !current.visible || self.published.is_none() {
            self.published = Some(current);
        }
        self.pending = Some(current);
        let quiet_window = if self
            .interactive_until
            .is_some_and(|deadline| now <= deadline)
        {
            INTERACTIVE_QUIET_WINDOW
        } else {
            QUIET_WINDOW
        };
        let deadline = now + quiet_window;
        self.deadline = Some(deadline);
        Some(deadline)
    }

    fn user_input(&mut self, now: Instant) {
        self.pending = None;
        self.deadline = None;
        self.interactive_until = Some(now + INTERACTION_WINDOW);
    }

    fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn settle(&mut self, now: Instant) -> bool {
        if self.deadline.is_none_or(|deadline| now < deadline) {
            return false;
        }
        self.deadline = None;
        let Some(pending) = self.pending.take() else {
            return false;
        };
        let changed = self.published != Some(pending);
        self.published = Some(pending);
        changed
    }
}

fn redraw_damage(previous: Option<&Frame>, current: &Frame) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    if previous.size != current.size || previous.offset != current.offset {
        return true;
    }
    let length_damage = previous.cells.len().abs_diff(current.cells.len());
    let changed = previous
        .cells
        .iter()
        .zip(&current.cells)
        .filter(|(before, after)| before != after)
        .take(REDRAW_DAMAGE_THRESHOLD)
        .count();
    length_damage.saturating_add(changed) >= REDRAW_DAMAGE_THRESHOLD
}

impl TerminalView {
    pub(super) fn note_cursor_input(&mut self) {
        self.cursor.user_input(Instant::now());
        // A pending noninteractive timer might sleep past the new, shorter deadline.
        self.cursor_task = None;
    }

    pub(super) fn set_frame(&mut self, frame: Frame, cx: &mut Context<Self>) {
        self.set_frame_with_cursor_policy(frame, true, cx);
    }

    pub(super) fn set_resized_frame(&mut self, frame: Frame, cx: &mut Context<Self>) {
        self.set_frame_with_cursor_policy(frame, false, cx);
    }

    fn set_frame_with_cursor_policy(
        &mut self,
        frame: Frame,
        stabilize: bool,
        cx: &mut Context<Self>,
    ) {
        let redraw = redraw_damage(self.frame.as_ref(), &frame);
        let stabilize = stabilize
            && (matches!(self.pane.tool.as_str(), "claude" | "codex")
                || frame.mode.contains(Mode::ALT_SCREEN));
        let deadline = self
            .cursor
            .observe(&frame, redraw, Instant::now(), stabilize);
        self.frame = Some(frame);
        if deadline.is_some() && self.cursor_task.is_none() {
            self.cursor_task = Some(cx.spawn(async |this, cx| {
                loop {
                    let Some(deadline) = this
                        .update(cx, |this, _| this.cursor.deadline())
                        .ok()
                        .flatten()
                    else {
                        let _ = this.update(cx, |this, _| this.cursor_task = None);
                        break;
                    };
                    cx.background_executor()
                        .timer(deadline.saturating_duration_since(Instant::now()))
                        .await;
                    let done = this
                        .update(cx, |this, cx| {
                            if this
                                .cursor
                                .deadline()
                                .is_some_and(|next| Instant::now() < next)
                            {
                                return false;
                            }
                            let changed = this.cursor.settle(Instant::now());
                            this.cursor_task = None;
                            if changed && this.visible {
                                cx.notify();
                            }
                            true
                        })
                        .unwrap_or(true);
                    if done {
                        break;
                    }
                }
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{
        event::{Event, EventListener},
        term::{Config, Term},
        vte::ansi,
    };
    use canopy_desktop::terminal::session::Cell;
    use core::prelude::v1::test;

    fn frame(revision: u64, cursor: (usize, usize), mode: Mode) -> Frame {
        Frame {
            cells: vec![],
            cursor,
            cursor_shape: CursorShape::Block,
            mode,
            offset: 0,
            size: Size::bounded(80, 24),
            revision,
        }
    }

    #[derive(Clone, Copy)]
    struct Mock;
    impl EventListener for Mock {
        fn send_event(&self, _: Event) {}
    }

    fn parsed_frame(term: &Term<Mock>, revision: u64) -> Frame {
        let content = term.renderable_content();
        let offset = content.display_offset;
        let cursor = (
            (content.cursor.point.line.0 + offset as i32).max(0) as usize,
            content.cursor.point.column.0,
        );
        let mode = content.mode;
        let cells = content
            .display_iter
            .map(|cell| Cell {
                row: (cell.point.line.0 + offset as i32).max(0) as usize,
                column: cell.point.column.0,
                text: cell.c.to_string(),
                width: usize::from(
                    cell.flags
                        .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR),
                ) + 1,
                fg: alacritty_terminal::vte::ansi::Rgb { r: 0, g: 0, b: 0 },
                bg: alacritty_terminal::vte::ansi::Rgb { r: 0, g: 0, b: 0 },
                flags: cell.flags,
                selected: false,
            })
            .collect();
        Frame {
            cells,
            cursor,
            cursor_shape: CursorShape::Block,
            mode,
            offset,
            size: Size::bounded(80, 24),
            revision,
        }
    }

    #[test]
    fn fragmented_primary_screen_redraw_publishes_only_the_settled_cursor() {
        let start = Instant::now();
        let size = Size::bounded(80, 24);
        let mut term = Term::new(Config::default(), &size, Mock);
        let mut parser: ansi::Processor = ansi::Processor::new();
        let mut cursor = CursorStabilizer::default();

        parser.advance(&mut term, b"\x1b[?25h\x1b[3;5Hinitial frame text\x1b[3;5H");
        let initial = parsed_frame(&term, 1);
        cursor.observe(&initial, false, start, true);
        assert_eq!(cursor.published().unwrap().position, (2, 4));

        // The next cursor address is intentionally split across PTY reads.
        parser.advance(&mut term, b"\x1b[2J\x1b[13;");
        let clearing = parsed_frame(&term, 2);
        assert!(redraw_damage(Some(&initial), &clearing));
        cursor.observe(&clearing, true, start + Duration::from_millis(8), true);
        assert!(cursor.published().unwrap().visible);
        assert_eq!(cursor.published().unwrap().position, (2, 4));
        parser.advance(&mut term, b"51H");
        cursor.observe(
            &parsed_frame(&term, 3),
            false,
            start + Duration::from_millis(16),
            true,
        );
        parser.advance(&mut term, b"\x1b[7;11H");
        cursor.observe(
            &parsed_frame(&term, 4),
            false,
            start + Duration::from_millis(24),
            true,
        );
        assert!(!cursor.settle(start + Duration::from_millis(103)));
        assert!(cursor.settle(start + Duration::from_millis(104)));
        assert_eq!(cursor.published().unwrap().position, (6, 10));
        assert!(cursor.published().unwrap().visible);
    }

    #[test]
    fn explicit_cursor_hide_is_immediate_during_a_redraw() {
        let start = Instant::now();
        let mut cursor = CursorStabilizer::default();
        cursor.observe(&frame(1, (1, 1), Mode::SHOW_CURSOR), false, start, true);
        assert_eq!(cursor.published().unwrap().position, (1, 1));
        assert!(cursor.published().unwrap().visible);
        cursor.observe(&frame(2, (9, 9), Mode::empty()), true, start, true);
        assert_eq!(cursor.published().unwrap().position, (9, 9));
        assert!(!cursor.published().unwrap().visible);
    }

    #[test]
    fn a_small_primary_screen_edit_updates_the_cursor_without_delay() {
        let start = Instant::now();
        let mut before = frame(1, (1, 1), Mode::SHOW_CURSOR);
        before.cells = vec![test_cell("a")];
        let mut after = frame(2, (1, 2), Mode::SHOW_CURSOR);
        after.cells = vec![test_cell("b")];
        assert!(!redraw_damage(Some(&before), &after));
        let mut cursor = CursorStabilizer::default();
        cursor.observe(&before, false, start, true);
        cursor.observe(&after, false, start, true);
        assert_eq!(cursor.published().unwrap().position, (1, 2));
        assert!(cursor.published().unwrap().visible);
        assert!(cursor.deadline().is_none());
    }

    #[test]
    fn keyboard_input_cancels_a_slow_burst_and_uses_the_interactive_deadline() {
        let start = Instant::now();
        let mut cursor = CursorStabilizer::default();
        cursor.observe(&frame(1, (1, 1), Mode::SHOW_CURSOR), false, start, true);
        cursor.observe(&frame(2, (8, 8), Mode::SHOW_CURSOR), true, start, true);
        assert_eq!(cursor.deadline(), Some(start + QUIET_WINDOW));

        let input = start + Duration::from_millis(10);
        cursor.user_input(input);
        assert!(cursor.deadline().is_none());
        cursor.observe(
            &frame(3, (1, 2), Mode::SHOW_CURSOR),
            true,
            input + Duration::from_millis(8),
            true,
        );
        assert_eq!(
            cursor.deadline(),
            Some(input + Duration::from_millis(8) + INTERACTIVE_QUIET_WINDOW)
        );
        assert!(!cursor.settle(input + Duration::from_millis(27)));
        assert!(cursor.settle(input + Duration::from_millis(28)));
        assert_eq!(cursor.published().unwrap().position, (1, 2));
    }

    fn test_cell(text: &str) -> Cell {
        Cell {
            row: 0,
            column: 0,
            text: text.into(),
            width: 1,
            fg: alacritty_terminal::vte::ansi::Rgb { r: 0, g: 0, b: 0 },
            bg: alacritty_terminal::vte::ansi::Rgb { r: 0, g: 0, b: 0 },
            flags: alacritty_terminal::term::cell::Flags::empty(),
            selected: false,
        }
    }
}
