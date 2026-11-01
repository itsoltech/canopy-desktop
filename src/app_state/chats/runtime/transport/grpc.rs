//! Client-side source for the ordinary chat runtime/view. No agent, local
//! history writer or host queue is created for these panes.
use super::super::*;
use super::{
    AttachmentSuggestion, ContextReading, ContextSource, FileOrigin, HistoryPosition,
    HistorySearch, PendingOutcome, PendingRequest, SavedMessageEdit, ThreadFile, ThreadFolder,
    ThreadPlace, ThreadProject,
};
use canopy_desktop::{
    remote::{
        client::{ChatMutation, ChatProjection, HostClient, HostId, Phase, presentation as wire},
        proto as p,
    },
    state::workspace::RemoteChat,
};
use prost::Message;

pub(super) async fn discover_accounts(
    host: Arc<HostClient>,
    workspace: Option<String>,
) -> Result<Vec<super::Account>, String> {
    tokio::time::timeout(std::time::Duration::from_secs(30), async move {
        let mut client = p::catalog_service_client::CatalogServiceClient::new(host.channel()?);
        let workspace = workspace.map(|workspace_id| p::WorkspaceRef {
            host_id: host.id().as_str().into(),
            workspace_id,
        });
        let mut accounts = Vec::new();
        let mut token = None;
        let mut seen = HashSet::new();
        loop {
            let page = client
                .list_agents(p::AgentDiscoveryRequest {
                    host_id: host.id().as_str().into(),
                    workspace: workspace.clone(),
                    page_token: token,
                })
                .await
                .map_err(canopy_desktop::remote::client::rpc_error)?
                .into_inner();
            for agent in page.agents {
                accounts.extend(agent.accounts.into_iter().map(|account| super::Account {
                    tool: agent.agent_id.clone(),
                    provider: Provider::from_tool(&agent.agent_id),
                    agent: agent.name.clone(),
                    id: account.account_id,
                    name: account.label,
                    native_default: account.native_default,
                }));
            }
            token = page.next_page_token;
            if let Some(next) = &token {
                if !seen.insert(next.clone()) || seen.len() > 256 {
                    return Err("Agent catalog pagination did not advance.".into());
                }
            } else {
                break;
            }
        }
        Ok(accounts)
    })
    .await
    .map_err(|_| "Loading agents timed out. Try again.".to_owned())?
}

pub(super) async fn create_chat(
    host: Arc<HostClient>,
    workspace: Option<String>,
    agent: String,
    profile: Option<String>,
) -> Result<RemoteChat, String> {
    let workspace = workspace.map(|workspace_id| p::WorkspaceRef {
        host_id: host.id().as_str().into(),
        workspace_id,
    });
    let receipt = host
        .mutate(None, move |channel, command| async move {
            p::catalog_service_client::CatalogServiceClient::new(channel)
                .create_chat(p::CreateChatRequest {
                    command: Some(command),
                    workspace,
                    agent_id: agent,
                    profile_id: profile,
                })
                .await
        })
        .await?;
    RemoteChat::new(host.id().as_str().into(), receipt.resource_id)
        .ok_or_else(|| "The computer returned an invalid chat.".into())
}

/// Creation selected from the environment catalog uses the same existing host
/// mutation owner as creation next to an attached conversation.
pub(super) fn create_environment(host: Arc<HostClient>, workspace: Option<String>, cx: &mut App) {
    let app = cx.global::<AppState>().clone();
    let target = app.projects.read(cx).catalog.active;
    let preferred = crate::app_state::Chats::provider(cx).tool().to_owned();
    let executor = match crate::ui::remote_client::executor(cx) {
        Ok(executor) => executor,
        Err(error) => {
            app.chats.update(cx, |chats, cx| {
                chats.error = Some(error.to_string());
                cx.notify();
            });
            return;
        }
    };
    let task = executor.spawn(async move {
        let agents = p::catalog_service_client::CatalogServiceClient::new(host.channel()?)
            .list_agents(p::AgentDiscoveryRequest {
                host_id: host.id().as_str().into(),
                workspace: workspace.clone().map(|workspace_id| p::WorkspaceRef {
                    host_id: host.id().as_str().into(),
                    workspace_id,
                }),
                page_token: None,
            })
            .await
            .map_err(canopy_desktop::remote::client::rpc_error)?
            .into_inner()
            .agents;
        let agent = agents
            .iter()
            .find(|agent| agent.agent_id == preferred)
            .or(agents.first())
            .map(|agent| agent.agent_id.clone())
            .ok_or_else(|| format!("{} has no agents set up.", host.name()))?;
        let reference = create_chat(host, workspace, agent.clone(), None).await?;
        Ok::<_, String>((reference, agent))
    });
    cx.spawn(async move |cx| {
        let result = task.await.unwrap_or_else(|error| Err(error.to_string()));
        cx.update(|cx| {
            let opened = result.and_then(|(reference, agent)| {
                app.projects.update(cx, |projects, cx| {
                    projects.open_chat(
                        target,
                        "New chat",
                        &agent,
                        canopy_desktop::state::workspace::PaneMetadata {
                            kind: canopy_desktop::state::workspace::PaneKind::Chat,
                            title: Some("New chat".into()),
                            remote: Some(Box::new(reference)),
                            ..Default::default()
                        },
                        cx,
                    )
                })
            });
            if let Err(error) = opened {
                app.chats.update(cx, |chats, cx| {
                    chats.error = Some(error);
                    cx.notify();
                });
            }
        });
    })
    .detach();
}

/// What the recovery tray asked the host about a saved request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Recovering {
    /// Check status: what became of it.
    Checking,
    /// Send again: replayed when the host has no record of it.
    Sending,
}

/// Unary reads belong to both their caller and the attached subscription. Dropping
/// either cancels the network work; durable mutations deliberately keep their owner.
struct ReadTask<T>(tokio::task::JoinHandle<T>);
impl<T> std::future::Future for ReadTask<T> {
    type Output = Result<T, tokio::task::JoinError>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.0).poll(cx)
    }
}
impl<T> Drop for ReadTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) struct RemoteSession {
    pub reference: RemoteChat,
    pub host: Option<Arc<HostClient>>,
    pub projection: ChatProjection,
    pub transcript: Transcript,
    pub options: p::AgentOptions,
    pub catalog: Option<p::AgentCatalog>,
    pub agents: Vec<p::AvailableAgent>,
    pub status: Status,
    pub goal: Option<canopy_desktop::chat::goal::Goal>,
    pub busy: bool,
    pub history_loading: bool,
    /// Why the last page of earlier messages couldn't be read; Earlier messages retries.
    pub history_error: Option<String>,
    /// The saved request being checked or sent again (recovery tray), until the host
    /// answers.
    pub recovering: Option<Recovering>,
    pub draft: Option<Draft>,
    pub draft_revision: u64,
    pub draft_context: Option<p::SendContext>,
    pub originals: HashMap<String, p::Attachment>,
    composer_refs: HashMap<String, p::Attachment>,
    pub preview: Option<p::EditPreview>,
    saved_edit: Option<(p::EditPreview, Draft)>,
    stream: Option<tokio::task::AbortHandle>,
    reads: std::cell::RefCell<Vec<tokio::task::AbortHandle>>,
    pump: Option<Task<()>>,
}
impl Drop for RemoteSession {
    fn drop(&mut self) {
        self.stop();
    }
}
impl RemoteSession {
    pub(super) fn queue_snapshot(&self) -> Option<canopy_desktop::host::ChatQueue> {
        use canopy_desktop::host::{ChatQueue, QueueDraft, QueueEntry, QueueState};
        let queue = self.projection.queue.as_ref()?;
        Some(ChatQueue::observed(
            queue.revision,
            queue.paused,
            queue
                .entries
                .iter()
                .map(|entry| {
                    let state = match p::QueueDelivery::try_from(entry.delivery) {
                        Ok(p::QueueDelivery::Queued) => QueueState::Queued,
                        Ok(p::QueueDelivery::QueueAttempting) => QueueState::Attempting,
                        Ok(p::QueueDelivery::Delivered) => QueueState::Delivered,
                        Ok(p::QueueDelivery::QueueFailed) => QueueState::Failed,
                        Ok(p::QueueDelivery::Dismissed) => QueueState::Dismissed,
                        _ => QueueState::DeliveryUnknown,
                    };
                    QueueEntry::observed(
                        entry.message_id.clone(),
                        QueueDraft::from(entry.draft.as_ref().map(wire::draft).unwrap_or_default()),
                        state,
                        entry.resolved_by_user,
                    )
                })
                .collect(),
        ))
    }

    fn own_read<T>(&self, task: tokio::task::JoinHandle<T>) -> ReadTask<T> {
        let mut reads = self.reads.borrow_mut();
        reads.retain(|task| !task.is_finished());
        reads.push(task.abort_handle());
        ReadTask(task)
    }
    pub(super) fn place(&self) -> ThreadPlace {
        let mut place = ThreadPlace {
            host: self.host.as_ref().map(|host| host.name()),
            ..Default::default()
        };
        let Some(summary) = &self.projection.summary else {
            return place;
        };
        let Some(workspace) = &summary.workspace else {
            place.private = true;
            return place;
        };
        let catalog = self.host.as_ref().and_then(|host| host.shared_catalog());
        if let Some((project, checkout)) = catalog.as_ref().and_then(|catalog| {
            catalog.projects.iter().find_map(|project| {
                Some((
                    project,
                    project
                        .checkouts
                        .iter()
                        .find(|checkout| checkout.workspace.as_ref() == Some(workspace))?,
                ))
            })
        }) {
            place.project = Some(ThreadProject {
                name: project
                    .origin
                    .as_deref()
                    .and_then(canopy_desktop::git::RepositoryIdentity::from_origin)
                    .map_or_else(|| project.name.clone(), |origin| origin.path().to_owned()),
                head: Some(
                    checkout
                        .branch
                        .clone()
                        .unwrap_or_else(|| checkout.name.clone()),
                ),
                main: checkout.repository_path.as_deref() == Some(checkout.path.as_str()),
            });
            place.folder = Some(ThreadFolder {
                path: checkout.path.clone().into(),
                display: Some(checkout.path.clone()),
            });
        }
        place
    }

    /// Tool calls whose epoch-qualified id is `call`.
    fn tool_calls<'a>(&'a self, call: &'a str) -> impl Iterator<Item = &'a p::ToolCall> {
        self.projection
            .items
            .iter()
            .filter_map(move |item| match &item.body {
                Some(p::transcript_item::Body::Tool(tool))
                    if item.key.is_some_and(|key| {
                        wire::call_id(key.history_epoch, &tool.call_id) == call
                    }) =>
                {
                    Some(tool)
                }
                _ => None,
            })
    }

    pub(super) fn tool_file(&self, call: &str) -> Option<ThreadFile> {
        let file = self.tool_calls(call).find_map(|tool| tool.files.first())?;
        let workspace = file.workspace.as_ref()?;
        Some(ThreadFile {
            target: canopy_desktop::chat::mentions::Target {
                path: file.relative_path.clone().into(),
                line: None,
            },
            tooltip: self.host.as_ref().map_or_else(
                || file.relative_path.clone(),
                |host| format!("{} · {}", file.relative_path, host.name()),
            ),
            origin: FileOrigin::Environment {
                host: workspace.host_id.clone(),
                workspace: workspace.workspace_id.clone(),
            },
        })
    }

    pub(super) fn context_reading(&self) -> Option<ContextReading> {
        let end = self
            .projection
            .items
            .iter()
            .rev()
            .find_map(|item| match &item.body {
                Some(p::transcript_item::Body::TurnEnd(end)) => Some(end),
                _ => None,
            })?;
        let usage = end.usage.as_ref()?;
        Some(ContextReading {
            usage: Some(canopy_desktop::chat::backend::ContextUsage {
                used: usage.context_used?,
                max: usage.context_max.filter(|max| *max > 0)?,
                buffer: usage.context_buffer.unwrap_or(0),
            }),
            rate: None,
            source: ContextSource::Remote(self.host.as_ref().map(|host| host.name())),
        })
    }

    pub(super) fn pending_request(&self) -> Option<PendingRequest> {
        let host = self.host.as_ref()?;
        let (id, mutation, receipt) =
            host.chat_mutations(&self.chat())
                .into_iter()
                .find(|(_, _, receipt)| {
                    receipt.as_ref().is_none_or(|receipt| {
                        !receipt.resolved_by_user && ![3, 4, 5, 7].contains(&receipt.state)
                    })
                })?;
        let draft = |draft: Option<&p::MessageDraft>| {
            draft
                .map(|draft| draft.text.trim().to_owned())
                .filter(|text| !text.is_empty())
        };
        let (kind, text) = match mutation {
            ChatMutation::Send(r) => (("Message", "message"), draft(r.draft.as_ref())),
            ChatMutation::Edit(r) => (("Edit", "edit"), draft(r.draft.as_ref())),
            ChatMutation::Fork(r) => (("Fork", "fork"), draft(r.replacement.as_ref())),
            ChatMutation::RunShell(r) => (
                ("Command", "command"),
                Some(format!("!{}", r.command_line.trim())),
            ),
            ChatMutation::StopShell(_) => (("Stop request", "stop request"), None),
        };
        Some(PendingRequest {
            id,
            kind,
            text,
            state: pending_outcome(receipt.as_ref().map(|receipt| receipt.state)),
        })
    }
    pub(super) fn commands(&self) -> Vec<canopy_desktop::chat::config::CommandInfo> {
        self.catalog
            .iter()
            .flat_map(|catalog| &catalog.commands)
            .map(|command| canopy_desktop::chat::config::CommandInfo {
                name: command.name.clone(),
                description: command.description.clone(),
                hint: command.argument_hint.clone(),
                path: command.path.as_ref().map(Into::into),
                skill: command.skill,
            })
            .collect()
    }

    pub(super) fn capabilities(&self) -> Capabilities {
        let mut capabilities = Capabilities::default();
        capabilities.images = self.enabled("images");
        capabilities.embedded_context = self.enabled("embedded_context");
        capabilities.cancel = self.enabled("cancel");
        capabilities.steer = self.enabled("steer");
        capabilities.fork = self.enabled("fork");
        capabilities.rewind = self.enabled("rewind");
        capabilities.goal = self.enabled("goal");
        capabilities
    }

    pub(super) fn decision_options(
        &self,
        epoch: u64,
        item: ItemId,
    ) -> Option<Vec<acp::PermissionOption>> {
        let decision = self
            .projection
            .items
            .iter()
            .find(|entry| {
                entry
                    .key
                    .is_some_and(|key| key.history_epoch == epoch && key.item_id == item.0)
            })
            .and_then(|entry| match &entry.body {
                Some(p::transcript_item::Body::Decision(decision)) => Some(decision),
                _ => None,
            })?;
        (!decision.options.is_empty()).then(|| {
            decision
                .options
                .iter()
                .map(|option| {
                    acp::PermissionOption::new(
                        option.id.clone(),
                        option.label.clone(),
                        String::new(),
                    )
                })
                .collect()
        })
    }

    pub(super) fn models(&self) -> Vec<canopy_desktop::chat::config::ModelInfo> {
        let options = self.options();
        let effort = options.iter().find(|option| option.id == "effort");
        options
            .iter()
            .find(|option| option.id == "model")
            .into_iter()
            .flat_map(|option| &option.choices)
            .map(|model| canopy_desktop::chat::config::ModelInfo {
                value: model.id.clone(),
                display_name: model.label.clone(),
                description: model.description.clone().unwrap_or_default(),
                effort_levels: effort
                    .into_iter()
                    .flat_map(|option| &option.choices)
                    .filter_map(|choice| Effort::from_key(&choice.id))
                    .collect(),
                default_effort: effort.and_then(|option| Effort::from_key(&option.value)),
                default: model.id == "default",
                supports_auto: true,
                ..Default::default()
            })
            .collect()
    }

    pub(super) fn stop(&mut self) {
        if let Some(task) = self.stream.take() {
            task.abort();
        }
        self.pump = None;
        for read in self.reads.get_mut().drain(..) {
            read.abort();
        }
    }
    pub fn new(reference: RemoteChat) -> Self {
        Self {
            reference,
            host: None,
            projection: Default::default(),
            transcript: Default::default(),
            options: Default::default(),
            catalog: None,
            agents: vec![],
            status: Status::Starting,
            goal: None,
            busy: false,
            history_loading: false,
            history_error: None,
            recovering: None,
            draft: None,
            draft_revision: 0,
            draft_context: None,
            originals: HashMap::new(),
            composer_refs: HashMap::new(),
            preview: None,
            saved_edit: None,
            stream: None,
            reads: Default::default(),
            pump: None,
        }
    }
    pub fn chat(&self) -> p::ChatRef {
        p::ChatRef {
            host_id: self.reference.host.clone(),
            chat_id: self.reference.chat.clone(),
        }
    }
    pub fn run(&self) -> String {
        self.projection
            .summary
            .as_ref()
            .and_then(|s| s.run_id.clone())
            .unwrap_or_default()
    }
    pub fn enabled(&self, name: &str) -> bool {
        self.options
            .capabilities
            .iter()
            .find(|c| c.operation == name)
            .or_else(|| {
                self.catalog
                    .as_ref()?
                    .capabilities
                    .iter()
                    .find(|c| c.operation == name)
            })
            .is_some_and(|c| c.supported)
    }
    pub fn context(&self) -> Option<p::SendContext> {
        let s = self.projection.summary.as_ref()?;
        Some(p::SendContext {
            agent_id: s.agent_id.clone(),
            profile_id: s.profile_id.clone(),
            history_epoch: s.history_epoch,
            workspace: s.workspace.clone(),
        })
    }
    pub(super) fn backend(&self, tool: &str) -> Option<BackendKind> {
        let kind = self
            .agents
            .iter()
            .find(|agent| agent.agent_id == tool)?
            .backend_kind;
        match p::BackendKind::try_from(kind).ok()? {
            p::BackendKind::ClaudeStreamJson => Some(BackendKind::Claude),
            p::BackendKind::CodexAppServer => Some(BackendKind::Codex),
            p::BackendKind::Acp => Some(BackendKind::Acp),
            p::BackendKind::Unspecified => None,
        }
    }
    pub fn options(&self) -> Vec<AgentOption> {
        let mut options = self
            .catalog
            .as_ref()
            .map(|c| c.options.clone())
            .unwrap_or_default();
        for option in &self.options.options {
            if let Some(existing) = options.iter_mut().find(|o| o.id == option.id) {
                *existing = option.clone();
            } else {
                options.push(option.clone());
            }
        }
        options
            .iter()
            .map(|o| AgentOption {
                id: o.id.clone(),
                label: o.label.clone(),
                description: None,
                category: o.category.clone(),
                value: o.current_value.clone(),
                choices: o
                    .choices
                    .iter()
                    .map(|c| {
                        let mut choice = OptionChoice::new(c.id.clone(), c.label.clone());
                        choice.description = c.description.clone();
                        choice
                    })
                    .collect(),
                effect: match o.effect.as_str() {
                    "restart" => OptionEffect::Restart,
                    "next_turn" => OptionEffect::NextTurn,
                    _ => OptionEffect::Immediate,
                },
            })
            .collect()
    }
    pub fn lines(&self) -> Vec<QueueLine> {
        let mut lines: Vec<_> = self
            .projection
            .queue
            .iter()
            .flat_map(|q| &q.entries)
            .filter_map(|entry| {
                let state = match p::QueueDelivery::try_from(entry.delivery).ok()? {
                    p::QueueDelivery::Queued => LineState::Queued,
                    p::QueueDelivery::QueueAttempting => LineState::Sending,
                    p::QueueDelivery::Unknown => LineState::Unknown,
                    _ => return None,
                };
                let draft = entry.draft.as_ref().map(wire::draft).unwrap_or_default();
                Some(QueueLine {
                    id: entry.message_id.clone(),
                    text: draft.text,
                    attachments: draft.attachments,
                    state,
                })
            })
            .collect();
        if let Some(host) = &self.host {
            for (id, request, receipt) in host.outbox() {
                if request.chat.as_ref() != Some(&self.chat())
                    || receipt
                        .as_ref()
                        .is_some_and(|r| [3, 4, 5, 7].contains(&r.state))
                    || self.projection.queue.iter().flat_map(|q| &q.entries)
                        .any(|e| e.message_id == request.message_id)
                    || self.projection.items.iter().any(|item| matches!(&item.body,
                        Some(p::transcript_item::Body::User(user)) if user.message_id == request.message_id))
                {
                    continue;
                }
                let state = if receipt.as_ref().is_some_and(|r| [1, 2].contains(&r.state)) {
                    LineState::Sending
                } else if request.command.is_some() {
                    LineState::Unknown
                } else {
                    LineState::Queued
                };
                let draft = request.draft.as_ref().map(wire::draft).unwrap_or_default();
                lines.push(QueueLine {
                    id: format!("device:{id}"),
                    text: if draft.text.is_empty() {
                        "Image waiting on this device".into()
                    } else {
                        draft.text
                    },
                    attachments: draft.attachments,
                    state,
                });
            }
        }
        lines
    }
}

fn pending_outcome(state: Option<i32>) -> PendingOutcome {
    match state {
        None => PendingOutcome::Unconfirmed { replayable: true },
        Some(1 | 2) => PendingOutcome::Processing,
        Some(6) => PendingOutcome::Unknown,
        Some(8) => PendingOutcome::Missing,
        Some(_) => PendingOutcome::Unconfirmed { replayable: false },
    }
}

impl ChatRuntime {
    pub(super) fn suggest_grpc(
        &self,
        query: String,
        pulls: bool,
        limit: usize,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<AttachmentSuggestion>, String>> {
        let source = self.transport.grpc().unwrap();
        let Some((host, workspace)) = source
            .host
            .clone()
            .zip(source.context().and_then(|context| context.workspace))
        else {
            return Task::ready(Err("This chat has no project folder.".into()));
        };
        let executor = match crate::ui::remote_client::executor(cx) {
            Ok(executor) => executor,
            Err(error) => return Task::ready(Err(error.to_string())),
        };
        let task = executor.spawn(async move {
            use canopy_desktop::remote::client::rpc_error;
            if pulls {
                let found = p::catalog_service_client::CatalogServiceClient::new(host.channel()?)
                    .list_pull_requests(p::PullRequestsRequest {
                        workspace: Some(workspace),
                        query,
                        page_token: None,
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                let mut suggestions = vec![];
                let mut originals = vec![];
                for choice in found.pulls {
                    let Some(mut pull) = choice.pull else {
                        continue;
                    };
                    pull.context_id = Some(choice.context_id);
                    let original = p::Attachment {
                        label: String::new(),
                        content: Some(p::attachment::Content::Pull(pull.clone())),
                    };
                    let Some(attachment) = wire::attachment(&original) else {
                        continue;
                    };
                    suggestions.push(AttachmentSuggestion {
                        title: format!("#{} {}", pull.number, pull.title),
                        detail: format!("{} → {}", pull.head, pull.base),
                        name: format!("#{}", pull.number),
                        attachment,
                        folder: false,
                    });
                    originals.push(original);
                }
                Ok::<_, String>((suggestions, originals))
            } else {
                let found = p::files_service_client::FilesServiceClient::new(host.channel()?)
                    .search_files(p::SearchFilesRequest {
                        workspace: Some(workspace),
                        query,
                        limit: limit as u32,
                    })
                    .await
                    .map_err(rpc_error)?
                    .into_inner();
                Ok((
                    found
                        .entries
                        .into_iter()
                        .map(|entry| {
                            let path = entry.relative_path;
                            AttachmentSuggestion {
                                title: entry.name,
                                detail: path.clone(),
                                name: path.clone(),
                                folder: false,
                                attachment: canopy_desktop::chat::transcript::Attachment::Path {
                                    path: path.into(),
                                    label: None,
                                },
                            }
                        })
                        .collect(),
                    vec![],
                ))
            }
        });
        let task = source.own_read(task);
        cx.spawn(async move |this, cx| {
            let (suggestions, originals) = task.await.map_err(|error| error.to_string())??;
            this.update(cx, |this, _| {
                for original in originals {
                    this.remember_remote_attachment(original);
                }
            })
            .map_err(|error| error.to_string())?;
            Ok(suggestions)
        })
    }

    pub(super) fn preview_edit_grpc(
        &mut self,
        item: ItemId,
        cx: &mut Context<Self>,
    ) -> Task<Result<EditPreview, String>> {
        let preview = self.client_preview(item, cx);
        cx.spawn(async move |_, _| {
            let preview = preview.await?;
            Ok(EditPreview {
                editable: Ok(preview.first_message),
                original: preview.original.as_ref().map(wire::draft),
                files: preview
                    .files
                    .map(|files| FilesPreview {
                        possible: files.possible,
                        paths: files.paths,
                        insertions: files.insertions,
                        deletions: files.deletions,
                        reason: None,
                    })
                    .unwrap_or_default(),
            })
        })
    }

    pub(super) fn saved_edit_grpc(&mut self) -> Option<SavedMessageEdit> {
        let (saved, previous) = self.take_client_edit()?;
        let item = saved.item?;
        let current = self
            .transport
            .grpc()
            .is_some_and(|session| session.run() == saved.run_id)
            && self.epoch == item.history_epoch;
        Some(SavedMessageEdit {
            target: self.host_message_edit(ItemId(item.item_id)),
            previous,
            current,
        })
    }

    pub(super) fn search_history_grpc(
        &self,
        query: String,
        token: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<HistorySearch, String>>> {
        let source = self.transport.grpc()?;
        let host = source.host.clone()?;
        let chat = source.chat();
        let executor = crate::ui::remote_client::executor(cx).ok()?;
        let search = executor.spawn(async move {
            let page = p::chat_service_client::ChatServiceClient::new(host.channel()?)
                .search_history(p::SearchHistoryRequest {
                    chat: Some(chat),
                    text: query,
                    page_token: token,
                })
                .await
                .map_err(canopy_desktop::remote::client::rpc_error)?
                .into_inner();
            Ok(HistorySearch {
                matches: page
                    .matches
                    .into_iter()
                    .filter_map(|found| found.key)
                    .map(|key| HistoryPosition {
                        history_epoch: key.history_epoch,
                        item_id: key.item_id,
                    })
                    .collect(),
                next_page_token: page.next_page_token,
            })
        });
        let search = source.own_read(search);
        Some(cx.spawn(async move |_, _| search.await.map_err(|error| error.to_string())?))
    }

    pub(super) fn load_history_grpc(
        &mut self,
        around: Option<HistoryPosition>,
        cx: &mut Context<Self>,
    ) {
        self.client_history(
            around.map(|position| p::ItemKey {
                history_epoch: position.history_epoch,
                item_id: position.item_id,
            }),
            cx,
        );
    }

    pub(super) fn image_grpc(
        &self,
        attachment: canopy_desktop::chat::transcript::Attachment,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<u8>>> {
        let canopy_desktop::chat::transcript::Attachment::Image { file, .. } = &attachment else {
            return Task::ready(None);
        };
        let source = self.transport.grpc().unwrap();
        let artifact =
            source
                .originals
                .get(file)
                .and_then(|attachment| match &attachment.content {
                    Some(p::attachment::Content::Image(image)) => Some(image.clone()),
                    _ => None,
                });
        if let Some((host, artifact)) = source.host.clone().zip(artifact) {
            let Ok(executor) = crate::ui::remote_client::executor(cx) else {
                return Task::ready(None);
            };
            let download = executor.spawn(async move { host.download(artifact).await.ok() });
            let download = source.own_read(download);
            return cx.spawn(async move |_, _| download.await.ok().flatten());
        }
        if file.starts_with("remote:") {
            return Task::ready(None);
        }
        self.device_image(attachment, cx)
    }

    pub(super) fn page_grpc(
        &self,
        call: String,
        source: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<String, String>> {
        let session = self.transport.grpc().unwrap();
        let key = session.tool_calls(&call).find_map(|tool| tool.page_item);
        let Some((host, key)) = session.host.clone().zip(key) else {
            return cx.background_spawn(async move {
                Ok(canopy_desktop::chat::page::inline_images(&source))
            });
        };
        let chat = session.chat();
        let executor = match crate::ui::remote_client::executor(cx) {
            Ok(executor) => executor,
            Err(error) => return Task::ready(Err(error.to_string())),
        };
        let download = executor.spawn(async move {
            let bytes = host.read_chat_page(chat, key).await?;
            let html = String::from_utf8(bytes).map_err(|_| "Invalid UTF-8 page.".to_owned())?;
            Ok(crate::ui::remote_client::isolated_html(&html))
        });
        let download = session.own_read(download);
        cx.spawn(async move |_, _| download.await.map_err(|error| error.to_string())?)
    }

    pub(super) fn move_queued_grpc(
        &mut self,
        message: String,
        before: String,
        cx: &mut Context<Self>,
    ) {
        match (
            message.strip_prefix("device:"),
            before.strip_prefix("device:"),
        ) {
            (Some(id), Some(before)) => {
                if let Some(host) = self.remote_host() {
                    if let Err(error) = host.move_unsent(id, before) {
                        self.note(error, cx);
                    }
                    cx.notify();
                }
                return;
            }
            (Some(_), _) | (_, Some(_)) => {
                self.note(
                    "Messages saved on this device can be reordered after they reach the host."
                        .into(),
                    cx,
                );
                return;
            }
            _ => {}
        }
        self.remote_queue(
            p::change_queue_request::Change::Move(p::QueueMove {
                message_id: message,
                before_message_id: Some(before),
            }),
            cx,
        );
    }

    pub(super) fn edit_queued_grpc(
        &mut self,
        message: String,
        draft: Draft,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let source = self.transport.grpc().unwrap();
        let Some((host, context)) = source.host.clone().zip(source.context()) else {
            return Task::ready(Err("Connect to the computer first.".into()));
        };
        let Some(revision) = source.projection.queue.as_ref().map(|queue| queue.revision) else {
            return Task::ready(Err("The queue is not available yet.".into()));
        };
        let chat = source.chat();
        let originals = source.originals.clone();
        let namespace = self.settings.id.clone();
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                let (draft, _) =
                    encode_draft(&host, &chat, &context, &originals, draft, namespace, false)
                        .await?;
                let receipt = host
                    .mutate(Some(revision), |channel, command| async move {
                        p::chat_service_client::ChatServiceClient::new(channel)
                            .change_queue(p::ChangeQueueRequest {
                                command: Some(command),
                                chat: Some(chat),
                                queue_revision: revision,
                                change: Some(p::change_queue_request::Change::Edit(p::QueueEdit {
                                    message_id: message,
                                    draft: Some(draft),
                                })),
                            })
                            .await
                    })
                    .await?;
                if receipt.state != p::OperationState::Succeeded as i32 {
                    return Err(receipt
                        .error
                        .map(|error| error.message)
                        .unwrap_or_else(|| "The queue change is not confirmed.".into()));
                }
                Ok(())
            });
        cx.spawn(async move |_, _| task.await.map_err(|error| error.to_string())?)
    }

    pub(super) fn remove_queued_grpc(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(id) = message.strip_prefix("device:") {
            self.client_remove_saved(id.into(), cx);
            return;
        }
        self.remote_queue(
            p::change_queue_request::Change::Remove(p::QueueRemove {
                message_id: message,
            }),
            cx,
        );
    }

    pub(super) fn resolve_unknown_grpc(
        &mut self,
        message: String,
        resolution: canopy_desktop::host::UnknownResolution,
        cx: &mut Context<Self>,
    ) {
        use canopy_desktop::host::UnknownResolution;
        if let Some(id) = message.strip_prefix("device:") {
            self.client_resolve_saved(
                id.into(),
                matches!(resolution, UnknownResolution::Resend { .. }),
                cx,
            );
            return;
        }
        let (resolution, new_message_id) = match resolution {
            UnknownResolution::Delivered => (p::UnknownResolution::AcceptAsDelivered, None),
            UnknownResolution::Resend { message } => {
                (p::UnknownResolution::SendAgain, Some(message))
            }
            UnknownResolution::Dismissed => (p::UnknownResolution::Dismiss, None),
        };
        self.remote_queue(
            p::change_queue_request::Change::Resolve(p::QueueResolve {
                message_id: message,
                resolution: resolution as i32,
                new_message_id,
            }),
            cx,
        );
    }

    pub(super) fn respond_grpc(
        &mut self,
        item: ItemId,
        response: Response,
        cx: &mut Context<Self>,
    ) {
        let (action, answers, note, accept_edits) = match response {
            Response::Allow => (p::NativeDecisionAction::AllowOnce, vec![], None, false),
            Response::AllowForChat => (p::NativeDecisionAction::AllowForChat, vec![], None, false),
            Response::Deny(note) => (p::NativeDecisionAction::Deny, vec![], note, false),
            Response::Answer(answers) => (p::NativeDecisionAction::Answer, answers, None, false),
            Response::ApprovePlan { accept_edits } => (
                p::NativeDecisionAction::ApprovePlan,
                vec![],
                None,
                accept_edits,
            ),
            Response::KeepPlanning(note) => {
                (p::NativeDecisionAction::KeepPlanning, vec![], note, false)
            }
        };
        self.client_respond(
            item,
            p::respond_decision_request::Response::Native(p::NativeDecision {
                action: action as i32,
                answers,
                note,
                accept_edits,
            }),
            cx,
        );
    }

    pub(super) fn respond_option_grpc(
        &mut self,
        item: ItemId,
        option: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(option) = option {
            self.client_respond(
                item,
                p::respond_decision_request::Response::OptionId(option),
                cx,
            );
        }
    }

    pub(super) fn goal_grpc(&mut self, command: ChatCommand, cx: &mut Context<Self>) {
        let action = match command {
            ChatCommand::PauseGoal => p::GoalAction::PauseGoal,
            ChatCommand::ResumeGoal => p::GoalAction::ResumeGoal,
            ChatCommand::ClearGoal => p::GoalAction::ClearGoal,
            _ => return,
        };
        self.client_goal(action, cx);
    }

    pub(crate) fn client_resume(&mut self, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let saved: Vec<_> = host
            .outbox()
            .into_iter()
            .filter(|(_, r, result)| {
                r.chat.as_ref() == Some(&chat)
                    && result
                        .as_ref()
                        .is_none_or(|r| !r.resolved_by_user && ![3, 4, 5, 7].contains(&r.state))
            })
            .map(|(id, _, _)| id)
            .collect();
        let queue = source.projection.queue.clone();
        self.remote_task(
            async move {
                let mut revision = queue.as_ref().map(|q| q.revision);
                for id in saved {
                    let receipt = host.send_outbox(&id).await?;
                    revision = receipt.queue_revision.or(revision);
                }
                if let Some(revision) = revision {
                    host.mutate(Some(revision), |channel, meta| async move {
                        p::chat_service_client::ChatServiceClient::new(channel)
                            .change_queue(p::ChangeQueueRequest {
                                command: Some(meta),
                                chat: Some(chat),
                                queue_revision: revision,
                                change: Some(p::change_queue_request::Change::Pause(
                                    p::QueuePause { paused: false },
                                )),
                            })
                            .await
                    })
                    .await?;
                }
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_remove_saved(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(host) = self.remote_host() else {
            return;
        };
        self.remote_task(async move { host.dismiss_outbox(&id) }, cx);
    }
    pub(crate) fn client_resolve_saved(
        &mut self,
        id: String,
        resend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.remote_host() else {
            return;
        };
        self.remote_task(
            async move {
                if let Some(id) = host.resolve_outbox(&id, resend).await? {
                    host.send_outbox(&id).await?;
                }
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_new_chat(&mut self, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let Some(context) = source.context() else {
            return;
        };
        let tool = context.agent_id.clone();
        let target = cx.global::<AppState>().projects.read(cx).catalog.active;
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                let receipt = host
                    .mutate(None, |channel, meta| async move {
                        p::catalog_service_client::CatalogServiceClient::new(channel)
                            .create_chat(p::CreateChatRequest {
                                command: Some(meta),
                                workspace: context.workspace,
                                agent_id: context.agent_id,
                                profile_id: context.profile_id,
                            })
                            .await
                    })
                    .await?;
                RemoteChat::new(host.id().as_str().into(), receipt.resource_id)
                    .ok_or_else(|| "Invalid chat identity.".to_owned())
            });
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| match result {
                Ok(reference) => {
                    let projects = cx.global::<AppState>().projects.clone();
                    let _ = projects.update(cx, |p, cx| {
                        p.open_chat(
                            target,
                            "New chat",
                            &tool,
                            canopy_desktop::state::workspace::PaneMetadata {
                                kind: canopy_desktop::state::workspace::PaneKind::Chat,
                                title: Some("New chat".into()),
                                remote: Some(Box::new(reference)),
                                ..Default::default()
                            },
                            cx,
                        )
                    });
                }
                Err(error) => this.note(error, cx),
            });
        })
        .detach();
    }
    pub(crate) fn client_set_agent(
        &mut self,
        tool: String,
        profile: String,
        model: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        if self.account_locked() {
            return;
        }
        let chat = source.chat();
        self.remote_task(
            async move {
                host.mutate(None, |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .set_chat_agent(p::SetChatAgentRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            agent_id: tool,
                            profile_id: profile,
                            model,
                        })
                        .await
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_stop_background(&mut self, call: String, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let run = source.run();
        let native = source
            .tool_calls(&call)
            .next()
            .map(|tool| tool.call_id.clone());
        let call = native.unwrap_or(call);
        self.remote_task(
            async move {
                host.mutate(None, |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .stop_background(p::StopBackgroundRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            run_id: run,
                            call_id: call,
                        })
                        .await
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_steer_queued(&mut self, message: String, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let run = source.run();
        self.remote_task(
            async move {
                host.mutate(None, |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .steer(p::SteerRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            run_id: run,
                            message_id: message,
                            draft: None,
                        })
                        .await
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_take_queued(
        &mut self,
        message: String,
        replacement: Draft,
        cx: &mut Context<Self>,
    ) -> Task<Option<Draft>> {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return Task::ready(None);
        };
        if let Some(id) = message.strip_prefix("device:") {
            let id = id.to_owned();
            let revision = source.draft_revision;
            let namespace = self.settings.id.clone();
            let task = crate::ui::remote_client::executor(cx)
                .unwrap()
                .spawn(async move {
                    let data = canopy_desktop::platform::directories::data_dir()?;
                    let dir = canopy_desktop::chat::history::attachments_dir(
                        &data,
                        namespace.as_deref().ok_or("Missing image directory.")?,
                    );
                    host.take_unsent(&id, &dir, revision).await
                });
            return cx.spawn(async move |this, cx| {
                match task.await.unwrap_or_else(|e| Err(e.to_string())) {
                    Ok((draft, context)) => {
                        let _ = this.update(cx, |this, cx| {
                            if !replacement.text.is_empty() || !replacement.attachments.is_empty() {
                                this.retry_remote(replacement, cx);
                            }
                            this.transport.grpc_mut().unwrap().draft_context = context;
                            cx.notify();
                        });
                        Some(draft)
                    }
                    Err(error) => {
                        let _ = this.update(cx, |this, cx| this.note(error, cx));
                        None
                    }
                }
            });
        }
        let Some(queue) = source.projection.queue.as_ref() else {
            return Task::ready(None);
        };
        let Some(draft) = queue
            .entries
            .iter()
            .find(|e| e.message_id == message)
            .and_then(|e| e.draft.as_ref())
            .map(wire::draft)
        else {
            return Task::ready(None);
        };
        let chat = source.chat();
        let revision = queue.revision;
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                host.mutate(Some(revision), |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .change_queue(p::ChangeQueueRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            queue_revision: revision,
                            change: Some(p::change_queue_request::Change::Remove(p::QueueRemove {
                                message_id: message,
                            })),
                        })
                        .await
                })
                .await
            });
        cx.spawn(
            async move |this, cx| match task.await.unwrap_or_else(|e| Err(e.to_string())) {
                Ok(_) => {
                    if !replacement.text.is_empty() || !replacement.attachments.is_empty() {
                        let _ =
                            this.update(cx, |this, cx| this.send_remote(replacement, false, cx));
                    }
                    Some(draft)
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| this.note(error, cx));
                    None
                }
            },
        )
    }
    /// Connection context for the independent Files/Git/terminal tools panel.
    pub(crate) fn project_tools_context(&self) -> Option<(Arc<HostClient>, Option<String>)> {
        let source = self.transport.grpc()?;
        Some((
            source.host.clone()?,
            source
                .context()
                .and_then(|context| context.workspace)
                .map(|workspace| workspace.workspace_id),
        ))
    }
    pub(crate) fn remote_host(&self) -> Option<Arc<HostClient>> {
        self.transport.grpc()?.host.clone()
    }
    pub(crate) fn update_client_options(&mut self) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        for option in source.options() {
            match option.id.as_str() {
                "model" => self.settings.model = (!option.value.is_empty()).then_some(option.value),
                "effort" => self.settings.effort = Effort::from_key(&option.value),
                "mode" => {
                    if let Some(mode) = Mode::from_key(&option.value) {
                        self.settings.mode = mode;
                    }
                }
                _ => {}
            }
        }
    }
    fn refresh_client_options(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                p::chat_service_client::ChatServiceClient::new(host.channel()?)
                    .get_agent_options(p::ChatRequest { chat: Some(chat) })
                    .await
                    .map(|r| r.into_inner())
                    .map_err(canopy_desktop::remote::client::rpc_error)
            });
        let task = source.own_read(task);
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(options)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.transport.grpc_mut().unwrap().options = options;
                    this.update_client_options();
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub(crate) fn remember_remote_attachment(&mut self, attachment: p::Attachment) {
        if let Some(source) = self.transport.grpc_mut() {
            remember_attachment(&mut source.originals, &attachment);
            if source.composer_refs.len() < 128 {
                remember_attachment(&mut source.composer_refs, &attachment);
            }
        }
    }
    pub(crate) fn client_fork(
        &mut self,
        item: HistoryPosition,
        destination: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let destination = destination.map(|workspace_id| p::WorkspaceRef {
            host_id: source.reference.host.clone(),
            workspace_id,
        });
        let tool = self.pane.tool.clone();
        let target = cx.global::<AppState>().projects.read(cx).catalog.active;
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                let receipt = host
                    .submit_chat_mutation(ChatMutation::Fork(p::ForkChatRequest {
                        command: None,
                        chat: Some(chat),
                        history_epoch: item.history_epoch,
                        item_id: item.item_id,
                        destination,
                        replacement: None,
                    }))
                    .await?;
                if receipt.state != p::OperationState::Succeeded as i32 {
                    return Err("Fork is pending. Review its saved result before retrying.".into());
                }
                let reference = RemoteChat::new(host.id().as_str().into(), receipt.resource_id)
                    .ok_or("Invalid fork identity.")?;
                Ok::<_, String>(reference)
            });
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| match result {
                Ok(reference) => {
                    let projects = cx.global::<AppState>().projects.clone();
                    let _ = projects.update(cx, |p, cx| {
                        p.open_chat(
                            target,
                            "Fork",
                            &tool,
                            canopy_desktop::state::workspace::PaneMetadata {
                                kind: canopy_desktop::state::workspace::PaneKind::Chat,
                                title: Some("Fork".into()),
                                remote: Some(Box::new(reference)),
                                ..Default::default()
                            },
                            cx,
                        )
                    });
                }
                Err(error) => this.note(error, cx),
            });
        })
        .detach();
    }

    pub(super) fn connect_remote(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = self.transport.grpc() else {
            return;
        };
        if remote.pump.is_some() {
            return;
        }
        let reference = remote.reference.clone();
        let registry = crate::ui::remote_client::registry(cx);
        let executor = crate::ui::remote_client::executor(cx);
        let (Ok(registry), Ok(executor)) = (registry, executor) else {
            self.error = Some("Could not open saved connections.".into());
            cx.notify();
            return;
        };
        self.starting = true;
        cx.spawn(async move |this, cx| {
            let result = registry.await.and_then(|registry| {
                registry
                    .get(&HostId::parse(&reference.host)?)
                    .ok_or_else(|| {
                        "This computer is no longer paired. Add it in Connections.".into()
                    })
            });
            let _ = this.update(cx, |this, cx| match result {
                Ok(host) => this.attach_remote(host, executor, cx),
                Err(error) => {
                    this.starting = false;
                    this.error = Some(error);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn attach_remote(
        &mut self,
        host: Arc<HostClient>,
        executor: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) {
        let remote = self.transport.grpc_mut().expect("gRPC source");
        if remote.pump.is_some() {
            return;
        }
        let chat = remote.chat();
        let cached = host.cached_chat(&chat.chat_id);
        let record = host.snapshot().record;
        if let Some(value) = record.view(&format!("chat:{}", chat.chat_id)) {
            let (text, context, _) =
                serde_json::from_str::<(String, Option<Vec<u8>>, Vec<String>)>(value).unwrap_or((
                    value.to_owned(),
                    None,
                    vec![],
                ));
            remote.draft_context = context.and_then(|b| p::SendContext::decode(b.as_slice()).ok());
            let mut draft = Draft {
                text,
                ..Default::default()
            };
            if let Some((bytes, _)) = record
                .view(&format!("context:{}", chat.chat_id))
                .and_then(|v| serde_json::from_str::<(Vec<u8>, Option<Vec<u8>>)>(v).ok())
                && let Ok(value) = p::MessageDraft::decode(bytes.as_slice())
            {
                draft.attachments = wire::draft(&value).attachments;
            }
            remote.draft = Some(draft);
        }
        remote.host = Some(host.clone());
        if let Some((preview, (text, attachments, context, goal))) = record
            .view(&format!("native-edit:{}", chat.chat_id))
            .and_then(|v| serde_json::from_str::<SavedEditing>(v).ok())
            && let Ok(preview) = p::EditPreview::decode(preview.as_slice())
        {
            remote.saved_edit = Some((
                preview,
                Draft {
                    text,
                    attachments,
                    context,
                    goal,
                },
            ));
        }
        if let Some((text, attachments, context, goal, expected)) = record
            .view(&format!("native-draft:{}", chat.chat_id))
            .and_then(|value| {
                serde_json::from_str::<(
                    String,
                    Vec<canopy_desktop::chat::transcript::Attachment>,
                    Option<String>,
                    bool,
                    Option<Vec<u8>>,
                )>(value)
                .ok()
            })
        {
            remote.draft = Some(Draft {
                text,
                attachments,
                context,
                goal,
            });
            remote.draft_context = expected.and_then(|b| p::SendContext::decode(b.as_slice()).ok());
        }
        if let Some(cached) = cached {
            self.apply_remote(cached, cx);
        }
        let (tx, rx) = async_channel::bounded(1);
        let task = executor.spawn(async move {
            host.start_connection();
            loop {
                while host.phase() != Phase::Ready {
                    if tx.is_closed() {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                let result = async {
                    let mut client =
                        p::chat_service_client::ChatServiceClient::new(host.channel()?);
                    let options = client
                        .get_agent_options(p::ChatRequest {
                            chat: Some(chat.clone()),
                        })
                        .await
                        .map_err(canopy_desktop::remote::client::rpc_error)?
                        .into_inner();
                    if tx.send(ClientEvent::Options(options)).await.is_err() {
                        return Ok(());
                    }
                    let mut stream = client
                        .subscribe_chat(p::SubscribeChatRequest {
                            chat: Some(chat.clone()),
                            since: None,
                        })
                        .await
                        .map_err(canopy_desktop::remote::client::rpc_error)?
                        .into_inner();
                    let mut projection = ChatProjection::default();
                    let mut saved = std::time::Instant::now();
                    while let Some(event) = stream
                        .message()
                        .await
                        .map_err(canopy_desktop::remote::client::rpc_error)?
                    {
                        let boundary = matches!(
                            &event.event,
                            Some(p::chat_event::Event::SnapshotEnd(_))
                                | Some(p::chat_event::Event::ReplaceItem(p::TranscriptItem {
                                    body: Some(p::transcript_item::Body::TurnEnd(_)),
                                    ..
                                }))
                        );
                        if projection.apply(&chat, event)? {
                            if boundary || saved.elapsed() >= std::time::Duration::from_millis(500)
                            {
                                host.cache_chat(&chat.chat_id, &projection)?;
                                saved = std::time::Instant::now();
                            }
                            if tx
                                .send(ClientEvent::Projection(Box::new(projection.clone())))
                                .await
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                    Err::<(), String>("Connection interrupted. Reconnecting…".into())
                }
                .await;
                match result {
                    Ok(()) => return,
                    Err(error) => {
                        if tx.send(ClientEvent::Error(error)).await.is_err() {
                            return;
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
        let abort = task.abort_handle();
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                if this
                    .update(cx, |this, cx| match event {
                        ClientEvent::Projection(projection) => this.apply_remote(*projection, cx),
                        ClientEvent::Options(options) => {
                            this.transport.grpc_mut().unwrap().options = options;
                            this.update_client_options();
                            this.discover_remote(cx);
                        }
                        ClientEvent::Error(error) => {
                            this.starting = false;
                            this.error = Some(error);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let remote = self.transport.grpc_mut().unwrap();
        remote.stream = Some(abort);
        remote.pump = Some(pump);
    }

    fn apply_remote(&mut self, mut projection: ChatProjection, cx: &mut Context<Self>) {
        let remote = self.transport.grpc_mut().unwrap();
        let state_changed = projection.summary.as_ref().map(|s| (&s.run_id, s.state))
            != remote
                .projection
                .summary
                .as_ref()
                .map(|s| (&s.run_id, s.state));
        // Fetched history survives subsequent live-tail snapshots.
        if let Some(first) = projection.items.first().and_then(|i| i.key) {
            let mut older: Vec<_> = remote
                .projection
                .items
                .iter()
                .filter(|i| {
                    i.key.is_some_and(|k| {
                        (k.history_epoch, k.item_id) < (first.history_epoch, first.item_id)
                    })
                })
                .cloned()
                .collect();
            older.append(&mut projection.items);
            projection.items = older;
        }
        let epoch = projection
            .summary
            .as_ref()
            .map_or(self.epoch, |s| s.history_epoch);
        if let Some(run) = projection
            .summary
            .as_ref()
            .and_then(|summary| summary.run_id.as_deref())
            .and_then(|run| run.parse::<u64>().ok())
        {
            // Shared preflights fence the executor's run, not a device-generated
            // generation assigned while the subscription was being constructed.
            self.generation = run;
        }
        let mut epochs = std::collections::BTreeMap::<u64, Vec<_>>::new();
        remote.originals = remote.composer_refs.clone();
        if let Some(preview) = &remote.preview {
            for a in preview.original.iter().flat_map(|d| &d.attachments) {
                remember_attachment(&mut remote.originals, a);
            }
        }
        for a in projection
            .queue
            .iter()
            .flat_map(|q| &q.entries)
            .flat_map(|e| e.draft.iter())
            .flat_map(|d| &d.attachments)
        {
            remember_attachment(&mut remote.originals, a);
        }
        for item in &projection.items {
            if let Some(p::transcript_item::Body::User(user)) = &item.body {
                for attachment in user.draft.iter().flat_map(|d| &d.attachments) {
                    remember_attachment(&mut remote.originals, attachment);
                }
            }
            if let (Some(key), Some(item)) = (item.key, wire::item(item)) {
                epochs.entry(key.history_epoch).or_default().push(item);
            }
        }
        let changed_epoch = epoch != self.epoch;
        let changed_items = remote
            .transcript
            .sync_projection(epochs.remove(&epoch).unwrap_or_default());
        let reset = changed_epoch || changed_items;
        self.epoch = epoch;
        self.serial = epoch.saturating_add(1);
        remote.status = match projection.summary.as_ref().map(|s| s.state) {
            Some(v) if v == p::ChatState::Working as i32 => Status::Working,
            Some(v) if v == p::ChatState::NeedsDecision as i32 => Status::NeedsDecision,
            Some(v) if v == p::ChatState::ChatStarting as i32 => Status::Starting,
            Some(v) if v == p::ChatState::Interrupting as i32 => Status::Interrupting,
            Some(v) if v == p::ChatState::Exited as i32 => Status::Exited {
                code: None,
                signal: None,
            },
            Some(v) if v == p::ChatState::ChatFailed as i32 => {
                Status::Failed("The agent stopped with an error.".into())
            }
            _ => Status::Ready,
        };
        if let Some(summary) = &projection.summary {
            self.pane.tool = summary.agent_id.clone();
            self.pane.metadata.profile_id = summary.profile_id.clone();
            self.pane.metadata.title = Some(summary.title.clone());
            remote.goal = summary.goal.as_ref().map(wire::goal);
        }
        self.archived = epochs
            .into_iter()
            .map(|(epoch, items)| {
                Arc::new(Archived {
                    epoch,
                    transcript: Transcript::restore(items),
                    agent: None,
                    label: "Earlier conversation",
                    serial: None,
                })
            })
            .collect();
        remote.projection = projection;
        self.starting = false;
        self.error = None;
        let changes: Rc<[Change]> = remote.transcript.take_changes().into();
        if reset {
            cx.emit(ChatRuntimeEvent::Reset);
        }
        cx.emit(ChatRuntimeEvent::Restored);
        cx.emit(ChatRuntimeEvent::Changed {
            serial: self.serial,
            changes,
        });
        cx.emit(ChatStateChanged);
        cx.notify();
        if state_changed {
            self.refresh_client_options(cx);
        }
    }

    fn discover_remote(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let summary = source.projection.summary.clone().or_else(|| {
            host.shared_catalog()?
                .chats
                .iter()
                .find(|c| c.chat.as_ref() == Some(&source.chat()))
                .cloned()
        });
        let Some(summary) = summary else {
            return;
        };
        let work = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                let mut client =
                    p::catalog_service_client::CatalogServiceClient::new(host.channel()?);
                let mut agents = vec![];
                let mut token = None;
                loop {
                    let page = client
                        .list_agents(p::AgentDiscoveryRequest {
                            host_id: host.id().as_str().into(),
                            workspace: summary.workspace.clone(),
                            page_token: token,
                        })
                        .await
                        .map_err(canopy_desktop::remote::client::rpc_error)?
                        .into_inner();
                    agents.extend(page.agents);
                    token = page.next_page_token;
                    if token.is_none() {
                        break;
                    }
                }
                let catalog = client
                    .get_agent_catalog(p::AgentCatalogRequest {
                        host_id: host.id().as_str().into(),
                        workspace: summary.workspace,
                        agent_id: summary.agent_id.clone(),
                        account_id: summary.profile_id.unwrap_or(summary.agent_id),
                        refresh: false,
                    })
                    .await
                    .map_err(canopy_desktop::remote::client::rpc_error)?
                    .into_inner();
                Ok::<_, String>((agents, catalog))
            });
        let work = source.own_read(work);
        cx.spawn(async move |this, cx| {
            if let Ok(Ok((agents, catalog))) = work.await {
                let _ = this.update(cx, |this, cx| {
                    let source = this.transport.grpc_mut().unwrap();
                    source.agents = agents;
                    source.catalog = Some(catalog);
                    this.update_client_options();
                    cx.emit(ChatRuntimeEvent::Changed {
                        serial: this.serial,
                        changes: Rc::from([]),
                    });
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn remote_task<F>(&mut self, future: F, cx: &mut Context<Self>)
    where
        F: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        self.remote_task_then(future, |_| {}, cx);
    }
    /// `remote_task`; `done` runs once the host has answered, before views hear of it.
    fn remote_task_then<F>(
        &mut self,
        future: F,
        done: impl FnOnce(&mut Self) + 'static,
        cx: &mut Context<Self>,
    ) where
        F: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        let Ok(executor) = crate::ui::remote_client::executor(cx) else {
            done(self);
            return;
        };
        self.transport.grpc_mut().unwrap().busy = true;
        let task = executor.spawn(future);
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|_| {
                Err(
                    "The operation was interrupted; review its saved result before retrying."
                        .into(),
                )
            });
            let _ = this.update(cx, |this, cx| {
                this.transport.grpc_mut().unwrap().busy = false;
                done(this);
                if let Err(error) = result {
                    this.notice = Some(ChatNotice::error(error));
                } else {
                    this.refresh_client_options(cx);
                }
                cx.emit(ChatRuntimeEvent::Changed {
                    serial: this.serial,
                    changes: Rc::from([]),
                });
                cx.notify();
            });
        })
        .detach();
    }
    /// Asks the host what became of the saved requests (recovery tray).
    pub(crate) fn client_reconcile(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.remote_host() else {
            return;
        };
        if !self.start_recovering(Recovering::Checking, cx) {
            return;
        }
        self.remote_task_then(
            async move { host.reconcile().await },
            Self::end_recovering,
            cx,
        );
    }
    /// Sends saved request `id` again when the host has no record of it.
    pub(crate) fn client_retry_saved(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(host) = self.remote_host() else {
            return;
        };
        if !self.start_recovering(Recovering::Sending, cx) {
            return;
        }
        self.remote_task_then(
            async move {
                host.retry_chat_mutation(&id).await?;
                Ok(())
            },
            Self::end_recovering,
            cx,
        );
    }
    /// One recovery action at a time; false while another one waits for the host.
    fn start_recovering(&mut self, action: Recovering, cx: &mut Context<Self>) -> bool {
        let Some(source) = self.transport.grpc_mut().filter(|s| s.recovering.is_none()) else {
            return false;
        };
        source.recovering = Some(action);
        cx.notify();
        true
    }
    fn end_recovering(&mut self) {
        if let Some(source) = self.transport.grpc_mut() {
            source.recovering = None;
        }
    }

    pub(crate) fn send_remote(&mut self, draft: Draft, steer: bool, cx: &mut Context<Self>) {
        self.send_remote_intent(draft, steer, true, cx);
    }
    pub(crate) fn retry_remote(&mut self, draft: Draft, cx: &mut Context<Self>) {
        self.send_remote_intent(draft, false, false, cx);
    }
    fn send_remote_intent(
        &mut self,
        draft: Draft,
        steer: bool,
        consume: bool,
        cx: &mut Context<Self>,
    ) {
        self.notice = ChatNotice::after_send(self.notice.take());
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            self.note(
                "Connecting to this computer. Your draft is kept.".into(),
                cx,
            );
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            return;
        };
        let Some(context) = source.draft_context.clone().or_else(|| source.context()) else {
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            return;
        };
        let chat = source.chat();
        let run = source.run();
        let originals = source.originals.clone();
        let revision = if consume { source.draft_revision } else { 0 };
        let tombstone = HostClient::next_view_revision();
        let namespace = self.settings.id.clone();
        let executor = crate::ui::remote_client::executor(cx).unwrap();
        self.transport.grpc_mut().unwrap().busy = true;
        let task = executor.spawn(async move {
            let stage = host.phase() != Phase::Ready
                && !steer
                && !draft.goal
                && draft.context.is_none()
                && draft.attachments.iter().all(|a| a.is_image());
            let (request, uploads) =
                encode_draft(&host, &chat, &context, &originals, draft, namespace, stage).await?;
            let key = format!("native-draft:{}", chat.chat_id);
            if steer {
                host.mutate(None, |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .steer(p::SteerRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            run_id: run,
                            message_id: uuid::Uuid::new_v4().to_string(),
                            draft: Some(request),
                        })
                        .await
                })
                .await?;
            } else if request
                .attachments
                .iter()
                .all(|a| matches!(a.content, Some(p::attachment::Content::Image(_))))
                && !request.goal
                && request.context.is_none()
            {
                let (id, _) = host.enqueue(chat, context, request, uploads, revision)?;
                if host.phase() == Phase::Ready {
                    host.send_outbox(&id).await?;
                }
            } else {
                host.submit_chat_mutation(ChatMutation::Send(p::SendMessageRequest {
                    command: None,
                    chat: Some(chat.clone()),
                    message_id: uuid::Uuid::new_v4().to_string(),
                    draft: Some(request),
                    expected_context: Some(context),
                }))
                .await?;
                host.clear_composer_version(&chat.chat_id, revision)?;
            }
            if consume {
                host.save_view_version(key, String::new(), tombstone)?;
            }
            Ok::<_, String>(())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|_| {
                Err(
                    "The operation was interrupted. Review the saved result before retrying."
                        .into(),
                )
            });
            let _ = this.update(cx, |this, cx| {
                let source = this.transport.grpc_mut().unwrap();
                source.busy = false;
                match result {
                    Ok(()) => {
                        if source.draft_revision == revision {
                            source.draft = None;
                            source.draft_context = None;
                        }
                        if consume {
                            cx.emit(ChatRuntimeEvent::DraftConsumed(revision));
                        }
                    }
                    Err(error) => this.notice = Some(ChatNotice::error(error)),
                }
                cx.emit(ChatRuntimeEvent::Changed {
                    serial: this.serial,
                    changes: Rc::from([]),
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn save_client_draft(
        &mut self,
        draft: Draft,
        revision: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.transport.grpc_mut() else {
            return;
        };
        if source.draft_context.is_none() {
            source.draft_context = source.context();
        }
        source.draft = Some(draft.clone());
        source.draft_revision = revision;
        let Some(host) = source.host.clone() else {
            return;
        };
        let key = format!("native-draft:{}", source.reference.chat);
        let expected = source.draft_context.as_ref().map(Message::encode_to_vec);
        let Ok(value) = serde_json::to_string(&(
            draft.text,
            draft.attachments,
            draft.context,
            draft.goal,
            expected,
        )) else {
            return;
        };
        cx.background_spawn(async move { host.save_view_version(key, value, revision) })
            .detach();
    }

    pub(crate) fn client_preview(
        &mut self,
        item: ItemId,
        cx: &mut Context<Self>,
    ) -> Task<Result<p::EditPreview, String>> {
        let Some(source) = self.transport.grpc() else {
            return Task::ready(Err("No remote source.".into()));
        };
        let Some(host) = source.host.clone() else {
            return Task::ready(Err("Connect to the computer first.".into()));
        };
        let request = p::PreviewEditRequest {
            chat: Some(source.chat()),
            run_id: source.run(),
            item: Some(p::ItemKey {
                history_epoch: self.epoch,
                item_id: item.0,
            }),
        };
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                let result = p::chat_service_client::ChatServiceClient::new(host.channel()?)
                    .preview_edit(request)
                    .await
                    .map_err(canopy_desktop::remote::client::rpc_error)?
                    .into_inner();
                if !result.editable {
                    return Err(result.reason);
                }
                Ok(result)
            });
        let task = source.own_read(task);
        cx.spawn(async move |this, cx| {
            let preview = task.await.map_err(|e| e.to_string())??;
            let _ = this.update(cx, |this, _| {
                let source = this.transport.grpc_mut().unwrap();
                if let Some(draft) = &preview.original {
                    for a in &draft.attachments {
                        remember_attachment(&mut source.originals, a);
                    }
                }
                source.preview = Some(preview.clone());
            });
            Ok(preview)
        })
    }
    pub(crate) fn take_client_edit(&mut self) -> Option<(p::EditPreview, Draft)> {
        self.transport.grpc_mut()?.saved_edit.take()
    }
    pub(crate) fn save_client_edit(&mut self, previous: Option<Draft>, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let key = format!("native-edit:{}", source.reference.chat);
        let revision = HostClient::next_view_revision();
        let value = match (previous, source.preview.as_ref()) {
            (Some(d), Some(p)) => serde_json::to_string(&(
                p.encode_to_vec(),
                (d.text, d.attachments, d.context, d.goal),
            ))
            .unwrap_or_default(),
            _ => String::new(),
        };
        cx.background_spawn(async move { host.save_view_version(key, value, revision) })
            .detach();
    }
    pub(crate) fn client_edit(
        &mut self,
        draft: Draft,
        restore: bool,
        revision: u64,
        cx: &mut Context<Self>,
    ) {
        let source = self.transport.grpc().unwrap();
        let (Some(host), Some(preview), Some(context)) = (
            source.host.clone(),
            source.preview.clone(),
            source.context(),
        ) else {
            return;
        };
        let originals = source.originals.clone();
        let namespace = self.settings.id.clone();
        let clear_key = format!("native-draft:{}", source.reference.chat);
        let edit_key = format!("native-edit:{}", source.reference.chat);
        let tombstone = HostClient::next_view_revision();
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                let (draft, _) = encode_draft(
                    &host,
                    preview.chat.as_ref().ok_or("Missing chat.")?,
                    &context,
                    &originals,
                    draft,
                    namespace,
                    false,
                )
                .await?;
                host.submit_chat_mutation(ChatMutation::Edit(p::EditMessageRequest {
                    command: None,
                    chat: preview.chat,
                    run_id: preview.run_id,
                    item: preview.item,
                    draft: Some(draft),
                    restore_files: restore,
                }))
                .await?;
                host.save_view_version(clear_key, String::new(), tombstone)?;
                host.save_view_version(edit_key, String::new(), tombstone)?;
                Ok::<_, String>(())
            });
        self.transport.grpc_mut().unwrap().busy = true;
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|_| {
                Err("Edit interrupted. Review the saved operation before retrying.".into())
            });
            let _ = this.update(cx, |this, cx| {
                this.transport.grpc_mut().unwrap().busy = false;
                cx.emit(ChatRuntimeEvent::EditFinished { revision, result });
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn client_shell(&mut self, command_line: String, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let request = p::RunShellRequest {
            command: None,
            chat: Some(source.chat()),
            run_id: source.run(),
            command_line,
        };
        let revision = source.draft_revision;
        let clear_key = format!("native-draft:{}", source.reference.chat);
        let tombstone = HostClient::next_view_revision();
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move {
                let receipt = host
                    .submit_chat_mutation(ChatMutation::RunShell(request))
                    .await?;
                host.save_view_version(clear_key, String::new(), tombstone)?;
                Ok::<_, String>(receipt)
            });
        self.transport.grpc_mut().unwrap().busy = true;
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                this.transport.grpc_mut().unwrap().busy = false;
                match result {
                    Ok(_) => cx.emit(ChatRuntimeEvent::DraftConsumed(revision)),
                    Err(error) => this.notice = Some(ChatNotice::error(error)),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn client_stop_shell(&mut self, item: ItemId, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let request = p::StopShellRequest {
            command: None,
            chat: Some(source.chat()),
            run_id: source.run(),
            item: Some(p::ItemKey {
                history_epoch: self.epoch,
                item_id: item.0,
            }),
        };
        self.remote_task(
            async move {
                host.submit_chat_mutation(ChatMutation::StopShell(request))
                    .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_history(&mut self, around: Option<p::ItemKey>, cx: &mut Context<Self>) {
        let source = self.transport.grpc().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        if source.history_loading {
            return;
        }
        let chat = source.chat();
        let earlier = around.is_none();
        let before = earlier
            .then(|| source.projection.items.first().and_then(|i| i.key))
            .flatten();
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move { host.history_page(chat, None, around, before).await });
        let task = source.own_read(task);
        let source = self.transport.grpc_mut().unwrap();
        source.history_loading = true;
        source.history_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((items, next)) => {
                        let mut projection = this.transport.grpc().unwrap().projection.clone();
                        for item in items {
                            if !projection.items.iter().any(|i| i.key == item.key) {
                                projection.items.push(item);
                            }
                        }
                        projection
                            .items
                            .sort_by_key(|i| i.key.map(|k| (k.history_epoch, k.item_id)));
                        projection.has_earlier = next.is_some();
                        this.apply_remote(projection, cx);
                    }
                    // Earlier messages turns into its Retry; a page around a search
                    // match fails like any other request.
                    Err(error) if earlier => {
                        this.transport.grpc_mut().unwrap().history_error = Some(error);
                    }
                    Err(error) => this.notice = Some(ChatNotice::error(error)),
                }
                this.transport.grpc_mut().unwrap().history_loading = false;
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn remote_command(&mut self, command: &'static str, cx: &mut Context<Self>) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let run = source.run();
        self.remote_task(
            async move {
                host.mutate(None, |channel, meta| async move {
                    let request = p::ChatCommandRequest {
                        command: Some(meta),
                        chat: Some(chat),
                        run_id: (!run.is_empty()).then_some(run),
                    };
                    let mut client = p::chat_service_client::ChatServiceClient::new(channel);
                    match command {
                        "interrupt" => client.interrupt(request).await,
                        _ => client.restart(request).await,
                    }
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn remote_option(&mut self, id: String, value: String, cx: &mut Context<Self>) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        self.remote_task(
            async move {
                host.mutate(None, |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .set_agent_option(p::SetAgentOptionRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            option_id: id,
                            value_id: value,
                        })
                        .await
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_respond(
        &mut self,
        item: ItemId,
        response: p::respond_decision_request::Response,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let Some(decision) = source
            .projection
            .items
            .iter()
            .find(|i| {
                i.key
                    .is_some_and(|k| k.history_epoch == self.epoch && k.item_id == item.0)
            })
            .and_then(|i| match &i.body {
                Some(p::transcript_item::Body::Decision(d)) => Some(d.clone()),
                _ => None,
            })
        else {
            return;
        };
        let chat = source.chat();
        self.remote_task(
            async move {
                host.mutate(None, |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .respond_decision(p::RespondDecisionRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            run_id: decision.run_id,
                            decision_id: decision.decision_id,
                            response: Some(response),
                        })
                        .await
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn remote_queue(
        &mut self,
        change: p::change_queue_request::Change,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let Some(revision) = source.projection.queue.as_ref().map(|q| q.revision) else {
            return;
        };
        let chat = source.chat();
        self.remote_task(
            async move {
                host.mutate(Some(revision), |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .change_queue(p::ChangeQueueRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            queue_revision: revision,
                            change: Some(change),
                        })
                        .await
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
    pub(crate) fn client_goal(&mut self, action: p::GoalAction, cx: &mut Context<Self>) {
        let Some(source) = self.transport.grpc() else {
            return;
        };
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        self.remote_task(
            async move {
                host.mutate(None, |channel, meta| async move {
                    p::chat_service_client::ChatServiceClient::new(channel)
                        .set_goal(p::SetGoalRequest {
                            command: Some(meta),
                            chat: Some(chat),
                            objective: None,
                            action: Some(action as i32),
                        })
                        .await
                })
                .await?;
                Ok(())
            },
            cx,
        );
    }
}
enum ClientEvent {
    Projection(Box<ChatProjection>),
    Options(p::AgentOptions),
    Error(String),
}
type SavedEditing = (
    Vec<u8>,
    (
        String,
        Vec<canopy_desktop::chat::transcript::Attachment>,
        Option<String>,
        bool,
    ),
);

fn remember_attachment(originals: &mut HashMap<String, p::Attachment>, a: &p::Attachment) {
    let key = match a.content.as_ref() {
        Some(p::attachment::Content::Image(i)) => format!("remote:{}", i.artifact_id),
        Some(p::attachment::Content::Pull(p)) => format!("pull:{}", p.number),
        Some(p::attachment::Content::Path(f)) => format!("path:{}", f.relative_path),
        _ => return,
    };
    originals.insert(key, a.clone());
}

async fn encode_draft(
    host: &HostClient,
    chat: &p::ChatRef,
    context: &p::SendContext,
    originals: &HashMap<String, p::Attachment>,
    mut value: Draft,
    namespace: Option<String>,
    stage: bool,
) -> Result<(p::MessageDraft, Vec<String>), String> {
    use canopy_desktop::chat::transcript::Attachment as A;
    for a in &mut value.attachments {
        if matches!(a,A::Image { data:None, file,.. } if !file.starts_with("remote:")) {
            let dir = canopy_desktop::chat::history::attachments_dir(
                &canopy_desktop::platform::directories::data_dir()?,
                namespace.as_deref().ok_or("Missing image namespace.")?,
            );
            let mut loaded = canopy_desktop::chat::attachments::with_data(&dir, vec![a.clone()])?;
            *a = loaded.remove(0);
        }
    }
    let mut attachments = vec![];
    let mut uploads = vec![];
    for a in &value.attachments {
        let old = match a {
            A::Image { file, .. } => originals.get(file),
            A::Pull { number, .. } => originals.get(&format!("pull:{number}")),
            A::Path { path, .. } => originals.get(&format!("path:{}", path.display())),
            _ => None,
        };
        if let Some(old) = old {
            attachments.push(old.clone());
            continue;
        }
        let content = match a {
            A::Image {
                data: Some(data),
                media_type,
                ..
            } => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data.as_ref())
                    .map_err(|_| "Invalid image data.")?;
                let id = host.prepare_attachment(
                    p::ResourceRef {
                        resource: Some(p::resource_ref::Resource::Chat(chat.clone())),
                    },
                    "chat-image",
                    media_type,
                    &bytes,
                )?;
                if stage {
                    uploads.push(id);
                    continue;
                }
                p::attachment::Content::Image(host.resume_upload(&id).await?)
            }
            A::Image { .. } => return Err("The image is not available. Attach it again.".into()),
            A::Path { path, .. } => {
                if path.is_absolute() {
                    return Err("Choose a file from the remote workspace, not a local path.".into());
                }
                p::attachment::Content::Path(p::FileRef {
                    workspace: context.workspace.clone(),
                    relative_path: path.to_string_lossy().into_owned(),
                })
            }
            A::Quote { text, .. } => {
                p::attachment::Content::Quote(p::QuoteAttachment { text: text.clone() })
            }
            A::Command {
                name,
                skill,
                path,
                description,
                ..
            } => p::attachment::Content::Command(p::CommandAttachment {
                name: name.clone(),
                skill: *skill,
                path: path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                description: description.clone(),
            }),
            A::Pull { .. } => return Err("Choose the pull request from this host again.".into()),
            A::Shell {
                command,
                exit,
                stopped,
                output,
                ..
            } => p::attachment::Content::Shell(p::ShellAttachment {
                command: command.clone(),
                exit_code: *exit,
                stopped: *stopped,
                output: output.clone(),
            }),
        };
        attachments.push(p::Attachment {
            label: a.label().unwrap_or_default().into(),
            content: Some(content),
        });
    }
    Ok((
        p::MessageDraft {
            text: value.text,
            context: value.context,
            goal: value.goal,
            attachments,
        },
        uploads,
    ))
}

#[cfg(test)]
mod tests {
    use super::{ChatProjection, ChatRuntime, RemoteChat, RemoteSession, p, pending_outcome};
    use core::prelude::v1::test;
    use gpui_kit::{AppContext, TestAppContext};

    #[test]
    fn pending_delivery_never_offers_automatic_or_unsafe_retry() {
        assert!(pending_outcome(None).can_send_again());
        assert!(pending_outcome(Some(p::OperationState::NotFound as i32)).can_send_again());
        assert!(!pending_outcome(Some(p::OperationState::NotFound as i32)).can_check());
        for state in [
            p::OperationState::Accepted,
            p::OperationState::Attempting,
            p::OperationState::DeliveryUnknown,
        ] {
            assert!(!pending_outcome(Some(state as i32)).can_send_again());
            assert!(pending_outcome(Some(state as i32)).can_check());
        }
        assert!(!pending_outcome(Some(i32::MAX)).can_send_again());
    }

    #[tokio::test]
    async fn subscription_detach_and_dropped_read_cancel_network_work() {
        let reference = RemoteChat::new(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        let mut subscription = RemoteSession::new(reference);
        let read = subscription.own_read(tokio::spawn(std::future::pending::<()>()));
        subscription.stop();
        assert!(read.await.unwrap_err().is_cancelled());
        struct Finished(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Finished {
            fn drop(&mut self) {
                let _ = self.0.take().unwrap().send(());
            }
        }
        let (finished, cancelled) = tokio::sync::oneshot::channel();
        let guard = Finished(Some(finished));
        let task = tokio::spawn(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        });
        drop(subscription.own_read(task));
        tokio::time::timeout(std::time::Duration::from_secs(2), cancelled)
            .await
            .unwrap()
            .unwrap();
    }
    #[gpui_kit::test]
    async fn remote_projection_is_a_view_source_without_a_local_agent_or_database(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            let mut workspace = canopy_desktop::state::workspace::Workspace::new();
            workspace.open_conversation("Remote", "claude");
            let mut pane = workspace.all_panes().remove(0);
            pane.metadata.remote = Some(Box::new(
                RemoteChat::new(
                    "5b860d32-7cf5-4ffc-a288-dc10cefe73a4".into(),
                    "2e4b3884-0e7a-4cec-bc87-ab3ff73401ea".into(),
                )
                .unwrap(),
            ));
            let runtime = cx.new(|cx| ChatRuntime::new(pane, cx));
            runtime.update(cx, |r, cx| {
                assert!(r.owned_chat_id().is_none());
                r.transport.grpc_mut().unwrap().agents = vec![p::AvailableAgent {
                    agent_id: "claude".into(),
                    backend_kind: p::BackendKind::Acp as i32,
                    ..Default::default()
                }];
                // The host owns backend selection even when a local tool has
                // the same name; a remote ACP must never borrow local controls.
                assert!(r.provider().is_none());
                assert_eq!(
                    r.backend_kind(cx),
                    Some(canopy_desktop::chat::agent::BackendKind::Acp)
                );
                assert!(r.process.is_none());
                assert!(r.native().is_none());
                assert!(r.host_queue().is_none());
                let mut projection = ChatProjection::default();
                projection.summary = Some(p::ChatSummary {
                    history_epoch: 4,
                    run_id: Some("9".into()),
                    agent_id: "claude".into(),
                    state: p::ChatState::Working as i32,
                    ..Default::default()
                });
                projection.queue = Some(p::ChatQueue {
                    revision: 7,
                    paused: true,
                    entries: vec![p::QueueEntry {
                        message_id: "queued-message".into(),
                        draft: Some(p::MessageDraft {
                            text: "keep this unsent message".into(),
                            ..Default::default()
                        }),
                        // An unrecognized delivery state must not become queued
                        // and offer an unsafe automatic retry.
                        delivery: i32::MAX,
                        resolved_by_user: false,
                    }],
                });
                projection.items = vec![
                    p::TranscriptItem {
                        key: Some(p::ItemKey {
                            history_epoch: 3,
                            item_id: 1,
                        }),
                        body: Some(p::transcript_item::Body::Text(p::TextBlock {
                            text: "earlier".into(),
                            streaming: false,
                        })),
                        ..Default::default()
                    },
                    p::TranscriptItem {
                        key: Some(p::ItemKey {
                            history_epoch: 4,
                            item_id: 1,
                        }),
                        body: Some(p::transcript_item::Body::Text(p::TextBlock {
                            text: "current".into(),
                            streaming: true,
                        })),
                        ..Default::default()
                    },
                    p::TranscriptItem {
                        key: Some(p::ItemKey {
                            history_epoch: 4,
                            item_id: 2,
                        }),
                        body: Some(p::transcript_item::Body::Decision(p::Decision {
                            decision_id: "permission".into(),
                            run_id: "9".into(),
                            title: "Read project".into(),
                            state: "pending".into(),
                            options: vec![p::OptionChoice {
                                id: "allow-once".into(),
                                label: "Allow once".into(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                ];
                r.apply_remote(projection, cx);
                assert_eq!(r.archived.len(), 1);
                assert_eq!(r.archived[0].epoch, 3);
                assert!(r.is_working());
                assert_eq!(r.transcript().unwrap().items().len(), 2);
                let snapshot = r.snapshot(cx);
                assert!(snapshot.working);
                assert_eq!(snapshot.history_epoch, 4);
                let queue = snapshot.queue.unwrap();
                assert_eq!(queue.revision(), 7);
                assert!(queue.paused());
                assert_eq!(
                    queue.entries()[0].draft().text(),
                    "keep this unsent message"
                );
                assert_eq!(
                    queue.entries()[0].state(),
                    canopy_desktop::host::QueueState::DeliveryUnknown
                );
                assert_eq!(snapshot.decisions.len(), 1);
                assert_eq!(snapshot.decisions[0].run, 9);
                assert_eq!(snapshot.decisions[0].item, 2);
                assert_eq!(snapshot.decisions[0].options[0].0, "allow-once");
                assert!(r.process.is_none());
                assert!(r.host_queue().is_none());
            });
            // Drop before the deferred attach: this test cannot load a user registry.
            drop(runtime);
        });
    }
}
