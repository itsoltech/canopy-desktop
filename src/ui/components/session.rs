//! Stateless session inspector sections. Callers own spacing, data and subscriptions.
use super::{column, dot, row};
use crate::ui::theme as t;
use gpui_kit::*;

pub fn session_section(title: &str) -> Div {
    column()
        .w_full()
        .flex_shrink_0()
        .child(super::panel_caption(title))
}

pub fn session_info(label: &'static str, value: impl Into<SharedString>) -> Div {
    row()
        .w_full()
        .flex_shrink_0()
        .items_start()
        .min_h(px(20.))
        .gap(px(8.))
        .text_size(px(11.5))
        .child(
            div()
                .w(px(64.))
                .flex_shrink_0()
                .whitespace_nowrap()
                .text_color(t::faint())
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(t::secondary())
                .child(value.into()),
        )
}

/// Integration health is separate from the agent's working/attention state: a
/// "Hooks" row beside the metadata, with what the state means under it.
pub fn integration_health(
    label: impl Into<SharedString>,
    color: Hsla,
    detail: impl Into<SharedString>,
) -> Div {
    column()
        .w_full()
        .flex_shrink_0()
        .gap(px(3.))
        .child(
            row()
                .w_full()
                .min_h(px(20.))
                .gap(px(8.))
                .child(
                    div()
                        .w(px(64.))
                        .flex_shrink_0()
                        .text_color(t::faint())
                        .child("Hooks"),
                )
                .child(dot().size(px(6.)).bg(color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(t::secondary())
                        .child(label.into()),
                ),
        )
        .child(
            div()
                .w_full()
                .pl(px(72.))
                .text_size(px(11.))
                .line_height(px(16.))
                .text_color(t::faint())
                .child(detail.into()),
        )
}

/// Single-line metadata with the complete value/raw identifier available on hover.
pub fn session_info_detail(
    label: &'static str,
    value: impl Into<SharedString>,
    detail: impl Into<SharedString>,
) -> Stateful<Div> {
    let detail = detail.into();
    session_info(label, value)
        .id(SharedString::from(format!("session-info-{label}")))
        .tooltip(move |w, cx| {
            gpui_kit::component::tooltip::Tooltip::new(detail.clone()).build(w, cx)
        })
}
