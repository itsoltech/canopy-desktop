//! Saved chat conversations: one row per conversation item, so a streaming reply
//! rewrites only itself. Rows carry their format version; a row from a newer version
//! is skipped on load and never overwritten.
use super::{Database, has_table};
use crate::settings::{Access, Error, Result};
use rusqlite::{Transaction, params};
use sha2::{Digest, Sha256};

const VERSION: i64 = 1;
/// Upper bound for one stored item; the chat trims long outputs well below it.
const MAX_PAYLOAD: usize = 1024 * 1024;

/// Changes to one chat's history: separators between sessions (`epochs`) and items,
/// each as JSON.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatHistoryBatch {
    pub chat: String,
    /// `(epoch, meta)`
    pub epochs: Vec<(u64, String)>,
    /// `(epoch, item id, item)`
    pub items: Vec<(u64, u64, String)>,
}

/// Proof returned only after the item rows and checkpoint counter commit together.
#[derive(Clone, Debug)]
pub struct HistoryCheckpoint {
    chat: String,
    checkpoint: u64,
    items: Vec<HistoryCheckpointItem>,
}
impl HistoryCheckpoint {
    pub fn chat(&self) -> &str {
        &self.chat
    }
    pub fn checkpoint(&self) -> u64 {
        self.checkpoint
    }
    pub fn items(&self) -> &[HistoryCheckpointItem] {
        &self.items
    }
}
#[derive(Clone, Debug)]
pub struct HistoryCheckpointItem {
    epoch: u64,
    item: u64,
    sha256: [u8; 32],
}
impl HistoryCheckpointItem {
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn item(&self) -> u64 {
        self.item
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
}

/// A chat's saved history in order, without rows of an unknown format.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatHistoryRows {
    pub epochs: Vec<(u64, String)>,
    pub items: Vec<(u64, u64, String)>,
    /// Rows left out: written by a newer version or unreadable.
    pub skipped: usize,
    /// `(epoch, item)` of item rows left out, so new rows can go after them.
    pub skipped_keys: Vec<(u64, u64)>,
}

/// A closed chat waiting for `/resume` (DEC-094): when it closed and what reopens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedChatRow {
    pub chat: String,
    /// Unix seconds.
    pub closed_at: i64,
    pub payload: String,
}

fn valid_chat(chat: &str) -> bool {
    uuid::Uuid::parse_str(chat).is_ok()
}

fn create(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS _canopy_rust_chat_epochs(chat TEXT NOT NULL,epoch INTEGER NOT NULL,version INTEGER NOT NULL,meta TEXT NOT NULL,PRIMARY KEY(chat,epoch));
         CREATE TABLE IF NOT EXISTS _canopy_rust_chat_items(chat TEXT NOT NULL,epoch INTEGER NOT NULL,item INTEGER NOT NULL,version INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(chat,epoch,item));
         CREATE TABLE IF NOT EXISTS _canopy_rust_closed_chats(chat TEXT PRIMARY KEY,version INTEGER NOT NULL,closed_at INTEGER NOT NULL,payload TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS _canopy_rust_models(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,payload TEXT NOT NULL);",
    )?;
    Ok(())
}

impl Database {
    pub fn save_chat_history(&mut self, batches: Vec<ChatHistoryBatch>) -> Result<()> {
        self.save_chat_history_checkpointed(batches).map(|_| ())
    }
    pub fn save_chat_history_checkpointed(
        &mut self,
        batches: Vec<ChatHistoryBatch>,
    ) -> Result<Vec<HistoryCheckpoint>> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if !batches.iter().all(|batch| valid_chat(&batch.chat)) {
            return Err(Error::InvalidChatHistory);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        create(&tx)?;
        let runtime = has_table(&tx, "_canopy_runtime_chats")?;
        let mut checkpoints = Vec::new();
        for batch in &batches {
            let mut committed = Vec::new();
            if has_table(&tx, "_canopy_runtime_chats")?
                && let Some(epoch) = batch
                    .epochs
                    .iter()
                    .map(|(epoch, _)| *epoch)
                    .chain(batch.items.iter().map(|(epoch, ..)| *epoch))
                    .max()
            {
                let epoch = i64::try_from(epoch).map_err(|_| Error::InvalidChatHistory)?;
                tx.execute("UPDATE _canopy_runtime_chats SET revision=revision+(history_epoch<?2),history_epoch=max(history_epoch,?2) WHERE chat_id=?1", params![batch.chat,epoch])?;
            }
            // An oversized row is left out alone; the rest of the chat still saves.
            let epochs = batch.epochs.iter().filter(|(_, m)| m.len() <= MAX_PAYLOAD);
            for (epoch, meta) in epochs {
                tx.execute(
                    "INSERT INTO _canopy_rust_chat_epochs VALUES(?1,?2,?3,?4) ON CONFLICT(chat,epoch) DO UPDATE SET meta=excluded.meta WHERE version=?3",
                    params![batch.chat, *epoch as i64, VERSION, meta],
                )?;
            }
            let items = batch.items.iter().filter(|(.., p)| p.len() <= MAX_PAYLOAD);
            for (epoch, item, payload) in items {
                let changed = tx.execute(
                    "INSERT INTO _canopy_rust_chat_items VALUES(?1,?2,?3,?4,?5) ON CONFLICT(chat,epoch,item) DO UPDATE SET payload=excluded.payload WHERE version=?4",
                    params![batch.chat, *epoch as i64, *item as i64, VERSION, payload],
                )?;
                if changed == 1 {
                    committed.push(HistoryCheckpointItem {
                        epoch: *epoch,
                        item: *item,
                        sha256: Sha256::digest(payload.as_bytes()).into(),
                    });
                }
            }
            let checkpoint = if runtime {
                use rusqlite::OptionalExtension;
                tx.query_row("UPDATE _canopy_runtime_chats SET checkpoint=checkpoint+1 WHERE chat_id=?1 AND checkpoint<9223372036854775807 RETURNING checkpoint",[&batch.chat],|r|r.get::<_,i64>(0)).optional()?.unwrap_or(0) as u64
            } else {
                0
            };
            checkpoints.push(HistoryCheckpoint {
                chat: batch.chat.clone(),
                checkpoint,
                items: committed,
            });
        }
        tx.commit()?;
        Ok(checkpoints)
    }

    /// Bounded proof for a snapshot's current tail, not a scan of the transcript.
    pub fn read_history_checkpoint(
        &self,
        chat: String,
        keys: Vec<(u64, u64)>,
    ) -> Result<HistoryCheckpoint> {
        if !valid_chat(&chat) || keys.len() > 100 {
            return Err(Error::InvalidChatHistory);
        }
        let checkpoint = if has_table(&self.db, "_canopy_runtime_chats")? {
            use rusqlite::OptionalExtension;
            self.db
                .query_row(
                    "SELECT checkpoint FROM _canopy_runtime_chats WHERE chat_id=?1",
                    [&chat],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0) as u64
        } else {
            0
        };
        let mut items = Vec::new();
        if has_table(&self.db, "_canopy_rust_chat_items")? {
            use rusqlite::OptionalExtension;
            let mut statement=self.db.prepare("SELECT payload FROM _canopy_rust_chat_items WHERE chat=?1 AND epoch=?2 AND item=?3 AND version=?4 AND length(CAST(payload AS BLOB))<=?5")?;
            for (epoch, item) in keys {
                let payload: Option<String> = statement
                    .query_row(
                        params![
                            chat,
                            i64::try_from(epoch).map_err(|_| Error::InvalidChatHistory)?,
                            i64::try_from(item).map_err(|_| Error::InvalidChatHistory)?,
                            VERSION,
                            MAX_PAYLOAD as i64
                        ],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(payload) = payload {
                    items.push(HistoryCheckpointItem {
                        epoch,
                        item,
                        sha256: Sha256::digest(payload.as_bytes()).into(),
                    });
                }
            }
        }
        Ok(HistoryCheckpoint {
            chat,
            checkpoint,
            items,
        })
    }

    pub fn load_chat_history(&self, chat: &str) -> Result<ChatHistoryRows> {
        if !valid_chat(chat) {
            return Err(Error::InvalidChatHistory);
        }
        super::schema(&self.db)?;
        let mut rows = ChatHistoryRows::default();
        if !has_table(&self.db, "_canopy_rust_chat_epochs")?
            || !has_table(&self.db, "_canopy_rust_chat_items")?
        {
            return Ok(rows);
        }
        let mut statement = self.db.prepare(
            "SELECT epoch,version,meta FROM _canopy_rust_chat_epochs WHERE chat=?1 ORDER BY epoch",
        )?;
        for row in statement.query_map([chat], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            match row? {
                (epoch, VERSION, meta) if epoch >= 0 => rows.epochs.push((epoch as u64, meta)),
                _ => rows.skipped += 1,
            }
        }
        let mut statement = self.db.prepare(
            "SELECT epoch,item,version,payload FROM _canopy_rust_chat_items WHERE chat=?1 ORDER BY epoch,item",
        )?;
        for row in statement.query_map([chat], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })? {
            match row? {
                (epoch, item, VERSION, payload) if epoch >= 0 && item >= 0 => {
                    rows.items.push((epoch as u64, item as u64, payload))
                }
                (epoch, item, ..) => {
                    rows.skipped += 1;
                    if epoch >= 0 && item >= 0 {
                        rows.skipped_keys.push((epoch as u64, item as u64));
                    }
                }
            }
        }
        Ok(rows)
    }

    /// The model picker's remembered model lists and favorites (DEC-098/099), as JSON.
    pub fn save_model_catalog(&mut self, payload: String) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(Error::InvalidChatHistory);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        create(&tx)?;
        tx.execute(
            "INSERT INTO _canopy_rust_models VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload WHERE version=?1",
            params![VERSION, payload],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_model_catalog(&self) -> Result<Option<String>> {
        super::schema(&self.db)?;
        if !has_table(&self.db, "_canopy_rust_models")? {
            return Ok(None);
        }
        let mut statement = self
            .db
            .prepare("SELECT payload FROM _canopy_rust_models WHERE id=1 AND version=?1")?;
        let mut rows = statement.query_map([VERSION], |row| row.get::<_, String>(0))?;
        Ok(rows.next().transpose()?)
    }

    /// Keeps a closed chat for `/resume`, or updates when it closed.
    pub fn save_closed_chat(&mut self, row: ClosedChatRow) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if !valid_chat(&row.chat) || row.payload.len() > MAX_PAYLOAD {
            return Err(Error::InvalidChatHistory);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        create(&tx)?;
        tx.execute(
            "INSERT INTO _canopy_rust_closed_chats VALUES(?1,?2,?3,?4) ON CONFLICT(chat) DO UPDATE SET closed_at=excluded.closed_at,payload=excluded.payload WHERE version=?2",
            params![row.chat, VERSION, row.closed_at, row.payload],
        )?;
        if has_table(&tx, "_canopy_runtime_chats")? {
            tx.execute("UPDATE _canopy_runtime_chats SET revision=revision+(closed_at IS NOT ?2),closed_at=?2,queue_paused=1,queue_revision=queue_revision+1 WHERE chat_id=?1",params![row.chat,row.closed_at])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Closed chats, latest first, without rows of another format.
    pub fn load_closed_chats(&self) -> Result<Vec<ClosedChatRow>> {
        super::schema(&self.db)?;
        if !has_table(&self.db, "_canopy_rust_closed_chats")? {
            return Ok(vec![]);
        }
        let mut statement = self.db.prepare(
            "SELECT chat,closed_at,payload FROM _canopy_rust_closed_chats WHERE version=?1 ORDER BY closed_at DESC",
        )?;
        let rows = statement
            .query_map([VERSION], |row| {
                Ok(ClosedChatRow {
                    chat: row.get(0)?,
                    closed_at: row.get(1)?,
                    payload: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// A closed chat opened again: it leaves the list, its history stays.
    pub fn remove_closed_chat(&mut self, chat: &str) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        create(&tx)?;
        tx.execute(
            "DELETE FROM _canopy_rust_closed_chats WHERE chat=?1",
            [chat],
        )?;
        if has_table(&tx, "_canopy_runtime_chats")? {
            tx.execute(
                "UPDATE _canopy_runtime_chats SET revision=revision+(closed_at IS NOT NULL),closed_at=NULL WHERE chat_id=?1",
                [chat],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes the chats neither in `keep` (open) nor closed at or after `closed_since`
    /// (unix seconds); returns the deleted chat ids.
    pub fn retain_chat_history(
        &mut self,
        keep: Vec<String>,
        closed_since: i64,
    ) -> Result<Vec<String>> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        create(&tx)?;
        let recent: Vec<String> = {
            let mut statement =
                tx.prepare("SELECT chat FROM _canopy_rust_closed_chats WHERE closed_at>=?1")?;
            statement
                .query_map([closed_since], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut stored: Vec<String> = {
            let mut statement = tx.prepare(
                "SELECT chat FROM _canopy_rust_chat_epochs UNION SELECT chat FROM _canopy_rust_chat_items UNION SELECT chat FROM _canopy_rust_closed_chats",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        let active: Vec<String> = if has_table(&tx, "_canopy_runtime_chats")? {
            tx.prepare("SELECT chat_id FROM _canopy_runtime_chats WHERE closed_at IS NULL")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        } else {
            Vec::new()
        };
        stored.retain(|chat| {
            !keep.contains(chat) && !recent.contains(chat) && !active.contains(chat)
        });
        for chat in &stored {
            delete(&tx, chat)?;
        }
        tx.commit()?;
        Ok(stored)
    }

    pub fn delete_chat_history(&mut self, chat: &str) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        create(&tx)?;
        delete(&tx, chat)?;
        tx.commit()?;
        Ok(())
    }
}

fn delete(tx: &Transaction, chat: &str) -> Result<()> {
    if has_table(tx, "_canopy_runtime_chats")? {
        // SQLite foreign_keys is not enabled on legacy connections: clean both explicitly.
        // Its thread alias stays, so the old chat ID keeps naming the same thread.
        tx.execute("DELETE FROM _canopy_chat_queue WHERE chat_id=?1", [chat])?;
        tx.execute("DELETE FROM _canopy_runtime_chats WHERE chat_id=?1", [chat])?;
    }
    tx.execute("DELETE FROM _canopy_rust_chat_epochs WHERE chat=?1", [chat])?;
    tx.execute("DELETE FROM _canopy_rust_chat_items WHERE chat=?1", [chat])?;
    tx.execute(
        "DELETE FROM _canopy_rust_closed_chats WHERE chat=?1",
        [chat],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";

    fn open() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("canopy.db");
        crate::settings::database::create(&path).expect("create");
        let db = Database::open(&path, Access::ReadWrite).expect("open");
        (dir, db)
    }

    fn batch(chat: &str, item: u64, payload: &str) -> ChatHistoryBatch {
        ChatHistoryBatch {
            chat: chat.into(),
            epochs: vec![(0, "{}".into())],
            items: vec![(0, item, payload.into())],
        }
    }

    #[test]
    fn checkpoint_proof_tracks_only_committed_rows_and_rolls_back_with_failed_turn() {
        let (_dir, mut db) = open();
        db.save_chat_history(vec![batch(A, 1, "old")]).unwrap();
        db.initialize_runtime().unwrap();
        let first = db
            .save_chat_history_checkpointed(vec![batch(A, 1, "saved text")])
            .unwrap()
            .remove(0);
        assert!(first.checkpoint() > 0);
        assert_eq!(
            first.items()[0].sha256(),
            &<[u8; 32]>::from(Sha256::digest(b"saved text"))
        );
        db.db.execute_batch("CREATE TEMP TRIGGER fail_turn BEFORE INSERT ON _canopy_rust_chat_items WHEN NEW.item=3 BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(
            db.save_chat_history_checkpointed(vec![ChatHistoryBatch {
                chat: A.into(),
                epochs: vec![],
                items: vec![
                    (0, 1, "unsaved final text".into()),
                    (0, 3, "turn end".into())
                ]
            }])
            .is_err()
        );
        let proof = db
            .read_history_checkpoint(A.into(), vec![(0, 1), (0, 3)])
            .unwrap();
        assert_eq!(proof.checkpoint(), first.checkpoint());
        assert_eq!(proof.items().len(), 1);
        assert_eq!(proof.items()[0].sha256(), first.items()[0].sha256());
        assert_eq!(db.load_chat_history(A).unwrap().items[0].2, "saved text");
        db.db.execute_batch("DROP TRIGGER fail_turn;").unwrap();
        let done = db
            .save_chat_history_checkpointed(vec![ChatHistoryBatch {
                chat: A.into(),
                epochs: vec![],
                items: vec![(0, 1, "final text".into()), (0, 3, "turn end".into())],
            }])
            .unwrap()
            .remove(0);
        assert!(done.checkpoint() > proof.checkpoint());
        assert_eq!(done.items().len(), 2);
        let restored = db
            .read_history_checkpoint(A.into(), vec![(0, 1), (0, 3)])
            .unwrap();
        assert_eq!(restored.checkpoint(), done.checkpoint());
        assert!(
            restored
                .items()
                .iter()
                .zip(done.items())
                .all(|(a, b)| a.sha256() == b.sha256())
        );
        db.db
            .execute(
                "UPDATE _canopy_rust_chat_items SET version=99 WHERE chat=?1 AND item=3",
                [A],
            )
            .unwrap();
        let omitted = db
            .save_chat_history_checkpointed(vec![batch(A, 3, "must not overwrite newer format")])
            .unwrap()
            .remove(0);
        assert!(
            omitted.items().is_empty(),
            "an unmodified newer-format row must never receive proof of this write"
        );
    }

    #[test]
    fn items_are_upserted_and_loaded_in_order() {
        let (_dir, mut db) = open();
        assert_eq!(db.load_chat_history(A).expect("empty"), Default::default());
        db.save_chat_history(vec![batch(A, 2, "b"), batch(A, 1, "a")])
            .expect("save");
        db.save_chat_history(vec![batch(A, 2, "b2")])
            .expect("update");
        let rows = db.load_chat_history(A).expect("load");
        assert_eq!(rows.epochs, [(0, "{}".to_owned())]);
        assert_eq!(rows.items, [(0, 1, "a".into()), (0, 2, "b2".into())]);
    }

    #[test]
    fn rows_of_a_newer_format_are_skipped_and_kept() {
        let (_dir, mut db) = open();
        db.save_chat_history(vec![batch(A, 1, "a")]).expect("save");
        db.db
            .execute(
                "UPDATE _canopy_rust_chat_items SET version=2,payload='future'",
                [],
            )
            .expect("newer row");
        db.save_chat_history(vec![batch(A, 1, "overwrite")])
            .expect("save");
        let rows = db.load_chat_history(A).expect("load");
        assert!(rows.items.is_empty());
        assert_eq!(rows.skipped, 1);
        assert_eq!(rows.skipped_keys, [(0, 1)]);
        let payload: String = db
            .db
            .query_row("SELECT payload FROM _canopy_rust_chat_items", [], |r| {
                r.get(0)
            })
            .expect("row");
        assert_eq!(payload, "future");
    }

    #[test]
    fn deleting_and_retaining_remove_whole_chats() {
        let (_dir, mut db) = open();
        db.save_chat_history(vec![batch(A, 1, "a"), batch(B, 1, "b")])
            .expect("save");
        assert_eq!(
            db.retain_chat_history(vec![A.into()], 0).expect("retain"),
            [B.to_owned()]
        );
        assert!(db.load_chat_history(B).expect("load").items.is_empty());
        db.delete_chat_history(A).expect("delete");
        assert!(db.load_chat_history(A).expect("load").items.is_empty());
        assert!(
            db.save_chat_history(vec![batch("not-a-uuid", 1, "x")])
                .is_err()
        );
    }

    #[test]
    fn the_model_catalog_is_one_row_rewritten() {
        let (_dir, mut db) = open();
        assert_eq!(db.load_model_catalog().expect("none"), None);
        db.save_model_catalog("{\"a\":1}".into()).expect("save");
        db.save_model_catalog("{\"a\":2}".into()).expect("again");
        assert_eq!(
            db.load_model_catalog().expect("load").as_deref(),
            Some("{\"a\":2}")
        );
    }

    #[test]
    fn closed_chats_stay_for_a_while_and_go_with_their_history() {
        const C: &str = "33333333-3333-4333-8333-333333333333";
        let (_dir, mut db) = open();
        assert!(db.load_closed_chats().expect("none yet").is_empty());
        db.save_chat_history(vec![batch(A, 1, "a"), batch(B, 1, "b"), batch(C, 1, "c")])
            .expect("save");
        let closed = |chat: &str, closed_at: i64| ClosedChatRow {
            chat: chat.into(),
            closed_at,
            payload: format!("{{\"title\":\"{chat}\"}}"),
        };
        db.save_closed_chat(closed(A, 100)).expect("close A");
        db.save_closed_chat(closed(B, 300)).expect("close B");
        let listed: Vec<String> = db
            .load_closed_chats()
            .expect("load")
            .into_iter()
            .map(|row| row.chat)
            .collect();
        assert_eq!(listed, [B, A]);
        // Open C is kept; A closed before the limit goes, history and all.
        assert_eq!(
            db.retain_chat_history(vec![C.into()], 200).expect("retain"),
            [A.to_owned()]
        );
        assert!(db.load_chat_history(A).expect("load").items.is_empty());
        assert!(!db.load_chat_history(B).expect("load").items.is_empty());
        // Opened again: off the list, history kept. Deleted: both gone.
        db.remove_closed_chat(B).expect("reopen");
        assert!(db.load_closed_chats().expect("load").is_empty());
        assert!(!db.load_chat_history(B).expect("load").items.is_empty());
        db.save_closed_chat(closed(C, 400)).expect("close C");
        db.delete_chat_history(C).expect("delete");
        assert!(db.load_closed_chats().expect("load").is_empty());
        assert!(db.save_closed_chat(closed("not-a-uuid", 1)).is_err());
    }
}
