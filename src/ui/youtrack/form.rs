use crate::{
    app_state::{AppState, WriteFinished},
    ui::{
        components::integrations::integration_message, components::*, task_edit::TaskEditRequest,
        theme as t,
    },
};
use canopy_desktop::integrations::{
    IssueDraft, Provider, TaskWrite, client, credentials,
    drafts::TaskDraft,
    youtrack::{self, YoutrackField, YoutrackFieldKind, YoutrackWrite},
};
use canopy_desktop::motion;
use gpui_kit::{
    base::Disableable,
    component::{
        combobox::{ComboboxEvent, ComboboxState},
        input::{InputEvent, InputState},
        scroll::ScrollableElement,
        select::SearchableVec,
    },
    *,
};
use serde_json::{Map, Value};
use std::time::Instant;

#[derive(Clone)]
pub struct YoutrackFieldChanged;
impl EventEmitter<YoutrackFieldChanged> for YoutrackFieldInput {}

pub struct YoutrackFieldInput {
    field: YoutrackField,
    control: FieldControl,
    initial: String,
    initial_boolean: bool,
    boolean_value: bool,
    initial_choices: Vec<String>,
    selected: Vec<String>,
    choice_changed: bool,
    enabled: bool,
    _events: Vec<Subscription>,
}

type ChoiceState = ComboboxState<SearchableVec<SelectOption>>;

enum FieldControl {
    Boolean,
    Text(Entity<InputState>),
    Choices(Entity<ChoiceState>),
}

fn value_text(value: &Value, multi: bool, kind: YoutrackFieldKind) -> String {
    if value.is_null() {
        return String::new();
    }
    if multi && let Some(values) = value.as_array() {
        return values
            .iter()
            .map(|value| {
                value["name"]
                    .as_str()
                    .or(value["login"].as_str())
                    .or(value["id"].as_str())
                    .or(value["text"].as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string())
            })
            .collect::<Vec<_>>()
            .join(", ");
    }
    if let Some(text) = value.as_str() {
        return text.into();
    }
    if let Some(text) = value["text"].as_str() {
        return text.into();
    }
    if let Some(text) = value["name"]
        .as_str()
        .or(value["login"].as_str())
        .or(value["id"].as_str())
    {
        return text.into();
    }
    if let Some(minutes) = value["minutes"].as_i64() {
        return minutes.to_string();
    }
    if let Some(ms) = value.as_i64() {
        if kind == YoutrackFieldKind::Date {
            return chrono::DateTime::from_timestamp_millis(ms)
                .map(|date| date.date_naive().to_string())
                .unwrap_or_else(|| ms.to_string());
        }
        return ms.to_string();
    }
    value.to_string()
}

impl YoutrackFieldInput {
    pub fn new(
        mut field: YoutrackField,
        use_default: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if use_default && field.value.is_null() && !field.default.is_null() {
            field.value = field.default.clone();
        }
        let initial_boolean = field
            .value
            .as_bool()
            .or(use_default.then(|| field.default.as_bool()).flatten())
            .unwrap_or(false);
        // Project metadata normally carries the complete bundle. Issue
        // responses can still contain a selected reference that is outside a
        // paged/permission-filtered bundle, so retain that value as a named
        // option instead of falling back to a free-form ID field.
        if field.kind != YoutrackFieldKind::StateMachine {
            let current_values = if field.multi_value {
                field.value.as_array().cloned().unwrap_or_default()
            } else if field.value.is_null() {
                Vec::new()
            } else {
                vec![field.value.clone()]
            };
            for current in current_values {
                if let Some(value) = choice_value(&current)
                    && !field
                        .allowed
                        .iter()
                        .any(|candidate| choice_value(candidate).as_deref() == Some(value.as_str()))
                {
                    field.allowed.push(current);
                }
            }
        }
        let initial = value_text(&field.value, field.multi_value, field.kind);
        let initial_choices: Vec<String> = if field.kind == YoutrackFieldKind::StateMachine {
            // The current state is informational; only a workflow event is a
            // writable selection for this field.
            Vec::new()
        } else if field.multi_value {
            field
                .value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(choice_value)
                .collect()
        } else {
            choice_value(&field.value).into_iter().collect()
        };
        let mut events = Vec::new();
        let is_choice_field = matches!(
            field.kind,
            YoutrackFieldKind::Enum
                | YoutrackFieldKind::State
                | YoutrackFieldKind::StateMachine
                | YoutrackFieldKind::User
                | YoutrackFieldKind::Group
                | YoutrackFieldKind::Version
                | YoutrackFieldKind::Build
                | YoutrackFieldKind::OwnedField
        );
        let control = if field.kind == YoutrackFieldKind::Boolean {
            FieldControl::Boolean
        } else if is_choice_field {
            let options: Vec<SelectOption> = if field.kind == YoutrackFieldKind::StateMachine {
                field
                    .events
                    .iter()
                    .map(|event| SelectOption::new(event.value.clone(), event.label.clone()))
                    .collect()
            } else {
                field
                    .allowed
                    .iter()
                    .filter_map(|value| {
                        Some(SelectOption::new(choice_value(value)?, choice_label(value)))
                    })
                    .collect()
            };
            let selected_values = initial_choices
                .iter()
                .cloned()
                .map(Into::into)
                .collect::<Vec<_>>();
            let select = cx.new(|cx| {
                ComboboxState::new(SearchableVec::new(options), vec![], window, cx)
                    .multiple(field.multi_value)
                    .searchable(true)
            });
            select.update(cx, |select, cx| {
                select.set_selected_values(&selected_values, window, cx)
            });
            events.push(cx.subscribe(
                &select,
                |this, _, event: &ComboboxEvent<SearchableVec<SelectOption>>, cx| {
                    if let ComboboxEvent::Change(values) = event {
                        this.selected = values.iter().map(ToString::to_string).collect();
                        this.choice_changed = true;
                        cx.emit(YoutrackFieldChanged);
                        cx.notify();
                    }
                },
            ));
            FieldControl::Choices(select)
        } else {
            let input = cx.new(|cx| InputState::new(window, cx));
            input.update(cx, |input, cx| input.set_value(initial.clone(), window, cx));
            events.push(cx.subscribe(&input, |_, _, _: &InputEvent, cx| {
                cx.emit(YoutrackFieldChanged);
                cx.notify();
            }));
            FieldControl::Text(input)
        };
        Self {
            field,
            control,
            initial,
            initial_boolean,
            boolean_value: initial_boolean,
            initial_choices: initial_choices.clone(),
            selected: initial_choices,
            choice_changed: false,
            enabled: true,
            _events: events,
        }
    }

    pub fn id(&self) -> &str {
        &self.field.id
    }

    pub fn field_type(&self) -> &str {
        &self.field.field_type
    }

    pub fn dirty(&self, cx: &App) -> bool {
        match &self.control {
            FieldControl::Boolean => self.boolean_value != self.initial_boolean,
            FieldControl::Text(input) => input.read(cx).value().as_ref() != self.initial,
            FieldControl::Choices(_) => {
                self.choice_changed || self.selected != self.initial_choices
            }
        }
    }

    pub fn draft(&self, cx: &App) -> Value {
        Value::Object(match &self.control {
            FieldControl::Boolean => [("boolean".into(), Value::Bool(self.boolean_value))]
                .into_iter()
                .collect(),
            FieldControl::Text(input) => [(
                "text".into(),
                Value::String(input.read(cx).value().to_string()),
            )]
            .into_iter()
            .collect(),
            FieldControl::Choices(_) => [(
                "choices".into(),
                Value::Array(self.selected.iter().cloned().map(Value::String).collect()),
            )]
            .into_iter()
            .collect(),
        })
    }

    pub fn restore(&mut self, value: &Value, window: &mut Window, cx: &mut Context<Self>) {
        match &self.control {
            FieldControl::Boolean => {
                if let Some(value) = value["boolean"].as_bool() {
                    self.boolean_value = value;
                }
            }
            FieldControl::Text(input) => {
                if let Some(text) = value["text"].as_str() {
                    input.update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
                }
            }
            FieldControl::Choices(select) => {
                if let Some(values) = value["choices"].as_array() {
                    self.selected = values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    self.choice_changed = false;
                    let values = self
                        .selected
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect::<Vec<_>>();
                    select.update(cx, |select, cx| {
                        select.set_selected_values(&values, window, cx)
                    });
                }
            }
        }
    }

    pub fn value(&self, cx: &App) -> Result<Value, String> {
        if !self.dirty(cx) {
            return Ok(self.field.value.clone());
        }
        let text = match &self.control {
            FieldControl::Boolean => return Ok(Value::Bool(self.boolean_value)),
            FieldControl::Text(input) => input.read(cx).value().to_string(),
            FieldControl::Choices(_) => String::new(),
        };
        if let FieldControl::Choices(_) = &self.control {
            if self.selected.is_empty() {
                if self.field.required {
                    return Err(format!("{} is required.", self.field.name));
                }
                return Ok(Value::Null);
            }
            let values = self
                .selected
                .iter()
                .map(|value| {
                    if self.field.kind == YoutrackFieldKind::StateMachine {
                        return serde_json::json!({"id": value});
                    }
                    self.field
                        .allowed
                        .iter()
                        .find(|candidate| {
                            choice_value(candidate).as_deref() == Some(value.as_str())
                        })
                        .cloned()
                        .unwrap_or_else(|| match self.field.kind {
                            YoutrackFieldKind::User => serde_json::json!({"login": value}),
                            YoutrackFieldKind::Group => serde_json::json!({"id": value}),
                            _ => serde_json::json!({"name": value}),
                        })
                })
                .collect::<Vec<_>>();
            return if self.field.multi_value {
                Ok(Value::Array(values))
            } else {
                Ok(values.into_iter().next().unwrap_or(Value::Null))
            };
        }
        if self.field.multi_value {
            if self.selected.is_empty() {
                if self.field.required {
                    return Err(format!("{} is required.", self.field.name));
                }
                return Ok(Value::Null);
            }
            let values = self
                .selected
                .iter()
                .map(|value| {
                    self.field
                        .allowed
                        .iter()
                        .find(|candidate| {
                            choice_value(candidate).as_deref() == Some(value.as_str())
                        })
                        .cloned()
                        .unwrap_or_else(|| match self.field.kind {
                            YoutrackFieldKind::User => serde_json::json!({"login":value}),
                            YoutrackFieldKind::Group => serde_json::json!({"id":value}),
                            _ => serde_json::json!({"name":value}),
                        })
                })
                .collect::<Vec<_>>();
            return Ok(Value::Array(values));
        }
        youtrack::value_for_field(&self.field, &text)
    }

    pub fn state_event(&self, cx: &App) -> Result<Option<String>, String> {
        if self.field.kind != YoutrackFieldKind::StateMachine || !self.dirty(cx) {
            return Ok(None);
        }
        let value = match &self.control {
            FieldControl::Boolean => String::new(),
            FieldControl::Text(input) => input.read(cx).value().to_string(),
            FieldControl::Choices(_) => self.selected.first().cloned().unwrap_or_default(),
        };
        let value = value.trim();
        self.field
            .events
            .iter()
            .find(|event| event.value == value || event.label.eq_ignore_ascii_case(value))
            .map(|event| event.value.clone())
            .ok_or_else(|| format!("Choose a valid event for {}.", self.field.name))
            .map(Some)
    }

    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }
}

impl Render for YoutrackFieldInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let help = match self.field.kind {
            YoutrackFieldKind::Date => "YYYY-MM-DD",
            YoutrackFieldKind::Unsupported => "This field is preserved as a structured value.",
            _ => "",
        };
        let control = match &self.control {
            FieldControl::Boolean => checkbox(
                SharedString::from(format!("youtrack-boolean-{}", self.field.id)),
                self.boolean_value,
                self.field.name.clone(),
            )
            .disabled(!self.enabled || self.field.read_only)
            .on_click(cx.listener(|this, value, _, cx| {
                this.boolean_value = *value;
                cx.emit(YoutrackFieldChanged);
                cx.notify();
            }))
            .into_any_element(),
            FieldControl::Text(state) => input(state)
                .w_full()
                .disabled(
                    !self.enabled
                        || self.field.read_only
                        || self.field.kind == YoutrackFieldKind::Unsupported,
                )
                .into_any_element(),
            FieldControl::Choices(select) => {
                let multi = self.field.multi_value;
                let placeholder = match self.field.kind {
                    YoutrackFieldKind::User => "Choose a person",
                    YoutrackFieldKind::Group => "Choose a group",
                    YoutrackFieldKind::StateMachine => "Choose a workflow event",
                    _ => "Choose…",
                };
                column()
                    .gap(px(6.))
                    .child(
                        combobox(select)
                            .w_full()
                            .placeholder(placeholder)
                            .cleanable(!multi && !self.field.required)
                            .disabled(!self.enabled || self.field.read_only),
                    )
                    .children(
                        (self.field.allowed.is_empty()
                            && self.field.kind != YoutrackFieldKind::StateMachine)
                            .then(|| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child("No options were returned for this field. Check YouTrack permissions and retry.")
                            }),
                    )
                    .children(
                        (self.field.kind == YoutrackFieldKind::StateMachine
                            && self.field.events.is_empty())
                            .then(|| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child("No workflow events are available for this field.")
                            }),
                    )
                    .into_any_element()
            }
        };
        form_field(
            format!(
                "{}{}",
                self.field.name,
                if self.field.required { " *" } else { "" }
            ),
            help,
            control,
        )
    }
}

fn choice_value(value: &Value) -> Option<String> {
    value["login"]
        .as_str()
        .or(value["id"].as_str())
        .or(value["name"].as_str())
        .or(value["value"].as_str())
        .or(value.as_str())
        .map(str::to_owned)
}

fn choice_label(value: &Value) -> String {
    let label = value["presentation"]
        .as_str()
        .or(value["fullName"].as_str())
        .or(value["name"].as_str())
        .or(value["login"].as_str())
        .or(value["value"].as_str())
        .unwrap_or("Value")
        .to_owned();
    value["login"]
        .as_str()
        .filter(|login| !label.eq_ignore_ascii_case(login))
        .map(|login| format!("{label} (@{login})"))
        .unwrap_or(label)
}

fn primary_field(field: &YoutrackField) -> bool {
    let name = field.name.to_ascii_lowercase();
    field.required
        || matches!(
            field.kind,
            YoutrackFieldKind::State | YoutrackFieldKind::StateMachine | YoutrackFieldKind::User
        )
        || ["priority", "type", "assignee", "status"]
            .iter()
            .any(|part| name.contains(part))
}

fn compact_field_rank(field: &YoutrackField) -> u8 {
    let name = field.name.to_ascii_lowercase();
    if name == "type" || name.ends_with(" type") {
        0
    } else if matches!(
        field.kind,
        YoutrackFieldKind::State | YoutrackFieldKind::StateMachine
    ) || name.contains("state")
        || name.contains("status")
    {
        1
    } else if name.contains("priority") {
        2
    } else if name.contains("assignee") {
        if name.contains("old") { 4 } else { 3 }
    } else if matches!(field.kind, YoutrackFieldKind::User) {
        3
    } else {
        10
    }
}

impl EventEmitter<super::super::task_edit::issue_editor::IssueSaved> for YoutrackForm {}
impl EventEmitter<super::super::task_edit::issue_editor::IssueDeleted> for YoutrackForm {}

pub struct YoutrackForm {
    request: TaskEditRequest,
    title: Entity<InputState>,
    body: Entity<crate::ui::task_edit::composer::MarkdownComposer>,
    fields: Vec<Entity<YoutrackFieldInput>>,
    baseline: IssueDraft,
    baseline_fields: std::collections::BTreeMap<String, Value>,
    key: Option<String>,
    hydrated: bool,
    enabled: bool,
    loading: bool,
    pending: Option<u64>,
    submit_loading: ButtonLoading,
    error: Option<String>,
    schema_error: Option<String>,
    finished: bool,
    more_fields: Disclosure,
    read: Option<Task<()>>,
    _events: Vec<Subscription>,
}

impl YoutrackForm {
    pub fn new(request: TaskEditRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = request.project().clone();
        let state = cx.global::<AppState>().clone();
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Issue summary"));
        let base = request
            .task()
            .map(|task| task.reference.url())
            .unwrap_or_else(|| project.site.clone().unwrap_or_default());
        let body = cx.new(|cx| {
            crate::ui::task_edit::composer::MarkdownComposer::new(base, 220., window, cx)
        });
        let baseline = request
            .task()
            .map(|task| IssueDraft {
                title: task.reference.title.clone(),
                body: task.body.to_string(),
                ..Default::default()
            })
            .unwrap_or_default();
        let key = state
            .integrations
            .read(cx)
            .draft_key(&project, &request.draft_kind());
        let mut this = Self {
            request,
            title,
            body,
            fields: vec![],
            baseline,
            baseline_fields: Default::default(),
            key,
            hydrated: false,
            enabled: false,
            loading: true,
            pending: None,
            submit_loading: ButtonLoading::default(),
            error: None,
            schema_error: None,
            finished: false,
            more_fields: Disclosure::new(false, Instant::now()),
            read: None,
            _events: vec![],
        };
        this._events
            .push(cx.subscribe(&this.title, |this, _, _: &InputEvent, cx| {
                this.save_draft(cx);
            }));
        this._events.push(cx.subscribe(
            &this.body,
            |this, _, _: &crate::ui::task_edit::composer::ComposerChanged, cx| {
                this.save_draft(cx);
            },
        ));
        this._events.push(
            cx.observe_in(&state.task_drafts, window, |this, _, window, cx| {
                this.hydrate(window, cx);
                cx.notify();
            }),
        );
        this._events
            .push(cx.observe(&state.integrations, |this, _, cx| {
                this.sync_enabled(cx);
                cx.notify();
            }));
        this._events
            .push(cx.observe(&state.settings, |this, _, cx| {
                this.sync_enabled(cx);
                cx.notify();
            }));
        this._events.push(cx.subscribe_in(
            &state.integrations,
            window,
            |this, _, event: &WriteFinished, _, cx| {
                if this.pending != Some(event.id) {
                    return;
                }
                this.pending = None;
                this.submit_loading.set(false, cx);
                match &event.result {
                    Ok(receipt) if receipt.task.is_some() => {
                        this.finished = true;
                        this.error = receipt.notice.clone();
                        cx.emit(super::super::task_edit::issue_editor::IssueSaved(
                            receipt.task.clone().unwrap(),
                            receipt.notice.clone(),
                        ));
                    }
                    Ok(_) => {
                        this.error = Some(
                            "The change was saved, but YouTrack returned no issue details.".into(),
                        )
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                this.sync_enabled(cx);
                cx.notify();
            },
        ));
        this.load_schema(window, cx);
        this
    }

    fn load_schema(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.request.project().clone();
        let task = self.request.task().cloned();
        let Some(account) = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .config
            .account_for(&project)
            .cloned()
        else {
            self.loading = false;
            self.schema_error = Some("Connect this YouTrack service in Preferences.".into());
            return;
        };
        let http = cx.http_client();
        self.read = Some(cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let key = account.credential.clone();
                let token = cx
                    .background_executor()
                    .spawn(async move { credentials::load(&key) })
                    .await?;
                let provider = client(http, &account, token)?;
                if project.provider != Provider::Youtrack {
                    return Err("This form requires a YouTrack project.".into());
                }
                let schema = provider.youtrack_schema(&project).await?;
                Ok::<_, String>((schema, task))
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match result {
                    Ok((schema, task)) => {
                        this.fields.clear();
                        this.baseline_fields.clear();
                        for mut field in schema.fields {
                            if let Some(task) = &task
                                && let Some(details) = &task.youtrack
                                && let Some(current) = details
                                    .custom_fields
                                    .iter()
                                    .find(|current| current.id == field.id)
                            {
                                field = field.with_current_value(current);
                            }
                            // Existing issues change status immediately in details.
                            // Ignore old status drafts so saving other fields cannot
                            // overwrite a later workflow transition.
                            if task.is_some() && field.is_status() {
                                continue;
                            }
                            this.baseline_fields
                                .insert(field.id.clone(), field.value.clone());
                            let input = cx.new(|cx| {
                                YoutrackFieldInput::new(field, task.is_none(), window, cx)
                            });
                            this._events.push(
                                cx.subscribe(&input, |this, _, _: &YoutrackFieldChanged, cx| {
                                    this.save_draft(cx)
                                }),
                            );
                            this.fields.push(input);
                        }
                        this.hydrate(window, cx);
                        this.sync_enabled(cx);
                    }
                    Err(error) => this.schema_error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    fn retry_schema(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.pending.is_some() {
            return;
        }
        self.schema_error = None;
        self.fields.clear();
        self.hydrated = false;
        self.loading = true;
        self.load_schema(window, cx);
        cx.notify();
    }

    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        self.sync_enabled(cx);
    }

    fn can_edit(&self, cx: &App) -> bool {
        self.enabled
            && self.hydrated
            && !self.loading
            && !self.finished
            && self.pending.is_none()
            && !cx.global::<AppState>().integrations.read(cx).busy
            && !cx.global::<AppState>().settings.read(cx).quitting
    }

    fn sync_enabled(&mut self, cx: &mut Context<Self>) {
        let enabled = self.can_edit(cx);
        self.body.update(cx, |body, cx| body.enable(enabled, cx));
        for field in &self.fields {
            let visible = self.more_fields.open || primary_field(&field.read(cx).field);
            field.update(cx, |field, cx| field.enable(enabled && visible, cx));
        }
        cx.notify();
    }

    fn hydrate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hydrated || self.loading || self.finished {
            return;
        }
        let drafts = cx.global::<AppState>().task_drafts.read(cx);
        if !drafts.ready {
            return;
        }
        let draft = self
            .key
            .as_ref()
            .and_then(|key| drafts.values.get(key))
            .cloned();
        let value = draft
            .as_ref()
            .map(|draft| draft.value.clone())
            .unwrap_or_else(|| self.baseline.clone());
        self.title
            .update(cx, |input, cx| input.set_value(value.title, window, cx));
        self.body
            .update(cx, |body, cx| body.set_value(&value.body, window, cx));
        if let Some(fields) = draft.as_ref().map(|draft| &draft.value.fields) {
            for field in &self.fields {
                if let Some(value) = fields.get(field.read(cx).id()) {
                    field.update(cx, |field, cx| field.restore(value, window, cx));
                }
            }
        }
        self.error = draft.and_then(|draft| draft.warning);
        self.hydrated = true;
        self.sync_enabled(cx);
    }

    fn value(&self, cx: &App) -> IssueDraft {
        IssueDraft {
            title: self.title.read(cx).value().to_string(),
            body: self.body.read(cx).value(cx),
            fields: self
                .fields
                .iter()
                .map(|field| (field.read(cx).id().to_owned(), field.read(cx).draft(cx)))
                .collect(),
            ..Default::default()
        }
    }

    fn save_draft(&mut self, cx: &mut Context<Self>) {
        if !self.hydrated || self.finished || self.pending.is_some() {
            return;
        }
        let value = self.value(cx);
        let baseline = IssueDraft {
            title: self.baseline.title.clone(),
            body: self.baseline.body.clone(),
            ..Default::default()
        };
        if let Some(key) = &self.key {
            let drafts = cx.global::<AppState>().task_drafts.clone();
            if value.title == baseline.title
                && value.body == baseline.body
                && self.fields.iter().all(|field| !field.read(cx).dirty(cx))
            {
                drafts.update(cx, |state, cx| state.remove(key, cx));
            } else {
                drafts.update(cx, |state, cx| {
                    state.set(
                        key.clone(),
                        TaskDraft {
                            project: self.request.project().clone(),
                            value,
                            baseline: Some(baseline),
                            warning: None,
                        },
                        cx,
                    )
                });
            }
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.can_edit(cx) {
            return;
        }
        let draft = self.value(cx);
        if let Err(error) = draft.validate() {
            self.error = Some(error);
            cx.notify();
            return;
        }
        let mut fields = Map::new();
        if draft.title != self.baseline.title {
            fields.insert("summary".into(), Value::String(draft.title.clone()));
        }
        if draft.body != self.baseline.body {
            fields.insert("description".into(), Value::String(draft.body.clone()));
        }
        let mut state_event = None;
        let creating = self.request.task().is_none();
        for field in &self.fields {
            let field = field.read(cx);
            if !creating && !field.dirty(cx) {
                continue;
            }
            if creating
                && (field.field.read_only || field.field.kind == YoutrackFieldKind::Unsupported)
            {
                if field.field.required
                    && field.field.value.is_null()
                    && field.field.default.is_null()
                {
                    self.error = Some(format!(
                        "{} is required but cannot be edited in this form.",
                        field.field.name
                    ));
                    cx.notify();
                    return;
                }
                continue;
            }
            match field.state_event(cx) {
                Ok(Some(event)) if creating => {
                    fields.insert(
                        field.id().to_owned(),
                        serde_json::json!({
                            "$type": field.field_type(),
                            "value": {"id": event}
                        }),
                    );
                }
                Ok(Some(event)) => state_event = Some((field.id().to_owned(), event)),
                Ok(None) => match field.value(cx) {
                    Ok(value) => {
                        if creating
                            && field.field.required
                            && (value.is_null()
                                || value.as_str().is_some_and(|text| text.trim().is_empty())
                                || value.as_array().is_some_and(Vec::is_empty))
                        {
                            self.error = Some(format!("{} is required.", field.field.name));
                            cx.notify();
                            return;
                        }
                        if !value.is_null() || !creating {
                            fields.insert(
                                field.id().to_owned(),
                                serde_json::json!({"$type":field.field_type(),"value":value}),
                            );
                        }
                    }
                    Err(error) => {
                        self.error = Some(error);
                        cx.notify();
                        return;
                    }
                },
                Err(error) => {
                    self.error = Some(error);
                    cx.notify();
                    return;
                }
            }
        }
        let command = if let Some(task) = self.request.task() {
            if let Some((id, event_id)) = state_event {
                TaskWrite::Youtrack {
                    project: task.reference.project.clone(),
                    task: Some(task.reference.clone()),
                    action: YoutrackWrite::StateMachineEvent { id, event_id },
                }
            } else {
                TaskWrite::Youtrack {
                    project: task.reference.project.clone(),
                    task: Some(task.reference.clone()),
                    action: YoutrackWrite::Fields {
                        fields: Value::Object(fields),
                    },
                }
            }
        } else {
            TaskWrite::Youtrack {
                project: self.request.project().clone(),
                task: None,
                action: YoutrackWrite::Create {
                    draft,
                    fields: Value::Object(fields),
                },
            }
        };
        let result = cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |state, cx| state.submit(command, self.key.clone(), cx));
        match result {
            Ok(id) => {
                self.pending = Some(id);
                self.submit_loading.set(true, cx);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        self.sync_enabled(cx);
        cx.notify();
    }
}

impl Render for YoutrackForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.can_edit(cx);
        let mut primary_fields = Vec::new();
        let mut additional_fields = Vec::new();
        for field in &self.fields {
            let input = field.read(cx);
            if primary_field(&input.field) {
                primary_fields.push(field.clone());
            } else {
                additional_fields.push(field.clone());
            }
        }
        primary_fields.sort_by_key(|field| compact_field_rank(&field.read(cx).field));
        let now = Instant::now();
        motion::request_frame(window, self.more_fields.active(now));
        let more_count = additional_fields.len();
        column()
            .size_full()
            .gap(px(12.))
            .child(
                column()
                    .id("youtrack-form-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .gap(px(16.))
                    .child(form_field("Summary", "", input(&self.title).w_full().disabled(disabled)))
                    .child(form_field("Description", "", self.body.clone()))
                    .children((!primary_fields.is_empty()).then(|| field_grid(primary_fields)))
                    .children((more_count > 0).then(|| {
                        column()
                            .flex_shrink_0()
                            .gap(px(4.))
                            .child(
                                self.more_fields
                                    .header("youtrack-more-fields", "MORE FIELDS", now, cx)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.more_fields.toggle(Instant::now(), cx);
                                        this.sync_enabled(cx);
                                    })),
                            )
                            .child(self.more_fields.measured_body(
                                "youtrack-more-fields-body",
                                field_grid(additional_fields),
                                now,
                            ))
                    })),
            )
            .children(self.schema_error.clone().map(|error| integration_message(error, true)))
            .children(self.error.clone().or_else(|| cx.global::<AppState>().task_drafts.read(cx).error.clone()).map(|error| integration_message(error, true)))
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(8.))
                    .pt(px(12.))
                    .border_t_1()
                    .border_color(t::border())
                    .child(div().flex_1().text_size(px(11.)).text_color(t::muted()).child("YouTrack fields are read from the selected project. Closing keeps your draft."))
                    .children((self.schema_error.is_some() && !self.loading).then(|| {
                        button("retry-youtrack-schema", "Retry")
                            .disabled(!self.enabled || self.pending.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.retry_schema(window, cx)
                            }))
                    }))
                    .child(button("discard-youtrack-draft", "Discard draft").disabled(disabled).on_click(cx.listener(|this, _, _, cx| { if let Some(key) = &this.key { cx.global::<AppState>().task_drafts.clone().update(cx, |s,cx|s.remove(key,cx)); } this.hydrated=false; cx.notify(); })))
                    .child(primary_loading_button("save-youtrack", if self.request.task().is_some() { "Save changes" } else { "Create issue" }, &self.submit_loading).disabled(disabled && !self.submit_loading.active()).on_click(cx.listener(|this, _, _, cx| this.submit(cx)))),
            )
    }
}
