mod comment_card;
mod comments;
use crate::{
    app_state::AppState,
    ui::{
        components::*,
        components::{
            integrations::integration_message,
            task_detail::{self as view, DetailTab},
        },
        markdown::MarkdownView,
        tasks_panel::TaskWorktreeRequest,
        theme as t,
    },
};
use canopy_desktop::{
    integrations::TaskItem,
    motion::{self, Presence, Transition, presets},
};
use comments::CommentsView;
use gpui_kit::{
    base::Disableable,
    component::{IconName, scroll::ScrollableElement},
    *,
};
use std::{path::PathBuf, time::Instant};
#[derive(Clone)]
pub struct TaskDetailRequest {
    pub task: TaskItem,
    pub path: PathBuf,
}
pub struct TaskDetailDialog {
    request: TaskDetailRequest,
    metadata_editor: Entity<super::task_edit::metadata::MetadataEditor>,
    description: Entity<MarkdownView>,
    comments: Entity<CommentsView>,
    tab: DetailTab,
    tabs: Transition,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    activation_scheduled: bool,
    interactive: bool,
    focus: FocusHandle,
    return_focus: FocusHandle,
    restore_focus: bool,
    credential: Option<String>,
    error: Option<String>,
    pub notice: Option<String>,
    requested_task: Option<canopy_desktop::integrations::TaskRef>,
    pub pending_reader: Option<TaskDetailRequest>,
    pub pending_edit: Option<super::task_edit::TaskEditRequest>,
    linking: bool,
    pub pending_worktree: Option<TaskWorktreeRequest>,
    _observers: Vec<Subscription>,
}
impl EventEmitter<ModalDismissed> for TaskDetailDialog {}
impl TaskDetailDialog {
    pub fn new(
        request: TaskDetailRequest,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let initial_focus = focus.clone();
        window.on_next_frame(move |window, cx| initial_focus.focus(window, cx));
        let description = cx.new(MarkdownView::reading);
        description.update(cx, |view, cx| {
            view.set_content(
                request.task.body.clone(),
                request.task.url.clone(),
                false,
                cx,
            )
        });
        let comments = cx.new(|cx| CommentsView::new(request.task.reference.clone(), window, cx));
        let metadata_editor = cx.new(|cx| {
            super::task_edit::metadata::MetadataEditor::new(
                request.task.clone(),
                request.path.clone(),
                window,
                cx,
            )
        });
        let state = cx.global::<AppState>().integrations.clone();
        let credential = state
            .read(cx)
            .config
            .account_for(&request.task.reference.project)
            .map(|a| a.credential.clone());
        let observers = vec![
            cx.subscribe(
                &metadata_editor,
                |s, _, e: &super::jira::panel::RelatedRequested, cx| {
                    s.requested_task = Some(e.0.clone());
                    s.error = None;
                    cx.notify();
                },
            ),
            cx.subscribe(
                &metadata_editor,
                |this, _, event: &super::jira::panel::JiraAction, cx| {
                    if this.interactive {
                        this.pending_edit = Some(super::task_edit::TaskEditRequest::Jira {
                            task: this.request.task.clone(),
                            path: this.request.path.clone(),
                            mode: event.0,
                            transition: None,
                        });
                        this.close(cx);
                    }
                },
            ),
            cx.subscribe(
                &metadata_editor,
                |this, _, event: &super::task_edit::status::StatusFormRequested, cx| {
                    if this.interactive {
                        this.pending_edit = Some(super::task_edit::TaskEditRequest::Jira {
                            task: this.request.task.clone(),
                            path: this.request.path.clone(),
                            mode: super::jira::JiraMode::Transition,
                            transition: Some(event.transition.clone()),
                        });
                        this.close(cx);
                    }
                },
            ),
            cx.subscribe(
                &metadata_editor,
                |this, _, _: &super::youtrack::panel::YoutrackEditRequested, cx| {
                    if this.interactive {
                        this.pending_edit = Some(super::task_edit::TaskEditRequest::Edit {
                            task: this.request.task.clone(),
                            path: this.request.path.clone(),
                        });
                        this.close(cx);
                    }
                },
            ),
            cx.subscribe(&state, |this, _, task: &TaskItem, cx| {
                if this
                    .requested_task
                    .as_ref()
                    .is_some_and(|r| r.same_task(&task.reference))
                    && !this.closing
                {
                    this.pending_reader = Some(TaskDetailRequest {
                        task: task.clone(),
                        path: this.request.path.clone(),
                    });
                    this.close(cx);
                    return;
                }
                if task.reference.same_task(&this.request.task.reference)
                    && task.jira.as_ref().is_none_or(|d| d.complete)
                {
                    this.request.task = task.clone();
                    this.metadata_editor
                        .update(cx, |s, cx| s.set_task(task.clone(), cx));
                    this.description.update(cx, |s, cx| {
                        s.set_content(
                            task.body.clone(),
                            task.url.clone(),
                            this.interactive && this.tab == DetailTab::Description,
                            cx,
                        )
                    });
                    cx.notify();
                }
            }),
            cx.subscribe(
                &state,
                |this, _, event: &crate::app_state::WriteFinished, cx| {
                    if event
                        .command
                        .task()
                        .is_some_and(|task| task.same_task(&this.request.task.reference))
                    {
                        // The operation's owning control presents its inline error.
                        if let Ok(receipt) = &event.result {
                            if receipt.deleted_task.is_some() {
                                this.close(cx);
                                return;
                            }
                            this.notice = receipt.notice.clone();
                        }
                        cx.notify();
                    }
                },
            ),
            cx.observe(&comments, |_, _, cx| cx.notify()),
            cx.subscribe(
                &cx.global::<AppState>().projects.clone(),
                |this, _, _: &crate::app_state::AgentFocusRequested, cx| {
                    this.restore_focus = false;
                    this.pending_reader = None;
                    this.requested_task = None;
                    this.pending_edit = None;
                    this.pending_worktree = None;
                    this.close(cx);
                },
            ),
            cx.observe(&state, |this, state, cx| {
                let state = state.read(cx);
                let credential = state
                    .config
                    .account_for(&this.request.task.reference.project)
                    .map(|a| a.credential.clone());
                if state.path.as_ref() != Some(&this.request.path) || credential != this.credential
                {
                    this.restore_focus = false;
                    this.pending_reader = None;
                    this.requested_task = None;
                    this.pending_edit = None;
                    this.pending_worktree = None;
                    this.close(cx);
                    return;
                }
                if this.requested_task.is_some() && state.task_error.is_some() {
                    this.error = state.task_error.clone();
                }
                if this.linking && !state.busy {
                    this.linking = false;
                    this.error = state.error.clone();
                }
                if let Some(task) = state
                    .items
                    .iter()
                    .find(|task| {
                        task.reference.same_task(&this.request.task.reference)
                            && task.jira.as_ref().is_none_or(|d| d.complete)
                    })
                    .cloned()
                {
                    this.request.task = task.clone();
                    this.metadata_editor
                        .update(cx, |s, cx| s.set_task(task.clone(), cx));
                    this.description.update(cx, |s, cx| {
                        s.set_content(
                            task.body,
                            task.url,
                            this.interactive && this.tab == DetailTab::Description,
                            cx,
                        )
                    });
                }
                cx.notify();
            }),
            cx.observe(&cx.global::<AppState>().git.clone(), |_, _, cx| cx.notify()),
        ];
        let refresh = state.clone();
        let reference = request.task.reference.clone();
        window.on_next_frame(move |_, cx| {
            refresh.update(cx, |state, cx| state.open_linked(reference, cx))
        });
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        Self {
            request,
            metadata_editor,
            description,
            comments,
            tab: DetailTab::Description,
            tabs: Transition::new(0., now),
            presence,
            closing: false,
            scheduled: false,
            activation_scheduled: false,
            interactive: false,
            focus,
            return_focus,
            restore_focus: true,
            credential,
            error: None,
            notice: None,
            pending_edit: None,
            requested_task: None,
            pending_reader: None,
            linking: false,
            pending_worktree: None,
            _observers: observers,
        }
    }
    fn enable_contents(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.interactive = enabled;
        self.metadata_editor
            .update(cx, |s, cx| s.enable(enabled, cx));
        self.description.update(cx, |view, cx| {
            view.set_content(
                self.request.task.body.clone(),
                self.request.task.url.clone(),
                enabled && self.tab == DetailTab::Description,
                cx,
            )
        });
        self.comments.update(cx, |view, cx| {
            view.enable(enabled && self.tab == DetailTab::Comments, cx)
        });
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.enable_contents(false, cx);
        self.comments.update(cx, |view, cx| view.stop(cx));
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn select_tab(&mut self, tab: DetailTab, cx: &mut Context<Self>) {
        if self.closing || self.tab == tab {
            return;
        }
        self.tab = tab;
        self.enable_contents(false, cx);
        self.tabs.retarget(
            tab.position(),
            presets::STATE_CHANGE,
            Instant::now(),
            motion::policy(cx),
        );
        cx.notify();
    }
    fn link(&mut self, cx: &mut Context<Self>) {
        if !self.interactive {
            return;
        }
        let path = self.request.path.clone();
        let reference = self.request.task.reference.clone();
        let state = cx.global::<AppState>().integrations.clone();
        let linked = state
            .read(cx)
            .config
            .links
            .get(&path)
            .is_some_and(|tasks| tasks.iter().any(|t| t.same_task(&reference)));
        let result = state.update(cx, |state, cx| {
            if linked {
                state.unlink(path, &reference, cx)
            } else {
                state.link(path, reference, cx)
            }
        });
        self.error = result.err();
        self.linking = self.error.is_none();
        cx.notify();
    }
    fn edit_issue(&mut self, cx: &mut Context<Self>) {
        if !self.interactive {
            return;
        }
        self.pending_edit = Some(super::task_edit::TaskEditRequest::Edit {
            task: self.request.task.clone(),
            path: self.request.path.clone(),
        });
        self.close(cx);
    }
    fn new_worktree(&mut self, cx: &mut Context<Self>) {
        if !self.interactive {
            return;
        }
        let state = cx.global::<AppState>();
        let repository = state
            .integrations
            .read(cx)
            .repository
            .as_ref()
            .and_then(|r| state.git.read(cx).repository(&r.repository))
            .cloned();
        if let Some(repository) = repository {
            self.pending_worktree = Some(TaskWorktreeRequest {
                task: self.request.task.reference.clone(),
                repository,
            });
            self.close(cx);
        } else {
            self.error = Some("Wait for repository metadata, then retry.".into());
            cx.notify();
        }
    }
}
impl Render for TaskDetailDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let page = self.tabs.value(now);
        let moving = self.presence.is_animating(now) || self.tabs.is_animating(now);
        motion::request_frame(window, moving);
        if self.closing && !moving && !self.scheduled {
            self.scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    if this.restore_focus {
                        this.return_focus.focus(window, cx);
                    }
                    cx.emit(ModalDismissed);
                });
            });
        } else if !self.closing && !moving && !self.interactive && !self.activation_scheduled {
            self.activation_scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.activation_scheduled = false;
                    if !this.closing && !this.tabs.is_animating(Instant::now()) {
                        this.enable_contents(true, cx);
                        cx.notify();
                    }
                });
            });
        }
        let layout = view::DetailLayout::new(window.viewport_size());
        let disabled = !self.interactive || moving || self.closing;
        let state = cx.global::<AppState>();
        let busy = state.integrations.read(cx).busy || state.git.read(cx).busy;
        let linked = state
            .integrations
            .read(cx)
            .config
            .links
            .get(&self.request.path)
            .is_some_and(|tasks| {
                tasks
                    .iter()
                    .any(|t| t.same_task(&self.request.task.reference))
            });
        let count = self.comments.read(cx).total;
        let url = self.request.task.url.clone();
        let tabs = view::tab_bar(DetailTab::ALL.map(|tab| {
            view::tab_button(tab, self.tab, count)
                .disabled(self.closing)
                .on_click(cx.listener(move |this, _, _, cx| this.select_tab(tab, cx)))
        }));
        let description = div()
            .id("expanded-task-description")
            .size_full()
            .overflow_y_scrollbar()
            .child(view::description(
                (!layout.wide()).then(|| self.metadata_editor.clone().into_any_element()),
                self.description.clone(),
            ));
        let body = row()
            .items_stretch()
            .flex_1()
            .min_h_0()
            .gap(px(24.))
            .child(
                column()
                    .flex_1()
                    .gap(px(12.))
                    .child(tabs)
                    .child(view::pages(page, description, self.comments.clone())),
            )
            .children(layout.wide().then(|| {
                view::metadata(
                    &self.request.task.reference,
                    &self.request.path,
                    self.metadata_editor.clone(),
                )
                .w(px(208.))
                .flex_shrink_0()
                .pl(px(20.))
                .border_l_1()
                .border_color(t::border())
                .id("task-detail-metadata")
                .overflow_y_scrollbar()
            }));
        let actions = row()
            .gap(px(8.))
            .child(
                icon_button("edit-task", IconName::Replace, "Edit issue")
                    .disabled(disabled || busy)
                    .on_click(cx.listener(|this, _, _, cx| this.edit_issue(cx))),
            )
            .child(
                icon_button(
                    "expanded-task-browser",
                    IconName::ExternalLink,
                    "Open task in browser",
                )
                .disabled(disabled)
                .on_click(move |_, _, cx| cx.open_url(&url)),
            )
            .child(
                icon_button("close-expanded-task", IconName::Close, "Back to task list")
                    .disabled(self.closing)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            );
        let panel = layout
            .surface(progress)
            .child(view::header(&self.request.task.reference, linked, actions))
            .child(view::title(self.request.task.reference.title.clone()))
            .child(body)
            .children(
                self.notice
                    .clone()
                    .map(|notice| integration_message(notice, false)),
            )
            .children(
                self.error
                    .clone()
                    .map(|error| integration_message(error, true)),
            )
            .child(view::footer(
                button(
                    "expanded-task-link",
                    if linked { "Unlink" } else { "Link to worktree" },
                )
                .disabled(disabled || busy)
                .on_click(cx.listener(|this, _, _, cx| this.link(cx))),
                (if self.tab == DetailTab::Comments {
                    button("expanded-task-worktree", "New worktree")
                } else {
                    primary_button("expanded-task-worktree", "New worktree")
                })
                .disabled(disabled || busy)
                .on_click(cx.listener(|this, _, _, cx| this.new_worktree(cx))),
            ));
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(true)
            .on_ok(|_, _, _| false)
            .on_cancel(move |event, window, cx| {
                cancel(event, window, cx);
                false
            })
            .backdrop(modal::modal_backdrop(progress))
            .popup(panel)
    }
}
