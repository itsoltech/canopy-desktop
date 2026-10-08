//! Declarative chat layout: centered conversation column above the composer.
use super::{
    rows::{Fold, Kind, Row, Rows, Source},
    subagent, tools,
    view::ChatView,
};
use crate::ui::{components::*, theme as t};
use canopy_desktop::{
    chat::{
        presentation::{self, ToolKind},
        session::Status,
        transcript::{
            Body, Decision, Delivery, Item, NoticeKind, Outcome, Permission, PermissionKind,
            ToolCall, ToolState,
        },
    },
    motion,
};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    bubble::{Bubble, BubbleContent},
    message::MessageAlignment,
    message_scroller::MessageScroller,
    shimmer::ShimmerText,
    spinner::Spinner,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Instant;

/// Readable line length for the conversation column.
pub(super) const COLUMN: f32 = 760.;
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
        let now = Instant::now();
        motion::request_frame(
            window,
            self.shown_rows().animating(now)
                || self.action.is_animating(now)
                || self.queue.animating(now)
                || self.drill.is_animating(now),
        );
        let in_thread = !self.threads.is_empty();
        let body = if self.shown_rows().rows.is_empty() && !in_thread {
            self.empty_state(cx).into_any_element()
        } else {
            let view = cx.entity().downgrade();
            // Each conversation keeps its own scroller, so Back returns to the same place.
            let id: ElementId = match self.threads.last() {
                Some(thread) => ("chat-thread", thread.rows.scroller.entity_id()).into(),
                None => "chat-messages".into(),
            };
            MessageScroller::new(
                id,
                self.shown_rows().scroller.clone(),
                move |index, window, cx| {
                    view.upgrade()
                        .map(|view| render_row(&view, index, window, cx))
                        .unwrap_or_else(|| div().into_any_element())
                },
            )
            .with_row_style(div().p_0().style().clone())
            .with_list_style(div().pt(px(16.)).pb(px(24.)).style().clone())
            .with_bottom_fade(t::bg())
            .flex_1()
            .min_h_0()
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
            .child(body);
        let composer = if in_thread {
            self.locked_composer(cx)
        } else {
            self.composer_island(window, cx)
        };
        column()
            .id("chat")
            .key_context("Chat")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &super::StopChat, _, cx| this.interrupt(cx)))
            .on_action(
                cx.listener(|this, _: &super::AllowPending, _, cx| this.answer_pending(true, cx)),
            )
            .on_action(cx.listener(|this, _: &super::DenyPending, window, cx| {
                // Esc leaves a subagent's conversation before it denies anything.
                if this.threads.is_empty() {
                    this.answer_pending(false, cx)
                } else {
                    this.back(window, cx)
                }
            }))
            .size_full()
            .bg(t::bg())
            .children(self.thread_header(cx))
            .child(body)
            .children(self.banner(cx))
            .children((!in_thread).then(|| self.queue_bar(cx)).flatten())
            .child(composer)
    }
}

impl ChatView {
    fn empty_state(&self, cx: &App) -> Div {
        let (account, model) = self.selection_label(cx);
        let cwd = self
            .pane
            .metadata
            .cwd
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let starting = self.starting && self.error.is_none();
        column()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .px(px(24.))
            .child(
                row()
                    .gap(px(8.))
                    .child(tool_icon("Claude Code", IconName::Bot))
                    .child(div().text_size(px(16.)).text_color(t::text()).child(account)),
            )
            .child(div().text_size(px(12.)).text_color(t::secondary()).child(model))
            .child(
                div()
                    .max_w(px(COLUMN))
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .truncate()
                    .child(cwd),
            )
            .children(starting.then(|| {
                div()
                    .text_size(px(12.))
                    .text_color(t::faint())
                    .child("Starting…")
            }))
            .children(self.resumed.then(|| {
                div()
                    .max_w(px(420.))
                    .text_center()
                    .text_size(px(12.))
                    .text_color(t::secondary())
                    .child("Earlier messages aren't shown yet. The agent still has the conversation context.")
            }))
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
                            .child(text),
                    )
                    .child(restart),
            )
            .pb(px(8.)),
        )
    }
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
        Kind::Restarted => row_separator("Session restarted").into_any_element(),
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
                    ),
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
        Kind::Changes { turn, files, live } => {
            tools::changes_card(view, this, entry, *turn, files, *live, cx).into_any_element()
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

fn row_separator(label: &'static str) -> Div {
    row()
        .pt(px(20.))
        .pb(px(4.))
        .gap(px(12.))
        .child(div().flex_1().h(px(1.)).bg(t::border()))
        .child(div().text_size(px(11.)).text_color(t::faint()).child(label))
        .child(div().flex_1().h(px(1.)).bg(t::border()))
}

fn user_bubble(text: String) -> Bubble {
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
        Body::User(message) => column()
            // A steered message continues the running turn: less space above it.
            .pt(px(if message.delivery == Delivery::Steered {
                12.
            } else {
                24.
            }))
            .items_end()
            .gap(px(4.))
            .child(user_bubble(message.text.clone()))
            .into_any_element(),
        Body::Text(_) => div()
            .pt(px(12.))
            .text_color(t::text())
            .children(entry.markdown.clone())
            .into_any_element(),
        Body::Tool(tool) if presentation::is_subagent(tool) => {
            subagent::card(view, this, rows, entry.key, source, tool, cx)
        }
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
            permission_card(view, item, permission, live, asker, this).into_any_element()
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
                        .child(notice.text.clone()),
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
                tools::body(view, entry, card.as_deref().expect("card")),
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
                    })),
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

fn permission_card(
    view: &Entity<ChatView>,
    item: &Item,
    permission: &Permission,
    live: bool,
    asker: Option<String>,
    this: &ChatView,
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
    if permission.decision != Decision::Pending || !live {
        let (text, glyph) = match permission.decision {
            Decision::Allowed => (format!("Allowed {}", permission.tool), IconName::Check),
            Decision::Denied => (format!("Denied {}", permission.tool), IconName::CircleX),
            Decision::Cancelled | Decision::Pending => (
                format!("{} request dismissed", permission.tool),
                IconName::Minus,
            ),
        };
        return row()
            .pt(px(4.))
            .h(px(32.))
            .px(px(6.))
            .gap(px(8.))
            .child(div().w(px(10.)).flex_shrink_0())
            .child(icon(glyph))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .truncate()
                    .child(if summary.is_empty() {
                        text
                    } else {
                        format!("{text} · {summary}")
                    }),
            );
    }
    let question = permission.kind == PermissionKind::Question;
    let title = if question {
        "Claude has a question".to_owned()
    } else {
        permission
            .title
            .clone()
            .unwrap_or_else(|| format!("Allow {}?", permission.tool))
    };
    let id = item.id;
    let decide = |allow: bool| {
        let view = view.downgrade();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = view.update(cx, |this, cx| this.decide(id, allow, cx));
        }
    };
    column()
        .mt(px(12.))
        .p(px(14.))
        .gap(px(10.))
        .rounded(px(10.))
        .border_1()
        .border_color(t::control_border())
        .bg(t::elevated())
        .children(asker.map(|asker| {
            row()
                .gap(px(6.))
                .text_size(px(11.))
                .text_color(t::muted())
                .child(tool_icon("Claude Code", IconName::Bot))
                .child(div().truncate().child(format!("Subagent · {asker}")))
        }))
        .child(div().text_size(px(13.)).text_color(t::text()).child(title))
        .children((!summary.is_empty()).then(|| {
            div()
                .p(px(8.))
                .rounded(px(6.))
                .bg(t::bg())
                .font_family(t::MONO)
                .text_size(px(12.))
                .text_color(t::secondary())
                .child(summary)
        }))
        .children(
            permission
                .description
                .clone()
                .map(|text| div().text_size(px(12.)).text_color(t::muted()).child(text)),
        )
        .children(question.then(|| {
            div()
                .text_size(px(12.))
                .text_color(t::muted())
                .child("Answering questions from the chat isn't available yet.")
        }))
        .child(
            row()
                .gap(px(8.))
                .justify_end()
                .child(
                    button(
                        ("chat-deny", id.0),
                        if question { "Dismiss" } else { "Deny" },
                    )
                    .tooltip(if question {
                        "Dismiss (Esc)"
                    } else {
                        "Deny (Esc)"
                    })
                    .on_click(decide(false)),
                )
                .children((!question).then(|| {
                    primary_button(("chat-allow", id.0), "Allow once")
                        .tooltip("Allow once (⌘⏎)")
                        .on_click(decide(true))
                })),
        )
}
