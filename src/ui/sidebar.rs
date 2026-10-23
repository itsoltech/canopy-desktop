mod chats;
mod projects;
pub use chats::RenameChat;
mod tasks;
mod tools;
use super::components::file_tree::FileTree;
use super::components::files::file_message;
use super::{components::*, theme as t};
use gpui_kit::component::{Icon, IconName, button::Button, scroll::ScrollableElement};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Instant;

pub struct NewFile;
impl EventEmitter<NewFile> for Sidebar {}
pub struct Sidebar {
    files: Entity<FileTree>,
    tasks: Entity<tasks::TasksSection>,
    scroll: ScrollHandle,
    git: Entity<crate::app_state::GitState>,
    tools: Entity<crate::app_state::ToolsState>,
    terminals: Entity<crate::app_state::Terminals>,
    profiles: std::collections::HashMap<String, Disclosure>,
    project_groups: std::collections::HashMap<std::path::PathBuf, Disclosure>,
    hovered_project_item: Option<String>,
    stop_feedback:
        std::collections::HashMap<canopy_desktop::state::workspace::WorkspaceId, ButtonLoading>,
    _tool_events: Vec<Subscription>,
    projects: Entity<crate::app_state::ProjectsState>,
    _projects_observer: Subscription,
    sections: [Disclosure; 4],
    actions_hovered: [bool; 3],
    hovered_tool_label: Option<String>,
    focus: FocusHandle,
}
impl Sidebar {
    pub fn new(files: Entity<FileTree>, cx: &mut Context<Self>) -> Self {
        let scroll = ScrollHandle::new();
        files.update(cx, |files, _| files.use_sidebar_scroll(scroll.clone()));
        let projects = cx.global::<crate::app_state::AppState>().projects.clone();
        let observer = cx.observe(&projects, |this, _, cx| this.sync_project_groups(cx));
        let app = cx.global::<crate::app_state::AppState>().clone();
        let tools = app.tools.clone();
        let terminals = app.terminals.clone();
        let tool_events = vec![
            cx.observe(&app.files, |_, _, cx| cx.notify()),
            cx.observe(&app.git, |_, _, cx| cx.notify()),
            cx.observe(&tools, |this, _, cx| this.sync_tools(cx)),
            cx.observe(&terminals, |this, _, cx| {
                this.sync_stop_feedback(cx);
                cx.notify();
            }),
            cx.observe(&app.chats, |_, _, cx| cx.notify()),
            cx.observe(&app.workspace, |_, _, cx| cx.notify()),
            cx.observe(&app.agents, |_, _, cx| cx.notify()),
            cx.observe(&app.chat_history, |_, _, cx| cx.notify()),
            cx.observe(&app.settings, |_, _, cx| cx.notify()),
        ];
        let profiles = tools
            .read(cx)
            .catalog
            .tools
            .iter()
            .map(|t| (t.id.clone(), Disclosure::new(false, Instant::now())))
            .collect();
        let now = Instant::now();
        Self {
            git: app.git.clone(),
            tools,
            terminals,
            project_groups: projects
                .read(cx)
                .catalog
                .items
                .iter()
                .map(|p| {
                    (
                        p.repository_path.as_ref().unwrap_or(&p.path).clone(),
                        Disclosure::new(true, now),
                    )
                })
                .collect(),
            hovered_project_item: None,
            stop_feedback: Default::default(),
            profiles,
            _tool_events: tool_events,
            files,
            tasks: cx.new(tasks::TasksSection::new),
            scroll,
            projects,
            _projects_observer: observer,
            sections: std::array::from_fn(|_| Disclosure::new(true, now)),
            actions_hovered: [false; 3],
            hovered_tool_label: None,
            focus: cx.focus_handle(),
        }
    }
    fn action(&self, index: usize, cx: &mut Context<Self>) -> Button {
        let (id, label) = [
            ("attach-project", "Attach project"),
            ("new-file", "New file"),
            ("refresh-files", "Refresh files"),
        ][index];
        let button = quiet_button(id, label, self.actions_hovered[index], cx);
        let button = if index == 0 {
            button.child(div().text_size(px(10.)).child("+ attach"))
        } else {
            button.w(px(20.)).icon(Icon::new(if index == 1 {
                IconName::Plus
            } else {
                IconName::RotateCw
            }))
        };
        let button = button.on_click(cx.listener(move |this, _, window, cx| {
            if index == 1 {
                cx.emit(NewFile);
            }
            if index == 2 {
                cx.global::<crate::app_state::AppState>()
                    .files
                    .clone()
                    .update(cx, |files, cx| files.refresh(cx));
            }
            if index == 0 {
                this.projects
                    .update(cx, |state, cx| state.open_folder(window, cx));
            }
        }));
        button.on_hover(cx.listener(move |this, hovered, _, cx| {
            if this.actions_hovered[index] != *hovered {
                this.actions_hovered[index] = *hovered;
                cx.notify();
            }
        }))
    }
    fn header(
        &self,
        index: usize,
        label: &'static str,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.sections[index]
            .header(label, label, now, cx)
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if this.sections[index].hovered != *hovered {
                    this.sections[index].hovered = *hovered;
                    cx.notify();
                }
            }))
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
            .h_8()
            .px_2()
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
                    .child(
                        Icon::new(IconName::Plus)
                            .size_3()
                            .text_color(t::secondary()),
                    )
                    .child(if opening {
                        "Creating chat…"
                    } else {
                        "New chat"
                    }),
            )
            .tooltip(reason.unwrap_or(if cfg!(target_os = "windows") {
                "New chat (Ctrl+Shift+N)"
            } else {
                "New chat (⌘N)"
            }))
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
            .size_8()
            .p_0()
            .rounded(t::control_radius(cx))
            .border_1()
            .border_color(t::control_border())
            .icon(IconName::ChevronDown)
            .accessibility_label("Choose chat context")
            .tooltip("New chat in…")
            .disabled(reason.is_some() || opening)
            .dropdown_menu(|menu, _, _| {
                menu.item(
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
        let has_project = self.projects.read(cx).catalog.active.is_some();
        let tree_height = self.files.read(cx).animated_height(now);
        canopy_desktop::motion::request_frame(
            window,
            self.sections.iter().any(|s| s.active(now))
                || self.project_groups.values().any(|s| s.active(now))
                || self.profiles.values().any(|d| d.active(now))
                || (self.sections[1].open && self.files.read(cx).is_animating(now)),
        );
        let (projects, projects_height) = self.projects_content(now, cx);
        let (tools, tools_height) = self.tools_content(now, cx);
        column()
            .size_full()
            .track_focus(&self.focus)
            .bg(t::sidebar())
            .border_r_1()
            .border_color(t::border())
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
                            .child(self.new_chat(cx))
                            .child(self.chats_content(now, cx))
                            .child(
                                column()
                                    .flex_shrink_0()
                                    .px(px(12.))
                                    .pt(px(8.))
                                    .pb(px(14.))
                                    .child(
                                        row()
                                            .h(px(36.))
                                            .flex_shrink_0()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(self.header(0, "PROJECTS", now, cx)),
                                            )
                                            .child(self.action(0, cx)),
                                    )
                                    .child(self.sections[0].body(projects_height, projects, now)),
                            )
                            .child(self.tasks.clone())
                            .when(has_project, |panel| {
                                panel
                                    .child(
                                        column()
                                            .flex_shrink_0()
                                            .border_t_1()
                                            .border_color(t::border())
                                            .px(px(12.))
                                            .py(px(10.))
                                            .child(
                                                row()
                                                    .h(px(36.))
                                                    .flex_shrink_0()
                                                    .child(
                                                        div().flex_1().child(
                                                            self.header(1, "FILES", now, cx),
                                                        ),
                                                    )
                                                    .child(
                                                        row()
                                                            .gap(px(t::SPACING_UNIT))
                                                            .child(self.action(1, cx))
                                                            .child(self.action(2, cx)),
                                                    ),
                                            )
                                            .child(self.sections[1].body(
                                                tree_height,
                                                self.files.clone(),
                                                now,
                                            ))
                                            .children(
                                                cx.global::<crate::app_state::AppState>()
                                                    .files
                                                    .read(cx)
                                                    .error
                                                    .clone()
                                                    .or(cx
                                                        .global::<crate::app_state::AppState>()
                                                        .files
                                                        .read(cx)
                                                        .index
                                                        .warning
                                                        .clone())
                                                    .map(|error| {
                                                        file_message(error, t::red())
                                                            .px(px(12.))
                                                            .text_size(px(11.))
                                                    }),
                                            )
                                            .children(
                                                (cx.global::<crate::app_state::AppState>()
                                                    .files
                                                    .read(cx)
                                                    .loading)
                                                    .then(|| {
                                                        file_message("Loading files…", t::muted())
                                                            .px(px(12.))
                                                    }),
                                            ),
                                    )
                                    .child(
                                        column()
                                            .flex_shrink_0()
                                            .border_t_1()
                                            .border_color(t::border())
                                            .px(px(12.))
                                            .py(px(10.))
                                            .child(self.header(2, "TOOLS", now, cx))
                                            .child(self.sections[2].body(tools_height, tools, now)),
                                    )
                            }),
                    )
                    .vertical_scrollbar(&self.scroll),
            )
            .child(
                row()
                    .h(px(28.))
                    .flex_shrink_0()
                    .px(px(12.))
                    .border_t_1()
                    .border_color(t::border())
                    .text_size(px(10.))
                    .text_color(t::faint())
                    .child("V0.13.0-NEXT.28"),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::Sidebar;
    use crate::app_state::AppState;
    use crate::ui::components::file_tree::{FileTree, FileTreeModel};
    use canopy_desktop::git::{RepositoryInfo, WorktreeHead, WorktreeInfo};
    use canopy_desktop::state::workspace::{PaneKind, PaneMetadata, Workspace};
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, px, size};

    #[gpui_kit::test]
    fn chat_rows_show_checkout_and_agent_and_navigate_with_keyboard_menus(cx: &mut TestAppContext) {
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
                let files = cx.new(|cx| FileTree::new(FileTreeModel::new(vec![], []).unwrap(), cx));
                let sidebar = cx.new(|cx| Sidebar::new(files, cx));
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
                // Inspect the actual metadata lanes at both sidebar widths and font sizes.
                for id in [pane, project_pane] {
                    let text = |part| {
                        window.find(gpui_kit::SharedString::from(format!("chat-{part}-{id:?}")))
                    };
                    let project = text("project").bounds();
                    let title = text("title").bounds();
                    let branch = text("checkout").bounds();
                    let agent = text("agent").bounds();
                    assert_eq!(project.left(), title.left());
                    assert!(project.bottom() <= title.top());
                    assert!(title.bottom() <= branch.top());
                    assert_eq!(branch.top(), agent.top());
                    assert!(branch.size.width > px(0.));
                    assert!(branch.right() < agent.left());
                    assert!(agent.right() <= title.right());
                }
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
