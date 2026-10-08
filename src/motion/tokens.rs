//! Application motion scale, adapted from transitions.dev to native GPUI.
use std::time::Duration;

pub mod duration {
    use super::Duration;
    pub const INSTANT: Duration = Duration::ZERO;
    pub const STAGGER: Duration = Duration::from_millis(40);
    pub const MICRO: Duration = Duration::from_millis(80);
    pub const QUICK: Duration = Duration::from_millis(150);
    pub const FAST: Duration = Duration::from_millis(250);
    pub const MEDIUM: Duration = Duration::from_millis(350);
    pub const SLOW: Duration = Duration::from_millis(400);
    pub const VERY_SLOW: Duration = Duration::from_millis(500);
}
pub mod distance {
    pub const MICRO: f32 = 4.;
    pub const SMALL: f32 = 6.;
    pub const BASE: f32 = 8.;
    pub const MEDIUM: f32 = 12.;
    pub const LARGE: f32 = 30.;
}
pub mod scale {
    pub const MODAL: f32 = 0.96;
    pub const DROPDOWN: f32 = 0.97;
    pub const TOOLTIP: f32 = 0.98;
    pub const SUBTLE: f32 = 0.99;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    SmoothOut,
    EaseInOut,
    EaseOut,
    Linear,
}
impl Easing {
    pub const fn control_points(self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Self::SmoothOut => Some((0.22, 1., 0.36, 1.)),
            Self::EaseInOut => Some((0.42, 0., 0.58, 1.)),
            Self::EaseOut => Some((0., 0., 0.58, 1.)),
            Self::Linear => None,
        }
    }
    pub fn sample(self, progress: f32) -> f32 {
        assert!(progress.is_finite(), "non-finite animation progress");
        if progress <= 0. {
            return 0.;
        }
        if progress >= 1. {
            return 1.;
        }
        let Some((x1, y1, x2, y2)) = self.control_points() else {
            return progress;
        };
        let cubic = |t: f32, a: f32, b: f32| {
            3. * (1. - t) * (1. - t) * t * a + 3. * (1. - t) * t * t * b + t * t * t
        };
        let (mut lo, mut hi) = (0., 1.);
        for _ in 0..18 {
            let t = (lo + hi) * 0.5;
            if cubic(t, x1, x2) < progress {
                lo = t;
            } else {
                hi = t;
            }
        }
        cubic((lo + hi) * 0.5, y1, y2)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TransitionSpec {
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
}
impl TransitionSpec {
    pub const fn new(duration: Duration, easing: Easing) -> Self {
        Self {
            duration,
            delay: Duration::ZERO,
            easing,
        }
    }
    pub const fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PresenceSpec {
    pub enter: TransitionSpec,
    pub exit: TransitionSpec,
}

pub mod presets {
    use super::{Easing::SmoothOut, PresenceSpec, TransitionSpec as Spec, duration as d};
    pub const PANEL: PresenceSpec = PresenceSpec {
        enter: Spec::new(d::SLOW, SmoothOut),
        exit: Spec::new(d::MEDIUM, SmoothOut),
    };
    pub const POPOVER: PresenceSpec = PresenceSpec {
        enter: Spec::new(d::FAST, SmoothOut),
        exit: Spec::new(d::QUICK, SmoothOut),
    };
    pub const CONTENT_REVEAL: PresenceSpec = PresenceSpec {
        enter: Spec::new(d::FAST, SmoothOut).delayed(d::STAGGER),
        exit: Spec::new(d::QUICK, SmoothOut),
    };
    pub const RESIZE: Spec = Spec::new(d::FAST, SmoothOut);
    pub const STATE_CHANGE: Spec = Spec::new(d::QUICK, SmoothOut);
}
