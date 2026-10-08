use super::super::{pane_layout::PanelSide, theme as t};
use super::{PaneAxis, resize_handle};
use gpui_kit::*;

struct ResizePreview;
impl Render for ResizePreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(0.))
    }
}
/// Transparent 8px drag target; caller positions it on the panel boundary.
pub fn pane_divider(id: &'static str, side: PanelSide) -> Stateful<Div> {
    resize_handle(id, PaneAxis::Horizontal)
        .child(
            div()
                .absolute()
                .left(px(3.5))
                .top_0()
                .bottom_0()
                .w(px(1.))
                .bg(t::border()),
        )
        .on_drag(side, |_, _, _, cx| cx.new(|_| ResizePreview))
}
