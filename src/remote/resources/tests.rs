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
pub(super) fn meta(_auth: &RpcAuth) -> proto::CommandMeta {
    proto::CommandMeta {
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: None,
    }
}

pub(super) fn project_target(
    auth: &RpcAuth,
    root: PathBuf,
    git: bool,
) -> (WorkspaceRoots, crate::api::proto::ResourceTarget) {
    use crate::{api::proto as p, state::projects::Projects};
    let mut projects = Projects::default();
    if git {
        projects.open_worktree(root.clone(), root);
    } else {
        projects.open(root);
    }
    let project = projects.current().unwrap();
    let target = p::ResourceTarget {
        resource: Some(p::resource_target::Resource::Project(p::ProjectRef {
            host_id: auth.host_id().into(),
            project_id: serde_json::to_string(&project.id).unwrap(),
            worktree_id: git.then(|| serde_json::to_string(&project.workspace).unwrap()),
        })),
    };
    let roots = WorkspaceRoots::default();
    roots.replace_projects(projects.items).unwrap();
    (roots, target)
}

pub(super) fn api_meta() -> crate::api::proto::CommandMeta {
    crate::api::proto::CommandMeta {
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: None,
    }
}

#[tokio::test]
async fn thread_file_presence_uses_the_thread_epoch_and_shared_atomic_owner() {
    use crate::{
        api::{proto as p, resources as api},
        command_bus::FileCommand,
        host::ChatRecord,
        state::workspace::{PaneKind, PaneMetadata},
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    let workspace = uuid::Uuid::new_v4().to_string();
    let chat = uuid::Uuid::new_v4().to_string();
    let record = auth
        .client()
        .save_runtime_chat(
            ChatRecord::new(
                chat.clone(),
                Some(workspace.clone()),
                "test".into(),
                PaneMetadata {
                    kind: PaneKind::Chat,
                    chat: Some(Box::new(crate::chat::config::ChatSettings {
                        id: Some(chat),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap()
            .with_authority_root(root.to_str().unwrap().into())
            .unwrap(),
        )
        .await
        .unwrap();
    let roots = WorkspaceRoots::default();
    roots
        .replace(HashMap::from([(workspace, root.clone())]))
        .unwrap();
    let target = p::ResourceTarget {
        resource: Some(p::resource_target::Resource::Thread(p::ThreadTarget {
            thread: Some(p::ThreadRef {
                host_id: auth.host_id().into(),
                thread_id: record.thread_id().unwrap().to_string(),
            }),
            executor_epoch: record.thread().unwrap().executor_epoch(),
        })),
    };
    let command = FileCommand::Presence {
        path: "edit.txt".into(),
        base_hash: [3; 32],
        dirty: true,
    };
    let prepared = api::prepare_file(&auth, &roots, &grant, api_meta(), &target, command.clone())
        .await
        .unwrap();
    let identity = command.identity(root.to_str().unwrap().into()).unwrap();
    let saved = auth
        .client()
        .set_runtime_dirty_presence(
            prepared.runtime_input().clone(),
            identity.clone(),
            [3; 32],
            true,
        )
        .await
        .unwrap();
    assert_eq!(saved.state(), CommandState::Succeeded);
    assert_eq!(
        auth.client()
            .runtime_file_presence(identity)
            .await
            .unwrap()
            .len(),
        1
    );
    let mut stale = target;
    let Some(p::resource_target::Resource::Thread(thread)) = stale.resource.as_mut() else {
        unreachable!()
    };
    thread.executor_epoch += 1;
    let meta = api_meta();
    let stale_command = FileCommand::Presence {
        path: "other.txt".into(),
        base_hash: [4; 32],
        dirty: true,
    };
    let rejected = api::prepare_file(
        &auth,
        &roots,
        &grant,
        meta.clone(),
        &stale,
        stale_command.clone(),
    )
    .await
    .unwrap();
    let receipt = api::receipt(&auth, rejected.prior().unwrap(), &stale).unwrap();
    assert_eq!(receipt.state, p::OperationState::Rejected as i32);
    assert_eq!(receipt.error.unwrap().code, "conditions_changed");
    assert!(
        auth.client()
            .runtime_file_presence(
                stale_command
                    .identity(root.to_str().unwrap().into())
                    .unwrap()
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        api::prepare_file(&auth, &roots, &grant, meta, &stale, stale_command)
            .await
            .unwrap()
            .prior(),
        rejected.prior()
    );
    auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn project_file_retry_survives_removal_but_never_bypasses_current_authorization() {
    use crate::{api::resources as api, command_bus::FileCommand};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    let (roots, target) = project_target(&auth, root.clone(), false);
    let meta = api_meta();
    let command = FileCommand::Create {
        path: "once.txt".into(),
        directory: false,
        content: None,
    };
    let prepared = api::prepare_file(
        &auth,
        &roots,
        &grant,
        meta.clone(),
        &target,
        command.clone(),
    )
    .await
    .unwrap();
    let locks = ResourceLocks::default();
    let saved = owned_mutation(
        auth.clone(),
        locks,
        root.clone(),
        prepared.runtime_input().clone(),
        {
            let root = root.clone();
            move || async move {
                std::fs::write(root.join("once.txt"), "original").unwrap();
                Ok(Vec::new())
            }
        },
    )
    .await
    .unwrap();
    roots.replace_projects(Vec::new()).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    let retry = api::prepare_file(
        &auth,
        &roots,
        &grant,
        meta.clone(),
        &target,
        command.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        api::receipt(&auth, retry.prior().unwrap(), &target).unwrap(),
        api::receipt(&auth, &saved, &target).unwrap()
    );
    let changed = FileCommand::Create {
        path: "different.txt".into(),
        directory: false,
        content: None,
    };
    assert_eq!(
        api::prepare_file(
            &auth,
            &roots,
            &grant,
            meta.clone(),
            &target,
            changed.clone()
        )
        .await
        .err()
        .unwrap()
        .code(),
        tonic::Code::Aborted
    );
    // Retain the scope but remove the root: neither a saved outcome nor a
    // fingerprint conflict may disclose a resource after sharing was removed.
    auth.client()
        .update_runtime_device_authorization(
            grant.id().into(),
            grant.authorization_revision(),
            host::READ | host::FILES_GIT,
            vec!["private".into()],
        )
        .await
        .unwrap();
    for command in [command, changed] {
        assert_eq!(
            api::prepare_file(&auth, &roots, &grant, meta.clone(), &target, command)
                .await
                .err()
                .unwrap()
                .code(),
            tonic::Code::PermissionDenied
        );
    }
    auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn project_file_validation_uses_catalog_metadata_before_resource_io() {
    use crate::{
        api::{proto as p, resources as api},
        command_bus::FileCommand,
    };
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("never-created");
    let (auth, grant) = fixture(&missing, &dir.path().join("db")).await;
    let (roots, valid) = project_target(&auth, missing.clone(), true);
    assert_eq!(
        api::read_root(&auth, &roots, &grant, &valid, host::READ)
            .await
            .unwrap(),
        missing
    );
    let mut invalid = valid.clone();
    let Some(p::resource_target::Resource::Project(project)) = invalid.resource.as_mut() else {
        unreachable!()
    };
    project.worktree_id = None;
    assert_eq!(
        api::read_root(&auth, &roots, &grant, &invalid, host::READ)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    assert_eq!(
        api::prepare_file(
            &auth,
            &roots,
            &grant,
            api_meta(),
            &invalid,
            FileCommand::Create {
                path: "file.txt".into(),
                directory: false,
                content: None
            }
        )
        .await
        .err()
        .unwrap()
        .code(),
        tonic::Code::InvalidArgument
    );
    let (roots, mut ordinary) = project_target(&auth, missing.clone(), false);
    let Some(p::resource_target::Resource::Project(project)) = ordinary.resource.as_mut() else {
        unreachable!()
    };
    project.worktree_id = Some("foreign".into());
    assert!(
        api::read_root(&auth, &roots, &grant, &ordinary, host::READ)
            .await
            .is_err()
    );
    assert!(!missing.exists());
    auth.client().shutdown().await.unwrap();
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
async fn completed_effect_is_retained_but_not_disclosed_after_root_authority_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (auth, grant) = fixture(&root, &dir.path().join("db")).await;
    let (roots, target) = project_target(&auth, root.clone(), false);
    let prepared = crate::api::resources::prepare_file(
        &auth,
        &roots,
        &grant,
        api_meta(),
        &target,
        crate::command_bus::FileCommand::Create {
            path: "once.txt".into(),
            directory: false,
            content: None,
        },
    )
    .await
    .unwrap();
    let key = prepared.runtime_input().key().clone();
    let worker_auth = auth.clone();
    let device = grant.id().to_owned();
    let effect_root = root.clone();
    let error = owned_mutation(
        auth.clone(),
        ResourceLocks::default(),
        root.clone(),
        prepared.runtime_input().clone(),
        move || async move {
            // The dispatch fence was crossed while authorized. The owner still
            // records the known outcome when sharing changes during its work.
            worker_auth
                .client()
                .update_runtime_device_authorization(
                    device,
                    grant.authorization_revision(),
                    host::READ | host::FILES_GIT,
                    vec![],
                )
                .await
                .unwrap();
            std::fs::write(effect_root.join("once.txt"), "saved once").unwrap();
            Ok(Vec::new())
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code(), tonic::Code::PermissionDenied);
    assert_eq!(
        std::fs::read_to_string(root.join("once.txt")).unwrap(),
        "saved once"
    );
    let current = auth
        .client()
        .lookup_runtime_device(vec![7; 32])
        .await
        .unwrap();
    auth.client()
        .update_runtime_device_authorization(
            current.id().into(),
            current.authorization_revision(),
            host::READ | host::FILES_GIT,
            vec![root.to_str().unwrap().into()],
        )
        .await
        .unwrap();
    assert_eq!(
        auth.client()
            .get_runtime_command(key, host::FILES_GIT)
            .await
            .unwrap()
            .unwrap()
            .state(),
        CommandState::Succeeded
    );
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
    let mut projects = crate::state::projects::Projects::default();
    projects.open(dir.path().join("not-even-created"));
    let project = projects.current().unwrap();
    let reference = proto::ProjectRef {
        host_id: auth.host_id().into(),
        project_id: serde_json::to_string(&project.id).unwrap(),
        worktree_id: None,
    };
    roots.replace_projects(projects.items).unwrap();
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
