//! Message navigator (DEC-048): a thin rail at the conversation's left edge with a mark
//! per user message. The marks of the user's messages on screen stand out; hovering grows the
//! marks near the pointer and previews the message; clicking glides it to the top.
use super::{
    rows::{Kind, Rows, Source},
    view::ChatView,
};
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::transcript::{Body, Delivery, Transcript};
use canopy_desktop::motion::{self, Transition, presets};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{ops::Range, time::Instant};

/// Shown from this many user messages (with one there is nowhere to go).
const MIN_MESSAGES: usize = 2;
const RAIL: f32 = 24.;
const MARK: f32 = 8.;
const CURRENT: f32 = 14.;
/// How much a mark grows under the pointer, and over how many neighbours.
const GROWTH: f32 = 10.;
const REACH: f32 = 3.;

/// Hover state of the rail.
pub(super) struct Navigator {
    hovered: Option<usize>,
    /// Animated position of the pointer, in marks.
    center: Transition,
    /// 1 while the rail is hovered.
    lift: Transition,
}
impl Default for Navigator {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            hovered: None,
            center: Transition::new(0., now),
            lift: Transition::new(0., now),
        }
    }
}
impl Navigator {
    pub fn animating(&self, now: Instant) -> bool {
        self.center.is_animating(now) || self.lift.is_animating(now)
    }
}

/// A user message of the main conversation: its row and row key.
#[derive(Clone, Copy)]
struct Mark {
    row: usize,
    key: u64,
}

fn marks(rows: &Rows, transcript: Option<&Transcript>) -> Vec<Mark> {
    rows.rows
        .iter()
        .enumerate()
        .filter_map(|(row, entry)| {
            let Kind::Item(source) = entry.kind else {
                return None;
            };
            let item = rows.item(source, transcript)?;
            // A `!` command's result is no message of the user's to jump to.
            matches!(&item.body, Body::User(message) if message.delivery == Delivery::Sent
                && !message.attachments.iter().any(|a| matches!(a,
                    canopy_desktop::chat::transcript::Attachment::Shell { .. })))
            .then_some(Mark {
                row,
                key: entry.key,
            })
        })
        .collect()
}

/// Whether the user's message of `mark` is on screen: every one in view is marked, as
/// in ChatGPT (the agent's replies don't count).
fn on_screen(mark: &Mark, visible: &Range<usize>) -> bool {
    visible.contains(&mark.row)
}

/// The text of the message at `mark` and the start of the reply after it.
fn preview(
    rows: &Rows,
    transcript: Option<&Transcript>,
    marks: &[Mark],
    index: usize,
) -> (String, Option<String>) {
    let item = |source: Source| rows.item(source, transcript);
    let mark = marks[index];
    let message = match rows.rows[mark.row].kind {
        Kind::Item(source) => match item(source).map(|item| &item.body) {
            Some(Body::User(message)) => message.text.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    };
    let end = marks
        .get(index + 1)
        .map_or(rows.rows.len(), |next| next.row);
    let reply = rows.rows[mark.row + 1..end]
        .iter()
        .filter_map(|entry| match entry.kind {
            Kind::Item(source) => item(source),
            _ => None,
        })
        .filter(|item| item.parent.is_none())
        .find_map(|item| match &item.body {
            Body::Text(block) if !block.text.trim().is_empty() => Some(block.text.clone()),
            _ => None,
        });
    let short = |text: String, limit: usize| {
        let text = text.trim().replace('\n', " ");
        match text.char_indices().nth(limit) {
            Some((end, _)) => format!("{}…", &text[..end]),
            None => text,
        }
    };
    (short(message, 220), reply.map(|reply| short(reply, 160)))
}

impl ChatView {
    fn nav_transcript<'a>(&self, cx: &'a App) -> Option<&'a Transcript> {
        self.transcript(cx)
    }

    /// The rail, over the conversation's left edge (main conversation only).
    pub(super) fn navigator(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        if !self.threads.is_empty() {
            return None;
        }
        let marks = marks(&self.rows, self.nav_transcript(cx));
        if marks.len() < MIN_MESSAGES {
            self.navigator.hovered = None;
            return None;
        }
        let now = Instant::now();
        let visible = self.rows.scroller.read(cx).visible_rows();
        // Marks share the height left for them, a little apart when there is room.
        let room = f32::from(window.viewport_size().height) - 240.;
        let step = (room / marks.len() as f32).clamp(3., 8.);
        let center = self.navigator.center.value(now);
        let lift = self.navigator.lift.value(now);
        let hovered = self.navigator.hovered;
        let card = hovered
            .filter(|&index| index < marks.len())
            .map(|index| preview(&self.rows, self.nav_transcript(cx), &marks, index));
        let marks_view = marks.iter().enumerate().map(|(index, mark)| {
            let near = (1. - (index as f32 - center).abs() / REACH).max(0.) * lift;
            let current = on_screen(mark, &visible);
            let base = if current { CURRENT } else { MARK };
            let width = base + GROWTH * near;
            let color = if current {
                t::text()
            } else if hovered == Some(index) {
                t::secondary()
            } else {
                t::muted()
            };
            let row = mark.row;
            div()
                .id(("chat-nav", mark.key))
                .w(px(RAIL))
                .h(px(step))
                .flex()
                .items_center()
                .cursor_pointer()
                .child(div().h(px(2.)).w(px(width)).rounded(px(1.)).bg(color))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.hover_mark(index, *hovered, cx)
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let policy = motion::policy(cx);
                    this.rows
                        .scroller
                        .update(cx, |scroller, cx| scroller.glide_to(row, policy, cx));
                }))
                .when(hovered == Some(index), |mark| {
                    mark.children(card.clone().map(|(message, reply)| {
                        deferred(
                            anchored()
                                .offset(point(px(RAIL + 8.), px(-12.)))
                                .snap_to_window_with_margin(px(8.))
                                .child(preview_card(message, reply)),
                        )
                        .with_priority(1)
                    }))
                })
        });
        Some(
            div()
                .absolute()
                .left(px(4.))
                .top_0()
                .bottom_0()
                // Centred in the conversation's height.
                .flex()
                .flex_col()
                .justify_center()
                .children(marks_view),
        )
    }

    fn hover_mark(&mut self, index: usize, hovered: bool, cx: &mut Context<Self>) {
        let now = Instant::now();
        let policy = motion::policy(cx);
        if hovered {
            // Entering the rail starts at the mark; moving along it glides.
            if self.navigator.hovered.is_none() && !self.navigator.lift.is_animating(now) {
                self.navigator.center = Transition::new(index as f32, now);
            } else {
                self.navigator
                    .center
                    .retarget(index as f32, presets::STATE_CHANGE, now, policy);
            }
            self.navigator.hovered = Some(index);
            self.navigator
                .lift
                .retarget(1., presets::STATE_CHANGE, now, policy);
        } else if self.navigator.hovered == Some(index) {
            self.navigator.hovered = None;
            self.navigator
                .lift
                .retarget(0., presets::STATE_CHANGE, now, policy);
        }
        cx.notify();
    }

    /// ⌥⌘↑ / ⌥⌘↓: the previous or next user message.
    pub(super) fn jump_message(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !self.threads.is_empty() {
            return;
        }
        let marks = marks(&self.rows, self.nav_transcript(cx));
        let top = self.rows.scroller.read(cx).top_row();
        let target = if forward {
            marks.iter().find(|mark| mark.row > top)
        } else {
            marks.iter().rev().find(|mark| mark.row < top)
        };
        if let Some(mark) = target.copied() {
            let policy = motion::policy(cx);
            self.rows
                .scroller
                .update(cx, |scroller, cx| scroller.glide_to(mark.row, policy, cx));
        } else if forward {
            self.rows
                .scroller
                .update(cx, |scroller, cx| scroller.scroll_to_end(cx));
        }
    }
}

fn preview_card(message: String, reply: Option<String>) -> Div {
    column()
        .occlude()
        .w(px(300.))
        .p(px(10.))
        .gap(px(6.))
        .rounded(px(10.))
        .border_1()
        .border_color(t::control_border())
        .bg(t::elevated())
        .shadow_md()
        .child(
            div()
                .text_size(px(13.))
                .text_color(t::text())
                .line_clamp(3)
                .child(message),
        )
        .children(reply.map(|reply| {
            div()
                .text_size(px(12.))
                .text_color(t::muted())
                .line_clamp(2)
                .child(reply)
        }))
}
