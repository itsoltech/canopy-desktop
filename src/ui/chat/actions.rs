//! Quiet actions under a message (DEC-042): copy, edit and retry, shown on hover with
//! the turn's numbers. Hovering anywhere on the message reveals them.
use super::{rows::Row, tools, view::ChatView};
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{
    presentation,
    transcript::{ItemId, TurnEnd},
};
use gpui_kit::component::{IconName, button::Button};
use gpui_kit::*;
use std::time::{Duration, Instant};

/// Hover group of a message row; actions inside it show while it is hovered.
pub(super) const MESSAGE_GROUP: &str = "chat-message";
/// How long Copy shows its check mark.
const COPIED: Duration = Duration::from_millis(1500);

impl ChatView {
    pub(super) fn copy_text(&mut self, key: u64, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = Some((key, Instant::now()));
        // One redraw when the check mark goes back to the copy icon.
        self.copied_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn copied(&self, key: u64) -> bool {
        self.copied
            .is_some_and(|(copied, at)| copied == key && at.elapsed() < COPIED)
    }

    /// Sends the last message again as a new turn.
    pub(super) fn retry(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        let Some(text) = session.retry_text() else {
            return;
        };
        let lines = session.submit(&text);
        self.write(lines);
        self.after_change(cx);
    }

    /// Nothing runs, waits or is queued: a message can be sent again or edited.
    pub(super) fn settled(&self) -> bool {
        !self.is_working()
            && self.decision.active().is_none()
            && self.editing.is_none()
            && self.session.as_ref().is_some_and(|s| s.queue().is_empty())
    }
}

/// Hidden until the message is hovered, or the button itself has keyboard focus.
fn action(id: impl Into<ElementId>, name: IconName, label: &'static str) -> Button {
    icon_button(id, name, label)
        .size(px(24.))
        .min_w(px(24.))
        .max_w(px(24.))
        .min_h(px(24.))
        .max_h(px(24.))
        .rounded(px(6.))
        .opacity(0.)
        .group_hover(MESSAGE_GROUP, |style| style.opacity(1.))
        .focus_visible(|style| style.opacity(1.).bg(t::hover()))
}

fn copy_button(
    view: &Entity<ChatView>,
    this: &ChatView,
    key: u64,
    id: &'static str,
    text: String,
) -> Button {
    let copied = this.copied(key);
    let view = view.downgrade();
    action(
        (id, key),
        if copied {
            IconName::Check
        } else {
            IconName::Copy
        },
        if copied { "Copied" } else { "Copy" },
    )
    .on_click(move |_, _, cx| {
        let _ = view.update(cx, |this, cx| this.copy_text(key, text.clone(), cx));
    })
}

/// The row under a message; its controls show on hover (and keyboard focus).
fn hover_row() -> Div {
    row().h(px(28.)).gap(px(2.))
}

/// Under the user's message: Copy, and Edit while nothing runs.
pub(super) fn user_actions(
    view: &Entity<ChatView>,
    this: &ChatView,
    entry: &Row,
    item: ItemId,
    text: &str,
    editable: bool,
) -> Div {
    let key = entry.key;
    hover_row()
        .child(copy_button(
            view,
            this,
            key,
            "chat-copy-user",
            text.to_owned(),
        ))
        .children(editable.then(|| {
            let view = view.downgrade();
            action(("chat-edit", key), IconName::Copy, "Edit message")
                .icon(custom_icon("pencil"))
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.start_edit(item, window, cx));
                })
        }))
}

/// Under a turn's reply: Copy, Retry on the last one, and the turn's numbers.
pub(super) fn reply_actions(
    view: &Entity<ChatView>,
    this: &ChatView,
    entry: &Row,
    end: Option<&TurnEnd>,
    text: &str,
    last: bool,
) -> Div {
    let key = entry.key;
    let billing = this.session.as_ref().is_some_and(|s| s.api_billing());
    hover_row()
        .child(copy_button(
            view,
            this,
            key,
            "chat-copy-reply",
            text.to_owned(),
        ))
        .children((last && this.settled()).then(|| {
            let view = view.downgrade();
            action(("chat-retry", key), IconName::RotateCw, "Retry").on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| this.retry(cx));
            })
        }))
        .children(end.map(|end| {
            div()
                .id(("chat-turn-numbers", key))
                .opacity(0.)
                .group_hover(MESSAGE_GROUP, |style| style.opacity(1.))
                .pl(px(8.))
                .text_size(px(12.))
                .text_color(t::faint())
                .child(turn_numbers(end, billing))
        }))
}

/// "2m 41s · 12k in · 1.2k out" (+ the cost when the account pays per token).
fn turn_numbers(end: &TurnEnd, billing: bool) -> String {
    let mut parts = vec![];
    if let Some(ms) = end.duration_ms {
        parts.push(tools::elapsed(ms as f32 / 1000.));
    }
    if let Some(usage) = end.usage {
        let input =
            usage.input_tokens + usage.cache_read_input_tokens + usage.cache_creation_input_tokens;
        parts.push(format!("{} in", presentation::tokens(input)));
        parts.push(format!("{} out", presentation::tokens(usage.output_tokens)));
    }
    if billing && let Some(cost) = end.cost_usd {
        parts.push(format!("${cost:.2}"));
    }
    parts.join(" · ")
}

/// The context indicator next to the pickers: how full the model's context window is.
pub(super) fn context_ring(this: &ChatView) -> Option<Stateful<Div>> {
    let usage = this.session.as_ref()?.context()?;
    // The CLI compacts once the free space before its buffer runs out: the ring
    // fills toward that point.
    let limit = usage.max.saturating_sub(usage.buffer).max(1);
    let percent = (usage.used as f32 / limit as f32 * 100.).clamp(0., 100.);
    let color = if percent >= 95. {
        t::red()
    } else if percent >= 80. {
        t::yellow()
    } else {
        t::secondary()
    };
    let tip: SharedString = format!(
        "{} / {} tokens of context used · Claude compacts the conversation near {}",
        presentation::tokens(usage.used),
        presentation::tokens(usage.max),
        presentation::tokens(limit)
    )
    .into();
    Some(
        div()
            .id("chat-context")
            .flex_shrink_0()
            .size(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(13.))
            .keyboard(format!(
                "{percent:.0}% of the context before compaction used"
            ))
            .hover(|style| style.bg(t::hover()))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
            })
            .child(
                gpui_kit::component::progress::ProgressCircle::new("chat-context-ring")
                    .value(percent)
                    .color(color)
                    .accessibility_label(format!("{percent:.0}% of the context window used"))
                    .size(px(14.)),
            ),
    )
}

/// Seconds left until `until` (unix seconds), while it is in the future.
pub(super) fn seconds_left(until: Option<u64>) -> Option<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    until?.checked_sub(now).filter(|left| *left > 0)
}

/// Time left until `until` (unix seconds), as "42s" / "12 min".
pub(super) fn countdown(until: u64) -> Option<String> {
    let left = seconds_left(Some(until))?;
    Some(if left < 90 {
        format!("{left}s")
    } else {
        format!("{} min", left.div_ceil(60))
    })
}

/// When a limit resets, in local time ("15:00").
pub(super) fn local_time(until: u64) -> Option<String> {
    let at = chrono::DateTime::from_timestamp(until as i64, 0)?;
    Some(at.with_timezone(&chrono::Local).format("%H:%M").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_desktop::chat::transcript::{Outcome, Usage};
    use core::prelude::v1::test;

    #[test]
    fn numbers_show_cost_only_when_billed() {
        let end = TurnEnd {
            outcome: Outcome::Completed,
            usage: Some(Usage {
                input_tokens: 2_000,
                output_tokens: 1_200,
                cache_read_input_tokens: 10_000,
                cache_creation_input_tokens: 400,
            }),
            cost_usd: Some(0.214),
            duration_ms: Some(161_000),
            resume_at: None,
        };
        assert_eq!(turn_numbers(&end, false), "2m 41s · 12k in · 1.2k out");
        assert_eq!(
            turn_numbers(&end, true),
            "2m 41s · 12k in · 1.2k out · $0.21"
        );
    }
}
