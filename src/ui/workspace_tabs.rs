//! Tab gestures and model updates, separate from presentational tab components.
use super::{Workspace, components::*, theme as t, workspace_panes::ContentDrag};
use canopy_desktop::state::workspace::{TabId, WorkspaceId};
use gpui_kit::base::Disableable;
use gpui_kit::{
    component::menu::{ContextMenuExt, PopupMenuItem},
    *,
};

mod scroll;
pub(super) use scroll::TabScroll;

impl Workspace {
    pub(super) fn close_workspace_tab(
        &mut self,
        workspace: WorkspaceId,
        tab: TabId,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<_> = self
            .state
            .workspace
            .read(cx)
            .tabs()
            .iter()
            .find(|t| t.id == tab)
            .map(|t| t.root.panes().iter().map(|p| p.id).collect())
            .unwrap_or_default();
        let target = self.state.workspace.clone();
        crate::app_state::Editors::guard_detaching(
            ids.clone(),
            move |cx| {
                if target.read(cx).id != workspace
                    || !target.read(cx).tabs().iter().any(|t| t.id == tab)
                {
                    return;
                }
                cx.global::<crate::app_state::AppState>()
                    .chats
                    .clone()
                    .update(cx, |chats, _| chats.detach_views(&ids));
                target.update(cx, |state, cx| {
                    if state.id == workspace && state.close(tab).is_ok() {
                        cx.notify();
                    }
                })
            },
            cx,
        );
        self.hovered_tab = None;
        cx.notify();
    }

    fn rename_tab(
        &mut self,
        workspace: WorkspaceId,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some() {
            return;
        }
        let target = self.state.workspace.clone();
        let state = target.read(cx);
        if state.id != workspace {
            return;
        }
        let Some(value) = state
            .tabs()
            .iter()
            .find(|t| t.id == tab)
            .map(|t| t.title.clone())
        else {
            return;
        };
        let return_focus = self.focus.clone();
        let modal = cx.new(|cx| {
            TextPrompt::new(
                "Rename tab",
                "Tab name",
                value,
                return_focus,
                move |title, cx| {
                    target.update(cx, |state, cx| {
                        if state.id != workspace {
                            return Err("The project has changed.".into());
                        }
                        match state.rename(tab, title.to_owned()) {
                            Ok(()) => {
                                cx.notify();
                                Ok(())
                            }
                            Err(_) => Err("Enter a non-empty name without line breaks.".into()),
                        }
                    })
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

    pub(super) fn render_tabs(
        &mut self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let workspace = self.state.workspace.read(cx);
        let active = workspace.active().and_then(|tab| {
            workspace
                .tabs()
                .iter()
                .position(|t| t.id == tab.id)
                .map(|index| (tab.id, index))
        });
        let labels: Vec<_> = workspace
            .tabs()
            .iter()
            .map(|tab| {
                let dirty = self.state.editors.read(cx).busy(
                    &tab.root.panes().iter().map(|p| p.id).collect::<Vec<_>>(),
                    cx,
                );
                let host = tab
                    .root
                    .find(tab.focused)
                    .and_then(|pane| self.state.chats.read(cx).runtime(pane.id))
                    .and_then(|runtime| runtime.read(cx).remote_host())
                    .map(|host| format!(" · {}", host.name()))
                    .unwrap_or_default();
                format!("{}{host}{}", tab.title, if dirty { " •" } else { "" })
            })
            .collect();
        let rem = f32::from(window.rem_size());
        let font = window.text_style().font();
        let minimum = f32::from(t::tab_min_width().to_pixels(window.rem_size()));
        let maximum = f32::from(t::tab_max_width().to_pixels(window.rem_size()))
            .min((width - rem * 2.5).max(minimum));
        let metrics = scroll::TabMetrics {
            widths: labels
                .iter()
                .map(|label| {
                    let run = TextRun {
                        len: label.len(),
                        font: font.clone(),
                        ..Default::default()
                    };
                    let text = window.text_system().shape_line(
                        label.clone().into(),
                        px(rem * 0.75),
                        &[run],
                        None,
                    );
                    (f32::from(text.width) + rem * 3.5).clamp(minimum, maximum)
                })
                .collect(),
            viewport: width,
            edge: rem * 1.25,
        };
        let animating = self.tab_scroll.update(
            workspace.id,
            active,
            &metrics,
            std::time::Instant::now(),
            canopy_desktop::motion::policy(cx),
        );
        canopy_desktop::motion::request_frame(window, animating);
        let tabs = tab_strip("workspace-tabs")
            .w_full()
            .track_scroll(&self.tab_scroll.handle)
            .on_scroll_wheel(cx.listener(|this, _, _, _| this.tab_scroll.interrupt()))
            .children(self.state.workspace.read(cx).tabs().iter().enumerate().map(
                |(index, tab)| {
                    let id = tab.id;
                    let active = self
                        .state
                        .workspace
                        .read(cx)
                        .active()
                        .is_some_and(|t| t.id == id);
                    let editor_pane = tab.focused;
                    let owner = cx.entity().downgrade();
                    let target = self.state.workspace.clone();
                    let workspace = target.read(cx).id;
                    workspace_tab(
                        SharedString::from(format!("{id:?}")),
                        labels[index].clone(),
                        active,
                        metrics.widths[index],
                        if tab.root.first().metadata.kind
                            == canopy_desktop::state::workspace::PaneKind::Chat
                            || canopy_desktop::chat::provider::Provider::from_tool(
                                &tab.root.first().tool,
                            )
                            .is_some()
                        {
                            let status = super::sidebar::navigation_status(tab.root.first().id, cx);
                            Some((status, super::sidebar::status_color(status)))
                        } else {
                            None
                        },
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            this.hovered_tab = Some(id);
                        } else if this.hovered_tab == Some(id) {
                            this.hovered_tab = None;
                        } else {
                            return;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .id(SharedString::from(format!("close-hit-{id:?}")))
                            .size(rems(1.))
                            .flex_shrink_0()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                close_tab_button(
                                    SharedString::from(format!("close-tab-{id:?}")),
                                    cx,
                                )
                                .opacity(if active || self.hovered_tab == Some(id) {
                                    1.
                                } else {
                                    0.
                                })
                                .disabled(!active && self.hovered_tab != Some(id))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.close_workspace_tab(workspace, id, cx);
                                    },
                                )),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.close_workspace_tab(workspace, id, cx);
                        }),
                    )
                    .on_drag(ContentDrag::Tab(id), |_, _, _, cx| {
                        cx.new(|_| DragPreview("Move tab"))
                    })
                    .on_drop(cx.listener(move |this, drag: &ContentDrag, _, cx| {
                        this.state.workspace.update(cx, |state, cx| {
                            let result = match *drag {
                                ContentDrag::Tab(source) => state.reorder_tab(source, Some(id)),
                                ContentDrag::Pane { tab, pane } => state
                                    .detach_pane(tab, pane)
                                    .and_then(|new| state.reorder_tab(new, Some(id))),
                            };
                            if result.is_ok() {
                                cx.notify();
                            }
                        });
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.workspace.update(cx, |state, cx| {
                            let _ = state.activate(id);
                            cx.notify();
                        });
                    }))
                    .context_menu(move |menu, _, cx| {
                        let editor = cx
                            .global::<crate::app_state::AppState>()
                            .editors
                            .read(cx)
                            .views
                            .get(&editor_pane)
                            .cloned();
                        let menu = if let Some(editor) = editor {
                            menu.item(PopupMenuItem::new("Reload file from disk").on_click(
                                move |_, window, cx| {
                                    editor
                                        .update(cx, |editor, cx| editor.request_reload(window, cx));
                                },
                            ))
                        } else {
                            menu
                        };
                        let rename_target = owner.clone();
                        let close_target = owner.clone();
                        menu.item(PopupMenuItem::new("Rename tab…").on_click(
                            move |_, window, cx| {
                                let _ = rename_target.update(cx, |this, cx| {
                                    this.rename_tab(workspace, id, window, cx)
                                });
                            },
                        ))
                        .item(PopupMenuItem::new("Close tab").on_click(move |_, _, cx| {
                            let _ = close_target
                                .update(cx, |this, cx| this.close_workspace_tab(workspace, id, cx));
                        }))
                    })
                },
            ))
            .child(
                tab_drop_slot("new-tab-drop")
                    .drag_over::<ContentDrag>(|s, _, _, _| s.bg(t::hover()))
                    .on_drop(cx.listener(|this, drag: &ContentDrag, _, cx| {
                        this.state.workspace.update(cx, |state, cx| {
                            let result = match *drag {
                                ContentDrag::Tab(tab) => state.reorder_tab(tab, None),
                                ContentDrag::Pane { tab, pane } => {
                                    state.detach_pane(tab, pane).map(|_| ())
                                }
                            };
                            if result.is_ok() {
                                cx.notify();
                            }
                        });
                    })),
            );
        tab_viewport(
            "workspace-tabs-viewport",
            tabs,
            self.tab_scroll.handle.clone(),
        )
    }
}
