//! Task details sections: metadata, provider actions, attachments, comments
//! and the inline option picker. Geometry follows the local task panels.
use super::*;
use crate::ui::components::{integrations::integration_message, task_detail as view};
use canopy_desktop::integrations::Provider;
use gpui_kit::component::IconName;

fn meta_field(label: impl Into<SharedString>, content: impl IntoElement) -> Div {
    column().gap(px(6.)).child(caption(label)).child(content)
}
fn line(label: &'static str, value: String) -> Div {
    column()
        .gap(px(3.))
        .child(div().text_size(px(10.)).text_color(t::muted()).child(label))
        .child(div().text_size(px(12.)).child(value))
}
fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(t::muted())
        .child(text.into())
}

impl TasksView {
    pub(super) fn render_metadata(&self, task: &p::Task, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let provider = self.local_provider();
        let mut body = column().gap(px(20.));
        for status in &task.statuses {
            let content = if let Some(reason) = &status.read_only_reason {
                column()
                    .gap(px(4.))
                    .child(div().text_size(px(12.)).child(status.label.clone()))
                    .child(muted(reason.clone()))
            } else if status.requires_form {
                column().child(
                    button(
                        SharedString::from(format!("transitions:{}", status.id)),
                        format!(
                            "{}…",
                            if status.label.is_empty() {
                                "Change status"
                            } else {
                                status.label.as_str()
                            }
                        ),
                    )
                    .w_full()
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.tab = DetailTab::Description;
                        s.action = None;
                        s.load_schema(FormMode::Transition, cx)
                    })),
                )
            } else {
                column().child(
                    row()
                        .flex_wrap()
                        .gap(px(2.))
                        .p(px(2.))
                        .rounded(px(6.))
                        .bg(t::hover())
                        .children(status.choices.iter().map(|choice| {
                            let id = status.id.clone();
                            let choice = choice.clone();
                            selection_button(
                                SharedString::from(format!("status:{}:{}", id, choice.id)),
                                choice.label.clone(),
                                status.selected_id.as_ref() == Some(&choice.id),
                            )
                            .border_0()
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.mutate(
                                        Mutation::Status(p::TaskStatusChoice {
                                            field_id: id.clone(),
                                            choice_id: choice.id.clone(),
                                            ..Default::default()
                                        }),
                                        cx,
                                    )
                                },
                            ))
                        })),
                )
            }
            .children(status.hint.clone().map(muted));
            body = body.child(meta_field(status.name.to_uppercase(), content));
        }
        if provider == Provider::Jira {
            let fields = [
                ("Type", adapter::field_text(task, "issuetype")),
                ("Assignee", adapter::field_text(task, "assignee")),
                ("Reporter", adapter::field_text(task, "reporter")),
                ("Priority", adapter::field_text(task, "priority")),
                ("Due date", adapter::field_text(task, "duedate")),
            ];
            let mut details = column().gap(px(12.));
            for (label, value) in fields {
                details = details.child(line(label, value.unwrap_or_else(|| "—".into())));
            }
            for field in &task.fields {
                if field.name.to_lowercase().contains("sprint") {
                    let v = fields::json(field.current_value.as_ref());
                    let values = v.as_array().cloned().unwrap_or_else(|| vec![v]);
                    let names: Vec<_> = values
                        .iter()
                        .filter_map(|v| v["name"].as_str().or(v.as_str()).map(str::to_owned))
                        .collect();
                    details = details.child(line(
                        "Sprint",
                        if names.is_empty() {
                            "Backlog".into()
                        } else {
                            names.join(", ")
                        },
                    ));
                }
            }
            body = body.child(meta_field("DETAILS", details));
        }
        if self.supported("labels") {
            body = body.child(self.render_option_field(
                "label",
                if provider == Provider::Youtrack {
                    "Tags"
                } else {
                    "Labels"
                },
                cx,
            ));
        }
        if self.supported("assignees") {
            body = body.child(self.render_option_field("assignee", "Assignees", cx));
        }
        if self.supported("milestone") {
            body = body.child(self.render_option_field("milestone", "Milestone", cx));
        }
        body = body.child(self.render_provider_actions(task, cx));
        if self.supported("attachment.read") || self.supported("attachment.write") {
            body = body.child(self.render_attachments(task, cx));
        }
        if provider == Provider::Jira {
            body = body.child(self.render_jira_activity(task, cx));
        }
        body
    }
    /// Local OptionPicker geometry: label, removable chips and an inline list.
    pub(super) fn render_option_field(
        &self,
        kind: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let disabled = !self.ready();
        let open = self.choice_kind == Some(Choices::Metadata(kind));
        let selected = self.selected_options(kind);
        let chips = row()
            .flex_wrap()
            .gap(px(4.))
            .children(selected.iter().map(|(id, name)| {
                let id = id.clone();
                choice_chip(
                    SharedString::from(format!("selected-{kind}-{id}")),
                    format!("{name} ×"),
                )
                .disabled(disabled)
                .on_click(cx.listener(move |s, _, _, cx| s.remove_option(kind, &id, cx)))
            }))
            .children(selected.is_empty().then(|| muted("None")));
        column()
            .gap(px(6.))
            .child(
                row()
                    .gap(px(8.))
                    .child(div().flex_1().child(caption(label.to_uppercase())))
                    .child(
                        icon_button(
                            SharedString::from(format!("choose-{kind}")),
                            if open {
                                IconName::ChevronUp
                            } else {
                                IconName::Plus
                            },
                            format!("Choose {}", label.to_lowercase()),
                        )
                        .disabled(disabled && !open)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if s.choice_kind == Some(Choices::Metadata(kind)) {
                                s.choice_kind = None;
                                cx.notify();
                            } else {
                                s.load_choices(Choices::Metadata(kind), false, cx);
                            }
                        })),
                    ),
            )
            .child(chips)
            .children(open.then(|| self.render_picker(label, cx)))
    }
    fn selected_options(&self, kind: &str) -> Vec<(String, String)> {
        let name = |id: &String| self.names.get(id).cloned();
        if self.mode == FormMode::Create {
            let ids: Vec<String> = match kind {
                "label" => self.create_labels.clone(),
                "assignee" => self.create_assignees.clone(),
                _ => self.create_milestone.clone().into_iter().collect(),
            };
            return ids
                .into_iter()
                .map(|id| {
                    let label = name(&id).unwrap_or_else(|| id.clone());
                    (id, label)
                })
                .collect();
        }
        let Some(task) = &self.task else {
            return vec![];
        };
        let provider = self.local_provider();
        match kind {
            "label" => task
                .labels
                .iter()
                .map(|id| {
                    let label = name(id).unwrap_or_else(|| {
                        if provider == Provider::Youtrack {
                            "Unnamed tag".into()
                        } else {
                            id.clone()
                        }
                    });
                    (id.clone(), label)
                })
                .collect(),
            "assignee" => {
                let assignee = adapter::field_text(task, "assignee");
                task.assignees
                    .iter()
                    .map(|id| {
                        let label = name(id)
                            .or_else(|| {
                                (provider == Provider::Jira)
                                    .then(|| assignee.clone())
                                    .flatten()
                            })
                            .unwrap_or_else(|| id.clone());
                        (id.clone(), label)
                    })
                    .collect()
            }
            _ => task
                .milestone
                .iter()
                .map(|m| (m.id.clone(), m.label.clone()))
                .collect(),
        }
    }
    fn remove_option(&mut self, kind: &'static str, id: &str, cx: &mut Context<Self>) {
        if self.mode == FormMode::Create {
            match kind {
                "label" => self.create_labels.retain(|v| v != id),
                "assignee" => self.create_assignees.retain(|v| v != id),
                _ => self.create_milestone = None,
            }
            self.persist_draft(cx);
            cx.notify();
            return;
        }
        let action = match kind {
            "label" => Mutation::Tag(p::TaskTagWrite {
                id: id.into(),
                add: false,
            }),
            "assignee" => Mutation::Assignee(p::TaskAssigneeWrite {
                id: id.into(),
                add: false,
            }),
            _ => Mutation::Milestone(p::TaskOptionalChoice { id: None }),
        };
        self.mutate(action, cx);
    }
    /// Inline searchable list for the open choice kind (local picker panel).
    pub(super) fn render_picker(&self, label: &str, cx: &mut Context<Self>) -> Div {
        let query = value(&self.choice_query, cx).to_lowercase();
        let selected: Vec<String> = match &self.choice_kind {
            Some(Choices::Metadata(kind)) => self
                .selected_options(kind)
                .into_iter()
                .map(|v| v.0)
                .collect(),
            Some(Choices::Types) => self.issue_type.iter().map(|t| t.id.clone()).collect(),
            Some(Choices::Boards) => self.board.iter().map(|b| b.id.clone()).collect(),
            _ => vec![],
        };
        let choices: Vec<_> = self
            .choices
            .iter()
            .filter(|c| c.label.to_lowercase().contains(&query))
            .cloned()
            .collect();
        let list = if choices.is_empty() {
            div()
                .h(px(64.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .text_color(t::muted())
                .child(if self.busy {
                    "Loading available values…"
                } else if query.is_empty() {
                    "No values are available."
                } else {
                    "No values match this search."
                })
                .into_any_element()
        } else {
            uniform_list(
                SharedString::from(format!("task-choices-{label}")),
                choices.len(),
                cx.processor(move |s, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|ix| {
                            let choice = choices[ix].clone();
                            selection_button(
                                SharedString::from(format!("choice:{}", choice.id)),
                                choice.label.clone(),
                                selected.contains(&choice.id),
                            )
                            .w_full()
                            .h(px(28.))
                            .disabled(!s.ready())
                            .on_click(
                                cx.listener(move |s, _, w, cx| s.choose(choice.clone(), w, cx)),
                            )
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .h(px(140.))
            .into_any_element()
        };
        column()
            .gap(px(6.))
            .p(px(8.))
            .rounded(px(4.))
            .bg(t::hover())
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(t::secondary())
                            .child(label.to_owned()),
                    )
                    .child(
                        icon_button("close-choices", IconName::Close, "Close list").on_click(
                            cx.listener(|s, _, _, cx| {
                                s.choice_kind = None;
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(input(&self.choice_query).w_full())
            .child(list)
            .children(self.choice_next.as_ref().map(|_| {
                button("more-choices", "Load more values")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|s, _, _, cx| {
                        if let Some(kind) = s.choice_kind.clone() {
                            s.load_choices(kind, true, cx);
                        }
                    }))
            }))
    }
    /// Provider commands, ordered like the local Jira and YouTrack panels.
    fn render_provider_actions(&self, task: &p::Task, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let provider = self.local_provider();
        let mut actions = column().gap(px(6.));
        if provider != Provider::Github && self.supported("edit") {
            actions = actions.child(
                button("edit-fields", "Edit fields")
                    .w_full()
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.tab = DetailTab::Description;
                        s.action = None;
                        s.load_schema(FormMode::Fields, cx)
                    })),
            );
        }
        for (cap, id, label, action) in [
            ("links", "link-issue", "Link issue", DetailAction::Link),
            ("worklogs", "log-work", "Log work", DetailAction::LogWork),
        ] {
            if self.supported(cap) {
                actions = actions.child(
                    selection_button(id, label, self.action == Some(action))
                        .w_full()
                        .disabled(disabled)
                        .on_click(cx.listener(move |s, _, w, cx| s.toggle_action(action, w, cx))),
                );
            }
        }
        if self.supported("sprints") {
            actions = actions.child(
                row()
                    .gap(px(6.))
                    .child(
                        button("boards", "Move to sprint…")
                            .flex_1()
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.action = None;
                                s.load_choices(Choices::Boards, false, cx)
                            })),
                    )
                    .child(
                        button("clear-sprint", "Move to backlog")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.mutate(Mutation::Sprint(p::TaskOptionalChoice { id: None }), cx)
                            })),
                    ),
            );
        }
        if self.supported("delete") {
            let id = task
                .task
                .as_ref()
                .map(|t| t.task_id.clone())
                .unwrap_or_default();
            actions = actions.child(
                button("delete-task", "Delete issue…")
                    .w_full()
                    .text_color(t::red())
                    .disabled(disabled)
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.confirm(
                            format!("Delete “{id}”?"),
                            "The task is deleted from its provider. This can’t be undone.",
                            "Delete",
                            Mutation::Delete(p::TaskDelete {
                                confirmation: id.clone(),
                                subtasks: false,
                            }),
                            w,
                            cx,
                        )
                    })),
            );
        }
        let field = |id: &str| {
            task.fields
                .iter()
                .find(|f| f.id == id)
                .map(|f| fields::json(f.current_value.as_ref()))
        };
        let watching = field("watches").is_some_and(|v| v["isWatching"] == true);
        let voted = field("votes").is_some_and(|v| v["hasVoted"] == true);
        let watch = self.supported("watch").then(|| {
            button("watch", if watching { "Unwatch" } else { "Watch" })
                .flex_1()
                .disabled(disabled)
                .on_click(
                    cx.listener(move |s, _, _, cx| s.mutate(Mutation::Watching(!watching), cx)),
                )
        });
        let vote = self.supported("vote").then(|| {
            button("vote", if voted { "Unvote" } else { "Vote" })
                .flex_1()
                .disabled(disabled)
                .on_click(cx.listener(move |s, _, _, cx| s.mutate(Mutation::Voted(!voted), cx)))
        });
        if watch.is_some() || vote.is_some() {
            actions = actions.child(row().gap(px(6.)).children(watch).children(vote));
        }
        actions = actions.children(match self.action {
            Some(DetailAction::Link) => Some(self.render_link_form(cx)),
            Some(DetailAction::LogWork) => Some(self.render_worklog_form(cx)),
            None => None,
        });
        if matches!(self.choice_kind, Some(Choices::Boards)) {
            actions = actions.child(self.render_picker("Board", cx));
        }
        if matches!(self.choice_kind, Some(Choices::Sprints)) {
            actions = actions.child(self.render_picker("Sprint", cx));
        }
        actions
    }
    fn toggle_action(&mut self, action: DetailAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.action == Some(action) {
            self.action = None;
        } else {
            self.action = Some(action);
            if action == DetailAction::LogWork && value(&self.work_started, cx).is_empty() {
                self.work_started
                    .update(cx, |v, cx| v.set_value(adapter::now_local(), window, cx));
            }
        }
        if matches!(self.choice_kind, Some(Choices::Links)) {
            self.choice_kind = None;
        }
        cx.notify();
    }
    fn form_panel() -> Div {
        column()
            .gap(px(10.))
            .p(px(10.))
            .rounded(px(6.))
            .bg(t::hover())
    }
    fn render_link_form(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        Self::form_panel()
            .child(form_field(
                "Issue",
                "",
                input(&self.link_target).disabled(disabled),
            ))
            .child(form_field(
                "Direction",
                "",
                row()
                    .gap(px(2.))
                    .p(px(2.))
                    .rounded(px(6.))
                    .bg(t::hover())
                    .children(
                        [(true, "From this issue"), (false, "To this issue")]
                            .into_iter()
                            .map(|(outward, label)| {
                                selection_button(
                                    SharedString::from(format!("link-direction-{outward}")),
                                    label,
                                    self.link_outward == outward,
                                )
                                .border_0()
                                .flex_1()
                                .disabled(disabled)
                                .on_click(cx.listener(
                                    move |s, _, _, cx| {
                                        s.link_outward = outward;
                                        cx.notify();
                                    },
                                ))
                            }),
                    ),
            ))
            .children(
                matches!(self.choice_kind, Some(Choices::Links))
                    .then(|| self.render_picker("Link type", cx)),
            )
            .child(
                row()
                    .gap(px(6.))
                    .justify_end()
                    .child(button("cancel-link", "Cancel").on_click(
                        cx.listener(|s, _, w, cx| s.toggle_action(DetailAction::Link, w, cx)),
                    ))
                    .child(
                        button("link-types", "Choose link type…")
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.load_choices(Choices::Links, false, cx)
                                }),
                            ),
                    ),
            )
    }
    fn render_worklog_form(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        Self::form_panel()
            .child(form_field(
                "Time spent",
                "Hours and minutes, for example 1h 30m.",
                input(&self.work_duration).disabled(disabled),
            ))
            .child(form_field(
                "Started",
                "YYYY-MM-DD HH:MM (local time)",
                input(&self.work_started).disabled(disabled),
            ))
            .child(form_field(
                "Description",
                "Optional",
                input(&self.work_comment).disabled(disabled),
            ))
            .child(
                row()
                    .gap(px(6.))
                    .justify_end()
                    .child(button("cancel-worklog", "Cancel").on_click(
                        cx.listener(|s, _, w, cx| s.toggle_action(DetailAction::LogWork, w, cx)),
                    ))
                    .child(
                        primary_loading_button("worklog", "Log work", &self.save_loading)
                            .disabled(disabled && !self.save_loading.active())
                            .on_click(cx.listener(|s, _, _, cx| s.worklog(cx))),
                    ),
            )
    }
    fn render_attachments(&self, task: &p::Task, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let writable = self.supported("attachment.write");
        let pending = self
            .pending_upload
            .as_ref()
            .filter(|(_, target, _)| task.task.as_ref() == Some(target));
        column()
            .gap(px(6.))
            .child(
                row()
                    .gap(px(8.))
                    .child(div().flex_1().child(caption("ATTACHMENTS")))
                    .children(writable.then(|| {
                        button("attach-file", "Attach file…")
                            .icon(icon(IconName::Plus))
                            .disabled(disabled || self.pending_upload.is_some())
                            .on_click(cx.listener(|s, _, _, cx| s.pick_attachment(cx)))
                    })),
            )
            .children(task.task_attachments.iter().map(|a| {
                let preview = a.clone();
                let remove = a.clone();
                row()
                    .gap(px(4.))
                    .child(
                        button(
                            SharedString::from(format!("attachment:{}", a.id)),
                            a.name.clone(),
                        )
                        .flex_1()
                        .disabled(disabled)
                        .on_click(
                            cx.listener(move |s, _, _, cx| {
                                s.preview_attachment(preview.clone(), cx)
                            }),
                        ),
                    )
                    .children(a.can_delete.then(|| {
                        icon_button(
                            SharedString::from(format!("delete-attachment:{}", remove.id)),
                            IconName::Delete,
                            "Delete attachment",
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.confirm(
                                format!("Delete “{}”?", remove.name),
                                "The attachment is removed from this task.",
                                "Delete",
                                Mutation::Attachment(p::TaskAttachmentWrite {
                                    action: Some(p::task_attachment_write::Action::DeleteId(
                                        remove.id.clone(),
                                    )),
                                    name: remove.name.clone(),
                                }),
                                w,
                                cx,
                            )
                        }))
                    }))
            }))
            .children(
                (task.task_attachments.is_empty() && pending.is_none()).then(|| muted("None")),
            )
            .children(pending.map(|(_, _, name)| {
                column()
                    .gap(px(6.))
                    .child(integration_message(
                        if self.upload_attempted {
                            format!(
                                "“{name}” may already be attached. Refresh before uploading again."
                            )
                        } else {
                            format!("“{name}” is ready to upload.")
                        },
                        false,
                    ))
                    .child(
                        row()
                            .gap(px(6.))
                            .justify_end()
                            .child(
                                button("discard-upload", "Remove")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|s, _, _, cx| s.discard_upload(cx))),
                            )
                            .child(
                                loading_button(
                                    "resume-upload",
                                    if self.upload_attempted {
                                        "Upload again…"
                                    } else {
                                        "Upload"
                                    },
                                    &self.save_loading,
                                )
                                .disabled(disabled && !self.save_loading.active())
                                .on_click(cx.listener(|s, _, w, cx| s.confirm_upload(w, cx))),
                            ),
                    )
            }))
    }
    fn render_jira_activity(&self, task: &p::Task, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let mut body = column().gap(px(20.));
        let mut notes = vec![];
        let mut links = vec![];
        let mut worklogs = vec![];
        for field in &task.fields {
            let v = fields::json(field.current_value.as_ref());
            match field.id.as_str() {
                "issuelinks" => {
                    for link in v.as_array().into_iter().flatten() {
                        let Some(id) = link["id"].as_str() else {
                            continue;
                        };
                        let id = id.to_owned();
                        let outward = link.get("outwardIssue").is_some();
                        let issue = if outward {
                            &link["outwardIssue"]
                        } else {
                            &link["inwardIssue"]
                        };
                        let key = issue["key"].as_str().unwrap_or_default().to_owned();
                        let label = format!(
                            "{} {}",
                            link["type"][if outward { "outward" } else { "inward" }]
                                .as_str()
                                .unwrap_or("Related"),
                            key
                        );
                        let mut reference = task.task.clone();
                        if let Some(t) = &mut reference {
                            t.task_id = key.clone();
                        }
                        links.push(
                            row()
                                .gap(px(4.))
                                .child(
                                    button(SharedString::from(format!("linked:{id}")), label)
                                        .flex_1()
                                        .disabled(disabled)
                                        .on_click(cx.listener(move |s, _, _, cx| {
                                            if let Some(t) = &reference {
                                                s.open_task(t.clone(), cx);
                                            }
                                        })),
                                )
                                .child(
                                    icon_button(
                                        SharedString::from(format!("unlink:{id}")),
                                        IconName::Close,
                                        "Remove relationship",
                                    )
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |s, _, w, cx| {
                                        s.confirm(
                                            format!("Remove the link to {key}?"),
                                            "Both issues remain; only this relationship is removed.",
                                            "Remove",
                                            Mutation::Link(p::TaskLinkWrite {
                                                delete_link_id: Some(id.clone()),
                                                ..Default::default()
                                            }),
                                            w,
                                            cx,
                                        )
                                    })),
                                ),
                        );
                    }
                }
                "worklog" => {
                    for log in v["worklogs"].as_array().into_iter().flatten() {
                        let mut parts = vec![];
                        if let Some(author) = log["author"]["displayName"].as_str() {
                            parts.push(author.to_owned());
                        }
                        if let Some(started) = log["started"].as_str() {
                            parts.push(adapter::format_time(started));
                        }
                        if let Some(seconds) = log["timeSpentSeconds"].as_u64() {
                            parts.push(adapter::format_duration(seconds));
                        }
                        worklogs.push(div().text_size(px(12.)).child(parts.join(" · ")));
                    }
                }
                "canopy.jira.metadata" => {
                    if v["description_editable"] == false {
                        notes.push(
                            "This description contains rich Jira content. Use Edit fields to keep its formatting.",
                        );
                    }
                    if v["complete"] == false {
                        notes.push(
                            "Some Jira details couldn’t be loaded. Refresh the task to retry.",
                        );
                    }
                }
                _ => {}
            }
        }
        if !links.is_empty() {
            body = body.child(meta_field(
                "LINKED ISSUES",
                column().gap(px(4.)).children(links),
            ));
        }
        if !worklogs.is_empty() {
            body = body.child(meta_field(
                "WORK LOG",
                column().gap(px(4.)).children(worklogs),
            ));
        }
        body = body.children(notes.into_iter().map(|n| integration_message(n, false)));
        body.child(meta_field(
            "HISTORY",
            column()
                .gap(px(6.))
                .children(self.activity.iter().map(|entry| muted(entry.clone())))
                .child(if self.activity.is_empty() {
                    button("activity", "Show history")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, _, cx| s.history(false, cx)))
                } else {
                    button("more-history", "Load more history")
                        .disabled(disabled || self.activity_next.is_none())
                        .on_click(cx.listener(|s, _, _, cx| s.history(true, cx)))
                }),
        ))
    }
    pub(super) fn render_comments(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let editing = self.comment_id.clone();
        let can_save = editing
            .as_ref()
            .is_none_or(|id| self.comments.iter().any(|c| &c.id == id && c.can_edit));
        let list = if self.loading == Loading::Comments && self.comments.is_empty() {
            view::comments_loading().pt(px(16.)).into_any_element()
        } else if self.comments_loaded && self.comments.is_empty() {
            view::comments_empty().py(px(24.)).into_any_element()
        } else {
            column()
                .children(self.comments.iter().map(|c| {
                    let edit = c.clone();
                    let remove = c.clone();
                    let url = c.url.clone();
                    let actions = row()
                        .gap(px(6.))
                        .children(c.can_edit.then(|| {
                            icon_button(
                                SharedString::from(format!("edit-comment-{}", c.id)),
                                IconName::Replace,
                                "Edit comment",
                            )
                            .disabled(disabled)
                            .on_click(
                                cx.listener(move |s, _, w, cx| s.edit_comment(edit.clone(), w, cx)),
                            )
                        }))
                        .children(c.can_delete.then(|| {
                            icon_button(
                                SharedString::from(format!("delete-comment-{}", c.id)),
                                IconName::Delete,
                                "Delete comment",
                            )
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    s.confirm(
                                        "Delete this comment?".into(),
                                        "The comment is removed from the task for everyone.",
                                        "Delete",
                                        Mutation::Comment(p::TaskCommentWrite {
                                            comment_id: Some(remove.id.clone()),
                                            delete: true,
                                            ..Default::default()
                                        }),
                                        w,
                                        cx,
                                    )
                                },
                            ))
                        }))
                        .children((!url.is_empty()).then(|| {
                            icon_button(
                                SharedString::from(format!("open-comment-{}", c.id)),
                                IconName::ExternalLink,
                                "Open comment in browser",
                            )
                            .on_click(move |_, _, cx| cx.open_url(&url))
                        }));
                    let body = self
                        .comment_views
                        .get(&c.id)
                        .map(|v| v.clone().into_any_element())
                        .unwrap_or_else(|| {
                            div()
                                .text_size(px(12.))
                                .child(c.body.clone())
                                .into_any_element()
                        });
                    view::comment_card(&adapter::comment(c), body, actions)
                }))
                .into_any_element()
        };
        let composer = self
            .comment_rich
            .as_ref()
            .map(|f| f.render())
            .unwrap_or_else(|| self.comment.clone().into_any_element());
        column()
            .gap(px(12.))
            .pb(px(16.))
            .child(view::comments_toolbar(
                self.comments.len(),
                None,
                icon_button("refresh-comments", IconName::RotateCw, "Refresh comments")
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| s.load_comments(false, cx))),
            ))
            .child(list)
            .children(self.comments_next.as_ref().map(|_| {
                button("more-comments", "Load more comments")
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| s.load_comments(true, cx)))
            }))
            .child(form_field(
                if editing.is_some() {
                    "Edit comment"
                } else {
                    "New comment"
                },
                "",
                composer,
            ))
            .child(
                row()
                    .gap(px(8.))
                    .justify_end()
                    .children(editing.is_some().then(|| {
                        button("new-comment", "Cancel")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.comment_id = None;
                                s.comment_rich = None;
                                s.comment.update(cx, |v, cx| v.set_value("", w, cx));
                                cx.notify();
                            }))
                    }))
                    .child(
                        primary_loading_button(
                            "save-comment",
                            if editing.is_some() {
                                "Save comment"
                            } else {
                                "Comment"
                            },
                            &self.save_loading,
                        )
                        .disabled((disabled || !can_save) && !self.save_loading.active())
                        .on_click(cx.listener(|s, _, _, cx| s.save_comment(cx))),
                    ),
            )
    }
}
