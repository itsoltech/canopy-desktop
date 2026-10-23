//! Badges of commands (`/name`), skills (`$name`) and pull requests (`#123`) in the
//! composer and in sent messages (DEC-071), drawn like attachment and quote badges.
use crate::ui::components::hover_card::content as card;
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

/// Icon, text and tint of a command, skill or pull request badge.
pub(super) fn look(attachment: &Attachment) -> Option<(&'static str, String, Hsla)> {
    match attachment {
        Attachment::Command {
            name, skill: true, ..
        } => Some(("box", skill_title(name), t::skill())),
        Attachment::Command { name, .. } => Some(("square-slash", format!("/{name}"), t::accent())),
        Attachment::Pull {
            number,
            title,
            state,
            ..
        } => Some((pull_icon(state), format!("#{number} {title}"), t::accent())),
        _ => None,
    }
}

/// What a badge stands for, on hover: the command's or skill's description, or
/// the pull request's branches and the start of its description.
fn mention_card(attachment: &Attachment) -> Div {
    let Some((icon, _, tint)) = look(attachment) else {
        return card::body();
    };
    let heading = |title: String, subtitle: String| {
        row()
            .items_start()
            .gap_1p5()
            .child(
                div()
                    .flex_shrink_0()
                    .pt(rems(0.1875))
                    .child(custom_icon(icon).size(rems(0.875)).text_color(tint)),
            )
            .child(
                column().flex_1().min_w_0().child(card::title(title)).child(
                    div()
                        .text_xs()
                        .line_height(t::chat_metadata_line_height())
                        .text_color(t::muted())
                        .child(subtitle),
                ),
            )
    };
    let prose = |text: &str| {
        div()
            .text_xs()
            .line_height(t::chat_metadata_line_height())
            .text_color(t::secondary())
            .line_clamp(4)
            .child(text.to_owned())
    };
    match attachment {
        Attachment::Command {
            name,
            description,
            skill,
            ..
        } => card::body()
            .child(if *skill {
                heading(skill_title(name), format!("Skill · ${name}"))
            } else {
                heading(format!("/{name}"), "Command".into())
            })
            .children(description.as_deref().filter(|d| !d.is_empty()).map(prose)),
        Attachment::Pull {
            number,
            title,
            head,
            base,
            state,
            body,
            ..
        } => {
            let mut state = state.clone();
            if let Some(first) = state.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            card::body()
                .child(heading(title.clone(), format!("#{number} · {state}")))
                .child(card::separator())
                .child(
                    card::facts().child(
                        card::fact(
                            gpui_kit::component::Icon::default().path("canopy/git-branch.svg"),
                        )
                        .child(div().min_w_0().truncate().child(format!("{head} → {base}"))),
                    ),
                )
                .children((!body.trim().is_empty()).then(|| prose(body.trim())))
        }
        _ => card::body(),
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
    let (icon, text, tint) = look(attachment)?;
    let id = id.into();
    let owner = format!("mention-{id:?}");
    let attachment = attachment.clone();
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
            .hover_card(owner, gpui_kit::base::Placement::Top, move |_, _| {
                mention_card(&attachment).into_any_element()
            }),
    )
}
