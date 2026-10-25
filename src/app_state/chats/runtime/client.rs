//! Client-side source for the ordinary chat runtime/view. No agent, local
//! history writer or host queue is created for these panes.
use super::*;
use canopy_desktop::{
    remote::{
        client::{ChatMutation, ChatProjection, HostClient, HostId, Phase, presentation as wire},
        proto as p,
    },
    state::workspace::RemoteChat,
};
use prost::Message;

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
    pub draft: Option<Draft>,
    pub draft_revision: u64,
    pub draft_context: Option<p::SendContext>,
    pub originals: HashMap<String, p::Attachment>,
    composer_refs: HashMap<String, p::Attachment>,
    pub preview: Option<p::EditPreview>,
    saved_edit: Option<(p::EditPreview, Draft)>,
    stream: Option<tokio::task::AbortHandle>,
    pump: Option<Task<()>>,
}
impl Drop for RemoteSession {
    fn drop(&mut self) {
        if let Some(task) = self.stream.take() {
            task.abort();
        }
    }
}
impl RemoteSession {
    pub(super) fn stop(&mut self) {
        if let Some(task) = self.stream.take() {
            task.abort();
        }
        self.pump = None;
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
            draft: None,
            draft_revision: 0,
            draft_context: None,
            originals: HashMap::new(),
            composer_refs: HashMap::new(),
            preview: None,
            saved_edit: None,
            stream: None,
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

impl ChatRuntime {
    pub(crate) fn client_resume(&mut self, cx: &mut Context<Self>) {
        let source = self.remote.as_ref().unwrap();
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
        let source = self.remote.as_ref().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let Some(context) = source.context() else {
            return;
        };
        let tool = context.agent_id.clone();
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
                    let target = projects.read(cx).catalog.active;
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
        let source = self.remote.as_ref().unwrap();
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
        let source = self.remote.as_ref().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let run = source.run();
        let call = source
            .projection
            .items
            .iter()
            .find_map(|i| match &i.body {
                Some(p::transcript_item::Body::Tool(t))
                    if i.key.is_some_and(|key| {
                        wire::call_id(key.history_epoch, &t.call_id) == call
                    }) =>
                {
                    Some(t.call_id.clone())
                }
                _ => None,
            })
            .unwrap_or(call);
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
        let source = self.remote.as_ref().unwrap();
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
        let source = self.remote.as_ref().unwrap();
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
                            this.remote.as_mut().unwrap().draft_context = context;
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
    pub(crate) fn remote_source(&self) -> Option<&RemoteSession> {
        self.remote.as_ref()
    }
    pub fn is_remote(&self) -> bool {
        self.remote.is_some()
    }
    pub(crate) fn remote_host(&self) -> Option<Arc<HostClient>> {
        self.remote.as_ref()?.host.clone()
    }
    pub(crate) fn update_client_options(&mut self) {
        let Some(source) = &self.remote else {
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
        let Some(source) = &self.remote else {
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
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(options)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.remote.as_mut().unwrap().options = options;
                    this.update_client_options();
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub(crate) fn remember_remote_attachment(&mut self, attachment: p::Attachment) {
        if let Some(source) = &mut self.remote {
            remember_attachment(&mut source.originals, &attachment);
            if source.composer_refs.len() < 128 {
                remember_attachment(&mut source.composer_refs, &attachment);
            }
        }
    }
    pub(crate) fn client_fork(
        &mut self,
        item: p::ItemKey,
        destination: Option<p::WorkspaceRef>,
        cx: &mut Context<Self>,
    ) {
        let source = self.remote.as_ref().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        let chat = source.chat();
        let tool = self.pane.tool.clone();
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
                    let target = projects.read(cx).catalog.active;
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
        let Some(remote) = self.remote.as_ref() else {
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
        let remote = self.remote.as_mut().expect("remote source");
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
                            this.remote.as_mut().unwrap().options = options;
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
        let remote = self.remote.as_mut().unwrap();
        remote.stream = Some(abort);
        remote.pump = Some(pump);
    }

    fn apply_remote(&mut self, mut projection: ChatProjection, cx: &mut Context<Self>) {
        let remote = self.remote.as_mut().unwrap();
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
        let Some(source) = self.remote.as_ref() else {
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
        cx.spawn(async move |this, cx| {
            if let Ok(Ok((agents, catalog))) = work.await {
                let _ = this.update(cx, |this, cx| {
                    let source = this.remote.as_mut().unwrap();
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
        let Ok(executor) = crate::ui::remote_client::executor(cx) else {
            return;
        };
        self.remote.as_mut().unwrap().busy = true;
        let task = executor.spawn(future);
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|_| {
                Err(
                    "The operation was interrupted; review its saved result before retrying."
                        .into(),
                )
            });
            let _ = this.update(cx, |this, cx| {
                this.remote.as_mut().unwrap().busy = false;
                if let Err(error) = result {
                    this.notice = Some(error);
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
    pub(crate) fn client_reconcile(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.remote_host() else {
            return;
        };
        self.remote_task(async move { host.reconcile().await }, cx);
    }
    pub(crate) fn client_retry_saved(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(host) = self.remote_host() else {
            return;
        };
        self.remote_task(
            async move {
                host.retry_chat_mutation(&id).await?;
                Ok(())
            },
            cx,
        );
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
        let Some(source) = self.remote.as_ref() else {
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
        self.remote.as_mut().unwrap().busy = true;
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
                let source = this.remote.as_mut().unwrap();
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
                    Err(error) => this.notice = Some(error),
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
        let Some(source) = &mut self.remote else {
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
        let Some(source) = &self.remote else {
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
        cx.spawn(async move |this, cx| {
            let preview = task.await.map_err(|e| e.to_string())??;
            let _ = this.update(cx, |this, _| {
                let source = this.remote.as_mut().unwrap();
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
        self.remote.as_mut()?.saved_edit.take()
    }
    pub(crate) fn save_client_edit(&mut self, previous: Option<Draft>, cx: &mut Context<Self>) {
        let source = self.remote.as_ref().unwrap();
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
        let source = self.remote.as_ref().unwrap();
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
        self.remote.as_mut().unwrap().busy = true;
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|_| {
                Err("Edit interrupted. Review the saved operation before retrying.".into())
            });
            let _ = this.update(cx, |this, cx| {
                this.remote.as_mut().unwrap().busy = false;
                cx.emit(ChatRuntimeEvent::EditFinished { revision, result });
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn client_shell(&mut self, command_line: String, cx: &mut Context<Self>) {
        let source = self.remote.as_ref().unwrap();
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
        self.remote.as_mut().unwrap().busy = true;
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                this.remote.as_mut().unwrap().busy = false;
                match result {
                    Ok(_) => cx.emit(ChatRuntimeEvent::DraftConsumed(revision)),
                    Err(error) => this.notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn client_stop_shell(&mut self, item: ItemId, cx: &mut Context<Self>) {
        let source = self.remote.as_ref().unwrap();
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
        let source = self.remote.as_ref().unwrap();
        let Some(host) = source.host.clone() else {
            return;
        };
        if source.history_loading {
            return;
        }
        let chat = source.chat();
        let before = around
            .is_none()
            .then(|| source.projection.items.first().and_then(|i| i.key))
            .flatten();
        let task = crate::ui::remote_client::executor(cx)
            .unwrap()
            .spawn(async move { host.history_page(chat, None, around, before).await });
        self.remote.as_mut().unwrap().history_loading = true;
        cx.spawn(async move |this, cx| {
            let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((items, next)) => {
                        let mut projection = this.remote.as_ref().unwrap().projection.clone();
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
                    Err(error) => {
                        this.notice = Some(error);
                        cx.notify();
                    }
                }
                this.remote.as_mut().unwrap().history_loading = false;
            });
        })
        .detach();
    }

    pub(crate) fn remote_command(&mut self, command: &'static str, cx: &mut Context<Self>) {
        let Some(source) = self.remote.as_ref() else {
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
        let Some(source) = self.remote.as_ref() else {
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
        let Some(source) = self.remote.as_ref() else {
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
        let Some(source) = self.remote.as_ref() else {
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
        let Some(source) = self.remote.as_ref() else {
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
    use super::{ChatProjection, ChatRuntime, RemoteChat, p};
    use gpui_kit::{AppContext, TestAppContext};
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
                assert!(r.is_remote());
                r.remote.as_mut().unwrap().agents = vec![p::AvailableAgent {
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
                    agent_id: "claude".into(),
                    state: p::ChatState::Working as i32,
                    ..Default::default()
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
                ];
                r.apply_remote(projection, cx);
                assert_eq!(r.archived.len(), 1);
                assert_eq!(r.archived[0].epoch, 3);
                assert!(r.is_working());
                assert_eq!(r.transcript().unwrap().items().len(), 1);
                assert!(r.process.is_none());
                assert!(r.host_queue().is_none());
            });
            // Drop before the deferred attach: this test cannot load a user registry.
            drop(runtime);
        });
    }
}
