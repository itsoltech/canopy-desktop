//! File UI presentation. Callers retain state, callbacks and outer spacing.
use super::{super::theme as t, row};
use gpui_kit::*;

pub const SEARCH_ROW_HEIGHT: f32 = 28.;
pub const SEARCH_VIEWPORT_HEIGHT: f32 = 336.;

pub fn file_message(message: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_color(color).child(message.into())
}

pub fn file_search_row(
    id: impl Into<ElementId>,
    path: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    row()
        .id(id)
        .h(px(SEARCH_ROW_HEIGHT))
        .flex_shrink_0()
        .px(px(8.))
        .rounded(px(4.))
        .bg(if selected {
            t::hover()
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .child(div().truncate().child(path.into()))
}

pub const TREE_INDENT: f32 = 12.;
pub const TREE_INSET: f32 = 8.;

/// Decorative ancestry guides; no hit targets and no independent scrolling.
pub fn tree_indent_guides(depth: usize) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                for level in 0..depth {
                    let x = bounds.left() + px(TREE_INSET + 6. + level as f32 * TREE_INDENT);
                    window.paint_quad(fill(
                        Bounds::new(point(x, bounds.top()), size(px(1.), bounds.size.height)),
                        t::control_border(),
                    ));
                }
            },
        )
        .size_full(),
    )
}
