use super::{
    JiraMode,
    field::{FieldChanged, FieldInput},
};
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{
        components::integrations::integration_message,
        components::*,
        task_edit::{
            TaskEditRequest,
            issue_editor::{IssueDeleted, IssueSaved},
        },
        theme as t,
    },
};
use canopy_desktop::integrations::{
    IssueDraft, TaskWrite, client, credentials,
    drafts::TaskDraft,
    jira::{JiraField, JiraSchema, JiraWrite, SchemaRequest},
};
use canopy_desktop::motion;
use gpui_kit::{
    base::Disableable,
    component::{
        scroll::ScrollableElement,
        select::{SelectEvent, SelectState},
    },
    *,
};
use serde_json::{Value, json};
use std::time::Instant;
pub struct JiraForm {
    request: TaskEditRequest,
    mode: JiraMode,
    selector: Entity<SelectState<Vec<SelectOption>>>,
    selected: String,
    schema: Option<JiraSchema>,
    fields: Vec<Entity<FieldInput>>,
    baseline: Value,
    restored: Value,
    key: Option<String>,
    hydrated: bool,
    ready: bool,
    enabled: bool,
    loading: bool,
    pending: Option<u64>,
    submit_loading: ButtonLoading,
    finished: bool,
    more_fields: Disclosure,
    error: Option<String>,
    read: Option<Task<()>>,
    field_events: Vec<Subscription>,
    _events: Vec<Subscription>,
}
impl EventEmitter<IssueSaved> for JiraForm {}
impl EventEmitter<IssueDeleted> for JiraForm {}
impl JiraForm {
    pub fn new(request: TaskEditRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mode = match &request {
            TaskEditRequest::Jira { mode, .. } => *mode,
            _ => JiraMode::Edit,
        };
        let selector = cx.new(|cx| SelectState::new(Vec::<SelectOption>::new(), None, window, cx));
        let state = cx.global::<AppState>().clone();
        let mut key = state
            .integrations
            .read(cx)
            .draft_key(request.project(), &request.draft_kind());
        if mode == JiraMode::Delete {
            key = None;
        }
        let events = vec![
            cx.subscribe_in(
                &selector,
                window,
                |this, _, event: &SelectEvent<Vec<SelectOption>>, window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event
                        && this.request.jira_transition().is_none()
                        && value.as_ref() != this.selected
                    {
                        this.save_draft(cx);
                        this.restored = Value::Object(
                            this.fields
                                .iter()
                                .map(|f| {
                                    let f = f.read(cx);
                                    (f.field.id.clone(), f.draft(cx))
                                })
                                .collect(),
                        );
                        this.selected = value.to_string();
                        if this.request.task().is_none() {
                            this.load(
                                SchemaRequest::Create {
                                    issue_type: this.selected.clone(),
                                },
                                window,
                                cx,
                            );
                        } else if this.mode == JiraMode::Sprint {
                            this.load(
                                SchemaRequest::Sprints {
                                    board: this.selected.clone(),
                                    cursor: None,
                                },
                                window,
                                cx,
                            );
                        } else if this.mode == JiraMode::Transition {
                            this.install_fields(window, cx);
                        }
                        cx.notify();
                    }
                },
            ),
            cx.observe_in(&state.task_drafts, window, |this, _, w, cx| {
                this.hydrate(w, cx);
                cx.notify();
            }),
            cx.observe(&state.integrations, |this, _, cx| this.sync_enabled(cx)),
            cx.observe(&state.settings, |this, _, cx| this.sync_enabled(cx)),
            cx.subscribe(&state.integrations, |this, _, e: &WriteFinished, cx| {
                if this.pending != Some(e.id) {
                    return;
                }
                this.pending = None;
                this.submit_loading.set(false, cx);
                match &e.result {
                    Ok(receipt) => {
                        this.finished = true;
                        let task = receipt
                            .task
                            .clone()
                            .or_else(|| this.request.task().cloned());
                        if let Some(task) = task {
                            cx.emit(IssueSaved(task, receipt.notice.clone()));
                        }
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                this.sync_enabled(cx);
                cx.notify();
            }),
        ];
        let baseline = request
            .task()
            .and_then(|t| t.jira.as_ref())
            .map(|d| d.fields.clone())
            .unwrap_or(json!({}));
        let mut this = Self {
            request,
            mode,
            selector,
            selected: String::new(),
            schema: None,
            fields: vec![],
            baseline,
            restored: Value::Null,
            key,
            hydrated: false,
            ready: false,
            enabled: false,
            loading: false,
            pending: None,
            submit_loading: ButtonLoading::default(),
            finished: false,
            more_fields: Disclosure::new(false, Instant::now()),
            error: None,
            read: None,
            field_events: vec![],
            _events: events,
        };
        this.hydrate(window, cx);
        this
    }
    fn hydrate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hydrated {
            return;
        }
        let drafts = cx.global::<AppState>().task_drafts.read(cx);
        if !drafts.ready {
            return;
        }
        if let Some(d) = self.key.as_ref().and_then(|k| drafts.values.get(k)) {
            self.restored = json!(d.value.fields);
            self.selected = self.restored["_selection"].as_str().unwrap_or("").into();
            self.error = d.warning.clone();
        }
        // A saved draft cannot switch the workflow action selected in details.
        if let Some(transition) = self.request.jira_transition() {
            self.selected = transition.to_owned();
        }
        self.hydrated = true;
        let request = match (self.request.task(), self.mode) {
            (None, _) => Some(SchemaRequest::Types),
            (Some(t), JiraMode::Edit) => Some(SchemaRequest::Edit {
                key: t.reference.id.clone(),
            }),
            (Some(t), JiraMode::Transition) => Some(SchemaRequest::Transitions {
                key: t.reference.id.clone(),
            }),
            (_, JiraMode::Link) => Some(SchemaRequest::LinkTypes),
            (_, JiraMode::Sprint) => Some(SchemaRequest::Boards { cursor: None }),
            (_, JiraMode::LogWork | JiraMode::Delete) => None,
        };
        if let Some(request) = request {
            self.load(request, window, cx);
        } else {
            self.schema = Some(JiraSchema::default());
            self.install_fields(window, cx);
        }
    }
    fn load(&mut self, request: SchemaRequest, window: &mut Window, cx: &mut Context<Self>) {
        let Some(account) = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .config
            .account_for(self.request.project())
            .cloned()
        else {
            self.error = Some("Connect this Jira site in Preferences.".into());
            return;
        };
        let target = self.request.project().clone();
        let task = self.request.task().map(|t| t.reference.clone());
        let http = cx.http_client();
        self.loading = true;
        self.ready = false;
        self.sync_enabled(cx);
        self.read = Some(cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let key = account.credential.clone();
                let token = cx
                    .background_executor()
                    .spawn(async move { credentials::load(&key) })
                    .await?;
                let client = client(http, &account, token)?;
                let baseline=if let Some(task)=&task {Some(client.task(task).await?.jira.ok_or("Jira returned no task fields.")?.fields.clone())}else{None};
                let mut schema = client.schema(&target, &request).await?;
                if matches!(request,SchemaRequest::Boards {..}|SchemaRequest::Sprints {..}) {
                    let mut seen=std::collections::HashSet::new();
                    while let Some(cursor)=schema.next_cursor.take() {
                        if !seen.insert(cursor.clone())||schema.options.len()>=2000{return Err("Too many or repeated Jira board/sprint pages. Narrow the selected project.".into());}
                        let next=match &request {SchemaRequest::Boards {..}=>SchemaRequest::Boards {cursor:Some(cursor)},SchemaRequest::Sprints {board,..}=>SchemaRequest::Sprints {board:board.clone(),cursor:Some(cursor)},_=>unreachable!()};
                        let page=client.schema(&target,&next).await?;schema.options.extend(page.options);schema.next_cursor=page.next_cursor;
                    }
                }
                // User identity is accountId; display names are only labels.
                if schema
                    .fields
                    .iter()
                    .any(|f| f.schema["type"] == "user" && f.allowed.is_empty())
                    && let Ok(users) = client
                        .options(
                            &target,
                            canopy_desktop::integrations::OptionKind::Assignee,
                            None,
                        )
                        .await
                    {
                        for f in &mut schema.fields {
                            if f.schema["type"] == "user" && f.allowed.is_empty() {
                                f.allowed = users
                                    .items
                                    .iter()
                                    .map(|u| json!({"accountId":u.value,"displayName":u.label}))
                                    .collect();
                            }
                        }
                    }
                Ok::<_, String>((schema,baseline))
            }
            .await;
            let _ = this.update_in(cx, |s, window, cx| {
                s.loading = false;
                match result {
                    Err(e) => s.error = Some(e),
                    Ok((schema,baseline)) => {
                        if let Some(baseline)=baseline {s.baseline=baseline;}
                        let options = if matches!(request, SchemaRequest::Transitions { .. }) {
                            schema
                                .transitions
                                .iter()
                                .map(|t| {
                                    SelectOption::new(t.id.clone(), t.label())
                                })
                                .collect::<Vec<_>>()
                        } else {
                            schema
                                .options
                                .iter()
                                .map(|o| SelectOption::new(o.value.clone(), o.label.clone()))
                                .collect::<Vec<_>>()
                        };
                        if matches!(request, SchemaRequest::Transitions { .. })
                            && s.request.jira_transition().is_some()
                            && !options.iter().any(|o| o.value.as_ref() == s.selected)
                        {
                            s.schema = None;
                            s.fields.clear();
                            s.field_events.clear();
                            s.error = Some("This workflow transition is no longer available. Close the form and refresh the status choices.".into());
                            s.sync_enabled(cx);
                            cx.notify();
                            return;
                        }
                        if !options.is_empty() && !matches!(request, SchemaRequest::Sprints { .. })
                        {
                            if !options.iter().any(|o| o.value.as_ref() == s.selected) {
                                s.selected = options[0].value.to_string();
                            }
                            s.selector.update(cx, |v, cx| {
                                v.set_items(options, window, cx);
                                v.set_selected_value(&s.selected.clone().into(), window, cx);
                            });
                        }
                        s.schema = Some(schema);
                        if matches!(request, SchemaRequest::Types) {
                            if s.selected.is_empty() {
                                s.error =
                                    Some("No issue types can be created in this project.".into());
                            } else {
                                s.load(
                                    SchemaRequest::Create {
                                        issue_type: s.selected.clone(),
                                    },
                                    window,
                                    cx,
                                );
                            }
                        } else if matches!(request, SchemaRequest::Boards { .. }) {
                            if s.selected.is_empty() {
                                s.error = Some("No boards are available for this project.".into());
                            } else {
                                s.load(
                                    SchemaRequest::Sprints {
                                        board: s.selected.clone(),
                                        cursor: None,
                                    },
                                    window,
                                    cx,
                                );
                            }
                        } else {
                            s.install_fields(window, cx);
                        }
                    }
                }
                s.sync_enabled(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn install_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fields.clear();
        self.field_events.clear();
        let Some(schema) = &self.schema else {
            return;
        };
        let simple = |id: &str, name: &str, kind: &str, required| JiraField {
            id: id.into(),
            name: name.into(),
            required,
            schema: json!({"type":kind}),
            allowed: vec![],
            default: Value::Null,
        };
        let mut fields = match self.mode {
            JiraMode::Edit => schema.fields.clone(),
            JiraMode::Transition => {
                let Some(t) = schema.transitions.iter().find(|t| t.id == self.selected) else {
                    self.error =
                        Some("No workflow transitions are available for this task.".into());
                    return;
                };
                let mut fs = t.fields.clone();
                fs.push(simple("_comment", "Comment", "string", false));
                fs
            }
            JiraMode::Link => vec![
                simple("_other", "Issue key", "string", true),
                simple(
                    "_outward",
                    "Apply relationship from this issue",
                    "boolean",
                    true,
                ),
            ],
            JiraMode::Delete => vec![
                simple(
                    "_confirmation",
                    "Type the issue key to confirm",
                    "string",
                    true,
                ),
                simple("_delete_subtasks", "Also delete subtasks", "boolean", false),
            ],
            JiraMode::Sprint => {
                let mut f = simple("_sprint", "Sprint", "string", false);
                f.allowed = std::iter::once(json!({"id":"backlog","name":"Backlog"}))
                    .chain(
                        schema
                            .options
                            .iter()
                            .map(|o| json!({"id":o.value,"name":o.label})),
                    )
                    .collect();
                vec![f]
            }
            JiraMode::LogWork => vec![
                simple("_minutes", "Time spent (minutes)", "number", true),
                simple("_started", "Started", "datetime", true),
                simple("_comment", "Work description", "string", false),
            ],
        };
        fields.retain(|f| {
            !(self.request.task().is_some() && self.mode == JiraMode::Edit && f.id == "status")
                && (!matches!(f.id.as_str(), "project" | "issuetype")
                    || self.request.task().is_some()
                        && self.mode == JiraMode::Edit
                        && f.id == "issuetype")
        });
        // Put summary/description first; preserve the required ordering from the provider.
        fields.sort_by_key(|f| match f.id.as_str() {
            "summary" => 0,
            "description" => 1,
            _ => {
                if f.required {
                    2
                } else {
                    3
                }
            }
        });
        for field in fields {
            let value = if field.id == "_started" {
                json!(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string())
            } else if field.id == "_sprint" {
                json!({"id":"backlog","name":"Backlog"})
            } else if field.id == "_outward" {
                json!(true)
            } else if self.mode == JiraMode::Transition && self.baseline[&field.id].is_null() {
                field.default.clone()
            } else if self.request.task().is_some() {
                self.baseline[&field.id].clone()
            } else {
                field.default.clone()
            };
            let base = self
                .request
                .task()
                .map(|t| t.url.clone())
                .unwrap_or_else(|| self.request.project().site.clone().unwrap_or_default());
            let id = field.id.clone();
            let input = cx.new(|cx| FieldInput::new(field, value, base, window, cx));
            if let Some(v) = self.restored.get(&id) {
                input.update(cx, |s, cx| s.restore(v, window, cx));
            }
            self.field_events
                .push(cx.subscribe(&input, |s, _, _: &FieldChanged, cx| s.save_draft(cx)));
            self.fields.push(input);
        }
        self.ready = true;
        self.sync_enabled(cx);
        cx.notify();
    }
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        if !self.hydrated || !self.ready || self.pending.is_some() || self.finished {
            return;
        }
        if let Some(key) = &self.key {
            let mut values = self
                .fields
                .iter()
                .map(|f| {
                    let f = f.read(cx);
                    (f.field.id.clone(), f.draft(cx))
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            values.insert("_selection".into(), json!(self.selected));
            cx.global::<AppState>()
                .task_drafts
                .clone()
                .update(cx, |s, cx| {
                    s.set(
                        key.clone(),
                        TaskDraft {
                            project: self.request.project().clone(),
                            value: IssueDraft {
                                fields: values,
                                ..Default::default()
                            },
                            baseline: None,
                            warning: None,
                        },
                        cx,
                    )
                });
        }
        cx.notify();
    }
    pub fn enable(&mut self, value: bool, cx: &mut Context<Self>) {
        self.enabled = value;
        self.sync_enabled(cx);
    }
    fn editable(&self, cx: &App) -> bool {
        let app = cx.global::<AppState>();
        self.enabled
            && self.hydrated
            && self.ready
            && !self.loading
            && !self.finished
            && self.pending.is_none()
            && !app.integrations.read(cx).busy
            && !app.settings.read(cx).quitting
    }
    fn sync_enabled(&mut self, cx: &mut Context<Self>) {
        let enabled = self.editable(cx);
        for f in &self.fields {
            let visible = self.more_fields.open || primary_field(&f.read(cx).field, self.mode);
            f.update(cx, |s, cx| s.enable(enabled && visible, cx));
        }
        cx.notify();
    }
    fn has_empty_reference_field(&self, cx: &App) -> bool {
        self.fields.iter().any(|field| {
            let field = field.read(cx);
            let kind = field.field.schema["type"].as_str().unwrap_or("");
            let item_kind = field.field.schema["items"].as_str().unwrap_or("");
            field.field.allowed.is_empty()
                && (matches!(
                    kind,
                    "user"
                        | "issuelink"
                        | "option"
                        | "priority"
                        | "resolution"
                        | "issuetype"
                        | "project"
                        | "version"
                        | "component"
                        | "status"
                ) || matches!(
                    item_kind,
                    "user" | "option" | "version" | "component" | "issuelink"
                ) || field.field.id == "parent")
        })
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        let mut values = serde_json::Map::new();
        for input in &self.fields {
            let f = input.read(cx);
            let value = match f.value(cx) {
                Ok(v) => v,
                Err(e) => {
                    self.error = Some(e);
                    cx.notify();
                    return;
                }
            };
            if f.field.required
                && (value.is_null()
                    || value.as_str().is_some_and(|s| s.trim().is_empty())
                    || value.as_array().is_some_and(Vec::is_empty))
            {
                self.error = Some(format!("{} is required.", f.field.name));
                cx.notify();
                return;
            }
            if self.request.task().is_none() && !value.is_null()
                || match self.mode {
                    JiraMode::Edit => f.dirty(cx),
                    JiraMode::Transition => f.field.required || f.dirty(cx),
                    _ => true,
                }
            {
                values.insert(f.field.id.clone(), value);
            }
        }
        let action = if self.request.task().is_none() {
            values.insert("issuetype".into(), json!({"id":self.selected}));
            JiraWrite::Create {
                fields: Value::Object(values),
            }
        } else {
            match self.mode {
                JiraMode::Delete => JiraWrite::DeleteIssue {
                    confirmation: values["_confirmation"].as_str().unwrap_or("").into(),
                    subtasks: values["_delete_subtasks"].as_bool().unwrap_or(false),
                },
                JiraMode::Edit => JiraWrite::Fields {
                    fields: Value::Object(values),
                },
                JiraMode::Transition => {
                    let comment = values
                        .remove("_comment")
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default();
                    JiraWrite::Transition {
                        id: self.selected.clone(),
                        fields: Value::Object(values),
                        comment,
                    }
                }
                JiraMode::Link => JiraWrite::Link {
                    kind: self.selected.clone(),
                    other: values["_other"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_ascii_uppercase(),
                    outward: values["_outward"].as_bool().unwrap_or(true),
                },
                JiraMode::Sprint => JiraWrite::Sprint {
                    id: values["_sprint"]["id"]
                        .as_str()
                        .filter(|id| *id != "backlog")
                        .map(str::to_owned),
                },
                JiraMode::LogWork => JiraWrite::LogWork {
                    seconds: values["_minutes"].as_u64().unwrap_or(0).saturating_mul(60),
                    started: match parse_worklog_start(values["_started"].as_str().unwrap_or("")) {
                        Ok(value) => value,
                        Err(error) => {
                            self.error = Some(error);
                            cx.notify();
                            return;
                        }
                    },
                    comment: values["_comment"].as_str().unwrap_or("").into(),
                },
            }
        };
        self.save_draft(cx);
        self.error = None;
        let command = TaskWrite::Jira {
            project: self.request.project().clone(),
            task: self.request.task().map(|t| t.reference.clone()),
            action,
        };
        match cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |s, cx| s.submit(command, self.key.clone(), cx))
        {
            Ok(id) => {
                self.pending = Some(id);
                self.submit_loading.set(true, cx);
            }
            Err(e) => self.error = Some(e),
        }
        self.sync_enabled(cx);
    }
}
impl Render for JiraForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.editable(cx);
        let mut full_width = Vec::new();
        let mut compact = Vec::new();
        let mut additional_fields = Vec::new();
        for field in &self.fields {
            let input = field.read(cx);
            let id = input.field.id.as_str();
            let primary = primary_field(&input.field, self.mode);
            if !primary {
                additional_fields.push(field.clone());
            } else if matches!(id, "summary" | "description") {
                full_width.push(field.clone());
            } else {
                compact.push(field.clone());
            }
        }
        compact.sort_by_key(|field| compact_field_rank(field.read(cx).field.id.as_str()));
        let now = Instant::now();
        motion::request_frame(window, self.more_fields.active(now));
        let more_count = additional_fields.len();
        column()
            .size_full()
            .gap(px(12.))
            .child(
                column()
                    .id("jira-form-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .gap(px(16.))
                    .children(
                        (self.request.jira_transition().is_none()
                            && (self.request.task().is_none()
                                || matches!(
                                    self.mode,
                                    JiraMode::Transition | JiraMode::Link | JiraMode::Sprint
                                )))
                        .then(|| {
                            form_field(
                                if self.request.task().is_none() {
                                    "Issue type"
                                } else if self.mode == JiraMode::Delete {"Delete issue"} else if self.mode == JiraMode::Transition {
                                    "Workflow transition"
                                } else if self.mode == JiraMode::Sprint {
                                    "Board"
                                } else {
                                    "Relationship"
                                },
                                "",
                                dropdown(&self.selector).w_full().disabled(
                                    self.loading
                                        || self.pending.is_some()
                                        || !self.enabled
                                        || cx.global::<AppState>().integrations.read(cx).busy
                                        || cx.global::<AppState>().settings.read(cx).quitting,
                                ),
                            )
                        }),
                    )
                    .children(self.request.jira_transition().and_then(|id| {
                        self.schema.as_ref()?.transitions.iter().find(|transition| transition.id == id)
                    }).map(|transition| {
                        form_field("Workflow transition", "Complete the required fields to change status.",
                            div().text_size(px(12.)).child(transition.label()))
                    }))
                    .children((self.mode==JiraMode::Delete).then(||integration_message(format!("Permanently delete {} and its comments and attachments? This cannot be undone.",self.request.task().map(|t|t.reference.label()).unwrap_or_default()),true)))
                    .children(self.loading.then(|| div().child("Loading Jira fields…")))
                    .children(full_width)
                    .children((!compact.is_empty()).then(|| field_grid(compact)))
                    .children((more_count > 0).then(|| {
                        column()
                            .flex_shrink_0()
                            .gap(px(4.))
                            .child(
                                self.more_fields
                                    .header("jira-more-fields", "MORE FIELDS", now, cx)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.more_fields.toggle(Instant::now(), cx);
                                        this.sync_enabled(cx);
                                    })),
                            )
                            .child(self.more_fields.measured_body(
                                "jira-more-fields-body",
                                field_grid(additional_fields),
                                now,
                            ))
                    })),
            )
            .children(
                self.error
                    .clone()
                    .or_else(|| cx.global::<AppState>().task_drafts.read(cx).error.clone())
                    .map(|e| integration_message(e, true)),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(t::border())
                    .pt(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child(if self.mode==JiraMode::Delete {"Deletion requires the exact issue key."}else{"Closing keeps your draft."}),
                    )
                    .children(((!self.ready && !self.loading)
                        || self.has_empty_reference_field(cx))
                        .then(|| {
                        button("jira-retry-schema", "Retry").on_click(cx.listener(|s, _, w, cx| {
                            s.hydrated = false;
                            s.ready = false;
                            s.fields.clear();
                            s.hydrate(w, cx);
                        }))
                    }))
                    .child(
                        primary_loading_button(
                            "jira-save",
                            if self.request.task().is_none() {
                                "Create issue"
                            } else if self.mode == JiraMode::Transition {
                                "Apply transition"
                            } else {
                                "Save changes"
                            },
                            &self.submit_loading,
                        )
                        .disabled(disabled && !self.submit_loading.active())
                        .on_click(cx.listener(|s, _, _, cx| s.submit(cx))),
                    ),
            )
    }
}

fn primary_field(field: &JiraField, mode: JiraMode) -> bool {
    mode != JiraMode::Edit
        || field.required
        || matches!(
            field.id.as_str(),
            "summary"
                | "description"
                | "assignee"
                | "priority"
                | "parent"
                | "issuetype"
                | "status"
                | "labels"
        )
        || field.schema["type"] == "user"
        || field.schema["items"] == "user"
}

fn compact_field_rank(id: &str) -> u8 {
    match id {
        "issuetype" => 0,
        "status" => 1,
        "priority" => 2,
        "assignee" => 3,
        "labels" => 4,
        "parent" => 5,
        _ => 10,
    }
}

fn parse_worklog_start(text: &str) -> Result<String, String> {
    canopy_desktop::integrations::jira::date_time(text)
}
