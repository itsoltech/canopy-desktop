use super::super::theme as t;
use super::{column, row};
use gpui_kit::*;

pub fn caption(label: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(10.))
        .line_height(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t::faint())
        .child(label.into())
}
pub fn section(
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    rows: Vec<AnyElement>,
) -> Div {
    column()
        .flex_shrink_0()
        .gap(px(12.))
        .child(
            column().gap(px(4.)).child(caption(title)).child(
                div()
                    .text_size(px(11.))
                    .line_height(px(14.85))
                    .text_color(t::muted())
                    .child(description.into()),
            ),
        )
        .child(column().children(rows))
}
pub fn setting(
    label: impl Into<SharedString>,
    help: impl Into<SharedString>,
    first: bool,
    control: impl IntoElement,
) -> Div {
    row()
        .items_start()
        .flex_shrink_0()
        .gap(px(24.))
        .pb(px(12.))
        .pt(px(if first { 0. } else { 12. }))
        .border_t(px(if first { 0. } else { 1. }))
        .border_color(t::border())
        .child(
            column()
                .flex_1()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(17.55))
                        .child(label.into()),
                )
                .child(
                    div()
                        .max_w(px(390.))
                        .text_size(px(11.))
                        .line_height(px(14.85))
                        .text_color(t::muted())
                        .child(help.into()),
                ),
        )
        .child(div().flex_shrink_0().pt(px(2.)).child(control))
}

/// Label, caller-provided control and optional help. External spacing remains caller-owned.
pub fn form_field(
    label: impl Into<SharedString>,
    help: impl Into<SharedString>,
    control: impl IntoElement,
) -> Div {
    let help: SharedString = help.into();
    column()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(13.))
                .line_height(px(18.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t::secondary())
                .child(label.into()),
        )
        .child(control)
        .children((!help.is_empty()).then(|| {
            div()
                .text_size(px(11.))
                .line_height(px(14.85))
                .text_color(t::muted())
                .child(help)
        }))
}

/// Two equal columns with top-aligned labels. A leftover field stays half-width
/// so column edges remain aligned with the row above.
pub fn field_grid(fields: impl IntoIterator<Item = impl IntoElement>) -> Div {
    let mut fields = fields.into_iter().map(|field| field.into_any_element());
    let mut rows = Vec::new();
    while let Some(left) = fields.next() {
        let right = fields.next();
        rows.push(
            div()
                .flex()
                .flex_shrink_0()
                .items_start()
                .gap(px(12.))
                .min_w_0()
                .child(div().flex_1().min_w_0().child(left))
                .child(div().flex_1().min_w_0().children(right)),
        );
    }
    column().flex_shrink_0().gap(px(12.)).children(rows)
}

/// Electron PrefsRow's stacked variant: help precedes the control.
pub fn stacked_setting(
    label: impl Into<SharedString>,
    help: impl Into<SharedString>,
    first: bool,
    control: impl IntoElement,
) -> Div {
    column()
        .flex_shrink_0()
        .gap(px(8.))
        .pb(px(12.))
        .pt(px(if first { 0. } else { 12. }))
        .border_t(px(if first { 0. } else { 1. }))
        .border_color(t::border())
        .child(
            column()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(17.55))
                        .child(label.into()),
                )
                .child(
                    div()
                        .max_w(px(390.))
                        .text_size(px(11.))
                        .line_height(px(14.85))
                        .text_color(t::muted())
                        .child(help.into()),
                ),
        )
        .child(control)
}
pub fn preference_section(title: impl Into<SharedString>, content: impl IntoElement) -> Div {
    column()
        .flex_shrink_0()
        .gap(px(12.))
        .child(caption(title))
        .child(content)
}
