//! Host file identity and persisted warnings; never buffer text.
use super::{CommandInput, CommandKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileIdentity {
    root: String,
    relative_path: String,
}
impl FileIdentity {
    /// The host supplies its canonical absolute root. Relative paths are lexical
    /// identities and remain usable after deletion; I/O must separately resolve
    /// symlinks/reparse points under the authorized root.
    pub fn new(root: String, relative_path: String) -> Option<Self> {
        let path = Path::new(&root);
        if root.len() > 4096
            || root.contains('\0')
            || !path.is_absolute()
            || path.components().collect::<std::path::PathBuf>().to_str() != Some(root.as_str())
            || path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
            || relative_path.is_empty()
            || relative_path.len() > 4096
            || relative_path.contains(['\0', '\\'])
            || Path::new(&relative_path)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return None;
        }
        let normalized = Path::new(&relative_path)
            .components()
            .map(|c| c.as_os_str().to_str())
            .collect::<Option<Vec<_>>>()?
            .join("/");
        if normalized != relative_path {
            return None;
        }
        Some(Self {
            root,
            relative_path,
        })
    }
    pub fn root(&self) -> &str {
        &self.root
    }
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
    pub(crate) fn resource(&self) -> String {
        // A pair encoding prevents ambiguous root/path delimiter collisions.
        serde_json::to_string(&(self.root(), self.relative_path())).expect("string serialization")
    }
    pub fn presence_command(
        &self,
        key: CommandKey,
        base_hash: [u8; 32],
        dirty: bool,
    ) -> Option<CommandInput> {
        let arguments = serde_json::to_vec(&(self, base_hash, dirty)).ok()?;
        let fingerprint = Sha256::digest(&arguments).to_vec();
        CommandInput::new(
            key,
            "files.presence".into(),
            self.resource(),
            arguments,
            fingerprint,
        )?
        .with_root(self.root.clone())
    }
    pub fn dismiss_presence_command(
        &self,
        key: CommandKey,
        device: &str,
        revision: u64,
    ) -> Option<CommandInput> {
        if device != super::LOCAL_PRINCIPAL && !super::valid_id(device) {
            return None;
        }
        let arguments = serde_json::to_vec(&(self, device, revision)).ok()?;
        let fingerprint = Sha256::digest(&arguments).to_vec();
        CommandInput::new(
            key,
            "files.dismiss_presence".into(),
            self.resource(),
            arguments,
            fingerprint,
        )?
        .with_root(self.root.clone())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtyPresence {
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) base_hash: [u8; 32],
    pub(crate) last_seen: i64,
    pub(crate) revision: u64,
}
impl DirtyPresence {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn device_name(&self) -> &str {
        &self.device_name
    }
    pub fn base_hash(&self) -> &[u8; 32] {
        &self.base_hash
    }
    /// Host Unix timestamp in seconds; does not imply the client is connected.
    pub fn last_seen(&self) -> i64 {
        self.last_seen
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
}
