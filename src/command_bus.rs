//! BUS-01..04: typed mutation admission shared by desktop and RPC adapters.
//!
//! The concrete enum and the existing SQLite writer replace adapter-owned scope,
//! fingerprint and retry checks. There is no second inbox, worker or repository.
//! Pure reads stay outside this module. Secret bytes stay with the existing vault.
mod admin;
pub use admin::AdminCommand;
mod artifacts;
mod catalog;
pub use artifacts::{ArtifactCommand, UploadContent, UploadDestination, UploadOrigin};
mod control;
pub use catalog::CatalogCommand;
mod drafts;
mod files;
mod settings;
pub use crate::remote::settings_rpc::SettingsCommand;
pub mod tasks;
mod terminal;
pub use tasks::TaskCommand;
pub use terminal::TerminalCommand;
pub(crate) mod git;
pub use control::{Control, DecisionAnswer, LocalForkDestination};
pub use files::{FileCommand, FileContent};
pub use git::{GitCommand, GitIntent, GitPaths, GitSnapshot, WorktreeAction, WorktreeIntent};
mod peer;
pub(crate) mod preferences;
pub use drafts::{AttachmentIntent, DraftIntent, DraftResources, PullIntent};
pub use peer::PeerConnection;
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
impl CommandMetadata {
    /// Entry for commands that carry no workspace attachments.
    pub(crate) const fn new(name: &'static str, risk: Risk, scope: u32) -> Self {
        Self {
            name,
            risk,
            scope,
            workspace_attachment_scope: 0,
        }
    }
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
pub fn registry() -> impl Iterator<Item = &'static CommandMetadata> {
    REGISTRY
        .iter()
        .chain(control::REGISTRY)
        .chain(catalog::REGISTRY)
        .chain(files::REGISTRY)
        .chain(git::REGISTRY)
        .chain(terminal::REGISTRY)
        .chain(artifacts::REGISTRY)
        .chain(settings::REGISTRY)
        .chain(admin::REGISTRY)
        .chain(peer::REGISTRY)
        .chain(tasks::REGISTRY)
}

/// CMD-03: supplied by the authenticated transport or the in-process owner,
/// never deserialized from a request. The local owner is not a remote credential.
#[derive(Clone, Debug)]
pub struct Principal {
    device: String,
    command_epoch: u64,
}
impl Principal {
    pub(crate) fn is_local_owner(&self) -> bool {
        self.device == host::LOCAL_PRINCIPAL && self.command_epoch == 0
    }
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
    /// Preserve an already authenticated in-process operation identity.
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
    /// Retained command inspection must not require a live catalog entry.
    pub(crate) fn from_storage_key(host: HostId, key: &str, executor_epoch: u64) -> Option<Self> {
        let resource = if key == format!("environment:{host}") {
            Self::Environment(EnvironmentRef { host_id: host })
        } else if let Some(thread_id) = crate::thread::ThreadId::parse(key) {
            Self::Thread {
                thread: ThreadRef {
                    host_id: host,
                    thread_id,
                },
                executor_epoch,
            }
        } else {
            let project = key.strip_prefix(&format!("project:{host}:"))?;
            let (project_id, worktree) = project.split_once(':')?;
            let project_id = project_id.strip_prefix("ProjectId(")?.strip_suffix(')')?;
            let project_id = serde_json::from_str(project_id).ok()?;
            let worktree = if worktree.is_empty() {
                None
            } else {
                Some(crate::thread::WorktreeId::new(worktree.into())?)
            };
            Self::Project(
                ProjectRef::new(host, project_id, worktree.clone(), worktree.is_some()).ok()?,
            )
        };
        (resource.storage_key() == (key.into(), executor_epoch)).then_some(resource)
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
    Task(TaskCommand),
    Peer(PeerConnection),
    Admin(AdminCommand),
    Settings(SettingsCommand),
    Artifact(ArtifactCommand),
    Terminal(TerminalCommand),
    File(FileCommand),
    Git(GitCommand),
    Catalog(CatalogCommand),
    Control(Control),
    /// Restoring host-adopted launch context is the local form of choosing an agent.
    Recovery {
        record: Box<ChatRecord>,
        expected_reason: String,
    },
    Rename {
        title: String,
    },
    Send {
        message: String,
        draft: DraftIntent,
        context: SendContext,
        steer_run: Option<u64>,
    },
    /// The public thread API resolves the checkout from host-owned metadata.
    ThreadSend {
        message: String,
        draft: DraftIntent,
        account: String,
        profile: Option<String>,
        history_epoch: u64,
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
            Self::Control(control) => control
                .draft()
                .is_some_and(DraftIntent::has_workspace_files),
            Self::Send { draft, .. }
            | Self::ThreadSend { draft, .. }
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
        if matches!(self, Self::Recovery { .. }) {
            return control::REGISTRY
                .iter()
                .find(|m| m.name == "chat.set_agent")
                .expect("registered account change");
        }
        if let Self::Task(command) = self {
            return command.metadata();
        }
        if let Self::Peer(command) = self {
            return command.metadata();
        }
        if let Self::Admin(command) = self {
            return command.metadata();
        }
        if let Self::Settings(command) = self {
            return command.metadata();
        }
        if let Self::Artifact(command) = self {
            return command.metadata();
        }
        if let Self::Terminal(command) = self {
            return command.metadata();
        }
        if let Self::File(command) = self {
            return command.metadata();
        }
        if let Self::Git(command) = self {
            return command.metadata();
        }
        if let Self::Catalog(command) = self {
            return command.metadata();
        }
        if let Self::Control(control) = self {
            return control::REGISTRY
                .iter()
                .find(|m| m.name == control.method())
                .expect("registered control");
        }
        &REGISTRY[match self {
            Self::Recovery { .. }
            | Self::Control(_)
            | Self::Catalog(_)
            | Self::File(_)
            | Self::Git(_)
            | Self::Terminal(_) => unreachable!(),
            Self::Artifact(_)
            | Self::Settings(_)
            | Self::Admin(_)
            | Self::Peer(_)
            | Self::Task(_) => {
                unreachable!()
            }
            Self::Rename { .. } => 0,
            Self::Send { .. } | Self::ThreadSend { .. } => 1,
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
            Self::Recovery {
                record,
                expected_reason,
            } if !host::valid_id(record.id())
                || expected_reason.is_empty()
                || expected_reason.len() > host::CONTROL_BYTES =>
            {
                return Err(Error::InvalidRuntimeState);
            }
            Self::Task(command) => command.normalize()?,
            Self::Peer(command) => command.normalize()?,
            Self::Admin(command) => command.normalize()?,
            Self::Settings(command) => command.normalize()?,
            Self::Artifact(command) => command.normalize()?,
            Self::Terminal(command) => command.normalize()?,
            Self::File(command) => command.normalize()?,
            Self::Git(command) => command.normalize()?,
            Self::Catalog(command) => command.normalize()?,
            Self::Control(control) => control.normalize()?,
            Self::Rename { title } => {
                *title = title.trim().into();
                if title.is_empty() || title.len() > 256 || title.chars().any(char::is_control) {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            Self::Send { message, draft, .. } | Self::ThreadSend { message, draft, .. } => {
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
        if matches!(self.command, Command::Git(_))
            && !matches!(&self.resource, ResourceRef::Project(project) if project.worktree_id().is_some())
        {
            return Err(Error::InvalidRuntimeState);
        }
        if let Command::File(command) = &self.command
            && command
                .content()
                .is_some_and(|content| &content.host != self.resource.host_id())
        {
            return Err(Error::NotAuthorized);
        }
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
    pub fn runtime_input(&self) -> &CommandInput {
        &self.input
    }
    pub fn fingerprint(&self) -> &[u8] {
        &self.input.fingerprint
    }
    pub fn record(&self) -> Option<&ChatRecord> {
        self.record.as_ref()
    }
    pub fn control(&self) -> Option<&Control> {
        match &self.envelope.command {
            Command::Control(control) => Some(control),
            _ => None,
        }
    }
    pub fn runtime_draft(&self, resources: &DraftResources) -> Result<Option<QueueDraft>> {
        self.control()
            .and_then(Control::draft)
            .map(|d| d.resolve(self, Some(resources)))
            .transpose()
    }
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
    pub async fn recover_local(
        &self,
        source: &ChatRecord,
        record: ChatRecord,
        expected_reason: String,
    ) -> Result<CommandReceipt> {
        let thread = source.thread().ok_or(Error::NotFound)?;
        let prepared = self
            .prepare(
                CommandEnvelope {
                    command_id: uuid::Uuid::new_v4(),
                    resource: ResourceRef::Thread {
                        thread: thread.thread().clone(),
                        executor_epoch: thread.executor_epoch(),
                    },
                    command: Command::Recovery {
                        record: Box::new(record.clone()),
                        expected_reason: expected_reason.clone(),
                    },
                    conditions: Conditions::default(),
                },
                Principal::local_owner(),
            )
            .await?;
        let receipt = self
            .client
            .recover_runtime_chat(prepared.input, record, expected_reason)
            .await?;
        if receipt.state() == host::CommandState::Rejected {
            self.client
                .acknowledge_runtime_command(receipt.key().clone())
                .await?;
            return Err(Error::ResourceConflict);
        }
        Ok(receipt)
    }
    /// Local resource effects use the same typed admission and durable fences as
    /// RPCs. The application supplies owner-resolved authority, never a wire DTO.
    pub async fn begin_local_resource(
        &self,
        resource: ResourceRef,
        root: String,
        command: Command,
        authority: Vec<String>,
    ) -> Result<host::CommandInput> {
        self.begin_local_bound(resource, root, command, authority, None, None)
            .await
    }
    pub async fn begin_local_catalog(
        &self,
        resource: ResourceRef,
        root: String,
        target: String,
        command: CatalogCommand,
    ) -> Result<host::CommandInput> {
        self.begin_local_bound(
            resource,
            root,
            Command::Catalog(command),
            Vec::new(),
            None,
            Some(target),
        )
        .await
    }
    pub async fn begin_local_task(
        &self,
        resource: ResourceRef,
        root: String,
        command: TaskCommand,
        account: crate::integrations::Account,
    ) -> Result<host::CommandInput> {
        self.begin_local_bound(
            resource,
            root,
            Command::Task(command),
            Vec::new(),
            Some(account),
            None,
        )
        .await
    }
    async fn begin_local_bound(
        &self,
        resource: ResourceRef,
        root: String,
        command: Command,
        authority: Vec<String>,
        account: Option<crate::integrations::Account>,
        target: Option<String>,
    ) -> Result<host::CommandInput> {
        let scope = command.required_scope();
        let git = matches!(command, Command::Git(_));
        let envelope = CommandEnvelope {
            command_id: uuid::Uuid::new_v4(),
            resource,
            command,
            conditions: Conditions::default(),
        };
        let prepared = if matches!(envelope.resource, ResourceRef::Project(_))
            || matches!(
                envelope.command,
                Command::Catalog(
                    CatalogCommand::OpenProject { .. } | CatalogCommand::Create { .. }
                )
            ) {
            let target = target.unwrap_or_else(|| root.clone());
            self.prepare_catalog(envelope, Principal::local_owner(), Some((target, root)))
                .await?
                .ok_or(Error::NotFound)?
        } else {
            self.prepare(envelope, Principal::local_owner()).await?
        };
        if prepared.prior().is_some() {
            return Err(Error::CommandConflict);
        }
        let mut input = prepared.input;
        if git {
            git::SavedAuthority::bind(&mut input, authority)?;
        }
        if let Some(account) = account {
            tasks::AccountBinding::bind(&mut input, account)?;
        }
        let receipt = self
            .client
            .accept_runtime_command(input.clone(), scope)
            .await?;
        if receipt.state() != host::CommandState::Accepted {
            return Err(Error::ResourceConflict);
        }
        self.client
            .transition_runtime_command(
                input.key().clone(),
                host::CommandState::Accepted,
                host::CommandState::Attempting,
                None,
            )
            .await?;
        Ok(input)
    }
    /// A returned domain error records failure; interruption before this call
    /// leaves the attempting fence for existing startup recovery, never replay.
    pub async fn finish_local_resource<T>(
        &self,
        input: host::CommandInput,
        outcome: std::result::Result<T, String>,
    ) -> std::result::Result<T, String> {
        self.finish_local_outcome(input, outcome, false).await
    }
    /// A provider may have accepted a request before the connection failed.
    /// Retain that fence for explicit reconciliation instead of acknowledging it.
    pub async fn finish_local_outcome<T>(
        &self,
        input: host::CommandInput,
        outcome: std::result::Result<T, String>,
        uncertain: bool,
    ) -> std::result::Result<T, String> {
        let (state, detail) = match &outcome {
            Ok(_) => (host::CommandState::Succeeded, None),
            Err(error) => (if uncertain { host::CommandState::DeliveryUnknown } else { host::CommandState::Failed }, Some(serde_json::to_vec(&serde_json::json!({
                "code": "operation_failed", "reason": error.chars().take(8192).collect::<String>()
            })).map_err(|e| e.to_string())?)),
        };
        self.complete_local_resource(input, state, detail).await?;
        outcome
    }
    /// Owners with a structured result classify refusals and partial effects
    /// without flattening the result their existing UI needs to display.
    pub async fn complete_local_resource(
        &self,
        input: host::CommandInput,
        state: host::CommandState,
        detail: Option<Vec<u8>>,
    ) -> std::result::Result<(), String> {
        if !matches!(
            state,
            host::CommandState::Succeeded
                | host::CommandState::Failed
                | host::CommandState::Rejected
                | host::CommandState::DeliveryUnknown
        ) {
            return Err("A local operation requires a final outcome.".into());
        }
        let receipt = self
            .client
            .transition_runtime_command(
                input.key().clone(),
                host::CommandState::Attempting,
                state,
                detail,
            )
            .await
            .map_err(|e| e.to_string())?;
        if state != host::CommandState::DeliveryUnknown {
            self.client
                .acknowledge_runtime_command(receipt.key().clone())
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    /// Resolve a local catalog alias to its durable thread identity and epoch.
    /// A fully bound retained key preserves its original executor epoch.
    pub async fn catalog_envelope(
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
        let executor_epoch = if key.resource() == thread_id.to_string() {
            key.executor_epoch()
        } else {
            self.client
                .load_runtime_thread(thread_id)
                .await?
                .and_then(|record| record.thread().map(|thread| thread.executor_epoch()))
                .ok_or(Error::NotFound)?
        };
        Ok(CommandEnvelope {
            command_id: uuid::Uuid::parse_str(key.id()).map_err(|_| Error::InvalidRuntimeState)?,
            resource: ResourceRef::Thread {
                thread: ThreadRef {
                    host_id: host,
                    thread_id,
                },
                executor_epoch,
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
    /// The owner supplies authoritative catalog resolution separately from the
    /// stable intent. A first call without resolution checks saved auth/dedupe
    /// before any project lookup or filesystem probe.
    pub async fn prepare_catalog(
        &self,
        envelope: CommandEnvelope,
        principal: Principal,
        resolution: Option<(String, String)>,
    ) -> Result<Option<PreparedCommand>> {
        self.client
            .prepare_bus_catalog(envelope, principal, resolution)
            .await
    }
    /// Project resource owners resolve metadata only after this first pass has
    /// checked saved authorization and dedupe. The writer owns the fingerprint.
    pub async fn prepare_project_resource(
        &self,
        envelope: CommandEnvelope,
        principal: Principal,
        resolution: Option<String>,
    ) -> Result<Option<PreparedCommand>> {
        let root = resolution.map(|root| (root.clone(), root));
        self.client
            .prepare_bus_catalog(envelope, principal, root)
            .await
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
            .catalog_envelope(&key, host, chat, command, Conditions::default())
            .await?;
        let receipt = self.execute(envelope, principal, None).await?;
        // Existing local convenience callers report a domain conflict as an
        // error. The bus still owns and commits the rejection first.
        if receipt.key().device() == host::LOCAL_PRINCIPAL
            && receipt.state() == host::CommandState::Rejected
        {
            self.client
                .acknowledge_runtime_command(receipt.key().clone())
                .await?;
            return Err(Error::ResourceConflict);
        }
        Ok(receipt)
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
    RevokeDevice(String),
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

#[cfg(test)]
mod local_resource_tests {
    use super::*;
    #[tokio::test]
    async fn uncertain_local_provider_write_retains_the_original_fence() {
        let directory = tempfile::tempdir().unwrap();
        let client = SettingsClient::create(directory.path().join("settings.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let resource = ResourceRef::Environment(EnvironmentRef {
            host_id: HostId::new(host.id().into()).unwrap(),
        });
        let account = crate::integrations::Account {
            id: uuid::Uuid::new_v4().to_string(),
            provider: crate::integrations::Provider::Github,
            login: "test".into(),
            credential: uuid::Uuid::new_v4().to_string(),
            scope: crate::integrations::AccountScope::Default,
        };
        let command = TaskCommand::Mutate {
            integration: account.id.clone(),
            project: "owner/repo".into(),
            task: Some("1".into()),
            action: tasks::TaskAction::Watching(true),
        };
        let bus = CommandBus::new(client.clone());
        let input = bus
            .begin_local_task(resource.clone(), "private".into(), command.clone(), account)
            .await
            .unwrap();
        let key = input.key().clone();
        assert!(
            bus.finish_local_outcome::<()>(
                input,
                Err("Connection closed after delivery.".into()),
                true
            )
            .await
            .is_err()
        );
        let retained = client
            .get_runtime_command(key.clone(), host::TASKS)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.state(), host::CommandState::DeliveryUnknown);
        let retry = bus
            .prepare(
                CommandEnvelope {
                    command_id: uuid::Uuid::parse_str(key.id()).unwrap(),
                    resource,
                    command: Command::Task(command),
                    conditions: Conditions::default(),
                },
                Principal::local_owner(),
            )
            .await
            .unwrap();
        assert_eq!(retry.prior(), Some(&retained));
        assert!(!retained.resolved_by_user());
        client.shutdown().await.unwrap();
    }
}
