use super::*;
use crate::api::{proto, resources as api};
use crate::{
    artifacts::{ArtifactStore, CHUNK_BYTES, Purpose, Target},
    command_bus::{FileCommand, FileContent},
    files,
    git::GitClient,
    host::{FILES_GIT, FileIdentity, READ},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{io::Read, pin::Pin, sync::atomic::AtomicBool};
use tonic::{Request, Response};

#[derive(Clone)]
pub struct FilesRpc {
    auth: RpcAuth,
    roots: WorkspaceRoots,
    git: GitClient,
    artifacts: Arc<ArtifactStore>,
    locks: ResourceLocks,
}
impl FilesRpc {
    pub fn new(
        auth: RpcAuth,
        roots: WorkspaceRoots,
        git: GitClient,
        artifacts: Arc<ArtifactStore>,
        locks: ResourceLocks,
    ) -> Self {
        Self {
            auth,
            roots,
            git,
            artifacts,
            locks,
        }
    }
    async fn file(
        &self,
        grant: &DeviceGrant,
        file: &proto::FileRef,
        scope: u32,
    ) -> Result<(PathBuf, FileIdentity), Status> {
        let root = api::read_root(
            &self.auth,
            &self.roots,
            grant,
            required(file.target.as_ref())?,
            scope,
        )
        .await?;
        let identity = FileIdentity::new(
            WorkspaceRoots::root_string(&root)?.into(),
            file.relative_path.clone(),
        )
        .ok_or_else(|| Status::invalid_argument("Invalid relative file identity."))?;
        if Path::new(identity.relative_path())
            .components()
            .any(|part| part.as_os_str() == ".git")
        {
            return Err(Status::permission_denied(
                "Git metadata is not a file resource.",
            ));
        }
        Ok((root, identity))
    }
    async fn content(
        &self,
        grant: &DeviceGrant,
        identity: &FileIdentity,
        reference: proto::ArtifactRef,
    ) -> Result<String, Status> {
        self.auth.check_host(&reference.host_id)?;
        let store = self.artifacts.clone();
        let owner = grant.id().to_owned();
        let identity = identity.clone();
        blocking(move || {
            let upload = store
                .status(&owner, &reference.artifact_id)
                .map_err(storage_status)?;
            let binding = upload.binding();
            if upload.artifact_id() != Some(reference.artifact_id.as_str())
                || binding.purpose() != &Purpose::FileSave
                || binding.target()
                    != &(Target::File {
                        root: identity.root().into(),
                        path: identity.relative_path().into(),
                    })
                || binding.size() != reference.size
                || binding.sha256().as_slice() != reference.sha256
                || binding.media_type() != reference.media_type
            {
                return Err(Status::aborted(
                    "Artifact binding does not match this file save.",
                ));
            }
            if reference.size > files::MAX_TEXT as u64 {
                return Err(Status::resource_exhausted(
                    "File exceeds the 2 MiB editor limit.",
                ));
            }
            let mut bytes = Vec::with_capacity(reference.size as usize);
            while bytes.len() < reference.size as usize {
                let chunk = store
                    .download(
                        &owner,
                        &reference.artifact_id,
                        bytes.len() as u64,
                        CHUNK_BYTES,
                    )
                    .map_err(storage_status)?;
                if chunk.is_empty() {
                    return Err(Status::aborted(
                        "Artifact ended before its declared length.",
                    ));
                }
                bytes.extend(chunk);
            }
            if Sha256::digest(&bytes).as_slice() != reference.sha256 {
                return Err(Status::aborted("Artifact hash changed."));
            }
            let text = String::from_utf8(bytes)
                .map_err(|_| domain("Only UTF-8 text files are supported."))?;
            let text = text
                .strip_prefix('\u{feff}')
                .unwrap_or(&text)
                .replace("\r\n", "\n");
            if text.contains('\0') || text.bytes().filter(|b| *b == b'\n').count() > 50_000 {
                return Err(domain(
                    "Binary files or files over 50,000 lines cannot be edited.",
                ));
            }
            Ok(text)
        })
        .await
    }
}
#[tonic::async_trait]
impl proto::files_service_server::FilesService for FilesRpc {
    async fn list_directory(
        &self,
        request: Request<proto::ListDirectoryRequest>,
    ) -> Result<Response<proto::DirectoryEntries>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        let file = required(request.directory)?;
        let root = api::read_root(
            &self.auth,
            &self.roots,
            &grant,
            required(file.target.as_ref())?,
            READ,
        )
        .await?;
        if !file.relative_path.is_empty() {
            self.file(&grant, &file, READ).await?;
        }
        let listing = self
            .git
            .file_directory(
                root.clone(),
                file.relative_path.clone().into(),
                files::MAX_DIRECTORY_ENTRIES,
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .map_err(domain)?;
        // Bind continuation to this exact sorted listing, including sizes.
        let entries = blocking(move || {
            listing
                .entries
                .into_iter()
                .map(|entry| directory_entry(&root, &entry.path, entry.directory))
                .collect::<Result<Vec<_>, _>>()
        })
        .await?;
        let digest = Sha256::digest(
            entries
                .iter()
                .flat_map(Message::encode_to_vec)
                .collect::<Vec<_>>(),
        );
        let hash = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let offset = match request.page_token {
            None => 0,
            Some(token) => {
                let (version, offset) = token
                    .split_once(':')
                    .ok_or_else(|| Status::invalid_argument("Invalid directory page token."))?;
                if version != hash {
                    return Err(Status::aborted("Directory changed; reload its first page."));
                }
                offset
                    .parse::<usize>()
                    .map_err(|_| Status::invalid_argument("Invalid directory page offset."))?
            }
        };
        if offset > entries.len() {
            return Err(Status::invalid_argument(
                "Directory page offset is out of range.",
            ));
        }
        let end = (offset + 100).min(entries.len());
        Ok(Response::new(proto::DirectoryEntries {
            target: file.target,
            entries: entries[offset..end].to_vec(),
            next_page_token: (end < entries.len()).then(|| format!("{hash}:{end}")),
            incomplete: listing.warning.is_some(),
            warning: listing.warning,
        }))
    }
    async fn search_files(
        &self,
        request: Request<proto::SearchFilesRequest>,
    ) -> Result<Response<proto::DirectoryEntries>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        let root = api::read_root(
            &self.auth,
            &self.roots,
            &grant,
            required(request.target.as_ref())?,
            READ,
        )
        .await?;
        if request.query.len() > 4096 {
            return Err(Status::invalid_argument("Search query is too long."));
        }
        let (entries, warning) = blocking(move || {
            let found = files::search::find(&root, &request.query, &AtomicBool::new(false))
                .map_err(domain)?;
            let entries = found
                .paths
                .iter()
                .take(if request.limit == 0 {
                    100
                } else {
                    request.limit.min(100) as usize
                })
                .map(|path| directory_entry(&root, path, false))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((entries, found.warning))
        })
        .await?;
        Ok(Response::new(proto::DirectoryEntries {
            target: request.target,
            entries,
            next_page_token: None,
            incomplete: warning.is_some(),
            warning,
        }))
    }
    type ReadFileStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<proto::FileChunk, Status>> + Send>>;
    async fn read_file(
        &self,
        request: Request<proto::ReadFileRequest>,
    ) -> Result<Response<Self::ReadFileStream>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        let file = required(request.file)?;
        let (root, identity) = self.file(&grant, &file, READ).await?;
        let bytes =
            blocking(move || read_snapshot(&root, Path::new(identity.relative_path()))).await?;
        if request.offset > bytes.len() as u64 {
            return Err(Status::out_of_range(
                "File offset is beyond end of snapshot.",
            ));
        }
        let hash = Sha256::digest(&bytes).to_vec();
        let start = request.offset as usize;
        let end = if request.max_bytes == 0 {
            bytes.len()
        } else {
            start
                .saturating_add(request.max_bytes.min(bytes.len() as u64) as usize)
                .min(bytes.len())
        };
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let auth = self.auth.clone();
        let root = api::read_root(
            &self.auth,
            &self.roots,
            &grant,
            required(file.target.as_ref())?,
            READ,
        )
        .await?;
        tokio::spawn(async move {
            let mut offset = start;
            loop {
                if let Err(error) = auth
                    .resource(grant.id(), READ, root.to_str().unwrap_or(""))
                    .await
                {
                    let _ = tx.send(Err(error)).await;
                    break;
                }
                let next = (offset + CHUNK_BYTES).min(end);
                let chunk = proto::FileChunk {
                    file: Some(file.clone()),
                    base_hash: hash.clone(),
                    offset: offset as u64,
                    data: bytes[offset..next].to_vec(),
                    complete: next == bytes.len(),
                };
                if tx.send(Ok(chunk)).await.is_err() || next == end {
                    break;
                }
                offset = next;
            }
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
    async fn save_file(
        &self,
        request: Request<proto::SaveFileRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        let file = required(request.file.clone())?;
        let target = required(file.target.clone())?;
        let hash: [u8; 32] = request
            .expected_hash
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("Expected SHA256 is required."))?;
        let content = required(request.content.clone())?;
        let command = FileCommand::Save {
            path: file.relative_path,
            expected_hash: hash,
            content: content_identity(&content)?,
        };
        let prepared = api::prepare_file(
            &self.auth,
            &self.roots,
            &grant,
            required(request.command)?,
            &target,
            command.clone(),
        )
        .await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return Ok(Response::new(api::receipt(&self.auth, prior, &target)?));
        }
        let input = prepared.runtime_input().clone();
        let identity = command
            .identity(
                input
                    .root
                    .clone()
                    .ok_or_else(|| Status::internal("Missing file authority."))?,
            )
            .map_err(storage_status)?;
        let root = PathBuf::from(identity.root());
        let service = self.clone();
        let validation_target = target.clone();
        let operation = self.locks.begin()?;
        let result = tokio::spawn(async move {
            let _operation = operation;
            let _guard = service.locks.acquire(&root).await?;
            let accepted = service
                .auth
                .client()
                .accept_runtime_command(input, FILES_GIT)
                .await
                .map_err(storage_status)?;
            if accepted.state() != CommandState::Accepted {
                return Ok::<_, Status>(accepted);
            }
            let ready = async {
                api::revalidate_root(
                    &service.auth,
                    &service.roots,
                    &grant,
                    &validation_target,
                    FILES_GIT,
                    &root,
                )
                .await?;
                let revision = service
                    .auth
                    .client()
                    .runtime_file_presence(identity.clone())
                    .await
                    .map_err(storage_status)?
                    .into_iter()
                    .find(|p| p.device_id() == grant.id())
                    .map(|p| p.revision());
                let text = service.content(&grant, &identity, content).await?;
                let path = PathBuf::from(identity.relative_path());
                let check_root = root.clone();
                let document = blocking(move || {
                    let document = files::Document::load(&check_root, &path).map_err(domain)?;
                    if Sha256::digest(&document.original).as_slice() != hash {
                        return Err(Status::aborted(
                            "File changed on disk. Your edits remain on your device.",
                        ));
                    }
                    Ok(document)
                })
                .await?;
                Ok::<_, Status>((revision, document, text))
            }
            .await;
            let (revision, document, text) = match ready {
                Ok(ready) => ready,
                Err(error) => return reject_precondition(&service.auth, &accepted, error).await,
            };
            service
                .auth
                .client()
                .transition_runtime_command(
                    accepted.key().clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .map_err(storage_status)?;
            let path = PathBuf::from(identity.relative_path());
            let outcome = blocking(move || {
                let saved = document
                    .save(&root, &path, &text)
                    .map_err(Status::aborted)?;
                Ok(<[u8; 32]>::from(Sha256::digest(&saved.original)))
            })
            .await;
            let transport_error = outcome
                .as_ref()
                .err()
                .filter(|error| {
                    matches!(
                        error.code(),
                        tonic::Code::Unavailable
                            | tonic::Code::Internal
                            | tonic::Code::Unknown
                            | tonic::Code::PermissionDenied
                            | tonic::Code::Unauthenticated
                    )
                })
                .cloned();
            let final_receipt = match outcome {
                Ok(hash) => service
                    .auth
                    .client()
                    .finish_file_save(accepted.key().clone(), identity, revision, hash)
                    .await
                    .map_err(storage_status)?,
                Err(error) => {
                    let detail = proto::DomainError {
                        code: if matches!(
                            error.code(),
                            tonic::Code::Aborted | tonic::Code::FailedPrecondition
                        ) {
                            "conditions_changed"
                        } else {
                            "operation_failed"
                        }
                        .into(),
                        message: error.message().chars().take(8192).collect(),
                        retryable: false,
                    };
                    service
                        .auth
                        .client()
                        .transition_runtime_command(
                            accepted.key().clone(),
                            CommandState::Attempting,
                            if matches!(
                                error.code(),
                                tonic::Code::Unavailable
                                    | tonic::Code::Internal
                                    | tonic::Code::Unknown
                            ) {
                                CommandState::DeliveryUnknown
                            } else {
                                CommandState::Failed
                            },
                            Some(detail.encode_to_vec()),
                        )
                        .await
                        .map_err(storage_status)?
                }
            };
            if let Some(error) = transport_error {
                return Err(error);
            }
            Ok(final_receipt)
        })
        .await
        .map_err(|_| Status::unavailable("Save outcome requires reconciliation."))??;
        self.auth
            .client()
            .get_runtime_command(result.key().clone(), 0)
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::unavailable("Save outcome requires reconciliation."))?;
        Ok(Response::new(api::receipt(&self.auth, &result, &target)?))
    }

    async fn create_file(
        &self,
        request: Request<proto::CreateFileRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        let file = required(request.file.clone())?;
        let target = required(file.target.clone())?;
        if request.directory && request.content.is_some() {
            return Err(Status::invalid_argument("Directories cannot have content."));
        }
        let command = FileCommand::Create {
            path: file.relative_path,
            directory: request.directory,
            content: request.content.as_ref().map(content_identity).transpose()?,
        };
        let prepared = api::prepare_file(
            &self.auth,
            &self.roots,
            &grant,
            required(request.command)?,
            &target,
            command.clone(),
        )
        .await?;
        if let Some(prior) = prepared
            .prior()
            .filter(|r| r.state() != CommandState::Accepted)
        {
            return Ok(Response::new(api::receipt(&self.auth, prior, &target)?));
        }
        let input = prepared.runtime_input().clone();
        let identity = command
            .identity(
                input
                    .root
                    .clone()
                    .ok_or_else(|| Status::internal("Missing file authority."))?,
            )
            .map_err(storage_status)?;
        let root = PathBuf::from(identity.root());
        let service = self.clone();
        let validation_target = target.clone();
        let result = validated_mutation(
            self.auth.clone(),
            self.locks.clone(),
            root.clone(),
            input,
            move || async move {
                api::revalidate_root(
                    &service.auth,
                    &service.roots,
                    &grant,
                    &validation_target,
                    FILES_GIT,
                    &root,
                )
                .await?;
                let text = match request.content {
                    Some(reference) => Some(service.content(&grant, &identity, reference).await?),
                    None => None,
                };
                let check_root = root.clone();
                let check_path = PathBuf::from(identity.relative_path());
                blocking(move || {
                    let path = files::resolve(&check_root, &check_path).map_err(domain)?;
                    match std::fs::symlink_metadata(path) {
                        Ok(_) => Err(Status::aborted("The file or folder already exists.")),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(error) => Err(domain(error)),
                    }
                })
                .await?;
                Ok(async move {
                    blocking(move || {
                        let path = Path::new(identity.relative_path());
                        if request.directory {
                            std::fs::create_dir(files::resolve(&root, path).map_err(domain)?)
                                .map_err(domain)?;
                        } else {
                            if let Some(text) = text {
                                files::create_text(&root, path, &text).map_err(domain)?;
                            } else {
                                files::create(&root, path).map_err(domain)?;
                            }
                        }
                        Ok(Vec::new())
                    })
                    .await
                })
            },
        )
        .await?;
        Ok(Response::new(api::receipt(&self.auth, &result, &target)?))
    }
    async fn set_dirty_presence(
        &self,
        request: Request<proto::SetDirtyPresenceRequest>,
    ) -> Result<Response<proto::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&request).await?;
        let request = request.into_inner();
        let file = required(request.file)?;
        let target = required(file.target)?;
        let hash: [u8; 32] = request
            .base_hash
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("Expected SHA256 is required."))?;
        let command = FileCommand::Presence {
            path: file.relative_path,
            base_hash: hash,
            dirty: request.dirty,
        };
        let prepared = api::prepare_file(
            &self.auth,
            &self.roots,
            &grant,
            required(request.command)?,
            &target,
            command.clone(),
        )
        .await?;
        if let Some(prior) = prepared.prior() {
            return Ok(Response::new(api::receipt(&self.auth, prior, &target)?));
        }
        let input = prepared.runtime_input().clone();
        let identity = command
            .identity(
                input
                    .root
                    .clone()
                    .ok_or_else(|| Status::internal("Missing file authority."))?,
            )
            .map_err(storage_status)?;
        let root = PathBuf::from(identity.root());
        let service = self.clone();
        let operation = self.locks.begin()?;
        let result = tokio::spawn(async move {
            let _operation = operation;
            let _guard = service.locks.acquire(&root).await?;
            service
                .auth
                .client()
                .set_runtime_dirty_presence(input, identity, hash, request.dirty)
                .await
                .map_err(storage_status)
        })
        .await
        .map_err(|_| Status::unavailable("Presence outcome requires reconciliation."))??;
        Ok(Response::new(api::receipt(&self.auth, &result, &target)?))
    }
}
fn content_identity(value: &proto::ArtifactRef) -> Result<FileContent, Status> {
    Ok(FileContent {
        host: crate::thread::HostId::new(value.host_id.clone())
            .ok_or_else(|| Status::invalid_argument("Invalid artifact host."))?,
        artifact: value.artifact_id.clone(),
        media_type: value.media_type.clone(),
        size: value.size,
        sha256: value
            .sha256
            .as_slice()
            .try_into()
            .map_err(|_| Status::invalid_argument("Expected SHA256 is required."))?,
    })
}
fn directory_entry(
    root: &Path,
    path: &Path,
    directory: bool,
) -> Result<proto::DirectoryEntry, Status> {
    let resolved = files::resolve(root, path).map_err(domain)?;
    let metadata = std::fs::metadata(resolved).map_err(domain)?;
    Ok(proto::DirectoryEntry {
        relative_path: path
            .to_str()
            .ok_or_else(|| domain("Non-UTF8 path."))?
            .into(),
        name: path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .into(),
        directory,
        size: metadata.len(),
    })
}
fn read_snapshot(root: &Path, path: &Path) -> Result<Vec<u8>, Status> {
    let path = files::resolve(root, path).map_err(domain)?;
    if !std::fs::symlink_metadata(&path).map_err(domain)?.is_file() {
        return Err(Status::failed_precondition("Read requires a regular file."));
    }
    let file = std::fs::File::open(&path).map_err(domain)?;
    let before = file.metadata().map_err(domain)?;
    const LIMIT: u64 = 32 * 1024 * 1024;
    if !before.is_file() || before.len() > LIMIT {
        return Err(Status::resource_exhausted(
            "Read requires a regular file no larger than 32 MiB.",
        ));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&file)
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(domain)?;
    let after = file.metadata().map_err(domain)?;
    if bytes.len() as u64 != before.len()
        || before.modified().ok() != after.modified().ok()
        || before.len() != after.len()
    {
        return Err(Status::aborted("File changed while reading its snapshot."));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_and_atomic_create_do_not_follow_links_or_clobber() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        files::create_text(&root, Path::new("text"), "content").unwrap();
        assert_eq!(read_snapshot(&root, Path::new("text")).unwrap(), b"content");
        assert!(files::create_text(&root, Path::new("text"), "replacement").is_err());
        assert_eq!(std::fs::read(root.join("text")).unwrap(), b"content");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("outside")).unwrap();
            assert!(read_snapshot(&root, Path::new("outside/text")).is_err());
            assert!(files::create_text(&root, Path::new("outside/escape"), "escape").is_err());
            assert!(!dir.path().join("escape").exists());
        }
    }
    #[tokio::test]
    async fn save_artifact_requires_exact_purpose_target_and_content_hash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let (auth, grant) = super::super::tests::fixture(&root, &dir.path().join("db")).await;
        let store = Arc::new(
            ArtifactStore::open(auth.client().clone(), dir.path().join("artifacts")).unwrap(),
        );
        let worker = GitClient::start().unwrap();
        let service = FilesRpc::new(
            auth.clone(),
            WorkspaceRoots::default(),
            worker.clone(),
            store.clone(),
            ResourceLocks::default(),
        );
        let file = FileIdentity::new(root.to_str().unwrap().into(), "a.txt".into()).unwrap();
        let binding = crate::artifacts::BeginUpload::new(
            grant.id().into(),
            Target::File {
                root: file.root().into(),
                path: file.relative_path().into(),
            },
            Purpose::FileSave,
            5,
            Sha256::digest(b"hello").into(),
            "text/plain".into(),
        )
        .unwrap();
        let upload = store.begin(binding).unwrap();
        store
            .put_chunk(grant.id(), upload.id(), 0, b"hello")
            .unwrap();
        let artifact = store.finish(grant.id(), upload.id()).unwrap();
        let reference = proto::ArtifactRef {
            host_id: auth.host_id().into(),
            artifact_id: artifact.id().into(),
            media_type: artifact.media_type().into(),
            size: artifact.size(),
            sha256: artifact.sha256().to_vec(),
        };
        assert_eq!(
            service
                .content(&grant, &file, reference.clone())
                .await
                .unwrap(),
            "hello"
        );
        let wrong = FileIdentity::new(file.root().into(), "different.txt".into()).unwrap();
        assert_eq!(
            service
                .content(&grant, &wrong, reference.clone())
                .await
                .unwrap_err()
                .code(),
            tonic::Code::Aborted
        );
        let mut changed = reference;
        changed.sha256[0] ^= 1;
        assert_eq!(
            service
                .content(&grant, &file, changed)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::Aborted
        );
        worker.shutdown().await;
        auth.client().shutdown().await.unwrap();
    }
}
