//! Before the first message, where and with whom the chat works: agent and model,
//! the computer of a chat on another one, project and branch, and the folder. Read
//! from the runtime, the project catalogue, the Git watcher and the host's shared
//! catalogue; no I/O. Account, model choice and mode stay with the composer's pickers.
use super::view::ChatView;
use crate::app_state::AppState;
use crate::ui::components::checkout_tag;
use crate::ui::worktree_overview::{Checkout, agent_mark, home_relative, path_label};
use crate::ui::{components::*, theme as t};
use canopy_desktop::git::RepositoryIdentity;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::{prelude::FluentBuilder, *};

/// The checkout a chat works in.
struct Project {
    name: String,
    head: Option<String>,
    main: bool,
}

/// Where a chat works, as far as this device knows.
#[derive(Default)]
struct Place {
    /// `None` with `private` unset: not known (yet).
    project: Option<Project>,
    private: bool,
    /// Shown and copied text of the working folder.
    folder: Option<(String, String)>,
    /// The other computer that runs the chat.
    host: Option<String>,
}

impl ChatView {
    fn place(&self, cx: &App) -> Place {
        let runtime = self.rt(cx);
        if let Some(remote) = runtime.remote_source() {
            let mut place = Place {
                host: remote.host.as_ref().map(|host| host.name()),
                ..Default::default()
            };
            let Some(summary) = remote.projection.summary.as_ref() else {
                return place;
            };
            let Some(workspace) = summary.workspace.as_ref() else {
                place.private = true;
                return place;
            };
            let catalog = remote.host.as_ref().and_then(|host| host.shared_catalog());
            let found = catalog.as_ref().and_then(|catalog| {
                catalog.projects.iter().find_map(|project| {
                    let checkout = project
                        .checkouts
                        .iter()
                        .find(|c| c.workspace.as_ref() == Some(workspace))?;
                    Some((project, checkout))
                })
            });
            if let Some((project, checkout)) = found {
                let name = project
                    .origin
                    .as_deref()
                    .and_then(RepositoryIdentity::from_origin)
                    .map_or_else(|| project.name.clone(), |o| o.path().to_owned());
                place.project = Some(Project {
                    name,
                    head: Some(
                        checkout
                            .branch
                            .clone()
                            .unwrap_or_else(|| checkout.name.clone()),
                    ),
                    main: checkout.repository_path.as_deref() == Some(checkout.path.as_str()),
                });
                // A path on the other computer: shown as that computer reports it.
                place.folder = Some((checkout.path.clone(), checkout.path.clone()));
            }
            return place;
        }
        let app = cx.global::<AppState>();
        let projects = app.projects.read(cx);
        let cwd = runtime.cwd();
        let workspace = runtime
            .chat_id()
            .and_then(|chat| app.chats.read(cx).workspace_of_chat(&chat));
        let project = workspace
            .and_then(|id| projects.catalog.items.iter().find(|p| p.workspace == id))
            .or_else(|| {
                let cwd = cwd.as_ref()?;
                projects
                    .catalog
                    .items
                    .iter()
                    .find(|p| p.worktree_path.as_ref().unwrap_or(&p.path) == cwd)
            });
        Place {
            project: project.map(|project| {
                let checkout = Checkout::of(project, app.git.read(cx));
                Project {
                    name: checkout.project,
                    head: checkout.head.ok(),
                    main: checkout.main,
                }
            }),
            // Chats outside every project live in the Private chats space.
            private: project.is_none() && workspace.is_some(),
            folder: cwd.map(|cwd| (home_relative(&cwd), cwd.display().to_string())),
            host: None,
        }
    }

    /// The empty conversation's facts; gone with the first message.
    pub(super) fn details(&self, cx: &App) -> Div {
        let runtime = self.rt(cx);
        let agent = runtime.agent_name(cx);
        let model = runtime.model_label(cx);
        let Place {
            project,
            private,
            folder,
            host,
        } = self.place(cx);
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
