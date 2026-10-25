use super::*;
use crate::remote::{
    client::{HostId, pairing, store::CredentialStore},
    host::HostRpc,
    sync::ReplayHub,
};
use std::{collections::BTreeMap, sync::Mutex};
use tonic::transport::{Identity, Server};

#[derive(Default)]
struct Vault(Mutex<BTreeMap<String, String>>);
impl CredentialStore for Vault {
    fn load(&self, id: &str) -> Result<String, String> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("missing".into())
    }
    fn save(&self, id: &str, value: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(id.into(), value.into());
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}
struct Node {
    _dir: tempfile::TempDir,
    client: SettingsClient,
    identity: Arc<HostIdentity>,
    local: Arc<LocalPeer>,
    registry: ClientRegistry,
    path: std::path::PathBuf,
    address: std::net::SocketAddr,
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}
impl Node {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let client = SettingsClient::create(&dir.path().join("host.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let identity = Arc::new(HostIdentity::generate(host.id()).unwrap());
        let path = dir.path().join("clients.json");
        let registry = ClientRegistry::open(path.clone(), Arc::new(Vault::default())).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let local = Arc::new(LocalPeer::new(identity.clone(), client.clone(), address));
        registry.set_local_peer(Some(local.clone()));
        let auth = RpcAuth::new(host.id().into(), client.clone()).unwrap();
        let state = HostState::new(host.id(), vec![]);
        state.set(p::HostReadiness::Ready, String::new());
        let locks = ResourceLocks::default();
        let rpc = PeerRpc::new(
            auth.clone(),
            local.clone(),
            registry.clone(),
            state.clone(),
            locks.clone(),
        );
        let info = HostRpc::new(auth, state, ReplayHub::new(Default::default()), locks);
        let router = Server::builder()
            .tls_config(identity.server_config())
            .unwrap()
            .add_service(p::peer_service_server::PeerServiceServer::new(rpc))
            .add_service(p::host_service_server::HostServiceServer::new(info));
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            router
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    async {
                        let _ = stopped.await;
                    },
                )
                .await
                .unwrap();
        });
        Self {
            _dir: dir,
            client,
            identity,
            local,
            registry,
            path,
            address,
            stop,
            task,
        }
    }
    async fn approve(&self, requester: &Node, scopes: u32) -> HostRecord {
        let key = rcgen::KeyPair::generate().unwrap();
        let csr = rcgen::CertificateParams::new(Vec::<String>::new())
            .unwrap()
            .serialize_request(&key)
            .unwrap();
        let device = uuid::Uuid::new_v4().to_string();
        let cert = self.identity.issue_device(&device, csr.der()).unwrap();
        self.client
            .register_runtime_device(
                DeviceRegistration::new(
                    device.clone(),
                    "Requesting computer".into(),
                    cert.der().to_vec(),
                    scopes,
                    vec!["*".into()],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let reference = uuid::Uuid::new_v4().to_string();
        let pem = pairing::pem_certificate(cert.der())
            + &cert
                .chain()
                .iter()
                .map(|c| pairing::pem_certificate(c))
                .collect::<String>();
        pairing::save_bundle(
            requester.registry.vault().as_ref(),
            &reference,
            &key.serialize_pem(),
            &pem,
        )
        .unwrap();
        HostRecord {
            mutual: false,
            passive_peer: false,
            auto_connect: true,
            id: HostId::parse(self.local.host_id()).unwrap(),
            name: "Other computer".into(),
            endpoint: format!("https://{}/", self.address),
            server_name: self.identity.server_name().into(),
            certificate: self.identity.certificate().into(),
            fingerprint: self.identity.fingerprint().into(),
            device_id: device,
            credential_ref: reference,
            ssh: None,
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
        }
    }
    async fn channel(&self, identity: Option<Identity>) -> Channel {
        crate::remote::tls::client_endpoint(
            &format!("https://{}/", self.address),
            self.identity.server_name(),
            self.identity.certificate(),
            &self.identity.fingerprint(),
            identity,
        )
        .unwrap()
        .connect()
        .await
        .unwrap()
    }
    async fn stop(self) {
        for host in self.registry.hosts() {
            host.pause_connection().unwrap();
        }
        let _ = self.stop.send(());
        self.task.await.unwrap();
        self.client.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn one_approval_creates_both_identities_and_retries_preserve_grants_and_credentials() {
    let a = Node::start().await;
    let b = Node::start().await;
    let record = b.approve(&a, host::ALL_SCOPES).await;
    let identity = pairing::identity(a.registry.vault().as_ref(), &record.credential_ref).unwrap();
    let channel = b.channel(Some(identity)).await;
    assert!(
        a.local
            .connect_back(channel.clone(), &record, None)
            .await
            .unwrap()
    );
    let id = HostId::parse(a.local.host_id()).unwrap();
    let reverse = b.registry.get(&id).unwrap();
    let saved = reverse.snapshot().record;
    assert!(saved.mutual);
    assert!(saved.auto_connect());
    // Reopening an SSH session changes its remote forwarding port. Updating
    // that route must keep the device, vault reference and route identity.
    let cert = a
        .client
        .runtime_device_certificate(saved.device_id.clone())
        .await
        .unwrap()
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let first_port = listener.local_addr().unwrap().port();
    b.registry
        .install_peer(
            &b.local,
            a.local.description(Some(first_port)).unwrap(),
            cert.clone(),
            vec![a.identity.issuer_certificate().to_vec()],
            Some("127.0.0.1".parse().unwrap()),
        )
        .unwrap();
    let first = reverse
        .routes()
        .into_iter()
        .find(|route| route.name() == "SSH return")
        .unwrap();
    let second_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let second_port = second_listener.local_addr().unwrap().port();
    b.registry
        .install_peer(
            &b.local,
            a.local.description(Some(second_port)).unwrap(),
            cert,
            vec![a.identity.issuer_certificate().to_vec()],
            Some("127.0.0.1".parse().unwrap()),
        )
        .unwrap();
    let second = reverse
        .routes()
        .into_iter()
        .find(|route| route.name() == "SSH return")
        .unwrap();
    assert_eq!(first.id(), second.id());
    assert!(second.endpoint().contains(&format!(":{second_port}/")));
    assert_eq!(
        reverse.snapshot().record.credential_ref,
        saved.credential_ref
    );
    for root in ["private", "/project-opened-after-pairing"] {
        a.client
            .authorize_runtime_read(saved.device_id.clone(), host::ALL_SCOPES, root.into())
            .await
            .unwrap();
    }
    let reverse_tls =
        pairing::identity(b.registry.vault().as_ref(), &saved.credential_ref).unwrap();
    let reverse_channel = a.channel(Some(reverse_tls)).await;
    let mut host = p::host_service_client::HostServiceClient::new(reverse_channel);
    assert_eq!(
        host.get_info(p::HostRequest {
            host_id: a.local.host_id().into()
        })
        .await
        .unwrap()
        .into_inner()
        .host_id,
        a.local.host_id()
    );
    // Lost completion responses/repeated Connect cannot make a second device,
    // change a chosen scope, replace the key, or discard saved drafts.
    a.client
        .update_runtime_device_authorization(
            saved.device_id.clone(),
            1,
            host::READ,
            vec!["private".into()],
        )
        .await
        .unwrap();
    assert!(
        a.local
            .connect_back(channel.clone(), &record, None)
            .await
            .unwrap()
    );
    assert_eq!(a.client.runtime_devices().await.unwrap().len(), 1);
    assert_eq!(b.registry.hosts().len(), 1);
    assert_eq!(
        reverse.snapshot().record.credential_ref,
        saved.credential_ref
    );
    assert!(
        a.client
            .authorize_runtime_read(saved.device_id.clone(), host::CHAT, "private".into())
            .await
            .is_err()
    );
    reverse.pause_connection().unwrap();
    assert!(
        a.local
            .connect_back(channel.clone(), &record, None)
            .await
            .unwrap()
    );
    assert!(
        !reverse.auto_connect(),
        "a remote reconnect must not undo a local pause"
    );
    let reopened = ClientRegistry::open(b.path.clone(), b.registry.vault()).unwrap();
    assert_eq!(
        reopened.get(&id).unwrap().snapshot().record.credential_ref,
        saved.credential_ref
    );
    a.client
        .revoke_runtime_device(saved.device_id.clone())
        .await
        .unwrap();
    assert!(a.local.connect_back(channel, &record, None).await.is_err());
    assert!(
        host.get_info(p::HostRequest {
            host_id: a.local.host_id().into()
        })
        .await
        .is_err()
    );
    a.stop().await;
    b.stop().await;
}

#[tokio::test]
async fn reciprocal_setup_requires_authorization_and_the_paired_host_key() {
    let a = Node::start().await;
    let b = Node::start().await;
    let public = b.channel(None).await;
    let mut service = p::peer_service_client::PeerServiceClient::new(public);
    assert_eq!(
        service
            .get_peer_request(p::HostRequest {
                host_id: b.local.host_id().into()
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    let record = b.approve(&a, host::READ).await;
    let identity = pairing::identity(a.registry.vault().as_ref(), &record.credential_ref).unwrap();
    let channel = b.channel(Some(identity)).await;
    assert!(
        !a.local
            .connect_back(channel.clone(), &record, None)
            .await
            .unwrap()
    );
    assert!(a.client.runtime_devices().await.unwrap().is_empty());
    b.client
        .update_runtime_device_authorization(
            record.device_id.clone(),
            1,
            host::ALL_SCOPES,
            vec!["*".into()],
        )
        .await
        .unwrap();
    let wrong = a.local.request().unwrap();
    assert!(a.local.certificate(&record, &wrong).await.is_err());
    let wrong_certificate = a
        .identity
        .issue_device(&uuid::Uuid::new_v4().to_string(), &wrong)
        .unwrap();
    let mut service = p::peer_service_client::PeerServiceClient::new(channel);
    assert!(
        service
            .connect_peer(p::ConnectPeerRequest {
                host_id: b.local.host_id().into(),
                peer: Some(a.local.description(None).unwrap()),
                certificate: wrong_certificate.der().into(),
                certificate_chain: wrong_certificate.chain().into()
            })
            .await
            .is_err()
    );
    assert!(b.registry.hosts().is_empty());
    assert!(a.client.runtime_devices().await.unwrap().is_empty());
    a.stop().await;
    b.stop().await;
}
