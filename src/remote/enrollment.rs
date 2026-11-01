//! Single-use enrollment. CSR signatures and mTLS are standard library protocols;
//! the QR token only gates a locally approved request over the pinned TLS channel.
use super::{
    auth::{RpcAuth, storage_status},
    tls::{DeviceCertificate, HostIdentity},
};
use crate::api::proto;
use crate::host::{DeviceGrant, DeviceRegistration};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use prost::Message;
use sha2::Sha256;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tonic::{Request, Response, Status};
const WINDOW: Duration = Duration::from_secs(300);
const RETRIEVAL: Duration = Duration::from_secs(600);

struct Pending {
    id: String,
    name: String,
    csr: Vec<u8>,
    fingerprint: [u8; 32],
    stage: Stage,
}
enum Stage {
    Pending,
    Approving,
    Denied,
    Approved {
        certificate: DeviceCertificate,
        grant: DeviceGrant,
        until: Instant,
    },
}
struct Window {
    tag: [u8; 32],
    expires: Instant,
    pending: Option<Pending>,
}
pub struct PendingDevice {
    id: String,
    name: String,
    csr_fingerprint: [u8; 32],
}
impl PendingDevice {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn csr_fingerprint(&self) -> &[u8; 32] {
        &self.csr_fingerprint
    }
}
#[derive(Clone)]
pub struct EnrollmentRpc {
    auth: RpcAuth,
    identity: Arc<HostIdentity>,
    window: Arc<Mutex<Option<Window>>>,
    changed: async_channel::Sender<()>,
}
impl EnrollmentRpc {
    pub fn new(auth: RpcAuth, identity: Arc<HostIdentity>) -> (Self, async_channel::Receiver<()>) {
        let (changed, events) = async_channel::bounded(1);
        (
            Self {
                auth,
                identity,
                window: Arc::new(Mutex::new(None)),
                changed,
            },
            events,
        )
    }
    /// Local user action only. Opening a window never changes listener binding.
    pub fn begin(&self, address: String, port: u16) -> Result<proto::PairingInvite, Status> {
        if address.is_empty() || address.len() > 256 || port == 0 {
            return Err(Status::invalid_argument(
                "Select a reachable listener address.",
            ));
        }
        let mut state = self
            .window
            .lock()
            .map_err(|_| Status::internal("Pairing state unavailable."))?;
        if state
            .as_ref()
            .and_then(|w| w.pending.as_ref())
            .is_some_and(|p| matches!(p.stage, Stage::Approving))
        {
            return Err(Status::failed_precondition(
                "Device approval is still being saved.",
            ));
        }
        let mut token = Vec::with_capacity(32);
        token.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        token.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        *state = Some(Window {
            tag: tag(&token)?,
            expires: Instant::now() + WINDOW,
            pending: None,
        });
        let invite = proto::PairingInvite {
            protocol_version: 1,
            host_id: self.auth.host_id().into(),
            server_name: self.identity.server_name().into(),
            address,
            port: u32::from(port),
            host_certificate: self.identity.certificate().into(),
            certificate_sha256: self.identity.fingerprint().into(),
            one_time_token: token,
            expires_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |v| v.as_secs())
                + WINDOW.as_secs(),
        };
        let _ = self.changed.try_send(());
        Ok(invite)
    }
    pub fn invite_uri(invite: &proto::PairingInvite) -> String {
        format!(
            "canopy://pair?v=1&data={}",
            URL_SAFE_NO_PAD.encode(invite.encode_to_vec())
        )
    }
    pub fn pending(&self) -> Option<PendingDevice> {
        let state = self.window.lock().ok()?;
        let window = state.as_ref()?;
        if Instant::now() >= window.expires {
            return None;
        }
        let pending = window.pending.as_ref()?;
        if !matches!(pending.stage, Stage::Pending) {
            return None;
        }
        Some(PendingDevice {
            id: pending.id.clone(),
            name: pending.name.clone(),
            csr_fingerprint: pending.fingerprint,
        })
    }
    pub fn deny(&self, request_id: &str) -> Result<(), Status> {
        let mut state = self
            .window
            .lock()
            .map_err(|_| Status::internal("Pairing state unavailable."))?;
        let pending = state
            .as_mut()
            .and_then(|w| w.pending.as_mut())
            .filter(|p| p.id == request_id)
            .ok_or_else(|| Status::not_found("Pairing request expired."))?;
        if !matches!(pending.stage, Stage::Pending) {
            return Err(Status::failed_precondition(
                "This request is already being resolved.",
            ));
        }
        pending.stage = Stage::Denied;
        let _ = self.changed.try_send(());
        Ok(())
    }
    /// The caller is the host's approval UI, never an unauthenticated RPC.
    pub async fn approve(
        &self,
        request_id: &str,
        scopes: u32,
        roots: Vec<String>,
    ) -> Result<(), Status> {
        let (issued, registration) = {
            let mut state = self
                .window
                .lock()
                .map_err(|_| Status::internal("Pairing state unavailable."))?;
            let window = state
                .as_mut()
                .filter(|w| Instant::now() < w.expires)
                .ok_or_else(|| Status::not_found("Pairing request expired."))?;
            let pending = window
                .pending
                .as_mut()
                .filter(|p| p.id == request_id)
                .ok_or_else(|| Status::not_found("Pairing request expired."))?;
            if !matches!(pending.stage, Stage::Pending) {
                return Err(Status::failed_precondition(
                    "This request is already being resolved.",
                ));
            }
            // Keep validation and certificate creation before claiming the
            // request. An invalid local grant must not strand it in Approving.
            let device = uuid::Uuid::new_v4().to_string();
            let issued = self
                .identity
                .issue_device(&device, &pending.csr)
                .map_err(Status::invalid_argument)?;
            let registration = DeviceRegistration::new(
                device,
                pending.name.clone(),
                issued.der().into(),
                scopes,
                roots,
            )
            .ok_or_else(|| Status::invalid_argument("Invalid device grants."))?;
            pending.stage = Stage::Approving;
            (issued, registration)
        };
        let saved = self
            .auth
            .client()
            .register_runtime_device(registration)
            .await;
        // Reconcile an acknowledgement lost after the metadata commit.
        let grant = self
            .auth
            .client()
            .lookup_runtime_device(issued.der().into())
            .await;
        let outcome = match grant {
            Ok(grant) => Ok(grant),
            Err(error) => Err(saved.err().unwrap_or(error)),
        };
        let mut state = self
            .window
            .lock()
            .map_err(|_| Status::internal("Pairing state unavailable."))?;
        let pending = state
            .as_mut()
            .and_then(|w| w.pending.as_mut())
            .filter(|p| p.id == request_id)
            .ok_or_else(|| Status::not_found("Pairing request expired."))?;
        match outcome {
            Ok(grant) => {
                pending.stage = Stage::Approved {
                    certificate: issued,
                    grant,
                    until: Instant::now() + RETRIEVAL,
                }
            }
            Err(error) => {
                pending.stage = Stage::Denied;
                let _ = self.changed.try_send(());
                return Err(storage_status(error));
            }
        }
        let _ = self.changed.try_send(());
        Ok(())
    }
}
fn tag(token: &[u8]) -> Result<[u8; 32], Status> {
    if token.len() != 32 {
        return Err(Status::permission_denied(
            "Invalid or expired enrollment token.",
        ));
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(token)
        .map_err(|_| Status::permission_denied("Invalid enrollment token."))?;
    mac.update(b"Canopy enrollment gate v1");
    Ok(mac.finalize().into_bytes().into())
}
fn verifies(token: &[u8], expected: &[u8; 32]) -> bool {
    if token.len() != 32 {
        return false;
    }
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(token) else {
        return false;
    };
    mac.update(b"Canopy enrollment gate v1");
    mac.verify_slice(expected).is_ok()
}
fn status(window: &Window, pending: &Pending) -> proto::PairingStatus {
    let now = Instant::now();
    let mut result = proto::PairingStatus {
        request_id: pending.id.clone(),
        state: proto::PairingState::Pending as i32,
        certificate: None,
        certificate_chain: vec![],
        device: None,
    };
    match &pending.stage {
        Stage::Approved {
            certificate,
            grant,
            until,
        } if now < *until => {
            result.state = proto::PairingState::Approved as i32;
            result.certificate = Some(certificate.der().into());
            result.certificate_chain = certificate.chain().to_vec();
            result.device = Some(proto::Device {
                device_id: grant.id().into(),
                name: grant.name().into(),
                scopes: scope_values(grant.scopes()),
                roots: grant.roots().into(),
                revoked: false,
            });
        }
        Stage::Approved { .. } => result.state = proto::PairingState::Expired as i32,
        _ if now >= window.expires => result.state = proto::PairingState::Expired as i32,
        Stage::Denied => result.state = proto::PairingState::Denied as i32,
        Stage::Pending | Stage::Approving => {}
    }
    result
}
pub(crate) fn scope_values(scopes: u32) -> Vec<i32> {
    [
        (crate::host::READ, proto::Scope::Read),
        (crate::host::CHAT, proto::Scope::Chat),
        (crate::host::APPROVE, proto::Scope::Approve),
        (crate::host::FILES_GIT, proto::Scope::FilesGit),
        (crate::host::TERMINAL, proto::Scope::Terminal),
        (crate::host::TASKS, proto::Scope::Tasks),
        (crate::host::ADMIN, proto::Scope::Admin),
    ]
    .into_iter()
    .filter(|(bit, _)| scopes & bit != 0)
    .map(|(_, scope)| scope as i32)
    .collect()
}
#[tonic::async_trait]
impl proto::enrollment_service_server::EnrollmentService for EnrollmentRpc {
    async fn request_pairing(
        &self,
        request: Request<proto::RequestPairingRequest>,
    ) -> Result<Response<proto::PairingStatus>, Status> {
        let request = request.into_inner();
        self.auth.check_host(
            &request
                .environment
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Environment is required."))?
                .host_id,
        )?;
        if request.device_name.trim().is_empty()
            || request.device_name.len() > 256
            || request.device_name.chars().any(char::is_control)
        {
            return Err(Status::invalid_argument("Invalid device name."));
        }
        let fingerprint = HostIdentity::verify_csr(&request.certificate_signing_request)
            .map_err(Status::invalid_argument)?;
        let mut state = self
            .window
            .lock()
            .map_err(|_| Status::internal("Pairing state unavailable."))?;
        let window = state
            .as_mut()
            .ok_or_else(|| Status::permission_denied("Pairing is not enabled."))?;
        if !verifies(&request.one_time_token, &window.tag) {
            return Err(Status::permission_denied(
                "Invalid or expired enrollment token.",
            ));
        }
        if let Some(pending) = &window.pending {
            if pending.csr != request.certificate_signing_request
                || pending.name != request.device_name
            {
                return Err(Status::permission_denied(
                    "The enrollment token has already been used.",
                ));
            }
            return Ok(Response::new(status(window, pending)));
        }
        if Instant::now() >= window.expires {
            return Err(Status::permission_denied(
                "Invalid or expired enrollment token.",
            ));
        }
        window.pending = Some(Pending {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.device_name,
            csr: request.certificate_signing_request,
            fingerprint,
            stage: Stage::Pending,
        });
        let result = status(window, window.pending.as_ref().expect("request installed"));
        let _ = self.changed.try_send(());
        Ok(Response::new(result))
    }
    async fn get_pairing_status(
        &self,
        request: Request<proto::GetPairingStatusRequest>,
    ) -> Result<Response<proto::PairingStatus>, Status> {
        let request = request.into_inner();
        self.auth.check_host(
            &request
                .environment
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("Environment is required."))?
                .host_id,
        )?;
        let state = self
            .window
            .lock()
            .map_err(|_| Status::internal("Pairing state unavailable."))?;
        let window = state
            .as_ref()
            .ok_or_else(|| Status::not_found("Pairing request expired."))?;
        if !verifies(&request.proof, &window.tag) {
            return Err(Status::permission_denied("Invalid enrollment proof."));
        }
        let pending = window
            .pending
            .as_ref()
            .filter(|p| p.id == request.request_id)
            .ok_or_else(|| Status::not_found("Pairing request expired."))?;
        Ok(Response::new(status(window, pending)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::proto::enrollment_service_server::EnrollmentService;

    #[tokio::test]
    async fn approval_binds_one_key_and_bad_grants_do_not_strand_pairing() {
        let dir = tempfile::tempdir().unwrap();
        let client = crate::settings::SettingsClient::create(&dir.path().join("test.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let identity = Arc::new(HostIdentity::generate(host.id()).unwrap());
        let auth = RpcAuth::new(host.id().into(), client.clone()).unwrap();
        let (rpc, _changed) = EnrollmentRpc::new(auth, identity);
        let invite = rpc.begin("127.0.0.1".into(), 4433).unwrap();
        let key = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        let csr = params.serialize_request(&key).unwrap().der().to_vec();
        let request = proto::RequestPairingRequest {
            environment: Some(proto::EnvironmentRef {
                host_id: invite.host_id.clone(),
            }),
            one_time_token: invite.one_time_token.clone(),
            certificate_signing_request: csr,
            device_name: "phone".into(),
        };
        let first = rpc
            .request_pairing(Request::new(request.clone()))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            rpc.request_pairing(Request::new(request.clone()))
                .await
                .unwrap()
                .into_inner()
                .request_id,
            first.request_id
        );
        let second_key = rcgen::KeyPair::generate().unwrap();
        let mut substitution = request;
        substitution.certificate_signing_request = params
            .serialize_request(&second_key)
            .unwrap()
            .der()
            .to_vec();
        assert_eq!(
            rpc.request_pairing(Request::new(substitution))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::PermissionDenied
        );
        assert!(
            rpc.approve(&first.request_id, 128, vec!["private".into()])
                .await
                .is_err()
        );
        assert_eq!(rpc.pending().unwrap().id(), first.request_id);
        rpc.approve(&first.request_id, crate::host::READ, vec!["private".into()])
            .await
            .unwrap();
        assert!(
            rpc.get_pairing_status(Request::new(proto::GetPairingStatusRequest {
                environment: Some(proto::EnvironmentRef {
                    host_id: invite.host_id.clone()
                }),
                request_id: first.request_id.clone(),
                proof: vec![0; 32]
            }))
            .await
            .is_err()
        );
        let approved = rpc
            .get_pairing_status(Request::new(proto::GetPairingStatusRequest {
                environment: Some(proto::EnvironmentRef {
                    host_id: invite.host_id.clone(),
                }),
                request_id: first.request_id,
                proof: invite.one_time_token,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(approved.state, proto::PairingState::Approved as i32);
        let certificate = approved.certificate.unwrap();
        let grant = client
            .lookup_runtime_device(certificate.clone())
            .await
            .unwrap();
        assert_eq!(grant.scopes(), crate::host::READ);
        client
            .revoke_runtime_device(grant.id().into())
            .await
            .unwrap();
        assert!(matches!(
            client.lookup_runtime_device(certificate).await,
            Err(crate::settings::Error::NotAuthorized)
        ));
        client.shutdown().await.unwrap();
    }
}
