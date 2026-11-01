use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSnapshot {
    pub head: String,
    pub index: [u8; 32],
    pub worktree: [u8; 32],
    pub preflight: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitPaths {
    Stage,
    Unstage,
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitIntent {
    Paths {
        operation: GitPaths,
        paths: Vec<String>,
    },
    Commit {
        message: String,
        sign: bool,
    },
    Pull {
        remote: String,
        upstream: Option<String>,
    },
    Push {
        remote: String,
        upstream: Option<String>,
    },
    Worktree(WorktreeIntent),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorktreeAction {
    Create,
    Remove,
    Merge,
    Stop,
    DeleteBranch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeIntent {
    pub action: WorktreeAction,
    pub branch: String,
    pub path: Option<String>,
    pub start_point: Option<String>,
    pub target_branch: Option<String>,
    pub delete_branch: bool,
    pub discard_changes: bool,
    pub delete_unmerged_branch: bool,
    pub acknowledge_offline_dirty: bool,
    /// The existing local metadata-only cleanup of a missing worktree. The
    /// registration identity prevents retry intent from becoming directory removal.
    pub registration_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitCommand {
    pub intent: GitIntent,
    pub expected: GitSnapshot,
}

/// Host resolution is retained with acceptance, independently of the immutable
/// client intent and its fingerprint. No request field supplies this authority.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedAuthority {
    version: u8,
    intent: String,
    pub(crate) roots: Vec<String>,
}

impl SavedAuthority {
    pub(crate) fn bind(input: &mut CommandInput, mut roots: Vec<String>) -> Result<()> {
        let source = input.root.as_deref().ok_or(Error::NotAuthorized)?;
        roots.push(source.into());
        roots.sort();
        roots.dedup();
        let value = Self {
            version: 1,
            intent: String::from_utf8(input.arguments.clone())
                .map_err(|_| Error::InvalidRuntimeState)?,
            roots,
        };
        value.validate(source)?;
        let bytes = serde_json::to_vec(&value).map_err(|_| Error::InvalidRuntimeState)?;
        if bytes.len() > host::CONTROL_BYTES {
            return Err(Error::ResourceLimit);
        }
        input.arguments = bytes;
        Ok(())
    }

    pub(crate) fn read(bytes: &[u8], source: &str) -> Result<Self> {
        if bytes.len() > host::CONTROL_BYTES {
            return Err(Error::NotAuthorized);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| Error::NotAuthorized)?;
        value.validate(source)?;
        Ok(value)
    }

    fn validate(&self, source: &str) -> Result<()> {
        if self.version != 1
            || self.roots.is_empty()
            || self.roots.len() > 129
            || !self.roots.iter().any(|root| root == source)
            || self.roots.windows(2).any(|pair| pair[0] >= pair[1])
            || self.roots.iter().any(|root| {
                root.len() > 4096
                    || root.chars().any(char::is_control)
                    || !std::path::Path::new(root).is_absolute()
                    || std::path::Path::new(root).components().any(|part| {
                        matches!(
                            part,
                            std::path::Component::ParentDir | std::path::Component::CurDir
                        )
                    })
            })
        {
            return Err(Error::NotAuthorized);
        }
        let (_, _, _, command, _): (String, String, u64, Command, Conditions) =
            serde_json::from_str(&self.intent).map_err(|_| Error::NotAuthorized)?;
        if !matches!(command, Command::Git(_)) {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }

    pub(crate) fn intent(&self) -> &[u8] {
        self.intent.as_bytes()
    }
}

pub(crate) fn client_arguments(input: &CommandInput) -> Result<Vec<u8>> {
    if input.method.starts_with("git.") {
        let saved = SavedAuthority::read(
            &input.arguments,
            input.root.as_deref().ok_or(Error::NotAuthorized)?,
        )?;
        Ok(saved.intent().into())
    } else if super::tasks::requires_account(&input.method) {
        Ok(super::tasks::AccountBinding::read(&input.arguments)?
            .intent()
            .into())
    } else {
        Ok(input.arguments.clone())
    }
}

pub(crate) fn same_arguments(saved: &[u8], current: &CommandInput) -> bool {
    let Some(root) = current.root.as_deref() else {
        return false;
    };
    let Ok(saved) = SavedAuthority::read(saved, root) else {
        return false;
    };
    let incoming = client_arguments(current).unwrap_or_else(|_| current.arguments.clone());
    saved.intent() == incoming
}

pub(super) static REGISTRY: &[CommandMetadata] = &[
    entry("git.stage"),
    entry("git.unstage"),
    entry("git.discard"),
    entry("git.commit"),
    entry("git.pull"),
    entry("git.push"),
    entry("git.worktree"),
];

const fn entry(name: &'static str) -> CommandMetadata {
    CommandMetadata::new(name, Risk::Delivery, host::FILES_GIT)
}

impl GitCommand {
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[match &self.intent {
            GitIntent::Paths { operation, .. } => match operation {
                GitPaths::Stage => 0,
                GitPaths::Unstage => 1,
                GitPaths::Discard => 2,
            },
            GitIntent::Commit { .. } => 3,
            GitIntent::Pull { .. } => 4,
            GitIntent::Push { .. } => 5,
            GitIntent::Worktree(_) => 6,
        }]
    }

    pub(super) fn normalize(&mut self) -> Result<()> {
        self.intent.normalize()?;
        if !self.expected.head.is_empty() && git2::Oid::from_str(&self.expected.head).is_err()
            || self
                .expected
                .preflight
                .as_ref()
                .is_some_and(|id| !host::valid_id(id))
        {
            return Err(Error::InvalidRuntimeState);
        }
        Ok(())
    }
}

impl GitIntent {
    pub(crate) fn normalize(&mut self) -> Result<()> {
        match self {
            Self::Paths { paths, .. } => {
                paths.sort();
                paths.dedup();
                if paths.is_empty() || paths.len() > 10_000 {
                    return Err(Error::InvalidRuntimeState);
                }
                for path in paths {
                    if !host::FileIdentity::valid_relative_path(path) {
                        return Err(Error::InvalidRuntimeState);
                    }
                    if std::path::Path::new(path)
                        .components()
                        .any(|p| p.as_os_str() == ".git")
                    {
                        return Err(Error::NotAuthorized);
                    }
                }
            }
            Self::Commit { message, .. } => {
                if message.trim().is_empty() || message.contains('\0') {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            Self::Pull { remote, upstream } | Self::Push { remote, upstream } => {
                if remote.is_empty()
                    || remote.contains('\0')
                    || upstream
                        .as_ref()
                        .is_some_and(|v| v.is_empty() || v.contains('\0'))
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            Self::Worktree(intent) => {
                if intent.branch.contains('\0')
                    || [
                        &intent.path,
                        &intent.start_point,
                        &intent.target_branch,
                        &intent.registration_name,
                    ]
                    .into_iter()
                    .flatten()
                    .any(|v| v.is_empty() || v.contains('\0'))
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::InvalidRuntimeState)?
            .len()
            > host::CONTROL_BYTES / 2
        {
            return Err(Error::ResourceLimit);
        }
        Ok(())
    }
}
