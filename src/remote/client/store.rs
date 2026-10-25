//! Device-local records; keys and enrollment tokens never enter this file.
use super::HostId;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct HostRecord {
    pub(crate) id: HostId,
    pub(crate) name: String,
    pub(crate) endpoint: String,
    pub(crate) server_name: String,
    pub(crate) certificate: Vec<u8>,
    pub(crate) fingerprint: Vec<u8>,
    pub(crate) device_id: String,
    pub(crate) credential_ref: String,
    pub(crate) ssh: Option<SshRoute>,
    pub(crate) last_seen: Option<i64>,
    pub(crate) epoch: Option<u64>,
    pub(crate) closing_epoch: Option<u64>,
    pub(crate) catalog: Vec<u8>,
    pub(crate) outbox: Vec<OutboxEntry>,
    pub(crate) mutations: Vec<MutationRecord>,
    pub(crate) views: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) uploads: Vec<UploadRecord>,
    #[serde(default)]
    pub(crate) chats: BTreeMap<String, CachedChat>,
}
impl HostRecord {
    pub fn id(&self) -> &HostId {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn last_seen(&self) -> Option<i64> {
        self.last_seen
    }
    pub fn view(&self, key: &str) -> Option<&str> {
        self.views.get(key).map(String::as_str)
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SshRoute {
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) fingerprint: String,
    pub(crate) credential_ref: String,
}
impl SshRoute {
    pub fn new(
        address: String,
        port: u16,
        username: String,
        fingerprint: String,
        credential_ref: String,
    ) -> Self {
        Self {
            address,
            port,
            username,
            fingerprint,
            credential_ref,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct OutboxEntry {
    pub id: String,
    pub request: Vec<u8>,
    pub attempted: bool,
    #[serde(default)]
    pub uploads: Vec<String>,
    pub receipt: Option<Vec<u8>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct MutationRecord {
    #[serde(default)]
    pub chat_intent: Option<(String, Vec<u8>)>,
    pub id: String,
    pub epoch: u64,
    pub receipt: Option<Vec<u8>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CachedChat {
    pub items: Vec<Vec<u8>>,
    pub summary: Option<Vec<u8>>,
    pub queue: Option<Vec<u8>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct UploadRecord {
    pub id: String,
    pub begin: Vec<u8>,
    pub finish: Option<Vec<u8>>,
    pub upload_id: Option<String>,
    pub artifact: Option<Vec<u8>>,
}
#[derive(Default, Serialize, Deserialize)]
struct Records {
    version: u32,
    hosts: BTreeMap<HostId, HostRecord>,
}
pub(crate) struct Store {
    path: PathBuf,
    records: Records,
}
impl Store {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let records = match std::fs::metadata(&path) {
            Ok(meta) => {
                if meta.len() > 64 * 1024 * 1024 {
                    return Err("Remote client storage exceeds 64 MiB.".into());
                }
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let records: Records = serde_json::from_slice(&bytes)
                    .map_err(|_| "Remote client storage is invalid; it has not been replaced.")?;
                if records.version != 1 {
                    return Err("Unsupported remote client storage version.".into());
                }
                records
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Records {
                version: 1,
                ..Default::default()
            },
            Err(e) => return Err(e.to_string()),
        };
        Ok(Self { path, records })
    }
    pub fn attachment_dir(&self) -> PathBuf {
        self.path.parent().unwrap().join("remote-attachments")
    }
    pub fn hosts(&self) -> Vec<HostRecord> {
        self.records.hosts.values().cloned().collect()
    }
    pub fn save(&mut self, host: HostRecord) -> Result<(), String> {
        let old = self.records.hosts.insert(host.id.clone(), host.clone());
        if let Err(error) = self.flush() {
            if let Some(old) = old {
                self.records.hosts.insert(host.id, old);
            } else {
                self.records.hosts.remove(&host.id);
            }
            return Err(error);
        }
        Ok(())
    }
    fn flush(&self) -> Result<(), String> {
        let parent = self.path.parent().ok_or("Invalid client storage path.")?;
        crate::platform::directories::ensure_private_dir(parent)?;
        let bytes = serde_json::to_vec(&self.records).map_err(|e| e.to_string())?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(
                "Remote client storage is full; preserve or remove old drafts first.".into(),
            );
        }
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(&self.path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
/// Injectable in tests. Production implementations never disclose vault errors
/// containing supplied values and never implement Debug for key material.
pub trait CredentialStore: Send + Sync {
    fn load(&self, id: &str) -> Result<String, String>;
    fn save(&self, id: &str, value: &str) -> Result<(), String>;
    fn remove(&self, id: &str) -> Result<(), String>;
}
pub struct SystemCredentials;
impl CredentialStore for SystemCredentials {
    fn load(&self, id: &str) -> Result<String, String> {
        crate::platform::credentials::load("tech.itsol.canopy.remote.client", id)
    }
    fn save(&self, id: &str, value: &str) -> Result<(), String> {
        crate::platform::credentials::store("tech.itsol.canopy.remote.client", id, value)
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        crate::platform::credentials::remove("tech.itsol.canopy.remote.client", id)
    }
}
