use super::*;
use crate::command_bus::tasks::{TaskAction as A, TaskAttachmentAction, TaskChanges, TaskStatus};
use mapping as m;
use serde_json::{Value, json};

pub(super) struct MutationContext<'a> {
    pub(super) provider: &'a dyn i::TaskProvider,
    pub(super) project: &'a i::ProjectTarget,
    pub(super) task: Option<&'a i::TaskRef>,
    pub(super) grant: &'a DeviceGrant,
    pub(super) reference: &'a proto::TaskProject,
    pub(super) root: &'a str,
}

impl TasksRpc {
    pub(super) async fn configure_project(
        &self,
        g: DeviceGrant,
        mut r: proto::ConfigureTaskProjectRequest,
    ) -> Result<proto::CommandReceipt, Status> {
        use crate::command_bus::tasks::TaskConfiguration;
        let meta = required(r.command.take())?;
        let change = decode::configuration(required(r.change.take())?)?;
        let resource = required(r.resource.take())?;
        let is_project = matches!(
            resource,
            proto::configure_task_project_request::Resource::Project(_)
        );
        if matches!(change, TaskConfiguration::SelectFilter(_)) == is_project {
            return Err(invalid(
                "Filter selection requires the environment; repository and links require a project.",
            ));
        }
        let context = proto::TaskProject {
            resource: Some(match resource {
                proto::configure_task_project_request::Resource::Project(p) => {
                    proto::task_project::Resource::Project(p)
                }
                proto::configure_task_project_request::Resource::Environment(e) => {
                    proto::task_project::Resource::Environment(e)
                }
            }),
            ..Default::default()
        };
        let (prepared, resource) = self
            .prepare(&g, meta, &context, TaskCommand::Configure(change.clone()))
            .await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt_for_resource(&self.auth, prior, resource);
        }
        let input = prepared.runtime_input().clone();
        let root = required(input.root.clone())?;
        let this = self.clone();
        self.mutation(input, resource, TASKS, false, move || async move {
            this.auth.resource(g.id(), TASKS, &root).await?;
            Ok(async move {
                this.host.configure(root, change).await?;
                Ok(b"{}".to_vec())
            })
        })
        .await
    }
    pub(super) async fn apply(
        &self,
        g: DeviceGrant,
        mut r: proto::ApplyMutationRequest,
    ) -> Result<proto::CommandReceipt, Status> {
        let meta = required(r.command.take())?;
        let p = required(r.project.clone())?;
        let action = decode::action(required(r.action.take())?)?;
        let command = TaskCommand::Mutate {
            integration: p.integration_id.clone(),
            project: p.project_key.clone(),
            task: r.task_id.clone(),
            action: action.clone(),
        };
        let (prepared, resource) = self.prepare(&g, meta, &p, command).await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt_for_resource(&self.auth, prior, resource);
        }
        let reject_recovery = prepared.prior().is_some() && action.requires_fresh_admission();
        let (input, account) = self.account_input(&prepared, &p.integration_id).await?;
        let root = required(input.root.clone())?;
        let this = self.clone();
        self.mutation(
            input,
            resource,
            TASKS,
            reject_recovery,
            move || async move {
                let project = Self::project(&account, &p.project_key)?;
                this.current_account(&account).await?;
                let provider = this.provider(&account, &g, &root).await?;
                let task = r
                    .task_id
                    .map(|task_id| {
                        Self::task(
                            &project,
                            &proto::TaskRef {
                                project: Some(p.clone()),
                                task_id,
                            },
                        )
                    })
                    .transpose()?;
                let command = this
                    .command(
                        MutationContext {
                            provider: provider.as_ref(),
                            project: &project,
                            task: task.as_ref(),
                            grant: &g,
                            reference: &p,
                            root: &root,
                        },
                        action,
                    )
                    .await?;
                this.auth.resource(g.id(), TASKS, &root).await?;
                this.current_account(&account).await?;
                Ok(async move {
                    match command {
                        Some(command) => Ok(m::receipt(
                            provider.write(&command).await.map_err(write_error)?,
                        )),
                        None => {
                            Ok(br#"{"notice":"Task already has the requested state."}"#.to_vec())
                        }
                    }
                })
            },
        )
        .await
    }
    pub(super) async fn current_account(&self, account: &i::Account) -> Result<(), Status> {
        if self.account(&account.id).await? != *account {
            return Err(Status::aborted(
                "Integration changed before dispatch. Refresh the connection.",
            ));
        }
        Ok(())
    }
    pub(super) async fn update(
        &self,
        g: DeviceGrant,
        mut r: proto::UpdateTaskRequest,
    ) -> Result<proto::CommandReceipt, Status> {
        let meta = required(r.command.take())?;
        let reference = required(r.task.clone())?;
        let context = required(reference.project.as_ref())?;
        let changes = decode::changes(&r)?;
        if r.status.is_some()
            && (r.title.is_some()
                || r.description.is_some()
                || r.labels.is_some()
                || r.assignees.is_some()
                || r.milestone.is_some())
        {
            return Err(invalid("Submit status separately from other fields."));
        }
        let command = TaskCommand::Update {
            integration: context.integration_id.clone(),
            project: context.project_key.clone(),
            task: reference.task_id.clone(),
            changes: changes.clone(),
        };
        let (prepared, resource) = self.prepare(&g, meta, context, command).await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt_for_resource(&self.auth, prior, resource);
        }
        let (input, account) = self
            .account_input(&prepared, &context.integration_id)
            .await?;
        let root = required(input.root.clone())?;
        let project_key = context.project_key.clone();
        let this = self.clone();
        self.mutation(input,resource,TASKS,false,move||async move{
            let task = Self::task(&Self::project(&account, &project_key)?, &reference)?;
            this.current_account(&account).await?;
            let provider=this.provider(&account,&g,&root).await?;
            if account.provider==i::Provider::Jira && r.description.is_some() {
                let current=provider.task(&task).await.map_err(domain)?;
                if current.jira.as_ref().is_none_or(|d|!d.description_editable){return Err(domain("The description contains rich Jira content. Use the native field document to preserve it."));}
            }
            let command=if let Some(status)=changes.status.clone(){status_command(provider.as_ref(),&task,status).await?}else{Some(update_command(&task,&changes)?)};
            this.auth.resource(g.id(),TASKS,&root).await?;this.current_account(&account).await?;
            Ok(async move { match command{Some(c)=>Ok(m::receipt(provider.write(&c).await.map_err(write_error)?)),None=>Ok(br#"{"notice":"Task already has the requested state."}"#.to_vec())} })
        }).await
    }
    pub(super) async fn save(
        &self,
        g: DeviceGrant,
        mut r: proto::SaveFilterRequest,
    ) -> Result<proto::CommandReceipt, Status> {
        let meta = required(r.command.take())?;
        let environment = required(r.environment.take())?;
        let f = required(r.filter.as_ref())?;
        if f.builtin {
            return Err(invalid("Built-in filters cannot be overwritten."));
        }
        let filter = i::filters::TaskFilter {
            id: f.id.clone(),
            name: f.name.clone(),
            expression: f.expression.clone(),
            provider: match f.provider.as_str() {
                "jira" => i::Provider::Jira,
                "youtrack" => i::Provider::Youtrack,
                _ => return Err(invalid("Unsupported custom filter provider.")),
            },
        };
        filter.validate().map_err(invalid)?;
        let context = proto::TaskProject {
            resource: Some(proto::task_project::Resource::Environment(environment)),
            ..Default::default()
        };
        let (prepared, resource) = self
            .prepare(&g, meta, &context, TaskCommand::SaveFilter(filter.clone()))
            .await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt_for_resource(&self.auth, prior, resource);
        }
        let input = prepared.runtime_input().clone();
        let root = required(input.root.clone())?;
        let this = self.clone();
        self.mutation(input, resource, TASKS, false, move || async move {
            this.auth.resource(g.id(), TASKS, &root).await?;
            Ok(async move {
                this.host.save_filter(filter).await?;
                Ok(b"{}".to_vec())
            })
        })
        .await
    }
    pub(super) async fn remove(
        &self,
        g: DeviceGrant,
        mut r: proto::RemoveFilterRequest,
    ) -> Result<proto::CommandReceipt, Status> {
        let meta = required(r.command.take())?;
        let environment = required(r.environment.take())?;
        if uuid::Uuid::parse_str(&r.filter_id).is_err() {
            return Err(invalid("Only custom filters can be removed."));
        }
        let context = proto::TaskProject {
            resource: Some(proto::task_project::Resource::Environment(environment)),
            ..Default::default()
        };
        let (prepared, resource) = self
            .prepare(
                &g,
                meta,
                &context,
                TaskCommand::RemoveFilter(r.filter_id.clone()),
            )
            .await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt_for_resource(&self.auth, prior, resource);
        }
        let reject_recovery = prepared.prior().is_some();
        let input = prepared.runtime_input().clone();
        let root = required(input.root.clone())?;
        let this = self.clone();
        self.mutation(
            input,
            resource,
            TASKS,
            reject_recovery,
            move || async move {
                this.auth.resource(g.id(), TASKS, &root).await?;
                Ok(async move {
                    this.host.remove_filter(r.filter_id).await?;
                    Ok(b"{}".to_vec())
                })
            },
        )
        .await
    }
    pub(super) async fn worktree(
        &self,
        g: DeviceGrant,
        mut r: proto::TaskToWorktreeRequest,
    ) -> Result<proto::CommandReceipt, Status> {
        let meta = required(r.command.take())?;
        let reference = required(r.task.clone())?;
        let context = required(reference.project.as_ref())?;
        let project_ref = match required(context.resource.clone())? {
            proto::task_project::Resource::Project(project) => project,
            _ => return Err(invalid("Worktree requires a project checkout.")),
        };
        if project_ref.worktree_id.is_none() {
            return Err(invalid("Worktree requires a Git checkout."));
        }
        if r.branch.is_empty()
            || !git2::Reference::is_valid_name(&format!("refs/heads/{}", r.branch))
        {
            return Err(invalid("Invalid worktree branch."));
        }
        let command = TaskCommand::Worktree {
            integration: context.integration_id.clone(),
            project: context.project_key.clone(),
            task: reference.task_id.clone(),
            branch: r.branch.clone(),
        };
        let (prepared, resource) = self.prepare(&g, meta, context, command).await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt_for_resource(&self.auth, prior, resource);
        }
        let reject_recovery = prepared.prior().is_some();
        let (input, account) = self
            .account_input(&prepared, &context.integration_id)
            .await?;
        let root = required(input.root.clone())?;
        let project_key = context.project_key.clone();
        let this = self.clone();
        self.mutation(
            input,
            resource,
            TASKS | crate::host::FILES_GIT,
            reject_recovery,
            move || async move {
                let (project_ref, _) = this.roots.project(project_ref, &this.auth)?;
                let task = Self::task(&Self::project(&account, &project_key)?, &reference)?;
                this.current_account(&account).await?;
                let provider = this.provider(&account, &g, &root).await?;
                let item = provider.task(&task).await.map_err(domain)?;
                if !item.reference.same_task(&task) {
                    return Err(domain("Provider returned a different task."));
                }
                this.auth
                    .resource(g.id(), TASKS | crate::host::FILES_GIT, &root)
                    .await?;
                this.current_account(&account).await?;
                Ok(async move {
                    let id = this
                        .host
                        .task_to_worktree(project_ref, item.reference, r.branch)
                        .await?;
                    Ok(serde_json::to_vec(&json!({"workspace_id":id})).expect("string result"))
                })
            },
        )
        .await
    }
    pub(super) async fn command(
        &self,
        context: MutationContext<'_>,
        action: A,
    ) -> Result<Option<i::TaskWrite>, Status> {
        let MutationContext {
            provider,
            project,
            task,
            grant: g,
            reference: p,
            root,
        } = context;
        let task_required = || {
            task.cloned()
                .ok_or_else(|| invalid("Task is required for this operation."))
        };
        let jira = |action| -> Result<i::TaskWrite, Status> {
            if project.provider != i::Provider::Jira {
                return Err(Status::unimplemented("This action requires Jira."));
            }
            Ok(i::TaskWrite::Jira {
                project: project.clone(),
                task: task.cloned(),
                action,
            })
        };
        let youtrack = |action| i::TaskWrite::Youtrack {
            project: project.clone(),
            task: task.cloned(),
            action,
        };
        use i::{jira::JiraWrite as J, youtrack::YoutrackWrite as Y};
        Ok(Some(match action {
            A::Create(d) => {
                if task.is_some() {
                    return Err(invalid("Create must not select an existing task."));
                }
                let fields = d.fields;
                let draft = i::IssueDraft {
                    title: d.title,
                    body: d.description,
                    labels: d.labels,
                    assignees: d.assignees,
                    milestone: number(d.milestone_id.as_deref())?,
                    fields: Default::default(),
                };
                draft.validate().map_err(invalid)?;
                match project.provider {
                    i::Provider::Github => {
                        if !fields.as_object().is_some_and(|f| f.is_empty()) {
                            return Err(invalid("GitHub does not support custom issue fields."));
                        }
                        i::TaskWrite::Create {
                            project: project.clone(),
                            draft,
                        }
                    }
                    i::Provider::Jira => {
                        let mut fields = fields;
                        fields["summary"] = json!(draft.title);
                        if !draft.body.is_empty() || fields.get("description").is_none() {
                            fields["description"] =
                                i::jira::adf::from_markdown(&draft.body).map_err(invalid)?;
                        }
                        if !draft.labels.is_empty() {
                            fields["labels"] = json!(draft.labels);
                        }
                        if !draft.assignees.is_empty() {
                            if draft.assignees.len() != 1 {
                                return Err(invalid("Jira supports one assignee."));
                            }
                            fields["assignee"] = json!({"accountId":draft.assignees[0]});
                        }
                        if draft.milestone.is_some() {
                            return Err(invalid("Jira does not use GitHub milestones."));
                        }
                        jira(J::Create { fields })?
                    }
                    i::Provider::Youtrack => {
                        if !draft.labels.is_empty()
                            || !draft.assignees.is_empty()
                            || draft.milestone.is_some()
                        {
                            return Err(invalid(
                                "Use native YouTrack fields; tags are applied after creation.",
                            ));
                        }
                        let fields = youtrack_values(provider, project, None, fields, true).await?;
                        youtrack(Y::Create { draft, fields })
                    }
                }
            }
            A::Status(s) => return status_command(provider, &task_required()?, s).await,
            A::Fields(s) => {
                let fields = s;
                task_required()?;
                match project.provider {
                    i::Provider::Jira => jira(J::Fields { fields })?,
                    i::Provider::Youtrack => youtrack(Y::Fields {
                        fields: youtrack_values(provider, project, task, fields, false).await?,
                    }),
                    _ => {
                        return Err(Status::unimplemented(
                            "GitHub does not support provider field forms.",
                        ));
                    }
                }
            }
            A::Field(f) => {
                let t = task_required()?;
                let v = f.value;
                match project.provider {
                    i::Provider::Jira => jira(J::Fields {
                        fields: json!({f.field_id:v}),
                    })?,
                    i::Provider::Youtrack => youtrack(Y::Fields {
                        fields: youtrack_values(
                            provider,
                            project,
                            Some(&t),
                            json!({f.field_id:v}),
                            false,
                        )
                        .await?,
                    }),
                    _ => {
                        return Err(Status::unimplemented(
                            "GitHub does not support custom fields.",
                        ));
                    }
                }
            }
            A::Comment(c) => {
                let t = task_required()?;
                i::validate_body(&c.body, !c.delete && c.rich_body.is_none()).map_err(invalid)?;
                if let Some(id) = c.comment_id {
                    let comment = find_comment(provider, &t, &id).await?;
                    if c.delete {
                        if c.rich_body.is_some() {
                            return Err(invalid("Delete comment cannot contain a rich body."));
                        }
                        i::TaskWrite::DeleteComment { task: t, comment }
                    } else if let Some(rich) = c.rich_body {
                        jira(J::CommentDocument {
                            comment,
                            document: rich,
                        })?
                    } else {
                        i::TaskWrite::EditComment {
                            task: t,
                            comment,
                            body: c.body,
                        }
                    }
                } else {
                    if c.delete || c.rich_body.is_some() {
                        return Err(invalid(
                            "Rich document replacement and deletion require an existing comment.",
                        ));
                    }
                    i::TaskWrite::AddComment {
                        task: t,
                        body: c.body,
                    }
                }
            }
            A::Tag(v) => {
                let t = task_required()?;
                if project.provider == i::Provider::Youtrack {
                    youtrack(Y::Tag {
                        id: v.id,
                        add: v.add,
                    })
                } else {
                    i::TaskWrite::Label {
                        task: t,
                        label: v.id,
                        add: v.add,
                    }
                }
            }
            A::Assignee(v) => {
                let t = task_required()?;
                if project.provider == i::Provider::Youtrack {
                    return Err(Status::unimplemented(
                        "Use the native YouTrack assignee field.",
                    ));
                }
                i::TaskWrite::Assignee {
                    task: t,
                    login: v.id,
                    add: v.add,
                }
            }
            A::Milestone(v) => {
                let t = task_required()?;
                if project.provider != i::Provider::Github {
                    return Err(Status::unimplemented("Milestones require GitHub."));
                }
                i::TaskWrite::Milestone {
                    task: t,
                    number: number(v.as_deref())?,
                }
            }
            A::Attachment(v) => {
                let t = task_required()?;
                if project.provider == i::Provider::Github {
                    return Err(Status::unimplemented(
                        "GitHub attachment writes are not supported by the adapter.",
                    ));
                }
                match v.action {
                    TaskAttachmentAction::DeleteId(id) => {
                        if project.provider == i::Provider::Jira {
                            jira(J::DeleteAttachment { id })?
                        } else {
                            youtrack(Y::DeleteAttachment { id })
                        }
                    }
                    TaskAttachmentAction::Attach(artifact) => {
                        self.auth.check_host(artifact.host.as_str())?;
                        let expected = crate::artifacts::Target::Task {
                            integration_id: p.integration_id.clone(),
                            project_id: p.project_key.clone(),
                            task_id: t.id,
                            root: root.into(),
                        };
                        let store = self.artifacts.clone();
                        let owner = g.id().to_owned();
                        let (bytes, media_type) = tokio::task::spawn_blocking(move || {
                            let upload = store
                                .status(&owner, &artifact.artifact)
                                .map_err(storage_status)?;
                            let b = upload.binding();
                            if b.target() != &expected
                                || b.purpose() != &crate::artifacts::Purpose::TaskAttachment
                                || upload.artifact_id() != Some(artifact.artifact.as_str())
                            {
                                return Err(Status::permission_denied(
                                    "Attachment is not bound to this task.",
                                ));
                            }
                            if b.size() != artifact.size
                                || b.sha256().as_slice() != artifact.sha256.as_slice()
                                || b.media_type() != artifact.media_type
                            {
                                return Err(invalid(
                                    "Attachment metadata differs from the finished upload.",
                                ));
                            }
                            if b.size() > 25 * 1024 * 1024 {
                                return Err(Status::resource_exhausted(
                                    "Attachment exceeds 25 MiB.",
                                ));
                            }
                            let mut bytes = Vec::with_capacity(b.size() as usize);
                            while bytes.len() < (b.size() as usize) {
                                let chunk = store
                                    .download(
                                        &owner,
                                        &artifact.artifact,
                                        bytes.len() as u64,
                                        crate::artifacts::CHUNK_BYTES,
                                    )
                                    .map_err(storage_status)?;
                                if chunk.is_empty() {
                                    return Err(domain("Attachment is incomplete."));
                                }
                                bytes.extend(chunk);
                            }
                            Ok((Arc::new(bytes), b.media_type().to_owned()))
                        })
                        .await
                        .map_err(|_| domain("Attachment worker stopped."))??;
                        if project.provider == i::Provider::Jira {
                            jira(J::Attach {
                                name: v.name,
                                bytes,
                            })?
                        } else {
                            youtrack(Y::Attach {
                                name: v.name,
                                mime_type: media_type,
                                bytes,
                            })
                        }
                    }
                }
            }
            A::Link(v) => {
                task_required()?;
                jira(if let Some(id) = v.delete_link_id {
                    J::DeleteLink { id }
                } else {
                    J::Link {
                        kind: v.kind,
                        other: v.other_task,
                        outward: v.outward,
                    }
                })?
            }
            A::Worklog(v) => {
                task_required()?;
                jira(J::LogWork {
                    seconds: v.seconds,
                    started: v.started,
                    comment: v.comment,
                })?
            }
            A::Watching(watching) => {
                task_required()?;
                jira(J::Watch { watching })?
            }
            A::Voted(voted) => {
                task_required()?;
                jira(J::Vote { voted })?
            }
            A::Sprint(v) => {
                task_required()?;
                jira(J::Sprint { id: v })?
            }
            A::Delete(v) => {
                task_required()?;
                match project.provider {
                    i::Provider::Jira => jira(J::DeleteIssue {
                        confirmation: v.confirmation,
                        subtasks: v.subtasks,
                    })?,
                    i::Provider::Youtrack => {
                        if v.subtasks {
                            return Err(invalid("YouTrack recursive deletion is not supported."));
                        }
                        youtrack(Y::DeleteIssue {
                            confirmation: v.confirmation,
                        })
                    }
                    _ => {
                        return Err(Status::unimplemented(
                            "GitHub issue deletion is not supported by this adapter.",
                        ));
                    }
                }
            }
        }))
    }
}
fn number(id: Option<&str>) -> Result<Option<u64>, Status> {
    id.map(|v| {
        v.parse::<u64>()
            .ok()
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("Invalid GitHub milestone number."))
    })
    .transpose()
}
fn update_command(task: &i::TaskRef, r: &TaskChanges) -> Result<i::TaskWrite, Status> {
    if let Some(title) = &r.title
        && (title.trim().is_empty() || title.chars().count() > 256)
    {
        return Err(invalid("Enter a title up to 256 characters."));
    }
    if let Some(body) = &r.description {
        i::validate_body(body, false).map_err(invalid)?;
    }
    if r.title.is_none()
        && r.description.is_none()
        && r.labels.is_none()
        && r.assignees.is_none()
        && r.milestone.is_none()
    {
        return Err(invalid("No task changes were supplied."));
    }
    if task.project.provider == i::Provider::Github {
        return Ok(i::TaskWrite::Edit {
            task: task.clone(),
            title: r.title.clone(),
            body: r.description.clone(),
            labels: r.labels.clone(),
            assignees: r.assignees.clone(),
            milestone: r
                .milestone
                .as_ref()
                .map(|v| number(v.as_deref()))
                .transpose()?,
        });
    }
    let mut fields = serde_json::Map::new();
    if let Some(v) = &r.title {
        fields.insert("summary".into(), json!(v));
    }
    if let Some(v) = &r.description {
        fields.insert(
            "description".into(),
            if task.project.provider == i::Provider::Jira {
                i::jira::adf::from_markdown(v).map_err(invalid)?
            } else {
                json!(v)
            },
        );
    }
    if r.milestone.is_some() {
        return Err(Status::unimplemented("Milestones require GitHub."));
    }
    if task.project.provider == i::Provider::Jira {
        if let Some(v) = &r.labels {
            fields.insert("labels".into(), json!(v));
        }
        if let Some(v) = &r.assignees {
            if v.len() > 1 {
                return Err(invalid("Jira supports one assignee."));
            }
            fields.insert(
                "assignee".into(),
                v.first()
                    .map(|v| json!({"accountId":v}))
                    .unwrap_or(Value::Null),
            );
        }
        Ok(i::TaskWrite::Jira {
            project: task.project.clone(),
            task: Some(task.clone()),
            action: i::jira::JiraWrite::Fields {
                fields: Value::Object(fields),
            },
        })
    } else {
        if r.labels.is_some() || r.assignees.is_some() {
            return Err(invalid("Use native YouTrack fields and tag actions."));
        }
        Ok(i::TaskWrite::Youtrack {
            project: task.project.clone(),
            task: Some(task.clone()),
            action: i::youtrack::YoutrackWrite::Fields {
                fields: Value::Object(fields),
            },
        })
    }
}
pub(super) async fn status_command(
    provider: &dyn i::TaskProvider,
    task: &i::TaskRef,
    s: TaskStatus,
) -> Result<Option<i::TaskWrite>, Status> {
    let fields = i::status::load(provider, task).await.map_err(domain)?;
    let field = fields
        .into_iter()
        .find(|f| f.id() == s.field_id)
        .ok_or_else(|| invalid("Status field is no longer available."))?;
    if let Some(reason) = field.read_only_reason() {
        return Err(domain(reason));
    }
    let form = s.form_fields;
    if let i::status::StatusField::Jira { transitions, .. } = &field {
        let transition = transitions
            .iter()
            .find(|t| t.id == s.choice_id)
            .ok_or_else(|| domain("Workflow transition is no longer available."))?;
        for f in &transition.fields {
            if f.required
                && form.get(&f.id).is_none_or(|v| {
                    v.is_null()
                        || v.as_str() == Some("")
                        || v.as_array().is_some_and(|v| v.is_empty())
                })
            {
                return Err(invalid(format!("Required transition field: {}.", f.name)));
            }
        }
        if form.as_object().is_some_and(|v| {
            v.keys()
                .any(|id| !transition.fields.iter().any(|f| &f.id == id))
        }) {
            return Err(invalid(
                "Transition form contains a field outside this workflow.",
            ));
        }
        if transition.requires_form()
            || form.as_object().is_some_and(|v| !v.is_empty())
            || s.comment.as_ref().is_some_and(|v| !v.is_empty())
        {
            return Ok(Some(i::TaskWrite::Jira {
                project: task.project.clone(),
                task: Some(task.clone()),
                action: i::jira::JiraWrite::Transition {
                    id: s.choice_id,
                    fields: form,
                    comment: s.comment.unwrap_or_default(),
                },
            }));
        }
    } else if form.as_object().is_some_and(|v| !v.is_empty()) || s.comment.is_some() {
        return Err(invalid(
            "This status does not accept a Jira transition form.",
        ));
    }
    match field.choose(task, &s.choice_id).map_err(domain)? {
        None => Ok(None),
        Some(i::status::StatusChange::Write(c)) => Ok(Some(*c)),
        Some(i::status::StatusChange::JiraForm { .. }) => {
            Err(invalid("This workflow transition requires a form."))
        }
    }
}
async fn find_comment(
    provider: &dyn i::TaskProvider,
    task: &i::TaskRef,
    id: &str,
) -> Result<i::TaskComment, Status> {
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let page = provider
            .comments(task, cursor.as_deref())
            .await
            .map_err(domain)?;
        if let Some(c) = page.comments.into_iter().find(|c| c.id == id) {
            return Ok(c);
        }
        cursor = page.next_cursor;
        let Some(next) = &cursor else {
            return Err(Status::not_found("Comment is no longer part of this task."));
        };
        if !seen.insert(next.clone()) || seen.len() > 100 {
            return Err(domain("Could not verify comment membership."));
        }
    }
}
async fn youtrack_values(
    provider: &dyn i::TaskProvider,
    project: &i::ProjectTarget,
    task: Option<&i::TaskRef>,
    values: Value,
    create: bool,
) -> Result<Value, Status> {
    let mut schema = provider.youtrack_schema(project).await.map_err(domain)?;
    if let Some(task) = task
        && let Some(details) = provider.task(task).await.map_err(domain)?.youtrack
    {
        for f in &mut schema.fields {
            if let Some(c) = details.custom_fields.iter().find(|c| c.id == f.id) {
                *f = f.clone().with_current_value(c);
            }
        }
    }
    let mut result = serde_json::Map::new();
    for (id, value) in values
        .as_object()
        .ok_or_else(|| invalid("Fields must be an object."))?
    {
        let f = schema
            .fields
            .iter()
            .find(|f| &f.id == id)
            .ok_or_else(|| invalid("Unknown YouTrack field."))?;
        if f.read_only
            || f.kind == i::youtrack::YoutrackFieldKind::Unsupported
            || (!create && f.is_status())
        {
            return Err(invalid(
                "Read-only or status field cannot be saved in an edit form.",
            ));
        }
        if f.required && (value.is_null() || value.as_str() == Some("")) {
            return Err(invalid(format!("Required field: {}.", f.name)));
        }
        let native = if let Some(text) = value.as_str() {
            i::youtrack::value_for_field(f, text).map_err(invalid)?
        } else {
            value.clone()
        };
        result.insert(id.clone(), json!({"$type":f.field_type,"value":native}));
    }
    if create {
        for f in &schema.fields {
            if f.required && !result.contains_key(&f.id) && f.default.is_null() {
                return Err(invalid(format!("Required field: {}.", f.name)));
            }
        }
    }
    Ok(Value::Object(result))
}
