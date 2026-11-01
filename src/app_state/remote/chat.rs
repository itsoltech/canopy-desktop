//! Thread reads and runtime dispatch use the existing application owners.
mod api;
mod durability;
mod history;
mod projection;
#[cfg(test)]
mod tests;

use crate::app_state::{
    chat_bridge::ChatBridge,
    chats::{ChatCommand, DecisionAnswer, ForkReplacement, RemoteKey},
};
use canopy_desktop::{
    api::proto,
    artifacts::ArtifactStore,
    chat::{session::Draft, transcript::Attachment},
    host::{self, ChatRecord, DeviceGrant, QueueDraft},
    remote::{
        auth::{RpcAuth, storage_status},
        composer::PullContexts,
        host::{HostState, RpcStream},
        sync::ReplayHub,
    },
};
pub(super) use projection::state as chat_state;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tonic::{Request, Status};

#[derive(Clone)]
pub struct ChatsRpc {
    auth: RpcAuth,
    bridge: ChatBridge,
    artifacts: Arc<ArtifactStore>,
    state: HostState,
    projections: Arc<Mutex<projection::Registry>>,
    reads: Arc<projection::Reads>,
    grace: std::time::Duration,
    attachments_root: PathBuf,
    replay: ReplayHub,
    pulls: PullContexts,
}
impl ChatsRpc {
    pub fn new(
        auth: RpcAuth,
        bridge: ChatBridge,
        artifacts: Arc<ArtifactStore>,
        state: HostState,
        attachments_root: PathBuf,
        replay: ReplayHub,
    ) -> Self {
        Self {
            auth,
            bridge,
            artifacts,
            state,
            projections: Arc::default(),
            reads: Arc::default(),
            grace: projection::GRACE,
            attachments_root,
            replay,
            pulls: PullContexts::default(),
        }
    }
    pub fn with_pull_contexts(mut self, pulls: PullContexts) -> Self {
        self.pulls = pulls;
        self
    }
}

pub(super) fn required<T>(value: Option<T>) -> Result<T, Status> {
    value.ok_or_else(|| Status::invalid_argument("A required field is missing."))
}
pub(super) fn bridge_status(error: String) -> Status {
    if error.starts_with("resource_exhausted") {
        Status::resource_exhausted(error)
    } else if error.contains("not found") || error.contains("not in the host catalog") {
        Status::not_found(error)
    } else {
        Status::failed_precondition(error)
    }
}
pub(super) fn reference(auth: &RpcAuth, record: &ChatRecord) -> Option<proto::ThreadRef> {
    Some(proto::ThreadRef {
        host_id: auth.host_id().into(),
        thread_id: record.thread_id()?.to_string(),
    })
}
pub(super) fn option_effect(effect: canopy_desktop::chat::agent::OptionEffect) -> &'static str {
    use canopy_desktop::chat::agent::OptionEffect;
    match effect {
        OptionEffect::Immediate => "immediate",
        OptionEffect::NextTurn => "next_turn",
        OptionEffect::Restart => "restart",
    }
}
pub(super) fn capabilities(
    c: &canopy_desktop::chat::agent::Capabilities,
) -> Vec<proto::Capability> {
    [
        ("resume", c.resume),
        ("images", c.images),
        ("embedded_context", c.embedded_context),
        ("cancel", c.cancel),
        ("steer", c.steer),
        ("fork", c.fork),
        ("rewind", c.rewind),
        ("goal", c.goal),
    ]
    .into_iter()
    .map(|(operation, supported)| proto::Capability {
        operation: operation.into(),
        supported,
        reason: if supported {
            String::new()
        } else {
            "The agent does not support this operation.".into()
        },
    })
    .collect()
}
pub(in crate::app_state) fn edit_reason(error: &str) -> String {
    match error {
        "stale_run" => "The chat restarted since; open the message again.".into(),
        error => error
            .split_once(": ")
            .filter(|(code, _)| code.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .map_or(error, |(_, text)| text)
            .to_owned(),
    }
}
pub(super) fn resource_id(key: &host::CommandKey, kind: &str) -> String {
    use sha2::{Digest, Sha256};
    let identity = if key.resource().is_empty() {
        serde_json::to_vec(&(kind, key.device(), key.epoch(), key.id()))
    } else {
        serde_json::to_vec(&(
            kind,
            key.resource(),
            key.executor_epoch(),
            key.device(),
            key.epoch(),
            key.id(),
        ))
    };
    let digest = Sha256::digest(identity.expect("command identity encodes"));
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 15) | 0x80;
    bytes[8] = (bytes[8] & 63) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}
