//! Finalize a known filesystem effect without re-authorizing the former caller.
//! Revocation can prohibit another effect; it cannot make an applied save vanish.
use super::*;
use crate::host::FileIdentity;
impl Database {
    pub fn finish_file_save(
        &mut self,
        key: CommandKey,
        file: FileIdentity,
        expected_presence_revision: Option<u64>,
        result_hash: [u8; 32],
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old = receipt(&tx, &key)?.ok_or(Error::NotFound)?;
        if old.method() != "files.save" || old.target() != file.resource() {
            return Err(Error::CommandConflict);
        }
        if old.state() != CommandState::Attempting {
            return Err(Error::ResourceConflict);
        }
        check_result_budget(&tx, &key, Some(&result_hash))?;
        let changed=tx.execute("UPDATE _canopy_command_inbox SET state='succeeded',result=?4,updated_at=?5 WHERE device_id=?1 AND command_epoch=?2 AND command_id=?3 AND state='attempting'",params![key.device(),integer(key.epoch())?,key.id(),result_hash.as_slice(),now()])?;
        if changed != 1 {
            return Err(Error::ResourceConflict);
        }
        if let Some(revision) = expected_presence_revision {
            tx.execute("DELETE FROM _canopy_dirty_presence WHERE device_id=?1 AND resource=?2 AND revision=?3",params![key.device(),file.resource(),integer(revision)?])?;
        }
        let result = receipt(&tx, &key)?.ok_or(Error::NotFound)?;
        tx.commit()?;
        Ok(result)
    }
}
