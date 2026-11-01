//! Temporary value projection at the retained desktop boundary. Resource identities
//! are mapped explicitly; no operation is dispatched through the legacy services.
use super::{compat, proto as p};
use crate::remote::proto as old;
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::Status;

/// Leaf values with identical standalone schemas (no resource references).
pub fn value<A: Message, B: Message + Default>(v: A) -> B {
    B::decode(v.encode_to_vec().as_slice()).expect("matching value schemas")
}

#[derive(Clone)]
pub struct Location {
    pub thread: p::ThreadRef,
    pub catalog: old::ChatRef,
    pub workspace: Option<old::WorkspaceRef>,
}
impl Location {
    pub fn draft(&self, draft: old::MessageDraft) -> Result<p::MessageDraft, Status> {
        let context = old::SendContext {
            workspace: self.workspace.clone(),
            ..Default::default()
        };
        compat::draft(draft, &self.catalog, Some(&context)).map_err(Status::internal)
    }
    pub fn legacy_draft(&self, draft: p::MessageDraft) -> Result<old::MessageDraft, Status> {
        let mut attachments = Vec::new();
        for a in draft.attachments {
            use old::attachment::Content as O;
            use p::attachment::Content as N;
            let content = match a
                .content
                .ok_or_else(|| Status::internal("Missing attachment content."))?
            {
                N::Image(v) => O::Image(old::ArtifactRef {
                    host_id: v.host_id,
                    artifact_id: v.artifact_id,
                    media_type: v.media_type,
                    size: v.size,
                    sha256: v.sha256,
                }),
                N::File(v) => O::Path(self.legacy_file(v)?),
                N::Quote(v) => O::Quote(value(v)),
                N::Command(v) => O::Command(value(v)),
                N::Pull(v) => O::Pull(value(v)),
                N::Shell(v) => O::Shell(value(v)),
            };
            attachments.push(old::Attachment {
                label: a.label,
                content: Some(content),
            });
        }
        Ok(old::MessageDraft {
            text: draft.text,
            context: draft.context,
            goal: draft.goal,
            attachments,
        })
    }
    fn file(&self, file: old::FileRef) -> Result<p::FileAttachment, Status> {
        if file.workspace.is_some() && file.workspace != self.workspace {
            return Err(Status::internal("File belongs to another thread project."));
        }
        Ok(p::FileAttachment {
            relative_path: file.relative_path,
            project_file: file.workspace.is_some(),
        })
    }
    fn legacy_file(&self, file: p::FileAttachment) -> Result<old::FileRef, Status> {
        let workspace = if file.project_file {
            Some(self.workspace.clone().ok_or_else(|| {
                Status::failed_precondition("Thread project metadata is unavailable.")
            })?)
        } else {
            None
        };
        Ok(old::FileRef {
            workspace,
            relative_path: file.relative_path,
        })
    }
    pub fn item(&self, item: old::TranscriptItem) -> Result<p::TranscriptItem, Status> {
        use old::transcript_item::Body as O;
        use p::transcript_item::Body as N;
        let body = item
            .body
            .map(|body| {
                Ok::<_, Status>(match body {
                    O::User(v) => N::User(p::UserMessage {
                        message_id: v.message_id,
                        delivery: v.delivery,
                        draft: v.draft.map(|v| self.draft(v)).transpose()?,
                    }),
                    O::Tool(v) => N::Tool(p::ToolCall {
                        call_id: v.call_id,
                        original_name: v.original_name,
                        kind: v.kind,
                        title: v.title,
                        state: v.state,
                        paths: v.paths,
                        output: v.output,
                        background: v.background,
                        activity: v.activity,
                        elapsed_seconds: v.elapsed_seconds,
                        command: v.command,
                        diff: v.diff.into_iter().map(value).collect(),
                        generic_input: v.generic_input,
                        plan: v.plan.into_iter().map(value).collect(),
                        files: v
                            .files
                            .into_iter()
                            .map(|v| self.file(v))
                            .collect::<Result<_, _>>()?,
                        page_item: v.page_item.map(value),
                    }),
                    O::Text(v) => N::Text(value(v)),
                    O::Thinking(v) => N::Thinking(value(v)),
                    O::Decision(v) => N::Decision(value(v)),
                    O::Notice(v) => N::Notice(value(v)),
                    O::TurnEnd(v) => N::TurnEnd(value(v)),
                    O::Shell(v) => N::Shell(value(v)),
                    O::Goal(v) => N::Goal(value(v)),
                })
            })
            .transpose()?;
        Ok(p::TranscriptItem {
            key: item.key.map(value),
            revision: item.revision,
            turn_id: item.turn_id,
            parent_call_id: item.parent_call_id,
            trimmed: item.trimmed,
            body,
        })
    }
    pub fn legacy_item(&self, item: p::TranscriptItem) -> Result<old::TranscriptItem, Status> {
        use old::transcript_item::Body as O;
        use p::transcript_item::Body as N;
        let body = item
            .body
            .map(|body| {
                Ok::<_, Status>(match body {
                    N::User(v) => O::User(old::UserMessage {
                        message_id: v.message_id,
                        delivery: v.delivery,
                        draft: v.draft.map(|v| self.legacy_draft(v)).transpose()?,
                    }),
                    N::Tool(v) => O::Tool(old::ToolCall {
                        call_id: v.call_id,
                        original_name: v.original_name,
                        kind: v.kind,
                        title: v.title,
                        state: v.state,
                        paths: v.paths,
                        output: v.output,
                        background: v.background,
                        activity: v.activity,
                        elapsed_seconds: v.elapsed_seconds,
                        command: v.command,
                        diff: v.diff.into_iter().map(value).collect(),
                        generic_input: v.generic_input,
                        plan: v.plan.into_iter().map(value).collect(),
                        files: v
                            .files
                            .into_iter()
                            .map(|v| self.legacy_file(v))
                            .collect::<Result<_, _>>()?,
                        page_item: v.page_item.map(value),
                    }),
                    N::Text(v) => O::Text(value(v)),
                    N::Thinking(v) => O::Thinking(value(v)),
                    N::Decision(v) => O::Decision(value(v)),
                    N::Notice(v) => O::Notice(value(v)),
                    N::TurnEnd(v) => O::TurnEnd(value(v)),
                    N::Shell(v) => O::Shell(value(v)),
                    N::Goal(v) => O::Goal(value(v)),
                })
            })
            .transpose()?;
        Ok(old::TranscriptItem {
            key: item.key.map(value),
            revision: item.revision,
            turn_id: item.turn_id,
            parent_call_id: item.parent_call_id,
            trimmed: item.trimmed,
            body,
        })
    }
    fn queue(&self, queue: old::ChatQueue) -> Result<p::ThreadQueue, Status> {
        Ok(p::ThreadQueue {
            revision: queue.revision,
            paused: queue.paused,
            entries: queue
                .entries
                .into_iter()
                .map(|v| {
                    Ok(p::QueueEntry {
                        message_id: v.message_id,
                        draft: v.draft.map(|v| self.draft(v)).transpose()?,
                        delivery: v.delivery,
                        resolved_by_user: v.resolved_by_user,
                    })
                })
                .collect::<Result<_, Status>>()?,
        })
    }
    fn legacy_queue(&self, queue: p::ThreadQueue) -> Result<old::ChatQueue, Status> {
        Ok(old::ChatQueue {
            revision: queue.revision,
            paused: queue.paused,
            entries: queue
                .entries
                .into_iter()
                .map(|v| {
                    Ok(old::QueueEntry {
                        message_id: v.message_id,
                        draft: v.draft.map(|v| self.legacy_draft(v)).transpose()?,
                        delivery: v.delivery,
                        resolved_by_user: v.resolved_by_user,
                    })
                })
                .collect::<Result<_, Status>>()?,
        })
    }
    pub fn status(&self, s: old::ChatSummary) -> Result<p::ThreadStatus, Status> {
        Ok(p::ThreadStatus {
            thread: Some(self.thread.clone()),
            title: s.title,
            agent_id: s.agent_id,
            profile_id: s.profile_id,
            state: s.state,
            pending_decisions: s.pending_decisions,
            queued_messages: s.queued_messages,
            run_id: s
                .run_id
                .map(|s| {
                    s.parse()
                        .map_err(|_| Status::internal("Invalid run identity."))
                })
                .transpose()?,
            history_epoch: s.history_epoch,
            goal: s.goal.map(value),
        })
    }
    pub fn legacy_status(&self, s: p::ThreadStatus) -> Result<old::ChatSummary, Status> {
        if s.thread.as_ref() != Some(&self.thread) {
            return Err(Status::internal("Thread status identity differs."));
        }
        Ok(old::ChatSummary {
            chat: Some(self.catalog.clone()),
            workspace: self.workspace.clone(),
            title: s.title,
            agent_id: s.agent_id,
            profile_id: s.profile_id,
            state: s.state,
            pending_decisions: s.pending_decisions,
            queued_messages: s.queued_messages,
            run_id: s.run_id.map(|v| v.to_string()),
            history_epoch: s.history_epoch,
            goal: s.goal.map(value),
        })
    }
    pub fn event(&self, event: old::ChatEvent) -> Result<p::ThreadEvent, Status> {
        use old::chat_event::Event as O;
        use p::thread_event::Event as N;
        let body = match event
            .event
            .ok_or_else(|| Status::internal("Empty thread event."))?
        {
            O::SnapshotStart(v) => N::SnapshotStart(p::SnapshotStart {
                boundary: v.boundary.map(value),
                summary: v.summary.map(|s| self.status(s)).transpose()?,
                queue: v.queue.map(|q| self.queue(q)).transpose()?,
                saved_sequence: v.saved_sequence,
            }),
            O::SnapshotItems(v) => N::SnapshotItems(p::ItemChunk {
                items: v
                    .items
                    .into_iter()
                    .map(|i| self.item(i))
                    .collect::<Result<_, _>>()?,
            }),
            O::ReplaceItem(v) => N::ReplaceItem(self.item(v)?),
            O::QueueChanged(v) => N::QueueChanged(self.queue(v)?),
            O::Status(v) => N::Status(self.status(v)?),
            O::SnapshotEnd(v) => N::SnapshotEnd(value(v)),
            O::AppendText(v) => N::AppendText(value(v)),
            O::Resync(v) => N::Resync(value(v)),
            O::Decision(v) => N::Decision(value(v)),
            O::SavedSequence(v) => N::SavedSequence(v),
            O::ItemFragment(_) => {
                return Err(Status::internal("A fragment must be mapped atomically."));
            }
        };
        Ok(p::ThreadEvent {
            thread: Some(self.thread.clone()),
            cursor: event.cursor.map(value),
            event: Some(body),
        })
    }
    pub fn legacy_event(&self, event: p::ThreadEvent) -> Result<old::ChatEvent, Status> {
        if event.thread.as_ref() != Some(&self.thread) {
            return Err(Status::internal("Thread event identity differs."));
        }
        use old::chat_event::Event as O;
        use p::thread_event::Event as N;
        let body = match event
            .event
            .ok_or_else(|| Status::internal("Empty thread event."))?
        {
            N::SnapshotStart(v) => O::SnapshotStart(old::SnapshotStart {
                boundary: v.boundary.map(value),
                summary: v.summary.map(|s| self.legacy_status(s)).transpose()?,
                queue: v.queue.map(|q| self.legacy_queue(q)).transpose()?,
                saved_sequence: v.saved_sequence,
            }),
            N::SnapshotItems(v) => O::SnapshotItems(old::ItemChunk {
                items: v
                    .items
                    .into_iter()
                    .map(|i| self.legacy_item(i))
                    .collect::<Result<_, _>>()?,
            }),
            N::ReplaceItem(v) => O::ReplaceItem(self.legacy_item(v)?),
            N::QueueChanged(v) => O::QueueChanged(self.legacy_queue(v)?),
            N::Status(v) => O::Status(self.legacy_status(v)?),
            N::SnapshotEnd(v) => O::SnapshotEnd(value(v)),
            N::AppendText(v) => O::AppendText(value(v)),
            N::Resync(v) => O::Resync(value(v)),
            N::Decision(v) => O::Decision(value(v)),
            N::SavedSequence(v) => O::SavedSequence(v),
            N::ItemFragment(_) => {
                return Err(Status::internal("A fragment must be mapped atomically."));
            }
        };
        Ok(old::ChatEvent {
            chat: Some(self.catalog.clone()),
            cursor: event.cursor.map(value),
            event: Some(body),
        })
    }
}

/// Bounded, non-interleaved conversion of one fragmented item. Original frame
/// cursors are retained so conversion cannot create a live sequence gap.
#[derive(Default)]
pub struct Fragments<F> {
    identity: Option<old::ItemFragment>,
    bytes: Vec<u8>,
    frames: Vec<F>,
}
type CompletedFragments<F> = Option<(Vec<F>, Vec<u8>)>;
impl<F> Fragments<F> {
    pub fn new() -> Self {
        Self {
            identity: None,
            bytes: vec![],
            frames: vec![],
        }
    }
    pub fn push(
        &mut self,
        fragment: old::ItemFragment,
        frame: F,
    ) -> Result<CompletedFragments<F>, Status> {
        if fragment.total_bytes == 0
            || fragment.total_bytes > 32 * 1024 * 1024
            || fragment.data.is_empty()
            || fragment.data.len() > 64 * 1024
            || fragment.sha256.len() != 32
            || fragment.offset != self.bytes.len() as u64
            || fragment.offset.saturating_add(fragment.data.len() as u64) > fragment.total_bytes
            || self.frames.len() >= 512
        {
            return Err(Status::data_loss("Invalid item fragment."));
        }
        if let Some(old) = &self.identity {
            if old.transfer_id != fragment.transfer_id
                || old.key != fragment.key
                || old.revision != fragment.revision
                || old.total_bytes != fragment.total_bytes
                || old.sha256 != fragment.sha256
            {
                return Err(Status::data_loss("Item fragment identity changed."));
            }
        } else {
            // Keep assembly within its declared reservation. Geometric growth
            // could otherwise retain almost twice the item's bytes while decoding.
            self.bytes.reserve_exact(fragment.total_bytes as usize);
            let mut identity = fragment.clone();
            identity.data.clear();
            self.identity = Some(identity);
        }
        self.bytes.extend_from_slice(&fragment.data);
        self.frames.push(frame);
        if self.bytes.len() as u64 != fragment.total_bytes {
            return Ok(None);
        }
        if Sha256::digest(&self.bytes).as_slice() != fragment.sha256 {
            return Err(Status::data_loss("Item fragment digest differs."));
        }
        self.identity = None;
        Ok(Some((
            std::mem::take(&mut self.frames),
            std::mem::take(&mut self.bytes),
        )))
    }
    pub fn pending(&self) -> bool {
        self.identity.is_some()
    }
}

pub fn reframe(
    bytes: Vec<u8>,
    count: usize,
    template: old::ItemFragment,
) -> Result<Vec<old::ItemFragment>, Status> {
    if count == 0
        || bytes.len() < count
        || bytes.len() > count.saturating_mul(64 * 1024)
        || bytes.len() > 32 * 1024 * 1024
    {
        return Err(Status::resource_exhausted(
            "Converted item exceeds its fragment budget.",
        ));
    }
    let hash = Sha256::digest(&bytes).to_vec();
    let mut offset = 0;
    let total = bytes.len();
    let mut result = Vec::with_capacity(count);
    for remaining in (1..=count).rev() {
        let end = offset + (total - offset).div_ceil(remaining);
        result.push(old::ItemFragment {
            offset: offset as u64,
            total_bytes: total as u64,
            sha256: hash.clone(),
            data: bytes[offset..end].to_vec(),
            ..template.clone()
        });
        offset = end;
    }
    Ok(result)
}

pub fn thread_stream(
    location: Location,
    stream: crate::remote::host::RpcStream<old::ChatEvent>,
    budget: crate::remote::sync::ReplayHub,
) -> crate::remote::host::RpcStream<p::ThreadEvent> {
    use futures_lite::StreamExt;
    let state = (
        location,
        stream,
        Fragments::<old::ChatEvent>::new(),
        std::collections::VecDeque::new(),
        None,
    );
    Box::pin(futures_lite::stream::try_unfold(
        state,
        move |(location, mut stream, mut fragments, mut pending, mut reservation)| {
            let budget = budget.clone();
            async move {
                loop {
                    if let Some(event) = pending.pop_front() {
                        return Ok(Some((
                            event,
                            (location, stream, fragments, pending, reservation),
                        )));
                    }
                    if !fragments.pending() {
                        reservation = None;
                    }
                    let Some(event) = stream.next().await.transpose()? else {
                        if fragments.pending() {
                            return Err(Status::data_loss("Thread stream ended inside an item."));
                        }
                        return Ok(None);
                    };
                    let Some(old::chat_event::Event::ItemFragment(fragment)) = event.event.as_ref()
                    else {
                        if fragments.pending() {
                            return Err(Status::data_loss("Interleaved item fragments."));
                        }
                        let mapped = location.event(event)?;
                        return Ok(Some((
                            mapped,
                            (location, stream, fragments, pending, reservation),
                        )));
                    };
                    if !fragments.pending() {
                        reservation = Some(
                            budget
                                .reserve_conversion(fragment.total_bytes as usize * 2 + 512 * 1024)
                                .map_err(|_| {
                                    Status::resource_exhausted(
                                        "Thread conversion exceeds the assembly budget.",
                                    )
                                })?,
                        );
                    }
                    let template = fragment.clone();
                    let mut frame = event.clone();
                    if let Some(old::chat_event::Event::ItemFragment(f)) = &mut frame.event {
                        f.data.clear();
                    }
                    if let Some((frames, bytes)) = fragments.push(template.clone(), frame)? {
                        let item = old::TranscriptItem::decode(bytes.as_slice())
                            .map_err(|_| Status::data_loss("Invalid fragmented item."))?;
                        drop(bytes);
                        let mapped = location.item(item)?.encode_to_vec();
                        let converted = reframe(mapped, frames.len(), template)?;
                        for (frame, fragment) in frames.into_iter().zip(converted) {
                            pending.push_back(p::ThreadEvent {
                                thread: Some(location.thread.clone()),
                                cursor: frame.cursor.map(value),
                                event: Some(p::thread_event::Event::ItemFragment(value(fragment))),
                            });
                        }
                    }
                }
            }
        },
    ))
}

pub fn legacy_thread_stream(
    location: Location,
    stream: crate::remote::host::RpcStream<p::ThreadEvent>,
) -> crate::remote::host::RpcStream<old::ChatEvent> {
    use futures_lite::StreamExt;
    let state = (
        location,
        stream,
        Fragments::<p::ThreadEvent>::new(),
        std::collections::VecDeque::new(),
    );
    Box::pin(futures_lite::stream::try_unfold(
        state,
        |(location, mut stream, mut fragments, mut pending)| async move {
            loop {
                if let Some(event) = pending.pop_front() {
                    return Ok(Some((event, (location, stream, fragments, pending))));
                }
                let Some(event) = stream.next().await.transpose()? else {
                    if fragments.pending() {
                        return Err(Status::data_loss("Thread stream ended inside an item."));
                    }
                    return Ok(None);
                };
                if event.thread.as_ref() != Some(&location.thread) {
                    return Err(Status::data_loss("Thread event identity differs."));
                }
                let Some(p::thread_event::Event::ItemFragment(fragment)) = event.event.as_ref()
                else {
                    if fragments.pending() {
                        return Err(Status::data_loss("Interleaved item fragments."));
                    }
                    let mapped = location.legacy_event(event)?;
                    return Ok(Some((mapped, (location, stream, fragments, pending))));
                };
                let template: old::ItemFragment = value(fragment.clone());
                let mut frame = event.clone();
                if let Some(p::thread_event::Event::ItemFragment(f)) = &mut frame.event {
                    f.data.clear();
                }
                if let Some((frames, bytes)) = fragments.push(template.clone(), frame)? {
                    let item = p::TranscriptItem::decode(bytes.as_slice())
                        .map_err(|_| Status::data_loss("Invalid fragmented item."))?;
                    drop(bytes);
                    let mapped = location.legacy_item(item)?.encode_to_vec();
                    let converted = reframe(mapped, frames.len(), template)?;
                    for (frame, fragment) in frames.into_iter().zip(converted) {
                        pending.push_back(old::ChatEvent {
                            chat: Some(location.catalog.clone()),
                            cursor: frame.cursor.map(value),
                            event: Some(old::chat_event::Event::ItemFragment(fragment)),
                        });
                    }
                }
            }
        },
    ))
}

pub fn history_stream(
    location: Location,
    stream: crate::remote::host::RpcStream<old::HistoryPageEvent>,
    budget: crate::remote::sync::ReplayHub,
) -> crate::remote::host::RpcStream<p::HistoryPageEvent> {
    use futures_lite::StreamExt;
    let state = (
        location,
        stream,
        Fragments::<old::HistoryPageEvent>::new(),
        std::collections::VecDeque::new(),
        None,
    );
    Box::pin(futures_lite::stream::try_unfold(
        state,
        move |(location, mut stream, mut fragments, mut pending, mut reservation)| {
            let budget = budget.clone();
            async move {
                loop {
                    if let Some(event) = pending.pop_front() {
                        return Ok(Some((
                            event,
                            (location, stream, fragments, pending, reservation),
                        )));
                    }
                    if !fragments.pending() {
                        reservation = None;
                    }
                    let Some(event) = stream.next().await.transpose()? else {
                        if fragments.pending() {
                            return Err(Status::data_loss("History ended inside an item."));
                        }
                        return Ok(None);
                    };
                    let part = match event
                        .part
                        .clone()
                        .ok_or_else(|| Status::data_loss("Missing history part."))?
                    {
                        old::history_page_event::Part::Fragment(fragment) => {
                            if !fragments.pending() {
                                reservation = Some(
                                    budget
                                        .reserve_conversion(
                                            fragment.total_bytes as usize * 2 + 512 * 1024,
                                        )
                                        .map_err(|_| {
                                            Status::resource_exhausted(
                                                "History conversion exceeds the assembly budget.",
                                            )
                                        })?,
                                );
                            }
                            let mut frame = event;
                            if let Some(old::history_page_event::Part::Fragment(f)) =
                                &mut frame.part
                            {
                                f.data.clear();
                            }
                            if let Some((frames, bytes)) =
                                fragments.push(fragment.clone(), frame)?
                            {
                                let item = old::TranscriptItem::decode(bytes.as_slice())
                                    .map_err(|_| Status::data_loss("Invalid history item."))?;
                                drop(bytes);
                                let converted = reframe(
                                    location.item(item)?.encode_to_vec(),
                                    frames.len(),
                                    fragment,
                                )?;
                                for fragment in converted {
                                    pending.push_back(p::HistoryPageEvent {
                                        thread: Some(location.thread.clone()),
                                        part: Some(p::history_page_event::Part::Fragment(value(
                                            fragment,
                                        ))),
                                    });
                                }
                            }
                            continue;
                        }
                        old::history_page_event::Part::Start(v) => {
                            p::history_page_event::Part::Start(value(v))
                        }
                        old::history_page_event::Part::Items(v) => {
                            p::history_page_event::Part::Items(p::ItemChunk {
                                items: v
                                    .items
                                    .into_iter()
                                    .map(|i| location.item(i))
                                    .collect::<Result<_, _>>()?,
                            })
                        }
                        old::history_page_event::Part::EndReadId(v) => {
                            p::history_page_event::Part::EndReadId(v)
                        }
                    };
                    if fragments.pending() {
                        return Err(Status::data_loss("Interleaved history fragments."));
                    }
                    return Ok(Some((
                        p::HistoryPageEvent {
                            thread: Some(location.thread.clone()),
                            part: Some(part),
                        },
                        (location, stream, fragments, pending, reservation),
                    )));
                }
            }
        },
    ))
}

pub fn legacy_history_stream(
    location: Location,
    stream: crate::remote::host::RpcStream<p::HistoryPageEvent>,
) -> crate::remote::host::RpcStream<old::HistoryPageEvent> {
    use futures_lite::StreamExt;
    let state = (
        location,
        stream,
        Fragments::<p::HistoryPageEvent>::new(),
        std::collections::VecDeque::new(),
    );
    Box::pin(futures_lite::stream::try_unfold(
        state,
        |(location, mut stream, mut fragments, mut pending)| async move {
            loop {
                if let Some(event) = pending.pop_front() {
                    return Ok(Some((event, (location, stream, fragments, pending))));
                }
                let Some(event) = stream.next().await.transpose()? else {
                    if fragments.pending() {
                        return Err(Status::data_loss("History ended inside an item."));
                    }
                    return Ok(None);
                };
                if event.thread.as_ref() != Some(&location.thread) {
                    return Err(Status::data_loss("History thread identity differs."));
                }
                let part = match event
                    .part
                    .clone()
                    .ok_or_else(|| Status::data_loss("Missing history part."))?
                {
                    p::history_page_event::Part::Fragment(fragment) => {
                        let mut frame = event;
                        if let Some(p::history_page_event::Part::Fragment(f)) = &mut frame.part {
                            f.data.clear();
                        }
                        let template: old::ItemFragment = value(fragment);
                        if let Some((frames, bytes)) = fragments.push(template.clone(), frame)? {
                            let item = p::TranscriptItem::decode(bytes.as_slice())
                                .map_err(|_| Status::data_loss("Invalid history item."))?;
                            drop(bytes);
                            let converted = reframe(
                                location.legacy_item(item)?.encode_to_vec(),
                                frames.len(),
                                template,
                            )?;
                            for fragment in converted {
                                pending.push_back(old::HistoryPageEvent {
                                    chat: Some(location.catalog.clone()),
                                    part: Some(old::history_page_event::Part::Fragment(fragment)),
                                });
                            }
                        }
                        continue;
                    }
                    p::history_page_event::Part::Start(v) => {
                        old::history_page_event::Part::Start(value(v))
                    }
                    p::history_page_event::Part::Items(v) => {
                        old::history_page_event::Part::Items(old::ItemChunk {
                            items: v
                                .items
                                .into_iter()
                                .map(|i| location.legacy_item(i))
                                .collect::<Result<_, _>>()?,
                        })
                    }
                    p::history_page_event::Part::EndReadId(v) => {
                        old::history_page_event::Part::EndReadId(v)
                    }
                };
                if fragments.pending() {
                    return Err(Status::data_loss("Interleaved history fragments."));
                }
                return Ok(Some((
                    old::HistoryPageEvent {
                        chat: Some(location.catalog.clone()),
                        part: Some(part),
                    },
                    (location, stream, fragments, pending),
                )));
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::StreamExt;

    fn location() -> Location {
        Location {
            thread: p::ThreadRef {
                host_id: "host".into(),
                thread_id: crate::thread::ThreadId::new().to_string(),
            },
            catalog: old::ChatRef {
                host_id: "host".into(),
                chat_id: "catalog".into(),
            },
            workspace: Some(old::WorkspaceRef {
                host_id: "host".into(),
                workspace_id: "checkout".into(),
            }),
        }
    }
    fn events(
        location: &Location,
        item: &old::TranscriptItem,
    ) -> Vec<Result<old::ChatEvent, Status>> {
        crate::remote::transcript::fragments(item)
            .unwrap()
            .into_iter()
            .enumerate()
            .map(|(i, fragment)| {
                Ok(old::ChatEvent {
                    chat: Some(location.catalog.clone()),
                    cursor: Some(old::Cursor {
                        runtime_epoch: "epoch".into(),
                        stream_id: "stream".into(),
                        sequence: i as u64 + 1,
                    }),
                    event: Some(old::chat_event::Event::ItemFragment(fragment)),
                })
            })
            .collect()
    }
    #[tokio::test]
    async fn fragmented_thread_preserves_large_items_attachments_and_live_cursors() {
        let location = location();
        let original = old::TranscriptItem {
            key: Some(old::ItemKey {
                history_epoch: 4,
                item_id: 7,
            }),
            revision: 9,
            body: Some(old::transcript_item::Body::User(old::UserMessage {
                message_id: "message".into(),
                delivery: "delivered".into(),
                draft: Some(old::MessageDraft {
                    // More than half the old item limit catches accidental 16 MiB
                    // rejection when conversion admits both decode and encode space.
                    text: "x".repeat(17 * 1024 * 1024),
                    attachments: vec![
                        old::Attachment {
                            label: "file".into(),
                            content: Some(old::attachment::Content::Path(old::FileRef {
                                workspace: location.workspace.clone(),
                                relative_path: "src/main.rs".into(),
                            })),
                        },
                        old::Attachment {
                            label: "image".into(),
                            content: Some(old::attachment::Content::Image(old::ArtifactRef {
                                host_id: "host".into(),
                                artifact_id: "image".into(),
                                media_type: "image/png".into(),
                                size: 123,
                                sha256: vec![7; 32],
                            })),
                        },
                    ],
                    ..Default::default()
                }),
            })),
            ..Default::default()
        };
        let frames = events(&location, &original);
        let count = frames.len();
        let hub = crate::remote::sync::ReplayHub::new(Default::default());
        let mapped = thread_stream(
            location.clone(),
            Box::pin(futures_lite::stream::iter(frames)),
            hub.clone(),
        );
        let mut restored = legacy_thread_stream(location, mapped);
        let mut assembly = Fragments::new();
        let mut completed = None;
        let mut sequence = 0;
        while let Some(event) = restored.next().await {
            let event = event.unwrap();
            sequence += 1;
            assert_eq!(event.cursor.unwrap().sequence, sequence);
            let Some(old::chat_event::Event::ItemFragment(fragment)) = event.event else {
                panic!("fragment expected")
            };
            assert!(fragment.data.len() <= 64 * 1024);
            if let Some((_, bytes)) = assembly.push(fragment, ()).unwrap() {
                completed = Some(old::TranscriptItem::decode(bytes.as_slice()).unwrap());
            }
        }
        assert_eq!(sequence as usize, count);
        assert_eq!(completed.as_ref(), Some(&original));
        assert!(
            hub.reserve_conversion(2 * crate::remote::transcript::MAX_ITEM_BYTES + 512 * 1024)
                .is_ok()
        );
    }
    #[tokio::test]
    async fn cancelling_an_incomplete_conversion_releases_its_workspace() {
        let location = location();
        let item = old::TranscriptItem {
            body: Some(old::transcript_item::Body::Text(old::TextBlock {
                text: "x".repeat(128 * 1024),
                streaming: false,
            })),
            ..Default::default()
        };
        let first = events(&location, &item).remove(0);
        let source = futures_lite::stream::iter(vec![first]).chain(futures_lite::stream::pending());
        let hub = crate::remote::sync::ReplayHub::new(Default::default());
        let mut stream = thread_stream(location, Box::pin(source), hub.clone());
        assert!(
            futures_lite::future::poll_once(stream.next())
                .await
                .is_none()
        );
        let whole = 2 * crate::remote::transcript::MAX_ITEM_BYTES + 512 * 1024;
        assert!(hub.reserve_conversion(whole).is_err());
        drop(stream);
        assert!(hub.reserve_conversion(whole).is_ok());
    }
}
