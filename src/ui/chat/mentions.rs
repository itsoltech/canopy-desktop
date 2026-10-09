//! Badges of commands (`/name`) and pull requests (`#123`) in the composer and in sent
//! messages (DEC-071), drawn like attachment and quote badges.
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::transcript::Attachment;
use gpui_kit::*;

/// The mention a badge stands for in the message text: `[/review]`, `[#42 Add find]`.
pub(super) fn mention_of(attachment: &Attachment) -> String {
    let clean = |text: &str| -> String {
        text.chars()
            .map(|c| {
                if c.is_control() || c == ']' || c == '[' {
                    ' '
                } else {
                    c
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    match attachment {
        Attachment::Command { name, .. } => format!("[/{name}]"),
        Attachment::Pull { number, title, .. } => {
            let title: String = clean(title).chars().take(60).collect();
            format!("[#{number} {title}]")
        }
        _ => String::new(),
    }
}

/// Icon, text and tooltip of a command or pull request badge.
pub(super) fn look(attachment: &Attachment) -> Option<(&'static str, String, String)> {
    match attachment {
        Attachment::Command {
            name, description, ..
        } => Some((
            "square-slash",
            format!("/{name}"),
            description
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| format!("/{name}")),
        )),
        Attachment::Pull {
            number,
            title,
            head,
            base,
            state,
            ..
        } => Some((
            pull_icon(state),
            format!("#{number} {title}"),
            format!("#{number} {title}\n{head} → {base} · {state}"),
        )),
        _ => None,
    }
}

/// The pull request icon for its state.
pub(super) fn pull_icon(state: &str) -> &'static str {
    match state {
        "merged" => "git-merge",
        "closed" => "git-pull-request-closed",
        "draft" => "git-pull-request-draft",
        _ => "git-pull-request",
    }
}

/// A command or pull request badge at `height` (a line of text), `None` for other
/// attachments.
pub(super) fn mention_badge(
    id: impl Into<ElementId>,
    attachment: &Attachment,
    height: Pixels,
    selected: bool,
) -> Option<Stateful<Div>> {
    let (icon, text, tooltip) = look(attachment)?;
    let tooltip: SharedString = tooltip.into();
    Some(
        row()
            .id(id)
            .h(height)
            .min_w_0()
            .gap(px(4.))
            .px(px(5.))
            .rounded(px(5.))
            .border_1()
            .border_color(if selected {
                t::accent()
            } else {
                t::accent().opacity(0.14)
            })
            .bg(t::accent().opacity(if selected { 0.2 } else { 0.08 }))
            .hover(|style| style.bg(t::accent().opacity(0.14)))
            .text_color(t::text())
            .child(
                custom_icon(icon)
                    .size(height - px(8.))
                    .text_color(t::accent()),
            )
            .child(div().min_w_0().truncate().child(text))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            }),
    )
}
