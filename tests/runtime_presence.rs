//! Real worker regressions for stale presence, authorization and orphan recovery.
use canopy_desktop::{
    chat::config::ChatSettings,
    host::{self, ChatRecord, CommandKey, DeviceRegistration, FileIdentity},
    settings::{Access, ChatHistoryBatch, Error, SettingsClient},
    state::workspace::{PaneKind, PaneMetadata},
};
use futures_lite::future::block_on;
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn local() -> CommandKey {
    CommandKey::local(id()).unwrap()
}
fn remote(device: &str) -> CommandKey {
    CommandKey::remote(device.into(), 1, id()).unwrap()
}
async fn register(c: &SettingsClient, name: &str, credential: Vec<u8>, root: &str) -> String {
    let device = id();
    c.register_runtime_device(
        DeviceRegistration::new(
            device.clone(),
            name.into(),
            credential,
            host::READ | host::FILES_GIT,
            vec![root.into()],
        )
        .unwrap(),
    )
    .await
    .unwrap();
    device
}
#[test]
fn presence_survives_restart_and_stale_dismiss_cannot_remove_a_new_buffer() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let root = dir
            .path()
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let file = FileIdentity::new(root.clone(), "src/file.rs".into()).unwrap();
        assert!(FileIdentity::new(root.clone(), "src/../outside".into()).is_none());
        assert!(FileIdentity::new(root.clone(), "./src/file.rs".into()).is_none());
        let c = SettingsClient::create(&path).await.unwrap();
        c.initialize_runtime().await.unwrap();
        let device = register(&c, "Alice's phone", vec![1; 32], &root).await;
        let input = file
            .presence_command(remote(&device), [3; 32], true)
            .unwrap();
        c.set_runtime_dirty_presence(input.clone(), file.clone(), [3; 32], true)
            .await
            .unwrap();
        let old = c.runtime_file_presence(file.clone()).await.unwrap();
        let revision = old[0].revision();
        // A retry is a receipt replay, not a presence heartbeat/new revision.
        c.set_runtime_dirty_presence(input, file.clone(), [3; 32], true)
            .await
            .unwrap();
        assert_eq!(c.runtime_file_presence(file.clone()).await.unwrap(), old);
        c.shutdown().await.unwrap();
        let c = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        c.initialize_runtime().await.unwrap();
        assert_eq!(c.runtime_file_presence(file.clone()).await.unwrap(), old);
        // Only this owner's explicit save/discard clears the record.
        let clear = file
            .presence_command(remote(&device), [3; 32], false)
            .unwrap();
        c.set_runtime_dirty_presence(clear, file.clone(), [3; 32], false)
            .await
            .unwrap();
        assert!(
            c.runtime_file_presence(file.clone())
                .await
                .unwrap()
                .is_empty()
        );
        let new = file
            .presence_command(remote(&device), [4; 32], true)
            .unwrap();
        c.set_runtime_dirty_presence(new, file.clone(), [4; 32], true)
            .await
            .unwrap();
        let stale = file
            .dismiss_presence_command(local(), &device, revision)
            .unwrap();
        assert!(matches!(
            c.dismiss_runtime_dirty_presence(stale, file.clone(), device.clone(), revision)
                .await,
            Err(Error::ResourceConflict)
        ));
        let current = c.runtime_file_presence(file.clone()).await.unwrap();
        assert_eq!(current[0].device_name(), "Alice's phone");
        assert_eq!(current[0].base_hash(), &[4; 32]);
        let denied = file
            .dismiss_presence_command(remote(&device), &device, current[0].revision())
            .unwrap();
        assert!(matches!(
            c.dismiss_runtime_dirty_presence(
                denied,
                file.clone(),
                device.clone(),
                current[0].revision()
            )
            .await,
            Err(Error::NotAuthorized)
        ));
        let dismiss = file
            .dismiss_presence_command(local(), &device, current[0].revision())
            .unwrap();
        c.dismiss_runtime_dirty_presence(
            dismiss.clone(),
            file.clone(),
            device.clone(),
            current[0].revision(),
        )
        .await
        .unwrap();
        c.dismiss_runtime_dirty_presence(
            dismiss,
            file.clone(),
            device.clone(),
            current[0].revision(),
        )
        .await
        .unwrap();
        assert!(
            c.runtime_file_presence(file.clone())
                .await
                .unwrap()
                .is_empty()
        );
        let input = file
            .presence_command(remote(&device), [5; 32], true)
            .unwrap();
        c.set_runtime_dirty_presence(input.clone(), file.clone(), [5; 32], true)
            .await
            .unwrap();
        c.revoke_runtime_device(device).await.unwrap();
        assert!(
            c.runtime_file_presence(file.clone())
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            c.set_runtime_dirty_presence(input, file, [5; 32], true)
                .await,
            Err(Error::NotAuthorized)
        ));
        c.shutdown().await.unwrap();
    });
}
#[test]
fn credential_binding_names_and_current_authorization_are_independent() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let c = SettingsClient::create(dir.path().join("test.db"))
            .await
            .unwrap();
        c.initialize_runtime().await.unwrap();
        let a = register(&c, "same display name", vec![1; 32], "/allowed").await;
        let b = register(&c, "same display name", vec![2; 32], "/other").await;
        assert!(matches!(
            c.register_runtime_device(
                DeviceRegistration::new(
                    id(),
                    "new name".into(),
                    vec![1; 32],
                    host::ADMIN,
                    vec!["*".into()]
                )
                .unwrap()
            )
            .await,
            Err(Error::ResourceConflict)
        ));
        assert!(matches!(
            c.lookup_runtime_device(a.as_bytes().to_vec()).await,
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            c.lookup_runtime_device(b"same display name".to_vec()).await,
            Err(Error::NotAuthorized)
        ));
        assert_eq!(c.lookup_runtime_device(vec![2; 32]).await.unwrap().id(), b);
        assert!(matches!(
            c.authorize_runtime_read("local".into(), host::READ, "/allowed".into())
                .await,
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            c.authorize_runtime_read(b, host::READ, "/allowed".into())
                .await,
            Err(Error::NotAuthorized)
        ));
        let grant = c
            .authorize_runtime_read(a.clone(), host::READ, "/allowed".into())
            .await
            .unwrap();
        c.update_runtime_device_authorization(
            a.clone(),
            grant.authorization_revision(),
            host::FILES_GIT,
            vec!["/allowed".into()],
        )
        .await
        .unwrap();
        assert!(matches!(
            c.authorize_runtime_read(a.clone(), host::READ, "/allowed".into())
                .await,
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            c.update_runtime_device_authorization(
                a.clone(),
                grant.authorization_revision(),
                host::ADMIN,
                vec!["*".into()]
            )
            .await,
            Err(Error::ResourceConflict)
        ));
        c.revoke_runtime_device(a.clone()).await.unwrap();
        assert!(matches!(
            c.lookup_runtime_device(vec![1; 32]).await,
            Err(Error::NotAuthorized)
        ));
        assert!(
            c.runtime_devices()
                .await
                .unwrap()
                .iter()
                .any(|g| g.id() == a && g.revoked())
        );
        // Re-registering a revoked credential must not resurrect its identity.
        assert!(matches!(
            c.register_runtime_device(
                DeviceRegistration::new(
                    id(),
                    "another name".into(),
                    vec![1; 32],
                    host::READ,
                    vec!["*".into()]
                )
                .unwrap()
            )
            .await,
            Err(Error::ResourceConflict)
        ));
        c.shutdown().await.unwrap();
    });
}
#[test]
fn orphan_recovery_requires_explicit_account_and_current_epoch_and_has_one_winner() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let c = SettingsClient::create(&path).await.unwrap();
        let chat_id = id();
        c.save_chat_history(vec![ChatHistoryBatch {
            chat: chat_id.clone(),
            epochs: vec![(7, "{}".into())],
            items: vec![(7, 42, "retained history".into())],
        }])
        .await
        .unwrap();
        c.initialize_runtime().await.unwrap();
        let orphan = c.load_runtime_chats().await.unwrap().pop().unwrap();
        let reason = orphan.recovery_reason().unwrap().to_owned();
        let make = |profile: Option<String>, epoch| {
            ChatRecord::new(
                chat_id.clone(),
                None,
                "claude".into(),
                PaneMetadata {
                    kind: PaneKind::Chat,
                    profile_id: profile,
                    chat: Some(Box::new(ChatSettings {
                        id: Some(chat_id.clone()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap()
            .with_history_epoch(epoch)
            .unwrap()
        };
        let missing = make(None, 7);
        let cmd = missing.recovery_command(local(), &reason).unwrap();
        assert!(matches!(
            c.recover_runtime_chat(cmd, missing, reason.clone()).await,
            Err(Error::InvalidRuntimeState)
        ));
        let stale = make(Some("claude-default".into()), 7);
        c.save_chat_history(vec![ChatHistoryBatch {
            chat: chat_id.clone(),
            epochs: vec![(8, "{}".into())],
            items: vec![],
        }])
        .await
        .unwrap();
        let cmd = stale.recovery_command(local(), &reason).unwrap();
        assert!(matches!(
            c.recover_runtime_chat(cmd, stale, reason.clone()).await,
            Err(Error::ResourceConflict)
        ));
        let record = make(Some("claude-default".into()), 8);
        let winner = record.recovery_command(local(), &reason).unwrap();
        let loser = record.recovery_command(local(), &reason).unwrap();
        c.recover_runtime_chat(winner.clone(), record.clone(), reason.clone())
            .await
            .unwrap();
        c.recover_runtime_chat(winner, record.clone(), reason.clone())
            .await
            .unwrap();
        assert!(matches!(
            c.recover_runtime_chat(loser, record, reason).await,
            Err(Error::ResourceConflict)
        ));
        let loaded = c.load_runtime_chats().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id(), chat_id);
        assert!(loaded[0].recovery_reason().is_none());
        assert_eq!(
            c.load_chat_history(chat_id.clone()).await.unwrap().items,
            vec![(7, 42, "retained history".into())]
        );
        c.shutdown().await.unwrap();
        let c = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        c.initialize_runtime().await.unwrap();
        assert!(
            c.load_runtime_chats().await.unwrap()[0]
                .recovery_reason()
                .is_none()
        );
        c.shutdown().await.unwrap();
    });
}
#[test]
fn tls_identity_reference_is_durable_and_cas_never_replaces_a_winner() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let c = SettingsClient::create(&path).await.unwrap();
        c.initialize_runtime().await.unwrap();
        let reference = id();
        c.set_runtime_identity_ref(None, reference.clone())
            .await
            .unwrap();
        assert!(matches!(
            c.set_runtime_identity_ref(None, id()).await,
            Err(Error::ResourceConflict)
        ));
        c.shutdown().await.unwrap();
        let c = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(
            c.initialize_runtime().await.unwrap().identity_ref(),
            Some(reference.as_str())
        );
        assert!(matches!(
            c.set_runtime_identity_ref(Some(id()), id()).await,
            Err(Error::ResourceConflict)
        ));
        c.shutdown().await.unwrap();
    });
}

#[test]
fn root_preflight_keeps_deleted_offline_files_and_fails_closed_at_limit() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let workspace = dir.path().join("quoted-\"-root");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("gone.rs"), "base").unwrap();
        let root = workspace
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let file = FileIdentity::new(root.clone(), "gone.rs".into()).unwrap();
        let c = SettingsClient::create(&path).await.unwrap();
        c.initialize_runtime().await.unwrap();
        let device = register(&c, "Offline phone", vec![7; 32], &root).await;
        for key in [local(), remote(&device)] {
            let input = file.presence_command(key, [3; 32], true).unwrap();
            c.set_runtime_dirty_presence(input, file.clone(), [3; 32], true)
                .await
                .unwrap();
        }
        for other in [format!("{root}-sibling"), format!("{root}/nested")] {
            let f = FileIdentity::new(other, "other.rs".into()).unwrap();
            c.set_runtime_dirty_presence(
                f.presence_command(local(), [3; 32], true).unwrap(),
                f,
                [3; 32],
                true,
            )
            .await
            .unwrap();
        }
        let expected = c.runtime_file_presence(file.clone()).await.unwrap();
        c.shutdown().await.unwrap();
        std::fs::remove_file(workspace.join("gone.rs")).unwrap();
        std::fs::remove_dir(&workspace).unwrap();
        let c = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        c.initialize_runtime().await.unwrap();
        let actual = c.runtime_root_presence(root.clone()).await.unwrap();
        assert_eq!(
            actual,
            expected
                .into_iter()
                .map(|p| (file.clone(), p))
                .collect::<Vec<_>>()
        );
        // The first row beyond the bound must reject the entire preflight,
        // rather than silently hide one owner's unsaved buffer.
        for n in 0..1023 {
            if n == 1022 {
                assert_eq!(
                    c.runtime_root_presence(root.clone()).await.unwrap().len(),
                    1024
                );
            }
            let f = FileIdentity::new(root.clone(), format!("missing-{n}.rs")).unwrap();
            c.set_runtime_dirty_presence(
                f.presence_command(local(), [3; 32], true).unwrap(),
                f,
                [3; 32],
                true,
            )
            .await
            .unwrap();
        }
        assert!(matches!(
            c.runtime_root_presence(root).await,
            Err(Error::ResourceLimit)
        ));
        c.shutdown().await.unwrap();
    });
}

#[test]
fn chat_path_grants_authorize_send_and_artifacts_independently_of_catalog_identity() {
    use canopy_desktop::{
        artifacts::{ArtifactStore, BeginUpload, Purpose, Target},
        chat::session::Draft,
        host::QueueDraft,
    };
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let c = SettingsClient::create(dir.path().join("test.db"))
            .await
            .unwrap();
        c.initialize_runtime().await.unwrap();
        let root = dir
            .path()
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let grouping = serde_json::to_string(&id()).unwrap();
        let chat_id = id();
        let record = ChatRecord::new(
            chat_id.clone(),
            Some(grouping.clone()),
            "claude".into(),
            PaneMetadata {
                kind: PaneKind::Chat,
                chat: Some(Box::new(ChatSettings {
                    id: Some(chat_id.clone()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            c.save_runtime_chat(record.clone()).await,
            Err(Error::InvalidRuntimeState)
        ));
        let record = record.with_authority_root(root.clone()).unwrap();
        c.save_runtime_chat(record.clone()).await.unwrap();
        let owner = id();
        c.register_runtime_device(
            DeviceRegistration::new(
                owner.clone(),
                "Path grant".into(),
                vec![19; 32],
                host::CHAT | host::READ,
                vec![root],
            )
            .unwrap(),
        )
        .await
        .unwrap();
        let message = id();
        let (input, draft) = QueueDraft::from(Draft::text("path authorized"))
            .command_for_message(
                remote(&owner),
                &chat_id,
                message.clone(),
                record.send_context(),
            )
            .unwrap();
        c.accept_runtime_message(
            input.clone(),
            chat_id.clone(),
            message.clone(),
            draft.clone(),
        )
        .await
        .unwrap();
        c.accept_runtime_message(input, chat_id.clone(), message, draft)
            .await
            .unwrap();
        assert_eq!(
            c.load_runtime_queue(chat_id.clone())
                .await
                .unwrap()
                .entries()
                .len(),
            1
        );
        let store = ArtifactStore::open(c.clone(), dir.path().join("bulk")).unwrap();
        let upload = BeginUpload::new(
            owner,
            Target::Chat(chat_id.clone()),
            Purpose::Image,
            1,
            [2; 32],
            "image/png".into(),
        )
        .unwrap();
        assert!(store.begin(upload).is_ok());
        let wrong = id();
        c.register_runtime_device(
            DeviceRegistration::new(
                wrong.clone(),
                "Catalog ID is not authority".into(),
                vec![20; 32],
                host::CHAT | host::READ,
                vec![grouping],
            )
            .unwrap(),
        )
        .await
        .unwrap();
        let message = id();
        let (input, draft) = QueueDraft::from(Draft::text("denied"))
            .command_for_message(
                remote(&wrong),
                &chat_id,
                message.clone(),
                record.send_context(),
            )
            .unwrap();
        assert!(matches!(
            c.accept_runtime_message(input, chat_id.clone(), message, draft)
                .await,
            Err(Error::NotAuthorized)
        ));
        let upload = BeginUpload::new(
            wrong,
            Target::Chat(chat_id),
            Purpose::Image,
            1,
            [2; 32],
            "image/png".into(),
        )
        .unwrap();
        assert!(matches!(store.begin(upload), Err(Error::NotAuthorized)));
        c.shutdown().await.unwrap();
    });
}

#[test]
fn adoption_uses_checkout_authority_and_quarantines_unresolved_project_paths() {
    use canopy_desktop::{
        chat::session::Draft,
        host::QueueDraft,
        state::{projects::Projects, session::SessionSnapshot, workspace::Workspace},
    };
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let checkout = dir.path().canonicalize().unwrap();
        let missing = checkout.join("missing-checkout");
        let mut projects = Projects::default();
        let mut workspaces = Vec::new();
        let mut identities = Vec::new();
        for path in [checkout.clone(), missing] {
            let workspace_id = projects.open(path);
            let mut workspace = Workspace::new();
            workspace.id = workspace_id;
            let tab = workspace.open("Chat", "claude");
            let pane = workspace.active().unwrap().focused;
            let chat_id = id();
            workspace
                .set_pane_metadata(
                    tab,
                    pane,
                    PaneMetadata {
                        kind: PaneKind::Chat,
                        profile_id: Some("claude-default".into()),
                        chat: Some(Box::new(ChatSettings {
                            id: Some(chat_id.clone()),
                            ..Default::default()
                        })),
                        ..Default::default()
                    },
                )
                .unwrap();
            identities.push(chat_id);
            workspaces.push(workspace);
        }
        let c = SettingsClient::create(&path).await.unwrap();
        c.save_session(SessionSnapshot {
            projects,
            workspaces,
            layout: Default::default(),
            scratch: None,
        })
        .await
        .unwrap();
        c.initialize_runtime().await.unwrap();
        let records = c.load_runtime_chats().await.unwrap();
        let valid = records.iter().find(|r| r.id() == identities[0]).unwrap();
        assert_eq!(valid.authority_root(), checkout.to_str().unwrap());
        assert!(valid.recovery_reason().is_none());
        let unresolved = records.iter().find(|r| r.id() == identities[1]).unwrap();
        assert!(unresolved.authority_root().is_empty());
        assert!(unresolved.recovery_reason().is_some());
        let owner = id();
        c.register_runtime_device(
            DeviceRegistration::new(
                owner.clone(),
                "Wildcard cannot guess".into(),
                vec![21; 32],
                host::CHAT,
                vec!["*".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
        let message = id();
        let (input, draft) = QueueDraft::from(Draft::text("no implicit authority"))
            .command_for_message(
                remote(&owner),
                unresolved.id(),
                message.clone(),
                unresolved.send_context(),
            )
            .unwrap();
        assert!(matches!(
            c.accept_runtime_message(input, unresolved.id().into(), message, draft)
                .await,
            Err(Error::NotAuthorized)
        ));
        c.shutdown().await.unwrap();
    });
}
