//! Exact typed requests share the existing device journal and epoch owner.
use super::{
    HostClient,
    commands::{check_receipt, terminal_state},
    rpc_error,
    store::MutationRecord,
};
use crate::{
    api::{compat, proto as p},
    remote::proto as old,
};
use prost::Message;
use tonic::transport::Channel;

#[derive(Clone)]
pub(super) enum ApiIntent {
    Create(p::CreateThreadRequest),
    Close(p::CloseThreadRequest),
    OpenProject(p::OpenProjectRequest),
    Start(p::StartRequest),
    Interrupt(p::InterruptRequest),
    Restart(p::RestartRequest),
    Steer(p::SteerRequest),
    Respond(p::RespondDecisionRequest),
    Option(p::SetAgentOptionRequest),
    Agent(p::SetThreadAgentRequest),
    Goal(p::SetGoalRequest),
    PauseGoal(p::PauseGoalRequest),
    ResumeGoal(p::ResumeGoalRequest),
    ClearGoal(p::ClearGoalRequest),
    Background(p::StopBackgroundRequest),
    MessageEdit(p::EditMessageRequest),
    Shell(p::RunShellRequest),
    StopShell(p::StopShellRequest),
    Fork(p::ForkThreadRequest),
    Rename(p::RenameThreadRequest),
    Send(p::SendMessageRequest),
    Edit(p::EditQueueMessageRequest),
    Move(p::MoveQueueMessageRequest),
    Remove(p::RemoveQueueMessageRequest),
    Pause(p::PauseQueueRequest),
    Resolve(p::ResolveUnknownQueueMessageRequest),
    Preferences(p::UpdatePreferencesRequest),
}

macro_rules! request_field {
    ($value:expr, $field:ident) => {
        match $value {
            ApiIntent::Create(v) => &mut v.$field,
            ApiIntent::Close(v) => &mut v.$field,
            ApiIntent::OpenProject(v) => &mut v.$field,
            ApiIntent::Start(v) => &mut v.$field,
            ApiIntent::Interrupt(v) => &mut v.$field,
            ApiIntent::Restart(v) => &mut v.$field,
            ApiIntent::Steer(v) => &mut v.$field,
            ApiIntent::Respond(v) => &mut v.$field,
            ApiIntent::Option(v) => &mut v.$field,
            ApiIntent::Agent(v) => &mut v.$field,
            ApiIntent::Goal(v) => &mut v.$field,
            ApiIntent::PauseGoal(v) => &mut v.$field,
            ApiIntent::ResumeGoal(v) => &mut v.$field,
            ApiIntent::ClearGoal(v) => &mut v.$field,
            ApiIntent::Background(v) => &mut v.$field,
            ApiIntent::MessageEdit(v) => &mut v.$field,
            ApiIntent::Shell(v) => &mut v.$field,
            ApiIntent::StopShell(v) => &mut v.$field,
            ApiIntent::Fork(v) => &mut v.$field,
            ApiIntent::Rename(v) => &mut v.$field,
            ApiIntent::Send(v) => &mut v.$field,
            ApiIntent::Edit(v) => &mut v.$field,
            ApiIntent::Move(v) => &mut v.$field,
            ApiIntent::Remove(v) => &mut v.$field,
            ApiIntent::Pause(v) => &mut v.$field,
            ApiIntent::Resolve(v) => &mut v.$field,
            ApiIntent::Preferences(v) => &mut v.$field,
        }
    };
}
impl ApiIntent {
    pub(super) fn meta(&mut self) -> &mut Option<p::CommandMeta> {
        request_field!(self, command)
    }
    pub(super) fn encode(&self) -> (&'static str, Vec<u8>) {
        match self {
            Self::Create(v) => ("create", v.encode_to_vec()),
            Self::Close(v) => ("close", v.encode_to_vec()),
            Self::OpenProject(v) => ("project.open", v.encode_to_vec()),
            Self::Start(v) => ("start", v.encode_to_vec()),
            Self::Interrupt(v) => ("interrupt", v.encode_to_vec()),
            Self::Restart(v) => ("restart", v.encode_to_vec()),
            Self::Steer(v) => ("steer", v.encode_to_vec()),
            Self::Respond(v) => ("respond", v.encode_to_vec()),
            Self::Option(v) => ("option", v.encode_to_vec()),
            Self::Agent(v) => ("agent", v.encode_to_vec()),
            Self::Goal(v) => ("goal", v.encode_to_vec()),
            Self::PauseGoal(v) => ("goal.pause", v.encode_to_vec()),
            Self::ResumeGoal(v) => ("goal.resume", v.encode_to_vec()),
            Self::ClearGoal(v) => ("goal.clear", v.encode_to_vec()),
            Self::Background(v) => ("background", v.encode_to_vec()),
            Self::MessageEdit(v) => ("message.edit", v.encode_to_vec()),
            Self::Shell(v) => ("shell", v.encode_to_vec()),
            Self::StopShell(v) => ("shell.stop", v.encode_to_vec()),
            Self::Fork(v) => ("fork", v.encode_to_vec()),
            Self::Rename(v) => ("rename", v.encode_to_vec()),
            Self::Send(v) => ("send", v.encode_to_vec()),
            Self::Edit(v) => ("queue.edit", v.encode_to_vec()),
            Self::Move(v) => ("queue.move", v.encode_to_vec()),
            Self::Remove(v) => ("queue.remove", v.encode_to_vec()),
            Self::Pause(v) => ("queue.pause", v.encode_to_vec()),
            Self::Resolve(v) => ("queue.resolve", v.encode_to_vec()),
            Self::Preferences(v) => ("preferences", v.encode_to_vec()),
        }
    }
    pub(super) fn decode(kind: &str, bytes: &[u8]) -> Result<Self, String> {
        match kind {
            "create" => p::CreateThreadRequest::decode(bytes).map(Self::Create),
            "close" => p::CloseThreadRequest::decode(bytes).map(Self::Close),
            "project.open" => p::OpenProjectRequest::decode(bytes).map(Self::OpenProject),
            "start" => p::StartRequest::decode(bytes).map(Self::Start),
            "interrupt" => p::InterruptRequest::decode(bytes).map(Self::Interrupt),
            "restart" => p::RestartRequest::decode(bytes).map(Self::Restart),
            "steer" => p::SteerRequest::decode(bytes).map(Self::Steer),
            "respond" => p::RespondDecisionRequest::decode(bytes).map(Self::Respond),
            "option" => p::SetAgentOptionRequest::decode(bytes).map(Self::Option),
            "agent" => p::SetThreadAgentRequest::decode(bytes).map(Self::Agent),
            "goal" => p::SetGoalRequest::decode(bytes).map(Self::Goal),
            "goal.pause" => p::PauseGoalRequest::decode(bytes).map(Self::PauseGoal),
            "goal.resume" => p::ResumeGoalRequest::decode(bytes).map(Self::ResumeGoal),
            "goal.clear" => p::ClearGoalRequest::decode(bytes).map(Self::ClearGoal),
            "background" => p::StopBackgroundRequest::decode(bytes).map(Self::Background),
            "message.edit" => p::EditMessageRequest::decode(bytes).map(Self::MessageEdit),
            "shell" => p::RunShellRequest::decode(bytes).map(Self::Shell),
            "shell.stop" => p::StopShellRequest::decode(bytes).map(Self::StopShell),
            "fork" => p::ForkThreadRequest::decode(bytes).map(Self::Fork),
            "rename" => p::RenameThreadRequest::decode(bytes).map(Self::Rename),
            "send" => p::SendMessageRequest::decode(bytes).map(Self::Send),
            "queue.edit" => p::EditQueueMessageRequest::decode(bytes).map(Self::Edit),
            "queue.move" => p::MoveQueueMessageRequest::decode(bytes).map(Self::Move),
            "queue.remove" => p::RemoveQueueMessageRequest::decode(bytes).map(Self::Remove),
            "queue.pause" => p::PauseQueueRequest::decode(bytes).map(Self::Pause),
            "queue.resolve" => {
                p::ResolveUnknownQueueMessageRequest::decode(bytes).map(Self::Resolve)
            }
            "preferences" => p::UpdatePreferencesRequest::decode(bytes).map(Self::Preferences),
            _ => return Err("Unknown saved API request; intent retained.".into()),
        }
        .map_err(|_| "Invalid saved API request; intent retained.".into())
    }
    fn resource(&self) -> Option<p::command_receipt::Resource> {
        let target = match self {
            Self::Close(v) => &v.target,
            Self::Create(v) => {
                return v.resource.clone().map(|r| match r {
                    p::create_thread_request::Resource::Project(p) => {
                        p::command_receipt::Resource::Project(p)
                    }
                    p::create_thread_request::Resource::Environment(e) => {
                        p::command_receipt::Resource::Environment(e)
                    }
                });
            }
            Self::OpenProject(v) => {
                return v
                    .environment
                    .clone()
                    .map(p::command_receipt::Resource::Environment);
            }
            Self::Start(v) => &v.target,
            Self::Interrupt(v) => &v.target,
            Self::Restart(v) => &v.target,
            Self::Steer(v) => &v.target,
            Self::Respond(v) => &v.target,
            Self::Option(v) => &v.target,
            Self::Agent(v) => &v.target,
            Self::Goal(v) => &v.target,
            Self::PauseGoal(v) => &v.target,
            Self::ResumeGoal(v) => &v.target,
            Self::ClearGoal(v) => &v.target,
            Self::Background(v) => &v.target,
            Self::MessageEdit(v) => &v.target,
            Self::Shell(v) => &v.target,
            Self::StopShell(v) => &v.target,
            Self::Fork(v) => &v.target,
            Self::Rename(v) => &v.target,
            Self::Send(v) => &v.target,
            Self::Edit(v) => &v.target,
            Self::Move(v) => &v.target,
            Self::Remove(v) => &v.target,
            Self::Pause(v) => &v.target,
            Self::Resolve(v) => &v.target,
            Self::Preferences(v) => {
                return v
                    .environment
                    .clone()
                    .map(p::command_receipt::Resource::Environment);
            }
        };
        target.clone().map(p::command_receipt::Resource::Target)
    }
    pub(super) async fn dispatch(
        mut self,
        channel: Channel,
        host: &str,
        catalog_id: String,
    ) -> Result<old::CommandReceipt, String> {
        let meta = self
            .meta()
            .clone()
            .ok_or("Saved API command identity is missing.")?;
        let resource = self.resource().ok_or("Saved API target is missing.")?;
        let requested_host = match &resource {
            p::command_receipt::Resource::Target(target) => {
                let thread = target
                    .thread
                    .as_ref()
                    .ok_or("Thread identity is missing.")?;
                if crate::thread::ThreadId::parse(&thread.thread_id).is_none() {
                    return Err("Invalid saved thread identity.".into());
                }
                &thread.host_id
            }
            p::command_receipt::Resource::Environment(e) => &e.host_id,
            p::command_receipt::Resource::Project(p) => &p.host_id,
        };
        if requested_host != host {
            return Err("Saved API request belongs to another host.".into());
        }
        let mut thread = p::thread_service_client::ThreadServiceClient::new(channel.clone());
        let result = match self {
            Self::Create(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .create_thread(v)
                    .await
            }
            Self::Close(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .close_thread(v)
                    .await
            }
            Self::OpenProject(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .open_project(v)
                    .await
            }
            Self::Start(v) => thread.start(v).await,
            Self::Interrupt(v) => thread.interrupt(v).await,
            Self::Restart(v) => thread.restart(v).await,
            Self::Steer(v) => thread.steer(v).await,
            Self::Respond(v) => thread.respond_decision(v).await,
            Self::Option(v) => thread.set_agent_option(v).await,
            Self::Agent(v) => thread.set_thread_agent(v).await,
            Self::Goal(v) => thread.set_goal(v).await,
            Self::PauseGoal(v) => thread.pause_goal(v).await,
            Self::ResumeGoal(v) => thread.resume_goal(v).await,
            Self::ClearGoal(v) => thread.clear_goal(v).await,
            Self::Background(v) => thread.stop_background(v).await,
            Self::MessageEdit(v) => thread.edit_message(v).await,
            Self::Shell(v) => thread.run_shell(v).await,
            Self::StopShell(v) => thread.stop_shell(v).await,
            Self::Fork(v) => thread.fork_thread(v).await,
            Self::Rename(v) => {
                p::catalog_service_client::CatalogServiceClient::new(channel)
                    .rename_thread(v)
                    .await
            }
            Self::Send(v) => thread.send_message(v).await,
            Self::Edit(v) => thread.edit_queue_message(v).await,
            Self::Move(v) => thread.move_queue_message(v).await,
            Self::Remove(v) => thread.remove_queue_message(v).await,
            Self::Pause(v) => thread.pause_queue(v).await,
            Self::Resolve(v) => thread.resolve_unknown_queue_message(v).await,
            Self::Preferences(v) => {
                p::settings_service_client::SettingsServiceClient::new(channel)
                    .update_preferences(v)
                    .await
            }
        }
        .map_err(rpc_error)?
        .into_inner();
        if result.host_id != host
            || result.command_id != meta.command_id
            || result.command_epoch != meta.command_epoch
            || result.resource.as_ref() != Some(&resource)
        {
            return Err("API result identity differs from the original request; retain intent for reconciliation.".into());
        }
        Ok(compat::legacy_receipt(result, catalog_id))
    }
}

impl HostClient {
    pub async fn start_thread(&self, chat: old::ChatRef) -> Result<old::CommandReceipt, String> {
        let target = self.thread_target(&chat).await?;
        self.mutate_api(
            ApiIntent::Start(p::StartRequest {
                command: None,
                target: Some(target),
            }),
            chat.chat_id,
            None,
        )
        .await
    }
    pub(super) async fn api_chat(&self, intent: &super::ChatMutation) -> Result<ApiIntent, String> {
        use super::ChatMutation as C;
        if let C::Send(request) = intent {
            return self.api_send(request).await;
        }
        let chat = intent.chat().ok_or("Thread identity is missing.")?;
        let target = Some(self.thread_target(chat).await?);
        let run = |run: &str| {
            run.parse::<u64>()
                .map_err(|_| "Invalid run identity.".to_owned())
        };
        let item = |item: &Option<old::ItemKey>| {
            item.map(|v| p::ItemKey {
                history_epoch: v.history_epoch,
                item_id: v.item_id,
            })
            .ok_or("History item is missing.".to_owned())
        };
        let summary = self
            .cached_chat(&chat.chat_id)
            .and_then(|c| c.summary)
            .or_else(|| {
                self.snapshot()
                    .catalog
                    .and_then(|c| c.chats.into_iter().find(|s| s.chat.as_ref() == Some(chat)))
            });
        let context = summary.as_ref().map(|s| old::SendContext {
            agent_id: s.agent_id.clone(),
            profile_id: s.profile_id.clone(),
            workspace: s.workspace.clone(),
            history_epoch: s.history_epoch,
        });
        Ok(match intent {
            C::Interrupt(v) => ApiIntent::Interrupt(p::InterruptRequest {
                command: None,
                target,
                run_id: run(v.run_id.as_deref().ok_or("Run is missing.")?)?,
            }),
            C::Restart(v) => ApiIntent::Restart(p::RestartRequest {
                command: None,
                target,
                run_id: run(v.run_id.as_deref().ok_or("Run is missing.")?)?,
            }),
            C::Steer(v) => ApiIntent::Steer(p::SteerRequest {
                command: None,
                target,
                run_id: run(&v.run_id)?,
                message_id: v.message_id.clone(),
                draft: v
                    .draft
                    .clone()
                    .map(|d| compat::draft(d, chat, context.as_ref()))
                    .transpose()?,
            }),
            C::Respond(v) => {
                let response = match v.response.clone().ok_or("Decision answer is missing.")? {
                    old::respond_decision_request::Response::OptionId(id) => {
                        p::respond_decision_request::Response::OptionId(id)
                    }
                    old::respond_decision_request::Response::Native(n) => {
                        p::respond_decision_request::Response::Native(p::NativeDecision {
                            action: n.action,
                            answers: n.answers,
                            note: n.note,
                            accept_edits: n.accept_edits,
                        })
                    }
                };
                ApiIntent::Respond(p::RespondDecisionRequest {
                    command: None,
                    target,
                    run_id: run(&v.run_id)?,
                    decision_id: run(&v.decision_id)?,
                    response: Some(response),
                })
            }
            C::Option(v) => ApiIntent::Option(p::SetAgentOptionRequest {
                command: None,
                target,
                option_id: v.option_id.clone(),
                value_id: v.value_id.clone(),
            }),
            C::Agent(v) => ApiIntent::Agent(p::SetThreadAgentRequest {
                command: None,
                target,
                agent_id: v.agent_id.clone(),
                profile_id: v.profile_id.clone(),
                model: v.model.clone(),
            }),
            C::Goal(v) => match v.action.map(old::GoalAction::try_from) {
                Some(Ok(old::GoalAction::PauseGoal)) if v.objective.is_none() => {
                    ApiIntent::PauseGoal(p::PauseGoalRequest {
                        command: None,
                        target,
                    })
                }
                Some(Ok(old::GoalAction::ResumeGoal)) if v.objective.is_none() => {
                    ApiIntent::ResumeGoal(p::ResumeGoalRequest {
                        command: None,
                        target,
                    })
                }
                Some(Ok(old::GoalAction::ClearGoal)) if v.objective.is_none() => {
                    ApiIntent::ClearGoal(p::ClearGoalRequest {
                        command: None,
                        target,
                    })
                }
                None | Some(Ok(old::GoalAction::Unspecified)) => match &v.objective {
                    Some(objective) => ApiIntent::Goal(p::SetGoalRequest {
                        command: None,
                        target,
                        objective: objective.clone(),
                    }),
                    None => ApiIntent::ClearGoal(p::ClearGoalRequest {
                        command: None,
                        target,
                    }),
                },
                _ => return Err("Invalid goal action.".into()),
            },
            C::Background(v) => ApiIntent::Background(p::StopBackgroundRequest {
                command: None,
                target,
                run_id: run(&v.run_id)?,
                call_id: v.call_id.clone(),
            }),
            C::Edit(v) => ApiIntent::MessageEdit(p::EditMessageRequest {
                command: None,
                target,
                run_id: run(&v.run_id)?,
                item: Some(item(&v.item)?),
                draft: Some(compat::draft(
                    v.draft.clone().ok_or("Edit draft is missing.")?,
                    chat,
                    context.as_ref(),
                )?),
                restore_files: v.restore_files,
            }),
            C::RunShell(v) => ApiIntent::Shell(p::RunShellRequest {
                command: None,
                target,
                run_id: run(&v.run_id)?,
                command_line: v.command_line.clone(),
            }),
            C::StopShell(v) => ApiIntent::StopShell(p::StopShellRequest {
                command: None,
                target,
                run_id: run(&v.run_id)?,
                item: Some(item(&v.item)?),
            }),
            C::Fork(v) => {
                let source = summary
                    .as_ref()
                    .and_then(|s| s.workspace.as_ref())
                    .ok_or("Fork requires a project thread.")?;
                if v.destination.as_ref() != Some(source) {
                    return Err("Fork is supported only in the source project and worktree. Choose that location.".into());
                }
                ApiIntent::Fork(p::ForkThreadRequest {
                    command: None,
                    target,
                    history_epoch: v.history_epoch,
                    item_id: v.item_id,
                    replacement: v
                        .replacement
                        .clone()
                        .map(|d| compat::draft(d, chat, context.as_ref()))
                        .transpose()?,
                })
            }
            C::Send(_) => unreachable!(),
        })
    }
    pub async fn list_threads(&self) -> Result<Vec<p::ThreadSummary>, String> {
        let mut threads = Vec::new();
        let mut page_token = None;
        let mut bytes = 0;
        loop {
            let response = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
                .list_threads(p::ListThreadsRequest {
                    environment: Some(p::EnvironmentRef {
                        host_id: self.id().as_str().into(),
                    }),
                    catalog_id: None,
                    page_token: page_token.clone(),
                    limit: 100,
                })
                .await
                .map_err(rpc_error)?
                .into_inner();
            bytes += response.encoded_len();
            if bytes > super::catalog::MAX_CATALOG_BYTES {
                return Err("Thread catalog exceeds the client budget.".into());
            }
            if response
                .next_page_token
                .as_ref()
                .zip(page_token.as_ref())
                .is_some_and(|(next, previous)| next <= previous)
            {
                return Err("Thread catalog pagination did not advance.".into());
            }
            threads.extend(response.threads);
            let Some(next) = response.next_page_token else {
                return Ok(threads);
            };
            page_token = Some(next);
        }
    }
    pub(super) async fn retry_api_outbox(
        &self,
        entry: &super::store::OutboxEntry,
    ) -> Result<old::CommandReceipt, String> {
        let bytes = entry
            .api_request
            .as_ref()
            .ok_or("Original typed send is missing; retain the draft without replay.")?;
        let mut intent = ApiIntent::decode("send", bytes)?;
        let meta = intent
            .meta()
            .as_ref()
            .ok_or("Original command identity is missing.")?;
        if meta.command_id != entry.id || self.snapshot().record.epoch != Some(meta.command_epoch) {
            return Err(
                "Saved send identity or epoch differs; intent retained without replay.".into(),
            );
        }
        let request = old::SendMessageRequest::decode(entry.request.as_slice())
            .map_err(|_| "Invalid retained send.")?;
        let original = request
            .command
            .as_ref()
            .ok_or("Retained command identity is missing.")?;
        if original.host_id != self.id().as_str()
            || original.command_id != meta.command_id
            || original.command_epoch != meta.command_epoch
        {
            return Err("Retained send identity mismatch.".into());
        }
        intent
            .dispatch(
                self.channel()?,
                self.id().as_str(),
                request.chat.map(|c| c.chat_id).unwrap_or_default(),
            )
            .await
    }
    /// Discovery returns the stored UUID and epoch. A legacy key is never parsed as UUIDv7.
    pub async fn thread_target(&self, chat: &old::ChatRef) -> Result<p::ThreadTarget, String> {
        if chat.host_id != self.id().as_str() {
            return Err("Thread catalog target differs from this host.".into());
        }
        let mut request = tonic::Request::new(p::ListThreadsRequest {
            environment: Some(p::EnvironmentRef {
                host_id: chat.host_id.clone(),
            }),
            catalog_id: Some(chat.chat_id.clone()),
            page_token: None,
            limit: 1,
        });
        request.set_timeout(std::time::Duration::from_secs(15));
        let response = p::catalog_service_client::CatalogServiceClient::new(self.channel()?)
            .list_threads(request)
            .await
            .map_err(rpc_error)?
            .into_inner();
        let mut matching = response
            .threads
            .into_iter()
            .filter(|t| t.catalog_id == chat.chat_id);
        let summary = matching
            .next()
            .ok_or("Thread is no longer available on its original host.")?;
        if matching.next().is_some() {
            return Err("Thread discovery is ambiguous.".into());
        }
        let target = summary
            .target
            .ok_or("Thread discovery omitted its address.")?;
        let thread = target
            .thread
            .as_ref()
            .ok_or("Thread identity is missing.")?;
        if thread.host_id != chat.host_id
            || crate::thread::ThreadId::parse(&thread.thread_id).is_none()
        {
            return Err("Invalid discovered thread identity.".into());
        }
        Ok(target)
    }
    pub(super) async fn api_send(
        &self,
        request: &old::SendMessageRequest,
    ) -> Result<ApiIntent, String> {
        let chat = request.chat.as_ref().ok_or("Thread is missing.")?;
        let context = request
            .expected_context
            .as_ref()
            .ok_or("Message context is missing.")?;
        Ok(ApiIntent::Send(p::SendMessageRequest {
            command: request.command.clone().map(compat::command),
            target: Some(self.thread_target(chat).await?),
            message_id: request.message_id.clone(),
            draft: Some(compat::draft(
                request.draft.clone().ok_or("Message draft is missing.")?,
                chat,
                Some(context),
            )?),
            expected_context: Some(p::SendContext {
                account_id: context.agent_id.clone(),
                profile_id: context.profile_id.clone(),
                history_epoch: context.history_epoch,
            }),
        }))
    }
    pub(super) async fn mutate_api(
        &self,
        mut intent: ApiIntent,
        catalog_id: String,
        revision: Option<u64>,
    ) -> Result<old::CommandReceipt, String> {
        let _gate = self.mutation.lock().await;
        let channel = self.channel()?;
        let meta = self.command(revision)?;
        *intent.meta() = Some(compat::command(meta.clone()));
        let (kind, bytes) = intent.encode();
        if bytes.len() > crate::host::CONTROL_BYTES {
            return Err("Operation exceeds the control limit; intent retained.".into());
        }
        self.update_record(|record| {
            if record.mutations.iter().any(|e| e.receipt.as_ref().and_then(|b| old::CommandReceipt::decode(b.as_slice()).ok()).is_none_or(|r| !r.resolved_by_user && !terminal_state(r.state))) {
                return Err("An earlier operation is unresolved. Review its result before another mutation.".into());
            }
            // Every journaled operation is resolved here; only the new one remains.
            record.mutations.clear();
            record.mutations.push(MutationRecord { id: meta.command_id.clone(), epoch: meta.command_epoch, receipt: None, chat_intent: None, api_intent: Some((kind.into(), bytes)) });
            Ok(())
        })?;
        let started = std::time::Instant::now();
        let response = intent
            .dispatch(channel, self.id().as_str(), catalog_id.clone())
            .await;
        crate::remote::metrics::record(
            self.id().as_str(),
            &crate::remote::metrics::resource_id(&catalog_id),
            Some(&meta.command_id),
            0,
            crate::remote::metrics::Stage::ClientRoundTrip,
            started,
            if response.is_ok() {
                crate::remote::metrics::Outcome::Ok
            } else {
                crate::remote::metrics::Outcome::DeliveryUnknown
            },
        );
        let receipt = response?;
        self.store_api_receipt(&meta.command_id, &receipt)?;
        check_receipt(receipt)
    }
    pub(super) fn store_api_receipt(
        &self,
        id: &str,
        receipt: &old::CommandReceipt,
    ) -> Result<(), String> {
        self.update_record(|record| {
            let entry = record
                .mutations
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or("Operation journal disappeared.")?;
            entry.receipt = Some(receipt.encode_to_vec());
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub async fn update_preferences(
        &self,
        request: old::UpdatePreferencesRequest,
        revision: u64,
    ) -> Result<old::CommandReceipt, String> {
        self.mutate_api(
            ApiIntent::Preferences(compat::preferences(request, self.id().as_str())),
            "private".into(),
            Some(revision),
        )
        .await
    }
    pub async fn rename_thread(
        &self,
        chat: old::ChatRef,
        title: String,
        revision: Option<u64>,
    ) -> Result<old::CommandReceipt, String> {
        let target = self.thread_target(&chat).await?;
        self.mutate_api(
            ApiIntent::Rename(p::RenameThreadRequest {
                command: None,
                target: Some(target),
                title,
            }),
            chat.chat_id,
            revision,
        )
        .await
    }
    pub async fn change_queue(
        &self,
        chat: old::ChatRef,
        revision: u64,
        change: old::change_queue_request::Change,
        context: Option<old::SendContext>,
    ) -> Result<old::CommandReceipt, String> {
        let target = Some(self.thread_target(&chat).await?);
        use old::change_queue_request::Change as C;
        let intent = match change {
            C::Edit(v) => ApiIntent::Edit(p::EditQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
                draft: Some(compat::draft(
                    v.draft.ok_or("Draft is missing.")?,
                    &chat,
                    context.as_ref(),
                )?),
            }),
            C::Move(v) => ApiIntent::Move(p::MoveQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
                before_message_id: v.before_message_id,
            }),
            C::Remove(v) => ApiIntent::Remove(p::RemoveQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
            }),
            C::Pause(v) => ApiIntent::Pause(p::PauseQueueRequest {
                command: None,
                target,
                queue_revision: revision,
                paused: v.paused,
            }),
            C::Resolve(v) => ApiIntent::Resolve(p::ResolveUnknownQueueMessageRequest {
                command: None,
                target,
                queue_revision: revision,
                message_id: v.message_id,
                resolution: v.resolution,
                new_message_id: v.new_message_id,
            }),
        };
        // Queue CAS is carried by queue_revision. expected_revision refers to
        // thread metadata and must not be filled with the unrelated queue value.
        self.mutate_api(intent, chat.chat_id, None).await
    }
    pub(super) async fn retry_api(
        &self,
        entry: &MutationRecord,
        catalog_id: String,
    ) -> Result<old::CommandReceipt, String> {
        if self.snapshot().record.epoch != Some(entry.epoch) {
            return Err("Command epoch retired; intent retained without replay.".into());
        }
        let (kind, bytes) = entry
            .api_intent
            .as_ref()
            .ok_or("Saved typed request is missing.")?;
        let mut intent = ApiIntent::decode(kind, bytes)?;
        let meta = intent
            .meta()
            .as_ref()
            .ok_or("Saved command identity is missing.")?;
        if meta.command_id != entry.id || meta.command_epoch != entry.epoch {
            return Err("Saved API command identity mismatch.".into());
        }
        intent
            .dispatch(self.channel()?, self.id().as_str(), catalog_id)
            .await
    }
}
