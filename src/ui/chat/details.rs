//! Before the first message, where and with whom the chat works: agent and model,
//! the computer of a chat on another one, project and branch, and the folder. Read
//! from the runtime, the project catalogue, the Git watcher and the host's shared
//! catalogue; no I/O. Account, model choice and mode stay with the composer's pickers.
use super::view::ChatView;
use crate::app_state::ThreadPlace;
use crate::ui::components::checkout_tag;
use crate::ui::worktree_overview::{agent_mark, home_relative, path_label};
use crate::ui::{components::*, theme as t};
use gpui_kit::component::{Icon, IconName};
use gpui_kit::{prelude::FluentBuilder, *};

impl ChatView {
    /// The empty conversation's facts; gone with the first message.
    pub(super) fn details(&self, cx: &App) -> Div {
        let runtime = self.rt(cx);
        let agent = runtime.agent_name(cx);
        let model = runtime.model_label(cx);
        let ThreadPlace {
            project,
            private,
            folder,
            host,
        } = runtime.place(cx);
        let folder = folder.map(|folder| {
            (
                folder
                    .display
                    .unwrap_or_else(|| home_relative(&folder.path)),
                folder.path.display().to_string(),
            )
        });
        column()
            .gap_1()
            .text_xs()
            .line_height(t::chat_metadata_line_height())
            .text_color(t::secondary())
            .child(line(agent_mark(runtime.provider(), &agent)).child(
                div().min_w_0().truncate().child(match model {
                    Some(model) => format!("{agent} · {model}"),
                    None => agent,
                }),
            ))
            .children(host.map(|host| {
                line(glyph("canopy/monitor.svg"))
                    .child(div().min_w_0().truncate().child(format!("On {host}")))
            }))
            .children(project.map(|project| {
                line(glyph("canopy/git-branch.svg"))
                    .child(div().min_w_0().truncate().child(match &project.head {
                        Some(head) => format!("{} · {head}", project.name),
                        None => project.name.clone(),
                    }))
                    .when(project.main, |line| line.child(checkout_tag("default")))
            }))
            .when(private, |details| {
                details.child(line(glyph("canopy/shield.svg")).child("Private chat, no project"))
            })
            .children(folder.map(|(shown, copied)| {
                line(Icon::new(IconName::Folder).size_3().text_color(t::faint()))
                    .child(path_label(shown))
                    .child(
                        icon_button("chat-copy-folder", IconName::Copy, "Copy path")
                            .flex_shrink_0()
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                            }),
                    )
            }))
    }
}

fn glyph(path: &'static str) -> Icon {
    Icon::default().path(path).size_3().text_color(t::faint())
}

/// An icon-led fact; the icon stands in for its label (as in the sidebar's cards).
fn line(lead: impl IntoElement) -> Div {
    row().w_full().min_w_0().gap_2().child(
        div()
            .flex_shrink_0()
            .w_3p5()
            .flex()
            .justify_center()
            .child(lead),
    )
}
