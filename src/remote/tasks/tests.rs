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
    async fn save_filter(&self, _: i::filters::TaskFilter) -> Result<(), Status> {
        panic!("unexpected config effect")
    }
    async fn remove_filter(&self, _: String) -> Result<(), Status> {
        panic!("unexpected config effect")
    }
    async fn task_to_worktree(
        &self,
        _: proto::WorkspaceRef,
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
fn meta(rpc: &TasksRpc) -> proto::CommandMeta {
    proto::CommandMeta {
        host_id: rpc.auth.host_id().into(),
        command_id: uuid::Uuid::new_v4().to_string(),
        command_epoch: 1,
        expected_revision: None,
    }
}

#[tokio::test]
async fn cancelled_rpc_keeps_accepted_effect_and_durable_unknown_is_never_retried() {
    let dir = tempfile::tempdir().unwrap();
    let (rpc, g) = fixture(dir.path()).await;
    let input = rpc
        .auth
        .input(
            &meta(&rpc),
            &g,
            "tasks.mutate",
            "issue".into(),
            vec![1],
            "private".into(),
        )
        .unwrap();
    let key = input.key().clone();
    let (started, wait) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let worker = rpc.clone();
    let first = input.clone();
    let task = tokio::spawn(async move {
        worker
            .mutation(first, move || async move {
                started.send(()).unwrap();
                released.await.unwrap();
                Err(Status::unknown("Provider disconnected after dispatch."))
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
    let output = rpc.auth.receipt(&saved);
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
        .mutation(input, || async {
            panic!("must not repeat an uncertain provider mutation")
        })
        .await
        .unwrap();
    assert_eq!(result.state, proto::OperationState::DeliveryUnknown as i32);
    rpc.auth.client().shutdown().await.unwrap();
}
#[tokio::test]
async fn tls_identity_and_canonical_grants_are_required_for_reads() {
    use proto::tasks_service_server::TasksService;
    let dir = tempfile::tempdir().unwrap();
    let (rpc, g) = fixture(dir.path()).await;
    let error = rpc
        .list_projects(Request::new(proto::TaskProjectsRequest {
            host_id: rpc.auth.host_id().into(),
            ..Default::default()
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unauthenticated);
    assert_eq!(
        rpc.authorize(&g, rpc.auth.host_id(), None).await.unwrap(),
        "private"
    );
    let root = dir.path().canonicalize().unwrap();
    rpc.roots
        .replace(std::collections::HashMap::from([(
            "workspace".into(),
            root,
        )]))
        .unwrap();
    let workspace = proto::WorkspaceRef {
        host_id: rpc.auth.host_id().into(),
        workspace_id: "workspace".into(),
    };
    assert_eq!(
        rpc.authorize(&g, rpc.auth.host_id(), Some(&workspace))
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
    let err = writes::status_command(&provider, &jira_task(), choice.clone())
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
    let command = writes::status_command(&provider, &jira_task(), choice)
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
        host_id: rpc.auth.host_id().into(),
        integration_id: integration,
        project_id: "CAN".into(),
        workspace: None,
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
            proto::task_mutation_request::Action::Attachment(proto::TaskAttachmentWrite {
                action: Some(proto::task_attachment_write::Action::Attach(
                    artifact.clone(),
                )),
                name: "test.txt".into(),
            }),
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
