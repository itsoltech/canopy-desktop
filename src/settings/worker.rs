use super::{
    Access, Change, Error, ImportReport, Result, Snapshot,
    database::{self, Database},
};
use futures_channel::oneshot;
use std::{
    path::PathBuf,
    sync::mpsc::{self, SyncSender, TrySendError},
};

use crate::state::{projects::ProjectSnapshot, session::SessionSnapshot};

type Reply<T> = oneshot::Sender<Result<T>>;
enum Command {
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
    Shutdown(Reply<()>),
}

/// Cloneable async handle. The SQLite connection never leaves its dedicated thread.
#[derive(Clone)]
pub struct SettingsClient {
    sender: SyncSender<Command>,
}
impl SettingsClient {
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
    /// Completes earlier queued operations and closes SQLite before acknowledging.
    pub async fn shutdown(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(Command::Shutdown(tx))?;
        rx.await.map_err(|_| Error::WorkerStopped)?
    }
}
