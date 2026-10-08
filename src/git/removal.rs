//! Confirmed worktree deletion and metadata-only cleanup of missing directories.
use super::{Result, err, inspect, root_repo};
use git2::{Repository, StatusOptions, WorktreePruneOptions};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum RemovalKind {
    Directory,
    /// Registration name and path must still refer to the entry shown to the user.
    MissingRegistration {
        name: String,
    },
}
#[derive(Clone, Debug)]
pub struct RemoveWorktree {
    pub repository: PathBuf,
    pub path: PathBuf,
    pub kind: RemovalKind,
}
impl RemoveWorktree {
    pub fn registration_only(&self) -> bool {
        matches!(self.kind, RemovalKind::MissingRegistration { .. })
    }
}
pub fn remove_worktree(request: &RemoveWorktree, discard_changes: bool) -> Result<RemovalOutcome> {
    match &request.kind {
        RemovalKind::Directory => {
            remove_confirmed(&request.repository, &request.path, discard_changes)
        }
        RemovalKind::MissingRegistration { name } => {
            prune_missing(&request.repository, &request.path, name)
        }
    }
}

fn require_missing(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot verify that the worktree directory is missing. Nothing was removed.".into()),
        Ok(_) => Err("The worktree path exists again. Refresh Git metadata before removing it; nothing was removed.".into()),
    }
}

fn prune_missing(repository: &Path, path: &Path, name: &str) -> Result<RemovalOutcome> {
    let main = root_repo(repository)?;
    require_missing(path)?;
    let wt = match main.find_worktree(name) {
        Ok(wt) => wt,
        // An external prune may have already removed this cached entry. Its
        // missing saved workspace can still be closed without touching Git.
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(RemovalOutcome::Removed),
        Err(e) => return Err(err(e)),
    };
    if !super::worktree_identity::same_worktree_path(wt.path(), path) {
        return Err(
            "Worktree registration changed. Refresh and try again; nothing was removed.".into(),
        );
    }
    if wt.is_locked().map_err(err)? != git2::WorktreeLockStatus::Unlocked {
        return Err("This worktree is locked. Unlock it before removing its entry.".into());
    }
    let registration = main.commondir().join("worktrees").join(name);
    if ["index.lock", "HEAD.lock", "packed-refs.lock", "config.lock"]
        .iter()
        .any(|lock| registration.join(lock).exists() || main.commondir().join(lock).exists())
    {
        return Err("A Git operation is holding a lock. Try again after it finishes.".into());
    }
    require_missing(wt.path())?;
    let mut options = WorktreePruneOptions::new();
    // Never fall back to directory deletion, even if the path is recreated
    // after the check. libgit2 also rechecks validity and the worktree lock.
    options.valid(false).working_tree(false).locked(false);
    wt.prune(Some(&mut options)).map_err(err)?;
    Ok(RemovalOutcome::Removed)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemovalOutcome {
    Removed,
    NeedsDiscardConfirmation(usize),
}
pub fn remove(repository: &Path, path: &Path) -> Result<()> {
    match remove_confirmed(repository, path, false)? {
        RemovalOutcome::Removed => Ok(()),
        RemovalOutcome::NeedsDiscardConfirmation(_) => Err(
            "Worktree contains changed, untracked or ignored files. Nothing was removed.".into(),
        ),
    }
}
pub fn remove_confirmed(
    repository: &Path,
    path: &Path,
    discard_changes: bool,
) -> Result<RemovalOutcome> {
    let info = inspect(repository)?.ok_or("Not a Git repository.")?;
    let canonical = path
        .canonicalize()
        .map_err(|_| "Worktree is unavailable. No files were removed.")?;
    let target = info
        .worktrees
        .iter()
        .find(|wt| wt.path == canonical)
        .ok_or("Worktree registration changed. Refresh and try again.")?;
    let name = target
        .name
        .as_deref()
        .ok_or("The main worktree cannot be removed.")?;
    if target.locked {
        return Err("This worktree is locked. Unlock it before removal.".into());
    }
    let repo = Repository::open(&canonical).map_err(err)?;
    if ["index.lock", "HEAD.lock", "packed-refs.lock", "config.lock"]
        .iter()
        .any(|name| repo.path().join(name).exists() || repo.commondir().join(name).exists())
    {
        return Err("A Git operation is holding a lock. Try again after it finishes.".into());
    }
    if !repo.is_worktree() || repo.state() != git2::RepositoryState::Clean {
        return Err("Worktree has an unfinished Git operation.".into());
    }
    if repo.head_detached().map_err(err)? {
        return Err("Attach the detached HEAD to a branch before removing this worktree.".into());
    }
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .include_ignored(true)
        .recurse_untracked_dirs(true)
        .recurse_ignored_dirs(true)
        .update_index(false);
    let changed = repo.statuses(Some(&mut options)).map_err(err)?.len();
    if changed > 0 && !discard_changes {
        return Ok(RemovalOutcome::NeedsDiscardConfirmation(changed));
    }
    if repo
        .submodules()
        .map_err(err)?
        .iter()
        .any(|m| canonical.join(m.path()).exists())
    {
        return Err("Worktree contains submodules; remove it outside Canopy.".into());
    }
    let main = root_repo(repository)?;
    let wt = main.find_worktree(name).map_err(err)?;
    wt.validate().map_err(err)?;
    if wt.path().canonicalize().ok().as_ref() != Some(&canonical) {
        return Err("Worktree path changed. Nothing was removed.".into());
    }
    let mut options = WorktreePruneOptions::new();
    options.valid(true).working_tree(true).locked(false);
    wt.prune(Some(&mut options)).map_err(err)?;
    Ok(RemovalOutcome::Removed)
}
