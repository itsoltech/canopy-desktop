use super::{HostClient, commands::check_receipt, rpc_error, store::MutationRecord};
use crate::remote::proto as p;
use prost::Message;

/// Non-secret chat intent retained by the device until its outcome is known.
#[derive(Clone, Debug)]
pub enum ChatMutation {
    Send(p::SendMessageRequest),
    Edit(p::EditMessageRequest),
    Fork(p::ForkChatRequest),
    RunShell(p::RunShellRequest),
    StopShell(p::StopShellRequest),
}
impl ChatMutation {
    fn command_mut(&mut self) -> &mut Option<p::CommandMeta> {
        match self {
            Self::Send(v) => &mut v.command,
            Self::Edit(v) => &mut v.command,
            Self::Fork(v) => &mut v.command,
            Self::RunShell(v) => &mut v.command,
            Self::StopShell(v) => &mut v.command,
        }
    }
    pub fn chat(&self) -> Option<&p::ChatRef> {
        match self {
            Self::Send(v) => v.chat.as_ref(),
            Self::Edit(v) => v.chat.as_ref(),
            Self::Fork(v) => v.chat.as_ref(),
            Self::RunShell(v) => v.chat.as_ref(),
            Self::StopShell(v) => v.chat.as_ref(),
        }
    }
    fn encode(&self) -> (&'static str, Vec<u8>) {
        match self {
            Self::Send(v) => ("send", v.encode_to_vec()),
            Self::Edit(v) => ("edit", v.encode_to_vec()),
            Self::Fork(v) => ("fork", v.encode_to_vec()),
            Self::RunShell(v) => ("shell", v.encode_to_vec()),
            Self::StopShell(v) => ("stop_shell", v.encode_to_vec()),
        }
    }
    fn decode(kind: &str, bytes: &[u8]) -> Result<Self, String> {
        match kind {
            "send" => p::SendMessageRequest::decode(bytes).map(Self::Send),
            "edit" => p::EditMessageRequest::decode(bytes).map(Self::Edit),
            "fork" => p::ForkChatRequest::decode(bytes).map(Self::Fork),
            "shell" => p::RunShellRequest::decode(bytes).map(Self::RunShell),
            "stop_shell" => p::StopShellRequest::decode(bytes).map(Self::StopShell),
            _ => return Err("Unknown saved chat operation; intent retained.".into()),
        }
        .map_err(|error| error.to_string())
    }
    async fn dispatch(
        self,
        channel: tonic::transport::Channel,
    ) -> Result<p::CommandReceipt, String> {
        let mut client = p::chat_service_client::ChatServiceClient::new(channel);
        match self {
            Self::Send(value) => client.send_message(value).await,
            Self::Edit(value) => client.edit_message(value).await,
            Self::Fork(value) => client.fork_chat(value).await,
            Self::RunShell(value) => client.run_shell(value).await,
            Self::StopShell(value) => client.stop_shell(value).await,
        }
        .map(|response| response.into_inner())
        .map_err(rpc_error)
    }
}
impl HostClient {
    pub fn chat_mutations(
        &self,
        chat: &p::ChatRef,
    ) -> Vec<(String, ChatMutation, Option<p::CommandReceipt>)> {
        self.snapshot()
            .record
            .mutations
            .into_iter()
            .filter_map(|record| {
                let (kind, bytes) = record.chat_intent?;
                let intent = ChatMutation::decode(&kind, &bytes).ok()?;
                (intent.chat() == Some(chat)).then(|| {
                    (
                        record.id,
                        intent,
                        record
                            .receipt
                            .and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok()),
                    )
                })
            })
            .collect()
    }
    pub async fn submit_chat_mutation(
        &self,
        intent: ChatMutation,
    ) -> Result<p::CommandReceipt, String> {
        self.submit_chat_mutation_with(intent, |intent, channel| intent.dispatch(channel))
            .await
    }
    pub(super) async fn submit_chat_mutation_with<F, Fut>(
        &self,
        mut intent: ChatMutation,
        dispatch: F,
    ) -> Result<p::CommandReceipt, String>
    where
        F: FnOnce(ChatMutation, tonic::transport::Channel) -> Fut,
        Fut: std::future::Future<Output = Result<p::CommandReceipt, String>>,
    {
        let _gate = self.mutation.lock().await;
        let channel = self.channel()?;
        if intent
            .chat()
            .is_none_or(|chat| chat.host_id != self.id().as_str())
        {
            return Err("Chat operation target differs from this host.".into());
        }
        let meta = self.command(None)?;
        *intent.command_mut() = Some(meta.clone());
        let (kind, bytes) = intent.encode();
        if bytes.len() > 1024 * 1024 {
            return Err("Chat operation exceeds control limit; draft retained.".into());
        }
        self.update_record(|record| {
            if record.mutations.iter().any(|entry| entry.receipt.as_ref().and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok()).is_none_or(|receipt| !receipt.resolved_by_user && ![3,4,5,7].contains(&receipt.state))) {
                return Err("An earlier operation is unresolved. Review its result before another mutation.".into());
            }
            record.mutations.retain(|entry| entry.receipt.as_ref().and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok()).is_none_or(|receipt| !receipt.resolved_by_user && ![3,4,5,7].contains(&receipt.state)));
            if record.mutations.len() >= 100 { return Err("Operation recovery storage is full.".into()); }
            record.mutations.push(MutationRecord { id: meta.command_id.clone(), epoch: meta.command_epoch, receipt: None, chat_intent: Some((kind.into(), bytes)) });
            Ok(())
        })?;
        let resource = intent
            .chat()
            .map(|chat| chat.chat_id.clone())
            .unwrap_or_default();
        let started = std::time::Instant::now();
        let response = dispatch(intent, channel).await;
        crate::remote::metrics::record(
            self.id().as_str(),
            &crate::remote::metrics::resource_id(&resource),
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
        self.save_chat_receipt(&meta, receipt)
    }
    /// Explicit recovery only: Send retries its original typed RPC in the open
    /// epoch; legacy mutations use NOT_FOUND. Host dedupe fences every retry.
    pub async fn retry_chat_mutation(&self, id: &str) -> Result<p::CommandReceipt, String> {
        let _gate = self.mutation.lock().await;
        let record = self.snapshot().record;
        if record.closing_epoch.is_some() {
            return Err("Command epoch is closing.".into());
        }
        let entry = record
            .mutations
            .iter()
            .find(|entry| entry.id == id)
            .ok_or("Chat operation not found.")?;
        let (kind, bytes) = entry
            .chat_intent
            .as_ref()
            .ok_or("Operation has no replayable chat intent.")?;
        let mut intent = ChatMutation::decode(kind, bytes)?;
        let meta = intent
            .command_mut()
            .clone()
            .ok_or("Saved command identity is missing.")?;
        if meta.command_id != id
            || meta.command_epoch != entry.epoch
            || meta.host_id != self.id().as_str()
        {
            return Err("Saved command identity mismatch.".into());
        }
        let channel = self.channel()?;
        // Migrated Send has a per-thread inbox key. Legacy GetCommand cannot
        // disambiguate a UUID shared by threads; the original typed RPC can.
        if matches!(intent, ChatMutation::Send(_)) {
            if record.epoch != Some(meta.command_epoch) {
                return Err("Command epoch retired; intent retained without replay.".into());
            }
            let receipt = intent.dispatch(channel).await?;
            return self.save_chat_receipt(&meta, receipt);
        }
        let result = p::host_service_client::HostServiceClient::new(channel.clone())
            .get_command(p::GetCommandRequest {
                host_id: meta.host_id.clone(),
                command_id: id.into(),
                command_epoch: meta.command_epoch,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        self.validate_receipt(&meta, &result)?;
        let receipt = if result.state == p::OperationState::NotFound as i32 {
            if record.epoch != Some(meta.command_epoch) {
                return Err("Command epoch retired; intent retained without replay.".into());
            }
            intent.dispatch(channel).await?
        } else {
            result
        };
        self.save_chat_receipt(&meta, receipt)
    }
    fn save_chat_receipt(
        &self,
        meta: &p::CommandMeta,
        receipt: p::CommandReceipt,
    ) -> Result<p::CommandReceipt, String> {
        self.validate_receipt(meta, &receipt)?;
        self.update_record(|record| {
            let entry = record
                .mutations
                .iter_mut()
                .find(|entry| entry.id == meta.command_id)
                .ok_or("Chat operation journal disappeared.")?;
            entry.receipt = Some(receipt.encode_to_vec());
            Ok(())
        })?;
        self.changed();
        check_receipt(receipt)
    }
}
