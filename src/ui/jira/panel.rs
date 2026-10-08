use super::JiraMode;
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::integrations::{
    AccountScope, TaskItem, TaskRef, TaskWrite, client, credentials,
    jira::{Jira, JiraWrite, SchemaRequest},
};
use gpui_kit::{base::Disableable, component::IconName, *};
use std::{path::PathBuf, sync::Arc};
#[derive(Clone)]
pub struct JiraAction(pub JiraMode);
#[derive(Clone)]
pub struct RelatedRequested(pub TaskRef);
#[derive(Clone, Copy)]
enum Activity {
    Watch,
    Vote,
    Attach,
    Remove,
    Download,
    History,
    MoreHistory,
    Idle,
}
impl EventEmitter<RelatedRequested> for JiraPanel {}
impl EventEmitter<JiraAction> for JiraPanel {}
pub struct JiraPanel {
    task: TaskItem,
    path: PathBuf,
    enabled: bool,
    pending: Option<u64>,
    error: Option<String>,
    notice: Option<String>,
    confirm: Option<JiraWrite>,
    previews: std::collections::HashMap<String, WindowHandle<gpui_kit::component::Root>>,
    read: Option<Task<()>>,
    history: Vec<String>,
    history_next: Option<String>,
    history_open: bool,
    loading: bool,
    feedback: [ButtonLoading; 8],
    downloading: Option<String>,
    _events: Vec<Subscription>,
}
impl JiraPanel {
    pub fn new(task: TaskItem, path: PathBuf, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let events = vec![
            cx.subscribe(&state, |s, _, e: &WriteFinished, cx| {
                if s.pending != Some(e.id) {
                    return;
                }
                s.pending = None;
                let removing = s.feedback[Activity::Remove as usize].active();
                s.finish_feedback(cx);
                match &e.result {
                    Ok(r) => {
                        if removing {
                            s.confirm = None;
                        }
                        s.error = None;
                        s.notice = r.notice.clone();
                        if let Some(task) = &r.task {
                            s.task = task.clone();
                        }
                    }
                    Err(e) => s.error = Some(e.to_string()),
                }
                cx.notify();
            }),
            cx.observe(&state, |_, _, cx| cx.notify()),
        ];
        Self {
            task,
            path,
            enabled: false,
            pending: None,
            error: None,
            notice: None,
            confirm: None,
            previews: Default::default(),
            read: None,
            history: vec![],
            history_next: None,
            history_open: false,
            loading: false,
            feedback: std::array::from_fn(|_| ButtonLoading::default()),
            downloading: None,
            _events: events,
        }
    }
    pub fn set_task(&mut self, task: TaskItem, cx: &mut Context<Self>) {
        if task.jira.as_ref().is_some_and(|d| d.complete) {
            self.task = task;
        }
        cx.notify();
    }
    pub fn enable(&mut self, value: bool, cx: &mut Context<Self>) {
        self.enabled = value;
        cx.notify();
    }
    fn editable(&self, cx: &App) -> bool {
        self.enabled
            && self.pending.is_none()
            && !self.loading
            && !cx.global::<AppState>().integrations.read(cx).busy
            && !cx.global::<AppState>().settings.read(cx).quitting
    }
    fn send(&mut self, action: JiraWrite, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        self.error = None;
        self.notice = None;
        let activity = match &action {
            JiraWrite::Watch { .. } => Activity::Watch,
            JiraWrite::Vote { .. } => Activity::Vote,
            JiraWrite::Attach { .. } => Activity::Attach,
            _ => Activity::Remove,
        };
        let command = TaskWrite::Jira {
            project: self.task.reference.project.clone(),
            task: Some(self.task.reference.clone()),
            action,
        };
        match cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |s, cx| s.submit(command, None, cx))
        {
            Ok(id) => {
                self.pending = Some(id);
                self.start_feedback(activity, cx);
            }
            Err(e) => self.error = Some(e),
        }
        cx.notify();
    }
    fn start_feedback(&mut self, activity: Activity, cx: &App) {
        for (index, feedback) in self.feedback.iter_mut().enumerate() {
            feedback.set(index == activity as usize, cx);
        }
    }
    fn finish_feedback(&mut self, cx: &App) {
        for feedback in &mut self.feedback {
            feedback.set(false, cx);
        }
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
        self.start_feedback(Activity::Attach, cx);
        self.error = None;
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let path = selection
                    .await
                    .map_err(|_| "File picker closed.")?
                    .map_err(|e| e.to_string())?
                    .and_then(|v| v.into_iter().next());
                let Some(path) = path else {
                    return Ok(None);
                };
                cx.background_executor()
                    .spawn(async move {
                        use std::io::Read;
                        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                        let mut bytes = vec![];
                        file.by_ref()
                            .take(25 * 1024 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|e| e.to_string())?;
                        if bytes.len() > 25 * 1024 * 1024 {
                            return Err("Choose a file up to 25 MiB.".into());
                        }
                        let name = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .ok_or("Invalid filename.")?
                            .to_owned();
                        Ok(Some(JiraWrite::Attach {
                            name,
                            bytes: Arc::new(bytes),
                        }))
                    })
                    .await
            }
            .await;
            let _ = this.update(cx, |s, cx| {
                s.loading = false;
                s.finish_feedback(cx);
                match result {
                    Ok(Some(action)) => s.send(action, cx),
                    Ok(None) => {}
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn preview(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        self.previews
            .retain(|_, window| window.update(cx, |_, _, _| ()).is_ok());
        if let Some(window) = self.previews.get(&id)
            && window
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        match crate::ui::attachment_preview::open(
            self.task.reference.clone(),
            id.clone(),
            name,
            self.path.clone(),
            cx,
        ) {
            Ok(window) => {
                self.previews.insert(id, window);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.notify();
    }
    fn download(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        let filename = std::path::Path::new(&name)
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("attachment");
        let selection = cx.prompt_for_new_path(&self.path, Some(filename));
        let Some(account) = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .config
            .account_for(&self.task.reference.project)
            .cloned()
        else {
            return;
        };
        let task = self.task.reference.clone();
        let http = cx.http_client();
        self.loading = true;
        self.downloading = Some(id.clone());
        self.start_feedback(Activity::Download, cx);
        self.error = None;
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let path = selection
                    .await
                    .map_err(|_| "Save picker closed.")?
                    .map_err(|e| e.to_string())?;
                let Some(path) = path else {
                    return Ok(None);
                };
                let key = account.credential.clone();
                let token = cx
                    .background_executor()
                    .spawn(async move { credentials::load(&key) })
                    .await?;
                let AccountScope::Jira {
                    site,
                    email,
                    cloud_id,
                } = account.scope
                else {
                    return Err("Select a Jira connection.".into());
                };
                let bytes = Jira::new(
                    http,
                    &site,
                    &email,
                    cloud_id.as_deref(),
                    token,
                    account.login,
                )?
                .download_attachment(&task, &id)
                .await?;
                cx.background_executor()
                    .spawn(async move {
                        use std::io::Write;
                        let parent = path.parent().ok_or("Invalid save path.")?;
                        let mut temp =
                            tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
                        temp.write_all(&bytes).map_err(|e| e.to_string())?;
                        temp.as_file().sync_all().map_err(|e| e.to_string())?;
                        temp.persist(&path).map_err(|e| e.to_string())?;
                        Ok::<_, String>(Some(path))
                    })
                    .await
            }
            .await;
            let _ = this.update(cx, |s, cx| {
                s.loading = false;
                s.finish_feedback(cx);
                match result {
                    Ok(Some(_)) => s.notice = Some("Attachment downloaded.".into()),
                    Ok(None) => {}
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn history(&mut self, more: bool, cx: &mut Context<Self>) {
        if !self.editable(cx) {
            return;
        }
        let Some(account) = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .config
            .account_for(&self.task.reference.project)
            .cloned()
        else {
            return;
        };
        let task = self.task.reference.clone();
        let cursor = if more {
            self.history_next.clone()
        } else {
            None
        };
        let http = cx.http_client();
        self.loading = true;
        self.start_feedback(
            if more {
                Activity::MoreHistory
            } else {
                Activity::History
            },
            cx,
        );
        self.history_open = true;
        self.error = None;
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let key = account.credential.clone();
                let token = cx
                    .background_executor()
                    .spawn(async move { credentials::load(&key) })
                    .await?;
                client(http, &account, token)?
                    .schema(
                        &task.project,
                        &SchemaRequest::History {
                            key: task.id,
                            cursor,
                        },
                    )
                    .await
            }
            .await;
            let _ = this.update(cx, |s, cx| {
                s.loading = false;
                s.finish_feedback(cx);
                match result {
                    Ok(v) => {
                        if !more {
                            s.history.clear();
                        }
                        s.history.extend(v.activity);
                        s.history_next = if s.history.len() < 500 {
                            v.next_cursor
                        } else {
                            None
                        };
                    }
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn open_related(&self, key: String, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        if let Some((project, _)) = key.split_once('-')
            && let Some(site) = &self.task.reference.project.site
            && let Ok(target) = canopy_desktop::integrations::ProjectTarget::jira(site, project)
        {
            let reference = TaskRef {
                project: target,
                id: key,
                title: String::new(),
            };
            cx.emit(RelatedRequested(reference.clone()));
            cx.global::<AppState>()
                .integrations
                .clone()
                .update(cx, |s, cx| s.open_linked(reference, cx));
        }
    }
}
impl Render for JiraPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(details) = self.task.jira.clone() else {
            return column();
        };
        let f = &details.fields;
        let disabled = !self.editable(cx);
        let line = |label: &str, value: String| {
            column()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(t::muted())
                        .child(label.to_owned()),
                )
                .child(div().text_size(px(12.)).child(value))
        };
        let fields = [
            ("Type", details.issue_type.clone()),
            ("Assignee", self.task.assignees.join(", ")),
            (
                "Reporter",
                f["reporter"]["displayName"].as_str().unwrap_or("—").into(),
            ),
            (
                "Priority",
                f["priority"]["name"].as_str().unwrap_or("—").into(),
            ),
            ("Due date", f["duedate"].as_str().unwrap_or("—").into()),
            ("Labels", self.task.labels.join(", ")),
        ];
        column()
            .gap(px(12.))
            .children(fields.into_iter().map(|(name, value)| {
                line(
                    name,
                    if value.is_empty() {
                        "—".into()
                    } else {
                        value
                    },
                )
            }))
            .child(
                column().gap(px(6.)).children(
                    [
                        JiraMode::Edit,
                        JiraMode::Link,
                        JiraMode::LogWork,
                        JiraMode::Sprint,
                        JiraMode::Delete,
                    ]
                    .into_iter()
                    .map(|mode| {
                        button(mode.label(), mode.label())
                            .w_full()
                            .disabled(disabled)
                            .on_click(cx.listener(move |_, _, _, cx| cx.emit(JiraAction(mode))))
                    }),
                ),
            )
            .child(
                row()
                    .gap(px(6.))
                    .child(
                        loading_button(
                            "jira-watch",
                            if f["watches"]["isWatching"] == true {
                                "Unwatch"
                            } else {
                                "Watch"
                            },
                            &self.feedback[Activity::Watch as usize],
                        )
                        .disabled(disabled && !self.feedback[Activity::Watch as usize].active())
                        .on_click(cx.listener(|s, _, _, cx| {
                            let watching = s
                                .task
                                .jira
                                .as_ref()
                                .is_none_or(|d| d.fields["watches"]["isWatching"] != true);
                            s.send(JiraWrite::Watch { watching }, cx)
                        })),
                    )
                    .child(
                        loading_button(
                            "jira-vote",
                            if f["votes"]["hasVoted"] == true {
                                "Unvote"
                            } else {
                                "Vote"
                            },
                            &self.feedback[Activity::Vote as usize],
                        )
                        .disabled(disabled && !self.feedback[Activity::Vote as usize].active())
                        .on_click(cx.listener(|s, _, _, cx| {
                            let voted = s
                                .task
                                .jira
                                .as_ref()
                                .is_none_or(|d| d.fields["votes"]["hasVoted"] != true);
                            s.send(JiraWrite::Vote { voted }, cx)
                        })),
                    ),
            )
            .child(
                loading_button_with_icon(
                    "jira-upload",
                    "Attach file",
                    IconName::Plus,
                    &self.feedback[Activity::Attach as usize],
                )
                .disabled(disabled && !self.feedback[Activity::Attach as usize].active())
                .on_click(cx.listener(|s, _, _, cx| s.upload(cx))),
            )
            .children(
                f["attachment"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| {
                        let id = a["id"].as_str()?.to_owned();
                        let delete = id.clone();
                        let download = id.clone();
                        let download_name =
                            a["filename"].as_str().unwrap_or("Attachment").to_owned();
                        let name = a["filename"].as_str().unwrap_or("Attachment").to_owned();
                        Some(
                            row()
                                .gap(px(4.))
                                .child(
                                    button(
                                        SharedString::from(format!("download-{id}")),
                                        name.clone(),
                                    )
                                    .flex_1()
                                    .disabled(disabled)
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| s.preview(id.clone(), name.clone(), cx),
                                    )),
                                )
                                .child(
                                    loading_icon_button(
                                        SharedString::from(format!("save-attachment-{download}")),
                                        custom_icon("download"),
                                        "Download attachment",
                                        Some(
                                            &self.feedback[if self.downloading.as_ref()
                                                == Some(&download)
                                            {
                                                Activity::Download
                                            } else {
                                                Activity::Idle
                                            }
                                                as usize],
                                        ),
                                    )
                                    .disabled(
                                        disabled
                                            && !(self.downloading.as_ref() == Some(&download)
                                                && self.feedback[Activity::Download as usize]
                                                    .active()),
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.download(download.clone(), download_name.clone(), cx)
                                        },
                                    )),
                                )
                                .child(
                                    icon_button(
                                        SharedString::from(format!("delete-attachment-{delete}")),
                                        IconName::Close,
                                        "Delete attachment",
                                    )
                                    .disabled(disabled)
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.confirm = Some(JiraWrite::DeleteAttachment {
                                                id: delete.clone(),
                                            });
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                    }),
            )
            .children(
                f["issuelinks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|l| {
                        let id = l["id"].as_str()?.to_owned();
                        let outward = l.get("outwardIssue");
                        let issue = outward.unwrap_or(&l["inwardIssue"]);
                        let key = issue["key"].as_str()?.to_owned();
                        let text = format!(
                            "{} {}",
                            l["type"][if outward.is_some() {
                                "outward"
                            } else {
                                "inward"
                            }]
                            .as_str()
                            .unwrap_or("Related"),
                            key
                        );
                        Some(
                            row()
                                .gap(px(4.))
                                .child(
                                    button(SharedString::from(format!("jira-related-{id}")), text)
                                        .flex_1()
                                        .disabled(disabled)
                                        .on_click(cx.listener(move |s, _, _, cx| {
                                            s.open_related(key.clone(), cx)
                                        })),
                                )
                                .child(
                                    icon_button(
                                        SharedString::from(format!("remove-link-{id}")),
                                        IconName::Close,
                                        "Remove relationship",
                                    )
                                    .disabled(disabled)
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            s.confirm =
                                                Some(JiraWrite::DeleteLink { id: id.clone() });
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                    }),
            )
            .children(
                f["subtasks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| {
                        let key = v["key"].as_str()?.to_owned();
                        Some(
                            button(
                                SharedString::from(format!("subtask-{key}")),
                                format!("Subtask {key}"),
                            )
                            .disabled(disabled)
                            .on_click(
                                cx.listener(move |s, _, _, cx| s.open_related(key.clone(), cx)),
                            ),
                        )
                    }),
            )
            .children(self.confirm.as_ref().map(|_| {
                column()
                    .gap(px(6.))
                    .child(integration_message(
                        "Remove this item from Jira? This cannot be undone.",
                        true,
                    ))
                    .child(
                        row()
                            .gap(px(6.))
                            .child(
                                button("jira-cancel-delete", "Cancel")
                                    .disabled(self.feedback[Activity::Remove as usize].active())
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.confirm = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                loading_button(
                                    "jira-confirm-delete",
                                    "Remove",
                                    &self.feedback[Activity::Remove as usize],
                                )
                                .text_color(t::red())
                                .disabled(
                                    disabled && !self.feedback[Activity::Remove as usize].active(),
                                )
                                .on_click(cx.listener(
                                    |s, _, _, cx| {
                                        if let Some(action) = s.confirm.clone() {
                                            s.send(action, cx);
                                        }
                                    },
                                )),
                            ),
                    )
            }))
            .children(self.error.clone().map(|e| integration_message(e, true)))
            .children(self.notice.clone().map(|e| integration_message(e, false)))
            .child(
                loading_button(
                    "jira-history",
                    "Activity",
                    &self.feedback[Activity::History as usize],
                )
                .disabled(disabled && !self.feedback[Activity::History as usize].active())
                .on_click(cx.listener(|s, _, _, cx| {
                    if s.history_open {
                        s.history_open = false;
                        cx.notify();
                    } else {
                        s.history(false, cx);
                    }
                })),
            )
            .children(self.history_open.then(|| {
                column()
                    .gap(px(12.))
                    .children(self.history.iter().map(|line| {
                        div()
                            .text_size(px(11.))
                            .text_color(t::secondary())
                            .child(line.clone())
                    }))
                    .children(self.history_next.is_some().then(|| {
                        loading_button(
                            "jira-more-history",
                            "Older activity",
                            &self.feedback[Activity::MoreHistory as usize],
                        )
                        .disabled(
                            disabled && !self.feedback[Activity::MoreHistory as usize].active(),
                        )
                        .on_click(cx.listener(|s, _, _, cx| s.history(true, cx)))
                    }))
            }))
    }
}
