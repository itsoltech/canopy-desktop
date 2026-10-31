//! Thread identity (ADR 0001): the environment is only its host (D7), a thread has a
//! global UUIDv7 id (D8), and project resources outside a thread are keyed by
//! `ProjectRef` (D16). No type tells local and remote threads apart (D1).
use crate::state::workspace::ProjectId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The only identity of an environment: the host's existing stable ID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HostId(String);
impl HostId {
    pub fn new(id: String) -> Option<Self> {
        crate::host::valid_id(&id).then_some(Self(id))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for HostId {
    type Error = &'static str;
    fn try_from(id: String) -> Result<Self, Self::Error> {
        Self::new(id).ok_or("invalid host id")
    }
}
impl From<HostId> for String {
    fn from(id: HostId) -> Self {
        id.0
    }
}
impl fmt::Display for HostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A thread's global ID. Ordering follows creation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ThreadId(uuid::Uuid);
impl ThreadId {
    #[allow(
        clippy::new_without_default,
        reason = "each call mints a new identity; a default would hide that"
    )]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
    /// Only the canonical lowercase hyphenated form of a version 7 UUID, so one
    /// thread has exactly one spelling in storage and aliases.
    pub fn parse(id: &str) -> Option<Self> {
        let uuid = uuid::Uuid::try_parse(id).ok()?;
        (uuid.get_version() == Some(uuid::Version::SortRand)
            && uuid.get_variant() == uuid::Variant::RFC4122
            && uuid.hyphenated().to_string() == id)
            .then_some(Self(uuid))
    }
}
impl TryFrom<String> for ThreadId {
    type Error = &'static str;
    fn try_from(id: String) -> Result<Self, Self::Error> {
        Self::parse(&id).ok_or("invalid thread id")
    }
}
impl From<ThreadId> for String {
    fn from(id: ThreadId) -> Self {
        id.to_string()
    }
}
impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentRef {
    pub host_id: HostId,
}

/// A thread and the environment it was created in, which never changes (D5).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadRef {
    pub host_id: HostId,
    pub thread_id: ThreadId,
}
impl ThreadRef {
    pub fn environment(&self) -> EnvironmentRef {
        EnvironmentRef {
            host_id: self.host_id.clone(),
        }
    }
}

/// One checkout of a Git project on its host.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorktreeId(String);
impl WorktreeId {
    pub fn new(id: String) -> Option<Self> {
        (!id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)).then_some(Self(id))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectRefError {
    /// A project without Git has no checkout to name.
    WorktreeWithoutGit,
    /// A Git project's resources belong to one checkout, which must be named.
    MissingWorktree,
}

/// Files, terminal, Git and Tasks outside a thread (D16, RPC-13).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectRef {
    host_id: HostId,
    project_id: ProjectId,
    worktree_id: Option<WorktreeId>,
}
impl ProjectRef {
    /// Checked against whether the host's project uses Git, before any I/O.
    pub fn new(
        host_id: HostId,
        project_id: ProjectId,
        worktree_id: Option<WorktreeId>,
        uses_git: bool,
    ) -> Result<Self, ProjectRefError> {
        match (uses_git, &worktree_id) {
            (false, Some(_)) => Err(ProjectRefError::WorktreeWithoutGit),
            (true, None) => Err(ProjectRefError::MissingWorktree),
            _ => Ok(Self {
                host_id,
                project_id,
                worktree_id,
            }),
        }
    }
    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }
    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }
    pub fn worktree_id(&self) -> Option<&WorktreeId> {
        self.worktree_id.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_ids_accept_only_canonical_v7_and_sort_by_creation() {
        let first = ThreadId::new();
        let second = ThreadId::new();
        assert!(first < second);
        assert_eq!(ThreadId::parse(&first.to_string()), Some(first));
        let json = serde_json::to_string(&first).unwrap();
        assert_eq!(serde_json::from_str::<ThreadId>(&json).unwrap(), first);

        let v4 = uuid::Uuid::new_v4().to_string();
        let upper = first.to_string().to_uppercase();
        let simple = first.0.simple().to_string();
        for rejected in [
            v4.as_str(),
            &upper,
            &simple,
            "00000000-0000-7000-0000-000000000000",
            "",
            "not-a-uuid",
        ] {
            assert_eq!(ThreadId::parse(rejected), None, "{rejected}");
            assert!(serde_json::from_value::<ThreadId>(rejected.into()).is_err());
        }
    }

    #[test]
    fn project_ref_names_a_worktree_exactly_when_the_project_uses_git() {
        let host = HostId::new(uuid::Uuid::new_v4().to_string()).unwrap();
        let project = ProjectId::new();
        let worktree = WorktreeId::new("main".into());
        assert_eq!(
            ProjectRef::new(host.clone(), project, worktree.clone(), false),
            Err(ProjectRefError::WorktreeWithoutGit)
        );
        assert_eq!(
            ProjectRef::new(host.clone(), project, None, true),
            Err(ProjectRefError::MissingWorktree)
        );
        assert!(ProjectRef::new(host.clone(), project, None, false).is_ok());
        let git = ProjectRef::new(host, project, worktree.clone(), true).unwrap();
        assert_eq!(git.worktree_id(), worktree.as_ref());
    }
}
