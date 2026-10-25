//! Native Tasks RPCs. Provider credentials and accepted work remain owned by the host.
mod mapping;
mod reads;
mod transport;
mod writes;
use super::{
    auth::{RpcAuth, WorkspaceRoots, storage_status},
    proto,
    resources::ResourceLocks,
};
use crate::{
    artifacts::ArtifactStore,
    host::{CommandInput, CommandState, DeviceGrant, TASKS},
    integrations as i,
};
use gpui_kit::http_client::HttpClient;
use prost::Message;
use std::{future::Future, sync::Arc};
use tonic::{Request, Response, Status};

/// The live application owns configuration reconciliation and worktree lifecycle.
#[tonic::async_trait]
pub trait TasksHost: Send + Sync + 'static {
    async fn save_filter(&self, filter: i::filters::TaskFilter) -> Result<(), Status>;
    async fn remove_filter(&self, id: String) -> Result<(), Status>;
    async fn task_to_worktree(
        &self,
        workspace: proto::WorkspaceRef,
        task: i::TaskRef,
        branch: String,
    ) -> Result<String, Status>;
}
#[derive(Clone)]
pub struct TasksRpc {
    auth: RpcAuth,
    roots: WorkspaceRoots,
    locks: ResourceLocks,
    http: Arc<dyn HttpClient>,
    artifacts: Arc<ArtifactStore>,
    host: Arc<dyn TasksHost>,
}
impl TasksRpc {
    pub fn new(
        auth: RpcAuth,
        roots: WorkspaceRoots,
        locks: ResourceLocks,
        http: Arc<dyn HttpClient>,
        artifacts: Arc<ArtifactStore>,
        host: Arc<dyn TasksHost>,
    ) -> Self {
        Self {
            auth,
            roots,
            locks,
            http,
            artifacts,
            host,
        }
    }
    async fn authorize(
        &self,
        grant: &DeviceGrant,
        host: &str,
        workspace: Option<&proto::WorkspaceRef>,
    ) -> Result<String, Status> {
        self.authorize_scope(grant, host, workspace, TASKS).await
    }
    fn read_scope(grant: &DeviceGrant) -> u32 {
        if grant.scopes() & crate::host::READ != 0 {
            crate::host::READ
        } else {
            TASKS
        }
    }
    async fn authorize_read(
        &self,
        grant: &DeviceGrant,
        host: &str,
        workspace: Option<&proto::WorkspaceRef>,
    ) -> Result<String, Status> {
        self.authorize_scope(grant, host, workspace, Self::read_scope(grant))
            .await
    }
    async fn authorize_scope(
        &self,
        grant: &DeviceGrant,
        host: &str,
        workspace: Option<&proto::WorkspaceRef>,
        scope: u32,
    ) -> Result<String, Status> {
        self.auth.check_host(host)?;
        self.auth.scope(grant, scope)?;
        let root = match workspace {
            Some(w) => WorkspaceRoots::root_string(&self.roots.resolve(w, &self.auth)?)?.to_owned(),
            None => "private".into(),
        };
        self.auth.resource(grant.id(), scope, &root).await?;
        Ok(root)
    }
    async fn account(&self, id: &str) -> Result<i::Account, Status> {
        self.auth
            .client()
            .load_integrations()
            .await
            .map_err(storage_status)?
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| Status::not_found("Integration is no longer configured."))
    }
    async fn provider(
        &self,
        account: &i::Account,
        grant: &DeviceGrant,
        root: &str,
    ) -> Result<Box<dyn i::TaskProvider>, Status> {
        let key = account.credential.clone();
        let token = tokio::task::spawn_blocking(move || i::credentials::load(&key))
            .await
            .map_err(|_| Status::unavailable("Credential worker stopped."))?
            .map_err(|_| {
                Status::failed_precondition(
                    "needs_host_interaction: unlock or reconnect the integration on the host.",
                )
            })?;
        i::client(self.scoped_http(account, grant, root), account, token).map_err(domain)
    }
    async fn read_provider(
        &self,
        account: &i::Account,
        grant: &DeviceGrant,
        root: &str,
    ) -> Result<Box<dyn i::TaskProvider>, Status> {
        let key = account.credential.clone();
        let token = tokio::task::spawn_blocking(move || i::credentials::load(&key))
            .await
            .map_err(|_| Status::unavailable("Credential worker stopped."))?
            .map_err(|_| {
                Status::failed_precondition(
                    "needs_host_interaction: unlock or reconnect the integration on the host.",
                )
            })?;
        i::client(self.read_http(account, grant, root), account, token).map_err(domain)
    }
    async fn read_task_context(
        &self,
        grant: &DeviceGrant,
        reference: &proto::TaskRef,
    ) -> Result<(String, i::Account, i::TaskRef), Status> {
        let root = self
            .authorize_read(grant, &reference.host_id, reference.workspace.as_ref())
            .await?;
        let account = self.account(&reference.integration_id).await?;
        let project = Self::project(&account, &reference.project_id)?;
        Ok((root, account, Self::task(&project, reference)?))
    }
    fn project(account: &i::Account, id: &str) -> Result<i::ProjectTarget, Status> {
        let target = match &account.scope {
            i::AccountScope::Jira { site, .. } => i::ProjectTarget::jira(site, id),
            i::AccountScope::Youtrack { service } => i::ProjectTarget::youtrack(service, id),
            i::AccountScope::Default | i::AccountScope::Owner(_) => i::ProjectTarget::github(id),
        }
        .map_err(invalid)?;
        if target.provider != account.provider {
            return Err(invalid("Integration provider mismatch."));
        }
        if let i::AccountScope::Owner(owner) = &account.scope
            && !target
                .key
                .split('/')
                .next()
                .is_some_and(|v| v.eq_ignore_ascii_case(owner))
        {
            return Err(Status::permission_denied(
                "Repository is outside this integration's owner scope.",
            ));
        }
        Ok(target)
    }
    fn task(project: &i::ProjectTarget, task: &proto::TaskRef) -> Result<i::TaskRef, Status> {
        if task.project_id != project.key {
            return Err(invalid("Task belongs to a different project."));
        }
        let reference = i::TaskRef {
            project: project.clone(),
            id: task.task_id.clone(),
            title: String::new(),
        };
        if !reference.valid() {
            return Err(invalid("Invalid task reference."));
        }
        Ok(reference)
    }
    async fn task_context(
        &self,
        grant: &DeviceGrant,
        reference: &proto::TaskRef,
    ) -> Result<(String, i::Account, i::TaskRef), Status> {
        let root = self
            .authorize(grant, &reference.host_id, reference.workspace.as_ref())
            .await?;
        let account = self.account(&reference.integration_id).await?;
        let project = Self::project(&account, &reference.project_id)?;
        Ok((root, account, Self::task(&project, reference)?))
    }
    async fn mutation<F, Fut>(
        &self,
        input: CommandInput,
        effect: F,
    ) -> Result<proto::CommandReceipt, Status>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Vec<u8>, Status>> + Send + 'static,
    {
        let operation = self.locks.begin()?;
        let auth = self.auth.clone();
        let root = required(input.root.clone())?;
        let owner = input.key().device().to_owned();
        let scope = if input.method == "tasks.worktree" {
            TASKS | crate::host::FILES_GIT
        } else {
            TASKS
        };
        tokio::spawn(async move {
            let _operation = operation;
            let accepted = auth
                .client()
                .accept_runtime_command(input, scope)
                .await
                .map_err(storage_status)?;
            if accepted.state() != CommandState::Accepted {
                auth.resource(&owner, scope, &root).await?;
                return Ok(auth.receipt(&accepted));
            }
            // The CAS grants exactly one dispatch; storage checks current grants again.
            auth.client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .map_err(storage_status)?;
            let (state, result) = match effect().await {
                Ok(bytes) => (CommandState::Succeeded, bytes),
                Err(error) => {
                    let unknown = error.code() == tonic::Code::Unknown;
                    let code = if error.message().starts_with("needs_host_interaction") {
                        "needs_host_interaction"
                    } else if unknown {
                        "delivery_unknown"
                    } else {
                        "operation_failed"
                    };
                    (
                        if unknown {
                            CommandState::DeliveryUnknown
                        } else {
                            CommandState::Failed
                        },
                        proto::DomainError {
                            code: code.into(),
                            message: error.message().chars().take(8192).collect(),
                            retryable: false,
                        }
                        .encode_to_vec(),
                    )
                }
            };
            let receipt = auth
                .client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Attempting,
                    state,
                    Some(result),
                )
                .await
                .map_err(storage_status)?;
            auth.resource(&owner, scope, &root).await?;
            Ok(auth.receipt(&receipt))
        })
        .await
        .map_err(|_| Status::unavailable("Command outcome requires reconciliation."))?
    }
}
fn required<T>(value: Option<T>) -> Result<T, Status> {
    value.ok_or_else(|| invalid("Required field is missing."))
}
fn invalid(value: impl ToString) -> Status {
    Status::invalid_argument(value.to_string())
}
fn domain(value: impl ToString) -> Status {
    Status::failed_precondition(value.to_string())
}
fn write_error(value: i::WriteError) -> Status {
    match value {
        i::WriteError::Rejected(s) => domain(s),
        i::WriteError::Uncertain(s) => Status::unknown(s),
    }
}

#[tonic::async_trait]
impl proto::tasks_service_server::TasksService for TasksRpc {
    async fn list_integrations(
        &self,
        r: Request<proto::TaskIntegrationsRequest>,
    ) -> Result<Response<proto::TaskIntegrations>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let request = r.into_inner();
        let root = self
            .authorize_read(&grant, &request.host_id, request.workspace.as_ref())
            .await?;
        let config = self
            .auth
            .client()
            .load_integrations()
            .await
            .map_err(storage_status)?;
        self.auth
            .resource(grant.id(), Self::read_scope(&grant), &root)
            .await?;
        Ok(Response::new(proto::TaskIntegrations {
            integrations: config
                .accounts
                .into_iter()
                .map(|account| {
                    let scope = match &account.scope {
                        i::AccountScope::Default => String::new(),
                        i::AccountScope::Owner(owner) => owner.clone(),
                        i::AccountScope::Jira { site, .. } => site.clone(),
                        i::AccountScope::Youtrack { service } => service.clone(),
                    };
                    proto::TaskIntegration {
                        id: account.id,
                        name: if scope.is_empty() {
                            account.login
                        } else {
                            format!("{} · {scope}", account.login)
                        },
                        provider: mapping::provider(account.provider).into(),
                        configured: !account.credential.is_empty(),
                    }
                })
                .collect(),
        }))
    }
    type ReadAttachmentStream = std::pin::Pin<
        Box<dyn futures_lite::Stream<Item = Result<proto::ArtifactChunk, Status>> + Send>,
    >;
    async fn list_projects(
        &self,
        r: Request<proto::TaskProjectsRequest>,
    ) -> Result<Response<proto::TaskChoicePage>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.projects(g, r.into_inner()).await.map(Response::new)
    }
    async fn get_schema(
        &self,
        r: Request<proto::TaskSchemaRequest>,
    ) -> Result<Response<proto::TaskSchema>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.schema(g, r.into_inner()).await.map(Response::new)
    }
    async fn read_options(
        &self,
        r: Request<proto::TaskOptionsRequest>,
    ) -> Result<Response<proto::TaskChoicePage>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.options(g, r.into_inner()).await.map(Response::new)
    }
    async fn read_comments(
        &self,
        r: Request<proto::TaskCommentsRequest>,
    ) -> Result<Response<proto::TaskCommentPage>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.comments(g, r.into_inner()).await.map(Response::new)
    }
    async fn query_tasks(
        &self,
        r: Request<proto::QueryTasksRequest>,
    ) -> Result<Response<proto::TaskPage>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.query(g, r.into_inner()).await.map(Response::new)
    }
    async fn get_task(
        &self,
        r: Request<proto::GetTaskRequest>,
    ) -> Result<Response<proto::Task>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.get(g, r.into_inner()).await.map(Response::new)
    }
    async fn read_attachment(
        &self,
        r: Request<proto::TaskAttachmentRequest>,
    ) -> Result<Response<Self::ReadAttachmentStream>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.attachment(g, r.into_inner()).await.map(Response::new)
    }
    async fn apply_mutation(
        &self,
        r: Request<proto::TaskMutationRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.apply(g, r.into_inner()).await.map(Response::new)
    }
    async fn update_task(
        &self,
        r: Request<proto::UpdateTaskRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.update(g, r.into_inner()).await.map(Response::new)
    }
    async fn save_filter(
        &self,
        r: Request<proto::SaveTaskFilterRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.save(g, r.into_inner()).await.map(Response::new)
    }
    async fn remove_filter(
        &self,
        r: Request<proto::RemoveTaskFilterRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.remove(g, r.into_inner()).await.map(Response::new)
    }
    async fn task_to_worktree(
        &self,
        r: Request<proto::TaskToWorktreeRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.worktree(g, r.into_inner()).await.map(Response::new)
    }
}

#[cfg(test)]
mod tests;
