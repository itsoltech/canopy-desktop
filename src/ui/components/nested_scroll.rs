//! Scroll chaining for a scroller inside another (tool output in the conversation): the
//! wheel moves the inner one, and reaches the outer one only when the inner one already
//! stands at its edge in that direction, as browsers do.
//!
//! The conversation's list takes the wheel in the capture phase (its `ScrollableMask`,
//! so code blocks don't turn vertical wheels sideways), before any bubbling listener of
//! a row. An inner scroller therefore needs its own vertical mask: painted before the
//! list's, it runs first, scrolls while it has room and stops the event; at its edge it
//! lets the event through to the list.
use gpui_kit::component::scroll::ScrollableMask;
use gpui_kit::*;

pub trait NestedScroll {
    /// Lets the wheel scroll this element before its scrolling ancestors. It needs an id.
    fn nested_scroll(self) -> NestedScroller;
}

impl NestedScroll for Stateful<Div> {
    fn nested_scroll(mut self) -> NestedScroller {
        let id = self
            .interactivity()
            .element_id
            .clone()
            .expect("a scroller has an id");
        NestedScroller { id, scroller: self }
    }
}

#[derive(IntoElement)]
pub struct NestedScroller {
    id: ElementId,
    scroller: Stateful<Div>,
}

impl RenderOnce for NestedScroller {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The offset lives as long as the element is drawn, like a scroller's own.
        let handle = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::new())
            .read(cx)
            .clone();
        // The wrapper takes the scroller's place in its parent's layout (size, flex and
        // margins), so the mask beside it covers exactly the scroller.
        let style = self.scroller.style();
        let place = StyleRefinement {
            size: style.size.clone(),
            min_size: style.min_size.clone(),
            max_size: style.max_size.clone(),
            margin: std::mem::take(&mut style.margin),
            flex_grow: style.flex_grow,
            flex_shrink: style.flex_shrink,
            flex_basis: style.flex_basis,
            align_self: style.align_self,
            ..Default::default()
        };
        let mut wrapper = div().relative();
        wrapper.style().refine(&place);
        // A block wrapper: the mask sits where its static position puts it, which it
        // expects (as in gpui-kit's own horizontal scroll area).
        wrapper
            .child(self.scroller.track_scroll(&handle))
            .child(ScrollableMask::new(Axis::Vertical, &handle).id(self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    struct Probe {
        outer: ScrollHandle,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            // As the conversation: the outer scroller takes the wheel in the capture
            // phase through a mask painted after its content.
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .id("outer")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.outer)
                        .child(
                            div()
                                .id("inner")
                                .h(px(100.))
                                .overflow_y_scroll()
                                .child(div().h(px(300.)))
                                .nested_scroll(),
                        )
                        .child(div().h(px(2000.))),
                )
                .child(ScrollableMask::new(Axis::Vertical, &self.outer).id("outer-mask"))
        }
    }

    fn wheel(cx: &mut VisualTestContext, y: f32) {
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(10.), px(20.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(y))),
            ..Default::default()
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn the_inner_scroller_takes_the_wheel_until_it_reaches_its_edge(cx: &mut TestAppContext) {
        let outer = ScrollHandle::new();
        let handle = outer.clone();
        let (_, cx) = cx.add_window_view(move |_, _| Probe { outer: handle });
        // Down within the inner one (200 px to go): the outer one stays.
        wheel(cx, -150.);
        wheel(cx, -100.);
        assert_eq!(outer.offset().y, px(0.));
        // The inner one stands at its bottom: the wheel goes on to the outer one.
        wheel(cx, -50.);
        assert_eq!(outer.offset().y, px(-50.));
        // Up: the inner one has room again and takes it.
        wheel(cx, 30.);
        assert_eq!(outer.offset().y, px(-50.));
    }
}
