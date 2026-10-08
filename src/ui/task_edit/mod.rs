pub mod comment_editor;
pub mod composer;
mod form;
mod github_metadata;
pub(crate) mod issue_editor;
pub mod metadata;
pub mod picker;
pub mod status;
use crate::{
    app_state::AppState,
    ui::{components::*, theme as t},
};
use canopy_desktop::{
    integrations::{ProjectTarget, TaskItem},
    motion::{self, Presence, presets},
};
use form::TaskForm;
use gpui_kit::{base::Disableable, component::IconName, *};
use issue_editor::{IssueDeleted, IssueSaved};
use std::{path::PathBuf, time::Instant};
#[derive(Clone)]
pub enum TaskEditRequest {
    Jira {
        task: TaskItem,
        path: PathBuf,
        mode: super::jira::JiraMode,
        /// A workflow selected in task details must never fall back to another transition.
        transition: Option<String>,
    },
    Create {
        project: ProjectTarget,
        path: PathBuf,
    },
    Edit {
        task: TaskItem,
        path: PathBuf,
    },
}
impl TaskEditRequest {
    pub fn project(&self) -> &ProjectTarget {
        match self {
            Self::Create { project, .. } => project,
            Self::Edit { task, .. } | Self::Jira { task, .. } => &task.reference.project,
        }
    }
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Create { path, .. } | Self::Edit { path, .. } | Self::Jira { path, .. } => path,
        }
    }
    pub fn task(&self) -> Option<&TaskItem> {
        match self {
            Self::Edit { task, .. } | Self::Jira { task, .. } => Some(task),
            _ => None,
        }
    }
    pub fn jira_transition(&self) -> Option<&str> {
        match self {
            Self::Jira { transition, .. } => transition.as_deref(),
            _ => None,
        }
    }
    pub fn draft_kind(&self) -> String {
        if let Self::Jira {
            task,
            mode,
            transition,
            ..
        } = self
        {
            let mut kind = format!("issue/{}/{mode:?}", task.reference.id);
            if let Some(id) = transition {
                kind.push('/');
                kind.push_str(id);
            }
            return kind;
        }
        self.task()
            .map(|t| format!("issue/{}/edit", t.reference.id))
            .unwrap_or_else(|| "new-issue".into())
    }
}
pub struct TaskEditorDialog {
    pub request: TaskEditRequest,
    pub saved: Option<TaskItem>,
    pub notice: Option<String>,
    pub return_to_task: bool,
    restore_focus: bool,
    form: Entity<TaskForm>,
    presence: Presence,
    focus: FocusHandle,
    return_focus: FocusHandle,
    closing: bool,
    scheduled: bool,
    enabled: bool,
    _events: Vec<Subscription>,
}
impl EventEmitter<ModalDismissed> for TaskEditorDialog {}
impl TaskEditorDialog {
    pub fn new(
        request: TaskEditRequest,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let initial = focus.clone();
        window.on_next_frame(move |window, cx| initial.focus(window, cx));
        let form = cx.new(|cx| TaskForm::new(request.clone(), window, cx));
        form.update(cx, |form, cx| form.enable(false, cx));
        let account_key = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .draft_key(request.project(), &request.draft_kind());
        let events = vec![
            cx.subscribe(&form, |s, _, _: &IssueDeleted, cx| {
                s.return_to_task = false;
                s.close(cx);
            }),
            cx.observe(
                &cx.global::<AppState>().integrations.clone(),
                move |this, state, cx| {
                    let state = state.read(cx);
                    if state.path.as_ref() != Some(this.request.path())
                        || state.draft_key(this.request.project(), &this.request.draft_kind())
                            != account_key
                    {
                        this.return_to_task = false;
                        this.restore_focus = false;
                        this.close(cx);
                    }
                },
            ),
            cx.subscribe(&form, |this, _, event: &IssueSaved, cx| {
                if this.closing {
                    return;
                }
                this.saved = Some(event.0.clone());
                this.notice = event.1.clone();
                this.close(cx);
            }),
            cx.subscribe(
                &cx.global::<AppState>().projects.clone(),
                |this, _, _: &crate::app_state::AgentFocusRequested, cx| {
                    this.return_to_task = false;
                    this.restore_focus = false;
                    this.close(cx);
                },
            ),
        ];
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        Self {
            request,
            saved: None,
            notice: None,
            return_to_task: true,
            restore_focus: true,
            form,
            presence,
            focus,
            return_focus,
            closing: false,
            scheduled: false,
            enabled: false,
            _events: events,
        }
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.form.update(cx, |f, cx| f.enable(false, cx));
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
}
impl Render for TaskEditorDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let p = self.presence.progress(now);
        let moving = self.presence.is_animating(now);
        motion::request_frame(window, moving);
        if self.closing && !moving && !self.scheduled {
            self.scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |s, cx| {
                    if s.restore_focus {
                        s.return_focus.focus(window, cx);
                    }
                    cx.emit(ModalDismissed);
                });
            });
        }
        if !moving && !self.closing && !self.enabled {
            self.enabled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let _ = owner.update(cx, |dialog, cx| {
                    if !dialog.closing {
                        dialog.form.update(cx, |form, cx| form.enable(true, cx));
                    }
                });
            });
        }
        let width = (f32::from(window.viewport_size().width) - 64.).clamp(0., 820.);
        let height = (f32::from(window.viewport_size().height) - 80.).clamp(0., 820.);
        let title = self
            .request
            .task()
            .map(|task| {
                format!(
                    "{} · {}",
                    if let TaskEditRequest::Jira { mode, .. } = &self.request {
                        mode.label()
                    } else {
                        "Edit issue"
                    },
                    task.reference.label()
                )
            })
            .unwrap_or_else(|| "New issue".into());
        let panel = modal::modal_surface("task-editor-dialog")
            .absolute()
            .w(px(width))
            .h(px(height))
            .left((window.viewport_size().width - px(width)) / 2.)
            .top(
                (window.viewport_size().height - px(height)) / 2.
                    + px(motion::distance::BASE * (1. - p)),
            )
            .opacity(p)
            .child(modal::modal_header(
                title,
                icon_button(
                    "close-task-editor",
                    IconName::Close,
                    "Close editor and keep draft",
                )
                .disabled(self.closing)
                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ))
            .child(
                div().text_size(px(11.)).text_color(t::secondary()).child(
                    self.request
                        .project()
                        .site
                        .as_ref()
                        .map(|site| format!("{site} / {}", self.request.project().key))
                        .unwrap_or_else(|| self.request.project().key.clone()),
                ),
            )
            .child(self.form.clone());
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(false)
            .on_ok(|_, _, _| false)
            .on_cancel(move |e, w, cx| {
                cancel(e, w, cx);
                false
            })
            .backdrop(modal::modal_backdrop(p))
            .popup(panel)
    }
}
