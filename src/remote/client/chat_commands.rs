use super::{
    HostClient,
    commands::{check_receipt, terminal_state},
    store::MutationRecord,
};
use crate::remote::proto as p;
use prost::Message;

/// Non-secret chat intent retained by the device until its outcome is known.
#[derive(Clone, Debug)]
pub enum ChatMutation {
    Interrupt(p::ChatCommandRequest),
    Restart(p::ChatCommandRequest),
    Steer(p::SteerRequest),
    Respond(p::RespondDecisionRequest),
    Option(p::SetAgentOptionRequest),
    Agent(p::SetChatAgentRequest),
    Goal(p::SetGoalRequest),
    Background(p::StopBackgroundRequest),
    Send(p::SendMessageRequest),
    Edit(p::EditMessageRequest),
    Fork(p::ForkChatRequest),
    RunShell(p::RunShellRequest),
    StopShell(p::StopShellRequest),
}
impl ChatMutation {
    fn command_mut(&mut self) -> &mut Option<p::CommandMeta> {
        match self {
            Self::Interrupt(v) | Self::Restart(v) => &mut v.command,
            Self::Steer(v) => &mut v.command,
            Self::Respond(v) => &mut v.command,
            Self::Option(v) => &mut v.command,
            Self::Agent(v) => &mut v.command,
            Self::Goal(v) => &mut v.command,
            Self::Background(v) => &mut v.command,
            Self::Send(v) => &mut v.command,
            Self::Edit(v) => &mut v.command,
            Self::Fork(v) => &mut v.command,
            Self::RunShell(v) => &mut v.command,
            Self::StopShell(v) => &mut v.command,
        }
    }
    pub fn chat(&self) -> Option<&p::ChatRef> {
        match self {
            Self::Interrupt(v) | Self::Restart(v) => v.chat.as_ref(),
            Self::Steer(v) => v.chat.as_ref(),
            Self::Respond(v) => v.chat.as_ref(),
            Self::Option(v) => v.chat.as_ref(),
            Self::Agent(v) => v.chat.as_ref(),
            Self::Goal(v) => v.chat.as_ref(),
            Self::Background(v) => v.chat.as_ref(),
            Self::Send(v) => v.chat.as_ref(),
            Self::Edit(v) => v.chat.as_ref(),
            Self::Fork(v) => v.chat.as_ref(),
            Self::RunShell(v) => v.chat.as_ref(),
            Self::StopShell(v) => v.chat.as_ref(),
        }
    }
    fn encode(&self) -> (&'static str, Vec<u8>) {
        match self {
            Self::Interrupt(v) => ("interrupt", v.encode_to_vec()),
            Self::Restart(v) => ("restart", v.encode_to_vec()),
            Self::Steer(v) => ("steer", v.encode_to_vec()),
            Self::Respond(v) => ("respond", v.encode_to_vec()),
            Self::Option(v) => ("option", v.encode_to_vec()),
            Self::Agent(v) => ("agent", v.encode_to_vec()),
            Self::Goal(v) => ("goal", v.encode_to_vec()),
            Self::Background(v) => ("background", v.encode_to_vec()),
            Self::Send(v) => ("send", v.encode_to_vec()),
            Self::Edit(v) => ("edit", v.encode_to_vec()),
            Self::Fork(v) => ("fork", v.encode_to_vec()),
            Self::RunShell(v) => ("shell", v.encode_to_vec()),
            Self::StopShell(v) => ("stop_shell", v.encode_to_vec()),
        }
    }
    fn decode(kind: &str, bytes: &[u8]) -> Result<Self, String> {
        match kind {
            "interrupt" => p::ChatCommandRequest::decode(bytes).map(Self::Interrupt),
            "restart" => p::ChatCommandRequest::decode(bytes).map(Self::Restart),
            "steer" => p::SteerRequest::decode(bytes).map(Self::Steer),
            "respond" => p::RespondDecisionRequest::decode(bytes).map(Self::Respond),
            "option" => p::SetAgentOptionRequest::decode(bytes).map(Self::Option),
            "agent" => p::SetChatAgentRequest::decode(bytes).map(Self::Agent),
            "goal" => p::SetGoalRequest::decode(bytes).map(Self::Goal),
            "background" => p::StopBackgroundRequest::decode(bytes).map(Self::Background),
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
        let _ = (self, channel);
        Err("This operation has no original typed thread address. Retain it for review without replay.".into())
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
        let api = Some(self.api_chat(&intent).await?);
        let host = self.id().as_str().to_owned();
        self.submit_chat_mutation_prepared(intent, api, move |intent, api, channel| async move {
            if let Some(api) = api {
                let catalog_id = intent.chat().map(|c| c.chat_id.clone()).unwrap_or_default();
                api.dispatch(channel, &host, catalog_id).await
            } else {
                intent.dispatch(channel).await
            }
        })
        .await
    }
    #[cfg(test)]
    pub(super) async fn submit_chat_mutation_with<F, Fut>(
        &self,
        intent: ChatMutation,
        dispatch: F,
    ) -> Result<p::CommandReceipt, String>
    where
        F: FnOnce(ChatMutation, tonic::transport::Channel) -> Fut,
        Fut: std::future::Future<Output = Result<p::CommandReceipt, String>>,
    {
        self.submit_chat_mutation_prepared(intent, None, move |intent, _, channel| {
            dispatch(intent, channel)
        })
        .await
    }
    pub(super) async fn submit_chat_mutation_prepared<F, Fut>(
        &self,
        mut intent: ChatMutation,
        mut api: Option<super::api_commands::ApiIntent>,
        dispatch: F,
    ) -> Result<p::CommandReceipt, String>
    where
        F: FnOnce(
            ChatMutation,
            Option<super::api_commands::ApiIntent>,
            tonic::transport::Channel,
        ) -> Fut,
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
        if let Some(api) = &mut api {
            *api.meta() = Some(crate::api::compat::command(meta.clone()));
        }
        let api_intent = api.as_ref().map(|intent| {
            let (kind, bytes) = intent.encode();
            (kind.into(), bytes)
        });
        let (kind, bytes) = intent.encode();
        if bytes.len() > 1024 * 1024
            || api_intent
                .as_ref()
                .is_some_and(|(_, bytes)| bytes.len() > crate::host::CONTROL_BYTES)
        {
            return Err("Chat operation exceeds control limit; draft retained.".into());
        }
        self.update_record(|record| {
            if record.mutations.iter().any(|entry| entry.receipt.as_ref().and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok()).is_none_or(|receipt| !receipt.resolved_by_user && !terminal_state(receipt.state))) {
                return Err("An earlier operation is unresolved. Review its result before another mutation.".into());
            }
            record.mutations.retain(|entry| entry.receipt.as_ref().and_then(|bytes| p::CommandReceipt::decode(bytes.as_slice()).ok()).is_none_or(|receipt| !receipt.resolved_by_user && !terminal_state(receipt.state)));
            if record.mutations.len() >= 100 { return Err("Operation recovery storage is full.".into()); }
            record.mutations.push(MutationRecord { id: meta.command_id.clone(), epoch: meta.command_epoch, receipt: None, chat_intent: Some((kind.into(), bytes)), api_intent });
            Ok(())
        })?;
        let resource = intent
            .chat()
            .map(|chat| chat.chat_id.clone())
            .unwrap_or_default();
        let started = std::time::Instant::now();
        let response = dispatch(intent, api, channel).await;
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
    /// Explicit recovery repeats the saved typed RPC without target discovery.
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
        if entry.api_intent.is_some() {
            let catalog_id = entry
                .chat_intent
                .as_ref()
                .and_then(|(kind, bytes)| ChatMutation::decode(kind, bytes).ok())
                .and_then(|intent| intent.chat().map(|chat| chat.chat_id.clone()))
                .unwrap_or_default();
            let receipt = self.retry_api(entry, catalog_id).await?;
            self.store_api_receipt(id, &receipt)?;
            return check_receipt(receipt);
        }
        Err("This saved operation has no original thread address. Retain it for review; it cannot be safely replayed.".into())
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
