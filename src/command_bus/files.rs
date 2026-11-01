use super::*;

/// Immutable content identity. Bytes remain in the existing artifact store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileContent {
    pub host: HostId,
    pub artifact: String,
    pub media_type: String,
    pub size: u64,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FileCommand {
    Save {
        path: String,
        expected_hash: [u8; 32],
        content: FileContent,
    },
    Create {
        path: String,
        directory: bool,
        content: Option<FileContent>,
    },
    Presence {
        path: String,
        base_hash: [u8; 32],
        dirty: bool,
    },
}

pub(super) static REGISTRY: &[CommandMetadata] = &[
    CommandMetadata::new("files.save", Risk::Delivery, host::FILES_GIT),
    CommandMetadata::new("files.create", Risk::Delivery, host::FILES_GIT),
    CommandMetadata::new("files.presence", Risk::Metadata, host::FILES_GIT),
];

impl FileCommand {
    pub fn content(&self) -> Option<&FileContent> {
        match self {
            Self::Save { content, .. } => Some(content),
            Self::Create { content, .. } => content.as_ref(),
            Self::Presence { .. } => None,
        }
    }
    pub fn path(&self) -> &str {
        match self {
            Self::Save { path, .. } | Self::Create { path, .. } | Self::Presence { path, .. } => {
                path
            }
        }
    }
    pub fn identity(&self, root: String) -> Result<host::FileIdentity> {
        host::FileIdentity::new(root, self.path().into()).ok_or(Error::InvalidRuntimeState)
    }
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[match self {
            Self::Save { .. } => 0,
            Self::Create { .. } => 1,
            Self::Presence { .. } => 2,
        }]
    }
    pub(super) fn normalize(&self) -> Result<()> {
        // Lexical validation only. Symlink and filesystem checks belong to the
        // existing resource owner after authorization and deduplication.
        if !host::FileIdentity::valid_relative_path(self.path()) {
            return Err(Error::InvalidRuntimeState);
        }
        if std::path::Path::new(self.path())
            .components()
            .any(|part| part.as_os_str() == ".git")
        {
            return Err(Error::NotAuthorized);
        }
        if matches!(
            self,
            Self::Create {
                directory: true,
                content: Some(_),
                ..
            }
        ) {
            return Err(Error::InvalidRuntimeState);
        }
        if self
            .content()
            .is_some_and(|content| content.size > crate::files::MAX_TEXT as u64)
        {
            return Err(Error::ResourceLimit);
        }
        if self.content().is_some_and(|content| {
            !host::valid_id(&content.artifact)
                || content.media_type.is_empty()
                || content.media_type.len() > 256
                || content.media_type.chars().any(char::is_control)
        }) {
            return Err(Error::InvalidRuntimeState);
        }
        Ok(())
    }
}
