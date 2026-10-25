//! Read-scoped agent/account and composer discovery. It never exposes launch
//! arguments, environment values or credentials to a client.
use super::{
    auth::{RpcAuth, WorkspaceRoots, storage_status},
    composer::PullContexts,
    host::RpcStream,
    proto,
    resources::ResourceLocks,
};
use crate::{
    artifacts::{ArtifactStore, BeginUpload, Purpose, Target},
    chat::{
        self,
        backend_session::BackendSession,
        process::Process,
        session::{ChatSession, Status},
    },
    host::READ,
    integrations as i,
    state::workspace::{Pane, PaneId, PaneMetadata},
    terminal::environment::ShellEnvironment,
};
use gpui_kit::http_client::HttpClient;
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tonic::{Request, Response, Status as RpcStatus};

#[tonic::async_trait]
pub trait PageSource: Send + Sync + 'static {
    /// A live item may not have reached its history checkpoint yet.
    async fn page(
        &self,
        chat: String,
        epoch: u64,
        item: u64,
    ) -> Result<Option<chat::page::Page>, RpcStatus>;
}
#[derive(Clone)]
pub struct Discovery {
    auth: RpcAuth,
    roots: WorkspaceRoots,
    git: crate::git::GitClient,
    http: Arc<dyn HttpClient>,
    artifacts: Arc<ArtifactStore>,
    environment: ShellEnvironment,
    pages: Arc<dyn PageSource>,
    pulls: PullContexts,
    locks: ResourceLocks,
    probes: Arc<tokio::sync::Semaphore>,
    cache: Arc<Mutex<HashMap<String, (Instant, proto::AgentCatalog)>>>,
}
impl Discovery {
    #[allow(clippy::too_many_arguments, reason = "concrete app owners")]
    pub fn new(
        auth: RpcAuth,
        roots: WorkspaceRoots,
        git: crate::git::GitClient,
        http: Arc<dyn HttpClient>,
        artifacts: Arc<ArtifactStore>,
        environment: ShellEnvironment,
        pages: Arc<dyn PageSource>,
        pulls: PullContexts,
        locks: ResourceLocks,
    ) -> Self {
        Self {
            auth,
            roots,
            git,
            http,
            artifacts,
            environment,
            pages,
            pulls,
            locks,
            probes: Arc::new(tokio::sync::Semaphore::new(4)),
            cache: Arc::default(),
        }
    }
    async fn root(
        &self,
        grant: &crate::host::DeviceGrant,
        host: &str,
        workspace: Option<&proto::WorkspaceRef>,
    ) -> Result<String, RpcStatus> {
        self.auth.check_host(host)?;
        self.auth.scope(grant, READ)?;
        let root = workspace
            .map(|w| {
                self.roots
                    .resolve(w, &self.auth)
                    .and_then(|p| Ok(WorkspaceRoots::root_string(&p)?.to_owned()))
            })
            .transpose()?
            .unwrap_or_else(|| "private".into());
        self.auth.resource(grant.id(), READ, &root).await?;
        Ok(root)
    }
    pub async fn list_agents(
        &self,
        r: Request<proto::AgentDiscoveryRequest>,
    ) -> Result<Response<proto::AgentDiscovery>, RpcStatus> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let root = self.root(&g, &r.host_id, r.workspace.as_ref()).await?;
        let catalog = self
            .auth
            .client()
            .load_tools()
            .await
            .map_err(storage_status)?;
        let agents: Vec<_> = catalog
            .tools
            .iter()
            .filter(|t| t.enabled && (t.acp || matches!(t.id.as_str(), "claude" | "codex")))
            .map(|tool| proto::AvailableAgent {
                agent_id: tool.id.clone(),
                name: tool.name.clone(),
                backend_kind: if tool.acp {
                    proto::BackendKind::Acp
                } else if tool.id == "claude" {
                    proto::BackendKind::ClaudeStreamJson
                } else {
                    proto::BackendKind::CodexAppServer
                } as i32,
                accounts: if tool.profiles.is_empty() {
                    vec![proto::AgentAccount {
                        account_id: tool.id.clone(),
                        label: "Native account".into(),
                        native_default: true,
                    }]
                } else {
                    tool.profiles
                        .iter()
                        .map(|p| proto::AgentAccount {
                            account_id: p.id.clone(),
                            label: p.name.clone(),
                            native_default: Some(&p.id) == tool.default_profile.as_ref(),
                        })
                        .collect()
                },
            })
            .collect();
        let result = agent_page(agents, r.page_token.as_deref())?;
        self.auth.resource(g.id(), READ, &root).await?;
        Ok(Response::new(result))
    }
    pub async fn agent_catalog(
        &self,
        r: Request<proto::AgentCatalogRequest>,
    ) -> Result<Response<proto::AgentCatalog>, RpcStatus> {
        let g = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let root = self.root(&g, &r.host_id, r.workspace.as_ref()).await?;
        let catalog = self
            .auth
            .client()
            .load_tools()
            .await
            .map_err(storage_status)?;
        let tool = catalog
            .get(&r.agent_id)
            .filter(|t| t.enabled && (t.acp || matches!(t.id.as_str(), "claude" | "codex")))
            .ok_or_else(|| RpcStatus::not_found("Agent is unavailable."))?;
        if !(tool.profiles.iter().any(|p| p.id == r.account_id)
            || tool.profiles.is_empty() && r.account_id == tool.id)
        {
            return Err(RpcStatus::invalid_argument(
                "Select an account from this agent's catalog.",
            ));
        }
        // Configuration participates in cache identity, including immutable secret
        // references. Neither it nor this digest is disclosed as diagnostic text.
        let config = serde_json::to_vec(tool)
            .map_err(|_| RpcStatus::internal("Agent configuration is unavailable."))?;
        let key = format!(
            "{root}\0{}\0{}\0{:x}",
            r.agent_id,
            r.account_id,
            Sha256::digest(config)
        );
        if !r.refresh
            && let Some((_, value)) = self.cache.lock().ok().and_then(|c| {
                c.get(&key)
                    .filter(|(at, _)| at.elapsed() < Duration::from_secs(60))
                    .cloned()
            })
        {
            self.auth.resource(g.id(), READ, &root).await?;
            return Ok(Response::new(value));
        }
        let slot = self.probes.clone().try_acquire_owned().map_err(|_| {
            RpcStatus::resource_exhausted("Agent discovery is busy. Retry shortly.")
        })?;
        self.auth.resource(g.id(), READ, &root).await?;
        let operation = self.locks.begin()?;
        let stop = StopProbe::default();
        let cancelled = stop.0.clone();
        let env = self.environment.clone();
        let agent = r.agent_id;
        let account = r.account_id;
        let launch_root = root.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _operation = operation;
            let _slot = slot;
            probe(catalog, env, launch_root, agent, account, cancelled)
        })
        .await
        .map_err(|_| RpcStatus::unavailable("Agent discovery worker stopped."))??;
        self.auth.resource(g.id(), READ, &root).await?;
        if result.encoded_len() > crate::host::CONTROL_BYTES {
            return Err(RpcStatus::resource_exhausted(
                "Agent catalog exceeds the control budget.",
            ));
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
            if cache.len() >= 128 {
                cache.clear();
            }
            cache.insert(key, (Instant::now(), result.clone()));
        }
        Ok(Response::new(result))
    }
    pub async fn pull_requests(
        &self,
        r: Request<proto::PullRequestsRequest>,
    ) -> Result<Response<proto::PullRequests>, RpcStatus> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let workspace = r
            .workspace
            .ok_or_else(|| RpcStatus::invalid_argument("Workspace is required."))?;
        let root = self
            .root(&grant, &workspace.host_id, Some(&workspace))
            .await?;
        if r.query.len() > 200 {
            return Err(RpcStatus::invalid_argument(
                "Pull request query exceeds 200 bytes.",
            ));
        }
        if r.page_token.is_some() {
            return Err(RpcStatus::invalid_argument(
                "This pull request search has no further page.",
            ));
        }
        let repo = self
            .git
            .task_repository(PathBuf::from(&root))
            .await
            .map_err(|_| RpcStatus::failed_precondition("Repository is unavailable."))?
            .ok_or_else(|| {
                RpcStatus::failed_precondition("This checkout has no repository origin.")
            })?;
        let config = self
            .auth
            .client()
            .load_integrations()
            .await
            .map_err(storage_status)?;
        let target = config
            .task_project(&repo)
            .filter(|t| t.provider == i::Provider::Github)
            .or(repo.inferred.filter(|t| t.provider == i::Provider::Github))
            .ok_or_else(|| {
                RpcStatus::failed_precondition(
                    "Pull request discovery requires a GitHub repository.",
                )
            })?;
        let account = config.account_for(&target).cloned().ok_or_else(|| {
            RpcStatus::failed_precondition("Connect this repository's GitHub account on the host.")
        })?;
        let key = account.credential.clone();
        let token = tokio::task::spawn_blocking(move || i::credentials::load(&key))
            .await
            .map_err(|_| RpcStatus::unavailable("Credential worker stopped."))?
            .map_err(|_| {
                RpcStatus::failed_precondition(
                    "needs_host_interaction: unlock the repository credential on the host.",
                )
            })?;
        self.auth.resource(grant.id(), READ, &root).await?;
        let (owner, name) = target
            .key
            .split_once('/')
            .ok_or_else(|| RpcStatus::failed_precondition("Invalid repository."))?;
        let http = Arc::new(PullHttp {
            inner: self.http.clone(),
            auth: self.auth.clone(),
            owner: grant.id().into(),
            root: root.clone(),
            account,
            git: self.git.clone(),
            target: target.clone(),
        });
        let found = i::github::Github::new(http, token)
            .pulls(owner, name, &r.query)
            .await
            .map_err(|error| {
                RpcStatus::failed_precondition(error.chars().take(1024).collect::<String>())
            })?;
        self.auth.resource(grant.id(), READ, &root).await?;
        if found.len() > 21 {
            return Err(RpcStatus::resource_exhausted(
                "Pull request response exceeds its result limit.",
            ));
        }
        let pulls = found
            .into_iter()
            .map(|p| {
                let pull = self.pulls.publish(
                    &root,
                    proto::PullAttachment {
                        number: p.number,
                        title: p.title,
                        url: p.url,
                        head: p.head,
                        base: p.base,
                        state: p.state,
                        body: p.body,
                        context_id: None,
                    },
                );
                proto::PullRequestChoice {
                    context_id: pull.context_id.clone().unwrap_or_default(),
                    pull: Some(pull),
                    author: p.author,
                }
            })
            .collect();
        let result = proto::PullRequests {
            pulls,
            next_page_token: None,
        };
        if result.encoded_len() > crate::host::CONTROL_BYTES {
            return Err(RpcStatus::resource_exhausted(
                "Pull request results exceed the control budget.",
            ));
        }
        Ok(Response::new(result))
    }
    pub async fn read_page(
        &self,
        r: Request<proto::ChatPageRequest>,
    ) -> Result<Response<RpcStream<proto::ArtifactChunk>>, RpcStatus> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let chat = r
            .chat
            .ok_or_else(|| RpcStatus::invalid_argument("Chat is required."))?;
        self.auth.check_host(&chat.host_id)?;
        let item = r
            .item
            .ok_or_else(|| RpcStatus::invalid_argument("Page item is required."))?;
        let record = self
            .auth
            .client()
            .load_runtime_chat(chat.chat_id.clone())
            .await
            .map_err(storage_status)?
            .ok_or_else(|| RpcStatus::not_found("Chat was deleted."))?;
        let root = record.authority_root().to_owned();
        self.auth.resource(grant.id(), READ, &root).await?;
        let mut page = self
            .pages
            .page(chat.chat_id.clone(), item.history_epoch, item.item_id)
            .await?;
        if page.is_none() {
            let row = self
                .auth
                .client()
                .read_remote_history_page(crate::settings::RemoteHistoryRequest {
                    chat: chat.chat_id.clone(),
                    upper: Some((item.history_epoch, item.item_id)),
                    before: None,
                    limit: 1,
                })
                .await
                .map_err(storage_status)?;
            if let Some((epoch, id, json)) = row.rows.items.first()
                && *epoch == item.history_epoch
                && *id == item.item_id
            {
                let parsed = serde_json::from_str::<t::Item>(json).ok();
                if let Some(t::Item {
                    body: t::Body::Tool(tool),
                    trimmed: false,
                    ..
                }) = parsed
                    && tool.name == chat::page::SHOW_TOOL
                {
                    page = chat::page::Page::from_input(&tool.input).ok();
                }
            }
        }
        let page = page
            .ok_or_else(|| RpcStatus::not_found("This item has no retained interactive page."))?;
        let store = self.artifacts.clone();
        let owner = grant.id().to_owned();
        let id = chat.chat_id;
        let cwd = record
            .metadata()
            .cwd
            .clone()
            .ok_or_else(|| RpcStatus::failed_precondition("Chat folder is unavailable."))?;
        self.auth.resource(grant.id(), READ, &root).await?;
        let resource_root = root.clone();
        let slot = self.probes.clone().try_acquire_owned().map_err(|_| {
            RpcStatus::resource_exhausted("Discovery reads are busy. Retry shortly.")
        })?;
        let operation = self.locks.begin()?;
        let stop = StopProbe::default();
        let cancelled = stop.0.clone();
        let artifact = tokio::task::spawn_blocking(move || {
            let _operation = operation;
            let _slot = slot;
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(RpcStatus::cancelled("Page read cancelled."));
            }
            let cwd = std::fs::canonicalize(cwd)
                .map_err(|_| RpcStatus::failed_precondition("Chat folder is unavailable."))?;
            if resource_root != "private" {
                let authorized = std::fs::canonicalize(&resource_root)
                    .map_err(|_| RpcStatus::failed_precondition("Workspace is unavailable."))?;
                if !cwd.starts_with(authorized) {
                    return Err(RpcStatus::permission_denied(
                        "Chat folder is outside its workspace.",
                    ));
                }
            }
            let html = chat::page::inline_images_scoped(&page.html, &cwd);
            if html.len() > 32 * 1024 * 1024 {
                return Err(RpcStatus::resource_exhausted(
                    "Page resources exceed 32 MiB.",
                ));
            }
            let bytes = html.as_bytes();
            let begin = BeginUpload::new(
                owner.clone(),
                Target::Chat(id),
                Purpose::Result,
                bytes.len() as u64,
                Sha256::digest(bytes).into(),
                "text/html".into(),
            )
            .map_err(storage_status)?;
            let upload = store.begin(begin).map_err(storage_status)?;
            let mut rollback = UploadRollback {
                store: store.clone(),
                owner: owner.clone(),
                id: upload.id().into(),
                armed: true,
            };
            for (i, chunk) in bytes.chunks(crate::artifacts::CHUNK_BYTES).enumerate() {
                if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(RpcStatus::cancelled("Page read cancelled."));
                }
                store
                    .put_chunk(
                        &owner,
                        upload.id(),
                        (i * crate::artifacts::CHUNK_BYTES) as u64,
                        chunk,
                    )
                    .map_err(storage_status)?;
            }
            let result = store.finish(&owner, upload.id()).map_err(storage_status);
            if result.is_ok() {
                rollback.armed = false;
            }
            result
        })
        .await
        .map_err(|_| RpcStatus::unavailable("Page publication worker stopped."))??;
        let reference = proto::ArtifactRef {
            host_id: self.auth.host_id().into(),
            artifact_id: artifact.id().into(),
            media_type: artifact.media_type().into(),
            size: artifact.size(),
            sha256: artifact.sha256().to_vec(),
        };
        let (store, auth, owner) = (
            self.artifacts.clone(),
            self.auth.clone(),
            grant.id().to_owned(),
        );
        let stream = futures_lite::stream::unfold((0, false), move |(offset, done)| {
            let (store, auth, owner, root, reference) = (
                store.clone(),
                auth.clone(),
                owner.clone(),
                root.clone(),
                reference.clone(),
            );
            async move {
                if done {
                    return None;
                }
                if let Err(e) = auth.resource(&owner, READ, &root).await {
                    return Some((Err(e), (offset, true)));
                }
                let id = reference.artifact_id.clone();
                let result = tokio::task::spawn_blocking(move || {
                    store.download(&owner, &id, offset, crate::artifacts::CHUNK_BYTES)
                })
                .await;
                let bytes = match result {
                    Ok(Ok(bytes)) => bytes,
                    Ok(Err(e)) => return Some((Err(storage_status(e)), (offset, true))),
                    Err(_) => {
                        return Some((
                            Err(RpcStatus::unavailable("Page read worker stopped.")),
                            (offset, true),
                        ));
                    }
                };
                let next = offset + bytes.len() as u64;
                let done = next == reference.size;
                Some((
                    Ok(proto::ArtifactChunk {
                        artifact: Some(reference),
                        offset,
                        data: bytes,
                        complete: done,
                    }),
                    (next, done),
                ))
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }
}
use chat::transcript as t;

fn probe(
    catalog: crate::state::tools::ToolCatalog,
    env: ShellEnvironment,
    root: String,
    agent: String,
    account: String,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
) -> Result<proto::AgentCatalog, RpcStatus> {
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(RpcStatus::cancelled("Agent discovery was cancelled."));
    }
    let failure = |_: String| {
        RpcStatus::failed_precondition(
            "Agent discovery failed. Check the selected account and agent on the host.",
        )
    };
    let cwd = if root == "private" {
        let path = crate::platform::directories::runtime_data_dir()
            .map_err(failure)?
            .join("agent-discovery");
        std::fs::create_dir_all(&path).map_err(|e| failure(e.to_string()))?;
        path
    } else {
        PathBuf::from(root)
    };
    let pane = Pane {
        id: PaneId::new(),
        tool: agent.clone(),
        metadata: PaneMetadata {
            cwd: Some(cwd.clone()),
            profile_id: Some(account.clone()),
            ..Default::default()
        },
    };
    let tool = catalog
        .get(&agent)
        .ok_or_else(|| RpcStatus::not_found("Agent was removed."))?;
    let settings = profile_settings(tool, &account);
    let (launch, session, lines, parse) = if tool.acp {
        let definition = catalog.agent_definition(&pane).map_err(failure)?;
        let launch = chat::launch::prepare_acp(&definition, cwd.clone(), &env).map_err(failure)?;
        let (session, lines) =
            chat::acp::AcpSession::new(chat::acp::Start::new(cwd)).map_err(failure)?;
        (
            launch,
            BackendSession::from(session),
            lines,
            chat::backend_session::parse_acp
                as fn(&[u8]) -> Result<Option<chat::backend_session::BackendIncoming>, String>,
        )
    } else if agent == "claude" {
        let launch = chat::launch::prepare_claude(&catalog, &pane, &env, &settings, None)
            .map_err(failure)?;
        let (session, lines) = ChatSession::with_settings(settings);
        (
            launch,
            BackendSession::from(session),
            lines,
            chat::backend_session::parse_claude as _,
        )
    } else {
        let launch = chat::launch::prepare_codex(&catalog, &pane, &env).map_err(failure)?;
        let (session, lines) = ChatSession::codex(
            settings,
            chat::session::CodexStart {
                cwd,
                ..Default::default()
            },
        );
        (
            launch,
            BackendSession::from(session),
            lines,
            chat::backend_session::parse_codex as _,
        )
    };
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(RpcStatus::cancelled("Agent discovery was cancelled."));
    }
    let (process, events) =
        Process::start(launch.spec, launch.files, parse, 8 * 1024 * 1024).map_err(failure)?;
    let result = (|| {
        for line in lines {
            process.send(&line).map_err(failure)?;
        }
        let mut session = session;
        let deadline = Instant::now() + Duration::from_secs(15);

        loop {
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                return Err(RpcStatus::cancelled("Agent discovery was cancelled."));
            }
            if Instant::now() > deadline {
                if matches!(session.status(), Status::Ready) {
                    break;
                }
                return Err(RpcStatus::deadline_exceeded(
                    "The agent took too long to report its catalog.",
                ));
            }
            match events.try_recv() {
                Ok(event) => {
                    for line in session.on_event(event).map_err(failure)? {
                        process.send(&line).map_err(failure)?;
                    }
                }
                Err(async_channel::TryRecvError::Closed) => {
                    return Err(RpcStatus::unavailable(
                        "The agent closed catalog discovery.",
                    ));
                }
                Err(async_channel::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
            }
            if session.status().ended() {
                return Err(RpcStatus::failed_precondition(
                    "The agent could not initialize this account.",
                ));
            }
            if session.catalog_ready() {
                break;
            }
        }
        let capabilities = session.capabilities();
        let options = session
            .options()
            .into_iter()
            .map(|o| proto::AgentOption {
                id: o.id,
                label: o.label,
                category: o.category,
                current_value: o.value,
                choices: o
                    .choices
                    .into_iter()
                    .map(|c| proto::OptionChoice {
                        id: c.id,
                        label: c.label,
                        description: c.description,
                    })
                    .collect(),
                effect: match o.effect {
                    chat::agent::OptionEffect::Immediate => "immediate",
                    chat::agent::OptionEffect::NextTurn => "next_turn",
                    chat::agent::OptionEffect::Restart => "restart",
                }
                .into(),
            })
            .collect();
        let commands = session
            .commands()
            .iter()
            .map(|c| proto::AgentCommand {
                id: format!("{}:{}", if c.skill { "skill" } else { "command" }, c.name),
                name: c.name.clone(),
                description: c.description.clone(),
                argument_hint: c.hint.clone(),
                skill: c.skill,
                path: c.path.as_ref().and_then(|p| p.to_str().map(str::to_owned)),
            })
            .collect();
        let capabilities = [
            ("resume", capabilities.resume),
            ("images", capabilities.images),
            ("embedded_context", capabilities.embedded_context),
            ("cancel", capabilities.cancel),
            ("steer", capabilities.steer),
            ("fork", capabilities.fork),
            ("rewind", capabilities.rewind),
            ("goal", capabilities.goal),
        ]
        .into_iter()
        .map(|(operation, supported)| proto::Capability {
            operation: operation.into(),
            supported,
            reason: if supported {
                String::new()
            } else {
                "The agent does not advertise this operation.".into()
            },
        })
        .collect();
        Ok(proto::AgentCatalog {
            agent_id: agent,
            account_id: account,
            options,
            capabilities,
            commands,
            refreshing: !session.catalog_ready(),
            error: (!session.catalog_ready()).then(|| {
                "The agent has not finished reporting catalog metadata. Refresh to retry.".into()
            }),
        })
    })();
    process
        .close_with(Duration::from_millis(200), Duration::from_secs(1))
        .map_err(failure)?;
    result
}

#[derive(Default)]
struct StopProbe(Arc<std::sync::atomic::AtomicBool>);
impl Drop for StopProbe {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}

fn profile_settings(
    tool: &crate::state::tools::ToolDefinition,
    account: &str,
) -> chat::config::ChatSettings {
    use chat::config::{ChatSettings, Effort, Mode};
    let profile = tool.profiles.iter().find(|p| p.id == account);
    ChatSettings {
        model: profile
            .map(|p| p.model.clone())
            .filter(|s| !s.trim().is_empty()),
        effort: profile.and_then(|p| Effort::from_key(&p.settings.effort_level)),
        mode: profile
            .and_then(|p| {
                if tool.id == "codex" {
                    Some(Mode::from_codex(
                        &p.settings.approval_mode,
                        &p.settings.sandbox,
                        p.settings.full_auto,
                        p.settings.bypass_approvals,
                    ))
                } else {
                    Mode::from_claude(&p.settings.permission_mode)
                }
            })
            .unwrap_or(if tool.id == "codex" {
                Mode::Auto
            } else {
                Mode::Ask
            }),
        ..Default::default()
    }
}

fn agent_page(
    agents: Vec<proto::AvailableAgent>,
    token: Option<&str>,
) -> Result<proto::AgentDiscovery, RpcStatus> {
    // A valid definition has <=128 profiles, each ID/label <=1024 bytes: one
    // definition fits control. Bind cursors to the complete public catalog.
    let mut digest = Sha256::new();
    for agent in &agents {
        digest.update(agent.encode_length_delimited_to_vec());
    }
    let revision = format!("{:x}", digest.finalize());
    let start = if let Some(token) = token {
        let (rev, offset) = token
            .split_once(':')
            .ok_or_else(|| RpcStatus::invalid_argument("Invalid agent page token."))?;
        if rev != revision {
            return Err(RpcStatus::aborted(
                "Agent catalog changed. Reload its first page.",
            ));
        }
        offset
            .parse::<usize>()
            .ok()
            .filter(|i| *i < agents.len())
            .ok_or_else(|| RpcStatus::invalid_argument("Invalid agent page offset."))?
    } else {
        0
    };
    let mut result = proto::AgentDiscovery {
        agents: vec![],
        next_page_token: None,
    };
    let mut bytes = 128; // Includes the bounded next-page token and protobuf framing.
    for (ix, agent) in agents.into_iter().enumerate().skip(start) {
        let size = agent.encoded_len() + 6;
        if bytes + size > crate::host::CONTROL_BYTES {
            if result.agents.is_empty() {
                return Err(RpcStatus::resource_exhausted(
                    "Agent definition exceeds the control budget.",
                ));
            }
            result.next_page_token = Some(format!("{revision}:{ix}"));
            break;
        }
        bytes += size;
        result.agents.push(agent);
    }
    Ok(result)
}

struct PullHttp {
    inner: Arc<dyn HttpClient>,
    auth: RpcAuth,
    owner: String,
    root: String,
    account: i::Account,
    git: crate::git::GitClient,
    target: i::ProjectTarget,
}
impl HttpClient for PullHttp {
    fn user_agent(&self) -> Option<&gpui_kit::http_client::http::HeaderValue> {
        self.inner.user_agent()
    }
    fn proxy(&self) -> Option<&gpui_kit::http_client::Url> {
        self.inner.proxy()
    }
    fn send(
        &self,
        request: gpui_kit::http_client::Request<gpui_kit::http_client::AsyncBody>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = gpui_kit::http_client::Result<
                        gpui_kit::http_client::Response<gpui_kit::http_client::AsyncBody>,
                    >,
                > + Send,
        >,
    > {
        let (inner, auth, owner, root, account, git, target) = (
            self.inner.clone(),
            self.auth.clone(),
            self.owner.clone(),
            self.root.clone(),
            self.account.clone(),
            self.git.clone(),
            self.target.clone(),
        );
        Box::pin(async move {
            let denied =
                || gpui_kit::http_client::anyhow!("Pull request access changed before dispatch.");
            let config = auth
                .client()
                .load_integrations()
                .await
                .map_err(|_| denied())?;
            let repo = git
                .task_repository(PathBuf::from(&root))
                .await
                .map_err(|_| denied())?
                .ok_or_else(denied)?;
            let current = config
                .task_project(&repo)
                .or(repo.inferred)
                .ok_or_else(denied)?;
            if current != target || config.account_for(&current) != Some(&account) {
                return Err(denied());
            }
            auth.resource(&owner, READ, &root)
                .await
                .map_err(|_| denied())?;
            inner.send(request).await
        })
    }
}

// Failed/cancelled publications roll back unfinished uploads. Published results
// stay available for download and use the artifact store's bounded result TTL.
struct UploadRollback {
    store: Arc<ArtifactStore>,
    owner: String,
    id: String,
    armed: bool,
}
impl Drop for UploadRollback {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.store.cancel(&self.owner, &self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_pages_preserve_all_accounts_and_reject_changed_catalog() {
        let agents: Vec<_> = (0..128)
            .map(|n| proto::AvailableAgent {
                agent_id: format!("agent-{n}"),
                name: "N".repeat(1024),
                backend_kind: 3,
                accounts: (0..128)
                    .map(|p| proto::AgentAccount {
                        account_id: format!("{n}-{p}"),
                        label: "L".repeat(1024),
                        native_default: false,
                    })
                    .collect(),
            })
            .collect();
        let mut token = None;
        let mut collected = vec![];
        loop {
            let page = agent_page(agents.clone(), token.as_deref()).unwrap();
            assert!(page.encoded_len() <= crate::host::CONTROL_BYTES);
            collected.extend(page.agents);
            token = page.next_page_token;
            if token.is_none() {
                break;
            }
        }
        assert_eq!(collected, agents);
        let token = agent_page(agents.clone(), None)
            .unwrap()
            .next_page_token
            .unwrap();
        let mut changed = agents;
        changed[0].name.push('!');
        assert_eq!(
            agent_page(changed, Some(&token)).unwrap_err().code(),
            tonic::Code::Aborted
        );
    }

    struct CountHttp(Arc<std::sync::atomic::AtomicUsize>);
    impl HttpClient for CountHttp {
        fn user_agent(&self) -> Option<&gpui_kit::http_client::http::HeaderValue> {
            None
        }
        fn proxy(&self) -> Option<&gpui_kit::http_client::Url> {
            None
        }
        fn send(
            &self,
            _: gpui_kit::http_client::Request<gpui_kit::http_client::AsyncBody>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = gpui_kit::http_client::Result<
                            gpui_kit::http_client::Response<gpui_kit::http_client::AsyncBody>,
                        >,
                    > + Send,
            >,
        > {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async {
                Ok(gpui_kit::http_client::Response::new(
                    gpui_kit::http_client::AsyncBody::empty(),
                ))
            })
        }
    }
    #[tokio::test]
    async fn pull_http_rechecks_read_account_and_repository_for_every_send() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repo");
        let repo = git2::Repository::init(&path).unwrap();
        repo.remote("origin", "https://github.com/owner/project.git")
            .unwrap();
        let root = path.to_string_lossy().into_owned();
        let client = crate::settings::SettingsClient::create(&dir.path().join("db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        client
            .register_runtime_device(
                crate::host::DeviceRegistration::new(
                    uuid::Uuid::new_v4().to_string(),
                    "test".into(),
                    vec![8; 32],
                    READ,
                    vec![root.clone()],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let grant = client.lookup_runtime_device(vec![8; 32]).await.unwrap();
        let account = i::Account {
            id: uuid::Uuid::new_v4().to_string(),
            provider: i::Provider::Github,
            login: "test".into(),
            credential: uuid::Uuid::new_v4().to_string(),
            scope: i::AccountScope::Default,
        };
        let config = i::Config {
            accounts: vec![account.clone()],
            ..Default::default()
        };
        client.save_integrations(config.clone()).await.unwrap();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let http = PullHttp {
            inner: Arc::new(CountHttp(count.clone())),
            auth: RpcAuth::new(host.id().into(), client.clone()).unwrap(),
            owner: grant.id().into(),
            root,
            account,
            git: crate::git::GitClient::start().unwrap(),
            target: i::ProjectTarget::github("owner/project").unwrap(),
        };
        let request = || {
            gpui_kit::http_client::Request::builder()
                .uri("https://api.github.com/graphql")
                .body(gpui_kit::http_client::AsyncBody::empty())
                .unwrap()
        };
        http.send(request()).await.unwrap(); // READ alone, no ADMIN or TASKS.
        let mut changed = config.clone();
        changed.accounts[0].credential = uuid::Uuid::new_v4().to_string();
        client.save_integrations(changed).await.unwrap();
        assert!(http.send(request()).await.is_err());
        client.save_integrations(config).await.unwrap();
        repo.remote_set_url("origin", "https://github.com/owner/other.git")
            .unwrap();
        assert!(http.send(request()).await.is_err());
        repo.remote_set_url("origin", "https://github.com/owner/project.git")
            .unwrap();
        client
            .revoke_runtime_device(grant.id().into())
            .await
            .unwrap();
        assert!(http.send(request()).await.is_err());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[cfg(unix)]
    fn fixture(
        acp: bool,
    ) -> (
        tempfile::TempDir,
        crate::state::tools::ToolCatalog,
        ShellEnvironment,
    ) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-agent");
        std::fs::write(&program, r#"#!/usr/bin/python3
import json,sys,time,os
open('pid','w').write(str(os.getpid()))
def send(id,result): print(json.dumps({'jsonrpc':'2.0','id':id,'result':result}),flush=True)
for line in sys.stdin:
 r=json.loads(line); m=r.get('method'); id=r.get('id')
 if m in ['turn/start','session/prompt']: sys.exit(70)
 if m=='initialize': send(id,{'protocolVersion':1,'agentCapabilities':{}})
 elif m=='thread/start':
  open('start.json','w').write(json.dumps(r['params']))
  send(id,{'thread':{'id':'test-thread'},'model':'chosen'})
 elif m=='model/list':
  time.sleep(.35); send(id,{'data':[{'id':'chosen','displayName':'Chosen'}]})
 elif m=='skills/list':
  time.sleep(.35); send(id,{'data':[{'skills':[{'name':'late-skill','path':'/test/SKILL.md'}]}]})
 elif m=='session/new':
  send(id,{'sessionId':'test-acp','models':{'currentModelId':'chosen','availableModels':[{'modelId':'chosen','name':'Chosen'}]}})
  time.sleep(.4)
  print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'test-acp','update':{'sessionUpdate':'available_commands_update','availableCommands':[{'name':'late-command','description':'Arrived after handshake'}]}}}),flush=True)
 elif id is not None: send(id,{})
"#).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut catalog = crate::state::tools::ToolCatalog::default();
        let tool = catalog.tools.iter_mut().find(|t| t.id == "codex").unwrap();
        tool.executable = program.to_string_lossy().into_owned();
        tool.acp = acp;
        tool.profiles = vec![crate::state::tools::Profile {
            id: "selected".into(),
            name: "Selected".into(),
            model: "chosen".into(),
            arguments: vec![],
            settings: crate::state::agent_settings::AgentSettings {
                approval_mode: "on-request".into(),
                sandbox: "read-only".into(),
                ..Default::default()
            },
        }];
        let env = ShellEnvironment {
            shell: "/bin/bash".into(),
            vars: [("PATH".into(), "/usr/bin:/bin".into())].into(),
        };
        (dir, catalog, env)
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cancelled_probe_remains_owned_until_child_is_reaped() {
        let (dir, catalog, env) = fixture(false);
        let root = dir.path().to_string_lossy().into_owned();
        let locks = ResourceLocks::default();
        let guard = locks.begin().unwrap();
        let stop = StopProbe::default();
        let cancelled = stop.0.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            probe(
                catalog,
                env,
                root,
                "codex".into(),
                "selected".into(),
                cancelled,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !dir.path().join("pid").exists() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(stop); // The RPC future's drop path.
        tokio::time::timeout(Duration::from_secs(2), locks.drain())
            .await
            .unwrap()
            .unwrap();
        let pid: i32 = std::fs::read_to_string(dir.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            worker.await.unwrap().unwrap_err().code(),
            tonic::Code::Cancelled
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_and_acp_probe_wait_for_delayed_metadata_without_prompt_and_close() {
        for acp in [false, true] {
            let (dir, catalog, env) = fixture(acp);
            let result = probe(
                catalog,
                env,
                dir.path().to_string_lossy().into_owned(),
                "codex".into(),
                "selected".into(),
                Arc::default(),
            )
            .unwrap();
            assert!(!result.refreshing);
            assert!(
                result
                    .commands
                    .iter()
                    .any(|c| c.name == if acp { "late-command" } else { "late-skill" })
            );
            assert!(
                result
                    .options
                    .iter()
                    .any(|o| o.category.as_deref() == Some("model")
                        && o.choices.iter().any(|c| c.id == "chosen"))
            );
            if !acp {
                let start: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(dir.path().join("start.json")).unwrap())
                        .unwrap();
                assert_eq!(start["model"], "chosen");
                assert_eq!(start["sandbox"], "read-only");
            }
            let pid: i32 = std::fs::read_to_string(dir.path().join("pid"))
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(
                unsafe { libc::kill(pid, 0) },
                -1,
                "probe child must be reaped"
            );
        }
    }
}
