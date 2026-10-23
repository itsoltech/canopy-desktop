//! The conversation and the worktree's independent tool area share existing pane trees.
use super::*;
use canopy_desktop::state::workspace::{ToolPage, ToolPosition};
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder;

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
            if w.tool_panel().page() != ToolPage::Tabs {
                return Ok(());
            }
            let target = w.tool_tab().and_then(|t| {
                t.root
                    .find(t.focused)
                    .filter(|p| {
                        p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
                    })
                    .map(|p| (t.id, p.id, p.tool.clone(), p.metadata.clone()))
            });
            if let Some((tab, pane, tool, mut metadata)) = target {
                metadata.resume_id = None;
                metadata.task_prompt = None;
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
                        "This terminal has reached the split limit.".into()
                    }
                    _ => "The terminal changed before it could be split. Try again.".into(),
                });
                cx.notify();
            });
        }
    }
    fn select_tool_page(&mut self, page: ToolPage, cx: &mut Context<Self>) {
        self.state.workspace.update(cx, |w, cx| {
            w.show_tools(page);
            cx.notify();
        });
    }
    fn tool_toolbar(&mut self, width: f32, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let model = self.state.workspace.read(cx);
        let panel = model.tool_panel().clone();
        let page = panel.page();
        let has_project = self.state.projects.read(cx).catalog.active.is_some();
        let terminal = model.tool_tab().is_some_and(|t| {
            t.root.find(t.focused).is_some_and(|p| {
                p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
            })
        });
        let compact = width < 280.;
        let utility_width = if page == ToolPage::Tabs { 0. } else { 72. };
        let tab_bar = self.render_tabs(
            (width - if compact { 60. } else { 116. } - utility_width).max(0.),
            window,
            cx,
        );
        let owner = cx.entity().downgrade();
        let tools = self.state.tools.read(cx).catalog.clone();
        let menu = Button::new("tool-panel-menu")
            .ghost()
            .small()
            .compact()
            .size_6()
            .p_0()
            .icon(IconName::Ellipsis)
            .tooltip("Tools and layout")
            .dropdown_menu(move |menu, _, cx| {
                let mut menu = menu;
                for (label, target) in [
                    ("Files", ToolPage::Files),
                    ("Changes", ToolPage::Changes),
                    ("Tasks", ToolPage::Tasks),
                    ("Agent", ToolPage::Agent),
                ] {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .disabled(
                                !has_project
                                    && matches!(target, ToolPage::Files | ToolPage::Changes),
                            )
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |w, cx| w.select_tool_page(target, cx));
                            }),
                    );
                }
                let new_owner = owner.clone();
                menu = menu.separator().item(
                    PopupMenuItem::new("New terminal")
                        .disabled(!has_project)
                        .on_click(move |_, _, cx| {
                            let _ = new_owner.update(cx, |w, cx| w.new_terminal(cx));
                        }),
                );
                for (label, axis) in [
                    ("Split terminal right", Axis::Horizontal),
                    ("Split terminal below", Axis::Vertical),
                ] {
                    let split_owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .disabled(!terminal || page != ToolPage::Tabs)
                            .on_click(move |_, _, cx| {
                                let _ = split_owner.update(cx, |w, cx| w.split_terminal(axis, cx));
                            }),
                    );
                }
                menu = menu.separator();
                for tool in tools.tools.iter().filter(|t| t.enabled) {
                    let profiles: Vec<_> = if tool.profiles.is_empty() {
                        vec![None]
                    } else {
                        tool.profiles.iter().map(|p| Some(p.id.clone())).collect()
                    };
                    for profile in profiles {
                        let mut label = tool.label(profile.as_deref());
                        let running = cx.global::<AppState>().terminals.read(cx).running_count(
                            &tool.id,
                            profile.as_deref(),
                            cx,
                        );
                        if running > 0 {
                            label = format!("{label} · {running} running");
                        }
                        let id = tool.id.clone();
                        menu =
                            menu.item(PopupMenuItem::new(label).disabled(!has_project).on_click(
                                move |_, _, cx| {
                                    cx.global::<AppState>()
                                        .tools
                                        .clone()
                                        .update(cx, |tools, cx| {
                                            tools.launch(&id, profile.as_deref(), cx)
                                        })
                                },
                            ));
                    }
                }
                menu = menu.separator();
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
                            })
                    }));
                }
                menu.item(PopupMenuItem::new("Hide tools").on_click(|_, _, cx| {
                    cx.global::<AppState>()
                        .workspace
                        .clone()
                        .update(cx, |w, cx| {
                            w.hide_tools();
                            cx.notify();
                        })
                }))
            });
        row()
            .h(px(t::TABS))
            .flex_shrink_0()
            .bg(t::sidebar())
            .border_b_1()
            .border_color(t::border())
            .gap_1()
            .child(div().flex_1().min_w_0().child(tab_bar))
            .children((page != ToolPage::Tabs).then(|| {
                Button::new("selected-tool-page")
                    .ghost()
                    .small()
                    .label(match page {
                        ToolPage::Files => "Files",
                        ToolPage::Changes => "Changes",
                        ToolPage::Tasks => "Tasks",
                        _ => "Agent",
                    })
                    .tooltip("Return to terminal tabs")
                    .on_click(
                        cx.listener(|this, _, _, cx| this.select_tool_page(ToolPage::Tabs, cx)),
                    )
            }))
            .children((!compact).then(|| {
                Button::new("new-terminal")
                    .ghost()
                    .small()
                    .compact()
                    .size_6()
                    .p_0()
                    .icon(IconName::Plus)
                    .tooltip("New terminal")
                    .disabled(!has_project)
                    .on_click(cx.listener(|this, _, _, cx| this.new_terminal(cx)))
            }))
            .children((!compact).then(|| {
                Button::new("split-terminal")
                    .ghost()
                    .small()
                    .compact()
                    .size_6()
                    .p_0()
                    .icon(IconName::PanelRight)
                    .tooltip("Split terminal")
                    .disabled(!terminal || page != ToolPage::Tabs)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.split_terminal(Axis::Horizontal, cx)),
                    )
            }))
            .child(
                Button::new("maximize-tools")
                    .ghost()
                    .small()
                    .compact()
                    .size_6()
                    .p_0()
                    .icon(if panel.is_maximized() {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .tooltip("Maximize / restore tools")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.workspace.update(cx, |w, cx| {
                            let value = !w.tool_panel().is_maximized();
                            w.tool_panel_mut().set_maximized(value);
                            cx.notify();
                        })
                    })),
            )
            .child(menu)
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
        let open = model.tools_open();
        let bottom = panel.position() == ToolPosition::Bottom;
        let maximized = panel.is_maximized() && open;
        let tool_width = if maximized || bottom {
            width
        } else {
            panel.width().min((width - 160.).max(0.))
        };
        let tool_height = if maximized || !bottom {
            height
        } else {
            panel.height().min((height - 120.).max(0.))
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
            let main = if let Some(tab) = model.primary_tab() {
                self.render_tree(
                    &tab.root,
                    tab.id,
                    tab.focused,
                    Bounds::new(
                        point(px(x), px(t::TITLEBAR)),
                        size(px(main_width), px(main_height)),
                    ),
                    cx,
                )
            } else {
                let reason = crate::app_state::Chats::unavailable(cx);
                empty_state(
                    primary_button("open-empty-chat", "New chat")
                        .disabled(reason.is_some())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state
                                .chats
                                .update(cx, |chats, cx| chats.open_current(cx))
                        })),
                    reason.map(Into::into),
                    None,
                )
                .into_any_element()
            };
            area = area.child(
                div()
                    .w(px(main_width))
                    .h(px(main_height))
                    .min_w_0()
                    .min_h_0()
                    .child(main),
            );
        }
        if open {
            let error = self.state.tools.read(cx).error.clone();
            let error_height = if error.is_some() { t::TABS } else { 0. };
            let toolbar = self.tool_toolbar(tool_width, window, cx);
            let content = if panel.page() == ToolPage::Tabs {
                model
                    .tool_tab()
                    .map(|tab| {
                        self.render_tree(
                            &tab.root,
                            tab.id,
                            tab.focused,
                            Bounds::new(
                                point(
                                    px(x + if bottom || maximized { 0. } else { main_width }),
                                    px(t::TITLEBAR
                                        + if bottom && !maximized {
                                            main_height
                                        } else {
                                            0.
                                        }
                                        + t::TABS
                                        + error_height),
                                ),
                                size(
                                    px(tool_width),
                                    px((tool_height - t::TABS - error_height).max(0.)),
                                ),
                            ),
                            cx,
                        )
                    })
                    .unwrap_or_else(|| {
                        div()
                            .p_4()
                            .text_color(t::muted())
                            .child("Open a terminal, file or tool from the menu.")
                            .into_any_element()
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
            let mut tools = column()
                .relative()
                .w(px(tool_width))
                .h(px(tool_height))
                .flex_shrink_0()
                .border_color(t::border())
                .when(bottom, |d| d.border_t_1())
                .when(!bottom, |d| d.border_l_1())
                .child(toolbar)
                .children(error.map(|error| {
                    row()
                        .h(px(t::TABS))
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
    fn tools_split_maximize_and_hide_without_closing_the_conversation(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (app, chat, terminal) = cx.update(|cx| {
            crate::ui::theme::init(cx);
            let app = AppState::install_test(Model::new(), cx);
            app.projects.update(cx, |p, cx| {
                p.ready = true;
                p.open_worktree("/workspace/repo".into(), "/workspace/repo".into(), cx);
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
        let second_terminal = cx
            .update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, chat);
                window.click("new-terminal", cx);
                let second_terminal = app.workspace.read(cx).tool_tab().unwrap().id;
                assert_ne!(second_terminal, terminal);
                assert_eq!(
                    app.workspace.read(cx).tool_tab().unwrap().title,
                    "Terminal 2"
                );
                window.render_frame(cx);
                window.click("split-terminal", cx);
                assert_eq!(
                    app.workspace
                        .read(cx)
                        .tool_tab()
                        .unwrap()
                        .root
                        .panes()
                        .len(),
                    2
                );
                assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, chat);
                window.render_frame(cx);
                window.click("maximize-tools", cx);
                assert!(app.workspace.read(cx).tool_panel().is_maximized());
                window.render_frame(cx);
                window.click("maximize-tools", cx);
                assert!(!app.workspace.read(cx).tool_panel().is_maximized());
                window.render_frame(cx);
                window.click("tool-panel-menu", cx);
                choose_menu("Dock at bottom", window, cx);
                assert_eq!(
                    app.workspace.read(cx).tool_panel().position(),
                    ToolPosition::Bottom
                );
                window.render_frame(cx);
                window.click("tool-panel-menu", cx);
                choose_menu("Agent", window, cx);
                assert_eq!(app.workspace.read(cx).tool_panel().page(), ToolPage::Agent);
                window.render_frame(cx);
                window.dispatch_action(Box::new(super::super::CloseTab), cx);
                second_terminal
            })
            .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            assert_eq!(app.workspace.read(cx).tool_panel().page(), ToolPage::Tabs);
            assert_eq!(app.workspace.read(cx).tabs().len(), 3);
            window.render_frame(cx);
            window.click("tool-panel-menu", cx);
            choose_menu("Hide tools", window, cx);
            let model = app.workspace.read(cx);
            assert!(!model.tools_open());
            assert_eq!(model.primary_tab().unwrap().id, chat);
            assert_eq!(model.tool_tab().unwrap().id, second_terminal);
            assert_eq!(model.tool_tab().unwrap().root.panes().len(), 2);
            assert_eq!(model.tool_panel().page(), ToolPage::Tabs);
            app.workspace.update(cx, |w, cx| {
                w.show_tools(ToolPage::Tabs);
                w.tool_panel_mut().set_position(ToolPosition::Right);
                w.tool_panel_mut().resize(180.);
                cx.notify();
            });
            gpui_kit::component::Theme::global_mut(cx).font_size = px(20.);
            gpui_kit::component::Theme::sync_base(cx);
            window.render_frame(cx);
            let menu = window.find("tool-panel-menu").bounds();
            let maximize = window.find("maximize-tools").bounds();
            assert!(maximize.right() <= menu.left());
            assert!(menu.right() <= window.viewport_size().width);
            assert!(window.try_find("new-terminal").is_none());
            window.click("tool-panel-menu", cx);
            choose_menu("New terminal", window, cx);
            assert_eq!(app.workspace.read(cx).tabs().len(), 4);
            assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, chat);
        })
        .unwrap();
    }
}
