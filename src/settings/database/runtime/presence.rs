use super::*;
use crate::host::{DirtyPresence, FileIdentity};

pub(super) fn same_intent(input: &CommandInput, normalized: &CommandInput) -> Result<()> {
    if input.method != normalized.method
        || input.target != normalized.target
        || input.arguments != normalized.arguments
        || input.fingerprint != normalized.fingerprint
        || input.root != normalized.root
    {
        return Err(Error::CommandConflict);
    }
    Ok(())
}
impl Database {
    /// Indexed prefix range over the encoded (root, relative path) pair. No
    /// filesystem lookup: deleted worktrees and offline buffers remain risks.
    pub fn runtime_root_presence(
        &self,
        root: String,
    ) -> Result<Vec<(FileIdentity, DirtyPresence)>> {
        require_host(self)?;
        FileIdentity::new(root.clone(), "validation".into()).ok_or(Error::InvalidRuntimeState)?;
        let encoded = serde_json::to_string(&root).map_err(|_| Error::InvalidRuntimeState)?;
        let lower = format!("[{encoded},");
        // ',' immediately precedes '-' in binary collation. This range matches
        // exactly the root JSON string, including escaped quotes/backslashes.
        let upper = format!("[{encoded}-");
        let mut statement = self.db.prepare("SELECT p.resource,p.device_id,CASE WHEN p.device_id='local' THEN 'This host' ELSE d.name END,p.base_hash,p.last_seen,p.revision FROM _canopy_dirty_presence p LEFT JOIN _canopy_remote_devices d ON d.device_id=p.device_id WHERE p.resource>=?1 AND p.resource<?2 ORDER BY p.resource,p.device_id LIMIT 1025")?;
        let mut rows = statement.query(params![lower, upper])?;
        let mut result = Vec::new();
        let mut bytes = 0usize;
        while let Some(row) = rows.next()? {
            if result.len() == 1024 {
                return Err(Error::ResourceLimit);
            }
            let resource: String = row.get(0)?;
            let device_id: String = row.get(1)?;
            let device_name: String = row.get(2)?;
            bytes =
                bytes.saturating_add(resource.len() + device_id.len() + device_name.len() + 128);
            if bytes > host::CONTROL_BYTES {
                return Err(Error::ResourceLimit);
            }
            let (stored_root, relative): (String, String) =
                serde_json::from_str(&resource).map_err(|_| Error::InvalidRuntimeState)?;
            if stored_root != root {
                return Err(Error::InvalidRuntimeState);
            }
            let file =
                FileIdentity::new(stored_root, relative).ok_or(Error::InvalidRuntimeState)?;
            let hash: Vec<u8> = row.get(3)?;
            result.push((
                file,
                DirtyPresence {
                    device_id,
                    device_name,
                    base_hash: hash.try_into().map_err(|_| Error::InvalidRuntimeState)?,
                    last_seen: row.get(4)?,
                    revision: unsigned(row.get(5)?)?,
                },
            ));
        }
        Ok(result)
    }
    pub fn set_runtime_dirty_presence(
        &mut self,
        input: CommandInput,
        file: FileIdentity,
        base_hash: [u8; 32],
        dirty: bool,
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        if input.key.resource.is_empty() {
            let normalized = file
                .presence_command(input.key.clone(), base_hash, dirty)
                .ok_or(Error::InvalidRuntimeState)?;
            same_intent(&input, &normalized)?;
        } else {
            let (_, _, _, command, _): (
                String,
                String,
                u64,
                crate::command_bus::Command,
                crate::command_bus::Conditions,
            ) = serde_json::from_slice(&input.arguments).map_err(|_| Error::InvalidRuntimeState)?;
            match command {
                crate::command_bus::Command::File(crate::command_bus::FileCommand::Presence {
                    path,
                    base_hash: expected,
                    dirty: value,
                }) if path == file.relative_path()
                    && expected == base_hash
                    && value == dirty
                    && input.target == file.resource()
                    && input.root.as_deref() == Some(file.root()) => {}
                _ => return Err(Error::CommandConflict),
            }
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(receipt) = bus::runtime_admission(&tx, &input, host::FILES_GIT)? {
            return Ok(receipt);
        }
        if dirty {
            let revision: i64 = tx.query_row("UPDATE _canopy_runtime_host SET presence_revision=presence_revision+1 WHERE id=1 AND presence_revision<9223372036854775807 RETURNING presence_revision", [], |r| r.get(0)).optional()?.ok_or(Error::ResourceLimit)?;
            tx.execute("INSERT INTO _canopy_dirty_presence(device_id,resource,base_hash,last_seen,revision) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(device_id,resource) DO UPDATE SET base_hash=excluded.base_hash,last_seen=excluded.last_seen,revision=excluded.revision", params![input.key.device,file.resource(),base_hash.as_slice(),now(),revision])?;
        } else {
            tx.execute(
                "DELETE FROM _canopy_dirty_presence WHERE device_id=?1 AND resource=?2",
                params![input.key.device, file.resource()],
            )?;
        }
        let receipt = insert_command(&tx, &input, CommandState::Succeeded, None, host::FILES_GIT)?;
        tx.commit()?;
        Ok(receipt)
    }
    /// Host read includes offline/restarted devices, with no TTL expiry. A remote
    /// handler must authorize its caller for this file root before invoking it.
    pub fn runtime_file_presence(&self, file: FileIdentity) -> Result<Vec<DirtyPresence>> {
        require_host(self)?;
        let rows = self.db.prepare("SELECT p.device_id,CASE WHEN p.device_id='local' THEN 'This host' ELSE d.name END,p.base_hash,p.last_seen,p.revision FROM _canopy_dirty_presence p LEFT JOIN _canopy_remote_devices d ON d.device_id=p.device_id WHERE p.resource=?1 ORDER BY p.device_id")?.query_map([file.resource()], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(device_id, device_name, hash, last_seen, revision)| {
                Ok(DirtyPresence {
                    device_id,
                    device_name,
                    base_hash: hash.try_into().map_err(|_| Error::InvalidRuntimeState)?,
                    last_seen,
                    revision: unsigned(revision)?,
                })
            })
            .collect()
    }
    /// Dismiss removes only the host warning, never the client's draft.
    pub fn dismiss_runtime_dirty_presence(
        &mut self,
        input: CommandInput,
        file: FileIdentity,
        device: String,
        revision: u64,
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        let normalized = file
            .dismiss_presence_command(input.key.clone(), &device, revision)
            .ok_or(Error::InvalidRuntimeState)?;
        same_intent(&input, &normalized)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        authorize_root(&tx, &input.key, host::ADMIN, file.root(), true)?;
        if let Some(receipt) = existing(&tx, &input)? {
            return Ok(receipt);
        }
        let changed = tx.execute(
            "DELETE FROM _canopy_dirty_presence WHERE device_id=?1 AND resource=?2 AND revision=?3",
            params![device, file.resource(), integer(revision)?],
        )?;
        if changed == 0 {
            return Err(Error::ResourceConflict);
        }
        let receipt = insert_command(&tx, &input, CommandState::Succeeded, None, host::ADMIN)?;
        tx.commit()?;
        Ok(receipt)
    }
}
