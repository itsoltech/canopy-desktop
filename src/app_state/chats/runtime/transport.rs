//! Access to a thread, independent of its durable identity and executor owner.
//!
//! Local operations enter the existing runtime and SQLite bus directly. Only the
//! gRPC implementation owns wire projections, network tasks and device recovery.
use super::*;

mod grpc;
pub(crate) use grpc::Recovering;
use grpc::RemoteSession;

/// A configured account in the executing environment's agent catalog.
pub(crate) struct Account {
    pub tool: String,
    pub provider: Option<Provider>,
    pub agent: String,
    pub id: String,
    pub name: String,
    pub native_default: bool,
}

/// A message within the conversation's durable history parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HistoryPosition {
    pub history_epoch: u64,
    pub item_id: u64,
}

pub(crate) struct HistorySearch {
    pub matches: Vec<HistoryPosition>,
    pub next_page_token: Option<String>,
}

pub(crate) struct SavedMessageEdit {
    pub position: HistoryPosition,
    pub previous: Draft,
    pub current: bool,
}

pub(crate) struct ContextReading {
    pub usage: Option<canopy_desktop::chat::backend::ContextUsage>,
    pub rate: Option<canopy_desktop::chat::codex::adapter::RateLimit>,
    pub source: ContextSource,
}

pub(crate) enum ContextSource {
    Local(Provider),
    Remote(Option<String>),
}

/// Persisted request recovery, independent of wire receipt discriminants.
pub(crate) struct PendingRequest {
    pub id: String,
    pub kind: (&'static str, &'static str),
    pub text: Option<String>,
    pub state: PendingOutcome,
}

#[derive(Default)]
pub(crate) struct ThreadPlace {
    pub project: Option<ThreadProject>,
    pub private: bool,
    pub folder: Option<ThreadFolder>,
    pub host: Option<String>,
}

pub(crate) struct ThreadProject {
    pub name: String,
    pub head: Option<String>,
    pub main: bool,
}

pub(crate) struct ThreadFolder {
    pub path: std::path::PathBuf,
    pub display: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingOutcome {
    Unconfirmed { replayable: bool },
    Processing,
    Unknown,
    Missing,
}

impl PendingOutcome {
    pub(crate) fn can_check(self) -> bool {
        self != Self::Missing
    }
    pub(crate) fn can_send_again(self) -> bool {
        matches!(self, Self::Unconfirmed { replayable: true } | Self::Missing)
    }
}

pub(crate) struct AttachmentSuggestion {
    pub title: String,
    pub detail: String,
    pub name: String,
    pub attachment: canopy_desktop::chat::transcript::Attachment,
    pub folder: bool,
}

/// A destination offered by the executing environment. The view keeps this
/// opaque token rather than interpreting an environment's workspace identity.
#[derive(Clone)]
pub(crate) struct ThreadDestination {
    pub name: String,
    pub path: Option<String>,
    pub selected: bool,
    /// Preserve the destination catalog's existing presentation.
    pub folder_icon: bool,
    target: Destination,
}

#[derive(Clone)]
enum Destination {
    Local(super::super::ChatTarget),
    Hosted {
        host: String,
        workspace: Option<String>,
    },
}

#[derive(Clone)]
pub struct ThreadFile {
    pub target: canopy_desktop::chat::mentions::Target,
    pub tooltip: String,
    origin: FileOrigin,
}

#[derive(Clone)]
enum FileOrigin {
    Device { root: std::path::PathBuf },
    Environment { host: String, workspace: String },
}

impl ThreadFile {
    /// Project tools have their own transport and consume the environment file
    /// address without exposing it to chat rendering.
    pub(crate) fn environment_address(&self) -> Option<(&str, &str, &std::path::Path)> {
        match &self.origin {
            FileOrigin::Device { .. } => None,
            FileOrigin::Environment { host, workspace } => {
                Some((host, workspace, &self.target.path))
            }
        }
    }
}

pub(crate) enum ThreadTransport {
    Local,
    Grpc(Box<RemoteSession>),
}

impl ThreadTransport {
    pub(super) fn observe_catalog(cx: &mut Context<ChatRuntime>) -> Option<Subscription> {
        let catalog = cx.try_global::<AppState>()?.model_catalog.clone();
        Some(cx.observe(&catalog, |_, _, cx| cx.notify()))
    }

    /// Rename a presentation. A subscription's tab label is device-owned; an
    /// executor-owned conversation keeps its existing durable Rename command.
    pub(in crate::app_state) fn rename_presentation(
        projects: &mut crate::app_state::ProjectsState,
        pane: Pane,
        workspace: canopy_desktop::state::workspace::WorkspaceId,
        title: &str,
        cx: &mut Context<crate::app_state::ProjectsState>,
    ) -> Result<(), SharedString> {
        if pane.metadata.remote.is_some() {
            projects.publish_chat_rename(workspace, pane.id, title, cx)
        } else {
            projects.rename_owned_chat(workspace, pane, title, cx)
        }
    }

    /// Existing durable aliases remain the registry identity; an attachment's
    /// device staging namespace is never a catalog conversation.
    pub(in crate::app_state) fn pane_key(pane: &Pane) -> Option<String> {
        pane.metadata
            .remote
            .as_ref()
            .map(|reference| reference.key())
            .or_else(|| {
                pane.metadata
                    .chat
                    .as_ref()
                    .and_then(|settings| settings.id.clone())
            })
    }

    pub(super) fn initialize(
        pane: &Pane,
        settings: &mut ChatSettings,
        cx: &mut Context<ChatRuntime>,
    ) -> (Self, History) {
        if let Some(reference) = &pane.metadata.remote {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(reference.key().as_bytes());
            let mut id = [0; 16];
            id.copy_from_slice(&digest[..16]);
            // Device attachment staging never becomes the host thread identity.
            settings.id = Some(uuid::Uuid::from_bytes(id).to_string());
            let runtime = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = runtime.update(cx, |this, cx| this.connect_remote(cx));
            });
            return (
                Self::Grpc(Box::new(RemoteSession::new(*reference.clone()))),
                History::Loaded,
            );
        }
        let history = match settings.id.clone() {
            Some(chat) => {
                let load = cx
                    .global::<AppState>()
                    .chat_history
                    .clone()
                    .update(cx, |history, cx| history.load(&chat, cx));
                History::Loading(cx.spawn(async move |this, cx| {
                    let saved = load.await;
                    let _ = this.update(cx, |this, cx| this.restore_history(saved, cx));
                }))
            }
            None => {
                settings.id = Some(ChatSettings::new_id());
                let runtime = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = runtime.update(cx, |this, cx| this.persist(None, cx));
                });
                History::Loaded
            }
        };
        (Self::Local, history)
    }

    fn grpc(&self) -> Option<&RemoteSession> {
        match self {
            Self::Local => None,
            Self::Grpc(session) => Some(session),
        }
    }

    fn grpc_mut(&mut self) -> Option<&mut RemoteSession> {
        match self {
            Self::Local => None,
            Self::Grpc(session) => Some(session),
        }
    }
}

impl ChatRuntime {
    pub(in crate::app_state) fn attach_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.attach_pane_local(pane, cx),
            ThreadTransport::Grpc(_) => {
                // A presentation never writes the executor's provider session
                // into this device's local history, including legacy metadata.
                self.pane.id = pane.id;
                if pane.metadata.title.is_some() {
                    self.pane.metadata.title = pane.metadata.title;
                    self.pane.metadata.chat_title_state = pane.metadata.chat_title_state;
                }
                cx.emit(ChatStateChanged);
                cx.notify();
            }
        }
    }

    pub fn snapshot(&self, cx: &App) -> ChatSnapshot {
        match &self.transport {
            ThreadTransport::Local => self.snapshot_local(cx),
            ThreadTransport::Grpc(source) => ChatSnapshot {
                chat: source.reference.chat.clone(),
                tool: self.pane.tool.clone(),
                agent: self.agent_name(cx),
                profile: self.profile(cx),
                title: self.pane.metadata.title.clone(),
                run: self.generation,
                serial: self.serial,
                history_epoch: self.epoch,
                status: self.navigation_status(),
                working: self.is_working(),
                error: self.error.clone(),
                notice: self.notice.as_ref().map(|notice| notice.text.clone()),
                recovery: self.recovery.clone(),
                account_locked: self.account_locked(),
                capabilities: self.capabilities(cx),
                options: self.agent_options(),
                decisions: self.decisions(),
                queue: source.queue_snapshot(),
                observers: self.observers,
                goal: self.goal().cloned(),
                edit_unavailable: (!source.enabled("edit"))
                    .then(|| "This chat can't edit sent messages.".into()),
                shell_unavailable: (!source.enabled("shell"))
                    .then(|| "This chat doesn't take `!` command results.".into()),
            },
        }
    }

    pub fn decisions(&self) -> Vec<PendingDecision> {
        match self.transport {
            ThreadTransport::Local => self.decisions_local(),
            ThreadTransport::Grpc(_) => {
                self.pending_decisions()
                    .into_iter()
                    .filter_map(|id| {
                        let permission = self.transcript()?.items().iter().find_map(|item| {
                            match &item.body {
                                Body::Permission(permission) if item.id == id => Some(permission),
                                _ => None,
                            }
                        })?;
                        Some(PendingDecision {
                            run: self.generation,
                            item: id.0,
                            kind: permission.kind,
                            tool: permission.tool.clone(),
                            title: permission.title.clone(),
                            remember: permission.remember.is_some(),
                            options: self
                                .decision_options(id)
                                .unwrap_or_default()
                                .into_iter()
                                .map(|option| (option.option_id, option.name, option.kind))
                                .collect(),
                        })
                    })
                    .collect()
            }
        }
    }

    pub(crate) fn create_on_environment(
        host: Arc<canopy_desktop::remote::client::HostClient>,
        workspace: Option<String>,
        cx: &mut App,
    ) {
        grpc::create_environment(host, workspace, cx);
    }

    /// Catalog selection before a thread exists still uses the executing
    /// environment's domain accounts, not a protocol discovery response.
    pub(crate) async fn discover_environment_accounts(
        host: Arc<canopy_desktop::remote::client::HostClient>,
        workspace: Option<String>,
    ) -> Result<Vec<Account>, String> {
        grpc::discover_accounts(host, workspace).await
    }

    pub(crate) async fn create_environment_chat(
        host: Arc<canopy_desktop::remote::client::HostClient>,
        workspace: Option<String>,
        agent: String,
        profile: Option<String>,
    ) -> Result<canopy_desktop::state::workspace::RemoteChat, String> {
        grpc::create_chat(host, workspace, agent, profile).await
    }

    pub(crate) fn create_sibling(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => cx
                .global::<AppState>()
                .chats
                .clone()
                .update(cx, |chats, cx| chats.open_current(cx)),
            ThreadTransport::Grpc(_) => self.client_new_chat(cx),
        }
    }

    pub(crate) fn place(&self, cx: &App) -> ThreadPlace {
        match &self.transport {
            ThreadTransport::Grpc(source) => source.place(),
            ThreadTransport::Local => {
                let app = cx.global::<AppState>();
                let projects = app.projects.read(cx);
                let cwd = self.cwd();
                let workspace = self
                    .chat_id()
                    .and_then(|chat| app.chats.read(cx).workspace_of_chat(&chat));
                let project = workspace
                    .and_then(|id| {
                        projects
                            .catalog
                            .items
                            .iter()
                            .find(|project| project.workspace == id)
                    })
                    .or_else(|| {
                        projects.catalog.items.iter().find(|project| {
                            Some(project.worktree_path.as_ref().unwrap_or(&project.path))
                                == cwd.as_ref()
                        })
                    });
                let git = app.git.read(cx);
                ThreadPlace {
                    project: project.map(|project| {
                        let path = project.worktree_path.as_ref().unwrap_or(&project.path);
                        let root = project.repository_path.as_ref().unwrap_or(&project.path);
                        let info = git.repository(root).or_else(|| git.repository(path));
                        let name_root = info.map_or(root, |repository| &repository.root);
                        ThreadProject {
                            name: info
                                .and_then(|repository| repository.origin.as_ref())
                                .map(|origin| origin.path().to_owned())
                                .unwrap_or_else(|| {
                                    name_root
                                        .file_name()
                                        .unwrap_or(name_root.as_os_str())
                                        .to_string_lossy()
                                        .into_owned()
                                }),
                            head: info
                                .and_then(|repository| {
                                    repository
                                        .worktrees
                                        .iter()
                                        .find(|worktree| worktree.path == *path)
                                })
                                .map(|worktree| worktree.head.description()),
                            main: info.is_some_and(|repository| repository.root == *path),
                        }
                    }),
                    private: project.is_none() && workspace.is_some(),
                    folder: cwd.map(|path| ThreadFolder {
                        path,
                        display: None,
                    }),
                    host: None,
                }
            }
        }
    }

    pub(crate) fn tool_file(
        &self,
        call: &str,
        checked: Option<&canopy_desktop::chat::mentions::FileRef>,
    ) -> Option<ThreadFile> {
        match &self.transport {
            ThreadTransport::Local => {
                let file = checked?;
                let target = canopy_desktop::chat::mentions::Target {
                    path: file.path(),
                    line: file.line,
                };
                let tooltip = match file.line {
                    Some(line) => format!("{}:{line}", target.path.display()),
                    None => target.path.display().to_string(),
                };
                Some(ThreadFile {
                    target,
                    tooltip,
                    origin: FileOrigin::Device {
                        root: file.root.clone(),
                    },
                })
            }
            ThreadTransport::Grpc(source) => source.tool_file(call),
        }
    }

    pub(crate) fn open_thread_file(&mut self, file: ThreadFile, cx: &mut Context<Self>) {
        match &file.origin {
            FileOrigin::Device { root } => {
                let root = root.clone();
                cx.spawn(async move |_, cx| {
                    let checked = cx
                        .background_spawn(async move {
                            canopy_desktop::chat::mentions::resolve(
                                &root,
                                &canopy_desktop::chat::mentions::Mention {
                                    range: 0..0,
                                    label: String::new(),
                                    path: file.target.path.to_string_lossy().into_owned(),
                                    line: file.target.line,
                                },
                            )
                        })
                        .await;
                    if let Some(file) = checked {
                        cx.update(|cx| {
                            crate::app_state::FilesState::open_in(
                                file.root,
                                &file.relative,
                                file.line,
                                cx,
                            )
                        });
                    }
                })
                .detach();
            }
            FileOrigin::Environment { host, .. } => {
                if self
                    .transport
                    .grpc()
                    .is_some_and(|source| source.reference.host == *host)
                {
                    cx.emit(ChatRuntimeEvent::FileRequested(file));
                }
            }
        }
    }

    pub(crate) fn context_reading(&self) -> Option<ContextReading> {
        match &self.transport {
            ThreadTransport::Local => {
                let session = self.native()?;
                let (usage, rate) = (session.context(), session.rate_limit());
                (usage.is_some() || rate.is_some()).then(|| ContextReading {
                    usage,
                    rate,
                    source: ContextSource::Local(session.provider()),
                })
            }
            ThreadTransport::Grpc(source) => source.context_reading(),
        }
    }

    pub(crate) fn pending_request(&self) -> Option<PendingRequest> {
        self.transport.grpc()?.pending_request()
    }

    pub(crate) fn check_pending_request(&mut self, cx: &mut Context<Self>) {
        if let ThreadTransport::Grpc(_) = self.transport {
            self.client_reconcile(cx);
        }
    }

    pub(crate) fn retry_pending_request(&mut self, id: String, cx: &mut Context<Self>) {
        if let ThreadTransport::Grpc(_) = self.transport {
            self.client_retry_saved(id, cx);
        }
    }

    pub(crate) fn save_composer_draft(&mut self, draft: Draft, cx: &mut Context<Self>) -> u64 {
        // The same revision fences asynchronous preparation and persisted recovery.
        let revision = canopy_desktop::remote::client::HostClient::next_view_revision();
        match self.transport {
            ThreadTransport::Local => {}
            ThreadTransport::Grpc(_) => self.save_client_draft(draft, revision, cx),
        }
        revision
    }

    pub(crate) fn resolve_tool_paths(
        &self,
        files: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) -> Task<Vec<(String, canopy_desktop::chat::mentions::FileRef)>> {
        match self.transport {
            ThreadTransport::Grpc(_) => Task::ready(vec![]),
            ThreadTransport::Local => {
                let Some(cwd) = self.cwd() else {
                    return Task::ready(vec![]);
                };
                cx.background_spawn(async move {
                    files
                        .into_iter()
                        .filter_map(|(call, path)| {
                            let mention = canopy_desktop::chat::mentions::Mention {
                                range: 0..0,
                                label: String::new(),
                                path,
                                line: None,
                            };
                            Some((
                                call,
                                canopy_desktop::chat::mentions::resolve(&cwd, &mention)?,
                            ))
                        })
                        .collect()
                })
            }
        }
    }

    pub(in crate::app_state) fn registry_key(&self) -> String {
        match &self.transport {
            ThreadTransport::Local => self.chat_id().expect("a runtime has a chat id"),
            ThreadTransport::Grpc(source) => source.reference.key(),
        }
    }

    pub(in crate::app_state) fn owned_chat_id(&self) -> Option<String> {
        match self.transport {
            ThreadTransport::Local => self.chat_id(),
            ThreadTransport::Grpc(_) => None,
        }
    }
    pub(crate) fn local_destinations(cx: &App) -> Vec<ThreadDestination> {
        use super::super::ChatTarget;
        let projects = &cx.global::<AppState>().projects.read(cx).catalog;
        let mut destinations = vec![ThreadDestination {
            name: "Without a project".into(),
            path: None,
            selected: projects.active.is_none(),
            folder_icon: false,
            target: Destination::Local(ChatTarget::NoProject),
        }];
        destinations.extend(projects.items.iter().map(|project| {
            ThreadDestination {
                name: project.name.clone(),
                path: Some(
                    project
                        .worktree_path
                        .as_ref()
                        .unwrap_or(&project.path)
                        .display()
                        .to_string(),
                ),
                selected: projects.active == Some(project.workspace),
                folder_icon: true,
                target: Destination::Local(ChatTarget::Project(project.workspace)),
            }
        }));
        destinations
    }

    pub(crate) fn fork_destinations(&self, cx: &App) -> Vec<ThreadDestination> {
        match &self.transport {
            ThreadTransport::Local => Self::local_destinations(cx),
            ThreadTransport::Grpc(source) => {
                let mut destinations = vec![ThreadDestination {
                    name: "Without a project".into(),
                    path: None,
                    selected: true,
                    folder_icon: false,
                    target: Destination::Hosted {
                        host: source.reference.host.clone(),
                        workspace: None,
                    },
                }];
                if let Some(catalog) = source.host.as_ref().and_then(|host| host.shared_catalog()) {
                    for project in &catalog.projects {
                        for checkout in &project.checkouts {
                            if checkout.closing == Some(true) {
                                continue;
                            }
                            let Some(workspace) = &checkout.workspace else {
                                continue;
                            };
                            destinations.push(ThreadDestination {
                                name: format!("{} · {}", project.name, checkout.name),
                                path: Some(checkout.path.clone()),
                                selected: false,
                                folder_icon: false,
                                target: Destination::Hosted {
                                    host: workspace.host_id.clone(),
                                    workspace: Some(workspace.workspace_id.clone()),
                                },
                            });
                        }
                    }
                }
                destinations
            }
        }
    }

    pub(crate) fn fork_unavailable(&self, cx: &App) -> Option<String> {
        match self.transport {
            ThreadTransport::Local => super::super::Chats::unavailable(cx).map(str::to_owned),
            ThreadTransport::Grpc(_) => None,
        }
    }

    pub(crate) fn fork_into(
        &mut self,
        position: HistoryPosition,
        destination: ThreadDestination,
        cx: &mut Context<Self>,
    ) {
        match (&self.transport, destination.target) {
            (ThreadTransport::Local, Destination::Local(target)) => {
                let Some(request) = self.fork_request(position.history_epoch, position.item_id)
                else {
                    return;
                };
                let chats = cx.global::<AppState>().chats.clone();
                chats.update(cx, |chats, cx| chats.fork(target, request, cx));
            }
            (ThreadTransport::Grpc(source), Destination::Hosted { host, workspace })
                if source.reference.host == host =>
            {
                self.client_fork(position, workspace, cx);
            }
            _ => self.note(
                "The destination belongs to another executing environment.".into(),
                cx,
            ),
        }
    }

    pub(crate) fn create_in(destination: ThreadDestination, cx: &mut App) {
        if let Destination::Local(target) = destination.target {
            cx.global::<AppState>()
                .chats
                .clone()
                .update(cx, |chats, cx| chats.open_new(target, cx));
        }
    }

    pub(crate) fn suggest_attachments(
        &self,
        query: String,
        pulls: bool,
        limit: usize,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<AttachmentSuggestion>, String>> {
        match self.transport {
            ThreadTransport::Grpc(_) => self.suggest_grpc(query, pulls, limit, cx),
            ThreadTransport::Local => {
                let Some(root) = self.cwd() else {
                    return Task::ready(Err("This chat has no folder.".into()));
                };
                if pulls {
                    let integrations = cx.global::<AppState>().integrations.clone();
                    return cx.spawn(async move |_, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(250))
                            .await;
                        let found = integrations
                            .update(cx, |integrations, cx| {
                                integrations.pull_requests(root, query, cx)
                            })
                            .await?;
                        Ok(found
                            .into_iter()
                            .map(|pull| AttachmentSuggestion {
                                title: format!("#{} {}", pull.number, pull.title),
                                detail: format!(
                                    "{} · {} → {} · {}",
                                    pull.author, pull.head, pull.base, pull.state
                                ),
                                name: format!("#{} {}", pull.number, pull.title),
                                folder: false,
                                attachment: canopy_desktop::chat::transcript::Attachment::Pull {
                                    number: pull.number,
                                    title: pull.title,
                                    url: pull.url,
                                    head: pull.head,
                                    base: pull.base,
                                    state: pull.state,
                                    body: pull.body,
                                    label: None,
                                },
                            })
                            .collect())
                    });
                }
                let cancel = canopy_desktop::files::CancelGuard::default();
                let signal = cancel.0.clone();
                cx.spawn(async move |_, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(80))
                        .await;
                    let found = cx
                        .background_spawn(async move {
                            let found = canopy_desktop::files::search::mentions(
                                &root, &query, limit, &signal,
                            )?;
                            Ok::<_, String>(
                                found
                                    .into_iter()
                                    .map(|file| {
                                        let relative = file.path.to_string_lossy().into_owned();
                                        let name = file
                                            .path
                                            .file_name()
                                            .map(|name| name.to_string_lossy().into_owned())
                                            .unwrap_or_else(|| relative.clone());
                                        let slash = if file.folder { "/" } else { "" };
                                        AttachmentSuggestion {
                                            title: format!("{name}{slash}"),
                                            detail: file
                                                .path
                                                .parent()
                                                .map(|parent| parent.to_string_lossy().into_owned())
                                                .unwrap_or_default(),
                                            name: format!("{name}{slash}"),
                                            folder: file.folder,
                                            attachment:
                                                canopy_desktop::chat::transcript::Attachment::Path {
                                                    path: root.join(&file.path),
                                                    label: Some(format!("@{relative}{slash}")),
                                                },
                                        }
                                    })
                                    .collect(),
                            )
                        })
                        .await;
                    drop(cancel);
                    found
                })
            }
        }
    }

    pub(crate) fn preview_message_edit(
        &mut self,
        item: ItemId,
        cx: &mut Context<Self>,
    ) -> (Option<Draft>, Task<Result<EditPreview, String>>) {
        match self.transport {
            ThreadTransport::Grpc(_) => (None, self.preview_edit_grpc(item, cx)),
            ThreadTransport::Local => {
                let original = match self.edit_target(self.generation, self.epoch, item.0, cx) {
                    Ok((_, original)) => original,
                    Err(error) => return (None, Task::ready(Err(error))),
                };
                let preview = self.remote_edit_preview(self.generation, self.epoch, item.0, cx);
                // The owner must finish its bounded dry-run wait even if the
                // composer cancels its observation, or the next edit stays busy.
                let (reply, answer) = futures_channel::oneshot::channel();
                cx.spawn(async move |_, _| {
                    let _ = reply.send(preview.await);
                })
                .detach();
                // The local message is already held in memory. Let its composer
                // open immediately while the existing owner checks file changes.
                (
                    Some(original),
                    cx.spawn(async move |_, _| {
                        let preview = answer
                            .await
                            .map_err(|_| "The edit preview stopped.".to_owned())?;
                        preview.editable.as_ref().map_err(Clone::clone)?;
                        Ok(preview)
                    }),
                )
            }
        }
    }

    pub(crate) fn take_saved_message_edit(&mut self) -> Option<SavedMessageEdit> {
        match self.transport {
            ThreadTransport::Local => None,
            ThreadTransport::Grpc(_) => self.saved_edit_grpc(),
        }
    }

    pub(crate) fn save_message_edit(&mut self, previous: Option<Draft>, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => {}
            ThreadTransport::Grpc(_) => self.save_client_edit(previous, cx),
        }
    }

    pub(crate) fn commit_message_edit(
        &mut self,
        item: ItemId,
        draft: Draft,
        restore: bool,
        revision: u64,
        expected_run: u64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if expected_run != self.generation {
            return Err(
                "The original run has changed. Review the saved request or cancel this edit."
                    .into(),
            );
        }
        match self.transport {
            ThreadTransport::Grpc(_) => {
                if self
                    .transport
                    .grpc()
                    .is_none_or(|session| session.preview.is_none())
                {
                    return Err(format!(
                        "{} no longer has this edit's preview.",
                        self.environment_name()
                            .unwrap_or_else(|| "The other computer".into())
                    ));
                }
                self.client_edit(draft, restore, revision, cx);
            }
            ThreadTransport::Local => {
                let key = canopy_desktop::host::CommandKey::local(uuid::Uuid::new_v4().to_string())
                    .expect("UUID");
                let edited = self.remote_edit(
                    key.clone().into(),
                    ChatCommand::Edit {
                        run: self.generation,
                        history_epoch: self.epoch,
                        item: item.0,
                        draft: canopy_desktop::host::QueueDraft::from(draft),
                        restore_files: restore,
                    },
                    cx,
                );
                let client = self.client(cx);
                cx.spawn(async move |this, cx| {
                    let result = edited.await.and_then(|receipt| {
                        if receipt.state() == canopy_desktop::host::CommandState::Succeeded {
                            return Ok(());
                        }
                        let reason = receipt
                            .result()
                            .and_then(|bytes| {
                                serde_json::from_slice::<serde_json::Value>(bytes).ok()
                            })
                            .and_then(|value| {
                                value
                                    .get("reason")
                                    .and_then(|reason| reason.as_str())
                                    .map(str::to_owned)
                            })
                            .unwrap_or_else(|| "The edit's outcome is not confirmed.".into());
                        Err(reason)
                    });
                    if result.is_ok()
                        && let Some(client) = client
                    {
                        let _ = client.acknowledge_runtime_command(key).await;
                    }
                    let _ = this.update(cx, |_, cx| {
                        cx.emit(ChatRuntimeEvent::EditFinished { revision, result });
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        Ok(())
    }

    pub(crate) fn saved_draft(&self) -> Option<Draft> {
        self.transport
            .grpc()
            .and_then(|session| session.draft.clone())
    }

    pub(crate) fn history_loading(&self) -> bool {
        match &self.transport {
            ThreadTransport::Local => matches!(self.history, History::Loading(_)),
            ThreadTransport::Grpc(session) => session.history_loading,
        }
    }

    pub(crate) fn has_earlier_history(&self) -> bool {
        self.transport
            .grpc()
            .is_some_and(|session| session.projection.has_earlier)
    }

    pub(crate) fn history_error(&self) -> Option<&str> {
        self.transport
            .grpc()
            .and_then(|session| session.history_error.as_deref())
    }

    pub(crate) fn recovering(&self) -> Option<Recovering> {
        self.transport.grpc().and_then(|session| session.recovering)
    }

    pub(crate) fn environment_name(&self) -> Option<String> {
        self.transport
            .grpc()
            .and_then(|session| session.host.as_ref())
            .map(|host| host.name())
            .filter(|name| !name.is_empty())
    }

    pub(crate) fn can_edit_message(&self, item: ItemId) -> bool {
        match &self.transport {
            ThreadTransport::Local => self
                .native()
                .is_some_and(|session| session.fork_point(item).is_some()),
            ThreadTransport::Grpc(session) => session.enabled("edit"),
        }
    }

    pub(crate) fn can_fork_at(&self, position: HistoryPosition, cx: &App) -> bool {
        match self.transport {
            ThreadTransport::Local => self
                .fork_request(position.history_epoch, position.item_id)
                .is_some(),
            ThreadTransport::Grpc(_) => self.capabilities(cx).fork
                && self
                    .transcript_of_epoch(position.history_epoch)
                    .and_then(|transcript| {
                        transcript
                            .items()
                            .iter()
                            .find(|item| item.id.0 == position.item_id)
                    })
                    .is_some_and(
                        |item| matches!(&item.body, Body::TurnEnd(end) if end.resume_at.is_some()),
                    ),
        }
    }

    fn transcript_of_epoch(&self, epoch: u64) -> Option<&Transcript> {
        if epoch == self.epoch {
            self.transcript()
        } else {
            self.archived
                .iter()
                .find(|archived| archived.epoch == epoch)
                .map(|archived| &archived.transcript)
        }
    }

    pub(crate) fn composer_commands(&self) -> Vec<canopy_desktop::chat::config::CommandInfo> {
        let mut commands = self.commands();
        commands.retain(|command| command.name != canopy_desktop::chat::closed::COMMAND);
        if let ThreadTransport::Local = self.transport {
            commands.push(canopy_desktop::chat::config::CommandInfo {
                name: canopy_desktop::chat::closed::COMMAND.into(),
                description: "Resume a closed chat".into(),
                hint: None,
                path: None,
                skill: false,
            });
        }
        commands
    }

    pub(crate) fn command_catalog_loading(&self) -> bool {
        match self.transport {
            ThreadTransport::Local => self
                .backend()
                .is_none_or(|session| matches!(session.status(), Status::Starting)),
            ThreadTransport::Grpc(_) => false,
        }
    }

    /// Search history absent from this device's rendered transcript. A fully loaded
    /// conversation needs only the view's rendered-text search and returns `None`.
    pub(crate) fn search_unloaded_history(
        &self,
        query: String,
        token: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<HistorySearch, String>>> {
        match self.transport {
            ThreadTransport::Local => None,
            ThreadTransport::Grpc(_) => self.search_history_grpc(query, token, cx),
        }
    }

    pub(crate) fn load_history(&mut self, around: Option<HistoryPosition>, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => {}
            ThreadTransport::Grpc(_) => self.load_history_grpc(around, cx),
        }
    }

    pub(crate) fn accounts(&self, cx: &App) -> (Vec<Account>, String) {
        let accounts = match &self.transport {
            ThreadTransport::Grpc(session) => session
                .agents
                .iter()
                .flat_map(|agent| {
                    agent.accounts.iter().map(|account| Account {
                        tool: agent.agent_id.clone(),
                        provider: Provider::from_tool(&agent.agent_id),
                        agent: agent.name.clone(),
                        id: account.account_id.clone(),
                        name: account.label.clone(),
                        native_default: account.native_default,
                    })
                })
                .collect(),
            ThreadTransport::Local => {
                let catalog = &cx.global::<AppState>().tools.read(cx).catalog;
                let mut accounts = vec![];
                for provider in Provider::ALL {
                    let Some(tool) = catalog.get(provider.tool()).filter(|tool| tool.enabled)
                    else {
                        continue;
                    };
                    let short = match provider {
                        Provider::Claude => "Claude",
                        Provider::Codex => "Codex",
                    };
                    accounts.extend(tool.profiles.iter().map(|profile| Account {
                        tool: tool.id.clone(),
                        provider: Some(provider),
                        agent: tool.name.clone(),
                        id: profile.id.clone(),
                        name: if tool.profiles.len() == 1 {
                            provider.name().into()
                        } else {
                            format!("{short} {}", profile.name)
                        },
                        native_default: false,
                    }));
                }
                for tool in catalog.tools.iter().filter(|tool| tool.acp && tool.enabled) {
                    let account = |id, name| Account {
                        tool: tool.id.clone(),
                        provider: None,
                        agent: tool.name.clone(),
                        id,
                        name,
                        native_default: false,
                    };
                    if tool.profiles.is_empty() {
                        accounts.push(account(tool.id.clone(), tool.name.clone()));
                    }
                    accounts.extend(tool.profiles.iter().map(|profile| {
                        account(
                            profile.id.clone(),
                            if tool.profiles.len() == 1 {
                                tool.name.clone()
                            } else {
                                format!("{} {}", tool.name, profile.name)
                            },
                        )
                    }));
                }
                accounts
            }
        };
        (accounts, self.profile(cx))
    }

    pub(crate) fn account_models(
        &self,
        account: &str,
        cx: &App,
    ) -> Vec<canopy_desktop::chat::config::ModelInfo> {
        if account == self.profile(cx) {
            return self.models(cx);
        }
        match self.transport {
            ThreadTransport::Local => cx
                .global::<AppState>()
                .model_catalog
                .read(cx)
                .models(account)
                .map(<[_]>::to_vec)
                .unwrap_or_default(),
            ThreadTransport::Grpc(_) => vec![],
        }
    }

    pub(crate) fn refresh_account_models(&self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Grpc(_) => {}
            ThreadTransport::Local => {
                let (accounts, current) = self.accounts(cx);
                let locked = self.account_locked();
                let live = self
                    .native()
                    .is_some_and(|session| !session.models().is_empty());
                let catalog = cx.global::<AppState>().model_catalog.clone();
                for account in accounts {
                    let here = account.id == current;
                    let Some(provider) = account
                        .provider
                        .filter(|_| !(locked && !here || here && live))
                    else {
                        continue;
                    };
                    catalog.update(cx, |catalog, cx| {
                        catalog.refresh(provider, account.id, &self.pane, cx)
                    });
                }
            }
        }
    }

    pub(crate) fn account_models_note(&self, account: &str, cx: &App) -> Option<String> {
        let current = self.profile(cx);
        match self.transport {
            ThreadTransport::Grpc(_) if account != current => {
                Some("Select this account to load its models from the host.".into())
            }
            ThreadTransport::Grpc(_) => None,
            ThreadTransport::Local => {
                let live = self
                    .native()
                    .is_some_and(|session| !session.models().is_empty());
                let catalog = cx.global::<AppState>().model_catalog.read(cx);
                if (account == current && live)
                    || catalog.models(account).is_some()
                    || !self
                        .accounts(cx)
                        .0
                        .iter()
                        .any(|candidate| candidate.id == account && candidate.provider.is_some())
                {
                    return None;
                }
                Some(if catalog.fetching(account) {
                    "Loading models…".into()
                } else if let Some(error) = catalog.error(account) {
                    format!("Couldn't load models: {error}")
                } else if account == current {
                    "The agent lists its models once it has started.".into()
                } else {
                    "Loading models…".into()
                })
            }
        }
    }

    pub(crate) fn attachment_paths(
        &self,
        paths: Vec<std::path::PathBuf>,
    ) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
        match self.transport {
            ThreadTransport::Local => (paths, vec![]),
            ThreadTransport::Grpc(_) => paths
                .into_iter()
                .partition(|path| canopy_desktop::chat::attachments::is_image_path(path)),
        }
    }

    pub(crate) fn attachment_image(
        &self,
        attachment: canopy_desktop::chat::transcript::Attachment,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<u8>>> {
        match self.transport {
            ThreadTransport::Local => self.device_image(attachment, cx),
            ThreadTransport::Grpc(_) => self.image_grpc(attachment, cx),
        }
    }

    fn device_image(
        &self,
        attachment: canopy_desktop::chat::transcript::Attachment,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<u8>>> {
        let chat = self.chat_id();
        cx.background_spawn(async move {
            let data = canopy_desktop::platform::directories::data_dir().ok()?;
            let folder = canopy_desktop::chat::history::attachments_dir(&data, chat.as_deref()?);
            canopy_desktop::chat::attachments::image_bytes(&folder, &attachment)
        })
    }

    pub(crate) fn thread_page(
        &self,
        call: String,
        source: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<String, String>> {
        match self.transport {
            ThreadTransport::Local => cx.background_spawn(async move {
                Ok(canopy_desktop::chat::page::inline_images(&source))
            }),
            ThreadTransport::Grpc(_) => self.page_grpc(call, source, cx),
        }
    }

    /// Resolve saved images in the executor's attachment namespace. Host artifact
    /// references are retained by gRPC and must never be read as device paths.
    pub(crate) fn prepare_thread_draft(
        &self,
        draft: Draft,
        cx: &mut Context<Self>,
    ) -> Task<Result<Draft, String>> {
        match self.transport {
            ThreadTransport::Grpc(_) => Task::ready(Ok(draft)),
            ThreadTransport::Local => {
                if !canopy_desktop::chat::attachments::needs_data(&draft.attachments) {
                    return Task::ready(Ok(draft));
                }
                let chat = self.chat_id();
                cx.background_spawn(async move {
                    let chat = chat.ok_or("This chat can't keep attachments.")?;
                    let data = canopy_desktop::platform::directories::data_dir()?;
                    let folder = canopy_desktop::chat::history::attachments_dir(&data, &chat);
                    Ok(Draft {
                        attachments: canopy_desktop::chat::attachments::with_data(
                            &folder,
                            draft.attachments,
                        )?,
                        ..draft
                    })
                })
            }
        }
    }

    pub(crate) fn submit_composer_draft(
        &mut self,
        draft: Draft,
        steer: bool,
        revision: u64,
        cx: &mut Context<Self>,
    ) {
        match self.transport {
            ThreadTransport::Grpc(_) => self.send_remote(draft, steer, cx),
            ThreadTransport::Local => {
                let target = (
                    self.pane.tool.clone(),
                    self.pane.metadata.profile_id.clone(),
                    self.epoch,
                );
                let run = self.generation;
                let prepared = self.prepare_thread_draft(draft, cx);
                cx.spawn(async move |this, cx| {
                    let result = prepared.await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok(draft) => {
                            if this.closed
                                || target != (this.pane.tool.clone(), this.pane.metadata.profile_id.clone(), this.epoch)
                                || (steer && this.generation != run)
                            {
                                this.note("The chat changed while preparing attachments. Your draft is kept; review it before sending.".into(), cx);
                                return;
                            }
                            cx.emit(ChatRuntimeEvent::DraftConsumed(revision));
                            if steer {
                                this.steer_draft(draft, cx);
                            } else {
                                this.send_draft(draft, cx);
                            }
                        }
                        Err(error) => this.note(error, cx),
                    });
                })
                .detach();
            }
        }
    }

    pub(crate) fn submit_composer_shell(
        &mut self,
        command: String,
        revision: u64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match self.transport {
            ThreadTransport::Local => {
                self.run_shell(&command, cx)?;
                cx.emit(ChatRuntimeEvent::DraftConsumed(revision));
            }
            ThreadTransport::Grpc(_) => self.client_shell(command, cx),
        }
        Ok(())
    }

    pub(crate) fn retry_last(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Grpc(_) => {
                let draft = self.transcript().and_then(|transcript| {
                    transcript
                        .items()
                        .iter()
                        .rev()
                        .find_map(|item| match &item.body {
                            Body::User(message) if item.parent.is_none() => Some(Draft {
                                text: message.text.clone(),
                                attachments: message.attachments.clone(),
                                context: message.context.clone(),
                                goal: message.goal,
                            }),
                            _ => None,
                        })
                });
                if let Some(draft) = draft {
                    self.retry_remote(draft, cx);
                }
            }
            ThreadTransport::Local => {
                let Some(draft) = self.native().and_then(ChatSession::retry_draft) else {
                    return;
                };
                let prepared = self.prepare_thread_draft(draft, cx);
                cx.spawn(async move |this, cx| {
                    let result = prepared.await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok(draft) => this.send_draft(draft, cx),
                        Err(error) => this.note(error, cx),
                    });
                })
                .detach();
            }
        }
    }

    pub fn queue_lines(&self) -> Vec<QueueLine> {
        match &self.transport {
            ThreadTransport::Local => self.queue_lines_local(),
            ThreadTransport::Grpc(session) => session.lines(),
        }
    }

    pub fn queue_paused(&self) -> bool {
        match &self.transport {
            ThreadTransport::Local => self.queue_paused_local(),
            ThreadTransport::Grpc(session) => {
                session
                    .lines()
                    .iter()
                    .any(|line| line.id.starts_with("device:"))
                    || session
                        .projection
                        .queue
                        .as_ref()
                        .is_some_and(|queue| queue.paused)
            }
        }
    }

    pub(super) fn save_record(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.save_record_local(cx),
            ThreadTransport::Grpc(_) => {}
        }
    }

    pub fn send_draft(&mut self, draft: Draft, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.send_draft_as(draft, false, cx),
            ThreadTransport::Grpc(_) => self.send_remote(draft, false, cx),
        }
    }

    pub fn resume_queue(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.resume_queue_local(cx),
            ThreadTransport::Grpc(_) => self.client_resume(cx),
        }
    }

    pub fn move_queued(&mut self, message: String, before: String, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.move_queued_local(message, before, cx),
            ThreadTransport::Grpc(_) => self.move_queued_grpc(message, before, cx),
        }
    }

    #[allow(dead_code, reason = "typed queue API")]
    pub fn edit_queued(
        &mut self,
        message: String,
        draft: Draft,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        match self.transport {
            ThreadTransport::Local => self.edit_queued_local(message, draft, cx),
            ThreadTransport::Grpc(_) => self.edit_queued_grpc(message, draft, cx),
        }
    }

    pub fn remove_queued(&mut self, message: String, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.remove_queued_local(message, cx),
            ThreadTransport::Grpc(_) => self.remove_queued_grpc(message, cx),
        }
    }

    pub fn take_queued(
        &mut self,
        message: String,
        replacement: Draft,
        cx: &mut Context<Self>,
    ) -> Task<Option<Draft>> {
        match self.transport {
            ThreadTransport::Local => self.take_queued_local(message, replacement, cx),
            ThreadTransport::Grpc(_) => self.client_take_queued(message, replacement, cx),
        }
    }

    pub fn resolve_unknown(
        &mut self,
        message: String,
        resolution: canopy_desktop::host::UnknownResolution,
        cx: &mut Context<Self>,
    ) {
        match self.transport {
            ThreadTransport::Local => self.resolve_unknown_local(message, resolution, cx),
            ThreadTransport::Grpc(_) => self.resolve_unknown_grpc(message, resolution, cx),
        }
    }

    pub fn steer_queued(&mut self, message: String, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.steer_queued_local(message, cx),
            ThreadTransport::Grpc(_) => self.client_steer_queued(message, cx),
        }
    }

    pub fn first_queued(&self) -> Option<String> {
        match &self.transport {
            ThreadTransport::Local => self.first_queued_local(),
            ThreadTransport::Grpc(session) => session
                .lines()
                .into_iter()
                .find(|line| line.state == LineState::Queued)
                .map(|line| line.id),
        }
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.start_local(cx),
            ThreadTransport::Grpc(_) => self.connect_remote(cx),
        }
    }

    pub fn set_presented(&mut self, presented: bool, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.set_presented_local(presented, cx),
            ThreadTransport::Grpc(_) => self.presented = presented,
        }
    }

    pub fn restart(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.restart_local(cx),
            ThreadTransport::Grpc(_) => self.remote_command("restart", cx),
        }
    }

    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        match &mut self.transport {
            ThreadTransport::Local => self.shutdown_local(cx),
            ThreadTransport::Grpc(session) => {
                session.stop();
                Task::ready(Ok(()))
            }
        }
    }

    /// Stop processes owned by this application when its local worktree stops.
    /// Observing another environment contributes no executor to that worktree.
    pub(in crate::app_state) fn stop_owned_execution(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        match self.transport {
            ThreadTransport::Local => self.shutdown_local(cx),
            ThreadTransport::Grpc(_) => Task::ready(Ok(())),
        }
    }

    pub fn set_account(
        &mut self,
        tool: String,
        profile: String,
        model: Option<String>,
        cx: &mut Context<Self>,
    ) {
        match self.transport {
            ThreadTransport::Local => self.set_account_local(tool, profile, model, cx),
            ThreadTransport::Grpc(_) => self.client_set_agent(tool, profile, model, cx),
        }
    }

    pub fn accepts_messages(&self) -> bool {
        match &self.transport {
            ThreadTransport::Local => self.accepts_messages_local(),
            ThreadTransport::Grpc(session) => {
                session.host.is_some() && !session.busy && session.projection.summary.is_some()
            }
        }
    }

    pub fn steer_draft(&mut self, draft: Draft, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.steer_draft_local(draft, cx),
            ThreadTransport::Grpc(_) => self.send_remote(draft, true, cx),
        }
    }

    pub fn interrupt(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.interrupt_local(cx),
            ThreadTransport::Grpc(_) => self.remote_command("interrupt", cx),
        }
    }

    pub fn respond(&mut self, item: ItemId, response: Response, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.respond_local(item, response, cx),
            ThreadTransport::Grpc(_) => self.respond_grpc(item, response, cx),
        }
    }

    pub fn respond_option(&mut self, item: ItemId, option: Option<String>, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.respond_option_local(item, option, cx),
            ThreadTransport::Grpc(_) => self.respond_option_grpc(item, option, cx),
        }
    }

    pub fn set_agent_option(&mut self, id: &str, value: &str, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.set_agent_option_local(id, value, cx),
            ThreadTransport::Grpc(_) => self.remote_option(id.into(), value.into(), cx),
        }
    }

    pub fn stop_background(&mut self, call: &str, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.stop_background_local(call, cx),
            ThreadTransport::Grpc(_) => self.client_stop_background(call.into(), cx),
        }
    }

    pub fn pause_goal(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.pause_goal_local(cx),
            ThreadTransport::Grpc(_) => self.goal_grpc(ChatCommand::PauseGoal, cx),
        }
    }

    pub fn resume_goal(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.resume_goal_local(cx),
            ThreadTransport::Grpc(_) => self.goal_grpc(ChatCommand::ResumeGoal, cx),
        }
    }

    pub fn clear_goal(&mut self, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.clear_goal_local(cx),
            ThreadTransport::Grpc(_) => self.goal_grpc(ChatCommand::ClearGoal, cx),
        }
    }

    pub fn set_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.set_model_local(model, cx),
            ThreadTransport::Grpc(_) => {
                self.remote_option("model".into(), model.unwrap_or_default(), cx)
            }
        }
    }

    pub fn set_effort(&mut self, effort: Option<Effort>, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.set_effort_local(effort, cx),
            ThreadTransport::Grpc(_) => self.remote_option(
                "effort".into(),
                effort.map(|e| e.key()).unwrap_or_default().into(),
                cx,
            ),
        }
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.set_mode_local(mode, cx),
            ThreadTransport::Grpc(_) => self.remote_option("mode".into(), mode.key().into(), cx),
        }
    }

    pub(super) fn persist(&mut self, account: Option<(String, String)>, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.persist_local(account, cx),
            ThreadTransport::Grpc(_) => {}
        }
    }

    pub fn stop_shell(&mut self, id: ItemId, cx: &mut Context<Self>) {
        match self.transport {
            ThreadTransport::Local => self.stop_shell_local(id, cx),
            ThreadTransport::Grpc(_) => self.client_stop_shell(id, cx),
        }
    }

    pub fn agent_name(&self, cx: &App) -> String {
        match &self.transport {
            ThreadTransport::Local => self.local_agent_name(cx),
            ThreadTransport::Grpc(session) => session
                .agents
                .iter()
                .find(|agent| agent.agent_id == self.pane.tool)
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| self.pane.tool.clone()),
        }
    }

    pub fn provider(&self) -> Option<Provider> {
        match &self.transport {
            ThreadTransport::Local => Provider::from_tool(&self.pane.tool),
            ThreadTransport::Grpc(session) => match session.backend(&self.pane.tool)? {
                BackendKind::Claude => Some(Provider::Claude),
                BackendKind::Codex => Some(Provider::Codex),
                BackendKind::Acp => None,
            },
        }
    }

    pub fn profile(&self, cx: &App) -> String {
        match &self.transport {
            ThreadTransport::Local => self.local_profile(cx),
            ThreadTransport::Grpc(_) => self
                .pane
                .metadata
                .profile_id
                .clone()
                .unwrap_or_else(|| self.pane.tool.clone()),
        }
    }

    pub fn status(&self) -> Option<&Status> {
        match &self.transport {
            ThreadTransport::Local => self.session.as_ref().map(BackendSession::status),
            ThreadTransport::Grpc(session) => Some(&session.status),
        }
    }

    pub fn dormant(&self) -> bool {
        match &self.transport {
            ThreadTransport::Local => {
                self.session.is_none() && !self.starting && !self.closed && self.error.is_none()
            }
            ThreadTransport::Grpc(_) => false,
        }
    }

    pub fn busy(&self) -> bool {
        match &self.transport {
            ThreadTransport::Local => self.starting || !self.closing_done(),
            ThreadTransport::Grpc(session) => session.busy,
        }
    }

    pub fn shell_running(&self, id: ItemId) -> bool {
        match &self.transport {
            ThreadTransport::Local => self.shells.contains_key(&id),
            ThreadTransport::Grpc(session) => session.transcript.items().iter().any(|item| {
                item.id == id && matches!(&item.body, Body::Shell(shell) if shell.running)
            }),
        }
    }

    pub fn account_locked(&self) -> bool {
        match &self.transport {
            ThreadTransport::Local => self.local_account_locked(),
            ThreadTransport::Grpc(session) => {
                session.busy
                    || !session.transcript.items().is_empty()
                    || !self.archived.is_empty()
                    || session
                        .projection
                        .queue
                        .as_ref()
                        .is_some_and(|queue| !queue.entries.is_empty())
            }
        }
    }

    pub fn goal(&self) -> Option<&canopy_desktop::chat::goal::Goal> {
        match &self.transport {
            ThreadTransport::Local => self.native().and_then(ChatSession::goal),
            ThreadTransport::Grpc(session) => session.goal.as_ref(),
        }
    }

    pub fn ready(&self) -> bool {
        match &self.transport {
            ThreadTransport::Local => self.backend().is_some(),
            ThreadTransport::Grpc(session) => session.projection.summary.is_some(),
        }
    }

    pub fn commands(&self) -> Vec<canopy_desktop::chat::config::CommandInfo> {
        match &self.transport {
            ThreadTransport::Local => self
                .backend()
                .map(|s| s.commands().to_vec())
                .unwrap_or_default(),
            ThreadTransport::Grpc(session) => session.commands(),
        }
    }

    pub fn transcript(&self) -> Option<&Transcript> {
        match &self.transport {
            ThreadTransport::Local => self.session.as_ref().map(BackendSession::transcript),
            ThreadTransport::Grpc(session) => Some(&session.transcript),
        }
    }

    pub fn capabilities(&self, cx: &App) -> Capabilities {
        match &self.transport {
            ThreadTransport::Local => self.local_capabilities(cx),
            ThreadTransport::Grpc(session) => session.capabilities(),
        }
    }

    pub fn backend_kind(&self, cx: &App) -> Option<BackendKind> {
        match &self.transport {
            ThreadTransport::Local => backend_of(&self.pane.tool, cx),
            ThreadTransport::Grpc(session) => session.backend(&self.pane.tool),
        }
    }

    pub fn agent_options(&self) -> Vec<AgentOption> {
        match &self.transport {
            ThreadTransport::Local => self.local_agent_options(),
            ThreadTransport::Grpc(session) => session.options(),
        }
    }

    pub fn pending_decisions(&self) -> Vec<ItemId> {
        match &self.transport {
            ThreadTransport::Local => self.local_pending_decisions(),
            ThreadTransport::Grpc(session) => session
                .transcript
                .items()
                .iter()
                .filter_map(|item| {
                    matches!(&item.body, Body::Permission(permission)
                    if permission.decision == canopy_desktop::chat::transcript::Decision::Pending)
                    .then_some(item.id)
                })
                .collect(),
        }
    }

    pub fn decision_options(&self, item: ItemId) -> Option<Vec<acp::PermissionOption>> {
        match &self.transport {
            ThreadTransport::Local => self.local_decision_options(item),
            ThreadTransport::Grpc(session) => session.decision_options(self.epoch, item),
        }
    }

    pub fn models(&self, cx: &App) -> Vec<canopy_desktop::chat::config::ModelInfo> {
        match &self.transport {
            ThreadTransport::Local => self.local_models(cx),
            ThreadTransport::Grpc(session) => session.models(),
        }
    }
}

impl super::super::Chats {
    /// Release a presentation according to its resource owner. A subscription
    /// detaches without touching the host's catalog, history, queue or executor.
    pub(in crate::app_state) fn release_thread_view(
        &mut self,
        id: canopy_desktop::state::workspace::PaneId,
        pane: Pane,
        workspace: canopy_desktop::state::workspace::WorkspaceId,
        chat: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if pane.metadata.remote.is_some() {
            self.detaching.remove(&id);
            if let Some(chat) = chat
                && !self.panes.values().any(|key| key == &chat)
                && let Some(runtime) = self.drop_runtime(&chat)
            {
                runtime
                    .update(cx, |runtime, cx| runtime.shutdown(cx))
                    .detach();
            }
            return;
        }
        if self.detaching.remove(&id) {
            if let Some(chat) = chat {
                self.workspaces.insert(chat.clone(), workspace);
                self.detached.insert(
                    chat.clone(),
                    super::super::DetachedChat {
                        chat,
                        id,
                        workspace,
                        tool: pane.tool.clone(),
                        title: pane
                            .metadata
                            .title
                            .clone()
                            .unwrap_or_else(|| crate::ui::chat::NEW_CHAT_TITLE.into()),
                    },
                );
            }
            return;
        }
        let runtime = chat.as_ref().and_then(|chat| self.drop_runtime(chat));
        self.retire(workspace, pane, runtime, cx);
    }
}
