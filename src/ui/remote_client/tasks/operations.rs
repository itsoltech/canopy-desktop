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
                let project = match workspace.clone() {
                    Some(workspace) => Some(workspace.project.ok_or("Project unavailable.")?),
                    None => None,
                };
                Ok(ResultView::Integrations(
                    Client::new(host.channel()?)
                        .list_integrations(p::ListIntegrationsRequest {
                            resource: Some(match project.clone() {
                                Some(project) => {
                                    p::list_integrations_request::Resource::Project(project)
                                }
                                None => p::list_integrations_request::Resource::Environment(
                                    p::EnvironmentRef {
                                        host_id: host.id().as_str().into(),
                                    },
                                ),
                            }),
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    workspace,
                    project,
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
        self.projects.clear();
        self.projects_next = None;
        self.task = None;
        self.tasks.clear();
        self.next = None;
        self.filter = None;
        self.filters.clear();
        self.fields.clear();
        self.mode = FormMode::Detail;
        self.editing = false;
        self.action = None;
        self.choice_kind = None;
        self.manage_filters = false;
        self.sync_integration_picker(cx);
        self.sync_project_picker(cx);
        self.sync_filter_picker(cx);
        self.load_choices(Choices::Projects, false, cx);
    }
    pub(super) fn pick_integration(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.integration.as_ref().is_some_and(|i| i.id == id) {
            return;
        }
        if let Some(integration) = self
            .integrations
            .iter()
            .find(|i| i.id == id && i.configured)
            .cloned()
        {
            self.choose_integration(integration, cx);
        }
    }
    pub(super) fn pick_project(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.project.as_ref().is_some_and(|p| p.id == id) {
            return;
        }
        if let Some(project) = self.projects.iter().find(|p| p.id == id).cloned() {
            self.select_project(project, cx);
        }
    }
    pub(super) fn select_project(&mut self, project: p::TaskChoice, cx: &mut Context<Self>) {
        self.project = Some(project);
        self.task = None;
        self.tasks.clear();
        self.next = None;
        self.issue_type = None;
        self.board = None;
        self.editing = false;
        self.action = None;
        if self.mode == FormMode::Create {
            self.mode = FormMode::Detail;
            self.fields.clear();
        }
        self.sync_project_picker(cx);
        self.search(false, cx);
    }
    pub(super) fn choose_filter(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if id.is_empty() {
            if self.filter.is_none() {
                return;
            }
            self.filter = None;
            self.filter_expression
                .update(cx, |v, cx| v.set_value("", window, cx));
        } else {
            if self.filter.as_ref().is_some_and(|f| f.id == id) {
                return;
            }
            let Some(f) = self.filters.iter().find(|f| f.id == id).cloned() else {
                return;
            };
            self.filter_name
                .update(cx, |v, cx| v.set_value(f.name.clone(), window, cx));
            self.filter_expression
                .update(cx, |v, cx| v.set_value(f.expression.clone(), window, cx));
            self.filter = Some(f);
        }
        self.search(false, cx);
    }
    pub(super) fn sync_integration_picker(&mut self, cx: &mut Context<Self>) {
        let items: Vec<_> = self
            .integrations
            .iter()
            .filter(|i| i.configured)
            .map(|i| SelectOption::new(i.id.clone(), i.name.clone()))
            .collect();
        let selected = self.integration.as_ref().map(|i| i.id.clone());
        let picker = self.integration_picker.clone();
        let _ = self.window.update(cx, |_, w, cx| {
            picker.update(cx, |s, cx| {
                s.set_items(items, w, cx);
                match selected {
                    Some(id) => s.set_selected_value(&id.into(), w, cx),
                    None => s.set_selected_index(None, w, cx),
                }
            })
        });
    }
    pub(super) fn sync_project_picker(&mut self, cx: &mut Context<Self>) {
        let mut items: Vec<_> = self
            .projects
            .iter()
            .map(|p| SelectOption::new(p.id.clone(), p.label.clone()))
            .collect();
        if let Some(p) = &self.project
            && !self.projects.iter().any(|v| v.id == p.id)
        {
            items.insert(0, SelectOption::new(p.id.clone(), p.label.clone()));
        }
        let selected = self.project.as_ref().map(|p| p.id.clone());
        let picker = self.project_picker.clone();
        let _ = self.window.update(cx, |_, w, cx| {
            picker.update(cx, |s, cx| {
                s.set_items(items, w, cx);
                match selected {
                    Some(id) => s.set_selected_value(&id.into(), w, cx),
                    None => s.set_selected_index(None, w, cx),
                }
            })
        });
    }
    pub(super) fn sync_filter_picker(&mut self, cx: &mut Context<Self>) {
        let items: Vec<_> = std::iter::once(SelectOption::new("", "No saved filter"))
            .chain(
                self.filters
                    .iter()
                    .map(|f| SelectOption::new(f.id.clone(), f.name.clone())),
            )
            .collect();
        let selected = self
            .filter
            .as_ref()
            .map(|f| f.id.clone())
            .unwrap_or_default();
        let picker = self.filter_picker.clone();
        let _ = self.window.update(cx, |_, w, cx| {
            picker.update(cx, |s, cx| {
                s.set_items(items, w, cx);
                s.set_selected_value(&selected.into(), w, cx);
            })
        });
    }
    pub(super) fn sync_description(&mut self, cx: &mut Context<Self>) {
        let (body, url) = self
            .task
            .as_ref()
            .map(|t| (Arc::from(t.description.as_str()), t.url.clone()))
            .unwrap_or_else(|| (Arc::from(""), String::new()));
        self.description
            .update(cx, |v, cx| v.set_content(body, url, true, cx));
    }
    pub(super) fn sync_comment_views(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<_> = self.comments.iter().map(|c| c.id.clone()).collect();
        self.comment_views.retain(|id, _| ids.contains(id));
        for c in &self.comments {
            let view = self
                .comment_views
                .entry(c.id.clone())
                .or_insert_with(|| cx.new(crate::ui::markdown::MarkdownView::reading))
                .clone();
            let (body, url) = (Arc::from(c.body.as_str()), c.url.clone());
            view.update(cx, |v, cx| v.set_content(body, url, true, cx));
        }
    }
    pub(super) fn select_tab(&mut self, tab: DetailTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        if tab == DetailTab::Comments && !self.comments_loaded && self.task.is_some() {
            self.load_comments(false, cx);
        }
        cx.notify();
    }
    pub(super) fn load_choices(&mut self, kind: Choices, more: bool, cx: &mut Context<Self>) {
        let Some(integration) = self.integration.clone() else {
            return;
        };
        let host = self.host.clone();
        let workspace = self.workspace.clone();
        let project = self.active_project().ok();
        let cursor = if kind == Choices::Projects {
            more.then(|| self.projects_next.clone()).flatten()
        } else {
            more.then(|| self.choice_next.clone()).flatten()
        };
        let board = self.board.as_ref().map(|b| b.id.clone());
        let loading = if kind == Choices::Projects {
            Loading::Projects
        } else {
            Loading::Other
        };
        self.run_as(
            loading,
            async move {
                let mut client = Client::new(host.channel()?);
                let page = match &kind {
                    Choices::Projects => client
                        .list_projects(p::ListProjectsRequest {
                            project: Some(p::TaskProject {
                                integration_id: integration.id,
                                project_key: String::new(),
                                resource: Some(match workspace {
                                    Some(w) => p::task_project::Resource::Project(
                                        w.project.ok_or("Project unavailable.")?,
                                    ),
                                    None => {
                                        p::task_project::Resource::Environment(p::EnvironmentRef {
                                            host_id: host.id().as_str().into(),
                                        })
                                    }
                                }),
                            }),
                            page_token: cursor,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                    Choices::Metadata(kind) => client
                        .read_options(p::ReadOptionsRequest {
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
                            .get_schema(p::GetSchemaRequest {
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
            Choices::Projects => self.select_project(choice, cx),
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
                    self.fail("Enter the issue key before you choose a link type.", cx);
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
            project: Some(project),
            query: value(&self.query, cx),
            page_token: more.then(|| self.next.clone()).flatten(),
            filter_id: self.filter.as_ref().map(|f| f.id.clone()),
            filter_expression: nonempty(value(&self.filter_expression, cx)),
            state: ["open", "closed"][self.state].into(),
        };
        let host = self.host.clone();
        self.run_as(
            if more {
                Loading::MoreTasks
            } else {
                Loading::Tasks
            },
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
        self.run_as(
            Loading::Task,
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
                        .get_schema(p::GetSchemaRequest {
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
        self.editing = false;
        self.action = None;
        self.choice_kind = None;
        self.sync_description(cx);
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
            let Some(context) = t.project.clone() else {
                self.fail("Task project is unavailable.", cx);
                return;
            };
            project = context;
        }
        let host = self.host.clone();
        let refresh_task = task.clone();
        let creation_project = project.clone();
        let deleting = matches!(action, Mutation::Delete(_));
        self.run_as(
            Loading::Save,
            async move {
                let receipt = host
                    .mutate_task(TaskMutation::Apply(p::ApplyMutationRequest {
                        command: None,
                        project: Some(project),
                        task_id: task.map(|t| t.task_id),
                        action: Some(action),
                    }))
                    .await?;
                if deleting {
                    return Ok(ResultView::Receipt(receipt));
                }
                let reference = refresh_task.or_else(|| {
                    task_result(&receipt)
                        .and_then(|r| r.task_id.clone())
                        .map(|task_id| p::TaskRef {
                            project: Some(creation_project),
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
            self.run_as(
                Loading::Save,
                async move {
                    let receipt = host
                        .mutate_task(TaskMutation::Update(p::UpdateTaskRequest {
                            command: None,
                            task: Some(task),
                            title,
                            description,
                            ..Default::default()
                        }))
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
                    let name = self
                        .schema
                        .fields
                        .iter()
                        .chain(
                            self.schema
                                .transitions
                                .iter()
                                .flat_map(|t| t.required_fields.iter()),
                        )
                        .find(|field| field.id == id)
                        .map(|field| field.name.clone())
                        .unwrap_or(id);
                    return Err(format!("Enter {name}."));
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
        self.run_as(
            Loading::Comments,
            async move {
                Ok(ResultView::Comments(
                    Client::new(host.channel()?)
                        .read_comments(p::ReadCommentsRequest {
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
        ok: &'static str,
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
                        .ok_text(ok)
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
        let Some(seconds) = adapter::parse_duration(&value(&self.work_duration, cx)) else {
            self.fail("Enter the time spent, for example 1h 30m.", cx);
            return;
        };
        let started = value(&self.work_started, cx);
        let started = if started.trim().is_empty() {
            adapter::now_local()
        } else {
            started.trim().to_owned()
        };
        let started = match canopy_desktop::integrations::jira::date_time(&started) {
            Ok(v) => v,
            Err(e) => {
                self.fail(e, cx);
                return;
            }
        };
        self.mutate(
            Mutation::Worklog(p::TaskWorklogWrite {
                seconds,
                started,
                comment: value(&self.work_comment, cx),
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
        if task.project.as_ref().and_then(|p| p.resource.as_ref()).is_none_or(|p| !matches!(p, p::task_project::Resource::Project(project) if project.worktree_id.is_some())) {
            self.fail(
                "Open this task from a project workspace to create a worktree.",
                cx,
            );
            return;
        }
        let branch = value(&self.branch, cx);
        if branch.trim().is_empty() {
            self.fail("Enter a branch name for the worktree.", cx);
            return;
        }
        let host = self.host.clone();
        self.run_as(
            Loading::Save,
            async move {
                let receipt = host
                    .mutate_task(TaskMutation::Worktree(p::TaskToWorktreeRequest {
                        command: None,
                        task: Some(task),
                        branch,
                    }))
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
            self.fail("Enter a filter name and condition.", cx);
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
                    host.mutate_task(TaskMutation::SaveFilter(p::SaveFilterRequest {
                        command: None,
                        environment: None,
                        filter: Some(filter),
                    }))
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
                    host.mutate_task(TaskMutation::RemoveFilter(p::RemoveFilterRequest {
                        command: None,
                        environment: None,
                        filter_id,
                    }))
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
            t.project
                .ok_or_else(|| "Task project is unavailable.".into())
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
                        .get_schema(p::GetSchemaRequest {
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
