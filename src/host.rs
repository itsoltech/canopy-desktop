//! Durable host-owned chat state and command receipts. These are storage contracts,
//! not an RPC dispatcher: typed feature handlers authorize and perform operations.
use crate::{
    chat::{session::Draft, transcript::Attachment},
    state::workspace::PaneMetadata,
    thread::{ThreadId, ThreadRef},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod presence;
pub use presence::{DirtyPresence, FileIdentity};
mod authorization;
pub use authorization::DeviceGrant;

pub const READ: u32 = 1;
pub const CHAT: u32 = 2;
pub const APPROVE: u32 = 4;
pub const FILES_GIT: u32 = 8;
pub const TERMINAL: u32 = 16;
pub const TASKS: u32 = 32;
pub const ADMIN: u32 = 64;
pub const ALL_SCOPES: u32 = READ | CHAT | APPROVE | FILES_GIT | TERMINAL | TASKS | ADMIN;

pub const REMOTE_INBOX_RECORDS: u64 = 100_000;
pub const REMOTE_INBOX_BYTES: u64 = 64 * 1024 * 1024;
pub const LOCAL_INBOX_BYTES: u64 = 256 * 1024 * 1024;
pub const CONTROL_BYTES: usize = 1024 * 1024;
pub const LOCAL_PRINCIPAL: &str = "local";

pub(crate) fn valid_authority_root(root: &str) -> bool {
    root == "private" || FileIdentity::new(root.into(), "validation".into()).is_some()
}

pub(crate) fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeHost {
    pub(crate) id: String,
    pub(crate) identity_ref: Option<String>,
}
impl RuntimeHost {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn identity_ref(&self) -> Option<&str> {
        self.identity_ref.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatRecord {
    pub(crate) id: String,
    pub(crate) workspace: Option<String>,
    #[serde(default)]
    pub(crate) authority_root: String,
    pub(crate) agent: String,
    pub(crate) metadata: PaneMetadata,
    #[serde(default)]
    pub(crate) recovery_reason: Option<String>,
    #[serde(default)]
    pub(crate) history_epoch: u64,
    /// Assigned by the host when the record is first stored; `None` before that.
    /// `chat.id` stays the legacy local key and resolves through `thread_alias`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) thread: Option<PersistedThread>,
}

/// The stored thread identity of a chat record and its metadata revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedThread {
    pub(crate) thread: ThreadRef,
    /// Bumped on every persisted metadata change of the record.
    pub(crate) revision: u64,
    /// Fences commands across executor handoff; unchanged in this stage (D5).
    pub(crate) executor_epoch: u64,
}
impl PersistedThread {
    pub fn thread(&self) -> &ThreadRef {
        &self.thread
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn executor_epoch(&self) -> u64 {
        self.executor_epoch
    }
}

impl ChatRecord {
    /// Explicit recovery retains this chat ID and binds the observed history epoch.
    pub fn recovery_command(&self, key: CommandKey, expected_reason: &str) -> Option<CommandInput> {
        if expected_reason.is_empty() || expected_reason.len() > CONTROL_BYTES {
            return None;
        }
        let arguments = serde_json::to_vec(&(self, expected_reason)).ok()?;
        let fingerprint = Sha256::digest(&arguments).to_vec();
        CommandInput::new(
            key,
            "chat.recover".into(),
            self.id.clone(),
            arguments,
            fingerprint,
        )
    }
    pub fn new(
        id: String,
        workspace: Option<String>,
        agent: String,
        metadata: PaneMetadata,
    ) -> Option<Self> {
        (valid_id(&id)
            && metadata.remote.is_none()
            && workspace
                .as_ref()
                .is_none_or(|root| !root.is_empty() && root != "*" && root.len() <= 4096)
            && !agent.is_empty()
            && agent.len() <= 256
            && metadata.chat.as_ref().and_then(|chat| chat.id.as_deref()) == Some(&id)
            && metadata.chat.as_ref().is_some_and(|chat| chat.valid()))
        .then_some(Self {
            id,
            authority_root: if workspace.is_none() {
                "private".into()
            } else {
                String::new()
            },
            workspace,
            agent,
            metadata,
            recovery_reason: None,
            history_epoch: 0,
            thread: None,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn thread(&self) -> Option<&PersistedThread> {
        self.thread.as_ref()
    }
    pub fn thread_id(&self) -> Option<ThreadId> {
        self.thread.as_ref().map(|t| t.thread.thread_id)
    }
    /// Host-resolved resource authority, independent of catalog workspace ID.
    /// Empty only for retained records that require explicit recovery.
    pub fn authority_root(&self) -> &str {
        &self.authority_root
    }
    pub fn with_authority_root(mut self, root: String) -> Option<Self> {
        if !valid_authority_root(&root) || root == "private" && self.workspace.is_some() {
            return None;
        }
        self.authority_root = root;
        Some(self)
    }
    pub fn workspace(&self) -> Option<&str> {
        self.workspace.as_deref()
    }
    pub fn agent(&self) -> &str {
        &self.agent
    }
    pub fn metadata(&self) -> &PaneMetadata {
        &self.metadata
    }
    pub fn recovery_reason(&self) -> Option<&str> {
        self.recovery_reason.as_deref()
    }
    pub fn with_history_epoch(mut self, epoch: u64) -> Option<Self> {
        i64::try_from(epoch).ok()?;
        self.history_epoch = epoch;
        Some(self)
    }
    pub fn history_epoch(&self) -> u64 {
        self.history_epoch
    }
    pub fn send_context(&self) -> SendContext {
        SendContext {
            agent: self.agent.clone(),
            profile: self.metadata.profile_id.clone(),
            workspace: self.workspace.clone(),
            history_epoch: self.history_epoch,
        }
    }
}

/// A draft is tied to an agent/account and session boundary, not to token updates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendContext {
    pub(crate) agent: String,
    pub(crate) profile: Option<String>,
    pub(crate) workspace: Option<String>,
    pub(crate) history_epoch: u64,
}
impl SendContext {
    pub fn new(
        agent: String,
        profile: Option<String>,
        workspace: Option<String>,
        history_epoch: u64,
    ) -> Option<Self> {
        (!agent.is_empty()
            && agent.len() <= 256
            && i64::try_from(history_epoch).is_ok()
            && profile
                .as_ref()
                .is_none_or(|p| !p.is_empty() && p.len() <= 256)
            && workspace
                .as_ref()
                .is_none_or(|p| !p.is_empty() && p != "*" && p.len() <= 4096))
        .then_some(Self {
            agent,
            profile,
            workspace,
            history_epoch,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MessageIntent {
    pub(crate) message_id: String,
    pub(crate) draft: QueueDraft,
    pub(crate) context: SendContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) steer_run: Option<u64>,
}

/// A persisted draft excludes in-memory image data, just like transcript storage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueueDraft {
    text: String,
    attachments: Vec<Attachment>,
    context: Option<String>,
    goal: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artifact_ids: Vec<String>,
}
impl From<Draft> for QueueDraft {
    fn from(draft: Draft) -> Self {
        Self {
            text: draft.text,
            attachments: draft.attachments,
            context: draft.context,
            goal: draft.goal,
            artifact_ids: Vec::new(),
        }
    }
}
impl QueueDraft {
    pub fn into_draft(self) -> Draft {
        Draft {
            text: self.text,
            attachments: self.attachments,
            context: self.context,
            goal: self.goal,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }
    /// Ready host artifacts are retained in the same transaction as Send.
    pub fn with_artifacts(mut self, ids: Vec<String>) -> Option<Self> {
        if ids.len() > 10
            || ids.iter().any(|id| !valid_id(id))
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
        {
            return None;
        }
        self.artifact_ids = ids;
        Some(self)
    }
    pub fn artifact_ids(&self) -> &[String] {
        &self.artifact_ids
    }
    pub(crate) fn valid(&self) -> bool {
        (!self.text.trim().is_empty()
            || !self.attachments.is_empty()
            || !self.artifact_ids.is_empty())
            && self.text.len() <= CONTROL_BYTES
            && self.attachments.len() <= 100
            && self.artifact_ids.len() <= 10
            && self.artifact_ids.iter().all(|id| valid_id(id))
    }
    pub fn command(self, key: CommandKey, chat: &str) -> Option<(CommandInput, Self)> {
        if key.device != LOCAL_PRINCIPAL || !valid_id(chat) || !self.valid() {
            return None;
        }
        let arguments = serde_json::to_vec(&self).ok()?;
        // This is the typed, normalized domain encoding, not raw Protobuf bytes.
        let fingerprint = Sha256::digest(&arguments).to_vec();
        Some((
            CommandInput::new(key, "chat.send".into(), chat.into(), arguments, fingerprint)?,
            self,
        ))
    }
    /// The wire-facing path binds every argument, including the message ID and
    /// captured context, to idempotency. Legacy local calls have no retry path.
    pub fn command_for_message(
        self,
        key: CommandKey,
        chat: &str,
        message_id: String,
        context: SendContext,
    ) -> Option<(CommandInput, Self)> {
        self.command_for_intent(key, chat, message_id, context, None)
    }
    /// Durable new-text steer intent. The run is an authenticated dispatch hint,
    /// never permission to write stdin before the queue enters attempting.
    pub fn command_for_steer(
        self,
        key: CommandKey,
        chat: &str,
        message_id: String,
        context: SendContext,
        run: u64,
    ) -> Option<(CommandInput, Self)> {
        i64::try_from(run).ok()?;
        self.command_for_intent(key, chat, message_id, context, Some(run))
    }
    fn command_for_intent(
        self,
        key: CommandKey,
        chat: &str,
        message_id: String,
        context: SendContext,
        steer_run: Option<u64>,
    ) -> Option<(CommandInput, Self)> {
        if !valid_id(chat) || !valid_id(&message_id) || !self.valid() {
            return None;
        }
        let intent = MessageIntent {
            message_id,
            draft: self.clone(),
            context,
            steer_run,
        };
        let arguments = serde_json::to_vec(&intent).ok()?;
        let fingerprint = Sha256::digest(&arguments).to_vec();
        Some((
            CommandInput::new(key, "chat.send".into(), chat.into(), arguments, fingerprint)?,
            self,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueState {
    Queued,
    Attempting,
    Delivered,
    Failed,
    Dismissed,
    DeliveryUnknown,
}
impl QueueState {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Attempting => "attempting",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
            Self::Dismissed => "dismissed",
            Self::DeliveryUnknown => "delivery_unknown",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "queued" => Self::Queued,
            "attempting" => Self::Attempting,
            "delivered" => Self::Delivered,
            "failed" => Self::Failed,
            "dismissed" => Self::Dismissed,
            "delivery_unknown" => Self::DeliveryUnknown,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct QueueEntry {
    pub(crate) id: String,
    pub(crate) draft: QueueDraft,
    pub(crate) state: QueueState,
    pub(crate) manual: bool,
    pub(crate) steer_run: Option<u64>,
}
impl QueueEntry {
    /// Present only within the host lifetime that accepted this steer intent.
    pub fn steer_run(&self) -> Option<u64> {
        self.steer_run
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn draft(&self) -> &QueueDraft {
        &self.draft
    }
    pub fn state(&self) -> QueueState {
        self.state
    }
    pub fn resolved_by_user(&self) -> bool {
        self.manual
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChatQueue {
    pub(crate) revision: u64,
    pub(crate) paused: bool,
    pub(crate) entries: Vec<QueueEntry>,
}
impl ChatQueue {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn entries(&self) -> &[QueueEntry] {
        &self.entries
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandKey {
    pub(crate) device: String,
    pub(crate) epoch: u64,
    pub(crate) id: String,
}
impl CommandKey {
    pub fn local(id: String) -> Option<Self> {
        valid_id(&id).then_some(Self {
            device: LOCAL_PRINCIPAL.into(),
            epoch: 0,
            id,
        })
    }
    pub fn remote(device: String, epoch: u64, id: String) -> Option<Self> {
        (valid_id(&device) && valid_id(&id) && epoch > 0 && epoch < i64::MAX as u64)
            .then_some(Self { device, epoch, id })
    }
    pub fn device(&self) -> &str {
        &self.device
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Only normalized, nonsecret arguments enter this record. Secret-bearing handlers
/// must first persist immutable vault references and a keyed fingerprint.
#[derive(Clone)]
pub struct CommandInput {
    pub(crate) key: CommandKey,
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) arguments: Vec<u8>,
    pub(crate) fingerprint: Vec<u8>,
    pub(crate) root: Option<String>,
}
impl CommandInput {
    pub fn new(
        key: CommandKey,
        method: String,
        target: String,
        arguments: Vec<u8>,
        fingerprint: Vec<u8>,
    ) -> Option<Self> {
        (!method.is_empty()
            && method.len() <= 256
            && target.len() <= 4096
            && arguments.len() <= CONTROL_BYTES
            && (16..=128).contains(&fingerprint.len()))
        .then_some(Self {
            key,
            method,
            target,
            arguments,
            fingerprint,
            root: None,
        })
    }
    /// Binds retries to the decoded wire intent, independent of resolved resources.
    pub fn with_fingerprint(mut self, fingerprint: Vec<u8>) -> Option<Self> {
        if !(16..=128).contains(&fingerprint.len()) {
            return None;
        }
        self.fingerprint = fingerprint;
        Some(self)
    }
    /// Canonical resource root resolved by the feature handler, never a client path.
    pub fn with_root(mut self, root: String) -> Option<Self> {
        if root.is_empty() || root == "*" || root.len() > 4096 {
            return None;
        }
        self.root = Some(root);
        Some(self)
    }
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandState {
    Accepted,
    Attempting,
    Succeeded,
    Failed,
    Rejected,
    DeliveryUnknown,
}
impl CommandState {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Attempting => "attempting",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::DeliveryUnknown => "delivery_unknown",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "accepted" => Self::Accepted,
            "attempting" => Self::Attempting,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "rejected" => Self::Rejected,
            "delivery_unknown" => Self::DeliveryUnknown,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandReceipt {
    pub(crate) key: CommandKey,
    pub(crate) state: CommandState,
    pub(crate) result: Option<Vec<u8>>,
    pub(crate) resolved: bool,
    pub(crate) method: String,
    pub(crate) target: String,
}
impl CommandReceipt {
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
    pub fn state(&self) -> CommandState {
        self.state
    }
    pub fn result(&self) -> Option<&[u8]> {
        self.result.as_deref()
    }
    pub fn resolved_by_user(&self) -> bool {
        self.resolved
    }
    pub fn method(&self) -> &str {
        &self.method
    }
    pub fn target(&self) -> &str {
        &self.target
    }
}

#[derive(Clone)]
pub struct DeviceRegistration {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) credential: Vec<u8>,
    pub(crate) scopes: u32,
    pub(crate) roots: Vec<String>,
}
impl DeviceRegistration {
    pub fn new(
        id: String,
        name: String,
        public_credential: Vec<u8>,
        scopes: u32,
        roots: Vec<String>,
    ) -> Option<Self> {
        (valid_id(&id)
            && !name.is_empty()
            && name.len() <= 256
            && !public_credential.is_empty()
            && public_credential.len() <= 16 * 1024
            && scopes & !127 == 0
            && roots.len() <= 128
            && roots
                .iter()
                .all(|root| !root.is_empty() && root.len() <= 4096))
        .then_some(Self {
            id,
            name,
            credential: public_credential,
            scopes,
            roots,
        })
    }
}

/// Revision-checked operations on the host's single durable chat queue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum QueueMutation {
    Edit {
        message: String,
        draft: QueueDraft,
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UnknownResolution {
    Delivered,
    Resend { message: String },
    Dismissed,
}
impl QueueMutation {
    pub fn command(&self, key: CommandKey, chat: &str, revision: u64) -> Option<CommandInput> {
        if !valid_id(chat) {
            return None;
        }
        let arguments = serde_json::to_vec(&(revision, self)).ok()?;
        let fingerprint = Sha256::digest(&arguments).to_vec();
        CommandInput::new(
            key,
            "chat.queue".into(),
            chat.into(),
            arguments,
            fingerprint,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_recovery_arguments_keep_their_bytes_and_reject_unknown_formats() {
        let legacy = r#"[{"id":"11111111-1111-4111-8111-111111111111","workspace":null,"authority_root":"private","agent":"claude","metadata":{"cwd":null,"profile_id":null,"title":null,"resource":null,"kind":"Chat","arguments":[],"resume_id":null},"recovery_reason":null,"history_epoch":0},"restore context"]"#;
        let (record, reason): (ChatRecord, String) = serde_json::from_str(legacy).unwrap();
        let key = CommandKey::local(uuid::Uuid::new_v4().to_string()).unwrap();
        let command = record.recovery_command(key, &reason).unwrap();
        assert_eq!(command.arguments, legacy.as_bytes());
        assert_eq!(
            command.fingerprint,
            Sha256::digest(legacy.as_bytes()).to_vec()
        );
        let mut newer = serde_json::to_value(&record).unwrap();
        newer["newer_format"] = true.into();
        assert!(serde_json::from_value::<ChatRecord>(newer).is_err());
    }
}
