//! Immediate status changes. Reads belong to this view; writes to IntegrationsState.
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::integrations::{
    Account, Provider, TaskItem, TaskRef, WriteError, client, credentials,
    status::{self, StatusChange, StatusField},
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName,
        select::{SelectEvent, SelectState},
    },
    *,
};
use std::path::PathBuf;

fn status_snapshot(task: &TaskItem) -> Option<serde_json::Value> {
    match task.reference.project.provider {
        Provider::Github => Some(serde_json::json!(
            task.state == canopy_desktop::integrations::TaskState::Open
        )),
        // Other fields can change which workflow actions are available.
        Provider::Jira => task
            .jira
            .as_ref()
            .filter(|details| details.complete)
            .map(|details| serde_json::json!([details.status, details.fields])),
        Provider::Youtrack => task
            .youtrack
            .as_ref()
            .filter(|details| details.complete)
            .map(|details| serde_json::json!([details.status, details.fields["customFields"]])),
    }
}

type StatusSelect = SelectState<Vec<SelectOption>>;

struct StatusControl {
    field: StatusField,
    label: String,
    has_choices: bool,
    select: Entity<StatusSelect>,
    _event: Subscription,
}

#[derive(Clone)]
pub struct StatusFormRequested {
    pub transition: String,
}

pub struct TaskStatus {
    reference: TaskRef,
    path: PathBuf,
    account: Option<Account>,
    current_label: String,
    snapshot: serde_json::Value,
    controls: Vec<StatusControl>,
    enabled: bool,
    loading: bool,
    ready: bool,
    refresh_required: bool,
    pending: Option<u64>,
    error: Option<String>,
    notice: Option<String>,
    feedback: ButtonLoading,
    generation: u64,
    read: Option<Task<()>>,
    _events: Vec<Subscription>,
}

impl EventEmitter<StatusFormRequested> for TaskStatus {}

impl TaskStatus {
    pub fn new(
        task: &TaskItem,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let account = state
            .read(cx)
            .config
            .account_for(&task.reference.project)
            .cloned();
        let events = vec![
            cx.subscribe_in(
                &state,
                window,
                |this, _, event: &WriteFinished, window, cx| {
                    if this.pending != Some(event.id) {
                        return;
                    }
                    this.pending = None;
                    this.feedback.set(false, cx);
                    if !this.valid_scope(cx) {
                        return;
                    }
                    match &event.result {
                        Ok(receipt) => {
                            this.error = None;
                            this.notice = receipt.notice.clone();
                            if let Some(task) = &receipt.task {
                                this.adopt_current(task, window, cx);
                                this.load(window, cx);
                            } else {
                                // An accepted write is not a failure, but its target state
                                // must not be guessed when a workflow or reload is involved.
                                this.refresh_required = true;
                                this.notice = Some(receipt.notice.clone().unwrap_or_else(|| {
                                    "Status saved. Refresh to load the current status.".into()
                                }));
                            }
                        }
                        Err(error) => {
                            this.refresh_required = matches!(error, WriteError::Uncertain(_));
                            this.error = Some(error.to_string());
                        }
                    }
                    this.restore_selection(window, cx);
                    cx.notify();
                },
            ),
            cx.subscribe_in(&state, window, |this, _, task: &TaskItem, window, cx| {
                if !task.reference.same_task(&this.reference)
                    || this.pending.is_some()
                    || !this.valid_scope(cx)
                {
                    return;
                }
                if let Some(snapshot) = status_snapshot(task)
                    && this.snapshot != snapshot
                {
                    this.adopt_current(task, window, cx);
                    this.load(window, cx);
                }
            }),
            cx.observe(&state, |this, _, cx| {
                if !this.valid_scope(cx) {
                    this.cancel_read(cx);
                    this.ready = false;
                    this.controls.clear();
                }
                cx.notify();
            }),
            cx.observe(&cx.global::<AppState>().settings.clone(), |_, _, cx| {
                cx.notify()
            }),
        ];
        let mut this = Self {
            reference: task.reference.clone(),
            path,
            account,
            current_label: String::new(),
            snapshot: serde_json::Value::Null,
            controls: vec![],
            enabled: false,
            loading: false,
            ready: false,
            refresh_required: false,
            pending: None,
            error: None,
            notice: None,
            feedback: ButtonLoading::default(),
            generation: 0,
            read: None,
            _events: events,
        };
        this.adopt_current(task, window, cx);
        this.load(window, cx);
        this
    }

    fn adopt_current(&mut self, task: &TaskItem, window: &mut Window, cx: &mut Context<Self>) {
        let fields = status::from_task(task);
        self.current_label = fields
            .first()
            .map(StatusField::label)
            .unwrap_or_else(|| "No status".into());
        self.snapshot = status_snapshot(task).unwrap_or_default();
        // Show the confirmed server state immediately. Options are reloaded
        // separately, so old workflow actions cannot remain selectable.
        self.ready = false;
        self.install(fields, window, cx);
    }

    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }

    fn valid_scope(&self, cx: &App) -> bool {
        let app = cx.global::<AppState>();
        let state = app.integrations.read(cx);
        self.account.is_some()
            && state.config.account_for(&self.reference.project) == self.account.as_ref()
            && state.path.as_ref() == Some(&self.path)
            && !app.settings.read(cx).quitting
    }

    fn can_refresh(&self, cx: &App) -> bool {
        self.enabled
            && self.pending.is_none()
            && !self.loading
            && self.valid_scope(cx)
            && !cx.global::<AppState>().integrations.read(cx).busy
    }

    fn editable(&self, cx: &App) -> bool {
        self.can_refresh(cx) && self.ready && !self.refresh_required
    }

    fn cancel_read(&mut self, cx: &App) {
        self.generation = self.generation.wrapping_add(1);
        self.read = None;
        self.loading = false;
        if self.pending.is_none() {
            self.feedback.set(false, cx);
        }
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() || !self.valid_scope(cx) {
            return;
        }
        self.cancel_read(cx);
        let Some(account) = self.account.clone() else {
            return;
        };
        let reference = self.reference.clone();
        let generation = self.generation;
        let http = cx.http_client();
        self.loading = true;
        self.feedback.set(true, cx);
        self.read = Some(cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let credential = account.credential.clone();
                let token = cx
                    .background_executor()
                    .spawn(async move { credentials::load(&credential) })
                    .await?;
                let provider = client(http, &account, token)?;
                status::load(provider.as_ref(), &reference).await
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != generation || !this.valid_scope(cx) {
                    return;
                }
                this.loading = false;
                this.feedback.set(false, cx);
                match result {
                    Ok(fields) => {
                        this.install(fields, window, cx);
                        this.ready = true;
                        this.refresh_required = false;
                        this.error = None;
                        this.notice = None;
                    }
                    Err(error) => {
                        this.ready = false;
                        this.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn install(&mut self, fields: Vec<StatusField>, window: &mut Window, cx: &mut Context<Self>) {
        let mut old = std::mem::take(&mut self.controls);
        for field in fields {
            let choices = field.choices();
            let has_choices = !choices.is_empty();
            let mut items: Vec<_> = choices
                .into_iter()
                .map(|choice| SelectOption::new(choice.value, choice.label))
                .collect();
            let label = field.label();
            if let Some(current) = field.selected()
                && !items.iter().any(|item| item.value.as_ref() == current)
            {
                // Keep the current name even if its value is archived/unavailable.
                items.insert(0, SelectOption::new(current.to_owned(), label.clone()));
            }
            let (select, event) = if let Some(index) = old
                .iter()
                .position(|control| control.field.id() == field.id())
            {
                let control = old.remove(index);
                (control.select, control._event)
            } else {
                let select =
                    cx.new(|cx| StatusSelect::new(vec![], None, window, cx).searchable(true));
                let id = field.id().to_owned();
                let event = cx.subscribe_in(
                    &select,
                    window,
                    move |this, _, event: &SelectEvent<Vec<SelectOption>>, window, cx| {
                        if let SelectEvent::Confirm(Some(choice)) = event {
                            this.choose(&id, choice, window, cx);
                        }
                    },
                );
                (select, event)
            };
            select.update(cx, |select, cx| select.set_items(items, window, cx));
            self.controls.push(StatusControl {
                field,
                label,
                has_choices,
                select,
                _event: event,
            });
        }
        self.restore_selection(window, cx);
    }

    fn restore_selection(&self, window: &mut Window, cx: &mut Context<Self>) {
        for control in &self.controls {
            control.select.update(cx, |select, cx| {
                if let Some(id) = control.field.selected() {
                    select.set_selected_value(&SharedString::from(id.to_owned()), window, cx);
                } else {
                    // Event names are actions, never the new confirmed status label.
                    select.set_selected_index(None, window, cx);
                }
            });
        }
    }

    fn choose(&mut self, id: &str, choice: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            self.restore_selection(window, cx);
            return;
        }
        let Some(control) = self
            .controls
            .iter()
            .find(|control| control.field.id() == id)
        else {
            return;
        };
        let command = match control.field.choose(&self.reference, choice) {
            Ok(None) => {
                self.restore_selection(window, cx);
                return;
            }
            Err(error) => {
                self.error = Some(error);
                self.restore_selection(window, cx);
                cx.notify();
                return;
            }
            Ok(Some(StatusChange::JiraForm { transition })) => {
                self.restore_selection(window, cx);
                cx.emit(StatusFormRequested { transition });
                return;
            }
            Ok(Some(StatusChange::Write(command))) => *command,
        };
        self.error = None;
        self.notice = None;
        // Invalidate any pre-write read. No form draft participates in this write.
        self.cancel_read(cx);
        match cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |state, cx| state.submit(command, None, cx))
        {
            Ok(id) => {
                self.pending = Some(id);
                self.feedback.set(true, cx);
            }
            Err(error) => self.error = Some(error),
        }
        // Do not present an optimistic choice as a confirmed remote state.
        self.restore_selection(window, cx);
        cx.notify();
    }
}

impl Render for TaskStatus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.editable(cx);
        column()
            .flex_shrink_0()
            .gap(px(6.))
            .child(
                row()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(t::muted())
                            .child("Status"),
                    )
                    .child(
                        loading_icon_button(
                            "refresh-task-status",
                            IconName::RotateCw,
                            if self.pending.is_some() {
                                "Updating status"
                            } else {
                                "Refresh status choices"
                            },
                            Some(&self.feedback),
                        )
                        .disabled(!self.can_refresh(cx) && !self.feedback.active())
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.can_refresh(cx) {
                                this.load(window, cx);
                            }
                        })),
                    ),
            )
            .children(self.controls.is_empty().then(|| {
                column()
                    .gap(px(4.))
                    .child(badge(self.current_label.clone()))
                    .children((self.ready && !self.loading).then(|| {
                        integration_message("No status field is available for this issue.", false)
                    }))
            }))
            .children(self.controls.iter().map(|control| {
                column()
                    .flex_shrink_0()
                    .gap(px(4.))
                    .children((self.controls.len() > 1).then(|| {
                        div()
                            .text_size(px(10.))
                            .text_color(t::muted())
                            .child(control.field.name().to_owned())
                    }))
                    .child(
                        dropdown(&control.select)
                            .w_full()
                            .h(px(28.))
                            .rounded(px(4.))
                            .menu_width(Length::Auto)
                            .placeholder(control.label.clone())
                            .search_placeholder("Search statuses…")
                            .accessibility_label(format!(
                                "{} — change status",
                                control.field.name()
                            ))
                            .disabled(
                                disabled
                                    || control.field.read_only_reason().is_some()
                                    || !control.has_choices,
                            ),
                    )
                    .children(
                        control
                            .field
                            .read_only_reason()
                            .or_else(|| {
                                (self.ready && !self.loading && !control.has_choices)
                                    .then_some("No status changes are currently available.")
                            })
                            .map(|reason| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child(reason)
                            }),
                    )
                    .children(
                        control
                            .field
                            .hint()
                            .filter(|_| control.has_choices)
                            .map(|hint| {
                                div().text_size(px(11.)).text_color(t::muted()).child(hint)
                            }),
                    )
            }))
            .children(
                self.error
                    .clone()
                    .map(|error| integration_message(error, true)),
            )
            .children(
                self.notice
                    .clone()
                    .map(|notice| integration_message(notice, false)),
            )
    }
}
