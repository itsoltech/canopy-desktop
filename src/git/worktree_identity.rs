use super::{Repository, WorktreeInfo};
use std::path::{Path, PathBuf};

/// Keep the same workspace identity after its leaf directory disappears, e.g.
/// when Git registered /tmp/foo but Canopy opened it as /private/tmp/foo.
pub(super) fn canonical_worktree_path(path: &Path) -> PathBuf {
    for ancestor in path.ancestors() {
        if let Ok(parent) = ancestor.canonicalize() {
            if ancestor == path {
                return parent;
            }
            return parent.join(path.strip_prefix(ancestor).expect("ancestor prefix"));
        }
    }
    path.to_owned()
}

pub(super) fn same_worktree_path(left: &Path, right: &Path) -> bool {
    canonical_worktree_path(left) == canonical_worktree_path(right)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorktreeHead {
    Branch(String),
    Unborn(String),
    Detached(String),
    Unavailable,
    Unknown,
}
impl WorktreeHead {
    pub fn branch_name(&self) -> Option<&str> {
        match self {
            Self::Branch(name) | Self::Unborn(name) => Some(name),
            _ => None,
        }
    }
    pub fn description(&self) -> String {
        match self {
            Self::Branch(name) => name.clone(),
            Self::Unborn(name) => format!("{name} · no commits yet"),
            Self::Detached(commit) => format!(
                "Detached HEAD · {}",
                commit.chars().take(8).collect::<String>()
            ),
            Self::Unavailable => "Worktree unavailable".into(),
            Self::Unknown => "Git HEAD unavailable".into(),
        }
    }
}
pub(super) fn read_head(repo: &Repository) -> WorktreeHead {
    match repo.head() {
        Ok(head) if head.is_branch() => head
            .name()
            .ok()
            .and_then(|name| name.strip_prefix("refs/heads/"))
            .map(|name| WorktreeHead::Branch(name.into()))
            .unwrap_or(WorktreeHead::Unknown),
        Ok(head) => head
            .target()
            .map(|oid| WorktreeHead::Detached(oid.to_string()))
            .unwrap_or(WorktreeHead::Unknown),
        Err(error)
            if error.code() == git2::ErrorCode::UnbornBranch
                || error.code() == git2::ErrorCode::NotFound =>
        {
            repo.find_reference("HEAD")
                .ok()
                .and_then(|head| {
                    head.symbolic_target()
                        .ok()
                        .flatten()
                        .and_then(|s| s.strip_prefix("refs/heads/"))
                        .map(str::to_owned)
                })
                .map(WorktreeHead::Unborn)
                .unwrap_or(WorktreeHead::Unknown)
        }
        Err(_) => WorktreeHead::Unknown,
    }
}
impl WorktreeInfo {
    pub fn branch_name(&self) -> Option<&str> {
        self.head.branch_name()
    }
    pub fn checkout_branch(&self) -> Option<&str> {
        self.branch_name().or(self.registered_branch.as_deref())
    }
    pub fn tooltip(&self) -> String {
        format!(
            "{}\n{}{}",
            crate::platform::paths::display(&self.path),
            if self.missing {
                "Worktree directory is missing".into()
            } else {
                self.head.description()
            },
            if self.locked { " · locked" } else { "" }
        )
    }
}
fn directory_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
fn contextual_label(base: &str, path: &Path, depth: usize) -> String {
    let Some(parent) = path.parent() else {
        return path.display().to_string();
    };
    let parts = parent
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let start = parts.len().saturating_sub(depth);
    format!("{base} ({})", parts[start..].join("/"))
}
/// Prepare short but distinguishable display labels without changing Git or workspace identity.
pub(super) fn set_labels(root: &Path, worktrees: &mut [WorktreeInfo]) {
    let base = worktrees
        .iter()
        .map(|w| {
            w.branch_name()
                .map(str::to_owned)
                .unwrap_or_else(|| directory_name(&w.path))
        })
        .collect::<Vec<_>>();
    let root_name = directory_name(root);
    let mut depth = worktrees
        .iter()
        .zip(&base)
        .map(|(w, label)| {
            usize::from(w.branch_name().is_none() && label == &root_name && w.path != root)
        })
        .collect::<Vec<_>>();
    loop {
        for (i, w) in worktrees.iter_mut().enumerate() {
            w.label = if depth[i] == 0 {
                base[i].clone()
            } else {
                contextual_label(&base[i], &w.path, depth[i])
            };
        }
        let mut changed = false;
        for i in 0..worktrees.len() {
            if worktrees
                .iter()
                .enumerate()
                .any(|(j, w)| j != i && w.label == worktrees[i].label)
                && depth[i] < worktrees[i].path.components().count()
            {
                depth[i] += 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}
