//! Durable bulk-transfer boundaries. Chunk bytes stay in the artifact owner.
use super::{CommandMetadata, Risk};
use crate::{
    artifacts, host,
    settings::{Error, Result},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadContent {
    pub size: u64,
    pub sha256: [u8; 32],
    pub media_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadOrigin {
    pub command: String,
    pub epoch: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum UploadDestination {
    Image,
    File {
        path: String,
    },
    Task {
        integration: String,
        project: String,
        task: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ArtifactCommand {
    Begin {
        destination: UploadDestination,
        content: UploadContent,
    },
    Finish {
        destination: UploadDestination,
        upload: String,
        origin: UploadOrigin,
    },
    Cancel {
        destination: UploadDestination,
        upload: String,
        origin: UploadOrigin,
    },
}
pub(super) static REGISTRY: &[CommandMetadata] = &[
    metadata("artifacts.image.begin", host::CHAT),
    metadata("artifacts.file.begin", host::FILES_GIT),
    metadata("artifacts.task.begin", host::TASKS),
    metadata("artifacts.image.finish", host::CHAT),
    metadata("artifacts.file.finish", host::FILES_GIT),
    metadata("artifacts.task.finish", host::TASKS),
    metadata("artifacts.image.cancel", host::CHAT),
    metadata("artifacts.file.cancel", host::FILES_GIT),
    metadata("artifacts.task.cancel", host::TASKS),
];
const fn metadata(name: &'static str, scope: u32) -> CommandMetadata {
    CommandMetadata::new(name, Risk::Delivery, scope)
}
impl ArtifactCommand {
    pub fn destination(&self) -> &UploadDestination {
        match self {
            Self::Begin { destination, .. }
            | Self::Finish { destination, .. }
            | Self::Cancel { destination, .. } => destination,
        }
    }
    pub fn metadata(&self) -> &'static CommandMetadata {
        let operation = match self {
            Self::Begin { .. } => 0,
            Self::Finish { .. } => 3,
            Self::Cancel { .. } => 6,
        };
        let kind = match self.destination() {
            UploadDestination::Image => 0,
            UploadDestination::File { .. } => 1,
            UploadDestination::Task { .. } => 2,
        };
        &REGISTRY[operation + kind]
    }
    pub(super) fn normalize(&mut self) -> Result<()> {
        match self.destination() {
            UploadDestination::Image => (),
            UploadDestination::File { path } => {
                if path.is_empty()
                    || path.len() > 16384
                    || path.contains('\0')
                    || !std::path::Path::new(path)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            UploadDestination::Task {
                integration,
                project,
                task,
            } => {
                if integration.is_empty()
                    || integration.len() > 256
                    || project.len() > 4096
                    || task.is_empty()
                    || task.len() > 4096
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
        }
        match self {
            Self::Begin {
                destination,
                content,
            } => {
                if content.media_type.len() > 256
                    || content.size > artifacts::MAX_STORAGE_BYTES
                    || (matches!(destination, UploadDestination::Image)
                        && content.size > artifacts::MAX_IMAGE_BYTES)
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            Self::Finish { upload, origin, .. } | Self::Cancel { upload, origin, .. } => {
                if !artifacts::valid_id(upload)
                    || !host::valid_id(&origin.command)
                    || origin.epoch == 0
                    || origin.epoch >= i64::MAX as u64
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
        }
        Ok(())
    }
}
