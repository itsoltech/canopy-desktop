//! Native TLS identities. Private keys stay in the platform credential store;
//! SQLite holds only a version reference. A missing referenced key is an error,
//! never a reason to silently replace the host identity trusted by other devices.
use crate::{host::RuntimeHost, platform::credentials, settings::SettingsClient};
use chrono::Datelike;
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject},
};
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};
use tonic::transport::{Certificate, ClientTlsConfig, Identity, ServerTlsConfig};

const VAULT: &str = "tech.itsol.canopy.remote";
const PARTS: [&str; 4] = ["server-key", "server-cert", "issuer-key", "issuer-cert"];

/// Tonic's builders use Rustls' process provider. HTTP dependencies also enable
/// aws-lc, so feature inference is ambiguous even when tonic enables only ring.
/// Respect a provider already selected by the application; otherwise select ring.
fn ensure_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

/// Deliberately has no Debug/Serialize implementation.
pub struct HostIdentity {
    host_id: String,
    server_name: String,
    bundle: Bundle,
    certificate: Vec<u8>,
    issuer_certificate: Vec<u8>,
}
struct Bundle {
    parts: [String; 4],
}
impl Bundle {
    fn save(
        &self,
        reference: &str,
        store: impl Fn(&str, &str) -> Result<(), String>,
        remove: impl Fn(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        for (index, part) in self.parts.iter().enumerate() {
            let id = format!("{reference}-{}", PARTS[index]);
            if store(&id, part).is_err() {
                let mut cleaned = true;
                for name in PARTS.iter().take(index + 1) {
                    cleaned &= remove(&format!("{reference}-{name}")).is_ok();
                }
                return Err(if cleaned {
                    "Could not store the remote identity."
                } else {
                    "Could not store the remote identity; credential cleanup also failed."
                }
                .into());
            }
        }
        Ok(())
    }
}
impl HostIdentity {
    pub fn generate(host_id: &str) -> Result<Self, String> {
        if uuid::Uuid::parse_str(host_id).is_err() {
            return Err("Invalid host identity.".into());
        }
        let name = format!("canopy-{host_id}.local");
        let server_key = KeyPair::generate().map_err(crypto_error)?;
        let mut server = parameters(&name)?;
        server.subject_alt_names = vec![SanType::DnsName(
            name.clone().try_into().map_err(crypto_error)?,
        )];
        server.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server_cert = server.self_signed(&server_key).map_err(crypto_error)?;
        let issuer_key = KeyPair::generate().map_err(crypto_error)?;
        let mut issuer = parameters(&format!("Canopy {host_id} device issuer"))?;
        issuer.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        issuer.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let issuer_cert = issuer.self_signed(&issuer_key).map_err(crypto_error)?;
        Self::restore(
            host_id,
            Bundle {
                parts: [
                    server_key.serialize_pem(),
                    server_cert.pem(),
                    issuer_key.serialize_pem(),
                    issuer_cert.pem(),
                ],
            },
        )
    }
    fn restore(host_id: &str, bundle: Bundle) -> Result<Self, String> {
        let certificate = CertificateDer::from_pem_slice(bundle.parts[1].as_bytes())
            .map_err(crypto_error)?
            .to_vec();
        let issuer_certificate = CertificateDer::from_pem_slice(bundle.parts[3].as_bytes())
            .map_err(crypto_error)?
            .to_vec();
        let provider = rustls::crypto::ring::default_provider();
        // Reject mismatched or corrupted slots before opening a listener.
        for (key, cert) in [(0, 1), (2, 3)] {
            let private = PrivateKeyDer::from_pem_slice(bundle.parts[key].as_bytes())
                .map_err(crypto_error)?;
            let cert = CertificateDer::from_pem_slice(bundle.parts[cert].as_bytes())
                .map_err(crypto_error)?;
            rustls::sign::CertifiedKey::from_der(vec![cert], private, &provider)
                .map_err(crypto_error)?;
        }
        Ok(Self {
            host_id: host_id.into(),
            server_name: format!("canopy-{host_id}.local"),
            bundle,
            certificate,
            issuer_certificate,
        })
    }
    pub fn host_id(&self) -> &str {
        &self.host_id
    }
    pub fn server_name(&self) -> &str {
        &self.server_name
    }
    pub fn certificate(&self) -> &[u8] {
        &self.certificate
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        Sha256::digest(&self.certificate).into()
    }
    /// The stable host key also identifies this computer as a client. Only the
    /// CSR leaves the computer; peer-specific ClientAuth certificates are issued
    /// by the other host, with their own revocable device IDs.
    pub(crate) fn peer_request(&self) -> Result<Vec<u8>, String> {
        let key = KeyPair::from_pem(&self.bundle.parts[0]).map_err(crypto_error)?;
        Ok(CertificateParams::new(Vec::<String>::new())
            .map_err(crypto_error)?
            .serialize_request(&key)
            .map_err(crypto_error)?
            .der()
            .to_vec())
    }
    pub(crate) fn peer_key(&self) -> &str {
        &self.bundle.parts[0]
    }
    pub(crate) fn issuer_certificate(&self) -> &[u8] {
        &self.issuer_certificate
    }
    pub(crate) fn csr_matches_certificate(csr: &[u8], cert: &[u8]) -> Result<(), String> {
        use rcgen::PublicKeyData;
        Self::verify_csr(csr)?;
        let request =
            CertificateSigningRequestParams::from_der(&csr.into()).map_err(crypto_error)?;
        let cert = CertificateDer::from(cert);
        let parsed = rustls::server::ParsedCertificate::try_from(&cert).map_err(crypto_error)?;
        if request.public_key.subject_public_key_info().as_slice()
            != parsed.subject_public_key_info().as_ref()
        {
            return Err(
                "The computer certificate request does not match its paired identity.".into(),
            );
        }
        Ok(())
    }
    /// Domain-separated stable key for immutable settings staging. Restoring the
    /// referenced issuer key is mandatory; a missing vault slot never generates
    /// a new fingerprint key or changes an already accepted secret's meaning.
    pub fn settings_mac_key(&self) -> Result<[u8; 32], String> {
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<Sha256>::new_from_slice(self.bundle.parts[2].as_bytes())
            .map_err(crypto_error)?;
        mac.update(b"Canopy remote settings staging fingerprint v1");
        Ok(mac.finalize().into_bytes().into())
    }
    pub fn server_config(&self) -> ServerTlsConfig {
        ensure_crypto_provider();
        ServerTlsConfig::new()
            .identity(Identity::from_pem(
                &self.bundle.parts[1],
                &self.bundle.parts[0],
            ))
            .client_ca_root(Certificate::from_pem(&self.bundle.parts[3]))
            .client_auth_optional(true)
            .timeout(std::time::Duration::from_secs(10))
    }
    /// Verify a standard PKCS#10 request, including its signature, before asking
    /// for local approval. Call again at issuance; requested roles/SANs are ignored.
    pub fn verify_csr(csr: &[u8]) -> Result<[u8; 32], String> {
        if csr.is_empty() || csr.len() > 16 * 1024 {
            return Err("Invalid device certificate request.".into());
        }
        CertificateSigningRequestParams::from_der(&csr.into()).map_err(crypto_error)?;
        Ok(Sha256::digest(csr).into())
    }
    pub fn issue_device(&self, device_id: &str, csr: &[u8]) -> Result<DeviceCertificate, String> {
        Self::verify_csr(csr)?;
        if uuid::Uuid::parse_str(device_id).is_err() {
            return Err("Invalid device identity.".into());
        }
        let mut request =
            CertificateSigningRequestParams::from_der(&csr.into()).map_err(crypto_error)?;
        let mut params = parameters(device_id)?;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        params.subject_alt_names = vec![SanType::URI(
            format!("urn:canopy:device:{device_id}")
                .try_into()
                .map_err(crypto_error)?,
        )];
        request.params = params;
        let issuer = Issuer::from_ca_cert_pem(
            &self.bundle.parts[3],
            KeyPair::from_pem(&self.bundle.parts[2]).map_err(crypto_error)?,
        )
        .map_err(crypto_error)?;
        let certificate = request.signed_by(&issuer).map_err(crypto_error)?;
        Ok(DeviceCertificate {
            der: certificate.der().to_vec(),
            chain: vec![self.issuer_certificate.clone()],
        })
    }
}
pub struct DeviceCertificate {
    der: Vec<u8>,
    chain: Vec<Vec<u8>>,
}
impl DeviceCertificate {
    pub fn der(&self) -> &[u8] {
        &self.der
    }
    pub fn chain(&self) -> &[Vec<u8>] {
        &self.chain
    }
}
fn parameters(name: &str) -> Result<CertificateParams, String> {
    let mut parameters = CertificateParams::new(Vec::<String>::new()).map_err(crypto_error)?;
    parameters.distinguished_name.push(DnType::CommonName, name);
    let now = chrono::Utc::now().date_naive();
    let start = now - chrono::Duration::days(1);
    let end = now + chrono::Duration::days(365 * 5);
    parameters.not_before =
        rcgen::date_time_ymd(start.year(), start.month() as u8, start.day() as u8);
    parameters.not_after = rcgen::date_time_ymd(end.year(), end.month() as u8, end.day() as u8);
    Ok(parameters)
}
fn crypto_error(_: impl fmt::Display) -> String {
    "Remote TLS identity or certificate could not be validated.".into()
}

/// Call off the UI thread. Tests use generated identities, never the user's vault.
pub async fn load_or_create(
    client: &SettingsClient,
    host: RuntimeHost,
) -> Result<HostIdentity, String> {
    if let Some(reference) = host.identity_ref() {
        let mut parts = Vec::with_capacity(4);
        for name in PARTS {
            parts.push(credentials::load(VAULT, &format!("{reference}-{name}"))?);
        }
        return HostIdentity::restore(
            host.id(),
            Bundle {
                parts: parts
                    .try_into()
                    .map_err(|_| "Incomplete remote identity.")?,
            },
        );
    }
    let identity = HostIdentity::generate(host.id())?;
    let reference = uuid::Uuid::new_v4().to_string();
    identity.bundle.save(
        &reference,
        |id, value| credentials::store(VAULT, id, value),
        |id| credentials::remove(VAULT, id),
    )?;
    if let Err(error) = client
        .set_runtime_identity_ref(None, reference.clone())
        .await
    {
        // A worker acknowledgement can be lost after commit. Do not delete keys
        // that a committed manifest may already reference.
        let current = client.initialize_runtime().await.map_err(|_| {
            "Remote identity commit result is unknown; credentials were retained.".to_owned()
        })?;
        if current.identity_ref() != Some(reference.as_str()) {
            let mut cleaned = true;
            for part in PARTS {
                cleaned &= credentials::remove(VAULT, &format!("{reference}-{part}")).is_ok();
            }
            return Err(if cleaned {
                error.to_string()
            } else {
                "Remote identity was not installed; credential cleanup failed.".into()
            });
        }
    }
    Ok(identity)
}

#[derive(Debug)]
struct PinVerifier {
    leaf: Vec<u8>,
    verifier: Arc<rustls::client::WebPkiServerVerifier>,
}
impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        leaf: &CertificateDer<'_>,
        chain: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if leaf.as_ref() != self.leaf {
            return Err(rustls::Error::General(
                "The paired host certificate changed.".into(),
            ));
        }
        self.verifier
            .verify_server_cert(leaf, chain, name, ocsp, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.verifier
            .verify_tls12_signature(message, cert, signature)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.verifier
            .verify_tls13_signature(message, cert, signature)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.verifier.supported_verify_schemes()
    }
}
/// Pin the actual leaf, not merely a CA that could sign another server identity.
/// PEM key material is supplied only from the client's secure credential owner.
pub fn client_endpoint(
    address: &str,
    server_name: &str,
    leaf_der: &[u8],
    fingerprint: &[u8],
    identity: Option<Identity>,
) -> Result<tonic::transport::Endpoint, String> {
    ensure_crypto_provider();
    let endpoint = url::Url::parse(address).map_err(crypto_error)?;
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(
            "A native remote endpoint must be an HTTPS address without embedded credentials."
                .into(),
        );
    }
    if fingerprint != Sha256::digest(leaf_der).as_slice() {
        return Err("The pairing certificate fingerprint does not match.".into());
    }
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(leaf_der.to_vec()))
        .map_err(crypto_error)?;
    let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::new(rustls::crypto::ring::default_provider()),
    )
    .build()
    .map_err(crypto_error)?;
    let mut config = ClientTlsConfig::new().domain_name(server_name);
    if let Some(identity) = identity {
        config = config.identity(identity);
    }
    tonic::transport::Endpoint::from_shared(address.to_owned())
        .map_err(crypto_error)?
        .connect_timeout(std::time::Duration::from_secs(10))
        .tcp_nodelay(true)
        .concurrency_limit(32)
        .buffer_size(32)
        .http2_keep_alive_interval(std::time::Duration::from_secs(30))
        .keep_alive_timeout(std::time::Duration::from_secs(10))
        .tls_config_with_verifier(
            config,
            Arc::new(PinVerifier {
                leaf: leaf_der.to_vec(),
                verifier,
            }),
        )
        .map_err(crypto_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn issuance_checks_signature_and_replaces_requested_authority() {
        let host = HostIdentity::generate(&uuid::Uuid::new_v4().to_string()).unwrap();
        let key = KeyPair::generate().unwrap();
        let mut requested = parameters("unapproved administrator").unwrap();
        requested.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        requested.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let csr = requested.serialize_request(&key).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let issued = host.issue_device(&id, csr.der()).unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(host.issuer_certificate.clone().into()).unwrap();
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .unwrap();
        // Requested CA/server authority cannot survive as a usable client leaf.
        verifier
            .verify_client_cert(&issued.der().into(), &[], UnixTime::now())
            .unwrap();
        let mut changed = csr.der().to_vec();
        let end = changed.len() - 1;
        changed[end] ^= 1;
        assert!(HostIdentity::verify_csr(&changed).is_err());
    }
    #[test]
    fn pinning_requires_the_paired_leaf_and_the_expected_server_name() {
        let host = HostIdentity::generate(&uuid::Uuid::new_v4().to_string()).unwrap();
        let other = HostIdentity::generate(&uuid::Uuid::new_v4().to_string()).unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(host.certificate().to_vec().into()).unwrap();
        roots.add(other.certificate().to_vec().into()).unwrap();
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .unwrap();
        let verifier = PinVerifier {
            leaf: host.certificate().into(),
            verifier,
        };
        let name = ServerName::try_from(host.server_name().to_owned()).unwrap();
        assert!(
            verifier
                .verify_server_cert(&host.certificate().into(), &[], &name, &[], UnixTime::now())
                .is_ok()
        );
        assert!(
            verifier
                .verify_server_cert(
                    &other.certificate().into(),
                    &[],
                    &name,
                    &[],
                    UnixTime::now()
                )
                .is_err()
        );
        let wrong_name = ServerName::try_from(other.server_name().to_owned()).unwrap();
        assert!(
            verifier
                .verify_server_cert(
                    &host.certificate().into(),
                    &[],
                    &wrong_name,
                    &[],
                    UnixTime::now()
                )
                .is_err()
        );
        assert!(
            client_endpoint(
                "http://127.0.0.1:1",
                host.server_name(),
                host.certificate(),
                &host.fingerprint(),
                None
            )
            .is_err()
        );
    }
    #[test]
    fn partially_written_identity_is_removed_without_leaving_a_dangling_manifest() {
        let bundle = Bundle {
            parts: ["a".into(), "b".into(), "c".into(), "d".into()],
        };
        let values = std::cell::RefCell::new(std::collections::HashMap::new());
        let result = bundle.save(
            "ref",
            |id, value| {
                values.borrow_mut().insert(id.to_owned(), value.to_owned());
                if id.ends_with("issuer-key") {
                    return Err("write completed but confirmation failed".into());
                }
                Ok(())
            },
            |id| {
                values.borrow_mut().remove(id);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(values.borrow().is_empty());
    }
}
