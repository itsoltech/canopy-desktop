//! Typed native file/Git handlers. Resource gates serialize Canopy operations,
//! never claim to lock external editors or Git processes.
mod files;
mod git;
mod snapshot;
use super::auth::{RpcAuth, WorkspaceRoots, storage_status};
#[cfg(test)]
use crate::api::proto;
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

#[cfg(test)]
pub(super) async fn workspace(
    auth: &RpcAuth,
    roots: &WorkspaceRoots,
    grant: &DeviceGrant,
    reference: Option<&proto::ProjectRef>,
    scope: u32,
) -> Result<PathBuf, Status> {
    let reference = reference.ok_or_else(|| Status::invalid_argument("Workspace is required."))?;
    let (_, root) = roots.project(reference.clone(), auth)?;
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
#[cfg(test)]
pub(super) fn receipt(auth: &RpcAuth, receipt: &CommandReceipt) -> proto::CommandReceipt {
    let mut result = auth.receipt(receipt).unwrap();
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
#[cfg(test)]
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
    let result = owned_mutation(auth.clone(), locks, root, input, effect).await?;
    Ok(receipt(&auth, &result))
}

#[cfg(test)]
pub(super) async fn owned_mutation<F, Fut>(
    auth: RpcAuth,
    locks: ResourceLocks,
    root: PathBuf,
    input: CommandInput,
    effect: F,
) -> Result<CommandReceipt, Status>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<Vec<u8>, Status>> + Send + 'static,
{
    validated_mutation(
        auth,
        locks,
        root,
        input,
        move || async move { Ok(effect()) },
    )
    .await
}

/// Preconditions run under the same reservation as the eventual effect. A
/// rejected precondition never crosses the durable attempting boundary.
pub(crate) async fn validated_mutation<F, Prepare, Effect>(
    auth: RpcAuth,
    locks: ResourceLocks,
    root: PathBuf,
    input: CommandInput,
    prepare: F,
) -> Result<CommandReceipt, Status>
where
    F: FnOnce() -> Prepare + Send + 'static,
    Prepare: Future<Output = Result<Effect, Status>> + Send + 'static,
    Effect: Future<Output = Result<Vec<u8>, Status>> + Send + 'static,
{
    let typed_resource = !input.key.resource.is_empty();
    let operation = locks.begin()?;
    tokio::spawn(async move {
        let _operation = operation;
        let _guard = if root == Path::new("private") {
            None
        } else {
            Some(locks.acquire(&root).await?)
        };
        let accepted = auth
            .client()
            .accept_runtime_command(input, crate::host::FILES_GIT)
            .await
            .map_err(storage_status)?;
        if accepted.state() != CommandState::Accepted {
            return Ok(accepted);
        }
        let effect = match prepare().await {
            Ok(effect) => effect,
            Err(error) => {
                return reject_precondition(&auth, &accepted, error).await;
            }
        };
        auth.client()
            .transition_runtime_command(
                accepted.key().clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None,
            )
            .await
            .map_err(storage_status)?;
        let outcome = effect.await;
        let transport_error = outcome
            .as_ref()
            .err()
            .filter(|error| {
                typed_resource
                    && matches!(
                        error.code(),
                        tonic::Code::Unavailable
                            | tonic::Code::Internal
                            | tonic::Code::Unknown
                            | tonic::Code::PermissionDenied
                            | tonic::Code::Unauthenticated
                    )
            })
            .cloned();
        let (state, bytes) = match outcome {
            Ok(bytes) => (CommandState::Succeeded, bytes),
            Err(error) => {
                let state = if error.code() == tonic::Code::Unknown
                    || (typed_resource
                        && matches!(
                            error.code(),
                            tonic::Code::Unavailable | tonic::Code::Internal
                        )) {
                    CommandState::DeliveryUnknown
                } else {
                    CommandState::Failed
                };
                let code = if error.message().starts_with("needs_host_interaction") {
                    "needs_host_interaction"
                } else {
                    match error.code() {
                        tonic::Code::Aborted | tonic::Code::FailedPrecondition
                            if typed_resource =>
                        {
                            "conditions_changed"
                        }
                        tonic::Code::Aborted => "conflict",
                        tonic::Code::PermissionDenied => "permission_denied",
                        tonic::Code::ResourceExhausted => "resource_limit",
                        _ => "operation_failed",
                    }
                };
                let error = crate::api::proto::DomainError {
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
        if let Some(error) = transport_error {
            return Err(error);
        }
        // The owned effect can outlive the sender's grant. Keep its known
        // outcome, but reauthorize the retained operation before disclosure.
        auth.client()
            .get_runtime_command(final_receipt.key().clone(), 0)
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::unavailable("Command outcome requires reconciliation."))?;
        Ok(final_receipt)
    })
    .await
    .map_err(|_| Status::unavailable("Command outcome requires reconciliation."))?
}

pub(super) async fn reject_precondition(
    auth: &RpcAuth,
    accepted: &CommandReceipt,
    error: Status,
) -> Result<CommandReceipt, Status> {
    let receipt = auth
        .client()
        .transition_runtime_command(
            accepted.key().clone(),
            CommandState::Accepted,
            CommandState::Rejected,
            Some(prost::Message::encode_to_vec(
                &crate::api::proto::DomainError {
                    code: if matches!(
                        error.code(),
                        tonic::Code::Aborted
                            | tonic::Code::FailedPrecondition
                            | tonic::Code::NotFound
                    ) {
                        "conditions_changed"
                    } else {
                        "operation_failed"
                    }
                    .into(),
                    message: error.message().chars().take(8192).collect(),
                    retryable: false,
                },
            )),
        )
        .await
        .map_err(storage_status)?;
    if matches!(
        error.code(),
        tonic::Code::Aborted | tonic::Code::FailedPrecondition | tonic::Code::NotFound
    ) {
        return Ok(receipt);
    }
    Err(error)
}

#[cfg(test)]
mod tests;
