use super::super::theme as t;
use super::{column, icon, row};
use gpui_kit::{component::IconName, prelude::FluentBuilder, *};

/// Geometry only: no project, tab or pane identifiers are owned by components.
#[derive(Clone, Copy)]
pub enum PaneAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy)]
pub enum DropHighlight {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

pub fn pane_surface(id: impl Into<ElementId>, focused: bool) -> Stateful<Div> {
    column()
        .id(id)
        .relative()
        .size_full()
        .overflow_hidden()
        .border_1()
        .border_color(if focused { t::border() } else { t::bg() })
}
pub fn pane_header(handle: impl IntoElement, actions: impl IntoElement) -> Div {
    row()
        .h(px(24.))
        .flex_shrink_0()
        .bg(t::sidebar())
        .border_b_1()
        .border_color(t::border())
        .child(handle)
        .child(actions)
}
pub fn pane_handle(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    row()
        .id(id)
        .flex_1()
        .h_full()
        .px(px(6.))
        .gap(px(6.))
        .cursor_pointer()
        .hover(|s| s.bg(t::hover()))
        .child(icon(IconName::Ellipsis))
        .child(
            div()
                .text_size(px(10.))
                .text_color(t::secondary())
                .truncate()
                .child(label.into()),
        )
}
pub fn pane_body(content: impl IntoElement) -> Div {
    div().flex_1().min_h_0().overflow_hidden().child(content)
}
pub fn pane_drop_highlight(target: DropHighlight) -> Div {
    let overlay = div().absolute().inset_0().bg(t::accent()).opacity(0.15);
    match target {
        DropHighlight::Left => overlay.right(relative(0.5)),
        DropHighlight::Right => overlay.left(relative(0.5)),
        DropHighlight::Top => overlay.bottom(relative(0.5)),
        DropHighlight::Bottom => overlay.top(relative(0.5)),
        DropHighlight::Center => overlay,
    }
}
pub fn split_surface(axis: PaneAxis) -> Div {
    div()
        .flex()
        .relative()
        .size_full()
        .when(matches!(axis, PaneAxis::Vertical), |d| d.flex_col())
}
pub fn split_child(axis: PaneAxis, fraction: f32, content: impl IntoElement) -> Div {
    div()
        .min_w_0()
        .min_h_0()
        .flex_shrink_0()
        .when(matches!(axis, PaneAxis::Horizontal), |d| {
            d.w(relative(fraction)).h_full()
        })
        .when(matches!(axis, PaneAxis::Vertical), |d| {
            d.h(relative(fraction)).w_full()
        })
        .child(content)
}

/// Eight-pixel hit target. Position and centering offsets belong to the caller.
pub fn resize_handle(id: impl Into<ElementId>, axis: PaneAxis) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .when(matches!(axis, PaneAxis::Horizontal), |d| {
            d.top_0().bottom_0().w(px(8.)).cursor_col_resize()
        })
        .when(matches!(axis, PaneAxis::Vertical), |d| {
            d.left_0().right_0().h(px(8.)).cursor_row_resize()
        })
        .hover(|s| s.bg(t::hover()))
}

/// GPUI requires an entity for a drag preview; it owns only its display label.
pub struct DragPreview(pub &'static str);
impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .bg(t::sidebar())
            .text_color(t::text())
            .text_size(px(11.))
            .child(self.0)
    }
}
