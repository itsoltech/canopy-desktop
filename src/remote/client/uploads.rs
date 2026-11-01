//! Local source retention and exact typed upload requests in the existing journal.
use super::{
    HostClient,
    api_commands::ApiIntent,
    commands::terminal_state,
    rpc_error,
    store::{UploadCommandResult, UploadRecord},
};
use crate::{api::proto as p, command_bus::UploadContent};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub enum UploadTarget {
    Image(p::ThreadTarget),
    File(p::FileRef),
    Task(p::TaskRef),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UploadDraft {
    version: u32,
    target: DraftTarget,
    pub content: UploadContent,
}
#[derive(Clone, Serialize, Deserialize)]
enum DraftTarget {
    Image(Vec<u8>),
    File(Vec<u8>),
    Task(Vec<u8>),
}
impl UploadDraft {
    pub fn task(host: &str, task: p::TaskRef, content: UploadContent) -> Result<Self, String> {
        Self::capture(host, UploadTarget::Task(task), "task_attachment", content)
    }
    pub fn capture(
        host: &str,
        target: UploadTarget,
        purpose: &str,
        content: UploadContent,
    ) -> Result<Self, String> {
        let target = match (purpose, target) {
            ("image", UploadTarget::Image(t))
                if t.thread.as_ref().is_some_and(|t| t.host_id == host) =>
            {
                DraftTarget::Image(t.encode_to_vec())
            }
            ("file_save", UploadTarget::File(f)) => {
                let target_host = match f
                    .target
                    .as_ref()
                    .and_then(|t| t.resource.as_ref())
                    .ok_or("File target is missing.")?
                {
                    p::resource_target::Resource::Thread(t) => {
                        t.thread.as_ref().map(|t| t.host_id.as_str())
                    }
                    p::resource_target::Resource::Project(p) => Some(p.host_id.as_str()),
                };
                if target_host != Some(host) {
                    return Err("Upload belongs to another host.".into());
                }
                DraftTarget::File(f.encode_to_vec())
            }
            ("task_attachment", UploadTarget::Task(t)) => {
                let target_host = match t
                    .project
                    .as_ref()
                    .and_then(|p| p.resource.as_ref())
                    .ok_or("Task target is missing.")?
                {
                    p::task_project::Resource::Project(p) => &p.host_id,
                    p::task_project::Resource::Environment(e) => &e.host_id,
                };
                if target_host != host {
                    return Err("Upload belongs to another host.".into());
                }
                DraftTarget::Task(t.encode_to_vec())
            }
            _ => return Err("Upload purpose does not match its resource.".into()),
        };
        Ok(Self {
            version: 1,
            target,
            content,
        })
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|_| "Upload draft cannot be encoded.".into())
    }
    async fn begin(&self, _host: &HostClient) -> Result<ApiIntent, String> {
        let content = Some(p::UploadContent {
            size: self.content.size,
            sha256: self.content.sha256.to_vec(),
            media_type: self.content.media_type.clone(),
        });
        Ok(match &self.target {
            DraftTarget::Image(bytes) => ApiIntent::BeginImage(p::BeginImageUploadRequest {
                command: None,
                target: Some(
                    p::ThreadTarget::decode(bytes.as_slice())
                        .map_err(|_| "Invalid upload target.")?,
                ),
                content,
            }),
            DraftTarget::File(bytes) => ApiIntent::BeginFile(p::BeginFileUploadRequest {
                command: None,
                file: Some(
                    p::FileRef::decode(bytes.as_slice()).map_err(|_| "Invalid upload target.")?,
                ),
                content,
            }),
            DraftTarget::Task(bytes) => ApiIntent::BeginTask(p::BeginTaskUploadRequest {
                command: None,
                task: Some(
                    p::TaskRef::decode(bytes.as_slice()).map_err(|_| "Invalid upload target.")?,
                ),
                content,
            }),
        })
    }
}
impl UploadRecord {
    pub(super) fn draft(&self) -> Result<UploadDraft, String> {
        let draft: UploadDraft = serde_json::from_slice(&self.begin).map_err(|_| "Upload journal lacks a typed target; review it before retrying. Source bytes were retained.")?;
        if draft.version != 1 || draft.content.size > 32 * 1024 * 1024 {
            return Err("Unsupported upload journal; source bytes were retained.".into());
        }
        Ok(draft)
    }
    pub(super) fn unresolved(&self) -> bool {
        // Legacy sources without a typed command cannot be replayed, but can be
        // explicitly discarded. Never infer an effect from an undecodable draft.
        if self.api_begin.is_none() {
            return false;
        }
        if self.api_finish.is_some()
            && (self.finish_result.uncertain()
                || self.finish_result.retired && self.finish_result.receipt().is_none())
            || self.api_cancel.is_some()
                && (self.cancel_result.uncertain()
                    || self.cancel_result.retired && self.cancel_result.receipt().is_none())
            || self.begin_result.uncertain()
        {
            return true;
        }
        if self.artifact.is_some()
            || self.cancel_result.succeeded()
            || self.begin_result.non_effect()
            || self.finish_result.non_effect()
            || self.cancel_result.non_effect()
            || self.bulk_unavailable
        {
            return false;
        }
        // A retired Begin can only have allocated temporary bulk storage. A
        // retired Finish with no receipt still has an unproven effect.
        !(self.begin_result.retired && self.api_finish.is_none())
    }
    pub(super) fn blocks_epoch(&self, epoch: u64) -> bool {
        self.unresolved()
            && [UploadPhase::Begin, UploadPhase::Finish, UploadPhase::Cancel]
                .into_iter()
                .any(|phase| {
                    let (intent, result) = self.phase(phase);
                    let pending_bulk = matches!(phase, UploadPhase::Begin)
                        && self.api_finish.is_none()
                        && self.api_cancel.is_none()
                        && self.begin_result.succeeded();
                    !result.retired
                        && (pending_bulk
                            || !result.receipt().is_some_and(|r| terminal_state(r.state)))
                        && intent.as_ref().is_some_and(|(kind, bytes)| {
                            ApiIntent::decode(kind, bytes)
                                .map(|mut intent| {
                                    intent
                                        .meta()
                                        .as_ref()
                                        .is_none_or(|meta| meta.command_epoch == epoch)
                                })
                                .unwrap_or(true)
                        })
                })
    }
    fn phase(&self, phase: UploadPhase) -> (&Option<(String, Vec<u8>)>, &UploadCommandResult) {
        match phase {
            UploadPhase::Begin => (&self.api_begin, &self.begin_result),
            UploadPhase::Finish => (&self.api_finish, &self.finish_result),
            UploadPhase::Cancel => (&self.api_cancel, &self.cancel_result),
        }
    }
    fn result_mut(&mut self, phase: UploadPhase) -> &mut UploadCommandResult {
        match phase {
            UploadPhase::Begin => &mut self.begin_result,
            UploadPhase::Finish => &mut self.finish_result,
            UploadPhase::Cancel => &mut self.cancel_result,
        }
    }
}
#[derive(Clone, Copy)]
enum UploadPhase {
    Begin,
    Finish,
    Cancel,
}
impl UploadCommandResult {
    fn receipt(&self) -> Option<p::CommandReceipt> {
        self.receipt
            .as_ref()
            .and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok())
    }
    fn uncertain(&self) -> bool {
        // Retirement never overwrites a retained Accepted/Attempting/unknown.
        self.receipt
            .as_ref()
            .is_some_and(|_| self.receipt().is_none_or(|r| !terminal_state(r.state)))
            || self.receipt.is_none() && !self.retired
    }
    fn succeeded(&self) -> bool {
        self.receipt()
            .is_some_and(|r| r.state == p::OperationState::Succeeded as i32)
    }
    fn non_effect(&self) -> bool {
        self.receipt().is_some_and(|r| {
            matches!(
                p::OperationState::try_from(r.state),
                Ok(p::OperationState::Failed | p::OperationState::Rejected)
            )
        })
    }
}
fn finish(begin: &ApiIntent, upload_id: String) -> Result<ApiIntent, String> {
    let origin = match begin {
        ApiIntent::BeginImage(r) => &r.command,
        ApiIntent::BeginFile(r) => &r.command,
        ApiIntent::BeginTask(r) => &r.command,
        _ => return Err("Upload journal has an invalid begin command.".into()),
    }
    .as_ref()
    .ok_or("Upload origin is missing from its journal.")?;
    let origin = Some(p::UploadOrigin {
        command_id: origin.command_id.clone(),
        command_epoch: origin.command_epoch,
    });
    Ok(match begin {
        ApiIntent::BeginImage(r) => ApiIntent::FinishImage(p::FinishImageUploadRequest {
            command: None,
            target: r.target.clone(),
            upload_id,
            origin,
        }),
        ApiIntent::BeginFile(r) => ApiIntent::FinishFile(p::FinishFileUploadRequest {
            command: None,
            file: r.file.clone(),
            upload_id,
            origin,
        }),
        ApiIntent::BeginTask(r) => ApiIntent::FinishTask(p::FinishTaskUploadRequest {
            command: None,
            task: r.task.clone(),
            upload_id,
            origin,
        }),
        _ => return Err("Upload journal has an invalid begin command.".into()),
    })
}
impl HostClient {
    fn upload_record(&self, id: &str) -> Result<UploadRecord, String> {
        self.snapshot()
            .record
            .uploads
            .into_iter()
            .find(|r| r.id == id)
            .ok_or("Local attachment is unavailable.".into())
    }
    fn save_upload_receipt(
        &self,
        id: &str,
        phase: UploadPhase,
        receipt: &p::CommandReceipt,
    ) -> Result<(), String> {
        self.update_record(|record| {
            let upload = record
                .uploads
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or("Upload disappeared.")?;
            upload.result_mut(phase).receipt = Some(receipt.encode_to_vec());
            Ok(())
        })
    }
    async fn upload_command_with<F, Fut>(
        &self,
        id: &str,
        phase: UploadPhase,
        mut command: ApiIntent,
        dispatch: F,
    ) -> Result<p::ArtifactResult, String>
    where
        F: FnOnce(ApiIntent) -> Fut,
        Fut: std::future::Future<Output = Result<p::CommandReceipt, String>>,
    {
        let record = self.upload_record(id)?;
        let (_, saved) = record.phase(phase);
        if saved.receipt.is_some() {
            let receipt = saved
                .receipt()
                .ok_or("Saved upload result is unreadable; source retained.")?;
            if receipt.state != p::OperationState::Accepted as i32 || saved.retired {
                return upload_result(receipt);
            }
        }
        if saved.retired {
            return Err(
                "Upload command retired; source retained. Prepare a new upload explicitly.".into(),
            );
        }
        let meta = command
            .meta()
            .clone()
            .ok_or("Upload command identity is missing.")?;
        let resource = command.resource();
        let receipt = dispatch(command).await?;
        self.validate_receipt(&meta, &receipt)?;
        if receipt.resource != resource {
            return Err("Upload result resource differs from its journal.".into());
        }
        // Even a final error is part of the durable result, before returning it.
        self.save_upload_receipt(id, phase, &receipt)?;
        upload_result(receipt)
    }
    pub(super) async fn reconcile_uploads_locked(&self) -> Result<(), String> {
        for upload in self.snapshot().record.uploads {
            if upload.unresolved() {
                self.reconcile_upload_locked(&upload.id).await?;
            }
        }
        Ok(())
    }
    pub(super) async fn reconcile_upload_locked(&self, id: &str) -> Result<(), String> {
        self.reconcile_upload_with(
            id,
            |mut command| async move {
                self.lookup_command(
                    &command
                        .meta()
                        .clone()
                        .ok_or("Upload command identity is missing.")?,
                )
                .await
            },
            |request| async move {
                p::artifacts_service_client::ArtifactsServiceClient::new(
                    self.channel().map_err(tonic::Status::unavailable)?,
                )
                .get_upload(request)
                .await
                .map(|r| r.into_inner())
            },
        )
        .await
    }
    async fn reconcile_upload_with<L, LF, S, SF>(
        &self,
        id: &str,
        mut lookup: L,
        status: S,
    ) -> Result<(), String>
    where
        L: FnMut(ApiIntent) -> LF,
        LF: std::future::Future<Output = Result<p::GetCommandResponse, String>>,
        S: FnOnce(p::GetUploadRequest) -> SF,
        SF: std::future::Future<Output = Result<p::UploadStatus, tonic::Status>>,
    {
        let record = self.upload_record(id)?;
        for phase in [UploadPhase::Begin, UploadPhase::Finish, UploadPhase::Cancel] {
            let (intent, saved) = record.phase(phase);
            let Some((kind, bytes)) = intent else {
                continue;
            };
            if saved.retired || saved.receipt().is_some_and(|r| terminal_state(r.state)) {
                continue;
            }
            let mut command = ApiIntent::decode(kind, bytes)?;
            let meta = command
                .meta()
                .clone()
                .ok_or("Upload command identity is missing.")?;
            let lookup = lookup(command.clone()).await?;
            match p::CommandLookupState::try_from(lookup.state) {
                Ok(p::CommandLookupState::Found) => {
                    let receipt = lookup.receipt.ok_or("Upload result is missing.")?;
                    self.validate_receipt(&meta, &receipt)?;
                    if receipt.resource != command.resource() {
                        return Err("Upload result resource differs from its journal.".into());
                    }
                    self.save_upload_receipt(id, phase, &receipt)?;
                }
                Ok(p::CommandLookupState::Retired) => self.update_record(|record| {
                    record
                        .uploads
                        .iter_mut()
                        .find(|r| r.id == id)
                        .ok_or("Upload disappeared.")?
                        .result_mut(phase)
                        .retired = true;
                    Ok(())
                })?,
                _ => {} // NotFound is no proof of an effect, and never causes replay.
            }
        }
        let record = self.upload_record(id)?;
        let upload_id = record.upload_id.clone().or_else(|| {
            upload_result(record.begin_result.receipt()?)
                .ok()
                .map(|r| r.upload_id)
        });
        let Some(upload_id) = upload_id else {
            return Ok(());
        };
        self.update_record(|r| {
            r.uploads
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or("Upload disappeared.")?
                .upload_id = Some(upload_id.clone());
            Ok(())
        })?;
        if let Some(receipt) = record.finish_result.receipt()
            && let Ok(result) = upload_result(receipt)
            && result.upload_id == upload_id
            && let Some(artifact) = result.artifact
        {
            return self.save_upload_artifact(id, artifact);
        }
        if !self.upload_record(id)?.unresolved() {
            return Ok(());
        }
        let environment = Some(p::EnvironmentRef {
            host_id: self.id().as_str().into(),
        });
        match status(p::GetUploadRequest {
            environment: environment.clone(),
            upload_id: upload_id.clone(),
        })
        .await
        {
            Ok(status) => {
                if status.environment != environment || status.upload_id != upload_id {
                    return Err("Upload identity differs from its journal.".into());
                }
                if let Some(artifact) = status.artifact {
                    self.save_upload_artifact(id, artifact)?;
                }
            }
            Err(error) if error.code() == tonic::Code::NotFound => {
                // This also covers expired/unavailable targets. It proves no
                // successful Finish; uncertain command receipts remain protected.
                self.update_record(|r| {
                    r.uploads
                        .iter_mut()
                        .find(|r| r.id == id)
                        .ok_or("Upload disappeared.")?
                        .bulk_unavailable = true;
                    Ok(())
                })?;
            }
            Err(error) => return Err(rpc_error(error)),
        }
        Ok(())
    }
    fn save_upload_artifact(&self, id: &str, artifact: p::ArtifactRef) -> Result<(), String> {
        let draft = self.upload_record(id)?.draft()?;
        if artifact.host_id != self.id().as_str()
            || artifact.sha256 != draft.content.sha256
            || artifact.size != draft.content.size
            || artifact.media_type != draft.content.media_type
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
        })
    }
    pub(super) async fn resume_upload_locked(&self, id: &str) -> Result<p::ArtifactRef, String> {
        let record = self.upload_record(id)?;
        if record.api_begin.is_some() && record.unresolved() {
            self.reconcile_upload_locked(id).await?;
        }
        self.resume_upload_with(id, |command| async move {
            command
                .dispatch_typed(self.channel()?, self.id().as_str())
                .await
        })
        .await
    }
    async fn resume_upload_with<F, Fut>(
        &self,
        id: &str,
        mut dispatch: F,
    ) -> Result<p::ArtifactRef, String>
    where
        F: FnMut(ApiIntent) -> Fut,
        Fut: std::future::Future<Output = Result<p::CommandReceipt, String>>,
    {
        let record = self.upload_record(id)?;
        for result in [&record.begin_result, &record.finish_result] {
            if result.retired {
                return Err(
                    "Upload command retired; source retained. Prepare a new upload explicitly."
                        .into(),
                );
            }
            if let Some(receipt) = result.receipt()
                && receipt.state != p::OperationState::Accepted as i32
            {
                upload_result(receipt)?;
            }
        }
        if record.api_cancel.is_some() {
            return Err("Upload cancellation was requested; reconcile or finish cancellation before preparing a new upload.".into());
        }
        if let Some((kind, bytes)) = &record.api_finish
            && record
                .finish_result
                .receipt()
                .is_none_or(|r| r.state == p::OperationState::Accepted as i32)
        {
            // A saved Finish was already submitted. An explicit retry must
            // reach its durable precondition rejection even if bulk is gone.
            let result = self
                .upload_command_with(
                    id,
                    UploadPhase::Finish,
                    ApiIntent::decode(kind, bytes)?,
                    &mut dispatch,
                )
                .await?;
            if record
                .upload_id
                .as_ref()
                .is_some_and(|id| id != &result.upload_id)
            {
                return Err("Upload completion identity differs.".into());
            }
            let artifact = result
                .artifact
                .ok_or("Upload completion result is missing; source retained.")?;
            self.save_upload_artifact(id, artifact.clone())?;
            return Ok(artifact);
        }
        if record.bulk_unavailable {
            return Err("Upload is unavailable on the host; source retained. Prepare a new upload explicitly.".into());
        }
        if let Some(artifact) = &record.artifact {
            return p::ArtifactRef::decode(artifact.as_slice())
                .map_err(|_| "Invalid saved attachment.".into());
        }
        let draft = record.draft()?;
        let begin = if let Some((kind, bytes)) = &record.api_begin {
            ApiIntent::decode(kind, bytes)?
        } else {
            let mut begin = draft.begin(self).await?;
            *begin.meta() = Some(self.command(None)?);
            let (kind, bytes) = begin.encode();
            self.update_record(|r| {
                r.uploads
                    .iter_mut()
                    .find(|r| r.id == id)
                    .ok_or("Upload disappeared.")?
                    .api_begin = Some((kind.into(), bytes));
                Ok(())
            })?;
            begin
        };
        let upload_id = if let Some(upload_id) = &record.upload_id {
            upload_id.clone()
        } else {
            let result = self
                .upload_command_with(id, UploadPhase::Begin, begin.clone(), &mut dispatch)
                .await?;
            self.update_record(|r| {
                r.uploads
                    .iter_mut()
                    .find(|r| r.id == id)
                    .ok_or("Upload disappeared.")?
                    .upload_id = Some(result.upload_id.clone());
                Ok(())
            })?;
            result.upload_id
        };
        let environment = Some(p::EnvironmentRef {
            host_id: self.id().as_str().into(),
        });
        let mut client = p::artifacts_service_client::ArtifactsServiceClient::new(self.channel()?);
        let status = client
            .get_upload(p::GetUploadRequest {
                environment: environment.clone(),
                upload_id: upload_id.clone(),
            })
            .await;
        let mut status = match status {
            Ok(status) => status.into_inner(),
            Err(error) if error.code() == tonic::Code::NotFound => {
                self.update_record(|r| {
                    r.uploads
                        .iter_mut()
                        .find(|r| r.id == id)
                        .ok_or("Upload disappeared.")?
                        .bulk_unavailable = true;
                    Ok(())
                })?;
                return Err("Upload is unavailable on the host; source retained. Prepare a new upload explicitly.".into());
            }
            Err(error) => return Err(rpc_error(error)),
        };
        if status.environment != environment || status.upload_id != upload_id {
            return Err("Upload identity differs from its journal.".into());
        }
        if status.artifact.is_none() {
            let path = self.store.lock().unwrap().attachment_dir().join(id);
            let bytes =
                std::fs::read(path).map_err(|_| "Local attachment source is unavailable.")?;
            if bytes.len() as u64 != draft.content.size
                || Sha256::digest(&bytes).as_slice() != draft.content.sha256
            {
                return Err("Local attachment changed; upload was not resumed.".into());
            }
            for (index, chunk) in bytes.chunks(65536).enumerate() {
                let offset = (index * 65536) as u64;
                let end = offset + chunk.len() as u64;
                if status.received.iter().any(|r| {
                    r.offset <= offset && r.offset.checked_add(r.length).is_some_and(|n| n >= end)
                }) {
                    continue;
                }
                status = client
                    .put_chunk(p::PutChunkRequest {
                        environment: environment.clone(),
                        upload_id: upload_id.clone(),
                        offset,
                        data: chunk.to_vec(),
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                if status.environment != environment || status.upload_id != upload_id {
                    return Err("Chunk acknowledgement differs from its upload.".into());
                }
            }
            let finish = if let Some((kind, bytes)) = &record.api_finish {
                ApiIntent::decode(kind, bytes)?
            } else {
                let mut finish = finish(&begin, upload_id.clone())?;
                *finish.meta() = Some(self.command(None)?);
                let (kind, bytes) = finish.encode();
                self.update_record(|r| {
                    r.uploads
                        .iter_mut()
                        .find(|r| r.id == id)
                        .ok_or("Upload disappeared.")?
                        .api_finish = Some((kind.into(), bytes));
                    Ok(())
                })?;
                finish
            };
            let result = self
                .upload_command_with(id, UploadPhase::Finish, finish, &mut dispatch)
                .await?;
            if result.upload_id != upload_id {
                return Err("Upload completion identity differs.".into());
            }
            status.artifact = result.artifact;
        }
        let artifact = status
            .artifact
            .ok_or("Upload is incomplete; reconcile it before sending.")?;
        self.save_upload_artifact(id, artifact.clone())?;
        Ok(artifact)
    }
    /// Explicitly abandon an unfinished transfer. Retain its source and every
    /// original command; uncertain Finish outcomes cannot be cancelled away.
    pub async fn cancel_upload(&self, id: &str) -> Result<(), String> {
        let _gate = self.mutation.lock().await;
        let record = self.upload_record(id)?;
        if record.api_begin.is_some()
            && (record.begin_result.uncertain()
                || record.api_finish.is_some() && record.finish_result.uncertain()
                || record.api_cancel.is_some() && record.cancel_result.uncertain())
        {
            self.reconcile_upload_locked(id).await?;
        }
        self.cancel_upload_with(id, |command| async move {
            command
                .dispatch_typed(self.channel()?, self.id().as_str())
                .await
        })
        .await
    }
    async fn cancel_upload_with<F, Fut>(&self, id: &str, dispatch: F) -> Result<(), String>
    where
        F: FnOnce(ApiIntent) -> Fut,
        Fut: std::future::Future<Output = Result<p::CommandReceipt, String>>,
    {
        let record = self.upload_record(id)?;
        if record.api_begin.is_none() || record.begin_result.non_effect() {
            return Ok(());
        }
        if record.begin_result.uncertain()
            || record.api_finish.is_some()
                && (record.finish_result.uncertain() || record.finish_result.retired)
        {
            return Err(
                "Reconcile the original upload commands before cancellation; source retained."
                    .into(),
            );
        }
        if record.cancel_result.succeeded() {
            return Ok(());
        }
        if record.api_cancel.is_none() && record.bulk_unavailable {
            return Ok(());
        }
        if record.api_cancel.is_none() && record.artifact.is_some() {
            return Err("A finished artifact cannot be cancelled.".into());
        }
        let (kind, bytes) = record
            .api_begin
            .as_ref()
            .ok_or("Upload origin is missing.")?;
        let begin = ApiIntent::decode(kind, bytes)?;
        let command = if let Some((kind, bytes)) = record.api_cancel {
            ApiIntent::decode(&kind, &bytes)?
        } else {
            let upload_id = record
                .upload_id
                .or_else(|| {
                    upload_result(record.begin_result.receipt()?)
                        .ok()
                        .map(|r| r.upload_id)
                })
                .ok_or("Upload identity is unavailable; source retained.")?;
            let mut command = match finish(&begin, upload_id)? {
                ApiIntent::FinishImage(r) => ApiIntent::CancelImage(p::CancelImageUploadRequest {
                    command: None,
                    target: r.target,
                    upload_id: r.upload_id,
                    origin: r.origin,
                }),
                ApiIntent::FinishFile(r) => ApiIntent::CancelFile(p::CancelFileUploadRequest {
                    command: None,
                    file: r.file,
                    upload_id: r.upload_id,
                    origin: r.origin,
                }),
                ApiIntent::FinishTask(r) => ApiIntent::CancelTask(p::CancelTaskUploadRequest {
                    command: None,
                    task: r.task,
                    upload_id: r.upload_id,
                    origin: r.origin,
                }),
                _ => return Err("Upload origin is invalid.".into()),
            };
            *command.meta() = Some(self.command(None)?);
            let (kind, bytes) = command.encode();
            self.update_record(|r| {
                r.uploads
                    .iter_mut()
                    .find(|r| r.id == id)
                    .ok_or("Upload disappeared.")?
                    .api_cancel = Some((kind.into(), bytes));
                Ok(())
            })?;
            command
        };
        self.upload_command_with(id, UploadPhase::Cancel, command, dispatch)
            .await?;
        Ok(())
    }
}

fn upload_result(receipt: p::CommandReceipt) -> Result<p::ArtifactResult, String> {
    if let Some(error) = receipt.error {
        return Err(error.message);
    }
    if receipt.state != p::OperationState::Succeeded as i32 {
        return Err("Upload outcome requires reconciliation with the original request.".into());
    }
    match receipt.result {
        Some(p::command_receipt::Result::Artifact(result)) => Ok(result),
        _ => Err("Upload result is missing.".into()),
    }
}

#[cfg(test)]
mod tests;
