//! Device-local records; keys and enrollment tokens never enter this file.
use super::HostId;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct HostRecord {
    #[serde(default)]
    pub(crate) mutual: bool,
    #[serde(default)]
    pub(crate) passive_peer: bool,
    #[serde(default = "auto_connect_default")]
    pub(crate) auto_connect: bool,
    pub(crate) id: HostId,
    pub(crate) name: String,
    pub(crate) endpoint: String,
    pub(crate) server_name: String,
    pub(crate) certificate: Vec<u8>,
    pub(crate) fingerprint: Vec<u8>,
    pub(crate) device_id: String,
    pub(crate) credential_ref: String,
    pub(crate) ssh: Option<SshRoute>,
    #[serde(default)]
    pub(crate) routes: Vec<ConnectionRoute>,
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
fn auto_connect_default() -> bool {
    true
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
    pub fn routes(&self) -> &[ConnectionRoute] {
        &self.routes
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
    pub fn auto_connect(&self) -> bool {
        self.auto_connect
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshRoute {
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) fingerprint: String,
    pub(crate) credential_ref: String,
}
impl SshRoute {
    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn username(&self) -> &str {
        &self.username
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
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
/// A device-local way to reach an already paired identity. Order is preference;
/// every route uses the host's existing certificate pin and client credential.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionRoute {
    id: String,
    name: String,
    endpoint: String,
    ssh: Option<SshRoute>,
}
impl ConnectionRoute {
    pub fn new(name: String, endpoint: String, ssh: Option<SshRoute>) -> Result<Self, String> {
        let route = Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            endpoint,
            ssh,
        };
        route.validate()?;
        Ok(route)
    }
    pub fn edited(
        &self,
        name: String,
        endpoint: String,
        ssh: Option<SshRoute>,
    ) -> Result<Self, String> {
        let route = Self {
            id: self.id.clone(),
            name,
            endpoint,
            ssh,
        };
        route.validate()?;
        Ok(route)
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn ssh(&self) -> Option<&SshRoute> {
        self.ssh.as_ref()
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        if uuid::Uuid::parse_str(&self.id).is_err()
            || self.name.trim().is_empty()
            || self.name.len() > 128
        {
            return Err("Give this route a name of up to 128 bytes.".into());
        }
        let url = url::Url::parse(&self.endpoint)
            .map_err(|_| "Enter a Canopy HTTPS address, for example https://192.168.1.25:44330.")?;
        if self.endpoint.len() > 2048
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.port() == Some(0)
        {
            return Err(
                "Use an HTTPS computer address and port, without a path or credentials.".into(),
            );
        }
        if let Some(ssh) = &self.ssh
            && (ssh.address.trim().is_empty()
                || ssh.address.len() > 253
                || ssh.port == 0
                || ssh.username.trim().is_empty()
                || ssh.username.len() > 128
                || !ssh.fingerprint.starts_with("SHA256:")
                || ssh.fingerprint.len() != 50
                || uuid::Uuid::parse_str(&ssh.credential_ref).is_err())
        {
            return Err("Complete the SSH server, login and verified host key.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct OutboxEntry {
    #[serde(default)]
    pub(crate) catalog_id: Option<String>,
    #[serde(default)]
    pub api_request: Option<Vec<u8>>,
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
    pub api_intent: Option<(String, Vec<u8>)>,
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
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct UploadCommandResult {
    pub receipt: Option<Vec<u8>>,
    pub retired: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct UploadRecord {
    #[serde(default)]
    pub api_begin: Option<(String, Vec<u8>)>,
    #[serde(default)]
    pub api_finish: Option<(String, Vec<u8>)>,
    #[serde(default)]
    pub api_cancel: Option<(String, Vec<u8>)>,
    #[serde(default)]
    pub begin_result: UploadCommandResult,
    #[serde(default)]
    pub finish_result: UploadCommandResult,
    #[serde(default)]
    pub cancel_result: UploadCommandResult,
    #[serde(default)]
    pub bulk_unavailable: bool,
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
                let mut records: Records = serde_json::from_slice(&bytes)
                    .map_err(|_| "Remote client storage is invalid; it has not been replaced.")?;
                if !matches!(records.version, 1 | 2) {
                    return Err("Unsupported remote client storage version.".into());
                }
                // Older builds must refuse, rather than overwrite a route list
                // they cannot retain. The v1 record is upgraded in memory and
                // written atomically on the next successful save.
                records.version = 2;
                records
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Records {
                version: 2,
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
    pub(super) fn committed_host(&self, id: &HostId) -> Result<Option<HostRecord>, String> {
        Ok(Self::open(self.path.clone())?
            .records
            .hosts
            .get(id)
            .cloned())
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
