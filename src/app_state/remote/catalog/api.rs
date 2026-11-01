//! Standalone catalog contract over the existing projection and lifecycle owners.
use super::*;
use canopy_desktop::{
    api::{compat, decode, projection::value, proto as p},
    command_bus::{CatalogCommand, Command, CommandBus, CommandEnvelope, Principal, ResourceRef},
    thread::{EnvironmentRef, HostId, ProjectRef, ThreadId, WorktreeId},
};
use futures_lite::StreamExt;
use proto::catalog_service_server::CatalogService as LegacyCatalog;

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
fn project_wire(host: &str, w: &WorkspaceSummary) -> p::ProjectRef {
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
    /// Agent discovery accepts an environment plus an optional readable project.
    async fn api_discovery<T>(
        &self,
        r: &Request<T>,
        env: Option<p::EnvironmentRef>,
        project: Option<p::ProjectRef>,
    ) -> Result<(String, Option<proto::WorkspaceRef>), Status> {
        let grant = self.auth.authenticate(r).await?;
        let host = environment(env)?.host_id.to_string();
        self.auth.check_host(&host)?;
        let workspace = match project {
            Some(p) => Some(proto::WorkspaceRef {
                host_id: host.clone(),
                workspace_id: workspace_ref(self.api_workspace(p, &grant, host::READ).await?.0),
            }),
            None => None,
        };
        Ok((host, workspace))
    }
    async fn api_alias(
        &self,
        catalog_id: String,
        absent: &'static str,
    ) -> Result<ThreadId, Status> {
        self.auth
            .client()
            .runtime_thread_alias(
                HostId::new(self.auth.host_id().into()).ok_or_else(missing)?,
                catalog_id,
            )
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::internal(absent))
    }
    async fn api_thread(&self, summary: proto::ChatSummary) -> Result<p::CatalogThread, Status> {
        let chat = required(summary.chat.clone())?;
        let id = self
            .api_alias(chat.chat_id.clone(), "Missing catalog thread identity.")
            .await?;
        let thread = p::ThreadRef {
            host_id: chat.host_id,
            thread_id: id.to_string(),
        };
        Ok(p::CatalogThread {
            thread: Some(thread.clone()),
            catalog_id: chat.chat_id,
            project_catalog_id: summary.workspace.map(|w| w.workspace_id),
            summary: Some(p::ThreadStatus {
                thread: Some(thread),
                title: summary.title,
                agent_id: summary.agent_id,
                profile_id: summary.profile_id,
                state: summary.state,
                pending_decisions: summary.pending_decisions,
                queued_messages: summary.queued_messages,
                run_id: summary
                    .run_id
                    .map(|id| {
                        id.parse()
                            .map_err(|_| Status::internal("Invalid run identity."))
                    })
                    .transpose()?,
                history_epoch: summary.history_epoch,
                goal: summary.goal.map(value),
            }),
        })
    }
    async fn api_catalog(&self, catalog: proto::Catalog) -> Result<p::Catalog, Status> {
        let mut projects = Vec::new();
        if !catalog.projects.is_empty() {
            let snapshot = self.snapshot().await?;
            for v in catalog.projects {
                let mut checkouts = Vec::new();
                for w in v.checkouts {
                    let id = required(w.workspace)?.workspace_id;
                    let metadata = snapshot
                        .workspaces
                        .iter()
                        .find(|w| workspace_ref(w.workspace) == id)
                        .ok_or_else(|| Status::aborted("Catalog changed; read it again."))?;
                    checkouts.push(p::ProjectCheckout {
                        project: Some(project_wire(self.auth.host_id(), metadata)),
                        catalog_id: id,
                        name: w.name,
                        path: w.path,
                        closing: w.closing.unwrap_or_default(),
                        branch: w.branch,
                        repository_path: w.repository_path,
                    });
                }
                projects.push(p::ProjectSummary {
                    name: v.name,
                    path: v.path,
                    origin: v.origin,
                    checkouts,
                });
            }
        }
        let mut threads = Vec::new();
        for v in catalog.chats {
            threads.push(self.api_thread(v).await?);
        }
        Ok(p::Catalog {
            environment: Some(p::EnvironmentRef {
                host_id: catalog.host_id,
            }),
            projects,
            threads,
            terminals: catalog
                .terminals
                .into_iter()
                .map(canopy_desktop::api::catalog_projection::terminal)
                .collect(),
        })
    }
    async fn api_event(&self, event: proto::SummaryEvent) -> Result<p::SummaryEvent, Status> {
        use p::summary_event::Change as N;
        use proto::summary_event::Change as O;
        let change = match required(event.change)? {
            O::Snapshot(v) => N::Snapshot(self.api_catalog(v).await?),
            O::Chat(v) => N::Thread(Box::new(self.api_thread(v).await?)),
            O::Terminal(v) => N::Terminal(canopy_desktop::api::catalog_projection::terminal(v)),
            O::SnapshotPart(v) => N::SnapshotPart(p::CatalogSnapshotPart {
                boundary: v.boundary.map(value),
                page: Some(self.api_catalog(required(v.page)?).await?),
                first: v.first,
                last: v.last,
            }),
            O::Deleted(v) => N::Deleted(p::CatalogDeleted {
                resource: Some(match required(v.resource)? {
                    proto::resource_ref::Resource::Chat(v) => {
                        p::catalog_deleted::Resource::ThreadCatalogId(v.chat_id)
                    }
                    proto::resource_ref::Resource::Terminal(v) => {
                        p::catalog_deleted::Resource::Terminal(value(v))
                    }
                    _ => return Err(Status::aborted("Catalog changed; read it again.")),
                }),
            }),
        };
        let result = p::SummaryEvent {
            cursor: event.cursor.map(value),
            change: Some(change),
        };
        if result.encoded_len() > host::CONTROL_BYTES {
            return Err(Status::resource_exhausted(
                "Catalog page exceeds the control budget.",
            ));
        }
        Ok(result)
    }
    fn api_stream(
        &self,
        stream: RpcStream<proto::SummaryEvent>,
        grant: DeviceGrant,
        credential: Vec<u8>,
    ) -> RpcStream<p::SummaryEvent> {
        Box::pin(futures_lite::stream::try_unfold(
            (self.clone(), stream, grant, credential),
            |(rpc, mut stream, grant, credential)| async move {
                match stream.next().await {
                    Some(v) => {
                        let event = rpc.api_event(v?).await?;
                        // Identity conversion awaits owner/DB reads. Nothing
                        // projected for an older grant may leave afterwards.
                        rpc.api_disclosure(&grant, &credential).await?;
                        Ok(Some((event, (rpc, stream, grant, credential))))
                    }
                    None => Ok(None),
                }
            },
        ))
    }
    async fn catalog_receipt(
        &self,
        receipt: &host::CommandReceipt,
        resource: p::command_receipt::Resource,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let mut result = compat::receipt(&self.auth, receipt, resource)?;
        if receipt.state() == CommandState::Succeeded && receipt.method() == "chat.create" {
            // The owner's target is the durable created catalog key even if runtime
            // startup returned a warning rather than the key in its result bytes.
            let id = receipt.target().to_owned();
            let thread = self
                .api_alias(id.clone(), "Missing created thread identity.")
                .await?;
            result.result = Some(p::command_receipt::Result::Runtime(p::RuntimeResult {
                created_target: Some(p::ThreadTarget {
                    thread: Some(p::ThreadRef {
                        host_id: self.auth.host_id().into(),
                        thread_id: thread.to_string(),
                    }),
                    executor_epoch: 0,
                }),
                catalog_id: Some(id),
                item: None,
            }));
        } else if receipt.state() == CommandState::Succeeded && receipt.method() == "projects.open"
        {
            let workspace: WorkspaceId =
                serde_json::from_slice(receipt.result().unwrap_or_default())
                    .map_err(|_| Status::internal("Invalid committed project identity."))?;
            result.result = Some(p::command_receipt::Result::Runtime(p::RuntimeResult {
                created_target: None,
                catalog_id: Some(workspace_ref(workspace)),
                item: None,
            }));
        }
        Ok(Response::new(result))
    }
}

#[tonic::async_trait]
impl canopy_desktop::api::CatalogPublisher for CatalogRpc {
    async fn read_catalog(
        &self,
        r: Request<p::ReadCatalogRequest>,
    ) -> Result<Response<RpcStream<p::SummaryEvent>>, Status> {
        let (grant, credential) = self.api_authority(&r).await?;
        let (metadata, extensions, r) = r.into_parts();
        let host = environment(r.environment)?.host_id.to_string();
        let stream = LegacyCatalog::read_catalog(
            self,
            Request::from_parts(metadata, extensions, proto::HostRequest { host_id: host }),
        )
        .await?
        .into_inner();
        Ok(Response::new(self.api_stream(stream, grant, credential)))
    }
    async fn list_resources(
        &self,
        r: Request<p::ListResourcesRequest>,
    ) -> Result<Response<p::Catalog>, Status> {
        let (grant, credential) = self.api_authority(&r).await?;
        let (metadata, extensions, r) = r.into_parts();
        let catalog = LegacyCatalog::list_resources(
            self,
            Request::from_parts(
                metadata,
                extensions,
                proto::HostRequest {
                    host_id: environment(r.environment)?.host_id.to_string(),
                },
            ),
        )
        .await?
        .into_inner();
        let result = self.api_catalog(catalog).await?;
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
        let (metadata, extensions, r) = r.into_parts();
        let stream = LegacyCatalog::watch_summaries(
            self,
            Request::from_parts(
                metadata,
                extensions,
                proto::SubscribeRequest {
                    host_id: environment(r.environment)?.host_id.to_string(),
                    since: r.since.map(value),
                },
            ),
        )
        .await?
        .into_inner();
        Ok(Response::new(self.api_stream(stream, grant, credential)))
    }
    async fn list_agents(
        &self,
        r: Request<p::ListAgentsRequest>,
    ) -> Result<Response<p::AgentDiscovery>, Status> {
        let (host, workspace) = self
            .api_discovery(
                &r,
                r.get_ref().environment.clone(),
                r.get_ref().project.clone(),
            )
            .await?;
        let (metadata, extensions, r) = r.into_parts();
        Ok(Response::new(value(
            self.discovery()?
                .list_agents(Request::from_parts(
                    metadata,
                    extensions,
                    proto::AgentDiscoveryRequest {
                        host_id: host,
                        workspace,
                        page_token: r.page_token,
                    },
                ))
                .await?
                .into_inner(),
        )))
    }
    async fn get_agent_catalog(
        &self,
        r: Request<p::GetAgentCatalogRequest>,
    ) -> Result<Response<p::AgentCatalog>, Status> {
        let (host, workspace) = self
            .api_discovery(
                &r,
                r.get_ref().environment.clone(),
                r.get_ref().project.clone(),
            )
            .await?;
        let (metadata, extensions, r) = r.into_parts();
        Ok(Response::new(value(
            self.discovery()?
                .agent_catalog(Request::from_parts(
                    metadata,
                    extensions,
                    proto::AgentCatalogRequest {
                        host_id: host,
                        workspace,
                        agent_id: r.agent_id,
                        account_id: r.account_id,
                        refresh: r.refresh,
                    },
                ))
                .await?
                .into_inner(),
        )))
    }
    async fn list_pull_requests(
        &self,
        r: Request<p::ListPullRequestsRequest>,
    ) -> Result<Response<p::PullRequests>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let (workspace, _) = self
            .api_workspace(required(r.get_ref().project.clone())?, &grant, host::READ)
            .await?;
        let (metadata, extensions, r) = r.into_parts();
        Ok(Response::new(value(
            self.discovery()?
                .pull_requests(Request::from_parts(
                    metadata,
                    extensions,
                    proto::PullRequestsRequest {
                        workspace: Some(proto::WorkspaceRef {
                            host_id: self.auth.host_id().into(),
                            workspace_id: workspace_ref(workspace),
                        }),
                        query: r.query,
                        page_token: r.page_token,
                    },
                ))
                .await?
                .into_inner(),
        )))
    }
    async fn read_thread_page(
        &self,
        r: Request<p::ReadThreadPageRequest>,
    ) -> Result<Response<RpcStream<p::ArtifactChunk>>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let record = self
            .api_record(&required(r.get_ref().thread.clone())?, &grant, host::READ)
            .await?;
        let (metadata, extensions, r) = r.into_parts();
        let stream = self
            .discovery()?
            .read_page(Request::from_parts(
                metadata,
                extensions,
                proto::ChatPageRequest {
                    chat: Some(reference(&self.auth, record.id())),
                    item: r.item.map(value),
                },
            ))
            .await?
            .into_inner();
        Ok(Response::new(Box::pin(stream.map(|r| r.map(value)))))
    }
    async fn preflight_close(
        &self,
        r: Request<p::PreflightCloseRequest>,
    ) -> Result<Response<p::ClosePlan>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let thread = required(r.get_ref().thread.clone())?;
        let record = self.api_record(&thread, &grant, host::CHAT).await?;
        let (metadata, extensions, _) = r.into_parts();
        let plan = LegacyCatalog::preflight_close(
            self,
            Request::from_parts(
                metadata,
                extensions,
                proto::PreflightCloseRequest {
                    resource: Some(proto::ResourceRef {
                        resource: Some(proto::resource_ref::Resource::Chat(reference(
                            &self.auth,
                            record.id(),
                        ))),
                    }),
                    delete: true,
                },
            ),
        )
        .await?
        .into_inner();
        Ok(Response::new(p::ClosePlan {
            thread: Some(thread),
            plan_id: plan.plan_id,
            risks: plan
                .risks
                .into_iter()
                .map(|v| p::CloseRisk {
                    title: v.title,
                    working: v.working,
                    queued: v.queued,
                    observers: v.observers,
                })
                .collect(),
            can_execute: plan.can_execute,
            reason: plan.reason,
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
            let resource = proto::ResourceRef {
                resource: Some(proto::resource_ref::Resource::Chat(reference(
                    &rpc.auth,
                    input.target(),
                ))),
            };
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
