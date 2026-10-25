use super::*;
use crate::{
    host::{self, CommandKey, DeviceRegistration, FileIdentity},
    settings::SettingsClient,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(super) async fn fixture(root: &Path, database: &Path) -> (RpcAuth, DeviceGrant) {
    let client = SettingsClient::create(database).await.unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let device = uuid::Uuid::new_v4().to_string();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                device,
                "test device".into(),
                vec![7; 32],
                host::READ | host::FILES_GIT,
                vec![root.to_str().unwrap().into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![7; 32]).await.unwrap();
    (RpcAuth::new(host.id().into(), client).unwrap(), grant)
}
pub(super) fn meta(auth: &RpcAuth) -> proto::CommandMeta {
    proto::CommandMeta {
        host_id: auth.host_id().into(),
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: None,
    }
}
#[tokio::test]
async fn accepted_effect_outlives_request_and_retry_does_not_repeat_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    let meta = meta(&auth);
    let input = auth
        .input(
            &meta,
            &grant,
            "files.create",
            "file".into(),
            vec![1],
            root.to_str().unwrap().into(),
        )
        .unwrap();
    let locks = ResourceLocks::default();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(mutation(
        auth.clone(),
        locks.clone(),
        root.clone(),
        input.clone(),
        {
            let root = root.clone();
            move || async move {
                let _ = started_tx.send(());
                let _ = release_rx.await;
                std::fs::write(root.join("file"), "once").unwrap();
                Ok(vec![])
            }
        },
    ));
    started_rx.await.unwrap();
    task.abort();
    release_tx.send(()).unwrap();
    // A retry takes the same resource gate and observes final storage, never the
    // disconnected request's cancellation state.
    let result = mutation(auth.clone(), locks, root.clone(), input, || async {
        panic!("must not execute twice")
    })
    .await
    .unwrap();
    assert_eq!(result.state, proto::OperationState::Succeeded as i32);
    assert_eq!(std::fs::read_to_string(root.join("file")).unwrap(), "once");
    auth.client().shutdown().await.unwrap();
}
#[tokio::test]
async fn save_completion_preserves_new_presence_and_records_effect_after_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    let file = FileIdentity::new(root.to_str().unwrap().into(), "a.txt".into()).unwrap();
    let presence = |hash| {
        file.presence_command(
            CommandKey::remote(grant.id().into(), 1, uuid::Uuid::new_v4().to_string()).unwrap(),
            hash,
            true,
        )
        .unwrap()
    };
    auth.client()
        .set_runtime_dirty_presence(presence([1; 32]), file.clone(), [1; 32], true)
        .await
        .unwrap();
    let revision = auth
        .client()
        .runtime_file_presence(file.clone())
        .await
        .unwrap()[0]
        .revision();
    let input = auth
        .input(
            &meta(&auth),
            &grant,
            "files.save",
            file.resource(),
            vec![1],
            file.root().into(),
        )
        .unwrap();
    let key = input.key().clone();
    auth.client()
        .accept_runtime_command(input, host::FILES_GIT)
        .await
        .unwrap();
    auth.client()
        .transition_runtime_command(
            key.clone(),
            CommandState::Accepted,
            CommandState::Attempting,
            None,
        )
        .await
        .unwrap();
    auth.client()
        .set_runtime_dirty_presence(presence([2; 32]), file.clone(), [2; 32], true)
        .await
        .unwrap();
    std::fs::write(root.join("a.txt"), "saved").unwrap();
    let result = auth
        .client()
        .finish_file_save(
            key,
            file.clone(),
            Some(revision),
            Sha256::digest(b"saved").into(),
        )
        .await
        .unwrap();
    assert_eq!(result.state(), CommandState::Succeeded);
    assert_eq!(
        auth.client()
            .runtime_file_presence(file.clone())
            .await
            .unwrap()[0]
            .base_hash(),
        &[2; 32]
    );
    let input = auth
        .input(
            &meta(&auth),
            &grant,
            "files.save",
            file.resource(),
            vec![2],
            file.root().into(),
        )
        .unwrap();
    let key = input.key().clone();
    auth.client()
        .accept_runtime_command(input, host::FILES_GIT)
        .await
        .unwrap();
    auth.client()
        .transition_runtime_command(
            key.clone(),
            CommandState::Accepted,
            CommandState::Attempting,
            None,
        )
        .await
        .unwrap();
    auth.client()
        .revoke_runtime_device(grant.id().into())
        .await
        .unwrap();
    assert_eq!(
        auth.client()
            .finish_file_save(key, file, None, [3; 32])
            .await
            .unwrap()
            .state(),
        CommandState::Succeeded
    );
    auth.client().shutdown().await.unwrap();
}
#[test]
fn repository_precondition_detects_bytes_index_and_symbolic_head_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let repo = git2::Repository::init(&root).unwrap();
    std::fs::write(root.join("file"), "before").unwrap();
    let first = snapshot::capture(&root).unwrap();
    std::fs::write(root.join("file"), "after!").unwrap();
    assert!(snapshot::check(&root, &first).is_err());
    let second = snapshot::capture(&root).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("file")).unwrap();
    index.write().unwrap();
    assert!(snapshot::check(&root, &second).is_err());
    let third = snapshot::capture(&root).unwrap();
    repo.set_head("refs/heads/other").unwrap();
    assert!(snapshot::check(&root, &third).is_err());
    let sub = root.join("nested");
    std::fs::create_dir(&sub).unwrap();
    assert!(snapshot::repository(&sub).is_err());
}
#[tokio::test]
async fn root_authorization_precedes_repository_access() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    let roots = WorkspaceRoots::default();
    roots
        .replace(HashMap::from([(
            "outside".into(),
            dir.path().join("not-even-created"),
        )]))
        .unwrap();
    let reference = proto::WorkspaceRef {
        host_id: auth.host_id().into(),
        workspace_id: "outside".into(),
    };
    assert_eq!(
        workspace(&auth, &roots, &grant, Some(&reference), host::READ)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn independent_resources_progress_while_another_operation_waits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("roots");
    std::fs::create_dir(&root).unwrap();
    let one = root.join("one");
    let two = root.join("two");
    std::fs::create_dir(&one).unwrap();
    std::fs::create_dir(&two).unwrap();
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    auth.client()
        .update_runtime_device_authorization(
            grant.id().into(),
            grant.authorization_revision(),
            grant.scopes(),
            vec![one.to_str().unwrap().into(), two.to_str().unwrap().into()],
        )
        .await
        .unwrap();
    let locks = ResourceLocks::default();
    let first = auth
        .input(
            &meta(&auth),
            &grant,
            "files.create",
            "one".into(),
            vec![1],
            one.to_str().unwrap().into(),
        )
        .unwrap();
    let second = auth
        .input(
            &meta(&auth),
            &grant,
            "files.create",
            "two".into(),
            vec![2],
            two.to_str().unwrap().into(),
        )
        .unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(mutation(
        auth.clone(),
        locks.clone(),
        one.clone(),
        first,
        move || async move {
            entered_tx.send(()).unwrap();
            release_rx.await.unwrap();
            std::fs::write(one.join("done"), "first").unwrap();
            Ok(vec![])
        },
    ));
    entered_rx.await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        mutation(auth.clone(), locks, two.clone(), second, {
            let two = two.clone();
            move || async move {
                std::fs::write(two.join("done"), "second").unwrap();
                Ok(vec![])
            }
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.state, proto::OperationState::Succeeded as i32);
    assert_eq!(std::fs::read_to_string(two.join("done")).unwrap(), "second");
    release_tx.send(()).unwrap();
    running.await.unwrap().unwrap();
    auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn drain_rejects_new_work_and_waits_for_disconnected_accepted_effects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    let locks = ResourceLocks::default();
    let input = auth
        .input(
            &meta(&auth),
            &grant,
            "files.create",
            "file".into(),
            vec![1],
            root.to_str().unwrap().into(),
        )
        .unwrap();
    let other = auth
        .input(
            &meta(&auth),
            &grant,
            "files.create",
            "other".into(),
            vec![2],
            root.to_str().unwrap().into(),
        )
        .unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let request = tokio::spawn(mutation(
        auth.clone(),
        locks.clone(),
        root.clone(),
        input,
        {
            let root = root.clone();
            move || async move {
                entered_tx.send(()).unwrap();
                release_rx.await.unwrap();
                std::fs::write(root.join("file"), "completed").unwrap();
                Ok(vec![])
            }
        },
    ));
    entered_rx.await.unwrap();
    request.abort();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), locks.drain())
            .await
            .is_err()
    );
    assert_eq!(
        mutation(auth.clone(), locks.clone(), root.clone(), other, || async {
            panic!("draining rejects new work")
        })
        .await
        .unwrap_err()
        .code(),
        tonic::Code::Unavailable
    );
    release_tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), locks.drain())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("file")).unwrap(),
        "completed"
    );
    auth.client().shutdown().await.unwrap();
}
