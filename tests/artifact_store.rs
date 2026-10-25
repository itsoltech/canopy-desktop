//! Real filesystem/SQLite crash boundaries, ownership races, and bounded storage.
use canopy_desktop::{
    artifacts::{
        ArtifactStore, BeginUpload, CHUNK_BYTES, MAX_IMAGE_BYTES, MAX_STORAGE_BYTES, Purpose,
        Target,
    },
    chat::config::ChatSettings,
    host::{ChatRecord, DeviceRegistration},
    settings::{Access, Error, SettingsClient},
    state::workspace::{PaneKind, PaneMetadata},
};
use futures_lite::future::block_on;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::sync::Arc;
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
struct Fixture {
    dir: tempfile::TempDir,
    client: SettingsClient,
    chat: String,
    owner: String,
    store: Arc<ArtifactStore>,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let client = block_on(SettingsClient::create(dir.path().join("test.db"))).unwrap();
        block_on(client.initialize_runtime()).unwrap();
        let chat = id();
        let record = ChatRecord::new(
            chat.clone(),
            None,
            "claude".into(),
            PaneMetadata {
                kind: PaneKind::Chat,
                chat: Some(Box::new(ChatSettings {
                    id: Some(chat.clone()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )
        .unwrap();
        block_on(client.save_runtime_chat(record)).unwrap();
        let owner = id();
        block_on(
            client.register_runtime_device(
                DeviceRegistration::new(
                    owner.clone(),
                    "Phone".into(),
                    vec![3; 32],
                    3,
                    vec!["*".into()],
                )
                .unwrap(),
            ),
        )
        .unwrap();
        let store = Arc::new(ArtifactStore::open(client.clone(), dir.path().join("bulk")).unwrap());
        Self {
            dir,
            client,
            chat,
            owner,
            store,
        }
    }
    fn binding(&self, bytes: &[u8]) -> BeginUpload {
        BeginUpload::new(
            self.owner.clone(),
            Target::Chat(self.chat.clone()),
            Purpose::Image,
            bytes.len() as u64,
            Sha256::digest(bytes).into(),
            "image/png".into(),
        )
        .unwrap()
    }
    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("test.db")).unwrap()
    }
}
#[test]
fn chunks_retry_overlap_conflict_hash_and_restart() {
    let f = Fixture::new();
    let bytes = vec![9; CHUNK_BYTES + 31];
    let upload = f.store.begin(f.binding(&bytes)).unwrap();
    assert!(matches!(
        f.store.finish(&f.owner, upload.id()),
        Err(Error::OperationPending)
    ));
    f.store
        .put_chunk(&f.owner, upload.id(), 0, &bytes[..CHUNK_BYTES])
        .unwrap();
    f.store
        .put_chunk(&f.owner, upload.id(), 0, &bytes[..CHUNK_BYTES])
        .unwrap();
    assert!(matches!(
        f.store.put_chunk(&f.owner, upload.id(), 32, &[4]),
        Err(Error::ResourceConflict)
    ));
    assert!(
        f.store
            .put_chunk(&f.owner, upload.id(), bytes.len() as u64, &[9])
            .is_err()
    );
    assert!(
        f.store
            .put_chunk(&f.owner, upload.id(), 0, &vec![9; CHUNK_BYTES + 1])
            .is_err()
    );
    // An overlapping identical fragment can also advance the confirmed frontier.
    f.store
        .put_chunk(
            &f.owner,
            upload.id(),
            (CHUNK_BYTES - 5) as u64,
            &bytes[CHUNK_BYTES - 5..],
        )
        .unwrap();
    let a = f.store.finish(&f.owner, upload.id()).unwrap();
    assert_eq!(a.id(), f.store.finish(&f.owner, upload.id()).unwrap().id());
    assert_eq!(
        f.store
            .download(&f.owner, a.id(), CHUNK_BYTES as u64, 31)
            .unwrap(),
        vec![9; 31]
    );
    assert!(f.store.put_chunk(&f.owner, upload.id(), 0, &[9]).is_err());
    let path = f.dir.path().join("bulk");
    drop(f.store);
    block_on(f.client.shutdown()).unwrap();
    let client = block_on(SettingsClient::open(
        f.dir.path().join("test.db"),
        Access::ReadWrite,
    ))
    .unwrap();
    block_on(client.initialize_runtime()).unwrap();
    let store = ArtifactStore::open(client.clone(), path).unwrap();
    assert_eq!(store.finish(&f.owner, upload.id()).unwrap().id(), a.id());
    assert_eq!(store.download(&f.owner, a.id(), 0, 4).unwrap(), vec![9; 4]);
    block_on(client.shutdown()).unwrap();
}
#[test]
fn wrong_hash_and_unacknowledged_bytes_never_publish() {
    let f = Fixture::new();
    let upload = f.store.begin(f.binding(b"good")).unwrap();
    // Simulate crash after durable bytes but before the range acknowledgement.
    std::fs::write(f.dir.path().join("bulk").join(upload.id()), b"bad!").unwrap();
    assert!(
        f.store
            .status(&f.owner, upload.id())
            .unwrap()
            .ranges()
            .is_empty()
    );
    assert!(matches!(
        f.store.finish(&f.owner, upload.id()),
        Err(Error::OperationPending)
    ));
    f.store
        .put_chunk(&f.owner, upload.id(), 0, b"bad!")
        .unwrap();
    assert!(matches!(
        f.store.finish(&f.owner, upload.id()),
        Err(Error::ResourceConflict)
    ));
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM _canopy_artifacts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    f.store.cancel(&f.owner, upload.id()).unwrap();
    assert!(!f.dir.path().join("bulk").join(upload.id()).exists());
}
#[test]
fn quota_ttl_orphans_and_owner_are_enforced() {
    let f = Fixture::new();
    let binding = BeginUpload::new(
        f.owner.clone(),
        Target::Chat(f.chat.clone()),
        Purpose::Image,
        MAX_IMAGE_BYTES,
        [0; 32],
        "image/png".into(),
    )
    .unwrap();
    let uploads = (0..4)
        .map(|_| f.store.begin(binding.clone()).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        f.store.begin(f.binding(b"x")),
        Err(Error::ResourceLimit)
    ));
    assert!(
        BeginUpload::new(
            f.owner.clone(),
            Target::Chat(f.chat.clone()),
            Purpose::Image,
            MAX_IMAGE_BYTES + 1,
            [0; 32],
            "image/png".into()
        )
        .is_err()
    );
    assert!(matches!(
        f.store.status("local", uploads[0].id()),
        Err(Error::NotAuthorized)
    ));
    assert!(matches!(
        f.store.cancel("local", uploads[0].id()),
        Err(Error::NotAuthorized)
    ));
    let orphan = id();
    std::fs::write(
        f.dir.path().join("bulk").join(&orphan),
        b"crash before insert",
    )
    .unwrap();
    f.db()
        .execute("UPDATE _canopy_uploads SET last_active=0", [])
        .unwrap();
    f.store.cleanup().unwrap();
    assert!(!f.dir.path().join("bulk").join(orphan).exists());
    for upload in uploads {
        assert!(!f.dir.path().join("bulk").join(upload.id()).exists());
    }
    assert!(f.store.begin(f.binding(b"new")).is_ok());
}
#[test]
fn finish_serializes_with_delete_and_revoke() {
    for revoke in [false, true] {
        for _ in 0..8 {
            let f = Fixture::new();
            let upload = f.store.begin(f.binding(b"payload")).unwrap();
            f.store
                .put_chunk(&f.owner, upload.id(), 0, b"payload")
                .unwrap();
            let store = f.store.clone();
            let owner = f.owner.clone();
            let uid = upload.id().to_owned();
            let finish = std::thread::spawn(move || store.finish(&owner, &uid));
            if revoke {
                block_on(f.client.revoke_runtime_device(f.owner.clone())).unwrap();
            } else {
                block_on(f.client.delete_chat_history(f.chat.clone())).unwrap();
            }
            // Either serialization order is valid, but neither can leave a usable object.
            let _ = finish.join().unwrap();
            assert!(f.store.finish(&f.owner, upload.id()).is_err());
            f.store.cleanup().unwrap();
            assert!(!f.dir.path().join("bulk").join(upload.id()).exists());
            assert_eq!(
                f.db()
                    .query_row("SELECT COUNT(*) FROM _canopy_artifacts", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}
#[test]
fn unattached_budget_and_retained_read_scope() {
    let f = Fixture::new();
    let upload = f.store.begin(f.binding(b"image")).unwrap();
    f.store
        .put_chunk(&f.owner, upload.id(), 0, b"image")
        .unwrap();
    let a = f.store.finish(&f.owner, upload.id()).unwrap();
    // Simulate existing large results without allocating 256 MiB in this regression.
    f.db()
        .execute(
            "UPDATE _canopy_artifacts SET size=?1",
            [MAX_STORAGE_BYTES as i64],
        )
        .unwrap();
    let other = f.store.begin(f.binding(b"next")).unwrap();
    f.store.put_chunk(&f.owner, other.id(), 0, b"next").unwrap();
    assert!(matches!(
        f.store.finish(&f.owner, other.id()),
        Err(Error::ResourceLimit)
    ));
    f.db()
        .execute("UPDATE _canopy_artifacts SET size=5", [])
        .unwrap();
    f.db()
        .execute(
            "INSERT INTO _canopy_artifact_refs VALUES(?1,'chat',?2)",
            [a.id(), &f.chat],
        )
        .unwrap();
    f.db()
        .execute(
            "UPDATE _canopy_uploads SET last_active=0 WHERE upload_id=?1",
            [a.id()],
        )
        .unwrap();
    f.store.cleanup().unwrap();
    let reader = id();
    block_on(
        f.client.register_runtime_device(
            DeviceRegistration::new(
                reader.clone(),
                "Reader".into(),
                vec![4; 32],
                1,
                vec!["*".into()],
            )
            .unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(f.store.download(&reader, a.id(), 0, 5).unwrap(), b"image");
    assert!(f.store.status(&reader, upload.id()).is_err());
    block_on(f.client.revoke_runtime_device(reader.clone())).unwrap();
    assert!(f.store.download(&reader, a.id(), 0, 5).is_err());
    block_on(f.client.delete_chat_history(f.chat.clone())).unwrap();
    f.store.cleanup().unwrap();
    assert!(f.store.download(&f.owner, a.id(), 0, 5).is_err());
}
#[test]
fn accepted_send_retains_bulk_without_copying_bytes_into_inbox() {
    use canopy_desktop::{
        chat::session::Draft,
        host::{CommandKey, QueueDraft, SendContext},
    };
    let f = Fixture::new();
    let bytes = vec![0x73; CHUNK_BYTES];
    let upload = f.store.begin(f.binding(&bytes)).unwrap();
    f.store.put_chunk(&f.owner, upload.id(), 0, &bytes).unwrap();
    let artifact = f.store.finish(&f.owner, upload.id()).unwrap();
    let message = id();
    let command = id();
    let key = CommandKey::remote(f.owner.clone(), 1, command.clone()).unwrap();
    let draft = QueueDraft::from(Draft::text("attached"))
        .with_artifacts(vec![artifact.id().into()])
        .unwrap();
    let (input, draft) = draft
        .command_for_message(
            key,
            &f.chat,
            message.clone(),
            SendContext::new("claude".into(), None, None, 0).unwrap(),
        )
        .unwrap();
    block_on(
        f.client
            .accept_runtime_message(input, f.chat.clone(), message, draft),
    )
    .unwrap();
    f.db()
        .execute("UPDATE _canopy_uploads SET last_active=0", [])
        .unwrap();
    f.store.cleanup().unwrap();
    assert_eq!(
        f.store
            .download(&f.owner, artifact.id(), 0, CHUNK_BYTES)
            .unwrap(),
        bytes
    );
    let arguments: Vec<u8> = f
        .db()
        .query_row(
            "SELECT arguments FROM _canopy_command_inbox WHERE command_id=?1",
            [command],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        f.store.read_retained_image(&f.chat, artifact.id()).unwrap(),
        bytes
    );
    assert!(f.store.read_retained_image(&id(), artifact.id()).is_err());
    block_on(f.client.revoke_runtime_device(f.owner.clone())).unwrap();
    f.store.cleanup().unwrap();
    assert_eq!(
        f.store.read_retained_image(&f.chat, artifact.id()).unwrap(),
        bytes
    );
    let private_file = f.dir.path().join("bulk").join(artifact.id());
    std::fs::write(&private_file, vec![0x74; CHUNK_BYTES]).unwrap();
    assert!(matches!(
        f.store.read_retained_image(&f.chat, artifact.id()),
        Err(Error::ResourceConflict)
    ));
    std::fs::write(&private_file, &bytes).unwrap();
    assert!(arguments.len() < 4096);
    assert!(
        String::from_utf8(arguments)
            .unwrap()
            .contains(artifact.id())
    );
    block_on(f.client.delete_chat_history(f.chat.clone())).unwrap();
    f.store.cleanup().unwrap();
    assert!(!f.dir.path().join("bulk").join(upload.id()).exists());
}

#[test]
fn deterministic_begin_reconciles_lost_ack_and_rejects_rebinding() {
    let f = Fixture::new();
    let id = id();
    let binding = f.binding(b"hello");
    let first = f.store.begin_with_id(id.clone(), binding.clone()).unwrap();
    f.store.put_chunk(&f.owner, &id, 0, b"hello").unwrap();
    let retry = f.store.begin_with_id(id.clone(), binding).unwrap();
    assert_eq!(first.id(), retry.id());
    assert_eq!(retry.ranges(), &[(0, 5)]);
    assert!(matches!(
        f.store.begin_with_id(id, f.binding(b"changed")),
        Err(Error::CommandConflict)
    ));
}
#[test]
fn local_history_import_reuses_real_blob_retains_chat_and_rejects_path_escape() {
    let f = Fixture::new();
    let root = f.dir.path().join("chat-attachments");
    std::fs::create_dir_all(root.join(&f.chat)).unwrap();
    let file = format!("{}.png", id());
    let bytes = b"bounded source image bytes";
    std::fs::write(root.join(&f.chat).join(&file), bytes).unwrap();
    let a = f.store.import_chat_image(&f.chat, &file, &root).unwrap();
    let b = f.store.import_chat_image(&f.chat, &file, &root).unwrap();
    assert_eq!(a.id(), b.id());
    assert_eq!(
        f.store
            .retained_image_metadata(&f.chat, a.id())
            .unwrap()
            .sha256(),
        &<[u8; 32]>::from(Sha256::digest(bytes))
    );
    assert_eq!(
        f.store.download(&f.owner, a.id(), 0, CHUNK_BYTES).unwrap(),
        bytes
    );
    assert!(
        f.store
            .import_chat_image(&f.chat, &format!("../{file}"), &root)
            .is_err()
    );
    #[cfg(unix)]
    {
        let link = format!("{}.png", id());
        std::os::unix::fs::symlink(
            root.join(&f.chat).join(&file),
            root.join(&f.chat).join(&link),
        )
        .unwrap();
        assert!(f.store.import_chat_image(&f.chat, &link, &root).is_err());
    }
    f.db()
        .execute("UPDATE _canopy_uploads SET last_active=0", [])
        .unwrap();
    f.store.cleanup().unwrap();
    assert_eq!(f.store.read_retained_image(&f.chat, a.id()).unwrap(), bytes);
    block_on(f.client.delete_chat_history(f.chat.clone())).unwrap();
    f.store.cleanup().unwrap();
    assert!(f.store.download(&f.owner, a.id(), 0, CHUNK_BYTES).is_err());
}

#[test]
fn queue_edit_retains_new_images_atomically_and_keeps_revoked_senders_accepted_images() {
    use canopy_desktop::{
        chat::session::Draft,
        host::{CommandKey, QueueDraft, QueueMutation, SendContext},
    };
    let f = Fixture::new();
    let original = f.store.begin(f.binding(b"original")).unwrap();
    f.store
        .put_chunk(&f.owner, original.id(), 0, b"original")
        .unwrap();
    let original = f.store.finish(&f.owner, original.id()).unwrap();
    let message = id();
    let draft = QueueDraft::from(Draft::text("first"))
        .with_artifacts(vec![original.id().into()])
        .unwrap();
    let (input, draft) = draft
        .command_for_message(
            CommandKey::remote(f.owner.clone(), 1, id()).unwrap(),
            &f.chat,
            message.clone(),
            SendContext::new("claude".into(), None, None, 0).unwrap(),
        )
        .unwrap();
    block_on(
        f.client
            .accept_runtime_message(input, f.chat.clone(), message.clone(), draft),
    )
    .unwrap();
    let editor = id();
    block_on(
        f.client.register_runtime_device(
            DeviceRegistration::new(
                editor.clone(),
                "Editor".into(),
                vec![9; 32],
                3,
                vec!["*".into()],
            )
            .unwrap(),
        ),
    )
    .unwrap();
    let foreign_chat = id();
    block_on(
        f.client.save_runtime_chat(
            ChatRecord::new(
                foreign_chat.clone(),
                None,
                "claude".into(),
                PaneMetadata {
                    kind: PaneKind::Chat,
                    chat: Some(Box::new(ChatSettings {
                        id: Some(foreign_chat.clone()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap(),
        ),
    )
    .unwrap();
    let publish = |chat: &str, value: &[u8]| {
        let binding = BeginUpload::new(
            editor.clone(),
            Target::Chat(chat.into()),
            Purpose::Image,
            value.len() as u64,
            Sha256::digest(value).into(),
            "image/png".into(),
        )
        .unwrap();
        let u = f.store.begin(binding).unwrap();
        f.store.put_chunk(&editor, u.id(), 0, value).unwrap();
        f.store.finish(&editor, u.id()).unwrap()
    };
    let foreign = publish(&foreign_chat, b"foreign");
    let own = publish(&f.chat, b"new");
    block_on(f.client.revoke_runtime_device(f.owner.clone())).unwrap();
    let before = block_on(f.client.load_runtime_queue(f.chat.clone())).unwrap();
    let attempt = |ids: Vec<String>, revision: u64| {
        let mutation = QueueMutation::Edit {
            message: message.clone(),
            draft: QueueDraft::from(Draft::text("edited"))
                .with_artifacts(ids)
                .unwrap(),
        };
        let command = mutation
            .command(
                CommandKey::remote(editor.clone(), 1, id()).unwrap(),
                &f.chat,
                revision,
            )
            .unwrap();
        block_on(
            f.client
                .mutate_runtime_queue(command, f.chat.clone(), revision, mutation),
        )
    };
    assert!(matches!(
        attempt(
            vec![original.id().into(), own.id().into(), foreign.id().into()],
            before.revision()
        ),
        Err(Error::ResourceConflict)
    ));
    assert_eq!(
        block_on(f.client.load_runtime_queue(f.chat.clone()))
            .unwrap()
            .revision(),
        before.revision()
    );
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM _canopy_artifact_refs WHERE artifact_id=?1",
                [own.id()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "later invalid image rolls back earlier retention in the same edit"
    );
    attempt(
        vec![original.id().into(), own.id().into()],
        before.revision(),
    )
    .unwrap();
    f.db()
        .execute("UPDATE _canopy_uploads SET last_active=0", [])
        .unwrap();
    f.store.cleanup().unwrap();
    assert_eq!(
        f.store.read_retained_image(&f.chat, original.id()).unwrap(),
        b"original"
    );
    assert_eq!(
        f.store.read_retained_image(&f.chat, own.id()).unwrap(),
        b"new"
    );
}
