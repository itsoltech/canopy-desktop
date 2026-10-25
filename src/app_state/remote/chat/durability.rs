//! Which part of a chat stream is saved (SYNC-06/07). Every published frame has a
//! stream sequence; a transcript frame is saved once a history commit proves the
//! stored JSON of exactly that item version (the SHA-256 of `history::item_json`).
//! An older frame of the same item stays unsaved until a newer version's proof
//! commits (superseding alone stores nothing). `saved` is the highest sequence
//! with every frame up to it saved. Nothing is assumed: without proofs (history
//! off, a failing disk) the watermark stays where it is.
use canopy_desktop::chat::{history, transcript as t};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

/// Unsaved frames kept; beyond it the watermark stops for this projection (the
/// client still sees live data, only `saved` no longer advances until a resync).
const FRAMES: usize = 4096;

pub(super) type Key = (u64, u64);

struct Frame {
    sequence: u64,
    /// Its item; items held before the first frame all have sequence 0.
    key: Option<Key>,
    /// The version this frame carried; dropped once resolved or superseded.
    item: Option<Arc<t::Item>>,
    done: bool,
}

#[derive(Default)]
pub(super) struct Durability {
    frames: VecDeque<Frame>,
    /// The unresolved frame of each item, by sequence.
    latest: HashMap<Key, u64>,
    saved: u64,
    overflowed: bool,
}

/// The stored JSON's hash of `item`, as the history writer computes it.
pub(super) fn stored_hash(item: &t::Item) -> Option<[u8; 32]> {
    let json = history::item_json(item)?;
    Some(Sha256::digest(json.as_bytes()).into())
}

impl Durability {
    pub(super) fn saved(&self) -> u64 {
        self.saved
    }
    /// Whether every frame published so far is saved.
    pub(super) fn settled(&self) -> bool {
        !self.overflowed && self.frames.is_empty()
    }
    /// A transcript item version, unsaved until proven. Sequence 0 marks items held
    /// before the stream's first frame.
    pub(super) fn item(&mut self, sequence: u64, key: Key, item: Arc<t::Item>) {
        if self.overflowed {
            return;
        }
        // The older version can no longer be proven by itself; it stays unsaved
        // until this newer one is.
        if let Some(previous) = self.latest.insert(key, sequence)
            && let Some(frame) = self.frame(key, previous)
        {
            frame.item = None;
        }
        self.push(Frame {
            sequence,
            key: Some(key),
            item: Some(item),
            done: false,
        });
    }
    /// A frame with nothing to save (status, queue, decision, a saved flag).
    pub(super) fn other(&mut self, sequence: u64) {
        if self.overflowed {
            return;
        }
        self.push(Frame {
            sequence,
            key: None,
            item: None,
            done: true,
        });
        self.advance();
    }
    fn frame(&mut self, key: Key, sequence: u64) -> Option<&mut Frame> {
        self.frames
            .iter_mut()
            .find(|f| f.key == Some(key) && f.sequence == sequence)
    }
    fn push(&mut self, frame: Frame) {
        self.frames.push_back(frame);
        if self.frames.len() > FRAMES {
            self.advance();
            if self.frames.len() > FRAMES {
                self.overflowed = true;
                self.frames.clear();
                self.latest.clear();
            }
        }
    }
    /// Applies a commit proof. Returns the keys whose current version it proved.
    pub(super) fn commit(&mut self, proof: &[(Key, [u8; 32])]) -> Vec<Key> {
        let mut proved = Vec::new();
        for (key, hash) in proof {
            let Some(sequence) = self.latest.get(key).copied() else {
                continue;
            };
            let matches = self
                .frame(*key, sequence)
                .and_then(|frame| frame.item.as_deref())
                .and_then(stored_hash)
                .is_some_and(|stored| stored == *hash);
            if matches {
                // The latest version is stored: it and every older frame of the
                // item are saved.
                for frame in self
                    .frames
                    .iter_mut()
                    .filter(|f| f.key == Some(*key) && f.sequence <= sequence)
                {
                    frame.done = true;
                    frame.item = None;
                }
                self.latest.remove(key);
                proved.push(*key);
            }
        }
        self.advance();
        proved
    }
    fn advance(&mut self) {
        while self.frames.front().is_some_and(|f| f.done) {
            let frame = self.frames.pop_front().expect("front exists");
            self.saved = self.saved.max(frame.sequence);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(id: u64, body: &str) -> Arc<t::Item> {
        Arc::new(t::Item {
            id: t::ItemId(id),
            turn: t::TurnId(1),
            parent: None,
            body: t::Body::Text(t::TextBlock {
                text: body.into(),
                streaming: false,
            }),
            trimmed: false,
        })
    }
    fn proof(epoch: u64, item: &t::Item) -> ((u64, u64), [u8; 32]) {
        ((epoch, item.id.0), stored_hash(item).unwrap())
    }

    #[test]
    fn the_watermark_moves_only_past_proven_versions_in_stream_order() {
        let mut d = Durability::default();
        let (a1, a2, b) = (text(1, "par"), text(1, "partial"), text(2, "next"));
        d.item(1, (0, 1), a1.clone());
        d.other(2);
        d.item(3, (0, 1), a2.clone());
        d.item(4, (0, 2), b.clone());
        // A commit of the older version proves nothing about the newer one, and the
        // superseded frame is not saved by being superseded.
        assert!(d.commit(&[proof(0, &a1)]).is_empty());
        assert_eq!(d.saved(), 0);
        // A later item saved first cannot move the watermark past an unsaved one.
        assert_eq!(d.commit(&[proof(0, &b)]), vec![(0, 2)]);
        assert_eq!(d.saved(), 0);
        assert_eq!(d.commit(&[proof(0, &a2)]), vec![(0, 1)]);
        assert_eq!(d.saved(), 4);
        assert!(d.settled());
    }

    #[test]
    fn nothing_counts_as_saved_before_any_proof() {
        let mut d = Durability::default();
        d.item(1, (0, 1), text(1, "a"));
        d.item(2, (0, 1), text(1, "ab"));
        d.item(3, (0, 2), text(2, "b"));
        d.other(4);
        assert_eq!(d.saved(), 0, "the database holds nothing yet");
        assert!(!d.settled());
    }

    #[test]
    fn items_held_before_the_first_frame_are_proven_by_their_own_key() {
        let mut d = Durability::default();
        let (a, b) = (text(1, "a"), text(2, "b"));
        d.item(0, (0, 1), a.clone());
        d.item(0, (0, 2), b.clone());
        d.other(1);
        assert_eq!(d.commit(&[proof(0, &b)]), vec![(0, 2)]);
        assert_eq!(d.saved(), 0, "item 1 is still unsaved");
        assert_eq!(d.commit(&[proof(0, &a)]), vec![(0, 1)]);
        assert_eq!(d.saved(), 1);
        assert!(d.settled());
    }

    #[test]
    fn a_wrong_hash_or_unknown_item_never_counts_as_saved() {
        let mut d = Durability::default();
        let a = text(1, "live");
        d.item(1, (0, 1), a.clone());
        assert!(d.commit(&[((0, 1), [7; 32]), proof(1, &a)]).is_empty());
        assert_eq!(d.saved(), 0);
        assert!(!d.settled());
    }

    #[test]
    fn an_oversized_item_is_proven_by_its_trimmed_stored_form() {
        let mut d = Durability::default();
        let big = text(1, &"x".repeat(4 * 1024 * 1024));
        d.item(1, (0, 1), big.clone());
        // The writer stores (and hashes) the trimmed JSON of the same live version.
        assert!(
            history::item_json(&big)
                .unwrap()
                .contains("\"trimmed\":true")
        );
        assert_eq!(d.commit(&[proof(0, &big)]), vec![(0, 1)]);
        assert_eq!(d.saved(), 1);
    }
}
