//! Static placeholders; sizing and surrounding spacing belong to callers.
use super::super::theme as t;
use gpui_kit::*;
pub fn skeleton_bar() -> Div {
    div().rounded(px(3.)).bg(t::hover())
}

/// A panel waiting for its content: a small spinner over one muted line.
pub fn panel_loading(label: impl Into<SharedString>) -> Div {
    use gpui_kit::component::{Sizable, spinner::Spinner};
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .child(Spinner::new().small().color(t::muted()))
        .child(div().text_xs().text_color(t::muted()).child(label.into()))
}
