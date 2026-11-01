//! Thread mutations use the same exact typed journal as every resource command.
use super::{ChatMutation, HostClient, commands::check_receipt};
use crate::api::proto as p;
use prost::Message;
impl ChatMutation {
    pub fn thread(&self) -> Option<&p::ThreadRef> {
        match self.resource()? {
            p::command_receipt::Resource::Target(_) => {}
            _ => return None,
        }
        match self {
            Self::Interrupt(v) => v.target.as_ref(),
            Self::Restart(v) => v.target.as_ref(),
            Self::Steer(v) => v.target.as_ref(),
            Self::Respond(v) => v.target.as_ref(),
            Self::Option(v) => v.target.as_ref(),
            Self::Agent(v) => v.target.as_ref(),
            Self::Goal(v) => v.target.as_ref(),
            Self::PauseGoal(v) => v.target.as_ref(),
            Self::ResumeGoal(v) => v.target.as_ref(),
            Self::ClearGoal(v) => v.target.as_ref(),
            Self::Background(v) => v.target.as_ref(),
            Self::Send(v) => v.target.as_ref(),
            Self::MessageEdit(v) => v.target.as_ref(),
            Self::Fork(v) => v.target.as_ref(),
            Self::Shell(v) => v.target.as_ref(),
            Self::StopShell(v) => v.target.as_ref(),
            Self::Edit(v) => v.target.as_ref(),
            Self::Move(v) => v.target.as_ref(),
            Self::Remove(v) => v.target.as_ref(),
            Self::Pause(v) => v.target.as_ref(),
            Self::Resolve(v) => v.target.as_ref(),
            _ => None,
        }
        .and_then(|t| t.thread.as_ref())
    }
}
impl HostClient {
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
        let thread = intent.thread().ok_or("Thread target is missing.")?;
        if thread.host_id != self.id().as_str() {
            return Err("Thread belongs to another host.".into());
        }
        self.mutate_api_with(intent.clone(), thread.thread_id.clone(), None, dispatch)
            .await
    }
    pub fn chat_mutations(
        &self,
        thread: &p::ThreadRef,
    ) -> Vec<(String, ChatMutation, Option<p::CommandReceipt>)> {
        self.snapshot()
            .record
            .mutations
            .into_iter()
            .filter_map(|r| {
                let (kind, bytes) = r.api_intent?;
                let intent = ChatMutation::decode(&kind, &bytes).ok()?;
                (intent.thread() == Some(thread)).then(|| {
                    (
                        r.id,
                        intent,
                        r.receipt
                            .and_then(|b| p::CommandReceipt::decode(b.as_slice()).ok()),
                    )
                })
            })
            .collect()
    }
    pub async fn submit_chat_mutation(
        &self,
        intent: ChatMutation,
    ) -> Result<p::CommandReceipt, String> {
        let thread = intent.thread().ok_or("Thread target is missing.")?;
        if thread.host_id != self.id().as_str() {
            return Err("Thread belongs to another host.".into());
        }
        let id = thread.thread_id.clone();
        self.mutate_api(intent, id, None).await
    }
    pub async fn retry_chat_mutation(&self, id: &str) -> Result<p::CommandReceipt, String> {
        let _gate = self.mutation.lock().await;
        let record = self.snapshot().record;
        if record.closing_epoch.is_some() {
            return Err("Command epoch is closing.".into());
        }
        let entry = record
            .mutations
            .iter()
            .find(|r| r.id == id)
            .ok_or("Operation not found.")?;
        let receipt = self.retry_api(entry, String::new()).await?;
        self.store_api_receipt(id, &receipt)?;
        check_receipt(receipt)
    }
}
