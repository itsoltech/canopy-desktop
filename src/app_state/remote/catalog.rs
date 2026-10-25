//! Native CatalogService: the host's projects, checkouts, chats and terminals for a
//! verified device, filtered to its grants, plus the catalog mutations. Every mutation
//! goes through the same app handlers as the desktop (via the chat and terminal
//! bridges) with the durable command states of §8; close/delete first needs a
//! preflight plan bound to the device, the resource and a short TTL, and the owner
//! checks its conditions again right before the effect. A retried command ID returns
//! its recorded state before anything about its target is looked up, so a retry after
//! the target is gone (or its plan was consumed) never repeats an effect or reports a
//! misleading not-found.
mod pages;
mod plans;
#[cfg(test)]
mod tests;
mod watch;

use super::chat::{
    ChatsRpc, bridge_status, chat_state, reference, required, resource_id, wire_fingerprint,
};
use crate::app_state::{
    chat_bridge::{CatalogEvent, ChatBridge},
    chats::{ChatCommand, NewChat, RemoteKey},
    project_commands::WorkspaceSummary,
    terminal_bridge::{TerminalBridge, workspace_ref},
};
use canopy_desktop::{
    host::{self, ChatRecord, CommandKey, CommandState, DeviceGrant},
    remote::{
        auth::{RpcAuth, WorkspaceRoots, storage_status},
        host::{HostState, RpcStream},
        proto,
        resources::ResourceLocks,
        terminal_rpc::{TerminalHost, TerminalInfo},
    },
    state::workspace::WorkspaceId,
};
use plans::{PlanKind, Plans};
use prost::Message;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct CatalogRpc {
    auth: RpcAuth,
    bridge: ChatBridge,
    roots: WorkspaceRoots,
    terminals: TerminalBridge,
    chats: ChatsRpc,
    state: HostState,
    operations: ResourceLocks,
    plans: Arc<Mutex<Plans>>,
    discovery: Option<canopy_desktop::remote::discovery::Discovery>,
    /// The fanout and assembly budgets shared with chat and terminal streams.
    output: canopy_desktop::remote::sync::ReplayHub,
}

impl CatalogRpc {
    pub fn new(
        auth: RpcAuth,
        bridge: ChatBridge,
        roots: WorkspaceRoots,
        terminals: TerminalBridge,
        chats: ChatsRpc,
        state: HostState,
        operations: ResourceLocks,
    ) -> Self {
        Self {
            auth,
            bridge,
            roots,
            terminals,
            chats,
            state,
            operations,
            plans: Arc::default(),
            discovery: None,
            output: canopy_desktop::remote::sync::ReplayHub::new(Default::default()),
        }
    }
    /// Charges catalog projections and pages to the host's shared stream budgets.
    pub fn with_output_budget(mut self, output: canopy_desktop::remote::sync::ReplayHub) -> Self {
        self.output = output;
        self
    }
    pub fn with_discovery(
        mut self,
        discovery: canopy_desktop::remote::discovery::Discovery,
    ) -> Self {
        self.discovery = Some(discovery);
        self
    }
    fn discovery(&self) -> Result<&canopy_desktop::remote::discovery::Discovery, Status> {
        self.discovery
            .as_ref()
            .ok_or_else(|| Status::unavailable("Host discovery is starting."))
    }

    async fn grant<T>(&self, request: &Request<T>, host: &str) -> Result<DeviceGrant, Status> {
        let grant = self.auth.authenticate(request).await?;
        self.auth.check_host(host)?;
        Ok(grant)
    }

    /// The recorded outcome of `key` when it was already accepted: a retry answers it
    /// without resolving its target again (CMD-02). The same ID with another method,
    /// target (`expected`, when the request names it) or request (`fingerprint`, see
    /// `wire_fingerprint`) is `idempotency_conflict`, never the old receipt.
    /// Authorization of the saved command is rechecked by the store (CMD-03).
    async fn prior(
        &self,
        key: &CommandKey,
        scope: u32,
        method: &str,
        expected: Option<&str>,
        fingerprint: &[u8],
    ) -> Result<Option<proto::CommandReceipt>, Status> {
        let client = self.auth.client();
        let Some(saved) = client
            .get_runtime_command(key.clone(), scope)
            .await
            .map_err(storage_status)?
        else {
            return Ok(None);
        };
        if saved.method() != method || expected.is_some_and(|t| t != saved.target()) {
            return Err(conflict());
        }
        match client
            .get_runtime_command_matching(
                key.clone(),
                scope,
                method.into(),
                saved.target().into(),
                fingerprint.to_vec(),
            )
            .await
        {
            Ok(receipt) => Ok(receipt.map(|r| self.receipt(&r))),
            Err(canopy_desktop::settings::Error::CommandConflict) => Err(conflict()),
            Err(error) => Err(storage_status(error)),
        }
    }

    fn receipt(&self, receipt: &host::CommandReceipt) -> proto::CommandReceipt {
        self.auth.receipt(receipt)
    }

    /// Runs `effect` as an owned operation that outlives the RPC (RPC-03).
    async fn owned<F>(&self, effect: F) -> Result<proto::CommandReceipt, Status>
    where
        F: std::future::Future<Output = Result<host::CommandReceipt, Status>> + Send + 'static,
    {
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            effect.await
        })
        .await
        .map_err(|_| Status::unknown("The outcome is uncertain; reconcile its command ID."))??;
        Ok(self.receipt(&receipt))
    }

    /// The workspace `reference` names, authorized for `scope`: its catalog ID and
    /// canonical root. Only IDs the host listed are parsed.
    async fn workspace(
        &self,
        grant: &DeviceGrant,
        reference: &proto::WorkspaceRef,
        scope: u32,
    ) -> Result<(WorkspaceId, PathBuf, String), Status> {
        let root = self.roots.resolve(reference, &self.auth)?;
        let text = WorkspaceRoots::root_string(&root)?.to_owned();
        self.auth.resource(grant.id(), scope, &text).await?;
        let id = serde_json::from_str(&reference.workspace_id)
            .map_err(|_| Status::not_found("Workspace is no longer available."))?;
        Ok((id, root, text))
    }

    async fn record(&self, chat: &str) -> Result<ChatRecord, Status> {
        self.auth
            .client()
            .load_runtime_chats()
            .await
            .map_err(storage_status)?
            .into_iter()
            .find(|r| r.id() == chat)
            .ok_or_else(|| Status::not_found("Chat is no longer available."))
    }

    async fn terminal(&self, id: &str) -> Result<TerminalInfo, Status> {
        self.terminals
            .terminal(id)
            .await?
            .ok_or_else(|| Status::not_found("The terminal is no longer available."))
    }
}

impl CatalogRpc {
    /// The app's current catalog: the `Arc` every catalog stream shares, not a
    /// reload per request.
    async fn snapshot(&self) -> Result<Arc<CatalogEvent>, Status> {
        let watch = self.bridge.watch_catalog().await.map_err(bridge_status)?;
        let event = watch
            .recv()
            .await
            .map_err(|_| Status::unavailable("Canopy is closing."))?;
        let _ = self.roots.replace(root_map(&event.workspaces));
        Ok(event)
    }

    /// The whole catalog as `grant` may see it now, terminals included, charged to
    /// the assembly budget before it is built.
    async fn project(&self, grant: &DeviceGrant) -> Result<Arc<pages::Projection>, Status> {
        let (event, terminals) = tokio::join!(self.snapshot(), self.terminals.terminals());
        let (event, terminals) = (event?, terminals?);
        pages::Projection::build(&self.output, &self.auth, grant, &event, &terminals)
    }
}

fn conflict() -> Status {
    Status::aborted("idempotency_conflict: this command ID was used for another operation.")
}

/// Workspace ID (as listed) to canonical root.
fn root_map(workspaces: &[WorkspaceSummary]) -> HashMap<String, PathBuf> {
    workspaces
        .iter()
        .map(|w| (workspace_ref(w.workspace), w.canonical_root.clone()))
        .collect()
}

/// Whether `grant` may read resources under authority `root` ("private" included).
fn visible(grant: &DeviceGrant, root: &str) -> bool {
    !grant.revoked()
        && grant.scopes() & host::READ == host::READ
        && !root.is_empty()
        && grant.roots().iter().any(|r| r == "*" || r == root)
}

/// The catalog as `grant` may see it.
fn catalog(
    auth: &RpcAuth,
    grant: &DeviceGrant,
    event: &CatalogEvent,
    terminals: &[TerminalInfo],
) -> proto::Catalog {
    let host = auth.host_id();
    let mut projects: BTreeMap<String, proto::ProjectSummary> = BTreeMap::new();
    for w in &event.workspaces {
        let Some(path) = w.canonical_root.to_str() else {
            continue;
        };
        if !visible(grant, path) {
            continue;
        }
        // Worktrees of one repository are checkouts of one project.
        let project = w
            .repository
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or(path)
            .to_owned();
        let entry = projects
            .entry(project.clone())
            .or_insert_with(|| proto::ProjectSummary {
                project_id: project.clone(),
                name: w.name.clone(),
                path: project.clone(),
                checkouts: vec![],
            });
        entry.checkouts.push(proto::WorkspaceSummary {
            workspace: Some(proto::WorkspaceRef {
                host_id: host.into(),
                workspace_id: workspace_ref(w.workspace),
            }),
            name: w.name.clone(),
            path: path.into(),
            closing: Some(w.closing),
            repository_path: w
                .repository
                .as_deref()
                .and_then(|p| p.to_str())
                .map(str::to_owned),
        });
    }
    let records: HashMap<&str, &ChatRecord> = event.records.iter().map(|r| (r.id(), r)).collect();
    let chats = event
        .chats
        .iter()
        .filter_map(|summary| {
            let record = records.get(summary.chat.as_str())?;
            visible(grant, record.authority_root()).then(|| chat_summary(auth, summary, record))
        })
        .collect();
    let terminals = terminals
        .iter()
        .filter(|t| t.root().to_str().is_some_and(|root| visible(grant, root)))
        .map(|t| t.summary(host))
        .collect();
    proto::Catalog {
        host_id: host.into(),
        projects: projects.into_values().collect(),
        chats,
        terminals,
    }
}

fn chat_summary(
    auth: &RpcAuth,
    summary: &crate::app_state::chats::ChatSummary,
    record: &ChatRecord,
) -> proto::ChatSummary {
    proto::ChatSummary {
        chat: Some(reference(auth, record.id())),
        workspace: record.workspace().map(|id| proto::WorkspaceRef {
            host_id: auth.host_id().into(),
            workspace_id: id.into(),
        }),
        title: summary.title.clone().unwrap_or_else(|| "New chat".into()),
        agent_id: record.agent().into(),
        profile_id: record.metadata().profile_id.clone(),
        state: chat_state(summary.status, summary.working, summary.decisions),
        pending_decisions: summary.decisions as u32,
        queued_messages: summary.unsent as u32,
        // The run is read from the chat's own stream (SubscribeChat status).
        run_id: None,
        history_epoch: record.history_epoch(),
        // The goal is read from the chat's own stream, where the runtime reports it.
        goal: None,
    }
}

/// A terminal command's typed failure, as `RpcAuth::receipt` reads it back.
fn domain_error(code: &str, message: &str) -> Option<Vec<u8>> {
    Some(
        proto::DomainError {
            code: code.into(),
            message: message.into(),
            retryable: false,
        }
        .encode_to_vec(),
    )
}

#[tonic::async_trait]
impl proto::catalog_service_server::CatalogService for CatalogRpc {
    async fn list_agents(
        &self,
        r: Request<proto::AgentDiscoveryRequest>,
    ) -> Result<Response<proto::AgentDiscovery>, Status> {
        self.discovery()?.list_agents(r).await
    }
    async fn get_agent_catalog(
        &self,
        r: Request<proto::AgentCatalogRequest>,
    ) -> Result<Response<proto::AgentCatalog>, Status> {
        self.discovery()?.agent_catalog(r).await
    }
    async fn list_pull_requests(
        &self,
        r: Request<proto::PullRequestsRequest>,
    ) -> Result<Response<proto::PullRequests>, Status> {
        self.discovery()?.pull_requests(r).await
    }
    type ReadChatPageStream = RpcStream<proto::ArtifactChunk>;
    async fn read_chat_page(
        &self,
        r: Request<proto::ChatPageRequest>,
    ) -> Result<Response<Self::ReadChatPageStream>, Status> {
        self.discovery()?.read_page(r).await
    }
    async fn list_resources(
        &self,
        r: Request<proto::HostRequest>,
    ) -> Result<Response<proto::Catalog>, Status> {
        let grant = self.grant(&r, &r.get_ref().host_id).await?;
        self.auth.scope(&grant, host::READ)?;
        let result = self.project(&grant).await?;
        if result.catalog.encoded_len() > pages::FRAME_LIMIT {
            return Err(Status::resource_exhausted(
                "catalog_too_large: the catalog exceeds one message; read it with ReadCatalog.",
            ));
        }
        Ok(Response::new(result.catalog.clone()))
    }

    type ReadCatalogStream = RpcStream<proto::SummaryEvent>;
    async fn read_catalog(
        &self,
        r: Request<proto::HostRequest>,
    ) -> Result<Response<Self::ReadCatalogStream>, Status> {
        let grant = self.grant(&r, &r.get_ref().host_id).await?;
        self.auth.scope(&grant, host::READ)?;
        let credential = r
            .peer_certs()
            .and_then(|c| c.first().map(|c| c.to_vec()))
            .ok_or_else(|| Status::unauthenticated("A paired device certificate is required."))?;
        let projection = self.project(&grant).await?;
        let boundary = proto::Cursor {
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

    type WatchSummariesStream = RpcStream<proto::SummaryEvent>;
    async fn watch_summaries(
        &self,
        r: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::WatchSummariesStream>, Status> {
        let grant = self.grant(&r, &r.get_ref().host_id).await?;
        self.auth.scope(&grant, host::READ)?;
        let credential = r
            .peer_certs()
            .and_then(|c| c.first().map(|c| c.to_vec()))
            .ok_or_else(|| Status::unauthenticated("A paired device certificate is required."))?;
        // A cursor never resumes a catalog stream: it starts from the whole catalog
        // (SYNC-02 allows a snapshot instead of replay).
        Ok(Response::new(
            watch::stream(self.clone(), credential).await?,
        ))
    }

    async fn create_chat(
        &self,
        r: Request<proto::CreateChatRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let host = r
            .get_ref()
            .command
            .as_ref()
            .map(|c| c.host_id.clone())
            .unwrap_or_default();
        let grant = self.grant(&r, &host).await?;
        self.auth.scope(&grant, host::CHAT)?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let key = self.auth.key(&meta, &grant)?;
        let chat = resource_id(&key, "chat");
        if let Some(receipt) = self
            .prior(&key, host::CHAT, "chat.create", Some(&chat), &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let workspace = match &r.workspace {
            Some(w) => Some(self.workspace(&grant, w, host::CHAT).await?.0),
            None => {
                self.auth
                    .resource(grant.id(), host::CHAT, "private")
                    .await?;
                None
            }
        };
        // No account is picked for a remote chat by default (F-04).
        let profile = r
            .profile_id
            .filter(|p| !p.is_empty())
            .ok_or_else(|| Status::invalid_argument("Choose the account for the new chat."))?;
        if r.agent_id.is_empty() || r.agent_id.len() > 256 {
            return Err(Status::invalid_argument(
                "Choose the agent for the new chat.",
            ));
        }
        let new = NewChat {
            chat,
            workspace,
            tool: r.agent_id,
            profile,
            model: None,
        };
        let bridge = self.bridge.clone();
        let receipt = self
            .owned(async move {
                let key = RemoteKey {
                    key,
                    fingerprint: Some(fingerprint),
                };
                bridge.create(key, new).await.map_err(bridge_status)
            })
            .await?;
        Ok(Response::new(receipt))
    }

    async fn rename_resource(
        &self,
        r: Request<proto::RenameResourceRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let host = r
            .get_ref()
            .command
            .as_ref()
            .map(|c| c.host_id.clone())
            .unwrap_or_default();
        let grant = self.grant(&r, &host).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let resource = required(r.resource.and_then(|r| r.resource))?;
        let proto::resource_ref::Resource::Chat(chat) = resource else {
            // Canopy has no rename for projects, checkouts, terminals, files or tasks;
            // remote does not add one (§3: existing operations only).
            return Err(Status::unimplemented(
                "unsupported: only chats can be renamed on the host.",
            ));
        };
        self.auth.scope(&grant, host::CHAT)?;
        self.state.accepting()?;
        let title = r.title.trim().to_owned();
        if title.is_empty() || title.len() > 512 {
            return Err(Status::invalid_argument(
                "Enter a title of at most 512 bytes.",
            ));
        }
        let key = self.auth.key(&meta, &grant)?;
        if let Some(receipt) = self
            .prior(
                &key,
                host::CHAT,
                "chat.rename",
                Some(&chat.chat_id),
                &fingerprint,
            )
            .await?
        {
            return Ok(Response::new(receipt));
        }
        self.chats
            .command(
                grant,
                meta,
                Some(chat),
                ChatCommand::Rename { title },
                fingerprint,
            )
            .await
    }

    async fn preflight_close(
        &self,
        r: Request<proto::PreflightCloseRequest>,
    ) -> Result<Response<proto::ClosePlan>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let resource = required(r.resource.clone().and_then(|r| r.resource))?;
        let (kind, risks, blocked) = match resource {
            proto::resource_ref::Resource::Chat(chat) => {
                self.auth.check_host(&chat.host_id)?;
                if !r.delete {
                    return Err(Status::invalid_argument(
                        "Closing a chat view is local to the device; deleting the host chat needs delete.",
                    ));
                }
                self.auth.scope(&grant, host::CHAT)?;
                let record = self.record(&chat.chat_id).await?;
                self.auth
                    .resource(grant.id(), host::CHAT, record.authority_root())
                    .await?;
                let preflight = self
                    .bridge
                    .delete_preflight(chat.chat_id.clone())
                    .await
                    .map_err(bridge_status)?;
                let title = self
                    .bridge
                    .summaries()
                    .await
                    .ok()
                    .and_then(|s| s.into_iter().find(|s| s.chat == chat.chat_id))
                    .and_then(|s| s.title)
                    .unwrap_or_else(|| "New chat".into());
                let risks = vec![proto::CloseRisk {
                    resource: r.resource.clone(),
                    title,
                    working: preflight.working || preflight.commands > 0,
                    queued: preflight.unsent as u32,
                    observers: preflight.observers as u32,
                    dirty: vec![],
                }];
                let blocked = preflight.blocked.clone();
                (PlanKind::Chat(preflight), risks, blocked)
            }
            proto::resource_ref::Resource::Workspace(w) => {
                self.auth.scope(&grant, host::CHAT)?;
                let (id, _, root) = self.workspace(&grant, &w, host::CHAT).await?;
                let preflight = self
                    .bridge
                    .close_project_preflight(id)
                    .await
                    .map_err(bridge_status)?;
                let dirty = plans::presence(&self.auth, &root, &self.state).await?;
                let summaries = self.bridge.summaries().await.map_err(bridge_status)?;
                let mut risks: Vec<_> = summaries
                    .iter()
                    .filter(|s| preflight.chats.contains(&s.chat))
                    .map(|s| proto::CloseRisk {
                        resource: Some(proto::ResourceRef {
                            resource: Some(proto::resource_ref::Resource::Chat(reference(
                                &self.auth, &s.chat,
                            ))),
                        }),
                        title: s.title.clone().unwrap_or_else(|| "New chat".into()),
                        working: s.working,
                        queued: s.unsent as u32,
                        observers: s.observers as u32,
                        dirty: vec![],
                    })
                    .collect();
                risks.insert(
                    0,
                    proto::CloseRisk {
                        resource: r.resource.clone(),
                        title: self
                            .bridge
                            .workspaces()
                            .await
                            .ok()
                            .and_then(|all| all.into_iter().find(|s| s.workspace == id))
                            .map(|s| s.name)
                            .unwrap_or_default(),
                        working: preflight.running || preflight.working > 0,
                        queued: preflight.unsent as u32,
                        observers: preflight.observers as u32,
                        dirty: dirty.iter().map(|d| d.wire(&w)).collect(),
                    },
                );
                let blocked = if preflight.dirty_editors {
                    Some("needs_host_interaction: unsaved editor files on the host".to_owned())
                } else if plans::active_dirty(&dirty) {
                    Some("Another connected device has unsaved edits. Save or discard them on that device before closing the project.".into())
                } else {
                    None
                };
                (
                    PlanKind::Project {
                        preflight,
                        root,
                        dirty,
                    },
                    risks,
                    blocked,
                )
            }
            proto::resource_ref::Resource::Terminal(t) => {
                self.auth.check_host(&t.host_id)?;
                self.auth.scope(&grant, host::TERMINAL)?;
                let info = self.terminal(&t.terminal_id).await?;
                let root = WorkspaceRoots::root_string(info.root())?.to_owned();
                self.auth
                    .resource(grant.id(), host::TERMINAL, &root)
                    .await?;
                let summary = info.summary(self.auth.host_id());
                let risks = vec![proto::CloseRisk {
                    resource: r.resource.clone(),
                    title: summary.title,
                    working: summary.running,
                    queued: 0,
                    observers: 0,
                    dirty: vec![],
                }];
                (
                    PlanKind::Terminal {
                        id: t.terminal_id,
                        root,
                    },
                    risks,
                    None,
                )
            }
            _ => {
                return Err(Status::unimplemented(
                    "unsupported: files and tasks are not closed through the catalog.",
                ));
            }
        };
        let can_execute = blocked.is_none();
        let plan_id = self.plans.lock().map_err(|_| poisoned())?.insert(
            grant.id(),
            required(r.resource)?,
            r.delete,
            kind,
        )?;
        Ok(Response::new(proto::ClosePlan {
            host_id: self.auth.host_id().into(),
            plan_id,
            risks,
            can_execute,
            reason: blocked.unwrap_or_default(),
        }))
    }

    async fn close_resource(
        &self,
        r: Request<proto::CloseResourceRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let host = r
            .get_ref()
            .command
            .as_ref()
            .map(|c| c.host_id.clone())
            .unwrap_or_default();
        let grant = self.grant(&r, &host).await?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let resource = required(r.resource.clone())?;
        let (scope, method, target) = match required(resource.resource.clone())? {
            proto::resource_ref::Resource::Chat(c) => {
                self.auth.check_host(&c.host_id)?;
                (host::CHAT, "chat.delete", Some(c.chat_id))
            }
            proto::resource_ref::Resource::Workspace(w) => (
                host::CHAT,
                "projects.close",
                self.roots
                    .resolve(&w, &self.auth)
                    .ok()
                    .and_then(|root| root.to_str().map(str::to_owned)),
            ),
            proto::resource_ref::Resource::Terminal(t) => {
                self.auth.check_host(&t.host_id)?;
                (host::TERMINAL, "terminal.close", Some(t.terminal_id))
            }
            _ => {
                return Err(Status::unimplemented(
                    "unsupported: files and tasks are not closed through the catalog.",
                ));
            }
        };
        self.auth.scope(&grant, scope)?;
        let key = self.auth.key(&meta, &grant)?;
        // Before the target is looked up again: a closed workspace no longer resolves.
        if let Some(receipt) = self
            .prior(&key, scope, method, target.as_deref(), &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let kind = self.plans.lock().map_err(|_| poisoned())?.take(
            &r.plan_id,
            grant.id(),
            &resource,
            key.id(),
        )?;
        match kind {
            PlanKind::Chat(preflight) => {
                let record = self.record(&preflight.chat).await?;
                self.auth
                    .resource(grant.id(), host::CHAT, record.authority_root())
                    .await?;
                let bridge = self.bridge.clone();
                let key = RemoteKey {
                    key,
                    fingerprint: Some(fingerprint),
                };
                let receipt = self
                    .owned(
                        async move { bridge.delete(key, preflight).await.map_err(bridge_status) },
                    )
                    .await?;
                Ok(Response::new(receipt))
            }
            PlanKind::Project {
                preflight,
                root,
                dirty,
            } => {
                self.auth.resource(grant.id(), host::CHAT, &root).await?;
                // Unsaved buffers reported meanwhile were not confirmed (APP-04).
                if plans::presence(&self.auth, &root, &self.state).await? != dirty
                    || plans::active_dirty(&dirty)
                {
                    return Err(Status::aborted(
                        "conditions_changed: unsaved edits on devices changed; preflight again.",
                    ));
                }
                let bridge = self.bridge.clone();
                let key = RemoteKey {
                    key,
                    fingerprint: Some(fingerprint),
                };
                let receipt = self
                    .owned(async move {
                        bridge
                            .close_project(key, preflight)
                            .await
                            .map_err(bridge_status)
                    })
                    .await?;
                Ok(Response::new(receipt))
            }
            PlanKind::Terminal { id, root } => {
                self.auth
                    .resource(grant.id(), host::TERMINAL, &root)
                    .await?;
                let input = self
                    .auth
                    .input(
                        &meta,
                        &grant,
                        "terminal.close",
                        id.clone(),
                        r.encode_to_vec(),
                        root.clone(),
                    )?
                    .with_fingerprint(fingerprint)
                    .ok_or_else(|| Status::invalid_argument("Invalid command arguments."))?;
                let (client, terminals) = (self.auth.client().clone(), self.terminals.clone());
                let receipt = self
                    .owned(async move {
                        let receipt = client
                            .accept_runtime_command(input, host::TERMINAL)
                            .await
                            .map_err(storage_status)?;
                        if receipt.state() != CommandState::Accepted {
                            return Ok(receipt);
                        }
                        // Checked again right before the effect (LIFE-03).
                        let current = terminals.terminal(&id).await;
                        let refusal = match &current {
                            Ok(Some(info)) if info.root().to_str() == Some(root.as_str()) => None,
                            Ok(_) => Some("resource_deleted"),
                            Err(_) => Some("conditions_changed"),
                        };
                        if let Some(refusal) = refusal {
                            return client
                                .transition_runtime_command(
                                    key,
                                    CommandState::Accepted,
                                    CommandState::Rejected,
                                    domain_error(refusal, refusal),
                                )
                                .await
                                .map_err(storage_status);
                        }
                        if client
                            .transition_runtime_command(
                                key.clone(),
                                CommandState::Accepted,
                                CommandState::Attempting,
                                None,
                            )
                            .await
                            .is_err()
                        {
                            // Another attempt of this command owns the effect.
                            return client
                                .get_runtime_command(key, host::TERMINAL)
                                .await
                                .map_err(storage_status)?
                                .ok_or_else(|| {
                                    Status::unknown("The terminal close is uncertain.")
                                });
                        }
                        let (state, result) = match terminals.close(id).await {
                            Ok(()) => (CommandState::Succeeded, None),
                            Err(error) => (
                                CommandState::Failed,
                                domain_error("operation_failed", error.message()),
                            ),
                        };
                        client
                            .transition_runtime_command(
                                key,
                                CommandState::Attempting,
                                state,
                                result,
                            )
                            .await
                            .map_err(storage_status)
                    })
                    .await?;
                Ok(Response::new(receipt))
            }
        }
    }

    async fn open_project(
        &self,
        r: Request<proto::OpenProjectRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let host = r
            .get_ref()
            .command
            .as_ref()
            .map(|c| c.host_id.clone())
            .unwrap_or_default();
        let grant = self.grant(&r, &host).await?;
        self.auth.scope(&grant, host::CHAT)?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let key = self.auth.key(&meta, &grant)?;
        let path = PathBuf::from(&r.host_path);
        if r.host_path.is_empty()
            || r.host_path.len() > 4096
            || r.host_path.contains('\0')
            || !path.is_absolute()
        {
            return Err(Status::invalid_argument(
                "Enter an absolute folder path on the host.",
            ));
        }
        let requested = path.clone();
        let folder = tokio::task::spawn_blocking(move || {
            canopy_desktop::git::workspace_directory(&requested)
        })
        .await
        .map_err(|_| Status::unavailable("The folder check stopped."))?;
        let root = folder
            .as_ref()
            .ok()
            .and_then(|f| f.to_str())
            .map(str::to_owned);
        if let Some(receipt) = self
            .prior(
                &key,
                host::CHAT,
                "projects.open",
                root.as_deref(),
                &fingerprint,
            )
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let root =
            root.ok_or_else(|| Status::not_found("This folder is unavailable or cannot be read."))?;
        self.auth.resource(grant.id(), host::CHAT, &root).await?;
        let bridge = self.bridge.clone();
        let receipt = self
            .owned(async move {
                let key = RemoteKey {
                    key,
                    fingerprint: Some(fingerprint),
                };
                bridge.open_project(key, path).await.map_err(bridge_status)
            })
            .await?;
        Ok(Response::new(receipt))
    }
}

fn poisoned() -> Status {
    Status::internal("The close plan registry is unavailable.")
}
