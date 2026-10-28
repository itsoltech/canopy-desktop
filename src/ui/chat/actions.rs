//! Quiet actions under a message (DEC-042): copy, edit and retry, shown on hover with
//! the turn's numbers. Hovering anywhere on the message reveals them.
use super::{rows::Row, tools, view::ChatView};
use crate::ui::components::hover_card::content as card;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{
    backend::ContextUsage,
    presentation,
    provider::Provider,
    transcript::{ItemId, TurnEnd},
};
use canopy_desktop::remote::proto as p;
use gpui_kit::component::{IconName, button::Button};
use gpui_kit::{prelude::FluentBuilder, *};
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
        if self.rt(cx).is_remote() {
            let draft = self.transcript(cx).and_then(|t| {
                t.items().iter().rev().find_map(|i| match &i.body {
                    canopy_desktop::chat::transcript::Body::User(m) if i.parent.is_none() => {
                        Some(canopy_desktop::chat::session::Draft {
                            text: m.text.clone(),
                            attachments: m.attachments.clone(),
                            context: m.context.clone(),
                            goal: m.goal,
                        })
                    }
                    _ => None,
                })
            });
            if let Some(draft) = draft {
                self.runtime.update(cx, |r, cx| r.retry_remote(draft, cx));
            }
            return;
        }
        let Some(draft) = self.native(cx).and_then(|session| session.retry_draft()) else {
            return;
        };
        self.with_images(draft, cx, |this, draft, cx| this.send_draft(draft, cx));
    }

    /// Nothing runs, waits or is queued: a message can be sent again or edited.
    pub(super) fn settled(&self, cx: &App) -> bool {
        !self.is_working(cx)
            && self.decision.active().is_none()
            && self.editing.is_none()
            && self.rt(cx).ready()
            && self.runtime.read(cx).queue_lines().is_empty()
    }
}

/// Hidden until the message is hovered, or the button itself has keyboard focus.
fn action(id: impl Into<ElementId>, name: IconName, label: &'static str) -> Button {
    hidden_action(icon_button(id, name, label))
}

/// `action`'s size and reveal for any icon button.
fn hidden_action(button: Button) -> Button {
    button
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
    other_agent: Option<&'static str>,
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
            // While the host checks the message, the pencil turns into a spinner and stays.
            let loading = this.edit_loading(item);
            let checking = loading.is_some_and(ButtonLoading::active);
            hidden_action(loading_icon_button(
                ("chat-edit", key),
                custom_icon("pencil"),
                "Edit message",
                loading,
            ))
            .when(checking, |button| button.opacity(1.))
            .on_click(move |_, window, cx| {
                let _ = view.update(cx, |this, cx| this.start_edit(item, window, cx));
            })
        }))
        .children(other_agent.map(|agent| {
            div()
                .id(("chat-other-agent", key))
                .opacity(0.)
                .group_hover(MESSAGE_GROUP, |style| style.opacity(1.))
                .pl(px(6.))
                .text_size(px(12.))
                .text_color(t::faint())
                .child(format!("Sent to {agent}"))
                .tooltip(|window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(
                        "Messages can be edited within the current agent's part",
                    )
                    .build(window, cx)
                })
        }))
}

/// Under a turn's reply: Copy, Retry on the last one, Fork, and the turn's numbers.
#[expect(
    clippy::too_many_arguments,
    reason = "Reply actions combine the retained view, row and turn data, copy text, last-turn state, fork source, and GPUI context."
)]
pub(super) fn reply_actions(
    view: &Entity<ChatView>,
    this: &ChatView,
    entry: &Row,
    end: Option<&TurnEnd>,
    text: &str,
    last: bool,
    fork: Option<super::rows::Source>,
    cx: &App,
) -> Div {
    let key = entry.key;
    let billing = this.native(cx).is_some_and(|s| s.api_billing());
    hover_row()
        .child(copy_button(
            view,
            this,
            key,
            "chat-copy-reply",
            text.to_owned(),
        ))
        .children((last && this.settled(cx)).then(|| {
            let view = view.downgrade();
            action(("chat-retry", key), IconName::RotateCw, "Retry").on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| this.retry(cx));
            })
        }))
        .children(fork.map(|at| {
            let view = view.downgrade();
            action(("chat-fork", key), IconName::Copy, "Fork into a new chat")
                .icon(custom_icon("git-branch"))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.fork_reply(at, cx));
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

/// How full a chat's context is, wherever its agent runs.
struct ContextReading {
    usage: Option<ContextUsage>,
    /// A ChatGPT plan's usage limit (Codex, on this computer only).
    rate: Option<canopy_desktop::chat::codex::adapter::RateLimit>,
    source: ContextSource,
}

/// Where a context reading comes from, for the card's wording.
enum ContextSource {
    /// The live session on this computer.
    Local(Provider),
    /// The last turn another computer reported; `None` while its name is unknown.
    Remote(Option<String>),
}

/// The context reading every kind of chat shares: the live Claude or Codex session,
/// the last turn a host reported for a chat on another computer. Agents that report
/// no usage (ACP today) have none, and the ring stays away.
fn context_reading(runtime: &crate::app_state::ChatRuntime) -> Option<ContextReading> {
    if let Some(remote) = runtime.remote_source() {
        // Only the newest turn counts: an older reading would understate the context.
        let end = remote
            .projection
            .items
            .iter()
            .rev()
            .find_map(|item| match &item.body {
                Some(p::transcript_item::Body::TurnEnd(end)) => Some(end),
                _ => None,
            })?;
        let usage = end.usage.as_ref()?;
        let usage = ContextUsage {
            used: usage.context_used?,
            max: usage.context_max.filter(|max| *max > 0)?,
            buffer: usage.context_buffer.unwrap_or(0),
        };
        return Some(ContextReading {
            usage: Some(usage),
            rate: None,
            source: ContextSource::Remote(runtime.remote_host().map(|host| host.name())),
        });
    }
    let session = runtime.native()?;
    let (usage, rate) = (session.context(), session.rate_limit());
    (usage.is_some() || rate.is_some()).then(|| ContextReading {
        usage,
        rate,
        source: ContextSource::Local(session.provider()),
    })
}

/// The context indicator next to the pickers: how full the model's context window is.
pub(super) fn context_ring(this: &ChatView, cx: &App) -> Option<Stateful<Div>> {
    let reading = context_reading(this.rt(cx))?;
    let mut meters = vec![];
    if let Some(usage) = reading.usage {
        // The CLI compacts once the free space before its buffer runs out: the ring
        // fills toward that point.
        let limit = usage.max.saturating_sub(usage.buffer).max(1);
        let tokens = format!(
            "{} / {} tokens",
            presentation::tokens(usage.used),
            presentation::tokens(usage.max),
        );
        meters.push(Meter {
            label: "Context".into(),
            percent: (usage.used as f32 / limit as f32 * 100.).clamp(0., 100.),
            detail: match &reading.source {
                ContextSource::Local(Provider::Claude) => format!(
                    "{tokens} · Claude compacts the conversation near {}",
                    presentation::tokens(limit)
                ),
                ContextSource::Local(Provider::Codex) => tokens,
                ContextSource::Remote(Some(host)) => {
                    format!("{tokens} · after the last turn on {host}")
                }
                ContextSource::Remote(None) => format!("{tokens} · after the last turn"),
            },
        });
    }
    if let Some(rate) = reading.rate {
        let window = match rate.window_mins {
            Some(10080) => "Weekly limit".to_owned(),
            Some(300) => "5-hour limit".to_owned(),
            Some(mins) if mins >= 1440 => format!("{}-day limit", mins / 1440),
            Some(mins) => format!("{}-hour limit", mins.div_ceil(60)),
            None => "Usage limit".to_owned(),
        };
        meters.push(Meter {
            label: window,
            percent: (rate.used_percent as f32).clamp(0., 100.),
            detail: rate
                .resets_at
                .and_then(reset_day)
                .map(|at| format!("Resets {at}"))
                .unwrap_or_default(),
        });
    }
    // The ring warns about whichever runs out first.
    let percent = meters.iter().map(|m| m.percent).fold(0., f32::max);
    let color = severity(percent);
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
            .hover_card(
                format!("chat-context-{:?}", this.pane(cx).id),
                gpui_kit::base::Placement::Top,
                move |_, _| meters_card(&meters).into_any_element(),
            )
            .child(
                gpui_kit::component::progress::ProgressCircle::new("chat-context-ring")
                    .value(percent)
                    .color(color)
                    .accessibility_label(format!("{percent:.0}% of the context window used"))
                    .size(px(14.)),
            ),
    )
}

/// One quota behind the context ring: the context window or a plan's usage limit.
struct Meter {
    label: String,
    percent: f32,
    detail: String,
}

fn severity(percent: f32) -> Hsla {
    if percent >= 95. {
        t::red()
    } else if percent >= 80. {
        t::yellow()
    } else {
        t::secondary()
    }
}

fn meters_card(meters: &[Meter]) -> Div {
    card::body().children(meters.iter().enumerate().map(|(index, meter)| {
        let color = severity(meter.percent);
        column()
            .gap_1()
            .when(index > 0, |meter| {
                meter.pt_2().border_t_1().border_color(t::border())
            })
            .child(
                row()
                    .justify_between()
                    .gap_2()
                    .text_xs()
                    .line_height(t::chat_metadata_line_height())
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t::text())
                            .child(meter.label.clone()),
                    )
                    .child(
                        div()
                            .text_color(if meter.percent >= 80. {
                                color
                            } else {
                                t::muted()
                            })
                            .child(format!("{:.0}%", meter.percent)),
                    ),
            )
            .child(
                div()
                    .h(px(4.))
                    .w_full()
                    .rounded_full()
                    .bg(t::border())
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .w(relative(meter.percent / 100.))
                            .bg(color),
                    ),
            )
            .when(!meter.detail.is_empty(), |line| {
                line.child(
                    div()
                        .text_xs()
                        .line_height(t::chat_metadata_line_height())
                        .text_color(t::muted())
                        .child(meter.detail.clone()),
                )
            })
    }))
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

/// When a weekly limit resets: "Thu 14:00" (today: just the time).
fn reset_day(until: u64) -> Option<String> {
    let at = chrono::DateTime::from_timestamp(until as i64, 0)?.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    Some(if at.date_naive() == today {
        at.format("%H:%M").to_string()
    } else {
        at.format("%a %H:%M").to_string()
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
            quiet: false,
        };
        assert_eq!(turn_numbers(&end, false), "2m 41s · 12k in · 1.2k out");
        assert_eq!(
            turn_numbers(&end, true),
            "2m 41s · 12k in · 1.2k out · $0.21"
        );
    }
}
