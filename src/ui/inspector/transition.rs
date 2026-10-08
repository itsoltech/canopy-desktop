use canopy_desktop::motion::{MotionPolicy, Transition, presets};
use std::time::Instant;

/// The underline follows tab positions; content fades by identity, so jumping
/// across a tab never reveals a page that the user did not select.
pub(super) struct InspectorTransition {
    selected: usize,
    indicator: Transition,
    content: [Transition; 3],
}

impl InspectorTransition {
    pub fn new(selected: usize, now: Instant) -> Self {
        Self {
            selected,
            indicator: Transition::new(selected as f32, now),
            content: std::array::from_fn(|page| {
                Transition::new(if page == selected { 1. } else { 0. }, now)
            }),
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, selected: usize, now: Instant, policy: MotionPolicy) {
        self.selected = selected;
        self.indicator
            .retarget(selected as f32, presets::RESIZE, now, policy);
        for (page, content) in self.content.iter_mut().enumerate() {
            content.retarget(
                if page == selected { 1. } else { 0. },
                presets::STATE_CHANGE,
                now,
                policy,
            );
        }
    }

    pub fn indicator(&self, now: Instant) -> f32 {
        self.indicator.value(now)
    }

    pub fn opacity(&self, page: usize, now: Instant) -> f32 {
        self.content[page].value(now)
    }

    pub fn content_is_animating(&self, now: Instant) -> bool {
        self.content.iter().any(|content| content.is_animating(now))
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.indicator.is_animating(now) || self.content_is_animating(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_desktop::motion::duration;
    use std::time::Duration;

    #[test]
    fn jumping_between_session_and_tasks_never_reveals_changes() {
        for (from, to) in [(0, 2), (2, 0)] {
            let now = Instant::now();
            let mut tabs = InspectorTransition::new(from, now);
            tabs.select(to, now, MotionPolicy::Full);
            for ms in 0..=300 {
                let frame = now + Duration::from_millis(ms);
                assert_eq!(tabs.opacity(1, frame), 0.);
                assert!((tabs.opacity(from, frame) + tabs.opacity(to, frame) - 1.).abs() < 1e-6);
            }
            assert_eq!(tabs.opacity(to, now + duration::FAST), 1.);
            assert!(!tabs.is_animating(now + duration::FAST));
        }
    }

    #[test]
    fn rapid_switches_continue_from_current_opacity_and_indicator() {
        let now = Instant::now();
        let mut tabs = InspectorTransition::new(0, now);
        tabs.select(2, now, MotionPolicy::Full);
        for (step, page) in [0, 2, 1, 0].into_iter().enumerate() {
            let frame = now + Duration::from_millis(25 * (step as u64 + 1));
            let opacity: [f32; 3] = std::array::from_fn(|page| tabs.opacity(page, frame));
            let indicator = tabs.indicator(frame);
            tabs.select(page, frame, MotionPolicy::Full);
            assert_eq!(tabs.indicator(frame), indicator);
            for (page, before) in opacity.into_iter().enumerate() {
                assert_eq!(tabs.opacity(page, frame), before);
            }
        }
        let finished = now + duration::SLOW;
        assert_eq!(tabs.opacity(0, finished), 1.);
        assert_eq!(tabs.opacity(1, finished), 0.);
        assert_eq!(tabs.opacity(2, finished), 0.);
        assert!(!tabs.is_animating(finished));
    }

    #[test]
    fn repeated_selection_keeps_deadline() {
        let now = Instant::now();
        let mut tabs = InspectorTransition::new(0, now);
        tabs.select(2, now, MotionPolicy::Full);
        tabs.select(2, now + duration::MICRO, MotionPolicy::Full);
        assert!(!tabs.content_is_animating(now + duration::QUICK));
        assert!(!tabs.is_animating(now + duration::FAST));
    }

    #[test]
    fn reduced_motion_settles_all_pages_immediately_even_mid_transition() {
        let now = Instant::now();
        let mut tabs = InspectorTransition::new(0, now);
        tabs.select(2, now, MotionPolicy::Full);
        let frame = now + duration::MICRO;
        tabs.select(2, frame, MotionPolicy::Reduced);
        assert_eq!(tabs.indicator(frame), 2.);
        assert_eq!(tabs.opacity(0, frame), 0.);
        assert_eq!(tabs.opacity(1, frame), 0.);
        assert_eq!(tabs.opacity(2, frame), 1.);
        assert!(!tabs.is_animating(frame));
    }
}
