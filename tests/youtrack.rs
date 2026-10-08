use canopy_desktop::integrations::{
    Account, AccountScope, Config, ProjectTarget, Provider, TaskProvider, TaskQuery, TaskRef,
    TaskState, TaskWrite, WriteError,
    youtrack::{self, Youtrack, YoutrackField, YoutrackFieldKind, YoutrackWrite},
};
use futures_lite::{future::block_on, io::AsyncReadExt};
use gpui_kit::http_client::{
    AsyncBody, HttpClient, RedirectPolicy, Request, Response, Url, http::HeaderValue,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
struct Sent {
    method: String,
    path: String,
    body: Value,
}
struct Reply {
    status: u16,
    body: String,
}
fn reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        body: body.to_string(),
    }
}
struct Mock {
    replies: Mutex<VecDeque<Reply>>,
    sent: Arc<Mutex<Vec<Sent>>>,
}
impl Mock {
    fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            sent: Arc::new(Mutex::new(vec![])),
        })
    }
}
impl HttpClient for Mock {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }
    fn proxy(&self) -> Option<&Url> {
        None
    }
    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = gpui_kit::http_client::Result<Response<AsyncBody>>> + Send>>
    {
        assert_eq!(request.uri().host(), Some("issues.example.com"));
        let authorization = request.headers().get("Authorization").unwrap();
        assert_eq!(authorization, "Bearer private-token");
        assert!(authorization.is_sensitive());
        assert_eq!(
            request.extensions().get::<RedirectPolicy>(),
            Some(&RedirectPolicy::NoFollow)
        );
        let method = request.method().to_string();
        let path = request.uri().path_and_query().unwrap().to_string();
        let sent = self.sent.clone();
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected YouTrack request");
        Box::pin(async move {
            let mut bytes = vec![];
            request.into_body().read_to_end(&mut bytes).await?;
            sent.lock().unwrap().push(Sent {
                method,
                path,
                body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            });
            Ok(Response::builder()
                .status(reply.status)
                .body(AsyncBody::from(reply.body))?)
        })
    }
}
fn service() -> &'static str {
    "https://issues.example.com/youtrack"
}
fn project() -> ProjectTarget {
    ProjectTarget::youtrack(service(), "can").unwrap()
}
fn task() -> TaskRef {
    TaskRef {
        project: project(),
        id: "CAN-42".into(),
        title: "Task".into(),
    }
}
fn yt(mock: Arc<Mock>) -> Youtrack {
    Youtrack::new(mock, service(), "private-token".into(), "alex".into()).unwrap()
}
fn issue() -> Value {
    json!({
        "id":"2-42","idReadable":"CAN-42","summary":"Task","description":"# Body",
        "project":{"id":"0-1","shortName":"CAN","name":"Canopy"},
        "resolved":null,"reporter":{"login":"alex","fullName":"Alex"},
        "tags":[{"id":"tag-1","name":"backend"}],
        "customFields":[
            {"id":"82-11","name":"State","$type":"StateIssueCustomField","value":{"name":"Open","isResolved":false},"projectCustomField":{"canBeEmpty":false,"field":{"fieldType":{"id":"state[1]","isMultiValue":false}},"bundle":{"id":"state-1"}},"possibleEvents":[{"id":"in-progress","presentation":"In progress"}]},
            {"id":"82-12","name":"Area","$type":"SingleEnumIssueCustomField","value":{"name":"Backend"},"projectCustomField":{"field":{"fieldType":{"id":"enum[1]","isMultiValue":false}}}}
        ],
        "attachments":[{"id":"file-1","name":"readme.txt","size":4,"mimeType":"text/plain","url":"/api/files/readme.txt?token=ok"}]
    })
}
#[test]
fn url_identity_and_config_are_site_scoped() {
    assert_eq!(
        youtrack::normalize_service("https://ISSUES.example.com/youtrack/").unwrap(),
        service()
    );
    for value in [
        "http://issues.example.com",
        "https://user:pass@issues.example.com",
        "https://issues.example.com/../x",
        "https://issues.example.com/youtrack?q=x",
    ] {
        assert!(youtrack::normalize_service(value).is_err());
    }
    assert_eq!(project().key, "CAN");
    assert!(task().valid());
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        provider: Provider::Youtrack,
        login: "alex".into(),
        credential: uuid::Uuid::new_v4().to_string(),
        scope: AccountScope::Youtrack {
            service: service().into(),
        },
    };
    let config = Config::default().with_account(account.clone()).unwrap();
    assert_eq!(config.account_for(&project()), Some(&account));
    let serialized = serde_json::to_string(&config).unwrap();
    assert!(!serialized.contains("private-token"));
    assert!(serde_json::from_str::<Config>(&serialized).unwrap().valid());
}
#[test]
fn issue_value_overlay_keeps_project_bundle_options() {
    let project = YoutrackField {
        id: "area".into(),
        name: "Area".into(),
        field_type: "SingleEnumIssueCustomField".into(),
        field_type_id: "enum[1]".into(),
        kind: YoutrackFieldKind::Enum,
        required: false,
        read_only: false,
        multi_value: false,
        bundle_id: Some("bundle-1".into()),
        value: Value::Null,
        default: Value::Null,
        allowed: vec![json!({"id":"backend","name":"Backend"})],
        events: vec![],
    };
    let current = YoutrackField {
        value: json!({"id":"frontend","name":"Frontend"}),
        allowed: vec![],
        ..project.clone()
    };
    let expected_options = project.allowed.clone();
    let merged = project.with_current_value(&current);
    assert_eq!(merged.allowed, expected_options);
    assert_eq!(merged.value, current.value);
}
#[test]
fn verifies_under_context_path_and_uses_bearer_header() {
    block_on(async {
        let mock = Mock::new(vec![reply(
            200,
            json!({"id":"1-1","login":"alex","fullName":"Alex"}),
        )]);
        assert_eq!(yt(mock.clone()).verify().await.unwrap(), "alex");
        assert_eq!(
            mock.sent.lock().unwrap()[0].path,
            "/youtrack/api/users/me?fields=id,login,name,fullName"
        );
    });
}
#[test]
fn project_pages_advance_by_raw_entries_and_skip_archived_rows() {
    block_on(async {
        let first: Vec<_> = (0..50).map(|i| json!({"id":format!("0-{i}"),"shortName":format!("P{i}"),"name":"Project","archived":i==1})).collect();
        let mock = Mock::new(vec![
            reply(200, Value::Array(first)),
            reply(
                200,
                json!([{"id":"0-51","shortName":"CAN","name":"Canopy","archived":false}]),
            ),
        ]);
        let provider = yt(mock.clone());
        let page = provider.projects(None).await.unwrap();
        assert_eq!(page.next_cursor.as_deref(), Some("50"));
        assert_eq!(page.items.len(), 49);
        let next = provider
            .projects(page.next_cursor.as_deref())
            .await
            .unwrap();
        assert_eq!(next.items[0].value, "CAN");
        assert!(mock.sent.lock().unwrap()[1].path.contains("$skip=50"));
    });
}
#[test]
fn list_uses_project_grouping_native_filters_and_light_projection() {
    block_on(async {
        let mock = Mock::new(vec![reply(200, json!([issue()]))]);
        let provider = yt(mock.clone());
        let page = provider
            .list_context(
                &project(),
                TaskState::Open,
                None,
                &TaskQuery {
                    text: "one OR two".into(),
                    filter: Some("#Unresolved OR State: {Won't fix}".into()),
                    reconcile: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(page.tasks[0].reference.id, "CAN-42");
        assert_eq!(page.tasks[0].youtrack.as_ref().unwrap().status, "Open");
        let request = &mock.sent.lock().unwrap()[0];
        assert!(request.path.contains("%28project%3A%20%7BCAN%7D%29"));
        assert!(
            request
                .path
                .contains("%29%20AND%20%28%23Unresolved%20OR%20State")
        );
        assert!(!request.path.contains("bundle(values"));
    });
}
#[test]
fn create_keeps_confirmed_issue_when_followup_read_fails() {
    block_on(async {
        let mock = Mock::new(vec![
            reply(
                200,
                json!([{"id":"0-1","shortName":"CAN","name":"Canopy","archived":false}]),
            ),
            reply(
                201,
                json!({"id":"2-99","idReadable":"CAN-99","summary":"New","description":"Body","project":{"id":"0-1","shortName":"CAN"}}),
            ),
            reply(503, json!({"error":"temporary"})),
        ]);
        let provider = yt(mock.clone());
        let receipt = provider
            .write(&TaskWrite::Youtrack {
                project: project(),
                task: None,
                action: YoutrackWrite::Create {
                    draft: canopy_desktop::integrations::IssueDraft {
                        title: "New".into(),
                        body: "Body".into(),
                        ..Default::default()
                    },
                    fields: Value::Null,
                },
            })
            .await
            .unwrap();
        assert_eq!(receipt.task.unwrap().reference.id, "CAN-99");
        let notice = receipt.notice.unwrap();
        assert!(notice.contains("Refresh") || notice.contains("refreshed"));
        assert_eq!(mock.sent.lock().unwrap()[1].body["project"]["id"], "0-1");
    });
}
#[test]
fn state_machine_tags_and_attachment_membership_use_issue_scoped_endpoints() {
    block_on(async {
        let mock = Mock::new(vec![
            reply(200, issue()),
            reply(204, Value::Null),
            reply(200, issue()),
            reply(200, issue()),
            reply(204, Value::Null),
            reply(200, issue()),
        ]);
        let provider = yt(mock.clone());
        provider
            .write(&TaskWrite::Youtrack {
                project: project(),
                task: Some(task()),
                action: YoutrackWrite::StateMachineEvent {
                    id: "82-11".into(),
                    event_id: "in-progress".into(),
                },
            })
            .await
            .unwrap();
        provider
            .write(&TaskWrite::Youtrack {
                project: project(),
                task: Some(task()),
                action: YoutrackWrite::Tag {
                    id: "tag-1".into(),
                    add: false,
                },
            })
            .await
            .unwrap();
        let sent = mock.sent.lock().unwrap();
        assert!(sent[1].path.contains("/customFields/82-11"));
        assert_eq!(sent[1].body["event"]["id"], "in-progress");
        assert_eq!(sent[4].method, "DELETE");
        assert!(sent[4].path.contains("/issues/2-42/tags/tag-1"));
    });
}
#[test]
fn uncertain_write_does_not_retry_or_leak_token() {
    block_on(async {
        let mock = Mock::new(vec![
            reply(200, issue()),
            reply(503, json!({"error_description":"private-token"})),
        ]);
        let error = yt(mock.clone())
            .write(&TaskWrite::Youtrack {
                project: project(),
                task: Some(task()),
                action: YoutrackWrite::Fields {
                    fields: json!({"summary":"Updated"}),
                },
            })
            .await
            .unwrap_err();
        assert!(matches!(error, WriteError::Uncertain(_)));
        assert!(!error.to_string().contains("private-token"));
        assert_eq!(mock.sent.lock().unwrap().len(), 2);
    });
}

mod immediate_status {
    use super::*;

    // Unlike the legacy issue fixture, normal state fields have no workflow events
    // and current values have provider IDs, not just display names.
    fn state_issue() -> Value {
        let mut value = issue();
        value["customFields"][0] = json!({
            "id":"82-11", "name":"State", "$type":"StateIssueCustomField",
            "readOnly":false,
            "value":{"id":"state-open", "name":"Open", "isResolved":false},
            "projectCustomField":{
                "id":"82-11", "canBeEmpty":false, "readOnly":false,
                "field":{"id":"58-1", "name":"State", "fieldType":{"id":"state[1]", "isMultiValue":false}},
                "bundle":{"id":"state-1", "$type":"StateBundle"}
            }
        });
        value["customFields"][1]["projectCustomField"]["bundle"] = json!({"id":"area-bundle"});
        value
    }

    fn workflow_issue() -> Value {
        let mut value = state_issue();
        value["customFields"][0]["$type"] = json!("StateMachineIssueCustomField");
        value["customFields"][0]["possibleEvents"] = json!([
            {"id":"event-start", "presentation":"Start work"},
            {"id":"event-close", "presentation":"Resolve"},
            {"id":"event-reopen", "presentation":null}
        ]);
        value
    }

    fn states() -> Value {
        json!([
            {"id":"state-open", "name":"Open", "archived":false, "isResolved":false},
            {"id":"state-progress", "name":"In progress", "archived":false, "isResolved":false},
            {"id":"state-old", "name":"Obsolete", "archived":true, "isResolved":true}
        ])
    }

    fn schema_replies() -> Vec<Reply> {
        vec![
            reply(
                200,
                json!([{"id":"0-1", "shortName":"CAN", "name":"Canopy", "archived":false}]),
            ),
            reply(
                200,
                json!([
                    {"id":"82-11", "$type":"StateProjectCustomField", "canBeEmpty":false,
                     "field":{"id":"58-1", "name":"State", "fieldType":{"id":"state[1]", "isMultiValue":false}},
                     "bundle":{"id":"state-1", "$type":"StateBundle"}},
                    {"id":"82-13", "$type":"SimpleProjectCustomField",
                     "field":{"name":"Notes", "fieldType":{"id":"text", "isMultiValue":false}}},
                    {"id":"82-14", "$type":"SimpleProjectCustomField",
                     "field":{"name":"Due date", "fieldType":{"id":"date", "isMultiValue":false}}}
                ]),
            ),
            reply(200, states()),
        ]
    }

    fn command(id: &str, choice: &str) -> TaskWrite {
        TaskWrite::Youtrack {
            project: project(),
            task: Some(task()),
            action: YoutrackWrite::Status {
                id: id.into(),
                choice: choice.into(),
            },
        }
    }

    fn url(sent: &Sent) -> Url {
        Url::parse(&format!("https://issues.example.com{}", sent.path)).unwrap()
    }

    fn query(sent: &Sent, key: &str) -> String {
        url(sent)
            .query_pairs()
            .find(|(name, _)| name == key)
            .unwrap()
            .1
            .into_owned()
    }

    fn assert_reads_only(mock: &Mock, count: usize) {
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent.len(), count);
        assert!(sent.iter().all(|request| request.method == "GET"));
        assert!(mock.replies.lock().unwrap().is_empty());
    }

    #[test]
    fn project_schema_recognizes_state_text_and_date_type_ids() {
        block_on(async {
            let mock = Mock::new(schema_replies());
            let schema = yt(mock.clone()).youtrack_schema(&project()).await.unwrap();
            assert_eq!(schema.fields.len(), 3);
            let state = &schema.fields[0];
            assert_eq!(state.kind, YoutrackFieldKind::State);
            assert_eq!(state.field_type, "StateIssueCustomField");
            assert_eq!(state.field_type_id, "state[1]");
            assert!(state.is_status());
            assert!(state.required);
            assert_eq!(state.status_choices().len(), 2);
            assert_eq!(schema.fields[1].kind, YoutrackFieldKind::Text);
            assert_eq!(schema.fields[2].kind, YoutrackFieldKind::Date);
            assert!(!schema.fields[1].is_status());
            let sent = mock.sent.lock().unwrap();
            assert_eq!(
                url(&sent[1]).path(),
                "/youtrack/api/admin/projects/0-1/customFields"
            );
            assert_eq!(
                url(&sent[2]).path(),
                "/youtrack/api/admin/customFieldSettings/bundles/state/state-1/values"
            );
            drop(sent);
            assert_reads_only(&mock, 3);
        });
    }

    #[test]
    fn status_load_paginates_raw_state_entries_without_loading_unrelated_bundles() {
        block_on(async {
            let first: Vec<_> = (0..100).map(|i| json!({
                "id":format!("state-{i}"), "name":format!("State {i}"), "archived":i % 2 == 0
            })).collect();
            let mock = Mock::new(vec![
                reply(200, state_issue()),
                reply(200, Value::Array(first)),
                reply(
                    200,
                    json!([{"id":"state-last", "name":"Last", "archived":false}]),
                ),
            ]);
            let fields = yt(mock.clone())
                .youtrack_status_fields(&task())
                .await
                .unwrap();
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].kind, YoutrackFieldKind::State);
            assert_eq!(fields[0].status_label(), "Open");
            let choices = fields[0].status_choices();
            assert_eq!(choices.len(), 51);
            assert_eq!(choices[0].value, "state-1");
            assert_eq!(choices.last().unwrap().value, "state-last");
            assert_eq!(choices.last().unwrap().label, "Last");
            let sent = mock.sent.lock().unwrap();
            assert_eq!(url(&sent[0]).path(), "/youtrack/api/issues/CAN-42");
            for (request, skip) in sent[1..].iter().zip(["0", "100"]) {
                assert_eq!(
                    url(request).path(),
                    "/youtrack/api/admin/customFieldSettings/bundles/state/state-1/values"
                );
                assert_eq!(query(request, "$skip"), skip);
                assert_eq!(query(request, "$top"), "100");
                assert!(
                    query(request, "fields")
                        .split(',')
                        .any(|field| field == "archived")
                );
            }
            drop(sent);
            assert_reads_only(&mock, 3);
        });
    }

    #[test]
    fn missing_runtime_type_uses_schema_type_not_its_raw_id_as_rest_class() {
        block_on(async {
            let mut value = state_issue();
            value["customFields"][0]
                .as_object_mut()
                .unwrap()
                .remove("$type");
            // Status identification must not depend on the English field name.
            value["customFields"][0]["name"] = json!("Etap");
            let mock = Mock::new(vec![reply(200, value), reply(200, states())]);
            let fields = yt(mock.clone())
                .youtrack_status_fields(&task())
                .await
                .unwrap();
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].kind, YoutrackFieldKind::State);
            assert_eq!(fields[0].field_type, "StateIssueCustomField");
            assert_eq!(
                fields[0].status_update("state-progress").unwrap().unwrap()["$type"],
                "StateIssueCustomField"
            );
            assert_reads_only(&mock, 2);
        });
    }

    #[test]
    fn runtime_state_machine_uses_events_without_loading_state_bundle() {
        block_on(async {
            let mock = Mock::new(vec![reply(200, workflow_issue())]);
            let fields = yt(mock.clone())
                .youtrack_status_fields(&task())
                .await
                .unwrap();
            assert_eq!(fields.len(), 1);
            let field = &fields[0];
            assert_eq!(field.field_type_id, "state[1]");
            assert_eq!(field.kind, YoutrackFieldKind::StateMachine);
            assert_eq!(field.status_label(), "Open");
            let choices = field.status_choices();
            assert_eq!(
                choices
                    .iter()
                    .map(|choice| (choice.value.as_str(), choice.label.as_str()))
                    .collect::<Vec<_>>(),
                [
                    ("event-start", "Start work"),
                    ("event-close", "Resolve"),
                    ("event-reopen", "event-reopen"),
                ]
            );
            assert!(field.status_update("state-progress").is_err());
            assert!(field.status_update("state-open").is_err());
            assert_reads_only(&mock, 1);
        });
    }

    #[test]
    fn status_write_rereads_and_posts_only_one_field_then_refreshes() {
        block_on(async {
            let mut refreshed = state_issue();
            refreshed["summary"] = json!("Concurrent summary edit");
            refreshed["customFields"][0]["value"] =
                json!({"id":"state-progress", "name":"In progress"});
            let mut current = refreshed.clone();
            current["customFields"][0]["value"] = state_issue()["customFields"][0]["value"].clone();
            let mock = Mock::new(vec![
                reply(200, current),
                reply(200, states()),
                reply(204, Value::Null),
                reply(200, refreshed.clone()),
            ]);
            let receipt = yt(mock.clone())
                .write(&command("82-11", "state-progress"))
                .await
                .unwrap();
            assert!(receipt.notice.is_none());
            let saved = receipt.task.unwrap();
            assert_eq!(saved.reference.title, "Concurrent summary edit");
            let details = saved.youtrack.unwrap();
            assert_eq!(details.status, "In progress");
            assert_eq!(details.fields["description"], refreshed["description"]);
            assert_eq!(
                details.fields["customFields"][1],
                refreshed["customFields"][1]
            );
            let sent = mock.sent.lock().unwrap();
            assert_eq!(
                sent.iter()
                    .map(|request| request.method.as_str())
                    .collect::<Vec<_>>(),
                ["GET", "GET", "POST", "GET"]
            );
            assert_eq!(url(&sent[0]).path(), "/youtrack/api/issues/CAN-42");
            assert_eq!(
                url(&sent[2]).path(),
                "/youtrack/api/issues/2-42/customFields/82-11"
            );
            assert_eq!(
                sent[2].body,
                json!({"id":"82-11", "$type":"StateIssueCustomField", "value":{"id":"state-progress"}})
            );
            assert_eq!(url(&sent[3]).path(), "/youtrack/api/issues/CAN-42");
            assert!(mock.replies.lock().unwrap().is_empty());
        });
    }

    #[test]
    fn choosing_current_state_returns_current_task_without_post_or_refresh() {
        block_on(async {
            let mock = Mock::new(vec![reply(200, state_issue()), reply(200, states())]);
            let receipt = yt(mock.clone())
                .write(&command("82-11", "state-open"))
                .await
                .unwrap();
            assert!(receipt.notice.is_none());
            assert_eq!(receipt.task.unwrap().youtrack.unwrap().status, "Open");
            assert_reads_only(&mock, 2);
        });
    }

    #[test]
    fn unavailable_choices_and_ineligible_fields_are_rejected_before_post() {
        block_on(async {
            // Each case uses the freshly read issue, not a cached selector snapshot.
            for case in [
                "stale",
                "archived",
                "empty",
                "foreign",
                "non-status",
                "invalid-id",
                "read-only",
                "project-read-only",
                "multi",
                "missing-options",
                "stale-event",
            ] {
                let mut current = state_issue();
                let mut id = "82-11";
                let mut choice = "state-progress";
                let mut bundle = Some(states());
                match case {
                    "stale" => choice = "removed-state",
                    "archived" => choice = "state-old",
                    "empty" => choice = "",
                    "foreign" => {
                        id = "82-99";
                        bundle = None;
                    }
                    "non-status" => {
                        id = "82-12";
                        bundle = None;
                    }
                    "invalid-id" => {
                        id = "../82-11";
                        bundle = None;
                    }
                    "read-only" => {
                        current["customFields"][0]["readOnly"] = json!(true);
                        bundle = None;
                    }
                    "project-read-only" => {
                        current["customFields"][0]["projectCustomField"]["readOnly"] = json!(true);
                        bundle = None;
                    }
                    "multi" => {
                        current["customFields"][0]["$type"] = json!("MultiStateIssueCustomField");
                        current["customFields"][0]["projectCustomField"]["field"]["fieldType"] =
                            json!({"id":"state[*]", "isMultiValue":true});
                        current["customFields"][0]["value"] =
                            json!([{"id":"state-open", "name":"Open"}]);
                        bundle = None;
                    }
                    "missing-options" => {
                        current["customFields"][0]["projectCustomField"]["bundle"]["values"] =
                            states();
                        bundle = Some(json!([]));
                    }
                    "stale-event" => {
                        current = workflow_issue();
                        choice = "event-removed";
                        bundle = None;
                    }
                    _ => unreachable!(),
                }
                let mut replies = if case == "invalid-id" {
                    vec![]
                } else {
                    vec![reply(200, current)]
                };
                if let Some(bundle) = bundle {
                    replies.push(reply(200, bundle));
                }
                let count = replies.len();
                let mock = Mock::new(replies);
                let result = yt(mock.clone()).write(&command(id, choice)).await;
                assert!(
                    matches!(result, Err(WriteError::Rejected(_))),
                    "{case}: {result:?}"
                );
                assert_reads_only(&mock, count);
            }
        });
    }

    #[test]
    fn status_workflow_write_sends_event_id_not_value_or_label() {
        block_on(async {
            let mock = Mock::new(vec![
                reply(200, workflow_issue()),
                reply(204, Value::Null),
                reply(200, workflow_issue()),
            ]);
            yt(mock.clone())
                .write(&command("82-11", "event-start"))
                .await
                .unwrap();
            let sent = mock.sent.lock().unwrap();
            assert_eq!(
                sent.iter()
                    .map(|request| request.method.as_str())
                    .collect::<Vec<_>>(),
                ["GET", "POST", "GET"]
            );
            assert_eq!(
                url(&sent[1]).path(),
                "/youtrack/api/issues/2-42/customFields/82-11"
            );
            assert_eq!(
                sent[1].body,
                json!({"id":"82-11", "$type":"StateMachineIssueCustomField", "event":{"id":"event-start", "$type":"Event"}})
            );
            assert!(sent[1].body.get("value").is_none());
            assert!(mock.replies.lock().unwrap().is_empty());
        });
    }

    #[test]
    fn confirmed_status_write_refresh_failure_is_not_an_uncertain_or_rejected_write() {
        block_on(async {
            for status in [204, 503, 403] {
                let mut replies = vec![
                    reply(200, state_issue()),
                    reply(200, states()),
                    reply(status, Value::Null),
                ];
                if status == 204 {
                    replies.push(reply(503, json!({"error":"unavailable"})));
                }
                let mock = Mock::new(replies);
                let result = yt(mock.clone())
                    .write(&command("82-11", "state-progress"))
                    .await;
                match status {
                    204 => {
                        let receipt = result.unwrap();
                        assert!(receipt.task.is_none());
                        let notice = receipt.notice.unwrap().to_lowercase();
                        assert!(notice.contains("saved"));
                        assert!(notice.contains("refresh"));
                    }
                    503 => assert!(matches!(result, Err(WriteError::Uncertain(_)))),
                    403 => assert!(matches!(result, Err(WriteError::Rejected(_)))),
                    _ => unreachable!(),
                }
                let sent = mock.sent.lock().unwrap();
                assert_eq!(sent.len(), if status == 204 { 4 } else { 3 });
                assert_eq!(
                    sent.iter()
                        .filter(|request| request.method == "POST")
                        .count(),
                    1
                );
                assert_eq!(
                    sent.last().unwrap().method,
                    if status == 204 { "GET" } else { "POST" }
                );
                assert!(mock.replies.lock().unwrap().is_empty());
            }
        });
    }

    #[test]
    fn current_value_overlay_preserves_options_but_honors_runtime_workflow_and_readonly() {
        block_on(async {
            let mut current = workflow_issue();
            current["customFields"][0]["readOnly"] = json!(true);
            let mut replies = schema_replies();
            replies.push(reply(200, current));
            let mock = Mock::new(replies);
            let provider = yt(mock.clone());
            let schema = provider.youtrack_schema(&project()).await.unwrap();
            let current = provider.task(&task()).await.unwrap().youtrack.unwrap();
            let project_field = &schema.fields[0];
            let current_field = &current.custom_fields[0];
            assert!(current_field.allowed.is_empty());
            let merged = project_field.clone().with_current_value(current_field);
            assert_eq!(merged.kind, YoutrackFieldKind::StateMachine);
            assert_eq!(merged.field_type, "StateMachineIssueCustomField");
            assert_eq!(merged.allowed, project_field.allowed);
            assert_eq!(merged.value, current_field.value);
            assert!(merged.read_only);
            assert!(merged.status_update("event-start").is_err());
            assert_eq!(merged.status_choices()[0].value, "event-start");
            // A permissive issue response must not unlock a read-only project field.
            let mut locked_project = project_field.clone();
            locked_project.read_only = true;
            let mut editable_current = current_field.clone();
            editable_current.read_only = false;
            assert!(
                locked_project
                    .with_current_value(&editable_current)
                    .read_only
            );
            assert_reads_only(&mock, 4);
        });
    }
}
