//! Tray on top of the composer: messages held until the agent has finished everything,
//! and subagents the agent is still waiting for after its turn ended.
use super::{render::centered, view::ChatView};
use crate::ui::{components::*, theme as t};
use canopy_desktop::{
    chat::session::{Queued, QueuedId},
    motion::{MotionPolicy, Presence, PresenceSpec, presets},
};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
    spinner::Spinner,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{collections::VecDeque, time::Instant};

const ROW: f32 = 32.;
/// Longer trays scroll instead of pushing the conversation away.
const VISIBLE_ROWS: f32 = 4.;
const ENTRY: PresenceSpec = PresenceSpec {
    enter: presets::STATE_CHANGE,
    exit: presets::STATE_CHANGE,
};

/// A background subagent the finished turn is waiting for.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Waiting {
    pub call: String,
    pub title: String,
    pub activity: String,
}

#[derive(Clone, PartialEq)]
enum Line {
    Waiting(Waiting),
    Queued(Queued),
}
impl Line {
    fn same(&self, other: &Line) -> bool {
        match (self, other) {
            (Line::Waiting(a), Line::Waiting(b)) => a.call == b.call,
            (Line::Queued(a), Line::Queued(b)) => a.id == b.id,
            _ => false,
        }
    }
}

struct Entry {
    line: Line,
    presence: Presence,
    leaving: bool,
}

/// View state of the tray: current lines plus removed ones until their exit ends.
#[derive(Default)]
pub(super) struct QueueBar {
    entries: Vec<Entry>,
}

impl QueueBar {
    /// Mirrors the waiting subagents (first) and the session queue (oldest first).
    pub fn sync(&mut self, waiting: &[Waiting], queue: &VecDeque<Queued>, policy: MotionPolicy) {
        let now = Instant::now();
        let lines: Vec<Line> = waiting
            .iter()
            .cloned()
            .map(Line::Waiting)
            .chain(queue.iter().cloned().map(Line::Queued))
            .collect();
        for entry in &mut self.entries {
            match lines.iter().find(|line| line.same(&entry.line)) {
                Some(line) => {
                    entry.line = line.clone();
                    if entry.leaving {
                        entry.leaving = false;
                        entry.presence.set_open(true, now, policy);
                    }
                }
                None if !entry.leaving => {
                    entry.leaving = true;
                    entry.presence.set_open(false, now, policy);
                }
                None => {}
            }
        }
        // Follow the session's order (reordering by drag and drop); entries on their
        // way out keep their place among the others.
        let rank = |entry: &Entry| lines.iter().position(|line| line.same(&entry.line));
        let mut ordered: Vec<(usize, Entry)> = vec![];
        let mut last = 0;
        for entry in self.entries.drain(..) {
            let key = match rank(&entry) {
                Some(position) => {
                    last = position;
                    position
                }
                None => last,
            };
            ordered.push((key, entry));
        }
        ordered.sort_by_key(|(key, _)| *key);
        self.entries = ordered.into_iter().map(|(_, entry)| entry).collect();
        for line in lines {
            if !self.entries.iter().any(|e| e.line.same(&line)) {
                let mut presence = Presence::new(false, ENTRY, now);
                presence.set_open(true, now, policy);
                // Waiting lines stay above queued messages.
                let at = match line {
                    Line::Waiting(_) => self
                        .entries
                        .iter()
                        .position(|e| matches!(e.line, Line::Queued(_)))
                        .unwrap_or(self.entries.len()),
                    Line::Queued(_) => self.entries.len(),
                };
                self.entries.insert(
                    at,
                    Entry {
                        line,
                        presence,
                        leaving: false,
                    },
                );
            }
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.entries.iter().any(|e| e.presence.is_animating(now))
    }

    /// Unmounts entries whose exit has finished.
    fn prune(&mut self, now: Instant) {
        self.entries
            .retain(|e| !e.leaving || e.presence.is_animating(now));
    }
}

impl ChatView {
    /// Sits on the composer's top edge, slightly narrower, like a tab behind it.
    pub(super) fn queue_bar(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let now = Instant::now();
        self.queue.prune(now);
        if self.queue.entries.is_empty() {
            return None;
        }
        let held = self.session.as_ref().is_some_and(|s| s.queue_held());
        let first = self
            .queue
            .entries
            .iter()
            .filter_map(|e| match &e.line {
                Line::Queued(q) if !e.leaving && q.steering.is_none() => Some(q.id),
                _ => None,
            })
            .next();
        let rows: Vec<_> = self
            .queue
            .entries
            .iter()
            .map(|entry| match &entry.line {
                Line::Queued(queued) => {
                    queued_row(entry, queued, first == Some(queued.id), held, now, cx)
                }
                Line::Waiting(waiting) => waiting_row(entry, waiting, now, cx),
            })
            .collect();
        Some(
            centered(
                column()
                    .id("chat-queue")
                    .mx(px(14.))
                    .px(px(6.))
                    .py(px(3.))
                    .max_h(px(ROW * VISIBLE_ROWS + 6.))
                    .overflow_y_scroll()
                    .rounded_t(px(12.))
                    .border_1()
                    .border_b_0()
                    .border_color(t::border())
                    .bg(t::composer_surface())
                    .when(held, |tray| tray.child(paused_row(cx)))
                    .children(rows)
                    .nested_scroll(),
            )
            // Cancels the composer island's top padding: the tray meets its border,
            // without covering the translucent surface.
            .mb(px(-4.)),
        )
    }
}

fn shell(entry: &Entry, now: Instant, content: impl IntoElement) -> Div {
    let progress = entry.presence.progress(now);
    div()
        .h(px(ROW * progress))
        .flex_shrink_0()
        .overflow_hidden()
        .opacity(progress)
        .child(row().h(px(ROW)).px(px(6.)).gap(px(8.)).child(content))
}

/// Stop paused the queue; Resume sends it again one message per turn.
fn paused_row(cx: &Context<ChatView>) -> Div {
    row()
        .flex_shrink_0()
        .h(px(ROW))
        .px(px(6.))
        .gap(px(8.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.))
                .text_color(t::muted())
                .child("Queue paused after Stop"),
        )
        // Drawn like a file pill so it stands out, at the height of the other actions.
        .child(
            row()
                .id("chat-queue-resume")
                .flex_shrink_0()
                .h(px(22.))
                .px(px(7.))
                .gap(px(4.))
                .rounded(px(6.))
                .bg(t::accent().opacity(0.08))
                .border_1()
                .border_color(t::accent().opacity(0.14))
                .hover(|style| style.bg(t::accent().opacity(0.14)))
                .active(|style| style.bg(t::accent().opacity(0.2)))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(t::text())
                .child(icon(IconName::Play).size(px(11.)).text_color(t::accent()))
                .child("Resume")
                .tooltip(|window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(
                        "Let the agent finish the stopped work, then send the queue",
                    )
                    .build(window, cx)
                })
                .on_click(cx.listener(|this, _, _, cx| this.resume_queue(cx))),
        )
}

fn status_dot(color: Hsla) -> Div {
    div().size(px(6.)).flex_shrink_0().rounded_full().bg(color)
}

/// `first` is the entry ⌘⏎ steers.
fn queued_row(
    entry: &Entry,
    queued: &Queued,
    first: bool,
    held: bool,
    now: Instant,
    cx: &Context<ChatView>,
) -> Div {
    let id: QueuedId = queued.id;
    let key = id.0;
    let active = !entry.leaving;
    // A `!` command's result waits like a message but has no text to edit.
    let ran = queued.attachments.iter().find_map(|a| match a {
        canopy_desktop::chat::transcript::Attachment::Shell { command, .. } => Some(command),
        _ => None,
    });
    let actions: AnyElement = if queued.steering.is_some() {
        row()
            .gap(px(6.))
            .text_size(px(11.))
            .text_color(t::muted())
            .child(Spinner::new().xsmall().color(t::muted()))
            .child("Sending…")
            .into_any_element()
    } else {
        row()
            .flex_shrink_0()
            .gap(px(2.))
            .child(
                action(("chat-queue-steer", key), "Steer")
                    .tooltip(if first {
                        "Send queued message now (⌘⏎)"
                    } else {
                        "Send queued message now"
                    })
                    .disabled(!active)
                    .on_click(cx.listener(move |this, _, _, cx| this.steer(id, cx))),
            )
            .child(
                action(("chat-queue-edit", key), "Edit")
                    .tooltip("Move back to the message field")
                    .disabled(!active || ran.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| this.edit(id, window, cx))),
            )
            .child(
                icon_button(("chat-queue-remove", key), IconName::Close, "Remove")
                    .disabled(!active)
                    .on_click(cx.listener(move |this, _, _, cx| this.remove(id, cx))),
            )
            .into_any_element()
    };
    let text = match ran {
        Some(command) => format!("Output of !{command}"),
        None => queued.text.clone(),
    };
    let tip: SharedString = if held {
        format!("{text}\n\nQueued — paused after Stop; Resume, send or steer to continue").into()
    } else {
        format!("{text}\n\nQueued — sent when the agent finishes").into()
    };
    let movable = active && queued.steering.is_none();
    shell(
        entry,
        now,
        row()
            .flex_1()
            .min_w_0()
            .gap(px(6.))
            // Drag handle: drop on another message to put this one in front of it.
            .child(
                div()
                    .id(("chat-queue-grip", key))
                    .flex_shrink_0()
                    .size(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(movable, |grip| {
                        grip.cursor_grab()
                            .hover(|style| style.text_color(t::secondary()))
                            .on_drag(QueuedDrag(id), |_, _, _, cx| {
                                cx.new(|_| DragPreview("Move message"))
                            })
                    })
                    .text_color(if held { t::faint() } else { t::muted() })
                    .child(
                        custom_icon("grip-vertical")
                            .size(px(12.))
                            .text_color(t::faint()),
                    ),
            )
            .child(
                div()
                    .id(("chat-queue-text", key))
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .text_color(t::text())
                    .truncate()
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                    })
                    .child(single_line(&text)),
            )
            .child(actions),
    )
    .drag_over::<QueuedDrag>(|style, _, _, _| style.border_t_2().border_color(t::accent()))
    .on_drop(cx.listener(move |this, drag: &QueuedDrag, _, cx| this.move_queued(drag.0, id, cx)))
}

/// A queued message being dragged in the tray.
#[derive(Clone, Copy)]
struct QueuedDrag(QueuedId);

fn waiting_row(entry: &Entry, waiting: &Waiting, now: Instant, cx: &Context<ChatView>) -> Div {
    let active = !entry.leaving;
    let call = waiting.call.clone();
    let key = format!("chat-waiting-{}", waiting.call);
    shell(
        entry,
        now,
        row()
            .flex_1()
            .min_w_0()
            .gap(px(8.))
            .child(status_dot(t::accent()))
            .child(
                Button::new(ElementId::Name(key.clone().into()))
                    .ghost()
                    .compact()
                    .flex_1()
                    .min_w_0()
                    .h(px(26.))
                    .px(px(4.))
                    .rounded(px(6.))
                    .border_0()
                    .accessibility_label(format!("Open subagent {}", waiting.title))
                    .tooltip("Open the subagent's conversation")
                    .disabled(!active)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_thread(call.clone(), None, window, cx)
                    }))
                    .child(
                        row()
                            .w_full()
                            .min_w_0()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .max_w(relative(0.6))
                                    .truncate()
                                    .text_size(px(13.))
                                    .text_color(t::text())
                                    .child(format!("Waiting on subagent {}", waiting.title)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(t::muted())
                                    .child(waiting.activity.clone()),
                            ),
                    ),
            )
            .child(
                div()
                    .id(ElementId::Name(format!("{key}-info").into()))
                    .flex_shrink_0()
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(
                            "The agent's turn ended; it continues when this subagent reports back.",
                        )
                        .build(window, cx)
                    })
                    .child(icon(IconName::Info).text_color(t::faint())),
            )
            .child(
                action(ElementId::Name(format!("{key}-stop").into()), "Stop")
                    .tooltip("Stop the agent and its subagents (⌘.)")
                    .disabled(!active)
                    .on_click(cx.listener(|this, _, _, cx| this.interrupt(cx))),
            ),
    )
}

fn action(id: impl Into<ElementId>, label: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .compact()
        .h(px(22.))
        .px(px(8.))
        .rounded(px(6.))
        .text_size(px(12.))
        .text_color(t::secondary())
        .label(label)
}

fn single_line(text: &str) -> String {
    canopy_desktop::chat::presentation::single_line(text, 400)
}
