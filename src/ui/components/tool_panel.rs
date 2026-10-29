//! Shared geometry of the tool panel pages (Files, Changes, Tasks, Agent): a
//! header with the page's context and actions, inset lists, a docked footer and
//! one empty state. Pages keep their state, data and callbacks.
use super::{super::theme as t, column, custom_icon, row};
use gpui_kit::{prelude::FluentBuilder, *};

/// Horizontal inset of headers, fields and footers.
pub const PANEL_INSET: f32 = 12.;
/// Horizontal inset of lists: their rows carry their own padding.
pub const LIST_INSET: f32 = 6.;
/// Height of a list row in every page.
pub const LIST_ROW: f32 = 26.;

/// Page header: context on the left, 20px icon actions on the right.
pub fn panel_header() -> Div {
    row()
        .w_full()
        .h(px(38.))
        .flex_shrink_0()
        .pl(px(PANEL_INSET))
        .pr(px(8.))
        .gap(px(6.))
}

/// Section title in small capitals.
pub fn panel_caption(label: &str) -> Div {
    div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t::faint())
        .child(label.to_uppercase())
}

/// The checked-out branch, truncated from the end.
pub fn branch_label(name: impl Into<SharedString>) -> Div {
    row()
        .min_w_0()
        .gap(px(5.))
        .text_size(px(11.))
        .font_family(t::MONO)
        .text_color(t::secondary())
        .child(
            custom_icon("git-branch")
                .size(px(12.))
                .text_color(t::faint())
                .flex_shrink_0(),
        )
        .child(div().min_w_0().truncate().child(name.into()))
}

/// A small count beside a tab or a group title.
pub fn count_badge(count: usize, selected: bool) -> Div {
    div()
        .flex_shrink_0()
        .px(px(4.))
        .rounded(px(4.))
        .bg(if selected {
            t::selected()
        } else {
            t::elevated()
        })
        .font_family(t::MONO)
        .text_size(px(10.))
        .line_height(px(15.))
        .text_color(if selected {
            t::accent()
        } else {
            t::secondary()
        })
        .child(count.to_string())
}

/// A state or label in its color on a faint fill of the same color.
pub fn status_badge(label: impl Into<SharedString>, color: Hsla) -> Div {
    row()
        .flex_shrink_0()
        .h(px(16.))
        .px(px(5.))
        .rounded(px(4.))
        .bg(color.opacity(0.12))
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label.into())
}

/// A compact toggle in a row of filters; the caller handles the click.
pub fn filter_chip(id: impl Into<ElementId>, label: &'static str, selected: bool) -> Stateful<Div> {
    row()
        .id(id)
        .flex_shrink_0()
        .h(px(18.))
        .px(px(6.))
        .rounded(px(4.))
        .cursor_pointer()
        .tab_index(0)
        .role(Role::Button)
        .aria_label(label)
        .aria_toggled(if selected {
            Toggled::True
        } else {
            Toggled::False
        })
        .bg(if selected {
            t::selected()
        } else {
            t::elevated()
        })
        .font_family(t::MONO)
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if selected { t::accent() } else { t::faint() })
        .when(!selected, |chip| {
            chip.hover(|style| style.bg(t::hover()).text_color(t::text()))
        })
        .focus_visible(|style| style.border_1().border_color(t::accent()))
        .child(label)
}

/// Added and deleted line counts; a zero side is left out.
pub fn line_counts(added: usize, deleted: usize) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(4.))
        .font_family(t::MONO)
        .text_size(px(10.5))
        .children((added > 0).then(|| div().text_color(t::green()).child(format!("+{added}"))))
        .children((deleted > 0).then(|| div().text_color(t::red()).child(format!("−{deleted}"))))
}

/// The share of added against deleted lines.
pub fn line_bar(added: usize, deleted: usize) -> Div {
    let total = (added + deleted).max(1) as f32;
    row()
        .flex_1()
        .min_w(px(30.))
        .h(px(4.))
        .rounded(px(2.))
        .overflow_hidden()
        .bg(t::elevated())
        .child(
            div()
                .h_full()
                .w(relative(added as f32 / total))
                .bg(t::green()),
        )
        .child(
            div()
                .h_full()
                .w(relative(deleted as f32 / total))
                .bg(t::red()),
        )
}

/// Footer docked to the bottom edge for the page's main actions.
pub fn panel_footer() -> Div {
    column()
        .w_full()
        .flex_shrink_0()
        .gap(px(8.))
        .px(px(PANEL_INSET))
        .pt(px(10.))
        .pb(px(12.))
        .border_t_1()
        .border_color(t::border())
}

/// Empty or unavailable page: a title, one sentence about what comes next and
/// optionally one action, centered in the remaining space.
pub fn panel_empty(title: impl Into<SharedString>, description: impl Into<SharedString>) -> Div {
    column()
        .flex_1()
        .w_full()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .px(px(28.))
        .text_center()
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t::text())
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(11.5))
                .line_height(px(17.))
                .text_color(t::faint())
                .child(description.into()),
        )
}
