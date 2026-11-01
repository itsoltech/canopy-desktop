//! Original RPC and inbox inspection project the same retained outcome.
use super::{proto as p, resources};
use crate::{
    host::{CommandReceipt, CommandState},
    remote::auth::{RpcAuth, storage_status},
    state::workspace::WorkspaceId,
    thread::HostId,
};
use prost::Message;
use serde::Deserialize;
use tonic::Status;

pub fn project_receipt(
    auth: &RpcAuth,
    value: &CommandReceipt,
    resource: impl Into<Option<p::command_receipt::Resource>>,
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
        resource: resource.into(),
        resolved_by_user: value.resolved_by_user(),
        error: None,
        result: None,
    };
    if value.state() != CommandState::Succeeded {
        receipt.error = p::DomainError::decode(value.result().unwrap_or_default())
            .ok()
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
    if (value.method().starts_with("settings.") && value.method() != "settings.preferences")
        || matches!(
            value.method(),
            "host.revoke" | "peer.connect" | "projects.close" | "terminal.close"
        )
    {
        return Ok(receipt);
    }
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

pub async fn committed_receipt(
    auth: &RpcAuth,
    receipt: &CommandReceipt,
    resource: Option<p::command_receipt::Resource>,
) -> Result<p::CommandReceipt, Status> {
    let mut result = match resource {
        Some(resource) => resources::receipt_for_resource(auth, receipt, resource)?,
        None => project_receipt(auth, receipt, None)?,
    };
    if receipt.state() != CommandState::Succeeded {
        return Ok(result);
    }
    let catalog_id = match receipt.method() {
        // Creation can succeed with a runtime warning in result bytes. The
        // committed target remains the authoritative created catalog identity.
        "chat.create" => Some(receipt.target().to_owned()),
        "chat.fork" => Some(
            std::str::from_utf8(receipt.result().unwrap_or_default())
                .map_err(|_| Status::data_loss("Invalid committed fork identity."))?
                .to_owned(),
        ),
        "projects.open" => {
            let workspace: WorkspaceId =
                serde_json::from_slice(receipt.result().unwrap_or_default())
                    .map_err(|_| Status::data_loss("Invalid committed project identity."))?;
            result.result = Some(p::command_receipt::Result::Runtime(p::RuntimeResult {
                created_target: None,
                catalog_id: Some(serde_json::to_string(&workspace).expect("workspace identity")),
                item: None,
            }));
            None
        }
        _ => None,
    };
    if let Some(catalog_id) = catalog_id {
        let thread = auth
            .client()
            .runtime_thread_alias(
                HostId::new(auth.host_id().into())
                    .ok_or_else(|| Status::internal("Invalid host identity."))?,
                catalog_id.clone(),
            )
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::data_loss("Missing committed thread identity."))?;
        result.result = Some(p::command_receipt::Result::Runtime(p::RuntimeResult {
            created_target: Some(p::ThreadTarget {
                thread: Some(p::ThreadRef {
                    host_id: auth.host_id().into(),
                    thread_id: thread.to_string(),
                }),
                executor_epoch: 0,
            }),
            catalog_id: Some(catalog_id),
            item: None,
        }));
        // Alias lookup awaits the writer. Recheck the saved operation's grants
        // afterwards, including roots of resources that have since disappeared.
        auth.client()
            .get_runtime_command(receipt.key().clone(), 0)
            .await
            .map_err(storage_status)?
            .ok_or_else(|| Status::data_loss("Committed command result is missing."))?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        command_bus::{
            CatalogCommand, Command, CommandBus, CommandEnvelope, Principal, ResourceRef,
        },
        host::{self, ChatRecord, DeviceRegistration},
        settings::SettingsClient,
        state::workspace::{PaneKind, PaneMetadata},
        thread::EnvironmentRef,
    };

    #[tokio::test]
    async fn created_result_survives_target_deletion_and_requires_current_authority() {
        let dir = tempfile::tempdir().unwrap();
        let client = SettingsClient::create(dir.path().join("runtime.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let device = uuid::Uuid::new_v4().to_string();
        client
            .register_runtime_device(
                DeviceRegistration::new(
                    device.clone(),
                    "test device".into(),
                    vec![9; 32],
                    host::CHAT,
                    vec!["private".into()],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let grant = client.lookup_runtime_device(vec![9; 32]).await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let prepared = CommandBus::new(client.clone())
            .prepare_catalog(
                CommandEnvelope {
                    command_id: uuid::Uuid::new_v4(),
                    resource: ResourceRef::Environment(EnvironmentRef {
                        host_id: HostId::new(host.id().into()).unwrap(),
                    }),
                    command: Command::Catalog(CatalogCommand::Create {
                        agent: "runtime".into(),
                        profile: "account".into(),
                    }),
                    conditions: Default::default(),
                },
                Principal::authenticated(&grant, 1).unwrap(),
                Some((id.clone(), "private".into())),
            )
            .await
            .unwrap()
            .unwrap();
        let accepted = client
            .accept_runtime_command(prepared.runtime_input().clone(), host::CHAT)
            .await
            .unwrap();
        client
            .transition_runtime_command(
                accepted.key().clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None,
            )
            .await
            .unwrap();
        let created = client
            .save_runtime_chat(
                ChatRecord::new(
                    id.clone(),
                    None,
                    "runtime".into(),
                    PaneMetadata {
                        kind: PaneKind::Chat,
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
        let receipt = client
            .transition_runtime_command(
                accepted.key().clone(),
                CommandState::Attempting,
                CommandState::Succeeded,
                Some(b"Created; runtime startup requires host attention.".to_vec()),
            )
            .await
            .unwrap();
        let auth = RpcAuth::new(host.id().into(), client.clone()).unwrap();
        let resource = Some(p::command_receipt::Resource::Environment(
            p::EnvironmentRef {
                host_id: host.id().into(),
            },
        ));
        let original = committed_receipt(&auth, &receipt, resource.clone())
            .await
            .unwrap();
        let Some(p::command_receipt::Result::Runtime(result)) = &original.result else {
            panic!("missing creation result")
        };
        assert_eq!(result.catalog_id.as_deref(), Some(id.as_str()));
        assert_eq!(
            result
                .created_target
                .as_ref()
                .unwrap()
                .thread
                .as_ref()
                .unwrap()
                .thread_id,
            created.thread_id().unwrap().to_string()
        );
        client.delete_chat_history(id.clone()).await.unwrap();
        assert!(client.load_runtime_chat(id).await.unwrap().is_none());
        assert_eq!(
            committed_receipt(&auth, &receipt, resource.clone())
                .await
                .unwrap(),
            original
        );
        client
            .update_runtime_device_authorization(
                device,
                grant.authorization_revision(),
                host::READ,
                vec!["private".into()],
            )
            .await
            .unwrap();
        assert_eq!(
            committed_receipt(&auth, &receipt, resource)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::PermissionDenied
        );
        client.shutdown().await.unwrap();
    }
}
