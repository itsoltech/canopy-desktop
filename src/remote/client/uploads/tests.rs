use super::*;
use crate::remote::client::{
    ClientRegistry,
    tests::{FakeVault, record},
};
use std::{path::Path, sync::Arc};

const SOURCE: &[u8] = b"retained attachment source";

fn fixture(path: &Path) -> (ClientRegistry, Arc<HostClient>) {
    let registry = ClientRegistry::open(path.into(), Arc::new(FakeVault)).unwrap();
    let host = registry.insert(record()).unwrap();
    (registry, host)
}
async fn prepare(host: &HostClient) -> (String, ApiIntent) {
    let id = host
        .prepare_task_attachment(
            p::TaskRef {
                project: Some(p::TaskProject {
                    resource: Some(p::task_project::Resource::Environment(p::EnvironmentRef {
                        host_id: host.id().as_str().into(),
                    })),
                    integration_id: uuid::Uuid::new_v4().to_string(),
                    project_key: "team/repo".into(),
                }),
                task_id: "42".into(),
            },
            "text/plain",
            SOURCE,
        )
        .unwrap();
    let mut begin = host
        .upload_record(&id)
        .unwrap()
        .draft()
        .unwrap()
        .begin(host)
        .await
        .unwrap();
    *begin.meta() = Some(host.command(None).unwrap());
    save_intent(host, &id, UploadPhase::Begin, &begin);
    (id, begin)
}
fn save_intent(host: &HostClient, id: &str, phase: UploadPhase, intent: &ApiIntent) {
    let (kind, bytes) = intent.encode();
    host.update_record(|record| {
        let upload = record.uploads.iter_mut().find(|r| r.id == id).unwrap();
        *match phase {
            UploadPhase::Begin => &mut upload.api_begin,
            UploadPhase::Finish => &mut upload.api_finish,
            UploadPhase::Cancel => &mut upload.api_cancel,
        } = Some((kind.into(), bytes));
        Ok(())
    })
    .unwrap();
}
fn receipt(
    host: &HostClient,
    mut command: ApiIntent,
    state: p::OperationState,
) -> p::CommandReceipt {
    let meta = command.meta().clone().unwrap();
    p::CommandReceipt {
        host_id: host.id().as_str().into(),
        command_id: meta.command_id,
        command_epoch: meta.command_epoch,
        state: state as i32,
        resource: command.resource(),
        error: matches!(
            state,
            p::OperationState::Rejected | p::OperationState::Failed
        )
        .then(|| p::DomainError {
            code: "conditions_changed".into(),
            message: "The original target changed.".into(),
            retryable: false,
        }),
        result: (state == p::OperationState::Succeeded).then(|| {
            p::command_receipt::Result::Artifact(p::ArtifactResult {
                upload_id: "upload-42".into(),
                artifact: None,
            })
        }),
        ..Default::default()
    }
}
fn retained_source(host: &HostClient, id: &str) -> Vec<u8> {
    std::fs::read(host.store.lock().unwrap().attachment_dir().join(id)).unwrap()
}
fn restored(path: &Path, host: &HostClient) -> Arc<HostClient> {
    ClientRegistry::open(path.into(), Arc::new(FakeVault))
        .unwrap()
        .get(host.id())
        .unwrap()
}

#[tokio::test]
async fn final_upload_errors_survive_restart_and_allow_explicit_cleanup_without_replay() {
    for (phase, state) in [
        (UploadPhase::Begin, p::OperationState::Rejected),
        (UploadPhase::Begin, p::OperationState::Failed),
        (UploadPhase::Finish, p::OperationState::Rejected),
        (UploadPhase::Finish, p::OperationState::Failed),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client.json");
        let (_registry, host) = fixture(&path);
        let (id, begin) = prepare(&host).await;
        let command = if matches!(phase, UploadPhase::Finish) {
            host.save_upload_receipt(
                &id,
                UploadPhase::Begin,
                &receipt(&host, begin.clone(), p::OperationState::Succeeded),
            )
            .unwrap();
            let mut finish = finish(&begin, "upload-42".into()).unwrap();
            *finish.meta() = Some(host.command(None).unwrap());
            save_intent(&host, &id, phase, &finish);
            finish
        } else {
            begin
        };
        if state == p::OperationState::Rejected {
            host.save_upload_receipt(
                &id,
                phase,
                &receipt(&host, command.clone(), p::OperationState::Accepted),
            )
            .unwrap();
        }
        let original = command.encode();
        let response = receipt(&host, command.clone(), state);
        assert_eq!(
            host.upload_command_with(&id, phase, command, |retried| async move {
                assert_eq!(retried.encode(), original);
                Ok(response)
            })
            .await
            .unwrap_err(),
            "The original target changed."
        );
        let host = restored(&path, &host);
        assert_eq!(
            host.resume_upload(&id).await.unwrap_err(),
            "The original target changed."
        );
        assert_eq!(retained_source(&host, &id), SOURCE);
        assert!(!host.upload_record(&id).unwrap().blocks_epoch(1));
        let (next, _) = prepare(&host).await;
        assert_ne!(next, id);
        host.remove_local_attachment(&id).unwrap();
        assert_eq!(host.upload_journal(), vec![next]);
    }
}

#[tokio::test]
async fn explicit_retry_of_accepted_completion_reaches_rejection_when_bulk_is_unavailable() {
    for phase in [UploadPhase::Finish, UploadPhase::Cancel] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("client.json");
        let (_registry, host) = fixture(&path);
        let (id, begin) = prepare(&host).await;
        host.save_upload_receipt(
            &id,
            UploadPhase::Begin,
            &receipt(&host, begin.clone(), p::OperationState::Succeeded),
        )
        .unwrap();
        let mut finish = finish(&begin, "upload-42".into()).unwrap();
        if matches!(phase, UploadPhase::Cancel) {
            let ApiIntent::FinishTask(request) = finish else {
                panic!("expected task Finish")
            };
            finish = ApiIntent::CancelTask(p::CancelTaskUploadRequest {
                command: None,
                task: request.task,
                upload_id: request.upload_id,
                origin: request.origin,
            });
        }
        *finish.meta() = Some(host.command(None).unwrap());
        save_intent(&host, &id, phase, &finish);
        let accepted = receipt(&host, finish.clone(), p::OperationState::Accepted);
        host.save_upload_receipt(&id, phase, &accepted).unwrap();
        host.reconcile_upload_with(
            &id,
            |_| async {
                Ok(p::GetCommandResponse {
                    state: p::CommandLookupState::Found as i32,
                    receipt: Some(accepted.clone()),
                })
            },
            |_| async { Err(tonic::Status::not_found("Upload is unavailable.")) },
        )
        .await
        .unwrap();
        assert!(host.remove_local_attachment(&id).is_err());
        let original = finish.encode();
        let dispatch = |command: ApiIntent| {
            assert_eq!(command.encode(), original);
            let rejected = receipt(&host, command, p::OperationState::Rejected);
            async { Ok(rejected) }
        };
        let error = match phase {
            UploadPhase::Finish => host.resume_upload_with(&id, dispatch).await.unwrap_err(),
            UploadPhase::Cancel => host.cancel_upload_with(&id, dispatch).await.unwrap_err(),
            _ => unreachable!(),
        };
        assert_eq!(error, "The original target changed.");
        let host = restored(&path, &host);
        assert_eq!(retained_source(&host, &id), SOURCE);
        assert!(!host.upload_record(&id).unwrap().blocks_epoch(1));
        host.remove_local_attachment(&id).unwrap();
    }
}

#[tokio::test]
async fn retired_begin_and_untyped_legacy_source_are_discardable_but_never_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("client.json");
    let (_registry, host) = fixture(&path);
    let (id, _) = prepare(&host).await;
    host.reconcile_upload_with(
        &id,
        |_| async {
            Ok(p::GetCommandResponse {
                state: p::CommandLookupState::Retired as i32,
                receipt: None,
            })
        },
        |_| async { panic!("a retired Begin has no proven bulk identity") },
    )
    .await
    .unwrap();
    let host = restored(&path, &host);
    assert!(
        host.resume_upload(&id)
            .await
            .unwrap_err()
            .contains("retired")
    );
    assert_eq!(retained_source(&host, &id), SOURCE);
    host.remove_local_attachment(&id).unwrap();
    let (legacy, _) = prepare(&host).await;
    host.update_record(|record| {
        let upload = record.uploads.iter_mut().find(|r| r.id == legacy).unwrap();
        upload.api_begin = None;
        upload.begin = b"pre-v1 undecodable request".to_vec();
        Ok(())
    })
    .unwrap();
    let host = restored(&path, &host);
    assert!(
        host.resume_upload(&legacy)
            .await
            .unwrap_err()
            .contains("typed target")
    );
    assert_eq!(retained_source(&host, &legacy), SOURCE);
    assert!(!host.upload_record(&legacy).unwrap().blocks_epoch(1));
    host.remove_local_attachment(&legacy).unwrap();
}

#[tokio::test]
async fn unavailable_bulk_releases_a_known_begin_without_inventing_a_finish_or_deleting_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("client.json");
    let (_registry, host) = fixture(&path);
    let (id, begin) = prepare(&host).await;
    let response = receipt(&host, begin, p::OperationState::Succeeded);
    host.reconcile_upload_with(
        &id,
        |_| async {
            Ok(p::GetCommandResponse {
                state: p::CommandLookupState::Found as i32,
                receipt: Some(response.clone()),
            })
        },
        |request| async move {
            assert_eq!(request.upload_id, "upload-42");
            Err(tonic::Status::not_found("Upload is unavailable."))
        },
    )
    .await
    .unwrap();
    let host = restored(&path, &host);
    assert!(
        host.resume_upload(&id)
            .await
            .unwrap_err()
            .contains("unavailable on the host")
    );
    assert_eq!(retained_source(&host, &id), SOURCE);
    let saved = host.upload_record(&id).unwrap();
    assert!(saved.artifact.is_none());
    assert!(!saved.blocks_epoch(1));
    host.remove_local_attachment(&id).unwrap();
}

#[tokio::test]
async fn unavailable_or_retired_bulk_does_not_release_uncertain_finish_or_lose_original_intent() {
    for state in [
        None,
        Some(p::OperationState::Accepted),
        Some(p::OperationState::Attempting),
        Some(p::OperationState::DeliveryUnknown),
    ] {
        for retired in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("client.json");
            let (_registry, host) = fixture(&path);
            let (id, begin) = prepare(&host).await;
            host.save_upload_receipt(
                &id,
                UploadPhase::Begin,
                &receipt(&host, begin.clone(), p::OperationState::Succeeded),
            )
            .unwrap();
            let mut finish = finish(&begin, "upload-42".into()).unwrap();
            *finish.meta() = Some(host.command(None).unwrap());
            save_intent(&host, &id, UploadPhase::Finish, &finish);
            let original = finish.encode();
            let response = state.map(|s| receipt(&host, finish.clone(), s));
            if let Some(receipt) = &response {
                host.save_upload_receipt(&id, UploadPhase::Finish, receipt)
                    .unwrap();
            }
            host.reconcile_upload_with(
                &id,
                |_| async {
                    Ok(p::GetCommandResponse {
                        state: if retired {
                            p::CommandLookupState::Retired
                        } else if response.is_some() {
                            p::CommandLookupState::Found
                        } else {
                            p::CommandLookupState::NotFound
                        } as i32,
                        receipt: if retired { None } else { response.clone() },
                    })
                },
                |_| async { Err(tonic::Status::not_found("Upload is unavailable.")) },
            )
            .await
            .unwrap();
            let host = restored(&path, &host);
            assert!(
                host.remove_local_attachment(&id)
                    .unwrap_err()
                    .contains("Unresolved")
            );
            assert_eq!(retained_source(&host, &id), SOURCE);
            let saved = host.upload_record(&id).unwrap();
            assert_eq!(saved.api_finish.unwrap(), (original.0.into(), original.1));
            assert!(!host.upload_record(&id).unwrap().blocks_epoch(2));
            if !retired {
                assert!(host.upload_record(&id).unwrap().blocks_epoch(1));
            }
            if matches!(
                state,
                Some(p::OperationState::Attempting | p::OperationState::DeliveryUnknown)
            ) {
                assert!(
                    host.upload_command_with(&id, UploadPhase::Finish, finish, |_| async {
                        panic!("uncertain Finish must never dispatch again")
                    })
                    .await
                    .is_err()
                );
            }
        }
    }
}

#[tokio::test]
async fn successful_begin_stays_protected_until_a_durable_cancel_or_finish() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("client.json");
    let (_registry, host) = fixture(&path);
    let (id, begin) = prepare(&host).await;
    host.save_upload_receipt(
        &id,
        UploadPhase::Begin,
        &receipt(&host, begin.clone(), p::OperationState::Succeeded),
    )
    .unwrap();
    assert!(host.remove_local_attachment(&id).is_err());
    assert!(host.upload_record(&id).unwrap().blocks_epoch(1));
    host.cancel_upload_with(&id, |command| {
        let ApiIntent::CancelTask(request) = &command else {
            panic!("expected task cancellation")
        };
        let ApiIntent::BeginTask(original) = &begin else {
            panic!("expected task origin")
        };
        assert_eq!(request.task, original.task);
        assert_eq!(
            request.origin.as_ref().unwrap().command_id,
            original.command.as_ref().unwrap().command_id
        );
        let response = receipt(&host, command, p::OperationState::Succeeded);
        async { Ok(response) }
    })
    .await
    .unwrap();
    let host = restored(&path, &host);
    assert_eq!(retained_source(&host, &id), SOURCE);
    assert!(!host.upload_record(&id).unwrap().blocks_epoch(1));
    host.remove_local_attachment(&id).unwrap();
}
