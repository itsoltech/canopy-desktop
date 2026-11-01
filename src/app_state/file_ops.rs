//! The host's one reservation gate for Canopy's own writes (`files::Gate`): editor
//! Save, an agent's file write and the remote file and Git services share it, so one
//! file or checkout is changed by one Canopy operation at a time. It doesn't lock other
//! programs. The remote server gets this same gate (`ResourceLocks::with_gate`).
use canopy_desktop::files::Gate;
use canopy_desktop::{
    command_bus::{Command, CommandBus, FileContent, ResourceRef},
    thread::{HostId, ProjectRef, WorktreeId},
};
use std::{path::Path, sync::Arc};

#[derive(Clone, Default)]
pub struct FileGate(Gate);
impl gpui_kit::Global for FileGate {}

/// The app's gate, created on first use.
pub fn gate(cx: &mut gpui_kit::App) -> Gate {
    cx.default_global::<FileGate>().0.clone()
}

/// A local operation captures the current resource owner before leaving GPUI.
/// Paths select an existing project/thread; they never mint a project identity.
#[derive(Clone)]
pub(crate) struct LocalResource {
    pub bus: CommandBus,
    pub resource: ResourceRef,
    pub root: String,
    host: HostId,
    artifacts: Option<Arc<canopy_desktop::artifacts::ArtifactStore>>,
}
impl LocalResource {
    pub fn environment(cx: &gpui_kit::App) -> Result<Self, String> {
        let settings = cx.global::<super::AppState>().settings.read(cx);
        let client = settings.client().ok_or("Host storage is unavailable.")?;
        let host = HostId::new(
            settings
                .host_id()
                .ok_or("Host identity is unavailable.")?
                .into(),
        )
        .ok_or("Invalid host identity.")?;
        Ok(Self {
            bus: CommandBus::new(client),
            resource: ResourceRef::Environment(canopy_desktop::thread::EnvironmentRef {
                host_id: host.clone(),
            }),
            root: "private".into(),
            host,
            artifacts: settings.artifacts(),
        })
    }
    pub fn capture(root: &Path, cx: &gpui_kit::App) -> Result<Self, String> {
        let app = cx.global::<super::AppState>();
        let settings = app.settings.read(cx);
        let client = settings.client().ok_or("Host storage is unavailable.")?;
        let host = HostId::new(
            settings
                .host_id()
                .ok_or("Host identity is unavailable.")?
                .into(),
        )
        .ok_or("Invalid host identity.")?;
        let projects = app.projects.read(cx);
        let resource = if let Some(project) = projects
            .catalog
            .items
            .iter()
            .find(|p| p.worktree_path.as_deref().unwrap_or(&p.path) == root)
        {
            let uses_git =
                project.repository_path.is_some() || app.git.read(cx).repository(root).is_some();
            ResourceRef::Project(
                ProjectRef::new(
                    host.clone(),
                    project.id,
                    uses_git.then(|| {
                        WorktreeId::new(
                            serde_json::to_string(&project.workspace).expect("workspace identity"),
                        )
                        .expect("workspace identity")
                    }),
                    uses_git,
                )
                .map_err(|_| "Invalid checkout identity.")?,
            )
        } else {
            let mut matching = settings
                .chat_records()
                .filter(|r| r.metadata().cwd.as_deref() == Some(root));
            let record = matching
                .next()
                .ok_or("The file has no active project or thread owner.")?;
            if matching.next().is_some() {
                return Err("Select the file's project or source thread before saving.".into());
            }
            let thread = record
                .thread()
                .ok_or("The source thread is not persisted yet.")?;
            ResourceRef::Thread {
                thread: thread.thread().clone(),
                executor_epoch: thread.executor_epoch(),
            }
        };
        Ok(Self {
            bus: CommandBus::new(client),
            resource,
            root: root.to_str().ok_or("Unsupported path encoding.")?.into(),
            host,
            artifacts: settings.artifacts(),
        })
    }
    pub async fn begin(
        &self,
        command: Command,
    ) -> Result<canopy_desktop::host::CommandInput, String> {
        self.bus
            .begin_local_resource(
                self.resource.clone(),
                self.root.clone(),
                command,
                vec![self.root.clone()],
            )
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn begin_many(
        &self,
        commands: Vec<Command>,
    ) -> Result<Vec<canopy_desktop::host::CommandInput>, String> {
        let mut inputs = Vec::new();
        for command in commands {
            match self.begin(command).await {
                Ok(input) => inputs.push(input),
                Err(error) => {
                    let _ = self.finish_many(inputs, Err(error.clone())).await;
                    return Err(error);
                }
            }
        }
        Ok(inputs)
    }
    pub async fn begin_set(
        commands: Vec<(Self, Command)>,
    ) -> Result<Vec<(Self, canopy_desktop::host::CommandInput)>, String> {
        let mut inputs = Vec::new();
        for (owner, command) in commands {
            match owner.begin(command).await {
                Ok(input) => inputs.push((owner, input)),
                Err(error) => {
                    let _ = Self::finish_set(inputs, Err(error.clone())).await;
                    return Err(error);
                }
            }
        }
        Ok(inputs)
    }
    pub async fn finish_set(
        inputs: Vec<(Self, canopy_desktop::host::CommandInput)>,
        outcome: Result<(), String>,
    ) -> Result<(), String> {
        let mut result = outcome.clone();
        for (owner, input) in inputs {
            if let Err(error) = owner
                .bus
                .finish_local_resource(input, outcome.clone())
                .await
            {
                result = Err(error);
            }
        }
        result
    }
    pub async fn finish_many(
        &self,
        inputs: Vec<canopy_desktop::host::CommandInput>,
        outcome: Result<(), String>,
    ) -> Result<(), String> {
        let mut result = outcome.clone();
        for input in inputs {
            if let Err(error) = self.bus.finish_local_resource(input, outcome.clone()).await {
                result = Err(error);
            }
        }
        result
    }
    pub async fn begin_git(
        &self,
        intent: canopy_desktop::command_bus::GitIntent,
        executor: &gpui_kit::BackgroundExecutor,
    ) -> Result<canopy_desktop::host::CommandInput, String> {
        self.begin_git_roots(intent, vec![self.root.clone()], executor)
            .await
    }
    pub async fn begin_git_roots(
        &self,
        intent: canopy_desktop::command_bus::GitIntent,
        authority: Vec<String>,
        executor: &gpui_kit::BackgroundExecutor,
    ) -> Result<canopy_desktop::host::CommandInput, String> {
        let root = std::path::PathBuf::from(&self.root);
        let expected = executor
            .spawn(async move {
                canopy_desktop::git::snapshot::capture_local(&root).map_err(|e| e.to_string())
            })
            .await?;
        self.bus
            .begin_local_resource(
                self.resource.clone(),
                self.root.clone(),
                Command::Git(canopy_desktop::command_bus::GitCommand { intent, expected }),
                authority,
            )
            .await
            .map_err(|e| e.to_string())
    }
    /// Staging uses the existing bulk store. Only its immutable identity enters
    /// the command inbox; editor text never becomes a command payload.
    pub fn stage_file(&self, path: &str, bytes: &[u8]) -> Result<FileContent, String> {
        self.stage(
            canopy_desktop::artifacts::Target::File {
                root: self.root.clone(),
                path: path.into(),
            },
            canopy_desktop::artifacts::Purpose::FileSave,
            "text/plain",
            bytes,
        )
    }
    pub fn stage_task(
        &self,
        integration: &str,
        project: &str,
        task: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<FileContent, String> {
        self.stage(
            canopy_desktop::artifacts::Target::Task {
                root: self.root.clone(),
                integration_id: integration.into(),
                project_id: project.into(),
                task_id: task.into(),
            },
            canopy_desktop::artifacts::Purpose::TaskAttachment,
            media_type,
            bytes,
        )
    }
    fn stage(
        &self,
        target: canopy_desktop::artifacts::Target,
        purpose: canopy_desktop::artifacts::Purpose,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<FileContent, String> {
        use canopy_desktop::artifacts::{BeginUpload, CHUNK_BYTES};
        use sha2::{Digest, Sha256};
        let store = self
            .artifacts
            .as_ref()
            .ok_or("File staging is unavailable.")?;
        let binding = BeginUpload::new(
            canopy_desktop::host::LOCAL_PRINCIPAL.into(),
            target,
            purpose,
            bytes.len() as u64,
            Sha256::digest(bytes).into(),
            media_type.into(),
        )
        .map_err(|e| e.to_string())?;
        let upload = store.begin(binding).map_err(|e| e.to_string())?;
        for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
            store
                .put_chunk(
                    canopy_desktop::host::LOCAL_PRINCIPAL,
                    upload.id(),
                    (index * CHUNK_BYTES) as u64,
                    chunk,
                )
                .map_err(|e| e.to_string())?;
        }
        let artifact = store
            .finish(canopy_desktop::host::LOCAL_PRINCIPAL, upload.id())
            .map_err(|e| e.to_string())?;
        Ok(FileContent {
            host: self.host.clone(),
            artifact: artifact.id().into(),
            media_type: artifact.media_type().into(),
            size: artifact.size(),
            sha256: *artifact.sha256(),
        })
    }
}
