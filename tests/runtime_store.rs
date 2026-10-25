//! Crash boundaries and replay against the real SQLite worker, on isolated data.
use canopy_desktop::{
    chat::{config::ChatSettings, session::Draft},
    host::{
        ChatRecord, CommandInput, CommandKey, CommandState, DeviceRegistration, QueueDraft,
        QueueState, SendContext,
    },
    settings::{Access, Error, SettingsClient},
    state::workspace::{PaneKind, PaneMetadata},
};
use futures_lite::future::block_on;
use rusqlite::Connection;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn chat() -> ChatRecord {
    let id = id();
    ChatRecord::new(
        id.clone(),
        None,
        "claude".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            chat: Some(Box::new(ChatSettings {
                id: Some(id),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap()
}
fn send(key: CommandKey, chat: &str, text: &str) -> (CommandInput, QueueDraft) {
    let draft = QueueDraft::from(Draft::text(text));
    if key.device() == "local" {
        return draft.command(key, chat).unwrap();
    }
    let message_id = key.id().to_owned();
    draft
        .command_for_message(
            key,
            chat,
            message_id,
            SendContext::new("claude".into(), None, None, 0).unwrap(),
        )
        .unwrap()
}
fn operation(key: CommandKey) -> CommandInput {
    CommandInput::new(
        key,
        "files.save".into(),
        "file".into(),
        b"normalized arguments".to_vec(),
        vec![7; 32],
    )
    .unwrap()
    .with_root("private".into())
    .unwrap()
}
async fn device(client: &SettingsClient) -> String {
    let id = id();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                id.clone(),
                "Test phone".into(),
                id.as_bytes().to_vec(),
                3,
                vec!["*".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    id
}

#[test]
fn migration_preserves_history_and_unknown_data_and_host_identity() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE unrelated(value TEXT); INSERT INTO unrelated VALUES('keep'); INSERT INTO preferences VALUES('unknown.token','opaque');").unwrap();
        let first = client.initialize_runtime().await.unwrap();
        assert_eq!(first, client.initialize_runtime().await.unwrap());
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(client.initialize_runtime().await.unwrap(), first);
        assert_eq!(
            db.query_row("SELECT value FROM unrelated", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        assert_eq!(
            db.query_row(
                "SELECT value FROM preferences WHERE key='unknown.token'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "opaque"
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM _canopy_runtime_meta", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn a_failed_adoption_rolls_back_ddl_and_the_release_marker() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE _canopy_rust_session(id INTEGER PRIMARY KEY,version INTEGER,payload TEXT); INSERT INTO _canopy_rust_session VALUES(1,1,'unreadable');").unwrap();
        assert!(matches!(
            client.initialize_runtime().await,
            Err(Error::InvalidProjectState)
        ));
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='_canopy_runtime_meta'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT payload FROM _canopy_rust_session", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "unreadable"
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn incompatible_private_schema_is_rejected_before_writes_and_one_host_owns_the_data() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.initialize_runtime().await.unwrap();
        let second = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert!(matches!(
            second.initialize_runtime().await,
            Err(Error::RuntimeInUse)
        ));
        second.shutdown().await.unwrap();
        client.shutdown().await.unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "UPDATE _canopy_runtime_meta SET fingerprint='old private stage'",
            [],
        )
        .unwrap();
        drop(db);
        let before = std::fs::read(&path).unwrap();
        assert!(matches!(
            SettingsClient::open(&path, Access::ReadWrite).await,
            Err(Error::UnsupportedSchema)
        ));
        assert_eq!(before, std::fs::read(&path).unwrap());
    });
}

#[test]
fn accepted_send_and_queue_are_one_commit_and_retry_survives_restart() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.initialize_runtime().await.unwrap();
        let chat = chat();
        client.save_runtime_chat(chat.clone()).await.unwrap();
        let device = device(&client).await;
        let key = CommandKey::remote(device, 1, id()).unwrap();
        let (input, draft) = send(key.clone(), chat.id(), "hello");
        let message = key.id().to_owned();
        let accepted = client
            .accept_runtime_message(
                input.clone(),
                chat.id().into(),
                message.clone(),
                draft.clone(),
            )
            .await
            .unwrap();
        assert_eq!(accepted.state(), CommandState::Succeeded);
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        client.initialize_runtime().await.unwrap();
        assert_eq!(
            client
                .accept_runtime_message(
                    input.clone(),
                    chat.id().into(),
                    message.clone(),
                    draft.clone()
                )
                .await
                .unwrap(),
            accepted
        );
        let queue = client.load_runtime_queue(chat.id().into()).await.unwrap();
        assert!(queue.paused());
        assert_eq!(queue.entries().len(), 1);
        assert_eq!(queue.entries()[0].id(), message);
        client.delete_chat_history(chat.id().into()).await.unwrap();
        assert_eq!(
            client
                .accept_runtime_message(input, chat.id().into(), message.clone(), draft)
                .await
                .unwrap(),
            accepted,
            "deleting history must not make a retained command new"
        );
        let (changed, draft) = send(key, chat.id(), "different");
        assert!(matches!(
            client
                .accept_runtime_message(changed, chat.id().into(), message, draft)
                .await,
            Err(Error::CommandConflict)
        ));
        client.shutdown().await.unwrap();
    });
}

#[test]
fn queue_failure_does_not_leave_a_successful_receipt() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.initialize_runtime().await.unwrap();
        let chat = chat();
        client.save_runtime_chat(chat.clone()).await.unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TRIGGER fail_queue BEFORE INSERT ON _canopy_chat_queue BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        let key = CommandKey::local(id()).unwrap();
        let (input, draft) = send(key.clone(), chat.id(), "hello");
        assert!(
            client
                .accept_runtime_message(input, chat.id().into(), id(), draft)
                .await
                .is_err()
        );
        assert!(client.get_runtime_command(key, 0).await.unwrap().is_none());
        assert!(
            client
                .load_runtime_queue(chat.id().into())
                .await
                .unwrap()
                .entries()
                .is_empty()
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn crash_marks_external_attempts_unknown_and_never_automatically_resends_the_queue() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.initialize_runtime().await.unwrap();
        let chat = chat();
        client.save_runtime_chat(chat.clone()).await.unwrap();
        let key = CommandKey::local(id()).unwrap();
        let op = operation(key.clone());
        client.accept_runtime_command(op, 0).await.unwrap();
        client
            .transition_runtime_command(
                key.clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None,
            )
            .await
            .unwrap();
        let (input, draft) = send(CommandKey::local(id()).unwrap(), chat.id(), "queued");
        let message = id();
        client
            .accept_runtime_message(input, chat.id().into(), message.clone(), draft)
            .await
            .unwrap();
        let q = client.load_runtime_queue(chat.id().into()).await.unwrap();
        let q = client
            .set_runtime_queue_paused(chat.id().into(), q.revision(), false)
            .await
            .unwrap();
        client
            .transition_runtime_queue(
                chat.id().into(),
                message,
                q.revision(),
                QueueState::Queued,
                QueueState::Attempting,
            )
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        client.initialize_runtime().await.unwrap();
        assert_eq!(
            client
                .get_runtime_command(key, 0)
                .await
                .unwrap()
                .unwrap()
                .state(),
            CommandState::DeliveryUnknown
        );
        let q = client.load_runtime_queue(chat.id().into()).await.unwrap();
        assert!(q.paused());
        assert_eq!(q.entries()[0].state(), QueueState::DeliveryUnknown);
        client.shutdown().await.unwrap();
    });
}

#[test]
fn epoch_gc_fences_delayed_mutations_and_keeps_unresolved_effects() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.initialize_runtime().await.unwrap();
        let device = device(&client).await;
        let key = CommandKey::remote(device.clone(), 1, id()).unwrap();
        let input = operation(key.clone());
        client
            .accept_runtime_command(input.clone(), 0)
            .await
            .unwrap();
        assert!(matches!(
            client.close_runtime_command_epoch(device.clone(), 1).await,
            Err(Error::OperationPending)
        ));
        client
            .transition_runtime_command(
                key.clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None,
            )
            .await
            .unwrap();
        client
            .transition_runtime_command(
                key.clone(),
                CommandState::Attempting,
                CommandState::DeliveryUnknown,
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            client
                .close_runtime_command_epoch(device.clone(), 1)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            client.close_runtime_command_epoch(device, 1).await.unwrap(),
            2,
            "lost Close ACK does not increment twice"
        );
        assert!(matches!(
            client.accept_runtime_command(input, 0).await,
            Err(Error::CommandRetired)
        ));
        assert_eq!(
            client
                .get_runtime_command(key, 0)
                .await
                .unwrap()
                .unwrap()
                .state(),
            CommandState::DeliveryUnknown
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn revoke_prevents_receipt_disclosure_and_dispatch_without_deleting_accepted_messages() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.initialize_runtime().await.unwrap();
        let device = device(&client).await;
        let key = CommandKey::remote(device.clone(), 1, id()).unwrap();
        client
            .accept_runtime_command(operation(key.clone()), 0)
            .await
            .unwrap();
        let chat = chat();
        client.save_runtime_chat(chat.clone()).await.unwrap();
        let (input, draft) = send(
            CommandKey::remote(device.clone(), 1, id()).unwrap(),
            chat.id(),
            "keep me",
        );
        let message = input.key().id().to_owned();
        client
            .accept_runtime_message(input, chat.id().into(), message, draft)
            .await
            .unwrap();
        client.revoke_runtime_device(device).await.unwrap();
        assert!(matches!(
            client.get_runtime_command(key.clone(), 0).await,
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            client
                .transition_runtime_command(
                    key,
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None
                )
                .await,
            Err(Error::NotAuthorized)
        ));
        assert_eq!(
            client
                .load_runtime_queue(chat.id().into())
                .await
                .unwrap()
                .entries()[0]
                .draft()
                .text(),
            "keep me"
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn saved_authorization_cannot_be_downgraded_and_empty_roots_deny_access() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let c = SettingsClient::create(&path).await.unwrap();
        c.initialize_runtime().await.unwrap();
        let d = device(&c).await;
        let k = CommandKey::remote(d.clone(), 1, id()).unwrap();
        let op = operation(k.clone());
        c.accept_runtime_command(op.clone(), 2).await.unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "UPDATE _canopy_remote_devices SET scopes=1 WHERE device_id=?1",
            [&d],
        )
        .unwrap();
        assert!(matches!(
            c.get_runtime_command(k.clone(), 0).await,
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            c.accept_runtime_command(op.clone(), 0).await,
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            c.transition_runtime_command(
                k.clone(),
                CommandState::Accepted,
                CommandState::Attempting,
                None
            )
            .await,
            Err(Error::NotAuthorized)
        ));
        db.execute(
            "UPDATE _canopy_remote_devices SET scopes=3,roots='[]' WHERE device_id=?1",
            [&d],
        )
        .unwrap();
        assert!(matches!(
            c.get_runtime_command(k.clone(), 0).await,
            Err(Error::NotAuthorized)
        ));
        assert!(matches!(
            c.accept_runtime_command(
                operation(CommandKey::remote(d.clone(), 1, id()).unwrap()),
                0
            )
            .await,
            Err(Error::NotAuthorized)
        ));
        db.execute(
            "UPDATE _canopy_remote_devices SET roots='[\"other-workspace\"]' WHERE device_id=?1",
            [&d],
        )
        .unwrap();
        assert!(matches!(
            c.get_runtime_command(k.clone(), 0).await,
            Err(Error::NotAuthorized)
        ));
        db.execute(
            "UPDATE _canopy_remote_devices SET roots='[\"private\"]' WHERE device_id=?1",
            [&d],
        )
        .unwrap();
        assert!(c.get_runtime_command(k, 0).await.unwrap().is_some());
        c.shutdown().await.unwrap();
    });
}

#[test]
fn retry_with_reused_fingerprint_cannot_replace_normalized_arguments_or_draft() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let c = SettingsClient::create(dir.path().join("test.db"))
            .await
            .unwrap();
        c.initialize_runtime().await.unwrap();
        let k = CommandKey::local(id()).unwrap();
        c.accept_runtime_command(operation(k.clone()), 0)
            .await
            .unwrap();
        let attack = CommandInput::new(
            k,
            "files.save".into(),
            "file".into(),
            b"different arguments".to_vec(),
            vec![7; 32],
        )
        .unwrap()
        .with_root("private".into())
        .unwrap();
        assert!(matches!(
            c.accept_runtime_command(attack, 0).await,
            Err(Error::CommandConflict)
        ));
        let chat = chat();
        c.save_runtime_chat(chat.clone()).await.unwrap();
        let (input, draft) = send(CommandKey::local(id()).unwrap(), chat.id(), "original");
        c.accept_runtime_message(input.clone(), chat.id().into(), id(), draft)
            .await
            .unwrap();
        assert!(matches!(
            c.accept_runtime_message(
                input,
                chat.id().into(),
                id(),
                QueueDraft::from(Draft::text("substituted"))
            )
            .await,
            Err(Error::CommandConflict)
        ));
        c.shutdown().await.unwrap();
    });
}

#[test]
fn queue_mutations_are_atomic_revision_checked_and_unknown_resend_is_a_new_identity() {
    use canopy_desktop::host::{QueueMutation as M, UnknownResolution as R};
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let c = SettingsClient::create(&path).await.unwrap();
        c.initialize_runtime().await.unwrap();
        let chat = chat();
        c.save_runtime_chat(chat.clone()).await.unwrap();
        let a = id();
        let b = id();
        for (message, text) in [(&a, "a"), (&b, "b")] {
            let (input, draft) = send(CommandKey::local(id()).unwrap(), chat.id(), text);
            c.accept_runtime_message(input, chat.id().into(), message.clone(), draft)
                .await
                .unwrap();
        }
        let q = c.load_runtime_queue(chat.id().into()).await.unwrap();
        let edit = M::Edit {
            message: a.clone(),
            draft: QueueDraft::from(Draft::text("edited")),
        };
        let key = CommandKey::local(id()).unwrap();
        let input = edit.command(key.clone(), chat.id(), q.revision()).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TRIGGER fail_edit BEFORE UPDATE OF payload ON _canopy_chat_queue BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        assert!(
            c.mutate_runtime_queue(input.clone(), chat.id().into(), q.revision(), edit.clone())
                .await
                .is_err()
        );
        assert!(c.get_runtime_command(key, 0).await.unwrap().is_none());
        assert_eq!(c.load_runtime_queue(chat.id().into()).await.unwrap(), q);
        db.execute_batch("DROP TRIGGER fail_edit").unwrap();
        let receipt = c
            .mutate_runtime_queue(input.clone(), chat.id().into(), q.revision(), edit.clone())
            .await
            .unwrap();
        assert_eq!(
            c.mutate_runtime_queue(input, chat.id().into(), q.revision(), edit)
                .await
                .unwrap(),
            receipt
        );
        let stale = M::Remove { message: b.clone() };
        assert!(matches!(
            c.mutate_runtime_queue(
                stale
                    .command(CommandKey::local(id()).unwrap(), chat.id(), q.revision())
                    .unwrap(),
                chat.id().into(),
                q.revision(),
                stale
            )
            .await,
            Err(Error::ResourceConflict)
        ));
        for mutation in [
            M::Move {
                message: b.clone(),
                before: Some(a.clone()),
            },
            M::Pause { paused: false },
        ] {
            let q = c.load_runtime_queue(chat.id().into()).await.unwrap();
            c.mutate_runtime_queue(
                mutation
                    .command(CommandKey::local(id()).unwrap(), chat.id(), q.revision())
                    .unwrap(),
                chat.id().into(),
                q.revision(),
                mutation,
            )
            .await
            .unwrap();
        }
        let q = c.load_runtime_queue(chat.id().into()).await.unwrap();
        assert_eq!(q.entries()[0].id(), b);
        let q = c
            .transition_runtime_queue(
                chat.id().into(),
                b.clone(),
                q.revision(),
                QueueState::Queued,
                QueueState::Attempting,
            )
            .await
            .unwrap();
        // Removing an in-flight position must roll back its revision too.
        let remove = M::Remove { message: b.clone() };
        assert!(matches!(
            c.mutate_runtime_queue(
                remove
                    .command(CommandKey::local(id()).unwrap(), chat.id(), q.revision())
                    .unwrap(),
                chat.id().into(),
                q.revision(),
                remove
            )
            .await,
            Err(Error::ResourceConflict)
        ));
        let q = c
            .transition_runtime_queue(
                chat.id().into(),
                b.clone(),
                q.revision(),
                QueueState::Attempting,
                QueueState::DeliveryUnknown,
            )
            .await
            .unwrap();
        assert!(matches!(
            c.transition_runtime_queue(
                chat.id().into(),
                a.clone(),
                q.revision(),
                QueueState::Queued,
                QueueState::Attempting
            )
            .await,
            Err(Error::OperationPending)
        ));
        let replacement = id();
        let resolution = M::ResolveUnknown {
            message: b.clone(),
            resolution: R::Resend {
                message: replacement.clone(),
            },
        };
        let input = resolution
            .command(CommandKey::local(id()).unwrap(), chat.id(), q.revision())
            .unwrap();
        let receipt = c
            .mutate_runtime_queue(
                input.clone(),
                chat.id().into(),
                q.revision(),
                resolution.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            c.mutate_runtime_queue(input, chat.id().into(), q.revision(), resolution)
                .await
                .unwrap(),
            receipt
        );
        let q = c.load_runtime_queue(chat.id().into()).await.unwrap();
        assert_eq!(
            q.entries().iter().find(|e| e.id() == b).unwrap().state(),
            QueueState::Dismissed
        );
        assert!(
            q.entries()
                .iter()
                .find(|e| e.id() == b)
                .unwrap()
                .resolved_by_user()
        );
        assert_eq!(
            q.entries().iter().filter(|e| e.id() == replacement).count(),
            1
        );
        let q = c
            .transition_runtime_queue(
                chat.id().into(),
                replacement.clone(),
                q.revision(),
                QueueState::Queued,
                QueueState::Attempting,
            )
            .await
            .unwrap();
        assert!(
            matches!(
                c.transition_runtime_queue(
                    chat.id().into(),
                    b,
                    q.revision(),
                    QueueState::Attempting,
                    QueueState::Delivered
                )
                .await,
                Err(Error::ResourceConflict)
            ),
            "late completion must not acknowledge replacement"
        );
        let q = c
            .transition_runtime_queue(
                chat.id().into(),
                replacement.clone(),
                q.revision(),
                QueueState::Attempting,
                QueueState::DeliveryUnknown,
            )
            .await
            .unwrap();
        let resolution = M::ResolveUnknown {
            message: replacement,
            resolution: R::Delivered,
        };
        c.mutate_runtime_queue(
            resolution
                .command(CommandKey::local(id()).unwrap(), chat.id(), q.revision())
                .unwrap(),
            chat.id().into(),
            q.revision(),
            resolution,
        )
        .await
        .unwrap();
        let q = c.load_runtime_queue(chat.id().into()).await.unwrap();
        let q = c
            .transition_runtime_queue(
                chat.id().into(),
                a.clone(),
                q.revision(),
                QueueState::Queued,
                QueueState::Attempting,
            )
            .await
            .unwrap();
        let q = c
            .transition_runtime_queue(
                chat.id().into(),
                a.clone(),
                q.revision(),
                QueueState::Attempting,
                QueueState::DeliveryUnknown,
            )
            .await
            .unwrap();
        let resolution = M::ResolveUnknown {
            message: a.clone(),
            resolution: R::Dismissed,
        };
        c.mutate_runtime_queue(
            resolution
                .command(CommandKey::local(id()).unwrap(), chat.id(), q.revision())
                .unwrap(),
            chat.id().into(),
            q.revision(),
            resolution,
        )
        .await
        .unwrap();
        let q = c.load_runtime_queue(chat.id().into()).await.unwrap();
        let dismissed = q.entries().iter().find(|entry| entry.id() == a).unwrap();
        assert_eq!(dismissed.state(), QueueState::Dismissed);
        assert_eq!(dismissed.draft().text(), "edited");
        assert!(dismissed.resolved_by_user());
        c.shutdown().await.unwrap();
    });
}

#[test]
fn results_obey_per_device_byte_budget_without_blocking_other_devices_or_gc() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let c = SettingsClient::create(&path).await.unwrap();
        c.initialize_runtime().await.unwrap();
        let d = device(&c).await;
        let k = CommandKey::remote(d.clone(), 1, id()).unwrap();
        c.accept_runtime_command(operation(k.clone()), 0)
            .await
            .unwrap();
        c.transition_runtime_command(
            k.clone(),
            CommandState::Accepted,
            CommandState::Attempting,
            None,
        )
        .await
        .unwrap();
        let db = Connection::open(&path).unwrap();
        // Seed near the cap without sending 64 MiB through the worker repeatedly.
        db.execute(
            "UPDATE _canopy_command_inbox SET arguments=zeroblob(?1) WHERE device_id=?2",
            rusqlite::params![canopy_desktop::host::REMOTE_INBOX_BYTES as i64 - 1024, d],
        )
        .unwrap();
        assert!(matches!(
            c.transition_runtime_command(
                k.clone(),
                CommandState::Attempting,
                CommandState::Succeeded,
                Some(vec![0; 2048])
            )
            .await,
            Err(Error::ResourceLimit)
        ));
        assert_eq!(
            c.get_runtime_command(k.clone(), 0)
                .await
                .unwrap()
                .unwrap()
                .state(),
            CommandState::Attempting
        );
        assert!(matches!(
            c.accept_runtime_command(
                operation(CommandKey::remote(d.clone(), 1, id()).unwrap()),
                0
            )
            .await,
            Err(Error::ResourceLimit)
        ));
        let other = device(&c).await;
        c.accept_runtime_command(operation(CommandKey::remote(other, 1, id()).unwrap()), 0)
            .await
            .unwrap();
        c.transition_runtime_command(
            k.clone(),
            CommandState::Attempting,
            CommandState::DeliveryUnknown,
            None,
        )
        .await
        .unwrap();
        c.close_runtime_command_epoch(d.clone(), 1).await.unwrap();
        c.resolve_runtime_command(k.clone()).await.unwrap();
        c.close_runtime_command_epoch(d, 2).await.unwrap();
        assert!(matches!(
            c.get_runtime_command(k, 0).await,
            Err(Error::CommandRetired)
        ));
        let local = CommandKey::local(id()).unwrap();
        c.accept_runtime_command(operation(local.clone()), 0)
            .await
            .unwrap();
        assert!(matches!(
            c.acknowledge_runtime_command(local.clone()).await,
            Err(Error::OperationPending)
        ));
        c.transition_runtime_command(
            local.clone(),
            CommandState::Accepted,
            CommandState::Attempting,
            None,
        )
        .await
        .unwrap();
        c.transition_runtime_command(
            local.clone(),
            CommandState::Attempting,
            CommandState::DeliveryUnknown,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(
            c.acknowledge_runtime_command(local.clone()).await,
            Err(Error::OperationPending)
        ));
        c.resolve_runtime_command(local.clone()).await.unwrap();
        c.acknowledge_runtime_command(local.clone()).await.unwrap();
        assert!(c.get_runtime_command(local, 0).await.unwrap().is_none());
        c.shutdown().await.unwrap();
    });
}

#[test]
fn orphan_history_survives_real_retention_and_explicit_close_then_expires() {
    use canopy_desktop::chat::{
        closed::ClosedChat,
        history::{AgentRef, EpochMeta},
    };
    use canopy_desktop::settings::{ChatHistoryBatch, ClosedChatRow};
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let c = SettingsClient::create(&path).await.unwrap();
        let orphan = id();
        let meta = EpochMeta {
            agent: Some(AgentRef {
                tool: "codex".into(),
                profile: Some("original-account".into()),
                session: Some("original-session".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        c.save_chat_history(vec![ChatHistoryBatch {
            chat: orphan.clone(),
            epochs: vec![(0, serde_json::to_string(&meta).unwrap())],
            items: vec![(0, 1, "opaque old item".into())],
        }])
        .await
        .unwrap();
        c.initialize_runtime().await.unwrap();
        let records = c.load_runtime_chats().await.unwrap();
        let record = records.iter().find(|r| r.id() == orphan).unwrap();
        assert_eq!(
            record.metadata().profile_id.as_deref(),
            Some("original-account")
        );
        assert!(record.recovery_reason().is_some());
        assert!(
            c.retain_chat_history(vec![], i64::MAX)
                .await
                .unwrap()
                .is_empty()
        );
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT payload FROM _canopy_rust_chat_items WHERE chat=?1",
                [&orphan],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "opaque old item"
        );
        let closed = ClosedChat {
            title: "recovered".into(),
            tool: "codex".into(),
            metadata: record.metadata().clone(),
            project: None,
        };
        c.save_closed_chat(ClosedChatRow {
            chat: orphan.clone(),
            closed_at: 100,
            payload: closed.to_json().unwrap(),
        })
        .await
        .unwrap();
        assert!(c.load_runtime_chats().await.unwrap().is_empty());
        assert!(c.retain_chat_history(vec![], 50).await.unwrap().is_empty());
        assert_eq!(
            c.retain_chat_history(vec![], 101).await.unwrap(),
            vec![orphan]
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM _canopy_runtime_chats", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.shutdown().await.unwrap();
    });
}

#[test]
fn offline_send_checks_account_and_session_context_without_invalidating_an_accepted_retry() {
    block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("context.db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.initialize_runtime().await.unwrap();
        let original = chat();
        client.save_runtime_chat(original.clone()).await.unwrap();
        let device = device(&client).await;
        let key = CommandKey::remote(device.clone(), 1, id()).unwrap();
        let message = id();
        let draft = QueueDraft::from(Draft::text("written before changing account"));
        let (stale, payload) = draft
            .clone()
            .command_for_message(key, original.id(), message.clone(), original.send_context())
            .unwrap();
        let mut metadata = original.metadata().clone();
        metadata.profile_id = Some(id());
        let changed =
            ChatRecord::new(original.id().into(), None, "claude".into(), metadata).unwrap();
        client.save_runtime_chat(changed.clone()).await.unwrap();
        assert!(matches!(
            client
                .accept_runtime_message(
                    stale.clone(),
                    original.id().into(),
                    message.clone(),
                    payload
                )
                .await,
            Err(Error::ResourceConflict)
        ));
        assert!(
            client
                .get_runtime_command(stale.key().clone(), 0)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            client
                .load_runtime_queue(original.id().into())
                .await
                .unwrap()
                .entries()
                .is_empty()
        );
        let key = CommandKey::remote(device.clone(), 1, id()).unwrap();
        let (accepted, payload) = draft
            .clone()
            .command_for_message(
                key.clone(),
                original.id(),
                message.clone(),
                changed.send_context(),
            )
            .unwrap();
        let receipt = client
            .accept_runtime_message(
                accepted.clone(),
                original.id().into(),
                message.clone(),
                payload.clone(),
            )
            .await
            .unwrap();
        // A session boundary changes send context, while ordinary text deltas do not.
        client
            .save_chat_history(vec![canopy_desktop::settings::ChatHistoryBatch {
                chat: original.id().into(),
                epochs: vec![(1, "{}".into())],
                items: vec![],
            }])
            .await
            .unwrap();
        assert_eq!(
            client
                .accept_runtime_message(accepted, original.id().into(), message.clone(), payload)
                .await
                .unwrap(),
            receipt
        );
        let different_message = id();
        let (different_id, payload) = draft
            .clone()
            .command_for_message(
                key,
                original.id(),
                different_message.clone(),
                changed.send_context(),
            )
            .unwrap();
        assert!(matches!(
            client
                .accept_runtime_message(
                    different_id,
                    original.id().into(),
                    different_message,
                    payload
                )
                .await,
            Err(Error::CommandConflict)
        ));
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        client.initialize_runtime().await.unwrap();
        let (new, payload) = draft
            .command_for_message(
                CommandKey::remote(device, 1, id()).unwrap(),
                original.id(),
                message.clone(),
                changed.send_context(),
            )
            .unwrap();
        assert!(matches!(
            client
                .accept_runtime_message(new, original.id().into(), message, payload)
                .await,
            Err(Error::ResourceConflict)
        ));
        assert_eq!(
            client
                .load_runtime_queue(original.id().into())
                .await
                .unwrap()
                .entries()
                .len(),
            1
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn steer_intent_binds_run_and_restart_or_explicit_resend_cannot_rearm_it() {
    use canopy_desktop::host::{QueueMutation, UnknownResolution};
    use sha2::{Digest, Sha256};
    #[derive(serde::Serialize)]
    struct PreviousSendIntent<'a> {
        message_id: &'a str,
        draft: &'a QueueDraft,
        context: &'a SendContext,
    }
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let c = SettingsClient::create(&path).await.unwrap();
        c.initialize_runtime().await.unwrap();
        let ordinary = chat();
        c.save_runtime_chat(ordinary.clone()).await.unwrap();
        let key = CommandKey::local(id()).unwrap();
        let message = id();
        let draft = QueueDraft::from(Draft::text("unchanged ordinary send"));
        let context = ordinary.send_context();
        // Simulate a previously encoded Send receipt; the new optional field
        // must not turn an identical retry into an idempotency conflict.
        let arguments = serde_json::to_vec(&PreviousSendIntent {
            message_id: &message,
            draft: &draft,
            context: &context,
        })
        .unwrap();
        let fingerprint = Sha256::digest(&arguments).to_vec();
        let previous = CommandInput::new(
            key.clone(),
            "chat.send".into(),
            ordinary.id().into(),
            arguments,
            fingerprint,
        )
        .unwrap();
        c.accept_runtime_message(
            previous,
            ordinary.id().into(),
            message.clone(),
            draft.clone(),
        )
        .await
        .unwrap();
        let (retry, same) = draft
            .clone()
            .command_for_message(key.clone(), ordinary.id(), message.clone(), context.clone())
            .unwrap();
        c.accept_runtime_message(retry, ordinary.id().into(), message.clone(), same)
            .await
            .unwrap();
        let (changed, same) = draft
            .command_for_steer(key, ordinary.id(), message.clone(), context, 11)
            .unwrap();
        assert!(matches!(
            c.accept_runtime_message(changed, ordinary.id().into(), message, same)
                .await,
            Err(Error::CommandConflict)
        ));

        let steered = chat();
        c.save_runtime_chat(steered.clone()).await.unwrap();
        let key = CommandKey::local(id()).unwrap();
        let message = id();
        let draft = QueueDraft::from(Draft::text("durable new steer text"));
        let (input, same) = draft
            .clone()
            .command_for_steer(
                key.clone(),
                steered.id(),
                message.clone(),
                steered.send_context(),
                11,
            )
            .unwrap();
        c.accept_runtime_message(input, steered.id().into(), message.clone(), same)
            .await
            .unwrap();
        let (changed, same) = draft
            .command_for_steer(
                key,
                steered.id(),
                message.clone(),
                steered.send_context(),
                12,
            )
            .unwrap();
        assert!(matches!(
            c.accept_runtime_message(changed, steered.id().into(), message.clone(), same)
                .await,
            Err(Error::CommandConflict)
        ));
        let queue = c.load_runtime_queue(steered.id().into()).await.unwrap();
        assert_eq!(queue.entries()[0].steer_run(), Some(11));
        let queue = c
            .set_runtime_queue_paused(steered.id().into(), queue.revision(), false)
            .await
            .unwrap();
        let queue = c
            .transition_runtime_queue(
                steered.id().into(),
                message.clone(),
                queue.revision(),
                QueueState::Queued,
                QueueState::Attempting,
            )
            .await
            .unwrap();
        let queue = c
            .transition_runtime_queue(
                steered.id().into(),
                message.clone(),
                queue.revision(),
                QueueState::Attempting,
                QueueState::DeliveryUnknown,
            )
            .await
            .unwrap();
        let new_message = id();
        let resend = QueueMutation::ResolveUnknown {
            message,
            resolution: UnknownResolution::Resend {
                message: new_message.clone(),
            },
        };
        let command = resend
            .command(
                CommandKey::local(id()).unwrap(),
                steered.id(),
                queue.revision(),
            )
            .unwrap();
        c.mutate_runtime_queue(command, steered.id().into(), queue.revision(), resend)
            .await
            .unwrap();
        let queue = c.load_runtime_queue(steered.id().into()).await.unwrap();
        assert!(
            queue
                .entries()
                .iter()
                .find(|e| e.id() == new_message)
                .unwrap()
                .steer_run()
                .is_none()
        );

        let held = chat();
        c.save_runtime_chat(held.clone()).await.unwrap();
        let message = id();
        let (input, draft) = QueueDraft::from(Draft::text("held across restart"))
            .command_for_steer(
                CommandKey::local(id()).unwrap(),
                held.id(),
                message.clone(),
                held.send_context(),
                11,
            )
            .unwrap();
        c.accept_runtime_message(
            input.clone(),
            held.id().into(),
            message.clone(),
            draft.clone(),
        )
        .await
        .unwrap();
        c.shutdown().await.unwrap();
        let c = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        c.initialize_runtime().await.unwrap();
        c.accept_runtime_message(input, held.id().into(), message, draft)
            .await
            .unwrap();
        let queue = c.load_runtime_queue(held.id().into()).await.unwrap();
        assert!(queue.paused());
        assert_eq!(queue.entries().len(), 1);
        assert_eq!(queue.entries()[0].state(), QueueState::Queued);
        assert_eq!(queue.entries()[0].draft().text(), "held across restart");
        assert!(queue.entries()[0].steer_run().is_none());
        c.shutdown().await.unwrap();
    });
}
