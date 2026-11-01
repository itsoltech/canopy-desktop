//! Stable typed attachment intent is deduped before host context is resolved.
use super::*;
use crate::{
    chat::{session::Draft, transcript::Attachment},
    remote::{auth::WorkspaceRoots, composer::PullContexts},
};
use std::path::{Component, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullIntent {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub head: String,
    pub base: String,
    pub state: String,
    pub body: String,
    pub context_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AttachmentIntent {
    Value(Attachment),
    Path {
        workspace: Option<(HostId, String)>,
        relative: String,
        label: Option<String>,
    },
    ThreadPath {
        project_file: bool,
        relative: String,
        label: Option<String>,
    },
    Image {
        host: HostId,
        artifact: String,
        sha256: Vec<u8>,
    },
    Pull {
        value: PullIntent,
        label: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DraftIntent {
    /// A draft already owned by the local process, never accepted from a device.
    Ready(QueueDraft),
    Incoming {
        text: String,
        context: Option<String>,
        goal: bool,
        attachments: Vec<AttachmentIntent>,
    },
}
/// Host-owned lookup capabilities; no request supplies this authority.
#[derive(Clone)]
pub struct DraftResources {
    pub roots: WorkspaceRoots,
    pub pulls: PullContexts,
}
fn relative(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty() || !path.components().all(|p| matches!(p, Component::Normal(_)))
    {
        return Err(Error::InvalidRuntimeState);
    }
    Ok(path)
}
impl DraftIntent {
    pub(super) fn has_workspace_files(&self) -> bool {
        matches!(self, Self::Incoming { attachments, .. } if attachments.iter().any(|a| matches!(a, AttachmentIntent::Path { workspace: Some(_), .. } | AttachmentIntent::ThreadPath { project_file: true, .. })))
    }
    pub(super) fn normalize(&mut self) -> Result<()> {
        match self {
            Self::Ready(draft) => {
                if !draft.valid() {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            Self::Incoming {
                text, attachments, ..
            } => {
                if text.len() > host::CONTROL_BYTES
                    || attachments.len() > 100
                    || text.trim().is_empty() && attachments.is_empty()
                {
                    return Err(Error::InvalidRuntimeState);
                }
                for attachment in attachments {
                    match attachment {
                        AttachmentIntent::Path {
                            relative: path,
                            label,
                            ..
                        }
                        | AttachmentIntent::ThreadPath {
                            relative: path,
                            label,
                            ..
                        } => {
                            *path = relative(path)?.to_string_lossy().into_owned();
                            if label.as_ref().is_some_and(String::is_empty) {
                                *label = None;
                            }
                        }
                        AttachmentIntent::Image {
                            artifact, sha256, ..
                        } if !host::valid_id(artifact) || sha256.len() != 32 => {
                            return Err(Error::InvalidRuntimeState);
                        }
                        AttachmentIntent::Value(
                            Attachment::Image { .. }
                            | Attachment::Path { .. }
                            | Attachment::Pull { .. },
                        ) => {
                            return Err(Error::InvalidRuntimeState);
                        }
                        _ => (),
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn resolve(
        &self,
        prepared: &PreparedCommand,
        resources: Option<&DraftResources>,
    ) -> Result<QueueDraft> {
        let Self::Incoming {
            text,
            context,
            goal,
            attachments,
        } = self
        else {
            let Self::Ready(draft) = self else {
                unreachable!()
            };
            if prepared.input.key.device != host::LOCAL_PRINCIPAL {
                return Err(Error::NotAuthorized);
            }
            return Ok(draft.clone());
        };
        let record = prepared.record.as_ref().ok_or(Error::NotFound)?;
        let mut values = Vec::new();
        let mut images = Vec::new();
        for attachment in attachments {
            match attachment {
                AttachmentIntent::ThreadPath {
                    project_file,
                    relative: path,
                    label,
                } => {
                    let root = if *project_file {
                        let workspace = record.workspace().ok_or(Error::ResourceConflict)?;
                        let roots = &resources.ok_or(Error::InvalidRuntimeState)?.roots;
                        let root = roots.resolve_host_workspace(workspace)?;
                        if root.to_str() != Some(record.authority_root()) {
                            return Err(Error::NotAuthorized);
                        }
                        root
                    } else {
                        if record.workspace().is_some() {
                            return Err(Error::ResourceConflict);
                        }
                        record.metadata().cwd.clone().ok_or(Error::NotFound)?
                    };
                    values.push(Attachment::Path {
                        path: root.join(relative(path)?),
                        label: label.clone(),
                    });
                }
                AttachmentIntent::Image { host, artifact, .. } => {
                    if host != prepared.envelope.resource.host_id() {
                        return Err(Error::NotAuthorized);
                    }
                    images.push(artifact.clone());
                }
                AttachmentIntent::Path {
                    workspace,
                    relative: path,
                    label,
                } => {
                    let root = if let Some((host, workspace)) = workspace {
                        if host != prepared.envelope.resource.host_id()
                            || record.workspace() != Some(workspace.as_str())
                        {
                            return Err(Error::NotAuthorized);
                        }
                        // Prepared admission already checked current grants for this
                        // canonical root. A caller cannot name another checkout.
                        let roots = &resources.ok_or(Error::InvalidRuntimeState)?.roots;
                        let root = roots.resolve_host_workspace(workspace)?;
                        if root.to_str() != Some(record.authority_root()) {
                            return Err(Error::NotAuthorized);
                        }
                        root
                    } else {
                        if record.workspace().is_some() {
                            return Err(Error::InvalidRuntimeState);
                        }
                        record.metadata().cwd.clone().ok_or(Error::NotFound)?
                    };
                    values.push(Attachment::Path {
                        path: root.join(relative(path)?),
                        label: label.clone(),
                    });
                }
                AttachmentIntent::Value(Attachment::Command {
                    name,
                    skill,
                    path,
                    description,
                    label,
                }) => {
                    if !crate::chat::config::valid_command(name) {
                        return Err(Error::InvalidRuntimeState);
                    }
                    let path =
                        path.as_ref()
                            .map(|path| {
                                let root = record.metadata().cwd.as_ref().ok_or(Error::NotFound)?;
                                let path = if path.is_absolute() {
                                    path.strip_prefix(root)
                                        .map_err(|_| Error::NotAuthorized)?
                                        .to_owned()
                                } else {
                                    path.clone()
                                };
                                Ok::<_, Error>(root.join(relative(
                                    path.to_str().ok_or(Error::InvalidRuntimeState)?,
                                )?))
                            })
                            .transpose()?;
                    values.push(Attachment::Command {
                        name: name.clone(),
                        skill: *skill,
                        path,
                        description: description.clone(),
                        label: label.clone(),
                    });
                }
                AttachmentIntent::Value(value) => values.push(value.clone()),
                AttachmentIntent::Pull { value, label } => {
                    let pull = crate::remote::proto::PullAttachment {
                        number: value.number,
                        title: value.title.clone(),
                        url: value.url.clone(),
                        head: value.head.clone(),
                        base: value.base.clone(),
                        state: value.state.clone(),
                        body: value.body.clone(),
                        context_id: value.context_id.clone(),
                    };
                    let pull = resources
                        .ok_or(Error::InvalidRuntimeState)?
                        .pulls
                        .resolve(record.authority_root(), pull)
                        .map_err(|_| Error::ResourceConflict)?;
                    values.push(Attachment::Pull {
                        number: pull.number,
                        title: pull.title,
                        url: pull.url,
                        head: pull.head,
                        base: pull.base,
                        state: pull.state,
                        body: pull.body,
                        label: label.clone(),
                    });
                }
            }
        }
        QueueDraft::from(Draft {
            text: text.clone(),
            context: context.clone(),
            goal: *goal,
            attachments: values,
        })
        .with_artifacts(images)
        .ok_or(Error::InvalidRuntimeState)
    }
}
pub(super) fn resolve_command(
    prepared: &PreparedCommand,
    resources: Option<&DraftResources>,
) -> Result<Dispatch> {
    Ok(match &prepared.envelope.command {
        Command::Rename { title } => Dispatch::Rename(title.clone()),
        Command::ThreadSend {
            message,
            draft,
            account,
            profile,
            history_epoch,
        } => {
            let record = prepared.record.as_ref().ok_or(Error::NotFound)?;
            Dispatch::Send {
                message: message.clone(),
                draft: draft.resolve(prepared, resources)?,
                context: SendContext::new(
                    account.clone(),
                    profile.clone(),
                    record.workspace().map(str::to_owned),
                    *history_epoch,
                )
                .ok_or(Error::InvalidRuntimeState)?,
                steer_run: None,
            }
        }
        Command::Send {
            message,
            draft,
            context,
            steer_run,
        } => Dispatch::Send {
            message: message.clone(),
            draft: draft.resolve(prepared, resources)?,
            context: context.clone(),
            steer_run: *steer_run,
        },
        Command::Queue { revision, change } => Dispatch::Queue {
            revision: *revision,
            mutation: match change {
                QueueChange::Edit { message, draft } => QueueMutation::Edit {
                    message: message.clone(),
                    draft: draft.resolve(prepared, resources)?,
                },
                QueueChange::Move { message, before } => QueueMutation::Move {
                    message: message.clone(),
                    before: before.clone(),
                },
                QueueChange::Remove { message } => QueueMutation::Remove {
                    message: message.clone(),
                },
                QueueChange::Pause { paused } => QueueMutation::Pause { paused: *paused },
                QueueChange::ResolveUnknown {
                    message,
                    resolution,
                } => QueueMutation::ResolveUnknown {
                    message: message.clone(),
                    resolution: resolution.clone(),
                },
            },
        },
        Command::Preferences { .. } | Command::Control(_) | Command::Catalog(_) => {
            return Err(Error::InvalidRuntimeState);
        }
    })
}
