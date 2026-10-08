use super::{
    Result, err,
    signing::{self, SigningInfo},
};
use crate::terminal::environment::ShellEnvironment;
use git2::{Delta, DiffFindOptions, DiffFormat, DiffOptions, Oid, Repository, StatusOptions};
use serde::{Deserialize, Serialize};
use std::{
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub old_path: Option<PathBuf>,
    pub staged: bool,
    pub kind: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangesSnapshot {
    pub revision: u64,
    pub files: Vec<FileChange>,
    pub signing: SigningInfo,
    pub head: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub kind: char,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    pub lines: Vec<DiffLine>,
    pub binary: bool,
    pub truncated: bool,
}
#[derive(Clone, Debug)]
pub enum Edit {
    Stage(Option<FileChange>),
    Unstage(Option<FileChange>),
    Discard(FileChange),
}
fn repo(path: &Path) -> Result<Repository> {
    let repo = Repository::open(path).map_err(err)?;
    if repo.is_bare() {
        return Err("Bare repository has no working tree.".into());
    }
    Ok(repo)
}
fn path_valid(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|c| matches!(c, Component::Normal(_)))
        && !path.components().any(|c| c.as_os_str() == ".git")
}
fn guard_path(root: &Path, path: &Path) -> Result<()> {
    if !path_valid(path) {
        return Err("Invalid repository-relative path.".into());
    }
    let mut parent = root
        .join(path)
        .parent()
        .ok_or("Invalid path.")?
        .to_path_buf();
    while !parent.exists() {
        if !parent.pop() {
            return Err("Path parent is unavailable.".into());
        }
    }
    if !parent
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("Path escapes the working tree through a symlink.".into());
    }
    Ok(())
}
fn item(delta: git2::DiffDelta<'_>, staged: bool) -> Result<FileChange> {
    let path = delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .ok_or("Git returned a missing path.")?
        .to_path_buf();
    let old_path = delta
        .old_file()
        .path()
        .filter(|old| *old != path)
        .map(Path::to_path_buf);
    let kind = match delta.status() {
        Delta::Added | Delta::Untracked => "A",
        Delta::Deleted => "D",
        Delta::Renamed => "R",
        Delta::Typechange => "T",
        Delta::Conflicted => "U",
        _ => "M",
    }
    .into();
    Ok(FileChange {
        path,
        old_path,
        staged,
        kind,
    })
}
pub fn status(path: &Path) -> Result<ChangesSnapshot> {
    let repo = repo(path)?;
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .update_index(false);
    let statuses = repo.statuses(Some(&mut options)).map_err(err)?;
    if statuses.len() > 10000 {
        return Err("More than 10,000 changed files. Narrow the repository scope.".into());
    }
    let mut files = vec![];
    for entry in statuses.iter() {
        if let Some(delta) = entry.head_to_index()
            && delta.status() != Delta::Unmodified
        {
            files.push(item(delta, true)?);
        }
        if let Some(delta) = entry.index_to_workdir()
            && delta.status() != Delta::Unmodified
        {
            files.push(item(delta, false)?);
        }
    }
    files.sort_by(|a, b| b.staged.cmp(&a.staged).then(a.path.cmp(&b.path)));
    Ok(ChangesSnapshot {
        revision: 0,
        files,
        signing: signing::info(&repo.config().map_err(err)?)?,
        head: repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .map(|id| id.to_string()),
    })
}
pub fn diff(path: &Path, file: &FileChange) -> Result<FileDiff> {
    let repo = repo(path)?;
    guard_path(path, &file.path)?;
    if let Some(old) = &file.old_path {
        guard_path(path, old)?;
    }
    let mut opts = DiffOptions::new();
    opts.disable_pathspec_match(true)
        .pathspec(&file.path)
        .include_typechange(true)
        .max_size(2 * 1024 * 1024)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    if let Some(old) = &file.old_path {
        opts.pathspec(old);
    }
    let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let index = repo.index().map_err(err)?;
    let mut diff = if file.staged {
        repo.diff_tree_to_index(head.as_ref(), Some(&index), Some(&mut opts))
    } else {
        repo.diff_index_to_workdir(Some(&index), Some(&mut opts))
    }
    .map_err(err)?;
    let mut find = DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find)).map_err(err)?;
    let mut lines = Vec::new();
    let mut bytes = 0;
    let mut truncated = false;
    diff.print(DiffFormat::Patch, |_, _, line| {
        if lines.len() >= 20000 || bytes > 2 * 1024 * 1024 {
            truncated = true;
            return false;
        }
        bytes += line.content().len();
        for text in String::from_utf8_lossy(line.content()).split_terminator('\n') {
            if lines.len() >= 20000 {
                truncated = true;
                return false;
            }
            let text = text.trim_end_matches('\r');
            let text = if text.len() > 16384 {
                truncated = true;
                text.chars().take(4096).collect()
            } else {
                text.to_owned()
            };
            lines.push(DiffLine {
                old: line.old_lineno(),
                new: line.new_lineno(),
                kind: line.origin(),
                text,
            });
        }
        true
    })
    .or_else(|e| if truncated { Ok(()) } else { Err(e) })
    .map_err(err)?;
    let oversized = diff
        .deltas()
        .any(|d| d.old_file().size() > 2 * 1024 * 1024 || d.new_file().size() > 2 * 1024 * 1024);
    truncated |= oversized;
    let binary = !oversized
        && diff
            .deltas()
            .any(|d| d.old_file().is_binary() || d.new_file().is_binary());
    Ok(FileDiff {
        lines,
        binary,
        truncated,
    })
}
pub fn edit(path: &Path, edit: Edit) -> Result<()> {
    let repo = repo(path)?;
    let snapshot = status(path)?;
    let mut index = repo.index().map_err(err)?;
    index.read(true).map_err(err)?;
    if index.has_conflicts() {
        return Err("Resolve index conflicts before staging or committing in Canopy.".into());
    }
    match edit {
        Edit::Stage(file) => {
            let files = if let Some(file) = file {
                vec![file]
            } else {
                snapshot.files.into_iter().filter(|f| !f.staged).collect()
            };
            for file in files {
                guard_path(path, &file.path)?;
                if let Some(old) = &file.old_path {
                    guard_path(path, old)?;
                    if index.get_path(old, 0).is_some() {
                        index.remove_path(old).map_err(err)?;
                    }
                }
                if std::fs::symlink_metadata(path.join(&file.path)).is_ok() {
                    if std::fs::symlink_metadata(path.join(&file.path)).is_ok_and(|m| m.is_dir()) {
                        return Err(
                            "Nested repositories and directories must be staged outside Canopy."
                                .into(),
                        );
                    }
                    index.add_path(&file.path).map_err(err)?;
                } else if index.get_path(&file.path, 0).is_some() {
                    index.remove_path(&file.path).map_err(err)?;
                }
            }
            index.write().map_err(err)
        }
        Edit::Unstage(file) => {
            let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            if let Some(file) = file {
                guard_path(path, &file.path)?;
                let mut paths = vec![file.path];
                if let Some(old) = file.old_path {
                    guard_path(path, &old)?;
                    paths.push(old);
                }
                let tree = head.as_ref().map(|h| h.tree()).transpose().map_err(err)?;
                for path in paths {
                    if let Some(entry) = tree.as_ref().and_then(|t| t.get_path(&path).ok()) {
                        let entry = git2::IndexEntry {
                            ctime: git2::IndexTime::new(0, 0),
                            mtime: git2::IndexTime::new(0, 0),
                            dev: 0,
                            ino: 0,
                            mode: entry.filemode() as u32,
                            uid: 0,
                            gid: 0,
                            file_size: 0,
                            id: entry.id(),
                            flags: 0,
                            flags_extended: 0,
                            path: path.as_os_str().as_encoded_bytes().to_vec(),
                        };
                        index.add(&entry).map_err(err)?;
                    } else if index.get_path(&path, 0).is_some() {
                        index.remove_path(&path).map_err(err)?;
                    }
                }
                index.write().map_err(err)
            } else {
                if let Some(head) = head {
                    index.read_tree(&head.tree().map_err(err)?).map_err(err)?;
                } else {
                    index.clear().map_err(err)?;
                }
                index.write().map_err(err)
            }
        }
        Edit::Discard(file) => {
            if file.staged {
                return Err("Unstage the file before discarding working tree changes.".into());
            }
            guard_path(path, &file.path)?;
            if index
                .get_path(&file.path, 0)
                .is_some_and(|entry| entry.mode == 0o160000)
            {
                return Err("Submodule changes must be handled in the submodule itself.".into());
            }
            if let Some(old) = &file.old_path {
                guard_path(path, old)?;
            }
            if index.get_path(&file.path, 0).is_none() {
                let target = path.join(&file.path);
                if std::fs::symlink_metadata(&target).is_ok_and(|m| m.is_dir()) {
                    return Err("Directory removal is not supported here.".into());
                }
                if std::fs::symlink_metadata(&target).is_ok() {
                    std::fs::remove_file(target).map_err(|e| e.to_string())?;
                }
            } else {
                let mut checkout = git2::build::CheckoutBuilder::new();
                checkout
                    .force()
                    .disable_pathspec_match(true)
                    .path(&file.path);
                repo.checkout_index(Some(&mut index), Some(&mut checkout))
                    .map_err(err)?;
            }
            if let Some(old) = file.old_path {
                guard_path(path, &old)?;
                if index.get_path(&old, 0).is_some() {
                    let mut checkout = git2::build::CheckoutBuilder::new();
                    checkout.force().disable_pathspec_match(true).path(old);
                    repo.checkout_index(Some(&mut index), Some(&mut checkout))
                        .map_err(err)?;
                }
            }
            Ok(())
        }
    }
}
struct IndexGuard {
    path: PathBuf,
    file: Option<std::fs::File>,
}
impl IndexGuard {
    fn acquire(repo: &Repository) -> Result<Self> {
        let path = repo.path().join("index.lock");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| "Git index is locked by another operation. Retry after it finishes.")?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }
}
impl Drop for IndexGuard {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.path);
    }
}
#[derive(Clone, Debug)]
pub struct CommitOutcome {
    pub id: Oid,
    pub hooks: Vec<super::hooks::HookReport>,
    pub warning: Option<String>,
}

pub fn commit(
    path: &Path,
    message: &str,
    expected_head: Option<&str>,
    env: &ShellEnvironment,
    cancel: &AtomicBool,
) -> Result<Oid> {
    commit_with_hooks(path, message, expected_head, env, cancel, &|_| {}).map(|result| result.id)
}
pub fn commit_with_hooks(
    path: &Path,
    message: &str,
    expected_head: Option<&str>,
    env: &ShellEnvironment,
    cancel: &AtomicBool,
    progress: &dyn Fn(&str),
) -> Result<CommitOutcome> {
    let repo = repo(path)?;
    if message.trim().is_empty() || message.len() > 1024 * 1024 || message.contains('\0') {
        return Err("Enter a non-empty commit message.".into());
    }
    if repo.state() != git2::RepositoryState::Clean {
        return Err("Finish the current merge/rebase before committing in Canopy.".into());
    }
    let head_ref = repo.find_reference("HEAD").map_err(err)?;
    let target = head_ref
        .symbolic_target()
        .map_err(err)?
        .ok_or("Attach HEAD to a branch before committing.")?
        .to_owned();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let old = parent.as_ref().map(|c| c.id());
    if old.map(|id| id.to_string()).as_deref() != expected_head {
        return Err("HEAD changed. Refresh changes before committing.".into());
    }
    let signature = repo
        .signature()
        .map_err(|_| "Configure user.name and user.email before committing.")?;
    let mut hooks = super::hooks::Hooks::new(&repo, env, cancel, progress)?;
    hooks.run("pre-commit", &[], &signature)?;
    let mut message_file =
        tempfile::NamedTempFile::new_in(repo.path()).map_err(|e| e.to_string())?;
    {
        use std::io::Write;
        message_file
            .write_all(message.as_bytes())
            .map_err(|e| e.to_string())?;
        message_file.flush().map_err(|e| e.to_string())?;
    }
    hooks.run(
        "prepare-commit-msg",
        &[
            message_file.path().as_os_str(),
            std::ffi::OsStr::new("message"),
        ],
        &signature,
    )?;
    hooks.run("commit-msg", &[message_file.path().as_os_str()], &signature)?;
    let message = super::hooks::read_message(message_file.path())?;
    if cancel.load(Ordering::Acquire) {
        return Err("Commit cancelled.".into());
    }
    if repo.state() != git2::RepositoryState::Clean
        || repo
            .find_reference("HEAD")
            .map_err(err)?
            .symbolic_target()
            .map_err(err)?
            != Some(target.as_str())
        || repo.head().ok().and_then(|h| h.target()) != old
    {
        return Err(
            "HEAD or repository state changed while hooks ran. Canopy stopped before publishing its commit; review the hook changes.".into(),
        );
    }
    let mut index = repo.index().map_err(err)?;
    index.read(true).map_err(err)?;
    if index.has_conflicts() {
        return Err("Resolve staged conflicts before committing.".into());
    }
    let tree_id = index.write_tree().map_err(err)?;
    if parent.as_ref().is_some_and(|p| p.tree_id() == tree_id)
        || parent.is_none() && index.is_empty()
    {
        return Err("There are no staged changes to commit.".into());
    }
    let tree = repo.find_tree(tree_id).map_err(err)?;
    let parents: Vec<_> = parent.iter().collect();
    let config = repo.config().map_err(err)?;
    let signing = signing::info(&config)?;
    let buffer = repo
        .commit_create_buffer(&signature, &signature, message.trim(), &tree, &parents)
        .map_err(err)?;
    progress(if signing.enabled {
        "Signing commit…"
    } else {
        "Creating commit…"
    });
    let signed = if signing.enabled {
        Some(signing::sign(
            &config,
            &buffer,
            signature.email().unwrap_or_default(),
            path,
            env,
            cancel,
        )?)
    } else {
        None
    };
    if cancel.load(Ordering::Acquire) {
        return Err("Commit cancelled.".into());
    }
    let _index_guard = IndexGuard::acquire(&repo)?;
    let mut tx = repo.transaction().map_err(err)?;
    tx.lock_ref("HEAD").map_err(err)?;
    tx.lock_ref(&target).map_err(err)?;
    if repo
        .find_reference("HEAD")
        .map_err(err)?
        .symbolic_target()
        .map_err(err)?
        != Some(target.as_str())
        || repo.refname_to_id(&target).ok() != old
    {
        return Err("HEAD changed while signing. No commit was published.".into());
    }
    index.read(true).map_err(err)?;
    if index.write_tree().map_err(err)? != tree_id {
        return Err("The index changed while signing. Review staged changes and retry.".into());
    }
    let oid = if let Some(signed) = signed {
        repo.commit_signed(
            std::str::from_utf8(&buffer).map_err(|_| "Commit encoding is unsupported.")?,
            &signed,
            None,
        )
        .map_err(err)?
    } else {
        repo.odb()
            .map_err(err)?
            .write(git2::ObjectType::Commit, &buffer)
            .map_err(err)?
    };
    tx.set_target(
        &target,
        oid,
        Some(&signature),
        &format!("commit: {}", message.lines().next().unwrap_or_default()),
    )
    .map_err(err)?;
    let mut log = repo.reflog("HEAD").map_err(err)?;
    log.append(
        oid,
        &signature,
        Some(&format!(
            "commit: {}",
            message.lines().next().unwrap_or_default()
        )),
    )
    .map_err(err)?;
    tx.set_reflog("HEAD", log).map_err(err)?;
    if cancel.load(Ordering::Acquire) {
        return Err("Commit cancelled before publication.".into());
    }
    tx.commit().map_err(err)?;
    drop(_index_guard);
    let warning = hooks
        .run("post-commit", &[], &signature)
        .err()
        .map(|error| format!("Commit {} was created, but {error}", oid));
    Ok(CommitOutcome {
        id: oid,
        hooks: hooks.reports,
        warning,
    })
}
