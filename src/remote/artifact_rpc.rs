//! Bounded native upload/download transport backed by the durable private store.
use super::{
    auth::{RpcAuth, WorkspaceRoots, storage_status},
    proto,
    resources::ResourceLocks,
};
use crate::{
    artifacts::{ArtifactStore, BeginUpload, CHUNK_BYTES, Purpose, Target, Upload},
    host::{CommandState, DeviceGrant},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{pin::Pin, sync::Arc};
use tonic::{Request, Response, Status};
#[derive(Clone)]
pub struct ArtifactsRpc {
    auth: RpcAuth,
    roots: WorkspaceRoots,
    locks: ResourceLocks,
    store: Arc<ArtifactStore>,
    gate: Arc<tokio::sync::Mutex<()>>,
}
fn required<T>(v: Option<T>) -> Result<T, Status> {
    v.ok_or_else(|| Status::invalid_argument("Required artifact field is missing."))
}
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> crate::settings::Result<T> + Send + 'static,
) -> Result<T, Status> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| Status::unavailable("Artifact worker stopped."))?
        .map_err(storage_status)
}
impl ArtifactsRpc {
    pub fn new(
        auth: RpcAuth,
        roots: WorkspaceRoots,
        locks: ResourceLocks,
        store: Arc<ArtifactStore>,
    ) -> Self {
        Self {
            auth,
            roots,
            locks,
            store,
            gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    async fn target(
        &self,
        g: &DeviceGrant,
        target: Option<proto::ResourceRef>,
    ) -> Result<(Target, String, u32), Status> {
        use proto::resource_ref::Resource;
        let (target, root, scope) = match required(required(target)?.resource)? {
            Resource::Chat(c) => {
                self.auth.check_host(&c.host_id)?;
                let chat = self
                    .auth
                    .client()
                    .load_runtime_chats()
                    .await
                    .map_err(storage_status)?
                    .into_iter()
                    .find(|v| v.id() == c.chat_id)
                    .ok_or_else(|| Status::not_found("Chat is unavailable."))?;
                (
                    Target::Chat(c.chat_id),
                    chat.authority_root().into(),
                    crate::host::CHAT,
                )
            }
            Resource::File(f) => {
                let root = self.roots.resolve(&required(f.workspace)?, &self.auth)?;
                let root = WorkspaceRoots::root_string(&root)?.to_owned();
                (
                    Target::File {
                        root: root.clone(),
                        path: f.relative_path,
                    },
                    root,
                    crate::host::FILES_GIT,
                )
            }
            Resource::Task(t) => {
                self.auth.check_host(&t.host_id)?;
                let root = if let Some(w) = t.workspace {
                    WorkspaceRoots::root_string(&self.roots.resolve(&w, &self.auth)?)?.to_owned()
                } else {
                    "private".into()
                };
                (
                    Target::Task {
                        integration_id: t.integration_id,
                        project_id: t.project_id,
                        task_id: t.task_id,
                        root: root.clone(),
                    },
                    root,
                    crate::host::TASKS,
                )
            }
            _ => {
                return Err(Status::invalid_argument(
                    "Upload target must be a chat, file, or typed task.",
                ));
            }
        };
        self.auth.resource(g.id(), scope, &root).await?;
        Ok((target, root, scope))
    }
    fn wire_status(&self, u: &Upload) -> proto::UploadStatus {
        let b = u.binding();
        proto::UploadStatus {
            host_id: self.auth.host_id().into(),
            upload_id: u.id().into(),
            received: u
                .ranges()
                .iter()
                .map(|&(a, b)| proto::ByteRange {
                    offset: a,
                    length: b - a,
                })
                .collect(),
            artifact: u.artifact_id().map(|id| proto::ArtifactRef {
                host_id: self.auth.host_id().into(),
                artifact_id: id.into(),
                media_type: b.media_type().into(),
                size: b.size(),
                sha256: b.sha256().to_vec(),
            }),
        }
    }
    async fn status(&self, device: &str, id: &str) -> Result<Upload, Status> {
        self.auth
            .client()
            .artifact_upload_status(device.to_owned(), id.to_owned())
            .await
            .map_err(storage_status)
    }
    async fn scope(&self, g: &DeviceGrant, u: &Upload) -> Result<(String, u32), Status> {
        let (root, scope) = match u.binding().target() {
            Target::Chat(id) => {
                let chat = self
                    .auth
                    .client()
                    .load_runtime_chats()
                    .await
                    .map_err(storage_status)?
                    .into_iter()
                    .find(|v| v.id() == id)
                    .ok_or_else(|| Status::not_found("Chat is unavailable."))?;
                (chat.authority_root().to_owned(), crate::host::CHAT)
            }
            Target::File { root, .. } => (root.clone(), crate::host::FILES_GIT),
            Target::Task { root, .. } => (root.clone(), crate::host::TASKS),
        };
        self.auth.resource(g.id(), scope, &root).await?;
        Ok((root, scope))
    }
    async fn begin(
        &self,
        g: DeviceGrant,
        r: proto::BeginUploadRequest,
    ) -> Result<proto::UploadStatus, Status> {
        let meta = required(r.command)?;
        if meta.expected_revision.is_some() {
            return Err(Status::invalid_argument(
                "Artifact commands do not take a revision.",
            ));
        }
        let key = self.auth.key(&meta, &g)?;
        let (target, root, scope) = self.target(&g, r.target).await?;
        let purpose = match r.purpose.as_str() {
            "image" => Purpose::Image,
            "file_save" => Purpose::FileSave,
            "task_attachment" => Purpose::TaskAttachment,
            _ => return Err(Status::invalid_argument("Unsupported upload purpose.")),
        };
        if !matches!(
            (&target, &purpose),
            (Target::Chat(_), Purpose::Image)
                | (Target::File { .. }, Purpose::FileSave)
                | (Target::Task { .. }, Purpose::TaskAttachment)
        ) {
            return Err(Status::invalid_argument(
                "Upload purpose does not match its target.",
            ));
        }
        let binding = BeginUpload::new(
            g.id().into(),
            target,
            purpose,
            r.size,
            r.sha256
                .try_into()
                .map_err(|_| Status::invalid_argument("SHA256 must contain 32 bytes."))?,
            r.media_type,
        )
        .map_err(storage_status)?;
        let identity =
            serde_json::to_vec(&(self.auth.host_id(), key.device(), key.epoch(), key.id()))
                .map_err(|_| Status::internal("Invalid command identity."))?;
        let hash = Sha256::digest(identity);
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash[..16]);
        let id = uuid::Uuid::from_bytes(bytes).to_string();
        let args = serde_json::to_vec(&binding)
            .map_err(|_| Status::invalid_argument("Invalid upload binding."))?;
        let input = self
            .auth
            .input(&meta, &g, "artifacts.begin", id.clone(), args, root)?;
        let operation = self.locks.begin()?;
        let this = self.clone();
        tokio::spawn(async move {
            let _operation = operation;
            let _gate = this.gate.lock().await;
            let accepted = this
                .auth
                .client()
                .accept_runtime_command(input, scope)
                .await
                .map_err(storage_status)?;
            if accepted.state() != CommandState::Accepted {
                if accepted.state() != CommandState::Succeeded {
                    return Err(Status::failed_precondition(
                        "Upload command requires reconciliation; inspect its receipt.",
                    ));
                }
                return this.status(g.id(), &id).await.map(|u| this.wire_status(&u));
            }
            this.auth
                .client()
                .transition_runtime_command(
                    key.clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .map_err(storage_status)?;
            let store = this.store.clone();
            let upload_id = id.clone();
            let result = blocking(move || store.begin_with_id(upload_id, binding)).await;
            let (state, payload) =
                outcome(result.as_ref().map(|_| id.clone()).map_err(Clone::clone));
            this.auth
                .client()
                .transition_runtime_command(key, CommandState::Attempting, state, Some(payload))
                .await
                .map_err(storage_status)?;
            result.map(|u| this.wire_status(&u))
        })
        .await
        .map_err(|_| Status::unavailable("Upload outcome requires reconciliation."))?
    }
    async fn finish_or_cancel(
        &self,
        g: DeviceGrant,
        meta: proto::CommandMeta,
        id: String,
        cancel: bool,
    ) -> Result<proto::CommandReceipt, Status> {
        if meta.expected_revision.is_some() {
            return Err(Status::invalid_argument(
                "Artifact commands do not take a revision.",
            ));
        }
        let key = self.auth.key(&meta, &g)?;
        let method = if cancel {
            "artifacts.cancel"
        } else {
            "artifacts.finish"
        };
        let operation = self.locks.begin()?;
        let this = self.clone();
        tokio::spawn(async move {
            let _operation = operation;
            let _gate = this.gate.lock().await;
            // A successful Cancel removes the row. Stored authorization permits its
            // receipt to be retried without requiring the deleted upload again.
            if let Some(receipt) = this
                .auth
                .client()
                .get_runtime_command(key.clone(), 0)
                .await
                .map_err(storage_status)?
            {
                if receipt.method() != method || receipt.target() != id {
                    return Err(Status::aborted(
                        "Command identity was used for another intent.",
                    ));
                }
                if receipt.state() != CommandState::Accepted {
                    return Ok(this.auth.receipt(&receipt));
                }
            }
            let upload = this.status(g.id(), &id).await?;
            let (root, scope) = this.scope(&g, &upload).await?;
            let input =
                this.auth
                    .input(&meta, &g, method, id.clone(), id.as_bytes().to_vec(), root)?;
            let accepted = this
                .auth
                .client()
                .accept_runtime_command(input, scope)
                .await
                .map_err(storage_status)?;
            if accepted.state() != CommandState::Accepted {
                return Ok(this.auth.receipt(&accepted));
            }
            this.auth
                .client()
                .transition_runtime_command(
                    key.clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .map_err(storage_status)?;
            let store = this.store.clone();
            let owner = g.id().to_owned();
            let result = blocking(move || {
                if cancel {
                    store.cancel(&owner, &id).map(|_| String::from("{}"))
                } else {
                    store.finish(&owner, &id).map(|a| a.id().to_owned())
                }
            })
            .await;
            let (state, payload) = outcome(result);
            let receipt = this
                .auth
                .client()
                .transition_runtime_command(key, CommandState::Attempting, state, Some(payload))
                .await
                .map_err(storage_status)?;
            Ok(this.auth.receipt(&receipt))
        })
        .await
        .map_err(|_| Status::unavailable("Artifact outcome requires reconciliation."))?
    }
}
fn outcome(result: Result<String, Status>) -> (CommandState, Vec<u8>) {
    match result {
        Ok(s) => (CommandState::Succeeded, s.into_bytes()),
        Err(e) => {
            let uncertain = matches!(e.code(), tonic::Code::Unavailable | tonic::Code::Unknown);
            (
                if uncertain {
                    CommandState::DeliveryUnknown
                } else {
                    CommandState::Failed
                },
                proto::DomainError {
                    code: if uncertain {
                        "delivery_unknown"
                    } else {
                        "artifact_failed"
                    }
                    .into(),
                    message: e.message().chars().take(8192).collect(),
                    retryable: false,
                }
                .encode_to_vec(),
            )
        }
    }
}
#[tonic::async_trait]
impl proto::artifacts_service_server::ArtifactsService for ArtifactsRpc {
    type DownloadStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<proto::ArtifactChunk, Status>> + Send>>;
    async fn begin_upload(
        &self,
        r: Request<proto::BeginUploadRequest>,
    ) -> Result<Response<proto::UploadStatus>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.begin(g, r.into_inner()).await.map(Response::new)
    }
    async fn get_upload(
        &self,
        r: Request<proto::GetUploadRequest>,
    ) -> Result<Response<proto::UploadStatus>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.auth.check_host(&r.get_ref().host_id)?;
        let u = self.status(g.id(), &r.get_ref().upload_id).await?;
        Ok(Response::new(self.wire_status(&u)))
    }
    async fn put_chunk(
        &self,
        r: Request<proto::PutChunkRequest>,
    ) -> Result<Response<proto::UploadStatus>, Status> {
        let g = self.auth.authenticate(&r).await?;
        self.auth.check_host(&r.get_ref().host_id)?;
        let r = r.into_inner();
        if r.data.is_empty() || r.data.len() > CHUNK_BYTES {
            return Err(Status::resource_exhausted(
                "Artifact chunks must contain 1 to 65536 bytes.",
            ));
        }
        let operation = self.locks.begin()?;
        let this = self.clone();
        let u = tokio::spawn(async move {
            let _operation = operation;
            let store = this.store;
            blocking(move || store.put_chunk(g.id(), &r.upload_id, r.offset, &r.data)).await
        })
        .await
        .map_err(|_| Status::unavailable("Chunk acknowledgement was lost."))??;
        Ok(Response::new(self.wire_status(&u)))
    }
    async fn finish_upload(
        &self,
        r: Request<proto::FinishUploadRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.finish_or_cancel(g, required(r.command)?, r.upload_id, false)
            .await
            .map(Response::new)
    }
    async fn cancel_upload(
        &self,
        r: Request<proto::CancelUploadRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.finish_or_cancel(g, required(r.command)?, r.upload_id, true)
            .await
            .map(Response::new)
    }
    async fn download(
        &self,
        r: Request<proto::DownloadRequest>,
    ) -> Result<Response<Self::DownloadStream>, Status> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let reference = required(r.artifact)?;
        self.auth.check_host(&reference.host_id)?;
        let (a, _) = self
            .auth
            .client()
            .authorize_artifact_download(g.id().into(), reference.artifact_id.clone())
            .await
            .map_err(storage_status)?;
        if reference.size != a.size()
            || reference.sha256 != a.sha256()
            || reference.media_type != a.media_type()
            || r.offset > a.size()
        {
            return Err(Status::aborted(
                "Artifact metadata or offset does not match.",
            ));
        }
        let operation = self.locks.begin()?;
        let this = self.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _operation = operation;
            let mut offset = r.offset;
            loop {
                let store = this.store.clone();
                let owner = g.id().to_owned();
                let id = reference.artifact_id.clone();
                let result =
                    blocking(move || store.download(&owner, &id, offset, CHUNK_BYTES)).await;
                let data = match result {
                    Ok(data) => data,
                    Err(e) => {
                        let _ = tx.try_send(Err(e));
                        break;
                    }
                };
                let complete = offset + data.len() as u64 == reference.size;
                let next = offset + data.len() as u64;
                let chunk = proto::ArtifactChunk {
                    artifact: Some(reference.clone()),
                    offset,
                    data,
                    complete,
                };
                let send = tx.send(Ok(chunk));
                tokio::pin!(send);
                let deadline = tokio::time::sleep(std::time::Duration::from_secs(30));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {_=&mut deadline=>{return;},result=&mut send=>{if result.is_err(){return;}break;},_=tokio::time::sleep(std::time::Duration::from_secs(1))=>{
                        if this.auth.client().authorize_artifact_download(g.id().into(),reference.artifact_id.clone()).await.is_err(){return;}
                    }}
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
    use crate::{
        host::{ChatRecord, DeviceRegistration},
        settings::SettingsClient,
        state::workspace::PaneMetadata,
    };
    async fn fixture(path: &std::path::Path) -> (ArtifactsRpc, DeviceGrant, String) {
        let client = SettingsClient::create(path.join("db")).await.unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let chat = uuid::Uuid::new_v4().to_string();
        client
            .save_runtime_chat(
                ChatRecord::new(
                    chat.clone(),
                    None,
                    "claude".into(),
                    PaneMetadata {
                        kind: crate::state::workspace::PaneKind::Chat,
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
        let g = client.lookup_runtime_device(vec![4; 32]).await.unwrap();
        let store = Arc::new(ArtifactStore::open(client.clone(), path.join("bulk")).unwrap());
        (
            ArtifactsRpc::new(
                RpcAuth::new(host.id().into(), client).unwrap(),
                WorkspaceRoots::default(),
                ResourceLocks::default(),
                store,
            ),
            g,
            chat,
        )
    }
    fn meta(rpc: &ArtifactsRpc) -> proto::CommandMeta {
        proto::CommandMeta {
            host_id: rpc.auth.host_id().into(),
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 1,
            expected_revision: None,
        }
    }
    fn request(rpc: &ArtifactsRpc, chat: String) -> proto::BeginUploadRequest {
        proto::BeginUploadRequest {
            command: Some(meta(rpc)),
            target: Some(proto::ResourceRef {
                resource: Some(proto::resource_ref::Resource::Chat(proto::ChatRef {
                    host_id: rpc.auth.host_id().into(),
                    chat_id: chat,
                })),
            }),
            purpose: "image".into(),
            size: 3,
            sha256: Sha256::digest(b"abc").to_vec(),
            media_type: "image/png".into(),
        }
    }
    #[tokio::test]
    async fn begin_lost_ack_finish_and_cancel_retain_identity_and_current_authority() {
        let d = tempfile::tempdir().unwrap();
        let (rpc, g, chat) = fixture(d.path()).await;
        let r = request(&rpc, chat.clone());
        let upload = rpc.begin(g.clone(), r.clone()).await.unwrap();
        assert_eq!(
            rpc.begin(g.clone(), r.clone()).await.unwrap().upload_id,
            upload.upload_id
        );
        let mut changed = r.clone();
        changed.sha256 = Sha256::digest(b"xyz").to_vec();
        assert_eq!(
            rpc.begin(g.clone(), changed).await.unwrap_err().code(),
            tonic::Code::Aborted
        );
        let store = rpc.store.clone();
        let id = upload.upload_id.clone();
        let owner = g.id().to_owned();
        blocking(move || store.put_chunk(&owner, &id, 0, b"abc"))
            .await
            .unwrap();
        let finish = meta(&rpc);
        let done = rpc
            .finish_or_cancel(g.clone(), finish.clone(), upload.upload_id.clone(), false)
            .await
            .unwrap();
        assert_eq!(done.state, proto::OperationState::Succeeded as i32);
        assert_eq!(
            rpc.finish_or_cancel(g.clone(), finish, upload.upload_id.clone(), false)
                .await
                .unwrap(),
            done
        );
        assert_eq!(
            rpc.begin(g.clone(), r)
                .await
                .unwrap()
                .artifact
                .unwrap()
                .artifact_id,
            upload.upload_id
        );
        let second = rpc.begin(g.clone(), request(&rpc, chat)).await.unwrap();
        let cancel = meta(&rpc);
        let result = rpc
            .finish_or_cancel(g.clone(), cancel.clone(), second.upload_id.clone(), true)
            .await
            .unwrap();
        assert_eq!(result.state, proto::OperationState::Succeeded as i32);
        assert_eq!(
            rpc.finish_or_cancel(g.clone(), cancel.clone(), second.upload_id.clone(), true)
                .await
                .unwrap(),
            result
        );
        assert_eq!(
            rpc.finish_or_cancel(g.clone(), cancel.clone(), upload.upload_id, true)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::Aborted
        );
        rpc.auth
            .client()
            .revoke_runtime_device(g.id().into())
            .await
            .unwrap();
        assert_eq!(
            rpc.finish_or_cancel(g, cancel, second.upload_id, true)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::PermissionDenied
        );
    }
    #[tokio::test]
    async fn task_binding_requires_tasks_scope_and_global_root() {
        let d = tempfile::tempdir().unwrap();
        let (rpc, g, _) = fixture(d.path()).await;
        let target = Some(proto::ResourceRef {
            resource: Some(proto::resource_ref::Resource::Task(proto::TaskRef {
                host_id: rpc.auth.host_id().into(),
                integration_id: "account".into(),
                task_id: "ISSUE-1".into(),
                project_id: "PROJ".into(),
                workspace: None,
            })),
        });
        let r = proto::BeginUploadRequest {
            command: Some(meta(&rpc)),
            target,
            purpose: "task_attachment".into(),
            size: 0,
            sha256: Sha256::digest(b"").to_vec(),
            media_type: "text/plain".into(),
        };
        let u = rpc.begin(g.clone(), r.clone()).await.unwrap();
        let binding = rpc.status(g.id(), &u.upload_id).await.unwrap();
        assert!(
            matches!(binding.binding().target(),Target::Task{task_id,..} if task_id=="ISSUE-1")
        );
        let g = rpc
            .auth
            .client()
            .update_runtime_device_authorization(
                g.id().into(),
                g.authorization_revision(),
                crate::host::READ | crate::host::CHAT,
                vec!["private".into()],
            )
            .await
            .unwrap();
        assert_eq!(
            rpc.begin(g.clone(), r).await.unwrap_err().code(),
            tonic::Code::PermissionDenied
        );
        assert_eq!(
            rpc.status(g.id(), &u.upload_id).await.unwrap_err().code(),
            tonic::Code::PermissionDenied
        );
    }
}
