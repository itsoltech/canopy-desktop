use super::{
    HostId,
    store::{CredentialStore, HostRecord},
};
use crate::{api::proto as p, remote::tls};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tonic::transport::Identity;

pub struct PairingInvite {
    wire: p::PairingInvite,
}
impl PairingInvite {
    pub fn parse(uri: &str) -> Result<Self, String> {
        if uri.len() > 32768 {
            return Err("Pairing code is too large.".into());
        }
        let url = url::Url::parse(uri).map_err(|_| "Invalid pairing code.")?;
        if url.scheme() != "canopy" || url.host_str() != Some("pair") {
            return Err("Expected a Canopy pairing code.".into());
        }
        if url.username() != ""
            || url.password().is_some()
            || url.port().is_some()
            || !matches!(url.path(), "" | "/")
            || url.fragment().is_some()
        {
            return Err("Invalid pairing code address.".into());
        }
        let mut query = BTreeMap::new();
        for (key, value) in url.query_pairs() {
            if !matches!(key.as_ref(), "v" | "data") || query.insert(key, value).is_some() {
                return Err("Ambiguous pairing code parameters.".into());
            }
        }
        if query.get("v").map(|s| s.as_ref()) != Some("1") {
            return Err("Unsupported pairing protocol.".into());
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(query.get("data").ok_or("Missing pairing data.")?.as_bytes())
            .map_err(|_| "Invalid pairing data.")?;
        let wire =
            p::PairingInvite::decode(bytes.as_slice()).map_err(|_| "Invalid pairing data.")?;
        HostId::parse(&wire.host_id)?;
        if wire.protocol_version != 1
            || wire.one_time_token.len() != 32
            || wire.port == 0
            || wire.port > 65535
            || wire.expires_unix <= chrono::Utc::now().timestamp() as u64
            || wire.certificate_sha256 != Sha256::digest(&wire.host_certificate).as_slice()
        {
            return Err("Pairing code is expired or invalid.".into());
        }
        let invite = Self { wire };
        tls::client_endpoint(
            &invite.endpoint()?,
            &invite.wire.server_name,
            &invite.wire.host_certificate,
            &invite.wire.certificate_sha256,
            None,
        )?;
        Ok(invite)
    }
    pub fn host_id(&self) -> &str {
        &self.wire.host_id
    }
    pub fn endpoint(&self) -> Result<String, String> {
        let mut url = url::Url::parse("https://localhost").unwrap();
        url.set_host(Some(&self.wire.address))
            .map_err(|_| "Invalid host address.")?;
        url.set_port(Some(self.wire.port as u16))
            .map_err(|_| "Invalid host port.")?;
        Ok(url.to_string())
    }
    pub async fn enroll(
        self,
        name: String,
        vault: Arc<dyn CredentialStore>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<HostRecord, String> {
        self.enroll_via(name, vault, cancelled, None, None).await
    }
    pub async fn enroll_via(
        self,
        name: String,
        vault: Arc<dyn CredentialStore>,
        cancelled: Arc<AtomicBool>,
        address_override: Option<String>,
        route: Option<super::store::SshRoute>,
    ) -> Result<HostRecord, String> {
        let address = address_override.unwrap_or(self.endpoint()?);
        tls::client_endpoint(
            &address,
            &self.wire.server_name,
            &self.wire.host_certificate,
            &self.wire.certificate_sha256,
            None,
        )?;
        let transport_address =
            url::Url::parse(&address).map_err(|_| "Invalid pairing address.")?;
        let first_hop = route
            .as_ref()
            .map(|ssh| (ssh.address.clone(), ssh.port))
            .unwrap_or_else(|| {
                (
                    transport_address.host_str().unwrap_or_default().to_owned(),
                    transport_address.port_or_known_default().unwrap_or(443),
                )
            });
        let reachable = tokio::task::spawn_blocking(move || {
            crate::remote::network::reachable_addresses(&first_hop.0, first_hop.1)
        })
        .await
        .map_err(|_| "Network route lookup stopped.")??;
        if cancelled.load(Ordering::Acquire) {
            return Err("Pairing cancelled.".into());
        }
        let tunnel = if let Some(route) = route.clone() {
            let mut route = route;
            route.address = reachable[0].to_string();
            let vault = vault.clone();
            let address = address.clone();
            Some(
                tokio::task::spawn_blocking(move || {
                    super::ssh::SshTunnel::open(route, &address, vault)
                })
                .await
                .map_err(|_| "SSH worker stopped.")??,
            )
        } else {
            None
        };
        let (endpoint, addresses) = if let Some(tunnel) = &tunnel {
            (tunnel.address(), vec!["127.0.0.1".parse().unwrap()])
        } else {
            (transport_address.as_str(), reachable)
        };
        let channel = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            super::routes::connect_tls(
                endpoint,
                addresses,
                &self.wire.server_name,
                &self.wire.host_certificate,
                &self.wire.certificate_sha256,
                None,
                || cancelled.load(Ordering::Acquire),
            ),
        )
        .await
        .map_err(|_| "The pairing address did not respond in time.")??;
        let key = rcgen::KeyPair::generate().map_err(|_| "Could not create a device key.")?;
        let csr = rcgen::CertificateParams::new(Vec::<String>::new())
            .map_err(|_| "Could not create a device request.")?
            .serialize_request(&key)
            .map_err(|_| "Could not sign a device request.")?;
        let mut service = p::enrollment_service_client::EnrollmentServiceClient::new(channel);
        let mut status = service
            .request_pairing(p::RequestPairingRequest {
                environment: Some(p::EnvironmentRef {
                    host_id: self.wire.host_id.clone(),
                }),
                one_time_token: self.wire.one_time_token.clone(),
                certificate_signing_request: csr.der().to_vec(),
                device_name: name.clone(),
            })
            .await
            .map_err(|e| e.message().to_owned())?
            .into_inner();
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err("Pairing cancelled; no device credential was installed.".into());
            }
            if status.state == p::PairingState::Approved as i32 {
                break;
            }
            if status.state != p::PairingState::Pending as i32
                || self.wire.expires_unix <= chrono::Utc::now().timestamp() as u64
            {
                return Err("Pairing was denied or expired.".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            status = service
                .get_pairing_status(p::GetPairingStatusRequest {
                    environment: Some(p::EnvironmentRef {
                        host_id: self.wire.host_id.clone(),
                    }),
                    request_id: status.request_id,
                    proof: self.wire.one_time_token.clone(),
                })
                .await
                .map_err(|e| e.message().to_owned())?
                .into_inner();
        }
        let cert = status
            .certificate
            .ok_or("Host did not issue a certificate.")?;
        let device = status
            .device
            .ok_or("Host did not return a device identity.")?;
        let reference = uuid::Uuid::new_v4().to_string();
        let pem = pem_certificate(&cert)
            + &status
                .certificate_chain
                .iter()
                .map(|der| pem_certificate(der))
                .collect::<String>();
        // Validate key/leaf binding before persisting credentials.
        let provider = rustls::crypto::ring::default_provider();
        use rustls::pki_types::{PrivateKeyDer, pem::PemObject};
        rustls::sign::CertifiedKey::from_der(
            vec![cert.into()],
            PrivateKeyDer::from_pem_slice(key.serialize_pem().as_bytes())
                .map_err(|_| "Invalid device key.")?,
            &provider,
        )
        .map_err(|_| "Issued certificate does not match this device key.")?;
        save_bundle(vault.as_ref(), &reference, &key.serialize_pem(), &pem)?;
        Ok(HostRecord {
            mutual: false,
            passive_peer: false,
            auto_connect: true,
            id: HostId::parse(&self.wire.host_id)?,
            name: self.wire.address.clone(),
            endpoint: address,
            server_name: self.wire.server_name,
            certificate: self.wire.host_certificate,
            fingerprint: self.wire.certificate_sha256,
            device_id: device.device_id,
            credential_ref: reference,
            ssh: route,
            routes: vec![],
            last_seen: None,
            epoch: None,
            closing_epoch: None,
            catalog: vec![],
            outbox: vec![],
            mutations: vec![],
            views: BTreeMap::new(),
            uploads: vec![],
            chats: BTreeMap::new(),
        })
    }
}
pub(crate) fn pem_certificate(der: &[u8]) -> String {
    let encoded = STANDARD.encode(der);
    format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        encoded
            .as_bytes()
            .chunks(64)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    )
}
pub(crate) fn save_bundle(
    vault: &dyn CredentialStore,
    reference: &str,
    key: &str,
    cert: &str,
) -> Result<(), String> {
    // A chain can exceed the Windows per-slot budget. Store fixed bounded chunks.
    let parts = cert
        .as_bytes()
        .chunks(2000)
        .map(|v| std::str::from_utf8(v).unwrap())
        .collect::<Vec<_>>();
    if parts.len() > 8 {
        return Err("Device certificate chain is too large.".into());
    }
    let mut written = vec![];
    for (id, value) in std::iter::once((format!("{reference}-key"), key.to_owned()))
        .chain(
            parts
                .iter()
                .enumerate()
                .map(|(i, p)| (format!("{reference}-cert-{i}"), p.to_string())),
        )
        .chain(std::iter::once((
            format!("{reference}-parts"),
            parts.len().to_string(),
        )))
    {
        written.push(id.clone());
        if vault.save(&id, &value).is_err() {
            let mut clean = true;
            for id in &written {
                clean &= vault.remove(id).is_ok();
            }
            return Err(if clean {
                "Could not store device credentials."
            } else {
                "Could not store device credentials; vault cleanup failed."
            }
            .into());
        }
    }
    Ok(())
}
pub(crate) fn identity(vault: &dyn CredentialStore, reference: &str) -> Result<Identity, String> {
    let key = vault.load(&format!("{reference}-key"))?;
    let count: usize = vault
        .load(&format!("{reference}-parts"))?
        .parse()
        .map_err(|_| "Invalid credential manifest.")?;
    if !(1..=8).contains(&count) {
        return Err("Invalid credential manifest.".into());
    }
    let mut cert = String::new();
    for i in 0..count {
        cert.push_str(&vault.load(&format!("{reference}-cert-{i}"))?);
    }
    Ok(Identity::from_pem(cert, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct FakeVault {
        values: Mutex<BTreeMap<String, String>>,
        fail: bool,
    }
    impl CredentialStore for FakeVault {
        fn load(&self, id: &str) -> Result<String, String> {
            self.values
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or("missing".into())
        }
        fn save(&self, id: &str, value: &str) -> Result<(), String> {
            self.values.lock().unwrap().insert(id.into(), value.into());
            if self.fail && id.ends_with("cert-1") {
                Err("failed after write".into())
            } else {
                Ok(())
            }
        }
        fn remove(&self, id: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(id);
            Ok(())
        }
    }
    #[test]
    fn partial_vault_write_cleans_every_written_slot() {
        let vault = FakeVault {
            values: Mutex::new(BTreeMap::new()),
            fail: true,
        };
        assert!(save_bundle(&vault, "device", "private key", &"a".repeat(4000)).is_err());
        assert!(vault.values.lock().unwrap().is_empty());
    }
    #[test]
    fn invitation_requires_matching_pin_and_unexpired_token() {
        let id = uuid::Uuid::new_v4().to_string();
        let identity = crate::remote::tls::HostIdentity::generate(&id).unwrap();
        let mut invite = p::PairingInvite {
            protocol_version: 1,
            host_id: id,
            server_name: identity.server_name().into(),
            address: "127.0.0.1".into(),
            port: 443,
            host_certificate: identity.certificate().into(),
            certificate_sha256: identity.fingerprint().to_vec(),
            one_time_token: vec![1; 32],
            expires_unix: chrono::Utc::now().timestamp() as u64 + 60,
        };
        let uri = |i: &p::PairingInvite| {
            format!(
                "canopy://pair?v=1&data={}",
                URL_SAFE_NO_PAD.encode(i.encode_to_vec())
            )
        };
        assert!(PairingInvite::parse(&uri(&invite)).is_ok());
        invite.certificate_sha256[0] ^= 1;
        assert!(PairingInvite::parse(&uri(&invite)).is_err());
        invite.certificate_sha256 = identity.fingerprint().to_vec();
        invite.expires_unix = 1;
        assert!(PairingInvite::parse(&uri(&invite)).is_err());
    }
}
