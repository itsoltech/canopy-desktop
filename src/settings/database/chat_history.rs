//! Saved chat conversations: one row per conversation item, so a streaming reply
//! rewrites only itself. Rows carry their format version; a row from a newer version
//! is skipped on load and never overwritten.
use super::{Database, has_table};
use crate::settings::{Access, Error, Result};
use rusqlite::{Transaction, params};

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

fn valid_chat(chat: &str) -> bool {
    uuid::Uuid::parse_str(chat).is_ok()
}

fn create(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS _canopy_rust_chat_epochs(chat TEXT NOT NULL,epoch INTEGER NOT NULL,version INTEGER NOT NULL,meta TEXT NOT NULL,PRIMARY KEY(chat,epoch));
         CREATE TABLE IF NOT EXISTS _canopy_rust_chat_items(chat TEXT NOT NULL,epoch INTEGER NOT NULL,item INTEGER NOT NULL,version INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(chat,epoch,item));",
    )?;
    Ok(())
}

impl Database {
    pub fn save_chat_history(&mut self, batches: Vec<ChatHistoryBatch>) -> Result<()> {
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
        for batch in &batches {
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
                tx.execute(
                    "INSERT INTO _canopy_rust_chat_items VALUES(?1,?2,?3,?4,?5) ON CONFLICT(chat,epoch,item) DO UPDATE SET payload=excluded.payload WHERE version=?4",
                    params![batch.chat, *epoch as i64, *item as i64, VERSION, payload],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
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

    /// Deletes the chats not in `keep`; returns the deleted chat ids.
    pub fn retain_chat_history(&mut self, keep: Vec<String>) -> Result<Vec<String>> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        if !has_table(&tx, "_canopy_rust_chat_epochs")? {
            return Ok(vec![]);
        }
        let mut stored: Vec<String> = {
            let mut statement = tx.prepare(
                "SELECT chat FROM _canopy_rust_chat_epochs UNION SELECT chat FROM _canopy_rust_chat_items",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        stored.retain(|chat| !keep.contains(chat));
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
        if has_table(&tx, "_canopy_rust_chat_epochs")? {
            delete(&tx, chat)?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn delete(tx: &Transaction, chat: &str) -> Result<()> {
    tx.execute("DELETE FROM _canopy_rust_chat_epochs WHERE chat=?1", [chat])?;
    tx.execute("DELETE FROM _canopy_rust_chat_items WHERE chat=?1", [chat])?;
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
            db.retain_chat_history(vec![A.into()]).expect("retain"),
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
}
