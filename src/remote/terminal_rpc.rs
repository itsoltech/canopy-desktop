//! Native TerminalService. The host's terminal owner (`TerminalHost`, implemented by
//! the app) keeps every process; this service authorizes the verified device and
//! reads the same Alacritty projection the desktop paints. Input and resize go
//! through the run's lease (`terminal::control::Run`), the check the desktop uses.
//! Open, Stop and Restart are durable commands owned by a task that outlives the RPC.
mod history;
#[cfg(test)]
mod load_tests;
mod projection;
mod stream;
#[cfg(test)]
mod tests;

use super::{
    auth::{RpcAuth, WorkspaceRoots, storage_status},
    host::RpcStream,
    resources::ResourceLocks,
};
use crate::{
    api::{self, proto},
    command_bus::{Command, TerminalCommand},
    host::{CommandState, DeviceGrant, READ, TERMINAL},
    terminal::control::{Holder, Refused, Run},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tonic::{Request, Response, Status};

/// Open TerminalSession streams across all devices.
const SESSION_STREAMS: usize = 64;

/// One terminal as the host's owner reports it.
#[derive(Clone)]
pub struct TerminalInfo {
    id: String,
    root: PathBuf,
    workspace: Option<String>,
    title: String,
    running: bool,
    run: Option<Arc<Run>>,
}
impl TerminalInfo {
    /// `root` is the canonical checkout the terminal runs in; devices are
    /// authorized against it, never against a client path.
    pub fn new(id: String, root: PathBuf, title: String, run: Option<Arc<Run>>) -> Self {
        let running = run.as_ref().is_some_and(|run| {
            !run.retired()
                && matches!(
                    run.session().status(),
                    crate::terminal::session::Status::Running
                )
        });
        Self {
            id,
            root,
            workspace: None,
            title,
            running,
            run,
        }
    }
    /// The catalog workspace ID of `root`, when it is an open project.
    pub fn with_workspace(mut self, workspace: String) -> Self {
        self.workspace = Some(workspace);
        self
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn workspace(&self) -> Option<&str> {
        self.workspace.as_deref()
    }
    pub fn run(&self) -> Option<&Arc<Run>> {
        self.run.as_ref()
    }
    /// Catalog row for a device already authorized to read `root`.
    pub fn summary(&self, host: &str) -> proto::TerminalSummary {
        let lease = self.run.as_ref().map(|run| run.lease());
        proto::TerminalSummary {
            terminal: Some(proto::TerminalRef {
                host_id: host.into(),
                terminal_id: self.id.clone(),
            }),
            project_catalog_id: self.workspace.clone().unwrap_or_default(),
            title: self.title.clone(),
            running: self.running,
            input_device_id: lease
                .as_ref()
                .and_then(|lease| lease.holder())
                .and_then(Holder::device_id)
                .map(str::to_owned),
        }
    }
}

/// A terminal a device asked for. It runs in `root` without a host pane or layout
/// change; `holder` owns its input first.
pub struct OpenTerminal {
    pub terminal: String,
    pub workspace: String,
    pub root: PathBuf,
    pub tool: String,
    pub profile: Option<String>,
    pub holder: Holder,
}

/// The app's terminal owner. Every method runs the same typed operations as the
/// desktop and waits for their confirmed outcome.
#[tonic::async_trait]
pub trait TerminalHost: Send + Sync + 'static {
    async fn terminals(&self) -> Result<Vec<TerminalInfo>, Status>;
    async fn terminal(&self, id: &str) -> Result<Option<TerminalInfo>, Status>;
    /// Starts the terminal; an existing `terminal` ID returns it unchanged.
    async fn open(&self, request: OpenTerminal) -> Result<TerminalInfo, Status>;
    /// Ends `run` and waits for confirmed process cleanup. The terminal remains.
    async fn stop(&self, terminal: &str, run: &str) -> Result<(), Status>;
    /// Confirmed Stop of `run`, then a new run owned by `holder`; its ID.
    async fn restart(&self, terminal: &str, run: &str, holder: Holder) -> Result<String, Status>;
}

#[derive(Clone)]
pub struct TerminalRpc {
    auth: RpcAuth,
    roots: WorkspaceRoots,
    locks: ResourceLocks,
    host: Arc<dyn TerminalHost>,
    /// Cursor epoch of this server lifetime; cursors never survive a restart.
    epoch: Arc<str>,
    /// Keys history page tokens to this server lifetime.
    token_key: Arc<[u8; 32]>,
    streams: Arc<AtomicUsize>,
    state: Option<super::host::HostState>,
    output: super::sync::ReplayHub,
}
impl TerminalRpc {
    /// `locks` bounds and drains owned command tasks with the other resource services.
    pub fn new(
        auth: RpcAuth,
        roots: WorkspaceRoots,
        locks: ResourceLocks,
        host: Arc<dyn TerminalHost>,
    ) -> Self {
        let mut key = [0; 32];
        key[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        key[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        Self {
            auth,
            roots,
            locks,
            host,
            epoch: uuid::Uuid::new_v4().to_string().into(),
            token_key: Arc::new(key),
            streams: Arc::default(),
            state: None,
            output: super::sync::ReplayHub::new(super::sync::Limits::default()),
        }
    }
    /// The app passes the same budget owner as chat fanout.
    pub fn with_output_budget(mut self, output: super::sync::ReplayHub) -> Self {
        self.output = output;
        self
    }
    pub fn with_state(mut self, state: super::host::HostState) -> Self {
        self.epoch = state.runtime_epoch().into();
        self.state = Some(state);
        self
    }
    pub fn into_server(self) -> proto::terminal_service_server::TerminalServiceServer<Self> {
        proto::terminal_service_server::TerminalServiceServer::new(self)
            .max_decoding_message_size(crate::host::CONTROL_BYTES)
            .max_encoding_message_size(crate::host::CONTROL_BYTES)
    }
    /// Catalog rows of the terminals `grant` may read.
    pub async fn summaries(
        &self,
        grant: &DeviceGrant,
    ) -> Result<Vec<proto::TerminalSummary>, Status> {
        let mut rows = Vec::new();
        for info in self.host.terminals().await? {
            if self.project_terminal(&info).is_err() {
                continue;
            }
            let root = WorkspaceRoots::root_string(info.root())?;
            if self.auth.resource(grant.id(), READ, root).await.is_ok() {
                rows.push(info.summary(self.auth.host_id()));
            }
        }
        Ok(rows)
    }
    /// Scope first, then the terminal, then the device's grant for its root.
    async fn authorize<T>(
        &self,
        request: &Request<T>,
        terminal: Option<&proto::TerminalRef>,
        scope: u32,
    ) -> Result<(DeviceGrant, TerminalInfo), Status> {
        let grant = self.auth.authenticate(request).await?;
        self.auth.scope(&grant, scope)?;
        self.terminal(&grant, terminal, scope).await
    }
    async fn terminal(
        &self,
        grant: &DeviceGrant,
        terminal: Option<&proto::TerminalRef>,
        scope: u32,
    ) -> Result<(DeviceGrant, TerminalInfo), Status> {
        let terminal = terminal.ok_or_else(|| Status::invalid_argument("Terminal is required."))?;
        self.auth.check_host(&terminal.host_id)?;
        let info = self
            .host
            .terminal(&terminal.terminal_id)
            .await?
            .ok_or_else(|| Status::not_found("The terminal is no longer available."))?;
        self.project_terminal(&info)?;
        let grant = self
            .auth
            .resource(grant.id(), scope, WorkspaceRoots::root_string(info.root())?)
            .await?;
        Ok((grant, info))
    }
    /// An absolute root or a Private grant cannot turn a scratch terminal into
    /// a project resource. Both identities must come from the existing owners.
    fn project_terminal(&self, info: &TerminalInfo) -> Result<(), Status> {
        let workspace = info
            .workspace()
            .ok_or_else(|| Status::not_found("The terminal has no shared project."))?;
        let root = self
            .roots
            .resolve_host_workspace(workspace)
            .map_err(storage_status)?;
        if root != info.root() {
            return Err(Status::not_found("The terminal project changed."));
        }
        Ok(())
    }
    fn reference(&self, id: &str) -> proto::TerminalRef {
        proto::TerminalRef {
            host_id: self.auth.host_id().into(),
            terminal_id: id.into(),
        }
    }
    fn lease(&self, terminal: &str, run: &Run, device: &str) -> proto::InputLease {
        let lease = run.lease();
        let held = lease.holder().and_then(Holder::device_id) == Some(device);
        proto::InputLease {
            terminal: Some(self.reference(terminal)),
            run_id: run.id().into(),
            fencing_token: if held { lease.token() } else { 0 },
            expires_in_ms: lease
                .expires_in()
                .filter(|_| held)
                .map_or(0, |left| left.as_millis().min(u32::MAX as u128) as u32),
            held,
        }
    }
    /// Project metadata and retained authority are checked before terminal I/O.
    async fn command(
        &self,
        grant: DeviceGrant,
        meta: Option<proto::CommandMeta>,
        project: Option<proto::ProjectRef>,
        command: TerminalCommand,
    ) -> Result<proto::CommandReceipt, Status> {
        let meta = required(meta)?;
        let project = required(project)?;
        let target = proto::ResourceTarget {
            resource: Some(proto::resource_target::Resource::Project(project.clone())),
        };
        let prepared = api::resources::prepare(
            &self.auth,
            &self.roots,
            &grant,
            meta,
            &target,
            Command::Terminal(command.clone()),
        )
        .await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt(&self.auth, prior, &target);
        }
        let recovering = prepared.prior().is_some();
        let input = prepared.runtime_input().clone();
        let root = PathBuf::from(
            input
                .root
                .as_ref()
                .ok_or_else(|| Status::internal("Missing terminal authority."))?,
        );
        let id = terminal_id(&input);
        let service = self.clone();
        let validation_target = target.clone();
        let done = super::resources::validated_mutation(
            self.auth.clone(),
            self.locks.clone(),
            root.clone(),
            input,
            move || async move {
                if recovering {
                    return Err(Status::failed_precondition(
                        "Review the current terminal state and submit a new command.",
                    ));
                }
                api::resources::revalidate_root(
                    &service.auth,
                    &service.roots,
                    &grant,
                    &validation_target,
                    TERMINAL,
                    &root,
                )
                .await?;
                let workspace = service.roots.workspace_id(&root)?;
                let holder = Holder::device(grant.id(), grant.name());
                let info = match &command {
                    TerminalCommand::Open { .. } => None,
                    TerminalCommand::Acquire { terminal, .. }
                    | TerminalCommand::Release { terminal, .. }
                    | TerminalCommand::Stop { terminal, .. }
                    | TerminalCommand::Restart { terminal, .. } => {
                        let info = service.host.terminal(terminal).await?.ok_or_else(|| {
                            Status::not_found("The terminal is no longer available.")
                        })?;
                        service.project_terminal(&info)?;
                        if info.root() != root || info.workspace() != Some(workspace.as_str()) {
                            return Err(Status::failed_precondition(
                                "The terminal checkout changed.",
                            ));
                        }
                        match &command {
                            TerminalCommand::Stop { run, .. }
                            | TerminalCommand::Restart { run, .. }
                            | TerminalCommand::Release { run, .. } => {
                                check_run(&info, run)?;
                            }
                            TerminalCommand::Acquire { take_over, .. } => {
                                let run = info.run().ok_or_else(|| {
                                    Status::failed_precondition("The terminal is not running.")
                                })?;
                                let lease = run.lease();
                                if !take_over && lease.holder().is_some() && !lease.held_by(&holder)
                                {
                                    return Err(Status::failed_precondition(
                                        "Terminal input belongs to another holder.",
                                    ));
                                }
                            }
                            _ => (),
                        }
                        if let TerminalCommand::Release { fencing_token, .. } = &command {
                            let lease = info.run().unwrap().lease();
                            if !lease.held_by(&holder) || lease.token() != *fencing_token {
                                return Err(Status::failed_precondition(
                                    "Terminal input moved to another owner.",
                                ));
                            }
                        }
                        Some(info)
                    }
                };
                Ok(async move {
                    let mut result = proto::TerminalResult::default();
                    match command {
                        TerminalCommand::Open { tool, profile } => {
                            let info = service
                                .host
                                .open(OpenTerminal {
                                    terminal: id,
                                    workspace,
                                    root,
                                    tool,
                                    profile,
                                    holder,
                                })
                                .await?;
                            result.terminal_id = info.id().into();
                            result.run_id =
                                info.run().map_or_else(String::new, |run| run.id().into());
                        }
                        TerminalCommand::Acquire {
                            terminal,
                            take_over,
                        } => {
                            let run = info.as_ref().unwrap().run().unwrap();
                            run.acquire(holder, take_over).map_err(|e| refused(&e))?;
                            result.lease = Some(service.lease(&terminal, run, grant.id()));
                            result.terminal_id = terminal;
                            result.run_id = run.id().into();
                        }
                        TerminalCommand::Release {
                            terminal,
                            run,
                            fencing_token,
                        } => {
                            let current = check_run(info.as_ref().unwrap(), &run)?;
                            current
                                .release(&holder, fencing_token)
                                .map_err(|e| refused(&e))?;
                            result.lease = Some(service.lease(&terminal, &current, grant.id()));
                            result.terminal_id = terminal;
                            result.run_id = run;
                        }
                        TerminalCommand::Stop { terminal, run } => {
                            service.host.stop(&terminal, &run).await?;
                            result.terminal_id = terminal;
                            result.run_id = run;
                        }
                        TerminalCommand::Restart { terminal, run } => {
                            result.run_id = service.host.restart(&terminal, &run, holder).await?;
                            result.terminal_id = terminal;
                        }
                    }
                    Ok(result.encode_to_vec())
                })
            },
        )
        .await?;
        api::resources::receipt(&self.auth, &done, &target)
    }
    /// The open stream count is bounded; a guard returns its slot.
    fn admit_stream(&self) -> Result<StreamSlot, Status> {
        self.streams
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |open| {
                (open < SESSION_STREAMS).then_some(open + 1)
            })
            .map_err(|_| Status::resource_exhausted("Too many terminal views are open."))?;
        Ok(StreamSlot(self.streams.clone()))
    }
}
struct StreamSlot(Arc<AtomicUsize>);
impl Drop for StreamSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Deterministic per command, so a retried Open names the same terminal.
fn terminal_id(input: &crate::host::CommandInput) -> String {
    let key = &input.key;
    let digest = Sha256::digest(
        format!(
            "{}\n{}\n{}\n{}\n{}",
            key.device(),
            key.epoch(),
            key.id(),
            key.resource(),
            key.executor_epoch()
        )
        .as_bytes(),
    );
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}
fn domain_error(error: &Status) -> proto::DomainError {
    let code = if error.message().starts_with("needs_host_interaction") {
        "needs_host_interaction"
    } else {
        match error.code() {
            tonic::Code::Aborted => "conflict",
            tonic::Code::NotFound => "not_found",
            tonic::Code::PermissionDenied => "permission_denied",
            tonic::Code::ResourceExhausted => "resource_limit",
            _ => "operation_failed",
        }
    };
    proto::DomainError {
        code: code.into(),
        message: error.message().chars().take(8192).collect(),
        retryable: false,
    }
}
pub(super) fn refused(error: &Refused) -> Status {
    match error {
        Refused::NotHolder | Refused::StaleToken | Refused::HeldBy(_) => {
            Status::failed_precondition(error.to_string())
        }
        Refused::InvalidSize => Status::invalid_argument(error.to_string()),
        Refused::Retired => Status::aborted(error.to_string()),
        Refused::Input(_) => Status::resource_exhausted(error.to_string()),
    }
}
fn required<T>(value: Option<T>) -> Result<T, Status> {
    value.ok_or_else(|| Status::invalid_argument("Required field is missing."))
}
fn check_run(info: &TerminalInfo, run_id: &str) -> Result<Arc<Run>, Status> {
    info.run()
        .filter(|run| run.id() == run_id)
        .cloned()
        .ok_or_else(|| Status::aborted("The terminal run changed. Refresh the terminal."))
}

#[tonic::async_trait]
impl proto::terminal_service_server::TerminalService for TerminalRpc {
    type ReadHistoryStream = RpcStream<proto::TerminalHistoryEvent>;
    async fn read_history(
        &self,
        request: Request<proto::TerminalServiceReadHistoryRequest>,
    ) -> Result<Response<Self::ReadHistoryStream>, Status> {
        let slot = self.admit_stream()?;
        let (grant, info) = self
            .authorize(&request, request.get_ref().terminal.as_ref(), READ)
            .await?;
        let events = history::read(self, &info, request.into_inner()).await?;
        Ok(Response::new(history::stream(
            self.clone(),
            grant.id().to_owned(),
            info,
            events,
            slot,
        )))
    }

    async fn open_terminal(
        &self,
        request: Request<proto::OpenTerminalRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        Ok(Response::new(
            self.command(
                grant,
                request.command,
                request.project,
                TerminalCommand::Open {
                    tool: request.tool_id,
                    profile: request.profile_id,
                },
            )
            .await?,
        ))
    }
    async fn acquire_input(
        &self,
        request: Request<proto::AcquireInputRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        Ok(Response::new(
            self.command(
                grant,
                request.command,
                request.project,
                TerminalCommand::Acquire {
                    terminal: request.terminal_id,
                    take_over: request.take_over,
                },
            )
            .await?,
        ))
    }
    async fn release_input(
        &self,
        request: Request<proto::ReleaseInputRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        Ok(Response::new(
            self.command(
                grant,
                request.command,
                request.project,
                TerminalCommand::Release {
                    terminal: request.terminal_id,
                    run: request.run_id,
                    fencing_token: request.fencing_token,
                },
            )
            .await?,
        ))
    }
    type TerminalSessionStream = RpcStream<proto::TerminalServerEvent>;
    async fn terminal_session(
        &self,
        request: Request<tonic::Streaming<proto::TerminalSessionRequest>>,
    ) -> Result<Response<Self::TerminalSessionStream>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth.scope(&grant, READ)?;
        let credential = request
            .peer_certs()
            .and_then(|certs| certs.first().map(|cert| cert.to_vec()))
            .ok_or_else(|| Status::unauthenticated("A paired device certificate is required."))?;
        let slot = self.admit_stream()?;
        let events =
            stream::open(self.clone(), grant, credential, request.into_inner(), slot).await?;
        Ok(Response::new(events))
    }
    async fn stop_terminal(
        &self,
        request: Request<proto::StopTerminalRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        Ok(Response::new(
            self.command(
                grant,
                request.command,
                request.project,
                TerminalCommand::Stop {
                    terminal: request.terminal_id,
                    run: request.run_id,
                },
            )
            .await?,
        ))
    }
    async fn restart_terminal(
        &self,
        request: Request<proto::RestartTerminalRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        Ok(Response::new(
            self.command(
                grant,
                request.command,
                request.project,
                TerminalCommand::Restart {
                    terminal: request.terminal_id,
                    run: request.run_id,
                },
            )
            .await?,
        ))
    }
}
