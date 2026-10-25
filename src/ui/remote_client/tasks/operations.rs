use super::*;
impl TasksView {
    pub(super) fn load_integrations(&mut self, cx: &mut Context<Self>) {
        if !self.ready() {
            return;
        }
        self.discovery_pending = false;
        let host = self.host.clone();
        let workspace = self.workspace.clone();
        self.run(
            async move {
                Ok(ResultView::Integrations(
                    Client::new(host.channel()?)
                        .list_integrations(p::TaskIntegrationsRequest {
                            host_id: host.id().as_str().into(),
                            workspace: workspace.clone(),
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    workspace,
                ))
            },
            cx,
        );
    }
    pub(super) fn choose_integration(
        &mut self,
        integration: p::TaskIntegration,
        cx: &mut Context<Self>,
    ) {
        self.integration = Some(integration);
        self.project = None;
        self.task = None;
        self.tasks.clear();
        self.filter = None;
        self.fields.clear();
        self.mode = FormMode::Detail;
        self.load_choices(Choices::Projects, false, cx);
    }
    pub(super) fn load_choices(&mut self, kind: Choices, more: bool, cx: &mut Context<Self>) {
        let Some(integration) = self.integration.clone() else {
            return;
        };
        let host = self.host.clone();
        let workspace = self.workspace.clone();
        let project = self.active_project().ok();
        let cursor = more.then(|| self.choice_next.clone()).flatten();
        let board = self.board.as_ref().map(|b| b.id.clone());
        self.run(
            async move {
                let mut client = Client::new(host.channel()?);
                let page = match &kind {
                    Choices::Projects => client
                        .list_projects(p::TaskProjectsRequest {
                            host_id: host.id().as_str().into(),
                            integration_id: integration.id,
                            workspace,
                            page_token: cursor,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    Choices::Metadata(kind) => client
                        .read_options(p::TaskOptionsRequest {
                            project,
                            kind: (*kind).into(),
                            page_token: cursor,
                            board_id: None,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    _ => {
                        let purpose = match kind {
                            Choices::Types => p::TaskSchemaPurpose::TaskTypes,
                            Choices::Boards => p::TaskSchemaPurpose::TaskBoards,
                            Choices::Sprints => p::TaskSchemaPurpose::TaskSprints,
                            _ => p::TaskSchemaPurpose::TaskLinkTypes,
                        };
                        let schema = client
                            .get_schema(p::TaskSchemaRequest {
                                project,
                                purpose: purpose as i32,
                                board_id: board,
                                page_token: cursor,
                                ..Default::default()
                            })
                            .await
                            .map_err(rpc_error)?
                            .into_inner();
                        p::TaskChoicePage {
                            choices: schema.choices,
                            next_page_token: schema.next_page_token,
                        }
                    }
                };
                Ok(ResultView::Choices(kind, page, more))
            },
            cx,
        );
    }
    pub(super) fn choose(
        &mut self,
        choice: p::TaskChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self.choice_kind.clone() else {
            return;
        };
        self.choice_kind = None;
        match kind {
            Choices::Projects => {
                self.project = Some(choice);
                self.task = None;
                self.tasks.clear();
                self.issue_type = None;
                self.board = None;
                self.search(false, cx);
            }
            Choices::Types => {
                self.issue_type = Some(choice);
                self.load_schema(FormMode::Create, cx);
            }
            Choices::Boards => {
                self.board = Some(choice);
                self.load_choices(Choices::Sprints, false, cx);
            }
            Choices::Sprints => self.mutate(
                Mutation::Sprint(p::TaskOptionalChoice {
                    id: Some(choice.id),
                }),
                cx,
            ),
            Choices::Links => {
                let other = value(&self.link_target, cx);
                if other.trim().is_empty() {
                    self.fail(
                        "Enter the linked issue key before choosing a link type.",
                        cx,
                    );
                    return;
                }
                self.mutate(
                    Mutation::Link(p::TaskLinkWrite {
                        kind: choice.id,
                        other_task: other,
                        outward: self.link_outward,
                        delete_link_id: None,
                    }),
                    cx,
                );
            }
            Choices::Metadata(kind) => {
                if self.mode == FormMode::Create {
                    match kind {
                        "label" => toggle(&mut self.create_labels, choice.id),
                        "assignee" => toggle(&mut self.create_assignees, choice.id),
                        _ => self.create_milestone = Some(choice.id),
                    }
                    self.persist_draft(cx);
                    cx.notify();
                    return;
                }
                let task = self.task.as_ref();
                let action = match kind {
                    "label" => Mutation::Tag(p::TaskTagWrite {
                        id: choice.id.clone(),
                        add: !task.is_some_and(|t| t.labels.contains(&choice.id)),
                    }),
                    "assignee" => Mutation::Assignee(p::TaskAssigneeWrite {
                        id: choice.id.clone(),
                        add: !task.is_some_and(|t| t.assignees.contains(&choice.id)),
                    }),
                    _ => Mutation::Milestone(p::TaskOptionalChoice {
                        id: Some(choice.id),
                    }),
                };
                self.mutate(action, cx);
            }
        }
        let _ = window;
    }
    pub(super) fn search(&mut self, more: bool, cx: &mut Context<Self>) {
        let project = match self.project_ref() {
            Ok(p) => p,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let request = p::QueryTasksRequest {
            host_id: project.host_id,
            integration_id: project.integration_id,
            project_id: project.project_id,
            workspace: project.workspace,
            query: value(&self.query, cx),
            page_token: more.then(|| self.next.clone()).flatten(),
            filter_id: self.filter.as_ref().map(|f| f.id.clone()),
            filter_expression: nonempty(value(&self.filter_expression, cx)),
            state: ["open", "closed"][self.state].into(),
        };
        let host = self.host.clone();
        self.run(
            async move {
                Ok(ResultView::Tasks(
                    Client::new(host.channel()?)
                        .query_tasks(request)
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    more,
                ))
            },
            cx,
        );
    }
    pub(super) fn open_task(&mut self, reference: p::TaskRef, cx: &mut Context<Self>) {
        let host = self.host.clone();
        self.run(
            async move {
                Ok(ResultView::Detail(
                    Client::new(host.channel()?)
                        .get_task(p::GetTaskRequest {
                            task: Some(reference),
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                ))
            },
            cx,
        );
    }
    pub(super) fn load_schema(&mut self, mode: FormMode, cx: &mut Context<Self>) {
        let project = match self.active_project() {
            Ok(p) => p,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let task = if mode == FormMode::Create {
            None
        } else {
            self.task_ref().ok()
        };
        let purpose = match mode {
            FormMode::Create => p::TaskSchemaPurpose::TaskCreate,
            FormMode::Transition => p::TaskSchemaPurpose::TaskTransitions,
            _ => p::TaskSchemaPurpose::TaskEdit,
        };
        let issue_type_id = self.issue_type.as_ref().map(|v| v.id.clone());
        let host = self.host.clone();
        self.run(
            async move {
                Ok(ResultView::Schema(
                    Client::new(host.channel()?)
                        .get_schema(p::TaskSchemaRequest {
                            project: Some(project),
                            task,
                            purpose: purpose as i32,
                            issue_type_id,
                            ..Default::default()
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    mode,
                ))
            },
            cx,
        );
    }
    pub(super) fn new_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.task = None;
        self.fields.clear();
        self.field_events.clear();
        self.mode = FormMode::Create;
        self.issue_type = None;
        self.create_labels.clear();
        self.create_assignees.clear();
        self.create_milestone = None;
        let saved = self
            .host
            .snapshot()
            .record
            .view(&self.draft_key())
            .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok());
        self.restoring = true;
        self.title.update(cx, |v, cx| {
            v.set_value(
                saved
                    .as_ref()
                    .and_then(|v| v["title"].as_str())
                    .unwrap_or("")
                    .to_owned(),
                window,
                cx,
            )
        });
        self.body.update(cx, |v, cx| {
            v.set_value(
                saved
                    .as_ref()
                    .and_then(|v| v["body"].as_str())
                    .unwrap_or(""),
                window,
                cx,
            )
        });
        self.restoring = false;
        if self.provider() == "jira" {
            self.load_choices(Choices::Types, false, cx);
        } else if self.provider() == "youtrack" {
            self.load_schema(FormMode::Create, cx);
        } else {
            cx.notify();
        }
    }
    pub(super) fn mutate(&mut self, action: Mutation, cx: &mut Context<Self>) {
        let mut project = match self.project_ref() {
            Ok(p) => p,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let task = self.task_ref().ok();
        // Mutations on the open object use the exact scope returned by the host.
        if let Some(t) = &task {
            project = p::TaskProject {
                host_id: t.host_id.clone(),
                integration_id: t.integration_id.clone(),
                project_id: t.project_id.clone(),
                workspace: t.workspace.clone(),
            };
        }
        let host = self.host.clone();
        let refresh_task = task.clone();
        let creation_project = project.clone();
        let deleting = matches!(action, Mutation::Delete(_));
        self.run(
            async move {
                let receipt = host
                    .mutate(None, move |channel, command| async move {
                        Client::new(channel)
                            .apply_mutation(p::TaskMutationRequest {
                                command: Some(command),
                                project: Some(project),
                                task,
                                action: Some(action),
                            })
                            .await
                    })
                    .await?;
                if deleting {
                    return Ok(ResultView::Receipt(receipt));
                }
                let reference = refresh_task.or_else(|| {
                    receipt
                        .operation_output
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .and_then(|r| r["task_id"].as_str().map(str::to_owned))
                        .map(|task_id| p::TaskRef {
                            host_id: creation_project.host_id,
                            integration_id: creation_project.integration_id,
                            project_id: creation_project.project_id,
                            workspace: creation_project.workspace,
                            task_id,
                        })
                });
                let refresh = if let Some(task) = reference {
                    Client::new(
                        host.channel()
                            .map_err(|e| format!("Change applied, refresh unavailable: {e}"))?,
                    )
                    .get_task(p::GetTaskRequest { task: Some(task) })
                    .await
                    .map(|r| Some(r.into_inner()))
                    .map_err(rpc_error)
                } else {
                    Ok(None)
                };
                let (task, error) = match refresh {
                    Ok(t) => (t, None),
                    Err(e) => (None, Some(e)),
                };
                Ok(ResultView::Changed(receipt, task, error))
            },
            cx,
        );
    }
    pub(super) fn save_task(&mut self, cx: &mut Context<Self>) {
        let title = value(&self.title, cx);
        let description = self.body.read(cx).value(cx);
        if title.trim().is_empty() {
            self.fail("Task title is required.", cx);
            return;
        }
        if self.mode == FormMode::Create {
            let fields = match self.collect_fields(true, cx) {
                Ok(f) => f,
                Err(e) => {
                    self.fail(e, cx);
                    return;
                }
            };
            self.mutate(
                Mutation::Create(p::TaskIssueDraft {
                    title,
                    description,
                    fields: Some(fields),
                    labels: self.create_labels.clone(),
                    assignees: self.create_assignees.clone(),
                    milestone_id: self.create_milestone.clone(),
                }),
                cx,
            );
        } else {
            let task = match self.task_ref() {
                Ok(t) => t,
                Err(e) => {
                    self.fail(e, cx);
                    return;
                }
            };
            let title = self
                .task
                .as_ref()
                .filter(|t| t.title != title)
                .map(|_| title);
            let description = self
                .task
                .as_ref()
                .filter(|t| t.description != description)
                .map(|_| description);
            if title.is_none() && description.is_none() {
                self.notice = "No title or description changes.".into();
                cx.notify();
                return;
            }
            let host = self.host.clone();
            let reference = task.clone();
            self.run(
                async move {
                    let receipt = host
                        .mutate(None, move |channel, command| async move {
                            Client::new(channel)
                                .update_task(p::UpdateTaskRequest {
                                    command: Some(command),
                                    task: Some(task),
                                    title,
                                    description,
                                    ..Default::default()
                                })
                                .await
                        })
                        .await?;
                    let refresh = match host.channel() {
                        Ok(channel) => Client::new(channel)
                            .get_task(p::GetTaskRequest {
                                task: Some(reference),
                            })
                            .await
                            .map(|r| Some(r.into_inner()))
                            .map_err(rpc_error),
                        Err(e) => Err(e),
                    };
                    let (task, error) = match refresh {
                        Ok(t) => (t, None),
                        Err(e) => (None, Some(e)),
                    };
                    Ok(ResultView::Changed(receipt, task, error))
                },
                cx,
            );
        }
    }
    fn collect_fields(&self, all: bool, cx: &App) -> Result<prost_types::Struct, String> {
        let mut fields = BTreeMap::new();
        for f in &self.fields {
            if matches!(f, FieldView::ReadOnly(_)) {
                continue;
            }
            if all || f.dirty(cx) {
                let id = f.id(cx);
                if self.mode == FormMode::Create
                    && ["summary", "description", "issuetype"].contains(&id.as_str())
                {
                    continue;
                }
                let v = f.value(cx)?;
                if self
                    .schema
                    .fields
                    .iter()
                    .chain(
                        self.schema
                            .transitions
                            .iter()
                            .flat_map(|t| t.required_fields.iter()),
                    )
                    .any(|field| field.id == id && field.required)
                    && (v.is_null()
                        || v.as_str().is_some_and(|v| v.trim().is_empty())
                        || v.as_array().is_some_and(|v| v.is_empty()))
                {
                    return Err(format!("{} is required.", id));
                }
                fields.insert(id, fields::proto(v));
            }
        }
        if self.mode == FormMode::Create && self.provider() == "jira" {
            let issue_type = self.issue_type.as_ref().ok_or("Choose an issue type.")?;
            fields.insert(
                "issuetype".into(),
                fields::proto(serde_json::json!({"id":issue_type.id})),
            );
        }
        Ok(prost_types::Struct { fields })
    }
    pub(super) fn save_fields(&mut self, cx: &mut Context<Self>) {
        let fields = match self.collect_fields(self.mode == FormMode::Transition, cx) {
            Ok(f) => f,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        if self.mode == FormMode::Transition {
            let Some(mut status) = self.transition.clone() else {
                self.fail("Choose a transition.", cx);
                return;
            };
            status.form_fields = Some(fields);
            status.comment = nonempty(self.comment.read(cx).value(cx));
            self.mutate(Mutation::Status(status), cx);
        } else {
            self.mutate(Mutation::Fields(fields), cx);
        }
    }
    pub(super) fn choose_transition(
        &mut self,
        t: p::TaskTransition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transition = Some(p::TaskStatusChoice {
            field_id: "jira-status".into(),
            choice_id: t.id,
            ..Default::default()
        });
        let provider = self.provider().to_owned();
        self.fields = t
            .required_fields
            .into_iter()
            .map(|f| FieldView::new(f, &provider, true, window, cx))
            .collect();
        self.field_events = self.fields.iter().filter_map(|f| f.subscribe(cx)).collect();
        cx.notify();
    }
    pub(super) fn load_comments(&mut self, more: bool, cx: &mut Context<Self>) {
        let task = match self.task_ref() {
            Ok(t) => t,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let page_token = more.then(|| self.comments_next.clone()).flatten();
        let host = self.host.clone();
        self.run(
            async move {
                Ok(ResultView::Comments(
                    Client::new(host.channel()?)
                        .read_comments(p::TaskCommentsRequest {
                            task: Some(task),
                            page_token,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    more,
                ))
            },
            cx,
        );
    }
    pub(super) fn edit_comment(
        &mut self,
        c: p::TaskComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.comment_rich = c.rich_body.clone().map(|v| {
            FieldView::new(
                p::TaskField {
                    id: "description".into(),
                    name: "Comment document".into(),
                    kind: "string".into(),
                    required: true,
                    current_value: Some(v),
                    ..Default::default()
                },
                "jira",
                false,
                window,
                cx,
            )
        });
        if let Some(f) = &self.comment_rich
            && let Some(event) = f.subscribe(cx)
        {
            self.field_events.push(event);
        }
        self.comment_id = Some(c.id);
        self.comment
            .update(cx, |v, cx| v.set_value(&c.body, window, cx));
        cx.notify();
    }
    pub(super) fn save_comment(&mut self, cx: &mut Context<Self>) {
        let body = self.comment.read(cx).value(cx);
        if body.trim().is_empty() && self.comment_rich.is_none() {
            self.fail("Comment cannot be empty.", cx);
            return;
        }
        let rich_body = match self.comment_rich.as_ref().map(|f| f.value(cx)).transpose() {
            Ok(v) => v.map(fields::proto),
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        self.mutate(
            Mutation::Comment(p::TaskCommentWrite {
                comment_id: self.comment_id.clone(),
                body,
                rich_body,
                delete: false,
            }),
            cx,
        );
    }
    pub(super) fn confirm(
        &mut self,
        title: String,
        help: &'static str,
        action: Mutation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let action = action.clone();
            dialog
                .title(title.clone())
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text("Delete")
                        .show_cancel(true),
                )
                .child(help)
                .on_ok(move |_, _, cx| {
                    let _ = owner.update(cx, |s, cx| s.mutate(action.clone(), cx));
                    true
                })
        });
    }
    pub(super) fn worklog(&mut self, cx: &mut Context<Self>) {
        let seconds = match value(&self.work_seconds, cx).parse::<u64>() {
            Ok(v) if v > 0 => v,
            _ => {
                self.fail("Enter a positive duration in seconds.", cx);
                return;
            }
        };
        self.mutate(
            Mutation::Worklog(p::TaskWorklogWrite {
                seconds,
                started: value(&self.work_started, cx),
                comment: self.comment.read(cx).value(cx),
            }),
            cx,
        );
    }
    pub(super) fn task_worktree(&mut self, cx: &mut Context<Self>) {
        let task = match self.task_ref() {
            Ok(t) => t,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        let workspace = task.workspace.clone();
        if workspace.is_none() {
            self.fail(
                "Open this task in a workspace scope before creating a worktree.",
                cx,
            );
            return;
        }
        let branch = value(&self.branch, cx);
        if branch.trim().is_empty() {
            self.fail("Enter a worktree branch.", cx);
            return;
        }
        let host = self.host.clone();
        self.run(
            async move {
                let receipt = host
                    .mutate(None, move |channel, command| async move {
                        Client::new(channel)
                            .task_to_worktree(p::TaskToWorktreeRequest {
                                command: Some(command),
                                task: Some(task),
                                workspace,
                                branch,
                            })
                            .await
                    })
                    .await?;
                Ok(ResultView::Receipt(receipt))
            },
            cx,
        );
    }
    pub(super) fn save_filter(&mut self, cx: &mut Context<Self>) {
        let name = value(&self.filter_name, cx);
        let expression = value(&self.filter_expression, cx);
        if name.trim().is_empty() || expression.trim().is_empty() {
            self.fail("Filter name and expression are required.", cx);
            return;
        }
        let filter = p::TaskFilter {
            id: self
                .filter
                .as_ref()
                .filter(|f| !f.builtin)
                .map(|f| f.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name,
            expression,
            provider: self.provider().into(),
            builtin: false,
        };
        let host = self.host.clone();
        self.run(
            async move {
                Ok(ResultView::Receipt(
                    host.mutate(None, move |channel, command| async move {
                        Client::new(channel)
                            .save_filter(p::SaveTaskFilterRequest {
                                command: Some(command),
                                filter: Some(filter),
                            })
                            .await
                    })
                    .await?,
                ))
            },
            cx,
        );
    }
    pub(super) fn remove_filter(&mut self, cx: &mut Context<Self>) {
        let Some(filter) = self.filter.as_ref().filter(|f| !f.builtin) else {
            return;
        };
        let filter_id = filter.id.clone();
        let host = self.host.clone();
        self.run(
            async move {
                Ok(ResultView::Receipt(
                    host.mutate(None, move |channel, command| async move {
                        Client::new(channel)
                            .remove_filter(p::RemoveTaskFilterRequest {
                                command: Some(command),
                                filter_id,
                            })
                            .await
                    })
                    .await?,
                ))
            },
            cx,
        );
    }
}
impl TasksView {
    fn active_project(&self) -> Result<p::TaskProject, String> {
        if let Ok(t) = self.task_ref() {
            Ok(p::TaskProject {
                host_id: t.host_id,
                integration_id: t.integration_id,
                project_id: t.project_id,
                workspace: t.workspace,
            })
        } else {
            self.project_ref()
        }
    }
    pub(super) fn history(&mut self, more: bool, cx: &mut Context<Self>) {
        let project = self.active_project().ok();
        let task = self.task_ref().ok();
        let page_token = if more {
            self.activity_next.clone()
        } else {
            None
        };
        let host = self.host.clone();
        self.run(
            async move {
                Ok(ResultView::Activity(
                    Client::new(host.channel()?)
                        .get_schema(p::TaskSchemaRequest {
                            project,
                            task,
                            purpose: p::TaskSchemaPurpose::TaskHistory as i32,
                            page_token,
                            ..Default::default()
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    more,
                ))
            },
            cx,
        );
    }
}

fn toggle(values: &mut Vec<String>, value: String) {
    if values.contains(&value) {
        values.retain(|v| v != &value);
    } else {
        values.push(value);
    }
}
