//! The conversation and the worktree's independent tool area share existing pane trees.
use super::*;
use canopy_desktop::state::workspace::{ToolPage, ToolPosition};
use gpui_kit::component::{
    Selectable, Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder;

const TOOL_PAGES: [(ToolPage, &str); 5] = [
    (ToolPage::Tabs, "Terminal"),
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
fn tool_shortcut(page: ToolPage) -> String {
    let number = TOOL_PAGES.iter().position(|(p, _)| *p == page).unwrap_or(0) + 1;
    format!(
        "{}{}",
        if cfg!(target_os = "macos") {
            "⌥⌘"
        } else {
            "Ctrl+Alt+"
        },
        number
    )
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
        let has_project = self.state.projects.read(cx).catalog.active.is_some();
        let terminal = model.tool_tab().is_some_and(|t| {
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
            .h_7()
            .px_2()
            .label("Tools")
            .dropdown_caret(true)
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
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .disabled(!terminal || page != ToolPage::Tabs)
                            .on_click(move |_, _, cx| {
                                let _ = target.update(cx, |s, cx| s.split_terminal(axis, cx));
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

    pub(super) fn tool_navigation(
        &self,
        width: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let model = self.state.workspace.read(cx);
        let selected = model.tool_panel().page();
        let open = model.tools_open() && (selected != ToolPage::Tabs || model.tool_tab().is_some());
        let compact = width < f32::from(window.rem_size()) * 36.;
        let count = model
            .all_panes()
            .iter()
            .filter(|p| p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal)
            .count();
        row()
            .id("tool-navigation")
            .h_full()
            .gap_1()
            .flex_shrink_0()
            .children(
                TOOL_PAGES
                    .into_iter()
                    .filter(|(page, _)| !compact || (open && *page == selected))
                    .map(|(page, label)| {
                        let label = if page == ToolPage::Tabs && count > 0 {
                            format!("{label} · {count}")
                        } else {
                            label.into()
                        };
                        Button::new(SharedString::from(format!("tool-page-{page:?}")))
                            .ghost()
                            .small()
                            .compact()
                            .h_7()
                            .px_2()
                            .label(label)
                            .selected(open && selected == page)
                            .tooltip(format!("{} ({})", page_label(page), tool_shortcut(page)))
                            .disabled(
                                self.state.projects.read(cx).catalog.active.is_none()
                                    && matches!(page, ToolPage::Files | ToolPage::Changes),
                            )
                            .on_click(cx.listener(move |s, _, _, cx| s.select_tool_page(page, cx)))
                    }),
            )
            .child(self.tool_menu(cx))
    }

    fn tool_toolbar(&mut self, width: f32, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let model = self.state.workspace.read(cx);
        let terminal = model.tool_tab().is_some_and(|t| {
            t.root.find(t.focused).is_some_and(|p| {
                p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
            })
        });
        let tabs: Vec<_> = model
            .tool_tabs()
            .map(|t| {
                (
                    t.id,
                    if canopy_desktop::chat::provider::Provider::from_tool(&t.root.first().tool)
                        .is_some()
                    {
                        format!("CLI · {}", t.title)
                    } else {
                        t.title.clone()
                    },
                )
            })
            .collect();
        let active = model.tool_tab().map(|t| t.id);
        let maximized = model.tool_panel().is_maximized();
        let owner = cx.entity().downgrade();
        let all_tabs = Button::new("all-tool-tabs")
            .ghost()
            .small()
            .compact()
            .size_7()
            .p_0()
            .icon(IconName::ChevronDown)
            .tooltip("All terminal and file tabs")
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
        let reserved = f32::from(window.rem_size()) * 7.75;
        let tab_bar = self.render_tabs((width - reserved).max(0.), window, cx);
        row()
            .h(t::tabs_height())
            .flex_shrink_0()
            .bg(t::panel())
            .border_b_1()
            .border_color(t::border())
            .gap_1()
            .pr_1()
            .child(div().flex_1().min_w_0().overflow_hidden().child(tab_bar))
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
                    )
                    .child(
                        Button::new("maximize-tools")
                            .ghost()
                            .small()
                            .compact()
                            .size_7()
                            .p_0()
                            .icon(if maximized {
                                IconName::Minimize
                            } else {
                                IconName::Maximize
                            })
                            .tooltip("Maximize / restore tools")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.state.workspace.update(cx, |w, cx| {
                                    let value = !w.tool_panel().is_maximized();
                                    w.tool_panel_mut().set_maximized(value);
                                    cx.notify();
                                })
                            })),
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
        let requested_open = model.tools_open()
            && (panel.page() != ToolPage::Tabs
                || model.tool_tab().is_some()
                || model.primary_tab().is_some());
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
                let terminal_count = model
                    .all_panes()
                    .iter()
                    .filter(|p| {
                        p.metadata.kind == canopy_desktop::state::workspace::PaneKind::Terminal
                    })
                    .count();
                let context = self
                    .state
                    .projects
                    .read(cx)
                    .catalog
                    .current()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Private workspace".into());
                empty_state(
                    column()
                        .id("empty-worktree-content")
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t::text())
                                .child(context),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(t::secondary())
                                .child("Start a conversation or open your worktree terminal."),
                        )
                        .child(
                            row()
                                .gap_3()
                                .child(
                                    primary_button("open-empty-chat", "New chat")
                                        .disabled(reason.is_some())
                                        .on_click(cx.listener(|s, _, _, cx| {
                                            s.state
                                                .chats
                                                .update(cx, |chats, cx| chats.open_current(cx))
                                        })),
                                )
                                .child(
                                    Button::new("open-empty-terminal")
                                        .ghost()
                                        .small()
                                        .label(if terminal_count > 0 {
                                            format!("Show terminals ({terminal_count})")
                                        } else {
                                            "Open terminal".into()
                                        })
                                        .disabled(
                                            self.state.projects.read(cx).catalog.active.is_none(),
                                        )
                                        .on_click(cx.listener(move |s, _, _, cx| {
                                            if terminal_count > 0 {
                                                s.select_tool_page(ToolPage::Tabs, cx);
                                            } else {
                                                s.new_terminal(cx);
                                            }
                                        })),
                                ),
                        ),
                    reason.map(Into::into),
                    None,
                )
                .into_any_element()
            };
            area = area.child(
                column()
                    .w(px(main_width))
                    .h(px(main_height))
                    .min_w_0()
                    .min_h_0()
                    .child(main),
            );
        }
        if open {
            let error = self.state.tools.read(cx).error.clone();
            let error_height = if error.is_some() { rem * 2.25 } else { 0. };
            let return_bar = maximized && model.primary_tab().is_some();
            let chrome_height = (if return_bar { rem * 2.25 } else { 0. })
                + if panel.page() == ToolPage::Tabs {
                    rem * 2.25
                } else {
                    0.
                };
            let toolbar =
                (panel.page() == ToolPage::Tabs).then(|| self.tool_toolbar(tool_width, window, cx));
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
                                        + chrome_height
                                        + error_height),
                                ),
                                size(
                                    px(tool_width),
                                    px((tool_height - chrome_height - error_height).max(0.)),
                                ),
                            ),
                            cx,
                        )
                    })
                    .unwrap_or_else(|| {
                        div()
                            .flex()
                            .flex_col()
                            .size_full()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .p_4()
                            .text_color(t::secondary())
                            .child("No terminals in this worktree")
                            .child(
                                Button::new("open-first-terminal")
                                    .ghost()
                                    .small()
                                    .label("Open terminal")
                                    .disabled(self.state.projects.read(cx).catalog.active.is_none())
                                    .on_click(cx.listener(|s, _, _, cx| s.new_terminal(cx))),
                            )
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
                .children(toolbar)
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
                window.click("tool-page-Agent", cx);
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
            assert!(maximize.right() <= window.viewport_size().width);
            assert!(menu.right() <= window.viewport_size().width);
            assert!(window.find("new-terminal").bounds().right() <= window.viewport_size().width);
        })
        .unwrap();
        // Let the previous popover finish dismissal and the font change settle.
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("tool-panel-menu", cx);
            choose_menu("New terminal", window, cx);
            assert_eq!(app.workspace.read(cx).tabs().len(), 4);
            assert_eq!(app.workspace.read(cx).primary_tab().unwrap().id, chat);
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
                model.open_conversation("Conversation A", "claude");
                let pane = model.active().unwrap().focused;
                for n in 1..8 {
                    model.open(format!("Terminal {n}"), "shell");
                }
                model.show_tools(ToolPage::Tabs);
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
                for id in [
                    "new-terminal",
                    "split-terminal",
                    "all-tool-tabs",
                    "tool-panel-menu",
                ] {
                    let bounds = window.find(id).bounds();
                    assert!(
                        bounds.left() >= px(0.) && bounds.right() <= px(width),
                        "{id}: {bounds:?}"
                    );
                }
                assert_eq!(app.workspace.read(cx).selected_pane().unwrap().id, pane);
                let settings = app.workspace.read(cx).tool_panel().clone();
                window.click("all-tool-tabs", cx);
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
