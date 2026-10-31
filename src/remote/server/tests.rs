//! Contract test over a real loopback HTTP/2 + TLS connection, with temporary
//! data and generated credentials. No app window, platform vault or provider.
use super::*;
use crate::remote::settings_rpc::{
    self, AgentEdit, IntegrationEdit, ProfileEdit, SecretVersion, SettingsHost, SettingsSecrets,
};
use tonic::{Status, transport::Identity};

struct Host;
#[tonic::async_trait]
impl crate::remote::tasks::TasksHost for Host {
    async fn save_filter(&self, _: crate::integrations::filters::TaskFilter) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn remove_filter(&self, _: String) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn task_to_worktree(
        &self,
        _: proto::WorkspaceRef,
        _: crate::integrations::TaskRef,
        _: String,
    ) -> Result<String, Status> {
        Err(Status::unimplemented("test"))
    }
}
struct NoHttp;
impl gpui_kit::http_client::HttpClient for NoHttp {
    fn user_agent(&self) -> Option<&gpui_kit::http_client::http::HeaderValue> {
        None
    }
    fn proxy(&self) -> Option<&gpui_kit::http_client::Url> {
        None
    }
    fn send(
        &self,
        _: gpui_kit::http_client::Request<gpui_kit::http_client::AsyncBody>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = gpui_kit::http_client::Result<
                        gpui_kit::http_client::Response<gpui_kit::http_client::AsyncBody>,
                    >,
                > + Send,
        >,
    > {
        Box::pin(async { panic!("discovery must never access a provider or vault") })
    }
}
#[tonic::async_trait]
impl SettingsHost for Host {
    async fn snapshot(&self) -> Result<proto::HostSettings, Status> {
        Ok(Default::default())
    }
    async fn update_preferences(
        &self,
        _: crate::command_bus::CommandEnvelope,
        _: crate::command_bus::Principal,
    ) -> Result<crate::host::CommandReceipt, Status> {
        Err(Status::unimplemented("test"))
    }
    async fn save_agent(&self, _: AgentEdit, _: Option<u64>) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn remove_agent(&self, _: String, _: Option<u64>) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn save_profile(&self, _: ProfileEdit, _: Option<u64>) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn remove_profile(&self, _: String, _: Option<u64>) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn save_integration(&self, _: IntegrationEdit, _: Option<u64>) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn remove_integration(&self, _: String, _: Option<u64>) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
}
struct Secrets;
#[tonic::async_trait]
impl SettingsSecrets for Secrets {
    async fn fingerprint(&self, _: &[u8]) -> Result<[u8; 32], Status> {
        Err(Status::unimplemented("test"))
    }
    async fn stage(&self, _: &SecretVersion, _: &str) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn read(&self, _: &SecretVersion) -> Result<String, Status> {
        Err(Status::unimplemented("test"))
    }
    async fn remove(&self, _: &SecretVersion) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
}
fn pem(der: &[u8]) -> String {
    use base64::Engine;
    let base64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = "-----BEGIN CERTIFICATE-----\n".to_owned();
    for line in base64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(line).unwrap());
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    pem
}
fn device(identity: &HostIdentity) -> (String, Vec<u8>, Identity) {
    let id = uuid::Uuid::new_v4().to_string();
    let key = rcgen::KeyPair::generate().unwrap();
    let csr = rcgen::CertificateParams::new(Vec::<String>::new())
        .unwrap()
        .serialize_request(&key)
        .unwrap();
    let certificate = identity.issue_device(&id, csr.der()).unwrap();
    let chain = std::iter::once(certificate.der())
        .chain(certificate.chain().iter().map(Vec::as_slice))
        .map(pem)
        .collect::<String>();
    (
        id,
        certificate.der().into(),
        Identity::from_pem(chain, key.serialize_pem()),
    )
}
#[tokio::test]
async fn typed_tls_health_requires_current_device_and_shutdown_ends_watches() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::settings::SettingsClient::create(&directory.path().join("test.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let credential_ref = uuid::Uuid::new_v4().to_string();
    let integration_id = uuid::Uuid::new_v4().to_string();
    client
        .save_integrations(crate::integrations::Config {
            accounts: vec![crate::integrations::Account {
                id: integration_id.clone(),
                provider: crate::integrations::Provider::Github,
                login: "test-login".into(),
                credential: credential_ref.clone(),
                scope: crate::integrations::AccountScope::Default,
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    let identity = Arc::new(HostIdentity::generate(host.id()).unwrap());
    let auth = RpcAuth::new(host.id().into(), client.clone()).unwrap();
    let state = HostState::new(host.id(), vec![]);
    let (enrollment, _) = EnrollmentRpc::new(auth.clone(), identity.clone());
    let operations = ResourceLocks::default();
    let settings = settings_rpc::SettingsRpc::new(
        auth.clone(),
        operations.clone(),
        Arc::new(Host),
        Arc::new(Secrets),
    );
    let artifacts = Arc::new(
        crate::artifacts::ArtifactStore::open(client.clone(), directory.path().join("artifacts"))
            .unwrap(),
    );
    let tasks = crate::remote::tasks::TasksRpc::new(
        auth.clone(),
        Default::default(),
        operations.clone(),
        Arc::new(NoHttp),
        artifacts,
        Arc::new(Host),
    );
    let features = Routes::new(proto::settings_service_server::SettingsServiceServer::new(
        settings.clone(),
    ))
    .add_service(proto::tasks_service_server::TasksServiceServer::new(tasks));
    let server = start(
        "127.0.0.1:0".parse().unwrap(),
        ServerServices {
            auth,
            state: state.clone(),
            identity: identity.clone(),
            enrollment,
            replay: ReplayHub::new(Default::default()),
            operations,
            features,
            settings,
        },
    )
    .await
    .unwrap();
    let endpoint = format!("https://{}", server.address());
    let endpoint_for = |credential| {
        super::super::tls::client_endpoint(
            &endpoint,
            identity.server_name(),
            identity.certificate(),
            &identity.fingerprint(),
            credential,
        )
        .unwrap()
    };
    let unpaired = endpoint_for(None).connect().await.unwrap();
    let mut host_rpc = proto::host_service_client::HostServiceClient::new(unpaired);
    assert_eq!(
        host_rpc
            .get_info(proto::HostRequest {
                host_id: host.id().into()
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unauthenticated
    );
    let (id, der, credential) = device(&identity);
    // A valid issuer signature alone is not authorization.
    let channel = endpoint_for(Some(credential)).connect().await.unwrap();
    let mut host_rpc = proto::host_service_client::HostServiceClient::new(channel.clone());
    assert_eq!(
        host_rpc
            .get_info(proto::HostRequest {
                host_id: host.id().into()
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    client
        .register_runtime_device(
            crate::host::DeviceRegistration::new(
                id.clone(),
                "test device".into(),
                der,
                crate::host::READ | crate::host::TASKS,
                vec!["private".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        host_rpc
            .get_info(proto::HostRequest {
                host_id: host.id().into()
            })
            .await
            .unwrap()
            .into_inner()
            .host_id,
        host.id()
    );
    let integrations = proto::tasks_service_client::TasksServiceClient::new(channel.clone())
        .list_integrations(proto::TaskIntegrationsRequest {
            host_id: host.id().into(),
            workspace: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(integrations.integrations.len(), 1);
    assert_eq!(integrations.integrations[0].id, integration_id);
    assert_eq!(integrations.integrations[0].provider, "github");
    assert!(integrations.integrations[0].configured);
    assert!(!format!("{integrations:?}").contains(&credential_ref));
    // READ includes Tasks browsing. A read-only device does not need TASKS or
    // ADMIN just to discover the host's safe integration metadata.
    client
        .update_runtime_device_authorization(
            id.clone(),
            1,
            crate::host::READ,
            vec!["private".into()],
        )
        .await
        .unwrap();
    let readonly = proto::tasks_service_client::TasksServiceClient::new(channel.clone())
        .list_integrations(proto::TaskIntegrationsRequest {
            host_id: host.id().into(),
            workspace: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(readonly.integrations, integrations.integrations);
    let admin_error = proto::settings_service_client::SettingsServiceClient::new(channel.clone())
        .get_settings(proto::HostRequest {
            host_id: host.id().into(),
        })
        .await
        .unwrap_err();
    assert_eq!(admin_error.code(), tonic::Code::PermissionDenied);
    assert_eq!(
        host_rpc
            .read_diagnostics(proto::Empty {})
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    client
        .update_runtime_device_authorization(
            id.clone(),
            2,
            crate::host::READ | crate::host::ADMIN,
            vec!["private".into()],
        )
        .await
        .unwrap();
    crate::remote::metrics::record(
        host.id(),
        host.id(),
        None,
        0,
        crate::remote::metrics::Stage::Fanout,
        std::time::Instant::now(),
        crate::remote::metrics::Outcome::Ok,
    );
    let diagnostics = host_rpc
        .read_diagnostics(proto::Empty {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(diagnostics.schema_version, 1);
    assert!(
        diagnostics
            .records
            .iter()
            .all(|record| record.host_id == host.id())
    );
    assert!(!format!("{diagnostics:?}").contains(&credential_ref));
    let mut host_watch = host_rpc
        .watch_host(proto::HostRequest {
            host_id: host.id().into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(host_watch.message().await.unwrap().is_some());
    assert!(state.device_online(&id));
    let mut health = tonic_health::pb::health_client::HealthClient::new(channel);
    let mut watch = health
        .watch(tonic_health::pb::HealthCheckRequest {
            service: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        watch.message().await.unwrap().unwrap().status,
        tonic_health::pb::health_check_response::ServingStatus::Serving as i32
    );
    client.revoke_runtime_device(id.clone()).await.unwrap();
    state.authorization_changed();
    let refused = tokio::time::timeout(Duration::from_secs(2), watch.message())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(refused.code(), tonic::Code::PermissionDenied);
    loop {
        let result = tokio::time::timeout(Duration::from_secs(2), host_watch.message())
            .await
            .unwrap();
        if result.is_err() || result.unwrap().is_none() {
            break;
        }
    }
    assert!(!state.device_online(&id));
    tokio::time::timeout(Duration::from_secs(7), server.stop())
        .await
        .unwrap()
        .unwrap();
    client.shutdown().await.unwrap();
}
