//! A device's catalog projection as typed, byte-bounded snapshot pages.
//!
//! What is shared and what is charged: the app host builds one canonical
//! `CatalogEvent` per change and every catalog stream (and each unary read) holds
//! the same `Arc` of it; that native state is not a remote ledger and is not charged
//! here. What one device costs is its projection (the rows its grant may see, the
//! index used to build them) and the pages cut from it. That material is estimated
//! from the shared event and charged to the host's assembly budget (32 MiB across
//! the host) *before* anything is copied, so parallel large reads are refused
//! instead of allocated. The built projection is shared by `Arc` while its pages are
//! cut lazily, one at a time; a project whose checkouts do not fit one page
//! continues on the next under the same metadata.
use super::*;
use canopy_desktop::remote::sync::{ByteReservation, ReplayHub};
use prost::encoding::{encoded_len_varint, message};
use std::mem::size_of;

/// What one page aims at; a single row larger than this gets a page of its own.
pub(super) const PAGE_TARGET: usize = 64 * 1024;
/// The transport's frame limit: no page (and no unary catalog) may exceed it.
pub(super) const FRAME_LIMIT: usize = 1024 * 1024;
/// The longest `workspace_ref`: a `u64` in JSON.
const WORKSPACE_REF: usize = 20;
/// A terminal's input owner as listed (a device ID, at most this long).
const INPUT_DEVICE: usize = 256;

fn too_large() -> Status {
    Status::resource_exhausted(
        "resource_exhausted: the host catalog is larger than the remote budget; retry later.",
    )
}

/// One projection with its share of the assembly budget, released when dropped.
pub(super) struct Projection {
    pub(super) catalog: proto::Catalog,
    _material: (ByteReservation, Option<ByteReservation>),
}

impl Projection {
    /// `event` as `grant` may see it, charged before it is built.
    pub(super) fn build(
        hub: &ReplayHub,
        auth: &RpcAuth,
        grant: &DeviceGrant,
        event: &CatalogEvent,
        terminals: &[TerminalInfo],
    ) -> Result<Arc<Self>, Status> {
        Self::admit(hub, estimate(auth, grant, event, terminals), || {
            catalog(auth, grant, event, terminals)
        })
    }

    /// Reserves `estimate` bytes, then runs `build`; refused without building when
    /// the budget is full. Should the build exceed the estimate, the excess is
    /// charged right after it, or the projection dropped (never held uncharged).
    pub(super) fn admit(
        hub: &ReplayHub,
        estimate: usize,
        build: impl FnOnce() -> proto::Catalog,
    ) -> Result<Arc<Self>, Status> {
        let material = hub.reserve_assembly(estimate).map_err(|_| too_large())?;
        let catalog = build();
        let more = footprint(&catalog).saturating_sub(estimate);
        let extra = (more > 0)
            .then(|| hub.reserve_assembly(more).map_err(|_| too_large()))
            .transpose()?;
        Ok(Arc::new(Self {
            catalog,
            _material: (material, extra),
        }))
    }

    /// Charges an already built `catalog` (tests that start from wire rows).
    #[cfg(test)]
    pub(super) fn charge(hub: &ReplayHub, catalog: proto::Catalog) -> Result<Arc<Self>, Status> {
        Self::admit(hub, footprint(&catalog), || catalog)
    }
}

/// An upper bound of what `catalog(auth, grant, event, terminals)` allocates, read
/// from the shared event without copying it: every visible row with its strings,
/// vector slack (doubling growth, at least four slots), the project map's nodes and
/// the chat-record index.
pub(super) fn estimate(
    auth: &RpcAuth,
    grant: &DeviceGrant,
    event: &CatalogEvent,
    terminals: &[TerminalInfo],
) -> usize {
    let host = auth.host_id().len();
    let checkout = size_of::<proto::WorkspaceSummary>();
    let project = size_of::<proto::ProjectSummary>();
    let mut bytes = size_of::<proto::Catalog>() + host;
    for w in &event.workspaces {
        let Some(path) = w.canonical_root.to_str().filter(|p| visible(grant, p)) else {
            continue;
        };
        let repository = w.repository.as_deref().and_then(|p| p.to_str());
        let root = repository.unwrap_or(path).len();
        // Its checkout row, in a vector that may be its project's own (4 slots).
        bytes += 6 * checkout
            + host
            + WORKSPACE_REF
            + w.name.len()
            + path.len()
            + repository.map_or(0, str::len);
        // Its project in the map (nodes at least half full), then in the result.
        bytes += 3 * (size_of::<String>() + project) + 2 * project + 3 * root + w.name.len();
    }
    let index = (event.records.len() * 8 / 7 + 8).next_power_of_two();
    bytes += index * (size_of::<(&str, &ChatRecord)>() + 1);
    let chat = size_of::<proto::ChatSummary>();
    bytes += 4 * chat;
    for record in &event.records {
        if !visible(grant, record.authority_root()) {
            continue;
        }
        bytes += 2 * chat
            + 2 * host
            + record.id().len()
            + record.workspace().map_or(0, str::len)
            + record.agent().len()
            + record.metadata().profile_id.as_deref().map_or(0, str::len);
    }
    // Titles of visible chats, bounded by all of them ("New chat" when unnamed).
    bytes += event
        .chats
        .iter()
        .map(|c| c.title.as_deref().map_or(8, str::len))
        .sum::<usize>();
    let terminal = size_of::<proto::TerminalSummary>();
    bytes += 4 * terminal;
    for t in terminals {
        if t.root().to_str().is_some_and(|root| visible(grant, root)) {
            bytes += 2 * terminal
                + 2 * host
                + t.id().len()
                + t.workspace().map_or(0, str::len)
                + t.title().len()
                + INPUT_DEVICE;
        }
    }
    bytes
}

/// What a built catalog holds in memory: its rows' slots and string capacities.
pub(super) fn footprint(catalog: &proto::Catalog) -> usize {
    let text = |s: &String| s.capacity();
    let optional = |s: &Option<String>| s.as_ref().map_or(0, String::capacity);
    let workspace = |w: &Option<proto::WorkspaceRef>| {
        w.as_ref()
            .map_or(0, |w| w.host_id.capacity() + w.workspace_id.capacity())
    };
    let checkouts = |p: &proto::ProjectSummary| {
        p.checkouts.capacity() * size_of::<proto::WorkspaceSummary>()
            + p.checkouts
                .iter()
                .map(|c| {
                    workspace(&c.workspace)
                        + text(&c.name)
                        + text(&c.path)
                        + optional(&c.repository_path)
                })
                .sum::<usize>()
    };
    let projects = catalog.projects.capacity() * size_of::<proto::ProjectSummary>()
        + catalog
            .projects
            .iter()
            .map(|p| text(&p.project_id) + text(&p.name) + text(&p.path) + checkouts(p))
            .sum::<usize>();
    let chats = catalog.chats.capacity() * size_of::<proto::ChatSummary>()
        + catalog
            .chats
            .iter()
            .map(|c| {
                c.chat
                    .as_ref()
                    .map_or(0, |c| c.host_id.capacity() + c.chat_id.capacity())
                    + workspace(&c.workspace)
                    + text(&c.title)
                    + text(&c.agent_id)
                    + optional(&c.profile_id)
                    + optional(&c.run_id)
                    + c.goal.as_ref().map_or(0, Message::encoded_len)
            })
            .sum::<usize>();
    let terminals = catalog.terminals.capacity() * size_of::<proto::TerminalSummary>()
        + catalog
            .terminals
            .iter()
            .map(|t| {
                t.terminal
                    .as_ref()
                    .map_or(0, |t| t.host_id.capacity() + t.terminal_id.capacity())
                    + workspace(&t.workspace)
                    + text(&t.title)
                    + optional(&t.input_device_id)
            })
            .sum::<usize>();
    size_of::<proto::Catalog>() + text(&catalog.host_id) + projects + chats + terminals
}

/// Cuts a projection into `CatalogSnapshotPart`s sharing `boundary`.
pub(super) struct Pager {
    projection: Arc<Projection>,
    boundary: proto::Cursor,
    project: usize,
    checkout: usize,
    chat: usize,
    terminal: usize,
    first: bool,
    done: bool,
}

impl Pager {
    pub(super) fn new(projection: Arc<Projection>, boundary: proto::Cursor) -> Self {
        Self {
            projection,
            boundary,
            project: 0,
            checkout: 0,
            chat: 0,
            terminal: 0,
            first: true,
            done: false,
        }
    }

    pub(super) fn boundary(&self) -> &proto::Cursor {
        &self.boundary
    }

    /// The next part, `None` after the last one.
    pub(super) fn next_part(&mut self) -> Result<Option<proto::CatalogSnapshotPart>, Status> {
        if self.done {
            return Ok(None);
        }
        let page = self.page();
        if page.encoded_len() > FRAME_LIMIT - 1024 {
            // One row over the frame limit cannot be sent whole.
            return Err(too_large());
        }
        let catalog = &self.projection.catalog;
        self.done = self.project == catalog.projects.len()
            && self.chat == catalog.chats.len()
            && self.terminal == catalog.terminals.len();
        let part = proto::CatalogSnapshotPart {
            boundary: Some(self.boundary.clone()),
            page: Some(page),
            first: std::mem::take(&mut self.first),
            last: self.done,
        };
        Ok(Some(part))
    }

    /// Rows from the current position until the page target is reached (at least one).
    fn page(&mut self) -> proto::Catalog {
        let catalog = &self.projection.catalog;
        let mut page = proto::Catalog {
            host_id: catalog.host_id.clone(),
            ..Default::default()
        };
        let mut size = page.encoded_len();
        let mut rows = 0usize;
        let fits = |size: usize, add: usize, rows: usize| rows == 0 || size + add <= PAGE_TARGET;
        while let Some(project) = catalog.projects.get(self.project) {
            // Only the metadata: the checkouts are copied page by page below.
            let mut entry = proto::ProjectSummary {
                project_id: project.project_id.clone(),
                name: project.name.clone(),
                path: project.path.clone(),
                checkouts: vec![],
            };
            let mut inner = entry.encoded_len();
            let outer = |inner: usize| 1 + encoded_len_varint(inner as u64) + inner;
            if !fits(size, outer(inner), rows) {
                return page;
            }
            // Alone on its page, a project takes at least one checkout (progress).
            let alone = rows == 0;
            rows += 1;
            while let Some(checkout) = project.checkouts.get(self.checkout) {
                let add = message::encoded_len(4, checkout);
                let first = alone && entry.checkouts.is_empty();
                if !first && size + outer(inner + add) > PAGE_TARGET {
                    break;
                }
                inner += add;
                entry.checkouts.push(checkout.clone());
                self.checkout += 1;
            }
            size += outer(inner);
            page.projects.push(entry);
            if self.checkout < project.checkouts.len() {
                // The project continues on the next page.
                return page;
            }
            self.project += 1;
            self.checkout = 0;
        }
        while let Some(chat) = catalog.chats.get(self.chat) {
            let add = message::encoded_len(3, chat);
            if !fits(size, add, rows) {
                return page;
            }
            size += add;
            rows += 1;
            page.chats.push(chat.clone());
            self.chat += 1;
        }
        while let Some(terminal) = catalog.terminals.get(self.terminal) {
            let add = message::encoded_len(4, terminal);
            if !fits(size, add, rows) {
                return page;
            }
            size += add;
            rows += 1;
            page.terminals.push(terminal.clone());
            self.terminal += 1;
        }
        page
    }
}

/// `ReadCatalog`: the pages of one projection, the device authorized again before
/// each page. A changed authorization ends the read (the client stages nothing of it
/// and reads again), so no page of the old projection follows a revoke.
pub(super) fn read(
    this: CatalogRpc,
    credential: Vec<u8>,
    grant: DeviceGrant,
    pager: Pager,
) -> RpcStream<proto::SummaryEvent> {
    struct Read {
        this: CatalogRpc,
        credential: Vec<u8>,
        grant: DeviceGrant,
        pager: Pager,
        host: tokio::sync::watch::Receiver<proto::HostStatus>,
        wire: Option<ByteReservation>,
    }
    impl Read {
        async fn next(&mut self) -> Result<Option<proto::SummaryEvent>, Status> {
            // The transport took the previous page.
            self.wire = None;
            let grant =
                super::watch::authorize(&self.this.auth, &mut self.host, &self.credential).await?;
            if grant != self.grant {
                return Err(Status::aborted(
                    "resync_required: the device's access changed; read the catalog again.",
                ));
            }
            let boundary = self.pager.boundary().clone();
            let Some(part) = self.pager.next_part()? else {
                return Ok(None);
            };
            let event = proto::SummaryEvent {
                cursor: Some(boundary),
                change: Some(proto::summary_event::Change::SnapshotPart(part)),
            };
            self.wire = Some(super::watch::wire(&self.this.output, &grant, &event)?);
            Ok(Some(event))
        }
    }
    let read = Read {
        host: this.state.watch(),
        this,
        credential,
        grant,
        pager,
        wire: None,
    };
    Box::pin(futures_lite::stream::unfold(
        Some(read),
        |read| async move {
            let mut read = read?;
            match read.next().await {
                Ok(Some(event)) => Some((Ok(event), Some(read))),
                Ok(None) => None,
                Err(status) => Some((Err(status), None)),
            }
        },
    ))
}
