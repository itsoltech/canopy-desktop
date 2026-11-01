use super::{
    AppState, ControlBridge, ControlState, SecretVersion, SettingsHost, SettingsSecrets, Status,
    edits,
};
use canopy_desktop::{
    remote::resources::ResourceLocks,
    settings::{Access, Change, SettingsClient},
    state::workspace::Workspace,
};
use gpui_kit::TestAppContext;
use std::sync::Arc;
fn preferences(
    host: &str,
    patch: canopy_desktop::remote::settings_rpc::PreferencesPatch,
    revision: u64,
) -> canopy_desktop::command_bus::CommandEnvelope {
    canopy_desktop::command_bus::CommandEnvelope {
        command_id: uuid::Uuid::new_v4(),
        resource: canopy_desktop::command_bus::ResourceRef::Environment(
            canopy_desktop::thread::EnvironmentRef {
                host_id: canopy_desktop::thread::HostId::new(host.into()).unwrap(),
            },
        ),
        command: patch.into_command(),
        conditions: canopy_desktop::command_bus::Conditions {
            expected_revision: Some(revision),
            ..Default::default()
        },
    }
}
struct FakeSecrets;
#[tonic::async_trait]
impl SettingsSecrets for FakeSecrets {
    async fn fingerprint(&self, _: &[u8]) -> Result<[u8; 32], Status> {
        Ok([7; 32])
    }
    async fn stage(&self, _: &SecretVersion, _: &str) -> Result<(), Status> {
        panic!("No vault operation expected")
    }
    async fn read(&self, version: &SecretVersion) -> Result<String, Status> {
        if version.id() == "11111111-1111-4111-8111-111111111111" {
            Ok(String::new())
        } else {
            Err(Status::failed_precondition("Missing fake staging value."))
        }
    }
    async fn remove(&self, _: &SecretVersion) -> Result<(), Status> {
        panic!("No vault operation expected")
    }
}
#[gpui_kit::test]
async fn preferences_cas_waits_for_commit_and_preserves_queued_local_change(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(dir.path().join("test.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let (app, bridge) = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.settings
            .update(cx, |s, _| s.install_test_client(client.clone(), host));
        app.tools
            .update(cx, |s, _| s.install_test_client(client.clone()));
        app.integrations
            .update(cx, |s, _| s.install_test_client(client.clone()));
        let bridge = ControlBridge::install(Arc::new(FakeSecrets), ResourceLocks::default(), cx);
        (app, bridge)
    });
    let first = bridge.snapshot().await.unwrap();
    let first_revision = first.revision;
    let patch = serde_json::from_value(serde_json::json!({"notch_enabled":false})).unwrap();
    let request = preferences(&first.host_id, patch, first.revision);
    let retry = request.clone();
    // A second request can finish admission before the first one commits, then
    // reach the owner after its CAS revision has advanced.
    let admitted_retry = canopy_desktop::command_bus::CommandBus::new(client.clone())
        .prepare(
            request.clone(),
            canopy_desktop::command_bus::Principal::local_owner(),
        )
        .await
        .unwrap();
    // Keep commit pending while admission can still read, so the local choice
    // deterministically arrives behind the reserved remote owner.
    let lock = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    let remote = cx.background_executor.spawn({
        let bridge = bridge.clone();
        async move {
            bridge
                .update_preferences(
                    request,
                    canopy_desktop::command_bus::Principal::local_owner(),
                )
                .await
        }
    });
    for _ in 0..100 {
        cx.run_until_parked();
        if cx.update(|cx| app.settings.read(cx).saving) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(cx.update(|cx| app.settings.read(cx).saving));
    cx.update(|cx| {
        app.settings
            .update(cx, |s, cx| s.change(Change::ResourceUsage(true), cx))
    });
    lock.execute_batch("COMMIT").unwrap();
    let receipt = remote.await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(receipt.result().unwrap()).unwrap();
    assert_eq!(result["values"]["notch_enabled"], false);
    assert_eq!(result["revision"], first_revision.wrapping_add(1).max(1));
    // Successful reply is backed by storage, not merely the UI enqueue.
    assert!(!client.load().await.unwrap().preferences.notch_enabled);
    for _ in 0..100 {
        cx.run_until_parked();
        if !cx.update(|cx| app.settings.read(cx).saving) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let saved = client.load().await.unwrap().preferences;
    assert!(saved.resource_usage);
    assert!(!saved.notch_enabled);
    assert_eq!(
        bridge
            .request(|reply| super::Op::Preferences(admitted_retry, reply))
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(
        bridge
            .update_preferences(retry, canopy_desktop::command_bus::Principal::local_owner())
            .await
            .unwrap(),
        receipt
    );
    let patch = serde_json::from_value(serde_json::json!({"notch_enabled":true})).unwrap();
    assert_eq!(
        bridge
            .update_preferences(
                preferences(
                    &bridge.snapshot().await.unwrap().host_id,
                    patch,
                    first_revision
                ),
                canopy_desktop::command_bus::Principal::local_owner()
            )
            .await
            .unwrap()
            .state(),
        canopy_desktop::host::CommandState::Rejected
    );
    let second = bridge.snapshot().await.unwrap();
    cx.update(|cx| {
        app.tools.update(cx, |s, cx| {
            s.discovering = true;
            cx.notify();
        })
    });
    assert_eq!(second.revision, bridge.snapshot().await.unwrap().revision);
    client.shutdown().await.unwrap();
}
#[gpui_kit::test]
async fn failed_preferences_commit_keeps_live_values_and_revision(cx: &mut TestAppContext) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let writer = SettingsClient::create(&path).await.unwrap();
    let host = writer.initialize_runtime().await.unwrap();
    writer.shutdown().await.unwrap();
    let client = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
    let (app, bridge) = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.settings
            .update(cx, |s, _| s.install_test_client(client.clone(), host));
        app.tools
            .update(cx, |s, _| s.install_test_client(client.clone()));
        app.integrations
            .update(cx, |s, _| s.install_test_client(client.clone()));
        (
            app,
            ControlBridge::install(Arc::new(FakeSecrets), ResourceLocks::default(), cx),
        )
    });
    let before = bridge.snapshot().await.unwrap();
    let patch = serde_json::from_value(serde_json::json!({"notch_enabled":false})).unwrap();
    assert!(
        bridge
            .update_preferences(
                preferences(&before.host_id, patch, before.revision),
                canopy_desktop::command_bus::Principal::local_owner()
            )
            .await
            .is_err()
    );
    assert!(cx.update(|cx| app.settings.read(cx).values.notch_enabled));
    assert_eq!(bridge.snapshot().await.unwrap().revision, before.revision);
    client.shutdown().await.unwrap();
}
#[gpui_kit::test]
fn stale_or_busy_owner_rejects_before_building_secret_write(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.tools.update(cx, |s, _| s.ready = true);
        app.integrations.update(cx, |s, _| s.ready = true);
        let mut state = ControlState::new();
        let version = state.revision(cx);
        app.tools.update(cx, |s, _| s.saving = true);
        assert_eq!(
            state.check(Some(version), cx).unwrap_err().code(),
            tonic::Code::ResourceExhausted
        );
        app.tools.update(cx, |s, _| {
            s.saving = false;
            s.catalog.tools[0].name = "changed locally".into();
        });
        assert_eq!(
            state.check(Some(version), cx).unwrap_err().code(),
            tonic::Code::Aborted
        );
    });
}
#[test]
fn remote_secret_fields_replace_legacy_values_without_plaintext_or_staging_refs() {
    let mut settings = canopy_desktop::state::agent_settings::AgentSettings::default();
    settings
        .custom_env
        .insert("PRIVATE_VALUE".into(), "old".into());
    settings.append_system_prompt = "old prompt".into();
    let mut pending = Vec::new();
    edits::environment(
        &mut settings.env_refs,
        &mut settings.custom_env,
        [(
            "PRIVATE_VALUE".into(),
            edits::Value::Set("new secret".into()),
        )]
        .into(),
        &mut pending,
    )
    .unwrap();
    edits::secret_field(
        &mut settings.prompt_ref,
        &mut settings.append_system_prompt,
        edits::Value::Set("Ż".repeat(3000)),
        &mut pending,
    )
    .unwrap();
    let encoded = serde_json::to_string(&settings).unwrap();
    assert!(!encoded.contains("new secret"));
    assert!(!encoded.contains("old"));
    assert!(!encoded.contains('Ż'));
    assert!(pending.iter().all(|(_, value)| value.len() <= 2560));
    assert_eq!(
        pending
            .iter()
            .skip(1)
            .map(|(_, v)| v.as_str())
            .collect::<String>(),
        "Ż".repeat(3000)
    );
}

#[gpui_kit::test]
async fn task_filters_share_live_owner_and_acknowledge_storage(cx: &mut TestAppContext) {
    use canopy_desktop::{
        integrations::{Provider, filters::TaskFilter},
        remote::tasks::TasksHost,
    };
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(dir.path().join("test.db"))
        .await
        .unwrap();
    let (app, bridge) = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.integrations
            .update(cx, |s, _| s.install_test_client(client.clone()));
        (
            app,
            ControlBridge::install(Arc::new(FakeSecrets), ResourceLocks::default(), cx),
        )
    });
    let filter = TaskFilter {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Assigned".into(),
        provider: Provider::Jira,
        expression: "assignee = currentUser()".into(),
    };
    // The desktop reserves this same owner for connect, filter, link and override writes.
    cx.update(|cx| app.integrations.update(cx, |s, _| s.busy = true));
    assert_eq!(
        bridge.save_filter(filter.clone()).await.unwrap_err().code(),
        tonic::Code::ResourceExhausted
    );
    assert!(
        client
            .load_integrations()
            .await
            .unwrap()
            .task_filters
            .is_empty()
    );
    cx.update(|cx| app.integrations.update(cx, |s, _| s.busy = false));
    bridge.save_filter(filter.clone()).await.unwrap();
    assert_eq!(
        client.load_integrations().await.unwrap().task_filters,
        vec![filter.clone()]
    );
    assert_eq!(
        cx.update(|cx| app.integrations.read(cx).config.task_filters.clone()),
        vec![filter.clone()]
    );
    bridge.remove_filter(filter.id).await.unwrap();
    assert!(
        client
            .load_integrations()
            .await
            .unwrap()
            .task_filters
            .is_empty()
    );
    client.shutdown().await.unwrap();
}

#[gpui_kit::test]
async fn partial_profile_edit_preserves_unmentioned_local_options_and_durable_catalog(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(dir.path().join("test.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let mut catalog = canopy_desktop::state::tools::ToolCatalog::default();
    let tool = catalog.tools.iter_mut().find(|t| t.id == "claude").unwrap();
    let profile = &mut tool.profiles[0];
    profile
        .settings
        .custom_env
        .insert("LOCAL".into(), "kept locally".into());
    profile.settings.permission_mode = "plan".into();
    profile.settings.append_system_prompt = "local prompt".into();
    profile.arguments = vec!["--verbose".into()];
    let id = profile.id.clone();
    client.save_tools(catalog.clone()).await.unwrap();
    let (app, bridge) = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.settings
            .update(cx, |s, _| s.install_test_client(client.clone(), host));
        app.tools.update(cx, |s, _| {
            s.install_test_client(client.clone());
            s.catalog = catalog.clone();
        });
        app.integrations
            .update(cx, |s, _| s.install_test_client(client.clone()));
        (
            app,
            ControlBridge::install(Arc::new(FakeSecrets), ResourceLocks::default(), cx),
        )
    });
    let snapshot = bridge.snapshot().await.unwrap();
    assert!(!format!("{snapshot:?}").contains("kept locally"));
    assert!(!format!("{snapshot:?}").contains("local prompt"));
    let edit = serde_json::from_value(serde_json::json!({"id":id,"agent_id":"claude","name":"Renamed remotely","environment":{"EMPTY_REMOTE":{"Set":{"id":"11111111-1111-4111-8111-111111111111","empty":true}}},"credential":"Unchanged","model":"updated-model","arguments":null,"settings":null})).unwrap();
    bridge
        .save_profile(edit, Some(snapshot.revision))
        .await
        .unwrap();
    let persisted = client.load_tools().await.unwrap();
    let profile = persisted
        .get("claude")
        .unwrap()
        .profiles
        .iter()
        .find(|p| p.id == id)
        .unwrap();
    assert_eq!(profile.model, "updated-model");
    assert_eq!(profile.settings.env_refs.get("EMPTY_REMOTE"), Some(&vec![]));
    assert!(
        !serde_json::to_string(profile)
            .unwrap()
            .contains("11111111-1111-4111-8111-111111111111")
    );
    assert_eq!(profile.settings.permission_mode, "plan");
    assert_eq!(profile.settings.custom_env["LOCAL"], "kept locally");
    assert_eq!(profile.settings.append_system_prompt, "local prompt");
    assert_eq!(profile.arguments, ["--verbose"]);
    assert_eq!(
        cx.update(|cx| app.tools.read(cx).catalog.clone()),
        persisted
    );
    client.shutdown().await.unwrap();
}

#[gpui_kit::test]
async fn task_worktree_returns_only_after_catalog_and_task_link_commit(cx: &mut TestAppContext) {
    use canopy_desktop::{
        integrations::{ProjectTarget, TaskRef},
        remote::{proto, tasks::TasksHost},
    };
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("repo");
    {
        let repo = git2::Repository::init(&repository).unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
    }
    let client = SettingsClient::create(dir.path().join("test.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let host_id = host.id().to_owned();
    let (app, bridge, workspace) = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.settings
            .update(cx, |s, _| s.install_test_client(client.clone(), host));
        app.integrations
            .update(cx, |s, _| s.install_test_client(client.clone()));
        let workspace = app.projects.update(cx, |s, _| {
            s.install_test_client(client.clone());
            let id = s.catalog.open(repository.clone());
            s.catalog.active = None;
            id
        });
        (
            app,
            ControlBridge::install(Arc::new(FakeSecrets), ResourceLocks::default(), cx),
            workspace,
        )
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let task = TaskRef {
        project: ProjectTarget::github("owner/repo").unwrap(),
        id: "42".into(),
        title: "A real task link".into(),
    };
    let create = runtime.spawn({
        let bridge = bridge.clone();
        let task = task.clone();
        async move {
            bridge
                .task_to_worktree(
                    proto::WorkspaceRef {
                        host_id,
                        workspace_id: serde_json::to_string(&workspace).unwrap(),
                    },
                    task,
                    "task-42".into(),
                )
                .await
        }
    });
    // The production writer debounces by 200 ms on the headless scheduler.
    for _ in 0..100 {
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(250));
        if create.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(create.is_finished(), "worktree callback did not finish");
    let id = create.await.unwrap().unwrap();
    let id: canopy_desktop::state::workspace::WorkspaceId = serde_json::from_str(&id).unwrap();
    let path = cx.update(|cx| {
        let state = app.projects.read(cx);
        assert_eq!(state.catalog.active, None);
        state
            .catalog
            .items
            .iter()
            .find(|p| p.workspace == id)
            .unwrap()
            .path
            .clone()
    });
    assert!(path.is_dir());
    let persisted = client.load_session().await.unwrap().unwrap();
    assert!(persisted.projects.items.iter().any(|p| p.workspace == id));
    assert_eq!(
        client.load_integrations().await.unwrap().links.get(&path),
        Some(&vec![task])
    );
    client.shutdown().await.unwrap();
}

#[derive(Default)]
struct FakeActiveCredentials {
    values: std::sync::Mutex<std::collections::HashMap<String, String>>,
    fail: std::sync::atomic::AtomicBool,
}
impl crate::app_state::tools::ToolCredentials for FakeActiveCredentials {
    fn store(&self, id: &str, value: &str) -> Result<(), String> {
        if self.fail.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("vault unavailable".into());
        }
        self.values
            .lock()
            .unwrap()
            .insert(id.to_owned(), value.to_owned());
        Ok(())
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        self.values.lock().unwrap().remove(id);
        Ok(())
    }
}
#[gpui_kit::test]
async fn local_secret_edits_use_vault_and_failed_replacement_preserves_committed_values(
    cx: &mut TestAppContext,
) {
    cx.background_executor.allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(dir.path().join("test.db"))
        .await
        .unwrap();
    let vault = Arc::new(FakeActiveCredentials::default());
    let mut initial = canopy_desktop::state::tools::ToolCatalog::default();
    initial
        .tools
        .iter_mut()
        .find(|t| t.id == "claude")
        .unwrap()
        .profiles[0]
        .settings
        .custom_env
        .insert("LEGACY_UNCHANGED".into(), "legacy".into());
    client.save_tools(initial.clone()).await.unwrap();
    let app = cx.update(|cx| {
        let app = AppState::install_test(Workspace::new(), cx);
        app.tools.update(cx, |s, _| {
            s.install_test_client(client.clone());
            s.use_test_credentials(vault.clone());
            s.catalog = initial.clone();
        });
        app
    });
    let stale = initial.get("claude").unwrap().clone();
    let mut edited = initial;
    let settings = &mut edited
        .tools
        .iter_mut()
        .find(|t| t.id == "claude")
        .unwrap()
        .profiles[0]
        .settings;
    settings
        .custom_env
        .insert("LOCAL_SECRET".into(), "new private environment".into());
    settings.append_system_prompt = "new private prompt".into();
    settings.settings_json = r#"{"private":"new private JSON"}"#.into();
    let done = cx.update(|cx| {
        app.tools.update(cx, |s, cx| {
            let (tx, rx) = futures_channel::oneshot::channel();
            s.save_with_completion(edited, Default::default(), Vec::new(), Some(tx), cx)
                .unwrap();
            rx
        })
    });
    done.await.unwrap().unwrap();
    let saved = client.load_tools().await.unwrap();
    cx.update(|cx| {
        app.tools.update(cx, |state, cx| {
            let mut draft = stale.clone();
            draft.name = "Stale local form".into();
            assert!(
                state
                    .save_tool_edit(Some(&stale), draft, Default::default(), cx)
                    .is_err()
            );
        })
    });
    let encoded = serde_json::to_string(&saved).unwrap();
    assert!(!encoded.contains("new private"));
    let settings = &saved.get("claude").unwrap().profiles[0].settings;
    assert_eq!(settings.custom_env["LEGACY_UNCHANGED"], "legacy");
    assert!(settings.prompt_ref.is_some() && settings.settings_json_ref.is_some());
    assert_eq!(
        vault
            .values
            .lock()
            .unwrap()
            .get(&settings.env_refs["LOCAL_SECRET"][0])
            .unwrap(),
        "new private environment"
    );
    vault.fail.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut replacement = saved.clone();
    replacement
        .tools
        .iter_mut()
        .find(|t| t.id == "claude")
        .unwrap()
        .profiles[0]
        .settings
        .append_system_prompt = "failed replacement".into();
    let done = cx.update(|cx| {
        app.tools.update(cx, |s, cx| {
            let (tx, rx) = futures_channel::oneshot::channel();
            s.save_with_completion(replacement, Default::default(), Vec::new(), Some(tx), cx)
                .unwrap();
            rx
        })
    });
    assert!(done.await.unwrap().is_err());
    assert_eq!(client.load_tools().await.unwrap(), saved);
    assert_eq!(cx.update(|cx| app.tools.read(cx).catalog.clone()), saved);
    assert!(
        vault
            .values
            .lock()
            .unwrap()
            .values()
            .any(|v| v == "new private prompt")
    );
    vault
        .fail
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let mut removed = saved.clone();
    removed
        .tools
        .iter_mut()
        .find(|t| t.id == "claude")
        .unwrap()
        .profiles[0]
        .settings
        .prompt_ref = None;
    let done = cx.update(|cx| {
        app.tools.update(cx, |s, cx| {
            let (tx, rx) = futures_channel::oneshot::channel();
            s.save_with_completion(removed, Default::default(), Vec::new(), Some(tx), cx)
                .unwrap();
            rx
        })
    });
    done.await.unwrap().unwrap();
    assert!(
        client
            .load_tools()
            .await
            .unwrap()
            .get("claude")
            .unwrap()
            .profiles[0]
            .settings
            .prompt_ref
            .is_none()
    );
    assert!(
        !vault
            .values
            .lock()
            .unwrap()
            .values()
            .any(|v| v == "new private prompt")
    );
    client.shutdown().await.unwrap();
}
