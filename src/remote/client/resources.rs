use super::{HostClient, rpc_error};
use crate::remote::proto as p;
use prost::Message;
use sha2::{Digest, Sha256};
impl HostClient {
    pub async fn download(&self, artifact: p::ArtifactRef) -> Result<Vec<u8>, String> {
        if artifact.host_id != self.id().as_str()
            || artifact.size > 32 * 1024 * 1024
            || artifact.sha256.len() != 32
        {
            return Err("Artifact identity or size is invalid.".into());
        }
        let mut stream = p::artifacts_service_client::ArtifactsServiceClient::new(self.channel()?)
            .download(p::DownloadRequest {
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
        chat: p::ChatRef,
        item: p::ItemKey,
    ) -> Result<Vec<u8>, String> {
        if chat.host_id != self.id().as_str() {
            return Err("Page belongs to another host.".into());
        }
        let mut stream = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .read_chat_page(p::ChatPageRequest {
                chat: Some(chat),
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
        let mut stream = p::files_service_client::FilesServiceClient::new(self.channel()?)
            .read_file(p::ReadFileRequest {
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
    pub fn prepare_attachment(
        &self,
        target: p::ResourceRef,
        purpose: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<String, String> {
        use std::io::Write;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Attachment exceeds 32 MiB.".into());
        }
        let begin = p::BeginUploadRequest {
            command: None,
            target: Some(target),
            purpose: purpose.into(),
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).to_vec(),
            media_type: media_type.into(),
        };
        let record = self.snapshot().record;
        if record.uploads.len() >= 100 {
            return Err("Local attachment limit reached. Remove unused attachments first.".into());
        }
        let total: u64 = record
            .uploads
            .iter()
            .filter_map(|u| p::BeginUploadRequest::decode(u.begin.as_slice()).ok())
            .map(|b| b.size)
            .sum();
        if total + begin.size > 100 * 1024 * 1024 {
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
                id: id.clone(),
                begin: begin.encode_to_vec(),
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
        target: p::ResourceRef,
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
    pub(super) async fn resume_upload_locked(&self, id: &str) -> Result<p::ArtifactRef, String> {
        let record = self
            .snapshot()
            .record
            .uploads
            .into_iter()
            .find(|r| r.id == id)
            .ok_or("Local attachment is unavailable.")?;
        if let Some(artifact) = record.artifact {
            return p::ArtifactRef::decode(artifact.as_slice())
                .map_err(|_| "Invalid saved attachment.".into());
        }
        let mut begin = p::BeginUploadRequest::decode(record.begin.as_slice())
            .map_err(|_| "Invalid upload journal.")?;
        if begin.command.is_none() {
            begin.command = Some(self.command(None)?);
            self.update_record(|r| {
                r.uploads
                    .iter_mut()
                    .find(|r| r.id == id)
                    .ok_or("Upload disappeared.")?
                    .begin = begin.encode_to_vec();
                Ok(())
            })?;
        }
        let mut client = p::artifacts_service_client::ArtifactsServiceClient::new(self.channel()?);
        let mut status = if let Some(upload_id) = record.upload_id {
            client
                .get_upload(p::GetUploadRequest {
                    host_id: self.id.0.clone(),
                    upload_id,
                })
                .await
                .map_err(rpc_error)?
                .into_inner()
        } else {
            client
                .begin_upload(begin.clone())
                .await
                .map_err(rpc_error)?
                .into_inner()
        };
        if status.host_id != self.id.0 {
            return Err("Upload belongs to another host.".into());
        }
        self.update_record(|r| {
            r.uploads
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or("Upload disappeared.")?
                .upload_id = Some(status.upload_id.clone());
            Ok(())
        })?;
        if status.artifact.is_none() {
            let path = self.store.lock().unwrap().attachment_dir().join(id);
            let bytes =
                std::fs::read(path).map_err(|_| "Local attachment source is unavailable.")?;
            if bytes.len() as u64 != begin.size || Sha256::digest(&bytes).as_slice() != begin.sha256
            {
                return Err("Local attachment changed; upload was not resumed.".into());
            }
            for (index, chunk) in bytes.chunks(65536).enumerate() {
                let offset = (index * 65536) as u64;
                let end = offset + chunk.len() as u64;
                if status
                    .received
                    .iter()
                    .any(|r| r.offset <= offset && r.offset + r.length >= end)
                {
                    continue;
                }
                status = client
                    .put_chunk(p::PutChunkRequest {
                        host_id: self.id.0.clone(),
                        upload_id: status.upload_id.clone(),
                        offset,
                        data: chunk.to_vec(),
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
            }
            let finish = if let Some(bytes) = record.finish {
                p::FinishUploadRequest::decode(bytes.as_slice())
                    .map_err(|_| "Invalid finish journal.")?
            } else {
                let request = p::FinishUploadRequest {
                    command: Some(self.command(None)?),
                    upload_id: status.upload_id.clone(),
                };
                self.update_record(|r| {
                    r.uploads
                        .iter_mut()
                        .find(|r| r.id == id)
                        .ok_or("Upload disappeared.")?
                        .finish = Some(request.encode_to_vec());
                    Ok(())
                })?;
                request
            };
            super::commands::check_receipt(
                client
                    .finish_upload(finish)
                    .await
                    .map_err(rpc_error)?
                    .into_inner(),
            )?;
            status = client
                .get_upload(p::GetUploadRequest {
                    host_id: self.id.0.clone(),
                    upload_id: status.upload_id,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
        }
        let artifact = status
            .artifact
            .ok_or("Upload is not complete; reconcile it before sending.")?;
        if artifact.host_id != self.id.0
            || artifact.sha256 != begin.sha256
            || artifact.size != begin.size
        {
            return Err("Uploaded attachment identity mismatch.".into());
        }
        self.update_record(|r| {
            r.uploads
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or("Upload disappeared.")?
                .artifact = Some(artifact.encode_to_vec());
            Ok(())
        })?;
        Ok(artifact)
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
        if p::BeginUploadRequest::decode(upload.begin.as_slice())
            .ok()
            .is_some_and(|b| b.command.is_some())
            && upload.artifact.is_none()
        {
            return Err("Unresolved upload must be reconciled before cleanup.".into());
        }
        let path = self.store.lock().unwrap().attachment_dir().join(id);
        self.update_record(|r| {
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
        chat: p::ChatRef,
        token: Option<String>,
        around: Option<p::ItemKey>,
        before: Option<p::ItemKey>,
    ) -> Result<(Vec<p::TranscriptItem>, Option<String>), String> {
        let mut stream = p::chat_service_client::ChatServiceClient::new(self.channel()?)
            .read_history_page(p::HistoryPageRequest {
                chat: Some(chat.clone()),
                page_token: token,
                limit: 100,
                around,
                before,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        let boundary = p::Cursor {
            runtime_epoch: "history".into(),
            stream_id: "page".into(),
            sequence: 0,
        };
        let mut projection = super::ChatProjection::default();
        let mut start: Option<p::HistoryPageStart> = None;
        let mut ended = false;
        while let Some(part) = stream.message().await.map_err(rpc_error)? {
            if part.chat.as_ref() != Some(&chat) || ended {
                return Err("History page identity mismatch.".into());
            }
            let event = match part.part.ok_or("Empty history frame.")? {
                p::history_page_event::Part::Start(value) if start.is_none() => {
                    start = Some(value);
                    p::chat_event::Event::SnapshotStart(p::SnapshotStart {
                        boundary: Some(boundary.clone()),
                        ..Default::default()
                    })
                }
                p::history_page_event::Part::Items(items) if start.is_some() => {
                    p::chat_event::Event::SnapshotItems(items)
                }
                p::history_page_event::Part::Fragment(fragment) if start.is_some() => {
                    p::chat_event::Event::ItemFragment(fragment)
                }
                p::history_page_event::Part::EndReadId(id)
                    if start.as_ref().is_some_and(|s| s.read_id == id) =>
                {
                    ended = true;
                    p::chat_event::Event::SnapshotEnd(p::SnapshotEnd {
                        boundary: Some(boundary.clone()),
                        has_earlier_history: false,
                    })
                }
                _ => return Err("History page boundary mismatch.".into()),
            };
            projection.apply(
                &chat,
                p::ChatEvent {
                    chat: Some(chat.clone()),
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
