//! Independent device window. It never selects, closes or restores host-local tabs.
mod activity;
mod chat;
mod discovery;
mod editing;
mod lifecycle;
mod notes;
mod preview;
mod settings;
mod terminal;
mod workspace;
use super::{components::*, theme as t};
use canopy_desktop::remote::{
    client::{
        ChatProjection, ClientRegistry, HostClient, Phase, pairing::PairingInvite,
        store::SystemCredentials,
    },
    proto as p,
};
use gpui_kit::component::{
    Disableable, Root,
    input::{InputEvent, InputState, TextareaState},
};
use gpui_kit::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone)]
struct ClientGlobal {
    runtime: Arc<tokio::runtime::Runtime>,
    registry: Arc<tokio::sync::OnceCell<ClientRegistry>>,
    notes: Arc<std::sync::Mutex<BTreeMap<String, String>>>,
}
impl Global for ClientGlobal {}
pub fn open(cx: &mut App) -> Result<WindowHandle<Root>> {
    if !cx.has_global::<ClientGlobal>() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        cx.set_global(ClientGlobal {
            runtime: Arc::new(runtime),
            registry: Arc::new(tokio::sync::OnceCell::new()),
            notes: Default::default(),
        });
    }
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
            let view = cx.new(|cx| RemoteWindow::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
}
#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
enum Page {
    Hosts,
    Catalog,
    Chat,
    Files,
    Notes,
    Git,
    Settings,
    Tasks,
    Terminal,
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
    window: AnyWindowHandle,
    global: ClientGlobal,
    registry: Option<ClientRegistry>,
    host: Option<Arc<HostClient>>,
    page: Page,
    fields: BTreeMap<&'static str, Entity<InputState>>,
    composer: Entity<TextareaState>,
    file_text: Entity<TextareaState>,
    busy: bool,
    error: Option<String>,
    info: String,
    cancelled: Arc<AtomicBool>,
    chat: Option<p::ChatRef>,
    workspace: Option<p::WorkspaceRef>,
    terminal: Option<p::TerminalRef>,
    projection: ChatProjection,
    markdown: BTreeMap<String, Entity<super::markdown::MarkdownView>>,
    chat_list: ListState,
    chat_rows: Vec<(u64, u64, u64)>,
    chat_indices: Vec<usize>,
    options: Option<p::AgentOptions>,
    fork_request: Option<(p::ItemKey, bool)>,
    editing: Option<editing::Editing>,
    shell_mode: bool,
    agents: Option<p::AgentDiscovery>,
    agent_catalog: Option<p::AgentCatalog>,
    discovery_busy: bool,
    discovery_abort: Option<tokio::task::AbortHandle>,
    discovery_revision: u64,
    pulls: Option<p::PullRequests>,
    mentions: Option<p::DirectoryEntries>,
    notes: BTreeMap<String, Entity<TextareaState>>,
    activity_expanded: std::collections::BTreeSet<String>,
    activity_thread: Option<(u64, String)>,
    activity_children: std::collections::BTreeSet<(u64, String)>,
    history_token: Option<String>,
    history_matches: Option<p::HistoryMatches>,
    history_query: String,
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
    worktree_flags: [bool; 4],
    git_sign: bool,
    features: settings::FeatureViews,
    attachments: Vec<p::Attachment>,
    terminal_frame: Option<p::TerminalFrame>,
    terminal_run: String,
    terminal_lease: Option<p::InputLease>,
    terminal_tx: Option<tokio::sync::mpsc::Sender<p::TerminalClientEvent>>,
    decisions: BTreeMap<String, Entity<chat::DecisionView>>,
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
    local_images: Vec<String>,
    ssh_fingerprint: Option<String>,
    installing_file: bool,
    file_dirty: bool,
    draft_context: Option<p::SendContext>,
    composer_revision: u64,
    renew: Option<tokio::task::AbortHandle>,
}
type PendingOperation =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Update, String>> + Send>>;
enum Update {
    Confirm(String, String, PendingOperation),
    Preview(
        Arc<canopy_desktop::integrations::attachments::PreviewFile>,
        String,
    ),
    TextPreview(String, String, bool),
    HtmlPreview(String),
    EditPreview(p::EditPreview),
    EditSent(p::ItemKey, Option<(u64, bool)>),
    Done(String),
    Connected,
    Registry(ClientRegistry),
    Paired(Arc<HostClient>),
    Options(p::AgentOptions),
    Directory(p::DirectoryEntries),
    DirectoryPage(p::DirectoryEntries),
    File(p::FileRef, Vec<u8>, String, bool),
    Git(p::GitStatus),
    GitHistory(p::GitHistory),
    GitRefs(p::GitRefs),
    GitDiff(String),
    History(Vec<p::TranscriptItem>, Option<String>),
    HistoryMatches(p::HistoryMatches),
    HistoryJump(Vec<p::TranscriptItem>, p::ItemKey),
    SshFingerprint(String),
    LocalAttachment(String),
    TerminalHistory(p::TerminalFrame),
    ComposerStored(u64, bool, String),
}
impl RemoteWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let fields = [
            "pair",
            "name",
            "host_label",
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
            (
                name,
                cx.new(|cx| {
                    InputState::new(window, cx).masked(matches!(
                        name,
                        "pair"
                            | "ssh_secret"
                            | "ssh_passphrase"
                            | "credential"
                            | "environment_value"
                            | "system_prompt"
                            | "settings_json"
                    ))
                }),
            )
        })
        .collect();
        let global = cx.global::<ClientGlobal>().clone();
        let mut this = Self {
            window: window.window_handle(),
            global,
            registry: None,
            host: None,
            page: Page::Hosts,
            fields,
            composer: cx.new(|cx| TextareaState::new(window, cx)),
            file_text: cx.new(|cx| TextareaState::new(window, cx)),
            busy: false,
            error: None,
            info: String::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            chat: None,
            workspace: None,
            terminal: None,
            projection: Default::default(),
            markdown: BTreeMap::new(),
            chat_list: ListState::new(0, ListAlignment::Bottom, px(400.)),
            chat_rows: Vec::new(),
            chat_indices: Vec::new(),
            options: None,
            fork_request: None,
            editing: None,
            shell_mode: false,
            agents: None,
            agent_catalog: None,
            discovery_busy: false,
            discovery_abort: None,
            discovery_revision: 0,
            pulls: None,
            mentions: None,
            notes: BTreeMap::new(),
            activity_expanded: Default::default(),
            activity_thread: None,
            activity_children: Default::default(),
            history_token: None,
            history_matches: None,
            history_query: String::new(),
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
            worktree_flags: [false; 4],
            git_sign: false,
            features: Default::default(),
            attachments: vec![],
            terminal_frame: None,
            terminal_run: String::new(),
            terminal_lease: None,
            terminal_tx: None,
            decisions: BTreeMap::new(),
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
            installing_file: false,
            file_dirty: false,
            draft_context: None,
            composer_revision: HostClient::next_view_revision(),
            renew: None,
            local_images: vec![],
        };
        this.subscriptions.push(cx.subscribe(
            &this.composer,
            |this, input, event: &InputEvent, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let _ = input;
                this.save_composer(cx);
            },
        ));
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
                let key = format!(
                    "file:{}:{}",
                    file.workspace
                        .as_ref()
                        .map(|w| w.workspace_id.as_str())
                        .unwrap_or_default(),
                    file.relative_path
                );
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
                            cx.notify();
                        });
                    }
                })
                .detach();
            },
        ));
        let cell = this.global.registry.clone();
        this.run(
            async move {
                let registry = cell
                    .get_or_try_init(|| async {
                        let path = canopy_desktop::platform::directories::runtime_data_dir()?
                            .join("remote-client-v1.json");
                        tokio::task::spawn_blocking(move || {
                            ClientRegistry::open(path, Arc::new(SystemCredentials))
                        })
                        .await
                        .map_err(|_| "Client storage worker failed.".to_string())?
                    })
                    .await?;
                Ok(Update::Registry(registry.clone()))
            },
            cx,
        );
        this
    }
    fn value(&self, field: &'static str, cx: &App) -> String {
        self.fields[field].read(cx).value().to_string()
    }
    fn field(&self, field: &'static str, label: &'static str) -> impl IntoElement {
        column()
            .gap_1()
            .child(div().text_color(t::muted()).child(label))
            .child(input(&self.fields[field]))
    }
    fn run(
        &mut self,
        future: impl std::future::Future<Output = Result<Update, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let task = self.global.runtime.spawn(future);
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|_| Err("Remote operation worker stopped.".into()));
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if this.generation != generation {
                    cx.notify();
                    return;
                }
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
    fn apply(&mut self, update: Update, cx: &mut Context<Self>) {
        match update {
            Update::EditPreview(preview) => self.install_edit(preview, cx),
            Update::EditSent(item, original) => self.finish_edit(item, original, cx),
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
                use gpui_kit::component::WindowExt;
                let owner = cx.entity().downgrade();
                let generation = self.generation;
                let operation = Arc::new(std::sync::Mutex::new(Some(operation)));
                let _ = self.window.update(cx, move |_, window, cx| {
                    window.open_dialog(cx, move |dialog, _, _| {
                        let owner = owner.clone();
                        let operation = operation.clone();
                        dialog
                            .title(title.clone())
                            .child(detail.clone())
                            .on_ok(move |_, _, cx| {
                                if let Some(operation) = operation.lock().unwrap().take() {
                                    let _ = owner.update(cx, |this, cx| {
                                        if this.generation != generation {
                                            this.error = Some("The selected resource changed. Review the operation again.".into());
                                            cx.notify();
                                            return;
                                        }
                                        this.run(operation, cx);
                                    });
                                }
                                true
                            })
                    });
                });
            }
            Update::Registry(registry) => {
                let last = registry
                    .hosts()
                    .into_iter()
                    .max_by_key(|host| host.last_seen());
                self.registry = Some(registry);
                if let Some(host) = last {
                    self.select_host(host, cx);
                }
            }
            Update::Paired(host) => {
                self.info = "Device paired. Connect to load resources.".into();
                self.select_host(host, cx);
            }
            Update::Done(info) => self.info = info,
            Update::Connected => {
                self.info = "Connected".into();
                self.refresh_connection(cx);
            }
            Update::TerminalHistory(frame) => self.terminal_history = Some(frame),
            Update::ComposerStored(revision, cleared, info) => {
                self.info = info;
                if cleared && self.composer_revision == revision {
                    self.composer_revision = HostClient::next_view_revision();
                    self.shell_mode = false;
                    self.draft_context = None;
                    self.local_images.clear();
                    self.attachments.clear();
                    let input = self.composer.clone();
                    let _ = self.window.update(cx, |_, window, cx| {
                        input.update(cx, |state, cx| state.set_value("", window, cx))
                    });
                }
            }
            Update::Options(options) => {
                if let (Some(host), Some(chat)) = (self.host.clone(), self.chat.clone()) {
                    let value = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        prost::Message::encode_to_vec(&options),
                    );
                    self.global.runtime.spawn_blocking(move || {
                        host.save_view(format!("options:{}", chat.chat_id), value)
                    });
                }
                self.options = Some(options);
            }
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
                    let key = format!(
                        "file:{}:{}",
                        file.workspace
                            .as_ref()
                            .map(|workspace| workspace.workspace_id.as_str())
                            .unwrap_or_default(),
                        file.relative_path
                    );
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
            Update::HistoryMatches(value) => {
                if value.chat != self.chat {
                    self.error = Some("History search identity mismatch.".into());
                    return;
                }
                if let Some(current) = self.history_matches.as_mut() {
                    current.matches.extend(value.matches);
                    current.next_page_token = value.next_page_token;
                    current.incomplete = value.incomplete;
                } else {
                    self.history_matches = Some(value);
                }
            }
            Update::HistoryJump(items, key) => {
                self.activity_thread = None;
                for item in items {
                    if !self.projection.items.iter().any(|i| i.key == item.key) {
                        self.projection.items.push(item);
                    }
                }
                self.projection
                    .items
                    .sort_by_key(|i| i.key.map(|k| (k.history_epoch, k.item_id)));
                self.activity_thread = self
                    .projection
                    .items
                    .iter()
                    .find(|item| item.key == Some(key))
                    .and_then(|item| {
                        item.parent_call_id
                            .clone()
                            .map(|parent| (key.history_epoch, parent))
                    });
                self.refresh_markdown(cx);
                if let Some(index) = self
                    .chat_rows
                    .iter()
                    .position(|i| i.0 == key.history_epoch && i.1 == key.item_id)
                {
                    self.chat_list.scroll_to_reveal_item(index);
                }
            }
            Update::History(items, token) => {
                for item in items.into_iter().rev() {
                    if !self.projection.items.iter().any(|i| i.key == item.key) {
                        self.projection.items.insert(0, item);
                    }
                }
                self.history_token = token;
                self.refresh_markdown(cx);
            }
            Update::SshFingerprint(value) => self.ssh_fingerprint = Some(value),
            Update::LocalAttachment(id) => {
                self.local_images.push(id);
                self.save_composer(cx);
            }
        }
    }
    fn select_host(&mut self, host: Arc<HostClient>, cx: &mut Context<Self>) {
        self.agents = None;
        self.agent_catalog = None;
        self.pulls = None;
        self.mentions = None;
        self.discovery_revision += 1;
        self.discovery_busy = false;
        self.detach();
        let layout = host
            .snapshot()
            .record
            .view("layout")
            .and_then(|value| serde_json::from_str::<DeviceLayout>(value).ok());
        self.host = Some(host.clone());
        self.chat = None;
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
        self.projection = Default::default();
        self.markdown.clear();
        self.options = None;
        self.history_matches = None;
        self.attachments.clear();
        self.local_images.clear();
        self.draft_context = None;
        self.terminal_frame = None;
        self.terminal_history = None;
        self.terminal_run.clear();
        self.terminal_selection = None;
        self.error = None;
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
                            let key = format!(
                                "file:{}:{}",
                                file.workspace
                                    .as_ref()
                                    .map(|workspace| workspace.workspace_id.as_str())
                                    .unwrap_or_default(),
                                file.relative_path
                            );
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
    fn refresh_connection(&mut self, cx: &mut Context<Self>) {
        match self.page {
            Page::Chat => {
                let Some(chat) = self.chat.clone() else {
                    return;
                };
                if self.stream.is_none() {
                    let owner = cx.entity().downgrade();
                    let window = self.window;
                    let generation = self.generation;
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                if this.generation != generation {
                                    return;
                                }
                                let text = this.composer.read(cx).value().to_string();
                                let context = this.draft_context.clone();
                                let images = this.local_images.clone();
                                let attachments = this.attachments.clone();
                                this.open_chat(chat, window, cx);
                                this.draft_context = context;
                                this.local_images = images;
                                this.attachments = attachments;
                                this.composer
                                    .update(cx, |state, cx| state.set_value(text, window, cx));
                            });
                        });
                    });
                } else if let Some(host) = self.host.clone() {
                    self.run(
                        async move {
                            Ok(Update::Options(
                                p::chat_service_client::ChatServiceClient::new(host.channel()?)
                                    .get_agent_options(p::ChatRequest { chat: Some(chat) })
                                    .await
                                    .map_err(canopy_desktop::remote::client::rpc_error)?
                                    .into_inner(),
                            ))
                        },
                        cx,
                    );
                }
            }
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
    fn save_layout(&self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let value = serde_json::to_string(&DeviceLayout {
            page: self.page,
            workspace: self.workspace.as_ref().map(prost::Message::encode_to_vec),
            chat: self.chat.as_ref().map(prost::Message::encode_to_vec),
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
        self.decisions.clear();
    }
    fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        self.detach();
        self.page = page;
        self.generation += 1;
        self.error = None;
        self.save_layout(cx);
        cx.notify();
    }
    fn ready(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|h| h.phase() == Phase::Ready)
            && !self.busy
    }
    fn pair_host(&mut self, ssh: bool, cx: &mut Context<Self>) {
        let code = self.value("pair", cx);
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
        self.run(async move{let invite=PairingInvite::parse(&code)?;let id=canopy_desktop::remote::client::HostId::parse(invite.host_id())?;if registry.get(&id).is_some(){return Err("This host is already paired. Change its route instead of creating another identity.".into());}
            let reference=uuid::Uuid::new_v4().to_string();let route=if ssh{let pin=fingerprint.ok_or("Inspect and verify the SSH host key first.")?;canopy_desktop::remote::client::ssh::save_credentials(registry.vault().as_ref(),&reference,&credential,passphrase.as_deref())?;Some(canopy_desktop::remote::client::store::SshRoute::new(address,port,username,pin,reference.clone()))}else{None};
            match invite.enroll_via(name,registry.vault(),cancelled,endpoint,route).await{Ok(record)=>Ok(Update::Paired(registry.insert(record)?)),Err(error)=>{if ssh{canopy_desktop::remote::client::ssh::remove_credentials(registry.vault().as_ref(),&reference)?;}Err(error)}}
        },cx);
    }
    fn render_hosts(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = column()
            .gap_3()
            .child("Hosts")
            .child(self.field("pair", "Pairing code"))
            .child(self.field("name", "This device’s name"))
            .child(self.field(
                "endpoint",
                "HTTPS endpoint (optional when pairing; LAN or VPN)",
            ))
            .child(
                button("pair-host", "Pair directly")
                    .disabled(self.busy || self.registry.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.pair_host(false, cx))),
            )
            .child(
                button("cancel-pair", "Cancel pairing")
                    .disabled(!self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancelled.store(true, Ordering::Release);
                        cx.notify();
                    })),
            )
            .child(self.field("ssh_address", "SSH server"))
            .child(self.field("ssh_port", "SSH port (default 22)"))
            .child(self.field("ssh_user", "SSH user"))
            .child(self.field("ssh_secret", "SSH password or private key"))
            .child(self.field("ssh_passphrase", "Private-key passphrase (optional)"))
            .child(
                button("probe-ssh", "Inspect SSH host key")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let address = this.value("ssh_address", cx);
                        let port = this.value("ssh_port", cx).parse().unwrap_or(22);
                        this.run(
                            async move {
                                Ok(Update::SshFingerprint(
                                    tokio::task::spawn_blocking(move || {
                                        canopy_desktop::remote::client::ssh::probe(&address, port)
                                    })
                                    .await
                                    .map_err(|_| "SSH probe stopped.")??,
                                ))
                            },
                            cx,
                        );
                    })),
            );
        if let Some(pin) = self.ssh_fingerprint.clone() {
            body = body
                .child(format!("Verify with the server owner: {pin}"))
                .child(
                    button("pair-ssh", "Trust this SSH key and pair")
                        .disabled(self.busy || self.registry.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.pair_host(true, cx))),
                );
        }
        if let Some(host) = self.host.clone() {
            body = body
                .child(self.field("host_label", "Label for this host on this device"))
                .child(
                    button("rename-host-label", "Save host label")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let name = this.value("host_label", cx);
                            this.run(
                                async move {
                                    host.set_display_name(name)?;
                                    Ok(Update::Done("Host label saved on this device.".into()))
                                },
                                cx,
                            );
                        })),
                );
            body = body
                .child(format!("Route for {} · {}", host.name(), host.endpoint()))
                .child(
                    button("direct-route", "Use direct connection")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let endpoint = this.value("endpoint", cx);
                            this.run(
                                async move {
                                    host.set_route(endpoint, None)?;
                                    Ok(Update::Done(
                                        "Route saved. Reconnect to load the host.".into(),
                                    ))
                                },
                                cx,
                            );
                        })),
                )
                .child(
                    button("ssh-route", "Trust this SSH key and use tunnel")
                        .disabled(self.busy || self.ssh_fingerprint.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let vault = this.registry.as_ref().unwrap().vault();
                            let address = this.value("ssh_address", cx);
                            let port = this.value("ssh_port", cx).parse().unwrap_or(22);
                            let username = this.value("ssh_user", cx);
                            let fingerprint = this.ssh_fingerprint.clone().unwrap();
                            let credential = this.value("ssh_secret", cx);
                            let passphrase = nonempty(this.value("ssh_passphrase", cx));
                            let endpoint = this.value("endpoint", cx);
                            this.run(
                                async move {
                                    let reference = uuid::Uuid::new_v4().to_string();
                                    canopy_desktop::remote::client::ssh::save_credentials(
                                        vault.as_ref(),
                                        &reference,
                                        &credential,
                                        passphrase.as_deref(),
                                    )?;
                                    let route =
                                        canopy_desktop::remote::client::store::SshRoute::new(
                                            address,
                                            port,
                                            username,
                                            fingerprint,
                                            reference.clone(),
                                        );
                                    if let Err(error) = host.set_route(endpoint, Some(route)) {
                                        canopy_desktop::remote::client::ssh::remove_credentials(
                                            vault.as_ref(),
                                            &reference,
                                        )?;
                                        return Err(error);
                                    }
                                    Ok(Update::Done(
                                        "SSH route saved. Reconnect to start the tunnel.".into(),
                                    ))
                                },
                                cx,
                            );
                        })),
                );
        }
        body.into_any_element()
    }
    fn render_catalog(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(host) = self.host.clone() else {
            return div().child("Pair or select a host.").into_any_element();
        };
        let catalog = host.catalog();
        let mut body = column()
            .gap_2()
            .child(self.field("project_path", "Project path on host"))
            .child(
                button("open-project", "Open project on host")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let host = this.host.clone().unwrap();
                        let host_path = this.value("project_path", cx);
                        this.run(
                            async move {
                                host.mutate(None, move |channel, command| async move {
                                    p::catalog_service_client::CatalogServiceClient::new(channel)
                                        .open_project(p::OpenProjectRequest {
                                            command: Some(command),
                                            host_path,
                                        })
                                        .await
                                })
                                .await?;
                                Ok(Update::Done("Project opened on host.".into()))
                            },
                            cx,
                        );
                    })),
            );
        if let Some(catalog) = catalog {
            for project in catalog.projects {
                let closing = project.checkouts.iter().any(|w| w.closing.unwrap_or(false));
                let close_target = project.checkouts.first().and_then(|w| w.workspace.clone());
                body = body.child(row().gap_2().child(project.name.clone()).children(
                    close_target.map(|workspace| {
                        button(
                            SharedString::from(format!("close-project:{}", project.project_id)),
                            if closing {
                                "Retry cleanup…"
                            } else {
                                "Close project…"
                            },
                        )
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project(workspace.clone(), cx)
                        }))
                    }),
                ));
                for workspace in project.checkouts {
                    if let Some(reference) = workspace.workspace {
                        body = body.child(
                            button(
                                SharedString::from(format!("workspace:{}", reference.workspace_id)),
                                if workspace.closing.unwrap_or(false) {
                                    format!("{} · Closing", workspace.name)
                                } else {
                                    workspace.name
                                },
                            )
                            .disabled(workspace.closing.unwrap_or(false))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.navigate(Page::Files, cx);
                                    this.workspace = Some(reference.clone());
                                    this.agents = None;
                                    this.agent_catalog = None;
                                    this.discovery_revision += 1;
                                    this.discovery_busy = false;
                                    this.file = None;
                                    this.file_hash.clear();
                                    this.file_dirty = false;
                                    this.directory = None;
                                    this.git = None;
                                    this.git_history = None;
                                    this.git_refs = None;
                                    this.git_diff = None;
                                    this.save_layout(cx);
                                    this.load_directory(cx);
                                },
                            )),
                        );
                    }
                }
            }
            for summary in catalog.chats {
                if let Some(chat) = summary.chat {
                    body = body.child(
                        button(
                            SharedString::from(format!("chat:{}", chat.chat_id)),
                            summary.title,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.open_chat(chat.clone(), window, cx),
                        )),
                    );
                }
            }
            for summary in catalog.terminals {
                if let Some(terminal) = summary.terminal {
                    body = body.child(
                        button(
                            SharedString::from(format!("terminal:{}", terminal.terminal_id)),
                            summary.title,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.navigate(Page::Terminal, cx);
                            this.terminal = Some(terminal.clone());
                            this.terminal_frame = None;
                            this.terminal_history = None;
                            this.terminal_history_offset = 0;
                            this.terminal_run.clear();
                            this.terminal_selection = None;
                            this.save_layout(cx);
                            this.open_terminal(cx);
                        })),
                    );
                }
            }
        } else {
            body = body.child("Connect to load projects, chats and terminals.");
        }
        body = body.child(self.field("tool", "Terminal tool ID")).child(
            button("new-terminal", "New terminal")
                .disabled(!self.ready() || self.workspace.is_none())
                .on_click(cx.listener(|this, _, _, cx| {
                    let host = this.host.clone().unwrap();
                    let workspace = this.workspace.clone();
                    let tool_id = this.value("tool", cx);
                    let profile_id = nonempty(this.value("profile", cx));
                    this.run(
                        async move {
                            host.mutate(None, move |channel, command| async move {
                                p::terminal_service_client::TerminalServiceClient::new(channel)
                                    .open_terminal(p::OpenTerminalRequest {
                                        command: Some(command),
                                        workspace,
                                        tool_id,
                                        profile_id,
                                    })
                                    .await
                            })
                            .await?;
                            Ok(Update::Done(
                                "Terminal created on the selected host.".into(),
                            ))
                        },
                        cx,
                    );
                })),
        );
        body.child(self.render_agent_discovery(cx))
            .into_any_element()
    }
}
impl Drop for RemoteWindow {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.detach();
    }
}
impl Render for RemoteWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = std::time::Instant::now();
        self.features.apply_device_preferences(window, cx);
        let mut sidebar = column()
            .w_56()
            .h_full()
            .flex_shrink_0()
            .bg(t::sidebar())
            .p_3()
            .gap_2()
            .child(
                button("add-host", "Hosts…")
                    .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Hosts, cx))),
            );
        if let Some(registry) = &self.registry {
            for host in registry.hosts() {
                let label = format!("{} · {:?}", host.name(), host.phase());
                let id = host.id().as_str().to_owned();
                sidebar = sidebar.child(button(SharedString::from(id), label).on_click(
                    cx.listener(move |this, _, _, cx| this.select_host(host.clone(), cx)),
                ));
            }
        }
        for (id, label, page) in [
            ("catalog", "Projects", Page::Catalog),
            ("files", "Files", Page::Files),
            ("git", "Changes", Page::Git),
            ("notes", "Notes", Page::Notes),
            ("tasks", "Tasks", Page::Tasks),
            ("settings", "Host settings", Page::Settings),
        ] {
            sidebar = sidebar.child(button(id, label).disabled(self.host.is_none()).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.navigate(page, cx);
                    match page {
                        Page::Files => this.load_directory(cx),
                        Page::Notes => this.open_notes(cx),
                        Page::Git => this.load_git(cx),
                        Page::Settings => this.load_settings(cx),
                        Page::Tasks => this.load_tasks(cx),
                        _ => {}
                    }
                }),
            ));
        }
        let mut header = row().gap_2().p_3().border_b_1().border_color(t::border());
        if let Some(host) = self.host.clone() {
            let phase = host.phase();
            header = header
                .child(div().flex_1().child(format!(
                    "{} · {:?}{}",
                    host.name(),
                    phase,
                    if phase != Phase::Ready {
                        " · cached, read only"
                    } else {
                        ""
                    }
                )))
                .child(
                    button(
                        "connect",
                        if phase == Phase::Ready {
                            "Reconnect"
                        } else {
                            "Connect"
                        },
                    )
                    .disabled(self.busy || phase == Phase::Connecting)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let host = this.host.clone().unwrap();
                        this.run(
                            async move {
                                host.connect().await?;
                                Ok(Update::Connected)
                            },
                            cx,
                        );
                    })),
                )
                .child(button("disconnect", "Disconnect").on_click(cx.listener(
                    |this, _, _, cx| {
                        if let Some(host) = &this.host {
                            host.disconnect();
                        }
                        this.detach();
                        cx.notify();
                    },
                )));
            if phase != Phase::Ready
                && let Some(seen) = host
                    .last_seen()
                    .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
            {
                header = header.child(format!("Last seen {}", seen.format("%Y-%m-%d %H:%M UTC")));
            }
            if let Some(error) = host.connection_error() {
                header = header.child(div().text_color(t::red()).child(error));
            }
        }
        let content = match self.page {
            Page::Hosts => self.render_hosts(cx),
            Page::Catalog => self.render_catalog(cx),
            Page::Chat => self.render_chat(window, cx),
            Page::Files => self.render_files(cx),
            Page::Notes => self.render_notes(cx),
            Page::Git => self.render_git(cx),
            Page::Settings => self.render_settings(cx),
            Page::Tasks => self.render_tasks(cx),
            Page::Terminal => self.render_terminal(cx),
        };
        let element = column()
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .text_sm()
            .child(window_titlebar("remote-titlebar", window).child("Canopy — Remote"))
            .child(
                row()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(header)
                            .child(
                                div()
                                    .id("remote-content")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .child(column().p_4().gap_3().child(content)),
                            )
                            .child(
                                div()
                                    .p_2()
                                    .text_color(if self.error.is_some() {
                                        t::red()
                                    } else {
                                        t::muted()
                                    })
                                    .child(self.error.clone().unwrap_or_else(|| self.info.clone())),
                            ),
                    ),
            );
        if let Some(host) = &self.host {
            let resource = self
                .chat
                .as_ref()
                .map(|chat| chat.chat_id.as_str())
                .unwrap_or("remote_window");
            canopy_desktop::remote::metrics::record(
                host.id().as_str(),
                &canopy_desktop::remote::metrics::resource_id(resource),
                None,
                self.generation,
                canopy_desktop::remote::metrics::Stage::Render,
                started,
                canopy_desktop::remote::metrics::Outcome::Ok,
            );
        }
        element
    }
}
fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
