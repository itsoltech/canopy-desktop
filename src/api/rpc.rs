use super::{
    decode::{self, required},
    proto as p,
};
use crate::{
    command_bus::{
        Command, CommandBus, CommandEnvelope, DraftResources, Principal, QueueChange, ResourceRef,
    },
    host::{self, CommandReceipt, DeviceGrant, UnknownResolution},
    remote::{
        auth::{RpcAuth, storage_status},
        host::HostState,
        resources::ResourceLocks,
    },
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Publication after commit uses the existing runtime owner. It cannot replay effects.
#[tonic::async_trait]
pub trait ThreadPublisher: Send + Sync + 'static {
    async fn committed(&self, receipt: &CommandReceipt);
}

#[derive(Clone)]
pub struct ThreadRpc {
    auth: RpcAuth,
    state: HostState,
    operations: ResourceLocks,
    resources: DraftResources,
    publisher: Arc<dyn ThreadPublisher>,
}
impl ThreadRpc {
    pub fn new(
        auth: RpcAuth,
        state: HostState,
        operations: ResourceLocks,
        resources: DraftResources,
        publisher: Arc<dyn ThreadPublisher>,
    ) -> Self {
        Self {
            auth,
            state,
            operations,
            resources,
            publisher,
        }
    }
    async fn execute(
        &self,
        grant: DeviceGrant,
        meta: p::CommandMeta,
        target: p::ThreadTarget,
        command: Command,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.state.accepting()?;
        let envelope = decode::envelope(meta.clone(), decode::target(target.clone())?, command)?;
        let principal =
            Principal::authenticated(&grant, meta.command_epoch).map_err(storage_status)?;
        let receipt = self.execute_owned(envelope, principal).await?;
        Ok(Response::new(super::compat::receipt(
            &self.auth,
            &receipt,
            p::command_receipt::Resource::Target(target),
        )?))
    }
    async fn execute_owned(
        &self,
        envelope: CommandEnvelope,
        principal: Principal,
    ) -> Result<CommandReceipt, Status> {
        let owned = self.operations.begin()?;
        let bus = CommandBus::new(self.auth.client().clone());
        let resources = self.resources.clone();
        let publisher = self.publisher.clone();
        tokio::spawn(async move {
            let _owned = owned;
            let receipt = bus
                .execute(envelope, principal, Some(&resources))
                .await
                .map_err(storage_status)?;
            publisher.committed(&receipt).await;
            Ok(receipt)
        })
        .await
        .map_err(|_| {
            Status::unknown(
                "Command outcome is uncertain; retry the original RPC with the same ID.",
            )
        })?
    }
    async fn queue(
        &self,
        grant: DeviceGrant,
        meta: Option<p::CommandMeta>,
        target: Option<p::ThreadTarget>,
        revision: u64,
        change: QueueChange,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.execute(
            grant,
            required(meta)?,
            required(target)?,
            Command::Queue { revision, change },
        )
        .await
    }
}

#[tonic::async_trait]
impl p::catalog_service_server::CatalogService for ThreadRpc {
    async fn list_threads(
        &self,
        r: Request<p::ListThreadsRequest>,
    ) -> Result<Response<p::ListThreadsResponse>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let environment = decode::environment(required(r.environment)?)?;
        self.auth.check_host(environment.host_id.as_str())?;
        self.auth.scope(&grant, host::READ)?;
        let records = if let Some(id) = r.catalog_id {
            self.auth
                .client()
                .load_runtime_chat(id)
                .await
                .map_err(storage_status)?
                .into_iter()
                .collect()
        } else {
            self.auth
                .client()
                .load_runtime_chats()
                .await
                .map_err(storage_status)?
        };
        let mut threads = Vec::new();
        for record in records {
            match self
                .auth
                .resource(grant.id(), host::READ, record.authority_root())
                .await
            {
                Ok(_) => (),
                Err(e) if e.code() == tonic::Code::PermissionDenied => continue,
                Err(e) => return Err(e),
            }
            let stored = record
                .thread()
                .ok_or_else(|| Status::internal("Thread identity is unavailable."))?;
            threads.push(p::ThreadSummary {
                target: Some(p::ThreadTarget {
                    thread: Some(p::ThreadRef {
                        host_id: stored.thread().host_id.to_string(),
                        thread_id: stored.thread().thread_id.to_string(),
                    }),
                    executor_epoch: stored.executor_epoch(),
                }),
                catalog_id: record.id().into(),
                title: record.metadata().title.clone().unwrap_or_default(),
                revision: stored.revision(),
            });
        }
        let response = p::ListThreadsResponse { threads };
        if prost::Message::encoded_len(&response) > host::CONTROL_BYTES {
            return Err(Status::resource_exhausted(
                "Thread discovery exceeds the control limit; use a catalog key.",
            ));
        }
        Ok(Response::new(response))
    }
    async fn rename_thread(
        &self,
        r: Request<p::RenameThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Rename { title: r.title },
        )
        .await
    }
}

#[tonic::async_trait]
impl p::thread_service_server::ThreadService for ThreadRpc {
    async fn send_message(
        &self,
        r: Request<p::SendMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let context = required(r.expected_context)?;
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::ThreadSend {
                message: r.message_id,
                draft: decode::draft(required(r.draft)?)?,
                account: context.account_id,
                profile: context.profile_id,
                history_epoch: context.history_epoch,
            },
        )
        .await
    }
    async fn edit_queue_message(
        &self,
        r: Request<p::EditQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Edit {
                message: r.message_id,
                draft: decode::draft(required(r.draft)?)?,
            },
        )
        .await
    }
    async fn move_queue_message(
        &self,
        r: Request<p::MoveQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Move {
                message: r.message_id,
                before: r.before_message_id,
            },
        )
        .await
    }
    async fn remove_queue_message(
        &self,
        r: Request<p::RemoveQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Remove {
                message: r.message_id,
            },
        )
        .await
    }
    async fn pause_queue(
        &self,
        r: Request<p::PauseQueueRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Pause { paused: r.paused },
        )
        .await
    }
    async fn resolve_unknown_queue_message(
        &self,
        r: Request<p::ResolveUnknownQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let resolution = match p::UnknownResolution::try_from(r.resolution) {
            Ok(p::UnknownResolution::AcceptAsDelivered) => UnknownResolution::Delivered,
            Ok(p::UnknownResolution::SendAgain) => UnknownResolution::Resend {
                message: required(r.new_message_id)?,
            },
            Ok(p::UnknownResolution::Dismiss) => UnknownResolution::Dismissed,
            _ => {
                return Err(Status::invalid_argument(
                    "Choose how to resolve the uncertain message.",
                ));
            }
        };
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::ResolveUnknown {
                message: r.message_id,
                resolution,
            },
        )
        .await
    }
}

#[tonic::async_trait]
impl p::settings_service_server::SettingsService for crate::remote::settings_rpc::SettingsRpc {
    async fn update_preferences(
        &self,
        r: Request<p::UpdatePreferencesRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.api_preferences(r).await
    }
}

pub(crate) fn preferences_envelope(
    mut r: p::UpdatePreferencesRequest,
) -> Result<(p::CommandMeta, p::EnvironmentRef, CommandEnvelope), Status> {
    let meta = required(r.command.take())?;
    let environment = required(r.environment.take())?;
    let envelope = decode::envelope(
        meta.clone(),
        ResourceRef::Environment(decode::environment(environment.clone())?),
        decode::preferences(r),
    )?;
    Ok((meta, environment, envelope))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Publication {
        started: tokio::sync::Notify,
        finish: tokio::sync::Notify,
    }
    #[tonic::async_trait]
    impl ThreadPublisher for Publication {
        async fn committed(&self, _: &CommandReceipt) {
            self.started.notify_one();
            self.finish.notified().await;
        }
    }
    #[tokio::test]
    async fn cancelled_rpc_keeps_committed_effect_and_publication_owned_until_drain() {
        let dir = tempfile::tempdir().unwrap();
        let client = crate::settings::SettingsClient::create(dir.path().join("test.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let record = client
            .save_runtime_chat(
                host::ChatRecord::new(
                    id.clone(),
                    None,
                    "test-account".into(),
                    crate::state::workspace::PaneMetadata {
                        kind: crate::state::workspace::PaneKind::Chat,
                        chat: Some(Box::new(crate::chat::config::ChatSettings {
                            id: Some(id.clone()),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
            .await
            .unwrap();
        client
            .register_runtime_device(
                host::DeviceRegistration::new(
                    uuid::Uuid::new_v4().to_string(),
                    "test".into(),
                    vec![7; 32],
                    host::ALL_SCOPES,
                    vec!["private".into()],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let grant = client.lookup_runtime_device(vec![7; 32]).await.unwrap();
        let state = HostState::new(host.id(), vec![]);
        state.set(crate::remote::proto::HostReadiness::Ready, String::new());
        let operations = ResourceLocks::default();
        let publication = Arc::new(Publication {
            started: tokio::sync::Notify::new(),
            finish: tokio::sync::Notify::new(),
        });
        let rpc = ThreadRpc::new(
            RpcAuth::new(host.id().into(), client.clone()).unwrap(),
            state,
            operations.clone(),
            DraftResources {
                roots: Default::default(),
                pulls: Default::default(),
            },
            publication.clone(),
        );
        let stored = record.thread().unwrap();
        let target = p::ThreadTarget {
            thread: Some(p::ThreadRef {
                host_id: host.id().into(),
                thread_id: stored.thread().thread_id.to_string(),
            }),
            executor_epoch: stored.executor_epoch(),
        };
        let request = tokio::spawn(async move {
            rpc.execute(
                grant,
                p::CommandMeta {
                    command_id: uuid::Uuid::new_v4().to_string(),
                    command_epoch: 1,
                    expected_revision: None,
                },
                target,
                Command::Rename {
                    title: "committed before cancel".into(),
                },
            )
            .await
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            publication.started.notified(),
        )
        .await
        .unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        let drain = operations.drain();
        tokio::pin!(drain);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut drain)
                .await
                .is_err()
        );
        publication.finish.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(2), drain)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            client
                .load_runtime_chat(id)
                .await
                .unwrap()
                .unwrap()
                .metadata()
                .title
                .as_deref(),
            Some("committed before cancel")
        );
        client.shutdown().await.unwrap();
    }
}
