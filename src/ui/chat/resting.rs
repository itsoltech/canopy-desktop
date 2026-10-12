//! The composer rests while the reader scrolls the conversation, as in T3 Code: 24 px of
//! scrolling in one go — wheel, touchpad or the scrollbar alike — shrinks it to one line
//! with Attach and Send. Scrolling back to the end, Jump to latest, typing or clicking
//! its text brings the full composer back.
use super::view::ChatView;
use canopy_desktop::motion::{self, Transition, presets};
use gpui_kit::*;
use std::time::{Duration, Instant};

/// Scrolling within one movement that puts the composer to rest.
const THRESHOLD: f32 = 24.;
/// A pause this long between scroll changes starts a new movement.
const MOVEMENT_GAP: Duration = Duration::from_millis(200);

pub(super) struct Resting {
    pub on: bool,
    travel: f32,
    last_move: Option<Instant>,
    /// Typing during a movement: its remainder (momentum) doesn't put the composer
    /// back to rest.
    suppressed: bool,
    /// The conversation's position in the last frame: (row, offset in it) and pixels.
    position: Option<(usize, Pixels)>,
    top: f32,
    /// `ChatScrollerState::ends` last seen: a jump to the end wakes the composer.
    ends: u64,
    /// Island height before the change and the eased 0..1 toward the new one.
    from: Pixels,
    progress: Transition,
    /// The island's natural height in the last frame.
    pub measured: std::rc::Rc<std::cell::Cell<Pixels>>,
}

impl Resting {
    pub fn new(now: Instant) -> Self {
        Self {
            on: false,
            travel: 0.,
            last_move: None,
            suppressed: false,
            position: None,
            top: 0.,
            ends: 0,
            from: px(0.),
            progress: Transition::new(1., now),
            measured: Default::default(),
        }
    }

    /// The island's height while it eases between its forms; `None` once settled.
    pub fn height(&self, now: Instant) -> Option<Pixels> {
        if !self.progress.is_animating(now) {
            return None;
        }
        let to = self.measured.get();
        Some(self.from + (to - self.from) * self.progress.value(now))
    }
}

/// The conversation's scroll state in one frame.
#[derive(Clone, Copy)]
pub(super) struct Scroll {
    /// Row at the top and the offset into it: changes only when the list moves.
    pub position: (usize, Pixels),
    /// Pixels scrolled from the top.
    pub top: f32,
    pub scrolled_up: bool,
    pub gliding: bool,
    pub ends: u64,
}

impl Resting {
    /// What this frame's scroll state asks of the composer: `Some(true)` rest,
    /// `Some(false)` wake. `allowed`: the composer may rest now.
    fn observe(&mut self, scroll: Scroll, allowed: bool, now: Instant) -> Option<bool> {
        let jumped = std::mem::replace(&mut self.ends, scroll.ends) != scroll.ends;
        let moved = self
            .position
            .replace(scroll.position)
            .is_some_and(|last| last != scroll.position);
        let delta = scroll.top - std::mem::replace(&mut self.top, scroll.top);
        if self.on {
            // Back at the end by scrolling down (not by the taller list the resting
            // composer leaves, which only scrolls it up).
            let ended = moved && delta > 0. && !scroll.scrolled_up;
            return (jumped || ended || !allowed).then_some(false);
        }
        // New output followed at the end, a glide or a jump moved it, not the reader.
        if !moved || scroll.gliding || jumped {
            return None;
        }
        if self
            .last_move
            .is_none_or(|last| now.saturating_duration_since(last) > MOVEMENT_GAP)
        {
            self.travel = 0.;
            self.suppressed = false;
        }
        self.last_move = Some(now);
        if self.suppressed || !scroll.scrolled_up || !allowed {
            self.travel = 0.;
            return None;
        }
        self.travel += delta.abs();
        if self.travel < THRESHOLD {
            return None;
        }
        self.travel = 0.;
        Some(true)
    }
}

impl ChatView {
    /// From render: follows the conversation's scroll position, whatever moved it.
    pub(super) fn sync_resting(&mut self, cx: &mut Context<Self>) {
        let scroller = self.rows.scroller.read(cx);
        let scroll = Scroll {
            position: scroller.position(),
            top: scroller.scroll_top(),
            scrolled_up: scroller.is_scrolled_up(),
            gliding: scroller.gliding(),
            ends: scroller.ends(),
        };
        let allowed = self.can_rest(cx);
        if let Some(on) = self.resting.observe(scroll, allowed, Instant::now()) {
            self.set_resting(on, cx);
        }
    }

    /// Wheeling down at the very end, where the list no longer moves, wakes it too.
    pub(super) fn composer_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let down = event.delta.pixel_delta(px(16.)).y < px(0.);
        if self.resting.on && down && !self.rows.scroller.read(cx).is_scrolled_up() {
            self.set_resting(false, cx);
        }
    }

    /// The text changed or was clicked: the full composer comes back, and the rest of a
    /// running scroll leaves it so.
    pub(super) fn wake_composer(&mut self, cx: &mut Context<Self>) {
        if self
            .resting
            .last_move
            .is_some_and(|last| Instant::now().saturating_duration_since(last) <= MOVEMENT_GAP)
        {
            self.resting.suppressed = true;
        }
        self.set_resting(false, cx);
    }

    /// Only a plain one-line draft rests; requests, edits, suggestions, shell mode
    /// and problems keep the full composer.
    fn can_rest(&self, cx: &App) -> bool {
        self.threads.is_empty()
            && !self.rows.rows.is_empty()
            && self.decision.active().is_none()
            && self.editing.is_none()
            && self.suggest.as_ref().is_none_or(|s| !s.open)
            && self.shell_error.is_none()
            && !self.shell_mode(cx)
            && !self.goal_mode()
            && !self.composer.read(cx).value().contains('\n')
            && self.attachment_errors(cx).is_none()
    }

    fn set_resting(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.resting.on == on {
            return;
        }
        let now = Instant::now();
        let resting = &mut self.resting;
        resting.on = on;
        resting.from = resting.height(now).unwrap_or(resting.measured.get());
        resting.progress = Transition::new(0., now);
        resting
            .progress
            .retarget(1., presets::RESIZE, now, motion::policy(cx));
        self.composer.update(cx, |state, cx| {
            if on {
                state.set_auto_grow(1, 1, cx)
            } else {
                state.set_auto_grow(1, 8, cx)
            }
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn at(row: usize, top: f32, scrolled_up: bool) -> Scroll {
        Scroll {
            position: (row, px(top % 50.)),
            top,
            scrolled_up,
            gliding: false,
            ends: 0,
        }
    }

    /// Feeds frames 10 ms apart; returns the composer's state after each.
    fn run(resting: &mut Resting, start: Instant, frames: &[Scroll]) -> Vec<bool> {
        frames
            .iter()
            .enumerate()
            .map(|(i, scroll)| {
                let now = start + Duration::from_millis(10 * i as u64);
                if let Some(on) = resting.observe(*scroll, true, now) {
                    resting.on = on;
                }
                resting.on
            })
            .collect()
    }

    #[test]
    fn any_reader_scroll_of_24_px_rests_and_scrolling_back_to_the_end_wakes() {
        let start = Instant::now();
        let mut resting = Resting::new(start);
        // At the end, then up in 10 px steps (wheel, touchpad or scrollbar alike).
        let up = [
            at(20, 1000., false),
            at(19, 990., true),
            at(19, 980., true),
            at(19, 970., true),
        ];
        assert_eq!(run(&mut resting, start, &up), [false, false, false, true]);
        // The taller list of the resting composer clamps it up: still resting.
        let later = start + Duration::from_secs(1);
        assert_eq!(run(&mut resting, later, &[at(19, 940., false)]), [true]);
        // Down to the end wakes it.
        let back = [at(19, 960., true), at(20, 1000., false)];
        let later = start + Duration::from_secs(2);
        assert_eq!(run(&mut resting, later, &back), [true, false]);
    }

    #[test]
    fn output_at_the_end_glides_and_jumps_never_rest() {
        let start = Instant::now();
        let mut resting = Resting::new(start);
        // Following new output: the list moves but is never scrolled up.
        let following: Vec<_> = (0..10).map(|i| at(i, i as f32 * 40., false)).collect();
        assert!(run(&mut resting, start, &following).iter().all(|on| !on));
        let gliding: Vec<_> = (0..10)
            .map(|i| Scroll {
                gliding: true,
                ..at(i, 400. - i as f32 * 40., true)
            })
            .collect();
        assert!(run(&mut resting, start, &gliding).iter().all(|on| !on));
    }

    #[test]
    fn typing_keeps_the_rest_of_a_movement_from_resting_again() {
        let start = Instant::now();
        let mut resting = Resting::new(start);
        run(
            &mut resting,
            start,
            &[at(20, 1000., false), at(19, 990., true)],
        );
        resting.suppressed = true;
        let momentum = [at(19, 970., true), at(19, 950., true), at(18, 930., true)];
        assert_eq!(
            run(&mut resting, start + Duration::from_millis(20), &momentum),
            [false, false, false]
        );
        // A new movement after a pause rests again.
        let next = [at(18, 920., true), at(18, 910., true), at(18, 900., true)];
        assert_eq!(
            run(&mut resting, start + Duration::from_secs(1), &next),
            [false, false, true]
        );
    }
}
