//! Typed command boundaries over the existing bulk store and operation owner.
use super::{ArtifactsRpc, blocking, required};
use crate::{
    api::{self, proto as p},
    artifacts::{BeginUpload, CHUNK_BYTES, Purpose, Target, Upload},
    command_bus::{
        ArtifactCommand as C, Command, CommandBus, Principal, UploadContent,
        UploadDestination as D, UploadOrigin,
    },
    host::{CommandState, DeviceGrant},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, pin::Pin};
use tonic::{Request, Response, Status};

enum Address {
    Resource(p::ResourceTarget),
    Environment(p::EnvironmentRef),
}
impl Address {
    fn receipt_resource(&self) -> Result<p::command_receipt::Resource, Status> {
        Ok(match self {
            Self::Environment(e) => p::command_receipt::Resource::Environment(e.clone()),
            Self::Resource(r) => match required(r.resource.clone())? {
                p::resource_target::Resource::Project(p) => {
                    p::command_receipt::Resource::Project(p)
                }
                p::resource_target::Resource::Thread(t) => p::command_receipt::Resource::Target(t),
            },
        })
    }
}
fn image(target: Option<p::ThreadTarget>) -> Result<Address, Status> {
    Ok(Address::Resource(p::ResourceTarget {
        resource: Some(p::resource_target::Resource::Thread(required(target)?)),
    }))
}
fn file(file: Option<p::FileRef>) -> Result<(Address, D), Status> {
    let f = required(file)?;
    Ok((
        Address::Resource(required(f.target)?),
        D::File {
            path: f.relative_path,
        },
    ))
}
fn task(task: Option<p::TaskRef>) -> Result<(Address, D), Status> {
    let task = required(task)?;
    let project = required(task.project)?;
    let address = match required(project.resource)? {
        p::task_project::Resource::Project(p) => Address::Resource(p::ResourceTarget {
            resource: Some(p::resource_target::Resource::Project(p)),
        }),
        p::task_project::Resource::Environment(e) => Address::Environment(e),
    };
    Ok((
        address,
        D::Task {
            integration: project.integration_id,
            project: project.project_key,
            task: task.task_id,
        },
    ))
}
fn content(content: Option<p::UploadContent>) -> Result<UploadContent, Status> {
    let c = required(content)?;
    Ok(UploadContent {
        size: c.size,
        sha256: c
            .sha256
            .try_into()
            .map_err(|_| Status::invalid_argument("SHA256 must contain 32 bytes."))?,
        media_type: c.media_type,
    })
}
fn origin(origin: Option<p::UploadOrigin>) -> Result<UploadOrigin, Status> {
    let origin = required(origin)?;
    Ok(UploadOrigin {
        command: origin.command_id,
        epoch: origin.command_epoch,
    })
}
impl ArtifactsRpc {
    async fn validate_begin(
        &self,
        grant: &DeviceGrant,
        address: &Address,
        root: &str,
        scope: u32,
    ) -> Result<(), Status> {
        match address {
            Address::Resource(target) => match api::resources::resource(target)? {
                crate::command_bus::ResourceRef::Thread {
                    thread,
                    executor_epoch,
                } => {
                    let record = self
                        .auth
                        .client()
                        .load_runtime_thread(thread.thread_id)
                        .await
                        .map_err(crate::remote::auth::storage_status)?
                        .ok_or_else(|| {
                            Status::failed_precondition("The upload thread is unavailable.")
                        })?;
                    if record.authority_root() != root
                        || record
                            .thread()
                            .is_none_or(|t| t.executor_epoch() != executor_epoch)
                    {
                        return Err(Status::failed_precondition("The upload thread changed."));
                    }
                    self.auth.resource(grant.id(), scope, root).await?;
                }
                crate::command_bus::ResourceRef::Project(_) => {
                    api::resources::revalidate_root(
                        &self.auth,
                        &self.roots,
                        grant,
                        target,
                        scope,
                        std::path::Path::new(root),
                    )
                    .await?;
                }
                crate::command_bus::ResourceRef::Environment(_) => unreachable!(),
            },
            Address::Environment(environment) => {
                self.auth.check_host(&environment.host_id)?;
                self.auth.resource(grant.id(), scope, "private").await?;
            }
        }
        Ok(())
    }
    async fn begin_authority(
        &self,
        grant: &DeviceGrant,
        address: &Address,
        command: &C,
    ) -> Result<Option<String>, Status> {
        let (origin, upload) = match command {
            C::Begin { .. } => return Ok(None),
            C::Finish { origin, upload, .. } | C::Cancel { origin, upload, .. } => (origin, upload),
        };
        let target = match address {
            Address::Resource(target) => api::resources::resource(target)?,
            Address::Environment(e) => {
                crate::command_bus::ResourceRef::Environment(crate::thread::EnvironmentRef {
                    host_id: crate::thread::HostId::new(e.host_id.clone())
                        .ok_or_else(|| Status::invalid_argument("Invalid host identity."))?,
                })
            }
        };
        self.auth.check_host(target.host_id().as_str())?;
        let mut key = crate::host::CommandKey::remote(
            grant.id().into(),
            origin.epoch,
            origin.command.clone(),
        )
        .ok_or_else(|| Status::invalid_argument("Invalid upload origin."))?;
        (key.resource, key.executor_epoch) = target.storage_key();
        let receipt = self
            .auth
            .client()
            .get_runtime_command(key, command.metadata().scope)
            .await
            .map_err(crate::remote::auth::storage_status)?
            .ok_or_else(|| Status::failed_precondition("Upload origin is unavailable."))?;
        let method = C::Begin {
            destination: command.destination().clone(),
            content: UploadContent {
                size: 0,
                sha256: [0; 32],
                media_type: String::new(),
            },
        }
        .metadata()
        .name;
        if receipt.method() != method || receipt.state() != crate::host::CommandState::Succeeded {
            return Err(Status::failed_precondition(
                "Upload origin did not succeed.",
            ));
        }
        let result = p::ArtifactResult::decode(receipt.result().unwrap_or_default())
            .map_err(|_| Status::data_loss("Upload origin is unreadable."))?;
        if result.upload_id != *upload {
            return Err(Status::failed_precondition(
                "Upload differs from its origin.",
            ));
        }
        Ok(
            matches!(target, crate::command_bus::ResourceRef::Project(_))
                .then(|| receipt.target().into()),
        )
    }
    fn api_status(&self, upload: &Upload) -> p::UploadStatus {
        let binding = upload.binding();
        p::UploadStatus {
            environment: Some(p::EnvironmentRef {
                host_id: self.auth.host_id().into(),
            }),
            upload_id: upload.id().into(),
            received: upload
                .ranges()
                .iter()
                .map(|&(start, end)| p::ByteRange {
                    offset: start,
                    length: end - start,
                })
                .collect(),
            artifact: upload.artifact_id().map(|id| p::ArtifactRef {
                host_id: self.auth.host_id().into(),
                artifact_id: id.into(),
                media_type: binding.media_type().into(),
                size: binding.size(),
                sha256: binding.sha256().to_vec(),
            }),
        }
    }
    async fn api_command(
        &self,
        grant: DeviceGrant,
        meta: Option<p::CommandMeta>,
        address: Address,
        command: C,
    ) -> Result<p::CommandReceipt, Status> {
        let resource = address.receipt_resource()?;
        let meta = required(meta)?;
        let prepared = match &address {
            Address::Resource(target) => {
                api::resources::prepare(
                    &self.auth,
                    &self.roots,
                    &grant,
                    meta.clone(),
                    target,
                    Command::Artifact(command.clone()),
                )
                .await
            }
            Address::Environment(environment) => {
                api::resources::prepare_environment(
                    &self.auth,
                    &grant,
                    meta.clone(),
                    environment.clone(),
                    Command::Artifact(command.clone()),
                )
                .await
            }
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) if error.code() == tonic::Code::NotFound => {
                let Address::Resource(target) = &address else {
                    return Err(error);
                };
                let resource = api::resources::resource(target)?;
                if !matches!(resource, crate::command_bus::ResourceRef::Project(_)) {
                    return Err(error);
                }
                let Some(root) = self.begin_authority(&grant, &address, &command).await? else {
                    return Err(error);
                };
                let principal = Principal::authenticated(&grant, meta.command_epoch)
                    .map_err(crate::remote::auth::storage_status)?;
                CommandBus::new(self.auth.client().clone())
                    .prepare_project_resource(
                        api::decode::envelope(meta, resource, Command::Artifact(command.clone()))?,
                        principal,
                        Some(root),
                    )
                    .await
                    .map_err(crate::remote::auth::storage_status)?
                    .ok_or_else(|| Status::internal("Upload admission is incomplete."))?
            }
            Err(error) => return Err(error),
        };
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return api::resources::receipt_for_resource(&self.auth, prior, resource);
        }
        let input = prepared.runtime_input().clone();
        let root = input
            .root
            .clone()
            .ok_or_else(|| Status::internal("Missing artifact authority."))?;
        let (target, purpose) = match command.destination() {
            D::Image => (Target::Chat(input.target.clone()), Purpose::Image),
            D::File { path } => (
                Target::File {
                    root: root.clone(),
                    path: path.clone(),
                },
                Purpose::FileSave,
            ),
            D::Task {
                integration,
                project,
                task,
            } => (
                Target::Task {
                    root: root.clone(),
                    integration_id: integration.clone(),
                    project_id: project.clone(),
                    task_id: task.clone(),
                },
                Purpose::TaskAttachment,
            ),
        };
        let key = &input.key;
        let digest = Sha256::digest(
            serde_json::to_vec(&(
                self.auth.host_id(),
                key.device(),
                key.epoch(),
                key.id(),
                key.resource(),
                key.executor_epoch(),
            ))
            .map_err(|_| Status::internal("Invalid command identity."))?,
        );
        let mut id = [0; 16];
        id.copy_from_slice(&digest[..16]);
        let generated = uuid::Uuid::from_bytes(id).to_string();
        let binding = if let C::Begin { content, .. } = &command {
            Some(
                BeginUpload::new(
                    grant.id().into(),
                    target.clone(),
                    purpose.clone(),
                    content.size,
                    content.sha256,
                    content.media_type.clone(),
                )
                .map_err(crate::remote::auth::storage_status)?,
            )
        } else {
            None
        };
        let service = self.clone();
        let done = crate::remote::resources::validated_mutation(
            self.auth.clone(),
            self.locks.clone(),
            PathBuf::from(&root),
            input,
            move || async move {
                let guard = service.gate.clone().lock_owned().await;
                if matches!(command, C::Begin { .. }) {
                    service
                        .validate_begin(&grant, &address, &root, command.metadata().scope)
                        .await?;
                } else if let Some(saved_root) =
                    service.begin_authority(&grant, &address, &command).await?
                    && saved_root != root
                {
                    return Err(Status::failed_precondition("Upload project changed."));
                }
                if let C::Finish { upload, .. } | C::Cancel { upload, .. } = &command {
                    let current = service.status(grant.id(), upload).await?;
                    if current.binding().target() != &target
                        || current.binding().purpose() != &purpose
                    {
                        return Err(Status::failed_precondition(
                            "Upload belongs to a different resource or purpose.",
                        ));
                    }
                    if matches!(command, C::Finish { .. })
                        && current.binding().size() != 0
                        && current.ranges() != [(0, current.binding().size())]
                    {
                        return Err(Status::failed_precondition("Upload is incomplete."));
                    }
                }
                Ok(async move {
                    let _guard = guard;
                    let owner = grant.id().to_owned();
                    let store = service.store.clone();
                    let host_id = service.auth.host_id().to_owned();
                    let result = blocking(move || {
                        Ok(match command {
                            C::Begin { .. } => {
                                let upload = store.begin_with_id(generated, binding.unwrap())?;
                                p::ArtifactResult {
                                    upload_id: upload.id().into(),
                                    artifact: None,
                                }
                            }
                            C::Finish { upload, .. } => {
                                let artifact = store.finish(&owner, &upload)?;
                                p::ArtifactResult {
                                    upload_id: upload,
                                    artifact: Some(p::ArtifactRef {
                                        host_id,
                                        artifact_id: artifact.id().into(),
                                        media_type: artifact.media_type().into(),
                                        size: artifact.size(),
                                        sha256: artifact.sha256().to_vec(),
                                    }),
                                }
                            }
                            C::Cancel { upload, .. } => {
                                store.cancel(&owner, &upload)?;
                                p::ArtifactResult {
                                    upload_id: upload,
                                    artifact: None,
                                }
                            }
                        })
                    })
                    .await?;
                    Ok(result.encode_to_vec())
                })
            },
        )
        .await?;
        api::resources::receipt_for_resource(&self.auth, &done, resource)
    }
}

#[tonic::async_trait]
impl p::artifacts_service_server::ArtifactsService for ArtifactsRpc {
    async fn begin_image_upload(
        &self,
        r: Request<p::BeginImageUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.api_command(
            g,
            r.command,
            image(r.target)?,
            C::Begin {
                destination: D::Image,
                content: content(r.content)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn finish_image_upload(
        &self,
        r: Request<p::FinishImageUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.api_command(
            g,
            r.command,
            image(r.target)?,
            C::Finish {
                destination: D::Image,
                upload: r.upload_id,
                origin: origin(r.origin)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn cancel_image_upload(
        &self,
        r: Request<p::CancelImageUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.api_command(
            g,
            r.command,
            image(r.target)?,
            C::Cancel {
                destination: D::Image,
                upload: r.upload_id,
                origin: origin(r.origin)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn begin_file_upload(
        &self,
        r: Request<p::BeginFileUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let (address, destination) = file(r.file)?;
        self.api_command(
            g,
            r.command,
            address,
            C::Begin {
                destination,
                content: content(r.content)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn finish_file_upload(
        &self,
        r: Request<p::FinishFileUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let (address, destination) = file(r.file)?;
        self.api_command(
            g,
            r.command,
            address,
            C::Finish {
                destination,
                upload: r.upload_id,
                origin: origin(r.origin)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn cancel_file_upload(
        &self,
        r: Request<p::CancelFileUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let (address, destination) = file(r.file)?;
        self.api_command(
            g,
            r.command,
            address,
            C::Cancel {
                destination,
                upload: r.upload_id,
                origin: origin(r.origin)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn begin_task_upload(
        &self,
        r: Request<p::BeginTaskUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let (address, destination) = task(r.task)?;
        self.api_command(
            g,
            r.command,
            address,
            C::Begin {
                destination,
                content: content(r.content)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn finish_task_upload(
        &self,
        r: Request<p::FinishTaskUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let (address, destination) = task(r.task)?;
        self.api_command(
            g,
            r.command,
            address,
            C::Finish {
                destination,
                upload: r.upload_id,
                origin: origin(r.origin)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn cancel_task_upload(
        &self,
        r: Request<p::CancelTaskUploadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let (address, destination) = task(r.task)?;
        self.api_command(
            g,
            r.command,
            address,
            C::Cancel {
                destination,
                upload: r.upload_id,
                origin: origin(r.origin)?,
            },
        )
        .await
        .map(Response::new)
    }
    async fn get_upload(
        &self,
        r: Request<p::GetUploadRequest>,
    ) -> Result<Response<p::UploadStatus>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.auth
            .check_host(&required(r.get_ref().environment.clone())?.host_id)?;
        Ok(Response::new(self.api_status(
            &self.status(g.id(), &r.get_ref().upload_id).await?,
        )))
    }
    async fn put_chunk(
        &self,
        r: Request<p::PutChunkRequest>,
    ) -> Result<Response<p::UploadStatus>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.auth.check_host(&required(r.environment)?.host_id)?;
        if r.data.is_empty() || r.data.len() > CHUNK_BYTES {
            return Err(Status::resource_exhausted(
                "Artifact chunks must contain 1 to 65536 bytes.",
            ));
        }
        let operation = self.locks.begin()?;
        let store = self.store.clone();
        let upload = tokio::spawn(async move {
            let _operation = operation;
            blocking(move || store.put_chunk(g.id(), &r.upload_id, r.offset, &r.data)).await
        })
        .await
        .map_err(|_| Status::unavailable("Chunk acknowledgement was lost."))??;
        Ok(Response::new(self.api_status(&upload)))
    }
    type DownloadStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<p::ArtifactChunk, Status>> + Send>>;
    async fn download(
        &self,
        r: Request<p::DownloadRequest>,
    ) -> Result<Response<Self::DownloadStream>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let reference = required(r.artifact)?;
        self.auth.check_host(&reference.host_id)?;
        let (artifact, _) = self
            .auth
            .client()
            .authorize_artifact_download(g.id().into(), reference.artifact_id.clone())
            .await
            .map_err(crate::remote::auth::storage_status)?;
        if reference.size != artifact.size()
            || reference.sha256 != artifact.sha256()
            || reference.media_type != artifact.media_type()
            || r.offset > artifact.size()
        {
            return Err(Status::aborted(
                "Artifact metadata or offset does not match.",
            ));
        }
        let operation = self.locks.begin()?;
        let service = self.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _operation = operation;
            let mut offset = r.offset;
            loop {
                let store = service.store.clone();
                let owner = g.id().to_owned();
                let id = reference.artifact_id.clone();
                let data = match blocking(move || store.download(&owner, &id, offset, CHUNK_BYTES))
                    .await
                {
                    Ok(data) => data,
                    Err(error) => {
                        let _ = tx.try_send(Err(error));
                        break;
                    }
                };
                let next = offset + data.len() as u64;
                let complete = next == reference.size;
                let send = tx.send(Ok(p::ArtifactChunk {
                    artifact: Some(reference.clone()),
                    offset,
                    data,
                    complete,
                }));
                tokio::pin!(send);
                let deadline = tokio::time::sleep(std::time::Duration::from_secs(30));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => return,
                        result = &mut send => { if result.is_err() { return; } break; },
                        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                            if service.auth.client().authorize_artifact_download(g.id().into(), reference.artifact_id.clone()).await.is_err() { return; }
                        }
                    }
                }
                if complete {
                    break;
                }
                offset = next;
            }
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture(path: &std::path::Path) -> (ArtifactsRpc, DeviceGrant, String) {
        use crate::{
            artifacts::ArtifactStore,
            host::{ChatRecord, DeviceRegistration},
            settings::SettingsClient,
            state::workspace::{PaneKind, PaneMetadata},
        };
        use std::sync::Arc;
        let client = SettingsClient::create(path.join("db")).await.unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let chat = uuid::Uuid::new_v4().to_string();
        client
            .save_runtime_chat(
                ChatRecord::new(
                    chat.clone(),
                    None,
                    "test-agent".into(),
                    PaneMetadata {
                        kind: PaneKind::Chat,
                        chat: Some(Box::new(crate::chat::config::ChatSettings {
                            id: Some(chat.clone()),
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
                DeviceRegistration::new(
                    uuid::Uuid::new_v4().to_string(),
                    "test".into(),
                    vec![4; 32],
                    crate::host::READ | crate::host::CHAT | crate::host::TASKS,
                    vec!["private".into()],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let grant = client.lookup_runtime_device(vec![4; 32]).await.unwrap();
        let store = Arc::new(ArtifactStore::open(client.clone(), path.join("bulk")).unwrap());
        (
            ArtifactsRpc::new(
                crate::remote::auth::RpcAuth::new(host.id().into(), client).unwrap(),
                Default::default(),
                Default::default(),
                store,
            ),
            grant,
            chat,
        )
    }
    fn meta() -> p::CommandMeta {
        p::CommandMeta {
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 1,
            expected_revision: None,
        }
    }
    fn result(receipt: &p::CommandReceipt) -> &p::ArtifactResult {
        let Some(p::command_receipt::Result::Artifact(result)) = &receipt.result else {
            panic!("missing artifact result")
        };
        result
    }
    #[tokio::test]
    async fn private_image_boundaries_retain_exact_results_and_authorize_before_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let (rpc, grant, chat) = fixture(dir.path()).await;
        let record = rpc
            .auth
            .client()
            .load_runtime_chats()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.id() == chat)
            .unwrap();
        let target = p::ThreadTarget {
            thread: Some(p::ThreadRef {
                host_id: rpc.auth.host_id().into(),
                thread_id: record.thread_id().unwrap().to_string(),
            }),
            executor_epoch: record.thread().unwrap().executor_epoch(),
        };
        let content = UploadContent {
            size: 3,
            sha256: Sha256::digest(b"abc").into(),
            media_type: "image/png".into(),
        };
        let begin = C::Begin {
            destination: D::Image,
            content: content.clone(),
        };
        let begin_meta = meta();
        let receipt = rpc
            .api_command(
                grant.clone(),
                Some(begin_meta.clone()),
                image(Some(target.clone())).unwrap(),
                begin.clone(),
            )
            .await
            .unwrap();
        assert_eq!(receipt.state, p::OperationState::Succeeded as i32);
        let upload = result(&receipt).upload_id.clone();
        let finish = C::Finish {
            destination: D::Image,
            upload: upload.clone(),
            origin: UploadOrigin {
                command: begin_meta.command_id.clone(),
                epoch: begin_meta.command_epoch,
            },
        };
        let incomplete_meta = meta();
        let incomplete = rpc
            .api_command(
                grant.clone(),
                Some(incomplete_meta.clone()),
                image(Some(target.clone())).unwrap(),
                finish.clone(),
            )
            .await
            .unwrap();
        assert_eq!(incomplete.state, p::OperationState::Rejected as i32);
        assert_eq!(
            incomplete.error.as_ref().unwrap().code,
            "conditions_changed"
        );
        let store = rpc.store.clone();
        let owner = grant.id().to_owned();
        let id = upload.clone();
        blocking(move || store.put_chunk(&owner, &id, 0, b"abc"))
            .await
            .unwrap();
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(incomplete_meta),
                image(Some(target.clone())).unwrap(),
                finish.clone()
            )
            .await
            .unwrap(),
            incomplete
        );
        let finish_meta = meta();
        let finished = rpc
            .api_command(
                grant.clone(),
                Some(finish_meta.clone()),
                image(Some(target.clone())).unwrap(),
                finish.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            result(&finished).artifact.as_ref().unwrap().sha256,
            content.sha256
        );
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(finish_meta),
                image(Some(target.clone())).unwrap(),
                finish
            )
            .await
            .unwrap(),
            finished
        );
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(begin_meta.clone()),
                image(Some(target.clone())).unwrap(),
                begin
            )
            .await
            .unwrap(),
            receipt
        );
        let second = rpc
            .api_command(
                grant.clone(),
                Some(meta()),
                image(Some(target.clone())).unwrap(),
                C::Begin {
                    destination: D::Image,
                    content: content.clone(),
                },
            )
            .await
            .unwrap();
        let cancel = C::Cancel {
            destination: D::Image,
            upload: result(&second).upload_id.clone(),
            origin: UploadOrigin {
                command: second.command_id.clone(),
                epoch: second.command_epoch,
            },
        };
        let cancel_meta = meta();
        let cancelled = rpc
            .api_command(
                grant.clone(),
                Some(cancel_meta.clone()),
                image(Some(target.clone())).unwrap(),
                cancel.clone(),
            )
            .await
            .unwrap();
        assert_eq!(cancelled.state, p::OperationState::Succeeded as i32);
        assert!(
            rpc.status(grant.id(), &result(&second).upload_id)
                .await
                .is_err()
        );
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(cancel_meta.clone()),
                image(Some(target.clone())).unwrap(),
                cancel.clone()
            )
            .await
            .unwrap(),
            cancelled
        );
        let changed = C::Cancel {
            destination: D::Image,
            upload,
            origin: UploadOrigin {
                command: begin_meta.command_id.clone(),
                epoch: begin_meta.command_epoch,
            },
        };
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(cancel_meta.clone()),
                image(Some(target.clone())).unwrap(),
                changed.clone()
            )
            .await
            .unwrap_err()
            .code(),
            tonic::Code::Aborted
        );
        rpc.auth
            .client()
            .update_runtime_device_authorization(
                grant.id().into(),
                grant.authorization_revision(),
                crate::host::READ | crate::host::TASKS,
                vec!["private".into()],
            )
            .await
            .unwrap();
        for command in [cancel, changed] {
            assert_eq!(
                rpc.api_command(
                    grant.clone(),
                    Some(cancel_meta.clone()),
                    image(Some(target.clone())).unwrap(),
                    command
                )
                .await
                .unwrap_err()
                .code(),
                tonic::Code::PermissionDenied
            );
        }
    }
    #[tokio::test]
    async fn file_completion_uses_host_owned_begin_authority_after_project_removal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let (rpc, grant, _) = fixture(dir.path()).await;
        let grant = rpc
            .auth
            .client()
            .update_runtime_device_authorization(
                grant.id().into(),
                grant.authorization_revision(),
                crate::host::READ | crate::host::FILES_GIT,
                vec![root.to_str().unwrap().into()],
            )
            .await
            .unwrap();
        let mut projects = crate::state::projects::Projects::default();
        projects.open(root.clone());
        let project = projects.current().unwrap();
        let target = p::ResourceTarget {
            resource: Some(p::resource_target::Resource::Project(p::ProjectRef {
                host_id: rpc.auth.host_id().into(),
                project_id: serde_json::to_string(&project.id).unwrap(),
                worktree_id: None,
            })),
        };
        rpc.roots.replace_projects(projects.items).unwrap();
        let begin_meta = meta();
        let removed_meta = meta();
        let begin_command = C::Begin {
            destination: D::File {
                path: "file.txt".into(),
            },
            content: UploadContent {
                size: 0,
                sha256: Sha256::digest(b"").into(),
                media_type: "text/plain".into(),
            },
        };
        // Both commands were accepted before an interruption. Only the first
        // still has a valid live target when its original RPC is retried.
        for meta in [&begin_meta, &removed_meta] {
            let prepared = api::resources::prepare(
                &rpc.auth,
                &rpc.roots,
                &grant,
                meta.clone(),
                &target,
                Command::Artifact(begin_command.clone()),
            )
            .await
            .unwrap();
            rpc.auth
                .client()
                .accept_runtime_command(prepared.runtime_input().clone(), crate::host::FILES_GIT)
                .await
                .unwrap();
        }
        let receipt = rpc
            .api_command(
                grant.clone(),
                Some(begin_meta.clone()),
                Address::Resource(target.clone()),
                begin_command.clone(),
            )
            .await
            .unwrap();
        let upload = result(&receipt).upload_id.clone();
        rpc.roots.replace_projects(Vec::new()).unwrap();
        let removed = rpc
            .api_command(
                grant.clone(),
                Some(removed_meta.clone()),
                Address::Resource(target.clone()),
                begin_command.clone(),
            )
            .await
            .unwrap();
        assert_eq!(removed.state, p::OperationState::Rejected as i32);
        assert_eq!(removed.error.as_ref().unwrap().code, "conditions_changed");
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(removed_meta),
                Address::Resource(target.clone()),
                begin_command,
            )
            .await
            .unwrap(),
            removed
        );
        let finish = C::Finish {
            destination: D::File {
                path: "file.txt".into(),
            },
            upload,
            origin: UploadOrigin {
                command: begin_meta.command_id.clone(),
                epoch: begin_meta.command_epoch,
            },
        };
        let mut unproven = finish.clone();
        if let C::Finish { origin, .. } = &mut unproven {
            origin.command = uuid::Uuid::new_v4().to_string();
        }
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(meta()),
                Address::Resource(target.clone()),
                unproven
            )
            .await
            .unwrap_err()
            .code(),
            tonic::Code::FailedPrecondition
        );
        let finish_meta = meta();
        let prepared = CommandBus::new(rpc.auth.client().clone())
            .prepare_project_resource(
                api::decode::envelope(
                    finish_meta.clone(),
                    api::resources::resource(&target).unwrap(),
                    Command::Artifact(finish.clone()),
                )
                .unwrap(),
                Principal::authenticated(&grant, finish_meta.command_epoch).unwrap(),
                Some(root.to_str().unwrap().into()),
            )
            .await
            .unwrap()
            .unwrap();
        rpc.auth
            .client()
            .accept_runtime_command(prepared.runtime_input().clone(), crate::host::FILES_GIT)
            .await
            .unwrap();
        let finished = rpc
            .api_command(
                grant.clone(),
                Some(finish_meta.clone()),
                Address::Resource(target.clone()),
                finish.clone(),
            )
            .await
            .unwrap();
        assert_eq!(finished.state, p::OperationState::Succeeded as i32);
        assert!(result(&finished).artifact.is_some());
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(finish_meta.clone()),
                Address::Resource(target.clone()),
                finish.clone()
            )
            .await
            .unwrap(),
            finished
        );
        let mut changed = finish.clone();
        if let C::Finish { destination, .. } = &mut changed {
            *destination = D::File {
                path: "other.txt".into(),
            };
        }
        assert_eq!(
            rpc.api_command(
                grant.clone(),
                Some(finish_meta.clone()),
                Address::Resource(target.clone()),
                changed.clone()
            )
            .await
            .unwrap_err()
            .code(),
            tonic::Code::Aborted
        );
        rpc.auth
            .client()
            .update_runtime_device_authorization(
                grant.id().into(),
                grant.authorization_revision(),
                crate::host::READ | crate::host::FILES_GIT,
                vec!["private".into()],
            )
            .await
            .unwrap();
        for command in [finish, changed] {
            assert_eq!(
                rpc.api_command(
                    grant.clone(),
                    Some(finish_meta.clone()),
                    Address::Resource(target.clone()),
                    command
                )
                .await
                .unwrap_err()
                .code(),
                tonic::Code::PermissionDenied
            );
        }
    }
    #[tokio::test]
    async fn task_binding_requires_tasks_scope_and_global_root() {
        let dir = tempfile::tempdir().unwrap();
        let (rpc, grant, _) = fixture(dir.path()).await;
        let address = || {
            Address::Environment(p::EnvironmentRef {
                host_id: rpc.auth.host_id().into(),
            })
        };
        let command = C::Begin {
            destination: D::Task {
                integration: "account".into(),
                project: "PROJ".into(),
                task: "ISSUE-1".into(),
            },
            content: UploadContent {
                size: 0,
                sha256: Sha256::digest(b"").into(),
                media_type: "text/plain".into(),
            },
        };
        let command_meta = meta();
        let receipt = rpc
            .api_command(
                grant.clone(),
                Some(command_meta.clone()),
                address(),
                command.clone(),
            )
            .await
            .unwrap();
        let binding = rpc
            .status(grant.id(), &result(&receipt).upload_id)
            .await
            .unwrap();
        assert!(
            matches!(binding.binding().target(), Target::Task { task_id, root, .. } if task_id == "ISSUE-1" && root == "private")
        );
        let grant = rpc
            .auth
            .client()
            .update_runtime_device_authorization(
                grant.id().into(),
                grant.authorization_revision(),
                crate::host::READ | crate::host::CHAT,
                vec!["private".into()],
            )
            .await
            .unwrap();
        assert_eq!(
            rpc.api_command(grant.clone(), Some(command_meta), address(), command)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::PermissionDenied
        );
        assert_eq!(
            rpc.status(grant.id(), &result(&receipt).upload_id)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::PermissionDenied
        );
    }
}
