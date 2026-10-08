use super::{
    TaskEditRequest,
    composer::{ComposerChanged, MarkdownComposer},
    picker::{OptionChanged, OptionPicker},
};
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::integrations::{
    IssueDraft, OptionKind, Provider, TaskItem, TaskWrite, drafts::TaskDraft,
    youtrack::YoutrackWrite,
};
use gpui_kit::{
    base::Disableable,
    component::{
        input::{InputEvent, InputState},
        scroll::ScrollableElement,
    },
    *,
};
#[derive(Clone)]
pub struct IssueDeleted;
#[derive(Clone)]
pub struct IssueSaved(pub TaskItem, pub Option<String>);
impl EventEmitter<IssueSaved> for IssueEditor {}
pub struct IssueEditor {
    request: TaskEditRequest,
    title: Entity<InputState>,
    body: Entity<MarkdownComposer>,
    labels: Entity<OptionPicker>,
    assignees: Entity<OptionPicker>,
    milestone: Entity<OptionPicker>,
    baseline: IssueDraft,
    key: Option<String>,
    hydrated: bool,
    finished: bool,
    enabled: bool,
    pending: Option<u64>,
    submit_loading: ButtonLoading,
    error: Option<String>,
    _events: Vec<Subscription>,
}
impl IssueEditor {
    pub fn new(request: TaskEditRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = request.project().clone();
        let state = cx.global::<AppState>().clone();
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Issue title"));
        let body = cx.new(|cx| {
            MarkdownComposer::new(
                if project.provider == Provider::Youtrack {
                    format!("{}/issue", project.site.as_deref().unwrap_or(""))
                } else {
                    format!("https://github.com/{}", project.key)
                },
                220.,
                window,
                cx,
            )
        });
        let labels = cx.new(|cx| OptionPicker::new(project.clone(), OptionKind::Label, window, cx));
        let assignees =
            cx.new(|cx| OptionPicker::new(project.clone(), OptionKind::Assignee, window, cx));
        let milestone =
            cx.new(|cx| OptionPicker::new(project.clone(), OptionKind::Milestone, window, cx));
        let baseline = request
            .task()
            .map(|task| IssueDraft {
                title: task.reference.title.clone(),
                body: task.body.to_string(),
                labels: task.labels.clone(),
                assignees: task.assignees.clone(),
                milestone: task.milestone.as_ref().map(|milestone| milestone.0),
                fields: Default::default(),
            })
            .unwrap_or_default();
        let key = state
            .integrations
            .read(cx)
            .draft_key(&project, &request.draft_kind());
        let events = vec![
            cx.subscribe(&title, |this, _, _: &InputEvent, cx| this.save_draft(cx)),
            cx.subscribe(&body, |this, _, _: &ComposerChanged, cx| {
                this.save_draft(cx)
            }),
            cx.subscribe(&labels, |this, _, _: &OptionChanged, cx| {
                this.save_draft(cx)
            }),
            cx.subscribe(&assignees, |this, _, _: &OptionChanged, cx| {
                this.save_draft(cx)
            }),
            cx.subscribe(&milestone, |this, _, _: &OptionChanged, cx| {
                this.save_draft(cx)
            }),
            cx.observe_in(&state.task_drafts, window, |this, _, window, cx| {
                this.hydrate(window, cx);
                cx.notify();
            }),
            cx.observe(&state.integrations, |this, _, cx| {
                this.sync_enabled(cx);
                cx.notify();
            }),
            cx.observe(&state.settings, |this, _, cx| {
                this.sync_enabled(cx);
                cx.notify();
            }),
            cx.subscribe_in(
                &state.integrations,
                window,
                |this, _, event: &WriteFinished, window, cx| {
                    if this.pending != Some(event.id) {
                        return;
                    }
                    this.pending = None;
                    this.submit_loading.set(false, cx);
                    match &event.result {
                        Ok(receipt) => {
                            if let Some(task) = &receipt.task {
                                this.error = receipt.notice.clone();
                                this.finished = true;
                                cx.emit(IssueSaved(task.clone(), receipt.notice.clone()));
                            } else {
                                this.error = Some(
                                    "The change was saved, but its details could not be loaded."
                                        .into(),
                                );
                            }
                        }
                        Err(error) => this.error = Some(error.to_string()),
                    }
                    this.sync_enabled(cx);
                    let _ = window;
                    cx.notify();
                },
            ),
        ];
        let mut this = Self {
            request,
            title,
            body,
            labels,
            assignees,
            milestone,
            baseline,
            key,
            hydrated: false,
            finished: false,
            enabled: true,
            pending: None,
            submit_loading: ButtonLoading::default(),
            error: None,
            _events: events,
        };
        this.sync_metadata(cx);
        this.hydrate(window, cx);
        this
    }
    fn sync_metadata(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.request.task() else {
            return;
        };
        self.labels.update(cx, |picker, cx| {
            for label in &task.labels {
                picker.seed_current(
                    canopy_desktop::integrations::TaskOption {
                        value: label.clone(),
                        label: label.clone(),
                    },
                    cx,
                );
            }
            picker.set_selected(task.labels.clone(), cx);
        });
        self.assignees.update(cx, |picker, cx| {
            for login in &task.assignees {
                picker.seed_current(
                    canopy_desktop::integrations::TaskOption {
                        value: login.clone(),
                        label: format!("@{login}"),
                    },
                    cx,
                );
            }
            picker.set_selected(task.assignees.clone(), cx);
        });
        self.milestone.update(cx, |picker, cx| {
            if let Some((number, title)) = &task.milestone {
                picker.seed_current(
                    canopy_desktop::integrations::TaskOption {
                        value: number.to_string(),
                        label: title.clone(),
                    },
                    cx,
                );
                picker.set_selected(vec![number.to_string()], cx);
            } else {
                picker.set_selected(vec![], cx);
            }
        });
    }
    fn hydrate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hydrated || self.finished {
            return;
        }
        let state = cx.global::<AppState>().task_drafts.read(cx);
        if !state.ready {
            return;
        }
        let draft = self
            .key
            .as_ref()
            .and_then(|key| state.values.get(key))
            .cloned();
        self.error = draft.as_ref().and_then(|d| d.warning.clone());
        let value = draft
            .as_ref()
            .map(|d| d.value.clone())
            .unwrap_or_else(|| self.baseline.clone());
        if let Some(baseline) = draft.and_then(|d| d.baseline) {
            self.baseline = baseline;
        }
        self.title
            .update(cx, |f, cx| f.set_value(value.title, window, cx));
        self.body
            .update(cx, |f, cx| f.set_value(&value.body, window, cx));
        self.labels
            .update(cx, |f, cx| f.set_selected(value.labels, cx));
        self.assignees
            .update(cx, |f, cx| f.set_selected(value.assignees, cx));
        self.milestone.update(cx, |f, cx| {
            f.set_selected(
                value
                    .milestone
                    .map(|m| vec![m.to_string()])
                    .unwrap_or_default(),
                cx,
            )
        });
        self.hydrated = true;
        self.sync_enabled(cx);
    }
    fn value(&self, cx: &App) -> IssueDraft {
        IssueDraft {
            fields: Default::default(),
            title: self.title.read(cx).value().to_string(),
            body: self.body.read(cx).value(cx),
            labels: self.labels.read(cx).selected(),
            assignees: self.assignees.read(cx).selected(),
            milestone: self
                .milestone
                .read(cx)
                .selected()
                .first()
                .and_then(|s| s.parse().ok()),
        }
    }
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        if !self.hydrated || self.finished || self.pending.is_some() {
            return;
        }
        if let Some(key) = &self.key {
            let value = self.value(cx);
            let state = cx.global::<AppState>().task_drafts.clone();
            if value == self.baseline {
                state.update(cx, |s, cx| s.remove(key, cx));
            } else {
                state.update(cx, |s, cx| {
                    s.set(
                        key.clone(),
                        TaskDraft {
                            project: self.request.project().clone(),
                            value,
                            baseline: self.request.task().map(|_| self.baseline.clone()),
                            warning: None,
                        },
                        cx,
                    )
                });
            }
        }
        cx.notify();
    }
    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        self.sync_enabled(cx);
        cx.notify();
    }
    fn can_edit(&self, cx: &App) -> bool {
        let app = cx.global::<AppState>();
        self.enabled
            && self.hydrated
            && !self.finished
            && self.pending.is_none()
            && !app.integrations.read(cx).busy
            && !app.settings.read(cx).quitting
    }
    fn sync_enabled(&mut self, cx: &mut Context<Self>) {
        let enabled = self.can_edit(cx);
        self.body.update(cx, |s, cx| s.enable(enabled, cx));
        for picker in [&self.labels, &self.assignees, &self.milestone] {
            picker.update(cx, |s, cx| s.enable(enabled, cx));
        }
    }
    pub fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.can_edit(cx) {
            return;
        }
        let draft = self.value(cx);
        if let Err(error) = draft.validate() {
            self.error = Some(error);
            cx.notify();
            return;
        }
        let command = if let Some(task) = self.request.task() {
            if task.reference.project.provider == Provider::Youtrack {
                let mut fields = serde_json::Map::new();
                if draft.title != self.baseline.title {
                    fields.insert("summary".into(), serde_json::Value::String(draft.title));
                }
                if draft.body != self.baseline.body {
                    fields.insert("description".into(), serde_json::Value::String(draft.body));
                }
                TaskWrite::Youtrack {
                    project: task.reference.project.clone(),
                    task: Some(task.reference.clone()),
                    action: YoutrackWrite::Fields {
                        fields: serde_json::Value::Object(fields),
                    },
                }
            } else {
                TaskWrite::Edit {
                    task: task.reference.clone(),
                    title: (draft.title != self.baseline.title).then(|| draft.title.clone()),
                    body: (draft.body != self.baseline.body).then(|| draft.body.clone()),
                    labels: (draft.labels != self.baseline.labels).then(|| draft.labels.clone()),
                    assignees: (draft.assignees != self.baseline.assignees)
                        .then(|| draft.assignees.clone()),
                    milestone: (draft.milestone != self.baseline.milestone)
                        .then_some(draft.milestone),
                }
            }
        } else {
            if self.request.project().provider == Provider::Youtrack {
                TaskWrite::Youtrack {
                    project: self.request.project().clone(),
                    task: None,
                    action: YoutrackWrite::Create {
                        fields: serde_json::Value::Object(
                            draft.fields.clone().into_iter().collect(),
                        ),
                        draft,
                    },
                }
            } else {
                TaskWrite::Create {
                    project: self.request.project().clone(),
                    draft,
                }
            }
        };
        self.error = None;
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
            Err(error) => self.error = Some(error),
        }
        self.sync_enabled(cx);
        cx.notify();
    }
    fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_edit(cx) {
            return;
        }
        if let Some(key) = &self.key {
            cx.global::<AppState>()
                .task_drafts
                .clone()
                .update(cx, |s, cx| s.remove(key, cx));
        }
        self.hydrated = false;
        self.hydrate(window, cx);
        self.error = None;
        cx.notify();
    }
}
impl Render for IssueEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.can_edit(cx);
        let draft_error = cx.global::<AppState>().task_drafts.read(cx).error.clone();
        column()
            .size_full()
            .gap(px(12.))
            .child(
                column()
                    .id("issue-form-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .gap(px(16.))
                    .child(form_field(
                        "Title",
                        "",
                        input(&self.title).w_full().disabled(disabled),
                    ))
                    .child(form_field("Description", "", self.body.clone()))
                    .child(field_grid([
                        self.labels.clone(),
                        self.assignees.clone(),
                        self.milestone.clone(),
                    ])),
            )
            .children(
                self.error
                    .clone()
                    .or(draft_error)
                    .map(|error| integration_message(error, true)),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(8.))
                    .pt(px(12.))
                    .border_t_1()
                    .border_color(t::border())
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child("Drafts are saved locally. Closing keeps your text."),
                    )
                    .child(
                        button("discard-issue-draft", "Discard draft")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| this.discard(window, cx))),
                    )
                    .child(
                        primary_loading_button(
                            "save-issue",
                            if self.request.task().is_some() {
                                "Save changes"
                            } else {
                                "Create issue"
                            },
                            &self.submit_loading,
                        )
                        .disabled(disabled && !self.submit_loading.active())
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}
