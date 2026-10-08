//! Quiet, in-app result feedback using Canopy motion and surfaces.
use super::{components::toast as toast_ui, theme as t};
mod state;
use canopy_desktop::motion::{self, Presence, Transition, presets};
use gpui_kit::{base::ToastTransitionStatus, *};
use state::{Countdown, enqueue};
use std::{collections::VecDeque, time::Instant};

pub struct ToastHost {
    messages: VecDeque<SharedString>,
    promoted: bool,
    stack: Transition,
    presence: Presence,
    open: bool,
    hovered: bool,
    timeout: Option<Task<()>>,
    generation: u64,
    countdown: Countdown,
    drag_start: Option<f32>,
    drag: Transition,
}
impl ToastHost {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            promoted: false,
            stack: Transition::new(0., Instant::now()),
            presence: Presence::new(false, presets::POPOVER, Instant::now()),
            open: false,
            hovered: false,
            timeout: None,
            generation: 0,
            countdown: Countdown::new(),
            drag_start: None,
            drag: Transition::new(0., Instant::now()),
        }
    }
    /// Queue lightweight results in arrival order. Only the active toast expires.
    /// At most ten results are retained; overflow replaces the newest pending one.
    pub fn show(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        let message = message.into();
        let first = self.messages.is_empty();
        enqueue(&mut self.messages, message);
        let now = Instant::now();
        self.update_stack(now, cx);
        if first {
            self.promoted = false;
            self.countdown = Countdown::new();
            self.open = true;
            self.presence.set_open(true, now, motion::policy(cx));
            self.schedule(cx);
        }
        cx.notify();
    }
    fn update_stack(&mut self, now: Instant, cx: &App) {
        self.stack.retarget(
            self.messages.len().saturating_sub(1).min(2) as f32,
            presets::RESIZE,
            now,
            motion::policy(cx),
        );
    }
    fn schedule(&mut self, cx: &mut Context<Self>) {
        self.timeout = None;
        let now = Instant::now();
        self.countdown.pause(now);
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        if self.hovered || !self.open || self.drag_start.is_some() {
            return;
        }
        let remaining = self.countdown.resume(now);
        self.timeout = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(remaining).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && this.open && !this.hovered {
                    this.dismiss(cx);
                }
            });
        }));
    }
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.timeout = None;
        self.generation = self.generation.wrapping_add(1);
        self.open = false;
        self.countdown.pause(Instant::now());
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn finish_swipe(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(start) = self.drag_start.take() else {
            return;
        };
        cx.stop_propagation();
        let distance = x - start;
        let now = Instant::now();
        if distance.abs() >= 64. {
            self.drag.retarget(
                distance.signum() * 400.,
                presets::POPOVER.exit,
                now,
                motion::policy(cx),
            );
            self.dismiss(cx);
        } else {
            self.drag
                .retarget(0., presets::RESIZE, now, motion::policy(cx));
            self.schedule(cx);
            cx.notify();
        }
    }
}
impl Render for ToastHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        if !self.open && !self.presence.is_animating(now) && !self.messages.is_empty() {
            self.messages.pop_front();
            self.drag_start = None;
            self.drag = Transition::new(0., now);
            if self.messages.is_empty() {
                self.hovered = false;
            }
            self.update_stack(now, cx);
            if !self.messages.is_empty() {
                self.promoted = true;
                self.countdown = Countdown::new();
                self.open = true;
                self.presence = Presence::new(false, presets::POPOVER, now);
                self.presence.set_open(true, now, motion::policy(cx));
                self.schedule(cx);
            }
        }
        let progress = self.presence.progress(now);
        let animated = self.presence.is_animating(now);
        let stack_depth = self.stack.value(now);
        let drag = self.drag.value(now);
        let elapsed = self.countdown.progress(now);
        motion::request_frame(
            window,
            animated
                || self.stack.is_animating(now)
                || self.drag.is_animating(now)
                || (self.open && self.countdown.is_running()),
        );
        let width = (f32::from(window.viewport_size().width) - 24.).clamp(0., 360.);
        div()
            .absolute()
            .right(px(12.))
            .bottom(px(t::STATUS + 12.))
            .w(px(width))
            .children((1..=2).rev().filter_map(|depth| {
                self.messages.get(depth).map(|_| {
                    let visibility = (stack_depth - (depth - 1) as f32).clamp(0., 1.);
                    let promotion_offset = if self.promoted && self.open {
                        1. - progress
                    } else {
                        0.
                    };
                    let inset = 8. * (depth as f32 + promotion_offset);
                    toast_ui::back_card(inset, visibility)
                })
            }))
            .children(self.messages.front().cloned().map(|message| {
                toast_ui::card(
                    message,
                    elapsed,
                    toast_ui::close_button(self.hovered, self.open).on_click(cx.listener(
                        |this, _, _, cx| {
                            cx.stop_propagation();
                            this.dismiss(cx);
                        },
                    )),
                )
                .transition_status(if !self.open {
                    ToastTransitionStatus::Ending
                } else if animated {
                    ToastTransitionStatus::Starting
                } else {
                    ToastTransitionStatus::Present
                })
                .flex()
                .items_center()
                .min_w_0()
                .occlude()
                .w(px(if self.promoted && self.open {
                    width - 16. * (1. - progress)
                } else {
                    width
                }))
                .relative()
                .left(px(drag
                    + if self.promoted && self.open {
                        8. * (1. - progress)
                    } else {
                        0.
                    }))
                .top(px(if self.promoted && self.open {
                    -8. * (1. - progress)
                } else {
                    motion::distance::BASE * (1. - progress)
                }))
                .opacity(
                    (if self.promoted && self.open {
                        0.8 + 0.2 * progress
                    } else {
                        progress
                    }) * (1. - drag.abs() / width.max(1.)).clamp(0., 1.),
                )
                .on_hover(cx.listener(|this, over, _, cx| {
                    this.hovered = *over;
                    this.schedule(cx);
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        if !this.open {
                            return;
                        }
                        cx.stop_propagation();
                        this.drag_start =
                            Some(f32::from(event.position.x) - this.drag.value(Instant::now()));
                        this.schedule(cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    if let Some(start) = this.drag_start
                        && event.pressed_button == Some(MouseButton::Left)
                    {
                        this.drag =
                            Transition::new(f32::from(event.position.x) - start, Instant::now());
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseUpEvent, _, cx| {
                        this.finish_swipe(f32::from(event.position.x), cx);
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseUpEvent, _, cx| {
                        this.finish_swipe(f32::from(event.position.x), cx);
                    }),
                )
            }))
    }
}
