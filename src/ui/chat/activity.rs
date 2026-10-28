//! The status bar on the composer's top edge (stage 12): the chat's goal, then the
//! agent's task list and the commands it left running in the background, folded to
//! one line (DEC-084). Finished work stays until the next message or × (DEC-085);
//! a background task stops on its own with Claude (DEC-086).
use super::{
    goal::{tip, tray_button},
    render::{centered, unfolds_over},
    rows::{Kind, Source},
    view::ChatView,
};
use crate::ui::{components::*, theme as t};
use canopy_desktop::{
    chat::{
        activity::{self, Job, JobKind, TodoStatus, Todos},
        transcript::ToolState,
    },
    motion::{self, MotionPolicy, Presence, presets},
};
use gpui_kit::component::{IconName, Sizable, shimmer::ShimmerText, spinner::Spinner};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

/// The agent's work beside the conversation, as last read from its transcript, and
/// what the user put away.
pub(super) struct ActivityPanel {
    presence: Presence,
    fold: Disclosure,
    /// Session generation and transcript revision the lists were read at.
    read_at: (u64, u64),
    todos: Option<Todos>,
    jobs: Vec<Job>,
    /// Kept on screen while the panel closes.
    shown: Option<(Option<Todos>, Vec<Job>)>,
    /// Finished background calls the user has seen off, and the finished list's version.
    hidden_jobs: HashSet<String>,
    hidden_list: usize,
    /// When a running call was first seen, and how long a finished one ran.
    started: HashMap<String, Instant>,
    ran: HashMap<String, Duration>,
    ticker: Option<Task<()>>,
}
impl Default for ActivityPanel {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            presence: Presence::new(false, presets::PANEL, now),
            fold: Disclosure::new(false, now),
            read_at: (u64::MAX, 0),
            todos: None,
            jobs: vec![],
            shown: None,
            hidden_jobs: HashSet::new(),
            hidden_list: 0,
            started: HashMap::new(),
            ran: HashMap::new(),
            ticker: None,
        }
    }
}
impl ActivityPanel {
    pub(super) fn animating(&self, now: Instant) -> bool {
        self.presence.is_animating(now) || self.fold.active(now)
    }

    fn visible_todos(&self) -> Option<&Todos> {
        self.todos
            .as_ref()
            .filter(|todos| !todos.finished() || todos.version > self.hidden_list)
    }

    fn visible_jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs
            .iter()
            .filter(|job| !job.state.finished() || !self.hidden_jobs.contains(&job.call))
    }

    fn running(&self) -> usize {
        self.jobs.iter().filter(|job| !job.state.finished()).count()
    }

    /// Puts finished work away: the next message was sent, or ×.
    fn put_away(&mut self) {
        for job in self.jobs.iter().filter(|job| job.state.finished()) {
            self.hidden_jobs.insert(job.call.clone());
        }
        if let Some(todos) = self.todos.as_ref().filter(|todos| todos.finished()) {
            self.hidden_list = todos.version;
        }
    }

    fn duration(&self, job: &Job, now: Instant) -> Option<Duration> {
        if job.state.finished() {
            job.elapsed_seconds
                .map(Duration::from_secs_f64)
                .or_else(|| self.ran.get(&job.call).copied())
        } else {
            self.started.get(&job.call).map(|at| now - *at)
        }
    }
}

/// "2:14", "41s"
fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    } else {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    }
}

impl ChatView {
    /// Reads the task list and background calls again after the transcript changed;
    /// a new session starts with what it restored put away.
    pub(super) fn sync_activity(&mut self, cx: &mut Context<Self>) {
        let runtime = self.runtime.read(cx);
        let Some(transcript) = runtime.transcript() else {
            return;
        };
        let at = (runtime.generation(), transcript.revision());
        let panel = &mut self.activity;
        if panel.read_at == at {
            return;
        }
        let fresh = panel.read_at.0 != at.0;
        panel.read_at = at;
        panel.todos = activity::todos(transcript);
        // Background subagents have their own tray ("waiting for").
        panel.jobs = activity::jobs(
            transcript,
            runtime
                .pane_ref()
                .metadata
                .cwd
                .as_deref()
                .unwrap_or("/".as_ref()),
        )
        .into_iter()
        .filter(|job| job.kind != JobKind::Agent)
        .collect();
        if fresh {
            panel.hidden_jobs.clear();
            panel.hidden_list = 0;
            panel.started.clear();
            panel.ran.clear();
            panel.put_away();
        }
        let now = Instant::now();
        for job in &panel.jobs {
            if job.state.finished() {
                if let Some(at) = panel.started.remove(&job.call) {
                    panel.ran.insert(job.call.clone(), now - at);
                }
            } else {
                panel.started.entry(job.call.clone()).or_insert(now);
            }
        }
        self.tick_activity(cx);
    }

    /// A message went out: finished work is put away.
    pub(super) fn activity_sent(&mut self) {
        self.activity.put_away();
    }

    /// Redraws each second while an open panel shows a running command's time.
    fn tick_activity(&mut self, cx: &mut Context<Self>) {
        let wanted = self.activity.fold.open && self.activity.running() > 0;
        if !wanted {
            self.activity.ticker = None;
            return;
        }
        if self.activity.ticker.is_some() {
            return;
        }
        self.activity.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let going = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        this.activity.fold.open && this.activity.running() > 0
                    })
                    .unwrap_or(false);
                if !going {
                    let _ = this.update(cx, |this, _| this.activity.ticker = None);
                    return;
                }
            }
        }));
    }

    pub(super) fn activity_animating(&self, now: Instant) -> bool {
        self.activity.animating(now)
    }

    /// The goal and the agent's work over the composer, in one surface.
    pub(super) fn status_bar(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.threads.is_empty() {
            return None;
        }
        let goal = self.goal_section(cx);
        let work = self.activity_section(goal.is_some(), cx);
        if goal.is_none() && work.is_none() {
            return None;
        }
        Some(
            centered(
                div().mx(px(14.)).child(
                    column()
                        .w_full()
                        .rounded_t(px(12.))
                        .border_1()
                        .border_b_0()
                        .border_color(t::border())
                        .bg(t::composer_surface())
                        .overflow_hidden()
                        .children(goal)
                        .children(work),
                ),
            )
            .mb(px(-4.)),
        )
    }

    fn activity_section(&mut self, below_goal: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = Instant::now();
        let panel = &mut self.activity;
        let visible = (
            panel.visible_todos().cloned(),
            panel.visible_jobs().cloned().collect::<Vec<_>>(),
        );
        let open = visible.0.is_some() || !visible.1.is_empty();
        panel.presence.set_open(open, now, motion::policy(cx));
        if open {
            panel.shown = Some(visible);
        } else if !panel.presence.is_animating(now) {
            panel.shown = None;
        }
        let (todos, jobs) = panel.shown.clone()?;
        let progress = panel.presence.progress(now);
        let animate = motion::policy(cx) == MotionPolicy::Full;
        let body = column()
            .children(todos.as_ref().map(|todos| self.todo_list(todos, cx)))
            .children((!jobs.is_empty()).then(|| self.job_list(&jobs, now, cx)));
        Some(
            gpui_kit::base::MotionReveal::new(
                "chat-activity-reveal",
                progress,
                column()
                    .w_full()
                    .relative()
                    .top(px(motion::distance::BASE * (1. - progress)))
                    .opacity(progress)
                    .when(below_goal, |column| {
                        column.border_t_1().border_color(t::border())
                    })
                    .child(self.activity_header(todos.as_ref(), &jobs, animate, now, cx))
                    // Unfolded, the lists lie over the conversation.
                    .child(unfolds_over(
                        &self.overlay_unfolded,
                        px(0.),
                        self.activity.fold.measured_body(
                            "chat-activity-body",
                            AnimatedHeight::new("chat-activity-height", animate, body),
                            now,
                        ),
                    ))
                    .into_any_element(),
            )
            .into_any_element(),
        )
    }

    /// The folded line: progress, what is being done, how much runs in the background.
    fn activity_header(
        &self,
        todos: Option<&Todos>,
        jobs: &[Job],
        animate: bool,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let running: Vec<&Job> = jobs.iter().filter(|job| !job.state.finished()).collect();
        let ended = jobs.len() - running.len();
        let working = self.is_working(cx);
        let all_done = todos.is_none_or(Todos::finished) && running.is_empty();
        let current = todos
            .and_then(Todos::current)
            .map(|todo| todo.doing().to_owned())
            .or_else(|| running.first().map(|job| job.label.clone()));
        let headline: AnyElement = match (current, todos) {
            (Some(text), _) if working && animate => ShimmerText::new(text)
                .id("chat-activity-current")
                .text_size(px(12.))
                .text_color(t::text())
                .highlight_color(t::shimmer_highlight())
                .into_any_element(),
            (Some(text), _) => div().truncate().child(text).into_any_element(),
            (None, Some(todos)) if todos.finished() => div()
                .text_color(t::secondary())
                .child("All tasks done")
                .into_any_element(),
            (None, Some(todos)) => div()
                .text_color(t::secondary())
                .child(format!("{} tasks left", todos.items.len() - todos.done()))
                .into_any_element(),
            (None, None) => div()
                .text_color(t::secondary())
                .child("Background work finished")
                .into_any_element(),
        };
        let open = self.activity.fold.open;
        row()
            .id("chat-activity-toggle")
            .keyboard(if open {
                "Collapse the agent's tasks"
            } else {
                "Expand the agent's tasks"
            })
            .w_full()
            .h(px(32.))
            .px(px(10.))
            .gap(px(8.))
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(t::text())
            .child(self.activity.fold.chevron(now).text_color(t::muted()))
            .children(todos.map(|todos| {
                let done = todos.finished();
                row()
                    .flex_shrink_0()
                    .gap(px(5.))
                    .child(
                        icon(if done {
                            IconName::CircleCheck
                        } else {
                            IconName::Loader
                        })
                        .size(px(12.))
                        .text_color(if done {
                            t::green()
                        } else {
                            t::accent()
                        }),
                    )
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                        "{}/{}",
                        todos.done(),
                        todos.items.len()
                    )))
            }))
            .child(div().flex_1().min_w_0().truncate().child(headline))
            .children((!running.is_empty()).then(|| {
                row()
                    .flex_shrink_0()
                    .gap(px(5.))
                    .text_color(t::secondary())
                    .child(Spinner::new().xsmall().color(t::accent()))
                    .child(format!("{} in background", running.len()))
            }))
            .children((running.is_empty() && ended > 0).then(|| {
                row()
                    .flex_shrink_0()
                    .gap(px(5.))
                    .text_color(t::muted())
                    .child(icon(IconName::Check).size(px(11.)))
                    .child(format!("{ended} finished in background"))
            }))
            .children(all_done.then(|| {
                icon_button(
                    "chat-activity-dismiss",
                    IconName::Close,
                    "Put the finished work away",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    // Not a click on the line beneath: the panel doesn't unfold.
                    cx.stop_propagation();
                    this.activity.put_away();
                    cx.notify();
                }))
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.activity.fold.toggle(Instant::now(), cx);
                this.tick_activity(cx);
                cx.notify();
            }))
    }

    fn todo_list(&self, todos: &Todos, cx: &mut Context<Self>) -> Div {
        let working = self.is_working(cx) && motion::policy(cx) == MotionPolicy::Full;
        column()
            .px(px(10.))
            .pt(px(4.))
            .pb(px(8.))
            .gap(px(2.))
            .border_t_1()
            .border_color(t::border())
            .child(section_title(
                "Tasks",
                format!("{} of {} done", todos.done(), todos.items.len()),
            ))
            .children(todos.items.iter().enumerate().map(|(index, todo)| {
                let (mark, color) = match todo.status {
                    TodoStatus::Completed => (IconName::CircleCheck, t::green()),
                    TodoStatus::InProgress => (IconName::Loader, t::accent()),
                    TodoStatus::Pending => (IconName::Minus, t::faint()),
                };
                let text: AnyElement = match todo.status {
                    TodoStatus::InProgress if working => ShimmerText::new(todo.doing().to_owned())
                        .id(("chat-activity-todo", index))
                        .text_size(px(12.))
                        .text_color(t::text())
                        .highlight_color(t::shimmer_highlight())
                        .into_any_element(),
                    TodoStatus::InProgress => div()
                        .text_color(t::text())
                        .child(todo.doing().to_owned())
                        .into_any_element(),
                    TodoStatus::Completed => div()
                        .text_color(t::muted())
                        .line_through()
                        .child(todo.text.clone())
                        .into_any_element(),
                    TodoStatus::Pending => div()
                        .text_color(t::secondary())
                        .child(todo.text.clone())
                        .into_any_element(),
                };
                row()
                    .items_start()
                    .gap(px(8.))
                    .py(px(2.))
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .pt(px(2.))
                            .child(icon(mark).size(px(12.)).text_color(color)),
                    )
                    .child(div().flex_1().min_w_0().child(text))
            }))
    }

    fn job_list(&self, jobs: &[Job], now: Instant, cx: &mut Context<Self>) -> Div {
        let running = jobs.iter().filter(|job| !job.state.finished()).count();
        column()
            .px(px(10.))
            .pt(px(4.))
            .pb(px(8.))
            .border_t_1()
            .border_color(t::border())
            .child(section_title(
                "Background",
                if running > 0 {
                    format!("{running} running")
                } else {
                    "finished".to_owned()
                },
            ))
            .children(jobs.iter().map(|job| self.job_row(job, now, cx)))
    }

    fn job_row(&self, job: &Job, now: Instant, cx: &mut Context<Self>) -> Stateful<Div> {
        let mark: AnyElement = match job.state {
            ToolState::Succeeded => icon(IconName::CircleCheck)
                .size(px(12.))
                .text_color(t::green())
                .into_any_element(),
            ToolState::Failed => icon(IconName::CircleX)
                .size(px(12.))
                .text_color(t::red())
                .into_any_element(),
            ToolState::Cancelled => icon(IconName::Square)
                .size(px(11.))
                .text_color(t::muted())
                .into_any_element(),
            ToolState::Running | ToolState::Streaming => Spinner::new()
                .xsmall()
                .color(t::accent())
                .into_any_element(),
        };
        let time = self.activity.duration(job, now).map(clock);
        let status = match job.state {
            ToolState::Cancelled => Some("stopped".to_owned()),
            ToolState::Failed => Some("failed".to_owned()),
            _ => None,
        };
        let detail = [status, time]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let call = job.call.clone();
        let stoppable = self
            .native(cx)
            .is_some_and(|session| session.can_stop_background(&job.call));
        let index = job.index;
        row()
            .id(SharedString::from(format!(
                "chat-activity-job-{}",
                job.call
            )))
            .keyboard(format!("Show {} in the conversation", job.label))
            .h(px(26.))
            .gap(px(8.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|style| style.bg(t::hover()))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(14.))
                    .flex()
                    .justify_center()
                    .child(mark),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(t::MONO)
                    .text_size(px(11.))
                    .text_color(if job.state.finished() {
                        t::secondary()
                    } else {
                        t::text()
                    })
                    .child(job.label.clone()),
            )
            .children((!detail.is_empty()).then(|| {
                div()
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child(detail)
            }))
            .children(stoppable.then(|| {
                let call = call.clone();
                tray_button(
                    SharedString::from(format!("chat-activity-stop-{call}")),
                    icon(IconName::Square),
                    "Stop",
                    cx,
                )
                .tooltip(tip("Stop this background task"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.stop_background(&call, cx);
                }))
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.show_item(index, cx)))
    }

    fn stop_background(&mut self, call: &str, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.stop_background(call, cx));
    }

    /// Scrolls the conversation to transcript item `index`, opening what folds it.
    fn show_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let source = Source::Current(index);
        let rows = &mut self.rows;
        let Some(row) = rows.rows.iter().position(|row| match &row.kind {
            Kind::Item(item) => *item == source,
            Kind::Subagents(items) => items.contains(&source),
            _ => false,
        }) else {
            return;
        };
        rows.uncover(row, cx);
        rows.scroller
            .update(cx, |state, cx| state.reveal_row(row, cx));
        cx.notify();
    }
}

fn section_title(title: &'static str, note: String) -> Div {
    row()
        .h(px(22.))
        .gap(px(6.))
        .text_size(px(11.))
        .text_color(t::muted())
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(div().flex_1())
        .child(note)
}
