//! Terminal presentation only. Process state and callbacks belong to the caller.
use super::super::theme as t;
use super::{column, row};
use gpui_kit::*;

pub fn terminal_surface(id: impl Into<ElementId>) -> Stateful<Div> {
    column().id(id).size_full().bg(t::bg())
}
pub fn terminal_viewport(surface: impl IntoElement) -> Div {
    div().flex_1().min_h_0().relative().child(surface)
}
pub fn terminal_message(message: impl Into<SharedString>) -> Div {
    div().text_color(t::muted()).child(message.into())
}
pub fn process_status_bar(
    message: impl Into<SharedString>,
    actions: impl IntoIterator<Item = AnyElement>,
) -> Div {
    row()
        .flex_shrink_0()
        .px(px(12.))
        .py(px(8.))
        .gap(px(8.))
        .bg(t::sidebar())
        .child(
            div()
                .flex_1()
                .text_size(px(12.))
                .text_color(t::secondary())
                .child(message.into()),
        )
        .children(actions)
}
