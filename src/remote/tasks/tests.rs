use super::*;
use crate::integrations::TaskProvider;
use futures_lite::io::AsyncReadExt;
use gpui_kit::http_client::{
    AsyncBody, HttpClient, Request as HttpRequest, Response as HttpResponse, Url, http::HeaderValue,
};
use serde_json::{Value, json};
use std::{collections::VecDeque, pin::Pin, sync::Mutex};
struct Http {
    replies: Mutex<VecDeque<(u16, Value)>>,
    sent: Arc<Mutex<Vec<(String, Value)>>>,
}
impl Http {
    fn new(replies: Vec<(u16, Value)>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            sent: Arc::new(Mutex::new(vec![])),
        })
    }
}
impl HttpClient for Http {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }
    fn proxy(&self) -> Option<&Url> {
        None
    }
    fn send(
        &self,
        r: HttpRequest<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = gpui_kit::http_client::Result<HttpResponse<AsyncBody>>> + Send>>
    {
        let (status, reply) = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected provider request");
        let sent = self.sent.clone();
        Box::pin(async move {
            let method = r.method().to_string();
            let mut bytes = vec![];
            r.into_body().read_to_end(&mut bytes).await?;
            sent.lock().unwrap().push((
                method,
                serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            ));
            Ok(HttpResponse::builder()
                .status(status)
                .body(AsyncBody::from(reply.to_string()))?)
        })
    }
}
struct Host;
#[tonic::async_trait]
impl TasksHost for Host {
    async fn configure(
        &self,
        _: String,
        _: crate::command_bus::tasks::TaskConfiguration,
    ) -> Result<(), Status> {
        panic!("unexpected config effect")
    }
    async fn save_filter(&self, _: i::filters::TaskFilter) -> Result<(), Status> {
        panic!("unexpected config effect")
    }
    async fn remove_filter(&self, _: String) -> Result<(), Status> {
        panic!("unexpected config effect")
    }
    async fn task_to_worktree(
        &self,
        _: crate::thread::ProjectRef,
        _: i::TaskRef,
        _: String,
    ) -> Result<String, Status> {
        panic!("unexpected worktree effect")
    }
}
async fn fixture(dir: &std::path::Path) -> (TasksRpc, DeviceGrant) {
    let client = crate::settings::SettingsClient::create(&dir.join("db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    client
        .register_runtime_device(
            crate::host::DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "test".into(),
                vec![8; 32],
                TASKS,
                vec!["private".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![8; 32]).await.unwrap();
    let artifacts = Arc::new(ArtifactStore::open(client.clone(), dir.join("artifacts")).unwrap());
    (
        TasksRpc::new(
            RpcAuth::new(host.id().into(), client).unwrap(),
            WorkspaceRoots::default(),
            ResourceLocks::default(),
            Http::new(vec![]),
            artifacts,
            Arc::new(Host),
        ),
        grant,
    )
}
fn meta(_: &TasksRpc) -> proto::CommandMeta {
    proto::CommandMeta {
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: None,
    }
}
fn environment(rpc: &TasksRpc) -> proto::TaskProject {
    proto::TaskProject {
        resource: Some(proto::task_project::Resource::Environment(
            proto::EnvironmentRef {
                host_id: rpc.auth.host_id().into(),
            },
        )),
        integration_id: uuid::Uuid::new_v4().to_string(),
        project_key: "team/repo".into(),
    }
}

#[tokio::test]
async fn cancelled_rpc_keeps_accepted_effect_and_durable_unknown_is_never_retried() {
    let dir = tempfile::tempdir().unwrap();
    let (rpc, g) = fixture(dir.path()).await;
    let context = environment(&rpc);
    let (prepared, resource) = rpc
        .prepare(
            &g,
            meta(&rpc),
            &context,
            TaskCommand::Mutate {
                integration: context.integration_id.clone(),
                project: context.project_key.clone(),
                task: Some("42".into()),
                action: crate::command_bus::tasks::TaskAction::Watching(true),
            },
        )
        .await
        .unwrap();
    let mut input = prepared.runtime_input().clone();
    crate::command_bus::tasks::AccountBinding::bind(
        &mut input,
        i::Account {
            id: context.integration_id.clone(),
            provider: i::Provider::Github,
            login: "test".into(),
            credential: "credential-reference".into(),
            scope: i::AccountScope::Default,
        },
    )
    .unwrap();
    let key = input.key().clone();
    let (started, wait) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let worker = rpc.clone();
    let first = input.clone();
    let first_resource = resource.clone();
    let task = tokio::spawn(async move {
        worker
            .mutation(first, first_resource, TASKS, false, move || async move {
                Ok(async move {
                    started.send(()).unwrap();
                    released.await.unwrap();
                    Err(Status::unknown("Provider disconnected after dispatch."))
                })
            })
            .await
    });
    wait.await.unwrap();
    task.abort();
    release.send(()).unwrap();
    rpc.locks.drain().await.unwrap();
    let saved = rpc
        .auth
        .client()
        .get_runtime_command(key.clone(), TASKS)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.state(), CommandState::DeliveryUnknown);
    let output = rpc.auth.receipt(&saved).unwrap();
    assert_eq!(output.error.unwrap().code, "delivery_unknown");
    // New service instance after drain shares the durable inbox, never a retry.
    let retry = TasksRpc::new(
        rpc.auth.clone(),
        rpc.roots.clone(),
        ResourceLocks::default(),
        rpc.http.clone(),
        rpc.artifacts.clone(),
        rpc.host.clone(),
    );
    let result = retry
        .mutation(input, resource, TASKS, false, || async {
            Ok(async { panic!("must not repeat an uncertain provider mutation") })
        })
        .await
        .unwrap();
    assert_eq!(result.state, proto::OperationState::DeliveryUnknown as i32);
    rpc.auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn accepted_task_keeps_original_account_and_reconfiguration_rejects_before_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let (rpc, grant) = fixture(dir.path()).await;
    let context = environment(&rpc);
    let command = meta(&rpc);
    let intent = TaskCommand::Mutate {
        integration: context.integration_id.clone(),
        project: context.project_key.clone(),
        task: Some("42".into()),
        action: crate::command_bus::tasks::TaskAction::Watching(true),
    };
    let (first, _) = rpc
        .prepare(&grant, command.clone(), &context, intent.clone())
        .await
        .unwrap();
    let account = i::Account {
        id: context.integration_id.clone(),
        provider: i::Provider::Github,
        login: "original".into(),
        credential: "original-immutable-reference".into(),
        scope: i::AccountScope::Default,
    };
    let mut input = first.runtime_input().clone();
    crate::command_bus::tasks::AccountBinding::bind(&mut input, account.clone()).unwrap();
    rpc.auth
        .client()
        .accept_runtime_command(input, TASKS)
        .await
        .unwrap();
    let (retry, resource) = rpc
        .prepare(&grant, command.clone(), &context, intent)
        .await
        .unwrap();
    let (input, retained) = rpc
        .account_input(&retry, &context.integration_id)
        .await
        .unwrap();
    assert_eq!(retained, account);
    // Configuration removal must become a durable rejection, with no credential
    // lookup or provider request. A changed credential/endpoint fails the same fence.
    let worker = rpc.clone();
    let receipt = rpc
        .mutation(input, resource, TASKS, false, move || async move {
            worker.current_account(&retained).await?;
            Ok(async { panic!("removed account must not dispatch") })
        })
        .await
        .unwrap();
    assert_eq!(receipt.state, proto::OperationState::Rejected as i32);
    assert_eq!(receipt.error.as_ref().unwrap().code, "conditions_changed");
    let (saved, _) = rpc
        .prepare(
            &grant,
            command.clone(),
            &context,
            TaskCommand::Mutate {
                integration: context.integration_id.clone(),
                project: context.project_key.clone(),
                task: Some("42".into()),
                action: crate::command_bus::tasks::TaskAction::Watching(true),
            },
        )
        .await
        .unwrap();
    assert_eq!(saved.prior().unwrap().state(), CommandState::Rejected);
    let conflict = rpc
        .prepare(
            &grant,
            command,
            &context,
            TaskCommand::Mutate {
                integration: context.integration_id.clone(),
                project: context.project_key.clone(),
                task: Some("42".into()),
                action: crate::command_bus::tasks::TaskAction::Watching(false),
            },
        )
        .await
        .err()
        .unwrap();
    assert_eq!(conflict.code(), tonic::Code::Aborted);
    rpc.auth
        .client()
        .revoke_runtime_device(grant.id().into())
        .await
        .unwrap();
    assert_eq!(
        rpc.prepare(
            &grant,
            meta(&rpc),
            &context,
            TaskCommand::RemoveFilter(uuid::Uuid::new_v4().to_string())
        )
        .await
        .err()
        .unwrap()
        .code(),
        tonic::Code::PermissionDenied
    );
    rpc.auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn accepted_destructive_task_is_rejected_without_replaying_the_host_effect() {
    let dir = tempfile::tempdir().unwrap();
    let (rpc, grant) = fixture(dir.path()).await;
    let command = meta(&rpc);
    let context = environment(&rpc);
    let filter_id = uuid::Uuid::new_v4().to_string();
    let (prepared, _) = rpc
        .prepare(
            &grant,
            command.clone(),
            &context,
            TaskCommand::RemoveFilter(filter_id.clone()),
        )
        .await
        .unwrap();
    rpc.auth
        .client()
        .accept_runtime_command(prepared.runtime_input().clone(), TASKS)
        .await
        .unwrap();
    let request = proto::RemoveFilterRequest {
        command: Some(command),
        environment: Some(proto::EnvironmentRef {
            host_id: rpc.auth.host_id().into(),
        }),
        filter_id,
    };
    let rejected = rpc.remove(grant.clone(), request.clone()).await.unwrap();
    assert_eq!(rejected.state, proto::OperationState::Rejected as i32);
    assert_eq!(rpc.remove(grant, request).await.unwrap(), rejected);
    rpc.auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn removed_project_rejects_an_accepted_task_before_provider_access() {
    let dir = tempfile::tempdir().unwrap();
    let (rpc, _) = fixture(dir.path()).await;
    let root = dir.path().canonicalize().unwrap();
    rpc.auth
        .client()
        .register_runtime_device(
            crate::host::DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "project device".into(),
                vec![9; 32],
                TASKS,
                vec![root.to_str().unwrap().into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = rpc
        .auth
        .client()
        .lookup_runtime_device(vec![9; 32])
        .await
        .unwrap();
    let mut projects = crate::state::projects::Projects::default();
    projects.open(root.clone());
    let context = proto::TaskProject {
        resource: Some(proto::task_project::Resource::Project(proto::ProjectRef {
            host_id: rpc.auth.host_id().into(),
            project_id: serde_json::to_string(&projects.current().unwrap().id).unwrap(),
            worktree_id: None,
        })),
        integration_id: uuid::Uuid::new_v4().to_string(),
        project_key: "team/repo".into(),
    };
    rpc.roots.replace_projects(projects.items).unwrap();
    let meta = meta(&rpc);
    let command = TaskCommand::Mutate {
        integration: context.integration_id.clone(),
        project: context.project_key.clone(),
        task: Some("42".into()),
        action: crate::command_bus::tasks::TaskAction::Watching(true),
    };
    let (prepared, _) = rpc
        .prepare(&grant, meta.clone(), &context, command.clone())
        .await
        .unwrap();
    let mut input = prepared.runtime_input().clone();
    crate::command_bus::tasks::AccountBinding::bind(
        &mut input,
        i::Account {
            id: context.integration_id.clone(),
            provider: i::Provider::Github,
            login: "test".into(),
            credential: "immutable-reference".into(),
            scope: i::AccountScope::Default,
        },
    )
    .unwrap();
    rpc.auth
        .client()
        .accept_runtime_command(input, TASKS)
        .await
        .unwrap();
    rpc.roots.replace_projects(vec![]).unwrap();
    let (retry, resource) = rpc.prepare(&grant, meta, &context, command).await.unwrap();
    let rejected = rpc
        .mutation(
            retry.runtime_input().clone(),
            resource,
            TASKS,
            false,
            || async {
                panic!("removed project must not resolve credentials or contact the provider");
                #[allow(unreachable_code)]
                Ok(async { Ok(vec![]) })
            },
        )
        .await
        .unwrap();
    assert_eq!(rejected.state, proto::OperationState::Rejected as i32);
    assert_eq!(rejected.error.unwrap().code, "conditions_changed");
    rpc.auth.client().shutdown().await.unwrap();
}
#[tokio::test]
async fn restarted_worktree_with_removed_target_is_rejected_durably_and_releases_epoch() {
    use crate::command_bus::tasks::AccountBinding;
    let dir = tempfile::tempdir().unwrap();
    let (rpc, _) = fixture(dir.path()).await;
    let root = dir.path().canonicalize().unwrap();
    let scopes = TASKS | crate::host::FILES_GIT;
    rpc.auth
        .client()
        .register_runtime_device(
            crate::host::DeviceRegistration::new(
                uuid::Uuid::new_v4().to_string(),
                "project device".into(),
                vec![9; 32],
                scopes,
                vec![root.to_str().unwrap().into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = rpc
        .auth
        .client()
        .lookup_runtime_device(vec![9; 32])
        .await
        .unwrap();
    let mut projects = crate::state::projects::Projects::default();
    projects.open_worktree(root.clone(), root);
    let project = projects.current().unwrap();
    let context = proto::TaskProject {
        resource: Some(proto::task_project::Resource::Project(proto::ProjectRef {
            host_id: rpc.auth.host_id().into(),
            project_id: serde_json::to_string(&project.id).unwrap(),
            worktree_id: Some(serde_json::to_string(&project.workspace).unwrap()),
        })),
        ..environment(&rpc)
    };
    rpc.roots.replace_projects(projects.items).unwrap();
    let command = meta(&rpc);
    let request = proto::TaskToWorktreeRequest {
        command: Some(command.clone()),
        task: Some(proto::TaskRef {
            project: Some(context.clone()),
            task_id: "42".into(),
        }),
        branch: "task-42".into(),
    };
    let (prepared, _) = rpc
        .prepare(
            &grant,
            command,
            &context,
            TaskCommand::Worktree {
                integration: context.integration_id.clone(),
                project: context.project_key.clone(),
                task: "42".into(),
                branch: request.branch.clone(),
            },
        )
        .await
        .unwrap();
    let mut input = prepared.runtime_input().clone();
    AccountBinding::bind(
        &mut input,
        i::Account {
            id: context.integration_id,
            provider: i::Provider::Github,
            login: "test".into(),
            credential: "immutable-reference".into(),
            scope: i::AccountScope::Default,
        },
    )
    .unwrap();
    let key = input.key().clone();
    rpc.auth
        .client()
        .accept_runtime_command(input, scopes)
        .await
        .unwrap();
    rpc.auth.client().shutdown().await.unwrap();
    let host_id = rpc.auth.host_id().to_owned();
    drop(rpc);

    let client = crate::settings::SettingsClient::open(
        dir.path().join("db"),
        crate::settings::Access::ReadWrite,
    )
    .await
    .unwrap();
    client.initialize_runtime().await.unwrap();
    let rpc = TasksRpc::new(
        RpcAuth::new(host_id, client.clone()).unwrap(),
        WorkspaceRoots::default(),
        ResourceLocks::default(),
        Http::new(vec![]),
        Arc::new(ArtifactStore::open(client.clone(), dir.path().join("artifacts")).unwrap()),
        Arc::new(Host),
    );
    let grant = client.lookup_runtime_device(vec![9; 32]).await.unwrap();
    assert_eq!(
        client
            .get_runtime_command(key.clone(), scopes)
            .await
            .unwrap()
            .unwrap()
            .state(),
        CommandState::Accepted
    );
    let receipt = rpc.worktree(grant.clone(), request.clone()).await.unwrap();
    assert_eq!(receipt.state, proto::OperationState::Rejected as i32);
    assert_eq!(receipt.error.as_ref().unwrap().code, "conditions_changed");
    assert_eq!(
        rpc.worktree(grant.clone(), request.clone()).await.unwrap(),
        receipt
    );
    assert_eq!(
        client
            .get_runtime_command(key, scopes)
            .await
            .unwrap()
            .unwrap()
            .state(),
        CommandState::Rejected
    );
    let restricted = client
        .update_runtime_device_authorization(
            grant.id().into(),
            grant.authorization_revision(),
            scopes,
            vec!["private".into()],
        )
        .await
        .unwrap();
    assert_eq!(
        rpc.worktree(grant.clone(), request.clone())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    client
        .update_runtime_device_authorization(
            grant.id().into(),
            restricted.authorization_revision(),
            scopes,
            vec![dir.path().canonicalize().unwrap().to_str().unwrap().into()],
        )
        .await
        .unwrap();
    assert_eq!(
        client
            .close_runtime_command_epoch(grant.id().into(), 1)
            .await
            .unwrap(),
        2
    );
    client
        .revoke_runtime_device(grant.id().into())
        .await
        .unwrap();
    assert_eq!(
        rpc.worktree(grant, request).await.unwrap_err().code(),
        tonic::Code::PermissionDenied
    );
    client.shutdown().await.unwrap();
}

#[tokio::test]
async fn combined_status_and_fields_are_rejected_before_account_or_target_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let (rpc, grant) = fixture(dir.path()).await;
    let error = rpc
        .update(
            grant,
            proto::UpdateTaskRequest {
                command: Some(meta(&rpc)),
                task: Some(proto::TaskRef {
                    project: Some(environment(&rpc)),
                    task_id: "42".into(),
                }),
                title: Some("New title".into()),
                status: Some(proto::TaskStatusChoice::default()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert_eq!(
        error.message(),
        "Submit status separately from other fields."
    );
    rpc.auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn tls_identity_and_canonical_grants_are_required_for_reads() {
    use proto::tasks_service_server::TasksService;
    let dir = tempfile::tempdir().unwrap();
    let (rpc, g) = fixture(dir.path()).await;
    let error = rpc
        .list_projects(Request::new(proto::ListProjectsRequest {
            project: Some(environment(&rpc)),
            ..Default::default()
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unauthenticated);
    assert_eq!(
        rpc.authorize_scope(&g, &environment(&rpc), TASKS)
            .await
            .unwrap(),
        "private"
    );
    let mut projects = crate::state::projects::Projects::default();
    projects.open(dir.path().canonicalize().unwrap());
    let project = projects.current().unwrap();
    let reference = proto::ProjectRef {
        host_id: rpc.auth.host_id().into(),
        project_id: serde_json::to_string(&project.id).unwrap(),
        worktree_id: None,
    };
    rpc.roots.replace_projects(projects.items).unwrap();
    let project = proto::TaskProject {
        resource: Some(proto::task_project::Resource::Project(reference)),
        ..environment(&rpc)
    };
    assert_eq!(
        rpc.authorize_scope(&g, &project, TASKS)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::PermissionDenied
    );
    rpc.auth.client().shutdown().await.unwrap();
}
fn jira_task() -> i::TaskRef {
    i::TaskRef {
        project: i::ProjectTarget::jira("https://team.atlassian.net", "CAN").unwrap(),
        id: "CAN-42".into(),
        title: "Task".into(),
    }
}
fn jira_issue() -> Value {
    json!({"id":"42","key":"CAN-42","fields":{"summary":"Task","description":null,"status":{"name":"Open","statusCategory":{"key":"new"}}}})
}
#[tokio::test]
async fn jira_detail_projection_keeps_native_link_ids_watch_state_and_adf_for_actual_actions() {
    let mut issue = jira_issue();
    issue["names"] = json!({"customfield_10020":"Sprint"});
    issue["fields"]["issuelinks"] = json!([{"id":"9007199254740993","type":{"name":"Relates","outward":"relates to"},"outwardIssue":{"key":"CAN-43"}}]);
    issue["fields"]["watches"] = json!({"isWatching":true,"watchCount":3});
    issue["fields"]["votes"] = json!({"hasVoted":true,"votes":2});
    issue["fields"]["worklog"] = json!({"total":1,"worklogs":[{"id":"70","timeSpentSeconds":600}]});
    issue["fields"]["customfield_10020"] = json!([{"id":45,"name":"Current sprint"}]);
    issue["fields"]["description"] = json!({"version":1,"type":"doc","content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"opaque-user","text":"user"}}]}]});
    let http = Http::new(vec![
        (200, issue.clone()),
        (200, issue.clone()),
        (204, Value::Null),
        (200, issue.clone()),
    ]);
    let provider = i::jira::Jira::new(
        http.clone(),
        "https://team.atlassian.net",
        "test@example.com",
        None,
        "dummy".into(),
        "me".into(),
    )
    .unwrap();
    let task = provider.task(&jira_task()).await.unwrap();
    let projected = mapping::task(task, proto::TaskRef::default());
    let field = |id: &str| {
        mapping::json(
            projected
                .fields
                .iter()
                .find(|f| f.id == id)
                .unwrap()
                .current_value
                .as_ref()
                .unwrap(),
        )
        .unwrap()
    };
    let links = field("issuelinks");
    let id = links[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(id, "9007199254740993");
    assert_eq!(field("watches")["isWatching"], true);
    assert_eq!(field("votes")["hasVoted"], true);
    assert_eq!(field("worklog")["worklogs"][0]["id"], "70");
    assert_eq!(field("description"), issue["fields"]["description"]);
    assert_eq!(
        projected
            .fields
            .iter()
            .find(|f| f.id == "customfield_10020")
            .unwrap()
            .name,
        "Sprint"
    );
    provider
        .write(&i::TaskWrite::Jira {
            project: jira_task().project,
            task: Some(jira_task()),
            action: i::jira::JiraWrite::DeleteLink { id },
        })
        .await
        .unwrap();
    assert_eq!(
        http.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, _)| method == "DELETE")
            .count(),
        1
    );
}
#[tokio::test]
async fn required_transition_form_preserves_native_value_and_never_submits_default() {
    let transition = json!({"transitions":[{"id":"finish/opaque","name":"Finish","to":{"name":"Done"},"fields":{"resolution":{"name":"Resolution","required":true,"schema":{"type":"resolution"},"allowedValues":[{"id":"9007199254740993","name":"Fixed"}],"defaultValue":{"id":"default"}}}}]});
    // Jira validates identifiers; use its valid opaque identifier grammar.
    let mut transition = transition;
    transition["transitions"][0]["id"] = json!("finish:opaque");
    let http = Http::new(vec![(200, jira_issue()), (200, transition.clone())]);
    let provider = i::jira::Jira::new(
        http.clone(),
        "https://team.atlassian.net",
        "test@example.com",
        None,
        "dummy".into(),
        "me".into(),
    )
    .unwrap();
    let choice = proto::TaskStatusChoice {
        field_id: "jira-status".into(),
        choice_id: "finish:opaque".into(),
        ..Default::default()
    };
    let err = writes::status_command(
        &provider,
        &jira_task(),
        decode::status(choice.clone()).unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(http.sent.lock().unwrap().iter().all(|(m, _)| m == "GET"));
    let http = Http::new(vec![
        (200, jira_issue()),
        (200, transition),
        (204, Value::Null),
        (200, jira_issue()),
    ]);
    let provider = i::jira::Jira::new(
        http.clone(),
        "https://team.atlassian.net",
        "test@example.com",
        None,
        "dummy".into(),
        "me".into(),
    )
    .unwrap();
    let mut choice = choice;
    choice.form_fields = Some(prost_types::Struct {
        fields: std::collections::BTreeMap::from([(
            "resolution".into(),
            mapping::value(&json!({"id":"9007199254740993"})),
        )]),
    });
    let command = writes::status_command(&provider, &jira_task(), decode::status(choice).unwrap())
        .await
        .unwrap()
        .unwrap();
    provider.write(&command).await.unwrap();
    let sent = http.sent.lock().unwrap();
    let (_, body) = sent.iter().find(|(method, _)| method == "POST").unwrap();
    assert_eq!(body["fields"]["resolution"]["id"], "9007199254740993");
    assert_eq!(body["transition"]["id"], "finish:opaque");
}
#[test]
fn structured_values_reject_nonfinite_numbers_and_preserve_nested_native_identifiers() {
    let input = json!({"accountId":"000123:abc","child":{"id":"9007199254740993"},"values":[true,null,"x"]});
    assert_eq!(mapping::json(&mapping::value(&input)).unwrap(), input);
    assert!(
        mapping::json(&prost_types::Value {
            kind: Some(prost_types::value::Kind::NumberValue(f64::NAN))
        })
        .is_err()
    );
    let mut deep = mapping::value(&Value::Null);
    for _ in 0..34 {
        deep = prost_types::Value {
            kind: Some(prost_types::value::Kind::ListValue(
                prost_types::ListValue { values: vec![deep] },
            )),
        };
    }
    assert!(mapping::json(&deep).is_err());
}

#[tokio::test]
async fn github_search_quotes_user_qualifiers_and_rejects_cross_repository_results() {
    let http = Http::new(vec![(
        200,
        json!({"data":{"search":{"issueCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[{"repository":{"nameWithOwner":"other/private"}}]}}}),
    )]);
    let provider = i::github::Github::new(http.clone(), "dummy".into());
    let project = i::ProjectTarget::github("team/repo").unwrap();
    let result = provider
        .list_context(
            &project,
            i::TaskState::Open,
            None,
            &i::TaskQuery {
                text: "hello\" repo:other/private".into(),
                ..Default::default()
            },
        )
        .await;
    assert!(result.unwrap_err().contains("outside this repository"));
    let sent = http.sent.lock().unwrap();
    let query = sent[0].1["variables"]["query"].as_str().unwrap();
    assert_eq!(
        query,
        "repo:team/repo is:issue is:open sort:updated-desc \"hello\\\" repo:other/private\""
    );
}

#[tokio::test]
async fn task_download_artifact_is_bound_not_reusable_as_upload_and_revoked_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let (rpc, g) = fixture(dir.path()).await;
    let integration = uuid::Uuid::new_v4().to_string();
    let target = crate::artifacts::Target::Task {
        integration_id: integration.clone(),
        project_id: "CAN".into(),
        task_id: "CAN-42".into(),
        root: "private".into(),
    };
    let artifact = rpc
        .publish_attachment(
            g.id().into(),
            target.clone(),
            "text/plain".into(),
            b"provider attachment".to_vec(),
        )
        .await
        .unwrap();
    let store = rpc.artifacts.clone();
    let owner = g.id().to_owned();
    let id = artifact.artifact_id.clone();
    let saved = tokio::task::spawn_blocking(move || {
        let status = store.status(&owner, &id).unwrap();
        assert_eq!(status.binding().target(), &target);
        store.download(&owner, &id, 0, 65536).unwrap()
    })
    .await
    .unwrap();
    assert_eq!(saved, b"provider attachment");
    let provider = i::jira::Jira::new(
        Http::new(vec![]),
        "https://team.atlassian.net",
        "test@example.com",
        None,
        "dummy".into(),
        "me".into(),
    )
    .unwrap();
    let p = proto::TaskProject {
        integration_id: integration,
        project_key: "CAN".into(),
        ..environment(&rpc)
    };
    let task = jira_task();
    let result = rpc
        .command(
            writes::MutationContext {
                provider: &provider,
                project: &task.project,
                task: Some(&task),
                grant: &g,
                reference: &p,
                root: "private",
            },
            decode::action(proto::apply_mutation_request::Action::Attachment(
                proto::TaskAttachmentWrite {
                    action: Some(proto::task_attachment_write::Action::Attach(
                        artifact.clone(),
                    )),
                    name: "test.txt".into(),
                },
            ))
            .unwrap(),
        )
        .await;
    assert_eq!(result.unwrap_err().code(), tonic::Code::PermissionDenied);
    rpc.auth
        .client()
        .revoke_runtime_device(g.id().into())
        .await
        .unwrap();
    let store = rpc.artifacts.clone();
    let owner = g.id().to_owned();
    let id = artifact.artifact_id;
    assert!(
        tokio::task::spawn_blocking(move || store.download(&owner, &id, 0, 65536))
            .await
            .unwrap()
            .is_err()
    );
    rpc.auth.client().shutdown().await.unwrap();
}

#[tokio::test]
async fn cached_provider_transport_rechecks_grants_between_preflight_and_write() {
    let dir = tempfile::tempdir().unwrap();
    let (mut rpc, g) = fixture(dir.path()).await;
    let account = i::Account {
        id: uuid::Uuid::new_v4().to_string(),
        provider: i::Provider::Github,
        login: "test".into(),
        credential: uuid::Uuid::new_v4().to_string(),
        scope: i::AccountScope::Default,
    };
    let mut config = i::Config::default();
    config.accounts.push(account.clone());
    rpc.auth.client().save_integrations(config).await.unwrap();
    let http = Http::new(vec![(200, json!({}))]);
    rpc.http = http.clone();
    let authorized = rpc.scoped_http(&account, &g, "private");
    authorized
        .send(
            HttpRequest::builder()
                .uri("https://api.github.com/preflight")
                .body(AsyncBody::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    rpc.auth
        .client()
        .revoke_runtime_device(g.id().into())
        .await
        .unwrap();
    let error = authorized
        .send(
            HttpRequest::builder()
                .method("POST")
                .uri("https://api.github.com/change")
                .body(AsyncBody::empty())
                .unwrap(),
        )
        .await;
    assert!(error.is_err());
    assert_eq!(http.sent.lock().unwrap().len(), 1);
    rpc.auth.client().shutdown().await.unwrap();
}
