use super::*;
use mapping as m;
impl TasksRpc {
    pub(super) async fn projects(
        &self,
        g: DeviceGrant,
        r: proto::TaskProjectsRequest,
    ) -> Result<proto::TaskChoicePage, Status> {
        let root = self
            .authorize_read(&g, &r.host_id, r.workspace.as_ref())
            .await?;
        let account = self.account(&r.integration_id).await?;
        let provider = self.read_provider(&account, &g, &root).await?;
        let mut result = provider
            .projects(r.page_token.as_deref())
            .await
            .map_err(domain)?;
        if let i::AccountScope::Owner(owner) = &account.scope {
            result.items.retain(|o| {
                o.value
                    .split('/')
                    .next()
                    .is_some_and(|v| v.eq_ignore_ascii_case(owner))
            });
        }
        self.auth
            .resource(g.id(), Self::read_scope(&g), &root)
            .await?;
        Ok(m::options(result))
    }
    pub(super) async fn options(
        &self,
        g: DeviceGrant,
        r: proto::TaskOptionsRequest,
    ) -> Result<proto::TaskChoicePage, Status> {
        let p = required(r.project)?;
        let root = self
            .authorize_read(&g, &p.host_id, p.workspace.as_ref())
            .await?;
        let account = self.account(&p.integration_id).await?;
        let project = Self::project(&account, &p.project_id)?;
        let kind = match r.kind.as_str() {
            "label" => i::OptionKind::Label,
            "assignee" => i::OptionKind::Assignee,
            "milestone" => i::OptionKind::Milestone,
            _ => return Err(invalid("Unknown task option kind.")),
        };
        let provider = self.read_provider(&account, &g, &root).await?;
        let result = provider
            .options(&project, kind, r.page_token.as_deref())
            .await
            .map_err(domain)?;
        self.auth
            .resource(g.id(), Self::read_scope(&g), &root)
            .await?;
        Ok(m::options(result))
    }
    pub(super) async fn query(
        &self,
        g: DeviceGrant,
        r: proto::QueryTasksRequest,
    ) -> Result<proto::TaskPage, Status> {
        let root = self
            .authorize_read(&g, &r.host_id, r.workspace.as_ref())
            .await?;
        let account = self.account(&r.integration_id).await?;
        let project = Self::project(&account, &r.project_id)?;
        let config = self
            .auth
            .client()
            .load_integrations()
            .await
            .map_err(storage_status)?;
        let filter = if let Some(id) = &r.filter_id {
            let f = config
                .filter(id)
                .ok_or_else(|| Status::not_found("Task filter no longer exists."))?;
            if f.provider != project.provider {
                return Err(invalid("Filter belongs to another provider."));
            }
            Some(f.expression)
        } else {
            r.filter_expression
        };
        if let Some(f) = &filter {
            match project.provider {
                i::Provider::Jira => i::filters::validate_expression(f),
                i::Provider::Youtrack => i::filters::validate_youtrack_expression(f),
                i::Provider::Github => Err("GitHub does not support custom query filters.".into()),
            }
            .map_err(invalid)?;
        }
        if r.query.len() > 4096 {
            return Err(invalid("Task search exceeds 4096 bytes."));
        }
        let state = match r.state.as_str() {
            "" | "open" => i::TaskState::Open,
            "closed" => i::TaskState::Closed,
            _ => return Err(invalid("Unknown task state.")),
        };
        let provider = self.read_provider(&account, &g, &root).await?;
        let result = provider
            .list_context(
                &project,
                state,
                r.page_token.as_deref(),
                &i::TaskQuery {
                    text: r.query,
                    filter,
                    reconcile: vec![],
                },
            )
            .await
            .map_err(domain)?;
        self.auth
            .resource(g.id(), Self::read_scope(&g), &root)
            .await?;
        let reference = proto::TaskRef {
            host_id: r.host_id,
            integration_id: r.integration_id,
            project_id: r.project_id,
            workspace: r.workspace,
            task_id: String::new(),
        };
        Ok(proto::TaskPage {
            tasks: result
                .tasks
                .into_iter()
                .map(|t| m::task(t, reference.clone()))
                .collect(),
            next_page_token: result.next_cursor,
            total_count: result.total_count.map(|v| v as u64),
            filters: config
                .filters_for(project.provider)
                .into_iter()
                .map(m::filter)
                .collect(),
        })
    }
    pub(super) async fn get(
        &self,
        g: DeviceGrant,
        r: proto::GetTaskRequest,
    ) -> Result<proto::Task, Status> {
        let reference = required(r.task)?;
        let (root, account, task) = self.read_task_context(&g, &reference).await?;
        let provider = self.read_provider(&account, &g, &root).await?;
        let item = provider.task(&task).await.map_err(domain)?;
        if !item.reference.same_task(&task) {
            return Err(domain("Provider returned a different task."));
        }
        let statuses = i::status::load(provider.as_ref(), &task)
            .await
            .map_err(domain)?;
        self.auth
            .resource(g.id(), Self::read_scope(&g), &root)
            .await?;
        let mut result = m::task(item, reference);
        result.statuses = statuses.into_iter().map(m::status).collect();
        Ok(result)
    }
    pub(super) async fn comments(
        &self,
        g: DeviceGrant,
        r: proto::TaskCommentsRequest,
    ) -> Result<proto::TaskCommentPage, Status> {
        let reference = required(r.task)?;
        let (root, account, task) = self.read_task_context(&g, &reference).await?;
        let provider = self.read_provider(&account, &g, &root).await?;
        let page = provider
            .comments(&task, r.page_token.as_deref())
            .await
            .map_err(domain)?;
        self.auth
            .resource(g.id(), Self::read_scope(&g), &root)
            .await?;
        Ok(proto::TaskCommentPage {
            comments: page.comments.into_iter().map(m::comment).collect(),
            next_page_token: page.next_cursor,
            total_count: page.total_count as u64,
        })
    }
    pub(super) async fn schema(
        &self,
        g: DeviceGrant,
        r: proto::TaskSchemaRequest,
    ) -> Result<proto::TaskSchema, Status> {
        let p = required(r.project.clone())?;
        let root = self
            .authorize_read(&g, &p.host_id, p.workspace.as_ref())
            .await?;
        let account = self.account(&p.integration_id).await?;
        let project = Self::project(&account, &p.project_id)?;
        if let Some(t) = &r.task {
            check_reference(&p, t)?;
            Self::task(&project, t)?;
        }
        let provider = self.read_provider(&account, &g, &root).await?;
        let result = read_schema(provider.as_ref(), &project, &r).await?;
        self.auth
            .resource(g.id(), Self::read_scope(&g), &root)
            .await?;
        Ok(result)
    }
    pub(super) async fn attachment(
        &self,
        g: DeviceGrant,
        r: proto::TaskAttachmentRequest,
    ) -> Result<<Self as proto::tasks_service_server::TasksService>::ReadAttachmentStream, Status>
    {
        let reference = required(r.task)?;
        let (root, account, task) = self.read_task_context(&g, &reference).await?;
        let key = account.credential.clone();
        let token = tokio::task::spawn_blocking(move || i::credentials::load(&key))
            .await
            .map_err(|_| domain("Credential worker stopped."))?
            .map_err(|_| {
                domain("needs_host_interaction: unlock the integration credential on the host.")
            })?;
        let http = self.read_http(&account, &g, &root);
        let provider = i::client(http.clone(), &account, token.clone()).map_err(domain)?;
        let item = provider.task(&task).await.map_err(domain)?;
        let attachment = m::task(item.clone(), reference.clone())
            .task_attachments
            .into_iter()
            .find(|a| a.id == r.attachment_id);
        let media_type = attachment
            .as_ref()
            .map(|a| a.media_type.clone())
            .unwrap_or_else(|| "application/octet-stream".into());
        let bytes = match &account.scope {
            i::AccountScope::Jira {
                site,
                email,
                cloud_id,
            } => i::jira::Jira::new(
                http.clone(),
                site,
                email,
                cloud_id.as_deref(),
                token,
                account.login.clone(),
            )
            .map_err(domain)?
            .download_attachment(&task, &r.attachment_id)
            .await
            .map_err(domain)?,
            i::AccountScope::Youtrack { service } => {
                i::youtrack::Youtrack::new(http.clone(), service, token, account.login.clone())
                    .map_err(domain)?
                    .download_attachment(&task, &r.attachment_id)
                    .await
                    .map_err(domain)?
            }
            _ => {
                let mut found = i::task_context::github_assets(&item.body)
                    .map_err(domain)?
                    .contains(&r.attachment_id);
                let mut cursor = None;
                let mut seen = std::collections::HashSet::new();
                while !found {
                    let page = provider
                        .comments(&task, cursor.as_deref())
                        .await
                        .map_err(domain)?;
                    for c in page.comments {
                        found |= i::task_context::github_assets(&c.body)
                            .map_err(domain)?
                            .contains(&r.attachment_id);
                    }
                    cursor = page.next_cursor;
                    let Some(next) = &cursor else { break };
                    if !seen.insert(next.clone()) || seen.len() > 100 {
                        return Err(domain("Could not verify attachment membership."));
                    }
                }
                if !found {
                    return Err(Status::not_found("Attachment is not part of this task."));
                }
                i::task_context::github_download(&http, &r.attachment_id, &token)
                    .await
                    .map_err(domain)?
            }
        };
        if bytes.len() > 25 * 1024 * 1024 {
            return Err(Status::resource_exhausted("Attachment exceeds 25 MiB."));
        }
        self.auth
            .resource(g.id(), Self::read_scope(&g), &root)
            .await?;
        let target = crate::artifacts::Target::Task {
            integration_id: reference.integration_id,
            project_id: reference.project_id,
            task_id: reference.task_id,
            root: root.clone(),
        };
        let artifact = self
            .publish_attachment(g.id().to_owned(), target, media_type, bytes)
            .await?;
        let auth = self.auth.clone();
        let owner = g.id().to_owned();
        let read_scope = Self::read_scope(&g);
        let store = self.artifacts.clone();
        Ok(Box::pin(futures_lite::stream::unfold(
            (0u64, false),
            move |(offset, done)| {
                let auth = auth.clone();
                let owner = owner.clone();
                let root = root.clone();
                let store = store.clone();
                let artifact = artifact.clone();
                async move {
                    if done {
                        return None;
                    }
                    if let Err(error) = auth.resource(&owner, read_scope, &root).await {
                        return Some((Err(error), (offset, true)));
                    }
                    let id = artifact.artifact_id.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        store.download(&owner, &id, offset, crate::artifacts::CHUNK_BYTES)
                    })
                    .await;
                    match result {
                        Ok(Ok(data)) => {
                            let next = offset + data.len() as u64;
                            let complete = next == artifact.size;
                            Some((
                                Ok(proto::ArtifactChunk {
                                    artifact: Some(artifact),
                                    offset,
                                    data,
                                    complete,
                                }),
                                (next, complete),
                            ))
                        }
                        Ok(Err(e)) => Some((Err(storage_status(e)), (offset, true))),
                        Err(_) => Some((
                            Err(Status::unavailable("Attachment worker stopped.")),
                            (offset, true),
                        )),
                    }
                }
            },
        )))
    }
    pub(super) async fn publish_attachment(
        &self,
        owner: String,
        target: crate::artifacts::Target,
        media_type: String,
        bytes: Vec<u8>,
    ) -> Result<proto::ArtifactRef, Status> {
        use sha2::{Digest, Sha256};
        let store = self.artifacts.clone();
        let host = self.auth.host_id().to_owned();
        tokio::task::spawn_blocking(move || {
            let binding = crate::artifacts::BeginUpload::new(
                owner.clone(),
                target,
                crate::artifacts::Purpose::Result,
                bytes.len() as u64,
                Sha256::digest(&bytes).into(),
                media_type,
            )
            .map_err(storage_status)?;
            let upload = store.begin(binding).map_err(storage_status)?;
            let publish = || -> Result<crate::artifacts::Artifact, Status> {
                for (ix, chunk) in bytes.chunks(crate::artifacts::CHUNK_BYTES).enumerate() {
                    store
                        .put_chunk(
                            &owner,
                            upload.id(),
                            (ix * crate::artifacts::CHUNK_BYTES) as u64,
                            chunk,
                        )
                        .map_err(storage_status)?;
                }
                store.finish(&owner, upload.id()).map_err(storage_status)
            };
            match publish() {
                Ok(a) => Ok(proto::ArtifactRef {
                    host_id: host,
                    artifact_id: a.id().into(),
                    media_type: a.media_type().into(),
                    size: a.size(),
                    sha256: a.sha256().to_vec(),
                }),
                Err(e) => {
                    let _ = store.cancel(&owner, upload.id());
                    Err(e)
                }
            }
        })
        .await
        .map_err(|_| Status::unavailable("Attachment worker stopped."))?
    }
}
pub(super) fn check_reference(p: &proto::TaskProject, t: &proto::TaskRef) -> Result<(), Status> {
    if p.host_id != t.host_id
        || p.integration_id != t.integration_id
        || p.project_id != t.project_id
        || p.workspace != t.workspace
    {
        return Err(invalid("Task and project context do not match."));
    }
    Ok(())
}
pub(super) async fn read_schema(
    provider: &dyn i::TaskProvider,
    project: &i::ProjectTarget,
    r: &proto::TaskSchemaRequest,
) -> Result<proto::TaskSchema, Status> {
    use proto::TaskSchemaPurpose as P;
    let purpose = P::try_from(r.purpose).map_err(|_| invalid("Unknown schema purpose."))?;
    let key = || {
        r.task
            .as_ref()
            .map(|t| t.task_id.clone())
            .ok_or_else(|| invalid("Task is required for this schema."))
    };
    if project.provider == i::Provider::Jira {
        use i::jira::SchemaRequest as S;
        let request = match purpose {
            P::TaskCreate => S::Create {
                issue_type: required(r.issue_type_id.clone())?,
            },
            P::TaskEdit => S::Edit { key: key()? },
            P::TaskTransitions => S::Transitions { key: key()? },
            P::TaskTypes => S::Types,
            P::TaskLinkTypes => S::LinkTypes,
            P::TaskBoards => S::Boards {
                cursor: r.page_token.clone(),
            },
            P::TaskSprints => S::Sprints {
                board: required(r.board_id.clone())?,
                cursor: r.page_token.clone(),
            },
            P::TaskHistory => S::History {
                key: key()?,
                cursor: r.page_token.clone(),
            },
            _ => return Err(invalid("Choose a schema purpose.")),
        };
        let schema = provider.schema(project, &request).await.map_err(domain)?;
        let mut fields: Vec<_> = schema.fields.into_iter().map(m::jira_field).collect();
        if purpose == P::TaskEdit {
            let current = provider
                .task(&TasksRpc::task(project, required(r.task.as_ref())?)?)
                .await
                .map_err(domain)?;
            if let Some(details) = current.jira {
                for f in &mut fields {
                    f.current_value = details.fields.get(&f.id).map(m::value);
                }
            }
        }
        if purpose == P::TaskCreate {
            fields.retain(|f| {
                !matches!(
                    f.id.as_str(),
                    "issuetype" | "summary" | "description" | "project"
                )
            });
            fields.push(proto::TaskField {
                id: "issuetype".into(),
                name: "Issue type".into(),
                kind: "object".into(),
                required: true,
                default_value: Some(m::value(&serde_json::json!({"id":r.issue_type_id}))),
                ..Default::default()
            });
        }
        Ok(proto::TaskSchema {
            fields,
            transitions: schema
                .transitions
                .into_iter()
                .map(|t| proto::TaskTransition {
                    id: t.id,
                    name: t.name,
                    destination: t.destination,
                    required_fields: t.fields.into_iter().map(m::jira_field).collect(),
                })
                .collect(),
            choices: schema.options.into_iter().map(m::option).collect(),
            activity: schema.activity,
            next_page_token: schema.next_cursor,
        })
    } else if project.provider == i::Provider::Youtrack {
        if !matches!(
            purpose,
            P::TaskCreate | P::TaskEdit | P::TaskTransitions | P::TaskTypes
        ) {
            return Err(Status::unimplemented(
                "This YouTrack adapter does not support that schema.",
            ));
        }
        let mut schema = provider.youtrack_schema(project).await.map_err(domain)?;
        if let Some(t) = &r.task {
            let reference = TasksRpc::task(project, t)?;
            let current = provider.task(&reference).await.map_err(domain)?;
            if let Some(details) = current.youtrack {
                for f in &mut schema.fields {
                    if let Some(c) = details.custom_fields.iter().find(|c| c.id == f.id) {
                        *f = f.clone().with_current_value(c);
                    }
                }
            }
        }
        if purpose == P::TaskEdit {
            schema.fields.retain(|f| !f.is_status());
        }
        if purpose == P::TaskTransitions {
            schema.fields.retain(|f| f.is_status());
        }
        Ok(proto::TaskSchema {
            fields: schema.fields.into_iter().map(m::youtrack_field).collect(),
            next_page_token: schema.next_cursor,
            ..Default::default()
        })
    } else if matches!(
        purpose,
        P::TaskCreate | P::TaskEdit | P::TaskTypes | P::TaskTransitions
    ) {
        Ok(proto::TaskSchema::default())
    } else {
        Err(Status::unimplemented(
            "This GitHub adapter does not support that schema.",
        ))
    }
}
