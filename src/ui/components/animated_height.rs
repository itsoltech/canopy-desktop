//! A container whose height follows its content smoothly (RESIZE): when the content
//! grows or shrinks — another question, "Show more" — the container eases to the new
//! measured height instead of jumping. Without motion it changes at once.
use canopy_desktop::motion::presets;
use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ParentElement, Pixels, Style, Styled, Window, px,
    relative, size,
};
use std::time::Instant;

pub struct AnimatedHeight {
    id: ElementId,
    animate: bool,
    child: AnyElement,
}

#[derive(Clone, Copy)]
struct HeightState {
    from: Pixels,
    to: Pixels,
    started: Instant,
}

impl HeightState {
    fn current(&self, now: Instant) -> (Pixels, bool) {
        let spec = presets::RESIZE;
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= spec.duration || self.from == self.to {
            return (self.to, false);
        }
        let progress = spec
            .easing
            .sample(elapsed.as_secs_f32() / spec.duration.as_secs_f32());
        (self.from + (self.to - self.from) * progress, true)
    }
}

impl AnimatedHeight {
    /// `animate` is false under Reduce Motion.
    pub fn new(id: impl Into<ElementId>, animate: bool, child: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            animate,
            // Measured as its own layout root: the wrapper gives it the full width,
            // which a root would otherwise shrink to its content's.
            child: gpui_kit::div().w_full().child(child).into_any_element(),
        }
    }
}

impl IntoElement for AnimatedHeight {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for AnimatedHeight {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let now = Instant::now();
        let height = window.with_element_state(
            global_id.expect("AnimatedHeight has an id"),
            |state: Option<Option<HeightState>>, _| {
                let state = state.flatten();
                (state.map(|s| s.current(now).0), state)
            },
        );
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = height.unwrap_or(px(0.)).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let measured = self
            .child
            .layout_as_root(
                size(
                    AvailableSpace::Definite(bounds.size.width),
                    AvailableSpace::MinContent,
                ),
                window,
                cx,
            )
            .height;
        let now = Instant::now();
        let animate = self.animate;
        let (changed, animating) = window.with_element_state(
            global_id.expect("AnimatedHeight has an id"),
            |state: Option<Option<HeightState>>, _| {
                let first = state.as_ref().is_none_or(|s| s.is_none());
                let mut state = state.flatten().unwrap_or(HeightState {
                    from: measured,
                    to: measured,
                    started: now,
                });
                // The first frame laid out at 0: show the measured height on the next.
                let changed = first || state.to != measured;
                if changed {
                    // Retarget from where the height is now.
                    state.from = if animate {
                        state.current(now).0
                    } else {
                        measured
                    };
                    state.to = measured;
                    state.started = now;
                }
                let animating = state.current(now).1;
                ((changed, animating), Some(state))
            },
        );
        if changed || animating {
            window.request_animation_frame();
        }
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.prepaint_at(bounds.origin, window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.paint(window, cx);
        });
    }
}
