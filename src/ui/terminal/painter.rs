use super::TerminalView;
use crate::ui::theme;
use alacritty_terminal::{
    term::cell::Flags,
    vte::ansi::{CursorShape, Rgb},
};
pub use canopy_desktop::terminal::geometry::{CELL_HEIGHT, CELL_WIDTH};
use gpui_kit::*;
fn color(c: Rgb) -> Hsla {
    rgb((u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)).into()
}
pub fn paint(
    view: &mut TerminalView,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut Context<TerminalView>,
) {
    window.handle_input(
        &view.focus,
        ElementInputHandler::new(bounds, cx.entity()),
        cx,
    );
    let Some(frame) = &view.frame else {
        return;
    };
    let base_font = theme::terminal_font();
    let geometry = view.grid_geometry();
    let origin = bounds.origin + point(px(geometry.left), px(geometry.top));
    window.paint_layer(bounds, |window| {
        // Background spans include spacer cells and fill all four margins/corners.
        let mut background_index = 0;
        while background_index < frame.cells.len() {
            let first = &frame.cells[background_index];
            let mut end = background_index + 1;
            while let Some(cell) = frame.cells.get(end) {
                if cell.row != first.row
                    || cell.bg != first.bg
                    || cell.column != frame.cells[end - 1].column + 1
                {
                    break;
                }
                end += 1;
            }
            let columns = frame.cells[end - 1].column - first.column + 1;
            let rect = geometry.background(first.row, first.column, columns);
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + point(px(rect.x), px(rect.y)),
                    size(px(rect.width), px(rect.height)),
                ),
                color(first.bg),
            ));
            background_index = end;
        }
        let mut i = 0;
        while i < frame.cells.len() {
            let cell = &frame.cells[i];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                i += 1;
                continue;
            }
            let mut text = cell.text.clone();
            let mut byte_columns = vec![0; cell.text.len()];
            let mut columns = cell.width;
            let mut next = i + 1;
            // Merge ordinary adjacent cells into shaped spans; wide graphemes get their own span.
            if cell.width == 1 {
                while let Some(other) = frame.cells.get(next) {
                    if other.row != cell.row
                        || other.column != cell.column + columns
                        || other.width != 1
                        || other.fg != cell.fg
                        || other.bg != cell.bg
                        || other.flags != cell.flags
                        || other.selected != cell.selected
                    {
                        break;
                    }
                    byte_columns.extend(std::iter::repeat_n(columns, other.text.len()));
                    text.push_str(&other.text);
                    columns += 1;
                    next += 1;
                }
            }
            let pos = origin
                + point(
                    px(cell.column as f32 * CELL_WIDTH),
                    px(cell.row as f32 * CELL_HEIGHT),
                );
            if cell.selected {
                window.paint_quad(fill(
                    Bounds::new(pos, size(px(columns as f32 * CELL_WIDTH), px(CELL_HEIGHT))),
                    theme::selected(),
                ));
            }
            if !cell.flags.contains(Flags::HIDDEN) {
                let mut font = base_font.clone();
                if cell.flags.contains(Flags::BOLD) {
                    font.weight = FontWeight::BOLD;
                }
                if cell.flags.contains(Flags::ITALIC) {
                    font.style = FontStyle::Italic;
                }
                let run = TextRun {
                    len: text.len(),
                    font,
                    color: color(cell.fg),
                    underline: cell.flags.intersects(Flags::ALL_UNDERLINES).then_some(
                        UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: cell.flags.contains(Flags::UNDERCURL),
                        },
                    ),
                    strikethrough: cell.flags.contains(Flags::STRIKEOUT).then_some(
                        StrikethroughStyle {
                            thickness: px(1.),
                            color: None,
                        },
                    ),
                    ..Default::default()
                };
                let mut line = window
                    .text_system()
                    .shape_line(text.into(), px(13.), &[run], None);
                align_to_grid(&mut line, &byte_columns, columns);
                let _ = line.paint(pos, px(CELL_HEIGHT), TextAlign::Left, None, window, cx);
            }
            i = next;
        }
        let painted_cursor = view.cursor.published();
        if painted_cursor.is_some_and(|cursor| {
            cursor.visible
                && cursor.position.0 < frame.size.rows
                && cursor.position.1 < frame.size.columns
        }) {
            let painted_cursor = painted_cursor.unwrap();
            let pos = origin
                + point(
                    px(painted_cursor.position.1 as f32 * CELL_WIDTH),
                    px(painted_cursor.position.0 as f32 * CELL_HEIGHT),
                );
            let (offset, sz) = match painted_cursor.shape {
                CursorShape::Beam => (point(px(0.), px(0.)), size(px(2.), px(CELL_HEIGHT))),
                CursorShape::Underline => (
                    point(px(0.), px(CELL_HEIGHT - 2.)),
                    size(px(CELL_WIDTH), px(2.)),
                ),
                CursorShape::Hidden => return,
                _ => (point(px(0.), px(0.)), size(px(CELL_WIDTH), px(CELL_HEIGHT))),
            };
            let mut cursor = theme::text();
            cursor.a = if view.focus.is_focused(window) {
                0.45
            } else {
                0.18
            };
            window.paint_quad(fill(Bounds::new(pos + offset, sz), cursor));
        }
        if !view.preedit.is_empty() {
            let text: SharedString = view.preedit.clone().into();
            let run = TextRun {
                len: text.len(),
                font: base_font.clone(),
                color: theme::yellow(),
                ..Default::default()
            };
            let line = window.text_system().shape_line(text, px(13.), &[run], None);
            let cursor = painted_cursor
                .map(|cursor| cursor.position)
                .unwrap_or(frame.cursor);
            let pos = origin
                + point(
                    px(cursor.1 as f32 * CELL_WIDTH),
                    px(cursor.0 as f32 * CELL_HEIGHT),
                );
            let _ = line.paint(pos, px(CELL_HEIGHT), TextAlign::Left, None, window, cx);
        }
    });
}

// Do not mutate GPUI's shared line cache: retain native shaping within each cell
// in an owned layout, while removing proportional advances between cells.
fn align_to_grid(line: &mut ShapedLine, byte_columns: &[usize], columns: usize) {
    let mut runs = line.runs.clone();
    let glyphs: Vec<_> = runs
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .map(|g| (g.index, f32::from(g.position.x)))
        .collect();
    let positions =
        canopy_desktop::terminal::glyph_layout::positions(&glyphs, byte_columns, CELL_WIDTH);
    for (glyph, x) in runs
        .iter_mut()
        .flat_map(|r| r.glyphs.iter_mut())
        .zip(positions)
    {
        glyph.position.x = px(x);
    }
    **line = std::sync::Arc::new(LineLayout {
        font_size: line.font_size,
        width: px(columns as f32 * CELL_WIDTH),
        ascent: line.ascent,
        descent: line.descent,
        runs,
        len: line.len(),
    });
}
