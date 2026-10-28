//! Presentation shared by the remote window, embedded workspace and tool panel.
//! Builders only: state, tasks and callbacks remain with `RemoteWindow`.
use super::*;
use gpui_kit::component::{Icon, IconName, Sizable, spinner::Spinner};

/// Navigation column, matching Preferences.
pub(super) const NAV_WIDTH: f32 = 208.;
/// Page inset in the standalone window; the narrow tool panel uses `COMPACT_X`.
pub(super) const CONTENT_X: f32 = 28.;
pub(super) const COMPACT_X: f32 = 12.;

pub(super) fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Ready => "Connected",
        Phase::Connecting => "Connecting…",
        Phase::Offline => "Offline",
    }
}
fn phase_color(phase: Phase) -> Hsla {
    match phase {
        Phase::Ready => t::green(),
        Phase::Connecting => t::yellow(),
        Phase::Offline => t::faint(),
    }
}
pub(super) fn status_dot(phase: Phase) -> Div {
    dot().bg(phase_color(phase))
}

/// Title, subtitle and a hairline below; trailing actions are appended by the caller.
pub(super) fn page_header(
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
    inset: f32,
) -> Div {
    let subtitle: SharedString = subtitle.into();
    row()
        .flex_shrink_0()
        .gap(px(16.))
        .px(px(inset))
        .py(px(16.))
        .border_b_1()
        .border_color(t::border())
        .child(
            column()
                .flex_1()
                .min_w_0()
                .gap(px(4.))
                .child(
                    div()
                        .truncate()
                        .text_size(px(18.))
                        .line_height(px(24.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.into()),
                )
                .children((!subtitle.is_empty()).then(|| {
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(t::secondary())
                        .child(subtitle)
                })),
        )
}

/// Navigation destination drawn like Preferences, with Button semantics:
/// keyboard focus, accessible name and press feedback come from the control.
fn nav_button(
    id: impl Into<ElementId>,
    accessible: SharedString,
    active: bool,
    content: Div,
    cx: &App,
) -> Button {
    use gpui_kit::component::button::{ButtonCustomVariant, ButtonVariants};
    let surface = if active {
        t::selected()
    } else {
        hsla(0., 0., 0., 0.)
    };
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(surface)
                .foreground(if active { t::text() } else { t::secondary() })
                .hover(if active { t::selected() } else { t::hover() })
                .active(t::selected()),
        )
        .compact()
        .w_full()
        .h(px(33.))
        .flex_shrink_0()
        .pl(px(12.))
        .pr(px(8.))
        .rounded(px(4.))
        .border_0()
        .accessibility_label(accessible)
        .child(
            content
                .relative()
                .w_full()
                .h(px(33.))
                .gap(px(8.))
                .text_size(px(13.))
                .text_color(if active { t::text() } else { t::secondary() })
                .children(active.then(|| {
                    div()
                        .absolute()
                        .left(px(-12.))
                        .top(px(8.5))
                        .w(px(2.))
                        .h(px(16.))
                        .rounded(px(3.))
                        .bg(t::accent())
                })),
        )
}

/// Preferences navigation row: icon and label.
pub(super) fn nav_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    glyph: Icon,
    active: bool,
    cx: &App,
) -> Button {
    let label: SharedString = label.into();
    nav_button(
        id,
        label.clone(),
        active,
        row()
            .child(glyph.text_color(if active { t::accent() } else { t::muted() }))
            .child(div().flex_1().min_w_0().truncate().text_left().child(label)),
        cx,
    )
}

/// Computer row: status dot in the icon slot, state text in the trailing lane.
pub(super) fn host_item(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    phase: Phase,
    active: bool,
    cx: &App,
) -> Button {
    let name: SharedString = name.into();
    nav_button(
        id,
        format!("{name}, {}", phase_label(phase)).into(),
        active,
        row()
            .child(
                row()
                    .size(px(14.))
                    .flex_shrink_0()
                    .justify_center()
                    .child(status_dot(phase)),
            )
            .child(div().flex_1().min_w_0().truncate().text_left().child(name))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .text_color(t::faint())
                    .child(phase_label(phase)),
            ),
        cx,
    )
}

pub(super) fn nav_group(label: &'static str, first: bool) -> Div {
    column()
        .flex_shrink_0()
        .mt(px(if first { 0. } else { 12. }))
        .child(caption(label).px(px(8.)).pb(px(4.)).line_height(px(16.)))
}

/// Caption with optional trailing actions on the same baseline.
pub(super) fn section_title(label: impl Into<SharedString>) -> Div {
    row()
        .h(px(24.))
        .flex_shrink_0()
        .gap(px(8.))
        .child(caption(label).flex_1())
}

/// Selectable list row; hover, keyboard focus and selection share one surface.
pub(super) fn list_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    row()
        .id(id)
        .keyboard(label)
        .w_full()
        .h(px(32.))
        .flex_shrink_0()
        .px(px(8.))
        .gap(px(8.))
        .rounded(px(4.))
        .cursor_pointer()
        .when(selected, |row| row.bg(t::selected()))
        .when(!selected, |row| row.hover(|style| style.bg(t::hover())))
}

/// Primary text with secondary metadata on the same line.
pub(super) fn row_text(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Div {
    let detail: SharedString = detail.into();
    row()
        .flex_1()
        .min_w_0()
        .gap(px(8.))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(13.))
                .child(title.into()),
        )
        .children((!detail.is_empty()).then(|| {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(11.))
                .text_color(t::muted())
                .child(detail)
        }))
}

pub(super) fn loading(label: impl Into<SharedString>) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(8.))
        .py(px(4.))
        .child(Spinner::new().xsmall().color(t::muted()))
        .child(
            div()
                .text_size(px(12.))
                .text_color(t::muted())
                .child(label.into()),
        )
}

/// Confirms a completed action whose result is not otherwise visible.
pub(super) fn done(message: impl Into<SharedString>) -> Div {
    row()
        .flex_shrink_0()
        .items_start()
        .gap(px(6.))
        .child(
            icon(IconName::CircleCheck)
                .size(px(12.))
                .mt(px(2.))
                .text_color(t::green()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(t::secondary())
                .child(message.into()),
        )
}

pub(super) fn failure(message: impl Into<SharedString>) -> Div {
    integrations::integration_message(message, true).flex_shrink_0()
}
pub(super) fn hint(message: impl Into<SharedString>) -> Div {
    integrations::integration_message(message, false).flex_shrink_0()
}

/// One sentence of guidance; the caller appends the action that resolves it.
pub(super) fn empty(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Div {
    column()
        .flex_shrink_0()
        .w_full()
        .items_center()
        .py(px(32.))
        .gap(px(8.))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t::secondary())
                .child(title.into()),
        )
        .child(
            div()
                .max_w(px(360.))
                .text_center()
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(t::muted())
                .child(detail.into()),
        )
}

/// Two-state or three-state choice drawn as one silhouette.
pub(super) fn segmented(children: impl IntoIterator<Item = Button>) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(6.))
        .bg(t::hover())
        .children(children)
}
pub(super) fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Button {
    selection_button(id, label, selected)
        .flex_1()
        .border_0()
        .text_color(if selected { t::text() } else { t::secondary() })
}

/// Verbatim machine values (fingerprints, paths) that users compare by eye.
pub(super) fn mono_value(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .px(px(8.))
        .py(px(6.))
        .rounded(px(4.))
        .bg(t::input_bg())
        .font_family(t::MONO)
        .text_size(px(12.))
        .text_color(t::secondary())
        .child(text.into())
}

pub(super) fn short_oid(oid: &str) -> String {
    oid.chars().take(7).collect()
}

use gpui_kit::component::button::Button;
