use super::*;
use crate::{
    chat::config::ChatSettings,
    command_bus::{Conditions, DraftIntent, QueueChange},
    state::workspace::{PaneKind, PaneMetadata},
};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commands.db");
    crate::settings::database::create(&path).unwrap();
    let mut db = Database::open(&path, Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    (dir, db)
}
fn chat(db: &mut Database) -> ChatRecord {
    let id = id();
    let record = ChatRecord::new(
        id.clone(),
        None,
        "test".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            chat: Some(Box::new(ChatSettings {
                id: Some(id),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap();
    db.save_runtime_chat(record).unwrap()
}
fn principal(db: &mut Database) -> (String, Principal) {
    let device = id();
    db.register_runtime_device(
        DeviceRegistration::new(
            device.clone(),
            "Device".into(),
            device.as_bytes().into(),
            host::ALL_SCOPES,
            vec!["*".into()],
        )
        .unwrap(),
    )
    .unwrap();
    let grant = db.lookup_runtime_device(device.as_bytes().into()).unwrap();
    (device, Principal::authenticated(&grant, 1).unwrap())
}
fn envelope(chat: &ChatRecord, command_id: uuid::Uuid, command: Command) -> CommandEnvelope {
    let thread = chat.thread().unwrap();
    CommandEnvelope {
        command_id,
        resource: ResourceRef::Thread {
            thread: thread.thread().clone(),
            executor_epoch: thread.executor_epoch(),
        },
        command,
        conditions: Conditions::default(),
    }
}
fn rename(chat: &ChatRecord, command_id: uuid::Uuid, title: &str) -> CommandEnvelope {
    envelope(
        chat,
        command_id,
        Command::Rename {
            title: title.into(),
        },
    )
}
fn execute(
    db: &mut Database,
    envelope: CommandEnvelope,
    principal: Principal,
) -> Result<CommandReceipt> {
    let prepared = db.prepare_bus_command(envelope, principal)?;
    if let Some(prior) = prepared.prior {
        return Ok(prior);
    }
    let dispatch = prepared.resolve(None)?;
    db.dispatch_bus_command(prepared, dispatch)
}
fn send(chat: &ChatRecord, message: String) -> Command {
    Command::Send {
        message,
        draft: incoming("original"),
        context: chat.send_context(),
        steer_run: None,
    }
}
fn incoming(text: &str) -> DraftIntent {
    DraftIntent::Incoming {
        text: text.into(),
        context: None,
        goal: false,
        attachments: vec![],
    }
}
fn preferences(
    db: &Database,
    command_id: uuid::Uuid,
    value: bool,
    revision: Option<u64>,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id,
        resource: ResourceRef::Environment(crate::thread::EnvironmentRef {
            host_id: HostId::new(db.runtime_host().unwrap().id).unwrap(),
        }),
        command: Command::preferences(vec![crate::settings::Change::NotchEnabled(value)]).unwrap(),
        conditions: Conditions {
            expected_revision: revision,
            ..Default::default()
        },
    }
}

#[test]
fn bus_retry_reopens_retains_deleted_target_and_normalizes_before_fingerprinting() {
    let (dir, mut db) = database();
    let chat = chat(&mut db);
    let (_device, principal) = principal(&mut db);
    let command_id = uuid::Uuid::new_v4();
    let original = rename(&chat, command_id, "  Committed name  ");
    let receipt = execute(&mut db, original.clone(), principal.clone()).unwrap();
    let saved = db.load_runtime_chat(chat.id.clone()).unwrap().unwrap();
    assert_eq!(saved.metadata.title.as_deref(), Some("Committed name"));
    assert!(saved.thread().unwrap().revision() > chat.thread().unwrap().revision());
    // A delayed normal runtime save cannot undo a bus-owned explicit name.
    let late = db.save_runtime_chat(chat.clone()).unwrap();
    assert_eq!(late.metadata.title.as_deref(), Some("Committed name"));
    assert_eq!(
        execute(
            &mut db,
            rename(&chat, command_id, "Committed name"),
            principal.clone()
        )
        .unwrap(),
        receipt
    );
    let other_method = envelope(
        &chat,
        command_id,
        Command::Queue {
            revision: 0,
            change: QueueChange::Pause { paused: false },
        },
    );
    assert!(matches!(
        execute(&mut db, other_method, principal.clone()),
        Err(Error::CommandConflict)
    ));
    drop(db);
    let mut db = Database::open(&dir.path().join("commands.db"), Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    assert_eq!(
        execute(&mut db, original.clone(), principal.clone()).unwrap(),
        receipt
    );
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [chat.id()],
        )
        .unwrap();
    let alias = db
        .runtime_thread_alias(
            chat.thread().unwrap().thread().host_id.clone(),
            chat.id.clone(),
        )
        .unwrap();
    assert_eq!(alias, chat.thread_id());
    assert_eq!(
        execute(&mut db, original.clone(), principal.clone()).unwrap(),
        receipt
    );
    let mut changed_conditions = original;
    changed_conditions.conditions.expected_revision = Some(1);
    assert!(matches!(
        execute(&mut db, changed_conditions, principal),
        Err(Error::CommandConflict)
    ));
}

#[test]
fn current_authority_precedes_dedupe_and_conflict_and_dispatch_rechecks_it() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let (device, principal) = principal(&mut db);
    let command_id = uuid::Uuid::new_v4();
    let original = rename(&chat, command_id, "original");
    execute(&mut db, original.clone(), principal.clone()).unwrap();
    let pending = db
        .prepare_bus_command(
            rename(&chat, uuid::Uuid::new_v4(), "must not run"),
            principal.clone(),
        )
        .unwrap();
    let dispatch = pending.resolve(None).unwrap();
    db.update_runtime_device_authorization(device.clone(), 1, host::READ, vec!["*".into()])
        .unwrap();
    for intent in [original.clone(), rename(&chat, command_id, "different")] {
        assert!(matches!(
            execute(&mut db, intent, principal.clone()),
            Err(Error::NotAuthorized)
        ));
    }
    assert!(matches!(
        db.dispatch_bus_command(pending, dispatch),
        Err(Error::NotAuthorized)
    ));
    assert_eq!(
        db.load_runtime_chat(chat.id.clone())
            .unwrap()
            .unwrap()
            .metadata
            .title
            .as_deref(),
        Some("original")
    );
    db.revoke_runtime_device(device).unwrap();
    assert!(matches!(
        execute(&mut db, original, principal),
        Err(Error::NotAuthorized)
    ));
}

#[test]
fn composite_identity_isolates_threads_executors_and_principals_and_legacy_is_ambiguous() {
    let (_dir, mut db) = database();
    let first = chat(&mut db);
    let second = chat(&mut db);
    let (device, principal) = principal(&mut db);
    let (_other_device, other) = self::principal(&mut db);
    let command_id = uuid::Uuid::new_v4();
    let a = execute(
        &mut db,
        rename(&first, command_id, "first"),
        principal.clone(),
    )
    .unwrap();
    let b = execute(
        &mut db,
        rename(&second, command_id, "second"),
        principal.clone(),
    )
    .unwrap();
    let c = execute(
        &mut db,
        rename(&first, command_id, "other principal"),
        other,
    )
    .unwrap();
    db.db
        .execute(
            "UPDATE _canopy_runtime_chats SET executor_epoch=1 WHERE chat_id=?1",
            [first.id()],
        )
        .unwrap();
    let current = db.load_runtime_chat(first.id.clone()).unwrap().unwrap();
    let d = execute(
        &mut db,
        rename(&current, command_id, "next executor"),
        principal.clone(),
    )
    .unwrap();
    assert_ne!(a.key(), b.key());
    assert_ne!(a.key(), c.key());
    assert_ne!(a.key(), d.key());
    assert_eq!(
        execute(
            &mut db,
            rename(&first, command_id, "first"),
            principal.clone(),
        )
        .unwrap(),
        a
    );
    assert!(matches!(
        execute(
            &mut db,
            rename(&first, command_id, "changed old intent"),
            principal.clone(),
        ),
        Err(Error::CommandConflict)
    ));
    assert_eq!(
        db.get_runtime_command(a.key.clone(), 0).unwrap(),
        Some(a.clone())
    );
    let legacy = CommandKey::remote(device.clone(), 1, command_id.to_string()).unwrap();
    assert!(matches!(
        db.get_runtime_command(legacy.clone(), 0),
        Err(Error::AmbiguousCommand)
    ));
    assert!(matches!(
        db.resolve_runtime_command(legacy),
        Err(Error::AmbiguousCommand)
    ));
    // State transitions and resolution affect exactly one full key.
    db.db.execute("UPDATE _canopy_command_inbox SET state='accepted' WHERE resource=?1 AND executor_epoch=?2 AND device_id=?3 AND command_id=?4",params![d.key.resource,1,device,command_id.to_string()]).unwrap();
    db.transition_runtime_command(
        d.key.clone(),
        CommandState::Accepted,
        CommandState::Attempting,
        None,
    )
    .unwrap();
    db.transition_runtime_command(
        d.key.clone(),
        CommandState::Attempting,
        CommandState::DeliveryUnknown,
        None,
    )
    .unwrap();
    db.resolve_runtime_command(d.key.clone()).unwrap();
    assert_eq!(db.get_runtime_command(b.key.clone(), 0).unwrap(), Some(b));
    assert_eq!(db.get_runtime_command(a.key.clone(), 0).unwrap(), Some(a));
    assert!(
        db.get_runtime_command(d.key, 0)
            .unwrap()
            .unwrap()
            .resolved_by_user()
    );
    assert!(matches!(
        execute(
            &mut db,
            rename(&first, uuid::Uuid::new_v4(), "stale executor"),
            principal
        ),
        Err(Error::ResourceConflict)
    ));
}

#[test]
fn local_acknowledgement_and_authorization_updates_use_the_full_key() {
    let (_dir, mut db) = database();
    let first = chat(&mut db);
    let mut second = chat(&mut db);
    second.workspace = Some("project".into());
    second.authority_root = "/project".into();
    second = db.save_runtime_chat(second).unwrap();
    let command_id = uuid::Uuid::new_v4();
    let a = execute(
        &mut db,
        rename(&first, command_id, "first"),
        Principal::local_owner(),
    )
    .unwrap();
    let b = execute(
        &mut db,
        rename(&second, command_id, "second"),
        Principal::local_owner(),
    )
    .unwrap();
    db.acknowledge_runtime_command(a.key.clone()).unwrap();
    assert!(receipt(&db.db, &a.key).unwrap().is_none());
    assert_eq!(receipt(&db.db, &b.key).unwrap(), Some(b));
    let (device, principal) = principal(&mut db);
    let a = execute(&mut db, rename(&first, command_id, "a"), principal.clone()).unwrap();
    let b = execute(&mut db, rename(&second, command_id, "b"), principal).unwrap();
    db.db
        .execute(
            "UPDATE _canopy_command_inbox SET state='accepted' WHERE device_id=?1",
            [&device],
        )
        .unwrap();
    db.update_runtime_device_authorization(device, 1, host::ALL_SCOPES, vec!["private".into()])
        .unwrap();
    assert_eq!(
        receipt(&db.db, &a.key).unwrap().unwrap().state(),
        CommandState::Accepted
    );
    assert_eq!(
        receipt(&db.db, &b.key).unwrap().unwrap().state(),
        CommandState::Rejected
    );
}

#[test]
fn failed_receipt_commit_rolls_back_rename_send_queue_and_preferences() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let old_queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    db.db.execute_batch("CREATE TRIGGER fail_bus BEFORE INSERT ON _canopy_command_inbox BEGIN SELECT RAISE(ABORT,'receipt failure'); END;").unwrap();
    for command in [
        Command::Rename {
            title: "not committed".into(),
        },
        send(&chat, id()),
        Command::Queue {
            revision: old_queue.revision(),
            change: QueueChange::Pause { paused: false },
        },
    ] {
        assert!(
            execute(
                &mut db,
                envelope(&chat, uuid::Uuid::new_v4(), command),
                Principal::local_owner()
            )
            .is_err()
        );
    }
    assert_eq!(
        db.load_runtime_chat(chat.id.clone()).unwrap(),
        Some(chat.clone())
    );
    assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), old_queue);
    let prefs = preferences(&db, uuid::Uuid::new_v4(), false, Some(42));
    let prepared = db
        .prepare_bus_command(prefs, Principal::local_owner())
        .unwrap();
    assert!(db.commit_bus_preferences(prepared, 42).is_err());
    assert!(db.load().unwrap().preferences.notch_enabled);
    assert_eq!(
        db.db
            .query_row("SELECT count(*) FROM _canopy_command_inbox", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn preferences_retry_returns_original_committed_values_and_revision_after_later_writes() {
    let (dir, mut db) = database();
    let (_device, principal) = principal(&mut db);
    let original = preferences(&db, uuid::Uuid::new_v4(), false, Some(42));
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    let (first, snapshot) = db.commit_bus_preferences(prepared, 42).unwrap();
    assert!(!snapshot.unwrap().preferences.notch_enabled);
    let result: serde_json::Value = serde_json::from_slice(first.result().unwrap()).unwrap();
    assert_eq!(result["values"]["notch_enabled"], false);
    assert_eq!(result["revision"], 43);
    let later = preferences(&db, uuid::Uuid::new_v4(), true, Some(43));
    let prepared = db.prepare_bus_command(later, principal.clone()).unwrap();
    let (second, _) = db.commit_bus_preferences(prepared, 43).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(second.result().unwrap()).unwrap()["revision"],
        44
    );
    let stale = preferences(&db, uuid::Uuid::new_v4(), false, Some(42));
    let prepared = db.prepare_bus_command(stale, principal.clone()).unwrap();
    let (rejected, snapshot) = db.commit_bus_preferences(prepared, 44).unwrap();
    assert_eq!(rejected.state(), CommandState::Rejected);
    assert!(snapshot.is_none());
    assert!(db.load().unwrap().preferences.notch_enabled);
    drop(db);
    let mut db = Database::open(&dir.path().join("commands.db"), Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    let (retry, snapshot) = db.commit_bus_preferences(prepared, 1000).unwrap();
    assert_eq!(retry, first);
    assert!(snapshot.is_none());
    let mut changed = original;
    changed.conditions.expected_revision = Some(43);
    assert!(matches!(
        db.prepare_bus_command(changed, principal),
        Err(Error::CommandConflict)
    ));
    assert!(db.load().unwrap().preferences.notch_enabled);
}

#[test]
fn send_dedupe_precedes_context_and_deleted_target_and_pause_is_not_undone() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let (_device, principal) = principal(&mut db);
    let message = id();
    let original = envelope(&chat, uuid::Uuid::new_v4(), send(&chat, message.clone()));
    let first = execute(&mut db, original.clone(), principal.clone()).unwrap();
    let queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    assert!(!queue.paused());
    assert_eq!(queue.entries().len(), 1);
    let pause = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Queue {
            revision: queue.revision(),
            change: QueueChange::Pause { paused: true },
        },
    );
    execute(&mut db, pause, principal.clone()).unwrap();
    let paused = db.load_runtime_queue(chat.id.clone()).unwrap();
    assert_eq!(
        execute(&mut db, original.clone(), principal.clone()).unwrap(),
        first
    );
    assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), paused);
    let mut changed = original.clone();
    if let Command::Send { context, .. } = &mut changed.command {
        context.history_epoch += 1;
    }
    assert!(matches!(
        execute(&mut db, changed, principal.clone()),
        Err(Error::CommandConflict)
    ));
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [chat.id()],
        )
        .unwrap();
    assert_eq!(execute(&mut db, original, principal).unwrap(), first);
}

#[test]
fn incoming_retry_precedes_vanished_workspace_and_rotated_context_and_checks_read_scope() {
    use crate::command_bus::{AttachmentIntent, DraftResources, PullIntent};
    let (_dir, mut db) = database();
    let mut chat = chat(&mut db);
    chat.workspace = Some("project".into());
    chat.authority_root = "/project".into();
    chat = db.save_runtime_chat(chat).unwrap();
    let (device, principal) = principal(&mut db);
    let resources = DraftResources {
        roots: Default::default(),
        pulls: Default::default(),
    };
    resources
        .roots
        .replace([("project".into(), "/project".into())].into())
        .unwrap();
    let pull = resources.pulls.publish(
        "/project",
        crate::remote::proto::PullAttachment {
            number: 42,
            title: "context".into(),
            ..Default::default()
        },
    );
    let original = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Send {
            message: id(),
            context: chat.send_context(),
            steer_run: None,
            draft: DraftIntent::Incoming {
                text: "with references".into(),
                context: None,
                goal: false,
                attachments: vec![
                    AttachmentIntent::Path {
                        workspace: Some((
                            chat.thread().unwrap().thread().host_id.clone(),
                            "project".into(),
                        )),
                        relative: "src/file.rs".into(),
                        label: None,
                    },
                    AttachmentIntent::Pull {
                        label: None,
                        value: PullIntent {
                            number: pull.number,
                            title: pull.title,
                            url: pull.url,
                            head: pull.head,
                            base: pull.base,
                            state: pull.state,
                            body: pull.body,
                            context_id: pull.context_id,
                        },
                    },
                ],
            },
        },
    );
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    let dispatch = prepared.resolve(Some(&resources)).unwrap();
    let receipt = db.dispatch_bus_command(prepared, dispatch).unwrap();
    // A fresh request still resolves its references; a retry never needs them again.
    resources.roots.replace(Default::default()).unwrap();
    let unavailable = DraftResources {
        roots: resources.roots,
        pulls: Default::default(),
    };
    let prepared = db
        .prepare_bus_command(original.clone(), principal.clone())
        .unwrap();
    assert_eq!(prepared.prior(), Some(&receipt));
    let mut new_request = original.clone();
    new_request.command_id = uuid::Uuid::new_v4();
    let prepared = db
        .prepare_bus_command(new_request.clone(), principal.clone())
        .unwrap();
    assert!(prepared.resolve(Some(&unavailable)).is_err());
    unavailable
        .roots
        .replace([("project".into(), "/project".into())].into())
        .unwrap();
    assert!(prepared.resolve(Some(&unavailable)).is_err());
    assert_eq!(
        db.load_runtime_queue(chat.id.clone())
            .unwrap()
            .entries()
            .len(),
        1
    );
    db.update_runtime_device_authorization(device, 1, host::CHAT, vec!["*".into()])
        .unwrap();
    assert!(matches!(
        db.prepare_bus_command(original, principal.clone()),
        Err(Error::NotAuthorized)
    ));
    assert!(matches!(
        db.get_runtime_command(receipt.key.clone(), 0),
        Err(Error::NotAuthorized)
    ));
    assert!(matches!(
        db.prepare_bus_command(new_request, principal),
        Err(Error::NotAuthorized)
    ));
}

#[test]
fn every_queue_change_is_durable_deduped_and_revision_checked() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let (_device, principal) = principal(&mut db);
    let first = id();
    let second = id();
    let resent = id();
    let before = db.load_runtime_queue(chat.id.clone()).unwrap();
    let invalid = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Queue {
            revision: before.revision(),
            change: QueueChange::Remove { message: id() },
        },
    );
    let rejected = execute(&mut db, invalid.clone(), principal.clone()).unwrap();
    assert_eq!(rejected.state(), CommandState::Rejected);
    // Validation failed after its tentative queue revision increment.
    assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), before);
    assert_eq!(
        execute(&mut db, invalid, principal.clone()).unwrap(),
        rejected
    );
    for message in [&first, &second] {
        execute(
            &mut db,
            envelope(&chat, uuid::Uuid::new_v4(), send(&chat, message.clone())),
            principal.clone(),
        )
        .unwrap();
    }
    let changes = [
        QueueChange::Edit {
            message: first.clone(),
            draft: incoming("edited"),
        },
        QueueChange::Move {
            message: second.clone(),
            before: Some(first.clone()),
        },
        QueueChange::Pause { paused: true },
        QueueChange::Remove {
            message: second.clone(),
        },
    ];
    for change in changes {
        let before = db.load_runtime_queue(chat.id.clone()).unwrap();
        let request = envelope(
            &chat,
            uuid::Uuid::new_v4(),
            Command::Queue {
                revision: before.revision(),
                change,
            },
        );
        let receipt = execute(&mut db, request.clone(), principal.clone()).unwrap();
        let after = db.load_runtime_queue(chat.id.clone()).unwrap();
        assert_eq!(after.revision(), before.revision() + 1);
        assert_eq!(
            execute(&mut db, request.clone(), principal.clone()).unwrap(),
            receipt
        );
        assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), after);
        let mut stale = request;
        stale.command_id = uuid::Uuid::new_v4();
        let rejected = execute(&mut db, stale.clone(), principal.clone()).unwrap();
        assert_eq!(rejected.state(), CommandState::Rejected);
        assert_eq!(
            execute(&mut db, stale, principal.clone()).unwrap(),
            rejected
        );
        assert_eq!(db.load_runtime_queue(chat.id.clone()).unwrap(), after);
    }
    let queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    assert!(queue.paused());
    assert_eq!(queue.entries()[0].id(), second);
    assert_eq!(queue.entries()[0].state(), host::QueueState::Dismissed);
    assert_eq!(queue.entries()[1].draft().text(), "edited");
    // The existing delivery owner fences attempting; restart creates uncertainty.
    let queue = db
        .set_runtime_queue_paused(chat.id.clone(), queue.revision(), false)
        .unwrap();
    let queue = db
        .transition_runtime_queue(
            chat.id.clone(),
            first.clone(),
            queue.revision(),
            host::QueueState::Queued,
            host::QueueState::Attempting,
        )
        .unwrap();
    db.transition_runtime_queue(
        chat.id.clone(),
        first.clone(),
        queue.revision(),
        host::QueueState::Attempting,
        host::QueueState::DeliveryUnknown,
    )
    .unwrap();
    let queue = db.load_runtime_queue(chat.id.clone()).unwrap();
    let request = envelope(
        &chat,
        uuid::Uuid::new_v4(),
        Command::Queue {
            revision: queue.revision(),
            change: QueueChange::ResolveUnknown {
                message: first,
                resolution: UnknownResolution::Resend {
                    message: resent.clone(),
                },
            },
        },
    );
    let receipt = execute(&mut db, request.clone(), principal.clone()).unwrap();
    assert_eq!(execute(&mut db, request, principal).unwrap(), receipt);
    let queue = db.load_runtime_queue(chat.id).unwrap();
    assert_eq!(
        queue
            .entries()
            .iter()
            .filter(|entry| entry.id() == resent)
            .count(),
        1
    );
    assert_eq!(
        queue
            .entries()
            .iter()
            .filter(|entry| entry.state() == host::QueueState::Queued)
            .count(),
        1
    );
}

#[test]
fn wrong_host_target_and_conditions_never_accept_an_effect() {
    let (_dir, mut db) = database();
    let chat = chat(&mut db);
    let mut wrong_host = rename(&chat, uuid::Uuid::new_v4(), "wrong host");
    if let ResourceRef::Thread { thread, .. } = &mut wrong_host.resource {
        thread.host_id = HostId::new(id()).unwrap();
    }
    assert!(matches!(
        execute(&mut db, wrong_host, Principal::local_owner()),
        Err(Error::NotFound)
    ));
    let mut stale = rename(&chat, uuid::Uuid::new_v4(), "stale");
    stale.conditions.expected_revision = Some(0);
    assert_eq!(
        execute(&mut db, stale, Principal::local_owner())
            .unwrap()
            .state(),
        CommandState::Rejected
    );
    assert_eq!(db.load_runtime_chat(chat.id.clone()).unwrap(), Some(chat));
}
