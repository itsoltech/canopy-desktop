//! Immutable remote staging references. No secret bytes or unkeyed secret hashes.
use super::Database;
use crate::{
    host::{ADMIN, CommandKey},
    settings::{Access, Error, Result},
};
use rusqlite::{OptionalExtension, params};
fn host(db: &Database) -> Result<()> {
    if db.access == Access::ReadOnly {
        return Err(Error::ReadOnly);
    }
    if db.runtime_lease.is_none() {
        return Err(Error::InvalidRuntimeState);
    }
    super::runtime::validate_schema(&db.db)
}
impl Database {
    pub fn reserve_remote_secret(
        &mut self,
        key: CommandKey,
        id: String,
        fingerprint: Vec<u8>,
    ) -> Result<()> {
        host(self)?;
        if !crate::artifacts::valid_id(&id)
            || fingerprint.len() != 32
            || !key.resource().is_empty()
            || key.executor_epoch() != 0
        {
            return Err(Error::InvalidRuntimeState);
        }
        self.authorize_runtime_read(key.device().into(), ADMIN, "private".into())?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let epoch:i64=tx.query_row("SELECT command_epoch FROM _canopy_remote_devices WHERE device_id=?1 AND revoked_at IS NULL",[key.device()],|r|r.get(0))?;
        if epoch as u64 != key.epoch() {
            return Err(Error::CommandRetired);
        }
        let existing:Option<Vec<u8>>=tx.query_row("SELECT fingerprint FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND resource='' AND executor_epoch=0",params![key.device(),key.id()],|r|r.get(0)).optional()?;
        if existing.is_some_and(|f| f != fingerprint) {
            return Err(Error::CommandConflict);
        }
        let conflict:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _canopy_settings_secrets WHERE device_id=?1 AND command_id=?2 AND (command_epoch!=?3 OR fingerprint!=?4))",params![key.device(),key.id(),key.epoch() as i64,fingerprint],|r|r.get(0))?;
        if conflict {
            return Err(Error::CommandConflict);
        }
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM _canopy_settings_secrets", [], |r| {
                r.get(0)
            })?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _canopy_settings_secrets WHERE version_id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        if count >= 8192 && !exists {
            return Err(Error::ResourceLimit);
        }
        tx.execute("INSERT OR IGNORE INTO _canopy_settings_secrets(version_id,device_id,command_id,command_epoch,fingerprint,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,key.device(),key.id(),key.epoch() as i64,fingerprint,crate::artifacts::now()])?;
        tx.commit()?;
        Ok(())
    }
    /// The RPC staging gate spans this query, vault removal, and forget. Versions
    /// are used only during dispatch; active config uses its owner's own keys.
    pub fn collect_remote_secrets(&self) -> Result<Vec<String>> {
        host(self)?;
        Ok(self.db.prepare("SELECT s.version_id FROM _canopy_settings_secrets s LEFT JOIN _canopy_command_inbox c ON c.device_id=s.device_id AND c.command_id=s.command_id AND c.command_epoch=s.command_epoch AND c.resource='' AND c.executor_epoch=0 WHERE c.state IN ('succeeded','failed','rejected') OR (c.command_id IS NULL AND s.created_at<?1) LIMIT 8192")?
            .query_map([crate::artifacts::now()-3600],|r|r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn forget_remote_secret(&mut self, id: String) -> Result<()> {
        host(self)?;
        self.db.execute(
            "DELETE FROM _canopy_settings_secrets WHERE version_id=?1",
            [id],
        )?;
        Ok(())
    }
}
