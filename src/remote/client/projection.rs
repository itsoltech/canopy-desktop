use crate::api::proto as p;
use prost::Message;
use sha2::{Digest, Sha256};

#[derive(Clone, Default)]
pub struct ChatProjection {
    pub items: Vec<p::TranscriptItem>,
    pub summary: Option<p::ThreadStatus>,
    pub queue: Option<p::ThreadQueue>,
    pub cursor: Option<p::Cursor>,
    pub has_earlier: bool,
    staging: Option<Box<ChatProjection>>,
    fragment: Option<Fragment>,
    seen: Option<p::Cursor>,
}
#[derive(Clone)]
struct Fragment {
    id: String,
    key: Option<p::ItemKey>,
    revision: u64,
    total: usize,
    hash: Vec<u8>,
    bytes: Vec<u8>,
}
impl ChatProjection {
    pub(super) fn from_cache(cached: &super::store::CachedChat) -> Self {
        Self {
            items: cached
                .items
                .iter()
                .filter_map(|b| p::TranscriptItem::decode(b.as_slice()).ok())
                .collect(),
            summary: cached
                .summary
                .as_ref()
                .and_then(|b| p::ThreadStatus::decode(b.as_slice()).ok()),
            queue: cached
                .queue
                .as_ref()
                .and_then(|b| p::ThreadQueue::decode(b.as_slice()).ok()),
            ..Default::default()
        }
    }

    pub fn apply(&mut self, chat: &p::ThreadRef, event: p::ThreadEvent) -> Result<bool, String> {
        if event.thread.as_ref() != Some(chat) {
            return Err("Chat event belongs to another resource.".into());
        }
        use p::thread_event::Event;
        if let Some(Event::SnapshotStart(start)) = event.event {
            if self.staging.is_some() {
                return Err("Nested chat snapshot.".into());
            }
            self.staging = Some(Box::new(Self {
                summary: start.summary,
                queue: start.queue,
                cursor: start.boundary,
                ..Default::default()
            }));
            self.fragment = None;
            self.seen = None;
            return Ok(false);
        }
        if let Some(Event::SnapshotEnd(end)) = event.event {
            if self.fragment.is_some() {
                return Err("Incomplete snapshot item.".into());
            }
            let stage = self.staging.take().ok_or("Snapshot end without start.")?;
            if stage.cursor != end.boundary {
                return Err("Snapshot boundary changed.".into());
            }
            *self = *stage;
            self.has_earlier = end.has_earlier_history;
            self.seen = self.cursor.clone();
            return Ok(true);
        }
        if self.staging.is_none() {
            let cursor = event.cursor.as_ref().ok_or("Missing live chat cursor.")?;
            let old = self
                .seen
                .as_ref()
                .or(self.cursor.as_ref())
                .ok_or("Live change before snapshot.")?;
            if old.runtime_epoch != cursor.runtime_epoch || old.stream_id != cursor.stream_id {
                return Err("Chat stream epoch changed.".into());
            }
            if cursor.sequence <= old.sequence {
                return Ok(false);
            }
            if cursor.sequence != old.sequence + 1 {
                return Err("Chat stream sequence gap.".into());
            }
            self.seen = Some(cursor.clone());
        }
        let body = event.event.ok_or("Empty chat event.")?;
        if self.fragment.is_some() && !matches!(body, Event::ItemFragment(_)) {
            return Err("Interleaved fragmented item.".into());
        }
        let body = if let Event::ItemFragment(part) = body {
            if part.total_bytes > 32 * 1024 * 1024
                || part.data.len() > 65536
                || part.total_bytes == 0
                || part.sha256.len() != 32
            {
                return Err("Invalid chat fragment size.".into());
            }
            if self.fragment.is_none() {
                self.fragment = Some(Fragment {
                    id: part.transfer_id.clone(),
                    key: part.key,
                    revision: part.revision,
                    total: part.total_bytes as usize,
                    hash: part.sha256.clone(),
                    bytes: vec![],
                });
            }
            let fragment = self.fragment.as_mut().unwrap();
            if fragment.id != part.transfer_id
                || fragment.key != part.key
                || fragment.revision != part.revision
                || fragment.total != part.total_bytes as usize
                || fragment.hash != part.sha256
                || fragment.bytes.len() != part.offset as usize
                || fragment.bytes.len() + part.data.len() > fragment.total
            {
                return Err("Chat fragment identity or offset mismatch.".into());
            }
            fragment.bytes.extend(part.data);
            if fragment.bytes.len() != fragment.total {
                return Ok(false);
            }
            let fragment = self.fragment.take().unwrap();
            if Sha256::digest(&fragment.bytes).as_slice() != fragment.hash {
                return Err("Chat fragment checksum mismatch.".into());
            }
            let item = p::TranscriptItem::decode(fragment.bytes.as_slice())
                .map_err(|_| "Invalid encoded chat item.")?;
            if item.key != fragment.key || item.revision != fragment.revision {
                return Err("Fragment item identity mismatch.".into());
            }
            if self.staging.is_some() {
                Event::SnapshotItems(p::ItemChunk { items: vec![item] })
            } else {
                Event::ReplaceItem(item)
            }
        } else {
            body
        };
        let snapshot = self.staging.is_some();
        self.reduce(body, snapshot)?;
        if snapshot {
            return Ok(false);
        }
        self.cursor = event.cursor;
        Ok(true)
    }
    fn reduce(&mut self, body: p::thread_event::Event, snapshot: bool) -> Result<(), String> {
        use p::thread_event::Event;
        let target = if snapshot {
            self.staging.as_deref_mut().unwrap()
        } else {
            self
        };
        match body {
            Event::SnapshotItems(chunk) if snapshot => {
                for item in chunk.items {
                    if item.key.is_none() || target.items.iter().any(|i| i.key == item.key) {
                        return Err("Duplicate or missing snapshot item identity.".into());
                    }
                    target.items.push(item);
                }
            }
            Event::ReplaceItem(item) => {
                if item.key.is_none() {
                    return Err("Missing chat item identity.".into());
                }
                if let Some(old) = target.items.iter_mut().find(|i| i.key == item.key) {
                    if old.revision >= item.revision {
                        return Err("Invalid item revision.".into());
                    }
                    *old = item;
                } else {
                    target.items.push(item);
                }
            }
            Event::AppendText(patch) => {
                let item = target
                    .items
                    .iter_mut()
                    .find(|i| i.key == patch.key)
                    .ok_or("Text append before item.")?;
                if item.trimmed
                    || item.revision != patch.expected_revision
                    || patch.revision <= item.revision
                {
                    return Err("Text revision conflict.".into());
                }
                let text = match item.body.as_mut() {
                    Some(
                        p::transcript_item::Body::Text(text)
                        | p::transcript_item::Body::Thinking(text),
                    ) => text,
                    _ => return Err("Text append on non-text item.".into()),
                };
                if text.text.len() as u64 != patch.utf8_offset {
                    return Err("Text UTF-8 offset mismatch.".into());
                }
                text.text.push_str(&patch.text);
                text.streaming = patch.streaming;
                item.revision = patch.revision;
            }
            Event::QueueChanged(queue) => target.queue = Some(queue),
            Event::Status(summary) => target.summary = Some(summary),
            Event::Decision(decision) => {
                for item in &mut target.items {
                    if let Some(p::transcript_item::Body::Decision(old)) = item.body.as_mut()
                        && old.decision_id == decision.decision_id
                        && old.run_id == decision.run_id
                    {
                        *old = decision.clone();
                    }
                }
            }
            Event::SavedSequence(_) => {}
            Event::Resync(reason) => return Err(reason.reason),
            _ => return Err("Unexpected chat event.".into()),
        }
        if target.items.len() > 2000
            || target.items.iter().map(Message::encoded_len).sum::<usize>() > 64 * 1024 * 1024
        {
            return Err("Chat projection limit exceeded; reload a bounded history page.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chat() -> p::ThreadRef {
        p::ThreadRef {
            host_id: "host-a".into(),
            thread_id: "chat".into(),
        }
    }
    fn cursor(sequence: u64) -> p::Cursor {
        p::Cursor {
            runtime_epoch: "run".into(),
            stream_id: "chat".into(),
            sequence,
        }
    }
    fn event(body: p::thread_event::Event, sequence: Option<u64>) -> p::ThreadEvent {
        p::ThreadEvent {
            thread: Some(chat()),
            cursor: sequence.map(cursor),
            event: Some(body),
        }
    }
    fn item() -> p::TranscriptItem {
        p::TranscriptItem {
            key: Some(p::ItemKey {
                history_epoch: 1,
                item_id: 1,
            }),
            revision: 1,
            body: Some(p::transcript_item::Body::Text(p::TextBlock {
                text: "ż".into(),
                streaming: true,
            })),
            ..Default::default()
        }
    }
    fn start(projection: &mut ChatProjection) {
        projection
            .apply(
                &chat(),
                event(
                    p::thread_event::Event::SnapshotStart(p::SnapshotStart {
                        boundary: Some(cursor(10)),
                        ..Default::default()
                    }),
                    None,
                ),
            )
            .unwrap();
    }
    fn end(projection: &mut ChatProjection) {
        projection
            .apply(
                &chat(),
                event(
                    p::thread_event::Event::SnapshotEnd(p::SnapshotEnd {
                        boundary: Some(cursor(10)),
                        has_earlier_history: false,
                    }),
                    None,
                ),
            )
            .unwrap();
    }
    #[test]
    fn incomplete_or_corrupt_fragment_never_replaces_visible_snapshot() {
        let mut projection = ChatProjection::default();
        start(&mut projection);
        let bytes = item().encode_to_vec();
        let hash = Sha256::digest(&bytes).to_vec();
        let midpoint = bytes.len() / 2;
        let part = p::ItemFragment {
            key: item().key,
            revision: 1,
            transfer_id: "transfer".into(),
            offset: 0,
            total_bytes: bytes.len() as u64,
            sha256: hash.clone(),
            data: bytes[..midpoint].to_vec(),
        };
        assert!(
            !projection
                .apply(
                    &chat(),
                    event(p::thread_event::Event::ItemFragment(part.clone()), None)
                )
                .unwrap()
        );
        assert!(projection.items.is_empty());
        let mut last = part;
        last.offset = midpoint as u64;
        last.data = bytes[midpoint..].to_vec();
        last.data[0] ^= 1;
        assert!(
            projection
                .apply(
                    &chat(),
                    event(p::thread_event::Event::ItemFragment(last), None)
                )
                .is_err()
        );
        assert!(projection.items.is_empty());
    }
    #[test]
    fn utf8_offsets_and_host_scope_are_checked_before_publishing() {
        let mut projection = ChatProjection::default();
        start(&mut projection);
        projection
            .apply(
                &chat(),
                event(
                    p::thread_event::Event::SnapshotItems(p::ItemChunk {
                        items: vec![item()],
                    }),
                    None,
                ),
            )
            .unwrap();
        end(&mut projection);
        let append = p::TextAppend {
            key: item().key,
            expected_revision: 1,
            revision: 2,
            utf8_offset: 1,
            text: "x".into(),
            streaming: false,
        };
        assert!(
            projection
                .apply(
                    &chat(),
                    event(p::thread_event::Event::AppendText(append), Some(11))
                )
                .is_err()
        );
        assert_eq!(projection.items[0], item());
        assert_eq!(projection.cursor, Some(cursor(10)));
        let mut wrong = event(
            p::thread_event::Event::QueueChanged(Default::default()),
            Some(12),
        );
        wrong.thread.as_mut().unwrap().host_id = "host-b".into();
        assert!(projection.apply(&chat(), wrong).is_err());
    }
    #[test]
    fn sequence_gap_and_trimmed_append_require_resync() {
        let mut projection = ChatProjection::default();
        start(&mut projection);
        let mut trimmed = item();
        trimmed.trimmed = true;
        projection
            .apply(
                &chat(),
                event(
                    p::thread_event::Event::SnapshotItems(p::ItemChunk {
                        items: vec![trimmed],
                    }),
                    None,
                ),
            )
            .unwrap();
        end(&mut projection);
        let append = p::TextAppend {
            key: item().key,
            expected_revision: 1,
            revision: 2,
            utf8_offset: 2,
            text: "x".into(),
            streaming: false,
        };
        assert!(
            projection
                .clone()
                .apply(
                    &chat(),
                    event(p::thread_event::Event::AppendText(append.clone()), Some(12))
                )
                .is_err()
        );
        assert!(
            projection
                .apply(
                    &chat(),
                    event(p::thread_event::Event::AppendText(append), Some(11))
                )
                .is_err()
        );
    }
}
