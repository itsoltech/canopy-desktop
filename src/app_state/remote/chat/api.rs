//! Typed API adapter for the existing runtime owner and projections.
use super::*;
use canopy_desktop::{
    api::proto as p,
    command_bus::{Control, DraftResources, PreparedCommand},
};
use futures_lite::StreamExt;

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
        let stream = self
            .subscribe(record.clone(), grant.clone(), request.into_inner().since)
            .await?;
        Ok(authorized(self.clone(), record, grant, stream))
    }
    async fn history(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: Request<p::ReadHistoryPageRequest>,
    ) -> Result<RpcStream<p::HistoryPageEvent>, Status> {
        let stream = self
            .history(record.clone(), grant.clone(), request.into_inner())
            .await?;
        Ok(authorized(self.clone(), record, grant, stream))
    }
    async fn search(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: Request<p::SearchHistoryRequest>,
    ) -> Result<p::HistoryMatches, Status> {
        self.search(record, grant, request.into_inner()).await
    }
    async fn options(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        _request: Request<p::GetAgentOptionsRequest>,
    ) -> Result<p::AgentOptions, Status> {
        let snapshot = self
            .bridge
            .snapshot(record.id().into())
            .await
            .map_err(bridge_status)?;
        self.auth
            .resource(grant.id(), host::READ, record.authority_root())
            .await?;
        let mut offered = capabilities(&snapshot.capabilities);
        for (operation, reason) in [
            ("edit", snapshot.edit_unavailable),
            ("shell", snapshot.shell_unavailable),
        ] {
            let reason = reason.or_else(|| {
                (operation == "shell" && self.auth.scope(&grant, host::TERMINAL).is_err())
                    .then(|| "Terminal access is not enabled for this device.".into())
            });
            offered.push(p::Capability {
                operation: operation.into(),
                supported: reason.is_none(),
                reason: reason.unwrap_or_default(),
            });
        }
        Ok(p::AgentOptions {
            thread: reference(&self.auth, &record),
            options: snapshot
                .options
                .into_iter()
                .map(|o| p::AgentOption {
                    id: o.id,
                    label: o.label,
                    category: o.category,
                    current_value: o.value,
                    choices: o
                        .choices
                        .into_iter()
                        .map(|c| p::OptionChoice {
                            id: c.id,
                            label: c.label,
                            description: c.description,
                        })
                        .collect(),
                    effect: option_effect(o.effect).into(),
                })
                .collect(),
            capabilities: offered,
        })
    }
    async fn preview(
        &self,
        record: ChatRecord,
        grant: DeviceGrant,
        request: Request<p::PreviewEditRequest>,
    ) -> Result<p::EditPreview, Status> {
        let r = request.into_inner();
        let item = required(r.item)?;
        let preview = self
            .bridge
            .edit_preview(
                record.id().into(),
                r.run_id,
                item.history_epoch,
                item.item_id,
            )
            .await
            .map_err(bridge_status)?;
        self.auth
            .resource(grant.id(), host::CHAT, record.authority_root())
            .await?;
        let original = match preview.original {
            Some(draft) => Some(self.editable_draft(&record, draft).await?),
            None => None,
        };
        let files = preview.files;
        Ok(p::EditPreview {
            thread: reference(&self.auth, &record),
            run_id: r.run_id,
            item: Some(item),
            editable: preview.editable.is_ok(),
            reason: preview
                .editable
                .as_ref()
                .err()
                .map(|e| edit_reason(e))
                .unwrap_or_default(),
            first_message: preview.editable == Ok(true),
            original,
            files: Some(p::FileRestorePreview {
                possible: files.possible,
                paths: files.paths,
                insertions: files.insertions,
                deletions: files.deletions,
                reason: files.reason.unwrap_or_default(),
            }),
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
