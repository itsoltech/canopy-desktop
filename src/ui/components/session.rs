//! Stateless session inspector sections. Callers own spacing, data and subscriptions.
use super::{caption, column, dot, row};
use crate::ui::theme as t;
use gpui_kit::*;

pub fn session_status(label: impl Into<SharedString>, color: Hsla) -> Div {
    row()
        .flex_shrink_0()
        .h(px(28.))
        .px(px(10.))
        .gap(px(8.))
        .rounded(px(4.))
        .bg(t::hover())
        .child(dot().bg(color))
        .child(label.into())
}

pub fn session_section(title: impl Into<SharedString>) -> Div {
    column().w_full().flex_shrink_0().child(caption(title))
}

pub fn session_info(label: &'static str, value: impl Into<SharedString>) -> Div {
    row()
        .w_full()
        .flex_shrink_0()
        .items_start()
        .min_h(px(20.))
        .gap(px(8.))
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

/// Integration health is separate from the agent's working/attention state.
pub fn integration_health(
    label: impl Into<SharedString>,
    color: Hsla,
    detail: impl Into<SharedString>,
) -> Div {
    session_section("INTEGRATION")
        .gap(px(8.))
        .child(
            row()
                .flex_shrink_0()
                .gap(px(6.))
                .child(dot().bg(color))
                .child(label.into()),
        )
        .child(
            div()
                .w_full()
                .flex_shrink_0()
                .text_size(px(11.))
                .line_height(px(17.))
                .text_color(t::secondary())
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
