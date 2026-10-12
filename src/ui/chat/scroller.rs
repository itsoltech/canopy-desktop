//! The conversation's virtual list: follows new output, offers "Jump to latest" (as
//! gpui-kit's MessageScroller; the composer above it fades the bottom edge), and also tells
//! where the reader is and glides to a chosen row (the message navigator, DEC-050).
use crate::ui::theme as t;
use canopy_desktop::motion::{self, MotionPolicy, Transition, presets};
use gpui_kit::base::motion::{Transition as Fade, transition};
use gpui_kit::component::{
    IconName,
    button::{Button, ButtonVariants},
    scroll::{ScrollableElement, ScrollableMask, ScrollbarHandle},
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
    /// Counts jumps to the end (Jump to latest, the navigator's last stop).
    ends: u64,
    /// The list's top and bottom padding in the last frame (the composer's room).
    padding: Pixels,
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
        Self {
            list,
            glide: None,
            ends: 0,
            padding: px(0.),
        }
    }

    pub fn is_scrolled_up(&self) -> bool {
        self.padded().max() > px(0.)
            && !self.list.is_following_tail()
            && !self.list.is_scrolled_to_end().unwrap_or(false)
    }

    /// How far the list is scrolled down, in pixels.
    pub fn scroll_top(&self) -> f32 {
        f32::from(self.padded().top())
    }

    fn padded(&self) -> PaddedList {
        PaddedList {
            list: self.list.clone(),
            padding: self.padding,
        }
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

    pub fn ends(&self) -> u64 {
        self.ends
    }

    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.ends += 1;
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

/// The list's own padding above the first row and below the last (before the room the
/// composer takes over the bottom edge).
const TOP: f32 = 16.;
const BOTTOM: f32 = 24.;

/// The list as its scrollbar and wheel mask see it. `ListState`'s scrollbar figures leave
/// out the list's padding, and at the followed end report the rows' height rather than
/// the end: with the composer's room below the conversation, the mask's first wheel
/// step from the end jumped by that room, and the thumb couldn't reach the end.
#[derive(Clone)]
struct PaddedList {
    list: ListState,
    /// Top and bottom padding together.
    padding: Pixels,
}

impl PaddedList {
    /// The rows' height.
    fn content(&self) -> Pixels {
        let viewport = self.list.viewport_bounds().size.height;
        let over = self.list.max_offset_for_scrollbar().y;
        if over > px(0.) {
            return viewport + over;
        }
        // Shorter than the viewport: every row is laid out.
        let last = self.list.item_count().checked_sub(1);
        match (
            self.list.bounds_for_item(0),
            last.and_then(|last| self.list.bounds_for_item(last)),
        ) {
            (Some(first), Some(last)) => last.bottom() - first.top(),
            _ => px(0.),
        }
    }

    /// How far the list scrolls, padding included.
    fn max(&self) -> Pixels {
        (self.content() + self.padding - self.list.viewport_bounds().size.height).max(px(0.))
    }

    /// How far it is scrolled, in the list's own measure (rows above the top).
    fn top(&self) -> Pixels {
        let max = self.max();
        if self.list.is_following_tail() || self.list.is_scrolled_to_end() == Some(true) {
            return max;
        }
        (-self.list.scroll_px_offset_for_scrollbar().y).clamp(px(0.), max)
    }
}

impl ScrollbarHandle for PaddedList {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.list.viewport_bounds()
    }

    fn offset(&self) -> Point<Pixels> {
        point(px(0.), -self.top())
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        // Takes the same measure, padding included.
        self.list.set_offset_from_scrollbar(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        let viewport = self.list.viewport_bounds().size;
        size(viewport.width, viewport.height + self.max())
    }

    fn start_drag(&self) {
        self.list.scrollbar_drag_started();
    }

    fn end_drag(&self) {
        self.list.scrollbar_drag_ended();
    }
}

type RowRenderer = Box<dyn FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static>;

/// The list element with its scrollbar and jump-to-latest button.
#[derive(IntoElement)]
pub(super) struct ChatScroller {
    id: ElementId,
    state: Entity<ChatScrollerState>,
    renderer: RowRenderer,
    /// What lies over the bottom edge (the composer): the list ends above it.
    inset: Pixels,
    /// Height of that overlay as drawn now (less while the composer rests): Jump to
    /// latest sits on it.
    overlay: Pixels,
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
            inset: px(0.),
            overlay: px(0.),
        }
    }

    pub fn bottom_inset(mut self, inset: Pixels, overlay: Pixels) -> Self {
        self.inset = inset;
        self.overlay = overlay;
        self
    }
}

impl RenderOnce for ChatScroller {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let inset = self.inset;
        let overlay = self.overlay;
        let (padded, scrolled_up, gliding) = self.state.update(cx, |state, _| {
            state.step(Instant::now());
            state.padding = px(TOP + BOTTOM) + inset;
            (state.padded(), state.is_scrolled_up(), state.gliding())
        });
        motion::request_frame(window, gliding);
        let jump = transition(
            (id.clone(), "jump-button"),
            if scrolled_up { 1. } else { 0. },
            Fade::new(FADE),
            window,
            cx,
        );
        let mut renderer = self.renderer;
        let list_state = padded.list.clone();
        let list = list(list_state.clone(), move |index, window, cx| {
            div()
                .w_full()
                .min_w_0()
                .child(renderer(index, window, cx))
                .into_any_element()
        })
        .size_full()
        .min_h_0()
        .pt(px(TOP))
        .pb(px(BOTTOM) + inset);
        let viewport = div()
            .id((id.clone(), "viewport"))
            .role(Role::Log)
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(list)
            .vertical_scrollbar(&padded);
        let state = self.state.clone();
        div()
            .id(id.clone())
            .relative()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(viewport)
            .child(ScrollableMask::new(Axis::Vertical, &padded).id(id.clone()))
            .when(jump > 0., |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(overlay + window.rem_size() * (0.5 + jump * 0.5))
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    /// A followed list of 50 rows, 100 px each, with the composer's room below it.
    struct Probe {
        list: PaddedList,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .relative()
                .size_full()
                .child(
                    list(self.list.list.clone(), |_, _, _| {
                        div().h(px(100.)).into_any_element()
                    })
                    .size_full()
                    .pt(px(TOP))
                    .pb(px(BOTTOM + 200.)),
                )
                .child(ScrollableMask::new(Axis::Vertical, &self.list).id("mask"))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    #[gpui_kit::test]
    fn a_wheel_step_from_the_end_moves_by_its_own_length(cx: &mut TestAppContext) {
        let state = ListState::new(50, ListAlignment::Top, px(200.));
        state.set_follow_mode(FollowMode::Tail);
        let padded = PaddedList {
            list: state,
            padding: px(TOP + BOTTOM + 200.),
        };
        let handle = padded.clone();
        let (_, cx) = cx.add_window_view(move |_, _| Probe { list: handle });
        draw(cx);
        padded.list.scroll_to_end();
        draw(cx);
        let end = padded.top();
        assert_eq!(end, padded.max());
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(10.), px(10.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(20.))),
            ..Default::default()
        });
        draw(cx);
        assert_eq!(padded.top(), end - px(20.));
    }
}
