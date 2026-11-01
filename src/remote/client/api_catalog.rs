//! Catalog aliases are resolved explicitly before constructing a native target.
use super::{HostClient, api_commands::ApiIntent, rpc_error};
use crate::api::proto as p;
use prost::Message;
impl HostClient {
    pub async fn close_project(
        &self,
        project: p::ProjectRef,
        plan_id: String,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            ApiIntent::CloseProject(p::CloseProjectRequest {
                command: None,
                project: Some(project),
                plan_id,
            }),
            String::new(),
            None,
        )
        .await
    }
    pub async fn close_terminal(
        &self,
        project: p::ProjectRef,
        terminal_id: String,
        plan_id: String,
    ) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            ApiIntent::CloseTerminal(p::CloseTerminalRequest {
                command: None,
                project: Some(project),
                terminal_id,
                plan_id,
            }),
            String::new(),
            None,
        )
        .await
    }
    pub async fn project_ref(&self, catalog_id: &str) -> Result<p::ProjectRef, String> {
        let mut stream = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .read_catalog(p::ReadCatalogRequest {
                environment: Some(p::EnvironmentRef {
                    host_id: self.id().as_str().into(),
                }),
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        let mut bytes = 0;
        let mut found = None;
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
                .is_none_or(|e| e.host_id != self.id().as_str())
            {
                return Err("Catalog host identity differs.".into());
            }
            for w in page.projects.into_iter().flat_map(|p| p.checkouts) {
                if w.catalog_id == catalog_id {
                    let project = w.project.ok_or("Project identity is missing.")?;
                    if project.host_id != self.id().as_str() || found.replace(project).is_some() {
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
    pub async fn list_agents(
        &self,
        request: p::ListAgentsRequest,
    ) -> Result<p::AgentDiscovery, String> {
        p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .list_agents(request)
            .await
            .map_err(rpc_error)
            .map(|r| r.into_inner())
    }
    pub async fn agent_catalog(
        &self,
        request: p::GetAgentCatalogRequest,
    ) -> Result<p::AgentCatalog, String> {
        p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .get_agent_catalog(request)
            .await
            .map_err(rpc_error)
            .map(|r| r.into_inner())
    }
    pub async fn list_pull_requests(
        &self,
        request: p::ListPullRequestsRequest,
    ) -> Result<p::PullRequests, String> {
        p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .list_pull_requests(request)
            .await
            .map_err(rpc_error)
            .map(|r| r.into_inner())
    }
    pub async fn create_thread(
        &self,
        project_catalog_id: Option<String>,
        agent: String,
        profile: Option<String>,
    ) -> Result<p::CommandReceipt, String> {
        let resource = match project_catalog_id {
            Some(id) => p::create_thread_request::Resource::Project(self.project_ref(&id).await?),
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
    pub async fn open_project(&self, host_path: String) -> Result<p::CommandReceipt, String> {
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
    pub async fn preflight_thread_close(&self, catalog_id: &str) -> Result<p::ClosePlan, String> {
        let target = self.thread_target(catalog_id).await?;
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
        catalog_id: String,
        plan: p::ClosePlan,
    ) -> Result<p::CommandReceipt, String> {
        let target = self.thread_target(&catalog_id).await?;
        if target.thread != plan.thread {
            return Err("The close confirmation belongs to another thread.".into());
        }
        self.mutate_api(
            ApiIntent::Close(p::CloseThreadRequest {
                command: None,
                target: Some(target),
                plan_id: plan.plan_id,
            }),
            catalog_id,
            None,
        )
        .await
    }
}
