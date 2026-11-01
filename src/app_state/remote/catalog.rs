//! Authorized catalog projections and lifecycle adapters over the application owners.
mod api;
mod pages;
mod plans;
#[cfg(test)]
mod tests;
mod watch;

use super::chat::{bridge_status, chat_state, reference, required, resource_id};
use crate::app_state::{
    chat_bridge::{CatalogEvent, ChatBridge},
    chats::{NewChat, RemoteKey},
    project_commands::WorkspaceSummary,
    terminal_bridge::{TerminalBridge, workspace_ref},
};
use canopy_desktop::{
    api::proto,
    host::{self, ChatRecord, CommandState, DeviceGrant},
    remote::{
        auth::{RpcAuth, WorkspaceRoots, storage_status},
        host::{HostState, RpcStream},
        resources::ResourceLocks,
        terminal_rpc::{TerminalHost, TerminalInfo},
    },
    state::workspace::WorkspaceId,
};
use plans::{PlanKind, PlanResource, Plans};
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
    state: HostState,
    operations: ResourceLocks,
    plans: Arc<Mutex<Plans>>,
    discovery: Option<canopy_desktop::remote::discovery::Discovery>,
    output: canopy_desktop::remote::sync::ReplayHub,
}
impl CatalogRpc {
    pub fn new(
        auth: RpcAuth,
        bridge: ChatBridge,
        roots: WorkspaceRoots,
        terminals: TerminalBridge,
        state: HostState,
        operations: ResourceLocks,
    ) -> Self {
        Self {
            auth,
            bridge,
            roots,
            terminals,
            state,
            operations,
            plans: Arc::default(),
            discovery: None,
            output: canopy_desktop::remote::sync::ReplayHub::new(Default::default()),
        }
    }
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
    async fn terminal(&self, id: &str) -> Result<TerminalInfo, Status> {
        self.terminals
            .terminal(id)
            .await?
            .ok_or_else(|| Status::not_found("The terminal is no longer available."))
    }
    async fn snapshot(&self) -> Result<Arc<CatalogEvent>, Status> {
        let watch = self.bridge.watch_catalog().await.map_err(bridge_status)?;
        let event = watch
            .recv()
            .await
            .map_err(|_| Status::unavailable("Canopy is closing."))?;
        self.update_roots(&event)?;
        Ok(event)
    }
    fn update_roots(&self, event: &CatalogEvent) -> Result<(), Status> {
        self.roots.replace_projects(
            event
                .workspaces
                .iter()
                .map(|w| canopy_desktop::state::projects::Project {
                    id: w.project,
                    workspace: w.workspace,
                    name: w.name.clone(),
                    path: w.canonical_root.clone(),
                    worktree_path: None,
                    repository_path: w.repository.clone(),
                    worktree_base: None,
                })
                .collect(),
        )
    }
    async fn project(&self, grant: &DeviceGrant) -> Result<Arc<pages::Projection>, Status> {
        let (event, terminals) = tokio::join!(self.snapshot(), self.terminals.terminals());
        let event = event?;
        let terminals = terminals?;
        pages::Projection::build(&self.output, &self.auth, grant, &event, &terminals)
    }
}
fn visible(grant: &DeviceGrant, root: &str) -> bool {
    !grant.revoked()
        && grant.scopes() & host::READ == host::READ
        && !root.is_empty()
        && grant.roots().iter().any(|r| r == "*" || r == root)
}
fn catalog(
    auth: &RpcAuth,
    grant: &DeviceGrant,
    event: &CatalogEvent,
    terminals: &[TerminalInfo],
) -> proto::Catalog {
    let mut projects: BTreeMap<String, proto::ProjectSummary> = BTreeMap::new();
    for w in &event.workspaces {
        let Some(path) = w.canonical_root.to_str().filter(|p| visible(grant, p)) else {
            continue;
        };
        let project = w
            .repository
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or(path)
            .to_owned();
        let entry = projects
            .entry(project.clone())
            .or_insert_with(|| proto::ProjectSummary {
                origin: w.origin.as_ref().map(|o| o.canonical_url()),
                name: w.name.clone(),
                path: project,
                checkouts: vec![],
            });
        entry.checkouts.push(proto::ProjectCheckout {
            project: Some(api::project_wire(auth.host_id(), w)),
            catalog_id: workspace_ref(w.workspace),
            name: w.name.clone(),
            path: path.into(),
            closing: w.closing,
            branch: w.branch.clone(),
            repository_path: w
                .repository
                .as_deref()
                .and_then(|p| p.to_str())
                .map(str::to_owned),
        });
    }
    let records: HashMap<&str, &ChatRecord> = event.records.iter().map(|r| (r.id(), r)).collect();
    let threads = event
        .chats
        .iter()
        .filter_map(|summary| {
            let record = records.get(summary.chat.as_str())?;
            visible(grant, record.authority_root()).then(|| chat_summary(auth, summary, record))
        })
        .collect();
    proto::Catalog {
        environment: Some(proto::EnvironmentRef {
            host_id: auth.host_id().into(),
        }),
        projects: projects.into_values().collect(),
        threads,
        terminals: terminals
            .iter()
            .filter(|t| {
                event.workspaces.iter().any(|w| {
                    t.workspace() == Some(workspace_ref(w.workspace).as_str())
                        && t.root() == w.canonical_root
                })
            })
            .filter(|t| t.root().to_str().is_some_and(|r| visible(grant, r)))
            .map(|t| t.summary(auth.host_id()))
            .collect(),
    }
}
fn chat_summary(
    auth: &RpcAuth,
    summary: &crate::app_state::chats::ChatSummary,
    record: &ChatRecord,
) -> proto::CatalogThread {
    let thread = reference(auth, record);
    proto::CatalogThread {
        thread: thread.clone(),
        catalog_id: record.id().into(),
        project_catalog_id: record.workspace().map(str::to_owned),
        summary: Some(proto::ThreadStatus {
            thread,
            title: summary.title.clone().unwrap_or_else(|| "New chat".into()),
            agent_id: record.agent().into(),
            profile_id: record.metadata().profile_id.clone(),
            state: chat_state(summary.status, summary.working, summary.decisions),
            pending_decisions: summary.decisions as u32,
            queued_messages: summary.unsent as u32,
            run_id: None,
            history_epoch: record.history_epoch(),
            goal: None,
        }),
    }
}
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
fn poisoned() -> Status {
    Status::unavailable("The close confirmations are unavailable.")
}
