//! Subagents: one card in the conversation that started them, and their own
//! read-only conversation opened from it in the same pane.
use super::{
    render::centered,
    rows::{Row, Rows, Source},
    view::ChatView,
};
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{
    presentation::{self, SubagentState},
    transcript::ToolCall,
};
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    shimmer::ShimmerText,
};
use gpui_kit::*;

/// A subagent's card: provider mark with a state dot, task, state and latest line.
pub(super) fn card(
    view: &Entity<ChatView>,
    this: &ChatView,
    rows: &Rows,
    key: u64,
    source: Source,
    tool: &ToolCall,
    cx: &App,
) -> AnyElement {
    let state = SubagentState::of(tool);
    let cwd = this.pane.metadata.cwd.clone().unwrap_or_default();
    let children = rows
        .children
        .get(&tool.call_id)
        .copied()
        .unwrap_or_default();
    let last_child = children
        .last_tool
        .and_then(|index| this.session.as_ref()?.transcript().get(index))
        .and_then(|item| match &item.body {
            canopy_desktop::chat::transcript::Body::Tool(tool) => Some(tool),
            _ => None,
        });
    let preview = presentation::subagent_preview(tool, last_child, &cwd);
    let title = presentation::subagent_title(tool);
    let (dot, label_color) = match state {
        SubagentState::Working => (t::accent(), t::secondary()),
        SubagentState::Finished => (t::green(), t::secondary()),
        SubagentState::Failed => (t::red(), t::red()),
        SubagentState::Interrupted => (t::faint(), t::muted()),
    };
    let label: AnyElement = if state == SubagentState::Working {
        ShimmerText::new(state.label())
            .id(("chat-subagent-state", key))
            .text_size(px(12.))
            .text_color(t::muted())
            .highlight_color(t::text())
            .into_any_element()
    } else {
        div()
            .text_size(px(12.))
            .text_color(label_color)
            .child(state.label())
            .into_any_element()
    };
    let mut details = vec![];
    if tool.background && state == SubagentState::Working {
        details.push("Background".to_owned());
    }
    if children.tools > 0 {
        details.push(if children.tools == 1 {
            "1 tool".to_owned()
        } else {
            format!("{} tools", children.tools)
        });
    }
    // An earlier session's subagent opens from that session's saved conversation.
    let archived = match source {
        Source::Current(_) => None,
        Source::Archived { epoch, .. } => Some(epoch),
    };
    let content =
        row()
            .w_full()
            .gap(px(12.))
            .items_start()
            .child(mark(dot))
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(t::text())
                            .truncate()
                            .child(title.clone()),
                    )
                    .child(row().gap(px(6.)).min_w_0().child(label).children(
                        details.into_iter().map(|detail| {
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .text_color(t::faint())
                                .child(format!("· {detail}"))
                        }),
                    ))
                    .children((!preview.is_empty()).then(|| {
                        div()
                            .text_size(px(12.))
                            .text_color(t::muted())
                            .truncate()
                            .child(preview)
                    })),
            )
            .child(
                icon(IconName::ChevronRight)
                    .text_color(t::faint())
                    .mt(px(10.)),
            );
    let card = Button::new(("chat-subagent", key))
        .custom(
            ButtonCustomVariant::new(cx)
                .color(t::bg())
                .hover(t::hover())
                .active(t::hover())
                .foreground(t::text()),
        )
        .compact()
        .w_full()
        .h_auto()
        .px(px(12.))
        .py(px(10.))
        .rounded(px(10.))
        .border_1()
        .border_color(t::border())
        .accessibility_label(format!("Subagent {title}, {}", state.label()))
        .child(content);
    let call = tool.call_id.clone();
    let view = view.downgrade();
    let card = card
        .tooltip("Open the subagent's conversation")
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| {
                this.open_thread(call.clone(), archived, window, cx)
            });
        })
        .into_any_element();
    div().pt(px(8.)).child(card).into_any_element()
}

/// Claude's mark with the state dot on its corner (Codex gets its own in stage 7).
fn mark(dot: Hsla) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .size(px(28.))
        .rounded(px(8.))
        .bg(t::elevated())
        .flex()
        .items_center()
        .justify_center()
        .child(tool_icon("Claude Code", IconName::Bot))
        .child(
            div()
                .absolute()
                .right(px(-2.))
                .bottom(px(-2.))
                .size(px(10.))
                .rounded_full()
                .border_2()
                .border_color(t::bg())
                .bg(dot),
        )
}

impl ChatView {
    /// "← Chat / Task" above an opened subagent conversation.
    pub(super) fn thread_header(&self, cx: &mut Context<Self>) -> Option<Div> {
        let thread = self.threads.last()?;
        let task = thread.title.clone();
        let title = match self.threads.len() {
            0 | 1 => self
                .pane
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| super::NEW_CHAT_TITLE.into()),
            depth => self.threads[depth - 2].title.clone(),
        };
        Some(
            centered(
                row()
                    .h(px(36.))
                    .gap(px(6.))
                    .child(
                        Button::new("chat-thread-back")
                            .ghost()
                            .xsmall()
                            .compact()
                            .h(px(24.))
                            .px(px(6.))
                            .rounded(px(6.))
                            .icon(icon(IconName::ChevronLeft))
                            .child(div().max_w(px(180.)).truncate().child(title))
                            .text_color(t::secondary())
                            .tooltip("Back (Esc)")
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                    )
                    .child(div().text_color(t::faint()).text_size(px(12.)).child("/"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(t::text())
                            .truncate()
                            .child(task),
                    ),
            )
            .border_b_1()
            .border_color(t::border()),
        )
    }

    /// The composer while a subagent's conversation is open.
    pub(super) fn locked_composer(&self, cx: &mut Context<Self>) -> Div {
        let working = self.is_working();
        centered(
            row()
                .w_full()
                .h(px(48.))
                .px(px(14.))
                .gap(px(8.))
                .rounded(px(16.))
                .border_1()
                .border_color(t::control_border())
                .bg(t::input_bg())
                .child(icon(IconName::Info).text_color(t::faint()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.))
                        .text_color(t::muted())
                        .truncate()
                        .child("Subagents can't receive messages"),
                )
                .children(working.then(|| {
                    button("chat-thread-stop", "Stop")
                        .tooltip("Stop the agent (⌘.)")
                        .on_click(cx.listener(|this, _, _, cx| this.interrupt(cx)))
                })),
        )
        .pt(px(4.))
        .pb(px(16.))
    }
}

/// Subagents started together: stacked marks, count and combined state; expands to
/// one compact row per subagent that opens its conversation.
pub(super) fn group(
    view: &Entity<ChatView>,
    this: &ChatView,
    rows: &Rows,
    entry: &Row,
    sources: &[Source],
    cx: &App,
) -> Div {
    let transcript = this.session.as_ref().map(|s| s.transcript());
    let members: Vec<(Source, &ToolCall)> = sources
        .iter()
        .filter_map(|source| match &rows.item(*source, transcript)?.body {
            canopy_desktop::chat::transcript::Body::Tool(tool) => Some((*source, tool)),
            _ => None,
        })
        .collect();
    let mut counts: Vec<(SubagentState, usize)> = vec![];
    for (_, tool) in &members {
        let state = SubagentState::of(tool);
        match counts.iter_mut().find(|(s, _)| *s == state) {
            Some((_, count)) => *count += 1,
            None => counts.push((state, 1)),
        }
    }
    let summary = counts
        .iter()
        .map(|(state, count)| format!("{count} {}", state.label().to_lowercase()))
        .collect::<Vec<_>>()
        .join(" · ");
    let working = counts.iter().any(|(s, _)| *s == SubagentState::Working);
    let now = std::time::Instant::now();
    let key = entry.key;
    const STEP: f32 = 14.;
    let stacked = members.len().min(3);
    let marks = div()
        .relative()
        .flex_shrink_0()
        .w(px(28. + STEP * stacked.saturating_sub(1) as f32))
        .h(px(28.))
        .children(members.iter().take(3).enumerate().map(|(i, (_, tool))| {
            mark(dot(SubagentState::of(tool)))
                .absolute()
                .top_0()
                .left(px(STEP * i as f32))
                .border_2()
                .border_color(t::bg())
        }));
    let header = Button::new(("chat-subagents", key))
        .custom(
            ButtonCustomVariant::new(cx)
                .color(t::bg())
                .hover(t::hover())
                .active(t::hover())
                .foreground(t::text()),
        )
        .compact()
        .w_full()
        .h_auto()
        .px(px(12.))
        .py(px(10.))
        .rounded(px(10.))
        .border_1()
        .border_color(t::border())
        .accessibility_label(format!("{} subagents, {summary}", members.len()))
        .child(
            row()
                .w_full()
                .gap(px(12.))
                .child(marks)
                .child(
                    column()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(t::text())
                                .child(format!("{} subagents", members.len())),
                        )
                        .child(if working {
                            ShimmerText::new(summary.clone())
                                .id(("chat-subagents-state", key))
                                .text_size(px(12.))
                                .text_color(t::muted())
                                .highlight_color(t::text())
                                .into_any_element()
                        } else {
                            div()
                                .text_size(px(12.))
                                .text_color(t::secondary())
                                .child(summary.clone())
                                .into_any_element()
                        }),
                )
                .children(
                    entry
                        .output
                        .as_ref()
                        .map(|disclosure| disclosure.chevron(now).text_color(t::faint())),
                ),
        )
        .on_click({
            let view = view.downgrade();
            move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.shown_rows_mut().toggle_output(key, cx);
                    cx.notify();
                });
            }
        });
    let list = column()
        .pl(px(16.))
        .children(members.iter().enumerate().map(|(index, (source, tool))| {
            let key = crate::ui::markdown::element_key(entry.key, index, 0);
            card(view, this, rows, key, *source, tool, cx)
        }));
    column().pt(px(8.)).child(header).children(
        entry
            .output
            .as_ref()
            .map(|disclosure| disclosure.measured_body(("chat-subagents-list", key), list, now)),
    )
}

fn dot(state: SubagentState) -> Hsla {
    match state {
        SubagentState::Working => t::accent(),
        SubagentState::Finished => t::green(),
        SubagentState::Failed => t::red(),
        SubagentState::Interrupted => t::faint(),
    }
}
