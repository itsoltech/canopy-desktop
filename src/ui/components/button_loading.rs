use super::row;
use canopy_desktop::motion::{self, MotionPolicy, Transition, presets};
use gpui_kit::{
    component::{IconName, Sizable, spinner::Spinner},
    *,
};
use std::time::Instant;
const ICON_SIZE: f32 = super::super::theme::SPACING_UNIT * 4.;
const SLOT_WIDTH: f32 = ICON_SIZE + super::super::theme::SPACING_UNIT;

/// View-owned feedback. Update from the operation's handler/subscription, not
/// render; operation completion never waits for this presentation animation.
pub struct ButtonLoading {
    active: bool,
    reveal: Transition,
    policy: MotionPolicy,
}
impl Default for ButtonLoading {
    fn default() -> Self {
        Self {
            active: false,
            reveal: Transition::new(0., Instant::now()),
            policy: MotionPolicy::Full,
        }
    }
}
impl ButtonLoading {
    pub fn set(&mut self, active: bool, cx: &App) {
        self.set_at(active, Instant::now(), motion::policy(cx));
    }
    fn set_at(&mut self, active: bool, now: Instant, policy: MotionPolicy) {
        self.active = active;
        self.policy = policy;
        self.reveal
            .retarget(if active { 1. } else { 0. }, presets::RESIZE, now, policy);
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn label(&self, label: impl Into<SharedString>) -> impl IntoElement {
        let now = Instant::now();
        LoadingLabel {
            label: label.into(),
            progress: self.reveal.value(now),
            animating: self.reveal.is_animating(now),
            reduced: self.policy == MotionPolicy::Reduced,
        }
    }
    pub fn icon(&self, name: impl Into<gpui_kit::component::Icon>) -> impl IntoElement {
        self.visual(name.into().size(px(ICON_SIZE)))
    }
    pub fn visual(&self, content: impl IntoElement) -> impl IntoElement {
        let now = Instant::now();
        LoadingIcon {
            icon: content.into_any_element(),
            progress: self.reveal.value(now),
            animating: self.reveal.is_animating(now),
            reduced: self.policy == MotionPolicy::Reduced,
        }
    }
}

fn spinner(reduced: bool) -> AnyElement {
    if reduced {
        gpui_kit::component::Icon::new(IconName::Loader)
            .size(px(ICON_SIZE))
            .into_any_element()
    } else {
        Spinner::new().with_size(px(ICON_SIZE)).into_any_element()
    }
}

#[derive(IntoElement)]
struct LoadingIcon {
    icon: AnyElement,
    progress: f32,
    animating: bool,
    reduced: bool,
}
impl RenderOnce for LoadingIcon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        motion::request_frame(window, self.animating);
        div()
            .relative()
            .size(px(ICON_SIZE))
            .child(
                row()
                    .size_full()
                    .justify_center()
                    .opacity(1. - self.progress)
                    .child(self.icon),
            )
            .children((self.progress > 0.).then(|| {
                div()
                    .absolute()
                    .inset_0()
                    .opacity(self.progress)
                    .child(spinner(self.reduced))
            }))
    }
}

#[derive(IntoElement)]
struct LoadingLabel {
    label: SharedString,
    progress: f32,
    animating: bool,
    reduced: bool,
}
impl RenderOnce for LoadingLabel {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        motion::request_frame(window, self.animating);
        row()
            .gap_0()
            .min_w_0()
            .child(div().min_w_0().truncate().child(self.label))
            .children((self.progress > 0.).then(|| {
                div()
                    .relative()
                    .w(px(SLOT_WIDTH * self.progress))
                    .h(px(ICON_SIZE))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .right_0()
                            .top_0()
                            .size(px(ICON_SIZE))
                            .opacity(self.progress)
                            .child(spinner(self.reduced)),
                    )
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::{ButtonLoading, MotionPolicy};
    use canopy_desktop::motion::duration;
    use std::time::Instant;
    #[test]
    fn completion_and_repeated_requests_preserve_continuous_width() {
        let now = Instant::now();
        let mut state = ButtonLoading::default();
        state.set_at(true, now, MotionPolicy::Full);
        let interrupted = now + duration::MICRO;
        let value = state.reveal.value(interrupted);
        state.set_at(false, interrupted, MotionPolicy::Full);
        assert!(!state.active());
        assert_eq!(state.reveal.value(interrupted), value);
        state.set_at(false, interrupted + duration::MICRO, MotionPolicy::Full);
        assert_eq!(state.reveal.value(interrupted + duration::FAST), 0.);
        assert!(!state.reveal.is_animating(interrupted + duration::FAST));
    }
    #[test]
    fn reduced_motion_has_an_immediate_static_indicator() {
        let now = Instant::now();
        let mut state = ButtonLoading::default();
        state.set_at(true, now, MotionPolicy::Reduced);
        assert_eq!(state.reveal.value(now), 1.);
        assert!(!state.reveal.is_animating(now));
        state.set_at(false, now, MotionPolicy::Reduced);
        assert_eq!(state.reveal.value(now), 0.);
    }
}
