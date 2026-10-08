//! Presentation only: project identity, handlers and trailing actions come from the caller.
use super::super::theme as t;
use super::column;
use gpui_kit::*;

pub fn empty_state(
    action: impl IntoElement,
    status: Option<SharedString>,
    error: Option<SharedString>,
) -> Div {
    column()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(12.))
        .child(action)
        .children(status.map(|message| div().text_color(t::muted()).child(message)))
        .children(error.map(|message| div().max_w(px(420.)).text_color(t::red()).child(message)))
}
