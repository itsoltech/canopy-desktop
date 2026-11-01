//! The conversation and the worktree's independent tool area share existing pane trees.
use super::*;
use canopy_desktop::state::workspace::{ToolPage, ToolPosition};
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder;

const TOOL_PAGES: [(ToolPage, &str); 4] = [
    (ToolPage::Files, "Files"),
    (ToolPage::Changes, "Changes"),
    (ToolPage::Tasks, "Tasks"),
    (ToolPage::Agent, "Agent"),
];

fn page_label(page: ToolPage) -> &'static str {
    TOOL_PAGES
        .iter()
        .find(|(p, _)| *p == page)
        .map(|(_, label)| *label)
        .unwrap_or("Tools")
}
fn tool_command(page: ToolPage) -> super::keymap::CommandId {
    use super::keymap::CommandId;
    match page {
        ToolPage::Tabs => CommandId::ShowTabs,
        ToolPage::Files => CommandId::ShowFiles,
        ToolPage::Changes => CommandId::ShowChanges,
        ToolPage::Tasks => CommandId::ShowTasks,
        ToolPage::Agent => CommandId::ShowAgent,
    }
}

#[derive(Clone, Copy)]
pub(super) struct ToolResize {
    pub origin: Point<Pixels>,
    pub width: f32,
    pub height: f32,
    pub bottom: bool,
}

impl Workspace {
    pub(super) fn open_new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || self.state.files.read(cx).root.is_none()
        {
            return;
        }
        let files = self.state.files.clone();
        let root = files.read(cx).root.clone();
        let modal = cx.new(|cx| {
            TextPrompt::new(
                "New file",
                "Path relative to worktree",
                "",
                self.focus.clone(),
                move |name, cx| {
                    if name.trim().is_empty() {
                        return Err("Enter a file name.".into());
                    }
                    if files.read(cx).root != root {
                        return Err("The worktree changed.".into());
                    }
                    files.update(cx, |files, cx| {
                        files.create_file(std::path::PathBuf::from(name), cx)
                    });
                    Ok(())
                },
                window,
                cx,
            )
        });
        self.modal_events = Some(cx.subscribe(&modal, |this, _, _: &ModalDismissed, cx| {
            this.modal = None;
            this.modal_events = None;
            cx.notify();
        }));
        self.modal = Some(modal);
        cx.notify();
    }
    pub(super) fn new_terminal(&mut self, cx: &mut Context<Self>) {
        self.state
            .tools
            .update(cx, |tools, cx| tools.launch("shell", None, cx));
    }
    pub(super) fn split_terminal(&mut self, axis: Axis, cx: &mut Context<Self>) {
        if self.state.git.read(cx).busy || self.state.projects.read(cx).busy {
            return;
        }
        let result = self.state.workspace.update(cx, |w, cx| {
            let target = w.active().and_then(|t| {
                t.root
                    .find(t.focused)
                    .filter(|p| {
                        matches!(
                            p.metadata.kind,
                            canopy_desktop::state::workspace::PaneKind::Terminal
                                | canopy_desktop::state::workspace::PaneKind::Chat
                        )
                    })
                    .map(|p| (t.id, p.id, p.tool.clone(), p.metadata.clone()))
            });
            if let Some((tab, pane, tool, mut metadata)) = target {
                if metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal {
                    metadata.resume_id = None;
                    metadata.task_prompt = None;
                }
                let id = w.split(tab, pane, axis, tool)?;
                w.set_pane_metadata(tab, id, metadata)?;
                w.activate(tab)?;
                cx.notify();
            }
            Ok::<_, canopy_desktop::state::workspace::StateError>(())
        });
        if let Err(error) = result {
            self.state.tools.update(cx, |tools, cx| {
                tools.error = Some(match error {
                    canopy_desktop::state::workspace::StateError::DepthLimit => {
                        "This pane has reached the split limit.".into()
                    }
                    _ => "The pane changed before it could be split. Try again.".into(),
                });
                cx.notify();
            });
        }
    }
    pub(super) fn select_tool_page(&mut self, page: ToolPage, cx: &mut Context<Self>) {
        self.state.workspace.update(cx, |w, cx| {
            w.show_tools(page);
            cx.notify();
        });
    }
    fn tool_menu(&self, cx: &Context<Self>) -> impl IntoElement {
        let model = self.state.workspace.read(cx);
        let page = model.tool_panel().page();
        let open = model.tools_open();
        let maximize = !model.tool_panel().is_maximized();
        let has_project = self.state.projects.read(cx).catalog.active.is_some()
            || self.focused_remote(cx).is_some();
        let terminal = model.active().is_some_and(|t| {
            t.root.find(t.focused).is_some_and(|p| {
                p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
            })
        });
        let owner = cx.entity().downgrade();
        let tools = self.state.tools.read(cx).catalog.clone();
        Button::new("tool-panel-menu")
            .ghost()
            .small()
            .compact()
            .size(px(24.))
            .icon(IconName::Ellipsis)
            .accessibility_label("Tools and layout")
            .tooltip("Tools and layout")
            .dropdown_menu(move |menu, _, cx| {
                let mut menu = menu;
                for (target_page, label) in TOOL_PAGES {
                    let target = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(open && target_page == page)
                            .disabled(
                                !has_project
                                    && matches!(target_page, ToolPage::Files | ToolPage::Changes),
                            )
                            .on_click(move |_, _, cx| {
                                let _ =
                                    target.update(cx, |s, cx| s.select_tool_page(target_page, cx));
                            }),
                    );
                }
                let target = owner.clone();
                menu = menu.separator().item(
                    PopupMenuItem::new("New terminal")
                        .disabled(!has_project)
                        .on_click(move |_, _, cx| {
                            let _ = target.update(cx, |s, cx| s.new_terminal(cx));
                        }),
                );
                for (label, axis) in [
                    ("Split terminal right", Axis::Horizontal),
                    ("Split terminal below", Axis::Vertical),
                ] {
                    let target = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).disabled(!terminal).on_click(
                        move |_, _, cx| {
                            let _ = target.update(cx, |s, cx| s.split_terminal(axis, cx));
                        },
                    ));
                }
                menu = menu.separator();
                // ACP agents open as chats, never as terminals.
                for tool in tools.tools.iter().filter(|t| t.enabled && !t.acp) {
                    let profiles: Vec<_> = if tool.profiles.is_empty() {
                        vec![None]
                    } else {
                        tool.profiles.iter().map(|p| Some(p.id.clone())).collect()
                    };
                    for profile in profiles {
                        let id = tool.id.clone();
                        let running = cx.global::<AppState>().terminals.read(cx).running_count(
                            &tool.id,
                            profile.as_deref(),
                            cx,
                        );
                        let label = if running > 0 {
                            format!("{} · {running} running", tool.label(profile.as_deref()))
                        } else {
                            tool.label(profile.as_deref())
                        };
                        menu =
                            menu.item(PopupMenuItem::new(label).disabled(!has_project).on_click(
                                move |_, _, cx| {
                                    cx.global::<AppState>()
                                        .tools
                                        .clone()
                                        .update(cx, |s, cx| s.launch(&id, profile.as_deref(), cx));
                                },
                            ));
                    }
                }
                menu = menu.separator().item(
                    PopupMenuItem::new(if maximize {
                        "Maximize tools"
                    } else {
                        "Restore split layout"
                    })
                    .disabled(!open)
                    .on_click(move |_, _, cx| {
                        cx.global::<AppState>()
                            .workspace
                            .clone()
                            .update(cx, |w, cx| {
                                w.tool_panel_mut().set_maximized(maximize);
                                cx.notify();
                            });
                    }),
                );
                for (label, position) in [
                    ("Dock on right", ToolPosition::Right),
                    ("Dock at bottom", ToolPosition::Bottom),
                ] {
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        cx.global::<AppState>()
                            .workspace
                            .clone()
                            .update(cx, |w, cx| {
                                w.tool_panel_mut().set_position(position);
                                cx.notify();
                            });
                    }));
                }
                menu.separator()
                    .item(
                        PopupMenuItem::new("Hide tools")
                            .disabled(!open)
                            .on_click(|_, _, cx| {
                                cx.global::<AppState>()
                                    .workspace
                                    .clone()
                                    .update(cx, |w, cx| {
                                        w.hide_tools();
                                        cx.notify();
                                    });
                            }),
                    )
                    .item(PopupMenuItem::new("Settings…").on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(ShowPreferences), cx);
                    }))
            })
    }

    /// The count a tab shows: changed files of the worktree in Changes.
    fn tool_count(&self, page: ToolPage, cx: &Context<Self>) -> Option<usize> {
        match page {
            ToolPage::Changes => self
                .state
                .changes
                .read(cx)
                .data
                .as_ref()
                .map(|data| data.files.len())
                .filter(|count| *count > 0),
            _ => None,
        }
    }

    /// The most urgent state among the agents that are running or unread.
    fn agent_signal(&self, cx: &Context<Self>) -> Option<Hsla> {
        use canopy_desktop::agents::Status;
        let agents = self.state.agents.read(cx);
        let statuses: Vec<_> = agents
            .sessions
            .values()
            .filter(|session| session.active || session.unseen)
            .map(|session| session.status)
            .collect();
        [
            (Status::Waiting, t::yellow()),
            (Status::Failed, t::red()),
            (Status::Working, t::accent()),
        ]
        .into_iter()
        .find(|(status, _)| statuses.contains(status))
        .map(|(_, color)| color)
    }

    pub(super) fn tool_navigation(
        &self,
        width: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let model = self.state.workspace.read(cx);
        let selected = model.tool_panel().page();
        let open = model.tools_open();
        // A narrow panel keeps only the selected page; the menu still switches pages.
        let compact = width < f32::from(window.rem_size()) * 20.;
        let no_project = self.state.projects.read(cx).catalog.active.is_none()
            && self.focused_remote(cx).is_none();
        let agent = self.agent_signal(cx);
        row()
            .id("tool-navigation")
            .h(t::tabs_height())
            .flex_shrink_0()
            .px(px(4.))
            .gap(px(2.))
            .border_b_1()
            .border_color(t::border())
            .children(
                TOOL_PAGES
                    .into_iter()
                    .filter(|(page, _)| !compact || (open && *page == selected))
                    .map(|(page, label)| {
                        let active = open && selected == page;
                        let disabled =
                            no_project && matches!(page, ToolPage::Files | ToolPage::Changes);
                        let tooltip =
                            super::keymap::with_shortcut(page_label(page), tool_command(page), cx);
                        let tab =
                            row()
                                .id(SharedString::from(format!("tool-page-{page:?}")))
                                .relative()
                                .h_full()
                                .px(px(8.))
                                .gap(px(5.))
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if active { t::text() } else { t::faint() })
                                .role(Role::Tab)
                                .aria_label(label)
                                .aria_selected(active)
                                .when(disabled, |tab| tab.opacity(0.5))
                                .when(!disabled, |tab| {
                                    tab.cursor_pointer()
                                        .tab_index(0)
                                        .when(!active, |tab| {
                                            tab.hover(|style| style.text_color(t::secondary()))
                                        })
                                        .focus_visible(|style| style.bg(t::hover()))
                                        .on_click(cx.listener(move |s, _, _, cx| {
                                            s.select_tool_page(page, cx)
                                        }))
                                })
                                .tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                        .build(window, cx)
                                })
                                .child(label)
                                .children(
                                    self.tool_count(page, cx)
                                        .map(|count| count_badge(count, active)),
                                )
                                .children(
                                    agent
                                        .filter(|_| page == ToolPage::Agent)
                                        .map(|color| dot().size(px(6.)).bg(color)),
                                )
                                .children(active.then(|| {
                                    div()
                                        .absolute()
                                        .left(px(8.))
                                        .right(px(8.))
                                        .bottom_0()
                                        .h(px(2.))
                                        .rounded(px(1.))
                                        .bg(t::accent())
                                }));
                        #[cfg(test)]
                        let tab = {
                            use gpui_kit::TestSupportExt;
                            tab.test_support()
                        };
                        tab.into_any_element()
                    }),
            )
            .child(div().flex_1())
            .child(self.tool_menu(cx))
    }

    fn tab_toolbar(&mut self, width: f32, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let model = self.state.workspace.read(cx);
        let terminal = model.active().is_some_and(|t| {
            t.root.find(t.focused).is_some_and(|p| {
                p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
            })
        });
        let tabs: Vec<_> = model
            .tabs()
            .iter()
            .map(|t| (t.id, t.title.clone()))
            .collect();
        let active = model.active().map(|t| t.id);
        let owner = cx.entity().downgrade();
        let all_tabs = Button::new("all-tabs")
            .ghost()
            .small()
            .compact()
            .size_7()
            .p_0()
            .icon(IconName::ChevronDown)
            .tooltip(super::keymap::with_shortcut(
                "All tabs",
                super::keymap::CommandId::ShowTabs,
                cx,
            ))
            .disabled(tabs.is_empty())
            .dropdown_menu(move |menu, _, _| {
                tabs.iter().fold(menu, |menu, (id, title)| {
                    let id = *id;
                    let owner = owner.clone();
                    menu.item(
                        PopupMenuItem::new(title.clone())
                            .checked(active == Some(id))
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |s, cx| {
                                    s.state.workspace.update(cx, |w, cx| {
                                        let _ = w.activate(id);
                                        cx.notify();
                                    })
                                });
                            }),
                    )
                })
            });
        let reserved = f32::from(window.rem_size()) * 5.75;
        let tab_bar = self.render_tabs((width - reserved).max(0.), window, cx);
        row()
            .h(t::tabs_height())
            .flex_shrink_0()
            .bg(components::background::window_fill(t::panel(), cx))
            .border_b_1()
            .border_color(t::border())
            .gap_1()
            .pr_1()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(tab_bar),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap_1()
                    .child(all_tabs)
                    .child(
                        Button::new("new-terminal")
                            .ghost()
                            .small()
                            .compact()
                            .size_7()
                            .p_0()
                            .icon(IconName::Plus)
                            .tooltip("New terminal")
                            .disabled(self.state.projects.read(cx).catalog.active.is_none())
                            .on_click(cx.listener(|s, _, _, cx| s.new_terminal(cx))),
                    )
                    .child(
                        Button::new("split-terminal")
                            .ghost()
                            .small()
                            .compact()
                            .size_7()
                            .p_0()
                            .icon(IconName::PanelRight)
                            .tooltip("Split terminal right")
                            .disabled(!terminal)
                            .on_click(
                                cx.listener(|s, _, _, cx| s.split_terminal(Axis::Horizontal, cx)),
                            ),
                    ),
            )
    }

    pub(super) fn render_work_area(
        &mut self,
        x: f32,
        width: f32,
        height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let model = self.state.workspace.read(cx).clone();
        let panel = model.tool_panel();
        let requested_open = model.tools_open();
        let rem = f32::from(window.rem_size());
        let bottom = panel.position() == ToolPosition::Bottom;
        let fits_split = if bottom {
            height >= rem * 32.
        } else {
            width >= rem * 52.
        };
        let single = panel.is_maximized() || model.primary_tab().is_none() || !fits_split;
        let open = requested_open
            && (!single
                || panel.is_focused()
                || panel.is_maximized()
                || model.primary_tab().is_none());
        let maximized = single && open;
        let tool_width = if maximized || bottom {
            width
        } else {
            panel
                .width()
                .max(24. * rem)
                .min((width - 28. * rem).max(0.))
        };
        let tool_height = if maximized || !bottom {
            height
        } else {
            panel
                .height()
                .max(12. * rem)
                .min((height - 20. * rem).max(0.))
        };
        let main_width = if open && !bottom {
            width - tool_width
        } else {
            width
        };
        let main_height = if open && bottom {
            height - tool_height
        } else {
            height
        };
        let mut area = if bottom {
            column()
        } else {
            row().items_stretch()
        };
        if !maximized {
            let error = self.state.tools.read(cx).error.clone();
            // Tab bar, then a tool error (launch, split) above the panes.
            let tabs_height = rem * 2.25 * if error.is_some() { 2. } else { 1. };
            let toolbar = self.tab_toolbar(main_width, window, cx);
            let main = if let Some(tab) = model.primary_tab() {
                self.render_tree(
                    &tab.root,
                    tab.id,
                    tab.focused,
                    Bounds::new(
                        point(px(x), px(t::TITLEBAR + tabs_height)),
                        size(px(main_width), px((main_height - tabs_height).max(0.))),
                    ),
                    cx,
                )
            } else {
                // No tab: an overview of the selected worktree (`worktree_overview.rs`).
                self.worktree_overview(cx)
            };
            area = area.child(
                column()
                    .w(px(main_width))
                    .h(px(main_height))
                    .min_w_0()
                    .min_h_0()
                    .child(toolbar)
                    .children(error.map(|error| {
                        row()
                            .h(t::tabs_height())
                            .flex_shrink_0()
                            .px_3()
                            .gap_2()
                            .text_xs()
                            .text_color(t::red())
                            .child(div().flex_1().min_w_0().truncate().child(error.clone()))
                            .child(
                                Button::new("dismiss-tool-error")
                                    .ghost()
                                    .small()
                                    .compact()
                                    .size_5()
                                    .p_0()
                                    .icon(IconName::Close)
                                    .tooltip(error)
                                    .accessibility_label("Dismiss tool error")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.tools.update(cx, |tools, cx| {
                                            tools.error = None;
                                            cx.notify();
                                        });
                                    })),
                            )
                    }))
                    .child(div().flex_1().min_h_0().child(main)),
            );
        }
        if open {
            let return_bar = maximized && model.primary_tab().is_some();
            let content = if self.focused_remote(cx).is_some() {
                self.remote_tools
                    .as_ref()
                    .map(|(_, _, view)| view.clone().into_any_element())
                    .unwrap_or_else(|| {
                        panel_loading("Connecting to the workspace…").into_any_element()
                    })
            } else {
                self.inspector.clone().into_any_element()
            };
            let resize = ToolResize {
                origin: point(px(x), px(t::TITLEBAR)),
                width,
                height,
                bottom,
            };
            // The page tabs belong to the panel, so hiding it hides them too.
            let navigation = self.tool_navigation(tool_width, window, cx);
            let mut tools = column()
                .relative()
                .w(px(tool_width))
                .h(px(tool_height))
                .flex_shrink_0()
                .border_color(t::border())
                .when(bottom, |d| d.border_t_1())
                .when(!bottom, |d| d.border_l_1())
                .bg(t::panel())
                .children(return_bar.then(|| {
                    let title = model
                        .primary_tab()
                        .map(|t| t.title.clone())
                        .unwrap_or_else(|| "Workspace".into());
                    row()
                        .h(t::navigation_height())
                        .flex_shrink_0()
                        .px_2()
                        .border_b_1()
                        .border_color(t::border())
                        .child(
                            Button::new("return-to-work")
                                .ghost()
                                .small()
                                .compact()
                                .h_7()
                                .icon(IconName::ArrowLeft)
                                .label(format!("Back to {title}"))
                                .max_w(relative(1.))
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.state.workspace.update(cx, |w, cx| {
                                        w.return_to_conversation();
                                        cx.notify();
                                    })
                                })),
                        )
                }))
                .child(navigation)
                .child(div().flex_1().min_h_0().child(content));
            if !maximized {
                tools = tools.child(
                    div()
                        .id("resize-tools")
                        .absolute()
                        .when(bottom, |d| {
                            d.top_0().left_0().right_0().h_1().cursor_row_resize()
                        })
                        .when(!bottom, |d| {
                            d.left_0().top_0().bottom_0().w_1().cursor_col_resize()
                        })
                        .on_drag(resize, |_, _, _, cx| {
                            cx.new(|_| DragPreview("Resize tools"))
                        }),
                );
            }
            area = area.child(tools);
        }
        area.size_full()
    }
}

#[cfg(test)]
mod tests {
    use super::super::Workspace;
    use crate::app_state::AppState;
    use canopy_desktop::state::workspace::{ToolPage, ToolPosition, Workspace as Model};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, component::Root, px, size};

    fn choose_menu(label: &str, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
        window.render_frame(cx);
        assert!(
            window.try_find("popup-menu").is_some(),
            "Menu did not open for {label}; button {:?}; viewport {:?}",
            window.find("tool-panel-menu").bounds(),
            window.viewport_size()
        );
        let mut menu = window.within("popup-menu");
        let index = (0..64usize)
            .find(|i| {
                menu.try_find(*i)
                    .is_some_and(|item| item.label() == Some(label))
            })
            .expect("tool menu action");
        menu.click(index, cx);
    }

    #[gpui_kit::test]
    fn work_area_tabs_and_tool_panel_stay_independent(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (app, chat, terminal) = cx.update(|cx| {
            crate::ui::theme::init(cx);
            let app = AppState::install_test(Model::new(), cx);
            app.projects.update(cx, |p, cx| {
                p.ready = true;
                p.install_test_worktree("/workspace/repo".into(), "/workspace/repo".into(), cx);
            });
            app.tools.update(cx, |tools, _| tools.ready = true);
            let (chat, terminal) = app.workspace.update(cx, |w, _| {
                let chat = w.open_conversation("Review navigation", "codex");
                let terminal = w.open("Terminal 1", "shell");
                (chat, terminal)
            });
            (app, chat, terminal)
        });
        let window = cx.open_window(size(px(1100.), px(740.)), |window, cx| {
            let view = cx.new(|cx| Workspace::new(window, cx));
            Root::new(view, window, cx)
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, terminal);
            window.click("new-terminal", cx);
            let second = app.workspace.read(cx).primary_tab().unwrap();
            assert_ne!(second.id, terminal);
            assert_eq!(second.title, "Terminal 2");
            window.render_frame(cx);
            window.click("split-terminal", cx);
            assert_eq!(
                app.workspace
                    .read(cx)
                    .primary_tab()
                    .unwrap()
                    .root
                    .panes()
                    .len(),
                2
            );
            app.workspace.update(cx, |w, cx| {
                w.activate(chat).unwrap();
                cx.notify();
            });
            window.render_frame(cx);
            window.click("toggle-inspector", cx);
            window.render_frame(cx);
            window.click("tool-page-Agent", cx);
            assert_eq!(app.workspace.read(cx).tool_panel().page(), ToolPage::Agent);
            assert!(app.workspace.read(cx).tools_open());
            assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, chat);
            window.render_frame(cx);
            window.click("tool-panel-menu", cx);
            choose_menu("Hide tools", window, cx);
            assert!(!app.workspace.read(cx).tools_open());
            assert_eq!(app.workspace.read(cx).tabs().len(), 3);
            assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, chat);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            // Hidden tools take their page tabs with them.
            assert!(window.try_find("tool-page-Agent").is_none());
            window.click("toggle-inspector", cx);
            assert!(app.workspace.read(cx).tools_open());
            window.render_frame(cx);
            window.click("tool-panel-menu", cx);
            choose_menu("Maximize tools", window, cx);
            assert!(app.workspace.read(cx).tool_panel().is_maximized());
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("return-to-work", cx);
            assert!(!app.workspace.read(cx).tool_panel().is_maximized());
            assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, chat);
            window.render_frame(cx);
            window.click("tool-panel-menu", cx);
            choose_menu("Dock at bottom", window, cx);
            assert_eq!(
                app.workspace.read(cx).tool_panel().position(),
                ToolPosition::Bottom
            );
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("tool-panel-menu", cx);
            choose_menu("New terminal", window, cx);
            assert_eq!(app.workspace.read(cx).tabs().len(), 4);
            assert_eq!(
                app.workspace.read(cx).primary_tab().unwrap().title,
                "Terminal 3"
            );
        })
        .unwrap();
    }
    #[gpui_kit::test]
    fn responsive_tools_keep_actions_and_existing_sessions_accessible(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for (width, font) in [(1280., 16.), (1280., 18.), (1280., 20.), (800., 20.)] {
            let (app, pane, ids) = cx.update(|cx| {
                crate::ui::theme::init(cx);
                gpui_kit::component::Theme::global_mut(cx).font_size = px(font);
                gpui_kit::component::Theme::sync_base(cx);
                let mut model = Model::new();
                let chat = model.open_conversation("Conversation A", "claude");
                let pane = model.active().unwrap().focused;
                for n in 1..8 {
                    model.open(format!("Terminal {n}"), "shell");
                }
                model.show_tools(ToolPage::Agent);
                // The tabs keep the work area where the split doesn't fit.
                model.activate(chat).unwrap();
                let ids = model.all_panes().iter().map(|p| p.id).collect::<Vec<_>>();
                let app = AppState::install_test(model, cx);
                app.projects.update(cx, |p, _| p.ready = true);
                (app, pane, ids)
            });
            let window = cx.open_window(size(px(width), px(800.)), |window, cx| {
                Root::new(cx.new(|cx| Workspace::new(window, cx)), window, cx)
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                // The tools menu lives in the panel, which a narrow window hides.
                let tools = window
                    .try_find("tool-panel-menu")
                    .map(|_| "tool-panel-menu");
                for id in [
                    "new-terminal",
                    "split-terminal",
                    "all-tabs",
                    "toggle-inspector",
                ]
                .into_iter()
                .chain(tools)
                {
                    let bounds = window.find(id).bounds();
                    assert!(
                        bounds.left() >= px(0.) && bounds.right() <= px(width),
                        "{id}: {bounds:?}"
                    );
                }
                assert_eq!(app.workspace.read(cx).selected_pane().unwrap().id, pane);
                let settings = app.workspace.read(cx).tool_panel().clone();
                window.click("all-tabs", cx);
                window.render_frame(cx);
                assert!(window.try_find("popup-menu").is_some());
                window.press("escape", cx);
                assert_eq!(app.workspace.read(cx).tool_panel(), &settings);
                if width == 800. {
                    assert!(window.try_find("sidebar-new-chat").is_none());
                    let previous_focus = window.focused(cx);
                    window.click("toggle-sidebar", cx);
                    window.render_frame(cx);
                    assert!(window.try_find("sidebar-new-chat").is_some());
                    for _ in 0..12 {
                        window.press("tab", cx);
                    }
                    let trap =
                        gpui_kit::base::active_focus_trap(window, cx).expect("sidebar focus trap");
                    assert!(trap.contains_focused(window, cx));
                    window.press("escape", cx);
                    assert_eq!(window.focused(cx), previous_focus);
                    assert!(app.layout.read(cx).sidebar_open);
                }
                assert_eq!(
                    app.workspace
                        .read(cx)
                        .all_panes()
                        .iter()
                        .map(|p| p.id)
                        .collect::<Vec<_>>(),
                    ids
                );
                assert!(app.terminals.read(cx).views.is_empty());
            })
            .unwrap();
            cx.run_until_parked();
        }
    }
}
