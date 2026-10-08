use super::composer::{ComposerChanged, MarkdownComposer};
use crate::ui::jira::field::{FieldChanged, FieldInput};
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::integrations::jira::{JiraField, JiraWrite, adf};
use canopy_desktop::integrations::{
    IssueDraft, TaskComment, TaskRef, TaskWrite, drafts::TaskDraft, validate_body,
};
use gpui_kit::{base::Disableable, *};
use serde_json::json;
pub struct CommentEditorDone;
impl EventEmitter<CommentEditorDone> for CommentEditor {}
pub struct CommentEditor {
    task: TaskRef,
    comment: Option<TaskComment>,
    body: Entity<MarkdownComposer>,
    rich: Option<Entity<FieldInput>>,
    key: Option<String>,
    hydrated: bool,
    enabled: bool,
    pending: Option<u64>,
    submit_loading: ButtonLoading,
    error: Option<String>,
    finished: bool,
    success_url: Option<String>,
    _events: Vec<Subscription>,
}
impl CommentEditor {
    pub fn new(
        task: TaskRef,
        comment: Option<TaskComment>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.global::<AppState>().clone();
        let kind = comment
            .as_ref()
            .map(|c| format!("issue/{}/comment/{}", task.id, c.id))
            .unwrap_or_else(|| format!("issue/{}/comment/new", task.id));
        let key = state.integrations.read(cx).draft_key(&task.project, &kind);
        let body = cx.new(|cx| MarkdownComposer::new(task.url(), 160., window, cx));
        let rich = comment
            .as_ref()
            .and_then(|c| c.rich_body.as_ref())
            .filter(|v| !adf::editable(v))
            .map(|v| {
                cx.new(|cx| {
                    FieldInput::new(
                        JiraField {
                            id: "description".into(),
                            name: "Comment document".into(),
                            required: true,
                            schema: json!({"type":"string"}),
                            allowed: vec![],
                            default: serde_json::Value::Null,
                        },
                        v.clone(),
                        task.url(),
                        window,
                        cx,
                    )
                })
            });
        let mut events = vec![
            cx.subscribe(&body, |this, _, _: &ComposerChanged, cx| {
                this.save_draft(cx)
            }),
            cx.observe_in(&state.task_drafts, window, |this, _, window, cx| {
                this.hydrate(window, cx);
                cx.notify();
            }),
            cx.observe(&state.integrations, |this, _, cx| this.sync_enabled(cx)),
            cx.observe(&state.settings, |this, _, cx| this.sync_enabled(cx)),
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
                            this.error = None;
                            this.success_url = receipt.comment.as_ref().map(|c| c.url.clone());
                            if this.comment.is_some() {
                                this.finished = true;
                                cx.emit(CommentEditorDone);
                            } else {
                                this.body.update(cx, |f, cx| f.set_value("", window, cx));
                            }
                        }
                        Err(error) => this.error = Some(error.to_string()),
                    }
                    this.sync_enabled(cx);
                    cx.notify();
                },
            ),
        ];
        if let Some(rich) = &rich {
            events.push(cx.subscribe(rich, |s, _, _: &FieldChanged, cx| s.save_draft(cx)));
        }
        let mut this = Self {
            rich,
            task,
            comment,
            body,
            key,
            hydrated: false,
            enabled: false,
            pending: None,
            submit_loading: ButtonLoading::default(),
            error: None,
            finished: false,
            success_url: None,
            _events: events,
        };
        this.hydrate(window, cx);
        this
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
        if let Some(rich) = &self.rich
            && let Some(value) = draft.as_ref().and_then(|d| d.value.fields.get("document"))
        {
            rich.update(cx, |s, cx| s.restore(value, window, cx));
        }
        let text = draft.map(|d| d.value.body).unwrap_or_else(|| {
            self.comment
                .as_ref()
                .map(|c| c.body.to_string())
                .unwrap_or_default()
        });
        self.body.update(cx, |f, cx| f.set_value(&text, window, cx));
        self.hydrated = true;
        self.sync_enabled(cx);
    }
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        if !self.hydrated || self.finished || self.pending.is_some() {
            return;
        }
        let body = self.body.read(cx).value(cx);
        if !body.is_empty() {
            self.success_url = None;
        }
        let baseline = self.comment.as_ref().map(|c| c.body.as_ref()).unwrap_or("");
        if let Some(key) = &self.key {
            let state = cx.global::<AppState>().task_drafts.clone();
            if body == baseline && self.rich.as_ref().is_none_or(|r| !r.read(cx).dirty(cx)) {
                state.update(cx, |s, cx| s.remove(key, cx));
            } else {
                state.update(cx, |s, cx| {
                    s.set(
                        key.clone(),
                        TaskDraft {
                            project: self.task.project.clone(),
                            value: IssueDraft {
                                fields: self
                                    .rich
                                    .as_ref()
                                    .map(|r| {
                                        [("document".into(), r.read(cx).draft(cx))]
                                            .into_iter()
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                body,
                                ..Default::default()
                            },
                            baseline: self.comment.as_ref().map(|c| IssueDraft {
                                body: c.body.to_string(),
                                ..Default::default()
                            }),
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
        if let Some(r) = &self.rich {
            r.update(cx, |s, cx| s.enable(enabled, cx));
        }
        cx.notify();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.can_edit(cx) {
            return;
        }
        let body = self.body.read(cx).value(cx);
        if let Err(error) = validate_body(&body, true) {
            self.error = Some(error);
            cx.notify();
            return;
        }
        let command = if let (Some(rich), Some(comment)) = (&self.rich, &self.comment) {
            let document = match rich.read(cx).value(cx) {
                Ok(v) => v,
                Err(e) => {
                    self.error = Some(e);
                    cx.notify();
                    return;
                }
            };
            TaskWrite::Jira {
                project: self.task.project.clone(),
                task: Some(self.task.clone()),
                action: JiraWrite::CommentDocument {
                    comment: comment.clone(),
                    document,
                },
            }
        } else if let Some(comment) = &self.comment {
            TaskWrite::EditComment {
                task: self.task.clone(),
                comment: comment.clone(),
                body,
            }
        } else {
            TaskWrite::AddComment {
                task: self.task.clone(),
                body,
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
}
impl Render for CommentEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.can_edit(cx);
        column()
            .gap(px(8.))
            .child(
                self.rich
                    .as_ref()
                    .map(|r| r.clone().into_any_element())
                    .unwrap_or_else(|| self.body.clone().into_any_element()),
            )
            .children(self.success_url.clone().map(|url| {
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t::green())
                            .child("Comment saved"),
                    )
                    .child(
                        button("open-saved-comment", "Open in browser")
                            .h(px(22.))
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
            }))
            .children(
                self.error
                    .clone()
                    .or(cx.global::<AppState>().task_drafts.read(cx).error.clone())
                    .map(|e| integration_message(e, true)),
            )
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.))
                            .text_color(t::muted())
                            .child("Closing keeps your draft"),
                    )
                    .children(self.comment.as_ref().map(|_| {
                        button("cancel-comment-edit", "Cancel")
                            .disabled(self.pending.is_some())
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(CommentEditorDone)))
                    }))
                    .child(
                        primary_loading_button(
                            "submit-comment",
                            if self.comment.is_some() {
                                "Save comment"
                            } else {
                                "Comment"
                            },
                            &self.submit_loading,
                        )
                        .disabled(disabled && !self.submit_loading.active())
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }
}
