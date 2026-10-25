//! Artifact metadata operations run only on the existing SQLite worker.
use super::Database;
use crate::{
    artifacts::*,
    host::LOCAL_PRINCIPAL,
    settings::{Access, Error, Result},
};
use rusqlite::{Connection, OptionalExtension, params};

fn host(db: &Database) -> Result<()> {
    if db.access == Access::ReadOnly {
        return Err(Error::ReadOnly);
    }
    if db.runtime_lease.is_none() {
        return Err(Error::InvalidRuntimeState);
    }
    super::runtime::validate_schema(&db.db)
}
fn json<T: serde::Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|_| Error::InvalidRuntimeState)
}
fn decode<T: serde::de::DeserializeOwned>(v: &str) -> Result<T> {
    serde_json::from_str(v).map_err(|_| Error::InvalidRuntimeState)
}
fn load(db: &Connection, id: &str) -> Result<Upload> {
    let row=db.query_row("SELECT device_id,target,purpose,expected_size,expected_hash,media_type,ranges,artifact_id,last_active FROM _canopy_uploads WHERE upload_id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,i64>(8)?))).optional()?.ok_or(Error::NotFound)?;
    Ok(Upload {
        id: id.into(),
        binding: BeginUpload {
            owner_device: row.0,
            target: decode(&row.1)?,
            purpose: decode(&row.2)?,
            size: u64::try_from(row.3).map_err(|_| Error::InvalidRuntimeState)?,
            sha256: row.4.try_into().map_err(|_| Error::InvalidRuntimeState)?,
            media_type: row.5,
        },
        ranges: decode(&row.6)?,
        artifact_id: row.7,
        last_active: row.8,
    })
}
fn authorize_root(db: &Connection, owner: &str, root: &str, scope: u32) -> Result<()> {
    if owner == LOCAL_PRINCIPAL {
        return Ok(());
    }
    if root.is_empty() {
        return Err(Error::NotAuthorized);
    }
    let row=db.query_row("SELECT scopes,roots FROM _canopy_remote_devices WHERE device_id=?1 AND revoked_at IS NULL",[owner],|r|Ok((r.get::<_,u32>(0)?,r.get::<_,String>(1)?))).optional()?.ok_or(Error::NotAuthorized)?;
    let roots: Vec<String> = decode(&row.1)?;
    if row.0 & scope != scope || !roots.iter().any(|v| v == "*" || v == root) {
        return Err(Error::NotAuthorized);
    }
    Ok(())
}
fn authorize_target_scope(db: &Connection, owner: &str, target: &Target, scope: u32) -> Result<()> {
    match target {
        Target::Chat(id) => {
            let root = db
                .query_row(
                    "SELECT authority_root FROM _canopy_runtime_chats WHERE chat_id=?1",
                    [id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .ok_or(Error::NotFound)?;
            authorize_root(db, owner, &root, scope)
        }
        Target::File { root, .. } | Target::Task { root, .. } => {
            authorize_root(db, owner, root, scope)
        }
    }
}
fn authorize_target(db: &Connection, owner: &str, target: &Target) -> Result<()> {
    authorize_target_scope(
        db,
        owner,
        target,
        match target {
            Target::Chat(_) => 2,
            Target::File { .. } => 8,
            Target::Task { .. } => 32,
        },
    )
}
fn authorize_binding(db: &Connection, binding: &BeginUpload) -> Result<()> {
    if binding.purpose == Purpose::Result
        && authorize_target_scope(
            db,
            &binding.owner_device,
            &binding.target,
            crate::host::READ,
        )
        .is_ok()
    {
        return Ok(());
    }
    authorize_target(db, &binding.owner_device, &binding.target)
}
fn authorized(db: &Connection, owner: &str, id: &str) -> Result<Upload> {
    let upload = load(db, id)?;
    if upload.binding.owner_device != owner {
        return Err(Error::NotAuthorized);
    }
    if upload.last_active < now() - TTL_SECONDS {
        let retained: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM _canopy_artifact_refs WHERE artifact_id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if !retained {
            return Err(Error::NotFound);
        }
    }
    authorize_binding(db, &upload.binding)?;
    Ok(upload)
}
fn artifact(upload: &Upload) -> Artifact {
    Artifact {
        id: upload
            .artifact_id
            .clone()
            .unwrap_or_else(|| upload.id.clone()),
        size: upload.binding.size,
        sha256: upload.binding.sha256,
        media_type: upload.binding.media_type.clone(),
    }
}

/// Call inside the accepting Send/operation transaction, before committing its receipt.
/// Retaining separately from acceptance is not sufficient. Only validated ready IDs enter inbox.
pub(crate) fn retain_for_chat(
    tx: &Connection,
    owner: &str,
    artifact_id: &str,
    chat: &str,
) -> Result<()> {
    let upload = authorized(tx, owner, artifact_id)?;
    if upload.artifact_id.is_none()
        || upload.binding.target != Target::Chat(chat.into())
        || upload.binding.purpose != Purpose::Image
    {
        return Err(Error::ResourceConflict);
    }
    tx.execute("INSERT OR IGNORE INTO _canopy_artifact_refs(artifact_id,owner_kind,owner_id) VALUES(?1,'chat',?2)",params![artifact_id,chat])?;
    Ok(())
}
/// An edit may keep an accepted image after its original sender is revoked.
/// New references still require publication, ownership and this exact chat target.
pub(crate) fn retain_for_queue_edit(
    tx: &Connection,
    owner: &str,
    chat: &str,
    previous: &[String],
    next: &[String],
) -> Result<()> {
    for id in next {
        if previous.contains(id) {
            let upload = load(tx, id)?;
            let retained:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _canopy_artifact_refs WHERE artifact_id=?1 AND owner_kind='chat' AND owner_id=?2)",params![id,chat],|r|r.get(0))?;
            if !retained
                || upload.artifact_id.is_none()
                || upload.binding.purpose != Purpose::Image
                || upload.binding.size > MAX_IMAGE_BYTES
            {
                return Err(Error::ResourceConflict);
            }
        } else {
            retain_for_chat(tx, owner, id, chat)?;
        }
    }
    Ok(())
}
impl Database {
    /// Local transcript import publishes only a real live chat's image.
    pub fn retain_local_chat_artifact(&mut self, chat: String, id: String) -> Result<()> {
        host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _canopy_runtime_chats WHERE chat_id=?1)",
            [&chat],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::NotFound);
        }
        retain_for_chat(&tx, LOCAL_PRINCIPAL, &id, &chat)?;
        tx.commit()?;
        Ok(())
    }
    pub fn retained_artifact_image(&mut self, chat: String, id: String) -> Result<Artifact> {
        host(self)?;
        let retained:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM _canopy_artifact_refs r JOIN _canopy_runtime_chats c ON c.chat_id=r.owner_id JOIN _canopy_artifacts a ON a.artifact_id=r.artifact_id WHERE r.owner_kind='chat' AND r.owner_id=?1 AND r.artifact_id=?2)",params![chat,id],|r|r.get(0))?;
        if !retained {
            return Err(Error::NotFound);
        }
        let upload = load(&self.db, &id)?;
        if upload.artifact_id.is_none()
            || upload.binding.purpose != Purpose::Image
            || upload.binding.size > MAX_IMAGE_BYTES
        {
            return Err(Error::ResourceConflict);
        }
        Ok(artifact(&upload))
    }
    pub fn begin_artifact_upload(&mut self, id: String, binding: BeginUpload) -> Result<Upload> {
        host(self)?;
        binding.validate()?;
        if !valid_id(&id) {
            return Err(Error::InvalidRuntimeState);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        authorize_binding(&tx, &binding)?;
        let (count,bytes):(i64,i64)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(expected_size),0) FROM _canopy_uploads WHERE artifact_id IS NULL",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if count >= 4 || bytes + binding.size as i64 > MAX_STORAGE_BYTES as i64 {
            return Err(Error::ResourceLimit);
        }
        tx.execute("INSERT INTO _canopy_uploads(upload_id,device_id,target,purpose,expected_size,expected_hash,media_type,ranges,last_active) VALUES(?1,?2,?3,?4,?5,?6,?7,'[]',?8)",params![id,binding.owner_device,json(&binding.target)?,json(&binding.purpose)?,binding.size as i64,binding.sha256.as_slice(),binding.media_type,now()])?;
        let upload = load(&tx, &id)?;
        tx.commit()?;
        Ok(upload)
    }
    pub fn artifact_upload_status(&mut self, owner: String, id: String) -> Result<Upload> {
        host(self)?;
        authorized(&self.db, &owner, &id)
    }
    pub fn record_artifact_chunk(
        &mut self,
        owner: String,
        id: String,
        ranges: Vec<(u64, u64)>,
    ) -> Result<Upload> {
        host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old = authorized(&tx, &owner, &id)?;
        if old.artifact_id.is_some()
            || ranges.len() > 4096
            || ranges.iter().any(|&(a, b)| a >= b || b > old.binding.size)
            || ranges.windows(2).any(|w| w[0].1 >= w[1].0)
            || old
                .ranges
                .iter()
                .any(|&(a, b)| !ranges.iter().any(|&(c, d)| c <= a && d >= b))
        {
            return Err(Error::ResourceConflict);
        }
        tx.execute(
            "UPDATE _canopy_uploads SET ranges=?2,last_active=?3 WHERE upload_id=?1",
            params![id, json(&ranges)?, now()],
        )?;
        let upload = load(&tx, &id)?;
        tx.commit()?;
        Ok(upload)
    }
    pub fn publish_artifact_upload(&mut self, owner: String, id: String) -> Result<Artifact> {
        host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // Delete/revoke use this same writer/transaction serialization point.
        let upload = authorized(&tx, &owner, &id)?;
        if upload.artifact_id.is_some() {
            return Ok(artifact(&upload));
        }
        if upload.binding.size > 0 && upload.ranges != vec![(0, upload.binding.size)] {
            return Err(Error::OperationPending);
        }
        let count:i64=tx.query_row("SELECT COUNT(*) FROM _canopy_artifacts a WHERE NOT EXISTS(SELECT 1 FROM _canopy_artifact_refs r WHERE r.artifact_id=a.artifact_id)",[],|r|r.get(0))?;
        if count >= 4096 {
            return Err(Error::ResourceLimit);
        }
        let used:i64=tx.query_row("SELECT COALESCE(SUM(size),0) FROM _canopy_artifacts a WHERE NOT EXISTS(SELECT 1 FROM _canopy_artifact_refs r WHERE r.artifact_id=a.artifact_id)",[],|r|r.get(0))?;
        if used + upload.binding.size as i64 > MAX_STORAGE_BYTES as i64 {
            return Err(Error::ResourceLimit);
        }
        tx.execute("INSERT INTO _canopy_artifacts(artifact_id,content_hash,size,media_type,relative_path,created_at) VALUES(?1,?2,?3,?4,?1,?5)",params![id,upload.binding.sha256.as_slice(),upload.binding.size as i64,upload.binding.media_type,now()])?;
        tx.execute(
            "UPDATE _canopy_uploads SET artifact_id=?1,last_active=?2 WHERE upload_id=?1",
            params![id, now()],
        )?;
        tx.commit()?;
        Ok(artifact(&upload))
    }
    pub fn cancel_artifact_upload(&mut self, owner: String, id: String) -> Result<()> {
        host(self)?;
        // Cancellation is allowed after revoke and expiry; ownership still required.
        let upload = match load(&self.db, &id) {
            Ok(v) => v,
            Err(Error::NotFound) => return Ok(()),
            Err(e) => return Err(e),
        };
        if upload.binding.owner_device != owner {
            return Err(Error::NotAuthorized);
        }
        if upload.artifact_id.is_some() {
            return Err(Error::ResourceConflict);
        }
        self.db
            .execute("DELETE FROM _canopy_uploads WHERE upload_id=?1", [id])?;
        Ok(())
    }
    pub fn authorize_artifact_download(
        &mut self,
        owner: String,
        id: String,
    ) -> Result<(Artifact, String)> {
        host(self)?;
        let upload = load(&self.db, &id)?;
        if upload.artifact_id.is_none() {
            return Err(Error::NotFound);
        }
        let mut permitted = false;
        let refs=self.db.prepare("SELECT owner_id FROM _canopy_artifact_refs WHERE artifact_id=?1 AND owner_kind='chat'")?.query_map([&id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for chat in refs {
            if authorize_target_scope(&self.db, &owner, &Target::Chat(chat), 1).is_ok() {
                permitted = true;
                break;
            }
        }
        if !permitted {
            if upload.binding.owner_device != owner || upload.last_active < now() - TTL_SECONDS {
                return Err(Error::NotAuthorized);
            }
            let scope = if matches!(upload.binding.target, Target::Task { .. })
                && upload.binding.purpose != Purpose::Result
            {
                32
            } else {
                1
            };
            if authorize_target_scope(&self.db, &owner, &upload.binding.target, scope).is_err() {
                authorize_target(&self.db, &owner, &upload.binding.target)?;
            }
        }
        self.db.execute(
            "UPDATE _canopy_uploads SET last_active=?2 WHERE upload_id=?1",
            params![id, now()],
        )?;
        Ok((artifact(&upload), id))
    }
    pub fn collect_artifact_storage(&mut self) -> Result<Vec<String>> {
        host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM _canopy_uploads WHERE artifact_id IS NULL AND (last_active<?1 OR (device_id<>'local' AND NOT EXISTS(SELECT 1 FROM _canopy_remote_devices d WHERE d.device_id=_canopy_uploads.device_id AND d.revoked_at IS NULL)))",[now()-TTL_SECONDS])?;
        tx.execute("DELETE FROM _canopy_artifacts WHERE NOT EXISTS(SELECT 1 FROM _canopy_artifact_refs r WHERE r.artifact_id=_canopy_artifacts.artifact_id) AND (NOT EXISTS(SELECT 1 FROM _canopy_uploads u WHERE u.artifact_id=_canopy_artifacts.artifact_id AND u.last_active>=?1) OR EXISTS(SELECT 1 FROM _canopy_uploads u JOIN _canopy_remote_devices d ON d.device_id=u.device_id WHERE u.artifact_id=_canopy_artifacts.artifact_id AND d.revoked_at IS NOT NULL) OR EXISTS(SELECT 1 FROM _canopy_uploads u WHERE u.artifact_id=_canopy_artifacts.artifact_id AND json_extract(u.target,'$.Chat') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM _canopy_runtime_chats c WHERE c.chat_id=json_extract(u.target,'$.Chat'))))",[now()-TTL_SECONDS])?;
        tx.execute("DELETE FROM _canopy_uploads WHERE artifact_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM _canopy_artifacts a WHERE a.artifact_id=_canopy_uploads.artifact_id)",[])?;
        let live=tx.prepare("SELECT upload_id FROM _canopy_uploads UNION SELECT relative_path FROM _canopy_artifacts")?.query_map([],|r|r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        tx.commit()?;
        Ok(live)
    }
}
