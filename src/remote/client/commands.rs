use super::{
    HostClient, rpc_error,
    store::{MutationRecord, OutboxEntry},
};
use crate::remote::proto as p;
use prost::Message;
use std::future::Future;
use tonic::transport::Channel;
impl HostClient {
    /// All online mutations share an epoch gate. Persist only command identity,
    /// never credentials/request bodies. Unknown outcomes require reconciliation.
    pub async fn mutate<F, Fut>(
        &self,
        revision: Option<u64>,
        call: F,
    ) -> Result<p::CommandReceipt, String>
    where
        F: FnOnce(Channel, p::CommandMeta) -> Fut,
        Fut: Future<Output = Result<tonic::Response<p::CommandReceipt>, tonic::Status>>,
    {
        let _gate = self.mutation.lock().await;
        let channel = self.channel()?;
        let record = self.snapshot().record;
        if record.closing_epoch.is_some() {
            return Err(
                "Command epoch closure is pending. Reconcile it before making changes.".into(),
            );
        }
        if record
            .mutations
            .iter()
            .filter(|r| Some(r.epoch) == record.epoch)
            .any(|r| {
                r.receipt
                    .as_ref()
                    .and_then(|v| p::CommandReceipt::decode(v.as_slice()).ok())
                    .is_none_or(|r| ![3, 4, 5, 7].contains(&r.state))
            })
        {
            return Err(
                "An earlier operation is unresolved. Query its result before another mutation."
                    .into(),
            );
        }
        let meta = self.command(revision)?;
        self.update_record(|r| {
            r.mutations.retain(|m| {
                m.receipt
                    .as_ref()
                    .and_then(|v| p::CommandReceipt::decode(v.as_slice()).ok())
                    .is_none_or(|v| !v.resolved_by_user && ![3, 4, 5, 7].contains(&v.state))
            });
            if r.mutations.len() >= 100 {
                return Err("Operation recovery storage is full.".into());
            }
            r.mutations.push(MutationRecord {
                chat_intent: None,
                id: meta.command_id.clone(),
                epoch: meta.command_epoch,
                receipt: None,
            });
            Ok(())
        })?;
        let started = std::time::Instant::now();
        let response = call(channel, meta.clone()).await;
        crate::remote::metrics::record(
            self.id().as_str(),
            &crate::remote::metrics::resource_id("control"),
            Some(&meta.command_id),
            0,
            crate::remote::metrics::Stage::ClientRoundTrip,
            started,
            if response.is_ok() {
                crate::remote::metrics::Outcome::Ok
            } else {
                crate::remote::metrics::Outcome::DeliveryUnknown
            },
        );
        let receipt = response.map_err(rpc_error)?.into_inner();
        self.validate_receipt(&meta, &receipt)?;
        self.update_record(|r| {
            if let Some(entry) = r.mutations.iter_mut().find(|r| r.id == meta.command_id) {
                entry.receipt = Some(receipt.encode_to_vec());
            }
            Ok(())
        })?;
        self.changed();
        check_receipt(receipt)
    }
    /// Input control is ephemeral and never replayed after a lost response.
    pub async fn acquire_input(
        &self,
        terminal: p::TerminalRef,
        take_over: bool,
    ) -> Result<p::InputLease, String> {
        let _gate = self.mutation.lock().await;
        let command = self.command(None)?;
        p::terminal_service_client::TerminalServiceClient::new(self.channel()?)
            .acquire_input(p::AcquireInputRequest {
                command: Some(command),
                terminal: Some(terminal),
                take_over,
            })
            .await
            .map_err(rpc_error)
            .map(|r| r.into_inner())
    }
    pub async fn release_input(
        &self,
        terminal: p::TerminalRef,
        lease: p::InputLease,
    ) -> Result<(), String> {
        let _gate = self.mutation.lock().await;
        let command = self.command(None)?;
        p::terminal_service_client::TerminalServiceClient::new(self.channel()?)
            .release_input(p::ReleaseInputRequest {
                command: Some(command),
                terminal: Some(terminal),
                run_id: lease.run_id,
                fencing_token: lease.fencing_token,
            })
            .await
            .map_err(rpc_error)?;
        Ok(())
    }
    pub(super) fn command(&self, revision: Option<u64>) -> Result<p::CommandMeta, String> {
        let record = self.snapshot().record;
        if record.closing_epoch.is_some() {
            return Err("Command epoch is closing.".into());
        }
        Ok(p::CommandMeta {
            host_id: self.id.0.clone(),
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: record
                .epoch
                .ok_or("Connect to establish the command epoch.")?,
            expected_revision: revision,
        })
    }
    pub(super) fn validate_receipt(
        &self,
        meta: &p::CommandMeta,
        receipt: &p::CommandReceipt,
    ) -> Result<(), String> {
        if receipt.host_id != self.id.0
            || receipt.command_id != meta.command_id
            || receipt.command_epoch != meta.command_epoch
        {
            return Err(
                "Command result identity mismatch; retain the operation for reconciliation.".into(),
            );
        }
        Ok(())
    }
    /// Text, local images and contextual attachments form one draft revision.
    pub fn save_composer_version(
        &self,
        chat: &str,
        draft: p::MessageDraft,
        context: Option<p::SendContext>,
        images: Vec<String>,
        revision: u64,
    ) -> Result<(), String> {
        let context = context.as_ref().map(Message::encode_to_vec);
        let text = serde_json::to_string(&(draft.text.clone(), context.clone(), images))
            .map_err(|e| e.to_string())?;
        let attachments = serde_json::to_string(&(
            p::MessageDraft {
                attachments: draft.attachments,
                ..Default::default()
            }
            .encode_to_vec(),
            context,
        ))
        .map_err(|e| e.to_string())?;
        if text.len() + attachments.len() > 1024 * 1024 {
            return Err("Draft exceeds the local 1 MiB limit.".into());
        }
        let key = format!("composer:{chat}");
        let mut state = self.state.lock().unwrap();
        // Equality also rejects a delayed write of the version already consumed by Send.
        if state
            .view_revisions
            .get(&key)
            .is_some_and(|saved| *saved >= revision)
        {
            return Ok(());
        }
        let mut record = state.record.clone();
        for (key, value) in [
            (format!("chat:{chat}"), text),
            (format!("context:{chat}"), attachments),
        ] {
            if record.views.len() >= 100 && !record.views.contains_key(&key) {
                return Err("Local draft limit reached.".into());
            }
            record.views.insert(key, value);
        }
        self.store.lock().unwrap().save(record.clone())?;
        state.record = record;
        state.view_revisions.insert(key, revision);
        Ok(())
    }
    /// Clear only the submitted revision. Keep upload journals: accepted commands
    /// and newer drafts may still refer to their local images.
    pub fn clear_composer_version(&self, chat: &str, revision: u64) -> Result<bool, String> {
        let mut state = self.state.lock().unwrap();
        let key = format!("composer:{chat}");
        if state
            .view_revisions
            .get(&key)
            .is_some_and(|saved| *saved > revision)
        {
            return Ok(false);
        }
        let mut record = state.record.clone();
        record.views.remove(&format!("chat:{chat}"));
        record.views.remove(&format!("context:{chat}"));
        self.store.lock().unwrap().save(record.clone())?;
        state.record = record;
        state.view_revisions.insert(key, revision);
        Ok(true)
    }
    /// Offline enqueue never dispatches. The exact host/chat/context are durable.
    pub fn enqueue(
        &self,
        chat: p::ChatRef,
        context: p::SendContext,
        draft: p::MessageDraft,
        uploads: Vec<String>,
        revision: u64,
    ) -> Result<(String, bool), String> {
        if chat.host_id != self.id.0
            || context
                .workspace
                .as_ref()
                .is_some_and(|w| w.host_id != self.id.0)
        {
            return Err("Message belongs to another host.".into());
        }
        if draft
            .attachments
            .iter()
            .any(|a| !matches!(a.content, Some(p::attachment::Content::Image(_))))
        {
            return Err("Offline outbox supports text and prepared images only. Send contextual attachments while online.".into());
        }
        let chat_id = chat.chat_id.clone();
        let id = uuid::Uuid::new_v4().to_string();
        let request = p::SendMessageRequest {
            command: None,
            chat: Some(chat),
            message_id: uuid::Uuid::new_v4().to_string(),
            draft: Some(draft),
            expected_context: Some(context),
        }
        .encode_to_vec();
        let mut state = self.state.lock().unwrap();
        let mut r = state.record.clone();
        if r.outbox.len() >= 100
            || r.outbox.iter().map(|e| e.request.len()).sum::<usize>() + request.len()
                > 4 * 1024 * 1024
        {
            return Err("Local outbox is full. Resolve or remove saved messages first.".into());
        }
        r.outbox.push(OutboxEntry {
            id: id.clone(),
            request,
            attempted: false,
            uploads,
            receipt: None,
        });
        let key = format!("composer:{chat_id}");
        let cleared = state
            .view_revisions
            .get(&key)
            .is_none_or(|saved| *saved <= revision);
        if cleared {
            r.views.remove(&format!("chat:{chat_id}"));
            r.views.remove(&format!("context:{chat_id}"));
        }
        self.store.lock().unwrap().save(r.clone())?;
        state.record = r;
        if cleared {
            state.view_revisions.insert(key, revision);
        }
        drop(state);
        self.changed();
        Ok((id, cleared))
    }
    pub fn outbox(&self) -> Vec<(String, p::SendMessageRequest, Option<p::CommandReceipt>)> {
        let entries = self.state.lock().unwrap().record.outbox.clone();
        entries
            .into_iter()
            .filter_map(|e| {
                Some((
                    e.id,
                    p::SendMessageRequest::decode(e.request.as_slice()).ok()?,
                    e.receipt
                        .and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok()),
                ))
            })
            .collect()
    }
    /// Taking a never-sent message persists the recovered composer in the same
    /// local write that removes the queue entry. Attempted messages keep their ID.
    pub async fn take_unsent(
        &self,
        id: &str,
        image_dir: &std::path::Path,
        revision: u64,
    ) -> Result<(crate::chat::session::Draft, Option<p::SendContext>), String> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let _gate = self.mutation.lock().await;
        let record = self.snapshot().record;
        let entry = record
            .outbox
            .iter()
            .find(|e| e.id == id)
            .ok_or("Message no longer exists.")?;
        if entry.attempted {
            return Err("Check the original delivery before editing this message.".into());
        }
        let request = p::SendMessageRequest::decode(entry.request.as_slice())
            .map_err(|_| "Invalid saved message.")?;
        let mut draft =
            super::presentation::draft(request.draft.as_ref().ok_or("Missing saved draft.")?);
        for id in &entry.uploads {
            let upload = record
                .uploads
                .iter()
                .find(|u| &u.id == id)
                .ok_or("Missing saved image.")?;
            let begin = p::BeginUploadRequest::decode(upload.begin.as_slice())
                .map_err(|_| "Invalid saved image.")?;
            if begin.size > 32 * 1024 * 1024 {
                return Err("Saved image exceeds 32 MiB.".into());
            }
            let path = self.store.lock().unwrap().attachment_dir().join(id);
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .map_err(|e| e.to_string())?
                .take(begin.size + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 != begin.size || Sha256::digest(&bytes).as_slice() != begin.sha256
            {
                return Err("Saved image changed. The queued message was kept.".into());
            }
            draft
                .attachments
                .push(crate::chat::attachments::prepare_image(&bytes, image_dir)?);
        }
        let chat = request.chat.as_ref().ok_or("Missing saved chat.")?;
        let key = format!("native-draft:{}", chat.chat_id);
        let value = serde_json::to_string(&(
            &draft.text,
            &draft.attachments,
            &draft.context,
            draft.goal,
            request
                .expected_context
                .as_ref()
                .map(prost::Message::encode_to_vec),
        ))
        .map_err(|e| e.to_string())?;
        let mut state = self.state.lock().unwrap();
        if state
            .view_revisions
            .get(&key)
            .is_some_and(|saved| *saved > revision)
        {
            return Err("The draft changed while taking the queued message. Try again.".into());
        }
        let mut record = state.record.clone();
        let index = record
            .outbox
            .iter()
            .position(|e| e.id == entry.id && !e.attempted && e.request == entry.request)
            .ok_or("The queued message changed. Check its delivery first.")?;
        record.outbox.remove(index);
        record.views.insert(key.clone(), value);
        self.store.lock().unwrap().save(record.clone())?;
        state.record = record;
        state.view_revisions.insert(key, revision);
        drop(state);
        self.changed();
        Ok((draft, request.expected_context))
    }

    pub fn move_unsent(&self, id: &str, before: &str) -> Result<(), String> {
        self.update_record(|record| {
            let at = record
                .outbox
                .iter()
                .position(|e| e.id == id && !e.attempted)
                .ok_or("Only never-sent messages can be reordered locally.")?;
            let target = record
                .outbox
                .iter()
                .position(|e| e.id == before && !e.attempted)
                .ok_or("Only messages waiting on this device can be reordered together.")?;
            let entry = record.outbox.remove(at);
            record
                .outbox
                .insert(if at < target { target - 1 } else { target }, entry);
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub async fn send_outbox(&self, id: &str) -> Result<p::CommandReceipt, String> {
        let _gate = self.mutation.lock().await;
        let channel = self.channel()?;
        let entry = self
            .snapshot()
            .record
            .outbox
            .into_iter()
            .find(|e| e.id == id)
            .ok_or("Message no longer exists.")?;
        if let Some(receipt) = entry
            .receipt
            .as_ref()
            .and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok())
            && receipt.resolved_by_user
        {
            return check_receipt(receipt);
        }
        let mut request = p::SendMessageRequest::decode(entry.request.as_slice())
            .map_err(|_| "Saved message is invalid.")?;
        if entry.attempted {
            let meta = request
                .command
                .as_ref()
                .ok_or("Missing saved command identity.")?;
            // Explicit retry uses the original typed Send with the same UUID,
            // context and artifact references. A legacy lookup lacks its thread key.
            if self.snapshot().record.epoch != Some(meta.command_epoch) {
                return Err(
                    "The saved command epoch was retired. Reconcile before creating a new message."
                        .into(),
                );
            }
        } else {
            let catalog = self.snapshot_catalog().await?;
            let summary = catalog
                .chats
                .iter()
                .find(|c| c.chat == request.chat)
                .ok_or("Chat no longer exists on its original host.")?;
            let context = p::SendContext {
                agent_id: summary.agent_id.clone(),
                profile_id: summary.profile_id.clone(),
                history_epoch: summary.history_epoch,
                workspace: summary.workspace.clone(),
            };
            if request.expected_context.as_ref() != Some(&context) {
                return Err("The chat account, history or workspace changed. Preserve this message and review its original context before sending.".into());
            }
            for upload in &entry.uploads {
                let artifact = self.resume_upload_locked(upload).await?;
                request
                    .draft
                    .as_mut()
                    .ok_or("Missing message draft.")?
                    .attachments
                    .push(p::Attachment {
                        label: "Image".into(),
                        content: Some(p::attachment::Content::Image(artifact)),
                    });
            }
            let mut meta = self.command(None)?;
            meta.command_id = id.to_owned();
            request.command = Some(meta);
        }
        self.update_record(|r| {
            let entry = r
                .outbox
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or("Saved message disappeared.")?;
            entry.request = request.encode_to_vec();
            entry.attempted = true;
            Ok(())
        })?;
        let started = std::time::Instant::now();
        let response = p::chat_service_client::ChatServiceClient::new(channel)
            .send_message(request.clone())
            .await;
        let resource = crate::remote::metrics::resource_id(
            request
                .chat
                .as_ref()
                .map(|chat| chat.chat_id.as_str())
                .unwrap_or("outbox"),
        );
        crate::remote::metrics::record(
            self.id().as_str(),
            &resource,
            Some(id),
            0,
            crate::remote::metrics::Stage::ClientRoundTrip,
            started,
            if response.is_ok() {
                crate::remote::metrics::Outcome::Ok
            } else {
                crate::remote::metrics::Outcome::DeliveryUnknown
            },
        );
        let receipt = response.map_err(rpc_error)?.into_inner();
        self.validate_receipt(request.command.as_ref().unwrap(), &receipt)?;
        self.save_outbox_receipt(id, &receipt)?;
        check_receipt(receipt)
    }
    fn save_outbox_receipt(&self, id: &str, receipt: &p::CommandReceipt) -> Result<(), String> {
        self.update_record(|r| {
            if let Some(entry) = r.outbox.iter_mut().find(|e| e.id == id) {
                entry.receipt = Some(receipt.encode_to_vec());
            }
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub fn dismiss_outbox(&self, id: &str) -> Result<(), String> {
        self.update_record(|r| {
            if r.outbox.iter().any(|e| {
                e.id == id
                    && e.attempted
                    && e.receipt
                        .as_ref()
                        .and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok())
                        .is_none_or(|v| !v.resolved_by_user && ![3, 4, 5, 7].contains(&v.state))
            }) {
                return Err("Reconcile the uncertain message before dismissing it.".into());
            }
            r.outbox.retain(|e| e.id != id);
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    /// Explicit user review of one uncertain send. This records an unknown
    /// outcome as reviewed; it never treats it as delivered and never retries it.
    pub async fn review_outbox(&self, id: &str) -> Result<(), String> {
        let _gate = self.mutation.lock().await;
        let (_, request, _) = self
            .outbox()
            .into_iter()
            .find(|(key, _, _)| key == id)
            .ok_or("Saved message not found.")?;
        let meta = request.command.ok_or("The message has not been sent.")?;
        let mut client = p::host_service_client::HostServiceClient::new(self.channel()?);
        let query = p::GetCommandRequest {
            host_id: self.id().as_str().into(),
            command_id: meta.command_id.clone(),
            command_epoch: meta.command_epoch,
        };
        let mut receipt = client
            .get_command(query.clone())
            .await
            .map_err(rpc_error)?
            .into_inner();
        self.validate_receipt(&meta, &receipt)?;
        if receipt.state == p::OperationState::DeliveryUnknown as i32 && !receipt.resolved_by_user {
            receipt = client
                .resolve_command(query)
                .await
                .map_err(rpc_error)?
                .into_inner();
            self.validate_receipt(&meta, &receipt)?;
            if !receipt.resolved_by_user {
                return Err("The host did not confirm the review.".into());
            }
        }
        self.save_outbox_receipt(id, &receipt)?;
        Ok(())
    }
    pub async fn resolve_outbox(&self, id: &str, resend: bool) -> Result<Option<String>, String> {
        self.review_outbox(id).await?;
        let next = resend.then(|| uuid::Uuid::new_v4().to_string());
        self.update_record(|record| {
            let at = record
                .outbox
                .iter()
                .position(|entry| entry.id == id)
                .ok_or("The saved message changed.")?;
            let old = &record.outbox[at];
            let receipt = old
                .receipt
                .as_ref()
                .and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok())
                .ok_or("No reviewed result.")?;
            if !receipt.resolved_by_user && ![3, 4, 5, 7].contains(&receipt.state) {
                return Err(
                    "The operation is still executing. Check it again before deciding.".into(),
                );
            }
            let mut request =
                p::SendMessageRequest::decode(old.request.as_slice()).map_err(|e| e.to_string())?;
            if record.mutations.len() >= 100 {
                return Err("Review saved operations before resolving this message.".into());
            }
            record.mutations.push(MutationRecord {
                id: receipt.command_id.clone(),
                epoch: receipt.command_epoch,
                receipt: Some(receipt.encode_to_vec()),
                chat_intent: None,
            });
            let old = record.outbox.remove(at);
            if let Some(next) = &next {
                request.command = None;
                request.message_id = uuid::Uuid::new_v4().to_string();
                record.outbox.push(OutboxEntry {
                    id: next.clone(),
                    request: request.encode_to_vec(),
                    attempted: false,
                    uploads: old.uploads,
                    receipt: None,
                });
            }
            Ok(())
        })?;
        self.changed();
        Ok(next)
    }
    pub async fn reconcile(&self) -> Result<(), String> {
        let _gate = self.mutation.lock().await;
        let mut client = p::host_service_client::HostServiceClient::new(self.channel()?);
        let record = self.snapshot().record;
        for entry in record.mutations {
            if entry
                .receipt
                .as_ref()
                .and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok())
                .is_some_and(|receipt| receipt.resolved_by_user)
            {
                continue;
            }
            let receipt = client
                .get_command(p::GetCommandRequest {
                    host_id: self.id.0.clone(),
                    command_id: entry.id.clone(),
                    command_epoch: entry.epoch,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
            self.validate_receipt(
                &p::CommandMeta {
                    host_id: self.id.0.clone(),
                    command_id: entry.id.clone(),
                    command_epoch: entry.epoch,
                    expected_revision: None,
                },
                &receipt,
            )?;
            self.update_record(|r| {
                if let Some(entry) = r.mutations.iter_mut().find(|e| e.id == entry.id) {
                    entry.receipt = Some(receipt.encode_to_vec());
                }
                Ok(())
            })?;
        }
        for entry in record.outbox.into_iter().filter(|e| e.attempted) {
            if entry
                .receipt
                .as_ref()
                .and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok())
                .is_some_and(|receipt| receipt.resolved_by_user)
            {
                continue;
            }
            let request = p::SendMessageRequest::decode(entry.request.as_slice())
                .map_err(|_| "Invalid saved message.")?;
            let meta = request.command.ok_or("Missing saved command identity.")?;
            let receipt = client
                .get_command(p::GetCommandRequest {
                    host_id: meta.host_id.clone(),
                    command_id: meta.command_id.clone(),
                    command_epoch: meta.command_epoch,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
            self.validate_receipt(&meta, &receipt)?;
            self.save_outbox_receipt(&entry.id, &receipt)?;
        }
        self.changed();
        Ok(())
    }
    /// Explicit closure; serialize against every mutating operation. Unknown
    /// results remain visible; unsent text has no epoch and survives closure.
    pub async fn close_epoch(&self) -> Result<(), String> {
        self.close_epoch_inner(false).await
    }
    /// Call only after the user reviewed uncertain outcomes. The server still
    /// rejects closure while an operation is actively executing.
    pub async fn close_epoch_after_review(&self) -> Result<(), String> {
        self.close_epoch_inner(true).await
    }
    async fn close_epoch_inner(&self, reviewed: bool) -> Result<(), String> {
        let _gate = self.mutation.lock().await;
        let record = self.snapshot().record;
        if record.uploads.iter().any(|u| {
            u.artifact.is_none()
                && p::BeginUploadRequest::decode(u.begin.as_slice())
                    .ok()
                    .is_some_and(|b| b.command.is_some())
        }) {
            return Err("An unresolved upload prevents epoch closure.".into());
        }
        if !reviewed
            && (record.outbox.iter().any(|e| {
                e.attempted
                    && e.receipt
                        .as_ref()
                        .and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok())
                        .is_none_or(|r| ![3, 4, 5, 7].contains(&r.state))
            }) || record.mutations.iter().any(|e| {
                e.receipt
                    .as_ref()
                    .and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok())
                    .is_none_or(|r| ![3, 4, 5, 7].contains(&r.state))
            }))
        {
            return Err(
                "Unresolved operations prevent epoch closure. Inspect their outcomes first.".into(),
            );
        }
        let epoch = record
            .closing_epoch
            .or(record.epoch)
            .ok_or("Missing command epoch.")?;
        self.update_record(|r| {
            r.closing_epoch = Some(epoch);
            Ok(())
        })?;
        if reviewed {
            let mut client = p::host_service_client::HostServiceClient::new(self.channel()?);
            let mut commands = record
                .mutations
                .iter()
                .map(|entry| (entry.id.clone(), entry.epoch, entry.receipt.clone(), None))
                .collect::<Vec<_>>();
            for entry in record.outbox.iter().filter(|entry| entry.attempted) {
                let request = p::SendMessageRequest::decode(entry.request.as_slice())
                    .map_err(|_| "Invalid saved message.")?;
                let meta = request.command.ok_or("Missing saved command identity.")?;
                commands.push((
                    meta.command_id,
                    meta.command_epoch,
                    entry.receipt.clone(),
                    Some(entry.id.clone()),
                ));
            }
            for (id, epoch, saved, outbox) in commands {
                let saved =
                    saved.and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok());
                if saved.as_ref().is_some_and(|receipt| {
                    receipt.resolved_by_user || [3, 4, 5, 7].contains(&receipt.state)
                }) {
                    continue;
                }
                let request = p::GetCommandRequest {
                    host_id: self.id.0.clone(),
                    command_id: id.clone(),
                    command_epoch: epoch,
                };
                let meta = p::CommandMeta {
                    host_id: self.id.0.clone(),
                    command_id: id.clone(),
                    command_epoch: epoch,
                    expected_revision: None,
                };
                let mut receipt = client
                    .get_command(request.clone())
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                self.validate_receipt(&meta, &receipt)?;
                if receipt.state == 6 && !receipt.resolved_by_user {
                    receipt = client
                        .resolve_command(request)
                        .await
                        .map_err(rpc_error)?
                        .into_inner();
                    self.validate_receipt(&meta, &receipt)?;
                    if !receipt.resolved_by_user {
                        return Err("Host did not acknowledge operation review.".into());
                    }
                }
                if let Some(outbox) = outbox {
                    self.save_outbox_receipt(&outbox, &receipt)?;
                } else {
                    self.update_record(|record| {
                        if let Some(entry) = record
                            .mutations
                            .iter_mut()
                            .find(|entry| entry.id == id && entry.epoch == epoch)
                        {
                            entry.receipt = Some(receipt.encode_to_vec());
                        }
                        Ok(())
                    })?;
                }
            }
        }
        let result = p::host_service_client::HostServiceClient::new(self.channel()?)
            .close_command_epoch(p::CloseCommandEpochRequest {
                host_id: self.id.0.clone(),
                expected_epoch: epoch,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        if result.host_id != self.id.0 || result.epoch != epoch + 1 {
            return Err("Unexpected epoch closure result.".into());
        }
        self.update_record(|r| {
            r.epoch = Some(result.epoch);
            r.closing_epoch = None;
            // Preserve uncertain outcomes for inspection; old epochs never replay.
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
}
pub fn check_receipt(receipt: p::CommandReceipt) -> Result<p::CommandReceipt, String> {
    if let Some(error) = &receipt.error {
        return Err(error.message.clone());
    }
    if [4, 5, 6, 7, 8].contains(&receipt.state) {
        return Err(format!(
            "Operation state {}. Inspect recovery before retrying.",
            receipt.state
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
mod draft_tests {
    use super::*;
    use crate::remote::client::{ClientRegistry, CredentialStore, HostId, store::HostRecord};
    use std::{
        collections::BTreeMap,
        sync::{Arc, Barrier},
    };
    struct NoVault;
    impl CredentialStore for NoVault {
        fn load(&self, _: &str) -> Result<String, String> {
            panic!("no vault access")
        }
        fn save(&self, _: &str, _: &str) -> Result<(), String> {
            panic!("no vault access")
        }
        fn remove(&self, _: &str) -> Result<(), String> {
            panic!("no vault access")
        }
    }
    fn fixture(path: std::path::PathBuf) -> (ClientRegistry, Arc<HostClient>) {
        let registry = ClientRegistry::open(path, Arc::new(NoVault)).unwrap();
        let host = registry
            .insert(HostRecord {
                mutual: false,
                passive_peer: false,
                auto_connect: false,
                id: HostId::parse(&uuid::Uuid::new_v4().to_string()).unwrap(),
                name: "Test".into(),
                endpoint: "https://127.0.0.1:1".into(),
                server_name: "test.local".into(),
                certificate: vec![],
                fingerprint: vec![],
                device_id: "device".into(),
                credential_ref: "fake".into(),
                ssh: None,
                routes: vec![],
                last_seen: None,
                epoch: Some(1),
                closing_epoch: None,
                catalog: vec![],
                outbox: vec![],
                mutations: vec![],
                views: BTreeMap::new(),
                uploads: vec![],
                chats: BTreeMap::new(),
            })
            .unwrap();
        (registry, host)
    }
    fn draft(text: &str) -> p::MessageDraft {
        p::MessageDraft {
            text: text.into(),
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn timed_out_chat_mutation_keeps_its_identity_and_blocks_a_new_attempt() {
        use crate::remote::client::ChatMutation;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client.json");
        let (registry, host) = fixture(path.clone());
        // The injected dispatcher never uses this lazy channel or any service.
        host.state.lock().unwrap().channel =
            Some(tonic::transport::Endpoint::from_static("http://127.0.0.1:1").connect_lazy());
        let chat = p::ChatRef {
            host_id: host.id().as_str().into(),
            chat_id: "chat".into(),
        };
        let intent = ChatMutation::Send(p::SendMessageRequest {
            chat: Some(chat.clone()),
            message_id: "original-message".into(),
            draft: Some(draft("keep this draft")),
            ..Default::default()
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(10),
            host.submit_chat_mutation_with(intent.clone(), |_, _| std::future::pending()),
        )
        .await;
        assert!(result.is_err());
        let pending = host.chat_mutations(&chat);
        assert_eq!(pending.len(), 1);
        let command = pending[0].0.clone();
        assert!(pending[0].2.is_none());
        let refusal = host
            .submit_chat_mutation_with(intent, |_, _| async {
                panic!("an unknown outcome must not dispatch a new command")
            })
            .await
            .unwrap_err();
        assert!(refusal.contains("unresolved"));
        let id = host.id().clone();
        drop(host);
        drop(registry);
        let restored = ClientRegistry::open(path, Arc::new(NoVault))
            .unwrap()
            .get(&id)
            .unwrap();
        let pending = restored.chat_mutations(&chat);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, command);
        assert!(pending[0].2.is_none());
        let ChatMutation::Send(request) = &pending[0].1 else {
            panic!("wrong intent")
        };
        assert_eq!(request.message_id, "original-message");
        assert_eq!(request.command.as_ref().unwrap().command_id, command);
        assert_eq!(request.draft.as_ref().unwrap().text, "keep this draft");
    }

    #[tokio::test]
    async fn taking_an_offline_message_keeps_a_durable_draft_and_refuses_late_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client.json");
        let (registry, host) = fixture(path.clone());
        let chat = p::ChatRef {
            host_id: host.id().as_str().into(),
            chat_id: "chat".into(),
        };
        let context = p::SendContext {
            agent_id: "remote-agent".into(),
            history_epoch: 9,
            ..Default::default()
        };
        let first = host
            .enqueue(chat.clone(), context.clone(), draft("queued"), vec![], 1)
            .unwrap()
            .0;
        let second = host
            .enqueue(chat.clone(), context.clone(), draft("later"), vec![], 1)
            .unwrap()
            .0;
        host.move_unsent(&second, &first).unwrap();
        assert_eq!(host.outbox()[0].0, second);
        host.save_view_version("native-draft:chat".into(), "newer".into(), 3)
            .unwrap();
        assert!(host.take_unsent(&first, dir.path(), 2).await.is_err());
        assert_eq!(host.outbox().len(), 2);
        let (taken, expected) = host.take_unsent(&first, dir.path(), 4).await.unwrap();
        assert_eq!(taken.text, "queued");
        assert_eq!(expected, Some(context));
        host.update_record(|r| {
            r.outbox[0].attempted = true;
            Ok(())
        })
        .unwrap();
        assert!(host.take_unsent(&second, dir.path(), 5).await.is_err());
        let id = host.id().clone();
        drop(host);
        drop(registry);
        let restored = ClientRegistry::open(path, Arc::new(NoVault))
            .unwrap()
            .get(&id)
            .unwrap();
        let saved = restored.snapshot().record;
        let recovered: serde_json::Value =
            serde_json::from_str(saved.view("native-draft:chat").unwrap()).unwrap();
        assert_eq!(recovered[0], "queued");
        assert_eq!(restored.outbox().len(), 1);
        assert_eq!(restored.outbox()[0].0, second);
    }
    #[test]
    fn completed_send_preserves_concurrent_draft_and_context_across_restart() {
        for enqueue in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("client.json");
            let (registry, host) = fixture(path.clone());
            let context = p::SendContext {
                agent_id: "agent".into(),
                history_epoch: 7,
                ..Default::default()
            };
            host.save_composer_version(
                "chat",
                draft("same text"),
                Some(context.clone()),
                vec!["image-a".into()],
                10,
            )
            .unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let worker = {
                let host = host.clone();
                let barrier = barrier.clone();
                let context = context.clone();
                std::thread::spawn(move || {
                    barrier.wait(); // A captured, fake upload/operation waits while the user edits B.
                    barrier.wait();
                    if enqueue {
                        host.enqueue(
                            p::ChatRef {
                                host_id: host.id().as_str().into(),
                                chat_id: "chat".into(),
                            },
                            context,
                            draft("same text"),
                            vec!["image-a".into()],
                            10,
                        )
                        .unwrap()
                        .1
                    } else {
                        host.clear_composer_version("chat", 10).unwrap()
                    }
                })
            };
            barrier.wait();
            let mut newer = draft("same text");
            newer.attachments.push(p::Attachment {
                label: "new quote".into(),
                content: Some(p::attachment::Content::Quote(p::QuoteAttachment {
                    text: "B".into(),
                })),
            });
            let context_b = p::SendContext {
                history_epoch: 8,
                ..context
            };
            host.save_composer_version("chat", newer, Some(context_b), vec!["image-b".into()], 11)
                .unwrap();
            let expected = host.snapshot().record;
            barrier.wait();
            assert!(!worker.join().unwrap());
            assert_eq!(
                host.snapshot().record.view("chat:chat"),
                expected.view("chat:chat")
            );
            assert_eq!(
                host.snapshot().record.view("context:chat"),
                expected.view("context:chat")
            );
            let id = host.id().clone();
            drop(host);
            drop(registry);
            let restored = ClientRegistry::open(path, Arc::new(NoVault))
                .unwrap()
                .get(&id)
                .unwrap();
            assert_eq!(
                restored.snapshot().record.view("chat:chat"),
                expected.view("chat:chat")
            );
            assert_eq!(
                restored.snapshot().record.view("context:chat"),
                expected.view("context:chat")
            );
            if enqueue {
                assert_eq!(
                    restored.outbox()[0].1.draft.as_ref().unwrap().text,
                    "same text"
                );
            }
        }
    }
    #[test]
    fn consumed_draft_cannot_resurrect_and_failed_clear_preserves_both_parts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client.json");
        let (registry, host) = fixture(path.clone());
        host.save_composer_version("chat", draft("A"), None, vec!["image".into()], 1)
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(host.clear_composer_version("chat", 1).is_err());
        assert!(host.snapshot().record.view("chat:chat").is_some());
        assert!(host.snapshot().record.view("context:chat").is_some());
        std::fs::remove_dir(&path).unwrap();
        assert!(host.clear_composer_version("chat", 1).unwrap());
        // A slow autosave from before Send must not recreate the consumed draft.
        host.save_composer_version("chat", draft("A"), None, vec!["image".into()], 1)
            .unwrap();
        let id = host.id().clone();
        drop(host);
        drop(registry);
        let restored = ClientRegistry::open(path, Arc::new(NoVault))
            .unwrap()
            .get(&id)
            .unwrap();
        assert!(restored.snapshot().record.view("chat:chat").is_none());
        assert!(restored.snapshot().record.view("context:chat").is_none());
    }
    #[tokio::test]
    async fn contextual_send_lost_response_keeps_exact_request_and_newer_draft() {
        use crate::remote::client::ChatMutation;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client.json");
        let (registry, host) = fixture(path.clone());
        host.state.lock().unwrap().channel =
            Some(tonic::transport::Endpoint::from_static("http://127.0.0.1:1").connect_lazy());
        let chat = p::ChatRef {
            host_id: host.id().as_str().into(),
            chat_id: "chat".into(),
        };
        let context = p::SendContext {
            agent_id: "agent-a".into(),
            history_epoch: 9,
            ..Default::default()
        };
        let mut original = draft("A");
        original.attachments.push(p::Attachment {
            label: "quote".into(),
            content: Some(p::attachment::Content::Quote(p::QuoteAttachment {
                text: "original quote".into(),
            })),
        });
        host.save_composer_version("chat", original.clone(), Some(context.clone()), vec![], 1)
            .unwrap();
        let captured = Arc::new(tokio::sync::Notify::new());
        let finish = Arc::new(tokio::sync::Notify::new());
        let intent = ChatMutation::Send(p::SendMessageRequest {
            command: None,
            chat: Some(chat.clone()),
            message_id: "original-message".into(),
            draft: Some(original.clone()),
            expected_context: Some(context.clone()),
        });
        let worker = {
            let host = host.clone();
            let captured = captured.clone();
            let finish = finish.clone();
            tokio::spawn(async move {
                host.submit_chat_mutation_with(intent, move |_, _| async move {
                    captured.notify_one();
                    finish.notified().await;
                    Err("response lost after accepted".into())
                })
                .await
            })
        };
        captured.notified().await;
        host.save_composer_version(
            "chat",
            draft("B"),
            Some(context.clone()),
            vec!["new-image".into()],
            2,
        )
        .unwrap();
        finish.notify_one();
        assert!(worker.await.unwrap().is_err());
        let command = host.chat_mutations(&chat)[0].0.clone();
        let expected = host.snapshot().record;
        let id = host.id().clone();
        drop(host);
        drop(registry);
        let restored = ClientRegistry::open(path, Arc::new(NoVault))
            .unwrap()
            .get(&id)
            .unwrap();
        let entries = restored.chat_mutations(&chat);
        let ChatMutation::Send(saved) = &entries[0].1 else {
            panic!("wrong intent")
        };
        assert_eq!(saved.command.as_ref().unwrap().command_id, command);
        assert_eq!(saved.message_id, "original-message");
        assert_eq!(saved.draft.as_ref(), Some(&original));
        assert_eq!(saved.expected_context.as_ref(), Some(&context));
        assert_eq!(
            restored.snapshot().record.view("chat:chat"),
            expected.view("chat:chat")
        );
        assert_eq!(
            restored.snapshot().record.view("context:chat"),
            expected.view("context:chat")
        );
    }
}
