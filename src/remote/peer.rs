//! Reciprocal desktop trust over an already authenticated connection. A CSR
//! binds the reverse credential to the host key pinned by the first pairing.
//! No listening socket or trust decision is created by an unauthenticated RPC.
use super::{
    auth::RpcAuth,
    client::{ClientRegistry, store::HostRecord},
    host::HostState,
    proto as p,
    resources::ResourceLocks,
    tls::HostIdentity,
};
use crate::{
    host::{self, DeviceRegistration},
    settings::SettingsClient,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tonic::{Request, Response, Status, transport::Channel};

pub struct LocalPeer {
    identity: Arc<HostIdentity>,
    client: SettingsClient,
    address: std::net::SocketAddr,
}
impl LocalPeer {
    pub fn new(
        identity: Arc<HostIdentity>,
        client: SettingsClient,
        address: std::net::SocketAddr,
    ) -> Self {
        Self {
            identity,
            client,
            address,
        }
    }
    pub fn host_id(&self) -> &str {
        self.identity.host_id()
    }
    pub(crate) fn key(&self) -> &str {
        self.identity.peer_key()
    }
    pub(crate) fn port(&self) -> u16 {
        self.address.port()
    }
    pub(crate) fn request(&self) -> Result<Vec<u8>, String> {
        self.identity.peer_request()
    }
    fn description(&self, return_port: Option<u16>) -> Result<p::PeerHost, String> {
        let addresses = if return_port.is_some() {
            vec!["127.0.0.1".into()]
        } else if self.address.ip().is_unspecified() {
            super::network::local_addresses()?
                .into_iter()
                .take(8)
                .map(|a| a.ip().to_string())
                .collect()
        } else if self.address.ip().is_loopback() {
            vec![]
        } else {
            vec![self.address.ip().to_string()]
        };
        Ok(p::PeerHost {
            host_id: self.host_id().into(),
            name: std::env::var("HOSTNAME")
                .or_else(|_| std::env::var("COMPUTERNAME"))
                .ok()
                .filter(|s| {
                    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
                })
                .unwrap_or_else(|| "Canopy computer".into()),
            server_name: self.identity.server_name().into(),
            certificate: self.identity.certificate().into(),
            certificate_sha256: self.identity.fingerprint().into(),
            addresses,
            port: return_port.unwrap_or(self.address.port()).into(),
            return_tunnel: return_port.is_some(),
        })
    }
    async fn certificate(
        &self,
        peer: &HostRecord,
        csr: &[u8],
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        HostIdentity::csr_matches_certificate(csr, &peer.certificate)?;
        let id = device_id(self.host_id(), peer.id.as_str());
        let cert = match self
            .client
            .runtime_device_certificate(id.clone())
            .await
            .map_err(|e| e.to_string())?
        {
            Some(cert) => cert,
            None => {
                let cert = self.identity.issue_device(&id, csr)?;
                let registration = DeviceRegistration::new(
                    id.clone(),
                    peer.name.clone(),
                    cert.der().into(),
                    host::ALL_SCOPES,
                    vec!["*".into()],
                )
                .ok_or("Invalid peer identity.")?;
                // A lost acknowledgement/concurrent completion is reconciled by
                // the stable ID, never by generating another authorized device.
                let result = self.client.register_runtime_device(registration).await;
                match self
                    .client
                    .runtime_device_certificate(id.clone())
                    .await
                    .map_err(|e| e.to_string())?
                {
                    Some(cert) => cert,
                    None => {
                        return Err(result
                            .err()
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "Peer registration was not saved.".into()));
                    }
                }
            }
        };
        HostIdentity::csr_matches_certificate(csr, &cert)?;
        self.client
            .lookup_runtime_device(cert.clone())
            .await
            .map_err(|e| e.to_string())?;
        Ok((cert, vec![self.identity.issuer_certificate().to_vec()]))
    }
    pub(crate) async fn connect_back(
        &self,
        channel: Channel,
        record: &HostRecord,
        return_port: Option<u16>,
    ) -> Result<bool, String> {
        if record.id.as_str() == self.host_id() {
            return Ok(true);
        }
        let mut service = p::peer_service_client::PeerServiceClient::new(channel);
        let mut request = Request::new(p::HostRequest {
            host_id: record.id.as_str().into(),
        });
        request.set_timeout(std::time::Duration::from_secs(15));
        let challenge = match service.get_peer_request(request).await {
            Ok(response) => response.into_inner(),
            Err(status)
                if matches!(
                    status.code(),
                    tonic::Code::PermissionDenied | tonic::Code::Unimplemented
                ) =>
            {
                return Ok(false);
            }
            Err(status) => return Err(super::client::rpc_error(status)),
        };
        if challenge.host_id != record.id.as_str() {
            return Err("The reciprocal host identity changed.".into());
        }
        let (certificate, certificate_chain) = self
            .certificate(record, &challenge.certificate_signing_request)
            .await?;
        let description = self.description(return_port)?;
        let mut request = Request::new(p::ConnectPeerRequest {
            peer: Some(description),
            certificate,
            certificate_chain,
            host_id: record.id.as_str().into(),
        });
        request.set_timeout(std::time::Duration::from_secs(20));
        service
            .connect_peer(request)
            .await
            .map_err(super::client::rpc_error)?;
        Ok(true)
    }
}
#[cfg(test)]
mod tests;

pub(crate) fn device_id(issuer: &str, peer: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"Canopy reciprocal device v1\0");
    hash.update(issuer);
    hash.update([0]);
    hash.update(peer);
    let mut bytes: [u8; 16] = hash.finalize()[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

#[derive(Clone)]
pub struct PeerRpc {
    auth: RpcAuth,
    local: Arc<LocalPeer>,
    registry: ClientRegistry,
    state: HostState,
    locks: ResourceLocks,
}
impl PeerRpc {
    pub fn new(
        auth: RpcAuth,
        local: Arc<LocalPeer>,
        registry: ClientRegistry,
        state: HostState,
        locks: ResourceLocks,
    ) -> Self {
        Self {
            auth,
            local,
            registry,
            state,
            locks,
        }
    }
    async fn authorize<T>(&self, request: &Request<T>) -> Result<String, Status> {
        self.state.accepting()?;
        let device = self.auth.authenticate(request).await?;
        self.auth
            .resource(device.id(), host::ADMIN, "private")
            .await?;
        Ok(device.id().into())
    }
}
#[tonic::async_trait]
impl p::peer_service_server::PeerService for PeerRpc {
    async fn get_peer_request(
        &self,
        request: Request<p::HostRequest>,
    ) -> Result<Response<p::PeerCertificateRequest>, Status> {
        self.authorize(&request).await?;
        self.auth.check_host(&request.get_ref().host_id)?;
        Ok(Response::new(p::PeerCertificateRequest {
            host_id: self.local.host_id().into(),
            certificate_signing_request: self.local.request().map_err(Status::internal)?,
        }))
    }
    async fn connect_peer(
        &self,
        request: Request<p::ConnectPeerRequest>,
    ) -> Result<Response<p::Empty>, Status> {
        let device = self.authorize(&request).await?;
        self.auth.check_host(&request.get_ref().host_id)?;
        let observed = request.remote_addr().map(|addr| addr.ip());
        let body = request.into_inner();
        let peer = body
            .peer
            .ok_or_else(|| Status::invalid_argument("Missing peer identity."))?;
        let operation = self.locks.begin()?;
        let this = self.clone();
        // The effect remains owned until the registry commits, even if the
        // caller disconnects. Shutdown waits for it through ResourceLocks.
        let host = tokio::spawn(async move {
            let _operation = operation;
            let grant = this.auth.resource(&device, host::ADMIN, "private").await?;
            let mut peer = peer;
            peer.name = grant.name().into();
            let registry = this.registry.clone();
            let local = this.local.clone();
            let host = tokio::task::spawn_blocking(move || {
                registry.install_peer(
                    &local,
                    peer,
                    body.certificate,
                    body.certificate_chain,
                    observed,
                )
            })
            .await
            .map_err(|_| Status::internal("Peer storage worker stopped."))?
            .map_err(Status::failed_precondition)?;
            Ok::<_, Status>(host)
        })
        .await
        .map_err(|_| Status::internal("Peer connection worker stopped."))??;
        // Starting the catalog is independent of the response. Failed routes are
        // visible in Connections and retry without repeating trust enrollment.
        host.start_connection();
        Ok(Response::new(p::Empty {}))
    }
}
