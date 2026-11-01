use super::{HostClient, rpc_error};
use crate::api::proto as p;
use prost::Message;
use sha2::{Digest, Sha256};
impl HostClient {
    pub async fn git_stage(
        &self,
        request: crate::api::proto::StageRequest,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::GitStage(request),
            String::new(),
            None,
        )
        .await
    }
    pub async fn git_unstage(
        &self,
        request: crate::api::proto::UnstageRequest,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::GitUnstage(request),
            String::new(),
            None,
        )
        .await
    }
    pub async fn git_discard(
        &self,
        request: crate::api::proto::DiscardRequest,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::GitDiscard(request),
            String::new(),
            None,
        )
        .await
    }
    pub async fn git_commit(
        &self,
        request: crate::api::proto::CommitRequest,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::GitCommit(request),
            String::new(),
            None,
        )
        .await
    }
    pub async fn git_pull(
        &self,
        request: crate::api::proto::PullRequest,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::GitPull(request),
            String::new(),
            None,
        )
        .await
    }
    pub async fn git_push(
        &self,
        request: crate::api::proto::PushRequest,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::GitPush(request),
            String::new(),
            None,
        )
        .await
    }
    pub async fn git_worktree(
        &self,
        request: crate::api::proto::ChangeWorktreeRequest,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::GitWorktree(request),
            String::new(),
            None,
        )
        .await
    }
    pub async fn download(&self, artifact: p::ArtifactRef) -> Result<Vec<u8>, String> {
        let artifact = crate::api::proto::ArtifactRef {
            host_id: artifact.host_id,
            artifact_id: artifact.artifact_id,
            media_type: artifact.media_type,
            size: artifact.size,
            sha256: artifact.sha256,
        };
        if artifact.host_id != self.id().as_str()
            || artifact.size > 32 * 1024 * 1024
            || artifact.sha256.len() != 32
        {
            return Err("Artifact identity or size is invalid.".into());
        }
        let mut stream = crate::api::proto::artifacts_service_client::ArtifactsServiceClient::new(
            self.channel()?,
        )
        .download(crate::api::proto::DownloadRequest {
            artifact: Some(artifact.clone()),
            offset: 0,
        })
        .await
        .map_err(rpc_error)?
        .into_inner();
        let mut bytes = Vec::new();
        let mut complete = false;
        while let Some(chunk) = stream.message().await.map_err(rpc_error)? {
            if complete
                || chunk.artifact.as_ref() != Some(&artifact)
                || chunk.offset != bytes.len() as u64
                || bytes.len() as u64 + chunk.data.len() as u64 > artifact.size
            {
                return Err("Artifact download identity or offset mismatch.".into());
            }
            bytes.extend(chunk.data);
            complete = chunk.complete;
        }
        if !complete
            || bytes.len() as u64 != artifact.size
            || Sha256::digest(&bytes).as_slice() != artifact.sha256
        {
            return Err("Artifact download is incomplete or corrupt.".into());
        }
        Ok(bytes)
    }

    pub async fn read_chat_page(
        &self,
        chat: p::ThreadRef,
        item: p::ItemKey,
    ) -> Result<Vec<u8>, String> {
        if chat.host_id != self.id().as_str() {
            return Err("Page belongs to another host.".into());
        }
        let thread = Some(chat);
        let mut stream =
            crate::api::proto::catalog_service_client::CatalogServiceClient::new(self.channel()?)
                .read_thread_page(crate::api::proto::ReadThreadPageRequest {
                    thread,
                    item: Some(item),
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
        let mut artifact = None;
        let mut bytes = Vec::new();
        let mut complete = false;
        while let Some(chunk) = stream.message().await.map_err(rpc_error)? {
            let current = chunk.artifact.as_ref().ok_or("Missing page artifact.")?;
            if current.host_id != self.id().as_str()
                || current.media_type != "text/html"
                || current.size > 32 * 1024 * 1024
                || current.sha256.len() != 32
                || artifact
                    .as_ref()
                    .is_some_and(|previous| previous != current)
                || complete
                || chunk.offset != bytes.len() as u64
                || chunk.data.len() > 64 * 1024
                || (chunk.data.is_empty() && !chunk.complete)
                || bytes.len() as u64 + chunk.data.len() as u64 > current.size
            {
                return Err("Page artifact identity, size or progress is invalid.".into());
            }
            artifact = Some(current.clone());
            bytes.extend(chunk.data);
            complete = chunk.complete;
        }
        let artifact = artifact.ok_or("Missing page artifact.")?;
        if !complete
            || bytes.len() as u64 != artifact.size
            || Sha256::digest(&bytes).as_slice() != artifact.sha256
        {
            return Err("Page artifact is incomplete or corrupt.".into());
        }
        Ok(bytes)
    }

    pub async fn read_file(&self, file: p::FileRef) -> Result<(Vec<u8>, Vec<u8>), String> {
        let mut stream =
            crate::api::proto::files_service_client::FilesServiceClient::new(self.channel()?)
                .read_file(crate::api::proto::ReadFileRequest {
                    file: Some(file.clone()),
                    offset: 0,
                    max_bytes: 32 * 1024 * 1024,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
        let mut bytes = vec![];
        let mut hash = None;
        let mut complete = false;
        while let Some(chunk) = stream.message().await.map_err(rpc_error)? {
            if chunk.file.as_ref() != Some(&file)
                || chunk.offset as usize != bytes.len()
                || complete
                || bytes.len() + chunk.data.len() > 32 * 1024 * 1024
                || hash.as_ref().is_some_and(|h| h != &chunk.base_hash)
            {
                return Err("File stream changed or exceeded its limit.".into());
            }
            hash = Some(chunk.base_hash);
            bytes.extend(chunk.data);
            complete = chunk.complete;
        }
        if !complete {
            return Err("File download is incomplete.".into());
        }
        Ok((bytes, hash.unwrap_or_default()))
    }
    pub async fn save_file(
        &self,
        file: p::FileRef,
        expected_hash: Vec<u8>,
        content: p::ArtifactRef,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::SaveFile(crate::api::proto::SaveFileRequest {
                command: None,
                file: Some(file),
                expected_hash,
                content: Some(crate::api::proto::ArtifactRef {
                    host_id: content.host_id,
                    artifact_id: content.artifact_id,
                    media_type: content.media_type,
                    size: content.size,
                    sha256: content.sha256,
                }),
            }),
            String::new(),
            None,
        )
        .await
    }
    pub async fn create_file(
        &self,
        file: p::FileRef,
        directory: bool,
        content: Option<p::ArtifactRef>,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::CreateFile(crate::api::proto::CreateFileRequest {
                command: None,
                file: Some(file),
                directory,
                content: content.map(|content| crate::api::proto::ArtifactRef {
                    host_id: content.host_id,
                    artifact_id: content.artifact_id,
                    media_type: content.media_type,
                    size: content.size,
                    sha256: content.sha256,
                }),
            }),
            String::new(),
            None,
        )
        .await
    }
    pub async fn set_file_presence(
        &self,
        file: p::FileRef,
        base_hash: Vec<u8>,
        dirty: bool,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            super::api_commands::ApiIntent::FilePresence(
                crate::api::proto::SetDirtyPresenceRequest {
                    command: None,
                    file: Some(file),
                    base_hash,
                    dirty,
                },
            ),
            String::new(),
            None,
        )
        .await
    }
    pub async fn list_directory(
        &self,
        directory: p::FileRef,
        page_token: Option<String>,
    ) -> Result<p::DirectoryEntries, String> {
        let response =
            crate::api::proto::files_service_client::FilesServiceClient::new(self.channel()?)
                .list_directory(crate::api::proto::ListDirectoryRequest {
                    directory: Some(directory),
                    page_token,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
        Ok(p::DirectoryEntries {
            target: response.target,
            entries: response
                .entries
                .into_iter()
                .map(|e| p::DirectoryEntry {
                    relative_path: e.relative_path,
                    name: e.name,
                    directory: e.directory,
                    size: e.size,
                })
                .collect(),
            next_page_token: response.next_page_token,
            warning: response.warning,
            incomplete: response.incomplete,
        })
    }
    pub async fn search_files(
        &self,
        target: p::ResourceTarget,
        query: String,
        limit: u32,
    ) -> Result<p::DirectoryEntries, String> {
        let response =
            crate::api::proto::files_service_client::FilesServiceClient::new(self.channel()?)
                .search_files(crate::api::proto::SearchFilesRequest {
                    target: Some(target),
                    query,
                    limit,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
        Ok(p::DirectoryEntries {
            target: response.target,
            entries: response
                .entries
                .into_iter()
                .map(|e| p::DirectoryEntry {
                    relative_path: e.relative_path,
                    name: e.name,
                    directory: e.directory,
                    size: e.size,
                })
                .collect(),
            next_page_token: response.next_page_token,
            warning: response.warning,
            incomplete: response.incomplete,
        })
    }
    pub fn prepare_attachment(
        &self,
        target: super::UploadTarget,
        purpose: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<String, String> {
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Attachment exceeds 32 MiB.".into());
        }
        let content = crate::command_bus::UploadContent {
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
            media_type: media_type.into(),
        };
        let begin =
            super::uploads::UploadDraft::capture(self.id().as_str(), target, purpose, content)?;
        self.retain_attachment(begin, bytes)
    }
    pub fn prepare_task_attachment(
        &self,
        task: crate::api::proto::TaskRef,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<String, String> {
        if bytes.len() > 25 * 1024 * 1024 {
            return Err("Attachment exceeds 25 MiB.".into());
        }
        let content = crate::command_bus::UploadContent {
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
            media_type: media_type.into(),
        };
        let begin = super::uploads::UploadDraft::task(self.id().as_str(), task, content)?;
        self.retain_attachment(begin, bytes)
    }
    fn retain_attachment(
        &self,
        begin: super::uploads::UploadDraft,
        bytes: &[u8],
    ) -> Result<String, String> {
        use std::io::Write;
        let record = self.snapshot().record;
        if record.uploads.len() >= 100 {
            return Err("Local attachment limit reached. Remove unused attachments first.".into());
        }
        let total: u64 = record
            .uploads
            .iter()
            .map(|u| u.draft().map(|b| b.content.size))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum();
        if total + begin.content.size > 100 * 1024 * 1024 {
            return Err("Local attachment storage exceeds 100 MiB.".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let directory = self.store.lock().unwrap().attachment_dir();
        crate::platform::directories::ensure_private_dir(&directory)?;
        let mut file = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        let path = directory.join(&id);
        file.persist(&path).map_err(|e| e.to_string())?;
        if let Err(error) = self.update_record(|r| {
            r.uploads.push(super::store::UploadRecord {
                api_begin: None,
                api_finish: None,
                api_cancel: None,
                begin_result: Default::default(),
                finish_result: Default::default(),
                cancel_result: Default::default(),
                bulk_unavailable: false,
                id: id.clone(),
                begin: begin.encode()?,
                finish: None,
                upload_id: None,
                artifact: None,
            });
            Ok(())
        }) {
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        Ok(id)
    }
    pub async fn upload(
        &self,
        target: super::UploadTarget,
        purpose: &str,
        media_type: &str,
        bytes: Vec<u8>,
    ) -> Result<p::ArtifactRef, String> {
        let id = self.prepare_attachment(target, purpose, media_type, &bytes)?;
        self.resume_upload(&id).await
    }
    pub async fn resume_upload(&self, id: &str) -> Result<p::ArtifactRef, String> {
        let _gate = self.mutation.lock().await;
        self.resume_upload_locked(id).await
    }
    pub fn upload_journal(&self) -> Vec<String> {
        self.snapshot()
            .record
            .uploads
            .into_iter()
            .map(|u| u.id)
            .collect()
    }
    pub fn remove_local_attachment(&self, id: &str) -> Result<(), String> {
        let record = self.snapshot().record;
        if record
            .outbox
            .iter()
            .any(|e| e.uploads.iter().any(|v| v == id))
        {
            return Err("Attachment belongs to an outbox message.".into());
        }
        if record
            .views
            .iter()
            .filter(|(key, _)| key.starts_with("chat:"))
            .any(|(_, value)| {
                serde_json::from_str::<(String, Option<Vec<u8>>, Vec<String>)>(value)
                    .is_ok_and(|(_, _, images)| images.iter().any(|image| image == id))
            })
        {
            return Err("Attachment belongs to a saved chat draft.".into());
        }
        for (key, value) in &record.views {
            if key.starts_with("edit:") {
                let (_, _, images, _): (Vec<u8>, Vec<u8>, Vec<String>, Option<Vec<u8>>) =
                    serde_json::from_str(value)
                        .map_err(|_| "Saved edit recovery is unreadable; attachments retained.")?;
                if images.iter().any(|image| image == id) {
                    return Err("Attachment belongs to the draft preserved before editing.".into());
                }
            }
        }
        let upload = record
            .uploads
            .iter()
            .find(|u| u.id == id)
            .ok_or("Attachment not found.")?;
        if upload.unresolved() {
            return Err("Unresolved upload must be reconciled before cleanup.".into());
        }
        let path = self.store.lock().unwrap().attachment_dir().join(id);
        self.update_record(|r| {
            // Upload admission may have persisted an intent since the snapshot.
            // Make the final protection check under the journal's write lock.
            if r.uploads
                .iter()
                .find(|u| u.id == id)
                .is_some_and(|u| u.unresolved())
            {
                return Err("Unresolved upload must be reconciled before cleanup.".into());
            }
            r.uploads.retain(|u| u.id != id);
            Ok(())
        })?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Attachment reference removed; file cleanup failed.".into()),
        }
    }
    pub async fn history_page(
        &self,
        chat: p::ThreadRef,
        token: Option<String>,
        around: Option<p::ItemKey>,
        before: Option<p::ItemKey>,
    ) -> Result<(Vec<p::TranscriptItem>, Option<String>), String> {
        let mut stream = self
            .read_thread_history(p::ReadHistoryPageRequest {
                thread: Some(chat.clone()),
                page_token: token,
                limit: 100,
                around,
                before,
            })
            .await?;
        let boundary = p::Cursor {
            runtime_epoch: "history".into(),
            stream_id: "page".into(),
            sequence: 0,
        };
        let mut projection = super::ChatProjection::default();
        let mut start: Option<p::HistoryPageStart> = None;
        let mut ended = false;
        while let Some(part) = stream.message().await.map_err(rpc_error)? {
            if part.thread.as_ref() != Some(&chat) || ended {
                return Err("History page identity mismatch.".into());
            }
            let event = match part.part.ok_or("Empty history frame.")? {
                p::history_page_event::Part::Start(value) if start.is_none() => {
                    start = Some(value);
                    p::thread_event::Event::SnapshotStart(p::SnapshotStart {
                        boundary: Some(boundary.clone()),
                        ..Default::default()
                    })
                }
                p::history_page_event::Part::Items(items) if start.is_some() => {
                    p::thread_event::Event::SnapshotItems(items)
                }
                p::history_page_event::Part::Fragment(fragment) if start.is_some() => {
                    p::thread_event::Event::ItemFragment(fragment)
                }
                p::history_page_event::Part::EndReadId(id)
                    if start.as_ref().is_some_and(|s| s.read_id == id) =>
                {
                    ended = true;
                    p::thread_event::Event::SnapshotEnd(p::SnapshotEnd {
                        boundary: Some(boundary.clone()),
                        has_earlier_history: false,
                    })
                }
                _ => return Err("History page boundary mismatch.".into()),
            };
            projection.apply(
                &chat,
                p::ThreadEvent {
                    thread: Some(chat.clone()),
                    cursor: None,
                    event: Some(event),
                },
            )?;
        }
        if !ended {
            return Err("History page did not complete.".into());
        }
        Ok((projection.items, start.and_then(|s| s.previous_page_token)))
    }
    pub fn recovery(&self) -> Vec<(String, Option<p::CommandReceipt>)> {
        self.snapshot()
            .record
            .mutations
            .into_iter()
            .map(|m| {
                (
                    m.id,
                    m.receipt
                        .and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok()),
                )
            })
            .collect()
    }
}
