//! Typed reads carry the host's native identities and projections unchanged.
use super::{HostClient, rpc_error};
use crate::api::proto as p;
pub struct ThreadRead<T>(tonic::Streaming<T>);
impl<T> ThreadRead<T> {
    pub async fn message(&mut self) -> Result<Option<T>, tonic::Status> {
        self.0.message().await
    }
}
impl HostClient {
    pub async fn thread_options(&self, thread: p::ThreadRef) -> Result<p::AgentOptions, String> {
        let result = p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .get_agent_options(p::GetAgentOptionsRequest {
                thread: Some(thread.clone()),
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        if result.thread.as_ref() != Some(&thread) {
            return Err("Agent options belong to another thread.".into());
        }
        Ok(result)
    }
    pub async fn subscribe_thread(
        &self,
        thread: p::ThreadRef,
        since: Option<p::Cursor>,
    ) -> Result<ThreadRead<p::ThreadEvent>, String> {
        Ok(ThreadRead(
            p::thread_service_client::ThreadServiceClient::new(self.channel()?)
                .subscribe_thread(p::SubscribeThreadRequest {
                    thread: Some(thread),
                    since,
                })
                .await
                .map_err(rpc_error)?
                .into_inner(),
        ))
    }
    pub async fn search_thread_history(
        &self,
        request: p::SearchHistoryRequest,
    ) -> Result<p::HistoryMatches, String> {
        p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .search_history(request)
            .await
            .map_err(rpc_error)
            .map(|r| r.into_inner())
    }
    pub async fn preview_thread_edit(
        &self,
        request: p::PreviewEditRequest,
    ) -> Result<p::EditPreview, String> {
        let thread = request.thread.clone();
        let result = p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .preview_edit(request)
            .await
            .map_err(rpc_error)?
            .into_inner();
        if result.thread != thread {
            return Err("Edit preview belongs to another thread.".into());
        }
        Ok(result)
    }
    pub(super) async fn read_thread_history(
        &self,
        request: p::ReadHistoryPageRequest,
    ) -> Result<ThreadRead<p::HistoryPageEvent>, String> {
        Ok(ThreadRead(
            p::thread_service_client::ThreadServiceClient::new(self.channel()?)
                .read_history_page(request)
                .await
                .map_err(rpc_error)?
                .into_inner(),
        ))
    }
}
