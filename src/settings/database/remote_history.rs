//! Bounded keyset reads share the metadata worker; they never scan a whole chat
//! or retain an SQLite read transaction while a slow client consumes a page.
use super::{Database, has_table};
use crate::settings::{ChatHistoryRows, Error, Result};
use rusqlite::params;

#[derive(Clone, Debug)]
pub struct RemoteHistoryRequest {
    pub chat: String,
    pub before: Option<(u64, u64)>,
    pub upper: Option<(u64, u64)>,
    pub limit: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    const CHAT: &str = "11111111-1111-4111-8111-111111111111";
    fn open() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        super::super::create(&path).unwrap();
        let db = Database::open(&path, crate::settings::Access::ReadWrite).unwrap();
        (dir, db)
    }
    #[test]
    fn keyset_paging_keeps_its_boundary_when_new_items_arrive() {
        let (_dir, mut db) = open();
        db.save_chat_history(vec![crate::settings::ChatHistoryBatch {
            chat: CHAT.into(),
            epochs: vec![],
            items: (1..=250)
                .map(|i| (0, i, format!("reply {i} żółw")))
                .collect(),
        }])
        .unwrap();
        let mut page = db
            .read_remote_history_page(RemoteHistoryRequest {
                chat: CHAT.into(),
                upper: None,
                before: None,
                limit: 25,
            })
            .unwrap();
        let upper = page.upper;
        let mut ids = page
            .rows
            .items
            .iter()
            .map(|(_, i, _)| *i)
            .collect::<Vec<_>>();
        db.save_chat_history(vec![crate::settings::ChatHistoryBatch {
            chat: CHAT.into(),
            epochs: vec![],
            items: vec![(0, 251, "new live text".into())],
        }])
        .unwrap();
        while let Some(before) = page.next_before {
            page = db
                .read_remote_history_page(RemoteHistoryRequest {
                    chat: CHAT.into(),
                    upper,
                    before: Some(before),
                    limit: 25,
                })
                .unwrap();
            assert!(page.rows.items.len() <= 25);
            ids.extend(page.rows.items.iter().map(|(_, i, _)| *i));
        }
        ids.sort_unstable();
        assert_eq!(ids, (1..=250).collect::<Vec<_>>());
    }
    #[test]
    fn byte_limited_pages_advance_past_unreadable_rows_without_loading_history() {
        let (_dir, mut db) = open();
        db.save_chat_history(vec![crate::settings::ChatHistoryBatch {
            chat: CHAT.into(),
            epochs: vec![],
            items: (1..=4).map(|i| (0, i, "x".repeat(600 * 1024))).collect(),
        }])
        .unwrap();
        db.db
            .execute(
                "UPDATE _canopy_rust_chat_items SET version=99 WHERE item=3",
                [],
            )
            .unwrap();
        let first = db
            .read_remote_history_page(RemoteHistoryRequest {
                chat: CHAT.into(),
                upper: None,
                before: None,
                limit: 100,
            })
            .unwrap();
        assert_eq!(
            first
                .rows
                .items
                .iter()
                .map(|(_, i, _)| *i)
                .collect::<Vec<_>>(),
            vec![4]
        );
        let second = db
            .read_remote_history_page(RemoteHistoryRequest {
                chat: CHAT.into(),
                upper: first.upper,
                before: first.next_before,
                limit: 100,
            })
            .unwrap();
        assert_eq!(second.rows.skipped_keys, vec![(0, 3)]);
        assert_eq!(
            second
                .rows
                .items
                .iter()
                .map(|(_, i, _)| *i)
                .collect::<Vec<_>>(),
            vec![2]
        );
        let third = db
            .read_remote_history_page(RemoteHistoryRequest {
                chat: CHAT.into(),
                upper: first.upper,
                before: second.next_before,
                limit: 100,
            })
            .unwrap();
        assert_eq!(third.rows.items[0].1, 1);
        assert!(third.next_before.is_none());
    }
}
#[derive(Clone, Debug, Default)]
pub struct RemoteHistoryPage {
    pub rows: ChatHistoryRows,
    pub upper: Option<(u64, u64)>,
    pub next_before: Option<(u64, u64)>,
}
fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::InvalidChatHistory)
}
impl Database {
    pub fn read_remote_history_page(
        &self,
        request: RemoteHistoryRequest,
    ) -> Result<RemoteHistoryPage> {
        if uuid::Uuid::parse_str(&request.chat).is_err()
            || request.limit == 0
            || request.limit > 100
        {
            return Err(Error::InvalidChatHistory);
        }
        if !has_table(&self.db, "_canopy_rust_chat_items")? {
            return Ok(RemoteHistoryPage::default());
        }
        let upper = match request.upper {
            Some(key) => Some(key),
            None => {
                use rusqlite::OptionalExtension;
                self.db.query_row("SELECT epoch,item FROM _canopy_rust_chat_items WHERE chat=?1 ORDER BY epoch DESC,item DESC LIMIT 1",[&request.chat],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))
                    .optional()?.map(|(e,i)|Ok::<_,Error>((u64::try_from(e).map_err(|_|Error::InvalidChatHistory)?,u64::try_from(i).map_err(|_|Error::InvalidChatHistory)?))).transpose()?
            }
        };
        let Some((upper_epoch, upper_item)) = upper else {
            return Ok(RemoteHistoryPage::default());
        };
        let (before_epoch, before_item) =
            request.before.unwrap_or((i64::MAX as u64, i64::MAX as u64));
        let upper = (integer(upper_epoch)?, integer(upper_item)?);
        let before = (integer(before_epoch)?, integer(before_item)?);
        // Give SQLite one tight range. With both tuple predicates it can seek
        // at `upper` and scan all newer rows to reach `before` on every page,
        // making a full traversal quadratic despite EXPLAIN reporting SEARCH.
        let (sql, bound) = if before <= upper {
            (
                "SELECT epoch,item,version,payload FROM _canopy_rust_chat_items WHERE chat=?1 AND (epoch,item)<(?2,?3) ORDER BY epoch DESC,item DESC LIMIT ?4",
                before,
            )
        } else {
            (
                "SELECT epoch,item,version,payload FROM _canopy_rust_chat_items WHERE chat=?1 AND (epoch,item)<=(?2,?3) ORDER BY epoch DESC,item DESC LIMIT ?4",
                upper,
            )
        };
        let mut statement = self.db.prepare(sql)?;
        let mut cursor = statement.query(params![
            request.chat,
            bound.0,
            bound.1,
            i64::from(request.limit) + 1,
        ])?;
        let mut rows = ChatHistoryRows::default();
        let mut bytes = 0;
        let mut last = None;
        let mut more = false;
        while let Some(row) = cursor.next()? {
            if rows.items.len() + rows.skipped >= request.limit as usize {
                more = true;
                break;
            }
            let epoch =
                u64::try_from(row.get::<_, i64>(0)?).map_err(|_| Error::InvalidChatHistory)?;
            let item =
                u64::try_from(row.get::<_, i64>(1)?).map_err(|_| Error::InvalidChatHistory)?;
            let version: i64 = row.get(2)?;
            let payload: String = row.get(3)?;
            if bytes + payload.len() > 1024 * 1024 && last.is_some() {
                more = true;
                break;
            }
            if payload.len() > 1024 * 1024 || version != 1 {
                rows.skipped += 1;
                rows.skipped_keys.push((epoch, item));
            } else {
                bytes += payload.len();
                rows.items.push((epoch, item, payload));
            }
            last = Some((epoch, item));
        }
        rows.items.reverse();
        rows.skipped_keys.reverse();
        Ok(RemoteHistoryPage {
            rows,
            upper: Some((upper_epoch, upper_item)),
            next_before: more.then_some(last).flatten(),
        })
    }
}
