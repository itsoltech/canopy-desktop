use super::super::theme as t;
use super::{column, row};
use canopy_desktop::motion::{self, Presence};
use gpui_kit::component::{
    Icon, IconName,
    button::{Button, ButtonCustomVariant, ButtonVariants},
};
use gpui_kit::*;
use std::time::Instant;

pub struct Disclosure {
    pub open: bool,
    pub hovered: bool,
    extent: Presence,
    content: Presence,
}
impl Disclosure {
    pub fn new(open: bool, now: Instant) -> Self {
        Self {
            open,
            hovered: false,
            extent: Presence::new(open, motion::presets::PANEL, now),
            content: Presence::new(open, motion::presets::CONTENT_REVEAL, now),
        }
    }
    pub fn toggle(&mut self, now: Instant, cx: &App) {
        self.open = !self.open;
        let policy = motion::policy(cx);
        self.extent.set_open(self.open, now, policy);
        self.content.set_open(self.open, now, policy);
    }
    pub fn active(&self, now: Instant) -> bool {
        self.extent.is_animating(now) || self.content.is_animating(now)
    }
    pub fn chevron(&self, now: Instant) -> Icon {
        Icon::new(IconName::ChevronRight)
            .size(px(10.))
            .rotate(radians(
                self.extent.progress(now) * std::f32::consts::FRAC_PI_2,
            ))
    }
    pub fn header(&self, id: &'static str, label: &'static str, now: Instant, cx: &App) -> Button {
        let foreground = if self.hovered {
            t::secondary()
        } else {
            t::faint()
        };
        Button::new(id)
            .custom(ButtonCustomVariant::new(cx).foreground(foreground))
            .compact()
            .h(px(36.))
            .w_full()
            .p_0()
            .border_0()
            .accessibility_label(format!(
                "{} {label}",
                if self.open { "Collapse" } else { "Expand" }
            ))
            .child(
                row()
                    .w_full()
                    .gap(px(6.))
                    .text_size(px(10.))
                    .text_color(foreground)
                    .child(self.chevron(now))
                    .child(label),
            )
    }
    pub fn height(&self, full: f32, now: Instant) -> f32 {
        full * self.extent.progress(now)
    }
    /// Intrinsic-height content, e.g. form rows with wrapping labels and help.
    /// Use the toolkit's measured reveal rather than estimating a height per row.
    /// Its content mask clips the animation without introducing another scroller.
    pub fn measured_body(
        &self,
        id: impl Into<ElementId>,
        content: impl IntoElement,
        now: Instant,
    ) -> Div {
        column()
            .flex_shrink_0()
            .children((self.open || self.active(now)).then(|| {
                gpui_kit::base::MotionReveal::new(
                    id,
                    self.extent.progress(now),
                    column()
                        .w_full()
                        .flex_shrink_0()
                        .relative()
                        .top(px(
                            motion::distance::MICRO * (1. - self.content.progress(now))
                        ))
                        .opacity(self.content.progress(now))
                        .child(content)
                        .into_any_element(),
                )
            }))
    }

    pub fn body(&self, height: f32, content: impl IntoElement, now: Instant) -> Div {
        column()
            .h(px(height * self.extent.progress(now)))
            .flex_shrink_0()
            .overflow_hidden()
            .children((self.open || self.active(now)).then(|| {
                div()
                    .w_full()
                    .h(px(height))
                    .relative()
                    .top(px(
                        motion::distance::MICRO * (1. - self.content.progress(now))
                    ))
                    .opacity(self.content.progress(now))
                    .child(content)
            }))
    }
}
