use super::proto as p;
use crate::command_bus::{GitIntent, GitPaths, GitSnapshot, WorktreeAction, WorktreeIntent};
use tonic::Status;

pub(crate) fn snapshot(value: &p::GitPrecondition) -> Result<GitSnapshot, Status> {
    Ok(GitSnapshot {
        head: value.head_oid.clone(),
        index: value
            .index_hash
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("Invalid index hash."))?,
        worktree: value
            .worktree_hash
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("Invalid worktree hash."))?,
        preflight: value.preflight_id.clone(),
    })
}

pub(crate) fn intent(value: &p::preflight_request::Intent) -> Result<GitIntent, Status> {
    use p::preflight_request::Intent;
    let mut value = match value {
        Intent::Paths(v) => GitIntent::Paths {
            operation: match p::GitPathOperation::try_from(v.operation).ok() {
                Some(p::GitPathOperation::Stage) => GitPaths::Stage,
                Some(p::GitPathOperation::Unstage) => GitPaths::Unstage,
                Some(p::GitPathOperation::Discard) => GitPaths::Discard,
                _ => return Err(Status::invalid_argument("Path operation is required.")),
            },
            paths: v.paths.clone(),
        },
        Intent::Commit(v) => GitIntent::Commit {
            message: v.message.clone(),
            sign: v.sign,
        },
        Intent::Network(v) => match p::GitNetworkAction::try_from(v.action).ok() {
            Some(p::GitNetworkAction::Pull) => GitIntent::Pull {
                remote: v.remote.clone(),
                upstream: v.upstream_branch.clone(),
            },
            Some(p::GitNetworkAction::Push) => GitIntent::Push {
                remote: v.remote.clone(),
                upstream: v.upstream_branch.clone(),
            },
            _ => return Err(Status::invalid_argument("Network operation is required.")),
        },
        Intent::Worktree(v) => GitIntent::Worktree(WorktreeIntent {
            action: match p::WorktreeAction::try_from(v.action).ok() {
                Some(p::WorktreeAction::Create) => WorktreeAction::Create,
                Some(p::WorktreeAction::Remove) => WorktreeAction::Remove,
                Some(p::WorktreeAction::Merge) => WorktreeAction::Merge,
                Some(p::WorktreeAction::Stop) => WorktreeAction::Stop,
                Some(p::WorktreeAction::DeleteBranch) => WorktreeAction::DeleteBranch,
                _ => return Err(Status::invalid_argument("Worktree operation is required.")),
            },
            branch: v.branch.clone(),
            path: v.path.clone(),
            start_point: v.start_point.clone(),
            target_branch: v.target_branch.clone(),
            delete_branch: v.delete_branch,
            discard_changes: v.discard_changes,
            delete_unmerged_branch: v.delete_unmerged_branch,
            acknowledge_offline_dirty: v.acknowledge_offline_dirty,
            registration_name: None,
        }),
    };
    value
        .normalize()
        .map_err(crate::remote::auth::storage_status)?;
    Ok(value)
}
