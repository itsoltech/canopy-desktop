//! Pane controls translate gestures into atomic domain operations.
use super::{Workspace, components::*};
use canopy_desktop::state::workspace::{Axis, PaneId, SplitId, SplitTree, TabId};
use gpui_kit::{component::IconName, prelude::FluentBuilder, *};

#[derive(Clone, Copy)]
pub(super) enum ContentDrag {
    Pane { tab: TabId, pane: PaneId },
    Tab(TabId),
}
#[derive(Clone, Copy)]
pub(super) struct SplitResize {
    pub tab: TabId,
    pub split: SplitId,
    pub axis: Axis,
    pub bounds: Bounds<Pixels>,
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Placement {
    Swap,
    Split(Axis, bool),
}
fn placement(position: Point<Pixels>, bounds: Bounds<Pixels>) -> Placement {
    let x = f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width).max(1.);
    let y = f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height).max(1.);
    if x < 0.25 {
        Placement::Split(Axis::Horizontal, true)
    } else if x > 0.75 {
        Placement::Split(Axis::Horizontal, false)
    } else if y < 0.25 {
        Placement::Split(Axis::Vertical, true)
    } else if y > 0.75 {
        Placement::Split(Axis::Vertical, false)
    } else {
        Placement::Swap
    }
}
impl Workspace {
    pub(super) fn drop_content(
        &mut self,
        drag: ContentDrag,
        tab: TabId,
        pane: PaneId,
        place: Placement,
        cx: &mut Context<Self>,
    ) {
        self.state.workspace.update(cx, |state, cx| {
            let result = match (drag, place) {
                (
                    ContentDrag::Pane {
                        tab: source,
                        pane: from,
                    },
                    Placement::Swap,
                ) => state.swap_panes(source, from, tab, pane),
                (
                    ContentDrag::Pane {
                        tab: source,
                        pane: from,
                    },
                    Placement::Split(axis, before),
                ) => state.dock_pane(source, from, tab, pane, axis, before),
                (ContentDrag::Tab(source), place) => {
                    let (axis, before) = match place {
                        Placement::Split(axis, before) => (axis, before),
                        Placement::Swap => (Axis::Horizontal, false),
                    };
                    state.dock_tab(source, tab, pane, axis, before)
                }
            };
            if result.is_ok() {
                cx.notify();
            }
        });
        self.drop_hint = None;
        cx.notify();
    }
    pub(super) fn render_tree(
        &self,
        tree: &SplitTree,
        tab: TabId,
        focused: PaneId,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match tree {
            SplitTree::Leaf(pane) => {
                let pane_id = pane.id;
                let show_header = self
                    .state
                    .workspace
                    .read(cx)
                    .tabs()
                    .iter()
                    .find(|candidate| candidate.id == tab)
                    .is_some_and(|candidate| matches!(candidate.root, SplitTree::Split { .. }));
                let hint = self
                    .drop_hint
                    .filter(|(id, _)| *id == pane_id)
                    .map(|(_, p)| p);
                let overlay = pane_drop_highlight(match hint {
                    Some(Placement::Split(Axis::Horizontal, true)) => DropHighlight::Left,
                    Some(Placement::Split(Axis::Horizontal, false)) => DropHighlight::Right,
                    Some(Placement::Split(Axis::Vertical, true)) => DropHighlight::Top,
                    Some(Placement::Split(Axis::Vertical, false)) => DropHighlight::Bottom,
                    _ => DropHighlight::Center,
                });
                pane_surface(
                    SharedString::from(format!("{pane_id:?}")),
                    focused == pane_id,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.state.workspace.update(cx, |state, cx| {
                            let _ = state.focus(tab, pane_id);
                            cx.notify();
                        });
                    }),
                )
                .on_drag_move::<ContentDrag>(cx.listener(
                    move |this, event: &DragMoveEvent<ContentDrag>, _, cx| {
                        if bounds.contains(&event.event.position) {
                            let hint = Some((pane_id, placement(event.event.position, bounds)));
                            if this.drop_hint != hint {
                                this.drop_hint = hint;
                                cx.notify();
                            }
                        }
                    },
                ))
                .on_drop(cx.listener(move |this, drag: &ContentDrag, window, cx| {
                    this.drop_content(
                        *drag,
                        tab,
                        pane_id,
                        placement(window.mouse_position(), bounds),
                        cx,
                    );
                }))
                .children(show_header.then(|| {
                    pane_header(
                        pane_handle("pane-handle", pane.tool.clone())
                            .on_drag(ContentDrag::Pane { tab, pane: pane_id }, |_, _, _, cx| {
                                cx.new(|_| DragPreview("Move pane"))
                            }),
                        icon_button("close-pane", IconName::Close, "Close pane").on_click(
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_editor_pane(tab, pane_id, cx);
                            }),
                        ),
                    )
                }))
                .child(pane_body(
                    if pane.metadata.kind == canopy_desktop::state::workspace::PaneKind::Diff {
                        self.state
                            .diffs
                            .read(cx)
                            .views
                            .get(&pane_id)
                            .map(|v| v.clone().into_any_element())
                            .unwrap_or_else(|| {
                                div().p(px(12.)).child("Loading diff…").into_any_element()
                            })
                    } else if let Some(media) = self.state.editors.read(cx).media_view(pane_id) {
                        media.into_any_element()
                    } else if let Some(image) = self.state.editors.read(cx).images.get(&pane_id) {
                        image.clone().into_any_element()
                    } else if matches!(
                        pane.metadata.kind,
                        canopy_desktop::state::workspace::PaneKind::Editor
                            | canopy_desktop::state::workspace::PaneKind::Image
                    ) {
                        self.state
                            .editors
                            .read(cx)
                            .views
                            .get(&pane_id)
                            .map(|view| view.clone().into_any_element())
                            .unwrap_or_else(|| div().child("Loading editor…").into_any_element())
                    } else {
                        self.state
                            .terminals
                            .read(cx)
                            .views
                            .get(&pane_id)
                            .map(|view| view.clone().into_any_element())
                            .unwrap_or_else(|| {
                                div()
                                    .p(px(12.))
                                    .text_color(super::theme::muted())
                                    .child(
                                        if pane.metadata.kind
                                            == canopy_desktop::state::workspace::PaneKind::Terminal
                                        {
                                            "Loading shell environment…"
                                        } else {
                                            "This pane type is not implemented yet."
                                        },
                                    )
                                    .into_any_element()
                            })
                    },
                ))
                .children((cx.has_active_drag() && hint.is_some()).then_some(overlay))
                .into_any_element()
            }
            SplitTree::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => {
                let horizontal = *axis == Axis::Horizontal;
                let mut first_bounds = bounds;
                let mut second_bounds = bounds;
                if horizontal {
                    first_bounds.size.width = bounds.size.width * *ratio;
                    second_bounds.origin.x += first_bounds.size.width;
                    second_bounds.size.width = bounds.size.width - first_bounds.size.width;
                } else {
                    first_bounds.size.height = bounds.size.height * *ratio;
                    second_bounds.origin.y += first_bounds.size.height;
                    second_bounds.size.height = bounds.size.height - first_bounds.size.height;
                }
                let component_axis = if horizontal {
                    PaneAxis::Horizontal
                } else {
                    PaneAxis::Vertical
                };
                let child = |node: &SplitTree,
                             fraction: f32,
                             rect: Bounds<Pixels>,
                             cx: &mut Context<Self>| {
                    split_child(
                        component_axis,
                        fraction,
                        self.render_tree(node, tab, focused, rect, cx),
                    )
                };
                let divider = resize_handle(SharedString::from(format!("{id:?}")), component_axis)
                    .when(horizontal, |d| d.left(relative(*ratio)).ml(px(-4.)))
                    .when(!horizontal, |d| d.top(relative(*ratio)).mt(px(-4.)))
                    .on_drag(
                        SplitResize {
                            tab,
                            split: *id,
                            axis: *axis,
                            bounds,
                        },
                        |_, _, _, cx| cx.new(|_| DragPreview("Resize")),
                    );
                split_surface(component_axis)
                    .child(child(first, *ratio, first_bounds, cx))
                    .child(child(second, 1. - *ratio, second_bounds, cx))
                    .child(divider)
                    .into_any_element()
            }
        }
    }
}

impl Workspace {
    pub(super) fn close_editor_pane(&mut self, tab: TabId, pane: PaneId, cx: &mut Context<Self>) {
        let target = self.state.workspace.clone();
        let workspace = target.read(cx).id;
        crate::app_state::Editors::guard(
            vec![pane],
            move |cx| {
                target.update(cx, |state, cx| {
                    if state.id == workspace {
                        let _ = state.close_pane(tab, pane);
                        cx.notify();
                    }
                })
            },
            cx,
        );
    }
}
