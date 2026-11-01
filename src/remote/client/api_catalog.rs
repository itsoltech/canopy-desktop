//! Catalog adapters preserve the desktop model while using standalone references.
use super::{HostClient, api_commands::ApiIntent, rpc_error};
use crate::{
    api::{projection::value, proto as p},
    remote::proto as old,
};
use prost::Message;

impl HostClient {
    pub async fn project_ref(&self, workspace: old::WorkspaceRef) -> Result<p::ProjectRef, String> {
        if workspace.host_id != self.id().as_str() {
            return Err("Project belongs to another host.".into());
        }
        let mut stream = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .read_catalog(p::ReadCatalogRequest {
                environment: Some(p::EnvironmentRef {
                    host_id: workspace.host_id.clone(),
                }),
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        let mut bytes = 0;
        let mut found = None;
        // Finish the snapshot so an interrupted/replaced catalog cannot choose a
        // target. Mutation recovery retains the result of this discovery verbatim.
        while let Some(event) = stream.message().await.map_err(rpc_error)? {
            bytes += event.encoded_len();
            if bytes > super::catalog::MAX_CATALOG_BYTES {
                return Err("Project catalog exceeds the client budget.".into());
            }
            let Some(p::summary_event::Change::SnapshotPart(part)) = event.change else {
                return Err("Unexpected project catalog event.".into());
            };
            let page = part.page.ok_or("Missing catalog page.")?;
            if page
                .environment
                .as_ref()
                .is_none_or(|e| e.host_id != workspace.host_id)
            {
                return Err("Catalog host identity differs.".into());
            }
            for w in page.projects.into_iter().flat_map(|p| p.checkouts) {
                if w.catalog_id == workspace.workspace_id {
                    let project = w.project.ok_or("Project identity is missing.")?;
                    if project.host_id != workspace.host_id || found.replace(project).is_some() {
                        return Err("Project discovery is ambiguous.".into());
                    }
                }
            }
            if part.last {
                return found.ok_or("Project checkout is no longer available.".into());
            }
        }
        Err("The project catalog ended before its boundary.".into())
    }
    async fn agent_project(
        &self,
        host_id: &str,
        workspace: Option<old::WorkspaceRef>,
    ) -> Result<Option<p::ProjectRef>, String> {
        if host_id != self.id().as_str() {
            return Err("Agent request belongs to another host.".into());
        }
        match workspace {
            Some(w) => self.project_ref(w).await.map(Some),
            None => Ok(None),
        }
    }
    pub async fn list_agents(
        &self,
        request: old::AgentDiscoveryRequest,
    ) -> Result<old::AgentDiscovery, String> {
        let project = self
            .agent_project(&request.host_id, request.workspace)
            .await?;
        let result = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .list_agents(p::ListAgentsRequest {
                environment: Some(p::EnvironmentRef {
                    host_id: request.host_id,
                }),
                project,
                page_token: request.page_token,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        Ok(value(result))
    }
    pub async fn agent_catalog(
        &self,
        request: old::AgentCatalogRequest,
    ) -> Result<old::AgentCatalog, String> {
        let project = self
            .agent_project(&request.host_id, request.workspace)
            .await?;
        let result = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .get_agent_catalog(p::GetAgentCatalogRequest {
                environment: Some(p::EnvironmentRef {
                    host_id: request.host_id,
                }),
                project,
                agent_id: request.agent_id,
                account_id: request.account_id,
                refresh: request.refresh,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        Ok(value(result))
    }
    pub async fn list_pull_requests(
        &self,
        request: old::PullRequestsRequest,
    ) -> Result<old::PullRequests, String> {
        let project = self
            .project_ref(request.workspace.ok_or("Project is missing.")?)
            .await?;
        let result = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .list_pull_requests(p::ListPullRequestsRequest {
                project: Some(project),
                query: request.query,
                page_token: request.page_token,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        Ok(value(result))
    }
    pub async fn create_thread(
        &self,
        workspace: Option<old::WorkspaceRef>,
        agent: String,
        profile: Option<String>,
    ) -> Result<old::CommandReceipt, String> {
        let resource = match workspace {
            Some(w) => p::create_thread_request::Resource::Project(self.project_ref(w).await?),
            None => p::create_thread_request::Resource::Environment(p::EnvironmentRef {
                host_id: self.id().as_str().into(),
            }),
        };
        self.mutate_api(
            ApiIntent::Create(p::CreateThreadRequest {
                command: None,
                resource: Some(resource),
                agent_id: agent,
                profile_id: profile.ok_or("Choose the account for the new chat.")?,
            }),
            String::new(),
            None,
        )
        .await
    }
    pub async fn open_project(&self, host_path: String) -> Result<old::CommandReceipt, String> {
        self.mutate_api(
            ApiIntent::OpenProject(p::OpenProjectRequest {
                command: None,
                environment: Some(p::EnvironmentRef {
                    host_id: self.id().as_str().into(),
                }),
                host_path,
            }),
            String::new(),
            None,
        )
        .await
    }
    pub async fn preflight_thread_close(&self, chat: old::ChatRef) -> Result<p::ClosePlan, String> {
        let target = self.thread_target(&chat).await?;
        p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .preflight_close(p::PreflightCloseRequest {
                thread: target.thread,
            })
            .await
            .map_err(rpc_error)
            .map(|r| r.into_inner())
    }
    pub async fn close_thread(
        &self,
        chat: old::ChatRef,
        plan: p::ClosePlan,
    ) -> Result<old::CommandReceipt, String> {
        let target = self.thread_target(&chat).await?;
        if target.thread != plan.thread {
            return Err("The close confirmation belongs to another thread.".into());
        }
        self.mutate_api(
            ApiIntent::Close(p::CloseThreadRequest {
                command: None,
                target: Some(target),
                plan_id: plan.plan_id,
            }),
            chat.chat_id,
            None,
        )
        .await
    }
}
