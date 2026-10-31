use super::{
    Access, Change, Error, ImportReport, Result, Snapshot,
    database::{self, Database},
};
use futures_channel::oneshot;
use std::{
    path::PathBuf,
    sync::mpsc::{self, SyncSender, TrySendError},
    time::Instant,
};

use crate::state::{projects::ProjectSnapshot, session::SessionSnapshot};

type Reply<T> = oneshot::Sender<Result<T>>;
enum Command {
    PrepareBus(
        crate::command_bus::CommandEnvelope,
        crate::command_bus::Principal,
        Reply<crate::command_bus::PreparedCommand>,
    ),
    DispatchBus(
        Box<crate::command_bus::PreparedCommand>,
        crate::command_bus::Dispatch,
        Reply<crate::host::CommandReceipt>,
    ),
    CommitBusPreferences(
        Box<crate::command_bus::PreparedCommand>,
        u64,
        Reply<(crate::host::CommandReceipt, Option<Snapshot>)>,
    ),
    RuntimeDeviceCertificate(String, Reply<Option<Vec<u8>>>),
    GetRuntimeCommandMatching(
        crate::host::CommandKey,
        u32,
        String,
        String,
        Vec<u8>,
        Reply<Option<crate::host::CommandReceipt>>,
    ),
    AcceptChatMutation(
        crate::host::CommandInput,
        u32,
        String,
        Vec<String>,
        Reply<crate::host::CommandReceipt>,
    ),
    CompleteRuntimeFork(
        crate::host::CommandKey,
        Box<crate::host::ChatRecord>,
        String,
        Option<(crate::host::CommandInput, String, crate::host::QueueDraft)>,
        Reply<crate::host::CommandReceipt>,
    ),
    ReserveRemoteSecret(crate::host::CommandKey, String, Vec<u8>, Reply<()>),
    CollectRemoteSecrets(Reply<Vec<String>>),
    ForgetRemoteSecret(String, Reply<()>),
    RetainLocalChatArtifact(String, String, Reply<()>),
    FinishFileSave(
        crate::host::CommandKey,
        crate::host::FileIdentity,
        Option<u64>,
        [u8; 32],
        Reply<crate::host::CommandReceipt>,
    ),
    LookupRuntimeDevice(Vec<u8>, Reply<crate::host::DeviceGrant>),
    RuntimeDevices(Reply<Vec<crate::host::DeviceGrant>>),
    RuntimeDevicesPage(
        Option<String>,
        bool,
        Reply<(Vec<crate::host::DeviceGrant>, Option<String>)>,
    ),
    AuthorizeRuntimeRead(String, u32, String, Reply<crate::host::DeviceGrant>),
    UpdateRuntimeDeviceAuthorization(
        String,
        u64,
        u32,
        Vec<String>,
        Reply<crate::host::DeviceGrant>,
    ),
    SetRuntimeIdentityRef(Option<String>, String, Reply<crate::host::RuntimeHost>),
    SetRuntimeDirtyPresence(
        crate::host::CommandInput,
        crate::host::FileIdentity,
        [u8; 32],
        bool,
        Reply<crate::host::CommandReceipt>,
    ),
    RuntimeRootPresence(
        String,
        Reply<Vec<(crate::host::FileIdentity, crate::host::DirtyPresence)>>,
    ),
    RuntimeFilePresence(
        crate::host::FileIdentity,
        Reply<Vec<crate::host::DirtyPresence>>,
    ),
    DismissRuntimeDirtyPresence(
        crate::host::CommandInput,
        crate::host::FileIdentity,
        String,
        u64,
        Reply<crate::host::CommandReceipt>,
    ),
    RecoverRuntimeChat(
        crate::host::CommandInput,
        crate::host::ChatRecord,
        String,
        Reply<crate::host::CommandReceipt>,
    ),
    RetainedArtifactImage(String, String, Reply<crate::artifacts::Artifact>),
    BeginArtifactUpload(
        String,
        crate::artifacts::BeginUpload,
        Reply<crate::artifacts::Upload>,
    ),
    ArtifactUploadStatus(String, String, Reply<crate::artifacts::Upload>),
    RecordArtifactChunk(
        String,
        String,
        Vec<(u64, u64)>,
        Reply<crate::artifacts::Upload>,
    ),
    PublishArtifactUpload(String, String, Reply<crate::artifacts::Artifact>),
    CancelArtifactUpload(String, String, Reply<()>),
    AuthorizeArtifactDownload(String, String, Reply<(crate::artifacts::Artifact, String)>),
    CollectArtifactStorage(Reply<Vec<String>>),

    MutateRuntimeQueue(
        crate::host::CommandInput,
        String,
        u64,
        crate::host::QueueMutation,
        Reply<crate::host::CommandReceipt>,
    ),
    ResolveRuntimeOperation(crate::host::CommandKey, Reply<crate::host::CommandReceipt>),
    AcknowledgeRuntimeOperation(crate::host::CommandKey, Reply<()>),
    InitializeRuntime(Reply<crate::host::RuntimeHost>),
    SaveRuntimeChat(crate::host::ChatRecord, Reply<crate::host::ChatRecord>),
    LoadRuntimeChats(Reply<Vec<crate::host::ChatRecord>>),
    LoadRuntimeChat(String, Reply<Option<crate::host::ChatRecord>>),
    LoadRuntimeThread(
        crate::thread::ThreadId,
        Reply<Option<crate::host::ChatRecord>>,
    ),
    RuntimeThreadAlias(
        crate::thread::HostId,
        String,
        Reply<Option<crate::thread::ThreadId>>,
    ),
    RegisterRuntimeDevice(crate::host::DeviceRegistration, Reply<()>),
    RevokeRuntimeDevice(String, Reply<()>),
    AcceptRuntimeOperation(
        Instant,
        crate::host::CommandInput,
        u32,
        Reply<crate::host::CommandReceipt>,
    ),
    GetRuntimeOperation(
        crate::host::CommandKey,
        u32,
        Reply<Option<crate::host::CommandReceipt>>,
    ),
    TransitionRuntimeOperation(
        crate::host::CommandKey,
        crate::host::CommandState,
        crate::host::CommandState,
        Option<Vec<u8>>,
        Reply<crate::host::CommandReceipt>,
    ),
    CloseRuntimeCommandEpoch(String, u64, Reply<u64>),
    AcceptRuntimeMessage(
        Instant,
        crate::host::CommandInput,
        String,
        String,
        crate::host::QueueDraft,
        Reply<crate::host::CommandReceipt>,
    ),
    LoadRuntimeQueue(String, Reply<crate::host::ChatQueue>),
    SetRuntimeQueuePaused(String, u64, bool, Reply<crate::host::ChatQueue>),
    TransitionRuntimeQueue(
        String,
        String,
        u64,
        crate::host::QueueState,
        crate::host::QueueState,
        Reply<crate::host::ChatQueue>,
    ),
    LoadTaskDrafts(Reply<crate::integrations::drafts::TaskDrafts>),
    SaveTaskDrafts(crate::integrations::drafts::TaskDrafts, Reply<()>),
    LoadIntegrations(Reply<crate::integrations::Config>),
    SaveIntegrations(crate::integrations::Config, Reply<()>),
    Load(Reply<Snapshot>),
    LoadTools(Reply<crate::state::tools::ToolCatalog>),
    SaveTools(crate::state::tools::ToolCatalog, Reply<()>),
    LoadSession(Reply<Option<SessionSnapshot>>),
    SaveSession(SessionSnapshot, Reply<()>),
    LoadProjects(Reply<ProjectSnapshot>),
    SaveProjects(ProjectSnapshot, Reply<()>),
    Apply(Vec<Change>, Reply<Snapshot>),
    SaveChatHistory(Vec<super::ChatHistoryBatch>, Reply<()>),
    SaveChatHistoryCheckpointed(
        Vec<super::ChatHistoryBatch>,
        Reply<Vec<super::HistoryCheckpoint>>,
    ),
    ReadHistoryCheckpoint(String, Vec<(u64, u64)>, Reply<super::HistoryCheckpoint>),
    LoadChatHistory(String, Reply<super::ChatHistoryRows>),
    ReadRemoteHistory(super::RemoteHistoryRequest, Reply<super::RemoteHistoryPage>),
    DeleteChatHistory(String, Reply<()>),
    RetainChatHistory(Vec<String>, i64, Reply<Vec<String>>),
    SaveClosedChat(super::ClosedChatRow, Reply<()>),
    SaveModelCatalog(String, Reply<()>),
    LoadModelCatalog(Reply<Option<String>>),
    LoadClosedChats(Reply<Vec<super::ClosedChatRow>>),
    RemoveClosedChat(String, Reply<()>),
    Shutdown(Reply<()>),
}

/// Cloneable async handle. The SQLite connection never leaves its dedicated thread.
#[derive(Clone)]
pub struct SettingsClient {
    sender: SyncSender<Command>,
}
impl SettingsClient {
    pub(crate) async fn prepare_bus_command(
        &self,
        envelope: crate::command_bus::CommandEnvelope,
        principal: crate::command_bus::Principal,
    ) -> Result<crate::command_bus::PreparedCommand> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::PrepareBus(envelope, principal, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn dispatch_bus_command(
        &self,
        prepared: crate::command_bus::PreparedCommand,
        dispatch: crate::command_bus::Dispatch,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::DispatchBus(Box::new(prepared), dispatch, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn commit_bus_preferences(
        &self,
        prepared: crate::command_bus::PreparedCommand,
        revision: u64,
    ) -> Result<(crate::host::CommandReceipt, Option<Snapshot>)> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::CommitBusPreferences(
            Box::new(prepared),
            revision,
            tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn runtime_device_certificate(&self, id: String) -> Result<Option<Vec<u8>>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RuntimeDeviceCertificate(id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn reserve_remote_secret(
        &self,
        key: crate::host::CommandKey,
        id: String,
        fingerprint: Vec<u8>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::ReserveRemoteSecret(key, id, fingerprint, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn collect_remote_secrets(&self) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::CollectRemoteSecrets(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn forget_remote_secret(&self, id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::ForgetRemoteSecret(id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn retain_local_chat_artifact(&self, chat: String, id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RetainLocalChatArtifact(chat, id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    /// Host transport only: pass the issuer-verified mTLS peer leaf DER, never
    /// certificate bytes or an identity claimed in a request body.
    pub async fn lookup_runtime_device(&self, leaf: Vec<u8>) -> Result<crate::host::DeviceGrant> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LookupRuntimeDevice(leaf, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    /// Trusted host administration. Remote handlers must first authorize ADMIN.
    pub async fn runtime_devices(&self) -> Result<Vec<crate::host::DeviceGrant>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RuntimeDevices(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn runtime_devices_page(
        &self,
        after: Option<String>,
        include_revoked: bool,
    ) -> Result<(Vec<crate::host::DeviceGrant>, Option<String>)> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RuntimeDevicesPage(after, include_revoked, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    /// Revalidates the identity established by mTLS against current grants.
    /// The device ID must not come from request metadata or a display name.
    pub async fn authorize_runtime_read(
        &self,
        device: String,
        scope: u32,
        root: String,
    ) -> Result<crate::host::DeviceGrant> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::AuthorizeRuntimeRead(device, scope, root, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn update_runtime_device_authorization(
        &self,
        device: String,
        revision: u64,
        scopes: u32,
        roots: Vec<String>,
    ) -> Result<crate::host::DeviceGrant> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::UpdateRuntimeDeviceAuthorization(
            device, revision, scopes, roots, tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn set_runtime_identity_ref(
        &self,
        expected: Option<String>,
        new: String,
    ) -> Result<crate::host::RuntimeHost> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SetRuntimeIdentityRef(expected, new, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn finish_file_save(
        &self,
        key: crate::host::CommandKey,
        file: crate::host::FileIdentity,
        revision: Option<u64>,
        hash: [u8; 32],
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::FinishFileSave(key, file, revision, hash, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn set_runtime_dirty_presence(
        &self,
        input: crate::host::CommandInput,
        file: crate::host::FileIdentity,
        base_hash: [u8; 32],
        dirty: bool,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SetRuntimeDirtyPresence(
            input, file, base_hash, dirty, tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    /// Trusted host preflight. Includes offline owners and deleted files, without
    /// filesystem I/O. More than 1024 records or 1 MiB returns ResourceLimit.
    pub async fn runtime_root_presence(
        &self,
        root: String,
    ) -> Result<Vec<(crate::host::FileIdentity, crate::host::DirtyPresence)>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RuntimeRootPresence(root, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    /// Host read, including offline records. Remote handlers authorize the root
    /// first. Last-seen timestamps do not imply an active connection.
    pub async fn runtime_file_presence(
        &self,
        file: crate::host::FileIdentity,
    ) -> Result<Vec<crate::host::DirtyPresence>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RuntimeFilePresence(file, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn dismiss_runtime_dirty_presence(
        &self,
        input: crate::host::CommandInput,
        file: crate::host::FileIdentity,
        device: String,
        revision: u64,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::DismissRuntimeDirtyPresence(
            input, file, device, revision, tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn recover_runtime_chat(
        &self,
        input: crate::host::CommandInput,
        chat: crate::host::ChatRecord,
        expected_reason: String,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RecoverRuntimeChat(
            input,
            chat,
            expected_reason,
            tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub(crate) async fn retained_artifact_image(
        &self,
        chat: String,
        id: String,
    ) -> Result<crate::artifacts::Artifact> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RetainedArtifactImage(chat, id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub(crate) async fn begin_artifact_upload(
        &self,
        id: String,
        binding: crate::artifacts::BeginUpload,
    ) -> Result<crate::artifacts::Upload> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::BeginArtifactUpload(id, binding, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn artifact_upload_status(
        &self,
        owner: String,
        id: String,
    ) -> Result<crate::artifacts::Upload> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::ArtifactUploadStatus(owner, id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn record_artifact_chunk(
        &self,
        owner: String,
        id: String,
        ranges: Vec<(u64, u64)>,
    ) -> Result<crate::artifacts::Upload> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RecordArtifactChunk(owner, id, ranges, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn publish_artifact_upload(
        &self,
        owner: String,
        id: String,
    ) -> Result<crate::artifacts::Artifact> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::PublishArtifactUpload(owner, id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn cancel_artifact_upload(&self, owner: String, id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::CancelArtifactUpload(owner, id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn authorize_artifact_download(
        &self,
        owner: String,
        id: String,
    ) -> Result<(crate::artifacts::Artifact, String)> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::AuthorizeArtifactDownload(owner, id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub(crate) async fn collect_artifact_storage(&self) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::CollectArtifactStorage(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn mutate_runtime_queue(
        &self,
        input: crate::host::CommandInput,
        chat: String,
        revision: u64,
        mutation: crate::host::QueueMutation,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::MutateRuntimeQueue(
            input, chat, revision, mutation, tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn resolve_runtime_command(
        &self,
        key: crate::host::CommandKey,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::ResolveRuntimeOperation(key, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn acknowledge_runtime_command(&self, key: crate::host::CommandKey) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::AcknowledgeRuntimeOperation(key, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn initialize_runtime(&self) -> Result<crate::host::RuntimeHost> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::InitializeRuntime(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_runtime_chat(
        &self,
        chat: crate::host::ChatRecord,
    ) -> Result<crate::host::ChatRecord> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveRuntimeChat(chat, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_runtime_chats(&self) -> Result<Vec<crate::host::ChatRecord>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadRuntimeChats(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_runtime_chat(&self, id: String) -> Result<Option<crate::host::ChatRecord>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadRuntimeChat(id, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_runtime_thread(
        &self,
        thread: crate::thread::ThreadId,
    ) -> Result<Option<crate::host::ChatRecord>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadRuntimeThread(thread, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    /// The thread a legacy chat ID of `host` resolves to, kept after delete.
    pub async fn runtime_thread_alias(
        &self,
        host: crate::thread::HostId,
        chat: String,
    ) -> Result<Option<crate::thread::ThreadId>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RuntimeThreadAlias(host, chat, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn register_runtime_device(
        &self,
        device: crate::host::DeviceRegistration,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RegisterRuntimeDevice(device, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn revoke_runtime_device(&self, device: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RevokeRuntimeDevice(device, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn get_runtime_command_matching(
        &self,
        key: crate::host::CommandKey,
        scope: u32,
        method: String,
        target: String,
        fingerprint: Vec<u8>,
    ) -> Result<Option<crate::host::CommandReceipt>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::GetRuntimeCommandMatching(
            key,
            scope,
            method,
            target,
            fingerprint,
            tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn accept_runtime_chat_command(
        &self,
        input: crate::host::CommandInput,
        scope: u32,
        chat: String,
        artifacts: Vec<String>,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::AcceptChatMutation(
            input, scope, chat, artifacts, tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn complete_runtime_fork(
        &self,
        key: crate::host::CommandKey,
        record: crate::host::ChatRecord,
        source_chat: String,
        replacement: Option<(crate::host::CommandInput, String, crate::host::QueueDraft)>,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::CompleteRuntimeFork(
            key,
            Box::new(record),
            source_chat,
            replacement,
            tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn accept_runtime_command(
        &self,
        input: crate::host::CommandInput,
        scope: u32,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::AcceptRuntimeOperation(
            Instant::now(),
            input,
            scope,
            tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn get_runtime_command(
        &self,
        key: crate::host::CommandKey,
        scope: u32,
    ) -> Result<Option<crate::host::CommandReceipt>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::GetRuntimeOperation(key, scope, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn transition_runtime_command(
        &self,
        key: crate::host::CommandKey,
        expected: crate::host::CommandState,
        state: crate::host::CommandState,
        result: Option<Vec<u8>>,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::TransitionRuntimeOperation(
            key, expected, state, result, tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn close_runtime_command_epoch(&self, device: String, expected: u64) -> Result<u64> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::CloseRuntimeCommandEpoch(device, expected, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn accept_runtime_message(
        &self,
        input: crate::host::CommandInput,
        chat: String,
        message: String,
        draft: crate::host::QueueDraft,
    ) -> Result<crate::host::CommandReceipt> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::AcceptRuntimeMessage(
            Instant::now(),
            input,
            chat,
            message,
            draft,
            tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_runtime_queue(&self, chat: String) -> Result<crate::host::ChatQueue> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadRuntimeQueue(chat, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn set_runtime_queue_paused(
        &self,
        chat: String,
        revision: u64,
        paused: bool,
    ) -> Result<crate::host::ChatQueue> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SetRuntimeQueuePaused(chat, revision, paused, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn transition_runtime_queue(
        &self,
        chat: String,
        message: String,
        revision: u64,
        expected: crate::host::QueueState,
        state: crate::host::QueueState,
    ) -> Result<crate::host::ChatQueue> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::TransitionRuntimeQueue(
            chat, message, revision, expected, state, tx,
        ))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_task_drafts(&self) -> Result<crate::integrations::drafts::TaskDrafts> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadTaskDrafts(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_task_drafts(
        &self,
        drafts: crate::integrations::drafts::TaskDrafts,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveTaskDrafts(drafts, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }

    pub async fn open(path: impl Into<PathBuf>, access: Access) -> Result<Self> {
        let path = path.into();
        Self::start(move || Database::open(&path, access)).await
    }
    /// Create a new Rust preferences database. Never overwrites an existing path.
    pub async fn create(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        Self::start(move || {
            database::create(&path)?;
            Database::open(&path, Access::ReadWrite)
        })
        .await
    }
    /// A consistent SQLite online backup, including the source's committed WAL.
    pub async fn import_electron(
        source: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
    ) -> Result<ImportReport> {
        let (source, destination) = (source.into(), destination.into());
        let (tx, rx) = oneshot::channel();
        std::thread::Builder::new()
            .name("canopy-db-import".into())
            .spawn(move || {
                let _ = tx.send(database::import(&source, &destination));
            })?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    async fn start(open: impl FnOnce() -> Result<Database> + Send + 'static) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(64);
        let (ready_tx, ready_rx) = oneshot::channel();
        std::thread::Builder::new()
            .name("canopy-settings".into())
            .spawn(move || {
                let mut db = match open() {
                    Ok(db) => db,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                if ready_tx.send(Ok(())).is_err() {
                    return;
                }
                for command in receiver {
                    match command {
                        Command::PrepareBus(envelope, principal, reply) => {
                            let _ = reply.send(db.prepare_bus_command(envelope, principal));
                        }
                        Command::DispatchBus(prepared, dispatch, reply) => {
                            let _ = reply.send(db.dispatch_bus_command(*prepared, dispatch));
                        }
                        Command::CommitBusPreferences(prepared, revision, reply) => {
                            let _ = reply.send(db.commit_bus_preferences(*prepared, revision));
                        }
                        Command::ReserveRemoteSecret(key, id, fingerprint, reply) => {
                            let _ = reply.send(db.reserve_remote_secret(key, id, fingerprint));
                        }
                        Command::CollectRemoteSecrets(reply) => {
                            let _ = reply.send(db.collect_remote_secrets());
                        }
                        Command::ForgetRemoteSecret(id, reply) => {
                            let _ = reply.send(db.forget_remote_secret(id));
                        }
                        Command::RetainLocalChatArtifact(chat, id, reply) => {
                            let _ = reply.send(db.retain_local_chat_artifact(chat, id));
                        }
                        Command::FinishFileSave(key, file, revision, hash, reply) => {
                            let _ = reply.send(db.finish_file_save(key, file, revision, hash));
                        }
                        Command::RetainedArtifactImage(chat, id, reply) => {
                            let _ = reply.send(db.retained_artifact_image(chat, id));
                        }
                        Command::BeginArtifactUpload(id, binding, reply) => {
                            let _ = reply.send(db.begin_artifact_upload(id, binding));
                        }
                        Command::ArtifactUploadStatus(owner, id, reply) => {
                            let _ = reply.send(db.artifact_upload_status(owner, id));
                        }
                        Command::RecordArtifactChunk(owner, id, ranges, reply) => {
                            let _ = reply.send(db.record_artifact_chunk(owner, id, ranges));
                        }
                        Command::PublishArtifactUpload(owner, id, reply) => {
                            let _ = reply.send(db.publish_artifact_upload(owner, id));
                        }
                        Command::CancelArtifactUpload(owner, id, reply) => {
                            let _ = reply.send(db.cancel_artifact_upload(owner, id));
                        }
                        Command::AuthorizeArtifactDownload(owner, id, reply) => {
                            let _ = reply.send(db.authorize_artifact_download(owner, id));
                        }
                        Command::CollectArtifactStorage(reply) => {
                            let _ = reply.send(db.collect_artifact_storage());
                        }
                        Command::MutateRuntimeQueue(input, chat, revision, mutation, reply) => {
                            let _ = reply
                                .send(db.mutate_runtime_queue(input, chat, revision, mutation));
                        }
                        Command::ResolveRuntimeOperation(key, reply) => {
                            let _ = reply.send(db.resolve_runtime_command(key));
                        }
                        Command::AcknowledgeRuntimeOperation(key, reply) => {
                            let _ = reply.send(db.acknowledge_runtime_command(key));
                        }
                        Command::InitializeRuntime(reply) => {
                            let _ = reply.send(db.initialize_runtime());
                        }
                        Command::SaveRuntimeChat(chat, reply) => {
                            let _ = reply.send(db.save_runtime_chat(chat));
                        }
                        Command::LoadRuntimeChats(reply) => {
                            let _ = reply.send(db.load_runtime_chats());
                        }
                        Command::LoadRuntimeChat(id, reply) => {
                            let _ = reply.send(db.load_runtime_chat(id));
                        }
                        Command::LoadRuntimeThread(thread, reply) => {
                            let _ = reply.send(db.load_runtime_thread(thread));
                        }
                        Command::RuntimeThreadAlias(host, chat, reply) => {
                            let _ = reply.send(db.runtime_thread_alias(host, chat));
                        }
                        Command::RuntimeDeviceCertificate(id, reply) => {
                            let _ = reply.send(db.runtime_device_certificate(id));
                        }
                        Command::LookupRuntimeDevice(leaf, reply) => {
                            let _ = reply.send(db.lookup_runtime_device(leaf));
                        }
                        Command::RuntimeDevices(reply) => {
                            let _ = reply.send(db.runtime_devices());
                        }
                        Command::RuntimeDevicesPage(after, include_revoked, reply) => {
                            let _ = reply.send(db.runtime_devices_page(after, include_revoked));
                        }
                        Command::AuthorizeRuntimeRead(device, scope, root, reply) => {
                            let _ = reply.send(db.authorize_runtime_read(device, scope, root));
                        }
                        Command::UpdateRuntimeDeviceAuthorization(
                            device,
                            revision,
                            scopes,
                            roots,
                            reply,
                        ) => {
                            let _ = reply.send(db.update_runtime_device_authorization(
                                device, revision, scopes, roots,
                            ));
                        }
                        Command::SetRuntimeIdentityRef(expected, new, reply) => {
                            let _ = reply.send(db.set_runtime_identity_ref(expected, new));
                        }
                        Command::SetRuntimeDirtyPresence(input, file, base_hash, dirty, reply) => {
                            let _ = reply
                                .send(db.set_runtime_dirty_presence(input, file, base_hash, dirty));
                        }
                        Command::RuntimeRootPresence(root, reply) => {
                            let _ = reply.send(db.runtime_root_presence(root));
                        }
                        Command::RuntimeFilePresence(file, reply) => {
                            let _ = reply.send(db.runtime_file_presence(file));
                        }
                        Command::DismissRuntimeDirtyPresence(
                            input,
                            file,
                            device,
                            revision,
                            reply,
                        ) => {
                            let _ = reply.send(
                                db.dismiss_runtime_dirty_presence(input, file, device, revision),
                            );
                        }
                        Command::RecoverRuntimeChat(input, chat, expected_reason, reply) => {
                            let _ =
                                reply.send(db.recover_runtime_chat(input, chat, expected_reason));
                        }
                        Command::RegisterRuntimeDevice(device, reply) => {
                            let _ = reply.send(db.register_runtime_device(device));
                        }
                        Command::RevokeRuntimeDevice(device, reply) => {
                            let _ = reply.send(db.revoke_runtime_device(device));
                        }
                        Command::GetRuntimeCommandMatching(
                            key,
                            scope,
                            method,
                            target,
                            fingerprint,
                            reply,
                        ) => {
                            let _ = reply.send(db.get_runtime_command_matching(
                                key,
                                scope,
                                method,
                                target,
                                fingerprint,
                            ));
                        }
                        Command::AcceptChatMutation(input, scope, chat, artifacts, reply) => {
                            let _ = reply.send(
                                db.accept_runtime_chat_command(input, scope, chat, artifacts),
                            );
                        }
                        Command::CompleteRuntimeFork(
                            key,
                            record,
                            source_chat,
                            replacement,
                            reply,
                        ) => {
                            let _ = reply.send(db.complete_runtime_fork(
                                key,
                                *record,
                                source_chat,
                                replacement,
                            ));
                        }
                        Command::AcceptRuntimeOperation(queued, input, scope, reply) => {
                            db.record_runtime_queue_wait(&input, queued);
                            let _ = reply.send(db.accept_runtime_command(input, scope));
                        }
                        Command::GetRuntimeOperation(key, scope, reply) => {
                            let _ = reply.send(db.get_runtime_command(key, scope));
                        }
                        Command::TransitionRuntimeOperation(
                            key,
                            expected,
                            state,
                            result,
                            reply,
                        ) => {
                            let _ = reply
                                .send(db.transition_runtime_command(key, expected, state, result));
                        }
                        Command::CloseRuntimeCommandEpoch(device, expected, reply) => {
                            let _ = reply.send(db.close_runtime_command_epoch(device, expected));
                        }
                        Command::AcceptRuntimeMessage(
                            queued,
                            input,
                            chat,
                            message,
                            draft,
                            reply,
                        ) => {
                            db.record_runtime_queue_wait(&input, queued);
                            let _ =
                                reply.send(db.accept_runtime_message(input, chat, message, draft));
                        }
                        Command::LoadRuntimeQueue(chat, reply) => {
                            let _ = reply.send(db.load_runtime_queue(chat));
                        }
                        Command::SetRuntimeQueuePaused(chat, revision, paused, reply) => {
                            let _ = reply.send(db.set_runtime_queue_paused(chat, revision, paused));
                        }
                        Command::TransitionRuntimeQueue(
                            chat,
                            message,
                            revision,
                            expected,
                            state,
                            reply,
                        ) => {
                            let _ = reply.send(db.transition_runtime_queue(
                                chat, message, revision, expected, state,
                            ));
                        }
                        Command::LoadTaskDrafts(reply) => {
                            let _ = reply.send(db.load_task_drafts());
                        }
                        Command::SaveTaskDrafts(drafts, reply) => {
                            let _ = reply.send(db.save_task_drafts(drafts));
                        }
                        Command::LoadIntegrations(reply) => {
                            let _ = reply.send(db.load_integrations());
                        }
                        Command::SaveIntegrations(config, reply) => {
                            let _ = reply.send(db.save_integrations(config));
                        }
                        Command::LoadTools(reply) => {
                            let _ = reply.send(db.load_tools());
                        }
                        Command::SaveTools(catalog, reply) => {
                            let _ = reply.send(db.save_tools(catalog));
                        }
                        Command::LoadSession(reply) => {
                            let _ = reply.send(db.load_session());
                        }
                        Command::SaveSession(snapshot, reply) => {
                            let _ = reply.send(db.save_session(snapshot));
                        }
                        Command::LoadProjects(reply) => {
                            let _ = reply.send(db.load_projects());
                        }
                        Command::SaveProjects(snapshot, reply) => {
                            let _ = reply.send(db.save_projects(snapshot));
                        }
                        Command::Load(reply) => {
                            let _ = reply.send(db.load());
                        }
                        Command::Apply(changes, reply) => {
                            let _ = reply.send(db.apply(changes));
                        }
                        Command::SaveChatHistory(batches, reply) => {
                            let _ = reply.send(db.save_chat_history(batches));
                        }
                        Command::SaveChatHistoryCheckpointed(batches, reply) => {
                            let _ = reply.send(db.save_chat_history_checkpointed(batches));
                        }
                        Command::ReadHistoryCheckpoint(chat, keys, reply) => {
                            let _ = reply.send(db.read_history_checkpoint(chat, keys));
                        }
                        Command::ReadRemoteHistory(request, reply) => {
                            let _ = reply.send(db.read_remote_history_page(request));
                        }
                        Command::LoadChatHistory(chat, reply) => {
                            let _ = reply.send(db.load_chat_history(&chat));
                        }
                        Command::DeleteChatHistory(chat, reply) => {
                            let _ = reply.send(db.delete_chat_history(&chat));
                        }
                        Command::SaveModelCatalog(payload, reply) => {
                            let _ = reply.send(db.save_model_catalog(payload));
                        }
                        Command::LoadModelCatalog(reply) => {
                            let _ = reply.send(db.load_model_catalog());
                        }
                        Command::SaveClosedChat(row, reply) => {
                            let _ = reply.send(db.save_closed_chat(row));
                        }
                        Command::LoadClosedChats(reply) => {
                            let _ = reply.send(db.load_closed_chats());
                        }
                        Command::RemoveClosedChat(chat, reply) => {
                            let _ = reply.send(db.remove_closed_chat(&chat));
                        }
                        Command::RetainChatHistory(keep, closed_since, reply) => {
                            let _ = reply.send(db.retain_chat_history(keep, closed_since));
                        }
                        Command::Shutdown(reply) => {
                            drop(db);
                            let _ = reply.send(Ok(()));
                            return;
                        }
                    }
                }
            })?;
        ready_rx.await.map_err(|_| Error::WorkerStopped)??;
        Ok(Self { sender })
    }
    fn enqueue(&self, command: Command) -> Result<()> {
        self.sender.try_send(command).map_err(|e| match e {
            TrySendError::Full(_) => Error::QueueFull,
            TrySendError::Disconnected(_) => Error::WorkerStopped,
        })
    }
    pub async fn load(&self) -> Result<Snapshot> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::Load(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_integrations(&self) -> Result<crate::integrations::Config> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadIntegrations(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_integrations(&self, config: crate::integrations::Config) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveIntegrations(config, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_tools(&self) -> Result<crate::state::tools::ToolCatalog> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadTools(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_tools(&self, catalog: crate::state::tools::ToolCatalog) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveTools(catalog, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn persist_tool_cleanup(
        &self,
        loaded: crate::state::tools::ToolCatalog,
        failed: Vec<String>,
    ) -> (crate::state::tools::ToolCatalog, Option<String>) {
        let mut cleaned = loaded.clone();
        cleaned.retired_credentials = failed;
        let retry_warning = (!cleaned.retired_credentials.is_empty()).then(|| {
            "Some retired API keys could not be removed from the system credential store. Save Tools to retry cleanup.".to_owned()
        });
        if cleaned == loaded {
            return (loaded, retry_warning);
        }
        if self.save_tools(cleaned.clone()).await.is_ok() {
            (cleaned, retry_warning)
        } else {
            (
                loaded,
                Some("API key cleanup ran, but its retry state could not be saved. The previous cleanup queue was retained and will be retried.".to_owned()),
            )
        }
    }
    pub async fn load_session(&self) -> Result<Option<SessionSnapshot>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadSession(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_session(&self, snapshot: SessionSnapshot) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveSession(snapshot, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_projects(&self) -> Result<ProjectSnapshot> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadProjects(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_projects(&self, snapshot: ProjectSnapshot) -> Result<()> {
        if !snapshot.valid() {
            return Err(Error::InvalidProjectState);
        }
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveProjects(snapshot, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn apply(&self, changes: Vec<Change>) -> Result<Snapshot> {
        if changes.len() > 64 {
            return Err(Error::BatchTooLarge);
        }
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::Apply(changes, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    /// Writes changed chat items in one transaction.
    pub async fn save_chat_history(&self, batches: Vec<super::ChatHistoryBatch>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveChatHistory(batches, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_chat_history_checkpointed(
        &self,
        batches: Vec<super::ChatHistoryBatch>,
    ) -> Result<Vec<super::HistoryCheckpoint>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveChatHistoryCheckpointed(batches, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn read_history_checkpoint(
        &self,
        chat: String,
        keys: Vec<(u64, u64)>,
    ) -> Result<super::HistoryCheckpoint> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::ReadHistoryCheckpoint(chat, keys, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn read_remote_history_page(
        &self,
        request: super::RemoteHistoryRequest,
    ) -> Result<super::RemoteHistoryPage> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::ReadRemoteHistory(request, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_chat_history(&self, chat: String) -> Result<super::ChatHistoryRows> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadChatHistory(chat, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn delete_chat_history(&self, chat: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::DeleteChatHistory(chat, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    /// Deletes every saved chat neither in `keep` nor closed since `closed_since` (unix
    /// seconds); returns the deleted ids.
    pub async fn retain_chat_history(
        &self,
        keep: Vec<String>,
        closed_since: i64,
    ) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RetainChatHistory(keep, closed_since, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_model_catalog(&self, payload: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveModelCatalog(payload, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn load_model_catalog(&self) -> Result<Option<String>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadModelCatalog(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn save_closed_chat(&self, row: super::ClosedChatRow) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::SaveClosedChat(row, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    /// Closed chats, latest first.
    pub async fn load_closed_chats(&self) -> Result<Vec<super::ClosedChatRow>> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::LoadClosedChats(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    pub async fn remove_closed_chat(&self, chat: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::RemoveClosedChat(chat, tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
    /// Completes earlier queued operations and closes SQLite before acknowledging.
    pub async fn shutdown(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::Shutdown(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
}
