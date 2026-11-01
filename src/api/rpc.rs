use super::{
    decode::{self, required},
    proto as p,
};
use crate::{
    command_bus::{
        Command, CommandBus, CommandEnvelope, DraftResources, Principal, QueueChange, ResourceRef,
    },
    host::{self, CommandReceipt, DeviceGrant, UnknownResolution},
    remote::{
        auth::{RpcAuth, storage_status},
        host::HostState,
        resources::ResourceLocks,
    },
};
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Publication after commit uses the existing runtime owner. It cannot replay effects.
#[tonic::async_trait]
pub trait ThreadPublisher: Send + Sync + 'static {
    async fn committed(&self, receipt: &CommandReceipt);
    async fn control(
        &self,
        _command: crate::command_bus::PreparedCommand,
        _resources: DraftResources,
    ) -> Result<CommandReceipt, Status> {
        Err(Status::unavailable("Thread runtime is unavailable."))
    }
    async fn subscribe(
        &self,
        _record: host::ChatRecord,
        _grant: DeviceGrant,
        _request: Request<p::SubscribeThreadRequest>,
    ) -> Result<crate::remote::host::RpcStream<p::ThreadEvent>, Status> {
        Err(Status::unavailable("Thread runtime is unavailable."))
    }
    async fn history(
        &self,
        _record: host::ChatRecord,
        _grant: DeviceGrant,
        _request: Request<p::ReadHistoryPageRequest>,
    ) -> Result<crate::remote::host::RpcStream<p::HistoryPageEvent>, Status> {
        Err(Status::unavailable("Thread runtime is unavailable."))
    }
    async fn search(
        &self,
        _record: host::ChatRecord,
        _grant: DeviceGrant,
        _request: Request<p::SearchHistoryRequest>,
    ) -> Result<p::HistoryMatches, Status> {
        Err(Status::unavailable("Thread runtime is unavailable."))
    }
    async fn options(
        &self,
        _record: host::ChatRecord,
        _grant: DeviceGrant,
        _request: Request<p::GetAgentOptionsRequest>,
    ) -> Result<p::AgentOptions, Status> {
        Err(Status::unavailable("Thread runtime is unavailable."))
    }
    async fn preview(
        &self,
        _record: host::ChatRecord,
        _grant: DeviceGrant,
        _request: Request<p::PreviewEditRequest>,
    ) -> Result<p::EditPreview, Status> {
        Err(Status::unavailable("Thread runtime is unavailable."))
    }
}

#[derive(Clone)]
pub struct ThreadRpc {
    pub(super) catalog: Option<Arc<dyn super::CatalogPublisher>>,
    auth: RpcAuth,
    state: HostState,
    operations: ResourceLocks,
    resources: DraftResources,
    publisher: Arc<dyn ThreadPublisher>,
}
impl ThreadRpc {
    pub fn new(
        auth: RpcAuth,
        state: HostState,
        operations: ResourceLocks,
        resources: DraftResources,
        publisher: Arc<dyn ThreadPublisher>,
    ) -> Self {
        Self {
            catalog: None,
            auth,
            state,
            operations,
            resources,
            publisher,
        }
    }
    async fn execute(
        &self,
        grant: DeviceGrant,
        meta: p::CommandMeta,
        target: p::ThreadTarget,
        command: Command,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.state.accepting()?;
        let envelope = decode::envelope(meta.clone(), decode::target(target.clone())?, command)?;
        let principal =
            Principal::authenticated(&grant, meta.command_epoch).map_err(storage_status)?;
        let receipt = self.execute_owned(envelope, principal).await?;
        let response = super::committed_receipt(
            &self.auth,
            &receipt,
            Some(p::command_receipt::Resource::Target(target)),
        )
        .await?;
        Ok(Response::new(response))
    }
    async fn execute_owned(
        &self,
        envelope: CommandEnvelope,
        principal: Principal,
    ) -> Result<CommandReceipt, Status> {
        let owned = self.operations.begin()?;
        let bus = CommandBus::new(self.auth.client().clone());
        let resources = self.resources.clone();
        let publisher = self.publisher.clone();
        tokio::spawn(async move {
            let _owned = owned;
            let receipt = if matches!(envelope.command, Command::Control(_)) {
                let prepared = bus
                    .prepare(envelope, principal)
                    .await
                    .map_err(storage_status)?;
                match prepared.prior() {
                    Some(prior) => prior.clone(),
                    None => publisher.control(prepared, resources).await?,
                }
            } else {
                bus.execute(envelope, principal, Some(&resources))
                    .await
                    .map_err(storage_status)?
            };
            publisher.committed(&receipt).await;
            Ok(receipt)
        })
        .await
        .map_err(|_| {
            Status::unknown(
                "Command outcome is uncertain; retry the original RPC with the same ID.",
            )
        })?
    }
    async fn queue(
        &self,
        grant: DeviceGrant,
        meta: Option<p::CommandMeta>,
        target: Option<p::ThreadTarget>,
        revision: u64,
        change: QueueChange,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.execute(
            grant,
            required(meta)?,
            required(target)?,
            Command::Queue { revision, change },
        )
        .await
    }
}

#[tonic::async_trait]
impl p::catalog_service_server::CatalogService for ThreadRpc {
    type ReadCatalogStream = crate::remote::host::RpcStream<p::SummaryEvent>;
    type WatchSummariesStream = crate::remote::host::RpcStream<p::SummaryEvent>;
    type ReadThreadPageStream = crate::remote::host::RpcStream<p::ArtifactChunk>;
    async fn read_catalog(
        &self,
        r: Request<p::ReadCatalogRequest>,
    ) -> Result<Response<Self::ReadCatalogStream>, Status> {
        self.catalog()?.read_catalog(r).await
    }
    async fn list_resources(
        &self,
        r: Request<p::ListResourcesRequest>,
    ) -> Result<Response<p::Catalog>, Status> {
        self.catalog()?.list_resources(r).await
    }
    async fn watch_summaries(
        &self,
        r: Request<p::WatchSummariesRequest>,
    ) -> Result<Response<Self::WatchSummariesStream>, Status> {
        self.catalog()?.watch_summaries(r).await
    }
    async fn list_agents(
        &self,
        r: Request<p::ListAgentsRequest>,
    ) -> Result<Response<p::AgentDiscovery>, Status> {
        self.catalog()?.list_agents(r).await
    }
    async fn get_agent_catalog(
        &self,
        r: Request<p::GetAgentCatalogRequest>,
    ) -> Result<Response<p::AgentCatalog>, Status> {
        self.catalog()?.get_agent_catalog(r).await
    }
    async fn list_pull_requests(
        &self,
        r: Request<p::ListPullRequestsRequest>,
    ) -> Result<Response<p::PullRequests>, Status> {
        self.catalog()?.list_pull_requests(r).await
    }
    async fn read_thread_page(
        &self,
        r: Request<p::ReadThreadPageRequest>,
    ) -> Result<Response<Self::ReadThreadPageStream>, Status> {
        self.catalog()?.read_thread_page(r).await
    }
    async fn preflight_close(
        &self,
        r: Request<p::PreflightCloseRequest>,
    ) -> Result<Response<p::ClosePlan>, Status> {
        self.catalog()?.preflight_close(r).await
    }
    async fn create_thread(
        &self,
        r: Request<p::CreateThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.catalog()?.create_thread(r).await
    }
    async fn preflight_close_project(
        &self,
        r: Request<p::PreflightCloseProjectRequest>,
    ) -> Result<Response<p::ProjectClosePlan>, Status> {
        self.catalog()?.preflight_close_project(r).await
    }
    async fn preflight_close_terminal(
        &self,
        r: Request<p::PreflightCloseTerminalRequest>,
    ) -> Result<Response<p::TerminalClosePlan>, Status> {
        self.catalog()?.preflight_close_terminal(r).await
    }
    async fn close_project(
        &self,
        r: Request<p::CloseProjectRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.catalog()?.close_project(r).await
    }
    async fn close_terminal(
        &self,
        r: Request<p::CloseTerminalRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.catalog()?.close_terminal(r).await
    }
    async fn close_thread(
        &self,
        r: Request<p::CloseThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.catalog()?.close_thread(r).await
    }
    async fn open_project(
        &self,
        r: Request<p::OpenProjectRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        self.catalog()?.open_project(r).await
    }
    async fn list_threads(
        &self,
        r: Request<p::ListThreadsRequest>,
    ) -> Result<Response<p::ListThreadsResponse>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let environment = decode::environment(required(r.environment.clone())?)?;
        self.auth.check_host(environment.host_id.as_str())?;
        self.auth.scope(&grant, host::READ)?;
        self.list_threads_page(grant, r).await.map(Response::new)
    }
    async fn rename_thread(
        &self,
        r: Request<p::RenameThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Rename { title: r.title },
        )
        .await
    }
}

impl ThreadRpc {
    async fn read_target(
        &self,
        grant: &DeviceGrant,
        reference: Option<p::ThreadRef>,
        scope: u32,
    ) -> Result<host::ChatRecord, Status> {
        let reference = required(reference)?;
        self.auth.check_host(&reference.host_id)?;
        self.auth.scope(grant, scope)?;
        let id = crate::thread::ThreadId::parse(&reference.thread_id)
            .ok_or_else(|| Status::invalid_argument("Invalid thread identity."))?;
        let record = self
            .auth
            .client()
            .load_runtime_thread(id)
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::not_found("Thread is no longer available."))?;
        self.auth
            .resource(grant.id(), scope, record.authority_root())
            .await?;
        Ok(record)
    }
    async fn list_threads_page(
        &self,
        grant: DeviceGrant,
        r: p::ListThreadsRequest,
    ) -> Result<p::ListThreadsResponse, Status> {
        if r.page_token.as_ref().is_some_and(|v| !host::valid_id(v))
            || r.catalog_id.is_some() && r.page_token.is_some()
        {
            return Err(Status::invalid_argument("Invalid thread page token."));
        }
        let limit = if r.limit == 0 { 100 } else { r.limit.min(200) } as usize;
        let mut response = p::ListThreadsResponse::default();
        let mut after = r.page_token;
        // Bound scanning as well as output, even when all rows are inaccessible.
        for _ in 0..16 {
            let records = if let Some(id) = &r.catalog_id {
                self.auth
                    .client()
                    .load_runtime_chat(id.clone())
                    .await
                    .map_err(storage_status)?
                    .into_iter()
                    .collect()
            } else {
                self.auth
                    .client()
                    .runtime_chats_page(after.clone())
                    .await
                    .map_err(storage_status)?
            };
            let exhausted = r.catalog_id.is_some() || records.len() < 64;
            for record in records {
                match self
                    .auth
                    .resource(grant.id(), host::READ, record.authority_root())
                    .await
                {
                    Ok(_) => (),
                    Err(e) if e.code() == tonic::Code::PermissionDenied => {
                        after = Some(record.id().into());
                        continue;
                    }
                    Err(e) => return Err(e),
                }
                let stored = record
                    .thread()
                    .ok_or_else(|| Status::internal("Thread identity is unavailable."))?;
                let summary = p::ThreadSummary {
                    target: Some(p::ThreadTarget {
                        thread: Some(p::ThreadRef {
                            host_id: stored.thread().host_id.to_string(),
                            thread_id: stored.thread().thread_id.to_string(),
                        }),
                        executor_epoch: stored.executor_epoch(),
                    }),
                    catalog_id: record.id().into(),
                    title: record.metadata().title.clone().unwrap_or_default(),
                    revision: stored.revision(),
                };
                // Reserve a maximal token plus framing before consuming this row.
                if prost::Message::encoded_len(&response)
                    + prost::Message::encoded_len(&summary)
                    + 512
                    > host::CONTROL_BYTES
                {
                    if response.threads.is_empty() {
                        return Err(Status::resource_exhausted(
                            "A thread row exceeds the control limit.",
                        ));
                    }
                    response.next_page_token = after;
                    return Ok(response);
                }
                after = Some(record.id().into());
                response.threads.push(summary);
                if response.threads.len() == limit && r.catalog_id.is_none() {
                    response.next_page_token = after;
                    return Ok(response);
                }
            }
            if exhausted {
                return Ok(response);
            }
        }
        response.next_page_token = after;
        Ok(response)
    }
}

#[tonic::async_trait]
impl p::thread_service_server::ThreadService for ThreadRpc {
    type SubscribeThreadStream = crate::remote::host::RpcStream<p::ThreadEvent>;
    type ReadHistoryPageStream = crate::remote::host::RpcStream<p::HistoryPageEvent>;
    async fn subscribe_thread(
        &self,
        r: Request<p::SubscribeThreadRequest>,
    ) -> Result<Response<Self::SubscribeThreadStream>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let record = self
            .read_target(&grant, r.get_ref().thread.clone(), host::READ)
            .await?;
        self.publisher
            .subscribe(record, grant, r)
            .await
            .map(Response::new)
    }
    async fn read_history_page(
        &self,
        r: Request<p::ReadHistoryPageRequest>,
    ) -> Result<Response<Self::ReadHistoryPageStream>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let record = self
            .read_target(&grant, r.get_ref().thread.clone(), host::READ)
            .await?;
        self.publisher
            .history(record, grant, r)
            .await
            .map(Response::new)
    }
    async fn search_history(
        &self,
        r: Request<p::SearchHistoryRequest>,
    ) -> Result<Response<p::HistoryMatches>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let record = self
            .read_target(&grant, r.get_ref().thread.clone(), host::READ)
            .await?;
        self.publisher
            .search(record, grant, r)
            .await
            .map(Response::new)
    }
    async fn get_agent_options(
        &self,
        r: Request<p::GetAgentOptionsRequest>,
    ) -> Result<Response<p::AgentOptions>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let record = self
            .read_target(&grant, r.get_ref().thread.clone(), host::READ)
            .await?;
        self.publisher
            .options(record, grant, r)
            .await
            .map(Response::new)
    }
    async fn preview_edit(
        &self,
        r: Request<p::PreviewEditRequest>,
    ) -> Result<Response<p::EditPreview>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let record = self
            .read_target(&grant, r.get_ref().thread.clone(), host::CHAT)
            .await?;
        self.publisher
            .preview(record, grant, r)
            .await
            .map(Response::new)
    }
    async fn start(
        &self,
        r: Request<p::StartRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::Start),
        )
        .await
    }
    async fn interrupt(
        &self,
        r: Request<p::InterruptRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::Interrupt { run: r.run_id }),
        )
        .await
    }
    async fn restart(
        &self,
        r: Request<p::RestartRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::Restart { run: r.run_id }),
        )
        .await
    }
    async fn steer(
        &self,
        r: Request<p::SteerRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::Steer {
                run: r.run_id,
                message: r.message_id,
                draft: r.draft.map(decode::draft).transpose()?,
            }),
        )
        .await
    }
    async fn respond_decision(
        &self,
        r: Request<p::RespondDecisionRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        use crate::command_bus::DecisionAnswer as A;
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let answer = match required(r.response)? {
            p::respond_decision_request::Response::OptionId(id) => A::Option(id),
            p::respond_decision_request::Response::Native(n) => {
                match p::NativeDecisionAction::try_from(n.action) {
                    Ok(p::NativeDecisionAction::AllowOnce) => A::Allow,
                    Ok(p::NativeDecisionAction::AllowForThread) => A::AllowForThread,
                    Ok(p::NativeDecisionAction::Deny) => A::Deny(n.note),
                    Ok(p::NativeDecisionAction::Answer) => A::Answer(n.answers),
                    Ok(p::NativeDecisionAction::ApprovePlan) => A::ApprovePlan {
                        accept_edits: n.accept_edits,
                    },
                    Ok(p::NativeDecisionAction::KeepPlanning) => A::KeepPlanning(n.note),
                    _ => return Err(Status::invalid_argument("Invalid decision action.")),
                }
            }
        };
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::Respond {
                run: r.run_id,
                item: r.decision_id,
                answer,
            }),
        )
        .await
    }
    async fn set_agent_option(
        &self,
        r: Request<p::SetAgentOptionRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::SetOption {
                id: r.option_id,
                value: r.value_id,
            }),
        )
        .await
    }
    async fn set_thread_agent(
        &self,
        r: Request<p::SetThreadAgentRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::SetAgent {
                tool: r.agent_id,
                profile: r.profile_id,
                model: r.model,
            }),
        )
        .await
    }
    async fn set_goal(
        &self,
        r: Request<p::SetGoalRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::SetGoal {
                objective: r.objective,
            }),
        )
        .await
    }
    async fn pause_goal(
        &self,
        r: Request<p::PauseGoalRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::PauseGoal),
        )
        .await
    }
    async fn resume_goal(
        &self,
        r: Request<p::ResumeGoalRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::ResumeGoal),
        )
        .await
    }
    async fn clear_goal(
        &self,
        r: Request<p::ClearGoalRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::ClearGoal),
        )
        .await
    }
    async fn stop_background(
        &self,
        r: Request<p::StopBackgroundRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::StopBackground {
                run: r.run_id,
                call: r.call_id,
            }),
        )
        .await
    }
    async fn edit_message(
        &self,
        r: Request<p::EditMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let item = required(r.item)?;
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::Edit {
                run: r.run_id,
                history_epoch: item.history_epoch,
                item: item.item_id,
                draft: decode::draft(required(r.draft)?)?,
                restore_files: r.restore_files,
            }),
        )
        .await
    }
    async fn run_shell(
        &self,
        r: Request<p::RunShellRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::RunShell {
                run: r.run_id,
                command: r.command_line,
            }),
        )
        .await
    }
    async fn stop_shell(
        &self,
        r: Request<p::StopShellRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let item = required(r.item)?;
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::StopShell {
                run: r.run_id,
                history_epoch: item.history_epoch,
                item: item.item_id,
            }),
        )
        .await
    }
    async fn fork_thread(
        &self,
        r: Request<p::ForkThreadRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::Control(crate::command_bus::Control::Fork {
                history_epoch: r.history_epoch,
                item: r.item_id,
                replacement: r.replacement.map(decode::draft).transpose()?,
                local_destination: None,
            }),
        )
        .await
    }
    async fn send_message(
        &self,
        r: Request<p::SendMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let context = required(r.expected_context)?;
        self.execute(
            grant,
            required(r.command)?,
            required(r.target)?,
            Command::ThreadSend {
                message: r.message_id,
                draft: decode::draft(required(r.draft)?)?,
                account: context.account_id,
                profile: context.profile_id,
                history_epoch: context.history_epoch,
            },
        )
        .await
    }
    async fn edit_queue_message(
        &self,
        r: Request<p::EditQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Edit {
                message: r.message_id,
                draft: decode::draft(required(r.draft)?)?,
            },
        )
        .await
    }
    async fn move_queue_message(
        &self,
        r: Request<p::MoveQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Move {
                message: r.message_id,
                before: r.before_message_id,
            },
        )
        .await
    }
    async fn remove_queue_message(
        &self,
        r: Request<p::RemoveQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Remove {
                message: r.message_id,
            },
        )
        .await
    }
    async fn pause_queue(
        &self,
        r: Request<p::PauseQueueRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::Pause { paused: r.paused },
        )
        .await
    }
    async fn resolve_unknown_queue_message(
        &self,
        r: Request<p::ResolveUnknownQueueMessageRequest>,
    ) -> Result<Response<p::CommandReceipt>, Status> {
        let grant = self.auth.authenticate(&r).await?;
        let r = r.into_inner();
        let resolution = match p::UnknownResolution::try_from(r.resolution) {
            Ok(p::UnknownResolution::AcceptAsDelivered) => UnknownResolution::Delivered,
            Ok(p::UnknownResolution::SendAgain) => UnknownResolution::Resend {
                message: required(r.new_message_id)?,
            },
            Ok(p::UnknownResolution::Dismiss) => UnknownResolution::Dismissed,
            _ => {
                return Err(Status::invalid_argument(
                    "Choose how to resolve the uncertain message.",
                ));
            }
        };
        self.queue(
            grant,
            r.command,
            r.target,
            r.queue_revision,
            QueueChange::ResolveUnknown {
                message: r.message_id,
                resolution,
            },
        )
        .await
    }
}

pub(crate) fn preferences_envelope(
    mut r: p::UpdatePreferencesRequest,
) -> Result<(p::CommandMeta, p::EnvironmentRef, CommandEnvelope), Status> {
    let meta = required(r.command.take())?;
    let environment = required(r.environment.take())?;
    let envelope = decode::envelope(
        meta.clone(),
        ResourceRef::Environment(decode::environment(environment.clone())?),
        decode::preferences(r),
    )?;
    Ok((meta, environment, envelope))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Silent;
    #[tonic::async_trait]
    impl ThreadPublisher for Silent {
        async fn committed(&self, _: &CommandReceipt) {}
    }

    #[tokio::test]
    async fn thread_pages_advance_across_inaccessible_rows_without_losing_the_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let client = crate::settings::SettingsClient::create(dir.path().join("pages.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let mut expected = Vec::new();
        for n in 0..1050 {
            let id = format!("00000000-0000-4000-8000-{n:012}");
            let visible = n >= 1024 && n % 2 == 0;
            let record = host::ChatRecord::new(
                id.clone(),
                None,
                "account".into(),
                crate::state::workspace::PaneMetadata {
                    kind: crate::state::workspace::PaneKind::Chat,
                    chat: Some(Box::new(crate::chat::config::ChatSettings {
                        id: Some(id.clone()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap()
            .with_authority_root(if visible { "private" } else { "/hidden" }.into())
            .unwrap();
            client.save_runtime_chat(record).await.unwrap();
            if visible {
                expected.push(id);
            }
        }
        client
            .register_runtime_device(
                host::DeviceRegistration::new(
                    uuid::Uuid::new_v4().to_string(),
                    "reader".into(),
                    vec![8; 32],
                    host::READ,
                    vec!["private".into()],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let grant = client.lookup_runtime_device(vec![8; 32]).await.unwrap();
        let rpc = ThreadRpc::new(
            RpcAuth::new(host.id().into(), client.clone()).unwrap(),
            HostState::new(host.id(), vec![]),
            ResourceLocks::default(),
            DraftResources {
                roots: Default::default(),
                pulls: Default::default(),
            },
            Arc::new(Silent),
        );
        let request = |page_token| p::ListThreadsRequest {
            environment: Some(p::EnvironmentRef {
                host_id: host.id().into(),
            }),
            page_token,
            limit: 3,
            catalog_id: None,
        };
        let first = rpc
            .list_threads_page(grant.clone(), request(None))
            .await
            .unwrap();
        assert!(
            first.threads.is_empty(),
            "scanning itself is bounded even with no visible rows"
        );
        let mut token = first.next_page_token;
        assert!(token.is_some());
        let mut received = Vec::new();
        for _ in 0..20 {
            let page = rpc
                .list_threads_page(grant.clone(), request(token.clone()))
                .await
                .unwrap();
            assert!(page.threads.len() <= 3);
            assert!(prost::Message::encoded_len(&page) <= host::CONTROL_BYTES);
            received.extend(page.threads.into_iter().map(|r| r.catalog_id));
            if page.next_page_token.is_none() {
                token = None;
                break;
            }
            assert_ne!(page.next_page_token, token);
            token = page.next_page_token;
        }
        assert!(token.is_none());
        assert_eq!(received, expected);
        let mut lookup = request(None);
        lookup.catalog_id = Some(received[3].clone());
        assert_eq!(
            rpc.list_threads_page(grant.clone(), lookup.clone())
                .await
                .unwrap()
                .threads[0]
                .catalog_id,
            received[3]
        );
        lookup.catalog_id = Some("00000000-0000-4000-8000-000000000000".into());
        assert!(
            rpc.list_threads_page(grant.clone(), lookup.clone())
                .await
                .unwrap()
                .threads
                .is_empty()
        );
        lookup.page_token = Some(received[0].clone());
        assert_eq!(
            rpc.list_threads_page(grant, lookup)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
        client.shutdown().await.unwrap();
    }
    struct Publication {
        started: tokio::sync::Notify,
        finish: tokio::sync::Notify,
    }
    #[tonic::async_trait]
    impl ThreadPublisher for Publication {
        async fn committed(&self, _: &CommandReceipt) {
            self.started.notify_one();
            self.finish.notified().await;
        }
    }
    #[tokio::test]
    async fn cancelled_rpc_keeps_committed_effect_and_publication_owned_until_drain() {
        let dir = tempfile::tempdir().unwrap();
        let client = crate::settings::SettingsClient::create(dir.path().join("test.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let record = client
            .save_runtime_chat(
                host::ChatRecord::new(
                    id.clone(),
                    None,
                    "test-account".into(),
                    crate::state::workspace::PaneMetadata {
                        kind: crate::state::workspace::PaneKind::Chat,
                        chat: Some(Box::new(crate::chat::config::ChatSettings {
                            id: Some(id.clone()),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
            .await
            .unwrap();
        client
            .register_runtime_device(
                host::DeviceRegistration::new(
                    uuid::Uuid::new_v4().to_string(),
                    "test".into(),
                    vec![7; 32],
                    host::ALL_SCOPES,
                    vec!["private".into()],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let grant = client.lookup_runtime_device(vec![7; 32]).await.unwrap();
        let state = HostState::new(host.id(), vec![]);
        state.set(p::HostReadiness::Ready, String::new());
        let operations = ResourceLocks::default();
        let publication = Arc::new(Publication {
            started: tokio::sync::Notify::new(),
            finish: tokio::sync::Notify::new(),
        });
        let rpc = ThreadRpc::new(
            RpcAuth::new(host.id().into(), client.clone()).unwrap(),
            state,
            operations.clone(),
            DraftResources {
                roots: Default::default(),
                pulls: Default::default(),
            },
            publication.clone(),
        );
        let stored = record.thread().unwrap();
        let target = p::ThreadTarget {
            thread: Some(p::ThreadRef {
                host_id: host.id().into(),
                thread_id: stored.thread().thread_id.to_string(),
            }),
            executor_epoch: stored.executor_epoch(),
        };
        let request = tokio::spawn(async move {
            rpc.execute(
                grant,
                p::CommandMeta {
                    command_id: uuid::Uuid::new_v4().to_string(),
                    command_epoch: 1,
                    expected_revision: None,
                },
                target,
                Command::Rename {
                    title: "committed before cancel".into(),
                },
            )
            .await
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            publication.started.notified(),
        )
        .await
        .unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        let drain = operations.drain();
        tokio::pin!(drain);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut drain)
                .await
                .is_err()
        );
        publication.finish.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(2), drain)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            client
                .load_runtime_chat(id)
                .await
                .unwrap()
                .unwrap()
                .metadata()
                .title
                .as_deref(),
            Some("committed before cancel")
        );
        client.shutdown().await.unwrap();
    }
}
