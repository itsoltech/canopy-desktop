//! Presentation-only Git components. Callers retain tasks, state and callbacks.
use super::{super::theme as t, *};
use canopy_desktop::git::changes::{DiffLine, FileChange};
use gpui_kit::{
    component::{IconName, button::Button},
    *,
};

/// File identity and comparison statistics; append toolbar actions at the call site.
pub fn diff_header(target: Option<&FileChange>, additions: usize, deletions: usize) -> Div {
    row()
        .h(px(40.))
        .flex_shrink_0()
        .px(px(12.))
        .gap(px(8.))
        .border_b_1()
        .border_color(t::border())
        .child(icon(IconName::File))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_color(t::secondary())
                .child(
                    target
                        .as_ref()
                        .map(|f| f.path.display().to_string())
                        .unwrap_or_default(),
                ),
        )
        .child(div().text_size(px(11.)).text_color(t::muted()).child(
            if target.as_ref().is_some_and(|f| f.staged) {
                "Staged"
            } else {
                "Working tree"
            },
        ))
        .child(
            div()
                .text_size(px(11.))
                .text_color(t::green())
                .child(format!("+{}", additions)),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(t::red())
                .child(format!("−{}", deletions)),
        )
}

/// A single virtualized unified-diff row, including gutter and change rail.
pub fn diff_line(line: &DiffLine) -> Div {
    let changed = matches!(line.kind, '+' | '-');
    let color = if line.kind == '+' {
        t::green()
    } else {
        t::red()
    };
    let hunk = line.kind == 'H';
    row()
        .w_full()
        .min_w_full()
        .h(px(24.))
        .font_family(t::MONO)
        .text_size(px(12.))
        .text_color(if hunk { t::muted() } else { t::text() })
        .bg(t::diff_line_background(line.kind))
        .border_l_2()
        .border_color(if changed {
            color.opacity(0.7)
        } else {
            rgba(0).into()
        })
        .child(
            row()
                .w(px(96.))
                .flex_shrink_0()
                .h_full()
                .border_r_1()
                .border_color(t::border())
                .text_color(if changed {
                    color.opacity(0.65)
                } else {
                    t::faint()
                })
                .children([line.old, line.new].into_iter().map(|number| {
                    div()
                        .w(px(48.))
                        .text_right()
                        .pr(px(12.))
                        .child(number.map(|v| v.to_string()).unwrap_or_default())
                })),
        )
        .child(
            div()
                .w(px(28.))
                .flex_shrink_0()
                .text_center()
                .text_color(color)
                .child(if changed {
                    line.kind.to_string()
                } else {
                    String::new()
                }),
        )
        .child(
            div()
                .flex_shrink_0()
                .pr(px(24.))
                .whitespace_nowrap()
                .child(line.text.replace('\t', "    ")),
        )
}

/// Matching geometry for per-file stage, unstage and discard actions.
pub fn change_action(
    id: impl Into<ElementId>,
    name: IconName,
    label: impl Into<SharedString>,
) -> Button {
    icon_button(id, name, label)
        .min_w(px(20.))
        .max_w(px(20.))
        .flex_shrink_0()
}

/// Group title and count. Append the relevant action without owning its behavior.
pub fn change_group(staged: bool, count: usize) -> Div {
    row()
        .w_full()
        .h(px(28.))
        .gap(px(8.))
        .child(
            div()
                .text_size(px(11.))
                .text_color(t::secondary())
                .child(format!(
                    "{} {count}",
                    if staged { "Staged changes" } else { "Changes" }
                )),
        )
        .child(div().flex_1())
}

/// Commit form surface; inputs and their owning entities remain with the panel.
pub fn commit_form() -> Div {
    column()
        .flex_shrink_0()
        .gap(px(8.))
        .py(px(12.))
        .border_t_1()
        .border_color(t::border())
}

/// File status and icon; the parent row owns hover, while its children own actions.
pub fn change_file_row(id: impl Into<ElementId>, kind: &str) -> Stateful<Div> {
    let color = match kind {
        "A" => t::green(),
        "D" => t::red(),
        "R" => t::accent(),
        _ => t::yellow(),
    };
    row()
        .w_full()
        .id(id)
        .h(px(28.))
        .px(px(4.))
        .rounded(px(4.))
        .gap(px(8.))
        .child(
            div()
                .w(px(12.))
                .flex_shrink_0()
                .text_color(color)
                .text_size(px(11.))
                .child(kind.to_owned()),
        )
        .child(icon(IconName::File).flex_shrink_0())
}

/// Fixed inspector toolbar geometry shared by Files and History.
pub fn inspector_toolbar(label: impl Into<SharedString>, action: impl IntoElement) -> Div {
    row()
        .h(px(t::ROW))
        .flex_shrink_0()
        .gap(px(t::SPACING_UNIT * 2.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_size(px(11.))
                .text_color(t::muted())
                .child(label.into()),
        )
        .child(action)
}
