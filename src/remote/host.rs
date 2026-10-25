//! Host liveness is separate from transport keepalive and command outcomes.
use super::{
    auth::{RpcAuth, storage_status},
    enrollment::scope_names,
    proto,
    resources::ResourceLocks,
    sync::ReplayHub,
};
use crate::host::{self, CommandKey, CommandState, DeviceGrant};
use futures_lite::stream;
use prost::Message;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

pub type RpcStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send + 'static>>;

#[derive(Clone)]
pub struct HostState {
    state: watch::Sender<proto::HostStatus>,
    capabilities: Arc<[proto::Capability]>,
    online: Arc<Mutex<HashMap<String, usize>>>,
}
impl HostState {
    pub fn new(host: &str, capabilities: Vec<proto::Capability>) -> Self {
        let (state, _) = watch::channel(proto::HostStatus {
            host_id: host.into(),
            readiness: proto::HostReadiness::Starting as i32,
            reason: String::new(),
            cursor: Some(proto::Cursor {
                runtime_epoch: uuid::Uuid::new_v4().to_string(),
                stream_id: "host".into(),
                sequence: 0,
            }),
        });
        Self {
            state,
            capabilities: capabilities.into(),
            online: Arc::default(),
        }
    }
    /// A client's long-lived host watch is its live application presence. Dirty
    /// records remain durable; disconnect changes only this transient fact.
    pub fn device_online(&self, device: &str) -> bool {
        self.online
            .lock()
            .is_ok_and(|online| online.get(device).copied().unwrap_or(0) > 0)
    }
    fn attach_device(&self, device: String) -> OnlineDevice {
        *self
            .online
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(device.clone())
            .or_default() += 1;
        self.authorization_changed();
        OnlineDevice {
            device,
            state: self.clone(),
        }
    }
    pub fn set(&self, readiness: proto::HostReadiness, reason: String) {
        self.state.send_modify(|state| {
            state.readiness = readiness as i32;
            state.reason = reason;
            if let Some(cursor) = &mut state.cursor {
                cursor.sequence = cursor.sequence.saturating_add(1);
            }
        });
    }
    /// Wake watches after a local or remote authorization change. Each watch
    /// rechecks its verified credential before disclosing another event.
    pub fn authorization_changed(&self) {
        self.state.send_modify(|state| {
            if let Some(cursor) = &mut state.cursor {
                cursor.sequence = cursor.sequence.saturating_add(1);
            }
        });
    }
    /// Liveness and authorization invalidations; streams reauthorize before
    /// sending more data and terminate when the host starts draining.
    pub fn watch(&self) -> watch::Receiver<proto::HostStatus> {
        self.state.subscribe()
    }
    pub fn status(&self) -> proto::HostStatus {
        self.state.borrow().clone()
    }
    pub fn runtime_epoch(&self) -> String {
        self.state
            .borrow()
            .cursor
            .as_ref()
            .expect("host cursor")
            .runtime_epoch
            .clone()
    }
    pub fn with_runtime_epoch(self, epoch: String) -> Self {
        self.state
            .send_modify(|state| state.cursor.as_mut().expect("host cursor").runtime_epoch = epoch);
        self
    }
    pub fn accepting(&self) -> Result<(), Status> {
        if self.state.borrow().readiness != proto::HostReadiness::Ready as i32 {
            return Err(Status::unavailable(
                "The host is not ready for new operations.",
            ));
        }
        Ok(())
    }
    fn serves(&self, name: &str) -> bool {
        if name.is_empty()
            || name == "grpc.health.v1.Health"
            || matches!(
                name,
                "canopy.remote.v1.HostService" | "canopy.remote.v1.EnrollmentService"
            )
        {
            return true;
        }
        let operation = match name {
            "canopy.remote.v1.CatalogService" => "catalog",
            "canopy.remote.v1.ChatService" => "chat",
            "canopy.remote.v1.FilesService" => "files",
            "canopy.remote.v1.GitService" => "git",
            "canopy.remote.v1.TerminalService" => "terminal",
            "canopy.remote.v1.TasksService" => "tasks",
            "canopy.remote.v1.SettingsService" => "settings",
            "canopy.remote.v1.ArtifactsService" => "artifacts",
            _ => name,
        };
        self.capabilities
            .iter()
            .any(|c| c.supported && c.operation == operation)
    }
}
struct OnlineDevice {
    device: String,
    state: HostState,
}
impl Drop for OnlineDevice {
    fn drop(&mut self) {
        let mut online = self.state.online.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = online.get_mut(&self.device) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                online.remove(&self.device);
            }
        }
        drop(online);
        self.state.authorization_changed();
    }
}

#[derive(Clone)]
pub struct HostRpc {
    auth: RpcAuth,
    state: HostState,
    hub: ReplayHub,
    operations: ResourceLocks,
}
impl HostRpc {
    pub fn new(auth: RpcAuth, state: HostState, hub: ReplayHub, operations: ResourceLocks) -> Self {
        Self {
            auth,
            state,
            hub,
            operations,
        }
    }
}
fn device(grant: &DeviceGrant) -> proto::Device {
    proto::Device {
        device_id: grant.id().into(),
        name: grant.name().into(),
        scopes: scope_names(grant.scopes()),
        roots: grant.roots().into(),
        revoked: grant.revoked(),
    }
}
fn peer<T>(request: &Request<T>) -> Result<Vec<u8>, Status> {
    request
        .peer_certs()
        .and_then(|c| c.first().map(|c| c.to_vec()))
        .ok_or_else(|| Status::unauthenticated("A paired device certificate is required."))
}
#[tonic::async_trait]
impl proto::host_service_server::HostService for HostRpc {
    async fn read_diagnostics(
        &self,
        request: Request<proto::Empty>,
    ) -> Result<Response<proto::DiagnosticsSnapshot>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth
            .resource(grant.id(), host::ADMIN, "private")
            .await?;
        let snapshot = super::metrics::snapshot_for(self.auth.host_id());
        let result = proto::DiagnosticsSnapshot {
            schema_version: snapshot.schema_version(),
            process_instance_id: snapshot.process_instance_id().into(),
            summaries: snapshot
                .summaries()
                .iter()
                .map(|s| proto::StageSummary {
                    stage: s.stage().as_str().into(),
                    count: s.count(),
                    total_us: s.total_us(),
                    max_us: s.max_us(),
                    p95_upper_us: s.p95_upper_us(),
                })
                .collect(),
            records: snapshot
                .records()
                .iter()
                .map(|r| proto::TimingRecord {
                    sequence: r.sequence(),
                    host_id: r.host_id().into(),
                    resource_id: r.resource_id().into(),
                    command_id: r.command_id().into(),
                    generation: r.generation(),
                    stage: r.stage().as_str().into(),
                    duration_us: r.duration_us(),
                    state: r.state().into(),
                    error_code: r.error_code().into(),
                    device_id: r.device_id().into(),
                    command_epoch: r.command_epoch(),
                })
                .collect(),
            dropped_records: snapshot.dropped_records(),
            rejected_keys: snapshot.rejected_keys(),
        };
        self.auth
            .resource(grant.id(), host::ADMIN, "private")
            .await?;
        Ok(Response::new(result))
    }
    async fn get_info(
        &self,
        request: Request<proto::HostRequest>,
    ) -> Result<Response<proto::HostInfo>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth.check_host(&request.get_ref().host_id)?;
        Ok(Response::new(proto::HostInfo {
            host_id: self.auth.host_id().into(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: 1,
            capabilities: self.state.capabilities.to_vec(),
            command_epoch: grant.epoch(),
            limits: Some(proto::Limits {
                control_bytes: host::CONTROL_BYTES as u64,
                chunk_bytes: crate::artifacts::CHUNK_BYTES as u64,
                client_queue_bytes: 4 * 1024 * 1024,
                history_page_bytes: 1024 * 1024,
                history_page_items: 100,
                chat_processes: 32,
            }),
        }))
    }
    type WatchHostStream = RpcStream<proto::HostStatus>;
    async fn watch_host(
        &self,
        request: Request<proto::HostRequest>,
    ) -> Result<Response<Self::WatchHostStream>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth.check_host(&request.get_ref().host_id)?;
        let credential = peer(&request)?;
        let auth = self.auth.clone();
        let events = self.state.state.subscribe();
        let online = self.state.attach_device(grant.id().into());
        let result = stream::unfold(
            (events, auth, credential, true, online),
            |(mut events, auth, credential, first, online)| async move {
                if credential.is_empty() || (!first && events.changed().await.is_err()) {
                    return None;
                }
                let result = auth
                    .client()
                    .lookup_runtime_device(credential.clone())
                    .await
                    .map(|_| events.borrow_and_update().clone())
                    .map_err(storage_status);
                let terminal = result.is_err()
                    || events.borrow().readiness != proto::HostReadiness::Ready as i32;
                Some((
                    result,
                    (
                        events,
                        auth,
                        if terminal { Vec::new() } else { credential },
                        false,
                        online,
                    ),
                ))
            },
        );
        // A failed authorization terminates the stream; no idle poll is needed.
        Ok(Response::new(Box::pin(result)))
    }
    async fn list_devices(
        &self,
        request: Request<proto::DeviceListRequest>,
    ) -> Result<Response<proto::Devices>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth.check_host(&request.get_ref().host_id)?;
        self.auth
            .resource(grant.id(), host::ADMIN, "private")
            .await?;
        let request = request.into_inner();
        let (devices, next_page_token) = self
            .auth
            .client()
            .runtime_devices_page(request.page_token, request.include_revoked)
            .await
            .map_err(storage_status)?;
        self.auth
            .resource(grant.id(), host::ADMIN, "private")
            .await?;
        Ok(Response::new(proto::Devices {
            host_id: self.auth.host_id().into(),
            devices: devices.iter().map(device).collect(),
            next_page_token,
        }))
    }
    async fn get_command(
        &self,
        request: Request<proto::GetCommandRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        self.auth.check_host(&request.host_id)?;
        let key = CommandKey::remote(
            grant.id().into(),
            request.command_epoch,
            request.command_id.clone(),
        )
        .ok_or_else(|| Status::invalid_argument("Invalid command identity."))?;
        let result = self.auth.client().get_runtime_command(key, 0).await;
        let state = match result {
            Ok(Some(receipt)) => return Ok(Response::new(self.auth.receipt(&receipt))),
            Ok(None) => proto::OperationState::NotFound,
            Err(crate::settings::Error::CommandRetired) => proto::OperationState::CommandRetired,
            Err(error) => return Err(storage_status(error)),
        };
        Ok(Response::new(proto::CommandReceipt {
            host_id: self.auth.host_id().into(),
            command_id: request.command_id,
            command_epoch: request.command_epoch,
            state: state as i32,
            ..Default::default()
        }))
    }
    async fn close_command_epoch(
        &self,
        request: Request<proto::CloseCommandEpochRequest>,
    ) -> Result<Response<proto::CommandEpoch>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth.check_host(&request.get_ref().host_id)?;
        self.state.accepting()?;
        let epoch = self
            .auth
            .client()
            .close_runtime_command_epoch(grant.id().into(), request.into_inner().expected_epoch)
            .await
            .map_err(storage_status)?;
        Ok(Response::new(proto::CommandEpoch {
            host_id: self.auth.host_id().into(),
            epoch,
        }))
    }
    async fn resolve_command(
        &self,
        request: Request<proto::GetCommandRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.state.accepting()?;
        let request = request.into_inner();
        self.auth.check_host(&request.host_id)?;
        let key = CommandKey::remote(grant.id().into(), request.command_epoch, request.command_id)
            .ok_or_else(|| Status::invalid_argument("Invalid command identity."))?;
        let owned = self.operations.begin()?;
        let this = self.clone();
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            let receipt = this
                .auth
                .client()
                .resolve_runtime_command(key)
                .await
                .map_err(storage_status)?;
            Ok::<_, Status>(this.auth.receipt(&receipt))
        })
        .await
        .map_err(|_| {
            Status::unavailable(
                "The command review outcome is uncertain. Read the command before retrying.",
            )
        })??;
        Ok(Response::new(receipt))
    }
    async fn revoke_device(
        &self,
        request: Request<proto::RevokeDeviceRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        self.auth.scope(&grant, host::ADMIN)?;
        self.state.accepting()?;
        let mut request = request.into_inner();
        let meta = request
            .command
            .take()
            .ok_or_else(|| Status::invalid_argument("Command is required."))?;
        if uuid::Uuid::parse_str(&request.device_id).is_err() {
            return Err(Status::invalid_argument("Invalid device."));
        }
        let input = self.auth.input(
            &meta,
            &grant,
            "host.revoke",
            request.device_id.clone(),
            request.encode_to_vec(),
            "private".into(),
        )?;
        let this = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            let receipt = this
                .auth
                .client()
                .accept_runtime_command(input, host::ADMIN)
                .await
                .map_err(storage_status)?;
            if receipt.state() != CommandState::Accepted {
                return Ok::<_, Status>(this.auth.receipt(&receipt));
            }
            this.auth
                .client()
                .transition_runtime_command(
                    receipt.key().clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .map_err(storage_status)?;
            let result = this
                .auth
                .client()
                .revoke_runtime_device(request.device_id.clone())
                .await;
            this.hub.revoke_device(&request.device_id);
            this.state.authorization_changed();
            let (state, bytes) = match result {
                Ok(()) => (CommandState::Succeeded, None),
                Err(error) => (
                    CommandState::Failed,
                    Some(
                        proto::DomainError {
                            code: "revoke_failed".into(),
                            message: error.to_string(),
                            retryable: false,
                        }
                        .encode_to_vec(),
                    ),
                ),
            };
            let final_receipt = this
                .auth
                .client()
                .transition_runtime_command(
                    receipt.key().clone(),
                    CommandState::Attempting,
                    state,
                    bytes,
                )
                .await
                .map_err(storage_status)?;
            Ok(this.auth.receipt(&final_receipt))
        })
        .await
        .map_err(|_| Status::internal("Device operation stopped."))??;
        Ok(Response::new(receipt))
    }
}

/// Standard grpc.health.v1, with the same verified-device boundary as other
/// established services. Enrollment is the single unauthenticated exception.
#[derive(Clone)]
pub struct HealthRpc {
    auth: RpcAuth,
    state: HostState,
}
impl HealthRpc {
    pub fn new(auth: RpcAuth, state: HostState) -> Self {
        Self { auth, state }
    }
    fn response(state: &HostState, name: &str) -> tonic_health::pb::HealthCheckResponse {
        use tonic_health::pb::health_check_response::ServingStatus;
        let status = if !state.serves(name) {
            ServingStatus::ServiceUnknown
        } else if state.state.borrow().readiness == proto::HostReadiness::Ready as i32 {
            ServingStatus::Serving
        } else {
            ServingStatus::NotServing
        };
        tonic_health::pb::HealthCheckResponse {
            status: status as i32,
        }
    }
}
#[tonic::async_trait]
impl tonic_health::pb::health_server::Health for HealthRpc {
    async fn check(
        &self,
        request: Request<tonic_health::pb::HealthCheckRequest>,
    ) -> Result<Response<tonic_health::pb::HealthCheckResponse>, Status> {
        self.auth.authenticate(&request).await?;
        if !self.state.serves(&request.get_ref().service) {
            return Err(Status::not_found("Unknown service."));
        }
        Ok(Response::new(Self::response(
            &self.state,
            &request.into_inner().service,
        )))
    }
    type WatchStream = RpcStream<tonic_health::pb::HealthCheckResponse>;
    async fn watch(
        &self,
        request: Request<tonic_health::pb::HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        self.auth.authenticate(&request).await?;
        let credential = peer(&request)?;
        let name = request.into_inner().service;
        let this = self.clone();
        let events = self.state.state.subscribe();
        let result = stream::unfold(
            (events, this, credential, name, true),
            |(mut events, this, credential, name, first)| async move {
                if credential.is_empty() || (!first && events.changed().await.is_err()) {
                    return None;
                }
                let result = this
                    .auth
                    .client()
                    .lookup_runtime_device(credential.clone())
                    .await
                    .map_err(storage_status)
                    .map(|_| Self::response(&this.state, &name));
                events.borrow_and_update();
                let terminal = result.is_err() || this.state.accepting().is_err();
                Some((
                    result,
                    (
                        events,
                        this,
                        if terminal { Vec::new() } else { credential },
                        name,
                        false,
                    ),
                ))
            },
        );
        Ok(Response::new(Box::pin(result)))
    }
}
