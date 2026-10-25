use super::*;
use crate::host::DeviceGrant;

fn grant(db: &Connection, id: &str) -> Result<DeviceGrant> {
    let row = db.query_row("SELECT device_id,name,scopes,roots,command_epoch,authorization_revision,revoked_at IS NOT NULL FROM _canopy_remote_devices WHERE device_id=?1", [id], |r| {
        Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u32>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,bool>(6)?))
    }).optional()?.ok_or(Error::NotAuthorized)?;
    Ok(DeviceGrant {
        id: row.0,
        name: row.1,
        scopes: row.2,
        roots: serde_json::from_str(&row.3).map_err(|_| Error::InvalidRuntimeState)?,
        epoch: unsigned(row.4)?,
        authorization_revision: unsigned(row.5)?,
        revoked: row.6,
    })
}

impl Database {
    /// Only the TLS transport's issuer-verified leaf DER may be supplied here.
    /// A claimed device ID or name is never an authentication credential.
    pub fn lookup_runtime_device(&self, leaf: Vec<u8>) -> Result<DeviceGrant> {
        require_host(self)?;
        if leaf.is_empty() || leaf.len() > 16 * 1024 {
            return Err(Error::NotAuthorized);
        }
        let id: String = self.db.query_row("SELECT device_id FROM _canopy_remote_devices WHERE public_credential=?1 AND revoked_at IS NULL", [leaf], |r| r.get(0)).optional()?.ok_or(Error::NotAuthorized)?;
        grant(&self.db, &id)
    }
    /// Trusted host management only; a remote caller must first authorize ADMIN.
    pub fn runtime_devices(&self) -> Result<Vec<DeviceGrant>> {
        require_host(self)?;
        let ids = self
            .db
            .prepare("SELECT device_id FROM _canopy_remote_devices ORDER BY device_id")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| grant(&self.db, id)).collect()
    }
    /// Bounded trusted-host metadata page. A valid grant can contain 512 KiB of
    /// roots, so a record-count limit alone cannot bound its transport payload.
    pub fn runtime_devices_page(
        &self,
        after: Option<String>,
        include_revoked: bool,
    ) -> Result<(Vec<DeviceGrant>, Option<String>)> {
        require_host(self)?;
        if after.as_ref().is_some_and(|id| !host::valid_id(id)) {
            return Err(Error::InvalidRuntimeState);
        }
        let mut query = self.db.prepare("SELECT device_id FROM _canopy_remote_devices WHERE (?1 IS NULL OR device_id>?1) AND (?2 OR revoked_at IS NULL) ORDER BY device_id LIMIT 33")?;
        let ids = query.query_map(params![after, include_revoked], |r| r.get::<_, String>(0))?;
        let mut rows: Vec<DeviceGrant> = Vec::new();
        let mut bytes = 0usize;
        for id in ids {
            let current = grant(&self.db, &id?)?;
            let size = current
                .roots
                .iter()
                .map(|root| root.len() + 8)
                .sum::<usize>()
                + current.name.len()
                + 1024;
            if rows.len() == 32 || bytes + size > 768 * 1024 {
                let next = rows.last().map(|row| row.id.clone());
                return Ok((rows, next));
            }
            bytes += size;
            rows.push(current);
        }
        Ok((rows, None))
    }
    /// Device ID must come from lookup_runtime_device, never request metadata.
    pub fn authorize_runtime_read(
        &self,
        device: String,
        scope: u32,
        root: String,
    ) -> Result<DeviceGrant> {
        require_host(self)?;
        if !host::valid_id(&device) || scope == 0 || scope & !127 != 0 || root.is_empty() {
            return Err(Error::NotAuthorized);
        }
        let current = grant(&self.db, &device)?;
        if current.revoked
            || current.scopes & scope != scope
            || !current.roots.iter().any(|r| r == "*" || r == &root)
        {
            return Err(Error::NotAuthorized);
        }
        Ok(current)
    }
    /// Local host management; effects already attempting are not reported undone.
    pub fn update_runtime_device_authorization(
        &mut self,
        device: String,
        revision: u64,
        scopes: u32,
        roots: Vec<String>,
    ) -> Result<DeviceGrant> {
        require_host(self)?;
        if scopes & !127 != 0
            || roots.len() > 128
            || roots.iter().any(|r| r.is_empty() || r.len() > 4096)
        {
            return Err(Error::InvalidRuntimeState);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let changed = tx.execute("UPDATE _canopy_remote_devices SET scopes=?3,roots=?4,authorization_revision=authorization_revision+1 WHERE device_id=?1 AND authorization_revision=?2 AND revoked_at IS NULL", params![device,integer(revision)?,scopes,serde_json::to_string(&roots).map_err(|_| Error::InvalidRuntimeState)?])?;
        if changed == 0 {
            return Err(Error::ResourceConflict);
        }
        let pending = tx.prepare("SELECT command_id,command_epoch FROM _canopy_command_inbox WHERE device_id=?1 AND state='accepted'")?.query_map([&device], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, epoch) in pending {
            let key = CommandKey {
                device: device.clone(),
                id,
                epoch: unsigned(epoch)?,
            };
            if matches!(authorize_saved(&tx, &key, true), Err(Error::NotAuthorized)) {
                tx.execute("UPDATE _canopy_command_inbox SET state='rejected',updated_at=?3 WHERE device_id=?1 AND command_id=?2", params![device,key.id,now()])?;
            }
        }
        let current = grant(&tx, &device)?;
        tx.commit()?;
        Ok(current)
    }
    pub fn set_runtime_identity_ref(
        &mut self,
        expected: Option<String>,
        new: String,
    ) -> Result<RuntimeHost> {
        require_host(self)?;
        if !host::valid_id(&new) || expected.as_ref().is_some_and(|id| !host::valid_id(id)) {
            return Err(Error::InvalidRuntimeState);
        }
        let changed = self.db.execute("UPDATE _canopy_runtime_host SET tls_identity_ref=?2 WHERE id=1 AND tls_identity_ref IS ?1", params![expected,new])?;
        if changed == 0 {
            return Err(Error::ResourceConflict);
        }
        self.runtime_host()
    }
}

#[cfg(test)]
mod paging_tests {
    use super::*;
    use prost::Message;
    #[test]
    fn valid_large_grants_and_revoked_history_are_readable_in_bounded_pages() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("db");
        crate::settings::database::create(&path).unwrap();
        let mut db = Database::open(&path, Access::ReadWrite).unwrap();
        db.initialize_runtime().unwrap();
        let roots = (0..128).map(|n| format!("/{n:0>4095}")).collect::<Vec<_>>();
        let mut ids = Vec::new();
        for n in 1..=3 {
            let id = uuid::Uuid::new_v4().to_string();
            db.register_runtime_device(
                DeviceRegistration::new(
                    id.clone(),
                    "phone".into(),
                    vec![n; 32],
                    host::READ,
                    roots.clone(),
                )
                .unwrap(),
            )
            .unwrap();
            ids.push(id);
        }
        db.revoke_runtime_device(ids[1].clone()).unwrap();
        let mut cursor = None;
        let mut found = std::collections::HashSet::new();
        let mut pages = 0;
        loop {
            let (rows, next) = db.runtime_devices_page(cursor.clone(), true).unwrap();
            let page = crate::remote::proto::Devices {
                host_id: "host".into(),
                devices: rows
                    .iter()
                    .map(|d| crate::remote::proto::Device {
                        device_id: d.id().into(),
                        name: d.name().into(),
                        scopes: vec!["read".into()],
                        roots: d.roots().into(),
                        revoked: d.revoked(),
                    })
                    .collect(),
                next_page_token: next.clone(),
            };
            assert!(page.encoded_len() <= host::CONTROL_BYTES);
            for row in rows {
                assert!(found.insert(row.id().to_owned()));
            }
            pages += 1;
            if next.is_none() {
                break;
            }
            assert_ne!(cursor, next);
            cursor = next;
        }
        assert_eq!(pages, 3);
        assert_eq!(found, ids.into_iter().collect());
        let (active, next) = db.runtime_devices_page(None, false).unwrap();
        assert_eq!(active.len(), 1);
        let (rest, final_cursor) = db.runtime_devices_page(next, false).unwrap();
        assert_eq!(rest.len(), 1);
        assert!(!active[0].revoked() && !rest[0].revoked());
        assert!(final_cursor.is_none());
    }
}
