//! Presentation of branch references and upstream comparison; no Git I/O.
use super::{super::theme as t, column, row};
use canopy_desktop::git::history::{HistorySnapshot, ReferenceKind, ReferenceLabel};
use gpui_kit::*;

pub fn reference_badge(reference: &ReferenceLabel) -> Div {
    let color = match reference.kind {
        ReferenceKind::Tag => t::yellow(),
        ReferenceKind::RemoteBranch => t::green(),
        ReferenceKind::LocalBranch => t::accent(),
    };
    row()
        .h(px(16.))
        .max_w(px(96.))
        .px(px(4.))
        .rounded(px(3.))
        .bg(color.opacity(0.1))
        .text_color(color)
        .text_size(px(10.))
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .child(reference.name.clone()),
        )
}

pub fn tracking_summary(snapshot: Option<&HistorySnapshot>) -> Div {
    column()
        .h(px(36.))
        .flex_shrink_0()
        .gap(px(3.))
        .children(snapshot.map(|snapshot| {
            row()
                .gap(px(8.))
                .text_size(px(11.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .font_family(t::MONO)
                        .text_color(t::faint())
                        .child(
                            snapshot
                                .upstream
                                .clone()
                                .unwrap_or_else(|| snapshot.branch.clone()),
                        ),
                )
                .children(
                    snapshot
                        .upstream
                        .as_ref()
                        .map(|_| ahead_behind(snapshot.ahead, snapshot.behind)),
                )
        }))
        .children(snapshot.map(|snapshot| {
            div()
                .text_size(px(10.))
                .text_color(t::muted())
                .child(snapshot.note.clone())
        }))
}

/// Counts refer to the same upstream snapshot; visibility belongs to the caller.
pub fn ahead_behind(ahead: usize, behind: usize) -> Div {
    row()
        .gap(px(8.))
        .font_family(t::MONO)
        .child(div().text_color(t::accent()).child(format!("↑{ahead}")))
        .child(div().text_color(t::faint()).child(format!("↓{behind}")))
}

/// Explicitly distinguish unknown membership from a shared upstream commit.
pub fn upstream_membership(local_only: Option<bool>) -> Div {
    div()
        .text_size(px(11.))
        .text_color(t::muted())
        .child(match local_only {
            Some(true) => "Local only relative to upstream",
            Some(false) => "Included in upstream history (last known state)",
            None => "Upstream membership unavailable",
        })
}
