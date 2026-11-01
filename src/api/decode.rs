use super::proto as p;
use crate::{
    chat::transcript::Attachment,
    command_bus::{
        AttachmentIntent as A, CommandEnvelope, Conditions, DraftIntent, PullIntent, ResourceRef,
    },
    state::projects::Project,
    thread::{EnvironmentRef, HostId, ProjectRef, ThreadId, ThreadRef, WorktreeId},
};
use tonic::Status;

pub(crate) fn required<T>(value: Option<T>) -> Result<T, Status> {
    value.ok_or_else(|| Status::invalid_argument("Required field is missing."))
}
pub(crate) fn host(value: String) -> Result<HostId, Status> {
    HostId::new(value).ok_or_else(|| Status::invalid_argument("Invalid host identity."))
}
pub(crate) fn environment(value: p::EnvironmentRef) -> Result<EnvironmentRef, Status> {
    Ok(EnvironmentRef {
        host_id: host(value.host_id)?,
    })
}
pub fn target(value: p::ThreadTarget) -> Result<ResourceRef, Status> {
    let thread = required(value.thread)?;
    Ok(ResourceRef::Thread {
        thread: ThreadRef {
            host_id: host(thread.host_id)?,
            thread_id: ThreadId::parse(&thread.thread_id)
                .ok_or_else(|| Status::invalid_argument("Invalid thread identity."))?,
        },
        executor_epoch: value.executor_epoch,
    })
}

/// RPC-13: use the owner's authoritative project record, before resource I/O.
/// The current catalog uses the workspace ID as the checkout ID.
pub fn project(
    value: p::ProjectRef,
    host_id: &HostId,
    record: &Project,
) -> Result<ProjectRef, Status> {
    let requested_host = host(value.host_id)?;
    let id = serde_json::to_string(&record.id)
        .map_err(|_| Status::internal("Invalid project metadata."))?;
    if &requested_host != host_id || value.project_id != id {
        return Err(Status::not_found("Project is not available on this host."));
    }
    let worktree = value
        .worktree_id
        .map(|id| {
            WorktreeId::new(id)
                .ok_or_else(|| Status::invalid_argument("Invalid worktree identity."))
        })
        .transpose()?;
    let checked = ProjectRef::new(
        requested_host,
        record.id,
        worktree,
        record.repository_path.is_some(),
    )
    .map_err(|_| {
        Status::invalid_argument("Worktree presence differs from the project's Git metadata.")
    })?;
    if let Some(worktree) = checked.worktree_id() {
        let expected = serde_json::to_string(&record.workspace)
            .map_err(|_| Status::internal("Invalid checkout metadata."))?;
        if worktree.as_str() != expected {
            return Err(Status::not_found(
                "Worktree does not belong to this project.",
            ));
        }
    }
    Ok(checked)
}

pub fn envelope(
    meta: p::CommandMeta,
    resource: ResourceRef,
    command: crate::command_bus::Command,
) -> Result<CommandEnvelope, Status> {
    Ok(CommandEnvelope {
        command_id: uuid::Uuid::parse_str(&meta.command_id)
            .map_err(|_| Status::invalid_argument("Invalid command identity."))?,
        resource,
        command,
        conditions: Conditions {
            expected_revision: meta.expected_revision,
            ..Default::default()
        },
    })
}

pub fn draft(value: p::MessageDraft) -> Result<DraftIntent, Status> {
    let attachments = value
        .attachments
        .into_iter()
        .map(|a| {
            let label = (!a.label.is_empty()).then_some(a.label);
            Ok(match required(a.content)? {
                p::attachment::Content::Image(v) => A::Image {
                    host: host(v.host_id)?,
                    artifact: v.artifact_id,
                    sha256: v.sha256,
                },
                p::attachment::Content::File(v) => A::ThreadPath {
                    project_file: v.project_file,
                    relative: v.relative_path,
                    label,
                },
                p::attachment::Content::Quote(v) => A::Value(Attachment::Quote {
                    text: v.text,
                    label,
                }),
                p::attachment::Content::Command(v) => A::Value(Attachment::Command {
                    name: v.name,
                    skill: v.skill,
                    path: v.path.map(Into::into),
                    description: v.description,
                    label,
                }),
                p::attachment::Content::Pull(v) => A::Pull {
                    value: PullIntent {
                        number: v.number,
                        title: v.title,
                        url: v.url,
                        head: v.head,
                        base: v.base,
                        state: v.state,
                        body: v.body,
                        context_id: v.context_id,
                    },
                    label,
                },
                p::attachment::Content::Shell(v) => A::Value(Attachment::Shell {
                    command: v.command,
                    exit: v.exit_code,
                    stopped: v.stopped,
                    output: v.output,
                    label,
                }),
            })
        })
        .collect::<Result<Vec<_>, Status>>()?;
    Ok(DraftIntent::Incoming {
        text: value.text,
        context: value.context,
        goal: value.goal,
        attachments,
    })
}

pub(crate) fn preferences(value: p::UpdatePreferencesRequest) -> crate::command_bus::Command {
    use crate::command_bus::PreferenceWrite;
    let mut changes = Vec::new();
    for (key, value) in [
        ("reopenLastWorkspace", value.reopen_last_workspace),
        ("notch.enabled", value.notch_enabled),
    ] {
        if let Some(value) = value {
            changes.push(PreferenceWrite {
                key: key.into(),
                value: Some(value.to_string()),
            });
        }
    }
    for (key, value) in [
        ("newTab.toolId", value.new_tab_tool),
        ("newWorktree.toolId", value.new_worktree_tool),
        ("chat.profileId", value.chat_profile),
        ("chat.model", value.chat_model),
        ("chat.effort", value.chat_effort),
        ("chat.mode", value.chat_mode),
    ] {
        if let Some(value) = value {
            changes.push(PreferenceWrite {
                key: key.into(),
                value: (!value.is_empty()).then_some(value),
            });
        }
    }
    crate::command_bus::Command::Preferences { changes }
}
