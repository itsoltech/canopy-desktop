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
    proto,
    resources::ResourceLocks,
};
use crate::{
    host::{CommandState, DeviceGrant, READ, TERMINAL},
    terminal::control::{Holder, Refused, Run},
};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
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
            workspace: self.workspace.as_ref().map(|id| proto::WorkspaceRef {
                host_id: host.into(),
                workspace_id: id.clone(),
            }),
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
        let grant = self
            .auth
            .resource(grant.id(), scope, WorkspaceRoots::root_string(info.root())?)
            .await?;
        Ok((grant, info))
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
    /// The normalized command record, bound to the terminal's canonical root.
    fn input(
        &self,
        grant: &DeviceGrant,
        meta: Option<&proto::CommandMeta>,
        method: &'static str,
        target: String,
        arguments: Vec<u8>,
        root: &Path,
    ) -> Result<crate::host::CommandInput, Status> {
        let meta = meta.ok_or_else(|| Status::invalid_argument("Command is required."))?;
        self.auth.input(
            meta,
            grant,
            method,
            target,
            arguments,
            WorkspaceRoots::root_string(root)?.into(),
        )
    }
    /// Accepted → attempting → outcome, owned by a detached task: dropping the RPC
    /// neither cancels a started effect nor loses its recorded result.
    async fn command<F, Fut>(
        &self,
        input: crate::host::CommandInput,
        effect: F,
    ) -> Result<proto::CommandReceipt, Status>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Vec<u8>, Status>> + Send + 'static,
    {
        let operation = self.locks.begin()?;
        let auth = self.auth.clone();
        tokio::spawn(async move {
            let _operation = operation;
            let accepted = auth
                .client()
                .accept_runtime_command(input, TERMINAL)
                .await
                .map_err(storage_status)?;
            if accepted.state() != CommandState::Accepted {
                return Ok(receipt(&auth, &accepted));
            }
            auth.client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .map_err(storage_status)?;
            let (state, bytes) = match effect().await {
                Ok(bytes) => (CommandState::Succeeded, bytes),
                Err(error) => {
                    let state = if error.code() == tonic::Code::Unknown {
                        CommandState::DeliveryUnknown
                    } else {
                        CommandState::Failed
                    };
                    (state, prost::Message::encode_to_vec(&domain_error(&error)))
                }
            };
            let done = auth
                .client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Attempting,
                    state,
                    Some(bytes),
                )
                .await
                .map_err(storage_status)?;
            Ok(receipt(&auth, &done))
        })
        .await
        .map_err(|_| Status::unavailable("Command outcome requires reconciliation."))?
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
fn terminal_id(device: &str, meta: &proto::CommandMeta) -> String {
    let digest =
        Sha256::digest(format!("{device}\n{}\n{}", meta.command_epoch, meta.command_id).as_bytes());
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
fn receipt(auth: &RpcAuth, receipt: &crate::host::CommandReceipt) -> proto::CommandReceipt {
    let mut result = auth.receipt(receipt);
    let bytes = receipt.result();
    match receipt.state() {
        CommandState::Failed | CommandState::Rejected | CommandState::DeliveryUnknown => {
            result.error = bytes.and_then(|bytes| prost::Message::decode(bytes).ok());
        }
        CommandState::Succeeded => {
            result.operation_output =
                bytes.and_then(|bytes| String::from_utf8(bytes.to_vec()).ok());
        }
        _ => {}
    }
    result
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
        request: Request<proto::TerminalHistoryRequest>,
    ) -> Result<Response<Self::ReadHistoryStream>, Status> {
        let slot = self.admit_stream()?;
        let (grant, info) = self
            .authorize(&request, request.get_ref().terminal.as_ref(), READ)
            .await?;
        let events = history::read(self, &info, request.into_inner()).await?;
        let root = WorkspaceRoots::root_string(info.root())?.to_owned();
        Ok(Response::new(history::stream(
            self.clone(),
            grant.id().to_owned(),
            root,
            events,
            slot,
        )))
    }

    async fn open_terminal(
        &self,
        request: Request<proto::OpenTerminalRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth.scope(&grant, TERMINAL)?;
        let request = request.into_inner();
        let workspace = request
            .workspace
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("Workspace is required."))?;
        let root = self.roots.resolve(workspace, &self.auth)?;
        let grant = self
            .auth
            .resource(grant.id(), TERMINAL, WorkspaceRoots::root_string(&root)?)
            .await?;
        if request.tool_id.is_empty()
            || request.tool_id.len() > 256
            || request.profile_id.as_ref().is_some_and(|p| p.len() > 256)
        {
            return Err(Status::invalid_argument("Invalid terminal tool."));
        }
        let meta = request
            .command
            .clone()
            .ok_or_else(|| Status::invalid_argument("Command is required."))?;
        let id = terminal_id(grant.id(), &meta);
        let arguments = serde_json::to_vec(&serde_json::json!({
            "workspace": workspace.workspace_id,
            "tool": request.tool_id,
            "profile": request.profile_id,
        }))
        .map_err(|_| Status::internal("Command arguments could not be encoded."))?;
        let open = OpenTerminal {
            terminal: id.clone(),
            workspace: workspace.workspace_id.clone(),
            root: root.clone(),
            tool: request.tool_id,
            profile: request.profile_id,
            holder: Holder::device(grant.id(), grant.name()),
        };
        let host = self.host.clone();
        let receipt = self
            .command(
                self.input(&grant, Some(&meta), "terminal.open", id, arguments, &root)?,
                move || async move {
                    let info = host.open(open).await?;
                    Ok(info.id().as_bytes().to_vec())
                },
            )
            .await?;
        Ok(Response::new(receipt))
    }
    async fn acquire_input(
        &self,
        request: Request<proto::AcquireInputRequest>,
    ) -> Result<Response<proto::InputLease>, Status> {
        let (grant, info) = self
            .authorize(&request, request.get_ref().terminal.as_ref(), TERMINAL)
            .await?;
        let request = request.into_inner();
        if let Some(meta) = &request.command {
            self.auth.check_host(&meta.host_id)?;
        }
        let run = info
            .run()
            .cloned()
            .ok_or_else(|| Status::failed_precondition("The terminal is not running."))?;
        run.acquire(Holder::device(grant.id(), grant.name()), request.take_over)
            .map_err(|error| refused(&error))?;
        Ok(Response::new(self.lease(info.id(), &run, grant.id())))
    }
    async fn release_input(
        &self,
        request: Request<proto::ReleaseInputRequest>,
    ) -> Result<Response<proto::InputLease>, Status> {
        let (grant, info) = self
            .authorize(&request, request.get_ref().terminal.as_ref(), TERMINAL)
            .await?;
        let request = request.into_inner();
        if let Some(meta) = &request.command {
            self.auth.check_host(&meta.host_id)?;
        }
        let run = check_run(&info, &request.run_id)?;
        run.release(
            &Holder::device(grant.id(), grant.name()),
            request.fencing_token,
        )
        .map_err(|error| refused(&error))?;
        Ok(Response::new(self.lease(info.id(), &run, grant.id())))
    }
    type TerminalSessionStream = RpcStream<proto::TerminalServerEvent>;
    async fn terminal_session(
        &self,
        request: Request<tonic::Streaming<proto::TerminalClientEvent>>,
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
        request: Request<proto::TerminalCommandRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let (grant, info) = self
            .authorize(&request, request.get_ref().terminal.as_ref(), TERMINAL)
            .await?;
        let request = request.into_inner();
        let host = self.host.clone();
        let (terminal, run) = (info.id().to_owned(), request.run_id.clone());
        let receipt = self
            .command(
                self.input(
                    &grant,
                    request.command.as_ref(),
                    "terminal.stop",
                    terminal.clone(),
                    run.as_bytes().to_vec(),
                    info.root(),
                )?,
                move || async move {
                    host.stop(&terminal, &run).await?;
                    Ok(Vec::new())
                },
            )
            .await?;
        Ok(Response::new(receipt))
    }
    async fn restart_terminal(
        &self,
        request: Request<proto::TerminalCommandRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let (grant, info) = self
            .authorize(&request, request.get_ref().terminal.as_ref(), TERMINAL)
            .await?;
        let request = request.into_inner();
        let host = self.host.clone();
        let holder = Holder::device(grant.id(), grant.name());
        let (terminal, run) = (info.id().to_owned(), request.run_id.clone());
        let receipt = self
            .command(
                self.input(
                    &grant,
                    request.command.as_ref(),
                    "terminal.restart",
                    terminal.clone(),
                    run.as_bytes().to_vec(),
                    info.root(),
                )?,
                move || async move {
                    let run = host.restart(&terminal, &run, holder).await?;
                    Ok(run.into_bytes())
                },
            )
            .await?;
        Ok(Response::new(receipt))
    }
}
