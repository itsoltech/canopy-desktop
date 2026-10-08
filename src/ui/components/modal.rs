//! Shared modal presentation. State and lifecycle live in text_prompt.
mod text_prompt;
use super::super::theme as t;
use super::{column, row};
use gpui_kit::*;
pub use text_prompt::{ModalDismissed, TextPrompt};

pub fn modal_surface(id: impl Into<ElementId>) -> Stateful<Div> {
    column()
        .id(id)
        .occlude()
        .p(px(t::SPACING_UNIT * 5.))
        .gap(px(t::SPACING_UNIT * 4.))
        .bg(t::elevated())
        .border_1()
        .border_color(t::control_border())
        .rounded(px(t::SPACING_UNIT * 2.))
        .text_size(px(13.))
        .text_color(t::text())
}
pub fn modal_header(title: impl Into<SharedString>, close: impl IntoElement) -> Div {
    row()
        .gap(px(12.))
        .child(
            div()
                .flex_1()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(close)
}
pub fn modal_field(
    label: impl Into<SharedString>,
    control: impl IntoElement,
    error: Option<SharedString>,
) -> Div {
    column()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(t::secondary())
                .child(label.into()),
        )
        .child(control)
        .children(error.map(|e| div().text_size(px(12.)).text_color(t::red()).child(e)))
}
pub fn modal_actions(cancel: impl IntoElement, confirm: impl IntoElement) -> Div {
    row().justify_end().gap(px(8.)).child(cancel).child(confirm)
}
pub fn modal_backdrop(progress: f32) -> Div {
    div()
        .absolute()
        .inset_0()
        .occlude()
        .bg(t::modal_overlay())
        .opacity(progress)
}

mod confirmation;
pub use confirmation::Confirmation;
