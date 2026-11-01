//! The running app owns the listener, pairing and shutdown. Merely opening a
//! preferences page never creates an identity, listener or background process.
use super::{ControlBridge, chat::ChatsRpc};
use crate::app_state::{AppState, chat_bridge, file_ops, terminal_bridge, worktree_lifecycle};
use canopy_desktop::{
    host::DeviceGrant,
    remote::{
        artifact_rpc::ArtifactsRpc,
        auth::{RpcAuth, WorkspaceRoots},
        enrollment::EnrollmentRpc,
        host::HostState,
        resources::{FilesRpc, GitRpc, ResourceLocks},
        server::{self, RunningServer, ServerServices},
        settings_rpc::{PlatformSettingsSecrets, SettingsRpc, SettingsSecrets},
        sync::ReplayHub,
        tasks::TasksRpc,
        terminal_rpc::TerminalRpc,
        tls,
    },
};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
    sync::Arc,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct ListenerPreferences {
    version: u8,
    enabled: bool,
    address: SocketAddr,
    #[serde(default)]
    mutual_connections: bool,
}
impl Default for ListenerPreferences {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            address: "127.0.0.1:44330".parse().expect("literal address"),
            mutual_connections: false,
        }
    }
}
impl ListenerPreferences {
    fn upgrade_mutual_connections(&mut self, has_connections: bool) {
        if self.mutual_connections {
            return;
        }
        if has_connections {
            self.enabled = true;
            if self.address.ip().is_loopback() {
                self.address.set_ip("0.0.0.0".parse().unwrap());
            }
        }
        self.mutual_connections = true;
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    fn load(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        let read =
            std::fs::File::open(path).and_then(|file| file.take(4097).read_to_end(&mut bytes));
        let bytes = match read {
            Ok(_) if bytes.len() <= 4096 => bytes,
            Ok(_) => {
                return Err(
                    "Remote listener preferences are too large; the file was preserved.".into(),
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err("Could not read remote listener preferences.".into()),
        };
        let value: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "Remote listener preferences are invalid; the file was preserved.")?;
        if value.version != 1 || value.address.port() == 0 {
            return Err("Unsupported remote listener preferences; the file was preserved.".into());
        }
        Ok(value)
    }
    fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or("Remote preferences directory is unavailable.")?;
        canopy_desktop::platform::directories::ensure_private_dir(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)
            .map_err(|_| "Could not create remote listener preferences.")?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| "Could not encode remote listener preferences.")?;
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|_| "Could not save remote listener preferences.")?;
        file.persist(path)
            .map_err(|_| "Could not publish remote listener preferences.")?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Could not confirm remote listener preferences.")?;
        Ok(())
    }
}

struct HostSession {
    registry: canopy_desktop::remote::client::ClientRegistry,
    server: RunningServer,
    chats: ChatsRpc,
    state: HostState,
    enrollment: EnrollmentRpc,
    replay: ReplayHub,
    operations: ResourceLocks,
    _roots: Task<()>,
}
impl HostSession {
    async fn stop(self) -> Result<(), String> {
        self.registry.set_local_peer(None);
        let acknowledged_fault = self.state.status().readiness
            == canopy_desktop::api::proto::HostReadiness::StorageFault as i32;
        self.state.set(
            canopy_desktop::api::proto::HostReadiness::Draining,
            String::new(),
        );
        // Refuse every feature's admission before waiting on stream teardown.
        self.operations
            .drain()
            .await
            .map_err(|e| e.message().to_owned())?;
        self.chats.close_subscriptions().await;
        let stopped = self.server.stop().await;
        if acknowledged_fault { Ok(()) } else { stopped }
    }
}

pub struct RemoteState {
    preferences: ListenerPreferences,
    loaded: bool,
    busy: bool,
    quitting: bool,
    attempted: bool,
    session: Option<HostSession>,
    pub error: Option<String>,
    pub invite: Option<canopy_desktop::api::proto::PairingInvite>,
    pub devices: Vec<DeviceGrant>,
    task: Option<Task<()>>,
    pairing_events: Option<Task<()>>,
    status_events: Option<Task<()>>,
    invite_expiration: Option<Task<()>>,
    observers: Vec<Subscription>,
}
impl RemoteState {
    pub fn device_online(&self, device: &str) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.state.device_online(device))
    }
    pub fn new() -> Self {
        Self {
            preferences: Default::default(),
            loaded: false,
            busy: false,
            quitting: false,
            attempted: false,
            session: None,
            error: None,
            invite: None,
            devices: vec![],
            task: None,
            pairing_events: None,
            status_events: None,
            invite_expiration: None,
            observers: vec![],
        }
    }
    pub fn preferences(&self) -> &ListenerPreferences {
        &self.preferences
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn quitting(&self) -> bool {
        self.quitting
    }
    pub fn address(&self) -> Option<SocketAddr> {
        self.session
            .as_ref()
            .filter(|s| s.state.accepting().is_ok())
            .map(|s| s.server.address())
    }
    pub fn pending(&self) -> Option<canopy_desktop::remote::enrollment::PendingDevice> {
        self.session.as_ref()?.enrollment.pending()
    }
    /// Adding another desktop also makes this desktop reachable. The requesting
    /// UI awaits the actual listener and registry, not just preference persistence.
    pub fn prepare_connection(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        cx.spawn(async move |this, cx| {
            for _ in 0..600 {
                let result = this
                    .update(cx, |this, cx| {
                        if this.quitting {
                            return Some(Err("Canopy is closing.".into()));
                        }
                        if this
                            .address()
                            .is_some_and(|address| !address.ip().is_loopback())
                        {
                            return Some(Ok(()));
                        }
                        if this.loaded && !this.busy {
                            if this.attempted
                                && this.session.is_none()
                                && let Some(error) = &this.error
                            {
                                return Some(Err(error.clone()));
                            }
                            let mut address = this.preferences.address;
                            if address.ip().is_loopback() {
                                address.set_ip("0.0.0.0".parse().unwrap());
                            }
                            this.configure(true, &address.to_string(), cx);
                        }
                        None
                    })
                    .map_err(|_| "Remote connection owner closed.".to_owned())?;
                if let Some(result) = result {
                    return result;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
            Err("This computer's remote access did not start. Check Connections and retry.".into())
        })
    }
    pub fn bind(&mut self, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        self.observers = vec![
            cx.observe(&app.settings, |this, _, cx| this.maybe_start(cx)),
            cx.observe(&app.tools, |this, _, cx| this.maybe_start(cx)),
        ];
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async {
                    let path = canopy_desktop::platform::directories::runtime_data_dir()?
                        .join("remote-host-v1.json");
                    ListenerPreferences::load(&path)
                })
                .await;
            let result = async {
                let mut preferences = result?;
                if !preferences.mutual_connections {
                    let registry = cx
                        .update(|cx| {
                            crate::ui::remote_client::registry(cx).map_err(|e| e.to_string())
                        })?
                        .await?;
                    preferences.upgrade_mutual_connections(
                        registry.hosts().iter().any(|host| host.auto_connect()),
                    );
                    let saved = preferences.clone();
                    cx.background_spawn(async move {
                        saved.save(
                            &canopy_desktop::platform::directories::runtime_data_dir()?
                                .join("remote-host-v1.json"),
                        )
                    })
                    .await?;
                }
                Ok::<_, String>(preferences)
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(value) => {
                        this.preferences = value;
                        this.loaded = true;
                    }
                    Err(e) => this.error = Some(e),
                }
                this.maybe_start(cx);
                cx.notify();
            });
        }));
    }
    fn maybe_start(&mut self, cx: &mut Context<Self>) {
        if !self.loaded
            || !self.preferences.enabled
            || self.busy
            || self.quitting
            || self.attempted
            || self.session.is_some()
        {
            return;
        }
        let app = cx.global::<AppState>().clone();
        let settings = app.settings.read(cx);
        let (Some(client), Some(_host), Some(artifacts)) = (
            settings.client(),
            settings.runtime.clone(),
            settings.artifacts(),
        ) else {
            return;
        };
        let Some(environment) = app.tools.read(cx).environment.clone() else {
            return;
        };
        let environment = match environment {
            Ok(env) => (*env).clone(),
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let Some(git) = app.git.read(cx).client() else {
            self.error = Some("Git service is unavailable.".into());
            return;
        };
        let bridge = chat_bridge::install(cx);
        let terminals = Arc::new(terminal_bridge::install(cx));
        let lifecycle = worktree_lifecycle::install(cx);
        let operations = ResourceLocks::with_gate(file_ops::gate(cx));
        let http = cx.http_client();
        let pages = Arc::new(super::pages::Pages::install(cx));
        let address = self.preferences.address;
        self.busy = true;
        self.attempted = true;
        self.error = None;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = async {
                // Keychain and filesystem access stay on a background executor.
                let identity = Arc::new(
                    cx.background_spawn(async move {
                        // Re-read the vault manifest when restarting the listener.
                        // Initialization is read-only once this worker owns its lease.
                        let host = client
                            .initialize_runtime()
                            .await
                            .map_err(|e| e.to_string())?;
                        tls::load_or_create(&client, host).await
                    })
                    .await?,
                );
                let (auth, state, roots, replay, secrets, enrollment, events, attachments) = cx
                    .update(|cx| {
                        let app = cx.global::<AppState>();
                        let client = app
                            .settings
                            .read(cx)
                            .client()
                            .ok_or("Host storage is unavailable.")?;
                        let auth = RpcAuth::new(identity.host_id().into(), client)
                            .map_err(|e| e.message().to_owned())?;
                        let capabilities = [
                            "chat",
                            "files",
                            "git",
                            "terminal",
                            "tasks",
                            "settings",
                            "artifacts",
                            "catalog",
                        ]
                        .into_iter()
                        .map(|operation| canopy_desktop::api::proto::Capability {
                            operation: operation.into(),
                            supported: true,
                            reason: String::new(),
                        })
                        .collect();
                        let replay = ReplayHub::new(Default::default());
                        let state = HostState::new(auth.host_id(), capabilities)
                            .with_runtime_epoch(replay.epoch().into());
                        let roots = WorkspaceRoots::default();
                        let secrets: Arc<dyn SettingsSecrets> =
                            Arc::new(PlatformSettingsSecrets::new(identity.settings_mac_key()?));
                        let (enrollment, events) =
                            EnrollmentRpc::new(auth.clone(), identity.clone());
                        let attachments =
                            canopy_desktop::platform::directories::runtime_data_dir()?
                                .join("chat-attachments");
                        Ok::<_, String>((
                            auth,
                            state,
                            roots,
                            replay,
                            secrets,
                            enrollment,
                            events,
                            attachments,
                        ))
                    })?;
                let control = cx.update(|cx| {
                    Arc::new(ControlBridge::install(
                        secrets.clone(),
                        operations.clone(),
                        cx,
                    ))
                });
                let registry = cx
                    .update(|cx| crate::ui::remote_client::registry(cx).map_err(|e| e.to_string()))?
                    .await?;
                let local_peer = Arc::new(canopy_desktop::remote::peer::LocalPeer::new(
                    identity.clone(),
                    auth.client().clone(),
                    address,
                ));
                let peer_rpc = canopy_desktop::remote::peer::PeerRpc::new(
                    auth.clone(),
                    local_peer.clone(),
                    registry.clone(),
                    state.clone(),
                    operations.clone(),
                );
                let root_events = bridge
                    .watch_catalog()
                    .await
                    .map_err(|_| "Could not load the host workspace catalog.".to_owned())?;
                let initial = root_events
                    .recv()
                    .await
                    .map_err(|_| "The host workspace catalog stopped.".to_owned())?;
                roots
                    .replace(
                        initial
                            .workspaces
                            .iter()
                            .map(|w| {
                                (
                                    serde_json::to_string(&w.workspace).expect("workspace ID"),
                                    w.canonical_root.clone(),
                                )
                            })
                            .collect(),
                    )
                    .map_err(|e| e.message().to_owned())?;
                let watched_roots = roots.clone();
                let roots_bridge = bridge.clone();
                let roots_task = cx.spawn(async move |_| {
                    let mut events = root_events;
                    loop {
                        while let Ok(event) = events.recv().await {
                            if watched_roots
                                .replace(
                                    event
                                        .workspaces
                                        .iter()
                                        .map(|w| {
                                            (
                                                serde_json::to_string(&w.workspace)
                                                    .expect("workspace ID"),
                                                w.canonical_root.clone(),
                                            )
                                        })
                                        .collect(),
                                )
                                .is_err()
                            {
                                return;
                            }
                        }
                        // A coalesced catalog stream can overflow. Register another
                        // atomic full catalog rather than retaining stale roots.
                        match roots_bridge.watch_catalog().await {
                            Ok(next) => events = next,
                            Err(_) => return,
                        }
                    }
                });
                let settings =
                    SettingsRpc::new(auth.clone(), operations.clone(), control.clone(), secrets);
                let chats = ChatsRpc::new(
                    auth.clone(),
                    bridge.clone(),
                    artifacts.clone(),
                    state.clone(),
                    attachments,
                    replay.clone(),
                );
                let pulls = canopy_desktop::remote::composer::PullContexts::default();
                let discovery = canopy_desktop::remote::discovery::Discovery::new(
                    auth.clone(),
                    roots.clone(),
                    git.clone(),
                    http.clone(),
                    artifacts.clone(),
                    environment.clone(),
                    pages,
                    pulls.clone(),
                    operations.clone(),
                );
                let chats = chats.with_pull_contexts(pulls.clone());
                let api = canopy_desktop::api::ThreadRpc::new(
                    auth.clone(), state.clone(), operations.clone(),
                    canopy_desktop::command_bus::DraftResources { roots: roots.clone(), pulls },
                    Arc::new(chats.clone()),
                );
                let terminal_rpc = TerminalRpc::new(
                    auth.clone(),
                    roots.clone(),
                    operations.clone(),
                    terminals.clone(),
                )
                .with_state(state.clone())
                .with_output_budget(replay.clone());
                let catalog = super::catalog::CatalogRpc::new(
                    auth.clone(),
                    bridge,
                    roots.clone(),
                    (*terminals).clone(),
                    state.clone(),
                    operations.clone(),
                )
                .with_discovery(discovery)
                .with_output_budget(replay.clone());
                let api = api.with_catalog(Arc::new(catalog.clone()));
                let files = FilesRpc::new(
                    auth.clone(),
                    roots.clone(),
                    git.clone(),
                    artifacts.clone(),
                    operations.clone(),
                );
                let git = GitRpc::new(
                    auth.clone(),
                    roots.clone(),
                    git,
                    environment,
                    operations.clone(),
                )
                .with_lifecycle(lifecycle);
                let tasks = TasksRpc::new(
                    auth.clone(),
                    roots.clone(),
                    operations.clone(),
                    http,
                    artifacts.clone(),
                    control,
                );
                let artifacts =
                    ArtifactsRpc::new(auth.clone(), roots, operations.clone(), artifacts);
                let features = tonic::service::Routes::new(
                    canopy_desktop::api::proto::catalog_service_server::CatalogServiceServer::new(api.clone())
                        .max_decoding_message_size(canopy_desktop::host::CONTROL_BYTES)
                        .max_encoding_message_size(canopy_desktop::host::CONTROL_BYTES),
                )
                .add_service(
                    canopy_desktop::api::proto::thread_service_server::ThreadServiceServer::new(api)
                        .max_decoding_message_size(canopy_desktop::host::CONTROL_BYTES)
                        .max_encoding_message_size(canopy_desktop::host::CONTROL_BYTES),
                )
                .add_service(
                    canopy_desktop::api::proto::settings_service_server::SettingsServiceServer::new(settings.clone())
                        .max_decoding_message_size(canopy_desktop::host::CONTROL_BYTES)
                        .max_encoding_message_size(canopy_desktop::host::CONTROL_BYTES),
                )
                .add_service(
                    canopy_desktop::api::proto::peer_service_server::PeerServiceServer::new(peer_rpc)
                        .max_decoding_message_size(64 * 1024)
                        .max_encoding_message_size(64 * 1024),
                )
                .add_service(terminal_rpc.into_server())
                .add_service(
                    canopy_desktop::api::proto::files_service_server::FilesServiceServer::new(files)
                        .max_decoding_message_size(canopy_desktop::host::CONTROL_BYTES)
                        .max_encoding_message_size(canopy_desktop::host::CONTROL_BYTES),
                )
                .add_service(
                    canopy_desktop::api::proto::git_service_server::GitServiceServer::new(git)
                        .max_decoding_message_size(canopy_desktop::host::CONTROL_BYTES)
                        .max_encoding_message_size(canopy_desktop::host::CONTROL_BYTES),
                )
                .add_service(
                    canopy_desktop::api::proto::tasks_service_server::TasksServiceServer::new(tasks)
                        .max_decoding_message_size(canopy_desktop::host::CONTROL_BYTES)
                        .max_encoding_message_size(canopy_desktop::host::CONTROL_BYTES),
                )
                .add_service(
                    canopy_desktop::api::proto::artifacts_service_server::ArtifactsServiceServer::new(artifacts)
                        .max_decoding_message_size(canopy_desktop::host::CONTROL_BYTES)
                        .max_encoding_message_size(canopy_desktop::host::CONTROL_BYTES),
                );
                let services = ServerServices {
                    auth,
                    state: state.clone(),
                    identity,
                    enrollment: enrollment.clone(),
                    replay: replay.clone(),
                    operations: operations.clone(),
                    features,
                    settings,
                };
                let server = server::start(address, services).await?;
                registry.set_local_peer(Some(local_peer));
                cx.update(|cx| {
                    crate::ui::remote_client::start_saved_connections(registry.clone(), cx)
                });
                Ok::<_, String>((
                    HostSession {
                        registry,
                        server,
                        chats,
                        state,
                        enrollment,
                        replay,
                        operations,
                        _roots: roots_task,
                    },
                    events,
                ))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((session, events)) => {
                        let mut status = session.state.watch();
                        this.status_events = Some(cx.spawn(async move |this, cx| {
                            while status.changed().await.is_ok() {
                                let current = status.borrow_and_update().clone();
                                if this
                                    .update(cx, |this, cx| {
                                        if current.readiness
                                            == canopy_desktop::api::proto::HostReadiness::StorageFault as i32
                                        {
                                            this.error = Some(current.reason);
                                        }
                                        cx.notify();
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }));
                        this.session = Some(session);
                        this.pairing_events = Some(cx.spawn(async move |this, cx| {
                            while events.recv().await.is_ok() {
                                if this.update(cx, |_, cx| cx.notify()).is_err() {
                                    break;
                                }
                            }
                        }));
                        this.refresh_devices(cx);
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn configure(&mut self, enabled: bool, address: &str, cx: &mut Context<Self>) {
        if self.busy || self.quitting || !self.loaded {
            return;
        }
        let Ok(address) = address.parse::<SocketAddr>() else {
            self.error = Some("Use an IP address and port, for example 127.0.0.1:44330.".into());
            cx.notify();
            return;
        };
        if address.port() == 0 {
            self.error = Some("Select a port from 1 to 65535.".into());
            cx.notify();
            return;
        }
        let preferences = ListenerPreferences {
            version: 1,
            enabled,
            address,
            mutual_connections: true,
        };
        self.busy = true;
        self.task = Some(cx.spawn(async move |this, cx| {
            let saved = preferences.clone();
            let result = cx
                .background_spawn(async move {
                    saved.save(
                        &canopy_desktop::platform::directories::runtime_data_dir()?
                            .join("remote-host-v1.json"),
                    )
                })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    this.error = Some(error);
                    cx.notify();
                });
                return;
            }
            let old = this
                .update(cx, |this, _| {
                    this.preferences = preferences;
                    this.invite = None;
                    this.pairing_events = None;
                    this.status_events = None;
                    this.invite_expiration = None;
                    this.session.take()
                })
                .ok()
                .flatten();
            let stopped = match old {
                Some(old) => old.stop().await,
                None => Ok(()),
            };
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.attempted = false;
                if let Err(error) = stopped {
                    this.error = Some(error);
                } else {
                    this.maybe_start(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            if self.session.as_ref().is_some_and(|s| {
                s.state.status().readiness
                    == canopy_desktop::api::proto::HostReadiness::StorageFault as i32
            }) {
                self.configure(true, &self.preferences.address.to_string(), cx);
                return;
            }
            self.attempted = false;
            self.maybe_start(cx);
        }
    }
    pub fn pair(&mut self, advertised_address: String, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let advertised_address = match canopy_desktop::remote::network::invitation_address(
            session.server.address(),
            &advertised_address,
        ) {
            Ok(address) => address,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        match session
            .enrollment
            .begin(advertised_address, session.server.address().port())
        {
            Ok(invite) => {
                let token = invite.one_time_token.clone();
                self.invite = Some(invite);
                self.error = None;
                self.invite_expiration = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(300))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this
                            .invite
                            .as_ref()
                            .is_some_and(|invite| invite.one_time_token == token)
                        {
                            this.invite = None;
                            cx.notify();
                        }
                    });
                }));
            }
            Err(e) => self.error = Some(e.message().into()),
        }
        cx.notify();
    }
    pub fn resolve_pairing(
        &mut self,
        id: String,
        grant: Option<(u32, Vec<String>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &self.session else {
            return;
        };
        if self.busy {
            return;
        }
        let enrollment = session.enrollment.clone();
        let state = session.state.clone();
        self.busy = true;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = match grant {
                Some((scopes, roots)) => enrollment.approve(&id, scopes, roots).await,
                None => enrollment.deny(&id),
            };
            state.authorization_changed();
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.error = result.err().map(|e| e.message().into());
                if this.error.is_none() {
                    this.invite = None;
                    this.invite_expiration = None;
                }
                this.refresh_devices(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn refresh_devices(&mut self, cx: &mut Context<Self>) {
        let Some(client) = cx.global::<AppState>().settings.read(cx).client() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.runtime_devices().await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(devices) => this.devices = devices,
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub fn update_device(
        &mut self,
        id: String,
        grant: Option<(u64, u32, Vec<String>)>,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(client) = cx.global::<AppState>().settings.read(cx).client() else {
            return;
        };
        let state = self.session.as_ref().map(|s| s.state.clone());
        let replay = self.session.as_ref().map(|s| s.replay.clone());
        self.busy = true;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = match grant {
                Some((revision, scopes, roots)) => client
                    .update_runtime_device_authorization(id.clone(), revision, scopes, roots)
                    .await
                    .map(|_| ()),
                None => client.revoke_runtime_device(id.clone()).await,
            };
            if result.is_ok() {
                if let Some(replay) = replay {
                    replay.revoke_device(&id);
                }
                if let Some(state) = state {
                    state.authorization_changed();
                }
            }
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.error = result.err().map(|e| e.to_string());
                this.refresh_devices(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.quitting = true;
        self.invite = None;
        self.invite_expiration = None;
        let task = self.task.take();
        cx.spawn(async move |this, cx| {
            if let Some(task) = task {
                task.await;
            }
            let session = this
                .update(cx, |this, _| {
                    this.pairing_events = None;
                    this.status_events = None;
                    this.session.take()
                })
                .map_err(|_| "Remote owner is unavailable.".to_owned())?;
            if let Some(session) = session {
                session.stop().await?;
            }
            Ok(())
        })
    }
    pub fn cancel_quit(&mut self, cx: &mut Context<Self>) {
        self.quitting = false;
        self.busy = false;
        self.attempted = false;
        self.maybe_start(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::ListenerPreferences;
    #[test]
    fn mutual_upgrade_enables_existing_connections_once_and_respects_later_disable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("remote-host-v1.json");
        std::fs::write(
            &path,
            r#"{"version":1,"enabled":false,"address":"127.0.0.1:45000"}"#,
        )
        .unwrap();
        let mut preferences = ListenerPreferences::load(&path).unwrap();
        preferences.upgrade_mutual_connections(true);
        preferences.save(&path).unwrap();
        let mut preferences = ListenerPreferences::load(&path).unwrap();
        assert!(preferences.enabled());
        assert_eq!(preferences.address().to_string(), "0.0.0.0:45000");
        preferences.enabled = false;
        preferences.save(&path).unwrap();
        let mut preferences = ListenerPreferences::load(&path).unwrap();
        preferences.upgrade_mutual_connections(true);
        assert!(!preferences.enabled());
        let mut new = ListenerPreferences::default();
        new.upgrade_mutual_connections(false);
        assert!(!new.enabled());
    }
}
