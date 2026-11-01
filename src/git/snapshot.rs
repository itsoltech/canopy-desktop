//! Exact bounded preconditions. Capture twice to reject a visibly moving tree;
//! final checks still cannot exclude arbitrary external writers after the check.
use crate::command_bus::GitSnapshot;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use tonic::Status;

fn domain(error: impl ToString) -> Status {
    Status::failed_precondition(error.to_string())
}
pub fn repository(root: &Path) -> Result<git2::Repository, Status> {
    let repository = git2::Repository::open(root).map_err(domain)?;
    let workdir = repository
        .workdir()
        .ok_or_else(|| domain("A working tree is required."))?
        .canonicalize()
        .map_err(domain)?;
    if workdir != root {
        return Err(Status::permission_denied(
            "Git operations require the authorized workspace root, not a containing repository.",
        ));
    }
    Ok(repository)
}
pub fn capture(root: &Path) -> Result<GitSnapshot, Status> {
    capture_with(root, false)
}
/// Local operations retain their existing scope: ignored build output is not
/// read and remote transfer budgets do not limit ordinary local Git actions.
/// Destructive worktree approval still belongs to WorktreeAnalysis.
pub fn capture_local(root: &Path) -> Result<GitSnapshot, Status> {
    capture_with(root, true)
}
fn capture_with(root: &Path, local: bool) -> Result<GitSnapshot, Status> {
    let first = once(root, local)?;
    let second = once(root, local)?;
    if first != second {
        return Err(Status::aborted(
            "Repository changed while capturing its precondition.",
        ));
    }
    Ok(second)
}
pub fn check(root: &Path, expected: &GitSnapshot) -> Result<(), Status> {
    let mut expected = expected.clone();
    expected.preflight = None;
    if capture(root)? != expected {
        return Err(Status::aborted(
            "HEAD, index or working tree changed. Review a new preflight.",
        ));
    }
    Ok(())
}
fn once(root: &Path, local: bool) -> Result<GitSnapshot, Status> {
    let repo = repository(root)?;
    let head = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|id| id.to_string())
        .unwrap_or_default();
    let mut index_hash = Sha256::new();
    let mut budget = if local {
        u64::MAX - 1
    } else {
        256 * 1024 * 1024u64
    };
    hash_file(&repo.path().join("index"), &mut index_hash, &mut budget)?;
    // Symbolic HEAD and config/refs affect where an operation publishes, even
    // when the currently resolved commit is unchanged.
    for name in ["HEAD", "config", "config.worktree", "packed-refs"] {
        hash_file(&repo.path().join(name), &mut index_hash, &mut budget)?;
    }
    if repo.commondir() != repo.path() {
        for name in ["config", "packed-refs"] {
            hash_file(&repo.commondir().join(name), &mut index_hash, &mut budget)?;
        }
    }
    let mut references = repo
        .references()
        .map_err(domain)?
        .take(if local { usize::MAX } else { 10_001 })
        .map(|r| {
            r.map(|r| {
                (
                    r.name_bytes().to_vec(),
                    r.target().map(|o| o.to_string()),
                    r.symbolic_target_bytes().map(|s| s.to_vec()),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(domain)?;
    if !local && references.len() > 10_000 {
        return Err(Status::resource_exhausted(
            "Git preflight is limited to 10,000 refs.",
        ));
    }
    references.sort();
    for reference in references {
        let bytes = serde_json::to_vec(&reference).map_err(domain)?;
        index_hash.update((bytes.len() as u64).to_le_bytes());
        index_hash.update(bytes);
    }
    index_hash.update(format!("{:?}", repo.state()));
    let mut worktree_hash = Sha256::new();
    let mut count = 0;
    if local {
        let mut options = git2::StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = repo.statuses(Some(&mut options)).map_err(domain)?;
        for entry in statuses.iter() {
            let name = entry.path_bytes();
            worktree_hash.update((name.len() as u64).to_le_bytes());
            worktree_hash.update(name);
            worktree_hash.update(entry.status().bits().to_le_bytes());
            let name = std::str::from_utf8(name).map_err(domain)?;
            let path = root.join(name);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    worktree_hash.update(
                        std::fs::read_link(&path)
                            .map_err(domain)?
                            .as_os_str()
                            .as_encoded_bytes(),
                    );
                }
                Ok(metadata) if metadata.is_dir() => {
                    worktree_hash.update(b"directory");
                }
                _ => hash_file(&path, &mut worktree_hash, &mut budget)?,
            }
        }
    } else {
        walk(root, root, &mut worktree_hash, &mut budget, &mut count)?;
    }
    Ok(GitSnapshot {
        head,
        index: index_hash.finalize().into(),
        worktree: worktree_hash.finalize().into(),
        preflight: None,
    })
}
fn walk(
    root: &Path,
    directory: &Path,
    digest: &mut Sha256,
    budget: &mut u64,
    count: &mut usize,
) -> Result<(), Status> {
    let mut children = std::fs::read_dir(directory)
        .map_err(domain)?
        .take(200_001usize.saturating_sub(*count))
        .collect::<Result<Vec<_>, _>>()
        .map_err(domain)?;
    if children.len() + *count > 200_000 {
        return Err(Status::resource_exhausted(
            "Git preflight is limited to 200,000 worktree entries.",
        ));
    }
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if entry.file_name() == ".git" {
            continue;
        }
        *count += 1;
        if *count > 200_000 {
            return Err(Status::resource_exhausted(
                "Git preflight is limited to 200,000 worktree entries.",
            ));
        }
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(domain)?;
        if relative.components().count() > 64 {
            return Err(Status::resource_exhausted(
                "Worktree nesting exceeds 64 levels.",
            ));
        }
        let name = relative
            .to_str()
            .ok_or_else(|| domain("Non-UTF8 paths cannot be approved remotely."))?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        let metadata = std::fs::symlink_metadata(&path).map_err(domain)?;
        if metadata.is_dir() {
            crate::files::resolve(root, relative).map_err(domain)?;
            digest.update(b"directory");
            walk(root, &path, digest, budget, count)?;
        } else if metadata.file_type().is_symlink() {
            digest.update(b"link");
            let link = std::fs::read_link(&path).map_err(domain)?;
            digest.update((link.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
            digest.update(link.as_os_str().as_encoded_bytes());
        } else if metadata.is_file() {
            digest.update(b"file");
            hash_file(&path, digest, budget)?;
        } else {
            return Err(domain(
                "Special files cannot be included in a destructive Git preflight.",
            ));
        }
    }
    Ok(())
}
fn hash_file(path: &Path, digest: &mut Sha256, budget: &mut u64) -> Result<(), Status> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(domain("Git precondition metadata must be a regular file.")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            digest.update(b"absent");
            return Ok(());
        }
        Err(error) => return Err(domain(error)),
    }
    digest.update(b"present");
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            digest.update(b"absent");
            return Ok(());
        }
        Err(e) => return Err(domain(e)),
    };
    let metadata = file.metadata().map_err(domain)?;
    if !metadata.is_file() || metadata.len() > *budget {
        return Err(Status::resource_exhausted(
            "Git preflight content budget exceeds 256 MiB.",
        ));
    }
    digest.update(metadata.len().to_le_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        digest.update(metadata.permissions().mode().to_le_bytes());
    }
    let mut reader = file.take(*budget + 1);
    let mut buffer = [0; 64 * 1024];
    loop {
        let len = reader.read(&mut buffer).map_err(domain)?;
        if len == 0 {
            break;
        }
        *budget = budget
            .checked_sub(len as u64)
            .ok_or_else(|| Status::resource_exhausted("Git preflight content budget exceeded."))?;
        digest.update(&buffer[..len]);
    }
    Ok(())
}
