//! History presentation. The owning view retains selection, tasks and Presence.
use super::git_tracking::{reference_badge, upstream_membership};
use super::{super::theme as t, *};
use canopy_desktop::{
    git::{history::CommitEntry, history_graph::GraphRow},
    motion,
};
use gpui_kit::*;

/// Compact commit row; caller attaches its selection handler.
pub fn commit_row(
    commit: &CommitEntry,
    graph: &GraphRow,
    graph_width: usize,
    selected: bool,
) -> Stateful<Div> {
    row()
        .id(SharedString::from(commit.id.clone()))
        .w_full()
        .h(px(32.))
        .pr(px(4.))
        .gap(px(8.))
        .rounded(px(4.))
        .bg(if selected {
            t::selected()
        } else {
            rgba(0).into()
        })
        .hover(|s| s.bg(t::hover()))
        .cursor_pointer()
        .child(graph_lane(graph, graph_width, commit.parents.len() > 1))
        .children(commit.references.first().map(reference_badge))
        .children(
            (commit.references.len() > 1)
                .then(|| badge(format!("+{}", commit.references.len() - 1))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .text_color(t::text())
                .text_ellipsis()
                .child(commit.subject.clone()),
        )
        .children((commit.local_only == Some(true)).then(|| badge("Local").text_color(t::yellow())))
        .children((commit.parents.len() > 1).then(|| badge("Merge")))
        .child(
            div()
                .flex_shrink_0()
                .font_family(t::MONO)
                .text_size(px(10.))
                .text_color(t::muted())
                .child(commit.id[..7].to_owned()),
        )
}

/// Animated detail surface; progress comes from the caller's motion state.
/// A supplied close control keeps event ownership and focus behavior in the view.
pub fn commit_details(commit: &CommitEntry, progress: f32, close_action: impl IntoElement) -> Div {
    column()
        .h(px(200. * progress))
        .flex_shrink_0()
        .overflow_hidden()
        .child(
            column()
                .h(px(200.))
                .w_full()
                .relative()
                .top(px(motion::distance::BASE * (1. - progress)))
                .opacity(progress)
                .id("commit-details")
                .overflow_y_scroll()
                .border_t_1()
                .border_color(t::border())
                .py(px(12.))
                .gap(px(8.))
                .child(
                    row()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(11.))
                                .text_color(t::secondary())
                                .child("Commit details"),
                        )
                        .child(close_action),
                )
                .child(
                    div()
                        .font_family(t::MONO)
                        .text_size(px(10.))
                        .text_color(t::accent())
                        .child(commit.id.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(t::muted())
                        .child(format!(
                            "{} · {} · {} parent{}",
                            commit.author,
                            age(commit.seconds),
                            commit.parents.len(),
                            if commit.parents.len() == 1 { "" } else { "s" }
                        )),
                )
                .child(
                    row()
                        .flex_wrap()
                        .gap(px(4.))
                        .children(commit.references.iter().map(reference_badge)),
                )
                .child(upstream_membership(commit.local_only))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t::text())
                        .child(commit.message.clone()),
                ),
        )
}

fn age(seconds: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(seconds);
    let elapsed = now.saturating_sub(seconds).max(0);
    if elapsed < 60 {
        "just now".into()
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{}h ago", elapsed / 3600)
    } else {
        format!("{}d ago", elapsed / 86400)
    }
}

fn graph_lane(graph: &GraphRow, width: usize, merge: bool) -> Div {
    let edges = graph.edges.clone();
    let x = |lane: usize| 8. + lane as f32 * 12.;
    div()
        .relative()
        .w(px(width as f32 * 12. + 4.))
        .h(px(32.))
        .flex_shrink_0()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    for edge in &edges {
                        let from = bounds.origin + point(px(x(edge.from)), px(edge.start * 32.));
                        let to = bounds.origin + point(px(x(edge.to)), px(edge.end * 32.));
                        let mut path = PathBuilder::stroke(px(1.));
                        path.move_to(from);
                        if edge.from == edge.to {
                            path.line_to(to);
                        } else {
                            path.curve_to(to, point(from.x, to.y));
                        }
                        if let Ok(path) = path.build() {
                            window
                                .paint_path(path, t::history_lane_color(edge.color).opacity(0.55));
                        }
                    }
                },
            )
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .left(px(x(graph.lane) - 3.))
                .top(px(13.))
                .size(px(6.))
                .rounded_full()
                .border_1()
                .border_color(if merge {
                    t::history_lane_color(graph.color)
                } else {
                    t::sidebar()
                })
                .bg(if merge {
                    t::sidebar()
                } else {
                    t::history_lane_color(graph.color)
                }),
        )
}

/// Static loading placeholder with the exact commit-row geometry.
/// No shimmer timer: loading never creates a perpetual animation loop.
pub fn commit_skeleton(index: usize) -> Div {
    let widths = [0.68, 0.48, 0.76, 0.56];
    row()
        .w_full()
        .h(px(32.))
        .pr(px(4.))
        .gap(px(8.))
        .child(
            div()
                .w(px(16.))
                .flex_shrink_0()
                .child(skeleton_bar().ml(px(5.)).size(px(6.)).rounded_full()),
        )
        .child(
            div().flex_1().min_w_0().child(
                skeleton_bar()
                    .w(relative(widths[index % widths.len()]))
                    .h(px(8.)),
            ),
        )
        .child(skeleton_bar().w(px(42.)).h(px(6.)).flex_shrink_0())
}
