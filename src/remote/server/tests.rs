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
    async fn configure(
        &self,
        _: String,
        _: crate::command_bus::tasks::TaskConfiguration,
    ) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn save_filter(&self, _: crate::integrations::filters::TaskFilter) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn remove_filter(&self, _: String) -> Result<(), Status> {
        Err(Status::unimplemented("test"))
    }
    async fn task_to_worktree(
        &self,
        _: crate::thread::ProjectRef,
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
    async fn snapshot(&self) -> Result<crate::api::proto::HostSettings, Status> {
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
#[tonic::async_trait]
impl crate::api::ThreadPublisher for Host {
    async fn committed(&self, _: &crate::host::CommandReceipt) {}
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
    let chat_id = uuid::Uuid::new_v4().to_string();
    let chat = client
        .save_runtime_chat(
            crate::host::ChatRecord::new(
                chat_id.clone(),
                None,
                "test-account".into(),
                crate::state::workspace::PaneMetadata {
                    kind: crate::state::workspace::PaneKind::Chat,
                    chat: Some(Box::new(crate::chat::config::ChatSettings {
                        id: Some(chat_id.clone()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let api = crate::api::ThreadRpc::new(
        auth.clone(),
        state.clone(),
        operations.clone(),
        crate::command_bus::DraftResources {
            roots: Default::default(),
            pulls: Default::default(),
        },
        Arc::new(Host),
    );
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
    let root = directory.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let mut projects = crate::state::projects::Projects::default();
    projects.open(root.clone());
    let project = projects.current().unwrap();
    let file_target = crate::api::proto::ResourceTarget {
        resource: Some(crate::api::proto::resource_target::Resource::Project(
            crate::api::proto::ProjectRef {
                host_id: host.id().into(),
                project_id: serde_json::to_string(&project.id).unwrap(),
                worktree_id: None,
            },
        )),
    };
    let roots = super::super::auth::WorkspaceRoots::default();
    let git_root = directory.path().join("git-project");
    std::fs::create_dir(&git_root).unwrap();
    let git_root = git_root.canonicalize().unwrap();
    git2::Repository::init(&git_root).unwrap();
    std::fs::write(git_root.join("change.txt"), "stage once").unwrap();
    projects.open_worktree(git_root.clone(), git_root.clone());
    let project = projects.current().unwrap();
    let git_project = crate::api::proto::ProjectRef {
        host_id: host.id().into(),
        project_id: serde_json::to_string(&project.id).unwrap(),
        worktree_id: Some(serde_json::to_string(&project.workspace).unwrap()),
    };
    roots.replace_projects(projects.items).unwrap();
    let git = crate::git::GitClient::start().unwrap();
    let files = super::super::resources::FilesRpc::new(
        auth.clone(),
        roots.clone(),
        git.clone(),
        artifacts.clone(),
        operations.clone(),
    );
    let git_rpc = super::super::resources::GitRpc::new(
        auth.clone(),
        roots.clone(),
        git.clone(),
        crate::terminal::environment::ShellEnvironment {
            shell: "/bin/sh".into(),
            vars: Default::default(),
        },
        operations.clone(),
    );
    let tasks = crate::remote::tasks::TasksRpc::new(
        auth.clone(),
        Default::default(),
        operations.clone(),
        Arc::new(NoHttp),
        artifacts,
        Arc::new(Host),
    );
    let features = Routes::new(
        crate::api::proto::settings_service_server::SettingsServiceServer::new(settings.clone()),
    )
    .add_service(proto::tasks_service_server::TasksServiceServer::new(tasks))
    .add_service(crate::api::proto::files_service_server::FilesServiceServer::new(files))
    .add_service(crate::api::proto::git_service_server::GitServiceServer::new(git_rpc))
    .add_service(crate::api::proto::catalog_service_server::CatalogServiceServer::new(api.clone()))
    .add_service(crate::api::proto::thread_service_server::ThreadServiceServer::new(api));
    let server = start(
        "127.0.0.1:0".parse().unwrap(),
        ServerServices {
            auth: auth.clone(),
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
            .get_info(proto::GetInfoRequest {
                environment: Some(proto::EnvironmentRef {
                    host_id: host.id().into()
                })
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
            .get_info(proto::GetInfoRequest {
                environment: Some(proto::EnvironmentRef {
                    host_id: host.id().into()
                })
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
                der.clone(),
                crate::host::READ | crate::host::TASKS | crate::host::CHAT | crate::host::FILES_GIT,
                vec![
                    "private".into(),
                    root.to_str().unwrap().into(),
                    git_root.to_str().unwrap().into(),
                ],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        host_rpc
            .get_info(proto::GetInfoRequest {
                environment: Some(proto::EnvironmentRef {
                    host_id: host.id().into()
                })
            })
            .await
            .unwrap()
            .into_inner()
            .environment
            .unwrap()
            .host_id,
        host.id()
    );
    let integrations = proto::tasks_service_client::TasksServiceClient::new(channel.clone())
        .list_integrations(proto::ListIntegrationsRequest {
            resource: Some(proto::list_integrations_request::Resource::Environment(
                proto::EnvironmentRef {
                    host_id: host.id().into(),
                },
            )),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(integrations.integrations.len(), 1);
    assert_eq!(integrations.integrations[0].id, integration_id);
    assert_eq!(integrations.integrations[0].provider, "github");
    assert!(integrations.integrations[0].configured);
    assert!(!format!("{integrations:?}").contains(&credential_ref));
    use crate::api::proto as api;
    let mut git_rpc = api::git_service_client::GitServiceClient::new(channel.clone());
    let plan = git_rpc
        .preflight(api::PreflightRequest {
            project: Some(git_project.clone()),
            intent: Some(api::preflight_request::Intent::Paths(api::GitPathIntent {
                operation: api::GitPathOperation::Stage as i32,
                paths: vec!["change.txt".into()],
            })),
        })
        .await
        .unwrap()
        .into_inner();
    let stage = api::StageRequest {
        command: Some(api::CommandMeta {
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 1,
            expected_revision: None,
        }),
        project: Some(git_project),
        paths: vec!["change.txt".into()],
        expected: plan.expected,
    };
    let staged = git_rpc.stage(stage.clone()).await.unwrap().into_inner();
    assert_eq!(staged.state, api::OperationState::Succeeded as i32);
    assert_eq!(
        host_rpc
            .get_command(proto::GetCommandRequest {
                environment: Some(proto::EnvironmentRef {
                    host_id: host.id().into()
                }),
                command_id: staged.command_id.clone(),
                command_epoch: 1,
                resource: None,
            })
            .await
            .unwrap()
            .into_inner()
            .receipt
            .unwrap(),
        staged
    );
    assert!(
        git2::Repository::open(&git_root)
            .unwrap()
            .index()
            .unwrap()
            .get_path(std::path::Path::new("change.txt"), 0)
            .is_some()
    );
    let mut changed_stage = stage.clone();
    changed_stage.paths = vec!["another.txt".into()];
    assert_eq!(
        git_rpc
            .stage(changed_stage.clone())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    let mut files_rpc = api::files_service_client::FilesServiceClient::new(channel.clone());
    let create_file = api::CreateFileRequest {
        command: Some(api::CommandMeta {
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 1,
            expected_revision: None,
        }),
        file: Some(api::FileRef {
            target: Some(file_target.clone()),
            relative_path: "once.txt".into(),
        }),
        directory: false,
        content: None,
    };
    // Simulate interruption after durable acceptance and before dispatch. The
    // original typed RPC must recover this state rather than strand the epoch.
    let grant = client.lookup_runtime_device(der).await.unwrap();
    let accept_file = |request: api::CreateFileRequest| {
        let auth = auth.clone();
        let roots = roots.clone();
        let grant = grant.clone();
        async move {
            let file = request.file.unwrap();
            let prepared = crate::api::resources::prepare_file(
                &auth,
                &roots,
                &grant,
                request.command.unwrap(),
                &file.target.unwrap(),
                crate::command_bus::FileCommand::Create {
                    path: file.relative_path,
                    directory: request.directory,
                    content: None,
                },
            )
            .await
            .unwrap();
            auth.client()
                .accept_runtime_command(prepared.runtime_input().clone(), crate::host::FILES_GIT)
                .await
                .unwrap()
        }
    };
    assert_eq!(
        accept_file(create_file.clone()).await.state(),
        crate::host::CommandState::Accepted
    );
    let mut removed_file = create_file.clone();
    removed_file.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    removed_file.file.as_mut().unwrap().relative_path = "never-created.txt".into();
    accept_file(removed_file.clone()).await;
    let created = files_rpc
        .create_file(create_file.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(created.state, api::OperationState::Succeeded as i32);
    assert!(root.join("once.txt").is_file());
    let mut conflicting_file = create_file.clone();
    conflicting_file.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    let rejected_file = files_rpc
        .create_file(conflicting_file.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(rejected_file.state, api::OperationState::Rejected as i32);
    assert_eq!(
        rejected_file.error.as_ref().unwrap().code,
        "conditions_changed"
    );
    let mut changed_file = create_file.clone();
    changed_file.directory = true;
    assert_eq!(
        files_rpc
            .create_file(changed_file.clone())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    roots.replace_projects(Vec::new()).unwrap();
    let removed_result = files_rpc
        .create_file(removed_file.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(removed_result.state, api::OperationState::Rejected as i32);
    assert_eq!(
        removed_result.error.as_ref().unwrap().code,
        "conditions_changed"
    );
    assert!(!root.join("never-created.txt").exists());
    assert_eq!(
        files_rpc
            .create_file(removed_file)
            .await
            .unwrap()
            .into_inner(),
        removed_result
    );
    std::fs::remove_dir_all(&git_root).unwrap();
    assert_eq!(
        git_rpc.stage(stage.clone()).await.unwrap().into_inner(),
        staged
    );
    assert!(!git_root.exists());
    std::fs::remove_file(root.join("once.txt")).unwrap();
    assert_eq!(
        files_rpc
            .create_file(create_file.clone())
            .await
            .unwrap()
            .into_inner(),
        created
    );
    assert_eq!(
        files_rpc
            .create_file(conflicting_file)
            .await
            .unwrap()
            .into_inner(),
        rejected_file
    );
    assert!(!root.join("once.txt").exists());
    let discovery = api::catalog_service_client::CatalogServiceClient::new(channel.clone())
        .list_threads(api::ListThreadsRequest {
            page_token: None,
            limit: 0,
            environment: Some(api::EnvironmentRef {
                host_id: host.id().into(),
            }),
            catalog_id: Some(chat_id.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    let target = discovery.threads[0].target.clone().unwrap();
    assert_eq!(
        target.thread.as_ref().unwrap().thread_id,
        chat.thread_id().unwrap().to_string()
    );
    assert_ne!(target.thread.as_ref().unwrap().thread_id, chat_id);
    let send = api::SendMessageRequest {
        command: Some(api::CommandMeta {
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 1,
            expected_revision: None,
        }),
        target: Some(target.clone()),
        message_id: uuid::Uuid::new_v4().to_string(),
        draft: Some(api::MessageDraft {
            text: "durable message".into(),
            ..Default::default()
        }),
        expected_context: Some(api::SendContext {
            account_id: "test-account".into(),
            profile_id: None,
            history_epoch: 0,
        }),
    };
    let mut thread_rpc = api::thread_service_client::ThreadServiceClient::new(channel.clone());
    let accepted = thread_rpc
        .send_message(send.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(accepted.state, api::OperationState::Succeeded as i32);
    assert_eq!(
        accepted.resource,
        Some(api::command_receipt::Resource::Target(target.clone()))
    );
    assert_eq!(
        thread_rpc
            .send_message(send.clone())
            .await
            .unwrap()
            .into_inner(),
        accepted
    );
    assert_eq!(
        client
            .load_runtime_queue(chat_id.clone())
            .await
            .unwrap()
            .entries()
            .len(),
        1
    );
    let mut changed = send.clone();
    changed.draft.as_mut().unwrap().text = "different intent".into();
    assert_eq!(
        thread_rpc.send_message(changed).await.unwrap_err().code(),
        tonic::Code::Aborted
    );
    let stale_queue = api::RemoveQueueMessageRequest {
        command: Some(api::CommandMeta {
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 1,
            expected_revision: None,
        }),
        target: Some(target.clone()),
        queue_revision: 0,
        message_id: send.message_id.clone(),
    };
    let rejected = thread_rpc
        .remove_queue_message(stale_queue.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(rejected.state, api::OperationState::Rejected as i32);
    assert_eq!(rejected.error.as_ref().unwrap().code, "conditions_changed");
    assert_eq!(
        thread_rpc
            .remove_queue_message(stale_queue)
            .await
            .unwrap()
            .into_inner(),
        rejected
    );
    assert_eq!(
        client
            .load_runtime_queue(chat_id.clone())
            .await
            .unwrap()
            .entries()
            .len(),
        1
    );
    let mut wrong_host = send.clone();
    wrong_host
        .target
        .as_mut()
        .unwrap()
        .thread
        .as_mut()
        .unwrap()
        .host_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        thread_rpc
            .send_message(wrong_host)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::NotFound
    );
    let mut stale_epoch = send.clone();
    stale_epoch.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    stale_epoch.target.as_mut().unwrap().executor_epoch += 1;
    let epoch_rejected = thread_rpc
        .send_message(stale_epoch.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(epoch_rejected.state, api::OperationState::Rejected as i32);
    assert_eq!(
        epoch_rejected.error.as_ref().unwrap().code,
        "conditions_changed"
    );
    assert_eq!(
        epoch_rejected.resource,
        stale_epoch
            .target
            .clone()
            .map(api::command_receipt::Resource::Target)
    );
    assert_eq!(
        thread_rpc
            .send_message(stale_epoch.clone())
            .await
            .unwrap()
            .into_inner(),
        epoch_rejected
    );
    assert_eq!(
        client
            .load_runtime_queue(chat_id.clone())
            .await
            .unwrap()
            .entries()
            .len(),
        1
    );
    let mut changed_epoch_intent = stale_epoch.clone();
    changed_epoch_intent.draft.as_mut().unwrap().text = "different stale intent".into();
    assert_eq!(
        thread_rpc
            .send_message(changed_epoch_intent)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    let environment = Some(proto::EnvironmentRef {
        host_id: host.id().into(),
    });
    let original_lookup = proto::GetCommandRequest {
        environment: environment.clone(),
        command_id: accepted.command_id.clone(),
        command_epoch: 1,
        resource: Some(proto::CommandResource {
            target: Some(proto::command_resource::Target::Thread(target.clone())),
        }),
    };
    assert_eq!(
        host_rpc
            .get_command(original_lookup.clone())
            .await
            .unwrap()
            .into_inner()
            .receipt
            .unwrap(),
        accepted
    );
    let grant = client
        .lookup_runtime_device(
            client
                .runtime_device_certificate(id.clone())
                .await
                .unwrap()
                .unwrap(),
        )
        .await
        .unwrap();
    let pending = crate::command_bus::CommandBus::new(client.clone())
        .prepare(
            crate::api::decode::envelope(
                proto::CommandMeta {
                    command_id: uuid::Uuid::new_v4().to_string(),
                    command_epoch: 1,
                    expected_revision: None,
                },
                crate::api::decode::target(target.clone()).unwrap(),
                crate::command_bus::Command::Control(crate::command_bus::Control::Start),
            )
            .unwrap(),
            crate::command_bus::Principal::authenticated(&grant, 1).unwrap(),
        )
        .await
        .unwrap();
    let pending = client
        .accept_runtime_command(pending.runtime_input().clone(), crate::host::CHAT)
        .await
        .unwrap();
    let close = proto::CloseCommandEpochRequest {
        environment: environment.clone(),
        expected_epoch: 1,
    };
    assert_eq!(
        host_rpc
            .close_command_epoch(close.clone())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    client
        .transition_runtime_command(
            pending.key().clone(),
            crate::host::CommandState::Accepted,
            crate::host::CommandState::Attempting,
            None,
        )
        .await
        .unwrap();
    client
        .transition_runtime_command(
            pending.key().clone(),
            crate::host::CommandState::Attempting,
            crate::host::CommandState::DeliveryUnknown,
            None,
        )
        .await
        .unwrap();
    let closed = host_rpc
        .close_command_epoch(close.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(closed.epoch, 2);
    assert_eq!(
        host_rpc
            .close_command_epoch(close)
            .await
            .unwrap()
            .into_inner(),
        closed
    );
    let lookup = proto::GetCommandRequest {
        command_id: pending.key().id().into(),
        ..original_lookup.clone()
    };
    let unknown = host_rpc
        .get_command(lookup.clone())
        .await
        .unwrap()
        .into_inner()
        .receipt
        .unwrap();
    assert_eq!(unknown.state, proto::OperationState::DeliveryUnknown as i32);
    assert!(!unknown.resolved_by_user);
    let resolve = proto::ResolveCommandRequest {
        environment: lookup.environment.clone(),
        command_id: lookup.command_id.clone(),
        command_epoch: lookup.command_epoch,
        resource: lookup.resource.clone(),
    };
    let archived = host_rpc
        .resolve_command(resolve.clone())
        .await
        .unwrap()
        .into_inner()
        .receipt
        .unwrap();
    assert_eq!(
        archived.state,
        proto::OperationState::DeliveryUnknown as i32
    );
    assert!(archived.resolved_by_user);
    assert_eq!(
        host_rpc
            .resolve_command(resolve)
            .await
            .unwrap()
            .into_inner()
            .receipt
            .unwrap(),
        archived
    );
    assert_eq!(
        host_rpc
            .close_command_epoch(proto::CloseCommandEpochRequest {
                environment: environment.clone(),
                expected_epoch: 2
            })
            .await
            .unwrap()
            .into_inner()
            .epoch,
        3
    );
    assert_eq!(
        host_rpc
            .get_command(lookup)
            .await
            .unwrap()
            .into_inner()
            .state,
        proto::CommandLookupState::Retired as i32
    );
    assert_eq!(
        host_rpc
            .close_command_epoch(proto::CloseCommandEpochRequest {
                environment,
                expected_epoch: 1
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
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
    for request in [create_file, changed_file] {
        assert_eq!(
            files_rpc.create_file(request).await.unwrap_err().code(),
            tonic::Code::PermissionDenied
        );
    }
    // The old successful receipt is still protected by the current grant.
    assert_eq!(
        thread_rpc
            .send_message(send.clone())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    assert_eq!(
        thread_rpc
            .send_message(stale_epoch)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    let readonly = proto::tasks_service_client::TasksServiceClient::new(channel.clone())
        .list_integrations(proto::ListIntegrationsRequest {
            resource: Some(proto::list_integrations_request::Resource::Environment(
                proto::EnvironmentRef {
                    host_id: host.id().into(),
                },
            )),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(readonly.integrations, integrations.integrations);
    let admin_error = proto::settings_service_client::SettingsServiceClient::new(channel.clone())
        .get_settings(proto::GetSettingsRequest {
            environment: Some(proto::EnvironmentRef {
                host_id: host.id().into(),
            }),
        })
        .await
        .unwrap_err();
    assert_eq!(admin_error.code(), tonic::Code::PermissionDenied);
    assert_eq!(
        host_rpc
            .read_diagnostics(proto::ReadDiagnosticsRequest {
                environment: Some(proto::EnvironmentRef {
                    host_id: host.id().into()
                })
            })
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
    for request in [stage, changed_stage] {
        assert_eq!(
            git_rpc.stage(request).await.unwrap_err().code(),
            tonic::Code::PermissionDenied
        );
    }
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
        .read_diagnostics(proto::ReadDiagnosticsRequest {
            environment: Some(proto::EnvironmentRef {
                host_id: host.id().into(),
            }),
        })
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
    let (revoked_id, revoked_der, _) = device(&identity);
    client
        .register_runtime_device(
            crate::host::DeviceRegistration::new(
                revoked_id.clone(),
                "revoked by command".into(),
                revoked_der,
                crate::host::READ,
                vec!["private".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let revoke = proto::RevokeDeviceRequest {
        command: Some(proto::CommandMeta {
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 3,
            expected_revision: None,
        }),
        environment: Some(proto::EnvironmentRef {
            host_id: host.id().into(),
        }),
        device_id: revoked_id.clone(),
    };
    let revoked = host_rpc
        .revoke_device(revoke.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(revoked.state, proto::OperationState::Succeeded as i32);
    assert!(revoked.result.is_none());
    assert_eq!(
        host_rpc
            .revoke_device(revoke.clone())
            .await
            .unwrap()
            .into_inner(),
        revoked
    );
    assert!(
        client
            .runtime_devices_page(None, true)
            .await
            .unwrap()
            .0
            .iter()
            .any(|d| d.id() == revoked_id && d.revoked())
    );
    assert_eq!(
        host_rpc
            .get_command(proto::GetCommandRequest {
                environment: revoke.environment.clone(),
                command_id: revoked.command_id.clone(),
                command_epoch: 3,
                resource: Some(proto::CommandResource {
                    target: Some(proto::command_resource::Target::Environment(
                        revoke.environment.clone().unwrap()
                    ))
                }),
            })
            .await
            .unwrap()
            .into_inner()
            .receipt
            .unwrap(),
        revoked
    );
    let mut changed = revoke;
    changed.device_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        host_rpc
            .revoke_device(changed.clone())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    changed.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    let absent = host_rpc
        .revoke_device(changed.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(absent.state, proto::OperationState::Rejected as i32);
    assert_eq!(absent.error.as_ref().unwrap().code, "not_found");
    assert_eq!(
        host_rpc.revoke_device(changed).await.unwrap().into_inner(),
        absent
    );
    let mut host_watch = host_rpc
        .watch_host(proto::WatchHostRequest {
            environment: Some(proto::EnvironmentRef {
                host_id: host.id().into(),
            }),
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
    assert_eq!(
        thread_rpc.send_message(send).await.unwrap_err().code(),
        tonic::Code::PermissionDenied
    );
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
    git.shutdown().await;
    client.shutdown().await.unwrap();
}
