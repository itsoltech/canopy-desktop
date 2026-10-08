//! Pure countdown and bounded queue policy, independent of rendering.
use gpui_kit::SharedString;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const DISPLAY_TIME: Duration = Duration::from_secs(4);
pub(super) struct Countdown {
    remaining: Duration,
    deadline: Option<Instant>,
}
impl Countdown {
    pub(super) fn is_running(&self) -> bool {
        self.deadline.is_some()
    }
    pub(super) fn new() -> Self {
        Self {
            remaining: DISPLAY_TIME,
            deadline: None,
        }
    }
    pub(super) fn pause(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline.take() {
            self.remaining = deadline.saturating_duration_since(now);
        }
    }
    pub(super) fn resume(&mut self, now: Instant) -> Duration {
        self.deadline = Some(now + self.remaining);
        self.remaining
    }
    pub(super) fn progress(&self, now: Instant) -> f32 {
        let remaining = self
            .deadline
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(self.remaining);
        (1. - remaining.as_secs_f32() / DISPLAY_TIME.as_secs_f32()).clamp(0., 1.)
    }
}

pub(super) fn enqueue(messages: &mut VecDeque<SharedString>, message: SharedString) {
    if messages.len() == 10 {
        messages.pop_back();
    }
    messages.push_back(message);
}

#[cfg(test)]
mod tests {
    use super::{Countdown, DISPLAY_TIME, enqueue};
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};
    #[test]
    fn hover_pauses_without_resetting_progress() {
        let now = Instant::now();
        let mut timer = Countdown::new();
        timer.resume(now);
        timer.pause(now + Duration::from_secs(1));
        assert_eq!(timer.progress(now + DISPLAY_TIME), 0.25);
        assert_eq!(timer.resume(now + DISPLAY_TIME), Duration::from_secs(3));
        assert_eq!(
            timer.progress(now + DISPLAY_TIME + Duration::from_secs(3)),
            1.
        );
        assert_eq!(Countdown::new().progress(now), 0.);
    }
    #[test]
    fn pending_results_preserve_active_message_and_fifo_order() {
        let mut messages = VecDeque::new();
        enqueue(&mut messages, "first".into());
        enqueue(&mut messages, "second".into());

        assert_eq!(messages.pop_front().unwrap().as_ref(), "first");
        assert_eq!(messages.pop_front().unwrap().as_ref(), "second");
    }
    #[test]
    fn repeated_pull_results_are_separate_pending_toasts() {
        let mut messages = VecDeque::new();
        enqueue(&mut messages, "Pulled from origin/main".into());
        enqueue(&mut messages, "Pulled from origin/main".into());
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages.pop_front().unwrap().as_ref(),
            "Pulled from origin/main"
        );
        assert_eq!(
            messages.pop_front().unwrap().as_ref(),
            "Pulled from origin/main"
        );
        assert!(messages.is_empty());
    }
    #[test]
    fn burst_is_bounded_without_displacing_the_active_toast() {
        let mut messages = VecDeque::new();
        for i in 0..20 {
            enqueue(&mut messages, format!("result {i}").into());
        }
        assert_eq!(messages.len(), 10);
        assert_eq!(messages.front().unwrap().as_ref(), "result 0");
        assert_eq!(messages.back().unwrap().as_ref(), "result 19");
    }
}
