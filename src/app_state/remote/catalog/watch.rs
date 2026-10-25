//! WatchSummaries: the whole catalog first, then only what changed for this device.
//! Wakes on the app's catalog pushes (projects, chats, catalog records, terminals),
//! terminal lease changes and host state (authorization, draining); never polls.
//! Each wake checks the device credential again before disclosing anything (SEC-04).
use super::{
    pages::{FRAME_LIMIT, PAGE_TARGET, Pager, Projection, footprint},
    *,
};
use crate::app_state::chat_bridge::CatalogWatch;
use canopy_desktop::remote::{metrics, sync::ByteReservation};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use tokio::sync::watch as tokio_watch;

/// What this device was last sent, as row digests (never a second catalog copy).
#[derive(Default, PartialEq)]
pub(super) struct Sent {
    projects: [u8; 32],
    chats: BTreeMap<String, [u8; 32]>,
    terminals: BTreeMap<String, [u8; 32]>,
}

/// Ends a lease forwarder with the stream or the next terminal refresh.
struct Forwarder(tokio::task::JoinHandle<()>);
impl Drop for Forwarder {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Watch {
    this: CatalogRpc,
    credential: Vec<u8>,
    events: CatalogWatch,
    host: tokio_watch::Receiver<proto::HostStatus>,
    leases: async_channel::Receiver<()>,
    lease_sender: async_channel::Sender<()>,
    forwarders: Vec<Forwarder>,
    last: Option<Arc<CatalogEvent>>,
    queue: Queue,
    /// The last event handed to the transport, released when it asks for the next.
    wire: Option<ByteReservation>,
}

enum Wake {
    Catalog(Arc<CatalogEvent>),
    Changed,
    End(Status),
}

pub(super) async fn stream(
    this: CatalogRpc,
    credential: Vec<u8>,
) -> Result<RpcStream<proto::SummaryEvent>, Status> {
    let events = this.bridge.watch_catalog().await.map_err(bridge_status)?;
    let (lease_sender, leases) = async_channel::bounded(1);
    let watch = Watch {
        host: this.state.watch(),
        queue: Queue::new(format!("catalog/{}", uuid::Uuid::new_v4())),
        this,
        credential,
        events,
        leases,
        lease_sender,
        forwarders: vec![],
        last: None,
        wire: None,
    };
    Ok(Box::pin(futures_lite::stream::unfold(
        Some(watch),
        |watch| async move {
            let mut watch = watch?;
            match watch.next().await {
                Ok(event) => Some((Ok(event), Some(watch))),
                Err(status) => Some((Err(status), None)),
            }
        },
    )))
}

impl Watch {
    /// The next event this device may see. The device is authorized again before
    /// every event; when its authorization changed, everything queued for the old
    /// one is dropped and the catalog is projected again from a new snapshot.
    async fn next(&mut self) -> Result<proto::SummaryEvent, Status> {
        // The transport took the previous event.
        self.wire = None;
        loop {
            if !self.queue.pending() {
                self.step().await?;
                continue;
            }
            let grant = authorize(&self.this.auth, &mut self.host, &self.credential).await?;
            if !self.queue.admit(&grant) {
                self.project(grant).await?;
                continue;
            }
            let epoch = self.this.state.runtime_epoch();
            let Some(event) = self.queue.take(&epoch)? else {
                continue;
            };
            self.wire = Some(wire(&self.this.output, &grant, &event)?);
            return Ok(event);
        }
    }

    async fn wake(&mut self) -> Wake {
        if self.last.is_none() {
            // The app sends the whole catalog right after the watch starts.
            return match self.events.recv().await {
                Ok(event) => Wake::Catalog(event),
                Err(_) => Wake::End(resync()),
            };
        }
        tokio::select! {
            event = self.events.recv() => match event {
                Ok(event) => Wake::Catalog(event),
                Err(_) => Wake::End(Status::unavailable("Canopy is closing.")),
            },
            changed = self.host.changed() => match changed {
                Ok(()) => Wake::Changed,
                Err(_) => Wake::End(Status::unavailable("The host stopped.")),
            },
            _ = self.leases.recv() => Wake::Changed,
        }
    }

    /// Waits for a change and queues what this device may see of it.
    async fn step(&mut self) -> Result<(), Status> {
        match self.wake().await {
            Wake::Catalog(event) => self.last = Some(event),
            Wake::Changed => {}
            Wake::End(status) => return Err(status),
        }
        let started = std::time::Instant::now();
        let grant = authorize(&self.this.auth, &mut self.host, &self.credential).await?;
        let result = self.project(grant).await;
        let outcome = match &result {
            Ok(()) => metrics::Outcome::Ok,
            Err(status) if status.code() == tonic::Code::ResourceExhausted => {
                metrics::Outcome::Overflow
            }
            Err(_) => metrics::Outcome::Invalid,
        };
        metrics::record(
            self.this.auth.host_id(),
            &metrics::resource_id(&self.queue.stream),
            None,
            self.queue.sequence,
            metrics::Stage::Fanout,
            started,
            outcome,
        );
        result
    }

    /// Queues the changes of the newest catalog as `grant` may see it.
    async fn project(&mut self, grant: DeviceGrant) -> Result<(), Status> {
        let terminals = self.this.terminals.terminals().await?;
        self.watch_leases(&terminals);
        let Some(event) = &self.last else {
            return Ok(());
        };
        let current = Projection::build(
            &self.this.output,
            &self.this.auth,
            &grant,
            event,
            &terminals,
        )?;
        let epoch = self.this.state.runtime_epoch();
        self.queue.queue(&self.this.output, grant, current, &epoch)
    }

    /// One forwarder per terminal run wakes the stream when its input owner changes.
    fn watch_leases(&mut self, terminals: &[TerminalInfo]) {
        self.forwarders = terminals
            .iter()
            .filter_map(TerminalInfo::run)
            .map(|run| {
                let mut changes = run.observe();
                let wake = self.lease_sender.clone();
                Forwarder(tokio::spawn(async move {
                    while changes.changed().await.is_ok() {
                        // Coalesced: one pending wake is enough.
                        let _ = wake.try_send(());
                    }
                }))
            })
            .collect();
    }
}

/// The device's current authorization, refused when the host is closing.
pub(super) async fn authorize(
    auth: &RpcAuth,
    host: &mut tokio_watch::Receiver<proto::HostStatus>,
    credential: &[u8],
) -> Result<DeviceGrant, Status> {
    let readiness = host.borrow_and_update().readiness;
    if readiness == proto::HostReadiness::Draining as i32
        || readiness == proto::HostReadiness::StorageFault as i32
    {
        return Err(Status::unavailable("The host is closing; reconnect later."));
    }
    let grant = auth
        .client()
        .lookup_runtime_device(credential.to_vec())
        .await
        .map_err(storage_status)?;
    auth.scope(&grant, host::READ)?;
    Ok(grant)
}

/// Charges `event` to the device's share of the stream budget until the transport
/// asks for the next one.
pub(super) fn wire(
    output: &canopy_desktop::remote::sync::ReplayHub,
    grant: &DeviceGrant,
    event: &proto::SummaryEvent,
) -> Result<ByteReservation, Status> {
    output
        .reserve_output(grant.id(), event.encoded_len())
        .map_err(|_| {
            Status::resource_exhausted(
                "resync_required: the device's stream budget is full; list the catalog again.",
            )
        })
}

/// What one device's catalog stream still has to send, projected for one grant.
pub(super) struct Queue {
    stream: String,
    sequence: u64,
    /// The authorization the queued events were projected for.
    grant: Option<DeviceGrant>,
    sent: Option<Sent>,
    /// A snapshot being sent page by page; deltas wait for its last page.
    snapshot: Option<Pager>,
    deltas: VecDeque<proto::summary_event::Change>,
    material: Option<ByteReservation>,
}

impl Queue {
    pub(super) fn new(stream: String) -> Self {
        Self {
            stream,
            sequence: 0,
            grant: None,
            sent: None,
            snapshot: None,
            deltas: VecDeque::new(),
            material: None,
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.snapshot.is_some() || !self.deltas.is_empty()
    }

    /// Whether what is queued was projected for `grant`. If not, all of it is dropped
    /// (nothing of an older authorization is sent) and the next projection starts
    /// from a snapshot.
    pub(super) fn admit(&mut self, grant: &DeviceGrant) -> bool {
        if self.grant.as_ref() == Some(grant) {
            return true;
        }
        self.snapshot = None;
        self.deltas.clear();
        self.material = None;
        self.sent = None;
        self.grant = None;
        false
    }

    /// Queues what changed for `grant` since the last projection: a snapshot (plain
    /// when small, else paged) or chat and terminal deltas.
    pub(super) fn queue(
        &mut self,
        output: &canopy_desktop::remote::sync::ReplayHub,
        grant: DeviceGrant,
        current: Arc<Projection>,
        epoch: &str,
    ) -> Result<(), Status> {
        if !self.admit(&grant) {
            self.grant = Some(grant);
        }
        let (plan, sent) = diff(self.sent.as_ref(), &current.catalog);
        self.sent = Some(sent);
        if let Plan::Deltas(changes) = plan {
            let bytes = changes.iter().map(|c| c.encoded_len()).sum::<usize>();
            // Many changes at once cost more than the catalog: a snapshot instead.
            if bytes <= FRAME_LIMIT {
                if !changes.is_empty() {
                    self.material = Some(output.reserve_assembly(bytes).map_err(|_| too_busy())?);
                }
                self.deltas.extend(changes);
                return Ok(());
            }
        }
        // A snapshot replaces whatever was still queued.
        self.deltas.clear();
        self.material = None;
        if current.catalog.encoded_len() <= PAGE_TARGET {
            // A small catalog stays one plain snapshot (older clients read it); its
            // copy is charged until it is sent.
            self.material = Some(
                output
                    .reserve_assembly(footprint(&current.catalog))
                    .map_err(|_| too_busy())?,
            );
            self.deltas
                .push_back(proto::summary_event::Change::Snapshot(
                    current.catalog.clone(),
                ));
            return Ok(());
        }
        let boundary = self.cursor(epoch);
        self.snapshot = Some(Pager::new(current, boundary));
        Ok(())
    }

    /// The next queued event, `None` when a snapshot just ended.
    pub(super) fn take(&mut self, epoch: &str) -> Result<Option<proto::SummaryEvent>, Status> {
        use proto::summary_event::Change;
        if let Some(pager) = &mut self.snapshot {
            let boundary = pager.boundary().clone();
            return match pager.next_part()? {
                Some(part) => Ok(Some(proto::SummaryEvent {
                    cursor: Some(boundary),
                    change: Some(Change::SnapshotPart(part)),
                })),
                None => {
                    self.snapshot = None;
                    Ok(None)
                }
            };
        }
        let Some(change) = self.deltas.pop_front() else {
            return Ok(None);
        };
        if self.deltas.is_empty() {
            self.material = None;
        }
        Ok(Some(proto::SummaryEvent {
            cursor: Some(self.cursor(epoch)),
            change: Some(change),
        }))
    }

    fn cursor(&mut self, epoch: &str) -> proto::Cursor {
        self.sequence += 1;
        proto::Cursor {
            // The host's runtime epoch: cursors never survive a restart.
            runtime_epoch: epoch.into(),
            stream_id: self.stream.clone(),
            sequence: self.sequence,
        }
    }
}

fn too_busy() -> Status {
    Status::resource_exhausted("resource_exhausted: the host is busy; list the catalog again.")
}

fn digest(message: &impl Message) -> [u8; 32] {
    Sha256::digest(message.encode_to_vec()).into()
}

/// What to send next for a device.
pub(super) enum Plan {
    /// The whole catalog (first, and whenever projects change: they have no delta).
    Snapshot,
    /// Changed and removed chats and terminals only.
    Deltas(Vec<proto::summary_event::Change>),
}

/// What to send after `sent` for catalog `current`, and what is then sent.
pub(super) fn diff(sent: Option<&Sent>, current: &proto::Catalog) -> (Plan, Sent) {
    use proto::summary_event::Change;
    let mut projects = Sha256::new();
    for project in &current.projects {
        projects.update(project.encode_to_vec());
    }
    let next = Sent {
        projects: projects.finalize().into(),
        chats: current
            .chats
            .iter()
            .filter_map(|c| Some((c.chat.as_ref()?.chat_id.clone(), digest(c))))
            .collect(),
        terminals: current
            .terminals
            .iter()
            .filter_map(|t| Some((t.terminal.as_ref()?.terminal_id.clone(), digest(t))))
            .collect(),
    };
    let Some(sent) = sent.filter(|sent| sent.projects == next.projects) else {
        return (Plan::Snapshot, next);
    };
    let host = &current.host_id;
    let mut changes = vec![];
    for chat in &current.chats {
        let Some(id) = chat.chat.as_ref().map(|c| &c.chat_id) else {
            continue;
        };
        if sent.chats.get(id) != next.chats.get(id) {
            changes.push(Change::Chat(chat.clone()));
        }
    }
    for id in sent.chats.keys().filter(|id| !next.chats.contains_key(*id)) {
        changes.push(Change::Deleted(proto::ResourceRef {
            resource: Some(proto::resource_ref::Resource::Chat(proto::ChatRef {
                host_id: host.clone(),
                chat_id: id.clone(),
            })),
        }));
    }
    for terminal in &current.terminals {
        let Some(id) = terminal.terminal.as_ref().map(|t| &t.terminal_id) else {
            continue;
        };
        if sent.terminals.get(id) != next.terminals.get(id) {
            changes.push(Change::Terminal(terminal.clone()));
        }
    }
    for id in sent
        .terminals
        .keys()
        .filter(|id| !next.terminals.contains_key(*id))
    {
        changes.push(Change::Deleted(proto::ResourceRef {
            resource: Some(proto::resource_ref::Resource::Terminal(
                proto::TerminalRef {
                    host_id: host.clone(),
                    terminal_id: id.clone(),
                },
            )),
        }));
    }
    (Plan::Deltas(changes), next)
}

fn resync() -> Status {
    Status::failed_precondition("resync_required: list the catalog again.")
}
