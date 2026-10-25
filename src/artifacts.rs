//! Bounded private bulk storage. Invoke on a background executor, never the UI thread.
//! One host-owned store serializes file operations; SQLite remains owned by SettingsClient.
use crate::settings::{Error, Result, SettingsClient};
use futures_lite::future::block_on;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_STORAGE_BYTES: u64 = 256 * 1024 * 1024;
pub const TTL_SECONDS: i64 = 3600;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Target {
    Chat(String),
    /// Canonical granted workspace root and a workspace-relative destination.
    /// SaveFile must check its base condition when applying, not during upload.
    File {
        root: String,
        path: String,
    },
    Task {
        integration_id: String,
        project_id: String,
        task_id: String,
        /// Canonical checkout root, or `private` for unscoped host Tasks.
        root: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Purpose {
    Image,
    FileSave,
    TaskAttachment,
    Result,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BeginUpload {
    pub(crate) owner_device: String,
    pub(crate) target: Target,
    pub(crate) purpose: Purpose,
    pub(crate) size: u64,
    pub(crate) sha256: [u8; 32],
    pub(crate) media_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Upload {
    pub(crate) id: String,
    pub(crate) binding: BeginUpload,
    /// Confirmed, nonoverlapping byte intervals [start,end).
    pub(crate) ranges: Vec<(u64, u64)>,
    pub(crate) artifact_id: Option<String>,
    pub(crate) last_active: i64,
}
#[derive(Clone, Debug)]
pub struct Artifact {
    pub(crate) id: String,
    pub(crate) size: u64,
    pub(crate) sha256: [u8; 32],
    pub(crate) media_type: String,
}
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |v| v.as_secs() as i64)
}
pub(crate) fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id)
}
impl BeginUpload {
    pub fn new(
        owner_device: String,
        target: Target,
        purpose: Purpose,
        size: u64,
        sha256: [u8; 32],
        media_type: String,
    ) -> Result<Self> {
        let binding = Self {
            owner_device,
            target,
            purpose,
            size,
            sha256,
            media_type,
        };
        binding.validate()?;
        Ok(binding)
    }
    pub fn owner_device(&self) -> &str {
        &self.owner_device
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn purpose(&self) -> &Purpose {
        &self.purpose
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let target_ok = match &self.target {
            Target::Chat(id) => valid_id(id),
            Target::File { root, path } => {
                root.len() <= 16384
                    && path.len() <= 16384
                    && Path::new(root).is_absolute()
                    && !path.is_empty()
                    && Path::new(path)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
            }
            Target::Task {
                integration_id,
                project_id,
                task_id,
                root,
            } => {
                !integration_id.is_empty()
                    && integration_id.len() <= 256
                    && project_id.len() <= 4096
                    && !task_id.is_empty()
                    && task_id.len() <= 4096
                    && root.len() <= 4096
                    && (root == "private" || Path::new(root).is_absolute())
            }
        };
        if !target_ok
            || self.owner_device.is_empty()
            || self.owner_device.len() > 128
            || self.media_type.len() > 256
            || self.size > MAX_STORAGE_BYTES
            || (self.purpose == Purpose::Image && self.size > MAX_IMAGE_BYTES)
            || (self.purpose == Purpose::FileSave && !matches!(self.target, Target::File { .. }))
            || (self.purpose == Purpose::TaskAttachment
                && !matches!(self.target, Target::Task { .. }))
        {
            return Err(Error::InvalidRuntimeState);
        }
        Ok(())
    }
}

impl Upload {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn binding(&self) -> &BeginUpload {
        &self.binding
    }
    pub fn ranges(&self) -> &[(u64, u64)] {
        &self.ranges
    }
    pub fn artifact_id(&self) -> Option<&str> {
        self.artifact_id.as_deref()
    }
    pub fn last_active(&self) -> i64 {
        self.last_active
    }
}
impl Artifact {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
}

pub struct ArtifactStore {
    client: SettingsClient,
    directory: PathBuf,
    gate: Mutex<()>,
    _lease: File,
}
impl ArtifactStore {
    /// `directory` is host configuration, never a path supplied by an RPC caller.
    pub fn open(client: SettingsClient, directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        if let Some(parent) = directory.parent() {
            File::open(parent)?.sync_all()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        let lease = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("store.lock"))?;
        lease.try_lock().map_err(|_| Error::RuntimeInUse)?;
        let store = Self {
            client,
            directory,
            gate: Mutex::new(()),
            _lease: lease,
        };
        store.cleanup()?;
        Ok(store)
    }
    fn path(&self, id: &str) -> Result<PathBuf> {
        if !valid_id(id) {
            return Err(Error::NotFound);
        }
        Ok(self.directory.join(id))
    }
    fn sync_directory(&self) -> Result<()> {
        #[cfg(unix)]
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
    pub fn begin(&self, binding: BeginUpload) -> Result<Upload> {
        self.begin_with_id(uuid::Uuid::new_v4().to_string(), binding)
    }
    /// A command-bound upload ID makes Begin safe to reconcile after an RPC or
    /// host crash. The same ID can never change owner, target, bytes or purpose.
    pub fn begin_with_id(&self, id: String, binding: BeginUpload) -> Result<Upload> {
        let _guard = self.gate.lock().map_err(|_| Error::WorkerStopped)?;
        binding.validate()?;
        if !valid_id(&id) {
            return Err(Error::InvalidRuntimeState);
        }
        match block_on(
            self.client
                .artifact_upload_status(binding.owner_device.clone(), id.clone()),
        ) {
            Ok(upload) => {
                return if upload.binding == binding {
                    Ok(upload)
                } else {
                    Err(Error::CommandConflict)
                };
            }
            Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        self.cleanup_locked()?;
        let mut options = OpenOptions::new();
        options.create_new(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = match options.open(self.path(&id)?) {
            Ok(file) => file,
            // Begin creates no content before the metadata row. An empty orphan
            // from an interrupted Begin can be adopted, never a populated file.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(self.path(&id)?)?;
                if !file.metadata()?.is_file() || file.metadata()?.len() != 0 {
                    return Err(Error::ResourceConflict);
                }
                file
            }
            Err(error) => return Err(error.into()),
        };
        file.sync_all()?;
        self.sync_directory()?;
        match block_on(self.client.begin_artifact_upload(id.clone(), binding)) {
            Ok(upload) => Ok(upload),
            Err(e) => {
                let _ = fs::remove_file(self.path(&id)?);
                Err(e)
            }
        }
    }
    pub fn status(&self, owner: &str, id: &str) -> Result<Upload> {
        block_on(
            self.client
                .artifact_upload_status(owner.to_owned(), id.to_owned()),
        )
    }
    pub fn put_chunk(&self, owner: &str, id: &str, offset: u64, bytes: &[u8]) -> Result<Upload> {
        let _guard = self.gate.lock().map_err(|_| Error::WorkerStopped)?;
        let mut upload = self.status(owner, id)?;
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or(Error::ResourceLimit)?;
        if bytes.is_empty()
            || bytes.len() > CHUNK_BYTES
            || end > upload.binding.size
            || upload.artifact_id.is_some()
        {
            return Err(Error::ResourceConflict);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.path(id)?)?;
        for &(start, stop) in &upload.ranges {
            let lo = start.max(offset);
            let hi = stop.min(end);
            if lo < hi {
                let mut old = vec![0; (hi - lo) as usize];
                file.seek(SeekFrom::Start(lo))?;
                file.read_exact(&mut old)?;
                if old != bytes[(lo - offset) as usize..(hi - offset) as usize] {
                    return Err(Error::ResourceConflict);
                }
            }
        }
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        upload.ranges.push((offset, end));
        upload.ranges.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::new();
        for (a, b) in upload.ranges {
            if let Some(last) = merged.last_mut().filter(|last| a <= last.1) {
                last.1 = last.1.max(b);
            } else {
                merged.push((a, b));
            }
        }
        // Bytes reach durable storage before their acknowledgement becomes durable.
        block_on(
            self.client
                .record_artifact_chunk(owner.to_owned(), id.to_owned(), merged),
        )
    }
    pub fn finish(&self, owner: &str, id: &str) -> Result<Artifact> {
        let _guard = self.gate.lock().map_err(|_| Error::WorkerStopped)?;
        let upload = self.status(owner, id)?;
        if upload.binding.size > 0 && upload.ranges != vec![(0, upload.binding.size)] {
            return Err(Error::OperationPending);
        }
        let mut file = File::open(self.path(id)?)?;
        if file.metadata()?.len() != upload.binding.size {
            return Err(Error::ResourceConflict);
        }
        let mut hash = Sha256::new();
        let mut buffer = [0; CHUNK_BYTES];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        let digest: [u8; 32] = hash.finalize().into();
        if digest != upload.binding.sha256 {
            return Err(Error::ResourceConflict);
        }
        // Same private filename before/after publication: no rename/DB crash window.
        block_on(
            self.client
                .publish_artifact_upload(owner.to_owned(), id.to_owned()),
        )
    }
    pub fn cancel(&self, owner: &str, id: &str) -> Result<()> {
        let _guard = self.gate.lock().map_err(|_| Error::WorkerStopped)?;
        block_on(
            self.client
                .cancel_artifact_upload(owner.to_owned(), id.to_owned()),
        )?;
        self.cleanup_locked()
    }
    /// Trusted host projection; callers authorize the chat before returning metadata.
    pub fn retained_image_metadata(&self, chat: &str, id: &str) -> Result<Artifact> {
        block_on(self.client.retained_artifact_image(chat.into(), id.into()))
    }
    /// Import an actual local transcript attachment. `source_dir` is the host-owned
    /// chat-attachments root; neither it nor `chat` may come from a client path.
    /// The content-bound ID deduplicates repeated transcript projections.
    pub fn import_chat_image(
        &self,
        chat: &str,
        filename: &str,
        source_dir: &Path,
    ) -> Result<Artifact> {
        if !valid_id(chat) {
            return Err(Error::InvalidRuntimeState);
        }
        let (stem, extension) = filename
            .rsplit_once('.')
            .ok_or(Error::InvalidRuntimeState)?;
        if !valid_id(stem) {
            return Err(Error::InvalidRuntimeState);
        }
        let media = match extension {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => return Err(Error::InvalidRuntimeState),
        };
        let root = source_dir.canonicalize()?;
        let directory = root.join(chat);
        if fs::symlink_metadata(&directory)?.file_type().is_symlink()
            || directory.canonicalize()? != directory
        {
            return Err(Error::NotAuthorized);
        }
        let path = directory.join(filename);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
            return Err(Error::ResourceLimit);
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(&path)?;
        let mut bytes = Vec::new();
        file.take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(Error::ResourceLimit);
        }
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let identity = serde_json::to_vec(&("local-chat-image-v1", chat, filename, digest))
            .map_err(|_| Error::InvalidRuntimeState)?;
        let hash = Sha256::digest(identity);
        let mut id = [0; 16];
        id.copy_from_slice(&hash[..16]);
        let id = uuid::Uuid::from_bytes(id).to_string();
        let binding = BeginUpload::new(
            crate::host::LOCAL_PRINCIPAL.into(),
            Target::Chat(chat.into()),
            Purpose::Image,
            bytes.len() as u64,
            digest,
            media.into(),
        )?;
        let upload = self.begin_with_id(id.clone(), binding)?;
        if upload.artifact_id().is_none() {
            for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
                self.put_chunk(
                    crate::host::LOCAL_PRINCIPAL,
                    &id,
                    (index * CHUNK_BYTES) as u64,
                    chunk,
                )?;
            }
        }
        let artifact = self.finish(crate::host::LOCAL_PRINCIPAL, &id)?;
        block_on(self.client.retain_local_chat_artifact(chat.into(), id))?;
        Ok(artifact)
    }
    /// Host dispatch reads an already accepted image. Sender revocation cannot
    /// revoke the host-owned message. Call on a background executor, then pass
    /// bytes through the existing image normalization before provider dispatch.
    pub fn read_retained_image(&self, chat: &str, id: &str) -> Result<Vec<u8>> {
        let _guard = self.gate.lock().map_err(|_| Error::WorkerStopped)?;
        let artifact = block_on(self.client.retained_artifact_image(chat.into(), id.into()))?;
        let file = File::open(self.path(id)?)?;
        if file.metadata()?.len() != artifact.size {
            return Err(Error::ResourceConflict);
        }
        let mut bytes = Vec::with_capacity(artifact.size as usize);
        // Bound even if another local process unexpectedly modifies the file.
        file.take(artifact.size + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != artifact.size
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != artifact.sha256
        {
            return Err(Error::ResourceConflict);
        }
        Ok(bytes)
    }
    pub fn download(
        &self,
        owner: &str,
        artifact: &str,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>> {
        let _guard = self.gate.lock().map_err(|_| Error::WorkerStopped)?;
        if length > CHUNK_BYTES {
            return Err(Error::ResourceLimit);
        }
        let (metadata, path) = block_on(
            self.client
                .authorize_artifact_download(owner.to_owned(), artifact.to_owned()),
        )?;
        if offset > metadata.size {
            return Err(Error::ResourceConflict);
        }
        let mut file = File::open(self.path(&path)?)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; length.min((metadata.size - offset) as usize)];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    /// Host maintenance invokes periodically and after Delete/revoke. Also runs at startup/Begin.
    pub fn cleanup(&self) -> Result<()> {
        let _guard = self.gate.lock().map_err(|_| Error::WorkerStopped)?;
        self.cleanup_locked()
    }
    fn cleanup_locked(&self) -> Result<()> {
        let live = block_on(self.client.collect_artifact_storage())?;
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if valid_id(name) && !live.iter().any(|id| id == name) {
                fs::remove_file(entry.path())?;
            }
        }
        self.sync_directory()
    }
}
