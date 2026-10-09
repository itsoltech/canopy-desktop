//! The conversation's virtual list: follows new output, offers "Jump to latest", fades
//! the bottom edge while scrolled up (as gpui-kit's MessageScroller), and also tells
//! where the reader is and glides to a chosen row (the message navigator, DEC-050).
use crate::ui::theme as t;
use canopy_desktop::motion::{self, MotionPolicy, Transition, presets};
use gpui_kit::base::motion::{Transition as Fade, transition};
use gpui_kit::component::{
    IconName,
    button::{Button, ButtonVariants},
    scroll::{ScrollableElement, ScrollableMask},
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{ops::Range, time::Duration, time::Instant};

const OVERDRAW: Pixels = px(400.);
const FADE: Duration = Duration::from_millis(200);
/// A glide farther than this many viewport heights first jumps near its target.
const FAR: f32 = 2.;

/// A scroll toward a row: jumps close when far, then eases the rest.
struct Glide {
    target: usize,
    /// Pixels to travel, known once the target was laid out.
    distance: Option<f32>,
    applied: f32,
    progress: Transition,
}

pub(super) struct ChatScrollerState {
    list: ListState,
    glide: Option<Glide>,
}

impl ChatScrollerState {
    pub fn new(count: usize, cx: &mut Context<Self>) -> Self {
        let list = ListState::new(count, ListAlignment::Top, OVERDRAW);
        list.set_follow_mode(FollowMode::Tail);
        let state = cx.weak_entity();
        // Only the reader's own scrolling reports here: it ends a glide.
        list.set_scroll_handler(move |_, _, cx| {
            let state = state.clone();
            cx.defer(move |cx| {
                let _ = state.update(cx, |state, cx| {
                    state.glide = None;
                    cx.notify();
                });
            });
        });
        Self { list, glide: None }
    }

    pub fn is_scrolled_up(&self) -> bool {
        self.list.max_offset_for_scrollbar().y > px(0.)
            && !self.list.is_following_tail()
            && !self.list.is_scrolled_to_end().unwrap_or(false)
    }

    /// Where the list is scrolled to (a selection's Quote button goes when it moves).
    pub fn position(&self) -> (usize, Pixels) {
        let top = self.list.logical_scroll_top();
        (top.item_ix, top.offset_in_item)
    }

    /// Brings `row` into view at once (find): new output no longer pulls the view down;
    /// a row already on screen stays put, its text view reveals the line itself.
    pub fn reveal_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if row >= self.list.item_count() {
            return;
        }
        self.glide = None;
        self.list.pause_following_tail();
        let viewport = self.list.viewport_bounds();
        let shown = self.list.bounds_for_item(row).is_some_and(|bounds| {
            bounds.top() < viewport.bottom() && bounds.bottom() > viewport.top()
        });
        if !shown {
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        }
        cx.notify();
    }

    /// The first row at the top of the viewport.
    pub fn top_row(&self) -> usize {
        self.list.logical_scroll_top().item_ix
    }

    pub fn splice(&mut self, range: Range<usize>, count: usize, cx: &mut Context<Self>) {
        if range.start > range.end || range.end > self.list.item_count() {
            return;
        }
        self.list.splice(range, count);
        cx.notify();
    }

    pub fn remeasure_items(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        if range.start > range.end || range.end > self.list.item_count() {
            return;
        }
        self.list.remeasure_items(range);
        cx.notify();
    }

    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.glide = None;
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
        cx.notify();
    }

    /// Scrolls `row` to the top of the view, smoothly unless motion is reduced.
    pub fn glide_to(&mut self, row: usize, policy: MotionPolicy, cx: &mut Context<Self>) {
        if row >= self.list.item_count() {
            return;
        }
        if policy == MotionPolicy::Reduced {
            self.glide = None;
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        } else {
            let now = Instant::now();
            let mut progress = Transition::new(0., now);
            progress.retarget(1., presets::RESIZE, now, policy);
            self.glide = Some(Glide {
                target: row,
                distance: None,
                applied: 0.,
                progress,
            });
        }
        cx.notify();
    }

    pub fn gliding(&self) -> bool {
        self.glide.is_some()
    }

    /// Advances a glide by one frame (from render, before the list lays out).
    pub fn step(&mut self, now: Instant) {
        let Some(glide) = &mut self.glide else {
            return;
        };
        let viewport = self.list.viewport_bounds();
        let height = f32::from(viewport.size.height).max(1.);
        if glide.distance.is_none() {
            let measured = self
                .list
                .bounds_for_item(glide.target)
                .map(|bounds| f32::from(bounds.top() - viewport.top()));
            glide.distance = Some(match measured {
                Some(distance) if distance.abs() <= FAR * height => distance,
                // Far away or not laid out yet: jump to a viewport before it, glide the rest.
                _ => {
                    let down = glide.target > self.list.logical_scroll_top().item_ix;
                    self.list.scroll_to(ListOffset {
                        item_ix: glide.target,
                        offset_in_item: px(0.),
                    });
                    let lead = height * 0.8;
                    self.list.scroll_by(px(if down { -lead } else { lead }));
                    if down { lead } else { -lead }
                }
            });
        }
        let distance = glide.distance.unwrap_or_default();
        let reached = distance * glide.progress.value(now);
        let delta = reached - glide.applied;
        if delta != 0. {
            self.list.scroll_by(px(delta));
            glide.applied = reached;
        }
        if !glide.progress.is_animating(now) {
            self.glide = None;
        }
    }
}

type RowRenderer = Box<dyn FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static>;

/// The list element with its scrollbar, bottom fade and jump-to-latest button.
#[derive(IntoElement)]
pub(super) struct ChatScroller {
    id: ElementId,
    state: Entity<ChatScrollerState>,
    renderer: RowRenderer,
}

impl ChatScroller {
    pub fn new(
        id: impl Into<ElementId>,
        state: Entity<ChatScrollerState>,
        renderer: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            state,
            renderer: Box::new(renderer),
        }
    }
}

impl RenderOnce for ChatScroller {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let (list_state, scrolled_up, gliding) = self.state.update(cx, |state, _| {
            state.step(Instant::now());
            (state.list.clone(), state.is_scrolled_up(), state.gliding())
        });
        motion::request_frame(window, gliding);
        let jump = transition(
            (id.clone(), "jump-button"),
            if scrolled_up { 1. } else { 0. },
            Fade::new(FADE),
            window,
            cx,
        );
        let fade = transition(
            (id.clone(), "bottom-fade"),
            if scrolled_up { 1. } else { 0. },
            Fade::new(FADE),
            window,
            cx,
        );
        let mut renderer = self.renderer;
        let list = list(list_state.clone(), move |index, window, cx| {
            div()
                .w_full()
                .min_w_0()
                .child(renderer(index, window, cx))
                .into_any_element()
        })
        .size_full()
        .min_h_0()
        .pt(px(16.))
        .pb(px(24.));
        let background = t::bg();
        let viewport = div()
            .id((id.clone(), "viewport"))
            .role(Role::Log)
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(list)
            .when(fade > 0., |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(rems(3.))
                        .opacity(fade)
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(background.opacity(0.), 0.),
                            linear_color_stop(background, 1.),
                        )),
                )
            })
            .vertical_scrollbar(&list_state);
        let state = self.state.clone();
        div()
            .id(id.clone())
            .relative()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(viewport)
            .child(ScrollableMask::new(Axis::Vertical, &list_state).id(id.clone()))
            .when(jump > 0., |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(rems(0.5 + jump * 0.5))
                        .flex()
                        .justify_center()
                        .opacity(jump)
                        .child(
                            Button::new((id, "jump-to-latest"))
                                .secondary()
                                .icon(IconName::ArrowDown)
                                .tooltip("Jump to latest")
                                .rounded_full()
                                .border_1()
                                .border_color(t::border())
                                .bg(t::bg())
                                .text_color(t::text())
                                .on_click(move |_, _, cx| {
                                    state.update(cx, |state, cx| state.scroll_to_end(cx));
                                })
                                .when(!scrolled_up, |button| {
                                    gpui_kit::base::Disableable::disabled(button, true)
                                }),
                        ),
                )
            })
    }
}
