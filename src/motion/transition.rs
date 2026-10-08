use super::{MotionPolicy, PresenceSpec, TransitionSpec};
use std::time::Instant;

/// A scalar lane for opacity, logical pixels, scale or normalized progress.
/// Caller owns the clock and schedules rendering; this value creates no tasks.
pub struct Transition {
    from: f32,
    target: f32,
    started: Instant,
    spec: TransitionSpec,
    system_settled: std::cell::Cell<bool>,
}
impl Transition {
    pub fn new(value: f32, now: Instant) -> Self {
        assert!(value.is_finite(), "non-finite animation value");
        Self {
            from: value,
            target: value,
            started: now,
            spec: super::presets::STATE_CHANGE,
            system_settled: std::cell::Cell::new(false),
        }
    }
    /// Returns true when state changes. Repeated targets never restart a pending transition.
    pub fn retarget(
        &mut self,
        target: f32,
        spec: TransitionSpec,
        now: Instant,
        policy: MotionPolicy,
    ) -> bool {
        assert!(target.is_finite(), "non-finite animation target");
        if policy == MotionPolicy::Reduced {
            let changed = self.target != target || self.is_animating(now);
            self.from = target;
            self.target = target;
            self.started = now;
            self.spec = spec;
            self.system_settled.set(false);
            return changed;
        }
        if self.target == target {
            return false;
        }
        self.from = self.value(now);
        self.target = target;
        self.started = now;
        self.spec = spec;
        self.system_settled.set(false);
        true
    }
    pub fn value(&self, now: Instant) -> f32 {
        self.value_with_system_policy(now, super::system_reduced())
    }
    fn value_with_system_policy(&self, now: Instant, reduced: bool) -> f32 {
        if reduced {
            self.system_settled.set(true);
            return self.target;
        }
        if self.system_settled.get() {
            return self.target;
        }
        let elapsed = now.saturating_duration_since(self.started);
        if self.from == self.target {
            return self.target;
        }
        if elapsed < self.spec.delay {
            return self.from;
        }
        if self.spec.duration.is_zero()
            || elapsed >= self.spec.delay.saturating_add(self.spec.duration)
        {
            return self.target;
        }
        let progress = elapsed.saturating_sub(self.spec.delay).as_secs_f32()
            / self.spec.duration.as_secs_f32();
        self.from + (self.target - self.from) * self.spec.easing.sample(progress)
    }
    pub fn is_animating(&self, now: Instant) -> bool {
        self.is_animating_with_system_policy(now, super::system_reduced())
    }
    fn is_animating_with_system_policy(&self, now: Instant, reduced: bool) -> bool {
        if reduced {
            self.system_settled.set(true);
        }
        !reduced
            && !self.system_settled.get()
            && self.from != self.target
            && now.saturating_duration_since(self.started)
                < self.spec.delay.saturating_add(self.spec.duration)
    }
}

/// A normalized 0..1 transition with semantic enter/exit specifications.
pub struct Presence {
    lane: Transition,
    spec: PresenceSpec,
}
impl Presence {
    pub fn new(open: bool, spec: PresenceSpec, now: Instant) -> Self {
        Self {
            lane: Transition::new(if open { 1. } else { 0. }, now),
            spec,
        }
    }
    pub fn set_open(&mut self, open: bool, now: Instant, policy: MotionPolicy) -> bool {
        self.lane.retarget(
            if open { 1. } else { 0. },
            if open {
                self.spec.enter
            } else {
                self.spec.exit
            },
            now,
            policy,
        )
    }
    pub fn progress(&self, now: Instant) -> f32 {
        self.lane.value(now)
    }
    pub fn is_animating(&self, now: Instant) -> bool {
        self.lane.is_animating(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::{Easing, duration as d, presets};
    use std::time::Duration;
    #[test]
    fn repeated_target_keeps_original_deadline() {
        let now = Instant::now();
        let mut value = Transition::new(0., now);
        assert!(value.retarget(1., presets::RESIZE, now, MotionPolicy::Full));
        assert!(!value.retarget(1., presets::RESIZE, now + d::QUICK, MotionPolicy::Full));
        assert_eq!(value.value(now + d::FAST), 1.);
        assert!(!value.is_animating(now + d::FAST));
    }
    #[test]
    fn interruption_preserves_value() {
        let now = Instant::now();
        let mut value = Transition::new(20., now);
        value.retarget(300., presets::RESIZE, now, MotionPolicy::Full);
        let middle = now + d::MICRO;
        let snapshot = value.value(middle);
        value.retarget(-20., presets::STATE_CHANGE, middle, MotionPolicy::Full);
        assert_eq!(value.value(middle), snapshot);
        assert_eq!(value.value(middle + d::QUICK), -20.);
    }
    #[test]
    fn zero_duration_can_have_delay_but_reduced_motion_skips_both() {
        let now = Instant::now();
        let spec = TransitionSpec::new(Duration::ZERO, Easing::Linear).delayed(d::MICRO);
        let mut value = Transition::new(0., now);
        value.retarget(1., spec, now, MotionPolicy::Full);
        assert_eq!(value.value(now), 0.);
        assert!(value.is_animating(now));
        assert_eq!(value.value(now + d::MICRO), 1.);
        assert!(value.retarget(1., spec, now, MotionPolicy::Reduced));
        assert_eq!(value.value(now), 1.);
        assert!(!value.is_animating(now));
    }
    #[test]
    fn system_reduce_motion_settles_an_in_flight_transition() {
        let now = Instant::now();
        let mut value = Transition::new(0., now);
        value.retarget(100., presets::RESIZE, now, MotionPolicy::Full);
        assert!(value.is_animating_with_system_policy(now, false));
        assert_eq!(value.value_with_system_policy(now, true), 100.);
        assert!(!value.is_animating_with_system_policy(now, true));
        assert_eq!(value.value_with_system_policy(now, false), 100.);
        assert!(!value.is_animating_with_system_policy(now, false));
    }
    #[test]
    fn presence_uses_a_quicker_exit() {
        let now = Instant::now();
        let mut value = Presence::new(false, presets::POPOVER, now);
        value.set_open(true, now, MotionPolicy::Full);
        assert_eq!(value.progress(now + d::FAST), 1.);
        value.set_open(false, now + d::FAST, MotionPolicy::Full);
        assert_eq!(value.progress(now + d::FAST + d::QUICK), 0.);
    }
    #[test]
    fn standard_easings_are_bounded_and_monotone() {
        for easing in [
            Easing::SmoothOut,
            Easing::EaseInOut,
            Easing::EaseOut,
            Easing::Linear,
        ] {
            let mut previous = 0.;
            for i in 0..=100 {
                let value = easing.sample(i as f32 / 100.);
                assert!(value >= previous && value <= 1.);
                previous = value;
            }
            assert_eq!(easing.sample(0.), 0.);
            assert_eq!(easing.sample(1.), 1.);
        }
    }
}
