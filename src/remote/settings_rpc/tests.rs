use super::*;
use crate::{
    host::{DeviceGrant, DeviceRegistration},
    settings::SettingsClient,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
#[derive(Default)]
struct Secrets {
    entries: Mutex<HashMap<String, String>>,
    unavailable: AtomicBool,
    fail_stage: AtomicBool,
}
#[tonic::async_trait]
impl SettingsSecrets for Secrets {
    async fn fingerprint(&self, bytes: &[u8]) -> Result<[u8; 32], Status> {
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(Status::unavailable("MAC key missing"));
        }
        let mut m = Hmac::<Sha256>::new_from_slice(&[37; 32]).unwrap();
        m.update(bytes);
        Ok(m.finalize().into_bytes().into())
    }
    async fn stage(&self, v: &SecretVersion, s: &str) -> Result<(), Status> {
        let mut entries = self.entries.lock().unwrap();
        for (id, part) in v.parts().zip(super::secrets::chunks(s)) {
            if entries.get(id).is_some_and(|old| old != part) {
                return Err(Status::aborted("immutable version changed"));
            }
            entries.insert(id.into(), part.into());
        }
        if self.fail_stage.load(Ordering::SeqCst) {
            return Err(Status::unavailable("simulated lost vault ACK"));
        }
        Ok(())
    }
    async fn read(&self, v: &SecretVersion) -> Result<String, Status> {
        let entries = self.entries.lock().unwrap();
        let mut value = String::new();
        for id in v.parts() {
            value.push_str(entries.get(id).ok_or_else(|| Status::not_found("secret"))?);
        }
        Ok(value)
    }
    async fn remove(&self, v: &SecretVersion) -> Result<(), Status> {
        let mut entries = self.entries.lock().unwrap();
        for id in v.parts() {
            entries.remove(id);
        }
        Ok(())
    }
}
struct Host {
    secrets: Arc<Secrets>,
    values: Mutex<Vec<String>>,
    calls: AtomicUsize,
    unknown: AtomicBool,
    panic_after_apply: AtomicBool,
    pause: AtomicBool,
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[tonic::async_trait]
impl SettingsHost for Host {
    async fn snapshot(&self) -> Result<proto::HostSettings, Status> {
        Ok(proto::HostSettings::default())
    }
    async fn update_preferences(
        &self,
        _: crate::command_bus::CommandEnvelope,
        _: crate::command_bus::Principal,
    ) -> Result<crate::host::CommandReceipt, Status> {
        Err(Status::unimplemented(
            "This fixture only implements credential settings.",
        ))
    }
    async fn save_agent(&self, _: AgentEdit, _: Option<u64>) -> Result<(), Status> {
        Ok(())
    }
    async fn remove_agent(&self, _: String, _: Option<u64>) -> Result<(), Status> {
        Ok(())
    }
    async fn remove_profile(&self, _: String, _: Option<u64>) -> Result<(), Status> {
        Ok(())
    }
    async fn remove_integration(&self, _: String, _: Option<u64>) -> Result<(), Status> {
        Ok(())
    }
    async fn save_integration(&self, _: IntegrationEdit, _: Option<u64>) -> Result<(), Status> {
        Ok(())
    }
    async fn save_profile(&self, e: ProfileEdit, expected: Option<u64>) -> Result<(), Status> {
        if expected.is_some_and(|revision| revision != 42) {
            return Err(Status::aborted("stale revision"));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        if self.pause.load(Ordering::SeqCst) {
            self.release.notified().await;
        }
        if let SecretChange::Set(v) = e.credential() {
            let value = self.secrets.read(v).await?;
            self.values.lock().unwrap().push(value);
        }
        if let Some(settings) = e.settings() {
            for change in [settings.append_system_prompt(), settings.settings_json()] {
                if let SecretChange::Set(v) = change {
                    let value = self.secrets.read(v).await?;
                    self.values.lock().unwrap().push(value);
                }
            }
        }
        for change in e.environment().values() {
            if let SecretChange::Set(v) = change {
                let value = self.secrets.read(v).await?;
                self.values.lock().unwrap().push(value);
            }
        }
        assert!(
            !self.panic_after_apply.load(Ordering::SeqCst),
            "simulated callback panic after effect"
        );
        if self.unknown.load(Ordering::SeqCst) {
            Err(Status::unknown("uncertain"))
        } else {
            Ok(())
        }
    }
}
async fn fixture(dir: &std::path::Path) -> (SettingsRpc, DeviceGrant, Arc<Host>, Arc<Secrets>) {
    let client = SettingsClient::create(dir.join("db")).await.unwrap();
    let host = client.initialize_runtime().await.unwrap();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "test".into(),
                vec![1; 32],
                ADMIN,
                vec!["private".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![1; 32]).await.unwrap();
    let secrets = Arc::new(Secrets::default());
    let owner = Arc::new(Host {
        secrets: secrets.clone(),
        values: Mutex::new(vec![]),
        calls: AtomicUsize::new(0),
        unknown: AtomicBool::new(false),
        panic_after_apply: AtomicBool::new(false),
        pause: AtomicBool::new(false),
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    (
        SettingsRpc::new(
            RpcAuth::new(host.id().into(), client).unwrap(),
            ResourceLocks::default(),
            owner.clone(),
            secrets.clone(),
        ),
        grant,
        owner,
        secrets,
    )
}
fn request(rpc: &SettingsRpc, value: &str) -> proto::SaveProfileRequest {
    proto::SaveProfileRequest {
        command: Some(proto::CommandMeta {
            host_id: rpc.auth.host_id().into(),
            command_id: uuid::Uuid::new_v4().to_string(),
            command_epoch: 1,
            expected_revision: None,
        }),
        profile: Some(proto::Profile {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: "claude".into(),
            name: "Default".into(),
            ..Default::default()
        }),
        credential: Some(proto::SecretUpdate {
            action: Some(proto::secret_update::Action::Set(value.as_bytes().to_vec())),
        }),
        environment: vec![],
        settings: None,
    }
}
#[tokio::test]
async fn secret_retry_rotation_and_key_loss_never_repeat_or_leak_intent() {
    let d = tempfile::tempdir().unwrap();
    let (rpc, g, host, secrets) = fixture(d.path()).await;
    let first = request(&rpc, "weak-secret-123");
    let a = rpc
        .write(g.clone(), writes::Write::profile(first.clone()))
        .await
        .unwrap();
    assert_eq!(a.state, proto::OperationState::Succeeded as i32);
    let mut rotation = first.clone();
    rotation.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    rotation.credential.as_mut().unwrap().action = Some(proto::secret_update::Action::Set(
        b"rotated-secret".to_vec(),
    ));
    rpc.write(g.clone(), writes::Write::profile(rotation))
        .await
        .unwrap();
    assert_eq!(
        rpc.write(g.clone(), writes::Write::profile(first.clone()))
            .await
            .unwrap(),
        a
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 2);
    let mut changed = first.clone();
    changed.credential.as_mut().unwrap().action =
        Some(proto::secret_update::Action::Set(b"changed".to_vec()));
    assert_eq!(
        rpc.write(g.clone(), writes::Write::profile(changed))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    assert!(secrets.entries.lock().unwrap().is_empty());
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    let rows = db
        .prepare("SELECT arguments,fingerprint FROM _canopy_command_inbox")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (args, hash) in rows {
        assert!(!String::from_utf8_lossy(&args).contains("weak-secret"));
        assert_ne!(hash, Sha256::digest(b"weak-secret-123").to_vec());
    }
    secrets.unavailable.store(true, Ordering::SeqCst);
    assert!(
        rpc.write(g.clone(), writes::Write::profile(first.clone()))
            .await
            .is_err()
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 2);
    rpc.auth
        .client()
        .revoke_runtime_device(g.id().into())
        .await
        .unwrap();
    assert_eq!(
        rpc.write(g, writes::Write::profile(first))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
}
#[tokio::test]
async fn request_cancellation_keeps_accepted_secret_and_unknown_retains_version() {
    let d = tempfile::tempdir().unwrap();
    let (rpc, g, host, secrets) = fixture(d.path()).await;
    host.pause.store(true, Ordering::SeqCst);
    host.unknown.store(true, Ordering::SeqCst);
    let initial = request(&rpc, "original");
    let task = tokio::spawn({
        let rpc = rpc.clone();
        let g = g.clone();
        let request = initial.clone();
        async move { rpc.write(g, writes::Write::profile(request)).await }
    });
    host.started.notified().await;
    task.abort();
    assert_eq!(secrets.entries.lock().unwrap().len(), 1);
    host.release.notify_one();
    let receipt = rpc
        .write(g.clone(), writes::Write::profile(initial.clone()))
        .await
        .unwrap();
    assert_eq!(receipt.state, proto::OperationState::DeliveryUnknown as i32);
    rpc.cleanup().await.unwrap();
    assert_eq!(secrets.entries.lock().unwrap().len(), 1);
    assert_eq!(&*host.values.lock().unwrap(), &["original"]);
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    // A newer committed rotation cleans only its own staging version.
    host.pause.store(false, Ordering::SeqCst);
    host.unknown.store(false, Ordering::SeqCst);
    let mut next = initial;
    next.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    next.credential.as_mut().unwrap().action =
        Some(proto::secret_update::Action::Set(b"new".to_vec()));
    rpc.write(g, writes::Write::profile(next)).await.unwrap();
    assert_eq!(
        secrets
            .entries
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["original"]
    );
    host.panic_after_apply.store(true, Ordering::SeqCst);
    let panic_request = request(&rpc, "callback-crash");
    let grant = rpc
        .auth
        .client()
        .lookup_runtime_device(vec![1; 32])
        .await
        .unwrap();
    let unknown = rpc
        .write(grant.clone(), writes::Write::profile(panic_request.clone()))
        .await
        .unwrap();
    assert_eq!(unknown.state, proto::OperationState::DeliveryUnknown as i32);
    rpc.write(grant, writes::Write::profile(panic_request))
        .await
        .unwrap();
    assert_eq!(host.calls.load(Ordering::SeqCst), 3);
    rpc.locks.drain().await.unwrap();
}
#[tokio::test]
async fn admin_without_global_root_and_uncertified_reads_are_rejected() {
    use proto::settings_service_server::SettingsService;
    let d = tempfile::tempdir().unwrap();
    let (rpc, g, host, _) = fixture(d.path()).await;
    let r = request(&rpc, "secret");
    rpc.auth
        .client()
        .update_runtime_device_authorization(
            g.id().into(),
            g.authorization_revision(),
            ADMIN,
            vec!["/allowed".into()],
        )
        .await
        .unwrap();
    assert_eq!(
        rpc.write(g, writes::Write::profile(r))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        rpc.get_settings(Request::new(proto::HostRequest {
            host_id: rpc.auth.host_id().into()
        }))
        .await
        .unwrap_err()
        .code(),
        tonic::Code::Unauthenticated
    );
}

#[tokio::test]
async fn vault_write_before_acceptance_is_recoverable_and_orphans_have_durable_cleanup() {
    let d = tempfile::tempdir().unwrap();
    let (rpc, g, host, secrets) = fixture(d.path()).await;
    secrets.fail_stage.store(true, Ordering::SeqCst);
    let r = request(&rpc, "lost-stage-ack");
    assert!(
        rpc.write(g.clone(), writes::Write::profile(r.clone()))
            .await
            .is_err()
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
    assert_eq!(secrets.entries.lock().unwrap().len(), 1);
    rpc.cleanup().await.unwrap();
    assert_eq!(secrets.entries.lock().unwrap().len(), 1);
    secrets.fail_stage.store(false, Ordering::SeqCst);
    rpc.write(g.clone(), writes::Write::profile(r))
        .await
        .unwrap();
    assert_eq!(&*host.values.lock().unwrap(), &["lost-stage-ack"]);
    assert!(secrets.entries.lock().unwrap().is_empty());
    secrets.fail_stage.store(true, Ordering::SeqCst);
    assert!(
        rpc.write(g, writes::Write::profile(request(&rpc, "orphan")))
            .await
            .is_err()
    );
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    db.execute("UPDATE _canopy_settings_secrets SET created_at=0", [])
        .unwrap();
    rpc.cleanup().await.unwrap();
    assert!(secrets.entries.lock().unwrap().is_empty());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM _canopy_settings_secrets", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn owner_checks_revision_before_effect_and_retry_keeps_original_expectation() {
    let d = tempfile::tempdir().unwrap();
    let (rpc, g, host, _) = fixture(d.path()).await;
    let mut stale = request(&rpc, "secret");
    stale.command.as_mut().unwrap().expected_revision = Some(41);
    let receipt = rpc
        .write(g.clone(), writes::Write::profile(stale.clone()))
        .await
        .unwrap();
    assert_eq!(receipt.state, proto::OperationState::Failed as i32);
    assert_eq!(receipt.error.unwrap().code, "conflict");
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
    stale.command.as_mut().unwrap().expected_revision = Some(42);
    assert_eq!(
        rpc.write(g.clone(), writes::Write::profile(stale.clone()))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    stale.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        rpc.write(g, writes::Write::profile(stale))
            .await
            .unwrap()
            .state,
        proto::OperationState::Succeeded as i32
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn typed_profile_secret_overrides_preserve_unicode_empty_env_and_never_enter_inbox() {
    let d = tempfile::tempdir().unwrap();
    let (rpc, g, host, secrets) = fixture(d.path()).await;
    host.unknown.store(true, Ordering::SeqCst);
    let prompt = "🔐 staged private prompt ".repeat(200);
    let json = serde_json::json!({"private":"secret JSON value 🐈".repeat(1000)}).to_string();
    let set = |s: &str| {
        Some(proto::SecretUpdate {
            action: Some(proto::secret_update::Action::Set(s.as_bytes().to_vec())),
        })
    };
    let mut r = request(&rpc, "unused");
    r.credential = None;
    r.settings = Some(proto::ProfileSettingsUpdate {
        options: Some(proto::ProfileAgentSettings {
            base_url: Some("https://api.example.test".into()),
            full_auto: Some(false),
            ..Default::default()
        }),
        append_system_prompt: set(&prompt),
        settings_json: set(&json),
    });
    r.environment = vec![proto::EnvironmentUpdate {
        name: "EMPTY".into(),
        value: set(""),
    }];
    let receipt = rpc
        .write(g.clone(), writes::Write::profile(r.clone()))
        .await
        .unwrap();
    assert_eq!(receipt.state, proto::OperationState::DeliveryUnknown as i32);
    assert_eq!(
        &*host.values.lock().unwrap(),
        &[prompt, json, String::new()]
    );
    assert!(secrets.entries.lock().unwrap().len() > 3);
    assert!(
        secrets
            .entries
            .lock()
            .unwrap()
            .values()
            .all(|v| v.len() <= 2560)
    );
    rpc.cleanup().await.unwrap();
    assert!(secrets.entries.lock().unwrap().len() > 3);
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    let args: Vec<u8> = db
        .query_row("SELECT arguments FROM _canopy_command_inbox", [], |r| {
            r.get(0)
        })
        .unwrap();
    let args = String::from_utf8(args).unwrap();
    assert!(!args.contains("secret JSON value"));
    assert!(!args.contains("private prompt"));
    rpc.write(g.clone(), writes::Write::profile(r.clone()))
        .await
        .unwrap();
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    r.command.as_mut().unwrap().command_id = uuid::Uuid::new_v4().to_string();
    r.settings
        .as_mut()
        .unwrap()
        .options
        .as_mut()
        .unwrap()
        .base_url = Some("https://user:secret@api.example.test".into());
    assert_eq!(
        rpc.write(g, writes::Write::profile(r))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
}
