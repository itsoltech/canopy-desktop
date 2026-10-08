//! Analysis and execution for branch-aware worktree removal.
//!
//! Analysis is side-effect free. Execution re-runs and compares it before each
//! mutation so a stale dialog cannot publish a merge or delete a changed ref.
use super::{RemovalKind, RemoveWorktree, Result, err, inspect, remove_worktree, root_repo};
use crate::terminal::environment::ShellEnvironment;
use git2::{BranchType, Index, Oid, Repository, Status, StatusOptions, build::CheckoutBuilder};
use std::{
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

mod merge;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeKind {
    AlreadyIntegrated,
    FastForward,
    MergeCommit,
    Conflicts(Vec<PathBuf>),
    Blocked(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchAnalysis {
    pub source: String,
    pub target: String,
    pub source_oid: String,
    pub target_oid: String,
    pub commits_not_in_target: usize,
    pub merge: MergeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeAnalysis {
    pub branch: String,
    pub source_oid: String,
    pub changed_entries: usize,
    pub tracked_changes: usize,
    pub untracked_or_ignored: usize,
    /// Exact approval boundary for index entries, paths and deletable file contents.
    pub cleanup_fingerprint: String,
    pub target: Option<BranchAnalysis>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemovalAction {
    KeepBranch,
    DeleteBranch { target: String },
    Merge { target: String, delete_branch: bool },
}
impl RemovalAction {
    fn target(&self) -> Option<&str> {
        match self {
            Self::KeepBranch => None,
            Self::DeleteBranch { target } | Self::Merge { target, .. } => Some(target),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RemovalApproval {
    pub discard_changes: bool,
    pub delete_unmerged_branch: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemovalResult {
    pub merge_performed: bool,
    pub merge_target_oid: Option<String>,
    pub worktree_removed: bool,
    pub branch_deleted: bool,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub retry_analysis: Option<WorktreeAnalysis>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorktreeRemovalOutcome {
    AnalysisChanged(WorktreeAnalysis),
    NeedsDiscardConfirmation(usize),
    NeedsBranchConfirmation(BranchAnalysis),
    Finished(RemovalResult),
}

fn local_commit<'a>(repo: &'a Repository, name: &str) -> Result<git2::Commit<'a>> {
    if name.is_empty() || !git2::Reference::is_valid_name(&format!("refs/heads/{name}")) {
        return Err("Choose a valid local branch.".into());
    }
    repo.find_branch(name, BranchType::Local)
        .map_err(|_| format!("Local branch '{name}' no longer exists."))?
        .get()
        .peel_to_commit()
        .map_err(err)
}

fn count_unique(repo: &Repository, source: Oid, target: Oid) -> Result<usize> {
    let mut walk = repo.revwalk().map_err(err)?;
    walk.push(source).map_err(err)?;
    walk.hide(target).map_err(err)?;
    let mut count = 0usize;
    for oid in walk {
        oid.map_err(err)?;
        count = count.saturating_add(1);
    }
    Ok(count)
}

struct CleanupState {
    changed: usize,
    tracked: usize,
    loose: usize,
    fingerprint: String,
}

fn status_path(root: &Path, bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        root.join(OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        root.join(String::from_utf8_lossy(bytes).as_ref())
    }
}

fn cleanup_state(repo: &Repository) -> Result<CleanupState> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .include_ignored(true)
        .recurse_untracked_dirs(true)
        .recurse_ignored_dirs(true)
        .update_index(false);
    let statuses = repo.statuses(Some(&mut options)).map_err(err)?;
    let mut tracked = 0;
    let mut loose = 0;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut entries = statuses
        .iter()
        .map(|entry| (entry.path_bytes().to_vec(), entry.status()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let root = repo.workdir().ok_or("Worktree directory is unavailable.")?;
    for (path, status) in &entries {
        path.hash(&mut hasher);
        status.bits().hash(&mut hasher);
        if status.intersects(Status::WT_NEW | Status::IGNORED) {
            loose += 1;
        } else {
            tracked += 1;
        }
        let absolute = status_path(root, path);
        match std::fs::symlink_metadata(&absolute) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                1u8.hash(&mut hasher);
                std::fs::read_link(&absolute)
                    .map_err(|error| error.to_string())?
                    .hash(&mut hasher);
            }
            Ok(metadata) if metadata.is_file() => {
                2u8.hash(&mut hasher);
                git2::Oid::hash_file(git2::ObjectType::Blob, &absolute)
                    .map_err(err)?
                    .hash(&mut hasher);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode().hash(&mut hasher);
                }
            }
            Ok(metadata) => {
                3u8.hash(&mut hasher);
                metadata.len().hash(&mut hasher);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                4u8.hash(&mut hasher);
            }
            Err(error) => return Err(format!("Cannot inspect {}: {error}", absolute.display())),
        }
    }
    let index = repo.index().map_err(err)?;
    for entry in index.iter() {
        entry.path.hash(&mut hasher);
        entry.id.hash(&mut hasher);
        entry.mode.hash(&mut hasher);
        entry.flags.hash(&mut hasher);
        entry.flags_extended.hash(&mut hasher);
    }
    Ok(CleanupState {
        changed: entries.len(),
        tracked,
        loose,
        fingerprint: format!("{:016x}", hasher.finish()),
    })
}

fn statuses(repo: &Repository) -> Result<(usize, usize, usize)> {
    let state = cleanup_state(repo)?;
    Ok((state.changed, state.tracked, state.loose))
}

fn conflict_paths(index: &Index) -> Result<Vec<PathBuf>> {
    if !index.has_conflicts() {
        return Ok(vec![]);
    }
    let mut paths = Vec::new();
    for conflict in index.conflicts().map_err(err)? {
        let conflict = conflict.map_err(err)?;
        let path = conflict
            .our
            .or(conflict.their)
            .or(conflict.ancestor)
            .map(|entry| PathBuf::from(String::from_utf8_lossy(&entry.path).into_owned()));
        if let Some(path) = path
            && !paths.contains(&path)
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn branch_analysis(repo: &Repository, source: &str, target: &str) -> Result<BranchAnalysis> {
    if source == target {
        return Err("Choose a different target branch.".into());
    }
    let source_commit = local_commit(repo, source)?;
    let target_commit = local_commit(repo, target)?;
    let source_oid = source_commit.id();
    let target_oid = target_commit.id();
    let commits_not_in_target = count_unique(repo, source_oid, target_oid)?;
    let merge = if source_oid == target_oid
        || repo
            .graph_descendant_of(target_oid, source_oid)
            .map_err(err)?
    {
        MergeKind::AlreadyIntegrated
    } else if repo
        .graph_descendant_of(source_oid, target_oid)
        .map_err(err)?
    {
        MergeKind::FastForward
    } else {
        let index = repo
            .merge_commits(&target_commit, &source_commit, None)
            .map_err(err)?;
        let conflicts = conflict_paths(&index)?;
        if conflicts.is_empty() {
            MergeKind::MergeCommit
        } else {
            MergeKind::Conflicts(conflicts)
        }
    };
    Ok(BranchAnalysis {
        source: source.into(),
        target: target.into(),
        source_oid: source_oid.to_string(),
        target_oid: target_oid.to_string(),
        commits_not_in_target,
        merge,
    })
}

pub fn analyze_worktree(
    request: &RemoveWorktree,
    target: Option<&str>,
) -> Result<WorktreeAnalysis> {
    if !matches!(request.kind, RemovalKind::Directory) {
        return Err("A missing worktree registration has no branch workflow.".into());
    }
    let info = inspect(&request.repository)?.ok_or("Not a Git repository.")?;
    let canonical = request
        .path
        .canonicalize()
        .map_err(|_| "Worktree is unavailable.".to_owned())?;
    let item = info
        .worktrees
        .iter()
        .find(|item| item.path == canonical)
        .ok_or("Worktree registration changed. Refresh and try again.")?;
    if item.name.is_none() {
        return Err("The main worktree cannot be removed.".into());
    }
    if item.locked {
        return Err("This worktree is locked. Unlock it before removal.".into());
    }
    let branch = item
        .branch_name()
        .ok_or("Attach the detached HEAD to a branch before removing this worktree.")?
        .to_owned();
    let repo = Repository::open(&canonical).map_err(err)?;
    if !repo.is_worktree() || repo.state() != git2::RepositoryState::Clean {
        return Err("Worktree has an unfinished Git operation.".into());
    }
    if ["index.lock", "HEAD.lock", "packed-refs.lock", "config.lock"]
        .iter()
        .any(|name| repo.path().join(name).exists() || repo.commondir().join(name).exists())
    {
        return Err("A Git operation is holding a lock. Try again after it finishes.".into());
    }
    if repo
        .submodules()
        .map_err(err)?
        .iter()
        .any(|module| canonical.join(module.path()).exists())
    {
        return Err("Worktree contains submodules; remove it outside Canopy.".into());
    }
    let source_oid = repo
        .head()
        .map_err(err)?
        .target()
        .ok_or("Worktree HEAD is unavailable.")?;
    let cleanup = cleanup_state(&repo)?;
    let main = root_repo(&request.repository)?;
    let mut target = target
        .map(|name| branch_analysis(&main, &branch, name))
        .transpose()?;
    if let Some(target) = &mut target
        && let Some(path) = checked_out_path(&info, &target.target)
    {
        let target_repo = Repository::open(path).map_err(err)?;
        if target_repo.state() != git2::RepositoryState::Clean {
            target.merge =
                MergeKind::Blocked("Target worktree has an unfinished Git operation.".into());
        } else if statuses(&target_repo)?.0 > 0 {
            target.merge = MergeKind::Blocked("Target worktree has local changes.".into());
        }
    }
    Ok(WorktreeAnalysis {
        branch,
        source_oid: source_oid.to_string(),
        changed_entries: cleanup.changed,
        tracked_changes: cleanup.tracked,
        untracked_or_ignored: cleanup.loose,
        cleanup_fingerprint: cleanup.fingerprint,
        target,
    })
}

fn checked_out_path(info: &super::RepositoryInfo, branch: &str) -> Option<PathBuf> {
    info.worktrees
        .iter()
        .find(|item| item.branch_name() == Some(branch) && item.available)
        .map(|item| item.path.clone())
}

fn hook_exists(repo: &Repository, name: &str) -> Result<bool> {
    let configured = repo.config().map_err(err)?.get_path("core.hooksPath").ok();
    let directory = configured
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                repo.workdir().unwrap_or(repo.commondir()).join(path)
            }
        })
        .unwrap_or_else(|| repo.commondir().join("hooks"));
    Ok(std::fs::metadata(directory.join(name)).is_ok())
}

#[cfg(any())]
fn publish_merge(
    repository: &Path,
    analysis: &BranchAnalysis,
    env: Option<&ShellEnvironment>,
) -> Result<Option<String>> {
    if analysis.merge == MergeKind::AlreadyIntegrated {
        return Ok(None);
    }
    let info = inspect(repository)?.ok_or("Not a Git repository.")?;
    let main = root_repo(repository)?;
    let source = local_commit(&main, &analysis.source)?;
    let target = local_commit(&main, &analysis.target)?;
    if source.id().to_string() != analysis.source_oid
        || target.id().to_string() != analysis.target_oid
    {
        return Err("A branch changed after analysis. Analyze again before merging.".into());
    }
    let target_ref = format!("refs/heads/{}", analysis.target);
    let source_ref = format!("refs/heads/{}", analysis.source);
    if analysis.merge == MergeKind::FastForward {
        let checked_out = checked_out_path(&info, &analysis.target);
        let mut tx = main.transaction().map_err(err)?;
        if checked_out.is_some() {
            tx.lock_ref("HEAD").map_err(err)?;
        }
        tx.lock_ref(&target_ref).map_err(err)?;
        tx.lock_ref(&source_ref).map_err(err)?;
        if main.refname_to_id(&target_ref).map_err(err)? != target.id()
            || main.refname_to_id(&source_ref).map_err(err)? != source.id()
        {
            return Err("A branch changed before publication. Analyze again.".into());
        }
        if let Some(path) = checked_out.as_ref() {
            let checkout_repo = Repository::open(path).map_err(err)?;
            if statuses(&checkout_repo)?.0 > 0 {
                return Err(format!(
                    "Target branch '{}' has local changes. Open that worktree and clean it before merging.",
                    analysis.target
                ));
            }
            let object = checkout_repo.find_object(source.id(), None).map_err(err)?;
            let mut checkout = CheckoutBuilder::new();
            checkout.safe().overwrite_ignored(false);
            checkout_repo
                .checkout_tree(&object, Some(&mut checkout))
                .map_err(err)?;
            let mut index = checkout_repo.index().map_err(err)?;
            index.read_tree(&source.tree().map_err(err)?).map_err(err)?;
            index.write().map_err(err)?;
        }
        tx.set_target(
            &target_ref,
            source.id(),
            None,
            "Canopy worktree merge: fast-forward",
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        return checked_out
            .as_deref()
            .map(|path| post_merge_warning(path, env))
            .transpose()
            .map(|warning| warning.flatten());
    }
    if !matches!(analysis.merge, MergeKind::MergeCommit) {
        return Err("The merge cannot be performed from the current analysis.".into());
    }
    let mut index = main.merge_commits(&target, &source, None).map_err(err)?;
    if index.has_conflicts() {
        return Err("The branches now conflict. No refs or worktree files were changed.".into());
    }
    let tree_id = index.write_tree_to(&main).map_err(err)?;
    let tree = main.find_tree(tree_id).map_err(err)?;
    let signature = main
        .signature()
        .map_err(|_| "Configure user.name and user.email before merging.".to_owned())?;
    let message = format!(
        "Merge branch '{}' into {}",
        analysis.source, analysis.target
    );
    let checked_out = checked_out_path(&info, &analysis.target);
    if checked_out.is_none()
        && [
            "pre-merge-commit",
            "prepare-commit-msg",
            "commit-msg",
            "post-merge",
        ]
        .iter()
        .any(|name| hook_exists(&main, name).unwrap_or(false))
    {
        return Err("The target branch has merge hooks but is not checked out. Open it as a worktree before merging.".into());
    }
    let cancel = AtomicBool::new(false);
    let report = |_phase: &str| {};
    let mut hook_runner = None;
    let mut message = message;
    let mut message_file = None;
    let hook_repo = checked_out
        .as_deref()
        .map(Repository::open)
        .transpose()
        .map_err(err)?;
    if let Some(hook_repo) = hook_repo.as_ref() {
        let has_hooks = [
            "pre-merge-commit",
            "prepare-commit-msg",
            "commit-msg",
            "post-merge",
        ]
        .iter()
        .any(|name| hook_exists(hook_repo, name).unwrap_or(false));
        if has_hooks {
            let env = env.ok_or("Shell environment is still loading; merge hooks cannot start.")?;
            let mut hooks = super::hooks::Hooks::new(hook_repo, env, &cancel, &report)?;
            hooks.run("pre-merge-commit", &[], &signature)?;
            let mut file = tempfile::NamedTempFile::new_in(hook_repo.path())
                .map_err(|error| error.to_string())?;
            file.write_all(message.as_bytes())
                .map_err(|error| error.to_string())?;
            file.flush().map_err(|error| error.to_string())?;
            hooks.run(
                "prepare-commit-msg",
                &[file.path().as_os_str(), std::ffi::OsStr::new("merge")],
                &signature,
            )?;
            hooks.run("commit-msg", &[file.path().as_os_str()], &signature)?;
            message = super::hooks::read_message(file.path())?;
            if statuses(hook_repo)?.0 > 0
                || main.refname_to_id(&target_ref).map_err(err)? != target.id()
                || main
                    .refname_to_id(&format!("refs/heads/{}", analysis.source))
                    .map_err(err)?
                    != source.id()
            {
                return Err("A merge hook changed the target worktree or refs. Review its changes; no merge commit was published.".into());
            }
            message_file = Some(file);
            hook_runner = Some(hooks);
        }
    }
    let config = main.config().map_err(err)?;
    let signing = super::signing::info(&config)?;
    let buffer = main
        .commit_create_buffer(&signature, &signature, &message, &tree, &[&target, &source])
        .map_err(err)?;
    let signed = if signing.enabled {
        let env = env.ok_or("Shell environment is still loading; signed merge cannot start.")?;
        Some(super::signing::sign(
            &config,
            &buffer,
            signature.email().unwrap_or_default(),
            checked_out.as_deref().unwrap_or(repository),
            env,
            &cancel,
        )?)
    } else {
        None
    };
    let oid = if let Some(signed) = signed {
        main.commit_signed(
            std::str::from_utf8(&buffer).map_err(|_| "Commit encoding is unsupported.")?,
            &signed,
            None,
        )
        .map_err(err)?
    } else {
        main.odb()
            .map_err(err)?
            .write(git2::ObjectType::Commit, &buffer)
            .map_err(err)?
    };
    let mut tx = main.transaction().map_err(err)?;
    if checked_out.is_some() {
        tx.lock_ref("HEAD").map_err(err)?;
    }
    tx.lock_ref(&target_ref).map_err(err)?;
    tx.lock_ref(&source_ref).map_err(err)?;
    if main.refname_to_id(&target_ref).map_err(err)? != target.id()
        || main.refname_to_id(&source_ref).map_err(err)? != source.id()
    {
        return Err("A branch changed before merge publication. Analyze again.".into());
    }
    if let Some(path) = checked_out.as_ref() {
        let checkout_repo = Repository::open(path).map_err(err)?;
        if statuses(&checkout_repo)?.0 > 0 {
            return Err("The target worktree changed before merge publication. Review it and analyze again.".into());
        }
        let object = checkout_repo.find_object(oid, None).map_err(err)?;
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().overwrite_ignored(false);
        checkout_repo
            .checkout_tree(&object, Some(&mut checkout))
            .map_err(err)?;
        let mut checkout_index = checkout_repo.index().map_err(err)?;
        checkout_index.read_tree(&tree).map_err(err)?;
        checkout_index.write().map_err(err)?;
    }
    tx.set_target(&target_ref, oid, Some(&signature), &message)
        .map_err(err)?;
    tx.commit().map_err(err)?;
    let warning = if let Some(mut hooks) = hook_runner {
        hooks
            .run("post-merge", &[std::ffi::OsStr::new("0")], &signature)
            .err()
            .map(|error| format!("Merge completed, but {error}"))
    } else {
        None
    };
    drop(message_file);
    Ok(warning)
}

pub fn delete_branch_after_removal(repository: &Path, analysis: &BranchAnalysis) -> Result<()> {
    let info = inspect(repository)?.ok_or("Not a Git repository.")?;
    if info
        .worktrees
        .iter()
        .any(|item| item.branch_name() == Some(&analysis.source))
    {
        return Err("The source branch is checked out in another worktree.".into());
    }
    let main = root_repo(repository)?;
    let checked_out_default = main
        .head()
        .ok()
        .and_then(|head| head.shorthand().ok().map(str::to_owned));
    let configured_default = main
        .config()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok());
    let remote_default = main
        .find_reference("refs/remotes/origin/HEAD")
        .ok()
        .and_then(|reference| {
            reference
                .symbolic_target()
                .ok()
                .flatten()
                .map(str::to_owned)
        })
        .and_then(|name| name.strip_prefix("refs/remotes/origin/").map(str::to_owned));
    if [
        checked_out_default.as_deref(),
        configured_default.as_deref(),
        remote_default.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|name| name == analysis.source)
    {
        return Err("The repository's main worktree branch cannot be deleted.".into());
    }
    if main
        .refname_to_id(&format!("refs/heads/{}", analysis.target))
        .map_err(err)?
        .to_string()
        != analysis.target_oid
    {
        return Err(
            "The comparison branch changed after cleanup. The source branch was kept.".into(),
        );
    }
    let mut branch = main
        .find_branch(&analysis.source, BranchType::Local)
        .map_err(err)?;
    if branch.get().target().map(|oid| oid.to_string()).as_deref() != Some(&analysis.source_oid) {
        return Err("The source branch changed after analysis and was kept.".into());
    }
    branch.delete().map_err(err)
}

pub fn execute_worktree_removal(
    request: &RemoveWorktree,
    action: &RemovalAction,
    expected: &WorktreeAnalysis,
    approval: RemovalApproval,
    env: Option<&ShellEnvironment>,
) -> Result<WorktreeRemovalOutcome> {
    let current = analyze_worktree(request, action.target())?;
    if &current != expected {
        return Ok(WorktreeRemovalOutcome::AnalysisChanged(current));
    }
    if matches!(action, RemovalAction::Merge { .. }) && current.tracked_changes > 0 {
        return Err("Commit or discard tracked changes in the source worktree before merging. Open Changes to review them.".into());
    }
    if current.changed_entries > 0 && !approval.discard_changes {
        return Ok(WorktreeRemovalOutcome::NeedsDiscardConfirmation(
            current.changed_entries,
        ));
    }
    if matches!(action, RemovalAction::DeleteBranch { .. }) {
        let branch = current
            .target
            .clone()
            .ok_or("Choose a comparison branch.")?;
        if branch.commits_not_in_target > 0 && !approval.delete_unmerged_branch {
            return Ok(WorktreeRemovalOutcome::NeedsBranchConfirmation(branch));
        }
    }
    let same_cleanup = |left: &WorktreeAnalysis, right: &WorktreeAnalysis| {
        left.branch == right.branch
            && left.source_oid == right.source_oid
            && left.changed_entries == right.changed_entries
            && left.tracked_changes == right.tracked_changes
            && left.untracked_or_ignored == right.untracked_or_ignored
            && left.cleanup_fingerprint == right.cleanup_fingerprint
    };
    let mut result = RemovalResult::default();
    let mut cleanup_analysis = current.clone();
    if matches!(action, RemovalAction::Merge { .. }) {
        let branch = current.target.as_ref().ok_or("Choose a merge target.")?;
        let publication = merge::publish(&request.repository, branch, env)?;
        result.warning = publication.warning;
        result.merge_performed = publication.performed;
        result.merge_target_oid = Some(publication.target_oid.clone());
        let after_merge = match analyze_worktree(request, action.target()) {
            Ok(analysis) => analysis,
            Err(error) => {
                result.error = Some(format!(
                    "Merge was published, but the source could not be analyzed before cleanup: {error}"
                ));
                return Ok(WorktreeRemovalOutcome::Finished(result));
            }
        };
        if after_merge
            .target
            .as_ref()
            .is_none_or(|target| target.target_oid != publication.target_oid)
        {
            result.error = Some(
                "Merge was published, but the target changed before cleanup. The worktree and source branch were kept."
                    .into(),
            );
            result.retry_analysis = Some(after_merge);
            return Ok(WorktreeRemovalOutcome::Finished(result));
        }
        if !same_cleanup(&current, &after_merge) {
            result.error = Some(
                "Merge was published, but source worktree files changed afterward. Review them and confirm cleanup again."
                    .into(),
            );
            result.retry_analysis = Some(after_merge);
            return Ok(WorktreeRemovalOutcome::Finished(result));
        }
        cleanup_analysis = after_merge;
    }
    let before_cleanup = match analyze_worktree(request, action.target()) {
        Ok(analysis) => analysis,
        Err(error) if result.merge_target_oid.is_some() => {
            result.error = Some(format!(
                "Merge was published, but the source could not be rechecked before cleanup: {error}"
            ));
            return Ok(WorktreeRemovalOutcome::Finished(result));
        }
        Err(error) => return Err(error),
    };
    if !same_cleanup(&cleanup_analysis, &before_cleanup) {
        if result.merge_performed {
            result.error = Some(
                "Merge was published, but source worktree files changed before cleanup. Review them and confirm again."
                    .into(),
            );
            result.retry_analysis = Some(before_cleanup);
            return Ok(WorktreeRemovalOutcome::Finished(result));
        }
        return Ok(WorktreeRemovalOutcome::AnalysisChanged(before_cleanup));
    }
    match remove_worktree(request, approval.discard_changes) {
        Ok(super::RemovalOutcome::Removed) => result.worktree_removed = true,
        Ok(super::RemovalOutcome::NeedsDiscardConfirmation(count)) => {
            return Ok(WorktreeRemovalOutcome::NeedsDiscardConfirmation(count));
        }
        Err(error) => {
            result.error = Some(error);
            result.retry_analysis = analyze_worktree(request, action.target()).ok();
            return Ok(WorktreeRemovalOutcome::Finished(result));
        }
    }
    let delete = matches!(action, RemovalAction::DeleteBranch { .. })
        || matches!(
            action,
            RemovalAction::Merge {
                delete_branch: true,
                ..
            }
        );
    if delete {
        let analysis = cleanup_analysis
            .target
            .as_ref()
            .ok_or("Choose a comparison branch.")?;
        match delete_branch_after_removal(&request.repository, analysis) {
            Ok(()) => result.branch_deleted = true,
            Err(error) => {
                result.error = Some(error);
                result.retry_analysis = Some(cleanup_analysis.clone());
            }
        }
    }
    Ok(WorktreeRemovalOutcome::Finished(result))
}
