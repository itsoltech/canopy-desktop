//! Independent device window. It never selects, closes or restores host-local tabs.
mod catalog;
mod chrome;
mod connections;
pub(crate) use connections::OpenChat;
pub use connections::{Connections, connections};
pub(crate) use connections::{registry, start_saved_connections};
mod discovery;
mod embedded;
pub use embedded::RemoteWorkspace;
mod hosts;
mod inline;
pub use inline::{ConnectionSetup, ConnectionSetupEvent};
mod lifecycle;
mod notes;
mod preview;
pub(crate) use preview::isolated_html;
mod routes;
mod settings;
mod shell;
mod terminal;
#[cfg(test)]
mod tests;
mod workspace;
use super::{components::*, theme as t};
use canopy_desktop::remote::{
    client::{ClientRegistry, HostClient, Phase, pairing::PairingInvite, store::SystemCredentials},
    proto as p,
};
use gpui_kit::component::{
    Disableable, Root,
    input::{InputEvent, InputState, TextareaState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

#[derive(Clone)]
struct ClientGlobal {
    runtime: Arc<tokio::runtime::Runtime>,
    registry: Arc<tokio::sync::OnceCell<ClientRegistry>>,
    notes: Arc<std::sync::Mutex<BTreeMap<String, String>>>,
    connections: Entity<Connections>,
}
fn prepare_connection(cx: &mut App) -> Task<std::result::Result<(), String>> {
    cx.global::<crate::app_state::AppState>()
        .remote
        .clone()
        .update(cx, |state, cx| state.prepare_connection(cx))
}
impl Global for ClientGlobal {}
impl ClientGlobal {
    async fn load_registry(&self) -> std::result::Result<ClientRegistry, String> {
        self.registry
            .get_or_try_init(|| async {
                let path = canopy_desktop::platform::directories::runtime_data_dir()?
                    .join("remote-client-v1.json");
                tokio::task::spawn_blocking(move || {
                    ClientRegistry::open(path, Arc::new(SystemCredentials))
                })
                .await
                .map_err(|_| "Client storage worker failed.".to_owned())?
            })
            .await
            .cloned()
    }
}
fn ensure_global(cx: &mut App) -> Result<()> {
    if !cx.has_global::<ClientGlobal>() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let connections = cx.new(|_| Connections::default());
        cx.set_global(ClientGlobal {
            runtime: Arc::new(runtime),
            registry: Arc::new(tokio::sync::OnceCell::new()),
            notes: Default::default(),
            connections,
        });
    }
    Ok(())
}
pub(crate) fn executor(cx: &mut App) -> Result<Arc<tokio::runtime::Runtime>> {
    ensure_global(cx)?;
    Ok(cx.global::<ClientGlobal>().runtime.clone())
}
/// Device window for one paired computer. Adding a computer and editing its
/// routes happen inline in Settings → Connections (`ConnectionSetup`).
pub fn open_host(host: Arc<HostClient>, cx: &mut App) -> Result<WindowHandle<Root>> {
    ensure_global(cx)?;
    cx.open_window(
        WindowOptions {
            titlebar: Some(titlebar_options("Canopy — Remote")),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1100.), px(760.)),
                cx,
            ))),
            window_min_size: Some(size(px(760.), px(540.))),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| {
                let mut view = RemoteWindow::new(window, cx);
                view.connect_on_open = true;
                view.host = Some(host);
                view
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
}
#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) enum Page {
    Hosts,
    Catalog,
    /// Persisted by older layouts. Restoring it hands the chat to a normal ChatView
    /// tab; the device window never renders a chat itself.
    Chat,
    Files,
    Notes,
    Git,
    Settings,
    Tasks,
    Terminal,
    Terminals,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct DeviceLayout {
    page: Page,
    workspace: Option<Vec<u8>>,
    chat: Option<Vec<u8>>,
    terminal: Option<Vec<u8>>,
    #[serde(default)]
    file: Option<Vec<u8>>,
    #[serde(default)]
    path: String,
}
struct RemoteWindow {
    embedded: bool,
    compact_tools: bool,
    requested_resource: Option<p::ResourceRef>,
    window: AnyWindowHandle,
    global: ClientGlobal,
    registry: Option<ClientRegistry>,
    host: Option<Arc<HostClient>>,
    page: Page,
    fields: BTreeMap<&'static str, Entity<InputState>>,
    file_text: Entity<TextareaState>,
    focus: FocusHandle,
    busy: bool,
    /// Operation in flight; its control shows the loading state.
    pending: Option<&'static str>,
    /// Owner of the current error/info, rendered beside the control that caused it.
    scope: Option<&'static str>,
    error: Option<String>,
    info: String,
    cancelled: Arc<AtomicBool>,
    workspace: Option<p::WorkspaceRef>,
    terminal: Option<p::TerminalRef>,
    agents: Option<p::AgentDiscovery>,
    discovery_busy: bool,
    discovery_abort: Option<tokio::task::AbortHandle>,
    discovery_revision: u64,
    notes: BTreeMap<String, Entity<TextareaState>>,
    stream: Option<tokio::task::AbortHandle>,
    host_watch: Option<Task<()>>,
    subscriptions: Vec<Subscription>,
    file: Option<p::FileRef>,
    file_hash: Vec<u8>,
    directory: Option<p::DirectoryEntries>,
    git: Option<p::GitStatus>,
    git_history: Option<p::GitHistory>,
    git_refs: Option<p::GitRefs>,
    git_diff: Option<String>,
    git_history_tab: bool,
    hovered: Option<SharedString>,
    worktree_flags: [bool; 4],
    git_sign: bool,
    features: settings::FeatureViews,
    terminal_frame: Option<p::TerminalFrame>,
    terminal_run: String,
    terminal_lease: Option<p::InputLease>,
    terminal_tx: Option<tokio::sync::mpsc::Sender<p::TerminalClientEvent>>,
    generation: u64,
    terminal_focus: FocusHandle,
    terminal_preedit: String,
    terminal_bounds: Bounds<Pixels>,
    terminal_size: (u32, u32),
    terminal_cell_size: (f32, f32),
    terminal_input_id: u64,
    terminal_retries: u8,
    terminal_history: Option<p::TerminalFrame>,
    terminal_history_offset: u32,
    terminal_selection: Option<((u32, u32), (u32, u32))>,
    ssh_fingerprint: Option<String>,
    pair_ssh: bool,
    /// Add computer: endpoint override and SSH tuning.
    advanced: Disclosure,
    /// Connection page: device-local label for the selected computer.
    details: Disclosure,
    /// Files: host-side edit marker.
    file_diagnostics: Disclosure,
    /// Changes: remote and upstream for Pull/Push.
    git_remote: Disclosure,
    /// Changes: worktree create/remove/merge/stop.
    worktrees: Disclosure,
    confirmation: Option<Entity<modal::Confirmation>>,
    prompt: Option<Entity<TextPrompt>>,
    modal_events: Option<Subscription>,
    opening_setup: bool,
    /// Settings → Connections renders only this flow (`inline.rs`).
    inline: Option<inline::Inline>,
    connect_on_open: bool,
    routes_on_open: bool,
    route_form: bool,
    route_ssh: bool,
    route_edit: Option<canopy_desktop::remote::client::store::ConnectionRoute>,
    installing_file: bool,
    file_dirty: bool,
    renew: Option<tokio::task::AbortHandle>,
}
type PendingOperation =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Update, String>> + Send>>;
enum Update {
    /// Title, consequence and the reviewed operation. The title selects the
    /// confirmation verb and destructive styling (`confirm_action`).
    Confirm(String, String, PendingOperation),
    Preview(
        Arc<canopy_desktop::integrations::attachments::PreviewFile>,
        String,
    ),
    TextPreview(String, String, bool),
    HtmlPreview(String),
    Done(String),
    Connected,
    Registry(ClientRegistry),
    Paired(Arc<HostClient>),
    Directory(p::DirectoryEntries),
    DirectoryPage(p::DirectoryEntries),
    File(p::FileRef, Vec<u8>, String, bool),
    Git(p::GitStatus),
    GitHistory(p::GitHistory),
    GitRefs(p::GitRefs),
    GitDiff(String),
    SshFingerprint(String),
    RouteSaved,
    TerminalHistory(p::TerminalFrame),
}
/// Confirmation verb and whether the result is destructive.
fn confirm_action(title: &str) -> (&'static str, bool) {
    match title {
        "Discard these changes?" => ("Discard", true),
        "Discard file edits?" => ("Discard edits", true),
        "Close project on host?" => ("Close project", true),
        "Remove worktree?" => ("Remove worktree", true),
        "Merge worktree?" => ("Merge", false),
        "Stop worktree processes?" => ("Stop processes", true),
        "Create worktree?" => ("Create worktree", false),
        "Stop terminal?" => ("Stop", true),
        "Restart terminal?" => ("Restart", true),
        "Export notes?" => ("Export", false),
        "Create workspace entry?" => ("Create", false),
        _ => ("Continue", false),
    }
}
impl RemoteWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let fields = [
            "pair",
            "name",
            "host_label",
            "route_name",
            "endpoint",
            "ssh_address",
            "ssh_port",
            "ssh_user",
            "ssh_secret",
            "ssh_passphrase",
            "path",
            "query",
            "title",
            "agent",
            "tool",
            "profile",
            "goal",
            "commit",
            "branch",
            "worktree_path",
            "start_point",
            "target_branch",
            "remote",
            "upstream_branch",
            "integration",
            "project",
            "project_path",
            "task_title",
            "task_body",
            "credential",
            "environment",
            "environment_value",
            "model",
            "effort",
            "mode",
            "arguments",
            "command",
            "system_prompt",
            "settings_json",
            "owner",
            "email",
            "cloud_id",
            "new_tab",
            "new_worktree",
            "terminal_input",
            "image_path",
        ]
        .into_iter()
        .map(|name| {
            let placeholder = match name {
                "pair" => "canopy://pair…",
                "path" => "Workspace root",
                "query" => "Search files…",
                "project_path" => "/path/on/the/other/computer",
                "tool" => "shell",
                "commit" => "Commit message",
                "remote" => "origin",
                "upstream_branch" => "Same as the current branch",
                "ssh_address" => "Computer name or IP address",
                "ssh_port" => "22",
                "branch" => "feature/name",
                _ => "",
            };
            (
                name,
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .masked(matches!(
                            name,
                            "pair"
                                | "ssh_secret"
                                | "ssh_passphrase"
                                | "credential"
                                | "environment_value"
                                | "system_prompt"
                                | "settings_json"
                        ))
                        .placeholder(placeholder)
                }),
            )
        })
        .collect();
        let global = cx.global::<ClientGlobal>().clone();
        let now = Instant::now();
        let mut this = Self {
            window: window.window_handle(),
            global,
            registry: None,
            host: None,
            page: Page::Hosts,
            embedded: false,
            compact_tools: false,
            requested_resource: None,
            fields,
            file_text: cx.new(|cx| TextareaState::new(window, cx)),
            focus: cx.focus_handle(),
            busy: false,
            pending: None,
            scope: None,
            error: None,
            info: String::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            workspace: None,
            terminal: None,
            agents: None,
            discovery_busy: false,
            discovery_abort: None,
            discovery_revision: 0,
            notes: BTreeMap::new(),
            stream: None,
            host_watch: None,
            subscriptions: vec![],
            file: None,
            file_hash: vec![],
            directory: None,
            git: None,
            git_history: None,
            git_refs: None,
            git_diff: None,
            git_history_tab: false,
            hovered: None,
            worktree_flags: [false; 4],
            git_sign: false,
            features: Default::default(),
            terminal_frame: None,
            terminal_run: String::new(),
            terminal_lease: None,
            terminal_tx: None,
            generation: 0,
            terminal_focus: cx.focus_handle(),
            terminal_preedit: String::new(),
            terminal_bounds: Bounds::default(),
            terminal_size: (0, 0),
            terminal_cell_size: (1., 1.),
            terminal_input_id: 0,
            terminal_retries: 0,
            terminal_history: None,
            terminal_history_offset: 0,
            terminal_selection: None,
            ssh_fingerprint: None,
            pair_ssh: false,
            advanced: Disclosure::new(false, now),
            details: Disclosure::new(false, now),
            file_diagnostics: Disclosure::new(false, now),
            git_remote: Disclosure::new(false, now),
            worktrees: Disclosure::new(false, now),
            confirmation: None,
            prompt: None,
            modal_events: None,
            opening_setup: false,
            inline: None,
            connect_on_open: false,
            routes_on_open: false,
            route_form: false,
            route_ssh: false,
            route_edit: None,
            installing_file: false,
            file_dirty: false,
            renew: None,
        };
        this.subscriptions.push(cx.subscribe(
            &this.file_text,
            |this, input, event: &InputEvent, cx| {
                if this.installing_file || !matches!(event, InputEvent::Change) {
                    return;
                }
                let (Some(host), Some(file)) = (this.host.clone(), this.file.clone()) else {
                    return;
                };
                let text = input.read(cx).value().to_string();
                let hash = this.file_hash.clone();
                let key = workspace::file_view_key(&file);
                let value = serde_json::to_string(&(hash.clone(), text)).unwrap();
                let revision = HostClient::next_view_revision();
                let mark = !this.file_dirty;
                this.file_dirty = true;
                let work = this.global.runtime.spawn(async move {
                    host.save_view_version(key, value, revision)?;
                    if mark && host.phase() == Phase::Ready {
                        host.mutate(None, move |channel, command| async move {
                            p::files_service_client::FilesServiceClient::new(channel)
                                .set_dirty_presence(p::DirtyPresenceRequest {
                                    command: Some(command),
                                    file: Some(file),
                                    base_hash: hash,
                                    dirty: true,
                                })
                                .await
                        })
                        .await?;
                    }
                    Ok::<_, String>(())
                });
                cx.spawn(async move |this, cx| {
                    if let Ok(Err(error)) = work.await {
                        let _ = this.update(cx, |this, cx| {
                            this.error = Some(error);
                            this.scope = Some("editor");
                            cx.notify();
                        });
                    }
                })
                .detach();
                cx.notify();
            },
        ));
        this.fields["name"].update(cx, |field, cx| {
            field.set_value("Canopy desktop", window, cx)
        });
        for name in ["ssh_address", "ssh_port"] {
            this.subscriptions.push(cx.subscribe(
                &this.fields[name],
                |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.ssh_fingerprint = None;
                        cx.notify();
                    }
                },
            ));
        }
        // Enter submits the field's own action; the visible button stays the primary path.
        for name in ["path", "query", "project_path", "pair"] {
            this.subscriptions.push(cx.subscribe(
                &this.fields[name],
                move |this, _, event: &InputEvent, cx| match event {
                    InputEvent::PressEnter { .. } => match name {
                        "path" => this.load_directory(cx),
                        "query" => this.search_files(cx),
                        "project_path" => this.open_project(cx),
                        _ if this.can_request(cx) => this.pair_host(this.pair_ssh, cx),
                        _ => {}
                    },
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            ));
        }
        let global = this.global.clone();
        this.subscriptions
            .push(cx.observe(&global.connections, |_, _, cx| cx.notify()));
        this.run(
            async move { Ok(Update::Registry(global.load_registry().await?)) },
            cx,
        );
        this
    }
    fn value(&self, field: &'static str, cx: &App) -> String {
        self.fields[field].read(cx).value().to_string()
    }
    fn run(
        &mut self,
        future: impl std::future::Future<Output = Result<Update, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.start(None, future, cx);
    }
    /// `run` whose result and loading state belong to one control.
    fn run_in(
        &mut self,
        scope: &'static str,
        future: impl std::future::Future<Output = Result<Update, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.start(Some(scope), future, cx);
    }
    fn start(
        &mut self,
        scope: Option<&'static str>,
        future: impl std::future::Future<Output = Result<Update, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.pending = scope;
        self.scope = scope;
        self.error = None;
        self.info.clear();
        let task = self.global.runtime.spawn(future);
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|_| Err("Remote operation worker stopped.".into()));
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.pending = None;
                if this.generation != generation {
                    cx.notify();
                    return;
                }
                // A progress message ends with its operation; results set their own.
                this.info.clear();
                match result {
                    Ok(update) => this.apply(update, cx),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    /// Notice owned by `scope`: the failure, a running step, or a confirmed result.
    fn notice(&self, scope: &'static str) -> Option<AnyElement> {
        if self.scope != Some(scope) {
            return None;
        }
        if let Some(error) = &self.error {
            return Some(chrome::failure(error.clone()).into_any_element());
        }
        if self.info.is_empty() {
            return None;
        }
        Some(if self.pending == Some(scope) {
            chrome::loading(self.info.clone()).into_any_element()
        } else {
            chrome::done(self.info.clone()).into_any_element()
        })
    }
    fn apply(&mut self, update: Update, cx: &mut Context<Self>) {
        match update {
            Update::HtmlPreview(html) => {
                if let Err(error) = preview::open_html(html, cx) {
                    self.error = Some(error.to_string());
                }
            }
            Update::TextPreview(title, text, markdown) => {
                if let Err(error) = preview::open_text(title, text, markdown, cx) {
                    self.error = Some(error.to_string());
                }
            }
            Update::Preview(file, media) => {
                if let Err(error) = preview::open(file, media, cx) {
                    self.error = Some(error.to_string());
                }
            }
            Update::Confirm(title, detail, operation) => {
                self.open_confirmation(title, detail, operation, cx)
            }
            Update::Registry(registry) => {
                let last = self.host.clone().or_else(|| {
                    registry
                        .hosts()
                        .into_iter()
                        .max_by_key(|host| host.last_seen())
                });
                self.global
                    .connections
                    .update(cx, |view, cx| view.install(registry.clone(), cx));
                self.registry = Some(registry);
                if !self.opening_setup
                    && let Some(host) = last
                {
                    let tool_context = self
                        .compact_tools
                        .then(|| (self.workspace.clone(), self.page));
                    self.select_host(host.clone(), cx);
                    if let Some((workspace, page)) = tool_context {
                        self.workspace = workspace;
                        self.page = page;
                        self.refresh_connection(cx);
                    }
                    self.apply_requested_resource(cx);
                    if std::mem::take(&mut self.routes_on_open) {
                        self.page = Page::Hosts;
                    }
                    if std::mem::take(&mut self.connect_on_open) && host.phase() == Phase::Offline {
                        if self.embedded {
                            self.connect_embedded(host, cx);
                        } else {
                            self.connect(cx);
                        }
                    }
                }
            }
            Update::Paired(host) => {
                if let Some(registry) = &self.registry {
                    self.global
                        .connections
                        .update(cx, |view, cx| view.install(registry.clone(), cx));
                }
                self.opening_setup = false;
                self.select_host(host.clone(), cx);
                self.run_in(
                    "connection",
                    async move {
                        host.connect().await?;
                        Ok(Update::Connected)
                    },
                    cx,
                );
                self.info = "Access approved. Connecting…".into();
                if self.inline.is_some() {
                    cx.emit(ConnectionSetupEvent::Paired);
                }
            }
            Update::RouteSaved => {
                self.route_form = false;
                self.route_edit = None;
                self.info =
                    "Route saved. Connect using it, or reconnect to try routes in order.".into();
                let fields = [
                    self.fields["ssh_secret"].clone(),
                    self.fields["ssh_passphrase"].clone(),
                ];
                let _ = self.window.update(cx, |_, window, cx| {
                    for field in fields {
                        field.update(cx, |f, cx| f.set_value("", window, cx));
                    }
                });
            }
            Update::Done(info) => self.info = info,
            Update::Connected => self.refresh_connection(cx),
            Update::TerminalHistory(frame) => self.terminal_history = Some(frame),
            Update::Directory(value) => {
                if value.workspace != self.workspace {
                    self.error = Some("Directory identity mismatch.".into());
                } else {
                    self.directory = Some(value);
                }
            }
            Update::DirectoryPage(value) => {
                if value.workspace != self.workspace {
                    self.error = Some("Directory identity mismatch.".into());
                    return;
                }
                if let Some(directory) = self.directory.as_mut() {
                    for entry in value.entries {
                        if !directory
                            .entries
                            .iter()
                            .any(|e| e.relative_path == entry.relative_path)
                        {
                            directory.entries.push(entry);
                        }
                    }
                    directory.next_page_token = value.next_page_token;
                    directory.warning = value.warning;
                    directory.incomplete = value.incomplete;
                } else {
                    self.directory = Some(value);
                }
            }
            Update::File(file, hash, text, dirty) => {
                if self.page != Page::Files {
                    self.navigate(Page::Files, cx);
                }
                self.workspace = file.workspace.clone();
                self.file = Some(file);
                self.file_hash = hash;
                self.file_dirty = dirty;
                self.installing_file = true;
                let input = self.file_text.clone();
                let _ = self.window.update(cx, |_, window, cx| {
                    input.update(cx, |state, cx| state.set_value(text, window, cx));
                });
                self.installing_file = false;
                self.save_layout(cx);
                if !dirty && let (Some(host), Some(file)) = (self.host.clone(), self.file.clone()) {
                    let key = workspace::file_view_key(&file);
                    let value = serde_json::to_string(&(
                        self.file_hash.clone(),
                        self.file_text.read(cx).value().to_string(),
                    ))
                    .unwrap();
                    let revision = HostClient::next_view_revision();
                    let task = self
                        .global
                        .runtime
                        .spawn_blocking(move || host.save_view_version(key, value, revision));
                    cx.spawn(async move |this, cx| {
                        if let Ok(Err(error)) = task.await {
                            let _ = this.update(cx, |this, cx| {
                                this.error = Some(error);
                                cx.notify();
                            });
                        }
                    })
                    .detach();
                }
            }
            Update::Git(value) => self.git = Some(value),
            Update::GitHistory(value) => {
                if let Some(history) = self.git_history.as_mut() {
                    for commit in value.commits {
                        if !history.commits.iter().any(|c| c.oid == commit.oid) {
                            history.commits.push(commit);
                        }
                    }
                    history.next_before_oid = value.next_before_oid;
                } else {
                    self.git_history = Some(value);
                }
            }
            Update::GitRefs(value) => self.git_refs = Some(value),
            Update::GitDiff(value) => self.git_diff = Some(value),
            Update::SshFingerprint(value) => self.ssh_fingerprint = Some(value),
        }
    }
    /// Our modal replaces stock OK/Cancel. Accepting starts the reviewed operation;
    /// its progress and result appear beside the control that requested it.
    fn open_confirmation(
        &mut self,
        title: String,
        detail: String,
        operation: PendingOperation,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation.is_some() || self.prompt.is_some() {
            return;
        }
        let (label, destructive) = confirm_action(&title);
        let owner = cx.entity().downgrade();
        let generation = self.generation;
        let scope = self.scope;
        let operation = Arc::new(std::sync::Mutex::new(Some(operation)));
        let focus = self.focus.clone();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let submit_owner = owner.clone();
                let modal = cx.new(|cx| {
                    let mut modal = modal::Confirmation::new(
                        title,
                        detail,
                        label,
                        focus,
                        move |cx| {
                            submit_owner
                                .update(cx, |this, cx| {
                                    if this.generation != generation {
                                        return Err(SharedString::from(
                                            "The selected resource changed. Review the operation again.",
                                        ));
                                    }
                                    if this.busy {
                                        return Err(SharedString::from(
                                            "Wait for the current operation to finish.",
                                        ));
                                    }
                                    let Some(operation) = operation.lock().unwrap().take() else {
                                        return Err(SharedString::from(
                                            "This operation already started.",
                                        ));
                                    };
                                    this.start(scope, operation, cx);
                                    this.info = "Applying the change…".into();
                                    Ok(())
                                })
                                .unwrap_or_else(|_| {
                                    Err(SharedString::from("The remote view was closed."))
                                })
                        },
                        window,
                        cx,
                    );
                    if destructive {
                        modal = modal.destructive();
                    }
                    modal
                });
                let _ = owner.update(cx, |this, cx| {
                    this.modal_events = Some(cx.subscribe(
                        &modal,
                        |this, _, _: &ModalDismissed, cx| {
                            this.confirmation = None;
                            this.modal_events = None;
                            cx.notify();
                        },
                    ));
                    this.confirmation = Some(modal);
                    cx.notify();
                });
            });
        });
    }
    /// One-field modal; `submit` validates and starts the operation, keeping the
    /// modal open with an inline error when it cannot.
    fn open_prompt(
        &mut self,
        title: &'static str,
        label: String,
        value: String,
        confirm: &'static str,
        submit: impl Fn(&mut Self, String, &mut Context<Self>) -> Result<(), String> + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation.is_some() || self.prompt.is_some() {
            return;
        }
        let owner = cx.entity().downgrade();
        let focus = self.focus.clone();
        let window = self.window;
        let submit = std::rc::Rc::new(submit);
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let submit_owner = owner.clone();
                let modal = cx.new(|cx| {
                    TextPrompt::new(
                        title,
                        label,
                        value,
                        focus,
                        move |value, cx| {
                            let submit = submit.clone();
                            let value = value.trim().to_owned();
                            submit_owner
                                .update(cx, |this, cx| {
                                    if this.busy {
                                        return Err(
                                            "Wait for the current operation to finish.".to_owned()
                                        );
                                    }
                                    submit(this, value, cx)
                                })
                                .unwrap_or_else(|_| Err("The remote view was closed.".into()))
                                .map_err(SharedString::from)
                        },
                        window,
                        cx,
                    )
                    .confirm_label(confirm)
                });
                let _ = owner.update(cx, |this, cx| {
                    this.modal_events =
                        Some(cx.subscribe(&modal, |this, _, _: &ModalDismissed, cx| {
                            this.prompt = None;
                            this.modal_events = None;
                            cx.notify();
                        }));
                    this.prompt = Some(modal);
                    cx.notify();
                });
            });
        });
    }
    fn select_host(&mut self, host: Arc<HostClient>, cx: &mut Context<Self>) {
        self.opening_setup = false;
        self.route_form = false;
        self.route_edit = None;
        self.agents = None;
        self.discovery_revision += 1;
        self.discovery_busy = false;
        self.detach();
        let layout = (!self.embedded)
            .then(|| {
                host.snapshot()
                    .record
                    .view("layout")
                    .and_then(|value| serde_json::from_str::<DeviceLayout>(value).ok())
            })
            .flatten();
        self.host = Some(host.clone());
        self.workspace = None;
        self.terminal = None;
        self.file = None;
        self.file_hash.clear();
        self.file_dirty = false;
        self.directory = None;
        self.git = None;
        self.git_history = None;
        self.git_refs = None;
        self.git_diff = None;
        self.features = Default::default();
        self.terminal_frame = None;
        self.terminal_history = None;
        self.terminal_run.clear();
        self.terminal_selection = None;
        self.error = None;
        self.scope = None;
        self.info.clear();
        let secret_fields = [
            "pair",
            "ssh_secret",
            "ssh_passphrase",
            "credential",
            "environment_value",
            "system_prompt",
            "settings_json",
        ]
        .iter()
        .filter_map(|name| self.fields.get(name).cloned())
        .collect::<Vec<_>>();
        let _ = self.window.update(cx, |_, window, cx| {
            for field in secret_fields {
                field.update(cx, |state, cx| state.set_value("", window, cx));
            }
        });
        self.page = Page::Catalog;
        self.generation += 1;
        let events = host.events();
        self.host_watch = Some(cx.spawn(async move |this, cx| {
            let mut phase = host.phase();
            while events.recv().await.is_ok() {
                let next = host.phase();
                if this
                    .update(cx, |this, cx| {
                        if next != Phase::Ready {
                            if let Some(task) = this.renew.take() {
                                task.abort();
                            }
                            this.terminal_lease = None;
                            if this.page == Page::Terminal
                                && let Some(task) = this.stream.take()
                            {
                                task.abort();
                            }
                        } else if phase != Phase::Ready && !this.busy {
                            this.refresh_connection(cx);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                phase = next;
            }
        }));
        if let Some(layout) = layout {
            let owner = cx.entity().downgrade();
            let window = self.window;
            let generation = self.generation;
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        if this.generation != generation {
                            return;
                        }
                        use prost::Message;
                        this.workspace = layout
                            .workspace
                            .and_then(|value| p::WorkspaceRef::decode(value.as_slice()).ok())
                            .filter(|workspace| {
                                this.host
                                    .as_ref()
                                    .is_some_and(|host| workspace.host_id == host.id().as_str())
                            });
                        this.fields["path"]
                            .update(cx, |state, cx| state.set_value(layout.path, window, cx));
                        if let Some(file) = layout
                            .file
                            .and_then(|value| p::FileRef::decode(value.as_slice()).ok())
                            .filter(|file| file.workspace == this.workspace)
                            && let Some(host) = this.host.clone()
                        {
                            let key = workspace::file_view_key(&file);
                            if let Some((hash, text)) =
                                host.snapshot().record.view(&key).and_then(|value| {
                                    serde_json::from_str::<(Vec<u8>, String)>(value).ok()
                                })
                            {
                                this.apply(Update::File(file, hash, text, true), cx);
                            }
                        }
                        match layout.page {
                            Page::Chat => {
                                if let Some(chat) = layout
                                    .chat
                                    .and_then(|value| p::ChatRef::decode(value.as_slice()).ok())
                                    .filter(|chat| {
                                        this.host
                                            .as_ref()
                                            .is_some_and(|host| chat.host_id == host.id().as_str())
                                    })
                                {
                                    this.open_chat(chat, window, cx);
                                }
                            }
                            Page::Terminal => {
                                this.terminal = layout
                                    .terminal
                                    .and_then(|value| p::TerminalRef::decode(value.as_slice()).ok())
                                    .filter(|terminal| {
                                        this.host.as_ref().is_some_and(|host| {
                                            terminal.host_id == host.id().as_str()
                                        })
                                    });
                                this.page = Page::Terminal;
                                if this.ready() {
                                    this.open_terminal(cx);
                                }
                            }
                            Page::Settings => {
                                this.page = Page::Settings;
                                this.load_settings(cx);
                            }
                            Page::Tasks => {
                                this.page = Page::Tasks;
                                this.load_tasks(cx);
                            }
                            page => {
                                this.page = page;
                            }
                        }
                        this.save_layout(cx);
                        cx.notify();
                    });
                });
            });
        }
        cx.notify();
    }
    /// Remote chats open in the normal ChatView tab of the main window.
    fn open_chat(&mut self, chat: p::ChatRef, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.global
            .connections
            .update(cx, |_, cx| cx.emit(connections::OpenChat { host, chat }));
        self.page = Page::Catalog;
        if let Some(task) = self.stream.take() {
            task.abort();
        }
        if !self.embedded {
            window.remove_window();
        }
        cx.notify();
    }
    fn refresh_connection(&mut self, cx: &mut Context<Self>) {
        match self.page {
            Page::Terminal if self.stream.is_none() => self.open_terminal(cx),
            Page::Terminal => {}
            Page::Files => {
                if self.file_dirty {
                    self.dirty_presence(true, cx);
                } else {
                    self.load_directory(cx);
                }
            }
            Page::Git => self.load_git(cx),
            Page::Settings => self.load_settings(cx),
            Page::Tasks => self.load_tasks(cx),
            _ => {}
        }
    }
    fn connect(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let ready = prepare_connection(cx);
        self.run_in(
            "connection",
            async move {
                ready.await?;
                host.connect().await?;
                Ok(Update::Connected)
            },
            cx,
        );
        self.info = "Connecting…".into();
    }
    fn save_layout(&self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let value = serde_json::to_string(&DeviceLayout {
            page: self.page,
            workspace: self.workspace.as_ref().map(prost::Message::encode_to_vec),
            chat: None,
            terminal: self.terminal.as_ref().map(prost::Message::encode_to_vec),
            file: self.file.as_ref().map(prost::Message::encode_to_vec),
            path: self.value("path", cx),
        })
        .unwrap();
        let revision = HostClient::next_view_revision();
        let task = self
            .global
            .runtime
            .spawn_blocking(move || host.save_view_version("layout".into(), value, revision));
        cx.spawn(async move |this, cx| {
            if let Ok(Err(error)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn detach(&mut self) {
        if let Some(abort) = self.discovery_abort.take() {
            abort.abort();
        }
        self.discovery_revision += 1;
        self.discovery_busy = false;
        if let Some(renew) = self.renew.take() {
            renew.abort();
        }
        if let Some(stream) = self.stream.take() {
            stream.abort();
        }
        self.terminal_tx = None;
        if let (Some(host), Some(terminal), Some(lease)) = (
            self.host.clone(),
            self.terminal.clone(),
            self.terminal_lease.take(),
        ) && lease.held
        {
            self.global
                .runtime
                .spawn(async move { host.release_input(terminal, lease).await });
        }
    }
    fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        self.detach();
        self.page = page;
        self.generation += 1;
        self.error = None;
        self.scope = None;
        self.info.clear();
        self.save_layout(cx);
        cx.notify();
    }
    /// Sidebar and tab navigation: switch, then load the page's data.
    fn show_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.navigate(page, cx);
        match page {
            Page::Files => self.load_directory(cx),
            Page::Notes => self.open_notes(cx),
            Page::Git => self.load_git(cx),
            Page::Settings => self.load_settings(cx),
            Page::Tasks => self.load_tasks(cx),
            Page::Catalog if self.embedded => self.discover(cx),
            _ => {}
        }
    }
    fn ready(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|h| h.phase() == Phase::Ready)
            && !self.busy
    }
    fn pair_host(&mut self, ssh: bool, cx: &mut Context<Self>) {
        if self.busy || self.value("pair", cx).trim().is_empty() || self.registry.is_none() {
            return;
        }
        let code = self.value("pair", cx);
        let invite = match PairingInvite::parse(&code) {
            Ok(invite) => invite,
            Err(error) => {
                self.error = Some(error);
                self.scope = Some("pair");
                cx.notify();
                return;
            }
        };
        let ready = prepare_connection(cx);
        let name = self.value("name", cx);
        let endpoint = nonempty(self.value("endpoint", cx));
        let registry = self.registry.clone().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancelled = cancelled.clone();
        let address = self.value("ssh_address", cx);
        let port = self.value("ssh_port", cx).parse().unwrap_or(22);
        let username = self.value("ssh_user", cx);
        let credential = self.value("ssh_secret", cx);
        let passphrase = nonempty(self.value("ssh_passphrase", cx));
        let fingerprint = self.ssh_fingerprint.clone();
        self.run_in("pair", async move {
            ready.await?;
            let id = canopy_desktop::remote::client::HostId::parse(invite.host_id())?;
            if registry.get(&id).is_some() {
                return Err("This computer is already paired. Change its route instead of pairing it again.".into());
            }
            let reference = uuid::Uuid::new_v4().to_string();
            let route = if ssh {
                let pin = fingerprint.ok_or("Check the SSH server and verify its fingerprint first.")?;
                canopy_desktop::remote::client::ssh::save_credentials(registry.vault().as_ref(), &reference, &credential, passphrase.as_deref())?;
                Some(canopy_desktop::remote::client::store::SshRoute::new(address, port, username, pin, reference.clone()))
            } else {
                None
            };
            match invite.enroll_via(name, registry.vault(), cancelled, endpoint, route).await {
                Ok(record) => Ok(Update::Paired(registry.insert(record)?)),
                Err(error) => {
                    if ssh {
                        canopy_desktop::remote::client::ssh::remove_credentials(registry.vault().as_ref(), &reference)?;
                    }
                    Err(error)
                }
            }
        }, cx);
        self.info =
            "Waiting for approval. Approve this device in Connections on the other computer."
                .into();
    }
}
impl Drop for RemoteWindow {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.detach();
    }
}
fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
