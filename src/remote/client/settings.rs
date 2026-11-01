//! Exact configuration retries retain secret-bearing bytes in the existing vault.
use super::{HostClient, api_commands::ApiIntent};
use crate::api::proto as p;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

pub enum SettingsMutation {
    SaveAgent(p::SaveAgentDefinitionRequest),
    RemoveAgent(p::RemoveAgentDefinitionRequest),
    SaveProfile(Box<p::SaveProfileRequest>),
    RemoveProfile(p::RemoveProfileRequest),
    SaveIntegration(p::SaveIntegrationRequest),
    RemoveIntegration(p::RemoveIntegrationRequest),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedRequest {
    parts: Vec<String>,
}
fn saved(bytes: &[u8]) -> Result<SavedRequest, String> {
    let request: SavedRequest = serde_json::from_slice(bytes)
        .map_err(|_| "Invalid settings journal; retain it for review.")?;
    if request.parts.is_empty()
        || request.parts.len() > crate::host::CONTROL_BYTES.div_ceil(1920)
        || request
            .parts
            .iter()
            .any(|id| uuid::Uuid::parse_str(id).is_err())
    {
        return Err("Invalid settings journal; retain it for review.".into());
    }
    Ok(request)
}
impl HostClient {
    pub async fn update_settings(
        &self,
        request: SettingsMutation,
        revision: u64,
    ) -> Result<p::CommandReceipt, String> {
        let environment = Some(p::EnvironmentRef {
            host_id: self.id().as_str().into(),
        });
        let intent = match request {
            SettingsMutation::SaveAgent(mut r) => {
                r.environment = environment;
                ApiIntent::SaveAgent(r)
            }
            SettingsMutation::RemoveAgent(mut r) => {
                r.environment = environment;
                ApiIntent::RemoveAgent(r)
            }
            SettingsMutation::SaveProfile(mut r) => {
                r.environment = environment;
                ApiIntent::SaveProfile(r)
            }
            SettingsMutation::RemoveProfile(mut r) => {
                r.environment = environment;
                ApiIntent::RemoveProfile(r)
            }
            SettingsMutation::SaveIntegration(mut r) => {
                r.environment = environment;
                ApiIntent::SaveIntegration(r)
            }
            SettingsMutation::RemoveIntegration(mut r) => {
                r.environment = environment;
                ApiIntent::RemoveIntegration(r)
            }
        };
        self.mutate_api(intent, "private".into(), Some(revision))
            .await
    }
    pub(super) fn save_settings_intent(
        &self,
        kind: &str,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        if !kind.starts_with("settings.") {
            return Ok(bytes);
        }
        let mut request = SavedRequest { parts: Vec::new() };
        for chunk in bytes.chunks(1920) {
            let id = uuid::Uuid::new_v4().to_string();
            request.parts.push(id.clone());
            if self.vault.save(&id, &STANDARD.encode(chunk)).is_err() {
                for id in &request.parts {
                    let _ = self.vault.remove(id);
                }
                return Err(
                    "Unlock the credential store before saving settings. Nothing was sent.".into(),
                );
            }
        }
        serde_json::to_vec(&request).map_err(|_| "Settings journal cannot be encoded.".into())
    }
    pub(super) fn load_settings_intent(
        &self,
        kind: &str,
        bytes: &[u8],
    ) -> Result<ApiIntent, String> {
        if !kind.starts_with("settings.") {
            return ApiIntent::decode(kind, bytes);
        }
        let request = saved(bytes)?;
        let mut value = Vec::new();
        for id in request.parts {
            let part = self.vault.load(&id).map_err(
                |_| "Unlock the credential store to reconcile settings. Nothing was sent.",
            )?;
            if part.len() > 2560 {
                return Err("Invalid saved settings; nothing was sent.".into());
            }
            value.extend(
                STANDARD
                    .decode(part)
                    .map_err(|_| "Invalid saved settings; nothing was sent.")?,
            );
            if value.len() > crate::host::CONTROL_BYTES {
                return Err("Saved settings exceed the control limit.".into());
            }
        }
        ApiIntent::decode(kind, &value)
    }
    pub(super) fn remove_settings_intent(&self, kind: &str, bytes: &[u8]) {
        if kind.starts_with("settings.")
            && let Ok(request) = saved(bytes)
        {
            for id in request.parts {
                let _ = self.vault.remove(&id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::client::{ClientRegistry, store::CredentialStore};
    use prost::Message;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use tonic::{Request, Response, Status};

    #[derive(Default)]
    struct Vault {
        values: Mutex<std::collections::BTreeMap<String, String>>,
        locked: AtomicBool,
    }
    impl CredentialStore for Vault {
        fn load(&self, id: &str) -> Result<String, String> {
            if self.locked.load(Ordering::SeqCst) {
                return Err("locked".into());
            }
            self.values
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or("missing".into())
        }
        fn save(&self, id: &str, value: &str) -> Result<(), String> {
            if self.locked.load(Ordering::SeqCst) {
                return Err("locked".into());
            }
            assert!(value.len() <= 2560);
            assert!(
                self.values
                    .lock()
                    .unwrap()
                    .insert(id.into(), value.into())
                    .is_none()
            );
            Ok(())
        }
        fn remove(&self, id: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(id);
            Ok(())
        }
    }
    #[derive(Clone, Default)]
    struct Endpoint(Arc<Mutex<Vec<p::SaveProfileRequest>>>);
    #[tonic::async_trait]
    impl p::settings_service_server::SettingsService for Endpoint {
        async fn save_profile(
            &self,
            r: Request<p::SaveProfileRequest>,
        ) -> Result<Response<p::CommandReceipt>, Status> {
            let r = r.into_inner();
            let mut calls = self.0.lock().unwrap();
            calls.push(r.clone());
            if calls.len() == 1 {
                return Err(Status::unavailable("response lost"));
            }
            let meta = r.command.unwrap();
            Ok(Response::new(p::CommandReceipt {
                host_id: r.environment.as_ref().unwrap().host_id.clone(),
                command_id: meta.command_id,
                command_epoch: meta.command_epoch,
                resource: r.environment.map(p::command_receipt::Resource::Environment),
                state: p::OperationState::Succeeded as i32,
                resolved_by_user: false,
                error: None,
                result: None,
            }))
        }
        async fn get_settings(
            &self,
            _: Request<p::GetSettingsRequest>,
        ) -> Result<Response<p::HostSettings>, Status> {
            Err(Status::unimplemented("unexpected method"))
        }
        async fn update_preferences(
            &self,
            _: Request<p::UpdatePreferencesRequest>,
        ) -> Result<Response<p::CommandReceipt>, Status> {
            Err(Status::unimplemented("unexpected method"))
        }
        async fn save_agent_definition(
            &self,
            _: Request<p::SaveAgentDefinitionRequest>,
        ) -> Result<Response<p::CommandReceipt>, Status> {
            Err(Status::unimplemented("unexpected method"))
        }
        async fn remove_agent_definition(
            &self,
            _: Request<p::RemoveAgentDefinitionRequest>,
        ) -> Result<Response<p::CommandReceipt>, Status> {
            Err(Status::unimplemented("unexpected method"))
        }
        async fn remove_profile(
            &self,
            _: Request<p::RemoveProfileRequest>,
        ) -> Result<Response<p::CommandReceipt>, Status> {
            Err(Status::unimplemented("unexpected method"))
        }
        async fn save_integration(
            &self,
            _: Request<p::SaveIntegrationRequest>,
        ) -> Result<Response<p::CommandReceipt>, Status> {
            Err(Status::unimplemented("unexpected method"))
        }
        async fn remove_integration(
            &self,
            _: Request<p::RemoveIntegrationRequest>,
        ) -> Result<Response<p::CommandReceipt>, Status> {
            Err(Status::unimplemented("unexpected method"))
        }
    }
    #[tokio::test]
    async fn lost_settings_response_retries_exact_rpc_after_restart_without_plaintext_journal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("client.json");
        let vault = Arc::new(Vault::default());
        let registry = ClientRegistry::open(path.clone(), vault.clone()).unwrap();
        let host = registry.insert(super::super::tests::record()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let recorder = Endpoint::default();
        let service = recorder.clone();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(p::settings_service_server::SettingsServiceServer::new(
                    service,
                ))
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    async {
                        let _ = stopped.await;
                    },
                )
                .await
                .unwrap();
        });
        let channel = tonic::transport::Endpoint::from_shared(endpoint)
            .unwrap()
            .connect()
            .await
            .unwrap();
        host.state.lock().unwrap().channel = Some(channel.clone());
        let secret = "private credential 🐈".repeat(300);
        let request = || {
            SettingsMutation::SaveProfile(Box::new(p::SaveProfileRequest {
                profile: Some(p::Profile {
                    id: "profile".into(),
                    agent_id: "agent".into(),
                    name: "Profile".into(),
                    ..Default::default()
                }),
                credential: Some(p::SecretUpdate {
                    action: Some(p::secret_update::Action::Set(secret.as_bytes().to_vec())),
                }),
                ..Default::default()
            }))
        };
        vault.locked.store(true, Ordering::SeqCst);
        assert!(
            host.update_settings(request(), 42)
                .await
                .unwrap_err()
                .contains("credential store")
        );
        assert!(host.snapshot().record.mutations.is_empty());
        assert!(recorder.0.lock().unwrap().is_empty());
        vault.locked.store(false, Ordering::SeqCst);
        assert!(host.update_settings(request(), 42).await.is_err());
        let journal = std::fs::read_to_string(&path).unwrap();
        assert!(!journal.contains("private credential"));
        let saved = host.snapshot().record.mutations[0].clone();
        let (kind, bytes) = saved.api_intent.as_ref().unwrap();
        assert!(!String::from_utf8_lossy(bytes).contains("private credential"));
        assert!(vault.values.lock().unwrap().len() > 1);
        let id = host.id().clone();
        drop(host);
        drop(registry);
        let restored = ClientRegistry::open(path, vault.clone())
            .unwrap()
            .get(&id)
            .unwrap();
        restored.state.lock().unwrap().channel = Some(channel);
        assert!(
            restored.reconcile().await.is_err(),
            "a result query cannot resend configuration"
        );
        assert_eq!(recorder.0.lock().unwrap().len(), 1);
        vault.locked.store(true, Ordering::SeqCst);
        assert!(
            restored
                .retry_api(&saved, "private".into())
                .await
                .unwrap_err()
                .contains("credential store")
        );
        assert_eq!(recorder.0.lock().unwrap().len(), 1);
        vault.locked.store(false, Ordering::SeqCst);
        let mut unknown = saved.clone();
        unknown.receipt = Some(
            p::CommandReceipt {
                command_id: saved.id.clone(),
                command_epoch: saved.epoch,
                state: p::OperationState::DeliveryUnknown as i32,
                ..Default::default()
            }
            .encode_to_vec(),
        );
        assert!(
            restored
                .retry_api(&unknown, "private".into())
                .await
                .unwrap_err()
                .contains("cannot be replayed")
        );
        assert_eq!(recorder.0.lock().unwrap().len(), 1);
        restored.retry_api(&saved, "private".into()).await.unwrap();
        {
            let calls = recorder.0.lock().unwrap();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].encode_to_vec(), calls[1].encode_to_vec());
            assert_eq!(calls[1].command.as_ref().unwrap().command_id, saved.id);
            assert_eq!(
                calls[1].command.as_ref().unwrap().expected_revision,
                Some(42)
            );
        }
        restored.remove_settings_intent(kind, bytes);
        assert!(vault.values.lock().unwrap().is_empty());
        stop.send(()).unwrap();
        server.await.unwrap();
    }
}
