//! Declarative chat layout: centered conversation column above the composer.
use super::{
    actions,
    rows::{Fold, Group, Kind, Row, Rows, Source},
    subagent, tools,
    view::ChatView,
};
use crate::ui::{components::*, theme as t};
use canopy_desktop::{
    chat::{
        presentation::{self, ToolKind},
        provider::Provider,
        session::Status,
        transcript::{
            Body, Decision, Delivery, Item, NoticeKind, Outcome, Permission, PermissionKind,
            Remember, Resolution, ToolCall, ToolState,
        },
    },
    motion,
};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    Icon, IconName, Sizable,
    bubble::{Bubble, BubbleContent},
    message::MessageAlignment,
    shimmer::ShimmerText,
    spinner::Spinner,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Instant;

/// Readable line length for the conversation column.
pub(super) const COLUMN: f32 = 760.;
/// The conversation's scrollbar strip, which the composer overlay leaves uncovered.
const SCROLLBAR: f32 = 16.;
/// How far above the composer the conversation fades out beneath it.
const COMPOSER_FADE: f32 = 24.;
/// Tool output shown inline; longer results end with a truncation note.
const OUTPUT_CHARS: usize = 20_000;

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.take_focus_request() {
            if self.threads.is_empty() {
                let composer = self.composer.clone();
                window.on_next_frame(move |window, cx| {
                    composer.update(cx, |state, cx| state.focus(window, cx))
                });
            } else {
                // The composer is not mounted in a subagent thread; keep Esc and ⌘. working.
                let focus = self.focus.clone();
                window.on_next_frame(move |window, cx| focus.focus(window, cx));
            }
        }
        self.sync_pages(window.scale_factor(), window, cx);
        self.sync_resting(cx);
        let now = Instant::now();
        motion::request_frame(
            window,
            self.shown_rows().animating(now)
                || self.action.is_animating(now)
                || self.queue.animating(now)
                || self.decision.animating(now)
                || self.drill.is_animating(now)
                || self.navigator.animating(now)
                || self.goal_animating(now)
                || self.activity_animating(now),
        );
        let in_thread = !self.threads.is_empty();
        // The composer and its trays lie over the conversation's bottom edge (like a
        // `position: fixed` footer), so their size never resizes the scrolled viewport;
        // the conversation ends above them by their last measured height.
        let inset = self.overlay_height.get();
        let body = if self.shown_rows().rows.is_empty() && !in_thread {
            self.empty_state(cx).pb(inset).into_any_element()
        } else {
            let view = cx.entity().downgrade();
            // Each conversation keeps its own scroller, so Back returns to the same place.
            let id: ElementId = match self.threads.last() {
                Some(thread) => ("chat-thread", thread.rows.scroller.entity_id()).into(),
                None => "chat-messages".into(),
            };
            div()
                .flex_1()
                .min_h_0()
                .child(
                    super::scroller::ChatScroller::new(
                        id,
                        self.shown_rows().scroller.clone(),
                        move |index, window, cx| {
                            view.upgrade()
                                .map(|view| render_row(&view, index, window, cx))
                                .unwrap_or_else(|| div().into_any_element())
                        },
                    )
                    .bottom_inset(inset, self.overlay_drawn.get()),
                )
                .into_any_element()
        };
        let drill = self.drill.value(now);
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .opacity(drill)
            .left(px(8. * (1. - drill)))
            .relative()
            // Wheeling down at the end wakes a resting composer (`resting.rs`).
            .when(!in_thread, |body| {
                body.on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                    this.composer_wheel(event, cx)
                }))
            })
            // A selection made in the conversation offers Quote where it ends.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.offer_quote(event.position, window, cx)
                }),
            )
            .child(body)
            // Pages and the message rail keep to the part the composer leaves visible.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom(inset)
                    .children(self.page_layer(window, cx))
                    .children(self.navigator(window, cx)),
            )
            .children(self.find_bar(window, cx))
            .children(self.quote_button(window, cx))
            .children(self.quote_reply_popover(window, cx));
        let composer = if in_thread {
            self.locked_composer(cx)
        } else {
            self.composer_island(window, cx)
        };
        // A waiting request takes the queue tray's place until it is answered.
        let decision = self.decision_bar(cx);
        let suggest = (decision.is_none() && !in_thread)
            .then(|| self.suggest_bar(window, cx))
            .flatten();
        let status = (!in_thread).then(|| self.status_bar(cx)).flatten();
        let tray = match decision {
            Some(panel) => Some(panel),
            None => match (!in_thread).then(|| self.edit_bar(cx)).flatten() {
                Some(bar) => Some(bar),
                None => (!in_thread).then(|| self.queue_bar(cx)).flatten(),
            },
        };
        column()
            .id("chat")
            .key_context("Chat")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &super::StopChat, _, cx| this.interrupt(cx)))
            .on_action(
                cx.listener(|this, _: &super::PreviousMessage, _, cx| this.jump_message(false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::NextMessage, _, cx| this.jump_message(true, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::FindInChat, window, cx| this.open_find(window, cx)),
            )
            // The composer leaves ⌘F to its parents as its own Search action.
            .on_action(
                cx.listener(|this, _: &gpui_kit::base::input::Search, window, cx| {
                    this.open_find(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &super::FindNext, _, cx| this.find_step(1, cx)))
            .on_action(cx.listener(|this, _: &super::FindPrevious, _, cx| this.find_step(-1, cx)))
            .on_action(cx.listener(|this, _: &super::AllowPending, window, cx| {
                this.decision_primary(window, cx)
            }))
            .on_action(cx.listener(|this, _: &super::DenyPending, window, cx| {
                // Esc leaves a subagent's conversation before it denies anything.
                if this.close_lightbox(cx)
                    || this.close_quote_reply(window, cx)
                    || this.dismiss_quote_offer(cx)
                    || this.close_find(window, cx)
                {
                } else if this.editing.is_some() {
                    this.cancel_edit(window, cx)
                } else if this.threads.is_empty() {
                    this.decision_dismiss(cx)
                } else {
                    this.back(window, cx)
                }
            }))
            .size_full()
            .relative()
            .bg(t::bg())
            .children(self.thread_header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .relative()
                    .child(body)
                    .child(self.composer_overlay(composer, status, tray, suggest, cx)),
            )
            // Files dropped anywhere on the chat, the message field included, attach.
            .children((!in_thread).then(|| self.drop_target(cx)))
            .children(self.lightbox(window, cx))
    }
}

impl ChatView {
    /// The banner, trays and composer over the conversation's bottom edge, on the
    /// window's background (the scrollbar's strip stays uncovered). The pointer and
    /// the wheel stop there, as outside a page's scroll container; the trays' own
    /// lists scroll. Suggestions for the word being typed float from the composer's
    /// top edge over everything else.
    fn composer_overlay(
        &mut self,
        composer: Div,
        status: Option<Div>,
        tray: Option<Div>,
        suggest: Option<Div>,
        cx: &mut Context<Self>,
    ) -> Div {
        let height = self.overlay_height.clone();
        let drawn = self.overlay_drawn.clone();
        let unfolded = self.overlay_unfolded.clone();
        // The resting composer (and its easing) keeps the full one's room below the
        // conversation: the list's length and scrollbar don't change as it shrinks.
        let frozen = self.resting.on || self.resting.height(Instant::now()).is_some();
        let background = t::bg();
        column()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("chat-composer-backdrop")
                    .absolute()
                    .flex()
                    .flex_col()
                    .top(px(-COMPOSER_FADE))
                    .bottom_0()
                    .left_0()
                    .right(px(SCROLLBAR))
                    .child(
                        div()
                            .flex_shrink_0()
                            .h(px(COMPOSER_FADE))
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(background.opacity(0.), 0.),
                                linear_color_stop(background.opacity(0.9), 1.),
                            )),
                    )
                    .child(
                        div()
                            .id("chat-composer-shade")
                            .flex_1()
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(background.opacity(0.9), 0.),
                                linear_color_stop(background, 1.),
                            ))
                            .occlude(),
                    ),
            )
            .children(self.banner(cx))
            .children(status)
            .children(tray)
            .child(
                div()
                    .relative()
                    .child(composer)
                    .children(suggest.map(|bar| {
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(relative(1.))
                            .child(bar)
                    })),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        // Trays painted before this canvas, in the same frame.
                        let room = bounds.size.height - unfolded.replace(px(0.));
                        let measured = bounds.size.height;
                        let moved = drawn.replace(measured) != measured;
                        if (!frozen && height.replace(room) != room) || moved {
                            window.refresh();
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
    }

    fn empty_state(&self, cx: &App) -> Div {
        let _ = cx;
        column().flex_1().justify_end().items_center().px_6()
            .child(column().w_full().max_w(rems(44.)).pb_6().gap_2()
                .child(div().text_lg().font_weight(FontWeight::MEDIUM).text_color(t::text())
                    .child(if self.resumed { "Continue this conversation" } else { "What would you like to work on?" }))
                .child(div().text_sm().text_color(t::secondary()).child(if self.resumed {
                    "Earlier messages aren't shown yet. The agent still has the conversation context."
                } else { "Describe a task, ask a question, or mention a file below." }))
                .children((self.starting && self.error.is_none()).then(|| div().text_xs().text_color(t::muted()).child("Connecting to agent…")))
)
    }

    /// Session problems and ends sit right above the composer, with Restart.
    fn banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (text, danger) = if let Some(error) = &self.error {
            (error.clone(), true)
        } else {
            match self.status()? {
                Status::Failed(error) => (error.clone(), true),
                Status::Exited { .. } => ("Session ended".to_owned(), false),
                _ => return None,
            }
        };
        let busy = self.busy();
        let restart = button("chat-restart", "Restart")
            .loading(busy)
            .disabled(!self.can_restart())
            .tooltip(if busy {
                "Restart is available once the session has stopped."
            } else {
                "Start the agent again with this chat's choices"
            })
            .on_click(cx.listener(|this, _, _, cx| this.restart(cx)));
        Some(
            centered(
                row()
                    .gap(px(12.))
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .bg(t::bg())
                    .border_1()
                    .border_color(if danger {
                        t::red().opacity(0.4)
                    } else {
                        t::border()
                    })
                    .child(
                        div()
                            .id("chat-banner-text")
                            .flex_1()
                            .min_w_0()
                            .max_h(px(96.))
                            .overflow_y_scroll()
                            .text_size(px(12.))
                            .text_color(if danger { t::red() } else { t::secondary() })
                            .child(text)
                            .nested_scroll(),
                    )
                    .child(restart),
            )
            .pb(px(8.)),
        )
    }
}

/// Tray content that lies over the conversation when it unfolds instead of lifting it:
/// all but `keep` of its drawn height is left out of the list's bottom padding.
pub(super) fn unfolds_over(
    sink: &std::rc::Rc<std::cell::Cell<Pixels>>,
    keep: Pixels,
    content: impl IntoElement,
) -> Div {
    let sink = sink.clone();
    div().relative().child(content).child(
        canvas(
            |_, _, _| (),
            move |bounds, _, _, _| {
                let over = (bounds.size.height - keep).max(px(0.));
                sink.set(sink.get() + over);
            },
        )
        .absolute()
        .inset_0(),
    )
}

/// Full-width row whose content is limited to the conversation column.
pub(super) fn centered(content: impl IntoElement) -> Div {
    row()
        .w_full()
        .justify_center()
        .px(px(24.))
        .child(div().w_full().max_w(px(COLUMN)).min_w_0().child(content))
}

fn render_row(view: &Entity<ChatView>, index: usize, _: &mut Window, cx: &mut App) -> AnyElement {
    let this = view.read(cx);
    let rows = this.shown_rows();
    let Some(entry) = rows.rows.get(index).filter(|entry| !rows.folded(entry)) else {
        return div().into_any_element();
    };
    let opacity = rows.reveal.opacity(index, Instant::now());
    let content = match &entry.kind {
        Kind::Thinking => row()
            .pt(px(12.))
            .gap(px(6.))
            .child(custom_icon("brain").size(px(14.)).text_color(t::muted()))
            .child(
                ShimmerText::new("Thinking…")
                    .id("chat-thinking")
                    .text_size(px(13.))
                    .text_color(t::muted())
                    .highlight_color(t::text()),
            )
            .into_any_element(),
        Kind::Separator(label) => row_separator(label).into_any_element(),
        Kind::Prompt(prompt) => column()
            .pt(px(16.))
            .items_end()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t::faint())
                    .child("Task"),
            )
            .child(
                div()
                    .id(("chat-thread-prompt", entry.key))
                    .w_full()
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    // A column gives the bubble the row's width to wrap in; a flex row
                    // collapsed it to nothing.
                    .child(
                        column()
                            .w_full()
                            .items_end()
                            .child(user_bubble(prompt.clone())),
                    )
                    .nested_scroll(),
            )
            .into_any_element(),
        Kind::Report => column()
            .child(row_separator("Reported back"))
            .child(
                div()
                    .pt(px(8.))
                    .text_color(t::text())
                    .children(entry.markdown.clone()),
            )
            .into_any_element(),
        Kind::Changes { turn, files, epoch } => {
            tools::changes_card(view, this, entry, *turn, files, *epoch, cx).into_any_element()
        }
        Kind::Subagents(sources) => {
            subagent::group(view, this, rows, entry, sources, cx).into_any_element()
        }
        Kind::Item(source) => {
            let transcript = this.session.as_ref().map(|s| s.transcript());
            match rows.item(*source, transcript) {
                Some(item) => item_row(view, this, rows, entry, *source, item, cx),
                None => div().into_any_element(),
            }
        }
    };
    // A run of actions closed by a reply: its header, the first action when open.
    let content = match &entry.extra.group {
        Some(group) => column()
            .child(group_header(view, entry.key, group))
            .when(group.disclosure.open, |column| column.child(content))
            .into_any_element(),
        None => content,
    };
    match &entry.extra.fold {
        // The header stays put while the work under it fades in.
        Some(fold) => centered(
            column()
                .child(worked_header(view, entry.key, fold))
                .when(fold.disclosure.open, |column| {
                    column.child(div().opacity(opacity).child(content))
                }),
        )
        .into_any_element(),
        None => centered(content).opacity(opacity).into_any_element(),
    }
}

/// "Worked for 2m 41s ›" above a completed turn's reply; opens the work behind it.
fn worked_header(view: &Entity<ChatView>, key: u64, fold: &Fold) -> Div {
    let label = match fold.duration_ms {
        Some(ms) => format!("Worked for {}", tools::elapsed(ms as f32 / 1000.)),
        None => "Worked".to_owned(),
    };
    let view = view.downgrade();
    column()
        .pt(px(16.))
        .child(
            row()
                .id(("chat-worked", key))
                .keyboard("Show or hide the work of this turn")
                .h(px(24.))
                .gap(px(4.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(t::muted())
                .hover(|style| style.text_color(t::secondary()))
                .child(label)
                .child(
                    fold.disclosure
                        .chevron(Instant::now())
                        .text_color(t::faint()),
                )
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        let policy = motion::policy(cx);
                        this.shown_rows_mut().toggle_fold(key, policy, cx);
                        cx.notify();
                    });
                }),
        )
        .child(div().mt(px(6.)).h(px(1.)).bg(t::border()))
}

/// "Ran 3 commands and read 2 files ›" over a run of actions; opens them (DEC-068).
fn group_header(view: &Entity<ChatView>, key: u64, group: &Group) -> Stateful<Div> {
    let view = view.downgrade();
    row()
        .id(("chat-actions", key))
        .keyboard(format!(
            "{} {}",
            if group.disclosure.open {
                "Hide"
            } else {
                "Show"
            },
            group.label
        ))
        .pt(px(12.))
        .h(px(36.))
        .gap(px(6.))
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(t::muted())
        .hover(|style| style.text_color(t::secondary()))
        .child(custom_icon("wrench").size(px(13.)))
        .child(group.label.clone())
        .child(
            group
                .disclosure
                .chevron(Instant::now())
                .text_color(t::faint()),
        )
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| {
                let policy = motion::policy(cx);
                this.shown_rows_mut().toggle_group(key, policy, cx);
                cx.notify();
            });
        })
}

/// A notice with its live countdown: when a limit resets, when a retry happens.
fn notice_text(notice: &canopy_desktop::chat::transcript::Notice) -> String {
    let Some(until) = notice.until else {
        return notice.text.clone();
    };
    match (notice.kind, actions::countdown(until)) {
        (NoticeKind::RateLimit, left) => match (actions::local_time(until), left) {
            (Some(at), Some(left)) => format!("{} · resets at {at} (in {left})", notice.text),
            (Some(at), None) => format!("{} · reset at {at}", notice.text),
            _ => notice.text.clone(),
        },
        (NoticeKind::ApiRetry, Some(left)) => format!("{} · next try in {left}", notice.text),
        _ => notice.text.clone(),
    }
}

/// Under a message sent after a switch: the context that went with it, folded.
fn sent_context(view: &Entity<ChatView>, entry: &Row, context: &str) -> Div {
    let key = entry.key;
    let open = entry.extra.expanded;
    let view = view.downgrade();
    column()
        .w_full()
        .items_end()
        .gap(px(6.))
        .child(
            row()
                .id(("chat-sent-context", key))
                .keyboard(format!(
                    "{} the context sent with this message",
                    if open { "Hide" } else { "Show" }
                ))
                .gap(px(4.))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(t::faint())
                .hover(|style| style.text_color(t::secondary()))
                .child(format!(
                    "Context from earlier in the conversation · {}",
                    super::handoff::size_label(context)
                ))
                .child(
                    Icon::new(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(10.)),
                )
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.shown_rows_mut().toggle_expanded(key, cx);
                        cx.notify();
                    });
                }),
        )
        .children(open.then(|| {
            super::handoff::context_text(("chat-sent-context-text", key), context.to_owned())
        }))
}

fn row_separator(label: &'static str) -> Div {
    row()
        .pt(px(20.))
        .pb(px(4.))
        .gap(px(12.))
        .child(div().flex_1().h(px(1.)).bg(t::border()))
        .child(div().text_size(px(11.)).text_color(t::faint()).child(label))
        .child(div().flex_1().h(px(1.)).bg(t::border()))
}

fn user_bubble(text: impl IntoElement) -> Bubble {
    Bubble::new()
        .alignment(MessageAlignment::End)
        .max_w(relative(0.85))
        .content(
            BubbleContent::new()
                .bg(t::hover())
                .text_color(t::text())
                .text_size(px(14.))
                .rounded(px(16.))
                .px(px(14.))
                .py(px(9.)),
        )
        .child(text)
}

fn item_row(
    view: &Entity<ChatView>,
    this: &ChatView,
    rows: &Rows,
    entry: &Row,
    source: Source,
    item: &Item,
    cx: &App,
) -> AnyElement {
    let live = matches!(source, Source::Current(_));
    match &item.body {
        // A `!` command's result: its card above already shows it went to the agent.
        Body::User(message)
            if message.attachments.iter().any(|a| {
                matches!(
                    a,
                    canopy_desktop::chat::transcript::Attachment::Shell { .. }
                )
            }) =>
        {
            div().into_any_element()
        }
        Body::Shell(run) => {
            super::shell::shell_card(view, this, entry, item.id, run, live).into_any_element()
        }
        Body::GoalCheck(check) => super::goal::check_row(check).into_any_element(),
        Body::GoalTurn(n) => super::goal::turn_row(*n).into_any_element(),
        Body::GoalEnd(end) => {
            super::goal::end_card(("chat-goal-end", entry.key), end).into_any_element()
        }
        Body::User(message) => column()
            // A steered message continues the running turn: less space above it.
            .pt(px(if message.delivery == Delivery::Steered {
                12.
            } else {
                24.
            }))
            .items_end()
            .gap(px(4.))
            .group(actions::MESSAGE_GROUP)
            // Attachments and quotes mentioned in the text show as badges inside it.
            .children((!message.text.is_empty()).then(|| {
                match (&entry.markdown, message.goal) {
                    // The goal's condition, after a "Goal" chip.
                    (_, true) => user_bubble(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .child(super::goal::goal_chip())
                            .child(message.text.clone()),
                    ),
                    (Some(markdown), false) => user_bubble(markdown.clone()),
                    (None, false) => user_bubble(message.text.clone()),
                }
            }))
            .children(
                message
                    .context
                    .as_ref()
                    .map(|context| sent_context(view, entry, context)),
            )
            .children(super::attachments::sent_attachments(
                view,
                this,
                entry.key,
                &message
                    .attachments
                    .iter()
                    .filter(|attachment| {
                        entry.markdown.is_none()
                            || !super::attachments::mentioned_in(&message.text, attachment)
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
            ))
            .child(actions::user_actions(
                view,
                this,
                entry,
                item.id,
                &message.text,
                live && this.settled()
                    && this
                        .session
                        .as_ref()
                        .is_some_and(|s| s.fork_point(item.id).is_some()),
                // Sent in another agent's part: it can't be edited from here.
                match source {
                    Source::Archived { epoch, .. } => rows
                        .archived
                        .get(epoch)
                        .and_then(|archived| archived.agent.as_ref())
                        .filter(|agent| agent.tool != this.pane.tool)
                        .and_then(|agent| Provider::from_tool(&agent.tool))
                        .map(Provider::name),
                    Source::Current(_) => None,
                },
            ))
            .into_any_element(),
        Body::Text(block) => {
            let end_at = entry.extra.turn_end.map(|index| match source {
                Source::Current(_) => Source::Current(index),
                Source::Archived { epoch, .. } => Source::Archived { epoch, index },
            });
            // Only the main conversation forks (a subagent's thread has no session).
            let fork = end_at
                .filter(|_| this.threads.is_empty())
                .filter(|&at| this.fork_request(at).is_some());
            let transcript = this.session.as_ref().map(|s| s.transcript());
            let end =
                end_at
                    .and_then(|end| rows.item(end, transcript))
                    .and_then(|item| match &item.body {
                        Body::TurnEnd(end) => Some(end),
                        _ => None,
                    });
            // Retry answers the newest turn only: its end is the conversation's last.
            let last = live
                && entry.extra.turn_end.is_some()
                && transcript.and_then(|t| {
                    t.items()
                        .iter()
                        .rposition(|item| matches!(item.body, Body::TurnEnd(_)))
                }) == entry.extra.turn_end;
            column()
                .pt(px(12.))
                .group(actions::MESSAGE_GROUP)
                .child(div().text_color(t::text()).children(entry.markdown.clone()))
                .children(entry.extra.turn_end.is_some().then(|| {
                    actions::reply_actions(view, this, entry, end, &block.text, last, fork)
                }))
                .into_any_element()
        }
        Body::Tool(tool)
            if tool.name == canopy_desktop::chat::page::SHOW_TOOL
                && tool.state != ToolState::Failed =>
        {
            super::page::page_card(view, this, entry, tool).into_any_element()
        }
        Body::Tool(tool) if presentation::is_subagent(tool) => {
            subagent::card(view, this, rows, entry.key, source, tool, cx)
        }
        Body::Tool(tool) if item.trimmed => column()
            .child(tool_row(view, this, entry, tool, cx))
            .child(
                div()
                    .pl(px(26.))
                    .text_size(px(11.))
                    .text_color(t::faint())
                    .child("Long output trimmed in saved history"),
            )
            .into_any_element(),
        Body::Tool(tool) => tool_row(view, this, entry, tool, cx).into_any_element(),
        Body::Permission(permission) => {
            let asker = live
                .then(|| rows.permission_thread(permission))
                .flatten()
                .and_then(|call| rows.calls.get(call))
                .and_then(|call| this.session.as_ref()?.transcript().get(call.index))
                .and_then(|item| match &item.body {
                    Body::Tool(tool) => Some(presentation::subagent_title(tool)),
                    _ => None,
                });
            permission_row(view, entry, permission, live, asker, this, cx).into_any_element()
        }
        Body::Notice(notice) => {
            let (color, icon_name) = match notice.kind {
                NoticeKind::Error => (t::red(), IconName::CircleX),
                NoticeKind::RateLimit | NoticeKind::ApiRetry => (t::yellow(), IconName::RefreshCw),
                NoticeKind::Compacted | NoticeKind::Diagnostic => (t::muted(), IconName::Info),
            };
            row()
                .pt(px(8.))
                .gap(px(8.))
                .items_start()
                .child(icon(icon_name).text_color(color).mt(px(2.)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .text_color(t::secondary())
                        .child(notice_text(notice)),
                )
                .into_any_element()
        }
        Body::TurnEnd(end) => {
            let (text, color) = match &end.outcome {
                Outcome::Interrupted => ("Interrupted".to_owned(), t::muted()),
                Outcome::Failed(error) => (format!("Failed: {error}"), t::red()),
                Outcome::Limit(reason) => (reason.clone(), t::yellow()),
                Outcome::Completed => (String::new(), t::muted()),
            };
            div()
                .pt(px(8.))
                .text_size(px(12.))
                .text_color(color)
                .child(text)
                .into_any_element()
        }
        Body::Thinking(block) => {
            tools::thinking_row(view, entry, &block.text, block.streaming, cx).into_any_element()
        }
    }
}

fn tool_icon_name(kind: ToolKind) -> IconName {
    match kind {
        ToolKind::Shell => IconName::SquareTerminal,
        ToolKind::Read => IconName::FileText,
        ToolKind::Edit => IconName::File,
        ToolKind::Search => IconName::Search,
        ToolKind::Web => IconName::Globe,
        ToolKind::Agent => IconName::Bot,
        ToolKind::Plan | ToolKind::Other => IconName::ChevronRight,
    }
}

fn tool_row(
    view: &Entity<ChatView>,
    this: &ChatView,
    entry: &Row,
    tool: &ToolCall,
    cx: &App,
) -> Div {
    let cwd = this.pane.metadata.cwd.clone().unwrap_or_default();
    let summary = presentation::tool_summary(tool, &cwd);
    let file = this.tool_files.get(&tool.call_id);
    let (status, status_label): (AnyElement, &'static str) = match tool.state {
        ToolState::Streaming | ToolState::Running => (
            Spinner::new().xsmall().color(t::muted()).into_any_element(),
            "Running",
        ),
        ToolState::Succeeded => (
            icon(IconName::Check)
                .text_color(t::green())
                .into_any_element(),
            "Done",
        ),
        ToolState::Failed => (
            icon(IconName::CircleX)
                .text_color(t::red())
                .into_any_element(),
            "Failed",
        ),
        ToolState::Cancelled => (
            div()
                .text_size(px(11.))
                .text_color(t::faint())
                .child("Interrupted")
                .into_any_element(),
            "Interrupted",
        ),
    };
    let activity = (tool.background && !tool.state.finished()).then(|| {
        let text = match &tool.activity {
            Some(activity) => format!("Running in background · {activity}"),
            None => "Running in background".to_owned(),
        };
        div()
            .pl(px(40.))
            .text_size(px(11.))
            .text_color(t::faint())
            .truncate()
            .child(text)
    });
    let key = entry.key;
    let card = entry.extra.card.clone();
    let (label, detail) = card
        .as_deref()
        .and_then(tools::header)
        .map(|(label, detail)| (label.to_owned(), detail))
        .unwrap_or_else(|| (tool.name.clone(), None));
    let has_output = match &card {
        Some(card) => tools::has_body(card),
        None => presentation::has_output(tool),
    };
    let now = Instant::now();
    let line = row()
        .w_full()
        .gap(px(8.))
        .child(match &entry.output {
            Some(disclosure) if has_output => disclosure
                .chevron(now)
                .text_color(t::faint())
                .into_any_element(),
            _ => div().w(px(10.)).flex_shrink_0().into_any_element(),
        })
        .child(icon(tool_icon_name(presentation::tool_kind(&tool.name))))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(13.))
                .text_color(t::secondary())
                .child(label),
        )
        .child(match file {
            Some(file) => div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .flex()
                .child(crate::ui::markdown::file_pill::pill(
                    ("chat-tool-file", key),
                    &cwd,
                    &canopy_desktop::chat::mentions::Target {
                        path: file.path(),
                        line: file.line,
                    },
                    px(12.),
                    px(20.),
                ))
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .font_family(t::MONO)
                .text_color(t::muted())
                .truncate()
                .child(summary)
                .into_any_element(),
        })
        .children(detail)
        .child(
            div()
                .id(("chat-tool-status", key))
                .flex_shrink_0()
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(status_label).build(window, cx)
                })
                .child(status),
        );
    // Only rows with output are interactive; the others stay plain lines.
    let header: AnyElement = if has_output {
        row_button(("chat-tool", key), line, cx)
            .w_auto()
            // Hover background reaches 6 px past both text edges; status stays on the right edge.
            .mx(px(-6.))
            .accessibility_label(format!("{} {status_label}, show output", tool.name))
            .on_click({
                let view = view.downgrade();
                move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.shown_rows_mut().toggle_output(key, cx);
                        cx.notify();
                    });
                }
            })
            .into_any_element()
    } else {
        row().h(px(28.)).child(line).into_any_element()
    };
    let output = match (&entry.output, &tool.output) {
        (Some(disclosure), Some(_)) if has_output && card.is_some() => {
            Some(disclosure.measured_body(
                ("chat-tool-output", key),
                // "Show more" eases the card's height.
                AnimatedHeight::new(
                    ("chat-tool-height", key),
                    motion::policy(cx) == motion::MotionPolicy::Full,
                    tools::body(view, entry, card.as_deref().expect("card")),
                ),
                now,
            ))
        }
        (Some(disclosure), Some(output)) if has_output => Some(
            disclosure.measured_body(
                ("chat-tool-output", key),
                div()
                    .id(("chat-tool-output-scroll", key))
                    .mt(px(4.))
                    .ml(px(40.))
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .p(px(10.))
                    .rounded(px(6.))
                    .bg(t::elevated())
                    .font_family(t::MONO)
                    .text_size(px(12.))
                    .text_color(if output.is_error {
                        t::red()
                    } else {
                        t::secondary()
                    })
                    .child(output.text.chars().take(OUTPUT_CHARS).collect::<String>())
                    .children((output.text.chars().count() > OUTPUT_CHARS).then(|| {
                        div()
                            .pt(px(6.))
                            .text_color(t::faint())
                            .child("Output truncated")
                    }))
                    .nested_scroll(),
                now,
            ),
        ),
        _ => None,
    };
    column()
        .pt(px(4.))
        .child(header)
        .children(activity)
        .children(output)
}

/// A full-width, left-aligned button row with quiet hover (no fill change on rest).
fn row_button(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    cx: &App,
) -> gpui_kit::component::button::Button {
    use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .hover(t::hover())
                .foreground(t::secondary()),
        )
        .compact()
        .w_full()
        .h(px(28.))
        .px(px(6.))
        .rounded(px(6.))
        .border_0()
        .child(content)
}

/// A request in the conversation: waiting (answered in the panel above the composer) or
/// how the user answered. An approved plan opens to its text.
fn permission_row(
    view: &Entity<ChatView>,
    entry: &Row,
    permission: &Permission,
    live: bool,
    asker: Option<String>,
    this: &ChatView,
    cx: &App,
) -> Div {
    let cwd = this.pane.metadata.cwd.clone().unwrap_or_default();
    let summary = presentation::tool_summary(
        &ToolCall {
            call_id: String::new(),
            name: permission.tool.clone(),
            input: permission.input.clone(),
            input_partial: String::new(),
            state: ToolState::Running,
            output: None,
            elapsed_seconds: None,
            activity: None,
            background: false,
        },
        &cwd,
    );
    let tool = &permission.tool;
    let with_summary = |text: String| {
        if summary.is_empty() {
            text
        } else {
            format!("{text} · {summary}")
        }
    };
    let with_note = |text: String, note: &Option<String>| match note {
        Some(note) => format!("{text} — {note}"),
        None => text,
    };
    let (glyph, text) = match (&permission.resolution, permission.decision) {
        (_, Decision::Pending) if live => (
            IconName::Info,
            match permission.kind {
                PermissionKind::Question => "Waiting for your answer".to_owned(),
                PermissionKind::Plan => "Waiting for plan approval".to_owned(),
                PermissionKind::Tool => with_summary(format!("Waiting for permission · {tool}")),
            },
        ),
        (Some(Resolution::Allowed), _) => {
            (IconName::Check, with_summary(format!("Allowed {tool}")))
        }
        (Some(Resolution::Remembered(Remember::Rules(rules))), _) => (
            IconName::Check,
            format!(
                "Always allowed in this chat · {}",
                rules
                    .iter()
                    .map(|rule| rule.text())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        (Some(Resolution::Remembered(Remember::Session)), _) => (
            IconName::Check,
            with_summary(format!("Allowed {tool} for this session")),
        ),
        (Some(Resolution::Remembered(Remember::AcceptEdits)), _) => (
            IconName::Check,
            with_summary(format!("Allowed {tool}, edits accepted in this chat")),
        ),
        (Some(Resolution::Denied(note)), _) => (
            IconName::CircleX,
            with_note(with_summary(format!("Denied {tool}")), note),
        ),
        (Some(Resolution::Answered(answers)), _) => (
            IconName::Check,
            format!(
                "{} asked · {}",
                this.provider().name(),
                answers
                    .iter()
                    .map(|(question, answer)| format!("{question} → {answer}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ),
        (Some(Resolution::PlanApproved { accept_edits }), _) => (
            IconName::Check,
            if *accept_edits {
                "Plan approved, edits accepted".to_owned()
            } else {
                "Plan approved".to_owned()
            },
        ),
        (Some(Resolution::KeepPlanning(note)), _) => {
            (IconName::Minus, with_note("Kept planning".to_owned(), note))
        }
        (Some(Resolution::Expired), _) => (
            IconName::Minus,
            with_summary(match permission.kind {
                PermissionKind::Question => "Question not answered".to_owned(),
                PermissionKind::Plan => "Plan not answered".to_owned(),
                PermissionKind::Tool => format!("{tool} request not answered"),
            }),
        ),
        (None, Decision::Allowed) => (IconName::Check, with_summary(format!("Allowed {tool}"))),
        (None, Decision::Denied) => (IconName::CircleX, with_summary(format!("Denied {tool}"))),
        (None, _) => (
            IconName::Minus,
            with_summary(match permission.kind {
                PermissionKind::Question => "Question dismissed".to_owned(),
                PermissionKind::Plan => "Plan dismissed".to_owned(),
                PermissionKind::Tool => format!("{tool} request dismissed"),
            }),
        ),
    };
    let text = match asker {
        Some(asker) => format!("{text} · Subagent {asker}"),
        None => text,
    };
    let plan = permission.kind == PermissionKind::Plan && entry.markdown.is_some();
    let now = Instant::now();
    let line = row()
        .w_full()
        .gap(px(8.))
        .child(match (&entry.output, plan) {
            (Some(disclosure), true) => disclosure
                .chevron(now)
                .text_color(t::faint())
                .into_any_element(),
            _ => div().w(px(10.)).flex_shrink_0().into_any_element(),
        })
        .child(icon(glyph).text_color(t::muted()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .text_color(t::muted())
                .truncate()
                .child(text),
        );
    let key = entry.key;
    let header: AnyElement = if plan {
        row_button(("chat-plan", key), line, cx)
            .w_auto()
            .mx(px(-6.))
            .accessibility_label("Show the plan")
            .on_click({
                let view = view.downgrade();
                move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.shown_rows_mut().toggle_output(key, cx);
                        cx.notify();
                    });
                }
            })
            .into_any_element()
    } else {
        row().h(px(28.)).child(line).into_any_element()
    };
    column()
        .pt(px(4.))
        .child(header)
        .children(entry.output.as_ref().filter(|_| plan).map(|disclosure| {
            disclosure.measured_body(
                ("chat-plan-body", key),
                div()
                    .mt(px(4.))
                    .px(px(18.))
                    .py(px(14.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(t::control_border())
                    .bg(t::elevated())
                    .text_color(t::text())
                    .children(entry.markdown.clone()),
                now,
            )
        }))
}

#[cfg(test)]
mod empty_tests {
    use super::ChatView;
    use crate::app_state::AppState;
    use canopy_desktop::state::workspace::Workspace;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, component::Root, px, size};

    #[gpui_kit::test]
    fn empty_chat_does_not_link_to_an_unrelated_agent_terminal(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (app, chat, terminal) = cx.update(|cx| {
            crate::ui::theme::init(cx);
            let mut model = Workspace::new();
            model.open_conversation("New chat", "claude");
            let chat = model.active().unwrap().root.first().clone();
            model.open("Codex — Default", "codex");
            let terminal = model.active().unwrap().focused;
            model.activate_pane(chat.id).unwrap();
            (AppState::install_test(model, cx), chat, terminal)
        });
        let window = cx.open_window(size(px(780.), px(780.)), |window, cx| {
            let view = cx.new(|cx| ChatView::new(chat, window, cx));
            Root::new(view, window, cx)
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("chat-focus-terminal").is_none());
            assert_ne!(app.workspace.read(cx).selected_pane().unwrap().id, terminal);
            assert!(!app.workspace.read(cx).tool_panel().is_maximized());
            assert_eq!(app.workspace.read(cx).all_panes().len(), 2);
            assert!(app.terminals.read(cx).views.is_empty());
        })
        .unwrap();
    }
}
