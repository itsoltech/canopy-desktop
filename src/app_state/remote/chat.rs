//! Native chat RPCs call the same app-owned handlers as the local desktop.
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
    artifacts::ArtifactStore,
    chat::{session::Draft, transcript::Attachment},
    host::{
        self, ChatRecord, DeviceGrant, QueueDraft, QueueMutation, SendContext, UnknownResolution,
    },
    remote::{
        auth::{RpcAuth, WorkspaceRoots, storage_status},
        composer::PullContexts,
        host::{HostState, RpcStream},
        proto,
        resources::ResourceLocks,
        sync::ReplayHub,
    },
};
pub(super) use projection::state as chat_state;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct ChatsRpc {
    auth: RpcAuth,
    bridge: ChatBridge,
    roots: WorkspaceRoots,
    artifacts: Arc<ArtifactStore>,
    operations: ResourceLocks,
    state: HostState,
    projections: Arc<Mutex<projection::Registry>>,
    reads: Arc<projection::Reads>,
    /// How long a projection without clients waits for a reconnect.
    grace: std::time::Duration,
    attachments_root: PathBuf,
    replay: ReplayHub,
    /// Signs pull requests the host listed; a draft names one only by that context.
    pulls: PullContexts,
}

/// Where a draft's attachments resolve: the chat (or fork destination) they go to.
struct DraftTarget<'a> {
    /// A project chat: its files name their workspace.
    project: bool,
    /// Canonical checkout or "private" (pull request contexts are signed for it).
    root: &'a str,
    /// The chat's folder (private files and skills), if it has one yet.
    folder: Option<&'a Path>,
}
impl<'a> DraftTarget<'a> {
    fn of(record: &'a ChatRecord) -> Self {
        Self {
            project: record.workspace().is_some(),
            root: record.authority_root(),
            folder: record.metadata().cwd.as_deref(),
        }
    }
}

/// The fingerprint a device command is deduplicated by: its decoded request with the
/// command's transport identity (host, ID, epoch) left out, plus the revision it
/// expects, which is part of what it asks. `body` is the request with its command
/// metadata already taken out. A retry under the same ID with anything else changed
/// is an idempotency conflict, however its targets resolve by then.
pub(in crate::app_state) fn wire_fingerprint(
    meta: &proto::CommandMeta,
    body: &impl prost::Message,
) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"canopy/wire-command/v1\0");
    match meta.expected_revision {
        Some(revision) => {
            hash.update([1]);
            hash.update(revision.to_le_bytes());
        }
        None => hash.update([0]),
    }
    hash.update(body.encode_to_vec());
    hash.finalize().to_vec()
}

/// `idempotency_conflict` for a command ID reused for anything else.
pub(super) fn idempotency_conflict() -> Status {
    Status::aborted("idempotency_conflict: this command ID was used for another operation.")
}

impl ChatsRpc {
    #[allow(
        clippy::too_many_arguments,
        reason = "the app wires each concrete owner"
    )]
    pub fn new(
        auth: RpcAuth,
        bridge: ChatBridge,
        roots: WorkspaceRoots,
        artifacts: Arc<ArtifactStore>,
        operations: ResourceLocks,
        state: HostState,
        attachments_root: PathBuf,
        replay: ReplayHub,
    ) -> Self {
        Self {
            auth,
            bridge,
            roots,
            artifacts,
            operations,
            state,
            projections: Arc::default(),
            reads: Arc::default(),
            grace: projection::GRACE,
            attachments_root,
            replay,
            pulls: PullContexts::default(),
        }
    }
    /// Uses the host's pull request signer (the one discovery lists them with).
    pub fn with_pull_contexts(mut self, pulls: PullContexts) -> Self {
        self.pulls = pulls;
        self
    }
    async fn record(
        &self,
        grant: &DeviceGrant,
        chat: &proto::ChatRef,
        scope: u32,
    ) -> Result<ChatRecord, Status> {
        self.auth.check_host(&chat.host_id)?;
        self.auth.scope(grant, scope)?;
        let record = self
            .auth
            .client()
            .load_runtime_chat(chat.chat_id.clone())
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::not_found("Chat is no longer available."))?;
        self.auth
            .resource(grant.id(), scope, record.authority_root())
            .await?;
        Ok(record)
    }
    /// The recorded outcome of a retried command `method` on `target`, answered before
    /// the chat is looked up: a retry after the chat was deleted gets its state, not
    /// not_found (CMD-02). The same ID with another method, target or request
    /// (`fingerprint`) is `idempotency_conflict`, never the old receipt.
    async fn matching(
        &self,
        key: &host::CommandKey,
        scope: u32,
        method: &str,
        target: &str,
        fingerprint: &[u8],
    ) -> Result<Option<proto::CommandReceipt>, Status> {
        match self
            .auth
            .client()
            .get_runtime_command_matching(
                key.clone(),
                scope,
                method.into(),
                target.into(),
                fingerprint.to_vec(),
            )
            .await
        {
            Ok(receipt) => Ok(receipt.map(|r| self.receipt(&r))),
            Err(canopy_desktop::settings::Error::CommandConflict) => Err(idempotency_conflict()),
            Err(error) => Err(storage_status(error)),
        }
    }
    /// `matching` for a chat command of `grant` named by `meta`.
    async fn answered(
        &self,
        grant: &DeviceGrant,
        meta: &proto::CommandMeta,
        chat: &proto::ChatRef,
        scope: u32,
        method: &str,
        fingerprint: &[u8],
    ) -> Result<Option<proto::CommandReceipt>, Status> {
        self.auth.check_host(&chat.host_id)?;
        self.auth.scope(grant, scope)?;
        let key = self.auth.key(meta, grant)?;
        self.matching(&key, scope, method, &chat.chat_id, fingerprint)
            .await
    }
    /// A named chat command: deduplicated by its wire `fingerprint`, then run by the
    /// chat's own handler, which records it under the same fingerprint.
    pub(super) async fn command(
        &self,
        grant: DeviceGrant,
        meta: proto::CommandMeta,
        chat: Option<proto::ChatRef>,
        command: ChatCommand,
        fingerprint: Vec<u8>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        self.state.accepting()?;
        let chat = required(chat)?;
        let scope = command.scope();
        if let Some(receipt) = self
            .answered(&grant, &meta, &chat, scope, command.method(), &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        self.record(&grant, &chat, scope).await?;
        let key = RemoteKey {
            key: self.auth.key(&meta, &grant)?,
            fingerprint: Some(fingerprint),
        };
        let owned = self
            .operations
            .begin()
            .map_err(|e| Status::resource_exhausted(e.message().to_owned()))?;
        let this = self.clone();
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            this.bridge
                .command(chat.chat_id, key, command)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Chat command outcome is uncertain."))??;
        Ok(Response::new(self.receipt(&receipt)))
    }
    pub(super) fn receipt(&self, receipt: &host::CommandReceipt) -> proto::CommandReceipt {
        self.auth.receipt(receipt)
    }
    async fn draft(
        &self,
        grant: &DeviceGrant,
        record: &ChatRecord,
        draft: proto::MessageDraft,
    ) -> Result<QueueDraft, Status> {
        self.draft_for(grant, DraftTarget::of(record), draft).await
    }
    async fn draft_for(
        &self,
        grant: &DeviceGrant,
        target: DraftTarget<'_>,
        draft: proto::MessageDraft,
    ) -> Result<QueueDraft, Status> {
        let mut attachments = Vec::new();
        let mut images = Vec::new();
        for value in draft.attachments {
            let label = (!value.label.is_empty()).then_some(value.label);
            let attachment = match required(value.content)? {
                proto::attachment::Content::Image(image) => {
                    self.auth.check_host(&image.host_id)?;
                    if image.sha256.len() != 32 {
                        return Err(Status::invalid_argument("Invalid image digest."));
                    }
                    images.push(image.artifact_id);
                    continue;
                }
                proto::attachment::Content::Path(file) => {
                    let Some(workspace) = file.workspace else {
                        // A file of the chat's own private folder (Private chats).
                        if target.project {
                            return Err(Status::invalid_argument(
                                "A project chat's file names its workspace.",
                            ));
                        }
                        let root = target.folder.ok_or_else(|| {
                            Status::failed_precondition("The chat folder is unavailable.")
                        })?;
                        attachments.push(Attachment::Path {
                            path: root.join(normal_relative(&file.relative_path)?),
                            label,
                        });
                        continue;
                    };
                    let root = self.roots.resolve(&workspace, &self.auth)?;
                    let scope_root = WorkspaceRoots::root_string(&root)?;
                    self.auth
                        .resource(grant.id(), host::READ, scope_root)
                        .await?;
                    if scope_root != target.root {
                        return Err(Status::permission_denied(
                            "The attachment belongs to another checkout.",
                        ));
                    }
                    let relative = PathBuf::from(file.relative_path);
                    if relative.as_os_str().is_empty()
                        || !relative
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_)))
                    {
                        return Err(Status::invalid_argument("Invalid attachment path."));
                    }
                    Attachment::Path {
                        path: root.join(relative),
                        label,
                    }
                }
                proto::attachment::Content::Quote(q) => Attachment::Quote {
                    text: q.text,
                    label,
                },
                proto::attachment::Content::Command(c) => {
                    if !canopy_desktop::chat::config::valid_command(&c.name) {
                        return Err(Status::invalid_argument("Invalid agent command."));
                    }
                    let path = if let Some(path) = c.path {
                        let root = target.folder.ok_or_else(|| {
                            Status::failed_precondition("The chat folder is unavailable.")
                        })?;
                        let path = PathBuf::from(path);
                        let relative = if path.is_absolute() {
                            path.strip_prefix(root)
                                .map_err(|_| {
                                    Status::permission_denied("The skill is outside this checkout.")
                                })?
                                .to_owned()
                        } else {
                            path
                        };
                        if !relative
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_)))
                        {
                            return Err(Status::permission_denied("Invalid skill path."));
                        }
                        Some(root.join(relative))
                    } else {
                        None
                    };
                    Attachment::Command {
                        name: c.name,
                        skill: c.skill,
                        path,
                        description: c.description,
                        label,
                    }
                }
                proto::attachment::Content::Pull(p) => {
                    // Only a pull request the host listed for this checkout, unchanged.
                    if p.context_id.is_none() {
                        return Err(Status::failed_precondition(
                            "pr_context_required: Select the pull request from the host's list.",
                        ));
                    }
                    let p = self.pulls.resolve(target.root, p)?;
                    Attachment::Pull {
                        number: p.number,
                        title: p.title,
                        url: p.url,
                        head: p.head,
                        base: p.base,
                        state: p.state,
                        body: p.body,
                        label,
                    }
                }
                proto::attachment::Content::Shell(s) => Attachment::Shell {
                    command: s.command,
                    exit: s.exit_code,
                    stopped: s.stopped,
                    output: s.output,
                    label,
                },
            };
            attachments.push(attachment);
        }
        QueueDraft::from(Draft {
            text: draft.text,
            attachments,
            context: draft.context,
            goal: draft.goal,
        })
        .with_artifacts(images)
        .ok_or_else(|| Status::invalid_argument("Invalid message attachments."))
    }
}
pub(super) fn required<T>(value: Option<T>) -> Result<T, Status> {
    value.ok_or_else(|| Status::invalid_argument("A required field is missing."))
}
/// A non-empty relative path of plain components (no `..`, root or prefix).
fn normal_relative(value: &str) -> Result<PathBuf, Status> {
    let relative = PathBuf::from(value);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(Status::invalid_argument("Invalid attachment path."));
    }
    Ok(relative)
}
fn run(value: &str) -> Result<u64, Status> {
    value
        .parse()
        .map_err(|_| Status::invalid_argument("A valid run ID is required."))
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
pub(super) fn reference(auth: &RpcAuth, id: &str) -> proto::ChatRef {
    proto::ChatRef {
        host_id: auth.host_id().into(),
        chat_id: id.into(),
    }
}
pub(super) fn workspace(auth: &RpcAuth, record: &ChatRecord) -> Option<proto::WorkspaceRef> {
    record.workspace().map(|id| proto::WorkspaceRef {
        host_id: auth.host_id().into(),
        workspace_id: id.into(),
    })
}
/// The wire name of when an option's choice takes effect (AGT-05).
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

#[tonic::async_trait]
impl proto::chat_service_server::ChatService for ChatsRpc {
    type SubscribeChatStream = RpcStream<proto::ChatEvent>;
    async fn subscribe_chat(
        &self,
        r: Request<proto::SubscribeChatRequest>,
    ) -> Result<Response<Self::SubscribeChatStream>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let chat = required(r.chat)?;
        let record = self.record(&grant, &chat, host::READ).await?;
        self.subscribe(record, grant, r.since)
            .await
            .map(Response::new)
    }
    type ReadHistoryPageStream = RpcStream<proto::HistoryPageEvent>;
    async fn read_history_page(
        &self,
        r: Request<proto::HistoryPageRequest>,
    ) -> Result<Response<Self::ReadHistoryPageStream>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let chat = required(r.chat.clone())?;
        let record = self.record(&grant, &chat, host::READ).await?;
        self.history(record, grant, r).await.map(Response::new)
    }
    async fn search_history(
        &self,
        r: Request<proto::SearchHistoryRequest>,
    ) -> Result<Response<proto::HistoryMatches>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let chat = required(r.chat.clone())?;
        let record = self.record(&grant, &chat, host::READ).await?;
        self.search(record, grant, r).await.map(Response::new)
    }
    async fn get_agent_options(
        &self,
        r: Request<proto::ChatRequest>,
    ) -> Result<Response<proto::AgentOptions>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let chat = required(r.into_inner().chat)?;
        let record = self.record(&grant, &chat, host::READ).await?;
        let snapshot = self
            .bridge
            .snapshot(chat.chat_id.clone())
            .await
            .map_err(bridge_status)?;
        self.auth
            .resource(grant.id(), host::READ, record.authority_root())
            .await?;
        Ok(Response::new(proto::AgentOptions {
            chat: Some(chat),
            capabilities: capabilities(&snapshot.capabilities),
            options: snapshot
                .options
                .into_iter()
                .map(|o| proto::AgentOption {
                    id: o.id,
                    label: o.label,
                    category: o.category,
                    current_value: o.value,
                    choices: o
                        .choices
                        .into_iter()
                        .map(|c| proto::OptionChoice {
                            id: c.id,
                            label: c.label,
                            description: c.description,
                        })
                        .collect(),
                    effect: option_effect(o.effect).into(),
                })
                .collect(),
        }))
    }
    async fn send_message(
        &self,
        r: Request<proto::SendMessageRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let chat = required(r.chat)?;
        if let Some(receipt) = self
            .answered(&grant, &meta, &chat, host::CHAT, "chat.send", &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let record = self.record(&grant, &chat, host::CHAT).await?;
        let expected = required(r.expected_context)?;
        if let Some(w) = &expected.workspace {
            self.auth.check_host(&w.host_id)?;
        }
        let context = SendContext::new(
            expected.agent_id,
            expected.profile_id,
            expected.workspace.map(|w| w.workspace_id),
            expected.history_epoch,
        )
        .ok_or_else(|| Status::invalid_argument("Invalid message context."))?;
        let key = self.auth.key(&meta, &grant)?;
        let draft = self.draft(&grant, &record, required(r.draft)?).await?;
        let (input, draft) = draft
            .command_for_message(key, &chat.chat_id, r.message_id.clone(), context)
            .and_then(|(input, draft)| Some((input.with_fingerprint(fingerprint)?, draft)))
            .ok_or_else(|| Status::invalid_argument("Invalid message identity or content."))?;
        let this = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            this.bridge
                .send(chat.chat_id, input, r.message_id, draft)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Message acceptance is uncertain; reconcile its ID."))??;
        Ok(Response::new(self.receipt(&receipt)))
    }
    async fn change_queue(
        &self,
        r: Request<proto::ChangeQueueRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let chat = required(r.chat.clone())?;
        if let Some(receipt) = self
            .answered(&grant, &meta, &chat, host::CHAT, "chat.queue", &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let record = self.record(&grant, &chat, host::CHAT).await?;
        let mutation = match required(r.change)? {
            proto::change_queue_request::Change::Edit(e) => QueueMutation::Edit {
                message: e.message_id,
                draft: self.draft(&grant, &record, required(e.draft)?).await?,
            },
            proto::change_queue_request::Change::Move(e) => QueueMutation::Move {
                message: e.message_id,
                before: e.before_message_id,
            },
            proto::change_queue_request::Change::Remove(e) => QueueMutation::Remove {
                message: e.message_id,
            },
            proto::change_queue_request::Change::Pause(e) => {
                QueueMutation::Pause { paused: e.paused }
            }
            proto::change_queue_request::Change::Resolve(e) => QueueMutation::ResolveUnknown {
                message: e.message_id,
                resolution: match proto::UnknownResolution::try_from(e.resolution) {
                    Ok(proto::UnknownResolution::AcceptAsDelivered) => UnknownResolution::Delivered,
                    Ok(proto::UnknownResolution::SendAgain) => UnknownResolution::Resend {
                        message: required(e.new_message_id)?,
                    },
                    Ok(proto::UnknownResolution::Dismiss) => UnknownResolution::Dismissed,
                    _ => {
                        return Err(Status::invalid_argument(
                            "Choose how to resolve the uncertain message.",
                        ));
                    }
                },
            },
        };
        let key = self.auth.key(&meta, &grant)?;
        let input = mutation
            .command(key, &chat.chat_id, r.queue_revision)
            .and_then(|input| input.with_fingerprint(fingerprint))
            .ok_or_else(|| Status::invalid_argument("Invalid queue edit."))?;
        let this = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            this.bridge
                .change_queue(chat.chat_id, input, r.queue_revision, mutation)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Queue outcome is uncertain; reconcile its ID."))??;
        Ok(Response::new(self.receipt(&receipt)))
    }
    async fn steer(
        &self,
        r: Request<proto::SteerRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let chat = required(r.chat.clone())?;
        if let Some(receipt) = self
            .answered(&grant, &meta, &chat, host::CHAT, "chat.send", &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let record = self.record(&grant, &chat, host::CHAT).await?;
        let generation = run(&r.run_id)?;
        let key = self.auth.key(&meta, &grant)?;
        let draft = self.draft(&grant, &record, required(r.draft)?).await?;
        let (input, draft) = draft
            .command_for_steer(
                key,
                &chat.chat_id,
                r.message_id.clone(),
                record.send_context(),
                generation,
            )
            .and_then(|(input, draft)| Some((input.with_fingerprint(fingerprint)?, draft)))
            .ok_or_else(|| Status::invalid_argument("Invalid steer message."))?;
        let this = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            this.bridge
                .steer(chat.chat_id, input, r.message_id, draft, generation)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Steer outcome is uncertain; reconcile its ID."))??;
        Ok(Response::new(self.receipt(&receipt)))
    }
    async fn interrupt(
        &self,
        r: Request<proto::ChatCommandRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let generation = run(&required(r.run_id)?)?;
        let command = ChatCommand::Interrupt { run: generation };
        self.command(grant, meta, r.chat, command, fingerprint)
            .await
    }
    async fn restart(
        &self,
        r: Request<proto::ChatCommandRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let generation = run(&required(r.run_id)?)?;
        let command = ChatCommand::Restart { run: generation };
        self.command(grant, meta, r.chat, command, fingerprint)
            .await
    }
    async fn respond_decision(
        &self,
        r: Request<proto::RespondDecisionRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let generation = run(&r.run_id)?;
        let item = r
            .decision_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid decision ID."))?;
        let answer = match required(r.response)? {
            proto::respond_decision_request::Response::OptionId(id) => {
                DecisionAnswer::Option(Some(id))
            }
            proto::respond_decision_request::Response::Native(n) => {
                match proto::NativeDecisionAction::try_from(n.action) {
                    Ok(proto::NativeDecisionAction::AllowOnce) => DecisionAnswer::Allow,
                    Ok(proto::NativeDecisionAction::AllowForChat) => DecisionAnswer::AllowForChat,
                    Ok(proto::NativeDecisionAction::Deny) => DecisionAnswer::Deny(n.note),
                    Ok(proto::NativeDecisionAction::Answer) => DecisionAnswer::Answer(n.answers),
                    Ok(proto::NativeDecisionAction::ApprovePlan) => DecisionAnswer::ApprovePlan {
                        accept_edits: n.accept_edits,
                    },
                    Ok(proto::NativeDecisionAction::KeepPlanning) => {
                        DecisionAnswer::KeepPlanning(n.note)
                    }
                    _ => return Err(Status::invalid_argument("Invalid native decision.")),
                }
            }
        };
        let command = ChatCommand::Respond {
            run: generation,
            item,
            answer,
        };
        self.command(grant, meta, r.chat, command, fingerprint)
            .await
    }
    async fn set_agent_option(
        &self,
        r: Request<proto::SetAgentOptionRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let command = ChatCommand::SetOption {
            id: r.option_id,
            value: r.value_id,
        };
        self.command(grant, meta, r.chat, command, fingerprint)
            .await
    }
    async fn fork_chat(
        &self,
        r: Request<proto::ForkChatRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.auth.scope(&grant, host::CHAT)?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let chat = required(r.chat.clone())?;
        self.auth.check_host(&chat.host_id)?;
        let key = self.auth.key(&meta, &grant)?;
        let new_id = resource_id(&key, "fork");
        // A retry answers from the inbox first: the source may be gone by now.
        if let Some(receipt) = self
            .matching(&key, host::CHAT, "chat.fork", &new_id, &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let source = self.record(&grant, &chat, host::CHAT).await?;
        let (workspace, root, folder) = match &r.destination {
            Some(w) => {
                let path = self.roots.resolve(w, &self.auth)?;
                let root = WorkspaceRoots::root_string(&path)?.to_owned();
                self.auth.resource(grant.id(), host::CHAT, &root).await?;
                let id = serde_json::from_str(&w.workspace_id)
                    .map_err(|_| Status::invalid_argument("Invalid destination workspace."))?;
                (Some(id), root, Some(path))
            }
            None => {
                self.auth
                    .resource(grant.id(), host::CHAT, "private")
                    .await?;
                // A private fork's edited message names files of its private source's
                // folder (the same files a send to the source may name); the fork
                // gets copies of exactly those in its own new folder. A project
                // source has no private folder: its fork's message names none.
                let folder = match (source.workspace(), &source.metadata().cwd) {
                    (None, Some(cwd)) if r.replacement.is_some() => {
                        let cwd = cwd.clone();
                        let canonical =
                            tokio::task::spawn_blocking(move || std::fs::canonicalize(cwd))
                                .await
                                .ok()
                                .and_then(Result::ok);
                        Some(canonical.ok_or_else(|| {
                            Status::failed_precondition("The chat folder is unavailable.")
                        })?)
                    }
                    _ => None,
                };
                (None, "private".to_owned(), folder)
            }
        };
        // Edit as a fork (CHAT-08): the edited message is checked against the fork's
        // destination before anything is forked, then queued in the same transaction
        // that publishes the fork, under a command derived from this one.
        let replacement = match r.replacement.clone() {
            None => None,
            Some(replacement) => {
                let target = DraftTarget {
                    project: workspace.is_some(),
                    root: &root,
                    folder: folder.as_deref(),
                };
                let draft = self.draft_for(&grant, target, replacement).await?;
                let message = resource_id(&key, "fork-message");
                let send =
                    host::CommandKey::remote(key.device().into(), key.epoch(), message.clone())
                        .ok_or_else(|| Status::invalid_argument("Invalid command identity."))?;
                Some(ForkReplacement {
                    key: send,
                    message,
                    draft,
                    source_folder: workspace.is_none().then_some(folder).flatten(),
                })
            }
        };
        let request = crate::app_state::chats::ForkChat {
            source: chat.chat_id,
            history_epoch: r.history_epoch,
            item: r.item_id,
            chat: new_id,
            workspace,
            replacement_fingerprint: replacement
                .as_ref()
                .map(|_| ForkReplacement::fingerprint(&fingerprint)),
        };
        let key = RemoteKey {
            key,
            fingerprint: Some(fingerprint),
        };
        let this = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            this.bridge
                .fork(key, request, replacement)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Fork outcome is uncertain; reconcile its ID."))??;
        Ok(Response::new(self.receipt(&receipt)))
    }
    async fn set_goal(
        &self,
        r: Request<proto::SetGoalRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let action = match r.action.map(proto::GoalAction::try_from) {
            None | Some(Ok(proto::GoalAction::Unspecified)) => None,
            Some(Ok(proto::GoalAction::PauseGoal)) => Some(ChatCommand::PauseGoal),
            Some(Ok(proto::GoalAction::ResumeGoal)) => Some(ChatCommand::ResumeGoal),
            Some(Ok(proto::GoalAction::ClearGoal)) => Some(ChatCommand::ClearGoal),
            Some(Err(_)) => return Err(Status::invalid_argument("Unknown goal action.")),
        };
        if let Some(command) = action {
            if r.objective.is_some() {
                return Err(Status::invalid_argument(
                    "A goal action takes no objective; set a goal without an action.",
                ));
            }
            return self
                .command(grant, meta, r.chat, command, fingerprint)
                .await;
        }
        // Without an action: an objective sets the goal, none clears it.
        let Some(objective) = r.objective else {
            return self
                .command(grant, meta, r.chat, ChatCommand::ClearGoal, fingerprint)
                .await;
        };
        self.state.accepting()?;
        let chat = required(r.chat)?;
        if let Some(receipt) = self
            .answered(&grant, &meta, &chat, host::CHAT, "chat.send", &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let record = self.record(&grant, &chat, host::CHAT).await?;
        let key = self.auth.key(&meta, &grant)?;
        let message = resource_id(&key, "goal-message");
        let draft = QueueDraft::from(Draft {
            text: objective,
            attachments: vec![],
            context: None,
            goal: true,
        });
        let (input, draft) = draft
            .command_for_message(key, &chat.chat_id, message.clone(), record.send_context())
            .and_then(|(input, draft)| Some((input.with_fingerprint(fingerprint)?, draft)))
            .ok_or_else(|| Status::invalid_argument("Invalid goal."))?;
        let this = self.clone();
        let owned = self.operations.begin()?;
        let receipt = tokio::spawn(async move {
            let _owned = owned;
            this.bridge
                .send(chat.chat_id, input, message, draft)
                .await
                .map_err(bridge_status)
        })
        .await
        .map_err(|_| Status::unknown("Goal outcome is uncertain; reconcile its ID."))??;
        Ok(Response::new(self.receipt(&receipt)))
    }
    async fn stop_background(
        &self,
        r: Request<proto::StopBackgroundRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let generation = run(&r.run_id)?;
        let command = ChatCommand::StopBackground {
            run: generation,
            call: r.call_id,
        };
        self.command(grant, meta, r.chat, command, fingerprint)
            .await
    }
    async fn preview_edit(
        &self,
        r: Request<proto::PreviewEditRequest>,
    ) -> Result<Response<proto::EditPreview>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let chat = required(r.chat)?;
        let record = self.record(&grant, &chat, host::CHAT).await?;
        let generation = run(&r.run_id)?;
        let item = required(r.item)?;
        let preview = self
            .bridge
            .edit_preview(
                chat.chat_id.clone(),
                generation,
                item.history_epoch,
                item.item_id,
            )
            .await
            .map_err(bridge_status)?;
        // The device's access may have changed during the agent's dry run.
        self.auth
            .resource(grant.id(), host::CHAT, record.authority_root())
            .await?;
        let original = match preview.original {
            Some(draft) => Some(self.editable_draft(&record, draft).await?),
            None => None,
        };
        let files = preview.files;
        Ok(Response::new(proto::EditPreview {
            chat: Some(chat),
            run_id: r.run_id,
            item: Some(item),
            editable: preview.editable.is_ok(),
            reason: preview
                .editable
                .as_ref()
                .err()
                .map(|e| edit_reason(e))
                .unwrap_or_default(),
            first_message: preview.editable == Ok(true),
            original,
            files: Some(proto::FileRestorePreview {
                possible: files.possible,
                paths: files.paths,
                insertions: files.insertions,
                deletions: files.deletions,
                reason: files.reason.unwrap_or_default(),
            }),
        }))
    }
    async fn edit_message(
        &self,
        r: Request<proto::EditMessageRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        self.state.accepting()?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let chat = required(r.chat.clone())?;
        if let Some(receipt) = self
            .answered(&grant, &meta, &chat, host::CHAT, "chat.edit", &fingerprint)
            .await?
        {
            return Ok(Response::new(receipt));
        }
        let record = self.record(&grant, &chat, host::CHAT).await?;
        let generation = run(&r.run_id)?;
        let item = required(r.item)?;
        let draft = self.draft(&grant, &record, required(r.draft)?).await?;
        let command = ChatCommand::Edit {
            run: generation,
            history_epoch: item.history_epoch,
            item: item.item_id,
            draft,
            restore_files: r.restore_files,
        };
        self.command(grant, meta, Some(chat), command, fingerprint)
            .await
    }
    async fn run_shell(
        &self,
        r: Request<proto::RunShellRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let generation = run(&r.run_id)?;
        let line = r.command_line.trim();
        if line.is_empty()
            || line.len() > canopy_desktop::host::CONTROL_BYTES
            || line.contains('\0')
        {
            return Err(Status::invalid_argument("Enter a command to run."));
        }
        let command = ChatCommand::RunShell {
            run: generation,
            command: line.to_owned(),
        };
        self.command(grant, meta, r.chat, command, fingerprint)
            .await
    }
    async fn stop_shell(
        &self,
        r: Request<proto::StopShellRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let mut r = r.into_inner();
        let meta = required(r.command.take())?;
        let fingerprint = wire_fingerprint(&meta, &r);
        let generation = run(&r.run_id)?;
        let item = required(r.item)?;
        let command = ChatCommand::StopShell {
            run: generation,
            history_epoch: item.history_epoch,
            item: item.item_id,
        };
        self.command(grant, meta, r.chat, command, fingerprint)
            .await
    }
}

/// An edit refusal as the device shows it: the text after its `code: ` prefix.
fn edit_reason(error: &str) -> String {
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
    let digest = Sha256::digest(
        serde_json::to_vec(&(kind, key.device(), key.epoch(), key.id()))
            .expect("command identity encodes"),
    );
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 15) | 0x80;
    bytes[8] = (bytes[8] & 63) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}
