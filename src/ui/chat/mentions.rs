//! Badges of commands (`/name`), skills (`$name`) and pull requests (`#123`) in the
//! composer and in sent messages (DEC-071), drawn like attachment and quote badges.
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
        Attachment::Command {
            name, skill: true, ..
        } => format!("[${name}]"),
        Attachment::Command { name, .. } => format!("[/{name}]"),
        Attachment::Pull { number, title, .. } => {
            let title: String = clean(title).chars().take(60).collect();
            format!("[#{number} {title}]")
        }
        _ => String::new(),
    }
}

/// A skill's name as its badge reads: `gpui-kit` → `Gpui Kit`.
pub(super) fn skill_title(name: &str) -> String {
    name.rsplit(':')
        .next()
        .unwrap_or(name)
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Icon, text, tooltip and tint of a command, skill or pull request badge.
pub(super) fn look(attachment: &Attachment) -> Option<(&'static str, String, String, Hsla)> {
    match attachment {
        Attachment::Command {
            name,
            description,
            skill: true,
            ..
        } => Some((
            "box",
            skill_title(name),
            match description.as_deref().filter(|d| !d.is_empty()) {
                Some(description) => format!("${name}\n{description}"),
                None => format!("${name}"),
            },
            t::skill(),
        )),
        Attachment::Command {
            name, description, ..
        } => Some((
            "square-slash",
            format!("/{name}"),
            description
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| format!("/{name}")),
            t::accent(),
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
            t::accent(),
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

/// A command, skill or pull request badge at `height` (a line of text), `None` for other
/// attachments.
pub(super) fn mention_badge(
    id: impl Into<ElementId>,
    attachment: &Attachment,
    height: Pixels,
    selected: bool,
) -> Option<Stateful<Div>> {
    let (icon, text, tooltip, tint) = look(attachment)?;
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
            .border_color(if selected { tint } else { tint.opacity(0.14) })
            .bg(tint.opacity(if selected { 0.2 } else { 0.08 }))
            .hover(|style| style.bg(tint.opacity(0.14)))
            .text_color(t::text())
            .child(custom_icon(icon).size(height - px(8.)).text_color(tint))
            .child(div().min_w_0().truncate().child(text))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            }),
    )
}
