//! Project identity and persistence contract; filesystem validation runs on a worker.
use super::workspace::{ProjectId, WorkspaceId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeBase {
    /// The local branch used as the starting point when this worktree was created.
    pub reference: String,
    /// The exact commit resolved from `reference` at creation time.
    pub oid: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: ProjectId,
    pub workspace: WorkspaceId,
    pub name: String,
    pub path: PathBuf,
    /// Selected working directory; currently the project root, later a Git worktree.
    #[serde(default)]
    pub worktree_path: Option<PathBuf>,
    #[serde(default)]
    pub repository_path: Option<PathBuf>,
    /// Creation-time comparison hint. It is not proof that the branch is merged.
    #[serde(default)]
    pub worktree_base: Option<WorktreeBase>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projects {
    pub items: Vec<Project>,
    pub active: Option<WorkspaceId>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectSnapshot {
    pub paths: Vec<PathBuf>,
    pub active: Option<PathBuf>,
}
impl ProjectSnapshot {
    pub fn valid(&self) -> bool {
        self.paths.len() <= 128
            && self.paths.iter().all(|p| p.is_absolute())
            && self
                .paths
                .iter()
                .enumerate()
                .all(|(i, p)| !self.paths[..i].contains(p))
            && match &self.active {
                Some(p) => self.paths.contains(p),
                None => self.paths.is_empty(),
            }
    }
}
impl Projects {
    pub fn current(&self) -> Option<&Project> {
        self.items.iter().find(|p| Some(p.workspace) == self.active)
    }
    /// The caller supplies a canonical, accessible directory.
    pub fn open(&mut self, path: PathBuf) -> WorkspaceId {
        if let Some(project) = self.items.iter().find(|p| p.path == path) {
            self.active = Some(project.workspace);
            return project.workspace;
        }
        let workspace = WorkspaceId::new();
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
        self.items.push(Project {
            id: ProjectId::new(),
            repository_path: None,
            worktree_base: None,
            worktree_path: Some(path.clone()),
            workspace,
            name,
            path,
        });
        self.active = Some(workspace);
        workspace
    }
    pub fn open_worktree(&mut self, repository: PathBuf, path: PathBuf) -> WorkspaceId {
        self.open_worktree_with_base(repository, path, None)
    }
    pub fn open_worktree_with_base(
        &mut self,
        repository: PathBuf,
        path: PathBuf,
        base: Option<WorktreeBase>,
    ) -> WorkspaceId {
        let workspace = self.open(path);
        if let Some(project) = self.items.iter_mut().find(|p| p.workspace == workspace) {
            project.repository_path = Some(repository);
            if base.is_some() {
                project.worktree_base = base;
            }
        }
        workspace
    }
    pub fn select(&mut self, id: WorkspaceId) -> bool {
        if !self.items.iter().any(|p| p.workspace == id) {
            return false;
        }
        self.active = Some(id);
        true
    }
    pub fn close(&mut self, id: WorkspaceId) -> bool {
        let Some(index) = self.items.iter().position(|p| p.workspace == id) else {
            return false;
        };
        self.items.remove(index);
        if self.active == Some(id) {
            self.active = self
                .items
                .get(index.min(self.items.len().saturating_sub(1)))
                .map(|p| p.workspace);
        }
        true
    }
    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            paths: self.items.iter().map(|p| p.path.clone()).collect(),
            active: self.current().map(|p| p.path.clone()),
        }
    }
}
pub fn canonical_directory(path: &Path) -> std::io::Result<PathBuf> {
    let path = std::fs::canonicalize(path)?;
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "Select a folder",
        ));
    }
    // Verify directory enumeration permission without reading the contents into memory.
    let _ = std::fs::read_dir(&path)?;
    Ok(path)
}
/// Skip unavailable folders at startup, without rewriting the saved snapshot.
pub fn restore(snapshot: ProjectSnapshot) -> (Projects, usize) {
    let mut projects = Projects::default();
    let mut skipped = 0;
    let active = snapshot
        .active
        .as_ref()
        .and_then(|p| canonical_directory(p).ok());
    for path in snapshot.paths {
        match canonical_directory(&path) {
            Ok(path) => {
                projects.open(path);
            }
            Err(_) => skipped += 1,
        }
    }
    if let Some(active) = active
        && let Some(project) = projects.items.iter().find(|p| p.path == active)
    {
        projects.active = Some(project.workspace);
    }
    (projects, skipped)
}
