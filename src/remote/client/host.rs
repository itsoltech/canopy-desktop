//! Inbox inspection and archival never replay the original effect.
use super::{HostClient, api_commands::ApiIntent, rpc_error};
use crate::api::proto as p;
use prost::Message;

impl HostClient {
    pub async fn revoke_device(&self, device: String) -> Result<p::CommandReceipt, String> {
        self.mutate_api(
            ApiIntent::RevokeDevice(p::RevokeDeviceRequest {
                command: None,
                environment: Some(p::EnvironmentRef {
                    host_id: self.id().as_str().into(),
                }),
                device_id: device,
            }),
            String::new(),
            None,
        )
        .await
    }
    fn lookup_request(&self, meta: &p::CommandMeta) -> Result<p::GetCommandRequest, String> {
        let record = self.snapshot().record;
        let resource = if let Some((kind, bytes)) = record
            .mutations
            .iter()
            .find(|entry| entry.id == meta.command_id && entry.epoch == meta.command_epoch)
            .and_then(|entry| entry.api_intent.as_ref())
        {
            if kind.starts_with("settings.") {
                Some(p::command_receipt::Resource::Environment(
                    p::EnvironmentRef {
                        host_id: self.id().as_str().into(),
                    },
                ))
            } else {
                ApiIntent::decode(kind, bytes)?.resource()
            }
        } else if let Some(bytes) = record
            .outbox
            .iter()
            .filter_map(|entry| entry.api_request.as_ref())
            .find(|bytes| {
                p::SendMessageRequest::decode(bytes.as_slice())
                    .ok()
                    .is_some_and(|request| {
                        request.command.is_some_and(|command| {
                            command.command_id == meta.command_id
                                && command.command_epoch == meta.command_epoch
                        })
                    })
            })
        {
            p::SendMessageRequest::decode(bytes.as_slice())
                .map_err(|_| "Invalid saved request.")?
                .target
                .map(p::command_receipt::Resource::Target)
        } else {
            record
                .uploads
                .iter()
                .flat_map(|upload| {
                    [&upload.api_begin, &upload.api_finish, &upload.api_cancel]
                        .into_iter()
                        .flatten()
                })
                .filter_map(|(kind, bytes)| {
                    let mut intent = ApiIntent::decode(kind, bytes).ok()?;
                    Some((intent.meta().clone()?, intent.resource()?))
                })
                .find(|(command, _)| {
                    command.command_id == meta.command_id
                        && command.command_epoch == meta.command_epoch
                })
                .map(|(_, resource)| resource)
        };
        Ok(p::GetCommandRequest {
            environment: Some(p::EnvironmentRef {
                host_id: self.id().as_str().into(),
            }),
            command_id: meta.command_id.clone(),
            command_epoch: meta.command_epoch,
            resource: resource.map(|resource| p::CommandResource {
                target: Some(match resource {
                    p::command_receipt::Resource::Target(t) => {
                        p::command_resource::Target::Thread(t)
                    }
                    p::command_receipt::Resource::Project(p) => {
                        p::command_resource::Target::Project(p)
                    }
                    p::command_receipt::Resource::Environment(e) => {
                        p::command_resource::Target::Environment(e)
                    }
                }),
            }),
        })
    }
    pub(super) async fn inspect_command(
        &self,
        meta: &p::CommandMeta,
    ) -> Result<p::CommandReceipt, String> {
        let result = self.lookup_command(meta).await?;
        match p::CommandLookupState::try_from(result.state) {
            Ok(p::CommandLookupState::Found) => result
                .receipt
                .ok_or_else(|| "Command result is missing.".into()),
            _ => Err("The original operation is absent or retired. Preserve its intent; reconcile by its original typed RPC.".into()),
        }
    }
    pub(super) async fn lookup_command(
        &self,
        meta: &p::CommandMeta,
    ) -> Result<p::GetCommandResponse, String> {
        let result = p::host_service_client::HostServiceClient::new(self.channel()?)
            .get_command(self.lookup_request(meta)?)
            .await
            .map_err(rpc_error)?
            .into_inner();
        match p::CommandLookupState::try_from(result.state) {
            Ok(p::CommandLookupState::Found) => {
                self.validate_receipt(
                    meta,
                    result
                        .receipt
                        .as_ref()
                        .ok_or("Command result is missing.")?,
                )?;
                Ok(result)
            }
            Ok(p::CommandLookupState::NotFound | p::CommandLookupState::Retired)
                if result.receipt.is_none() =>
            {
                Ok(result)
            }
            _ => Err("Invalid inbox lookup result.".into()),
        }
    }
    pub(super) async fn archive_command(
        &self,
        meta: &p::CommandMeta,
    ) -> Result<p::CommandReceipt, String> {
        let request = self.lookup_request(meta)?;
        p::host_service_client::HostServiceClient::new(self.channel()?)
            .resolve_command(p::ResolveCommandRequest {
                environment: request.environment,
                command_id: request.command_id,
                command_epoch: request.command_epoch,
                resource: request.resource,
            })
            .await
            .map_err(rpc_error)?
            .into_inner()
            .receipt
            .ok_or_else(|| "Command review result is missing.".into())
    }
}
