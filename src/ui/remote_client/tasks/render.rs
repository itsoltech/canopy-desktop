use super::*;
impl Render for TasksView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.ready();
        let mut body = column()
            .gap_3()
            .child(
                row().gap_2().child(div().flex_1().child("Tasks")).child(
                    button("accounts", "Refresh integrations")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, _, cx| s.load_integrations(cx))),
                ),
            )
            .children(self.busy.then(|| div().child("Loading…")))
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_color(t::red()).child(e)),
            )
            .children(
                (!self.notice.is_empty())
                    .then(|| div().text_color(t::muted()).child(self.notice.clone())),
            );
        let mut accounts = row().gap_2().flex_wrap();
        for i in &self.integrations {
            let i = i.clone();
            accounts = accounts.child(
                selection_button(
                    SharedString::from(format!("account:{}", i.id)),
                    i.name.clone(),
                    self.integration.as_ref().is_some_and(|v| v.id == i.id),
                )
                .disabled(disabled || !i.configured)
                .on_click(cx.listener(move |s, _, _, cx| s.choose_integration(i.clone(), cx))),
            );
        }
        body = body.child(accounts);
        if self.integrations.is_empty() {
            body = body.child("Connect GitHub, Jira or YouTrack in Host settings to browse tasks.");
        }
        if self.integration.is_some() {
            body = body.child(
                button(
                    "projects",
                    self.project
                        .as_ref()
                        .map(|p| format!("Project: {}…", p.label))
                        .unwrap_or_else(|| "Choose project…".into()),
                )
                .disabled(disabled)
                .on_click(cx.listener(|s, _, _, cx| s.load_choices(Choices::Projects, false, cx))),
            );
        }
        if self.choice_kind.is_some() {
            body = body.child(self.render_choices(cx));
        }
        if self.project.is_some() {
            body = body
                .child(labeled("Search tasks", &self.query, disabled))
                .child(
                    RadioGroup::new("task-state")
                        .children(["Open", "Closed"])
                        .selected_index(Some(self.state))
                        .disabled(disabled)
                        .on_change(cx.listener(|s, ix, _, cx| {
                            s.state = *ix;
                            s.search(false, cx);
                        })),
                )
                .child(
                    row()
                        .gap_2()
                        .child(
                            button("search", "Search")
                                .disabled(disabled)
                                .on_click(cx.listener(|s, _, _, cx| s.search(false, cx))),
                        )
                        .child(
                            button("create", "Create task…")
                                .disabled(disabled)
                                .on_click(cx.listener(|s, _, w, cx| s.new_task(w, cx))),
                        ),
                );
            let filters_disabled = disabled || self.provider() == "github";
            let mut filters = row().gap_2().flex_wrap().child(
                selection_button("all-filters", "No saved filter", self.filter.is_none())
                    .disabled(filters_disabled)
                    .on_click(cx.listener(|s, _, w, cx| {
                        s.filter = None;
                        s.filter_expression
                            .update(cx, |v, cx| v.set_value("", w, cx));
                        s.search(false, cx);
                    })),
            );
            for f in &self.filters {
                let f = f.clone();
                filters = filters.child(
                    selection_button(
                        SharedString::from(format!("filter:{}", f.id)),
                        f.name.clone(),
                        self.filter.as_ref().is_some_and(|v| v.id == f.id),
                    )
                    .disabled(filters_disabled)
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.filter = Some(f.clone());
                        s.filter_name
                            .update(cx, |v, cx| v.set_value(f.name.clone(), w, cx));
                        s.filter_expression
                            .update(cx, |v, cx| v.set_value(f.expression.clone(), w, cx));
                        s.search(false, cx);
                    })),
                );
            }
            body = body
                .child(filters)
                .child(labeled(
                    "Filter expression",
                    &self.filter_expression,
                    filters_disabled,
                ))
                .child(labeled(
                    "Saved filter name",
                    &self.filter_name,
                    filters_disabled,
                ))
                .child(
                    row()
                        .gap_2()
                        .child(
                            button("save-filter", "Save filter")
                                .disabled(filters_disabled)
                                .on_click(cx.listener(|s, _, _, cx| s.save_filter(cx))),
                        )
                        .child(
                            button("remove-filter", "Remove saved filter")
                                .disabled(
                                    filters_disabled
                                        || self.filter.as_ref().is_none_or(|f| f.builtin),
                                )
                                .on_click(cx.listener(|s, _, _, cx| s.remove_filter(cx))),
                        ),
                );
            if self.provider() == "github" {
                body = body.child("Saved provider filters are unavailable for GitHub. Use search and Open or Closed.");
            }
            if self.tasks.is_empty() && !self.busy {
                body = body.child("No tasks loaded. Search this project or change the filter.");
            }
            body = body
                .child(
                    uniform_list(
                        "task-results",
                        self.tasks.len(),
                        cx.processor(|s, range: std::ops::Range<usize>, _, cx| {
                            range
                                .filter_map(|ix| {
                                    let task = &s.tasks[ix];
                                    let reference = task.task.clone()?;
                                    Some(
                                        selection_button(
                                            SharedString::from(format!(
                                                "task:{}:{}",
                                                reference.integration_id, reference.task_id
                                            )),
                                            format!(
                                                "{} · {} · {}",
                                                reference.task_id, task.title, task.state
                                            ),
                                            s.task.as_ref().and_then(|t| t.task.as_ref())
                                                == Some(&reference),
                                        )
                                        .w_full()
                                        .h_8()
                                        .disabled(!s.ready())
                                        .on_click(
                                            cx.listener(move |s, _, _, cx| {
                                                s.open_task(reference.clone(), cx)
                                            }),
                                        ),
                                    )
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .h_48(),
                )
                .children(self.next.as_ref().map(|_| {
                    button("more-tasks", "Load more tasks")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, _, cx| s.search(true, cx)))
                }));
        }
        let detail =
            (self.task.is_some() || self.mode == FormMode::Create).then(|| self.render_detail(cx));
        column()
            .gap_3()
            .child(
                row()
                    .items_start()
                    .flex_wrap()
                    .gap_4()
                    .child(body.flex_1().min_w_64())
                    .children(detail.map(|d| d.flex_1().min_w_64())),
            )
            .child(self.render_pending_upload(cx))
    }
}
impl TasksView {
    fn render_choices(&self, cx: &mut Context<Self>) -> Div {
        let query = value(&self.choice_query, cx).to_lowercase();
        let choices: Vec<_> = self
            .choices
            .iter()
            .filter(|c| {
                c.label.to_lowercase().contains(&query) || c.id.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        column()
            .gap_2()
            .child(labeled("Find a value", &self.choice_query, false))
            .child(
                uniform_list(
                    "task-choices",
                    choices.len(),
                    cx.processor(move |s, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|ix| {
                                let choice = choices[ix].clone();
                                button(
                                    SharedString::from(format!("choice:{}", choice.id)),
                                    choice.label.clone(),
                                )
                                .w_full()
                                .h_8()
                                .disabled(!s.ready())
                                .on_click(
                                    cx.listener(move |s, _, w, cx| s.choose(choice.clone(), w, cx)),
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .h_40(),
            )
            .children(self.choice_next.as_ref().map(|_| {
                button("more-choices", "Load more values")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|s, _, _, cx| {
                        if let Some(kind) = s.choice_kind.clone() {
                            s.load_choices(kind, true, cx);
                        }
                    }))
            }))
            .child(
                button("close-choices", "Close choices").on_click(cx.listener(|s, _, _, cx| {
                    s.choice_kind = None;
                    cx.notify();
                })),
            )
    }
    fn render_detail(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let creating = self.mode == FormMode::Create;
        let mut body = column()
            .gap_3()
            .border_t_1()
            .border_color(t::border())
            .pt_3()
            .child(if creating {
                "Create task".to_owned()
            } else {
                self.task
                    .as_ref()
                    .map(|t| t.title.clone())
                    .unwrap_or_default()
            });
        if let Some(task) = &self.task {
            body = body.child(format!(
                "{} · {}",
                task.task.as_ref().map(|t| t.task_id.as_str()).unwrap_or(""),
                task.state
            ));
            if !task.url.is_empty() {
                let url = task.url.clone();
                body = body.child(
                    button("open-task-url", "Open in browser")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            body = body.child(
                button("refresh-task", "Refresh task")
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| {
                        if let Ok(reference) = s.task_ref() {
                            s.open_task(reference, cx);
                        }
                    })),
            );
            for capability in &task.capabilities {
                if !capability.supported {
                    body = body.child(
                        div()
                            .text_color(t::muted())
                            .child(format!("{}: {}", capability.operation, capability.reason)),
                    );
                }
            }
        }
        if self.mode == FormMode::Detail || creating {
            body = body
                .child(labeled("Title", &self.title, disabled))
                .child(form_field("Description", "Markdown", self.body.clone()));
            if creating {
                for field in &self.fields {
                    body = body.child(field.render());
                }
            }
            body = body.child(
                row()
                    .gap_2()
                    .child(
                        button(
                            "save-task",
                            if creating {
                                "Create task"
                            } else {
                                "Save title and description"
                            },
                        )
                        .disabled(
                            disabled
                                || (creating
                                    && self.provider() == "jira"
                                    && self.issue_type.is_none()),
                        )
                        .on_click(cx.listener(|s, _, _, cx| s.save_task(cx))),
                    )
                    .child(
                        button("save-draft", "Save draft on this device")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| s.save_draft(cx))),
                    ),
            );
        }
        if creating {
            if self.provider() == "github" {
                body = body.child(format!(
                    "Labels: {} · Assignees: {} · Milestone: {}",
                    self.create_labels.join(", "),
                    self.create_assignees.join(", "),
                    self.create_milestone.as_deref().unwrap_or("None")
                ));
                for (kind, label) in [
                    ("label", "Choose labels…"),
                    ("assignee", "Choose assignees…"),
                    ("milestone", "Choose milestone…"),
                ] {
                    body =
                        body.child(button(kind, label).disabled(disabled).on_click(cx.listener(
                            move |s, _, _, cx| s.load_choices(Choices::Metadata(kind), false, cx),
                        )));
                }
                body = body.child(
                    button("clear-create-milestone", "Clear milestone")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.create_milestone = None;
                            cx.notify();
                        })),
                );
            }
            return body;
        }
        let Some(task) = &self.task else { return body };
        if self.provider() != "github" {
            body = body.child(
                button("edit-fields", "Edit provider fields…")
                    .disabled(disabled || !self.supported("edit"))
                    .on_click(cx.listener(|s, _, _, cx| s.load_schema(FormMode::Fields, cx))),
            );
        }
        for status in &task.statuses {
            body = body.child(format!("{}: {}", status.name, status.label));
            if let Some(reason) = &status.read_only_reason {
                body = body.child(reason.clone());
                continue;
            }
            if status.requires_form {
                body = body.child(
                    button(
                        SharedString::from(format!("transitions:{}", status.id)),
                        "Choose transition…",
                    )
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| s.load_schema(FormMode::Transition, cx))),
                );
            } else {
                let mut choices = row().gap_2().flex_wrap();
                for choice in &status.choices {
                    let id = status.id.clone();
                    let choice = choice.clone();
                    choices = choices.child(
                        selection_button(
                            SharedString::from(format!("status:{}:{}", id, choice.id)),
                            choice.label.clone(),
                            status.selected_id.as_ref() == Some(&choice.id),
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.mutate(
                                Mutation::Status(p::TaskStatusChoice {
                                    field_id: id.clone(),
                                    choice_id: choice.id.clone(),
                                    ..Default::default()
                                }),
                                cx,
                            )
                        })),
                    );
                }
                body = body.child(choices);
            }
        }
        if matches!(self.mode, FormMode::Fields | FormMode::Transition) {
            for t in &self.schema.transitions {
                let t = t.clone();
                body = body.child(
                    selection_button(
                        SharedString::from(format!("transition:{}", t.id)),
                        format!("{} → {}", t.name, t.destination),
                        self.transition
                            .as_ref()
                            .is_some_and(|v| v.choice_id == t.id),
                    )
                    .disabled(disabled)
                    .on_click(
                        cx.listener(move |s, _, w, cx| s.choose_transition(t.clone(), w, cx)),
                    ),
                );
            }
            for field in &self.fields {
                body = body.child(field.render());
            }
            body = body.child(
                button(
                    "save-fields",
                    if self.mode == FormMode::Transition {
                        "Apply transition"
                    } else {
                        "Save fields"
                    },
                )
                .disabled(disabled)
                .on_click(cx.listener(|s, _, _, cx| s.save_fields(cx))),
            );
        }
        body = body
            .child(format!("Labels: {}", task.labels.join(", ")))
            .child(format!("Assignees: {}", task.assignees.join(", ")))
            .child(format!(
                "Milestone: {}",
                task.milestone
                    .as_ref()
                    .map(|m| m.label.as_str())
                    .unwrap_or("None")
            ));
        let mut metadata = row().gap_2().flex_wrap();
        for (kind, label, capability) in [
            ("label", "Choose labels…", "labels"),
            ("assignee", "Choose assignees…", "assignees"),
            ("milestone", "Choose milestone…", "milestone"),
        ] {
            metadata = metadata.child(
                button(kind, label)
                    .disabled(disabled || !self.supported(capability))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.load_choices(Choices::Metadata(kind), false, cx)
                    })),
            );
        }
        body = body
            .child(metadata)
            .children(self.supported("milestone").then(|| {
                button("clear-milestone", "Remove milestone")
                    .disabled(disabled || task.milestone.is_none())
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.mutate(Mutation::Milestone(p::TaskOptionalChoice { id: None }), cx)
                    }))
            }));
        body=body.child("Comments").child(row().gap_2().child(button("comments","Load comments").disabled(disabled).on_click(cx.listener(|s,_,_,cx|s.load_comments(false,cx)))).child(button("new-comment","New comment").disabled(disabled).on_click(cx.listener(|s,_,w,cx|{s.comment_id=None;s.comment_rich=None;s.comment.update(cx,|v,cx|v.set_value("",w,cx));cx.notify();}))))
            .child(uniform_list("task-comments",self.comments.len(),cx.processor(|s,range:std::ops::Range<usize>,_,cx|range.map(|ix|{let c=s.comments[ix].clone();let remove=c.clone();row().gap_2().h_8().child(button(SharedString::from(format!("comment:{}",c.id)),format!("{} · {} · {}",c.author,c.created_at,c.body.lines().next().unwrap_or(""))).disabled(!s.ready()).on_click(cx.listener(move|s,_,w,cx|s.edit_comment(c.clone(),w,cx))))
                .child(button(SharedString::from(format!("delete-comment:{}",remove.id)),"Delete…").disabled(!s.ready()||!remove.can_delete).on_click(cx.listener(move|s,_,w,cx|s.confirm("Delete comment?".into(),"This removes the selected comment from the host provider.",Mutation::Comment(p::TaskCommentWrite{comment_id:Some(remove.id.clone()),delete:true,..Default::default()}),w,cx))))}).collect::<Vec<_>>())).h_40())
            .children(self.comments_next.as_ref().map(|_|button("more-comments","Load more comments").disabled(disabled).on_click(cx.listener(|s,_,_,cx|s.load_comments(true,cx)))))
            .child(form_field(if self.comment_id.is_some(){"Selected comment"}else{"New comment"},"",self.comment_rich.as_ref().map(|f|f.render()).unwrap_or_else(||self.comment.clone().into_any_element())))
            .child(button("save-comment",if self.comment_id.is_some(){"Save comment"}else{"Add comment"}).disabled(disabled||self.comment_id.as_ref().is_some_and(|id|!self.comments.iter().any(|c|&c.id==id&&c.can_edit))).on_click(cx.listener(|s,_,_,cx|s.save_comment(cx))));
        body = body.child(self.render_jira_metadata(cx));
        body = body.child("Attachments");
        for a in &task.task_attachments {
            let a = a.clone();
            let remove = a.clone();
            body = body.child(
                row()
                    .gap_2()
                    .child(
                        button(
                            SharedString::from(format!("attachment:{}", a.id)),
                            format!("{} · {} bytes", a.name, a.size),
                        )
                        .disabled(disabled)
                        .on_click(
                            cx.listener(move |s, _, _, cx| s.preview_attachment(a.clone(), cx)),
                        ),
                    )
                    .child(
                        button(
                            SharedString::from(format!("delete-attachment:{}", remove.id)),
                            "Delete…",
                        )
                        .disabled(disabled || !remove.can_delete)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.confirm(
                                format!("Delete attachment “{}”?", remove.name),
                                "This removes the attachment from this task.",
                                Mutation::Attachment(p::TaskAttachmentWrite {
                                    action: Some(p::task_attachment_write::Action::DeleteId(
                                        remove.id.clone(),
                                    )),
                                    name: remove.name.clone(),
                                }),
                                w,
                                cx,
                            )
                        })),
                    ),
            );
        }
        if self.supported("attachment.write") {
            body = body
                .child(labeled(
                    "Local attachment path",
                    &self.attachment_path,
                    disabled,
                ))
                .child(
                    button("prepare-attachment", "Prepare attachment")
                        .disabled(disabled || self.pending_upload.is_some())
                        .on_click(cx.listener(|s, _, _, cx| s.prepare_attachment(cx))),
                );
        }
        if self.supported("links") {
            body = body
                .child(labeled("Linked issue key", &self.link_target, disabled))
                .child(
                    RadioGroup::new("link-direction")
                        .children(["Outward", "Inward"])
                        .selected_index(Some(usize::from(!self.link_outward)))
                        .disabled(disabled)
                        .on_change(cx.listener(|s, ix, _, cx| {
                            s.link_outward = *ix == 0;
                            cx.notify();
                        })),
                )
                .child(
                    button("link-types", "Choose link type…")
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|s, _, _, cx| s.load_choices(Choices::Links, false, cx)),
                        ),
                );
        }
        if self.supported("worklogs") {
            body = body
                .child(labeled("Duration (seconds)", &self.work_seconds, disabled))
                .child(labeled(
                    "Started (ISO date and time with offset)",
                    &self.work_started,
                    disabled,
                ))
                .child(
                    button("worklog", "Log work using comment above")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, _, cx| s.worklog(cx))),
                );
        }
        if self.supported("sprints") {
            body = body.child(
                row()
                    .gap_2()
                    .child(
                        button("boards", "Choose board and sprint…")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| {
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
        for (cap, label, enable) in [
            ("watch", "Watch", true),
            ("watch", "Stop watching", false),
            ("vote", "Vote", true),
            ("vote", "Remove vote", false),
        ] {
            if self.supported(cap) {
                body = body.child(
                    button(SharedString::from(format!("{cap}:{enable}")), label)
                        .disabled(disabled)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.mutate(
                                if cap == "watch" {
                                    Mutation::Watching(enable)
                                } else {
                                    Mutation::Voted(enable)
                                },
                                cx,
                            )
                        })),
                );
            }
        }
        body = body
            .child(labeled("Worktree branch", &self.branch, disabled))
            .child(
                button("worktree", "Create worktree from task")
                    .disabled(disabled || task.task.as_ref().is_none_or(|t| t.workspace.is_none()))
                    .on_click(cx.listener(|s, _, _, cx| s.task_worktree(cx))),
            );
        if let Some(id) = &self.returned_workspace {
            let workspace = p::WorkspaceRef {
                host_id: self.host.id().as_str().into(),
                workspace_id: id.clone(),
            };
            body = body.child(
                button("open-created-worktree", "Open created worktree")
                    .disabled(disabled)
                    .on_click(
                        cx.listener(move |_, _, _, cx| cx.emit(OpenWorkspace(workspace.clone()))),
                    ),
            );
        }
        if self.supported("delete") {
            let id = task
                .task
                .as_ref()
                .map(|t| t.task_id.clone())
                .unwrap_or_default();
            body=body.child(button("delete-task","Delete task…").disabled(disabled).on_click(cx.listener(move|s,_,w,cx|s.confirm(format!("Delete task {id}?"),"This deletes the task from its provider. This action cannot be undone here.",Mutation::Delete(p::TaskDelete{confirmation:id.clone(),subtasks:false}),w,cx))));
        }
        body
    }
}
impl TasksView {
    fn render_jira_metadata(&self, cx: &mut Context<Self>) -> Div {
        let mut body = column().gap_2();
        if self.provider() != "jira" {
            return body;
        }
        let Some(task) = &self.task else { return body };
        let disabled = !self.ready();
        for field in &task.fields {
            let v = fields::json(field.current_value.as_ref());
            match field.id.as_str() {
                "issuelinks" => {
                    body = body.child("Linked issues");
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
                            t.task_id = key;
                        }
                        body=body.child(row().gap_2().child(button(SharedString::from(format!("linked:{id}")),label).disabled(disabled).on_click(cx.listener(move|s,_,_,cx|{if let Some(t)=&reference{s.open_task(t.clone(),cx);}})))
                        .child(button(SharedString::from(format!("unlink:{id}")),"Remove link…").disabled(disabled).on_click(cx.listener(move|s,_,w,cx|s.confirm("Remove issue link?".into(),"Both issues remain. Only this relationship is removed.",Mutation::Link(p::TaskLinkWrite{delete_link_id:Some(id.clone()),..Default::default()}),w,cx)))));
                    }
                }
                "worklog" => {
                    body = body.child("Work logs");
                    for log in v["worklogs"].as_array().into_iter().flatten() {
                        body = body.child(format!(
                            "{} · {} · {} seconds",
                            log["author"]["displayName"].as_str().unwrap_or(""),
                            log["started"].as_str().unwrap_or(""),
                            log["timeSpentSeconds"]
                        ));
                    }
                }
                "watches" => {
                    body = body.child(format!(
                        "Watching: {} · {} watchers",
                        v["isWatching"].as_bool().unwrap_or(false),
                        v["watchCount"]
                    ));
                }
                "votes" => {
                    body = body.child(format!(
                        "Voted: {} · {} votes",
                        v["hasVoted"].as_bool().unwrap_or(false),
                        v["votes"]
                    ));
                }
                "canopy.jira.metadata" => {
                    if v["description_editable"] == false {
                        body=body.child("This description contains rich Jira content. Use Edit provider fields to preserve its document structure.");
                    }
                    if v["complete"] == false {
                        body=body.child("Jira metadata is incomplete; refresh before relying on missing details.");
                    }
                }
                _ if field.name.to_lowercase().contains("sprint") => {
                    let values = v.as_array().cloned().unwrap_or_else(|| vec![v]);
                    for v in values {
                        body = body.child(format!(
                            "{}: {}",
                            field.name,
                            v["name"].as_str().or(v.as_str()).unwrap_or("None")
                        ));
                    }
                }
                _ => {}
            }
        }
        body = body.child(
            button("activity", "Load history")
                .disabled(disabled)
                .on_click(cx.listener(|s, _, _, cx| s.history(false, cx))),
        );
        body = body.child(
            uniform_list(
                "task-activity",
                self.activity.len(),
                cx.processor(|s, range: std::ops::Range<usize>, _, _| {
                    range
                        .map(|ix| div().h_8().child(s.activity[ix].clone()))
                        .collect::<Vec<_>>()
                }),
            )
            .h_32(),
        );
        body.children(self.activity_next.as_ref().map(|_| {
            button("more-history", "Load more history")
                .disabled(disabled)
                .on_click(cx.listener(|s, _, _, cx| s.history(true, cx)))
        }))
    }
}
impl TasksView {
    fn render_pending_upload(&self, cx: &mut Context<Self>) -> Div {
        let mut body = column().gap_2();
        if let Some((_, task, name)) = &self.pending_upload {
            let target = task.clone();
            body = body
                .child(format!("Prepared attachment: {name} → {}", task.task_id))
                .child(
                    row()
                        .gap_2()
                        .child(
                            button("open-upload-task", "Open original task")
                                .disabled(!self.ready())
                                .on_click(
                                    cx.listener(move |s, _, _, cx| s.open_task(target.clone(), cx)),
                                ),
                        )
                        .child(
                            button(
                                "resume-upload",
                                if self.upload_attempted {
                                    "Review attachment retry…"
                                } else {
                                    "Upload and attach"
                                },
                            )
                            .disabled(!self.ready())
                            .on_click(cx.listener(|s, _, w, cx| s.confirm_upload(w, cx))),
                        )
                        .child(
                            button("discard-upload", "Remove local attachment")
                                .disabled(!self.ready())
                                .on_click(cx.listener(|s, _, _, cx| s.discard_upload(cx))),
                        ),
                );
        }
        body
    }
}
