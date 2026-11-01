//! Temporary translations at the retained read/UI boundary, never an RPC fallback.
use super::proto as p;
use crate::{
    host::{CommandReceipt, CommandState},
    remote::{auth::RpcAuth, proto as old},
};
use serde::Deserialize;
use tonic::Status;

pub fn receipt(
    auth: &RpcAuth,
    value: &CommandReceipt,
    resource: p::command_receipt::Resource,
) -> Result<p::CommandReceipt, Status> {
    let mut receipt = p::CommandReceipt {
        host_id: auth.host_id().into(),
        command_id: value.key().id().into(),
        command_epoch: value.key().epoch(),
        state: match value.state() {
            CommandState::Accepted => p::OperationState::Accepted,
            CommandState::Attempting => p::OperationState::Attempting,
            CommandState::Succeeded => p::OperationState::Succeeded,
            CommandState::Failed => p::OperationState::Failed,
            CommandState::Rejected => p::OperationState::Rejected,
            CommandState::DeliveryUnknown => p::OperationState::DeliveryUnknown,
        } as i32,
        resource: Some(resource),
        error: None,
        result: None,
    };
    if value.state() != CommandState::Succeeded {
        let error = auth.receipt(value).error;
        receipt.error = error
            .map(|e| p::DomainError {
                code: e.code,
                message: e.message,
                retryable: e.retryable,
            })
            .or_else(|| {
                let result: serde_json::Value = serde_json::from_slice(value.result()?).ok()?;
                let reason = result["reason"].as_str()?;
                Some(p::DomainError {
                    code: reason.split(':').next()?.into(),
                    message: reason.into(),
                    retryable: false,
                })
            });
        return Ok(receipt);
    }
    let invalid =
        || Status::internal("Committed command result is invalid; retain the original request.");
    let bytes = value.result().unwrap_or_default();
    use p::command_receipt::Result as R;
    receipt.result = Some(match value.method() {
        "chat.send" => R::Send(p::SendResult {
            message_id: std::str::from_utf8(bytes).map_err(|_| invalid())?.into(),
        }),
        "chat.rename" => {
            let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            R::Rename(p::RenameResult {
                title: v["title"].as_str().ok_or_else(invalid)?.into(),
                revision: v["revision"].as_u64().ok_or_else(invalid)?,
            })
        }
        "settings.preferences" => {
            let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            let values = &v["values"];
            let text = |key: &str| values[key].as_str().map(str::to_owned);
            let chat = &values["chat"];
            R::Preferences(p::PreferencesResult {
                revision: v["revision"].as_u64().ok_or_else(invalid)?,
                values: Some(p::Preferences {
                    reopen_last_workspace: values["reopen_last_workspace"]
                        .as_bool()
                        .ok_or_else(invalid)?,
                    notch_enabled: values["notch_enabled"].as_bool().ok_or_else(invalid)?,
                    new_tab_tool: text("new_tab_tool").ok_or_else(invalid)?,
                    new_worktree_tool: text("new_worktree_tool").ok_or_else(invalid)?,
                    chat_profile: chat["profile"].as_str().map(str::to_owned),
                    chat_model: chat["model"].as_str().map(str::to_owned),
                    chat_effort: stored_key(&chat["effort"], crate::chat::config::Effort::key)
                        .map_err(|_| invalid())?,
                    chat_mode: stored_key(&chat["mode"], crate::chat::config::Mode::key)
                        .map_err(|_| invalid())?,
                }),
            })
        }
        method if method.starts_with("chat.queue.") => R::Queue(p::QueueResult {
            revision: serde_json::from_slice(bytes).map_err(|_| invalid())?,
        }),
        method if crate::command_bus::registry().any(|m| m.name == method) => {
            R::Runtime(p::RuntimeResult {
                created_target: None,
                catalog_id: None,
                item: if method == "chat.shell" {
                    serde_json::from_slice::<serde_json::Value>(bytes)
                        .ok()
                        .and_then(|v| {
                            Some(p::ItemKey {
                                history_epoch: v["history_epoch"].as_u64()?,
                                item_id: v["item_id"].as_u64()?,
                            })
                        })
                } else {
                    None
                },
            })
        }
        _ => return Err(invalid()),
    });
    Ok(receipt)
}

fn stored_key<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
    key: fn(T) -> &'static str,
) -> serde_json::Result<Option<String>> {
    Ok(Option::<T>::deserialize(value)?.map(|value| key(value).into()))
}

pub fn legacy_receipt(value: p::CommandReceipt, catalog_id: String) -> old::CommandReceipt {
    let mut receipt = old::CommandReceipt {
        host_id: value.host_id,
        command_id: value.command_id,
        command_epoch: value.command_epoch,
        state: value.state,
        resource_id: catalog_id,
        error: value.error.map(|e| old::DomainError {
            code: e.code,
            message: e.message,
            retryable: e.retryable,
        }),
        ..Default::default()
    };
    match value.result {
        Some(p::command_receipt::Result::Send(v)) => receipt.message_id = v.message_id,
        Some(p::command_receipt::Result::Queue(v)) => receipt.queue_revision = Some(v.revision),
        Some(p::command_receipt::Result::Rename(v)) => {
            receipt.operation_output =
                Some(serde_json::json!({"title":v.title,"revision":v.revision}).to_string())
        }
        Some(p::command_receipt::Result::Preferences(v)) => {
            let values = v.values.map(|p| serde_json::json!({
                "reopen_last_workspace": p.reopen_last_workspace, "notch_enabled": p.notch_enabled,
                "new_tab_tool": p.new_tab_tool, "new_worktree_tool": p.new_worktree_tool,
                "chat": { "profile":p.chat_profile, "model":p.chat_model,
                    "effort":p.chat_effort.as_deref().and_then(crate::chat::config::Effort::from_key),
                    "mode":p.chat_mode.as_deref().and_then(crate::chat::config::Mode::from_key) }
            }));
            receipt.operation_output =
                Some(serde_json::json!({"values":values,"revision":v.revision}).to_string())
        }
        Some(p::command_receipt::Result::Runtime(v)) => {
            if let Some(id) = v.catalog_id {
                receipt.resource_id = id;
            }
            if let Some(item) = v.item {
                receipt.operation_output = Some(
                    serde_json::json!({"history_epoch":item.history_epoch,"item_id":item.item_id})
                        .to_string(),
                );
            }
        }
        None => (),
    }
    receipt
}

pub fn command(value: old::CommandMeta) -> p::CommandMeta {
    p::CommandMeta {
        command_id: value.command_id,
        command_epoch: value.command_epoch,
        expected_revision: value.expected_revision,
    }
}

pub fn draft(
    value: old::MessageDraft,
    chat: &old::ChatRef,
    context: Option<&old::SendContext>,
) -> Result<p::MessageDraft, String> {
    let attachments = value
        .attachments
        .into_iter()
        .map(|a| {
            use old::attachment::Content as O;
            use p::attachment::Content as N;
            let content = match a.content.ok_or("Attachment content is missing.")? {
                O::Image(v) => N::Image(p::ImageAttachment {
                    host_id: v.host_id,
                    artifact_id: v.artifact_id,
                    sha256: v.sha256,
                    media_type: v.media_type,
                    size: v.size,
                }),
                O::Path(v) => {
                    if let Some(workspace) = &v.workspace {
                        if workspace.host_id != chat.host_id
                            || context.is_none_or(|c| c.workspace.as_ref() != Some(workspace))
                        {
                            return Err(
                                "Attachment checkout differs from the thread's original context."
                                    .into(),
                            );
                        }
                    } else if context.is_some_and(|c| c.workspace.is_some()) {
                        return Err("A project thread cannot attach a private file.".into());
                    }
                    N::File(p::FileAttachment {
                        relative_path: v.relative_path,
                        project_file: v.workspace.is_some(),
                    })
                }
                O::Quote(v) => N::Quote(p::QuoteAttachment { text: v.text }),
                O::Command(v) => N::Command(p::CommandAttachment {
                    name: v.name,
                    skill: v.skill,
                    path: v.path,
                    description: v.description,
                }),
                O::Pull(v) => N::Pull(p::PullAttachment {
                    number: v.number,
                    title: v.title,
                    url: v.url,
                    head: v.head,
                    base: v.base,
                    state: v.state,
                    body: v.body,
                    context_id: v.context_id,
                }),
                O::Shell(v) => N::Shell(p::ShellAttachment {
                    command: v.command,
                    exit_code: v.exit_code,
                    stopped: v.stopped,
                    output: v.output,
                }),
            };
            Ok(p::Attachment {
                label: a.label,
                content: Some(content),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(p::MessageDraft {
        text: value.text,
        attachments,
        context: value.context,
        goal: value.goal,
    })
}

pub fn preferences(
    value: old::UpdatePreferencesRequest,
    host: &str,
) -> p::UpdatePreferencesRequest {
    p::UpdatePreferencesRequest {
        command: value.command.map(command),
        environment: Some(p::EnvironmentRef {
            host_id: host.into(),
        }),
        reopen_last_workspace: value.reopen_last_workspace,
        notch_enabled: value.notch_enabled,
        new_tab_tool: value.new_tab_tool,
        new_worktree_tool: value.new_worktree_tool,
        chat_profile: value.chat_profile,
        chat_model: value.chat_model,
        chat_effort: value.chat_effort,
        chat_mode: value.chat_mode,
    }
}
