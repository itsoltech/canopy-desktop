//! The verified TLS peer, not request metadata or a device name, selects authority.
use super::proto;
use crate::{
    host::{CommandInput, CommandKey, CommandReceipt, CommandState, DeviceGrant},
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
        self.check_host(&meta.host_id)?;
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
    pub fn receipt(&self, receipt: &CommandReceipt) -> proto::CommandReceipt {
        let state = match receipt.state() {
            CommandState::Accepted => proto::OperationState::Accepted,
            CommandState::Attempting => proto::OperationState::Attempting,
            CommandState::Succeeded => proto::OperationState::Succeeded,
            CommandState::Failed => proto::OperationState::Failed,
            CommandState::Rejected => proto::OperationState::Rejected,
            CommandState::DeliveryUnknown => proto::OperationState::DeliveryUnknown,
        };
        let message = if receipt.method() == "chat.send" {
            receipt
                .result()
                .and_then(|b| std::str::from_utf8(b).ok())
                .unwrap_or("")
        } else {
            ""
        };
        let succeeded = receipt.state() == CommandState::Succeeded;
        let resource_result = receipt.method().starts_with("files.")
            || receipt.method().starts_with("git.")
            || receipt.method().starts_with("tasks.")
            || receipt.method().starts_with("settings.")
            || receipt.method().starts_with("artifacts.")
            || receipt.method().starts_with("terminal.")
            || receipt.method().starts_with("host.");
        let failed = matches!(
            receipt.state(),
            CommandState::Failed | CommandState::Rejected | CommandState::DeliveryUnknown
        );
        let error = if resource_result
            && matches!(
                receipt.state(),
                CommandState::Failed | CommandState::Rejected | CommandState::DeliveryUnknown
            ) {
            receipt
                .result()
                .and_then(|bytes| <proto::DomainError as prost::Message>::decode(bytes).ok())
        } else if failed
            && (receipt.method().starts_with("chat.") || receipt.method().starts_with("projects."))
        {
            receipt
                .result()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
                .and_then(|v| v.get("reason")?.as_str().map(str::to_owned))
                .map(|reason| proto::DomainError {
                    code: match reason.split(':').next().unwrap_or_default() {
                        code @ ("stale_run" | "already_resolved" | "conditions_changed"
                        | "chat_exists" | "not_editable" | "files_not_restored"
                        | "edit_not_started" | "shell_unavailable") => code.into(),
                        _ => "operation_failed".into(),
                    },
                    message: reason,
                    retryable: false,
                })
        } else {
            None
        };
        proto::CommandReceipt {
            host_id: self.host_id().into(),
            command_id: receipt.key().id().into(),
            command_epoch: receipt.key().epoch(),
            state: state as i32,
            resource_id: if succeeded && receipt.method() == "projects.open" {
                receipt
                    .result()
                    .and_then(|bytes| {
                        serde_json::from_slice::<crate::state::workspace::WorkspaceId>(bytes).ok()
                    })
                    .and_then(|id| serde_json::to_string(&id).ok())
                    .unwrap_or_else(|| receipt.target().into())
            } else {
                receipt.target().into()
            },
            message_id: message.into(),
            error,
            resolved_by_user: receipt.resolved_by_user(),
            file_hash: (succeeded && receipt.method() == "files.save")
                .then(|| receipt.result().map(<[u8]>::to_vec))
                .flatten(),
            operation_output: (succeeded
                && (receipt.method().starts_with("git.")
                    || receipt.method().starts_with("tasks.")
                    || receipt.method().starts_with("settings.")
                    || receipt.method().starts_with("terminal.")
                    || receipt.method().starts_with("projects.")
                    || receipt.method() == "chat.shell"
                    || receipt.method().starts_with("artifacts.")))
            .then(|| {
                receipt
                    .result()
                    .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
            })
            .flatten(),
            queue_revision: (succeeded && receipt.method() == "chat.queue")
                .then(|| {
                    receipt
                        .result()
                        .and_then(|bytes| serde_json::from_slice(bytes).ok())
                })
                .flatten(),
        }
    }
}
/// Host-resolved roots only. Registering a root does not grant a device access.
#[derive(Clone, Default)]
pub struct WorkspaceRoots {
    roots: Arc<RwLock<HashMap<String, PathBuf>>>,
}
impl WorkspaceRoots {
    /// The owner supplies already canonical paths, prepared outside render.
    pub fn replace(&self, roots: HashMap<String, PathBuf>) -> Result<(), Status> {
        if roots
            .iter()
            .any(|(id, path)| id.is_empty() || id.len() > 256 || !path.is_absolute())
        {
            return Err(Status::invalid_argument("Invalid workspace roots."));
        }
        *self
            .roots
            .write()
            .map_err(|_| Status::internal("Workspace registry is unavailable."))? = roots;
        Ok(())
    }
    pub fn resolve(
        &self,
        workspace: &proto::WorkspaceRef,
        auth: &RpcAuth,
    ) -> Result<PathBuf, Status> {
        auth.check_host(&workspace.host_id)?;
        self.roots
            .read()
            .map_err(|_| Status::internal("Workspace registry is unavailable."))?
            .get(&workspace.workspace_id)
            .cloned()
            .ok_or_else(|| Status::not_found("Workspace is no longer available."))
    }
    pub fn root_string(path: &Path) -> Result<&str, Status> {
        path.to_str().ok_or_else(|| {
            Status::invalid_argument("Workspace path cannot be represented by the protocol.")
        })
    }
}
pub fn storage_status(error: Error) -> Status {
    match error {
        Error::NotAuthorized => {
            Status::permission_denied("The device is not authorized for this operation.")
        }
        Error::NotFound => Status::not_found("The resource was not found."),
        Error::ResourceConflict | Error::CommandConflict => Status::aborted(error.to_string()),
        Error::ResourceLimit => Status::resource_exhausted(error.to_string()),
        Error::CommandRetired => Status::failed_precondition(error.to_string()),
        Error::OperationPending => Status::failed_precondition(error.to_string()),
        Error::InvalidRuntimeState => Status::invalid_argument(error.to_string()),
        _ => Status::unavailable(
            "Host storage is unavailable; command delivery may require reconciliation.",
        ),
    }
}
