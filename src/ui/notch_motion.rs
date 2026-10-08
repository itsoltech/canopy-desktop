//! The notch composes shared motion primitives; it owns no interpolation engine.
use canopy_desktop::motion::{
    Easing, MotionPolicy, Presence, PresenceSpec, TransitionSpec, distance, duration, presets,
};
use std::time::Instant;

#[cfg(test)]
const OPEN_HEIGHT: std::time::Duration = duration::SLOW;
#[cfg(test)]
const CLOSE: std::time::Duration = duration::MEDIUM;
#[cfg(test)]
const CONTENT_OUT: std::time::Duration = duration::QUICK;
pub const CONTENT_TRAVEL: f32 = distance::MICRO;
const WIDTH: PresenceSpec = PresenceSpec {
    enter: TransitionSpec::new(duration::FAST, Easing::SmoothOut),
    exit: TransitionSpec::new(duration::MEDIUM, Easing::SmoothOut),
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Values {
    pub width: f32,
    pub height: f32,
    pub content: f32,
}
#[cfg(test)]
impl Values {
    fn settled(v: f32) -> Self {
        Self {
            width: v,
            height: v,
            content: v,
        }
    }
}

pub struct Motion {
    width: Presence,
    height: Presence,
    content: Presence,
}
impl Motion {
    pub fn new(now: Instant) -> Self {
        Self {
            width: Presence::new(false, WIDTH, now),
            height: Presence::new(false, presets::PANEL, now),
            content: Presence::new(false, presets::CONTENT_REVEAL, now),
        }
    }
    pub fn retarget(&mut self, open: bool, now: Instant, policy: MotionPolicy) {
        self.width.set_open(open, now, policy);
        self.height.set_open(open, now, policy);
        self.content.set_open(open, now, policy);
    }
    pub fn values(&self, now: Instant) -> Values {
        Values {
            width: self.width.progress(now),
            height: self.height.progress(now),
            content: self.content.progress(now),
        }
    }
    pub fn is_animating(&self, now: Instant) -> bool {
        self.width.is_animating(now)
            || self.height.is_animating(now)
            || self.content.is_animating(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn reversal_starts_at_current_geometry_and_opacity() {
        let now = Instant::now();
        let mut motion = Motion::new(now);
        motion.retarget(true, now, MotionPolicy::Full);
        let middle = now + Duration::from_millis(120);
        let before = motion.values(middle);
        motion.retarget(false, middle, MotionPolicy::Full);
        assert_eq!(motion.values(middle), before);
        assert_eq!(motion.values(middle + CLOSE), Values::settled(0.));
        assert!(!motion.is_animating(middle + CLOSE));
    }
    #[test]
    fn content_waits_for_surface_and_close_has_no_delay() {
        let now = Instant::now();
        let mut motion = Motion::new(now);
        motion.retarget(true, now, MotionPolicy::Full);
        assert!(motion.values(now + Duration::from_millis(20)).width > 0.);
        assert_eq!(motion.values(now + Duration::from_millis(20)).content, 0.);
        assert_eq!(motion.values(now + OPEN_HEIGHT), Values::settled(1.));
        motion.retarget(false, now + OPEN_HEIGHT, MotionPolicy::Full);
        assert!(
            motion
                .values(now + OPEN_HEIGHT + Duration::from_millis(1))
                .height
                < 1.
        );
        assert_eq!(motion.values(now + OPEN_HEIGHT + CONTENT_OUT).content, 0.);
    }
    #[test]
    fn reduced_motion_settles_immediately_without_frames() {
        let now = Instant::now();
        let mut motion = Motion::new(now);
        motion.retarget(true, now, MotionPolicy::Reduced);
        assert_eq!(motion.values(now), Values::settled(1.));
        assert!(!motion.is_animating(now));
        motion.retarget(false, now, MotionPolicy::Reduced);
        assert_eq!(motion.values(now), Values::settled(0.));
    }
    #[test]
    fn easing_is_monotone_and_never_overshoots() {
        let mut previous = 0.;
        for i in 0..=100 {
            let value = Easing::SmoothOut.sample(i as f32 / 100.);
            assert!((previous..=1.).contains(&value));
            previous = value;
        }
    }
}
