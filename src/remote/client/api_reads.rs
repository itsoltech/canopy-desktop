//! Typed thread reads, projected into the existing desktop model at this boundary.
use super::{HostClient, rpc_error};
use crate::{
    api::{
        projection::{self as mapping, Location},
        proto as p,
    },
    remote::proto as old,
};

pub struct ThreadRead<T>(crate::remote::host::RpcStream<T>);
impl<T> ThreadRead<T> {
    pub async fn message(&mut self) -> Result<Option<T>, tonic::Status> {
        use futures_lite::StreamExt;
        self.0.next().await.transpose()
    }
}
impl HostClient {
    async fn api_location(&self, chat: &old::ChatRef) -> Result<Location, String> {
        let thread = self
            .thread_target(chat)
            .await?
            .thread
            .ok_or("Thread address is missing.")?;
        let summary = self
            .cached_chat(&chat.chat_id)
            .and_then(|c| c.summary)
            .or_else(|| {
                self.snapshot()
                    .catalog
                    .and_then(|c| c.chats.into_iter().find(|s| s.chat.as_ref() == Some(chat)))
            });
        Ok(Location {
            thread,
            catalog: chat.clone(),
            workspace: summary.and_then(|s| s.workspace),
        })
    }
    pub async fn thread_options(&self, chat: old::ChatRef) -> Result<old::AgentOptions, String> {
        let location = self.api_location(&chat).await?;
        let result = p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .get_agent_options(p::GetAgentOptionsRequest {
                thread: Some(location.thread.clone()),
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        if result.thread.as_ref() != Some(&location.thread) {
            return Err("Agent options belong to another thread.".into());
        }
        Ok(old::AgentOptions {
            chat: Some(chat),
            options: result.options.into_iter().map(mapping::value).collect(),
            capabilities: result
                .capabilities
                .into_iter()
                .map(mapping::value)
                .collect(),
        })
    }
    pub async fn subscribe_thread(
        &self,
        chat: old::ChatRef,
        since: Option<old::Cursor>,
    ) -> Result<ThreadRead<old::ChatEvent>, String> {
        let location = self.api_location(&chat).await?;
        let stream = p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .subscribe_thread(p::SubscribeThreadRequest {
                thread: Some(location.thread.clone()),
                since: since.map(mapping::value),
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        Ok(ThreadRead(mapping::legacy_thread_stream(
            location,
            Box::pin(stream),
        )))
    }
    pub async fn search_thread_history(
        &self,
        request: old::SearchHistoryRequest,
    ) -> Result<old::HistoryMatches, String> {
        let chat = request.chat.ok_or("Thread is missing.")?;
        let location = self.api_location(&chat).await?;
        let result = p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .search_history(p::SearchHistoryRequest {
                thread: Some(location.thread),
                query: request.text,
                page_token: request.page_token,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        Ok(old::HistoryMatches {
            chat: Some(chat),
            matches: result
                .matches
                .into_iter()
                .map(|m| old::HistoryMatch {
                    key: m.item.map(mapping::value),
                    excerpt: m.excerpt,
                })
                .collect(),
            next_page_token: result.next_page_token,
            incomplete: result.incomplete,
        })
    }
    pub async fn preview_thread_edit(
        &self,
        request: old::PreviewEditRequest,
    ) -> Result<old::EditPreview, String> {
        let location = self
            .api_location(request.chat.as_ref().ok_or("Thread is missing.")?)
            .await?;
        let result = p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .preview_edit(p::PreviewEditRequest {
                thread: Some(location.thread.clone()),
                run_id: request
                    .run_id
                    .parse()
                    .map_err(|_| "Invalid run identity.")?,
                item: request.item.map(mapping::value),
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        if result.thread.as_ref() != Some(&location.thread) {
            return Err("Edit preview belongs to another thread.".into());
        }
        Ok(old::EditPreview {
            chat: Some(location.catalog.clone()),
            run_id: result.run_id.to_string(),
            item: result.item.map(mapping::value),
            editable: result.editable,
            reason: result.reason,
            first_message: result.first_message,
            original: result
                .original
                .map(|d| location.legacy_draft(d))
                .transpose()
                .map_err(rpc_error)?,
            files: result.files.map(mapping::value),
        })
    }
    pub(super) async fn read_thread_history(
        &self,
        request: old::HistoryPageRequest,
    ) -> Result<ThreadRead<old::HistoryPageEvent>, String> {
        let location = self
            .api_location(request.chat.as_ref().ok_or("Thread is missing.")?)
            .await?;
        let stream = p::thread_service_client::ThreadServiceClient::new(self.channel()?)
            .read_history_page(p::ReadHistoryPageRequest {
                thread: Some(location.thread.clone()),
                page_token: request.page_token,
                limit: request.limit,
                around: request.around.map(mapping::value),
                before: request.before.map(mapping::value),
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        Ok(ThreadRead(mapping::legacy_history_stream(
            location,
            Box::pin(stream),
        )))
    }
}
