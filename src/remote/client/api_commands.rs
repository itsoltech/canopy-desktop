//! Exact typed requests share the existing device journal and epoch owner.
use super::{HostClient, commands::check_receipt, rpc_error, store::MutationRecord};
use crate::{
    api::{compat, proto as p},
    remote::proto as old,
};
use prost::Message;
use tonic::transport::Channel;

#[derive(Clone)]
pub(super) enum ApiIntent {
    Rename(p::RenameThreadRequest),
    Send(p::SendMessageRequest),
    Edit(p::EditQueueMessageRequest),
    Move(p::MoveQueueMessageRequest),
    Remove(p::RemoveQueueMessageRequest),
    Pause(p::PauseQueueRequest),
    Resolve(p::ResolveUnknownQueueMessageRequest),
    Preferences(p::UpdatePreferencesRequest),
}

macro_rules! request_field {
    ($value:expr, $field:ident) => {
        match $value {
            ApiIntent::Rename(v) => &mut v.$field,
            ApiIntent::Send(v) => &mut v.$field,
            ApiIntent::Edit(v) => &mut v.$field,
            ApiIntent::Move(v) => &mut v.$field,
            ApiIntent::Remove(v) => &mut v.$field,
            ApiIntent::Pause(v) => &mut v.$field,
            ApiIntent::Resolve(v) => &mut v.$field,
            ApiIntent::Preferences(v) => &mut v.$field,
        }
    };
}
impl ApiIntent {
    pub(super) fn meta(&mut self) -> &mut Option<p::CommandMeta> {
        request_field!(self, command)
    }
    pub(super) fn encode(&self) -> (&'static str, Vec<u8>) {
        match self {
            Self::Rename(v) => ("rename", v.encode_to_vec()),
            Self::Send(v) => ("send", v.encode_to_vec()),
            Self::Edit(v) => ("queue.edit", v.encode_to_vec()),
            Self::Move(v) => ("queue.move", v.encode_to_vec()),
            Self::Remove(v) => ("queue.remove", v.encode_to_vec()),
            Self::Pause(v) => ("queue.pause", v.encode_to_vec()),
            Self::Resolve(v) => ("queue.resolve", v.encode_to_vec()),
            Self::Preferences(v) => ("preferences", v.encode_to_vec()),
        }
    }
    pub(super) fn decode(kind: &str, bytes: &[u8]) -> Result<Self, String> {
        match kind {
            "rename" => p::RenameThreadRequest::decode(bytes).map(Self::Rename),
            "send" => p::SendMessageRequest::decode(bytes).map(Self::Send),
            "queue.edit" => p::EditQueueMessageRequest::decode(bytes).map(Self::Edit),
            "queue.move" => p::MoveQueueMessageRequest::decode(bytes).map(Self::Move),
            "queue.remove" => p::RemoveQueueMessageRequest::decode(bytes).map(Self::Remove),
            "queue.pause" => p::PauseQueueRequest::decode(bytes).map(Self::Pause),
            "queue.resolve" => {
                p::ResolveUnknownQueueMessageRequest::decode(bytes).map(Self::Resolve)
            }
            "preferences" => p::UpdatePreferencesRequest::decode(bytes).map(Self::Preferences),
            _ => return Err("Unknown saved API request; intent retained.".into()),
        }
        .map_err(|_| "Invalid saved API request; intent retained.".into())
    }
    fn resource(&self) -> Option<p::command_receipt::Resource> {
        let target = match self {
            Self::Rename(v) => &v.target,
            Self::Send(v) => &v.target,
            Self::Edit(v) => &v.target,
            Self::Move(v) => &v.target,
            Self::Remove(v) => &v.target,
            Self::Pause(v) => &v.target,
            Self::Resolve(v) => &v.target,
            Self::Preferences(v) => {
                return v
                    .environment
                    .clone()
                    .map(p::command_receipt::Resource::Environment);
            }
        };
        target.clone().map(p::command_receipt::Resource::Target)
    }
    pub(super) async fn dispatch(
        mut self,
        channel: Channel,
        host: &str,
        catalog_id: String,
    ) -> Result<old::CommandReceipt, String> {
        let meta = self
            .meta()
            .clone()
            .ok_or("Saved API command identity is missing.")?;
        let resource = self.resource().ok_or("Saved API target is missing.")?;
        let requested_host = match &resource {
            p::command_receipt::Resource::Target(target) => {
                let thread = target
                    .thread
                    .as_ref()
                    .ok_or("Thread identity is missing.")?;
                if crate::thread::ThreadId::parse(&thread.thread_id).is_none() {
                    return Err("Invalid saved thread identity.".into());
                }
                &thread.host_id
            }
            p::command_receipt::Resource::Environment(e) => &e.host_id,
        };
        if requested_host != host {
            return Err("Saved API request belongs to another host.".into());
        }
        let mut thread = p::thread_service_client::ThreadServiceClient::new(channel.clone());
        let result = match self {
            Self::Rename(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .rename_thread(v)
                    .await
            }
            Self::Send(v) => thread.send_message(v).await,
            Self::Edit(v) => thread.edit_queue_message(v).await,
            Self::Move(v) => thread.move_queue_message(v).await,
            Self::Remove(v) => thread.remove_queue_message(v).await,
            Self::Pause(v) => thread.pause_queue(v).await,
            Self::Resolve(v) => thread.resolve_unknown_queue_message(v).await,
            Self::Preferences(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .update_preferences(v)
                    .await
            }
        }
        .map_err(rpc_error)?
        .into_inner();
        if result.host_id != host
            || result.command_id != meta.command_id
            || result.command_epoch != meta.command_epoch
            || result.resource.as_ref() != Some(&resource)
        {
            return Err("API result identity differs from the original request; retain intent for reconciliation.".into());
        }
        Ok(compat::legacy_receipt(result, catalog_id))
    }
}

impl HostClient {
    pub(super) async fn retry_api_outbox(
        &self,
        entry: &super::store::OutboxEntry,
    ) -> Result<old::CommandReceipt, String> {
        let bytes = entry
            .api_request
            .as_ref()
            .ok_or("Original typed send is missing; retain the draft without replay.")?;
        let mut intent = ApiIntent::decode("send", bytes)?;
        let meta = intent
            .meta()
            .as_ref()
            .ok_or("Original command identity is missing.")?;
        if meta.command_id != entry.id || self.snapshot().record.epoch != Some(meta.command_epoch) {
            return Err(
                "Saved send identity or epoch differs; intent retained without replay.".into(),
            );
        }
        let request = old::SendMessageRequest::decode(entry.request.as_slice())
            .map_err(|_| "Invalid retained send.")?;
        let original = request
            .command
            .as_ref()
            .ok_or("Retained command identity is missing.")?;
        if original.host_id != self.id().as_str()
            || original.command_id != meta.command_id
            || original.command_epoch != meta.command_epoch
        {
            return Err("Retained send identity mismatch.".into());
        }
        intent
            .dispatch(
                self.channel()?,
                self.id().as_str(),
                request.chat.map(|c| c.chat_id).unwrap_or_default(),
            )
            .await
    }
    /// Discovery returns the stored UUID and epoch. A legacy key is never parsed as UUIDv7.
    pub async fn thread_target(&self, chat: &old::ChatRef) -> Result<p::ThreadTarget, String> {
        if chat.host_id != self.id().as_str() {
            return Err("Thread catalog target differs from this host.".into());
        }
        let mut request = tonic::Request::new(p::ListThreadsRequest {
            environment: Some(p::EnvironmentRef {
                host_id: chat.host_id.clone(),
            }),
            catalog_id: Some(chat.chat_id.clone()),
        });
        request.set_timeout(std::time::Duration::from_secs(15));
        let response = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .list_threads(request)
            .await
            .map_err(rpc_error)?
            .into_inner();
        let mut matching = response
            .threads
            .into_iter()
            .filter(|t| t.catalog_id == chat.chat_id);
        let summary = matching
            .next()
            .ok_or("Thread is no longer available on its original host.")?;
        if matching.next().is_some() {
            return Err("Thread discovery is ambiguous.".into());
        }
        let target = summary
            .target
            .ok_or("Thread discovery omitted its address.")?;
        let thread = target
            .thread
            .as_ref()
            .ok_or("Thread identity is missing.")?;
        if thread.host_id != chat.host_id
            || crate::thread::ThreadId::parse(&thread.thread_id).is_none()
        {
            return Err("Invalid discovered thread identity.".into());
        }
        Ok(target)
    }
    pub(super) async fn api_send(
        &self,
        request: &old::SendMessageRequest,
    ) -> Result<ApiIntent, String> {
        let chat = request.chat.as_ref().ok_or("Thread is missing.")?;
        let context = request
            .expected_context
            .as_ref()
            .ok_or("Message context is missing.")?;
        Ok(ApiIntent::Send(p::SendMessageRequest {
            command: request.command.clone().map(compat::command),
            target: Some(self.thread_target(chat).await?),
            message_id: request.message_id.clone(),
            draft: Some(compat::draft(
                request.draft.clone().ok_or("Message draft is missing.")?,
                chat,
                Some(context),
            )?),
            expected_context: Some(p::SendContext {
                account_id: context.agent_id.clone(),
                profile_id: context.profile_id.clone(),
                history_epoch: context.history_epoch,
            }),
        }))
    }
    async fn mutate_api(
        &self,
        mut intent: ApiIntent,
        catalog_id: String,
        revision: Option<u64>,
    ) -> Result<old::CommandReceipt, String> {
        let _gate = self.mutation.lock().await;
        let channel = self.channel()?;
        let meta = self.command(revision)?;
        *intent.meta() = Some(compat::command(meta.clone()));
        let (kind, bytes) = intent.encode();
        if bytes.len() > crate::host::CONTROL_BYTES {
            return Err("Operation exceeds the control limit; intent retained.".into());
        }
        self.update_record(|record| {
            if record.mutations.iter().any(|e| e.receipt.as_ref().and_then(|b| old::CommandReceipt::decode(b.as_slice()).ok()).is_none_or(|r| !r.resolved_by_user && ![3,4,5,7].contains(&r.state))) {
                return Err("An earlier operation is unresolved. Review its result before another mutation.".into());
            }
            // Every journaled operation is resolved here; only the new one remains.
            record.mutations.clear();
            record.mutations.push(MutationRecord { id: meta.command_id.clone(), epoch: meta.command_epoch, receipt: None, chat_intent: None, api_intent: Some((kind.into(), bytes)) });
            Ok(())
        })?;
        let started = std::time::Instant::now();
        let response = intent
            .dispatch(channel, self.id().as_str(), catalog_id.clone())
            .await;
        crate::remote::metrics::record(
            self.id().as_str(),
            &crate::remote::metrics::resource_id(&catalog_id),
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
        let receipt = response?;
        self.store_api_receipt(&meta.command_id, &receipt)?;
        check_receipt(receipt)
    }
    pub(super) fn store_api_receipt(
        &self,
        id: &str,
        receipt: &old::CommandReceipt,
    ) -> Result<(), String> {
        self.update_record(|record| {
            let entry = record
                .mutations
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or("Operation journal disappeared.")?;
            entry.receipt = Some(receipt.encode_to_vec());
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub async fn update_preferences(
        &self,
        request: old::UpdatePreferencesRequest,
        revision: u64,
    ) -> Result<old::CommandReceipt, String> {
        self.mutate_api(
            ApiIntent::Preferences(compat::preferences(request, self.id().as_str())),
            "private".into(),
            Some(revision),
        )
        .await
    }
    pub async fn rename_thread(
        &self,
        chat: old::ChatRef,
        title: String,
        revision: Option<u64>,
    ) -> Result<old::CommandReceipt, String> {
        let target = self.thread_target(&chat).await?;
        self.mutate_api(
            ApiIntent::Rename(p::RenameThreadRequest {
                command: None,
                target: Some(target),
                title,
            }),
            chat.chat_id,
            revision,
        )
        .await
    }
    pub async fn change_queue(
        &self,
        chat: old::ChatRef,
        revision: u64,
        change: old::change_queue_request::Change,
        context: Option<old::SendContext>,
    ) -> Result<old::CommandReceipt, String> {
        let target = Some(self.thread_target(&chat).await?);
        use old::change_queue_request::Change as C;
        let intent = match change {
            C::Edit(v) => ApiIntent::Edit(p::EditQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
                draft: Some(compat::draft(
                    v.draft.ok_or("Draft is missing.")?,
                    &chat,
                    context.as_ref(),
                )?),
            }),
            C::Move(v) => ApiIntent::Move(p::MoveQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
                before_message_id: v.before_message_id,
            }),
            C::Remove(v) => ApiIntent::Remove(p::RemoveQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
            }),
            C::Pause(v) => ApiIntent::Pause(p::PauseQueueRequest {
                command: None,
                target,
                queue_revision: revision,
                paused: v.paused,
            }),
            C::Resolve(v) => ApiIntent::Resolve(p::ResolveUnknownQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
                resolution: v.resolution,
                new_message_id: v.new_message_id,
            }),
        };
        // Queue CAS is carried by queue_revision. expected_revision refers to
        // thread metadata and must not be filled with the unrelated queue value.
        self.mutate_api(intent, chat.chat_id, None).await
    }
    pub(super) async fn retry_api(
        &self,
        entry: &MutationRecord,
        catalog_id: String,
    ) -> Result<old::CommandReceipt, String> {
        if self.snapshot().record.epoch != Some(entry.epoch) {
            return Err("Command epoch retired; intent retained without replay.".into());
        }
        let (kind, bytes) = entry
            .api_intent
            .as_ref()
            .ok_or("Saved typed request is missing.")?;
        let mut intent = ApiIntent::decode(kind, bytes)?;
        let meta = intent
            .meta()
            .as_ref()
            .ok_or("Saved command identity is missing.")?;
        if meta.command_id != entry.id || meta.command_epoch != entry.epoch {
            return Err("Saved API command identity mismatch.".into());
        }
        intent
            .dispatch(self.channel()?, self.id().as_str(), catalog_id)
            .await
    }
}
