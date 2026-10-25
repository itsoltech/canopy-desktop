//! Native administrative settings. The live owner applies patches to current state.
mod model;
mod secrets;
mod writes;
use super::{
    auth::{RpcAuth, storage_status},
    proto,
    resources::ResourceLocks,
};
use crate::host::ADMIN;
pub use model::{
    AgentEdit, IntegrationEdit, PreferencesPatch, ProfileEdit, ProfileOptions, ProfileSettingsEdit,
    SecretChange, SecretVersion,
};
pub use secrets::{PlatformSettingsSecrets, SettingsSecrets};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Implement on the live app bridge. Resolve staging values using SettingsSecrets,
/// then reuse owner save APIs; never persist a stale load/modify/save Config.
/// Success means durable configuration commit, not merely enqueueing a UI action.
/// Check expected_revision atomically in the live owner's serialized mutation,
/// without an await gap between checking the version and applying the patch.
#[tonic::async_trait]
pub trait SettingsHost: Send + Sync + 'static {
    async fn snapshot(&self) -> Result<proto::HostSettings, Status>;
    async fn update_preferences(
        &self,
        patch: PreferencesPatch,
        expected_revision: Option<u64>,
    ) -> Result<(), Status>;
    async fn save_agent(
        &self,
        edit: AgentEdit,
        expected_revision: Option<u64>,
    ) -> Result<(), Status>;
    async fn remove_agent(&self, id: String, expected_revision: Option<u64>) -> Result<(), Status>;
    async fn save_profile(
        &self,
        edit: ProfileEdit,
        expected_revision: Option<u64>,
    ) -> Result<(), Status>;
    async fn remove_profile(
        &self,
        id: String,
        expected_revision: Option<u64>,
    ) -> Result<(), Status>;
    async fn remove_integration(
        &self,
        id: String,
        expected_revision: Option<u64>,
    ) -> Result<(), Status>;
    async fn save_integration(
        &self,
        edit: IntegrationEdit,
        expected_revision: Option<u64>,
    ) -> Result<(), Status>;
}
#[derive(Clone)]
pub struct SettingsRpc {
    auth: RpcAuth,
    locks: ResourceLocks,
    host: Arc<dyn SettingsHost>,
    secrets: Arc<dyn SettingsSecrets>,
    gate: Arc<tokio::sync::Mutex<()>>,
}
impl SettingsRpc {
    pub fn new(
        auth: RpcAuth,
        locks: ResourceLocks,
        host: Arc<dyn SettingsHost>,
        secrets: Arc<dyn SettingsSecrets>,
    ) -> Self {
        Self {
            auth,
            locks,
            host,
            secrets,
            gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    async fn authorize<T>(&self, r: &Request<T>) -> Result<crate::host::DeviceGrant, Status> {
        let g = self.auth.authenticate(r).await?;
        self.auth.resource(g.id(), ADMIN, "private").await?;
        Ok(g)
    }
    /// Periodic host maintenance; shares admission, ownership and staging gate.
    /// Unknown/accepted effects retain their immutable versions.
    pub async fn cleanup(&self) -> Result<(), Status> {
        let operation = self.locks.begin()?;
        let this = self.clone();
        tokio::spawn(async move {
            let _operation = operation;
            let _gate = this.gate.lock().await;
            this.cleanup_locked().await
        })
        .await
        .map_err(|_| Status::unavailable("Secret cleanup worker stopped."))?
    }
    async fn cleanup_locked(&self) -> Result<(), Status> {
        for id in self
            .auth
            .client()
            .collect_remote_secrets()
            .await
            .map_err(storage_status)?
        {
            self.secrets.remove(&SecretVersion::new(id.clone())).await?;
            self.auth
                .client()
                .forget_remote_secret(id)
                .await
                .map_err(storage_status)?;
        }
        Ok(())
    }
}
#[tonic::async_trait]
impl proto::settings_service_server::SettingsService for SettingsRpc {
    async fn get_settings(
        &self,
        r: Request<proto::HostRequest>,
    ) -> Result<Response<proto::HostSettings>, Status> {
        let grant = self.authorize(&r).await?;
        self.auth.check_host(&r.get_ref().host_id)?;
        let mut snapshot = self.host.snapshot().await?;
        self.auth.resource(grant.id(), ADMIN, "private").await?;
        snapshot.host_id = self.auth.host_id().into();
        for profile in &mut snapshot.profiles {
            if let Some(options) = &mut profile.settings
                && options
                    .base_url
                    .as_ref()
                    .is_some_and(|url| writes::validate_url(url).is_err())
            {
                options.base_url = None;
            }
        }
        for integration in &mut snapshot.integrations {
            if writes::validate_url(&integration.endpoint).is_err() {
                integration.endpoint.clear();
            }
        }

        if prost::Message::encoded_len(&snapshot) > crate::host::CONTROL_BYTES {
            return Err(Status::resource_exhausted(
                "Settings snapshot exceeds control limit.",
            ));
        }
        Ok(Response::new(snapshot))
    }
    async fn update_preferences(
        &self,
        r: Request<proto::UpdatePreferencesRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.authorize(&r).await?;
        self.write(g, writes::Write::Preferences(r.into_inner()))
            .await
            .map(Response::new)
    }
    async fn save_agent_definition(
        &self,
        r: Request<proto::SaveAgentDefinitionRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.authorize(&r).await?;
        self.write(g, writes::Write::Agent(r.into_inner()))
            .await
            .map(Response::new)
    }
    async fn remove_agent_definition(
        &self,
        r: Request<proto::RemoveAgentDefinitionRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.authorize(&r).await?;
        self.write(g, writes::Write::RemoveAgent(r.into_inner()))
            .await
            .map(Response::new)
    }
    async fn save_profile(
        &self,
        r: Request<proto::SaveProfileRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.authorize(&r).await?;
        self.write(g, writes::Write::profile(r.into_inner()))
            .await
            .map(Response::new)
    }
    async fn remove_profile(
        &self,
        r: Request<proto::RemoveProfileRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.authorize(&r).await?;
        self.write(g, writes::Write::RemoveProfile(r.into_inner()))
            .await
            .map(Response::new)
    }
    async fn remove_integration(
        &self,
        r: Request<proto::RemoveIntegrationRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.authorize(&r).await?;
        self.write(g, writes::Write::RemoveIntegration(r.into_inner()))
            .await
            .map(Response::new)
    }
    async fn save_integration(
        &self,
        r: Request<proto::SaveIntegrationRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let g = self.authorize(&r).await?;
        self.write(g, writes::Write::Integration(r.into_inner()))
            .await
            .map(Response::new)
    }
}

#[cfg(test)]
mod tests;
