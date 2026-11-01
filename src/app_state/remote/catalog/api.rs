//! Standalone catalog contract over the existing projection and lifecycle owners.
mod lifecycle;
use super::*;
use canopy_desktop::{
    api::{decode, proto as p},
    command_bus::{CatalogCommand, Command, CommandBus, CommandEnvelope, Principal, ResourceRef},
    thread::{EnvironmentRef, HostId, ProjectRef, ThreadId, WorktreeId},
};

fn missing() -> Status {
    Status::invalid_argument("Required field is missing.")
}
fn environment(v: Option<p::EnvironmentRef>) -> Result<EnvironmentRef, Status> {
    Ok(EnvironmentRef {
        host_id: HostId::new(v.ok_or_else(missing)?.host_id).ok_or_else(missing)?,
    })
}
fn project(v: &p::ProjectRef) -> Result<ProjectRef, Status> {
    ProjectRef::new(
        HostId::new(v.host_id.clone()).ok_or_else(missing)?,
        serde_json::from_str(&v.project_id).map_err(|_| missing())?,
        v.worktree_id
            .clone()
            .map(|id| WorktreeId::new(id).ok_or_else(missing))
            .transpose()?,
        v.worktree_id.is_some(),
    )
    .map_err(|_| missing())
}
pub(super) fn project_wire(host: &str, w: &WorkspaceSummary) -> p::ProjectRef {
    p::ProjectRef {
        host_id: host.into(),
        project_id: serde_json::to_string(&w.project).expect("project identity"),
        worktree_id: w.repository.as_ref().map(|_| workspace_ref(w.workspace)),
    }
}
impl CatalogRpc {
    async fn api_authority<T>(&self, r: &Request<T>) -> Result<(DeviceGrant, Vec<u8>), Status> {
        let grant = self.auth.authenticate(r).await?;
        let credential = r
            .peer_certs()
            .and_then(|certs| certs.first().map(|c| c.to_vec()))
            .ok_or_else(|| Status::unauthenticated("A paired device certificate is required."))?;
        Ok((grant, credential))
    }
    async fn api_disclosure(&self, grant: &DeviceGrant, credential: &[u8]) -> Result<(), Status> {
        self.state.accepting()?;
        let current = self
            .auth
            .client()
            .lookup_runtime_device(credential.to_vec())
            .await
            .map_err(storage_status)?;
        self.auth.scope(&current, host::READ)?;
        if &current != grant {
            return Err(Status::aborted(
                "Catalog authorization changed; reconnect to reload.",
            ));
        }
        Ok(())
    }
    async fn api_workspace(
        &self,
        reference: p::ProjectRef,
        grant: &DeviceGrant,
        scope: u32,
    ) -> Result<(WorkspaceId, String), Status> {
        self.auth.check_host(&reference.host_id)?;
        self.auth.scope(grant, scope)?;
        let snapshot = self.snapshot().await?;
        let w = snapshot
            .workspaces
            .iter()
            .find(|w| {
                serde_json::to_string(&w.project).ok().as_deref()
                    == Some(reference.project_id.as_str())
                    && w.repository.as_ref().map(|_| workspace_ref(w.workspace))
                        == reference.worktree_id
            })
            .ok_or_else(|| Status::not_found("Project checkout is unavailable."))?;
        let metadata = canopy_desktop::state::projects::Project {
            id: w.project,
            workspace: w.workspace,
            name: w.name.clone(),
            path: w.canonical_root.clone(),
            worktree_path: None,
            repository_path: w.repository.clone(),
            worktree_base: None,
        };
        canopy_desktop::api::checked_project_ref(
            reference,
            &HostId::new(self.auth.host_id().into()).ok_or_else(missing)?,
            &metadata,
        )?;
        let root = WorkspaceRoots::root_string(&w.canonical_root)?.to_owned();
        self.auth.resource(grant.id(), scope, &root).await?;
        Ok((w.workspace, root))
    }
    async fn api_record(
        &self,
        thread: &p::ThreadRef,
        grant: &DeviceGrant,
        scope: u32,
    ) -> Result<ChatRecord, Status> {
        self.auth.check_host(&thread.host_id)?;
        self.auth.scope(grant, scope)?;
        let id = ThreadId::parse(&thread.thread_id).ok_or_else(missing)?;
        let record = self
            .auth
            .client()
            .load_runtime_thread(id)
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::not_found("Thread is unavailable."))?;
        self.auth
            .resource(grant.id(), scope, record.authority_root())
            .await?;
        Ok(record)
    }
    async fn catalog_receipt(
        &self,
        receipt: &host::CommandReceipt,
        resource: p::command_receipt::Resource,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let result =
            canopy_desktop::api::committed_receipt(&self.auth, receipt, Some(resource)).await?;
        Ok(Response::new(result))
    }
}

#[tonic::async_trait]
impl canopy_desktop::api::CatalogPublisher for CatalogRpc {
    async fn preflight_close_project(
        &self,
        r: Request<p::PreflightCloseProjectRequest>,
    ) -> Result<Response<p::ProjectClosePlan>, Status> {
        self.api_preflight_project(r).await
    }
    async fn preflight_close_terminal(
        &self,
        r: Request<p::PreflightCloseTerminalRequest>,
    ) -> Result<Response<p::TerminalClosePlan>, Status> {
        self.api_preflight_terminal(r).await
    }
    async fn close_project(
        &self,
        r: Request<p::CloseProjectRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.api_close_project(r).await
    }
    async fn close_terminal(
        &self,
        r: Request<p::CloseTerminalRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.api_close_terminal(r).await
    }
    async fn read_catalog(
        &self,
        r: Request<p::ReadCatalogRequest>,
    ) -> Result<Response<RpcStream<p::SummaryEvent>>, Status> {
        let (grant, credential) = self.api_authority(&r).await?;
        self.auth
            .check_host(environment(r.into_inner().environment)?.host_id.as_str())?;
        self.auth.scope(&grant, host::READ)?;
        let projection = self.project(&grant).await?;
        let boundary = p::Cursor {
            runtime_epoch: self.state.runtime_epoch(),
            stream_id: format!("catalog-read/{}", uuid::Uuid::new_v4()),
            sequence: 0,
        };
        Ok(Response::new(pages::read(
            self.clone(),
            credential,
            grant,
            pages::Pager::new(projection, boundary),
        )))
    }
    async fn list_resources(
        &self,
        r: Request<p::ListResourcesRequest>,
    ) -> Result<Response<p::Catalog>, Status> {
        let (grant, credential) = self.api_authority(&r).await?;
        self.auth
            .check_host(environment(r.into_inner().environment)?.host_id.as_str())?;
        self.auth.scope(&grant, host::READ)?;
        let result = self.project(&grant).await?.catalog.clone();
        self.api_disclosure(&grant, &credential).await?;
        if result.encoded_len() > host::CONTROL_BYTES {
            return Err(Status::resource_exhausted(
                "Use ReadCatalog for a paged catalog.",
            ));
        }
        Ok(Response::new(result))
    }
    async fn watch_summaries(
        &self,
        r: Request<p::WatchSummariesRequest>,
    ) -> Result<Response<RpcStream<p::SummaryEvent>>, Status> {
        let (grant, credential) = self.api_authority(&r).await?;
        self.auth
            .check_host(environment(r.into_inner().environment)?.host_id.as_str())?;
        self.auth.scope(&grant, host::READ)?;
        Ok(Response::new(
            watch::stream(self.clone(), credential).await?,
        ))
    }
    async fn list_agents(
        &self,
        r: Request<p::ListAgentsRequest>,
    ) -> Result<Response<p::AgentDiscovery>, Status> {
        self.discovery()?.list_agents(r).await
    }
    async fn get_agent_catalog(
        &self,
        r: Request<p::GetAgentCatalogRequest>,
    ) -> Result<Response<p::AgentCatalog>, Status> {
        self.discovery()?.agent_catalog(r).await
    }
    async fn list_pull_requests(
        &self,
        r: Request<p::ListPullRequestsRequest>,
    ) -> Result<Response<p::PullRequests>, Status> {
        self.discovery()?.pull_requests(r).await
    }
    async fn read_thread_page(
        &self,
        r: Request<p::ReadThreadPageRequest>,
    ) -> Result<Response<RpcStream<p::ArtifactChunk>>, Status> {
        self.discovery()?.read_page(r).await
    }
    async fn preflight_close(
        &self,
        r: Request<p::PreflightCloseRequest>,
    ) -> Result<Response<p::ClosePlan>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let thread = required(r.get_ref().thread.clone())?;
        let record = self.api_record(&thread, &grant, host::CHAT).await?;
        let preflight = self
            .bridge
            .delete_preflight(record.id().into())
            .await
            .map_err(bridge_status)?;
        let title = self
            .bridge
            .summaries()
            .await
            .map_err(bridge_status)?
            .into_iter()
            .find(|s| s.chat == record.id())
            .and_then(|s| s.title)
            .unwrap_or_else(|| "New chat".into());
        let risks = vec![p::CloseRisk {
            title,
            working: preflight.working,
            queued: preflight.unsent as u32,
            observers: preflight.observers as u32,
        }];
        let reason = preflight.blocked.clone();
        let plan_id = self.plans.lock().map_err(|_| poisoned())?.insert(
            grant.id(),
            PlanResource::Thread(record.id().into()),
            true,
            PlanKind::Chat(preflight),
        )?;
        self.auth
            .resource(grant.id(), host::CHAT, record.authority_root())
            .await?;
        Ok(Response::new(p::ClosePlan {
            thread: Some(thread),
            plan_id,
            risks,
            can_execute: reason.is_none(),
            reason: reason.unwrap_or_default(),
        }))
    }
    async fn create_thread(
        &self,
        r: Request<p::CreateThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.state.accepting()?;
        let r = r.into_inner();
        let meta = required(r.command)?;
        let resource = required(r.resource)?;
        let (target, wire) = match &resource {
            p::create_thread_request::Resource::Project(v) => (
                ResourceRef::Project(project(v)?),
                p::command_receipt::Resource::Project(v.clone()),
            ),
            p::create_thread_request::Resource::Environment(v) => (
                ResourceRef::Environment(environment(Some(v.clone()))?),
                p::command_receipt::Resource::Environment(v.clone()),
            ),
        };
        let envelope = decode::envelope(
            meta.clone(),
            target,
            Command::Catalog(CatalogCommand::Create {
                agent: r.agent_id.clone(),
                profile: r.profile_id.clone(),
            }),
        )?;
        let principal =
            Principal::authenticated(&grant, meta.command_epoch).map_err(storage_status)?;
        let rpc = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            let bus = CommandBus::new(rpc.auth.client().clone());
            if let Some(prepared) = bus
                .prepare_catalog(envelope.clone(), principal.clone(), None)
                .await
                .map_err(storage_status)?
            {
                return Ok::<_, Status>(prepared.prior().ok_or_else(missing)?.clone());
            }
            let (workspace, root) = match resource {
                p::create_thread_request::Resource::Project(v) => {
                    let (id, root) = rpc.api_workspace(v, &grant, host::CHAT).await?;
                    (Some(id), root)
                }
                p::create_thread_request::Resource::Environment(_) => (None, "private".into()),
            };
            // Deterministic identity includes the complete bus resource key.
            let provisional = bus
                .prepare_catalog(
                    envelope.clone(),
                    principal.clone(),
                    Some(("pending".into(), root.clone())),
                )
                .await
                .map_err(storage_status)?
                .ok_or_else(missing)?;
            let chat = resource_id(provisional.runtime_input().key(), "chat");
            let prepared = bus
                .prepare_catalog(envelope, principal, Some((chat.clone(), root)))
                .await
                .map_err(storage_status)?
                .ok_or_else(missing)?;
            if let Some(prior) = prepared.prior() {
                return Ok(prior.clone());
            }
            rpc.bridge
                .create(
                    RemoteKey::admitted(prepared.runtime_input().clone()),
                    NewChat {
                        chat,
                        workspace,
                        tool: r.agent_id,
                        profile: r.profile_id,
                        model: None,
                    },
                )
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Reconcile the original typed command."))??;
        self.catalog_receipt(&receipt, wire).await
    }
    async fn close_thread(
        &self,
        r: Request<p::CloseThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.state.accepting()?;
        let r = r.into_inner();
        let meta = required(r.command)?;
        let target = required(r.target)?;
        let envelope = decode::envelope(
            meta.clone(),
            decode::target(target.clone())?,
            Command::Catalog(CatalogCommand::Close {
                plan: r.plan_id.clone(),
            }),
        )?;
        let principal =
            Principal::authenticated(&grant, meta.command_epoch).map_err(storage_status)?;
        let rpc = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            let prepared = CommandBus::new(rpc.auth.client().clone())
                .prepare(envelope, principal)
                .await
                .map_err(storage_status)?;
            if let Some(prior) = prepared.prior() {
                return Ok::<_, Status>(prior.clone());
            }
            let input = prepared.runtime_input().clone();
            let resource = PlanResource::Thread(input.target().into());
            let PlanKind::Chat(preflight) = rpc.plans.lock().map_err(|_| poisoned())?.take(
                &r.plan_id,
                grant.id(),
                &resource,
                input.key().id(),
            )?
            else {
                return Err(Status::invalid_argument("The plan is not for a thread."));
            };
            rpc.bridge
                .delete(RemoteKey::admitted(input), preflight)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Reconcile the original typed command."))??;
        self.catalog_receipt(&receipt, p::command_receipt::Resource::Target(target))
            .await
    }
    async fn open_project(
        &self,
        r: Request<p::OpenProjectRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.state.accepting()?;
        let r = r.into_inner();
        let meta = required(r.command)?;
        let wire = required(r.environment)?;
        let envelope: CommandEnvelope = decode::envelope(
            meta.clone(),
            ResourceRef::Environment(environment(Some(wire.clone()))?),
            Command::Catalog(CatalogCommand::OpenProject {
                path: r.host_path.clone(),
            }),
        )?;
        let principal =
            Principal::authenticated(&grant, meta.command_epoch).map_err(storage_status)?;
        let rpc = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            let bus = CommandBus::new(rpc.auth.client().clone());
            if let Some(prepared) = bus
                .prepare_catalog(envelope.clone(), principal.clone(), None)
                .await
                .map_err(storage_status)?
            {
                return Ok::<_, Status>(prepared.prior().ok_or_else(missing)?.clone());
            }
            let path = PathBuf::from(r.host_path);
            let requested = path.clone();
            let folder = tokio::task::spawn_blocking(move || {
                canopy_desktop::git::workspace_directory(&requested)
            })
            .await
            .map_err(|_| Status::unavailable("Folder check stopped."))?
            .map_err(|_| Status::not_found("Folder is unavailable."))?;
            let root = WorkspaceRoots::root_string(&folder)?.to_owned();
            let prepared = bus
                .prepare_catalog(envelope, principal, Some((root.clone(), root)))
                .await
                .map_err(storage_status)?
                .ok_or_else(missing)?;
            if let Some(prior) = prepared.prior() {
                return Ok(prior.clone());
            }
            rpc.bridge
                .open_project(RemoteKey::admitted(prepared.runtime_input().clone()), path)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Reconcile the original typed command."))??;
        self.catalog_receipt(&receipt, p::command_receipt::Resource::Environment(wire))
            .await
    }
}
