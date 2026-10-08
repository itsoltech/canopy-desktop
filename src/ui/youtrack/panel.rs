use crate::{
    app_state::{AppState, WriteFinished},
    ui::{
        components::integrations::integration_message,
        components::*,
        task_edit::picker::{OptionChanged, OptionPicker},
        theme as t,
    },
};
use canopy_desktop::integrations::{
    OptionKind, TaskItem, TaskOption, TaskWrite,
    youtrack::{YoutrackAttachment, YoutrackWrite},
};
use gpui_kit::{base::Disableable, component::IconName, *};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct YoutrackEditRequested;
impl EventEmitter<YoutrackEditRequested> for YoutrackPanel {}

pub struct YoutrackPanel {
    task: TaskItem,
    path: PathBuf,
    enabled: bool,
    pending: Option<u64>,
    error: Option<String>,
    notice: Option<String>,
    loading: bool,
    confirm_delete: Option<YoutrackWrite>,
    tag: Entity<OptionPicker>,
    previews: std::collections::HashMap<String, WindowHandle<gpui_kit::component::Root>>,
    read: Option<Task<()>>,
    feedback: ButtonLoading,
    _events: Vec<Subscription>,
}

impl YoutrackPanel {
    pub fn new(task: TaskItem, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let tag = cx.new(|cx| {
            OptionPicker::new(
                task.reference.project.clone(),
                OptionKind::Label,
                window,
                cx,
            )
        });
        let events = vec![
            cx.subscribe(&state, |this, _, event: &WriteFinished, cx| {
                if this.pending != Some(event.id) {
                    return;
                }
                this.pending = None;
                this.feedback.set(false, cx);
                this.tag
                    .update(cx, |picker, cx| picker.enable(this.enabled, cx));
                match &event.result {
                    Ok(receipt) => {
                        this.error = None;
                        this.notice = receipt.notice.clone();
                        if let Some(task) = &receipt.task {
                            this.task = task.clone();
                        }
                        this.sync_tags(cx);
                    }
                    Err(error) => {
                        this.error = Some(error.to_string());
                        this.sync_tags(cx);
                    }
                }
                cx.notify();
            }),
            cx.subscribe(&tag, |this, _, event: &OptionChanged, cx| {
                this.send(
                    YoutrackWrite::Tag {
                        id: event.value.clone(),
                        add: event.selected,
                    },
                    cx,
                );
            }),
            cx.observe(&state, |_, _, cx| cx.notify()),
        ];
        let mut this = Self {
            task,
            path,
            enabled: false,
            pending: None,
            error: None,
            notice: None,
            loading: false,
            confirm_delete: None,
            tag,
            previews: Default::default(),
            read: None,
            feedback: ButtonLoading::default(),
            _events: events,
        };
        this.sync_tags(cx);
        this
    }

    fn sync_tags(&mut self, cx: &mut Context<Self>) {
        let tags = self
            .task
            .youtrack
            .as_ref()
            .map(|details| details.tags.clone())
            .unwrap_or_default();
        self.tag.update(cx, |picker, cx| {
            picker.set_selected(tags.iter().map(|tag| tag.id.clone()).collect(), cx);
            for tag in &tags {
                picker.seed_current(
                    TaskOption {
                        value: tag.id.clone(),
                        label: tag.name.clone(),
                    },
                    cx,
                );
            }
        });
    }

    pub fn set_task(&mut self, task: TaskItem, cx: &mut Context<Self>) {
        if task
            .youtrack
            .as_ref()
            .is_some_and(|details| details.complete)
        {
            self.task = task;
            self.sync_tags(cx);
        }
        cx.notify();
    }

    pub fn enable(&mut self, value: bool, cx: &mut Context<Self>) {
        self.enabled = value;
        self.tag.update(cx, |picker, cx| picker.enable(value, cx));
        cx.notify();
    }

    fn editable(&self, cx: &App) -> bool {
        self.enabled
            && self.pending.is_none()
            && !self.loading
            && !cx.global::<AppState>().integrations.read(cx).busy
            && !cx.global::<AppState>().settings.read(cx).quitting
    }

    fn send(&mut self, action: YoutrackWrite, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        let command = TaskWrite::Youtrack {
            project: self.task.reference.project.clone(),
            task: Some(self.task.reference.clone()),
            action,
        };
        match cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |state, cx| state.submit(command, None, cx))
        {
            Ok(id) => {
                self.pending = Some(id);
                self.feedback.set(true, cx);
                self.tag.update(cx, |picker, cx| picker.enable(false, cx));
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn upload(&mut self, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Attach file".into()),
        });
        self.loading = true;
        self.error = None;
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let path = selection
                    .await
                    .map_err(|_| "File picker closed.")?
                    .map_err(|error| error.to_string())?
                    .and_then(|paths| paths.into_iter().next());
                let Some(path) = path else { return Ok(None) };
                cx.background_executor()
                    .spawn(async move {
                        use std::io::Read;
                        let mut bytes = Vec::new();
                        std::fs::File::open(&path)
                            .map_err(|error| error.to_string())?
                            .take(25 * 1024 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|error| error.to_string())?;
                        if bytes.len() > 25 * 1024 * 1024 {
                            return Err("Choose a file up to 25 MiB.".into());
                        }
                        let name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .ok_or("Invalid filename.")?
                            .to_owned();
                        Ok(Some(YoutrackWrite::Attach {
                            name,
                            mime_type: "application/octet-stream".into(),
                            bytes: Arc::new(bytes),
                        }))
                    })
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(Some(action)) => this.send(action, cx),
                    Ok(None) => {}
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn preview(&mut self, attachment: YoutrackAttachment, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        self.previews
            .retain(|_, window| window.update(cx, |_, _, _| ()).is_ok());
        if let Some(window) = self.previews.get(&attachment.id)
            && window
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        match crate::ui::attachment_preview::open(
            self.task.reference.clone(),
            attachment.id.clone(),
            attachment.name.clone(),
            self.path.clone(),
            cx,
        ) {
            Ok(window) => {
                self.previews.insert(attachment.id, window);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }
}

impl Render for YoutrackPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(details) = self.task.youtrack.clone() else {
            return column();
        };
        let disabled = !self.editable(cx);
        let edit = button("youtrack-edit-fields", "Edit fields")
            .w_full()
            .disabled(disabled)
            .on_click(cx.listener(|_, _, _, cx| cx.emit(YoutrackEditRequested)));
        let attachment_rows = details.attachments.iter().map(|attachment| {
            let value = attachment.clone();
            let preview_value = value.clone();
            let delete_id = value.id.clone();
            row()
                .gap(px(6.))
                .child(
                    button(
                        SharedString::from(format!("youtrack-attachment-{}", value.id)),
                        value.name.clone(),
                    )
                    .flex_1()
                    .disabled(disabled)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.preview(preview_value.clone(), cx)),
                    ),
                )
                .child(
                    icon_button(
                        SharedString::from(format!("delete-youtrack-attachment-{delete_id}")),
                        IconName::Delete,
                        "Delete attachment",
                    )
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm_delete = Some(YoutrackWrite::DeleteAttachment {
                            id: delete_id.clone(),
                        });
                        cx.notify();
                    })),
                )
        });
        let delete_issue = button("delete-youtrack-issue", "Delete issue")
            .w_full()
            .text_color(t::red())
            .disabled(disabled)
            .on_click(cx.listener(|this, _, _, cx| {
                this.confirm_delete = Some(YoutrackWrite::DeleteIssue {
                    confirmation: this.task.reference.id.clone(),
                });
                cx.notify();
            }));
        column()
            .gap(px(12.))
            .children(
                [
                    ("Type", details.issue_type.clone()),
                    (
                        "Priority",
                        if details.priority.is_empty() {
                            "—".into()
                        } else {
                            details.priority.clone()
                        },
                    ),
                    ("Reporter", details.reporter.clone()),
                    ("Assignee", self.task.assignees.join(", ")),
                ]
                .into_iter()
                .map(|(label, value)| {
                    column()
                        .gap(px(3.))
                        .child(div().text_size(px(10.)).text_color(t::muted()).child(label))
                        .child(div().text_size(px(12.)).child(if value.is_empty() {
                            "—".into()
                        } else {
                            value
                        }))
                }),
            )
            .child(edit)
            .child(delete_issue)
            .child(self.tag.clone())
            .child(
                column()
                    .gap(px(6.))
                    .child(
                        row()
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(t::secondary())
                                    .child("Attachments"),
                            )
                            .child(
                                button("upload-youtrack-attachment", "Upload")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, _, cx| this.upload(cx))),
                            ),
                    )
                    .children(attachment_rows),
            )
            .children(self.confirm_delete.clone().map(|action| {
                column()
                    .gap(px(6.))
                    .child(integration_message(
                        if matches!(action, YoutrackWrite::DeleteIssue { .. }) {
                            "Delete this YouTrack issue? This cannot be undone."
                        } else {
                            "Delete this YouTrack attachment? This cannot be undone."
                        },
                        true,
                    ))
                    .child(
                        row()
                            .gap(px(6.))
                            .justify_end()
                            .child(
                                button("cancel-youtrack-delete", "Cancel")
                                    .disabled(self.pending.is_some())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_delete = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                loading_button(
                                    "confirm-youtrack-delete",
                                    if matches!(action, YoutrackWrite::DeleteIssue { .. }) {
                                        "Delete issue"
                                    } else {
                                        "Delete attachment"
                                    },
                                    &self.feedback,
                                )
                                .text_color(t::red())
                                .disabled(disabled && !self.feedback.active())
                                .on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.send(action.clone(), cx)
                                    }),
                                ),
                            ),
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
