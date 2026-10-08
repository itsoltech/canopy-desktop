use super::{components::integrations::*, components::tasks::*, components::*, theme as t};
use crate::app_state::{AppState, IntegrationsState};
use canopy_desktop::{
    integrations::{ProjectTarget, Provider, TaskItem, TaskState},
    motion::{self, Presence, presets},
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName,
        input::{InputEvent, InputState},
        scroll::ScrollableElement,
    },
    *,
};
use std::{path::PathBuf, sync::Arc, time::Instant};
#[derive(Clone)]
pub struct TaskWorktreeRequest {
    pub task: canopy_desktop::integrations::TaskRef,
    pub repository: Arc<canopy_desktop::git::RepositoryInfo>,
}
impl EventEmitter<TaskWorktreeRequest> for TasksPanel {}
impl EventEmitter<super::task_edit::TaskEditRequest> for TasksPanel {}
impl EventEmitter<super::task_detail::TaskDetailRequest> for TasksPanel {}
pub struct TasksPanel {
    refresh_loading: ButtonLoading,
    more_loading: ButtonLoading,
    loading_more: bool,
    state: Entity<IntegrationsState>,
    search: Entity<InputState>,
    source_picker: Entity<super::task_source::TaskSourcePicker>,
    controls: Entity<super::task_controls::TaskControls>,
    results: Arc<Vec<TaskItem>>,
    list_scroll: UniformListScrollHandle,
    selected: Option<TaskItem>,
    description: Entity<super::markdown::MarkdownView>,
    details: Presence,
    details_open: bool,
    configure: bool,
    error: Option<String>,
    root: Option<PathBuf>,
    account: Option<String>,
    target: Option<ProjectTarget>,
    _events: Vec<Subscription>,
}
impl TasksPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search tasks…"));
        let source_picker = cx.new(|cx| super::task_source::TaskSourcePicker::new(window, cx));
        let events = vec![
            cx.subscribe(&state, |s, _, e: &crate::app_state::WriteFinished, cx| {
                if let Ok(r) = &e.result
                    && let Some(deleted) = &r.deleted_task
                    && s.selected
                        .as_ref()
                        .is_some_and(|t| t.reference.same_task(deleted))
                {
                    s.close_details(cx);
                    s.selected = None;
                    cx.notify();
                }
            }),
            cx.subscribe(
                &source_picker,
                |s, _, _: &super::task_source::SourceApplied, cx| {
                    s.configure = false;
                    cx.notify();
                },
            ),
            cx.subscribe(&state, |this, _, task: &TaskItem, cx| {
                this.select(task.clone(), cx)
            }),
            cx.subscribe(&search, |this, _, _: &InputEvent, cx| this.filter(cx)),
            cx.observe_in(&state, window, |this, state, window, cx| {
                let state = state.read(cx);
                this.refresh_loading
                    .set(state.loading && !this.loading_more, cx);
                this.more_loading
                    .set(state.loading && this.loading_more, cx);
                if !state.loading {
                    this.loading_more = false;
                }
                let account = state.active_account().map(|a| a.credential.clone());
                if this.root != state.path || this.account != account || this.target != state.target
                {
                    let query = state.query.clone();
                    this.target = state.target.clone();
                    this.account = account;
                    this.root = state.path.clone();
                    this.search
                        .update(cx, |s, cx| s.set_value(query, window, cx));
                    this.selected = None;
                    this.details_open = false;
                    this.details = Presence::new(false, presets::PANEL, Instant::now());
                    this.source_picker.update(cx, |s, cx| s.reset(window, cx));
                    this.error = None;
                }
                this.filter(cx);
                cx.notify();
            }),
        ];
        Self {
            refresh_loading: ButtonLoading::default(),
            more_loading: ButtonLoading::default(),
            loading_more: false,
            state,
            search,
            source_picker,
            controls: cx.new(|cx| super::task_controls::TaskControls::new(window, cx)),
            results: Arc::new(vec![]),
            list_scroll: UniformListScrollHandle::new(),
            selected: None,
            description: cx.new(super::markdown::MarkdownView::new),
            details: Presence::new(false, presets::PANEL, Instant::now()),
            details_open: false,
            configure: false,
            error: None,
            root: None,
            account: None,
            target: None,
            _events: events,
        }
    }
    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_lowercase();
        let state = self.state.read(cx);
        self.results = Arc::new(
            state
                .items
                .iter()
                .filter(|t| {
                    if state.target.as_ref().is_some_and(|target| {
                        matches!(target.provider, Provider::Jira | Provider::Youtrack)
                    }) {
                        return true;
                    }
                    format!(
                        "{} {} {} {}",
                        t.reference.id,
                        t.reference.title,
                        t.labels.join(" "),
                        t.assignees.join(" ")
                    )
                    .to_lowercase()
                    .contains(&query)
                })
                .cloned()
                .collect(),
        );
        if let Some(selected) = &mut self.selected
            && let Some(fresh) = state
                .items
                .iter()
                .find(|t| t.reference.same_task(&selected.reference))
        {
            *selected = fresh.clone();
        }
        self.sync_description(cx);
        cx.notify();
    }
    fn select(&mut self, task: TaskItem, cx: &mut Context<Self>) {
        self.selected = Some(task);
        self.details_open = true;
        self.sync_description(cx);
        self.details
            .set_open(true, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn sync_description(&mut self, cx: &mut Context<Self>) {
        let (body, url) = self
            .selected
            .as_ref()
            .map(|task| (task.body.clone(), task.url.clone()))
            .unwrap_or_else(|| (Arc::from(""), String::new()));
        self.description.update(cx, |view, cx| {
            view.set_content(body, url, self.details_open, cx)
        });
    }
    fn close_details(&mut self, cx: &mut Context<Self>) {
        self.details_open = false;
        self.sync_description(cx);
        self.details
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
}
impl TasksPanel {
    fn toggle_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.configure = !self.configure;
        if self.configure {
            self.source_picker.update(cx, |s, cx| s.reset(window, cx));
        }
        cx.notify();
    }
    fn header(&self, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let title = state
            .target
            .as_ref()
            .map(|t| t.key.as_str())
            .unwrap_or("GitHub Issues");
        let subtitle = if let Some(site) = state.target.as_ref().and_then(|t| t.site.as_deref()) {
            site
        } else if state
            .repository
            .as_ref()
            .is_some_and(|repo| state.config.overrides.contains_key(&repo.common))
        {
            "Project task source"
        } else if state.target.is_some() {
            "From origin"
        } else {
            "Connect your project to its issues."
        };
        row()
            .gap(px(8.))
            .py(px(12.))
            .flex_shrink_0()
            .child(
                provider_heading(
                    state
                        .target
                        .as_ref()
                        .map(|t| t.provider)
                        .unwrap_or(Provider::Github),
                    title.to_owned(),
                    subtitle.to_owned(),
                )
                .flex_1(),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .child(
                        icon_button("create-task", IconName::Plus, "New issue")
                            .disabled(
                                state.busy || !state.ready || state.active_account().is_none(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                let state = this.state.read(cx);
                                if let (Some(project), Some(path)) =
                                    (state.target.clone(), state.path.clone())
                                {
                                    cx.emit(super::task_edit::TaskEditRequest::Create {
                                        project,
                                        path,
                                    });
                                }
                            })),
                    )
                    .child(
                        icon_button("task-repository", IconName::Settings, "Task repository")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_repository(window, cx)
                            })),
                    )
                    .child(
                        loading_icon_button(
                            "refresh-tasks",
                            IconName::RotateCw,
                            "Refresh issues",
                            Some(&self.refresh_loading),
                        )
                        .disabled(
                            (state.busy
                                || !state.ready
                                || state.loading
                                || state.active_account().is_none())
                                && !self.refresh_loading.active(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |state, cx| state.refresh(true, cx))
                        })),
                    ),
            )
    }
    fn repository_form(&self, _disabled: bool, _cx: &mut Context<Self>) -> Div {
        column()
            .p(px(10.))
            .rounded(px(6.))
            .bg(t::hover())
            .child(self.source_picker.clone())
    }
    fn filters(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let tracker = state
            .target
            .as_ref()
            .is_some_and(|t| matches!(t.provider, Provider::Jira | Provider::Youtrack));
        let filter = state.filter;
        column()
            .gap(px(8.))
            .flex_shrink_0()
            .child(
                row()
                    .gap(px(6.))
                    .child(input(&self.search).flex_1())
                    .children(tracker.then(|| {
                        loading_button("task-search", "Search tasks", &self.refresh_loading)
                            .disabled((disabled || state.loading) && !self.refresh_loading.active())
                            .on_click(cx.listener(|s, _, _, cx| {
                                let query = s.search.read(cx).value().to_string();
                                s.state.update(cx, |state, cx| state.set_query(query, cx));
                            }))
                    })),
            )
            .child(
                row()
                    .gap(px(8.))
                    .children((!tracker).then(|| {
                        row()
                            .gap(px(2.))
                            .p(px(2.))
                            .rounded(px(6.))
                            .bg(t::hover())
                            .children(
                                [(TaskState::Open, "Open"), (TaskState::Closed, "Closed")]
                                    .into_iter()
                                    .map(|(value, label)| {
                                        selection_button(label, label, filter == value)
                                            .border_0()
                                            .disabled(disabled)
                                            .on_click(cx.listener(move |s, _, _, cx| {
                                                s.state.update(cx, |state, cx| {
                                                    state.set_filter(value, cx)
                                                })
                                            }))
                                    }),
                            )
                    }))
                    .child(div().flex_1())
                    .child(div().text_size(px(10.)).text_color(t::muted()).child(
                        match state.total_count {
                            Some(total) => format!("{} of {total}", state.items.len()),
                            None => format!("{} loaded", state.items.len()),
                        },
                    )),
            )
    }
    fn issue_list(&self, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let list = uniform_list(
            "task-list",
            self.results.len() + usize::from(state.more),
            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|i| {
                        if i >= this.results.len() {
                            return row()
                                .h(px(TASK_ROW_HEIGHT))
                                .justify_center()
                                .child(
                                    loading_button(
                                        "load-issues",
                                        "Load more issues",
                                        &this.more_loading,
                                    )
                                    .disabled(
                                        (this.state.read(cx).loading || this.state.read(cx).busy)
                                            && !this.more_loading.active(),
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.loading_more = true;
                                            this.state.update(cx, |state, cx| state.next_page(cx));
                                            if !this.state.read(cx).loading {
                                                this.loading_more = false;
                                            }
                                        },
                                    )),
                                )
                                .into_any_element();
                        }
                        let task = this.results[i].clone();
                        let selected = this.details_open
                            && this
                                .selected
                                .as_ref()
                                .is_some_and(|s| s.reference.same_task(&task.reference));
                        task_row(&task, selected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.state.read(cx).busy {
                                    this.select(task.clone(), cx);
                                }
                            }))
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .size_full()
        .track_scroll(&self.list_scroll);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(list)
            .vertical_scrollbar(&self.list_scroll)
    }
    fn detail_panel(
        &self,
        progress: f32,
        height: f32,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(task) = self.selected.clone() else {
            return div();
        };
        let reference = task.reference.clone();
        let create_task = reference.clone();
        let unlink = reference.clone();
        let url = task.url.clone();
        let state = self.state.read(cx);
        let linked = state
            .path
            .as_ref()
            .and_then(|path| state.config.links.get(path))
            .is_some_and(|links| links.iter().any(|t| t.same_task(&reference)));
        column()
            .h(px(height * progress))
            .flex_shrink_0()
            .overflow_hidden()
            .border_t_1()
            .border_color(t::border())
            .child(
                column()
                    .h(px(height))
                    .w_full()
                    .opacity(progress)
                    .gap(px(10.))
                    .child(
                        row()
                            .flex_shrink_0()
                            .h(px(32.))
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(11.))
                                    .text_color(t::secondary())
                                    .child(task.reference.label()),
                            )
                            .children(linked.then(|| badge("Linked").text_color(t::accent())))
                            .child(
                                icon_button(
                                    "expand-task-details",
                                    IconName::Maximize,
                                    "Expand task details",
                                )
                                .disabled(!self.details_open)
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let (Some(task), Some(path)) = (
                                            this.selected.clone(),
                                            this.state.read(cx).path.clone(),
                                        ) {
                                            cx.emit(super::task_detail::TaskDetailRequest {
                                                task,
                                                path,
                                            });
                                        }
                                    },
                                )),
                            )
                            .child(
                                icon_button(
                                    "task-open-browser",
                                    IconName::ExternalLink,
                                    "Open task in browser",
                                )
                                .disabled(!self.details_open)
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                            )
                            .child(
                                icon_button(
                                    "close-task-details",
                                    IconName::Close,
                                    "Close task details",
                                )
                                .disabled(!self.details_open)
                                .on_click(cx.listener(|this, _, _, cx| this.close_details(cx))),
                            ),
                    )
                    .child(
                        div()
                            .id("task-description")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .id(SharedString::from(format!(
                                "task-description-{}-{}",
                                task.reference.project.key, task.reference.id
                            )))
                            .child(task_details(&task, self.description.clone())),
                    )
                    .child(
                        row()
                            .flex_shrink_0()
                            .gap(px(8.))
                            .py(px(10.))
                            .border_t_1()
                            .border_color(t::border())
                            .child(
                                button("link-worktree", if linked { "Unlink" } else { "Link" })
                                    .tooltip(if linked {
                                        "Unlink from this worktree"
                                    } else {
                                        "Link to this worktree"
                                    })
                                    .disabled(disabled || !self.details_open)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let path = this.state.read(cx).path.clone();
                                        if let Some(path) = path {
                                            this.error = this
                                                .state
                                                .update(cx, |state, cx| {
                                                    if linked {
                                                        state.unlink(path, &unlink, cx)
                                                    } else {
                                                        state.link(path, reference.clone(), cx)
                                                    }
                                                })
                                                .err();
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(div().flex_1())
                            .child(
                                primary_button("task-new-worktree", "New worktree")
                                    .disabled(disabled || !self.details_open)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let app = cx.global::<AppState>();
                                        let info = this
                                            .state
                                            .read(cx)
                                            .repository
                                            .as_ref()
                                            .and_then(|r| {
                                                app.git.read(cx).repository(&r.repository)
                                            })
                                            .cloned();
                                        if let Some(repository) = info {
                                            cx.emit(TaskWorktreeRequest {
                                                task: create_task.clone(),
                                                repository,
                                            });
                                        } else {
                                            this.error = Some(
                                                "Wait for repository metadata, then retry.".into(),
                                            );
                                            cx.notify();
                                        }
                                    })),
                            ),
                    ),
            )
    }
}
impl Render for TasksPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let disabled = state.busy || !state.ready;
        let ready = state.ready;
        let connected = state.active_account().is_some();
        let target = state.target.is_some();
        let loading = state.loading;
        let error = self
            .error
            .clone()
            .or(state.task_error.clone())
            .or(state.error.clone());
        let show_list = !self.results.is_empty() || state.more;
        let empty_title = if self
            .target
            .as_ref()
            .is_some_and(|t| matches!(t.provider, Provider::Jira | Provider::Youtrack))
            || !self.search.read(cx).value().is_empty()
        {
            "No matching issues"
        } else if state.filter == TaskState::Open {
            "No open issues"
        } else {
            "No closed issues"
        };
        let now = Instant::now();
        let progress = self.details.progress(now);
        motion::request_frame(window, self.details.is_animating(now));
        column()
            .size_full()
            .gap(px(10.))
            .child(self.header(cx))
            .child(self.controls.clone())
            .children(self.configure.then(|| self.repository_form(disabled, cx)))
            .children(error.clone().map(|error| integration_message(error, true)))
            .children((ready && !connected).then(|| {
                column()
                    .flex_1()
                    .justify_center()
                    .gap(px(4.))
                    .child(task_empty_state(
                        if target {
                            "Connect task provider"
                        } else {
                            "Choose a task source"
                        },
                        if target {
                            "Read issues, link tasks and start a worktree from here."
                        } else {
                            "Choose a GitHub repository or a Jira project for this workspace."
                        },
                    ))
                    .child(
                        row().justify_center().child(
                            button(
                                "tasks-open-preferences",
                                if target {
                                    "Configure integration"
                                } else {
                                    "Choose repository"
                                },
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if target {
                                        let _ = super::preferences::open_integrations(cx);
                                    } else {
                                        this.toggle_repository(window, cx);
                                    }
                                },
                            )),
                        ),
                    )
            }))
            .children(connected.then(|| self.filters(disabled, cx)))
            .children(
                (!ready || (connected && loading && !show_list)).then(|| task_skeleton().flex_1()),
            )
            .children((connected && show_list).then(|| self.issue_list(cx)))
            .children(
                (connected && !show_list && !loading && error.is_none()).then(|| {
                    column().flex_1().justify_center().child(task_empty_state(
                        empty_title,
                        "Filter applies to loaded issues. Refresh to check for updates.",
                    ))
                }),
            )
            .children(
                (connected && !show_list && !loading && error.is_some()).then(|| div().flex_1()),
            )
            .children((progress > 0.).then(|| {
                self.detail_panel(
                    progress,
                    (f32::from(window.viewport_size().height) * 0.42).min(430.),
                    disabled,
                    cx,
                )
            }))
    }
}
