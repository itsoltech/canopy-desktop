//! Stateless notch presentation. Pointer handling, motion and window ownership
//! belong to the controller; these builders preserve ordinary Styled overrides.
use super::{column, custom_icon, icon, row};
use gpui_kit::component::IconName;
use gpui_kit::*;

pub const NOTCH_ROW_HEIGHT: f32 = 48.;

pub struct NotchSession {
    pub unseen: bool,
    pub state: canopy_desktop::agents::Status,
    pub pane: canopy_desktop::state::workspace::PaneId,
    pub workspace: SharedString,
    pub context: SharedString,
    pub status: SharedString,
    pub status_color: Hsla,
}

pub fn notch_surface(
    id: impl Into<ElementId>,
    size: Size<Pixels>,
    radius: Pixels,
) -> Stateful<Div> {
    column()
        .id(id)
        .w(size.width)
        .h(size.height)
        .flex_shrink_0()
        .bg(rgb(0))
        .rounded_b(radius)
        .overflow_hidden()
}

pub fn notch_header(height: Pixels, status_color: Hsla) -> Div {
    row()
        .h(height)
        .flex_shrink_0()
        .child(
            row().w(px(40.)).h_full().justify_center().child(
                icon(IconName::SquareTerminal)
                    .size(px(15.))
                    .text_color(status_color),
            ),
        )
        .child(div().flex_1())
        .child(
            row().w(px(40.)).h_full().justify_center().child(
                custom_icon("sparkles")
                    .size(px(14.))
                    .text_color(status_color),
            ),
        )
}

fn session_indicator(color: Hsla) -> Div {
    row()
        .size(px(28.))
        .flex_shrink_0()
        .justify_center()
        .rounded(px(8.))
        .bg(hsla(0., 0., 1., 0.06))
        .child(
            div()
                .size(px(13.))
                .rounded_full()
                .border(px(1.5))
                .border_color(color)
                .flex()
                .items_center()
                .justify_center()
                .child(div().size(px(3.)).rounded_full().bg(color)),
        )
}

/// Caller supplies identity, data and on_click; no embedded mock or window handle.
pub fn notch_session_row(id: impl Into<ElementId>, session: &NotchSession) -> Stateful<Div> {
    row()
        .id(id)
        .h(px(NOTCH_ROW_HEIGHT))
        .flex_shrink_0()
        .px(px(12.))
        .py(px(8.))
        .gap(px(10.))
        .rounded(px(18.))
        .cursor_pointer()
        .hover(|s| s.bg(hsla(0., 0., 1., 0.08)))
        .child(session_indicator(session.status_color))
        .child(
            column()
                .flex_1()
                .gap(px(2.))
                .overflow_hidden()
                .child(
                    row()
                        .gap(px(6.))
                        .whitespace_nowrap()
                        .text_size(px(13.))
                        .line_height(px(17.55))
                        .text_color(hsla(0., 0., 1., 0.9))
                        .font_weight(FontWeight::MEDIUM)
                        .child(div().flex_shrink_0().child(session.workspace.clone()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .text_color(hsla(0., 0., 1., 0.35))
                                .child(session.context.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .line_height(px(14.85))
                        .text_color(session.status_color)
                        .truncate()
                        .child(session.status.clone()),
                ),
        )
        .child(icon(IconName::ChevronRight).text_color(hsla(0., 0., 1., 0.2)))
}

/// Compact overview filter; the controller owns selection and callbacks.
pub fn notch_filter(label: &'static str, selected: bool) -> Stateful<Div> {
    row()
        .id(label)
        .h(px(24.))
        .px(px(8.))
        .rounded(px(5.))
        .text_size(px(10.))
        .cursor_pointer()
        .bg(hsla(0., 0., 1., if selected { 0.14 } else { 0. }))
        .text_color(hsla(0., 0., 1., if selected { 0.9 } else { 0.5 }))
        .hover(|style| style.bg(hsla(0., 0., 1., 0.1)))
        .child(label)
}
