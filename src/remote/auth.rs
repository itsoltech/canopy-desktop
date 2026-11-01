//! The verified TLS peer, not request metadata or a device name, selects authority.
use crate::api::proto;
use crate::{
    host::{CommandInput, CommandKey, CommandReceipt, DeviceGrant},
    settings::{Error, SettingsClient},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use tonic::{Request, Status};

#[derive(Clone)]
pub struct RpcAuth {
    host: Arc<str>,
    client: SettingsClient,
}
impl RpcAuth {
    pub fn new(host: String, client: SettingsClient) -> Result<Self, Status> {
        if uuid::Uuid::parse_str(&host).is_err() {
            return Err(Status::invalid_argument("Invalid host identity."));
        }
        Ok(Self {
            host: host.into(),
            client,
        })
    }
    pub fn host_id(&self) -> &str {
        &self.host
    }
    pub fn client(&self) -> &SettingsClient {
        &self.client
    }
    pub async fn authenticate<T>(&self, request: &Request<T>) -> Result<DeviceGrant, Status> {
        let certs = request
            .peer_certs()
            .ok_or_else(|| Status::unauthenticated("A paired device certificate is required."))?;
        let leaf = certs
            .first()
            .ok_or_else(|| Status::unauthenticated("A paired device certificate is required."))?;
        self.client
            .lookup_runtime_device(leaf.to_vec())
            .await
            .map_err(storage_status)
    }
    pub fn check_host(&self, host: &str) -> Result<(), Status> {
        if host != self.host.as_ref() {
            return Err(Status::not_found(
                "The requested host is not this endpoint.",
            ));
        }
        Ok(())
    }
    pub fn scope(&self, grant: &DeviceGrant, scope: u32) -> Result<(), Status> {
        if grant.revoked() || grant.scopes() & scope != scope {
            return Err(Status::permission_denied(
                "The device does not have this permission.",
            ));
        }
        Ok(())
    }
    pub async fn resource(
        &self,
        device: &str,
        scope: u32,
        root: &str,
    ) -> Result<DeviceGrant, Status> {
        self.client
            .authorize_runtime_read(device.into(), scope, root.into())
            .await
            .map_err(storage_status)
    }
    pub fn key(
        &self,
        meta: &proto::CommandMeta,
        device: &DeviceGrant,
    ) -> Result<CommandKey, Status> {
        CommandKey::remote(
            device.id().into(),
            meta.command_epoch,
            meta.command_id.clone(),
        )
        .ok_or_else(|| Status::invalid_argument("Invalid command identity."))
    }
    /// A feature supplies a normalized typed encoding. Secret-bearing feature
    /// handlers use immutable vault references, never plaintext RPC arguments.
    pub fn input(
        &self,
        meta: &proto::CommandMeta,
        device: &DeviceGrant,
        method: &str,
        target: String,
        arguments: Vec<u8>,
        root: String,
    ) -> Result<CommandInput, Status> {
        let hash = Sha256::digest(&arguments).to_vec();
        CommandInput::new(
            self.key(meta, device)?,
            method.into(),
            target,
            arguments,
            hash,
        )
        .and_then(|input| input.with_root(root))
        .ok_or_else(|| Status::invalid_argument("Invalid command arguments."))
    }
    pub fn receipt(&self, receipt: &CommandReceipt) -> Result<proto::CommandReceipt, Status> {
        crate::api::project_receipt(self, receipt, None)
    }
}
/// Host-resolved roots only. Registering a root does not grant a device access.
#[derive(Clone, Default)]
pub struct WorkspaceRoots {
    catalog: Arc<RwLock<WorkspaceCatalog>>,
}
#[derive(Default)]
struct WorkspaceCatalog {
    roots: HashMap<String, PathBuf>,
    projects: Vec<crate::state::projects::Project>,
}
impl WorkspaceRoots {
    /// Authoritative identities and Git metadata supplied by the catalog owner.
    /// No resource request infers identity from a filesystem path.
    pub fn replace_projects(
        &self,
        projects: Vec<crate::state::projects::Project>,
    ) -> Result<(), Status> {
        let roots = projects
            .iter()
            .map(|p| {
                serde_json::to_string(&p.workspace)
                    .map(|id| (id, p.worktree_path.as_ref().unwrap_or(&p.path).clone()))
                    .map_err(|_| Status::internal("Invalid checkout metadata."))
            })
            .collect::<Result<_, _>>()?;
        validate_roots(&roots)?;
        *self
            .catalog
            .write()
            .map_err(|_| Status::internal("Project registry is unavailable."))? =
            WorkspaceCatalog { roots, projects };
        Ok(())
    }
    pub fn project(
        &self,
        reference: crate::api::proto::ProjectRef,
        auth: &RpcAuth,
    ) -> Result<(crate::thread::ProjectRef, PathBuf), Status> {
        auth.check_host(&reference.host_id)?;
        let catalog = self
            .catalog
            .read()
            .map_err(|_| Status::internal("Project registry is unavailable."))?;
        let record = catalog
            .projects
            .iter()
            .find(|p| {
                serde_json::to_string(&p.id).ok().as_deref() == Some(reference.project_id.as_str())
                    && reference.worktree_id.as_ref().is_none_or(|id| {
                        serde_json::to_string(&p.workspace).ok().as_ref() == Some(id)
                    })
            })
            .ok_or_else(|| Status::not_found("Project checkout is unavailable."))?;
        let host = crate::thread::HostId::new(auth.host_id().into())
            .ok_or_else(|| Status::internal("Invalid host identity."))?;
        let checked = crate::api::checked_project_ref(reference, &host, record)?;
        Ok((
            checked,
            record
                .worktree_path
                .as_ref()
                .unwrap_or(&record.path)
                .clone(),
        ))
    }
    /// Bus lookup after admission verified the host and current resource grant.
    pub fn workspace_id(&self, root: &Path) -> Result<String, Status> {
        self.catalog
            .read()
            .map_err(|_| Status::internal("Project registry is unavailable."))?
            .roots
            .iter()
            .find(|(_, path)| path.as_path() == root)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| Status::not_found("Project checkout is unavailable."))
    }
    /// Bus lookup after admission verified the host and current resource grant.
    pub fn resolve_host_workspace(&self, workspace: &str) -> crate::settings::Result<PathBuf> {
        self.catalog
            .read()
            .map_err(|_| Error::InvalidRuntimeState)?
            .roots
            .get(workspace)
            .cloned()
            .ok_or(Error::NotFound)
    }
    /// The owner supplies already canonical paths, prepared outside render.
    pub fn replace(&self, roots: HashMap<String, PathBuf>) -> Result<(), Status> {
        validate_roots(&roots)?;
        *self
            .catalog
            .write()
            .map_err(|_| Status::internal("Workspace registry is unavailable."))? =
            WorkspaceCatalog {
                roots,
                projects: Vec::new(),
            };
        Ok(())
    }
    pub fn root_string(path: &Path) -> Result<&str, Status> {
        path.to_str().ok_or_else(|| {
            Status::invalid_argument("Workspace path cannot be represented by the protocol.")
        })
    }
}
fn validate_roots(roots: &HashMap<String, PathBuf>) -> Result<(), Status> {
    if roots
        .iter()
        .any(|(id, path)| id.is_empty() || id.len() > 256 || !path.is_absolute())
    {
        return Err(Status::invalid_argument("Invalid workspace roots."));
    }
    Ok(())
}
pub fn storage_status(error: Error) -> Status {
    match error {
        Error::NotAuthorized => {
            Status::permission_denied("The device is not authorized for this operation.")
        }
        Error::NotFound => Status::not_found("The resource was not found."),
        Error::ResourceConflict => Status::aborted(error.to_string()),
        Error::CommandConflict => {
            Status::aborted("idempotency_conflict: this command ID was used for another operation.")
        }
        Error::AmbiguousCommand => Status::failed_precondition(error.to_string()),
        Error::ResourceLimit => Status::resource_exhausted(error.to_string()),
        Error::CommandRetired => Status::failed_precondition(error.to_string()),
        Error::OperationPending => Status::failed_precondition(error.to_string()),
        Error::InvalidRuntimeState => Status::invalid_argument(error.to_string()),
        _ => Status::unavailable(
            "Host storage is unavailable; command delivery may require reconciliation.",
        ),
    }
}
