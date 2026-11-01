//! Resource identities are checked against host-owned metadata before I/O.
use super::{decode, proto as p};
use crate::{
    command_bus::{Command, CommandBus, FileCommand, PreparedCommand, Principal, ResourceRef},
    host::{CommandReceipt, CommandState, DeviceGrant},
    remote::auth::{RpcAuth, WorkspaceRoots, storage_status},
    thread::{ProjectRef, WorktreeId},
};
use prost::Message;
use std::path::PathBuf;
use tonic::Status;

pub(crate) fn resource(target: &p::ResourceTarget) -> Result<ResourceRef, Status> {
    match decode::required(target.resource.clone())? {
        p::resource_target::Resource::Thread(target) => decode::target(target),
        p::resource_target::Resource::Project(project) => {
            let worktree = project
                .worktree_id
                .map(|id| {
                    WorktreeId::new(id)
                        .ok_or_else(|| Status::invalid_argument("Invalid checkout identity."))
                })
                .transpose()?;
            Ok(ResourceRef::Project(
                ProjectRef::new(
                    decode::host(project.host_id)?,
                    serde_json::from_str(&project.project_id)
                        .map_err(|_| Status::invalid_argument("Invalid project identity."))?,
                    worktree.clone(),
                    worktree.is_some(),
                )
                .map_err(|_| Status::invalid_argument("Invalid project target."))?,
            ))
        }
    }
}

pub(crate) async fn read_root(
    auth: &RpcAuth,
    roots: &WorkspaceRoots,
    grant: &DeviceGrant,
    target: &p::ResourceTarget,
    scope: u32,
) -> Result<PathBuf, Status> {
    let target = resource(target)?;
    auth.check_host(target.host_id().as_str())?;
    auth.scope(grant, scope)?;
    let root = match &target {
        ResourceRef::Project(project) => roots.project(project_wire(project), auth)?.1,
        ResourceRef::Thread { thread, .. } => {
            let record = auth
                .client()
                .load_runtime_thread(thread.thread_id)
                .await
                .map_err(storage_status)?
                .ok_or_else(|| Status::not_found("Thread is unavailable."))?;
            auth.resource(grant.id(), scope, record.authority_root())
                .await?;
            let id = record
                .workspace()
                .ok_or_else(|| Status::failed_precondition("This thread has no project files."))?;
            roots.resolve_host_workspace(id).map_err(storage_status)?
        }
        ResourceRef::Environment(_) => unreachable!(),
    };
    auth.resource(grant.id(), scope, WorkspaceRoots::root_string(&root)?)
        .await?;
    Ok(root)
}

pub(crate) async fn prepare_file(
    auth: &RpcAuth,
    roots: &WorkspaceRoots,
    grant: &DeviceGrant,
    meta: p::CommandMeta,
    target: &p::ResourceTarget,
    command: FileCommand,
) -> Result<PreparedCommand, Status> {
    prepare(auth, roots, grant, meta, target, Command::File(command)).await
}

/// Accepted recovery must resolve the original target again before resource I/O.
pub(crate) async fn revalidate_root(
    auth: &RpcAuth,
    roots: &WorkspaceRoots,
    grant: &DeviceGrant,
    target: &p::ResourceTarget,
    scope: u32,
    expected: &std::path::Path,
) -> Result<(), Status> {
    if read_root(auth, roots, grant, target, scope).await? != expected {
        return Err(Status::failed_precondition(
            "The resource checkout changed.",
        ));
    }
    Ok(())
}

pub(crate) async fn prepare(
    auth: &RpcAuth,
    roots: &WorkspaceRoots,
    grant: &DeviceGrant,
    meta: p::CommandMeta,
    target: &p::ResourceTarget,
    command: Command,
) -> Result<PreparedCommand, Status> {
    let resource = resource(target)?;
    auth.check_host(resource.host_id().as_str())?;
    if matches!(command, Command::Git(_))
        && !matches!(&resource, ResourceRef::Project(project) if project.worktree_id().is_some())
    {
        return Err(Status::invalid_argument("Git requires a project checkout."));
    }
    let project = match &resource {
        ResourceRef::Project(p) => Some(project_wire(p)),
        _ => None,
    };
    let needs_checkout = !matches!(&command, Command::Artifact(command) if matches!(command.destination(), crate::command_bus::UploadDestination::Image));
    let envelope = decode::envelope(meta.clone(), resource, command)?;
    let principal = Principal::authenticated(grant, meta.command_epoch).map_err(storage_status)?;
    let bus = CommandBus::new(auth.client().clone());
    if let Some(project) = project {
        // A retained outcome stays retryable after the checkout was closed.
        // The writer checks its saved root before disclosing dedupe or conflict.
        if let Some(prepared) = bus
            .prepare_project_resource(envelope.clone(), principal.clone(), None)
            .await
            .map_err(storage_status)?
        {
            return Ok(prepared);
        }
        let (_, root) = roots.project(project, auth)?;
        bus.prepare_project_resource(
            envelope,
            principal,
            Some(WorkspaceRoots::root_string(&root)?.into()),
        )
        .await
        .map_err(storage_status)?
        .ok_or_else(|| Status::internal("Resource admission is incomplete."))
    } else {
        let prepared = bus
            .prepare(envelope, principal)
            .await
            .map_err(storage_status)?;
        if prepared.prior().is_none() && needs_checkout {
            let record = prepared
                .record()
                .ok_or_else(|| Status::not_found("Thread is unavailable."))?;
            let workspace = record
                .workspace()
                .ok_or_else(|| Status::failed_precondition("This thread has no project files."))?;
            let root = roots
                .resolve_host_workspace(workspace)
                .map_err(storage_status)?;
            if WorkspaceRoots::root_string(&root)? != record.authority_root() {
                return Err(Status::failed_precondition("Thread checkout changed."));
            }
        }
        Ok(prepared)
    }
}

pub(crate) async fn prepare_environment(
    auth: &RpcAuth,
    grant: &DeviceGrant,
    meta: p::CommandMeta,
    environment: p::EnvironmentRef,
    command: Command,
) -> Result<PreparedCommand, Status> {
    let resource = ResourceRef::Environment(crate::thread::EnvironmentRef {
        host_id: decode::host(environment.host_id)?,
    });
    auth.check_host(resource.host_id().as_str())?;
    let principal = Principal::authenticated(grant, meta.command_epoch).map_err(storage_status)?;
    CommandBus::new(auth.client().clone())
        .prepare(decode::envelope(meta, resource, command)?, principal)
        .await
        .map_err(storage_status)
}

fn project_wire(project: &ProjectRef) -> p::ProjectRef {
    p::ProjectRef {
        host_id: project.host_id().to_string(),
        project_id: serde_json::to_string(&project.project_id()).expect("project identity"),
        worktree_id: project.worktree_id().map(|w| w.as_str().into()),
    }
}

pub(crate) fn receipt(
    auth: &RpcAuth,
    value: &CommandReceipt,
    target: &p::ResourceTarget,
) -> Result<p::CommandReceipt, Status> {
    let resource = match decode::required(target.resource.clone())? {
        p::resource_target::Resource::Project(project) => {
            p::command_receipt::Resource::Project(project)
        }
        p::resource_target::Resource::Thread(thread) => {
            p::command_receipt::Resource::Target(thread)
        }
    };
    receipt_for_resource(auth, value, resource)
}

pub(crate) fn receipt_for_resource(
    auth: &RpcAuth,
    value: &CommandReceipt,
    resource: p::command_receipt::Resource,
) -> Result<p::CommandReceipt, Status> {
    if value.method() == "terminal.close"
        || !["files.", "git.", "terminal.", "artifacts.", "tasks."]
            .iter()
            .any(|prefix| value.method().starts_with(prefix))
    {
        return super::project_receipt(auth, value, resource);
    }
    let error = if matches!(
        value.state(),
        CommandState::Failed | CommandState::Rejected | CommandState::DeliveryUnknown
    ) {
        value
            .result()
            .and_then(|bytes| p::DomainError::decode(bytes).ok())
            .or_else(|| {
                let result: serde_json::Value = serde_json::from_slice(value.result()?).ok()?;
                let reason = result["reason"].as_str()?;
                Some(p::DomainError {
                    code: reason.split(':').next()?.into(),
                    message: reason.into(),
                    retryable: false,
                })
            })
    } else {
        None
    };
    Ok(p::CommandReceipt {
        host_id: auth.host_id().into(),
        command_id: value.key().id().into(),
        command_epoch: value.key().epoch(),
        state: match value.state() {
            CommandState::Accepted => p::OperationState::Accepted,
            CommandState::Attempting => p::OperationState::Attempting,
            CommandState::Succeeded => p::OperationState::Succeeded,
            CommandState::Failed => p::OperationState::Failed,
            CommandState::Rejected => p::OperationState::Rejected,
            CommandState::DeliveryUnknown => p::OperationState::DeliveryUnknown,
        } as i32,
        resource: Some(resource),
        resolved_by_user: value.resolved_by_user(),
        error,
        result: if value.state() == CommandState::Succeeded {
            Some({
                if value.method().starts_with("tasks.") {
                    let result: serde_json::Value =
                        serde_json::from_slice(value.result().unwrap_or(b"{}"))
                            .map_err(|_| Status::data_loss("Task result is unreadable."))?;
                    let text = |key: &str| result[key].as_str().map(str::to_owned);
                    p::command_receipt::Result::Task(p::TaskResult {
                        task_id: text("task_id"),
                        deleted_task_id: text("deleted_task_id"),
                        deleted_comment_id: text("deleted_comment_id"),
                        url: text("url"),
                        comment_id: text("comment_id"),
                        attachment_id: text("attachment_id"),
                        notice: text("notice"),
                        worktree_catalog_id: text("workspace_id"),
                    })
                } else if value.method().starts_with("artifacts.") {
                    p::command_receipt::Result::Artifact(
                        p::ArtifactResult::decode(
                            value
                                .result()
                                .ok_or_else(|| Status::data_loss("Artifact result is missing."))?,
                        )
                        .map_err(|_| Status::data_loss("Artifact result is unreadable."))?,
                    )
                } else if value.method().starts_with("terminal.") {
                    p::command_receipt::Result::Terminal(
                        p::TerminalResult::decode(
                            value
                                .result()
                                .ok_or_else(|| Status::data_loss("Terminal result is missing."))?,
                        )
                        .map_err(|_| Status::data_loss("Terminal result is unreadable."))?,
                    )
                } else if value.method().starts_with("git.") {
                    p::command_receipt::Result::Git(p::GitResult {
                        output: String::from_utf8_lossy(value.result().unwrap_or_default())
                            .into_owned(),
                    })
                } else {
                    p::command_receipt::Result::File(p::FileResult {
                        hash: if value.method() == "files.save" {
                            value.result().unwrap_or_default().into()
                        } else {
                            Vec::new()
                        },
                    })
                }
            })
        } else {
            None
        },
    })
}
