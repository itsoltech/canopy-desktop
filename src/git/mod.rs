//! Native Git operations. Never executes the git CLI.
pub mod hooks;
mod https_credentials;
mod process;
mod removal;
mod service;
mod worktree_identity;
mod worktree_workflow;
use git2::{BranchType, Repository, WorktreeAddOptions, WorktreeLockStatus};
pub use removal::{
    RemovalKind, RemovalOutcome, RemoveWorktree, remove, remove_confirmed, remove_worktree,
};
pub use service::{CommitRequest, GitClient, GitStats};
use std::path::{Path, PathBuf};
pub use worktree_identity::WorktreeHead;
pub use worktree_workflow::{
    BranchAnalysis, MergeKind, RemovalAction, RemovalApproval, RemovalResult, WorktreeAnalysis,
    WorktreeRemovalOutcome, analyze_worktree, delete_branch_after_removal,
    execute_worktree_removal,
};
pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub name: Option<String>,
    pub path: PathBuf,
    pub head: WorktreeHead,
    pub label: String,
    pub locked: bool,
    pub available: bool,
    /// A missing path is distinct from an unreadable or damaged existing folder.
    pub missing: bool,
    /// Branch recorded in linked-worktree metadata, even when its directory is missing.
    pub registered_branch: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryInfo {
    pub root: PathBuf,
    pub common: PathBuf,
    pub worktrees: Vec<WorktreeInfo>,
    pub branches: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct CreateWorktree {
    pub repository: PathBuf,
    pub destination: PathBuf,
    pub branch: String,
    pub new_branch: bool,
    pub base: String,
}
#[derive(Clone, Debug)]
pub struct CreatedWorktree {
    pub path: PathBuf,
    pub base: Option<crate::state::projects::WorktreeBase>,
}

/// Pure proposal from an already inspected canonical repository root.
pub fn worktree_path_proposal(root: &Path) -> Result<PathBuf> {
    if !root.is_absolute() {
        return Err("Repository root must be an absolute path.".into());
    }
    let parent = root.parent().ok_or("Repository has no parent directory.")?;
    let name = root
        .file_name()
        .ok_or("Repository has no directory name.")?
        .to_string_lossy();
    Ok(parent.join(format!(
        "{name}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..10]
    )))
}

/// Produces the stable per-dialog proposal used by every worktree entry point.
pub fn propose_worktree_path(repository: &Path) -> Result<PathBuf> {
    let root = repository_root(repository)?.ok_or("Not a Git repository.")?;
    for _ in 0..64 {
        let candidate = worktree_path_proposal(&root)?;
        if std::fs::symlink_metadata(&candidate).is_err() {
            return Ok(candidate);
        }
    }
    Err("Could not reserve an unused worktree directory name.".into())
}
fn err(e: git2::Error) -> String {
    e.message().to_owned()
}
fn root_repo(path: &Path) -> Result<Repository> {
    let repo = Repository::discover(path).map_err(err)?;
    Repository::open(repo.commondir()).map_err(err)
}
pub fn repository_root(path: &Path) -> Result<Option<PathBuf>> {
    let repo = match Repository::discover(path) {
        Ok(repo) => repo,
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    let main = Repository::open(repo.commondir()).map_err(err)?;
    let root = main
        .workdir()
        .ok_or("Bare repositories are not supported as a workspace.")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    Ok(Some(root))
}
pub fn inspect(path: &Path) -> Result<Option<RepositoryInfo>> {
    let discovered = match Repository::discover(path) {
        Ok(repo) => repo,
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    let repo = Repository::open(discovered.commondir()).map_err(err)?;
    let root = repo
        .workdir()
        .ok_or("Bare repositories are not supported as a workspace.")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let mut worktrees = vec![WorktreeInfo {
        name: None,
        path: root.clone(),
        head: worktree_identity::read_head(&repo),
        label: String::new(),
        locked: false,
        available: true,
        missing: false,
        registered_branch: None,
    }];
    let names = repo.worktrees().map_err(err)?;
    if names.len() > 128 {
        return Err("Repository has more than 128 worktrees.".into());
    }
    for name in names.iter() {
        let name = name.map_err(err)?.ok_or("Non-UTF-8 worktree name.")?;
        let wt = repo.find_worktree(name).map_err(err)?;
        let path = worktree_identity::canonical_worktree_path(wt.path());
        let opened = Repository::open(&path).ok();
        let registered_branch =
            std::fs::read_to_string(repo.commondir().join("worktrees").join(name).join("HEAD"))
                .ok()
                .and_then(|value| {
                    value
                        .trim()
                        .strip_prefix("ref: refs/heads/")
                        .map(str::to_owned)
                });
        worktrees.push(WorktreeInfo {
            name: Some(name.into()),
            path,
            head: opened
                .as_ref()
                .map(worktree_identity::read_head)
                .unwrap_or(WorktreeHead::Unavailable),
            label: String::new(),
            locked: wt.is_locked().map_err(err)? != WorktreeLockStatus::Unlocked,
            available: opened.is_some() && wt.validate().is_ok(),
            missing: matches!(std::fs::symlink_metadata(wt.path()), Err(e) if e.kind() == std::io::ErrorKind::NotFound),
            registered_branch,
        });
    }
    worktrees[1..].sort_by(|a, b| a.path.cmp(&b.path));
    worktree_identity::set_labels(&root, &mut worktrees);
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local)).map_err(err)? {
        let (branch, _) = entry.map_err(err)?;
        if let Some(name) = branch.name().map_err(err)? {
            branches.push(name.into());
        }
        if branches.len() > 4096 {
            return Err("Repository has more than 4096 local branches.".into());
        }
    }
    branches.sort();
    Ok(Some(RepositoryInfo {
        root,
        common: repo.commondir().canonicalize().map_err(|e| e.to_string())?,
        worktrees,
        branches,
    }))
}
pub fn create(request: &CreateWorktree) -> Result<PathBuf> {
    create_with_metadata(request).map(|created| created.path)
}

pub fn create_with_metadata(request: &CreateWorktree) -> Result<CreatedWorktree> {
    if request.branch.is_empty()
        || !git2::Reference::is_valid_name(&format!("refs/heads/{}", request.branch))
    {
        return Err("Enter a valid branch name.".into());
    }
    if !request.destination.is_absolute() {
        return Err("The worktree destination must be an absolute path.".into());
    }
    let parent = request
        .destination
        .parent()
        .ok_or("Missing destination parent.")?
        .canonicalize()
        .map_err(|_| "Destination parent does not exist.")?;
    let mut destination = parent.join(
        request
            .destination
            .file_name()
            .ok_or("Missing directory name.")?,
    );
    let info = inspect(&request.repository)?.ok_or("Not a Git repository.")?;
    // A proposal may become occupied while the form is open. Preserve the
    // managed naming policy and retry with a fresh sibling instead of overwriting.
    if std::fs::symlink_metadata(&destination).is_ok() {
        destination = propose_worktree_path(&request.repository)?;
    }
    if info
        .worktrees
        .iter()
        .any(|wt| destination.starts_with(&wt.path))
    {
        return Err("Create worktrees outside existing working trees.".into());
    }
    if info
        .worktrees
        .iter()
        .any(|wt| wt.checkout_branch() == Some(request.branch.as_str()))
    {
        return Err("This branch is already checked out in a worktree.".into());
    }
    let repo = root_repo(&request.repository)?;
    let (branch, base) = if request.new_branch {
        if request.base.is_empty() {
            return Err("Choose a local branch to start from.".into());
        }
        let base_name = if request.base == "HEAD" {
            repo.head()
                .map_err(err)?
                .shorthand()
                .map_err(err)?
                .to_owned()
        } else {
            request.base.clone()
        };
        let reference = format!("refs/heads/{base_name}");
        let commit = repo
            .find_reference(&reference)
            .map_err(|_| "The selected starting branch no longer exists.".to_owned())?
            .peel_to_commit()
            .map_err(err)?;
        let base = crate::state::projects::WorktreeBase {
            reference: base_name,
            oid: commit.id().to_string(),
        };
        (
            repo.branch(&request.branch, &commit, false).map_err(err)?,
            Some(base),
        )
    } else {
        (
            repo.find_branch(&request.branch, BranchType::Local)
                .map_err(err)?,
            None,
        )
    };
    let mut options = WorktreeAddOptions::new();
    options.reference(Some(branch.get()));
    let name = format!("canopy-{}", uuid::Uuid::new_v4().simple());
    match repo.worktree(&name, &destination, Some(&options)) {
        Ok(_) => Ok(CreatedWorktree {
            path: destination,
            base,
        }),
        Err(e) => {
            // libgit2 may have partially created files. Never recursively delete those on error.
            Err(format!(
                "Could not create worktree: {}. Check {} before retrying; any newly created branch is preserved.",
                e.message(),
                destination.display()
            ))
        }
    }
}
/// Canonical working directory for a selected folder, without requiring it to be Git.
pub fn workspace_directory(path: &Path) -> Result<PathBuf> {
    let folder = crate::state::projects::canonical_directory(path).map_err(|e| e.to_string())?;
    match Repository::discover(&folder) {
        Ok(repo) => repo
            .workdir()
            .ok_or("Bare repositories cannot be opened as workspaces.")?
            .canonicalize()
            .map_err(|e| e.to_string()),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(folder),
        Err(e) => Err(err(e)),
    }
}
mod change_watch;
pub mod changes;
pub mod signing;

pub mod history;

pub mod history_graph;

pub mod network;

mod ssh_credentials;
