//! App-owned catalog projections and lifecycle callbacks. The transport never
//! creates another catalog, lifecycle worker or command inbox.
use super::{proto as p, rpc::ThreadRpc};
use crate::remote::host::RpcStream;
use std::sync::Arc;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
pub trait CatalogPublisher: Send + Sync + 'static {
    async fn read_catalog(
        &self,
        request: Request<p::ReadCatalogRequest>,
    ) -> Result<Response<RpcStream<p::SummaryEvent>>, Status>;
    async fn list_resources(
        &self,
        request: Request<p::ListResourcesRequest>,
    ) -> Result<Response<p::Catalog>, Status>;
    async fn watch_summaries(
        &self,
        request: Request<p::WatchSummariesRequest>,
    ) -> Result<Response<RpcStream<p::SummaryEvent>>, Status>;
    async fn list_agents(
        &self,
        request: Request<p::ListAgentsRequest>,
    ) -> Result<Response<p::AgentDiscovery>, Status>;
    async fn get_agent_catalog(
        &self,
        request: Request<p::GetAgentCatalogRequest>,
    ) -> Result<Response<p::AgentCatalog>, Status>;
    async fn list_pull_requests(
        &self,
        request: Request<p::ListPullRequestsRequest>,
    ) -> Result<Response<p::PullRequests>, Status>;
    async fn read_thread_page(
        &self,
        request: Request<p::ReadThreadPageRequest>,
    ) -> Result<Response<RpcStream<p::ArtifactChunk>>, Status>;
    async fn preflight_close(
        &self,
        request: Request<p::PreflightCloseRequest>,
    ) -> Result<Response<p::ClosePlan>, Status>;
    async fn create_thread(
        &self,
        request: Request<p::CreateThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status>;
    async fn close_thread(
        &self,
        request: Request<p::CloseThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status>;
    async fn open_project(
        &self,
        request: Request<p::OpenProjectRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status>;
}
impl ThreadRpc {
    pub fn with_catalog(mut self, catalog: Arc<dyn CatalogPublisher>) -> Self {
        self.catalog = Some(catalog);
        self
    }
    pub(super) fn catalog(&self) -> Result<&dyn CatalogPublisher, Status> {
        self.catalog
            .as_deref()
            .ok_or_else(|| Status::unavailable("Catalog owner is unavailable."))
    }
}
