//! Typed API adapter for the existing runtime owner and projections.
use super::*;
use canopy_desktop::api::projection::{self as mapping, Location};
use canopy_desktop::{
    api::proto as p,
    command_bus::{Control, DraftResources, PreparedCommand},
};
use futures_lite::StreamExt;

fn location(auth: &RpcAuth, record: &ChatRecord) -> Result<Location, Status> {
    let stored = record
        .thread()
        .ok_or_else(|| Status::internal("Missing thread identity."))?;
    Ok(Location {
        thread: p::ThreadRef {
            host_id: auth.host_id().into(),
            thread_id: stored.thread().thread_id.to_string(),
        },
        catalog: reference(auth, record.id()),
        workspace: workspace(auth, record),
    })
}

pub(super) fn authorized<T: Send + 'static>(
    rpc: ChatsRpc,
    record: ChatRecord,
    grant: DeviceGrant,
    stream: RpcStream<T>,
) -> RpcStream<T> {
    Box::pin(futures_lite::stream::try_unfold(
        (rpc, record, grant, stream),
        |(rpc, record, grant, mut stream)| async move {
            let Some(event) = stream.next().await.transpose()? else {
                return Ok(None);
            };
            rpc.state.accepting()?;
            rpc.auth
                .resource(grant.id(), host::READ, record.authority_root())
                .await?;
            Ok(Some((event, (rpc, record, grant, stream))))
        },
    ))
}

#[tonic::async_trait]
impl canopy_desktop::api::ThreadPublisher for ChatsRpc {
    async fn subscribe(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: Request<p::SubscribeThreadRequest>,
    ) -> Result<RpcStream<p::ThreadEvent>, Status> {
        let location = location(&self.auth, &record)?;
        let stream = self
            .subscribe(
                record.clone(),
                grant.clone(),
                request.into_inner().since.map(mapping::value),
            )
            .await?;
        Ok(authorized(
            self.clone(),
            record,
            grant,
            mapping::thread_stream(location, stream, self.replay.clone()),
        ))
    }
    async fn history(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: Request<p::ReadHistoryPageRequest>,
    ) -> Result<RpcStream<p::HistoryPageEvent>, Status> {
        let location = location(&self.auth, &record)?;
        let r = request.into_inner();
        let stream = self
            .history(
                record.clone(),
                grant.clone(),
                proto::HistoryPageRequest {
                    chat: Some(location.catalog.clone()),
                    page_token: r.page_token,
                    limit: r.limit,
                    around: r.around.map(mapping::value),
                    before: r.before.map(mapping::value),
                },
            )
            .await?;
        Ok(authorized(
            self.clone(),
            record,
            grant,
            mapping::history_stream(location, stream, self.replay.clone()),
        ))
    }
    async fn search(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: Request<p::SearchHistoryRequest>,
    ) -> Result<p::HistoryMatches, Status> {
        let location = location(&self.auth, &record)?;
        let r = request.into_inner();
        let result = self
            .search(
                record,
                grant,
                proto::SearchHistoryRequest {
                    chat: Some(location.catalog),
                    text: r.query,
                    page_token: r.page_token,
                },
            )
            .await?;
        Ok(p::HistoryMatches {
            matches: result
                .matches
                .into_iter()
                .map(|m| p::HistoryMatch {
                    item: m.key.map(mapping::value),
                    excerpt: m.excerpt,
                })
                .collect(),
            next_page_token: result.next_page_token,
            incomplete: result.incomplete,
        })
    }
    async fn options(
        &self,
        record: ChatRecord,
        _grant: DeviceGrant,
        request: Request<p::GetAgentOptionsRequest>,
    ) -> Result<p::AgentOptions, Status> {
        let location = location(&self.auth, &record)?;
        let (metadata, extensions, _) = request.into_parts();
        let request = Request::from_parts(
            metadata,
            extensions,
            proto::ChatRequest {
                chat: Some(location.catalog),
            },
        );
        let result =
            <Self as proto::chat_service_server::ChatService>::get_agent_options(self, request)
                .await?
                .into_inner();
        Ok(p::AgentOptions {
            thread: Some(location.thread),
            options: result.options.into_iter().map(mapping::value).collect(),
            capabilities: result
                .capabilities
                .into_iter()
                .map(mapping::value)
                .collect(),
        })
    }
    async fn preview(
        &self,
        record: ChatRecord,
        _grant: DeviceGrant,
        request: Request<p::PreviewEditRequest>,
    ) -> Result<p::EditPreview, Status> {
        let location = location(&self.auth, &record)?;
        let (metadata, extensions, r) = request.into_parts();
        let request = Request::from_parts(
            metadata,
            extensions,
            proto::PreviewEditRequest {
                chat: Some(location.catalog.clone()),
                run_id: r.run_id.to_string(),
                item: r.item.map(mapping::value),
            },
        );
        let result = <Self as proto::chat_service_server::ChatService>::preview_edit(self, request)
            .await?
            .into_inner();
        Ok(p::EditPreview {
            thread: Some(location.thread.clone()),
            run_id: r.run_id,
            item: result.item.map(mapping::value),
            editable: result.editable,
            reason: result.reason,
            first_message: result.first_message,
            original: result.original.map(|d| location.draft(d)).transpose()?,
            files: result.files.map(mapping::value),
        })
    }
    async fn committed(&self, receipt: &host::CommandReceipt) {
        let client = self.auth.client();
        if let Ok(Some(record)) = client.load_runtime_chat(receipt.target().into()).await
            && let Ok(queue) = client.load_runtime_queue(receipt.target().into()).await
        {
            let _ = self.bridge.bus_committed(record, queue).await;
        }
    }
    async fn control(
        &self,
        prepared: PreparedCommand,
        resources: DraftResources,
    ) -> Result<host::CommandReceipt, Status> {
        let record = prepared
            .record()
            .cloned()
            .ok_or_else(|| Status::not_found("Thread is unavailable."))?;
        let draft = prepared.runtime_draft(&resources).map_err(storage_status)?;
        let input = prepared.runtime_input().clone();
        let key = RemoteKey::admitted(input.clone());
        let control = prepared
            .control()
            .cloned()
            .ok_or_else(|| Status::invalid_argument("Invalid runtime command."))?;
        let command = match control {
            Control::Start => ChatCommand::Start,
            Control::Interrupt { run } => ChatCommand::Interrupt { run },
            Control::Restart { run } => ChatCommand::Restart { run },
            Control::Steer { run, message, .. } => {
                if let Some(draft) = draft {
                    return self
                        .bridge
                        .steer(record.id().into(), input, message, draft, run)
                        .await
                        .map_err(bridge_status);
                }
                ChatCommand::Steer { run, message }
            }
            Control::Respond { run, item, answer } => {
                use canopy_desktop::command_bus::DecisionAnswer as A;
                let answer = match answer {
                    A::Allow => DecisionAnswer::Allow,
                    A::AllowForThread => DecisionAnswer::AllowForChat,
                    A::Deny(note) => DecisionAnswer::Deny(note),
                    A::Answer(answers) => DecisionAnswer::Answer(answers),
                    A::ApprovePlan { accept_edits } => DecisionAnswer::ApprovePlan { accept_edits },
                    A::KeepPlanning(note) => DecisionAnswer::KeepPlanning(note),
                    A::Option(id) => DecisionAnswer::Option(Some(id)),
                };
                ChatCommand::Respond { run, item, answer }
            }
            Control::SetOption { id, value } => ChatCommand::SetOption { id, value },
            Control::SetAgent {
                tool,
                profile,
                model,
            } => ChatCommand::SetAgent {
                tool,
                profile,
                model,
            },
            Control::PauseGoal => ChatCommand::PauseGoal,
            Control::ResumeGoal => ChatCommand::ResumeGoal,
            Control::ClearGoal => ChatCommand::ClearGoal,
            Control::StopBackground { run, call } => ChatCommand::StopBackground { run, call },
            Control::Edit {
                run,
                history_epoch,
                item,
                restore_files,
                ..
            } => ChatCommand::Edit {
                run,
                history_epoch,
                item,
                restore_files,
                draft: draft.ok_or_else(|| Status::invalid_argument("Edit draft is missing."))?,
            },
            Control::RunShell { run, command } => ChatCommand::RunShell { run, command },
            Control::StopShell {
                run,
                history_epoch,
                item,
            } => ChatCommand::StopShell {
                run,
                history_epoch,
                item,
            },
            Control::SetGoal { objective } => {
                let message = resource_id(input.key(), "goal-message");
                let draft = QueueDraft::from(Draft {
                    text: objective,
                    attachments: vec![],
                    context: None,
                    goal: true,
                });
                return self
                    .bridge
                    .send(record.id().into(), input, message, draft)
                    .await
                    .map_err(bridge_status);
            }
            Control::Fork {
                history_epoch,
                item,
                ..
            } => {
                let workspace = record.workspace().ok_or_else(|| {
                    Status::failed_precondition("Fork requires a project thread.")
                })?;
                let workspace = serde_json::from_str(workspace)
                    .map_err(|_| Status::internal("Invalid thread project metadata."))?;
                let replacement = draft
                    .map(|draft| {
                        let message = resource_id(input.key(), "fork-message");
                        let send = host::CommandKey::remote(
                            input.key().device().into(),
                            input.key().epoch(),
                            message.clone(),
                        )
                        .ok_or_else(|| Status::invalid_argument("Invalid replacement identity."))?;
                        Ok::<_, Status>(ForkReplacement {
                            key: send,
                            message,
                            draft,
                            source_folder: None,
                        })
                    })
                    .transpose()?;
                let fingerprint = prepared.fingerprint();
                let fork = crate::app_state::chats::ForkChat {
                    source: record.id().into(),
                    history_epoch,
                    item,
                    chat: resource_id(input.key(), "fork"),
                    workspace: Some(workspace),
                    replacement_fingerprint: replacement
                        .as_ref()
                        .map(|_| ForkReplacement::fingerprint(fingerprint)),
                };
                return self
                    .bridge
                    .fork(key, fork, replacement)
                    .await
                    .map_err(bridge_status);
            }
        };
        self.bridge
            .command(record.id().into(), key, command)
            .await
            .map_err(bridge_status)
    }
}
