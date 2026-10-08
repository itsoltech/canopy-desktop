//! Persistent project/filter choices above the task list, separate from repository setup.
use crate::{
    app_state::{AppState, IntegrationsState},
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::integrations::{ProjectTarget, Provider, TaskOption, client, credentials};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName, Root,
        select::{SelectEvent, SelectState},
    },
    *,
};
pub struct TaskControls {
    state: Entity<IntegrationsState>,
    projects: Entity<SelectState<Vec<SelectOption>>>,
    filters: Entity<SelectState<Vec<SelectOption>>>,
    project_rows: Vec<TaskOption>,
    filter_rows: Vec<(String, String)>,
    scope: Option<String>,
    attempted: bool,
    loading: bool,
    browse_loading: ButtonLoading,
    more_loading: ButtonLoading,
    next: Option<String>,
    error: Option<String>,
    read: Option<Task<()>>,
    load_generation: u64,
    preferences: Option<WindowHandle<Root>>,
    _events: Vec<Subscription>,
}
impl TaskControls {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let projects = cx.new(|cx| {
            SelectState::new(Vec::<SelectOption>::new(), None, window, cx).searchable(true)
        });
        let filters = cx.new(|cx| {
            SelectState::new(Vec::<SelectOption>::new(), None, window, cx).searchable(true)
        });
        let events = vec![
            cx.observe(&cx.global::<AppState>().settings.clone(), |_, _, cx| {
                cx.notify()
            }),
            cx.observe_in(&state, window, |s, _, w, cx| s.sync(w, cx)),
            cx.subscribe(
                &projects,
                |s, _, event: &SelectEvent<Vec<SelectOption>>, cx| {
                    if s.state.read(cx).busy {
                        return;
                    }
                    if let SelectEvent::Confirm(Some(key)) = event {
                        let target = s.state.read(cx).target.clone();
                        if let Some(target) = target
                            && target.key != key.as_ref()
                            && let Some(site) = target.site
                        {
                            let result = match target.provider {
                                Provider::Jira => ProjectTarget::jira(&site, key),
                                Provider::Youtrack => ProjectTarget::youtrack(&site, key),
                                Provider::Github => {
                                    Err("GitHub does not browse tracker projects.".into())
                                }
                            };
                            s.error = result
                                .and_then(|target| {
                                    s.state.update(cx, |state, cx| {
                                        state.override_repository(Some(target), cx)
                                    })
                                })
                                .err();
                            cx.notify();
                        }
                    }
                },
            ),
            cx.subscribe(
                &filters,
                |s, _, event: &SelectEvent<Vec<SelectOption>>, cx| {
                    if s.state.read(cx).busy {
                        return;
                    }
                    if let SelectEvent::Confirm(Some(id)) = event {
                        s.error = s
                            .state
                            .update(cx, |state, cx| state.choose_task_filter(id, cx))
                            .err();
                        cx.notify();
                    }
                },
            ),
        ];
        let mut s = Self {
            state,
            projects,
            filters,
            project_rows: vec![],
            filter_rows: vec![],
            scope: None,
            attempted: false,
            loading: false,
            browse_loading: ButtonLoading::default(),
            more_loading: ButtonLoading::default(),
            next: None,
            error: None,
            read: None,
            load_generation: 0,
            preferences: None,
            _events: events,
        };
        s.sync(window, cx);
        s
    }
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let target = state.target.clone();
        let account = state.active_account().cloned();
        let visible = state.tasks_visible();
        let busy = state.busy;
        let scope = account
            .as_ref()
            .filter(|a| {
                a.provider
                    == target
                        .as_ref()
                        .map(|t| t.provider)
                        .unwrap_or(Provider::Github)
            })
            .map(|a| a.credential.clone());
        let provider = target.as_ref().map(|target| target.provider);
        let filters = provider
            .map(|provider| state.config.filters_for(provider))
            .unwrap_or_default();
        let selected = target
            .as_ref()
            .and_then(|t| state.config.selected_filter(t));
        if scope != self.scope {
            self.scope = scope;
            self.project_rows.clear();
            self.attempted = false;
            self.next = None;
            self.read = None;
            self.loading = false;
            self.browse_loading.set(false, cx);
            self.more_loading.set(false, cx);
            self.error = None;
        }
        if let Some(target) = &target
            && matches!(target.provider, Provider::Jira | Provider::Youtrack)
        {
            if !self.project_rows.iter().any(|p| p.value == target.key) {
                self.project_rows.push(TaskOption {
                    value: target.key.clone(),
                    label: target.key.clone(),
                });
                self.update_projects(window, cx);
            }
            if !busy
                && self.projects.read(cx).selected_value().map(|v| v.as_ref())
                    != Some(target.key.as_str())
            {
                self.projects.update(cx, |s, cx| {
                    s.set_selected_value(&target.key.clone().into(), window, cx)
                });
            }
            let rows = filters
                .into_iter()
                .map(|f| (f.id, f.name))
                .collect::<Vec<_>>();
            if rows != self.filter_rows {
                self.filter_rows = rows;
                let items = self
                    .filter_rows
                    .iter()
                    .map(|(id, name)| SelectOption::new(id.clone(), name.clone()))
                    .collect();
                self.filters
                    .update(cx, |s, cx| s.set_items(items, window, cx));
            }
            if !busy
                && let Some(filter) = selected
                && self.filters.read(cx).selected_value().map(|v| v.as_ref())
                    != Some(filter.id.as_str())
            {
                self.filters.update(cx, |s, cx| {
                    s.set_selected_value(&filter.id.into(), window, cx)
                });
            }
            if visible && !self.attempted && !self.loading && self.scope.is_some() {
                self.load(false, window, cx);
            }
        }
        cx.notify();
    }
    fn update_projects(&self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self
            .project_rows
            .iter()
            .map(|p| SelectOption::new(p.value.clone(), p.label.clone()))
            .collect();
        self.projects
            .update(cx, |s, cx| s.set_items(items, window, cx));
    }
    fn load(&mut self, more: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let state = self.state.read(cx);
        let (Some(account), Some(target)) = (state.active_account().cloned(), state.target.clone())
        else {
            return;
        };
        if !matches!(target.provider, Provider::Jira | Provider::Youtrack) {
            return;
        }
        let http = cx.http_client();
        let scope = account.credential.clone();
        self.load_generation = self.load_generation.wrapping_add(1);
        let load_generation = self.load_generation;
        let cursor = if more { self.next.clone() } else { None };
        self.loading = true;
        self.browse_loading.set(!more, cx);
        self.more_loading.set(more, cx);
        self.attempted = true;
        self.error = None;
        self.read = Some(cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let key = account.credential.clone();
                let token = cx
                    .background_executor()
                    .spawn(async move { credentials::load(&key) })
                    .await?;
                client(http, &account, token)?
                    .projects(cursor.as_deref())
                    .await
            }
            .await;
            let _ = this.update_in(cx, |s, window, cx| {
                if s.load_generation != load_generation {
                    return;
                }
                if s.scope.as_ref() != Some(&scope) {
                    s.loading = false;
                    s.browse_loading.set(false, cx);
                    s.more_loading.set(false, cx);
                    return;
                }
                s.loading = false;
                s.browse_loading.set(false, cx);
                s.more_loading.set(false, cx);
                match result {
                    Ok(page) => {
                        if !more {
                            s.project_rows.clear();
                        }
                        for p in page.items {
                            if let Some(old) =
                                s.project_rows.iter_mut().find(|old| old.value == p.value)
                            {
                                *old = p;
                            } else {
                                s.project_rows.push(p);
                            }
                        }
                        s.project_rows.truncate(1000);
                        s.next = if s.project_rows.len() < 1000 {
                            page.next_cursor
                        } else {
                            None
                        };
                        s.update_projects(window, cx);
                        s.sync(window, cx);
                    }
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
impl Render for TaskControls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        if state
            .target
            .as_ref()
            .is_none_or(|t| !matches!(t.provider, Provider::Jira | Provider::Youtrack))
        {
            return column();
        }
        let disabled =
            state.busy || !state.ready || cx.global::<AppState>().settings.read(cx).quitting;
        column()
            .gap(px(8.))
            .flex_shrink_0()
            .child(
                row()
                    .gap(px(6.))
                    .child(
                        dropdown(&self.projects)
                            .flex_1()
                            .min_w_0()
                            .disabled(disabled),
                    )
                    .child(
                        loading_icon_button(
                            "refresh-jira-projects",
                            IconName::RotateCw,
                            "Refresh Jira projects",
                            Some(&self.browse_loading),
                        )
                        .disabled((disabled || self.loading) && !self.browse_loading.active())
                        .on_click(cx.listener(|s, _, w, cx| s.load(false, w, cx))),
                    ),
            )
            .children(self.loading.then(|| {
                div()
                    .text_size(px(10.))
                    .text_color(t::muted())
                    .child("Loading projects…")
            }))
            .children(self.next.is_some().then(|| {
                loading_button(
                    "more-jira-projects",
                    "Load more projects",
                    &self.more_loading,
                )
                .disabled((disabled || self.loading) && !self.more_loading.active())
                .on_click(cx.listener(|s, _, w, cx| s.load(true, w, cx)))
            }))
            .child(
                row()
                    .gap(px(6.))
                    .child(
                        dropdown(&self.filters)
                            .flex_1()
                            .min_w_0()
                            .disabled(disabled),
                    )
                    .child(
                        icon_button(
                            "manage-task-filters",
                            IconName::Settings2,
                            "Manage task filters",
                        )
                        .on_click(cx.listener(|s, _, _, cx| {
                            if let Some(w) = s.preferences
                                && w.update(cx, |_, window, _| window.activate_window())
                                    .is_ok()
                            {
                                return;
                            }
                            match super::preferences::open_task_filters(cx) {
                                Ok(w) => s.preferences = Some(w),
                                Err(e) => s.error = Some(e.to_string()),
                            }
                            cx.notify();
                        })),
                    ),
            )
            .children(self.error.clone().map(|e| integration_message(e, true)))
    }
}
