//! BUS-01..04: typed mutation admission shared by desktop and RPC adapters.
//!
//! The concrete enum and the existing SQLite writer replace adapter-owned scope,
//! fingerprint and retry checks. There is no second inbox, worker or repository.
//! Pure reads stay outside this module. Secret-bearing settings are not migrated.
mod drafts;
pub(crate) mod preferences;
pub use drafts::{AttachmentIntent, DraftIntent, DraftResources, PullIntent};
pub use preferences::{PreferenceWrite, SettingsRevision};

use crate::{
    host::{
        self, ChatRecord, CommandInput, CommandKey, CommandReceipt, DeviceGrant, QueueDraft,
        QueueMutation, SendContext, UnknownResolution,
    },
    settings::{Change, Error, Result, SettingsClient},
    thread::{EnvironmentRef, HostId, ProjectRef, ThreadRef},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// D10's admission hook. Current local-owner and device grants implement Allow or
/// Deny; risk metadata is also available to the later scoped Ask policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    Metadata,
    Delivery,
    Configuration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandMetadata {
    pub name: &'static str,
    pub risk: Risk,
    pub scope: u32,
    pub workspace_attachment_scope: u32,
}
pub static REGISTRY: &[CommandMetadata] = &[
    CommandMetadata {
        name: "chat.rename",
        risk: Risk::Metadata,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "chat.send",
        risk: Risk::Delivery,
        scope: host::CHAT,
        workspace_attachment_scope: host::READ,
    },
    CommandMetadata {
        name: "chat.queue.edit",
        risk: Risk::Metadata,
        scope: host::CHAT,
        workspace_attachment_scope: host::READ,
    },
    CommandMetadata {
        name: "chat.queue.move",
        risk: Risk::Metadata,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "chat.queue.remove",
        risk: Risk::Metadata,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "chat.queue.pause",
        risk: Risk::Metadata,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "chat.queue.resolve_unknown",
        risk: Risk::Delivery,
        scope: host::CHAT,
        workspace_attachment_scope: 0,
    },
    CommandMetadata {
        name: "settings.preferences",
        risk: Risk::Configuration,
        scope: host::ADMIN,
        workspace_attachment_scope: 0,
    },
];
pub fn registry() -> impl ExactSizeIterator<Item = &'static CommandMetadata> {
    REGISTRY.iter()
}

/// CMD-03: supplied by the authenticated transport or the in-process owner,
/// never deserialized from a request. The local owner is not a remote credential.
#[derive(Clone, Debug)]
pub struct Principal {
    device: String,
    command_epoch: u64,
}
impl Principal {
    pub fn local_owner() -> Self {
        Self {
            device: host::LOCAL_PRINCIPAL.into(),
            command_epoch: 0,
        }
    }
    pub fn authenticated(grant: &DeviceGrant, command_epoch: u64) -> Result<Self> {
        CommandKey::remote(
            grant.id().into(),
            command_epoch,
            uuid::Uuid::new_v4().to_string(),
        )
        .ok_or(Error::NotAuthorized)?;
        if grant.revoked() {
            return Err(Error::NotAuthorized);
        }
        Ok(Self {
            device: grant.id().into(),
            command_epoch,
        })
    }
    /// Bridge for existing authenticated callers while legacy RPCs are replaced.
    pub fn from_authenticated_key(key: &CommandKey) -> Self {
        Self {
            device: key.device().into(),
            command_epoch: key.epoch(),
        }
    }
}

/// CMD-01/D16 resource identity, independent of transport DTOs.
#[derive(Clone, Debug)]
pub enum ResourceRef {
    Thread {
        thread: ThreadRef,
        executor_epoch: u64,
    },
    Project(ProjectRef),
    Environment(EnvironmentRef),
}
impl ResourceRef {
    pub fn host_id(&self) -> &HostId {
        match self {
            Self::Thread { thread, .. } => &thread.host_id,
            Self::Project(p) => p.host_id(),
            Self::Environment(e) => &e.host_id,
        }
    }
    pub(crate) fn storage_key(&self) -> (String, u64) {
        match self {
            Self::Thread {
                thread,
                executor_epoch,
            } => (thread.thread_id.to_string(), *executor_epoch),
            Self::Environment(e) => (format!("environment:{}", e.host_id), 0),
            Self::Project(p) => (
                format!(
                    "project:{}:{:?}:{}",
                    p.host_id(),
                    p.project_id(),
                    p.worktree_id().map_or("", |w| w.as_str())
                ),
                0,
            ),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    pub expected_revision: Option<u64>,
    pub workspace_host: Option<HostId>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum QueueChange {
    Edit {
        message: String,
        draft: DraftIntent,
    },
    Move {
        message: String,
        before: Option<String>,
    },
    Remove {
        message: String,
    },
    Pause {
        paused: bool,
    },
    ResolveUnknown {
        message: String,
        resolution: UnknownResolution,
    },
}
impl From<QueueMutation> for QueueChange {
    fn from(m: QueueMutation) -> Self {
        match m {
            QueueMutation::Edit { message, draft } => Self::Edit {
                message,
                draft: DraftIntent::Ready(draft),
            },
            QueueMutation::Move { message, before } => Self::Move { message, before },
            QueueMutation::Remove { message } => Self::Remove { message },
            QueueMutation::Pause { paused } => Self::Pause { paused },
            QueueMutation::ResolveUnknown {
                message,
                resolution,
            } => Self::ResolveUnknown {
                message,
                resolution,
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Command {
    Rename {
        title: String,
    },
    Send {
        message: String,
        draft: DraftIntent,
        context: SendContext,
        steer_run: Option<u64>,
    },
    Queue {
        revision: u64,
        change: QueueChange,
    },
    Preferences {
        changes: Vec<PreferenceWrite>,
    },
}
impl Command {
    pub fn required_scope(&self) -> u32 {
        let workspace_files = match self {
            Self::Send { draft, .. }
            | Self::Queue {
                change: QueueChange::Edit { draft, .. },
                ..
            } => draft.has_workspace_files(),
            _ => false,
        };
        self.metadata().scope
            | if workspace_files {
                self.metadata().workspace_attachment_scope
            } else {
                0
            }
    }
    pub fn preferences(changes: Vec<Change>) -> Result<Self> {
        Ok(Self::Preferences {
            changes: PreferenceWrite::from_changes(changes)?,
        })
    }
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[match self {
            Self::Rename { .. } => 0,
            Self::Send { .. } => 1,
            Self::Queue { change, .. } => match change {
                QueueChange::Edit { .. } => 2,
                QueueChange::Move { .. } => 3,
                QueueChange::Remove { .. } => 4,
                QueueChange::Pause { .. } => 5,
                QueueChange::ResolveUnknown { .. } => 6,
            },
            Self::Preferences { .. } => 7,
        }]
    }
    fn normalize(&mut self) -> Result<()> {
        match self {
            Self::Rename { title } => {
                *title = title.trim().into();
                if title.is_empty() || title.len() > 256 || title.chars().any(char::is_control) {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            Self::Send { message, draft, .. } => {
                if !host::valid_id(message) {
                    return Err(Error::InvalidRuntimeState);
                }
                draft.normalize()?;
            }
            Self::Queue {
                change: QueueChange::Edit { draft, .. },
                ..
            } => draft.normalize()?,
            Self::Preferences { changes } => PreferenceWrite::normalize(changes)?,
            _ => (),
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct CommandEnvelope {
    pub command_id: uuid::Uuid,
    pub resource: ResourceRef,
    pub command: Command,
    pub conditions: Conditions,
}
impl CommandEnvelope {
    pub(crate) fn input(&mut self, principal: &Principal, target: String) -> Result<CommandInput> {
        if self
            .conditions
            .workspace_host
            .as_ref()
            .is_some_and(|host| host != self.resource.host_id())
        {
            return Err(Error::NotAuthorized);
        }
        self.command.normalize()?;
        let (resource, executor_epoch) = self.resource.storage_key();
        let mut key = if principal.device == host::LOCAL_PRINCIPAL {
            CommandKey::local(self.command_id.to_string())
        } else {
            CommandKey::remote(
                principal.device.clone(),
                principal.command_epoch,
                self.command_id.to_string(),
            )
        }
        .ok_or(Error::NotAuthorized)?;
        key.resource = resource;
        key.executor_epoch = executor_epoch;
        // Decoded meaning, with explicit method, resource and conditions. Neither
        // command identity nor volatile resolution (e.g. a file's contents) enters it.
        let arguments = serde_json::to_vec(&(
            self.command.metadata().name,
            &key.resource,
            executor_epoch,
            &self.command,
            &self.conditions,
        ))
        .map_err(|_| Error::InvalidRuntimeState)?;
        let fingerprint = Sha256::digest(&arguments).to_vec();
        CommandInput::new(
            key,
            self.command.metadata().name.into(),
            target,
            arguments,
            fingerprint,
        )
        .ok_or(Error::ResourceLimit)
    }
}

/// BUS-02/CMD-04: only the writer can construct an authorized, deduped dispatch.
pub struct PreparedCommand {
    pub(crate) input: CommandInput,
    pub(crate) envelope: CommandEnvelope,
    pub(crate) record: Option<ChatRecord>,
    pub(crate) prior: Option<CommandReceipt>,
}
impl PreparedCommand {
    pub fn prior(&self) -> Option<&CommandReceipt> {
        self.prior.as_ref()
    }
    pub(crate) fn resolve(&self, resources: Option<&DraftResources>) -> Result<Dispatch> {
        drafts::resolve_command(self, resources)
    }
}
#[derive(Clone)]
pub struct CommandBus {
    client: SettingsClient,
}
impl CommandBus {
    pub fn new(client: SettingsClient) -> Self {
        Self { client }
    }
    /// Legacy adapters resolve only the durable alias. Deleted targets can still
    /// return their retained receipt. Epoch 0 is the unreleased legacy executor.
    pub async fn legacy_envelope(
        &self,
        key: &CommandKey,
        host: HostId,
        chat: String,
        command: Command,
        conditions: Conditions,
    ) -> Result<CommandEnvelope> {
        let thread_id = self
            .client
            .runtime_thread_alias(host.clone(), chat)
            .await?
            .ok_or(Error::NotFound)?;
        Ok(CommandEnvelope {
            command_id: uuid::Uuid::parse_str(key.id()).map_err(|_| Error::InvalidRuntimeState)?,
            resource: ResourceRef::Thread {
                thread: ThreadRef {
                    host_id: host,
                    thread_id,
                },
                executor_epoch: 0,
            },
            command,
            conditions,
        })
    }
    pub async fn prepare(
        &self,
        envelope: CommandEnvelope,
        principal: Principal,
    ) -> Result<PreparedCommand> {
        self.client.prepare_bus_command(envelope, principal).await
    }
    pub async fn execute(
        &self,
        envelope: CommandEnvelope,
        principal: Principal,
        resources: Option<&DraftResources>,
    ) -> Result<CommandReceipt> {
        let prepared = self.prepare(envelope, principal).await?;
        if let Some(prior) = prepared.prior {
            return Ok(prior);
        }
        let command = prepared.resolve(resources)?;
        self.client.dispatch_bus_command(prepared, command).await
    }
    /// Existing runtime entry points already own a normalized local send/queue.
    /// They enter the same typed bus as decoded RPCs, with no wire fingerprint.
    pub async fn execute_local(
        &self,
        key: CommandKey,
        host: HostId,
        chat: String,
        command: Command,
    ) -> Result<CommandReceipt> {
        let principal = Principal::from_authenticated_key(&key);
        let envelope = self
            .legacy_envelope(&key, host, chat, command, Conditions::default())
            .await?;
        self.execute(envelope, principal, None).await
    }
    pub async fn queue_local(
        &self,
        key: CommandKey,
        host: HostId,
        chat: String,
        revision: u64,
        mutation: QueueMutation,
    ) -> Result<CommandReceipt> {
        self.execute_local(
            key,
            host,
            chat,
            Command::Queue {
                revision,
                change: mutation.into(),
            },
        )
        .await
    }
    pub async fn commit_preferences(
        &self,
        prepared: PreparedCommand,
        current_revision: u64,
    ) -> Result<(CommandReceipt, Option<crate::settings::Snapshot>)> {
        self.client
            .commit_bus_preferences(prepared, current_revision)
            .await
    }
}

/// Resolved arguments are host-owned; the retained intent remains the original
/// stable request. Only the bus can construct a dispatch after current auth/dedupe.
pub(crate) enum Dispatch {
    Rename(String),
    Send {
        message: String,
        draft: QueueDraft,
        context: SendContext,
        steer_run: Option<u64>,
    },
    Queue {
        revision: u64,
        mutation: QueueMutation,
    },
}
