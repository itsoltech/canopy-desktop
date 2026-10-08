use crate::{
    app_state::AppState,
    ui::{components::integrations::integration_message, components::*},
};
use canopy_desktop::integrations::{AccountScope, ProjectTarget, Provider, client, credentials};
use gpui_kit::{
    base::Disableable,
    component::{
        input::InputState,
        select::{SelectEvent, SelectState},
    },
    *,
};
pub struct TaskSourcePicker {
    account: Entity<SelectState<Vec<SelectOption>>>,
    project: Entity<InputState>,
    projects: Entity<SelectState<Vec<SelectOption>>>,
    rows: Vec<SelectOption>,
    next: Option<String>,
    loading: bool,
    browse_loading: ButtonLoading,
    more_loading: ButtonLoading,
    error: Option<String>,
    read: Option<Task<()>>,
    _events: Vec<Subscription>,
}
pub struct SourceApplied;
impl EventEmitter<SourceApplied> for TaskSourcePicker {}
impl TaskSourcePicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let account = cx.new(|cx| SelectState::new(Vec::<SelectOption>::new(), None, window, cx));
        let projects = cx.new(|cx| SelectState::new(Vec::<SelectOption>::new(), None, window, cx));
        let project =
            cx.new(|cx| InputState::new(window, cx).placeholder("Project key or owner/repository"));
        let events = vec![
            cx.subscribe_in(
                &account,
                window,
                |this, _, _: &SelectEvent<Vec<SelectOption>>, window, cx| {
                    this.read = None;
                    this.loading = false;
                    this.browse_loading.set(false, cx);
                    this.more_loading.set(false, cx);
                    this.rows.clear();
                    this.next = None;
                    this.error = None;
                    this.project.update(cx, |s, cx| s.set_value("", window, cx));
                    this.projects
                        .update(cx, |s, cx| s.set_items(vec![], window, cx));
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &projects,
                window,
                |this, _, e: &SelectEvent<Vec<SelectOption>>, window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = e {
                        this.project
                            .update(cx, |s, cx| s.set_value(value.clone(), window, cx));
                    }
                },
            ),
        ];
        let mut s = Self {
            account,
            project,
            projects,
            rows: vec![],
            next: None,
            loading: false,
            browse_loading: ButtonLoading::default(),
            more_loading: ButtonLoading::default(),
            error: None,
            read: None,
            _events: events,
        };
        s.reset(window, cx);
        s
    }
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.read = None;
        self.loading = false;
        self.browse_loading.set(false, cx);
        self.more_loading.set(false, cx);
        self.rows.clear();
        self.next = None;
        self.error = None;
        let state = cx.global::<AppState>().integrations.read(cx);
        let target = state.target.clone();
        let mut items = vec![SelectOption::new("github", "GitHub")];
        items.extend(
            state
                .config
                .accounts
                .iter()
                .filter(|a| a.provider == Provider::Jira)
                .map(|a| SelectOption::new(a.id.clone(), a.scope.label())),
        );
        items.extend(
            state
                .config
                .accounts
                .iter()
                .filter(|a| a.provider == Provider::Youtrack)
                .map(|a| SelectOption::new(a.id.clone(), a.scope.label())),
        );
        let selected = target
            .as_ref()
            .and_then(|t| state.config.account_for(t))
            .filter(|a| matches!(a.provider, Provider::Jira | Provider::Youtrack))
            .map(|a| a.id.clone())
            .unwrap_or_else(|| "github".into());
        self.account.update(cx, |s, cx| {
            s.set_items(items, window, cx);
            s.set_selected_value(&selected.into(), window, cx);
        });
        self.project.update(cx, |s, cx| {
            s.set_value(target.map(|t| t.key).unwrap_or_default(), window, cx)
        });
        cx.notify();
    }
    fn selection(&self, cx: &App) -> String {
        self.account
            .read(cx)
            .selected_value()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "github".into())
    }
    fn load(&mut self, more: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let id = self.selection(cx);
        let Some(account) = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .config
            .accounts
            .iter()
            .find(|a| a.id == id)
            .cloned()
        else {
            return;
        };
        let cursor = if more { self.next.clone() } else { None };
        let http = cx.http_client();
        self.loading = true;
        self.browse_loading.set(!more, cx);
        self.more_loading.set(more, cx);
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
                if s.selection(cx) != id {
                    return;
                }
                s.loading = false;
                s.browse_loading.set(false, cx);
                s.more_loading.set(false, cx);
                match result {
                    Ok(result) => {
                        if !more {
                            s.rows.clear();
                        }
                        s.rows.extend(
                            result
                                .items
                                .into_iter()
                                .map(|o| SelectOption::new(o.value, o.label)),
                        );
                        s.next = result.next_cursor;
                        s.projects
                            .update(cx, |v, cx| v.set_items(s.rows.clone(), window, cx));
                    }
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn apply(&mut self, origin: bool, cx: &mut Context<Self>) {
        self.read = None;
        self.loading = false;
        self.browse_loading.set(false, cx);
        self.more_loading.set(false, cx);
        let result = (|| {
            if origin {
                return Ok(None);
            }
            let value = self.project.read(cx).value();
            let selected = self.selection(cx);
            if selected == "github" {
                return ProjectTarget::github(&value).map(Some);
            }
            let state = cx.global::<AppState>().integrations.read(cx);
            let account = state
                .config
                .accounts
                .iter()
                .find(|a| a.id == selected)
                .ok_or("This Jira connection no longer exists.")?;
            match &account.scope {
                AccountScope::Jira { site, .. } => ProjectTarget::jira(site, &value).map(Some),
                AccountScope::Youtrack { service } => {
                    ProjectTarget::youtrack(service, &value).map(Some)
                }
                AccountScope::Default | AccountScope::Owner(_) => {
                    Err("Select a task tracker connection.".into())
                }
            }
        })();
        self.error = match result {
            Ok(target) => cx
                .global::<AppState>()
                .integrations
                .clone()
                .update(cx, |s, cx| s.override_repository(target, cx))
                .err(),
            Err(e) => Some(e),
        };
        if self.error.is_none() {
            cx.emit(SourceApplied);
        }
        cx.notify();
    }
}
impl Render for TaskSourcePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = cx.global::<AppState>().integrations.read(cx).busy || self.loading;
        column()
            .gap(px(10.))
            .child(form_field(
                "Task source",
                "Shared by this repository's worktrees.",
                dropdown(&self.account).w_full().disabled(disabled),
            ))
            .child(input(&self.project).w_full().disabled(disabled))
            .children((self.selection(cx) != "github").then(|| {
                column()
                    .gap(px(8.))
                    .child(
                        loading_button(
                            "jira-projects",
                            "Browse Jira projects",
                            &self.browse_loading,
                        )
                        .disabled(disabled && !self.browse_loading.active())
                        .on_click(cx.listener(|s, _, w, cx| s.load(false, w, cx))),
                    )
                    .children(
                        (!self.rows.is_empty())
                            .then(|| dropdown(&self.projects).w_full().disabled(disabled)),
                    )
                    .children(self.next.is_some().then(|| {
                        loading_button(
                            "jira-projects-more",
                            "Load more projects",
                            &self.more_loading,
                        )
                        .disabled(disabled && !self.more_loading.active())
                        .on_click(cx.listener(|s, _, w, cx| s.load(true, w, cx)))
                    }))
            }))
            .children(self.error.clone().map(|e| integration_message(e, true)))
            .child(
                row()
                    .gap(px(8.))
                    .justify_end()
                    .child(
                        button("task-origin", "Use origin")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| s.apply(true, cx))),
                    )
                    .child(
                        primary_button("task-source-apply", "Apply")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| s.apply(false, cx))),
                    ),
            )
    }
}
