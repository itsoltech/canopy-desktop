//! Bounded upload/download transport backed by the existing durable private store.
mod v1;
use super::{
    auth::{RpcAuth, WorkspaceRoots, storage_status},
    resources::ResourceLocks,
};
use crate::artifacts::{ArtifactStore, Upload};
use std::sync::Arc;
use tonic::Status;

#[derive(Clone)]
pub struct ArtifactsRpc {
    auth: RpcAuth,
    roots: WorkspaceRoots,
    locks: ResourceLocks,
    store: Arc<ArtifactStore>,
    gate: Arc<tokio::sync::Mutex<()>>,
}
fn required<T>(value: Option<T>) -> Result<T, Status> {
    value.ok_or_else(|| Status::invalid_argument("Required artifact field is missing."))
}
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> crate::settings::Result<T> + Send + 'static,
) -> Result<T, Status> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Status::unavailable("Artifact worker stopped."))?
        .map_err(storage_status)
}
impl ArtifactsRpc {
    pub fn new(
        auth: RpcAuth,
        roots: WorkspaceRoots,
        locks: ResourceLocks,
        store: Arc<ArtifactStore>,
    ) -> Self {
        Self {
            auth,
            roots,
            locks,
            store,
            gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    async fn status(&self, device: &str, id: &str) -> Result<Upload, Status> {
        self.auth
            .client()
            .artifact_upload_status(device.to_owned(), id.to_owned())
            .await
            .map_err(storage_status)
    }
}
