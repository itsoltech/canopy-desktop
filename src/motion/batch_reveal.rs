use super::{MotionPolicy, Transition, duration, presets};
use std::time::Instant;

/// Entrance for the latest appended range. Earlier items remain fully visible.
/// The view owns this value and schedules frames only while `is_animating`.
#[derive(Default)]
pub struct BatchReveal {
    start: usize,
    items: Vec<Transition>,
}
impl BatchReveal {
    pub fn reset(&mut self) {
        self.start = 0;
        self.items.clear();
    }
    pub fn begin(&mut self, start: usize, count: usize, now: Instant, policy: MotionPolicy) {
        self.start = start;
        // Spread the bounded entrance window evenly, without batching the tail
        // at one shared deadline. Every item retains the same fade duration.
        let intervals = count.saturating_sub(1).max(1).min(u32::MAX as usize) as u32;
        let step = duration::STAGGER.min(duration::STAGGER * 5 / intervals);
        self.items = (0..count)
            .map(|index| {
                let mut reveal = Transition::new(0., now);
                reveal.retarget(
                    1.,
                    presets::CONTENT_REVEAL.enter.delayed(step * index as u32),
                    now,
                    policy,
                );
                reveal
            })
            .collect();
    }
    pub fn opacity(&self, index: usize, now: Instant) -> f32 {
        index
            .checked_sub(self.start)
            .and_then(|i| self.items.get(i))
            .map(|item| item.value(now))
            .unwrap_or(1.)
    }
    pub fn is_animating(&self, now: Instant) -> bool {
        self.items.iter().any(|item| item.is_animating(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appended_range_keeps_old_rows_visible_and_stagger_is_bounded() {
        let now = Instant::now();
        let mut reveal = BatchReveal::default();
        reveal.begin(50, 50, now, MotionPolicy::Full);
        assert_eq!(reveal.opacity(49, now), 1.);
        assert_eq!(reveal.opacity(50, now), 0.);
        let end = now + duration::STAGGER * 5 + presets::CONTENT_REVEAL.enter.duration;
        assert_eq!(reveal.opacity(99, end), 1.);
        assert!(!reveal.is_animating(end));
        reveal.reset();
        assert_eq!(reveal.opacity(50, now), 1.);
    }
    #[test]
    fn every_row_has_equal_cadence_and_fade_duration() {
        let now = Instant::now();
        let mut reveal = BatchReveal::default();
        reveal.begin(0, 50, now, MotionPolicy::Full);
        let step = duration::STAGGER * 5 / 49;
        let sample = presets::CONTENT_REVEAL.enter.duration / 2;
        let expected = reveal.opacity(0, now + sample);
        for index in 0..50 {
            assert_eq!(
                reveal.opacity(index, now + step * index as u32 + sample),
                expected
            );
            assert_eq!(reveal.opacity(index, now + step * index as u32), 0.);
        }
        let tail_start = now + step * 48;
        assert!(reveal.opacity(47, tail_start) > reveal.opacity(48, tail_start));
    }
    #[test]
    fn reduced_motion_skips_both_fade_and_stagger() {
        let now = Instant::now();
        let mut reveal = BatchReveal::default();
        reveal.begin(0, 50, now, MotionPolicy::Reduced);
        assert_eq!(reveal.opacity(49, now), 1.);
        assert!(!reveal.is_animating(now));
    }
}
