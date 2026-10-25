use super::*;
use crate::{
    artifacts::{BeginUpload, Purpose, Target},
    chat::{config::ChatSettings, session::Draft},
    host::SendContext,
    state::workspace::{PaneKind, PaneMetadata},
};
use sha2::{Digest, Sha256};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.db");
    crate::settings::database::create(&path).unwrap();
    let mut db = Database::open(&path, Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    (dir, db)
}
fn record() -> ChatRecord {
    let chat = id();
    ChatRecord::new(
        chat.clone(),
        None,
        "claude".into(),
        PaneMetadata {
            kind: PaneKind::Chat,
            chat: Some(Box::new(ChatSettings {
                id: Some(chat),
                ..Default::default()
            })),
            ..Default::default()
        },
    )
    .unwrap()
}
fn device(db: &mut Database) -> String {
    let device = id();
    db.register_runtime_device(
        DeviceRegistration::new(
            device.clone(),
            "Phone".into(),
            device.as_bytes().to_vec(),
            3,
            vec!["*".into()],
        )
        .unwrap(),
    )
    .unwrap();
    device
}
fn command(key: CommandKey, method: &str, target: &str, value: serde_json::Value) -> CommandInput {
    let bytes = serde_json::to_vec(&value).unwrap();
    let fingerprint = Sha256::digest(&bytes).to_vec();
    CommandInput::new(key, method.into(), target.into(), bytes, fingerprint)
        .unwrap()
        .with_root("private".into())
        .unwrap()
}
fn image(db: &mut Database, owner: &str, chat: &str) -> String {
    let image = id();
    let binding = BeginUpload::new(
        owner.into(),
        Target::Chat(chat.into()),
        Purpose::Image,
        3,
        Sha256::digest(b"img").into(),
        "image/png".into(),
    )
    .unwrap();
    db.begin_artifact_upload(image.clone(), binding).unwrap();
    db.record_artifact_chunk(owner.into(), image.clone(), vec![(0, 3)])
        .unwrap();
    db.publish_artifact_upload(owner.into(), image.clone())
        .unwrap();
    image
}

#[test]
fn matching_retry_survives_deleted_target_and_checks_authority_before_conflicts() {
    let (_dir, mut db) = database();
    let chat = record();
    db.save_runtime_chat(chat.clone()).unwrap();
    let owner = device(&mut db);
    let key = CommandKey::remote(owner.clone(), 1, id()).unwrap();
    let input = command(
        key.clone(),
        "chat.edit",
        &chat.id,
        serde_json::json!({"text":"original"}),
    );
    let fingerprint = input.fingerprint.clone();
    db.accept_runtime_chat_command(input.clone(), 2, chat.id.clone(), vec![])
        .unwrap();
    let changed = command(
        key.clone(),
        "chat.edit",
        &chat.id,
        serde_json::json!({"text":"changed"}),
    );
    assert!(matches!(
        db.accept_runtime_chat_command(changed.clone(), 2, chat.id.clone(), vec![]),
        Err(Error::CommandConflict)
    ));
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [&chat.id],
        )
        .unwrap();
    for (method, target, hash) in [
        ("chat.stop", chat.id.clone(), fingerprint.clone()),
        ("chat.edit", id(), fingerprint.clone()),
        ("chat.edit", chat.id.clone(), changed.fingerprint),
    ] {
        assert!(matches!(
            db.get_runtime_command_matching(key.clone(), 2, method.into(), target, hash),
            Err(Error::CommandConflict)
        ));
    }
    assert_eq!(
        db.get_runtime_command_matching(
            key.clone(),
            2,
            "chat.edit".into(),
            chat.id.clone(),
            fingerprint.clone()
        )
        .unwrap()
        .unwrap()
        .state(),
        CommandState::Accepted
    );
    db.transition_runtime_command(
        key.clone(),
        CommandState::Accepted,
        CommandState::Attempting,
        None,
    )
    .unwrap();
    db.transition_runtime_command(
        key.clone(),
        CommandState::Attempting,
        CommandState::DeliveryUnknown,
        None,
    )
    .unwrap();
    db.close_runtime_command_epoch(owner.clone(), 1).unwrap();
    assert!(
        db.get_runtime_command_matching(
            key.clone(),
            2,
            "chat.edit".into(),
            chat.id.clone(),
            fingerprint
        )
        .unwrap()
        .is_some()
    );
    assert!(matches!(
        db.get_runtime_command_matching(
            CommandKey::remote(owner.clone(), 1, id()).unwrap(),
            2,
            "chat.edit".into(),
            chat.id.clone(),
            vec![1; 32]
        ),
        Err(Error::CommandRetired)
    ));
    db.revoke_runtime_device(owner).unwrap();
    assert!(matches!(
        db.get_runtime_command_matching(key, 2, "wrong".into(), id(), vec![0; 32]),
        Err(Error::NotAuthorized)
    ));
}

#[test]
fn chat_acceptance_rolls_back_images_and_accepts_retained_revoked_sender_image() {
    let (_dir, mut db) = database();
    let chat = record();
    db.save_runtime_chat(chat.clone()).unwrap();
    let owner = device(&mut db);
    let image = image(&mut db, &owner, &chat.id);
    let input = command(
        CommandKey::remote(owner.clone(), 1, id()).unwrap(),
        "chat.edit",
        &chat.id,
        serde_json::json!({"text":"edit"}),
    );
    assert!(
        db.accept_runtime_chat_command(
            input.clone(),
            2,
            chat.id.clone(),
            vec![image.clone(), id()]
        )
        .is_err()
    );
    assert!(
        db.get_runtime_command(input.key.clone(), 2)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.db
            .query_row("SELECT count(*) FROM _canopy_artifact_refs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.accept_runtime_chat_command(input, 2, chat.id.clone(), vec![image.clone()])
        .unwrap();
    db.revoke_runtime_device(owner).unwrap();
    let next = command(
        CommandKey::local(id()).unwrap(),
        "chat.edit",
        &chat.id,
        serde_json::json!({"text":"keep image"}),
    );
    db.accept_runtime_chat_command(next, 2, chat.id, vec![image])
        .unwrap();
}

#[test]
fn fork_is_atomic_with_replacement_images_and_restarts_paused() {
    let (dir, mut db) = database();
    let source = record();
    let fork = record();
    db.save_runtime_chat(source.clone()).unwrap();
    let image = image(&mut db, host::LOCAL_PRINCIPAL, &source.id);
    db.retain_local_chat_artifact(source.id.clone(), image.clone())
        .unwrap();
    let message = id();
    let (send, draft) = QueueDraft::from(Draft::text("replacement"))
        .with_artifacts(vec![image.clone()])
        .unwrap()
        .command_for_message(
            CommandKey::local(id()).unwrap(),
            &fork.id,
            message.clone(),
            SendContext::new("claude".into(), None, None, 0).unwrap(),
        )
        .unwrap();
    let replacement = Some((send.clone(), message.clone(), draft.clone()));
    let input = command(
        CommandKey::local(id()).unwrap(),
        "chat.fork",
        &fork.id,
        serde_json::json!({"source":source.id,"history_epoch":0,"replacement_fingerprint":send.fingerprint}),
    );
    let key = input.key.clone();
    db.accept_runtime_command(input, 2).unwrap();
    db.transition_runtime_command(
        key.clone(),
        CommandState::Accepted,
        CommandState::Attempting,
        None,
    )
    .unwrap();
    assert!(matches!(
        db.complete_runtime_fork(key.clone(), fork.clone(), source.id.clone(), None),
        Err(Error::CommandConflict)
    ));
    // A real SQL failure after record/ref/send writes must roll back all of them.
    db.db.execute_batch("CREATE TRIGGER fail_fork BEFORE UPDATE OF state ON _canopy_command_inbox WHEN NEW.method='chat.fork' AND NEW.state='succeeded' BEGIN SELECT RAISE(ABORT,'test finalization failure'); END;").unwrap();
    assert!(
        db.complete_runtime_fork(
            key.clone(),
            fork.clone(),
            source.id.clone(),
            replacement.clone()
        )
        .is_err()
    );
    assert!(
        !db.load_runtime_chats()
            .unwrap()
            .iter()
            .any(|v| v.id == fork.id)
    );
    assert!(
        db.get_runtime_command(send.key.clone(), 2)
            .unwrap()
            .is_none()
    );
    assert!(
        db.retained_artifact_image(fork.id.clone(), image.clone())
            .is_err()
    );
    assert_eq!(
        db.get_runtime_command(key.clone(), 2)
            .unwrap()
            .unwrap()
            .state(),
        CommandState::Attempting
    );
    db.db.execute_batch("DROP TRIGGER fail_fork;").unwrap();
    // A malformed derived message also rolls back the new catalog entry and refs.
    assert!(
        db.complete_runtime_fork(
            key.clone(),
            fork.clone(),
            source.id.clone(),
            Some((send.clone(), id(), draft))
        )
        .is_err()
    );
    assert!(
        !db.load_runtime_chats()
            .unwrap()
            .iter()
            .any(|v| v.id == fork.id)
    );
    db.complete_runtime_fork(
        key.clone(),
        fork.clone(),
        source.id.clone(),
        replacement.clone(),
    )
    .unwrap();
    db.complete_runtime_fork(key.clone(), fork.clone(), source.id.clone(), replacement)
        .unwrap();
    assert_eq!(
        db.load_runtime_queue(fork.id.clone())
            .unwrap()
            .entries()
            .len(),
        1
    );
    let queue = db.load_runtime_queue(fork.id.clone()).unwrap();
    let edit = QueueMutation::Edit {
        message: message.clone(),
        draft: QueueDraft::from(Draft::text("edited fork replacement"))
            .with_artifacts(vec![image.clone()])
            .unwrap(),
    };
    let edit_command = edit
        .command(CommandKey::local(id()).unwrap(), &fork.id, queue.revision())
        .unwrap();
    db.mutate_runtime_queue(edit_command, fork.id.clone(), queue.revision(), edit)
        .unwrap();
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [&source.id],
        )
        .unwrap();
    assert!(db.retained_artifact_image(fork.id.clone(), image).is_ok());
    drop(db);
    let mut db = Database::open(&dir.path().join("runtime.db"), Access::ReadWrite).unwrap();
    db.initialize_runtime().unwrap();
    let queue = db.load_runtime_queue(fork.id).unwrap();
    assert!(queue.paused());
    assert_eq!(queue.entries().len(), 1);
    assert_eq!(queue.entries()[0].state(), QueueState::Queued);
    assert_eq!(
        db.get_runtime_command(key, 2).unwrap().unwrap().state(),
        CommandState::Succeeded
    );
}

#[test]
fn fork_without_replacement_preserves_images_from_archived_epoch() {
    let (_dir, mut db) = database();
    let source = record();
    let fork = record();
    db.save_runtime_chat(source.clone()).unwrap();
    let image = image(&mut db, host::LOCAL_PRINCIPAL, &source.id);
    db.retain_local_chat_artifact(source.id.clone(), image.clone())
        .unwrap();
    db.db.execute_batch("CREATE TABLE IF NOT EXISTS _canopy_rust_chat_epochs(chat TEXT,epoch INTEGER,version INTEGER,meta TEXT);").unwrap();
    db.db
        .execute(
            "INSERT INTO _canopy_rust_chat_epochs VALUES(?1,0,1,'{}')",
            [&source.id],
        )
        .unwrap();
    db.db
        .execute(
            "UPDATE _canopy_runtime_chats SET history_epoch=2 WHERE chat_id=?1",
            [&source.id],
        )
        .unwrap();
    let input = command(
        CommandKey::local(id()).unwrap(),
        "chat.fork",
        &fork.id,
        serde_json::json!({"source":source.id,"history_epoch":0}),
    );
    let key = input.key.clone();
    db.accept_runtime_command(input, 2).unwrap();
    db.transition_runtime_command(
        key.clone(),
        CommandState::Accepted,
        CommandState::Attempting,
        None,
    )
    .unwrap();
    db.complete_runtime_fork(key, fork.clone(), source.id.clone(), None)
        .unwrap();
    db.db
        .execute(
            "DELETE FROM _canopy_runtime_chats WHERE chat_id=?1",
            [&source.id],
        )
        .unwrap();
    assert!(db.retained_artifact_image(fork.id.clone(), image).is_ok());
    assert!(db.load_runtime_queue(fork.id).unwrap().entries().is_empty());
}
