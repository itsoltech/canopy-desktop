//! Typed native file/Git handlers. Resource gates serialize Canopy operations,
//! never claim to lock external editors or Git processes.
mod files;
mod git;
mod snapshot;
use super::{
    auth::{RpcAuth, WorkspaceRoots, storage_status},
    proto,
};
use crate::host::{CommandInput, CommandReceipt, CommandState, DeviceGrant};
pub use files::FilesRpc;
pub use git::{GitRpc, WorktreeLifecycle};
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tonic::Status;

#[derive(Default)]
struct ResourceState {
    gate: crate::files::Gate,
    operations: Mutex<(bool, usize)>,
    finished: tokio::sync::Notify,
}
#[derive(Clone, Default)]
pub struct ResourceLocks {
    state: Arc<ResourceState>,
}
pub struct OperationGuard(Arc<ResourceState>);
impl Drop for OperationGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.operations.lock() {
            state.1 = state.1.saturating_sub(1);
        }
        self.0.finished.notify_waiters();
    }
}
impl ResourceLocks {
    pub fn with_gate(gate: crate::files::Gate) -> Self {
        Self {
            state: Arc::new(ResourceState {
                gate,
                ..ResourceState::default()
            }),
        }
    }
    pub fn gate(&self) -> crate::files::Gate {
        self.state.gate.clone()
    }
    pub fn begin(&self) -> Result<OperationGuard, Status> {
        let mut operations = self
            .state
            .operations
            .lock()
            .map_err(|_| Status::internal("Resource task owner unavailable."))?;
        if operations.0 {
            return Err(Status::unavailable("Host resource services are draining."));
        }
        if operations.1 >= 64 {
            return Err(Status::resource_exhausted(
                "Host resource operation queue is full.",
            ));
        }
        operations.1 += 1;
        Ok(OperationGuard(self.state.clone()))
    }
    /// Stop admission and wait for already owned operations, including operations
    /// whose RPC connections were lost. Call before dropping the Tokio runtime.
    pub async fn drain(&self) -> Result<(), Status> {
        self.state
            .operations
            .lock()
            .map_err(|_| Status::internal("Resource task owner unavailable."))?
            .0 = true;
        loop {
            let notified = self.state.finished.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self
                .state
                .operations
                .lock()
                .map_err(|_| Status::internal("Resource task owner unavailable."))?
                .1
                == 0
            {
                return Ok(());
            }
            notified.await;
        }
    }
    pub async fn acquire(&self, root: &Path) -> Result<crate::files::Reservation, Status> {
        self.state
            .gate
            .acquire_root(root)
            .await
            .map_err(Status::internal)
    }
}

pub(super) async fn workspace(
    auth: &RpcAuth,
    roots: &WorkspaceRoots,
    grant: &DeviceGrant,
    reference: Option<&proto::WorkspaceRef>,
    scope: u32,
) -> Result<PathBuf, Status> {
    let reference = reference.ok_or_else(|| Status::invalid_argument("Workspace is required."))?;
    let root = roots.resolve(reference, auth)?;
    auth.resource(grant.id(), scope, WorkspaceRoots::root_string(&root)?)
        .await?;
    Ok(root)
}
pub(super) fn required<T>(value: Option<T>) -> Result<T, Status> {
    value.ok_or_else(|| Status::invalid_argument("Required field is missing."))
}
pub(super) fn domain(error: impl ToString) -> Status {
    Status::failed_precondition(error.to_string())
}
pub(super) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Status> + Send + 'static,
) -> Result<T, Status> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Status::internal("Resource worker stopped."))?
}
pub(super) fn receipt(auth: &RpcAuth, receipt: &CommandReceipt) -> proto::CommandReceipt {
    let mut result = auth.receipt(receipt);
    if matches!(
        receipt.state(),
        CommandState::Failed | CommandState::Rejected
    ) {
        result.error = receipt
            .result()
            .and_then(|bytes| <proto::DomainError as prost::Message>::decode(bytes).ok());
    }
    result
}
/// The detached task owns acceptance AND execution. Dropping the HTTP request
/// cannot strand an accepted operation or cancel a published network mutation.
pub(super) async fn mutation<F, Fut>(
    auth: RpcAuth,
    locks: ResourceLocks,
    root: PathBuf,
    input: CommandInput,
    effect: F,
) -> Result<proto::CommandReceipt, Status>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<Vec<u8>, Status>> + Send + 'static,
{
    let operation = locks.begin()?;
    tokio::spawn(async move {
        let _operation = operation;
        let _guard = locks.acquire(&root).await?;
        let accepted = auth
            .client()
            .accept_runtime_command(input, crate::host::FILES_GIT)
            .await
            .map_err(storage_status)?;
        if accepted.state() != CommandState::Accepted {
            return Ok(receipt(&auth, &accepted));
        }
        auth.client()
            .transition_runtime_command(
                accepted.key().clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None,
            )
            .await
            .map_err(storage_status)?;
        let outcome = effect().await;
        let (state, bytes) = match outcome {
            Ok(bytes) => (CommandState::Succeeded, bytes),
            Err(error) => {
                let state = if error.code() == tonic::Code::Unknown {
                    CommandState::DeliveryUnknown
                } else {
                    CommandState::Failed
                };
                let code = if error.message().starts_with("needs_host_interaction") {
                    "needs_host_interaction"
                } else {
                    match error.code() {
                        tonic::Code::Aborted => "conflict",
                        tonic::Code::PermissionDenied => "permission_denied",
                        tonic::Code::ResourceExhausted => "resource_limit",
                        _ => "operation_failed",
                    }
                };
                let error = proto::DomainError {
                    code: code.into(),
                    message: error.message().chars().take(8192).collect(),
                    retryable: false,
                };
                (state, prost::Message::encode_to_vec(&error))
            }
        };
        let final_receipt = auth
            .client()
            .transition_runtime_command(
                accepted.key().clone(),
                CommandState::Attempting,
                state,
                Some(bytes),
            )
            .await
            .map_err(storage_status)?;
        Ok(receipt(&auth, &final_receipt))
    })
    .await
    .map_err(|_| Status::unavailable("Command outcome requires reconciliation."))?
}

#[cfg(test)]
mod tests;
