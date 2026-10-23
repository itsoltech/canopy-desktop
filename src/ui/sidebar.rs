mod chats;
mod projects;
use super::{components::*, theme as t};
pub use chats::RenameChat;
pub(super) use chats::{navigation_status, status_color};
use gpui_kit::component::{Icon, IconName, button::Button, scroll::ScrollableElement};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Instant;

pub struct Sidebar {
    scroll: ScrollHandle,
    private_scroll: ScrollHandle,
    git: Entity<crate::app_state::GitState>,
    terminals: Entity<crate::app_state::Terminals>,
    selected_workspace: Option<canopy_desktop::state::workspace::WorkspaceId>,
    collapsed_worktrees: std::collections::HashSet<std::path::PathBuf>,
    project_groups: std::collections::HashMap<std::path::PathBuf, Disclosure>,
    _tool_events: Vec<Subscription>,
    projects: Entity<crate::app_state::ProjectsState>,
    _projects_observer: Subscription,
    sections: [Disclosure; 2],
    attach_hovered: bool,
    focus: FocusHandle,
}
impl Sidebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let scroll = ScrollHandle::new();
        let projects = cx.global::<crate::app_state::AppState>().projects.clone();
        let observer = cx.observe(&projects, |this, _, cx| this.sync_project_groups(cx));
        let app = cx.global::<crate::app_state::AppState>().clone();
        let tools = app.tools.clone();
        let terminals = app.terminals.clone();
        let tool_events = vec![
            cx.observe(&app.files, |_, _, cx| cx.notify()),
            cx.observe(&app.git, |_, _, cx| cx.notify()),
            cx.observe(&tools, |_, _, cx| cx.notify()),
            cx.observe(&terminals, |_, _, cx| {
                cx.notify();
            }),
            cx.observe(&app.chats, |_, _, cx| cx.notify()),
            cx.observe(&app.workspace, |_, _, cx| cx.notify()),
            cx.observe(&app.agents, |_, _, cx| cx.notify()),
            cx.observe(&app.chat_history, |_, _, cx| cx.notify()),
            cx.observe(&app.settings, |_, _, cx| cx.notify()),
        ];
        let now = Instant::now();
        Self {
            git: app.git.clone(),
            terminals,
            selected_workspace: projects.read(cx).catalog.active,
            collapsed_worktrees: projects
                .read(cx)
                .catalog
                .items
                .iter()
                .filter(|p| {
                    projects.read(cx).catalog.active.is_some()
                        && Some(p.workspace) != projects.read(cx).catalog.active
                })
                .map(|p| p.path.clone())
                .collect(),
            project_groups: projects
                .read(cx)
                .catalog
                .items
                .iter()
                .map(|p| {
                    (
                        p.repository_path.as_ref().unwrap_or(&p.path).clone(),
                        Disclosure::new(
                            projects.read(cx).catalog.current().is_none_or(|active| {
                                active.repository_path.as_ref().unwrap_or(&active.path)
                                    == p.repository_path.as_ref().unwrap_or(&p.path)
                            }),
                            now,
                        ),
                    )
                })
                .collect(),
            _tool_events: tool_events,
            scroll,
            private_scroll: ScrollHandle::new(),
            projects,
            _projects_observer: observer,
            sections: std::array::from_fn(|_| Disclosure::new(true, now)),
            attach_hovered: false,
            focus: cx.focus_handle(),
        }
    }
    fn attach_project(&self, cx: &mut Context<Self>) -> Button {
        quiet_button("attach-project", "Attach project", self.attach_hovered, cx)
            .child(div().text_xs().text_color(t::secondary()).child("Open…"))
            .on_click(cx.listener(|this, _, window, cx| {
                this.projects
                    .update(cx, |state, cx| state.open_folder(window, cx));
            }))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.attach_hovered = *hovered;
                cx.notify();
            }))
    }
    fn header(
        &self,
        index: usize,
        label: &'static str,
        now: Instant,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        use gpui_kit::component::{Sizable, button::ButtonVariants};
        let section = &self.sections[index];
        Button::new(label)
            .ghost()
            .small()
            .compact()
            .h(t::navigation_height())
            .w_full()
            .p_0()
            .accessibility_label(format!(
                "{} {label}",
                if section.open { "Collapse" } else { "Expand" }
            ))
            .child(
                row()
                    .w_full()
                    .gap_2()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t::secondary())
                    .child(section.chevron(now))
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus.focus(window, cx);
                this.sections[index].toggle(Instant::now(), cx);
                cx.notify();
            }))
    }
}
impl Sidebar {
    fn new_chat(&self, cx: &mut Context<Self>) -> Div {
        use gpui_kit::{
            base::Disableable,
            component::{
                Sizable,
                button::{ButtonCustomVariant, ButtonVariants},
                menu::{DropdownMenu, PopupMenuItem},
            },
        };
        let reason = crate::app_state::Chats::unavailable(cx);
        let opening = cx
            .global::<crate::app_state::AppState>()
            .chats
            .read(cx)
            .is_opening();
        let shortcut = if cfg!(target_os = "macos") {
            "⌘N"
        } else {
            "Ctrl+N"
        };
        let tooltip = reason.map(str::to_owned).unwrap_or_else(|| {
            self.projects.read(cx).catalog.current().map_or_else(
                || format!("New private chat ({shortcut})"),
                |project| {
                    format!(
                        "New chat in {} ({shortcut})",
                        project
                            .worktree_path
                            .as_ref()
                            .unwrap_or(&project.path)
                            .display()
                    )
                },
            )
        });
        let button = Button::new("sidebar-new-chat")
            .custom(
                ButtonCustomVariant::new(cx)
                    .foreground(t::text())
                    .hover(t::hover())
                    .active(t::selected()),
            )
            .small()
            .compact()
            .w_full()
            .h(t::navigation_height())
            .px_3()
            .rounded(t::control_radius(cx))
            .border_1()
            .border_color(t::control_border())
            .accessibility_label("New chat")
            .loading(opening)
            .disabled(reason.is_some())
            .child(
                row()
                    .w_full()
                    .gap_2()
                    .text_sm()
                    .child(Icon::new(IconName::Plus).size_3().text_color(t::accent()))
                    .child(if opening {
                        "Creating chat…"
                    } else {
                        "New chat"
                    }),
            )
            .tooltip(tooltip)
            .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::ui::NewChat), cx));
        let menu = Button::new("sidebar-new-chat-menu")
            .custom(
                ButtonCustomVariant::new(cx)
                    .foreground(t::secondary())
                    .hover(t::hover())
                    .active(t::selected()),
            )
            .small()
            .compact()
            .w(t::navigation_height())
            .h(t::navigation_height())
            .p_0()
            .rounded(t::control_radius(cx))
            .border_1()
            .border_color(t::control_border())
            .icon(IconName::ChevronDown)
            .accessibility_label("More chat options")
            .tooltip("More chat options")
            .disabled(reason.is_some() || opening)
            .dropdown_menu(|menu, _, _| {
                menu.item(
                    PopupMenuItem::new("New private chat").on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(crate::ui::NewChatNoProject), cx)
                    }),
                )
                .separator()
                .item(
                    PopupMenuItem::new("New chat in…").on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(crate::ui::NewChatIn), cx)
                    }),
                )
            });
        div().flex_shrink_0().px_3().pt_3().child(
            row()
                .w_full()
                .gap_1()
                .child(div().flex_1().min_w_0().child(button))
                .child(menu),
        )
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        canopy_desktop::motion::request_frame(
            window,
            self.sections.iter().any(|s| s.active(now))
                || self.project_groups.values().any(|s| s.active(now)),
        );
        let projects = self.projects_content(now, cx);
        use gpui_kit::component::{Sizable, button::ButtonVariants};
        let private = self.chats_content(now, cx);
        let count = self
            .projects
            .read(cx)
            .scratch_workspace(cx)
            .all_panes()
            .iter()
            .filter(|p| {
                p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Chat
                    || (p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
                        && canopy_desktop::chat::provider::Provider::from_tool(&p.tool).is_some())
            })
            .count();
        let private_height = ((count.max(1) as f32 * 3.5).min(14.) + 0.5).max(2.5);
        column()
            .size_full()
            .track_focus(&self.focus)
            .bg(t::sidebar())
            .border_r_1()
            .border_color(t::border())
            .child(self.new_chat(cx))
            .child(self.chat_feedback(cx))
            .child(
                row()
                    .px_3()
                    .pt_2()
                    .flex_shrink_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.header(0, "Projects", now, cx)),
                    )
                    .child(self.attach_project(cx)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        column()
                            .id("sidebar-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .px_3()
                            .pb_3()
                            .child(self.sections[0].measured_body("project-groups", projects, now)),
                    )
                    .vertical_scrollbar(&self.scroll),
            )
            .child(
                column()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(t::border())
                    .pt_1()
                    .child(
                        row()
                            .px_3()
                            .flex_shrink_0()
                            .child(div().flex_1().min_w_0().child(self.header(
                                1,
                                "Private chats",
                                now,
                                cx,
                            )))
                            .child(
                                Button::new("new-private-chat")
                                    .ghost()
                                    .small()
                                    .compact()
                                    .size_6()
                                    .p_0()
                                    .icon(IconName::Plus)
                                    .tooltip("New private chat")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(
                                            Box::new(crate::ui::NewChatNoProject),
                                            cx,
                                        )
                                    }),
                            ),
                    )
                    .child(
                        self.sections[1].measured_body(
                            "private-chat-region",
                            div()
                                .relative()
                                .h(rems(private_height))
                                .max_h(window.viewport_size().height * 0.32)
                                .child(
                                    column()
                                        .id("private-chat-scroll")
                                        .size_full()
                                        .overflow_y_scroll()
                                        .track_scroll(&self.private_scroll)
                                        .px_3()
                                        .pb_2()
                                        .child(private),
                                )
                                .vertical_scrollbar(&self.private_scroll),
                            now,
                        ),
                    ),
            )
            .child(
                row()
                    .h(t::navigation_height())
                    .flex_shrink_0()
                    .px_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(t::border())
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(t::muted())
                            .child("Canopy"),
                    )
                    .child(
                        Button::new("settings")
                            .ghost()
                            .small()
                            .compact()
                            .size_6()
                            .p_0()
                            .icon(IconName::Settings)
                            .tooltip("Settings")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(crate::ui::ShowPreferences), cx)
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::Sidebar;
    use crate::app_state::AppState;
    use canopy_desktop::git::{RepositoryInfo, WorktreeHead, WorktreeInfo};
    use canopy_desktop::state::workspace::{PaneKind, PaneMetadata, Workspace};
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, px, size};

    #[gpui_kit::test]
    fn cli_agent_selection_uses_its_existing_terminal_and_chat_returns_to_split_layout(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (app, chat, cli) = cx.update(|cx| {
            crate::ui::theme::init(cx);
            let mut model = Workspace::new();
            model.open_conversation("Design review", "claude");
            let chat = model.active().unwrap().focused;
            model.open("Codex — Default", "codex");
            let cli = model.active().unwrap().focused;
            model.activate_pane(chat).unwrap();
            let app = AppState::install_test(model, cx);
            app.projects.update(cx, |p, _| p.ready = true);
            (app, chat, cli)
        });
        let window = cx.open_window(size(px(300.), px(800.)), |window, cx| {
            Root::new(cx.new(Sidebar::new), window, cx)
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let id = gpui_kit::SharedString::from(format!("agent-terminal-{cli:?}"));
            assert!(
                window
                    .find(id.clone())
                    .label()
                    .unwrap()
                    .contains("CLI terminal")
            );
            window.click(id, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            assert!(app.workspace.read(cx).tool_panel().is_maximized());
            assert_eq!(app.workspace.read(cx).active().unwrap().focused, cli);
            window.render_frame(cx);
            window.click(gpui_kit::SharedString::from(format!("chat-{chat:?}")), cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(!app.workspace.read(cx).tool_panel().is_maximized());
            assert_eq!(app.workspace.read(cx).primary_tab().unwrap().focused, chat);
            assert_eq!(app.workspace.read(cx).all_panes().len(), 2);
            assert!(app.terminals.read(cx).views.is_empty());
        });
    }

    #[gpui_kit::test]
    fn worktrees_nest_conversations_and_private_chats_keep_their_context(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (app, pane, first, second, project_pane) = cx.update(|cx| {
            crate::ui::theme::init(cx);
            let mut workspace = Workspace::new();
            let mut ids = vec![];
            for title in ["First conversation", "Second conversation"] {
                let tab = workspace.open(title, "codex");
                let pane = workspace.active().unwrap().focused;
                workspace
                    .set_pane_metadata(
                        tab,
                        pane,
                        PaneMetadata {
                            kind: PaneKind::Chat,
                            title: Some(title.into()),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                ids.push(pane);
            }
            let app = AppState::install_test(workspace, cx);
            let root = std::path::PathBuf::from("/repos/canopy-desktop");
            let checkout = std::path::PathBuf::from("/worktrees/chats-sidebar");
            app.projects.update(cx, |p, cx| {
                p.ready = true;
                p.open_worktree(root.clone(), checkout.clone(), cx);
                p.open_chat(
                    p.catalog.active,
                    "Fix conversation navigation",
                    "claude",
                    PaneMetadata {
                        kind: PaneKind::Chat,
                        title: Some("Fix conversation navigation".into()),
                        cwd: Some(checkout.clone()),
                        ..Default::default()
                    },
                    cx,
                )
                .unwrap();
            });
            let project_pane = app.workspace.read(cx).active().unwrap().focused;
            app.agents.update(cx, |agents, cx| {
                agents.insert_test_session(
                    app.workspace.read(cx).all_panes()[0].clone(),
                    canopy_desktop::agents::Status::Working,
                )
            });
            app.git.update(cx, |git, _| {
                git.entries.insert(
                    checkout.clone(),
                    Ok(Some(std::sync::Arc::new(RepositoryInfo {
                        root: root.clone(),
                        common: root.join(".git"),
                        branches: vec![],
                        worktrees: [
                            (root, None, "main"),
                            (checkout, Some("chats-sidebar"), "feature/sidebar"),
                        ]
                        .into_iter()
                        .map(|(path, name, branch)| WorktreeInfo {
                            path,
                            name: name.map(str::to_owned),
                            head: WorktreeHead::Branch(branch.into()),
                            label: branch.into(),
                            locked: false,
                            available: true,
                            missing: false,
                            registered_branch: None,
                        })
                        .collect(),
                    }))),
                );
            });
            (
                app,
                ids[0],
                format!("chat-{:?}", ids[0]),
                format!("chat-{:?}", ids[1]),
                project_pane,
            )
        });
        let mut last_window = None;
        for (width, font) in [(220., 16.), (320., 16.), (220., 20.), (320., 20.)] {
            cx.update(|cx| {
                gpui_kit::component::Theme::global_mut(cx).font_size = px(font);
                gpui_kit::component::Theme::sync_base(cx);
            });
            let window = cx.open_window(size(px(width), px(900.)), |window, cx| {
                let sidebar = cx.new(Sidebar::new);
                Root::new(sidebar, window, cx)
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                let a = window
                    .find(gpui_kit::SharedString::from(first.clone()))
                    .bounds();
                let b = window
                    .find(gpui_kit::SharedString::from(second.clone()))
                    .bounds();
                assert_eq!(a.left(), b.left());
                assert_eq!(a.right(), b.right());
                assert!(window.try_find("no-project").is_none());
                let project = window.find(gpui_kit::SharedString::from(format!(
                    "chat-{project_pane:?}"
                )));
                let label = project.label().unwrap();
                for expected in [
                    "canopy-desktop",
                    "feature/sidebar",
                    "chats-sidebar",
                    "Claude Code",
                    "Working",
                ] {
                    assert!(label.contains(expected), "{label} must identify {expected}");
                }
                assert!(
                    !label.contains("main"),
                    "the linked checkout must not inherit the main branch"
                );
                let label = window
                    .find(gpui_kit::SharedString::from(first.clone()))
                    .label()
                    .unwrap()
                    .to_owned();
                for expected in ["Without a project", "Private folder", "Codex"] {
                    assert!(label.contains(expected));
                }
                // Private and project conversations use the same two-line density.
                let title = window
                    .find(gpui_kit::SharedString::from(format!("chat-title-{pane:?}")))
                    .bounds();
                let agent = window
                    .find(gpui_kit::SharedString::from(format!("chat-agent-{pane:?}")))
                    .bounds();
                let card = window
                    .find(gpui_kit::SharedString::from(format!("chat-{pane:?}")))
                    .bounds();
                assert_eq!(title.left(), agent.left());
                assert!(agent.top() - title.bottom() >= px(font / 4.));
                assert_eq!(title.top() - card.top(), card.bottom() - agent.bottom());
                assert!(
                    window
                        .try_find(gpui_kit::SharedString::from(format!(
                            "chat-project-{pane:?}"
                        )))
                        .is_none()
                );
                assert!(
                    window
                        .try_find(gpui_kit::SharedString::from(format!(
                            "chat-checkout-{pane:?}"
                        )))
                        .is_none()
                );
                let title = window
                    .find(gpui_kit::SharedString::from(format!(
                        "chat-title-{project_pane:?}"
                    )))
                    .bounds();
                let agent = window
                    .find(gpui_kit::SharedString::from(format!(
                        "chat-agent-{project_pane:?}"
                    )))
                    .bounds();
                assert!(agent.top() >= title.bottom());
                assert!(
                    window
                        .try_find(gpui_kit::SharedString::from(format!(
                            "chat-project-{project_pane:?}"
                        )))
                        .is_none()
                );
                let collapse =
                    gpui_kit::SharedString::from("expand-worktree-/worktrees/chats-sidebar");
                window.click(collapse.clone(), cx);
                window.render_frame(cx);
                assert!(
                    window
                        .try_find(gpui_kit::SharedString::from(format!(
                            "chat-{project_pane:?}"
                        )))
                        .is_none()
                );
                window.click(collapse, cx);
                window.render_frame(cx);
                window.click(gpui_kit::SharedString::from(first.clone()), cx);
                assert_eq!(app.workspace.read(cx).active().unwrap().focused, pane);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert!(
                    window
                        .find(gpui_kit::SharedString::from(first.clone()))
                        .label()
                        .unwrap()
                        .contains("selected")
                );
                let menu = format!("chat-menu-{pane:?}");
                window.click(gpui_kit::SharedString::from(menu), cx);
                window.press("escape", cx);
                assert_eq!(app.workspace.read(cx).active().unwrap().focused, pane);
            })
            .unwrap();
            last_window = Some(window);
        }
        cx.update(|cx| {
            app.git.update(cx, |git, cx| {
                let info = git
                    .entries
                    .values_mut()
                    .next()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .as_mut()
                    .unwrap();
                std::sync::Arc::make_mut(info).worktrees[1].head =
                    WorktreeHead::Detached("abcdef0123456789".into());
                cx.notify();
            });
        });
        cx.run_until_parked();
        let window = last_window.unwrap();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let label = window
                .find(gpui_kit::SharedString::from(format!(
                    "chat-{project_pane:?}"
                )))
                .label()
                .unwrap()
                .to_owned();
            assert!(label.contains("Detached HEAD · abcdef01"));
            assert!(!label.contains("feature/sidebar"));
        })
        .unwrap();
    }
}
