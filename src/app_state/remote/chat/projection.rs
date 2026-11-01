//! One projection and replay stream per watched chat, shared by its clients.
//! Snapshot capture and tail registration share the projection lock, and frames are
//! published under it, so a client gets a complete snapshot at boundary S and then
//! exactly the frames after S (SYNC-01). The runtime's changes are taken once by the
//! bridge subscription and fanned out by the replay hub (SYNC-04).
//!
//! Lifetime: the projection counts its connected clients and reports that count to
//! the runtime as observers (LIFE-09). When the last client goes it reports zero
//! (the chat may sleep) and is kept for `GRACE`, so a reconnect replays the frames it
//! missed instead of a new snapshot; after that, or when the runtime stops, it closes.
//!
//! Memory: at most `PROJECTIONS` projections with the newest `TAIL` items each;
//! snapshot and history reads are admitted per device and host-wide; items above
//! `INLINE` are sent as ≤64 KiB fragments read lazily from one shared encoding.
use super::durability::{Durability, Key, stored_hash};
use super::*;
use crate::app_state::{
    chat_bridge::ChatEvent,
    chats::{ChatSnapshot, SnapshotEpoch},
};
use canopy_desktop::{
    chat::transcript as t,
    remote::{sync, transcript as wire},
};
use prost::Message;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::OnceLock,
    time::{Duration, Instant},
};

/// How long a projection without clients keeps its replay ring for a reconnect.
pub(super) const GRACE: Duration = Duration::from_secs(30);
/// Projections held at once; an idle one is closed first to make room.
const PROJECTIONS: usize = 64;
/// Items a projection holds (the rest is read as history pages).
const TAIL: usize = crate::app_state::chat_bridge::SNAPSHOT_ITEMS;
/// Items encoded above this are sent as fragments.
pub(super) const INLINE: usize = 512 * 1024;
/// Snapshot budget of ordinary items, and of fragmented large ones.
const SNAPSHOT_BYTES: usize = 1024 * 1024;
const SNAPSHOT_LARGE: usize = wire::MAX_ITEM_BYTES;
/// A live item larger than this is not pushed into the replay ring (it would evict
/// every other stream); its clients resync and read it from the snapshot instead.
const LIVE_FRAGMENTED: usize = 4 * 1024 * 1024;
/// Published image references kept beyond the ones the held items use.
const IMAGES: usize = 256;
/// Snapshot and history transfers in progress, host-wide and per device.
pub(super) const READS: usize = 32;
pub(super) const DEVICE_READS: usize = 4;
const FRAGMENT: usize = 64 * 1024;

pub(super) struct Projector {
    subscription: u64,
    data: Mutex<Data>,
    clients: Mutex<Clients>,
    stop: async_channel::Sender<()>,
    done: async_channel::Receiver<()>,
}
#[derive(Default)]
struct Clients {
    count: usize,
    /// The count the runtime was last told (its bridge subscription starts at one).
    reported: usize,
    reporting: bool,
    generation: u64,
    idle_since: Option<Instant>,
}

/// One held item: its wire form, the version it was projected from (for save
/// proofs) and, once needed, one shared encoding for fragments.
pub(super) struct Slot {
    item: proto::TranscriptItem,
    source: Arc<t::Item>,
    images: Vec<String>,
    encoded: OnceLock<Arc<Encoded>>,
}
pub(super) struct Encoded {
    bytes: Arc<[u8]>,
    sha256: Vec<u8>,
}
impl Slot {
    pub(super) fn new(item: proto::TranscriptItem, source: Arc<t::Item>) -> Arc<Self> {
        let images = image_files(&source);
        Arc::new(Self {
            item,
            source,
            images,
            encoded: OnceLock::new(),
        })
    }
    fn encoded(&self) -> Arc<Encoded> {
        self.encoded
            .get_or_init(|| {
                use sha2::{Digest, Sha256};
                let bytes: Arc<[u8]> = self.item.encode_to_vec().into();
                let sha256 = Sha256::digest(&bytes).to_vec();
                Arc::new(Encoded { bytes, sha256 })
            })
            .clone()
    }
}

struct Data {
    snapshot: ChatSnapshot,
    serials: HashMap<u64, u64>,
    items: BTreeMap<Key, Arc<Slot>>,
    revision: u64,
    images: HashMap<String, proto::ArtifactRef>,
    stream: String,
    earlier: bool,
    closed: bool,
    queue: proto::ThreadQueue,
    durability: Durability,
    /// The `saved_sequence` last published.
    announced: u64,
}

/// How a chat's paths are presented: its checkout (or private folder) root.
#[derive(Clone)]
pub(super) struct Context {
    project_file: bool,
    root: PathBuf,
}
impl Context {
    pub(super) fn of(_auth: &RpcAuth, record: &ChatRecord) -> Self {
        Self {
            project_file: record.workspace().is_some(),
            root: record
                .metadata()
                .cwd
                .clone()
                .unwrap_or_else(|| PathBuf::from("/")),
        }
    }
}

pub(super) fn image_files(item: &t::Item) -> Vec<String> {
    match &item.body {
        t::Body::User(u) => u
            .attachments
            .iter()
            .filter_map(|a| match a {
                Attachment::Image { file, .. } => Some(file.clone()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

/// A host path attachment as the protocol can present it: relative to the project
/// checkout, or (workspace absent) to the chat's private folder; a path outside both
/// keeps only its label (empty relative path: not openable, never a client path).
fn host_path(path: &Path, label: Option<&str>, context: &Context) -> proto::Attachment {
    let relative = if path.is_absolute() {
        path.strip_prefix(&context.root).ok()
    } else {
        Some(path)
    }
    .filter(|r| {
        !r.as_os_str().is_empty()
            && r.components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
    })
    .and_then(|r| r.to_str());
    let label = label
        .map(str::to_owned)
        .or_else(|| path.file_name().and_then(|n| n.to_str()).map(str::to_owned))
        .unwrap_or_default();
    proto::Attachment {
        label,
        content: Some(proto::attachment::Content::File(proto::FileAttachment {
            project_file: relative.is_some() && context.project_file,
            relative_path: relative.unwrap_or_default().into(),
        })),
    }
}

/// Swaps the path attachments for placeholders the shared projection accepts and
/// returns their presentation, by position.
fn presentable(
    attachments: &mut [Attachment],
    context: &Context,
) -> Vec<(usize, proto::Attachment)> {
    attachments
        .iter_mut()
        .enumerate()
        .filter_map(|(index, attachment)| {
            let Attachment::Path { path, label } = attachment else {
                return None;
            };
            let presented = host_path(path, label.as_deref(), context);
            *attachment = Attachment::Quote {
                text: String::new(),
                label: label.clone(),
            };
            Some((index, presented))
        })
        .collect()
}
fn restore(draft: &mut proto::MessageDraft, presented: Vec<(usize, proto::Attachment)>) {
    for (index, attachment) in presented {
        if let Some(slot) = draft.attachments.get_mut(index) {
            *slot = attachment;
        }
    }
}

/// `item` on the wire, with its host path attachments presented per `context`.
pub(super) fn project(
    item: &t::Item,
    epoch: u64,
    revision: u64,
    run: u64,
    context: &Context,
    images: &HashMap<String, proto::ArtifactRef>,
) -> Result<proto::TranscriptItem, Status> {
    let mut source = std::borrow::Cow::Borrowed(item);
    let mut presented = vec![];
    if let t::Body::User(user) = &item.body
        && user
            .attachments
            .iter()
            .any(|a| matches!(a, Attachment::Path { .. }))
        && let t::Body::User(user) = &mut source.to_mut().body
    {
        presented = presentable(&mut user.attachments, context);
    }
    let mut value = wire::item(
        &source,
        epoch,
        revision,
        run,
        context.project_file,
        &context.root,
        images,
    )?;
    if let Some(proto::transcript_item::Body::User(user)) = &mut value.body
        && let Some(draft) = &mut user.draft
    {
        restore(draft, presented);
    }
    Ok(value)
}

fn sync_status(error: sync::Error) -> Status {
    match error {
        sync::Error::Revoked => Status::permission_denied("Device access was revoked."),
        sync::Error::ResourceExhausted | sync::Error::FrameTooLarge => {
            Status::resource_exhausted("Remote stream budget exceeded.")
        }
        _ => resync("the chat stream must be reopened"),
    }
}
fn resync(reason: &str) -> Status {
    Status::failed_precondition(format!("resync_required: {reason}"))
}
fn cursor(value: &sync::Cursor) -> proto::Cursor {
    proto::Cursor {
        runtime_epoch: value.epoch().into(),
        stream_id: value.stream().into(),
        sequence: value.sequence(),
    }
}
pub(in crate::app_state::remote) fn state(status: &str, working: bool, decisions: usize) -> i32 {
    use proto::ThreadState as S;
    let state = if decisions > 0 {
        S::NeedsDecision
    } else if working {
        S::Working
    } else {
        match status {
            "Not started" => S::Dormant,
            "Starting" => S::Starting,
            "Stopping" => S::Interrupting,
            "Failed" => S::Failed,
            "Exited" => S::Exited,
            _ => S::Idle,
        }
    };
    state as i32
}
pub(super) fn summary(
    auth: &RpcAuth,
    record: &ChatRecord,
    snapshot: &ChatSnapshot,
) -> proto::ThreadStatus {
    proto::ThreadStatus {
        thread: reference(auth, record),
        title: snapshot.title.clone().unwrap_or_else(|| "New chat".into()),
        agent_id: record.agent().into(),
        profile_id: record.metadata().profile_id.clone(),
        state: state(snapshot.status, snapshot.working, snapshot.decisions.len()),
        pending_decisions: snapshot.decisions.len() as u32,
        queued_messages: snapshot.queue.as_ref().map_or(0, |q| {
            q.entries()
                .iter()
                .filter(|e| {
                    matches!(
                        e.state(),
                        host::QueueState::Queued
                            | host::QueueState::Attempting
                            | host::QueueState::DeliveryUnknown
                    )
                })
                .count() as u32
        }),
        run_id: Some(snapshot.run),
        history_epoch: snapshot.history_epoch,
        goal: snapshot.goal.as_ref().map(goal_state),
    }
}
/// The chat's current goal for its status (the transcript keeps its history).
fn goal_state(goal: &canopy_desktop::chat::goal::Goal) -> proto::GoalState {
    use canopy_desktop::chat::goal::GoalStatus as S;
    proto::GoalState {
        objective: goal.objective.clone(),
        state: match goal.status {
            S::Pursuing => "pursuing",
            S::Set => "set",
            S::Paused => "paused",
            S::Blocked => "blocked",
            S::UsageLimited => "usage_limited",
            S::BudgetLimited => "budget_limited",
        }
        .into(),
        turns: goal.turns,
        note: goal.last_check.clone(),
        tokens: goal.tokens,
        budget: goal.budget,
        elapsed_seconds: goal.seconds,
        checks: goal.checks,
        since_unix: (goal.since > 0).then_some(goal.since),
    }
}
fn set_saved(item: &mut proto::TranscriptItem) -> bool {
    match &mut item.body {
        Some(proto::transcript_item::Body::TurnEnd(end)) if !end.saved => {
            end.saved = true;
            true
        }
        _ => false,
    }
}

/// Live projections by chat.
#[derive(Default)]
pub(super) struct Registry {
    pub(super) entries: HashMap<String, Arc<Projector>>,
}

/// Host-wide and per-device admission of snapshot and history transfers (PERF-01).
pub(super) struct Reads {
    all: Arc<tokio::sync::Semaphore>,
    devices: Mutex<HashMap<String, Arc<tokio::sync::Semaphore>>>,
}
impl Default for Reads {
    fn default() -> Self {
        Self {
            all: Arc::new(tokio::sync::Semaphore::new(READS)),
            devices: Mutex::default(),
        }
    }
}
pub(super) struct ReadPermit {
    _all: tokio::sync::OwnedSemaphorePermit,
    _device: tokio::sync::OwnedSemaphorePermit,
}
impl Reads {
    pub(super) fn admit(&self, device: &str) -> Result<ReadPermit, Status> {
        let busy = || Status::resource_exhausted("Too many history reads in progress; retry.");
        let device = {
            let mut devices = self.devices.lock().unwrap_or_else(|e| e.into_inner());
            devices.retain(|_, s| Arc::strong_count(s) > 1 || s.available_permits() < DEVICE_READS);
            devices
                .entry(device.to_owned())
                .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(DEVICE_READS)))
                .clone()
        };
        let device = device.try_acquire_owned().map_err(|_| busy())?;
        let all = self.all.clone().try_acquire_owned().map_err(|_| busy())?;
        Ok(ReadPermit {
            _all: all,
            _device: device,
        })
    }
}

/// Ends a bridge subscription that was not handed over to a projection, also when
/// the RPC creating it is cancelled.
struct Pending(Option<(ChatBridge, u64)>);
impl Pending {
    fn disarm(&mut self) {
        self.0 = None;
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some((bridge, id)) = self.0.take() {
            tokio::spawn(async move { bridge.unsubscribe(id).await });
        }
    }
}

/// One connected client of a projection; dropping it detaches the client.
pub(super) struct Client {
    projector: Arc<Projector>,
    rpc: ChatsRpc,
    chat: String,
}
impl Drop for Client {
    fn drop(&mut self) {
        let generation = {
            let mut clients = self
                .projector
                .clients
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            clients.count = clients.count.saturating_sub(1);
            clients.generation += 1;
            if clients.count > 0 {
                None
            } else {
                clients.idle_since = Some(Instant::now());
                Some(clients.generation)
            }
        };
        self.rpc.report(&self.projector);
        if let Some(generation) = generation {
            let (projector, rpc, chat) =
                (self.projector.clone(), self.rpc.clone(), self.chat.clone());
            tokio::spawn(async move {
                tokio::time::sleep(rpc.grace).await;
                let idle = {
                    let clients = projector.clients.lock().unwrap_or_else(|e| e.into_inner());
                    clients.count == 0 && clients.generation == generation
                };
                if idle {
                    rpc.retire(&chat, &projector);
                }
            });
        }
    }
}

impl ChatsRpc {
    /// Tells the runtime how many clients watch it: one task at a time, until the
    /// reported count equals the current one.
    fn report(&self, projector: &Arc<Projector>) {
        {
            let mut clients = projector.clients.lock().unwrap_or_else(|e| e.into_inner());
            if clients.reporting || clients.reported == clients.count {
                return;
            }
            clients.reporting = true;
        }
        let (projector, bridge) = (projector.clone(), self.bridge.clone());
        tokio::spawn(async move {
            loop {
                let count = {
                    let mut clients = projector.clients.lock().unwrap_or_else(|e| e.into_inner());
                    if clients.reported == clients.count {
                        clients.reporting = false;
                        return;
                    }
                    clients.count
                };
                bridge.set_observers(projector.subscription, count).await;
                projector
                    .clients
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .reported = count;
            }
        });
    }
    fn attach(&self, chat: &str, projector: &Arc<Projector>) -> Client {
        {
            let mut clients = projector.clients.lock().unwrap_or_else(|e| e.into_inner());
            clients.count += 1;
            clients.generation += 1;
            clients.idle_since = None;
        }
        self.report(projector);
        Client {
            projector: projector.clone(),
            rpc: self.clone(),
            chat: chat.into(),
        }
    }
    /// Closes `projector` and forgets it (if it is still the chat's projection).
    fn retire(&self, chat: &str, projector: &Arc<Projector>) {
        let mut registry = self.projections.lock().unwrap_or_else(|e| e.into_inner());
        if registry
            .entries
            .get(chat)
            .is_some_and(|p| Arc::ptr_eq(p, projector))
        {
            registry.entries.remove(chat);
        }
        projector.stop.close();
    }

    /// A sent message as a device edits it: images published for reading, host paths
    /// presented relative to the checkout, pull requests signed again for this chat's
    /// checkout (so sending it back unchanged is accepted).
    pub(super) async fn editable_draft(
        &self,
        record: &ChatRecord,
        mut source: canopy_desktop::chat::session::Draft,
    ) -> Result<proto::MessageDraft, Status> {
        let files = source
            .attachments
            .iter()
            .filter_map(|a| match a {
                Attachment::Image { file, .. } => Some(file.clone()),
                _ => None,
            })
            .collect();
        let images = self.images(record.id(), files, HashMap::new()).await?;
        let context = Context::of(&self.auth, record);
        let presented = presentable(&mut source.attachments, &context);
        let mut draft = wire::draft(&source, context.project_file, &context.root, &images)?;
        restore(&mut draft, presented);
        for attachment in &mut draft.attachments {
            if let Some(proto::attachment::Content::Pull(pull)) = &mut attachment.content {
                *pull = self.pulls.publish(record.authority_root(), pull.clone());
            }
        }
        Ok(draft)
    }

    pub(super) async fn images(
        &self,
        chat: &str,
        files: Vec<String>,
        mut cache: HashMap<String, proto::ArtifactRef>,
    ) -> Result<HashMap<String, proto::ArtifactRef>, Status> {
        let files: Vec<_> = files
            .into_iter()
            .filter(|f| !cache.contains_key(f))
            .collect();
        if files.is_empty() {
            return Ok(cache);
        }
        let store = self.artifacts.clone();
        let root = self.attachments_root.clone();
        let chat = chat.to_owned();
        let host = self.auth.host_id().to_owned();
        tokio::task::spawn_blocking(move || {
            for file in files {
                if cache.contains_key(&file) {
                    continue;
                }
                let image = store
                    .import_chat_image(&chat, &file, &root)
                    .map_err(storage_status)?;
                cache.insert(
                    file,
                    proto::ArtifactRef {
                        host_id: host.clone(),
                        artifact_id: image.id().into(),
                        size: image.size(),
                        sha256: image.sha256().to_vec(),
                        media_type: image.media_type().into(),
                    },
                );
            }
            Ok(cache)
        })
        .await
        .map_err(|_| Status::unavailable("Image publication stopped."))?
    }
    async fn wire_queue(
        &self,
        record: &ChatRecord,
        context: &Context,
        snapshot: &ChatSnapshot,
    ) -> Result<proto::ThreadQueue, Status> {
        let Some(queue) = snapshot.queue.clone() else {
            return Ok(proto::ThreadQueue::default());
        };
        let store = self.artifacts.clone();
        let host = self.auth.host_id().to_owned();
        let chat = record.id().to_owned();
        let context = context.clone();
        let attachments_root = self.attachments_root.clone();
        tokio::task::spawn_blocking(move || {
            let reference = |image: canopy_desktop::artifacts::Artifact| proto::ArtifactRef {
                host_id: host.clone(),
                artifact_id: image.id().into(),
                media_type: image.media_type().into(),
                size: image.size(),
                sha256: image.sha256().to_vec(),
            };
            let mut entries = Vec::new();
            for entry in queue.entries() {
                let mut source = entry.draft().clone().into_draft();
                let mut images = HashMap::new();
                for a in &source.attachments {
                    if let Attachment::Image { file, .. } = a {
                        let image = store
                            .import_chat_image(&chat, file, &attachments_root)
                            .map_err(storage_status)?;
                        images.insert(file.clone(), reference(image));
                    }
                }
                let presented = presentable(&mut source.attachments, &context);
                let mut draft = wire::draft(&source, context.project_file, &context.root, &images)?;
                restore(&mut draft, presented);
                for id in entry.draft().artifact_ids() {
                    let image = store
                        .retained_image_metadata(&chat, id)
                        .map_err(storage_status)?;
                    draft.attachments.push(proto::Attachment {
                        label: "Image".into(),
                        content: Some(proto::attachment::Content::Image({
                            let artifact = reference(image);
                            proto::ImageAttachment {
                                host_id: artifact.host_id,
                                artifact_id: artifact.artifact_id,
                                sha256: artifact.sha256,
                                size: artifact.size,
                                media_type: artifact.media_type,
                            }
                        })),
                    });
                }
                let delivery = match entry.state() {
                    host::QueueState::Queued => proto::QueueDelivery::Queued,
                    host::QueueState::Attempting => proto::QueueDelivery::Attempting,
                    host::QueueState::Delivered => proto::QueueDelivery::Delivered,
                    host::QueueState::Failed => proto::QueueDelivery::Failed,
                    host::QueueState::Dismissed => proto::QueueDelivery::Dismissed,
                    host::QueueState::DeliveryUnknown => proto::QueueDelivery::Unknown,
                };
                entries.push(proto::QueueEntry {
                    message_id: entry.id().into(),
                    draft: Some(draft),
                    delivery: delivery as i32,
                    resolved_by_user: entry.resolved_by_user(),
                });
            }
            Ok(proto::ThreadQueue {
                revision: queue.revision(),
                paused: queue.paused(),
                entries,
            })
        })
        .await
        .map_err(|_| Status::unavailable("Queue projection stopped."))?
    }

    /// The chat's projection, created on first use.
    async fn projector(&self, record: &ChatRecord) -> Result<Arc<Projector>, Status> {
        if let Some(p) = self
            .projections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .get(record.id())
        {
            return Ok(p.clone());
        }
        let (subscription, events) = self
            .bridge
            .subscribe(record.id().into())
            .await
            .map_err(bridge_status)?;
        let mut pending = Pending(Some((self.bridge.clone(), subscription)));
        let first = events
            .recv()
            .await
            .map_err(|_| Status::unavailable("The chat snapshot stopped."))?;
        let ChatEvent::Snapshot(initial) = first else {
            return Err(Status::internal(
                "The runtime did not start with a snapshot.",
            ));
        };
        let (snapshot, epochs, earlier) = *initial;
        let context = Context::of(&self.auth, record);
        let mut serials = HashMap::new();
        let mut tail = Vec::new();
        for SnapshotEpoch {
            serial,
            history_epoch,
            items,
        } in epochs
        {
            if let Some(serial) = serial {
                serials.insert(serial, history_epoch);
            }
            tail.extend(items.into_iter().map(|i| (history_epoch, Arc::new(i))));
        }
        serials.insert(snapshot.serial, snapshot.history_epoch);
        let files = tail.iter().flat_map(|(_, i)| image_files(i)).collect();
        let images = self.images(record.id(), files, HashMap::new()).await?;
        let queue = self.wire_queue(record, &context, &snapshot).await?;
        // Which held versions are saved already: proven by the stored rows' hashes.
        let proof: HashMap<Key, [u8; 32]> = self
            .auth
            .client()
            .read_history_checkpoint(
                record.id().into(),
                tail.iter().map(|(e, i)| (*e, i.id.0)).collect(),
            )
            .await
            .map(|proof| {
                proof
                    .items()
                    .iter()
                    .map(|i| ((i.epoch(), i.item()), *i.sha256()))
                    .collect()
            })
            // Without a proof nothing counts as saved.
            .unwrap_or_default();
        let mut durability = Durability::default();
        let mut items = BTreeMap::new();
        for (epoch, source) in tail {
            let key = (epoch, source.id.0);
            let mut value = project(&source, epoch, 1, snapshot.run, &context, &images)?;
            decision_options(&mut value, &snapshot);
            if proof
                .get(&key)
                .is_some_and(|h| stored_hash(&source) == Some(*h))
            {
                set_saved(&mut value);
            } else {
                durability.item(0, key, source.clone());
            }
            items.insert(key, Slot::new(value, source));
        }
        let stream = format!("chat/{}/{}", record.id(), uuid::Uuid::new_v4());
        self.replay.open_stream(&stream).map_err(sync_status)?;
        let (stop, stopping) = async_channel::bounded::<()>(1);
        let (finished, done) = async_channel::bounded(1);
        let projector = Arc::new(Projector {
            subscription,
            data: Mutex::new(Data {
                snapshot,
                serials,
                items,
                revision: 1,
                images,
                stream: stream.clone(),
                earlier,
                closed: false,
                queue,
                durability,
                announced: 0,
            }),
            clients: Mutex::new(Clients {
                reported: 1,
                ..Default::default()
            }),
            stop,
            done,
        });
        {
            let mut registry = self.projections.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = registry.entries.get(record.id()) {
                // Lost a creation race: ours goes (`pending` unsubscribes it).
                self.replay.close_stream(&stream);
                return Ok(existing.clone());
            }
            if registry.entries.len() >= PROJECTIONS {
                let idle = registry
                    .entries
                    .iter()
                    .filter_map(|(chat, p)| {
                        let clients = p.clients.lock().unwrap_or_else(|e| e.into_inner());
                        (clients.count == 0).then(|| (clients.idle_since, chat.clone()))
                    })
                    .min();
                let Some((_, chat)) = idle else {
                    self.replay.close_stream(&stream);
                    return Err(Status::resource_exhausted(
                        "Too many chats are watched on this host; close one and retry.",
                    ));
                };
                if let Some(old) = registry.entries.remove(&chat) {
                    old.stop.close();
                }
            }
            registry
                .entries
                .insert(record.id().into(), projector.clone());
        }
        pending.disarm();
        let weak = Arc::downgrade(&projector);
        let this = self.clone();
        let record = record.clone();
        tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    _ = stopping.recv() => break,
                    event = events.recv() => match event { Ok(v) => v, Err(_) => break },
                };
                let Some(projector) = weak.upgrade() else {
                    break;
                };
                if this
                    .apply(&projector, &record, &context, event)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            this.replay.close_stream(&stream);
            if let Some(p) = weak.upgrade() {
                p.data.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
                this.retire(record.id(), &p);
            }
            this.bridge.unsubscribe(subscription).await;
            let _ = finished.try_send(());
        });
        Ok(projector)
    }

    async fn apply(
        &self,
        p: &Projector,
        record: &ChatRecord,
        context: &Context,
        event: ChatEvent,
    ) -> Result<(), Status> {
        use proto::thread_event::Event;
        let chat = required(reference(&self.auth, record))?;
        match event {
            ChatEvent::Items { serial, items } => {
                let incoming: Vec<_> = items.into_iter().map(|(_, i)| Arc::new(i)).collect();
                let cache = p
                    .data
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .images
                    .clone();
                let files = incoming.iter().flat_map(|i| image_files(i)).collect();
                let images = self.images(record.id(), files, cache).await?;
                let mut data = p.data.lock().unwrap_or_else(|e| e.into_inner());
                data.images = images;
                let epoch = *data
                    .serials
                    .get(&serial)
                    .ok_or_else(|| resync("unknown history generation"))?;
                for source in incoming {
                    data.revision = data
                        .revision
                        .checked_add(1)
                        .ok_or_else(|| resync("transcript revision exhausted"))?;
                    let mut value = project(
                        &source,
                        epoch,
                        data.revision,
                        data.snapshot.run,
                        context,
                        &data.images,
                    )?;
                    decision_options(&mut value, &data.snapshot);
                    let key = (epoch, source.id.0);
                    let append = data
                        .items
                        .get(&key)
                        .and_then(|old| wire::text_append(&old.item, &value));
                    let slot = Slot::new(value, source.clone());
                    data.items.insert(key, slot.clone());
                    while data.items.len() > TAIL {
                        data.items.pop_first();
                        data.earlier = true;
                    }
                    let size = slot.item.encoded_len();
                    let last = if let Some(append) = append {
                        self.publish(&data.stream, &chat, Event::AppendText(append))?
                    } else if size <= INLINE {
                        self.publish(&data.stream, &chat, Event::ReplaceItem(slot.item.clone()))?
                    } else if size <= LIVE_FRAGMENTED {
                        let encoded = slot.encoded();
                        let mut last = 0;
                        for fragment in fragments(&slot, &encoded) {
                            last =
                                self.publish(&data.stream, &chat, Event::ItemFragment(fragment))?;
                        }
                        last
                    } else {
                        self.publish(
                            &data.stream,
                            &chat,
                            Event::Resync(proto::ResyncRequired {
                                reason: "A large item changed; read it from a new snapshot.".into(),
                            }),
                        )?
                    };
                    data.durability.item(last, key, source);
                }
                // References the held items no longer use go beyond the cache size.
                if data.images.len() > IMAGES {
                    let used: std::collections::HashSet<String> = data
                        .items
                        .values()
                        .flat_map(|s| s.images.iter().cloned())
                        .collect();
                    data.images.retain(|file, _| used.contains(file));
                }
            }
            ChatEvent::State(snapshot) => {
                let queue = self.wire_queue(record, context, &snapshot).await?;
                let mut data = p.data.lock().unwrap_or_else(|e| e.into_inner());
                data.serials.insert(snapshot.serial, snapshot.history_epoch);
                if queue != data.queue {
                    data.queue = queue.clone();
                    let sequence = self.publish(&data.stream, &chat, Event::QueueChanged(queue))?;
                    data.durability.other(sequence);
                }
                let summary = summary(&self.auth, record, &snapshot);
                data.snapshot = *snapshot;
                let sequence = self.publish(&data.stream, &chat, Event::Status(summary))?;
                data.durability.other(sequence);
                // A permission's opaque choices can arrive after its item.
                let snapshot = data.snapshot.clone();
                let mut changed = vec![];
                for (key, slot) in &data.items {
                    let mut item = slot.item.clone();
                    decision_options(&mut item, &snapshot);
                    if item != slot.item {
                        changed.push((*key, Slot::new(item, slot.source.clone())));
                    }
                }
                for (key, slot) in changed {
                    if let Some(proto::transcript_item::Body::Decision(d)) = &slot.item.body {
                        let sequence =
                            self.publish(&data.stream, &chat, Event::Decision(d.clone()))?;
                        data.durability.other(sequence);
                    }
                    data.items.insert(key, slot);
                }
                self.announce(&mut data, &chat)?;
            }
            ChatEvent::Committed(proof) => {
                let proof: Vec<(Key, [u8; 32])> = proof
                    .items()
                    .iter()
                    .map(|i| ((i.epoch(), i.item()), *i.sha256()))
                    .collect();
                let mut data = p.data.lock().unwrap_or_else(|e| e.into_inner());
                for key in data.durability.commit(&proof) {
                    let Some(slot) = data.items.get(&key).cloned() else {
                        continue;
                    };
                    let mut item = slot.item.clone();
                    if !set_saved(&mut item) {
                        continue;
                    }
                    // The turn's end is saved with its final content (SYNC-07).
                    data.revision += 1;
                    item.revision = data.revision;
                    let saved = Slot::new(item, slot.source.clone());
                    data.items.insert(key, saved.clone());
                    let sequence =
                        self.publish(&data.stream, &chat, Event::ReplaceItem(saved.item.clone()))?;
                    data.durability.other(sequence);
                }
                self.announce(&mut data, &chat)?;
            }
            ChatEvent::SessionReplaced | ChatEvent::Stopping => {
                return Err(resync("the running history changed"));
            }
            ChatEvent::Snapshot(_) => {
                return Err(Status::internal("Unexpected second initial snapshot."));
            }
        }
        Ok(())
    }
    /// Publishes a higher saved watermark.
    fn announce(&self, data: &mut Data, chat: &proto::ThreadRef) -> Result<(), Status> {
        let saved = data.durability.saved();
        if saved > data.announced {
            data.announced = saved;
            self.publish(
                &data.stream,
                chat,
                proto::thread_event::Event::SavedSequence(saved),
            )?;
        }
        Ok(())
    }
    fn publish(
        &self,
        stream: &str,
        chat: &proto::ThreadRef,
        event: proto::thread_event::Event,
    ) -> Result<u64, Status> {
        let started = std::time::Instant::now();
        let result = self
            .replay
            .publish(
                stream,
                proto::ThreadEvent {
                    thread: Some(chat.clone()),
                    cursor: None,
                    event: Some(event),
                }
                .encode_to_vec(),
            )
            .map(|c| c.sequence())
            .map_err(sync_status);
        canopy_desktop::remote::metrics::record(
            &chat.host_id,
            &chat.thread_id,
            None,
            0,
            canopy_desktop::remote::metrics::Stage::Fanout,
            started,
            if result.is_ok() {
                canopy_desktop::remote::metrics::Outcome::Ok
            } else {
                canopy_desktop::remote::metrics::Outcome::Overflow
            },
        );
        result
    }

    pub(super) async fn subscribe(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        since: Option<proto::Cursor>,
    ) -> Result<RpcStream<proto::ThreadEvent>, Status> {
        let permit = self.reads.admit(grant.id())?;
        let p = self.projector(&record).await?;
        let client = self.attach(record.id(), &p);
        let chat = required(reference(&self.auth, &record))?;
        let event = |event| proto::ThreadEvent {
            thread: Some(chat.clone()),
            cursor: None,
            event: Some(event),
        };
        let (subscription, mut out, replay) = {
            let data = p.data.lock().unwrap_or_else(|e| e.into_inner());
            if data.closed {
                return Err(resync("the chat stream closed"));
            }
            let since =
                since.map(|c| sync::Cursor::from_parts(c.runtime_epoch, c.stream_id, c.sequence));
            let (start, subscription) = self
                .replay
                .subscribe(&data.stream, grant.id(), since.as_ref())
                .map_err(sync_status)?;
            let mut out = Out::default();
            let replay = match start {
                sync::Start::Replay { events, .. } => VecDeque::from(events),
                sync::Start::Snapshot { boundary } => {
                    let boundary = cursor(&boundary);
                    let saved = if data.durability.settled() {
                        boundary.sequence
                    } else {
                        data.durability.saved()
                    };
                    out.events.push_back(Part::Event(event(
                        proto::thread_event::Event::SnapshotStart(proto::SnapshotStart {
                            boundary: Some(boundary.clone()),
                            summary: Some(summary(&self.auth, &record, &data.snapshot)),
                            queue: Some(data.queue.clone()),
                            saved_sequence: saved,
                        }),
                    )));
                    let selected = select(data.items.values());
                    let earlier = data.earlier || selected.len() < data.items.len();
                    out.events.extend(selected.into_iter().map(Part::Slot));
                    out.events.push_back(Part::Event(event(
                        proto::thread_event::Event::SnapshotEnd(proto::SnapshotEnd {
                            boundary: Some(boundary),
                            has_earlier_history: earlier,
                        }),
                    )));
                    out.permit = Some(permit);
                    VecDeque::new()
                }
            };
            (subscription, out, replay)
        };
        out.chat = Some(chat);
        let state = Live {
            out,
            replay,
            subscription,
            host: self.state.watch(),
            auth: self.auth.clone(),
            grant,
            record,
            _client: client,
        };
        Ok(Box::pin(futures_lite::stream::unfold(
            Some(state),
            |state| async move {
                let mut state = state?;
                match state.next().await {
                    Ok(event) => Some((Ok(event), Some(state))),
                    Err(status) => Some((Err(status), None)),
                }
            },
        )))
    }

    pub async fn close_subscriptions(&self) {
        let projectors: Vec<_> = self
            .projections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .drain()
            .map(|(_, p)| p)
            .collect();
        for p in &projectors {
            p.stop.close();
        }
        for p in projectors {
            let _ = p.done.recv().await;
        }
    }
}

/// The newest held items within the snapshot budgets, oldest first.
fn select<'a>(items: impl DoubleEndedIterator<Item = &'a Arc<Slot>>) -> Vec<Arc<Slot>> {
    let (mut small, mut large) = (0, 0);
    let mut selected = Vec::new();
    for slot in items.rev() {
        let size = slot.item.encoded_len();
        let (used, limit) = if size <= INLINE {
            (&mut small, SNAPSHOT_BYTES)
        } else {
            (&mut large, SNAPSHOT_LARGE)
        };
        if *used + size > limit && !selected.is_empty() {
            break;
        }
        *used += size;
        selected.push(slot.clone());
    }
    selected.reverse();
    selected
}

/// Fragments of `slot`'s encoding, made one at a time (each copies ≤64 KiB).
fn fragments(
    slot: &Slot,
    encoded: &Arc<Encoded>,
) -> impl Iterator<Item = proto::ItemFragment> + Send + use<> {
    let (key, revision) = (slot.item.key, slot.item.revision);
    let transfer = uuid::Uuid::new_v4().to_string();
    let encoded = encoded.clone();
    let total = encoded.bytes.len();
    (0..total.div_ceil(FRAGMENT)).map(move |index| {
        let start = index * FRAGMENT;
        let end = (start + FRAGMENT).min(total);
        proto::ItemFragment {
            key,
            revision,
            transfer_id: transfer.clone(),
            offset: start as u64,
            total_bytes: total as u64,
            sha256: encoded.sha256.clone(),
            data: encoded.bytes[start..end].to_vec(),
        }
    })
}

/// A part of a transfer, produced lazily: small items are batched into one frame,
/// a large one becomes fragments of its shared encoding.
pub(super) enum Part<E> {
    Event(E),
    Slot(Arc<Slot>),
}
pub(super) struct Out<E> {
    pub(super) events: VecDeque<Part<E>>,
    fragments: Option<Box<dyn Iterator<Item = proto::ItemFragment> + Send>>,
    /// Held until the transfer part is sent (read admission).
    pub(super) permit: Option<ReadPermit>,
    pub(super) chat: Option<proto::ThreadRef>,
}
impl<E> Default for Out<E> {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            fragments: None,
            permit: None,
            chat: None,
        }
    }
}
/// How a transfer's items and fragments are framed for one stream type.
pub(super) trait Frame: Sized {
    fn items(chat: &proto::ThreadRef, items: Vec<proto::TranscriptItem>) -> Self;
    fn fragment(chat: &proto::ThreadRef, fragment: proto::ItemFragment) -> Self;
}
impl Frame for proto::ThreadEvent {
    fn items(chat: &proto::ThreadRef, items: Vec<proto::TranscriptItem>) -> Self {
        Self {
            thread: Some(chat.clone()),
            cursor: None,
            event: Some(proto::thread_event::Event::SnapshotItems(
                proto::ItemChunk { items },
            )),
        }
    }
    fn fragment(chat: &proto::ThreadRef, fragment: proto::ItemFragment) -> Self {
        Self {
            thread: Some(chat.clone()),
            cursor: None,
            event: Some(proto::thread_event::Event::ItemFragment(fragment)),
        }
    }
}
impl<E: Frame> Out<E> {
    /// The next frame of the transfer; `None` when it is complete.
    pub(super) fn next(&mut self) -> Option<E> {
        let chat = self.chat.clone().unwrap_or_default();
        let event = 'next: {
            if let Some(fragments) = &mut self.fragments {
                if let Some(fragment) = fragments.next() {
                    break 'next E::fragment(&chat, fragment);
                }
                self.fragments = None;
            }
            match self.events.pop_front()? {
                Part::Event(event) => event,
                Part::Slot(slot) if slot.item.encoded_len() > INLINE => {
                    let encoded = slot.encoded();
                    let mut fragments = fragments(&slot, &encoded);
                    let first = fragments.next()?;
                    self.fragments = Some(Box::new(fragments));
                    E::fragment(&chat, first)
                }
                Part::Slot(slot) => {
                    let mut bytes = slot.item.encoded_len();
                    let mut items = vec![slot.item.clone()];
                    while let Some(Part::Slot(next)) = self.events.front() {
                        let size = next.item.encoded_len();
                        if size > INLINE || bytes + size > INLINE {
                            break;
                        }
                        bytes += size;
                        items.push(next.item.clone());
                        self.events.pop_front();
                    }
                    E::items(&chat, items)
                }
            }
        };
        if self.events.is_empty() && self.fragments.is_none() {
            self.permit = None;
        }
        Some(event)
    }
}

/// One client's live stream: snapshot (if any), replayed frames, then the tail.
struct Live {
    out: Out<proto::ThreadEvent>,
    replay: VecDeque<sync::Delivery>,
    subscription: sync::Subscription,
    host: tokio::sync::watch::Receiver<canopy_desktop::api::proto::HostStatus>,
    auth: RpcAuth,
    grant: DeviceGrant,
    record: ChatRecord,
    _client: Client,
}
impl Live {
    async fn next(&mut self) -> Result<proto::ThreadEvent, Status> {
        self.subscription.check().map_err(sync_status)?;
        if let Some(event) = self.out.next() {
            return Ok(event);
        }
        let delivery = match self.replay.pop_front() {
            Some(delivery) => delivery,
            None => loop {
                tokio::select! {
                    delivery = self.subscription.recv() => break delivery.map_err(sync_status)?,
                    changed = self.host.changed() => {
                        changed.map_err(|_| Status::unavailable("The host stopped."))?;
                        self.recheck().await?;
                    }
                }
            },
        };
        let mut event = proto::ThreadEvent::decode(delivery.payload())
            .map_err(|_| Status::internal("Invalid host stream encoding."))?;
        event.cursor = Some(cursor(delivery.cursor()));
        Ok(event)
    }
    /// Authorization or readiness changed: the grant is checked again before any
    /// further disclosure (SEC-04).
    async fn recheck(&mut self) -> Result<(), Status> {
        let readiness = self.host.borrow_and_update().readiness;
        if readiness == canopy_desktop::api::proto::HostReadiness::Draining as i32
            || readiness == canopy_desktop::api::proto::HostReadiness::StorageFault as i32
        {
            return Err(Status::unavailable("The host is closing; reconnect later."));
        }
        self.auth
            .resource(self.grant.id(), host::READ, self.record.authority_root())
            .await?;
        Ok(())
    }
}

pub(super) fn decision_options(item: &mut proto::TranscriptItem, snapshot: &ChatSnapshot) {
    let Some(proto::transcript_item::Body::Decision(decision)) = &mut item.body else {
        return;
    };
    if let Some(pending) = snapshot
        .decisions
        .iter()
        .find(|p| p.item.to_string() == decision.decision_id)
    {
        decision.run_id = pending.run.to_string();
        decision.options = pending
            .options
            .iter()
            .map(|(id, name, kind)| proto::OptionChoice {
                id: id.clone(),
                label: name.clone(),
                description: Some(kind.clone()),
            })
            .collect();
        if !decision.options.is_empty() {
            decision.native_kind = None;
        }
    }
}
