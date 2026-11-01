//! Wire projection of the shared host Git precondition owner.
use crate::{api::proto, git::snapshot};
pub(super) use snapshot::repository;
use std::path::Path;
use tonic::Status;
pub(super) fn capture(root: &Path) -> Result<proto::GitPrecondition, Status> {
    let value = snapshot::capture(root)?;
    Ok(proto::GitPrecondition {
        head_oid: value.head,
        index_hash: value.index.to_vec(),
        worktree_hash: value.worktree.to_vec(),
        preflight_id: value.preflight,
    })
}
pub(super) fn check(root: &Path, expected: &proto::GitPrecondition) -> Result<(), Status> {
    snapshot::check(root, &crate::api::git::snapshot(expected)?)
}
