//! `/goal` in the chat (DEC-079…082): the composer's goal mode, the goal's row over the
//! composer, and its marks in the conversation.
use super::view::ChatView;
use crate::ui::{components::*, theme as t};
use canopy_desktop::{
    chat::{
        goal::{self, EndReason, Goal, GoalEnd, GoalStatus},
        provider::Provider,
        session::Draft,
        transcript::{Attachment, GoalCheck},
    },
    motion::{self, MotionPolicy, Presence, presets},
};
use gpui_kit::component::{IconName, shimmer::ShimmerText};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Instant;

pub(super) const PLACEHOLDER: &str = "Describe when the work is done and how to check it";
const REPLACES: &str = "A new condition replaces the current goal";
/// The counter shows this close to the limit.
const COUNTER_FROM: usize = goal::LIMIT - 400;

/// The goal's row on the composer: open while the chat has a goal.
pub(super) struct GoalRow {
    presence: Presence,
    /// The goal drawn; kept while the row closes.
    shown: Option<Goal>,
    expanded: bool,
}
impl Default for GoalRow {
    fn default() -> Self {
        Self {
            presence: Presence::new(false, presets::PANEL, Instant::now()),
            shown: None,
            expanded: false,
        }
    }
}
impl GoalRow {
    pub(super) fn animating(&self, now: Instant) -> bool {
        self.presence.is_animating(now)
    }
}

impl ChatView {
    pub(super) fn goal_mode(&self) -> bool {
        self.goal_mode
    }

    /// Turns goal mode on or off; the field says what Enter does.
    pub(super) fn set_goal_mode(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.goal_mode == on {
            return;
        }
        if on {
            self.set_shell(false, window, cx);
        }
        self.goal_mode = on;
        let placeholder = match (on, self.rt(cx).goal()) {
            (true, Some(_)) => REPLACES,
            (true, None) => PLACEHOLDER,
            (false, _) => super::decision::COMPOSER_PLACEHOLDER,
        };
        self.composer.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx)
        });
        cx.notify();
    }

    /// `/goal ` typed at the start of the message turns goal mode on; `/goal` isn't
    /// kept as text.
    pub(super) fn goal_typed(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let prefix = format!("/{} ", goal::COMMAND);
        if self.goal_mode
            || self.shell
            || !self.capabilities(cx).goal
            || self.has_code_blocks()
            || self.editing.is_some()
            || self.decision.active().is_some()
            || !self.threads.is_empty()
            || !value.starts_with(&prefix)
        {
            return;
        }
        self.enter_goal_mode(value[prefix.len()..].to_owned(), window, cx);
    }

    /// Goal mode with `text` in the field (after the field's own update).
    pub(super) fn enter_goal_mode(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_goal_mode(true, window, cx);
        let composer = self.composer.clone();
        cx.defer_in(window, move |this, window, cx| {
            composer.update(cx, |state, cx| {
                state.set_value(text.clone(), window, cx);
                state.focus(window, cx);
            });
            this.composer_was_empty = text.is_empty();
        });
    }

    /// Enter in goal mode: the field's text becomes the goal.
    pub(super) fn submit_goal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(cx);
        let text = plain_condition(&draft);
        if text.is_empty() || text.chars().count() > goal::LIMIT || !self.rt(cx).accepts_messages()
        {
            return;
        }
        if self.rt(cx).is_remote() {
            self.save_remote_draft(cx);
            self.send_draft(
                Draft {
                    text,
                    goal: true,
                    ..Default::default()
                },
                cx,
            );
            return;
        }
        self.clear_composer(window, cx);
        self.set_goal_mode(false, window, cx);
        self.send_draft(
            Draft {
                text,
                goal: true,
                ..Default::default()
            },
            cx,
        );
    }

    /// The condition is over the limit: Enter does nothing.
    pub(super) fn goal_too_long(&self, cx: &App) -> bool {
        self.goal_mode && self.composer.read(cx).value().chars().count() > goal::LIMIT
    }

    /// "Goal" over the field in goal mode, with the count near the limit.
    pub(super) fn goal_label(&self, cx: &App) -> Div {
        let count = self.composer.read(cx).value().chars().count();
        row()
            .px(px(4.))
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(t::goal())
            .child(custom_icon("target").size(px(13.)).text_color(t::goal()))
            .child("Goal")
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(t::muted())
                    .child("— the agent keeps working until this holds"),
            )
            .children((count >= COUNTER_FROM).then(|| {
                div()
                    .flex_shrink_0()
                    .text_color(if count > goal::LIMIT {
                        t::red()
                    } else {
                        t::muted()
                    })
                    .child(format!("{count} / {}", goal::LIMIT))
            }))
    }

    pub(super) fn goal_animating(&self, now: Instant) -> bool {
        self.goal_row.animating(now)
    }

    /// Opens the row while the chat has a goal and closes it after.
    pub(super) fn sync_goal_row(&mut self, cx: &App) {
        let goal = self.rt(cx).goal().cloned();
        let now = Instant::now();
        let open = goal.is_some();
        self.goal_row
            .presence
            .set_open(open, now, motion::policy(cx));
        if goal.is_some() {
            self.goal_row.shown = goal;
        } else if !self.goal_row.presence.is_animating(now) {
            self.goal_row.shown = None;
            self.goal_row.expanded = false;
        }
    }

    /// The goal's row in the status bar on the composer's top edge (like the queue).
    pub(super) fn goal_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.sync_goal_row(cx);
        let progress = self.goal_row.presence.progress(Instant::now());
        let goal = self.goal_row.shown.clone()?;
        let content = self.goal_line(&goal, cx);
        Some(
            gpui_kit::base::MotionReveal::new(
                "chat-goal-reveal",
                progress,
                div()
                    .w_full()
                    .relative()
                    .top(px(motion::distance::BASE * (1. - progress)))
                    .opacity(progress)
                    .child(div().id("chat-goal").px(px(10.)).py(px(8.)).child(content))
                    .into_any_element(),
            )
            .into_any_element(),
        )
    }

    fn goal_line(&self, goal: &Goal, cx: &mut Context<Self>) -> Div {
        let working = self.is_working(cx);
        let pursuing = goal.status == GoalStatus::Pursuing && working;
        let codex = self.provider(cx) == Some(Provider::Codex);
        let now = goal::now();
        let title = goal::title(goal, working);
        let warning = matches!(
            goal.status,
            GoalStatus::Blocked | GoalStatus::UsageLimited | GoalStatus::BudgetLimited
        );
        let (mark, mark_color) = match goal.status {
            _ if warning => (custom_icon("target").into_any_element(), t::yellow()),
            GoalStatus::Paused => (icon(IconName::Pause).into_any_element(), t::secondary()),
            _ => (custom_icon("target").into_any_element(), t::goal()),
        };
        let title: AnyElement = if pursuing && motion::policy(cx) == MotionPolicy::Full {
            ShimmerText::new(title)
                .id("chat-goal-title")
                .text_size(px(12.))
                .text_color(t::text())
                .highlight_color(t::shimmer_highlight())
                .into_any_element()
        } else {
            div().child(title).into_any_element()
        };
        let mut buttons: Vec<AnyElement> = vec![];
        if pursuing {
            if codex {
                buttons.push(
                    tray_button("chat-goal-pause", icon(IconName::Pause), "Pause", cx)
                        .tooltip(tip("Finish this turn, then stop working on the goal"))
                        .on_click(cx.listener(|this, _, _, cx| this.pause_goal(cx)))
                        .into_any_element(),
                );
            } else {
                buttons.push(
                    tray_button("chat-goal-clear", icon(IconName::Close), "Clear", cx)
                        .tooltip(tip("Stop and remove the goal"))
                        .on_click(cx.listener(|this, _, _, cx| this.clear_goal(cx)))
                        .into_any_element(),
                );
            }
        } else {
            if codex && goal.status.resumable() {
                buttons.push(
                    tray_button("chat-goal-resume", icon(IconName::Play), "Resume", cx)
                        .tooltip(tip("Continue working toward the goal"))
                        .on_click(cx.listener(|this, _, _, cx| this.resume_goal(cx)))
                        .into_any_element(),
                );
            }
            if !working {
                buttons.push(
                    tray_button("chat-goal-edit", custom_icon("pencil"), "Edit", cx)
                        .tooltip(tip("Change the condition; it replaces the goal"))
                        .on_click(cx.listener(|this, _, window, cx| this.edit_goal(window, cx)))
                        .into_any_element(),
                );
            }
            buttons.push(
                icon_button("chat-goal-remove", IconName::Close, "Clear the goal")
                    .on_click(cx.listener(|this, _, _, cx| this.clear_goal(cx)))
                    .into_any_element(),
            );
        }
        let expanded = self.goal_row.expanded;
        row()
            .w_full()
            .items_start()
            .gap(px(9.))
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(22.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(mark_color.opacity(0.14))
                    .text_color(mark_color)
                    .child(mark),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.))
                    .child(
                        row()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t::text())
                            .child(title)
                            .children(goal::usage(goal, now).map(|usage| {
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(t::muted())
                                    .child(format!("· {usage}"))
                            })),
                    )
                    .child(
                        div()
                            .id("chat-goal-objective")
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(t::secondary())
                            .when(!expanded, |text| text.line_clamp(2))
                            .child(goal.objective.clone())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.goal_row.expanded = !this.goal_row.expanded;
                                cx.notify();
                            })),
                    )
                    .children(
                        goal.last_check
                            .clone()
                            .filter(|_| goal.checks > 0)
                            .map(|reason| {
                                row()
                                    .items_start()
                                    .gap(px(5.))
                                    .pt(px(3.))
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_color(t::yellow())
                                            .child("Not yet:"),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .when(!expanded, |text| text.line_clamp(2))
                                            .child(reason),
                                    )
                            }),
                    )
                    .when(warning, |column| {
                        column.child(
                            div()
                                .pt(px(3.))
                                .text_size(px(11.))
                                .text_color(t::yellow())
                                .child("Resume continues it; edit or clear it to change course."),
                        )
                    }),
            )
            .child(row().flex_shrink_0().gap(px(2.)).children(buttons))
    }

    fn pause_goal(&mut self, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.pause_goal(cx));
    }

    fn resume_goal(&mut self, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.resume_goal(cx));
    }

    fn clear_goal(&mut self, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.clear_goal(cx));
    }

    /// The condition back in the composer, in goal mode: sending it replaces the goal.
    fn edit_goal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(objective) = self
            .native(cx)
            .and_then(|s| s.goal())
            .map(|goal| goal.objective.clone())
        else {
            return;
        };
        self.enter_goal_mode(objective, window, cx);
    }
}

/// The condition as the agent reads it: badges in the text become plain mentions.
pub(super) fn plain_condition(draft: &Draft) -> String {
    let mut text = draft.text.clone();
    for attachment in &draft.attachments {
        let Some(label) = attachment.label() else {
            continue;
        };
        let plain = match attachment {
            Attachment::Path { path, .. } => path.display().to_string(),
            Attachment::Pull { number, title, .. } => format!("#{number} ({title})"),
            Attachment::Quote { text, .. } => format!("\"{text}\""),
            Attachment::Command { name, skill, .. } => {
                format!("{}{name}", if *skill { "$" } else { "/" })
            }
            Attachment::Image { .. } | Attachment::Shell { .. } => String::new(),
        };
        text = text.replace(label, &plain);
    }
    text.trim().to_owned()
}

pub(super) fn tip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text).build(window, cx)
}

pub(super) fn tray_button(
    id: impl Into<ElementId>,
    mark: gpui_kit::component::Icon,
    label: &'static str,
    _: &Context<ChatView>,
) -> Stateful<Div> {
    row()
        .id(id)
        .flex_shrink_0()
        .h(px(22.))
        .px(px(7.))
        .gap(px(4.))
        .rounded(px(6.))
        .hover(|style| style.bg(t::hover()))
        .active(|style| style.bg(t::selected()))
        .cursor_pointer()
        .text_size(px(12.))
        .text_color(t::secondary())
        .child(mark.size(px(11.)).text_color(t::secondary()))
        .child(label)
}

/// "Check 2 · not yet · reason" under a step of Claude's goal.
pub(super) fn check_row(check: &GoalCheck) -> Div {
    row()
        .pt(px(10.))
        .items_start()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(t::secondary())
        .child(
            div()
                .flex_shrink_0()
                .mt(px(1.))
                .size(px(16.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(t::yellow().opacity(0.16))
                .child(
                    icon(IconName::RefreshCw)
                        .size(px(9.))
                        .text_color(t::yellow()),
                ),
        )
        .child(
            column()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_color(t::text())
                        .child(format!("Check {} · not yet", check.n)),
                )
                .child(div().text_color(t::muted()).child(check.reason.clone())),
        )
}

/// A thin rule where the agent started another turn for its goal.
pub(super) fn turn_row(n: u32) -> Div {
    row()
        .pt(px(16.))
        .pb(px(2.))
        .gap(px(10.))
        .child(div().flex_1().h(px(1.)).bg(t::border()))
        .child(
            row()
                .gap(px(5.))
                .text_size(px(11.))
                .text_color(t::faint())
                .child(custom_icon("target").size(px(11.)).text_color(t::faint()))
                .child(format!("Goal turn {n}")),
        )
        .child(div().flex_1().h(px(1.)).bg(t::border()))
}

/// The card closing a goal: achieved (green), impossible or an error (red), or cleared.
pub(super) fn end_card(id: impl Into<ElementId>, end: &GoalEnd) -> Stateful<Div> {
    let (color, mark) = match end.reason {
        EndReason::Achieved => (t::green(), IconName::Check),
        EndReason::Impossible | EndReason::ClearedByError => (t::red(), IconName::CircleX),
        _ => (t::muted(), IconName::Close),
    };
    let summary = goal::summary(end);
    column()
        .id(id)
        .mt(px(12.))
        .px(px(12.))
        .py(px(10.))
        .gap(px(3.))
        .rounded(px(10.))
        .border_1()
        .border_color(color.opacity(0.3))
        .bg(color.opacity(0.07))
        .child(
            row()
                .gap(px(8.))
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t::text())
                .child(icon(mark).size(px(14.)).text_color(color))
                .child(goal::end_title(end.reason)),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(t::muted())
                .line_clamp(3)
                .child(if summary.is_empty() {
                    end.objective.clone()
                } else {
                    format!("{summary} — {}", end.objective)
                }),
        )
        .children(end.note.clone().map(|note| {
            div()
                .text_size(px(11.5))
                .text_color(t::secondary())
                .child(note)
        }))
}

/// "Goal" at the start of the message that set a goal.
pub(super) fn goal_chip() -> Div {
    row()
        .flex_shrink_0()
        .gap(px(4.))
        .px(px(6.))
        .py(px(1.))
        .rounded(px(5.))
        .bg(t::goal().opacity(0.14))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t::goal())
        .child(custom_icon("target").size(px(12.)).text_color(t::goal()))
        .child("Goal")
}
