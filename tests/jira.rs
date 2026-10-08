use canopy_desktop::integrations::{
    jira::{Jira, JiraField, JiraWrite, SchemaRequest, adf, field_value},
    status::{self, StatusChange, StatusField},
    *,
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
#[derive(Clone)]
struct Sent {
    method: String,
    path: String,
    body: Value,
}
struct Reply {
    status: u16,
    body: String,
    rate: bool,
}
fn response(status: u16, body: Value) -> Reply {
    Reply {
        status,
        body: body.to_string(),
        rate: false,
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
        assert!(matches!(
            request.uri().host(),
            Some("team.atlassian.net" | "api.atlassian.com")
        ));
        assert_eq!(
            request.headers()["Authorization"],
            "Basic dXNlckBleGFtcGxlLmNvbTpwcml2YXRlLXRva2Vu"
        );
        assert!(request.headers()["Authorization"].is_sensitive());
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
            .expect("unexpected request");
        Box::pin(async move {
            let mut bytes = vec![];
            request.into_body().read_to_end(&mut bytes).await?;
            sent.lock().unwrap().push(Sent {
                method,
                path,
                body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            });
            if reply.status == 0 {
                return Err(gpui_kit::http_client::anyhow!(
                    "private-token transport failure"
                ));
            }
            let mut builder = Response::builder().status(reply.status);
            if reply.rate {
                builder = builder.header("x-ratelimit-remaining", "0");
            }
            Ok(builder.body(AsyncBody::from(reply.body))?)
        })
    }
}
fn project() -> ProjectTarget {
    ProjectTarget::jira("https://team.atlassian.net", "CAN").unwrap()
}
fn task() -> TaskRef {
    TaskRef {
        project: project(),
        id: "CAN-42".into(),
        title: "Task".into(),
    }
}
fn jira(mock: Arc<Mock>) -> Jira {
    Jira::new(
        mock,
        "https://team.atlassian.net",
        "user@example.com",
        None,
        "private-token".into(),
        "account-1".into(),
    )
    .unwrap()
}
fn issue() -> Value {
    json!({"id":"12345","key":"CAN-42","fields":{"summary":"Task","description":{"version":1,"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello","marks":[{"type":"strong"}]}]}]},"status":{"id":"3","name":"In progress","statusCategory":{"key":"indeterminate"}},"issuetype":{"id":"10001","name":"Story"},"labels":["backend"],"assignee":{"accountId":"account-1","displayName":"Alex"},"customfield_10001":{"value":"Preserve me"},"attachment":[{"id":"77","filename":"test.txt"}]} ,"names":{"customfield_10001":"Area"}})
}
fn comment() -> Value {
    json!({"id":"77","created":"2026-09-10T12:00:00.000+0000","author":{"accountId":"account-1","displayName":"Alex"},"body":{"version":1,"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"A comment"}]}]}})
}
fn write(action: JiraWrite) -> TaskWrite {
    TaskWrite::Jira {
        project: project(),
        task: Some(task()),
        action,
    }
}
fn status_transition(id: &str, name: &str, fields: Value) -> Value {
    json!({"id":id,"name":name,"to":{"name":"Done"},"fields":fields})
}

#[test]
fn shared_status_jira_routes_by_transition_id_and_requires_explicit_required_values() {
    block_on(async {
        let m = Mock::new(vec![
            response(200, issue()),
            response(
                200,
                json!({"transitions":[
                    status_transition("31", "Finish", json!({})),
                    status_transition("32", "Resolve", json!({"resolution":{
                        "required":true,"defaultValue":{"id":"1"},"schema":{"type":"resolution"}
                    }})),
                    status_transition("33", "Complete", json!({"resolution":{
                        "required":true,"schema":{"type":"resolution"}
                    }})),
                    status_transition("34", "Archive", json!({"comment":{
                        "required":false,"defaultValue":"Do not submit me","schema":{"type":"string"}
                    }})),
                    {"id":"35","name":"Unavailable","isAvailable":false,"fields":{}}
                ]}),
            ),
        ]);
        let fields = status::load(&jira(m.clone()), &task()).await.unwrap();
        assert_eq!(fields.len(), 1);
        let field = &fields[0];
        assert_eq!(field.label(), "In progress");
        assert_eq!(field.selected(), None);
        let choices = field.choices();
        assert_eq!(
            choices
                .iter()
                .map(|c| (c.value.as_str(), c.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("31", "Done — Finish"),
                ("32", "Done — Resolve"),
                ("33", "Done — Complete"),
                ("34", "Done — Archive")
            ]
        );
        for id in ["31", "34"] {
            let Some(StatusChange::Write(command)) = field.choose(&task(), id).unwrap() else {
                panic!("optional fields must allow a quick transition");
            };
            assert!(matches!(*command, TaskWrite::Jira {
                project: target, task: Some(reference), action: JiraWrite::QuickTransition { id: chosen }
            } if target == project() && reference == task() && chosen == id));
        }
        for id in ["32", "33"] {
            assert!(matches!(field.choose(&task(), id).unwrap(),
                Some(StatusChange::JiraForm { transition }) if transition == id));
        }
        for choice in ["Done", "Finish", "In progress", "35", "999", ""] {
            assert!(field.choose(&task(), choice).is_err());
        }
        let foreign = TaskRef {
            project: ProjectTarget::github("owner/repo").unwrap(),
            id: "42".into(),
            title: String::new(),
        };
        assert!(field.choose(&foreign, "31").is_err());
        assert!(
            StatusField::Github(TaskState::Open)
                .choose(&task(), "closed")
                .is_err()
        );
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].method, "GET");
        assert_eq!(
            sent[0].path,
            "/rest/api/3/issue/CAN-42?fields=*all&expand=names"
        );
        assert_eq!(sent[1].method, "GET");
        assert_eq!(
            sent[1].path,
            "/rest/api/3/issue/CAN-42/transitions?expand=transitions.fields"
        );
    });
}

#[test]
fn shared_status_jira_without_transitions_keeps_current_status_without_guessing_actions() {
    block_on(async {
        let m = Mock::new(vec![
            response(200, issue()),
            response(200, json!({"transitions":[]})),
        ]);
        let fields = status::load(&jira(m.clone()), &task()).await.unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].label(), "In progress");
        assert_eq!(fields[0].selected(), None);
        assert!(fields[0].choices().is_empty());
        for choice in ["", "In progress", "open", "closed", "31"] {
            assert!(fields[0].choose(&task(), choice).is_err());
        }
        assert_eq!(m.sent.lock().unwrap().len(), 2);
    });
}

#[test]
fn shared_status_jira_from_task_does_not_invent_transitions() {
    block_on(async {
        let current = jira(Mock::new(vec![response(200, issue())]))
            .task(&task())
            .await
            .unwrap();
        let fields = status::from_task(&current);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].label(), "In progress");
        assert!(fields[0].choices().is_empty());
        assert!(fields[0].choose(&task(), "Done").is_err());
    });
}

#[test]
fn shared_status_jira_quick_transition_revalidates_then_posts_only_transition_then_reloads() {
    block_on(async {
        let metadata = json!({"transitions":[status_transition("31", "Finish", json!({
            "comment":{"required":false,"defaultValue":"Do not submit me"}
        }))]});
        let mut updated = issue();
        updated["fields"]["status"] = json!({"name":"Done","statusCategory":{"key":"done"}});
        let m = Mock::new(vec![
            response(200, issue()),
            response(200, metadata.clone()),
            response(200, metadata),
            response(204, Value::Null),
            response(200, updated),
        ]);
        let j = jira(m.clone());
        let fields = status::load(&j, &task()).await.unwrap();
        let Some(StatusChange::Write(command)) = fields[0].choose(&task(), "31").unwrap() else {
            panic!("expected quick transition");
        };
        let receipt = j.write(&command).await.unwrap();
        assert!(receipt.notice.is_none());
        assert_eq!(receipt.task.unwrap().jira.unwrap().status, "Done");
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent.len(), 5);
        assert_eq!(
            sent.iter().map(|s| s.method.as_str()).collect::<Vec<_>>(),
            vec!["GET", "GET", "GET", "POST", "GET"]
        );
        assert_eq!(
            sent[2].path,
            "/rest/api/3/issue/CAN-42/transitions?expand=transitions.fields"
        );
        assert_eq!(sent[2].body, Value::Null);
        assert_eq!(sent[3].path, "/rest/api/3/issue/CAN-42/transitions");
        assert_eq!(sent[3].body, json!({"transition":{"id":"31"}}));
        assert_eq!(
            sent[4].path,
            "/rest/api/3/issue/CAN-42?fields=*all&expand=names"
        );
    });
}

#[test]
fn shared_status_jira_stale_transition_or_new_requirements_never_post() {
    block_on(async {
        for fresh in [
            json!({"transitions":[]}),
            json!({"transitions":[{"id":"31","name":"Finish","isAvailable":false,"fields":{}}]}),
            json!({"transitions":[status_transition("32", "Finish", json!({}))]}),
            json!({"transitions":[status_transition("31", "Finish", json!({"resolution":{"required":true}}))]}),
            json!({"transitions":[status_transition("31", "Finish", json!({"resolution":{"required":true,"defaultValue":{"id":"1"}}}))]}),
        ] {
            let m = Mock::new(vec![
                response(200, issue()),
                response(
                    200,
                    json!({"transitions":[status_transition("31", "Finish", json!({}))]}),
                ),
                response(200, fresh),
            ]);
            let j = jira(m.clone());
            let fields = status::load(&j, &task()).await.unwrap();
            let Some(StatusChange::Write(command)) = fields[0].choose(&task(), "31").unwrap()
            else {
                panic!("expected initially available quick transition");
            };
            assert!(matches!(
                j.write(&command).await,
                Err(WriteError::Rejected(_))
            ));
            let sent = m.sent.lock().unwrap();
            assert_eq!(sent.len(), 3);
            assert!(sent.iter().all(|s| s.method == "GET"));
        }
    });
}

#[test]
fn shared_status_jira_missing_or_malformed_metadata_never_authorizes_quick_write() {
    block_on(async {
        for metadata in [
            json!({}),
            json!({"transitions":null}),
            json!({"transitions":[{"id":"31","name":"Finish","to":{"name":"Done"}}]}),
            json!({"transitions":[status_transition("31", "Finish", Value::Null)]}),
            json!({"transitions":[status_transition("31", "Finish", json!([]))]}),
            json!({"transitions":[status_transition("31", "Finish", json!({"resolution":{}}))]}),
            json!({"transitions":[status_transition("31", "Finish", json!({"resolution":{"required":"false"}}))]}),
            json!({"transitions":[status_transition("31", "Finish", json!({"resolution":{"required":null}}))]}),
        ] {
            let m = Mock::new(vec![response(200, metadata.clone())]);
            assert!(matches!(
                jira(m.clone())
                    .write(&write(JiraWrite::QuickTransition { id: "31".into() }))
                    .await,
                Err(WriteError::Rejected(_))
            ));
            {
                let sent = m.sent.lock().unwrap();
                assert_eq!(sent.len(), 1);
                assert_eq!(sent[0].method, "GET");
                assert_eq!(
                    sent[0].path,
                    "/rest/api/3/issue/CAN-42/transitions?expand=transitions.fields"
                );
            }
            let m = Mock::new(vec![response(200, issue()), response(200, metadata)]);
            assert!(status::load(&jira(m.clone()), &task()).await.is_err());
            assert_eq!(m.sent.lock().unwrap().len(), 2);
        }
    });
}

#[test]
fn shared_status_jira_quick_transition_uncertain_post_is_not_retried_or_reloaded() {
    block_on(async {
        for code in [0, 500, 502] {
            let m = Mock::new(vec![
                response(
                    200,
                    json!({"transitions":[status_transition("31", "Finish", json!({}))]}),
                ),
                response(code, json!({"message":"private-token"})),
            ]);
            let error = jira(m.clone())
                .write(&write(JiraWrite::QuickTransition { id: "31".into() }))
                .await
                .unwrap_err();
            assert!(matches!(error, WriteError::Uncertain(_)));
            assert!(!error.to_string().contains("private-token"));
            let sent = m.sent.lock().unwrap();
            assert_eq!(sent.len(), 2);
            assert_eq!(sent[0].method, "GET");
            assert_eq!(sent[1].method, "POST");
            assert_eq!(sent[1].body, json!({"transition":{"id":"31"}}));
        }
    });
}

#[test]
fn shared_status_jira_accepted_quick_transition_keeps_success_when_reload_fails() {
    block_on(async {
        let m = Mock::new(vec![
            response(
                200,
                json!({"transitions":[status_transition("31", "Finish", json!({}))]}),
            ),
            response(204, Value::Null),
            response(503, json!({})),
        ]);
        let receipt = jira(m.clone())
            .write(&write(JiraWrite::QuickTransition { id: "31".into() }))
            .await
            .unwrap();
        assert!(receipt.task.is_none());
        let notice = receipt.notice.unwrap();
        assert!(notice.contains("saved"));
        assert!(notice.contains("Refresh"));
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent.len(), 3);
        assert_eq!(
            sent.iter().map(|s| s.method.as_str()).collect::<Vec<_>>(),
            vec!["GET", "POST", "GET"]
        );
        assert_eq!(
            sent[2].path,
            "/rest/api/3/issue/CAN-42?fields=*all&expand=names"
        );
    });
}

#[test]
fn site_routing_and_old_github_configuration_remain_valid() {
    assert!(jira::normalize_site("http://team.atlassian.net").is_err());
    for value in [
        "https://secret@team.atlassian.net",
        "https://team.atlassian.net/path",
        "https://team.atlassian.net?q=x",
        "https://team.atlassian.net:8443",
    ] {
        assert!(jira::normalize_site(value).is_err());
    }
    assert_eq!(
        jira::normalize_site("https://TEAM.atlassian.net/").unwrap(),
        "https://team.atlassian.net"
    );
    let gh: ProjectTarget =
        serde_json::from_value(json!({"provider":"github","key":"owner/repo"})).unwrap();
    assert!(gh.valid());
    let a = Account {
        id: uuid::Uuid::new_v4().to_string(),
        provider: Provider::Jira,
        credential: uuid::Uuid::new_v4().to_string(),
        login: "account-1".into(),
        scope: AccountScope::Jira {
            site: "https://team.atlassian.net".into(),
            email: "user@example.com".into(),
            cloud_id: None,
        },
    };
    let config = Config::default().with_account(a.clone()).unwrap();
    assert_eq!(config.account_for(&project()), Some(&a));
    assert!(
        config
            .account_for(&ProjectTarget::jira("https://other.atlassian.net", "CAN").unwrap())
            .is_none()
    );
    assert!(task().valid());
    let mut foreign = task();
    foreign.id = "OTHER-42".into();
    assert!(!foreign.valid());
    let json = serde_json::to_string(&config).unwrap();
    assert!(!json.contains("private-token"));
    assert!(serde_json::from_str::<Config>(&json).unwrap().valid());
}
#[test]
fn basic_and_scoped_auth_verify_identity_without_redirects() {
    block_on(async {
        let m = Mock::new(vec![
            response(200, json!({"accountId":"account-1"})),
            response(200, json!({"baseUrl":"https://team.atlassian.net"})),
            response(200, json!({"accountId":"account-1"})),
        ]);
        assert_eq!(jira(m.clone()).verify().await.unwrap(), "account-1");
        let scoped = Jira::new(
            m.clone(),
            "https://team.atlassian.net",
            "user@example.com",
            Some("12345678-1234-1234-1234-123456789abc"),
            "private-token".into(),
            String::new(),
        )
        .unwrap();
        scoped.verify().await.unwrap();
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].path, "/rest/api/3/myself");
        assert_eq!(
            sent[2].path,
            "/ex/jira/12345678-1234-1234-1234-123456789abc/rest/api/3/myself"
        );
    });
}
#[test]
fn enhanced_search_paginates_and_retains_jira_status() {
    block_on(async {
        let m = Mock::new(vec![
            response(
                200,
                json!({"issues":[issue()],"nextPageToken":"page+2","isLast":false}),
            ),
            response(200, json!({"issues":[],"isLast":true})),
        ]);
        let j = jira(m.clone());
        let page = j.list(&project(), TaskState::Open, None).await.unwrap();
        assert_eq!(page.next_cursor.as_deref(), Some("page+2"));
        assert_eq!(page.tasks[0].jira.as_ref().unwrap().status, "In progress");
        assert_eq!(&*page.tasks[0].body, "**Hello**");
        assert_eq!(page.tasks[0].reference.label(), "CAN-42");
        assert!(
            j.list(&project(), TaskState::Closed, page.next_cursor.as_deref())
                .await
                .unwrap()
                .next_cursor
                .is_none()
        );
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].path, "/rest/api/3/search/jql");
        assert_eq!(sent[0].body["maxResults"], 30);
        assert!(
            sent[0].body["jql"]
                .as_str()
                .unwrap()
                .contains("statusCategory != Done")
        );
        assert_eq!(sent[1].body["nextPageToken"], "page+2");
    });
}
#[test]
fn fields_metadata_and_transitions_come_from_project_workflow() {
    block_on(async {
        let m = Mock::new(vec![
            response(
                200,
                json!({"fields":[{"fieldId":"summary","name":"Summary","required":true,"schema":{"type":"string"}},{"fieldId":"customfield_11","name":"Severity","required":true,"schema":{"type":"option"},"allowedValues":[{"id":"20","value":"High"}]}],"total":2,"isLast":true}),
            ),
            response(
                200,
                json!({"transitions":[{"id":"31","name":"Finish","to":{"name":"Done"},"fields":{"resolution":{"name":"Resolution","required":true,"schema":{"type":"resolution"},"allowedValues":[{"id":"1","name":"Done"}]}}}]}),
            ),
        ]);
        let j = jira(m.clone());
        let schema = j
            .schema(
                &project(),
                &SchemaRequest::Create {
                    issue_type: "10001".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(schema.fields.len(), 2);
        assert!(schema.fields.iter().all(|f| f.required));
        let t = j
            .schema(
                &project(),
                &SchemaRequest::Transitions {
                    key: "CAN-42".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(t.transitions[0].id, "31");
        assert_eq!(t.transitions[0].fields[0].id, "resolution");
        assert!(
            m.sent.lock().unwrap()[1]
                .path
                .ends_with("transitions?expand=transitions.fields")
        );
    });
}
#[test]
fn reference_fields_load_named_users_and_parent_issues_when_editmeta_is_empty() {
    block_on(async {
        let m = Mock::new(vec![
            response(
                200,
                json!({
                    "fields": {
                        "assignee": {"name":"Assignee","schema":{"type":"user"}},
                        "parent": {"name":"Parent","schema":{"type":"issuelink"}}
                    }
                }),
            ),
            response(
                200,
                json!([{"accountId":"acct-1","displayName":"Alex Example"}]),
            ),
            response(
                200,
                json!({"issues":[{"key":"CAN-41","fields":{"summary":"Parent issue"}}]}),
            ),
        ]);
        let schema = jira(m.clone())
            .schema(
                &project(),
                &SchemaRequest::Edit {
                    key: "CAN-42".into(),
                },
            )
            .await
            .unwrap();
        let assignee = schema
            .fields
            .iter()
            .find(|field| field.id == "assignee")
            .unwrap();
        assert_eq!(assignee.allowed[0]["accountId"], "acct-1");
        let parent = schema
            .fields
            .iter()
            .find(|field| field.id == "parent")
            .unwrap();
        assert_eq!(parent.allowed[0]["key"], "CAN-41");
        let sent = m.sent.lock().unwrap();
        assert!(sent[1].path.contains("/user/assignable/search"));
        assert_eq!(sent[2].method, "POST");
        assert_eq!(sent[2].path, "/rest/api/3/search/jql");
        assert!(
            sent[2].body["jql"]
                .as_str()
                .unwrap()
                .contains("project = \"CAN\"")
        );
    });
}
#[test]
fn create_success_survives_followup_read_failure() {
    block_on(async {
        let m = Mock::new(vec![
            response(201, json!({"key":"CAN-42","id":"12345"})),
            response(503, json!({})),
        ]);
        let j = jira(m.clone());
        let receipt=j.write(&TaskWrite::Jira {project:project(),task:None,action:JiraWrite::Create {fields:json!({"project":{"key":"WRONG"},"summary":"New","issuetype":{"id":"10001"},"customfield_11":{"id":"20"}})}}).await.unwrap();
        assert_eq!(receipt.task.unwrap().reference.id, "CAN-42");
        assert!(receipt.notice.unwrap().contains("saved"));
        assert_eq!(
            m.sent.lock().unwrap()[0].body["fields"]["project"]["key"],
            "CAN"
        );
    });
}
#[test]
fn editing_fields_does_not_overwrite_untouched_adf_or_custom_fields() {
    block_on(async {
        let m = Mock::new(vec![response(204, Value::Null), response(200, issue())]);
        jira(m.clone())
            .write(&write(JiraWrite::Fields {
                fields: json!({"priority":{"id":"2"},"duedate":null}),
            }))
            .await
            .unwrap();
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].method, "PUT");
        assert_eq!(
            sent[0].body,
            json!({"fields":{"priority":{"id":"2"},"duedate":null}})
        );
    });
}
#[test]
fn transition_sends_required_values_and_comment_atomically() {
    block_on(async {
        let m = Mock::new(vec![response(204, Value::Null), response(200, issue())]);
        jira(m.clone())
            .write(&write(JiraWrite::Transition {
                id: "31".into(),
                fields: json!({"resolution":{"id":"1"},"customfield_11":{"id":"20"}}),
                comment: "**Finished**".into(),
            }))
            .await
            .unwrap();
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].body["transition"]["id"], "31");
        assert_eq!(sent[0].body["fields"]["resolution"]["id"], "1");
        assert_eq!(
            sent[0].body["update"]["comment"][0]["add"]["body"]["type"],
            "doc"
        );
    });
}
#[test]
fn comments_support_pagination_create_edit_delete_and_rich_documents() {
    block_on(async {
        let m = Mock::new(vec![
            response(200, json!({"total":31,"comments":[comment()]})),
            response(201, comment()),
            response(200, comment()),
            response(204, Value::Null),
            response(200, comment()),
        ]);
        let j = jira(m.clone());
        let page = j.comments(&task(), Some("30")).await.unwrap();
        assert!(page.next_cursor.is_none());
        let c = page.comments[0].clone();
        assert!(
            j.write(&TaskWrite::AddComment {
                task: task(),
                body: "Hello **Jira**".into()
            })
            .await
            .unwrap()
            .comment
            .is_some()
        );
        j.write(&TaskWrite::EditComment {
            task: task(),
            comment: c.clone(),
            body: "Updated".into(),
        })
        .await
        .unwrap();
        j.write(&TaskWrite::DeleteComment {
            task: task(),
            comment: c.clone(),
        })
        .await
        .unwrap();
        let doc = json!({"version":1,"type":"doc","content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"account-1","text":"@Alex"}}]}]});
        j.write(&write(JiraWrite::CommentDocument {
            comment: c,
            document: doc.clone(),
        }))
        .await
        .unwrap();
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[1].body["body"]["type"], "doc");
        assert_eq!(sent[2].method, "PUT");
        assert_eq!(sent[3].method, "DELETE");
        assert_eq!(sent[4].body["body"], doc);
    });
}
#[test]
fn jira_errors_do_not_retry_writes_or_expose_tokens() {
    block_on(async {
        for status in [0, 500, 502] {
            let m = Mock::new(vec![response(status, json!({}))]);
            let err = jira(m.clone())
                .write(&write(JiraWrite::Fields {
                    fields: json!({"summary":"New"}),
                }))
                .await
                .unwrap_err();
            assert!(matches!(err, WriteError::Uncertain(_)));
            assert!(!err.to_string().contains("private-token"));
            assert_eq!(m.sent.lock().unwrap().len(), 1);
        }
        let m = Mock::new(vec![response(
            400,
            json!({"errors":{"customfield_11":"Required private-token"}}),
        )]);
        let err = jira(m)
            .write(&write(JiraWrite::Fields {
                fields: json!({"summary":"New"}),
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("customfield_11"));
        assert!(!err.to_string().contains("private-token"));
    });
}
#[test]
fn provider_rejects_cross_site_and_foreign_comment_before_request() {
    block_on(async {
        let m = Mock::new(vec![]);
        let j = jira(m.clone());
        let mut t = task();
        t.project.site = Some("https://other.atlassian.net".into());
        assert!(j.task(&t).await.is_err());
        let c = TaskComment {
            id: "5".into(),
            body: "".into(),
            url: "https://evil.invalid/comment".into(),
            author: "A".into(),
            created_at: "".into(),
            rich_body: None,
            can_edit: None,
            can_delete: None,
        };
        assert!(
            j.write(&TaskWrite::DeleteComment {
                task: task(),
                comment: c
            })
            .await
            .is_err()
        );
        assert!(m.sent.lock().unwrap().is_empty());
    });
}
#[test]
fn adf_preserves_structure_and_flags_lossy_editing() {
    let doc=adf::from_markdown("# Title\n\n**Bold** and [link](https://example.com)\n\n- One\n- Two\n\n```rust\nfn main() {}\n```").unwrap();
    assert_eq!(doc["type"], "doc");
    assert_eq!(doc["content"][0]["type"], "heading");
    let text = adf::to_markdown(&doc);
    assert!(text.contains("**Bold**"));
    assert!(text.contains("fn main() {}"));
    assert!(adf::editable(&doc));
    let rich = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"account-1","text":"@Alex"}}]}]});
    assert!(!adf::editable(&rich));
    assert!(adf::to_markdown(&rich).contains("@Alex"));
}
#[test]
fn custom_fields_obey_types_and_keep_explicit_clear() {
    let f = |kind: &str, required| JiraField {
        id: "customfield_11".into(),
        name: "Estimate".into(),
        required,
        schema: json!({"type":kind}),
        allowed: vec![],
        default: Value::Null,
    };
    assert_eq!(field_value(&f("number", true), "3.5").unwrap(), json!(3.5));
    assert!(field_value(&f("number", true), "abc").is_err());
    assert!(field_value(&f("string", true), "").is_err());
    assert_eq!(field_value(&f("date", false), "").unwrap(), Value::Null);
    assert!(field_value(&f("date", false), "10/12/2026").is_err());
}
#[test]
fn editing_text_roundtrips_mentions_media_and_tables_without_losing_identity() {
    let mention =
        json!({"type":"mention","attrs":{"id":"abc-123","text":"@Alex","accessLevel":""}});
    let media = json!({"type":"mediaSingle","attrs":{"layout":"center"},"content":[{"type":"media","attrs":{"id":"image-uuid","type":"file","collection":"jira"}}]});
    let table = json!({"type":"table","attrs":{"isNumberColumnEnabled":true},"content":[{"type":"tableRow","content":[{"type":"tableHeader","content":[{"type":"paragraph","content":[{"type":"text","text":"Column"}]}]}]}]});
    let doc = json!({"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"Hello "},mention.clone()]},media.clone(),table.clone()]});
    let text = adf::editing_text(&doc);
    assert!(text.contains("@Alex"));
    assert!(text.contains("canopy-jira://node/"));
    let updated = adf::from_editing_text(&text.replace("Hello", "Welcome"), &doc).unwrap();
    assert_eq!(updated["content"][0]["content"][1], mention);
    assert_eq!(updated["content"][1], media);
    assert_eq!(updated["content"][2], table);
    assert!(!updated.to_string().contains("canopy-jira:"));
    assert!(adf::from_editing_text("[bad](canopy-jira://node/99)", &doc).is_err());
}
#[test]
fn search_reconciles_written_ids_and_escapes_user_search_text() {
    block_on(async {
        let m = Mock::new(vec![response(200, json!({"issues":[],"isLast":true}))]);
        jira(m.clone())
            .list_context(
                &project(),
                TaskState::Open,
                None,
                &TaskQuery {
                    text: "a\" OR project=OTHER".into(),
                    filter: None,
                    reconcile: vec![12345],
                },
            )
            .await
            .unwrap();
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].body["reconcileIssues"], json!([12345]));
        assert_eq!(
            sent[0].body["jql"],
            "project = \"CAN\" AND (statusCategory != Done) AND text ~ \"a\\\" OR project=OTHER\" ORDER BY updated DESC"
        );
    });
}
#[test]
fn jira_links_watch_votes_sprints_and_worklogs_use_scoped_endpoints() {
    block_on(async {
        let m = Mock::new(
            (0..5)
                .flat_map(|_| [response(204, Value::Null), response(200, issue())])
                .collect(),
        );
        let j = jira(m.clone());
        j.write(&write(JiraWrite::Link {
            kind: "Blocks".into(),
            other: "OTHER-10".into(),
            outward: true,
        }))
        .await
        .unwrap();
        j.write(&write(JiraWrite::Watch { watching: true }))
            .await
            .unwrap();
        j.write(&write(JiraWrite::Vote { voted: false }))
            .await
            .unwrap();
        j.write(&write(JiraWrite::Sprint {
            id: Some("25".into()),
        }))
        .await
        .unwrap();
        j.write(&write(JiraWrite::LogWork {
            seconds: 3600,
            started: "2026-09-10T14:00:00.000+0200".into(),
            comment: "Review".into(),
        }))
        .await
        .unwrap();
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].path, "/rest/api/3/issueLink");
        assert_eq!(sent[2].body, "account-1");
        assert_eq!(sent[4].method, "DELETE");
        assert_eq!(sent[6].path, "/rest/agile/1.0/sprint/25/issue");
        assert_eq!(sent[8].body["timeSpentSeconds"], 3600);
    });
}
#[test]
fn attachment_delete_checks_membership_before_deleting() {
    block_on(async {
        let m = Mock::new(vec![
            response(200, issue()),
            response(204, Value::Null),
            response(200, issue()),
            response(200, issue()),
        ]);
        let j = jira(m.clone());
        j.write(&write(JiraWrite::DeleteAttachment { id: "77".into() }))
            .await
            .unwrap();
        assert!(
            j.write(&write(JiraWrite::DeleteAttachment { id: "999".into() }))
                .await
                .is_err()
        );
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[1].method, "DELETE");
        assert_eq!(sent[1].path, "/rest/api/3/attachment/77");
        assert_eq!(sent.len(), 4);
    });
}
#[test]
fn jira_settings_and_field_drafts_survive_restart_alongside_github() {
    block_on(async {
        use canopy_desktop::{
            integrations::drafts::{TaskDraft, TaskDrafts},
            settings::{Access, SettingsClient},
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.db");
        let db = SettingsClient::create(&path).await.unwrap();
        let config = Config::default()
            .with_account(Account {
                id: uuid::Uuid::new_v4().to_string(),
                provider: Provider::Github,
                login: "alex".into(),
                credential: uuid::Uuid::new_v4().to_string(),
                scope: AccountScope::Default,
            })
            .unwrap()
            .with_account(Account {
                id: uuid::Uuid::new_v4().to_string(),
                provider: Provider::Jira,
                login: "account-1".into(),
                credential: uuid::Uuid::new_v4().to_string(),
                scope: AccountScope::Jira {
                    site: "https://team.atlassian.net".into(),
                    email: "user@example.com".into(),
                    cloud_id: None,
                },
            })
            .unwrap();
        let mut config = config;
        config.overrides.insert(dir.path().join("repo"), project());
        config
            .links
            .insert(dir.path().join("worktree"), vec![task()]);
        db.save_integrations(config.clone()).await.unwrap();
        let draft=TaskDraft {project:project(),value:IssueDraft {fields:[("summary".into(),json!({"text":"Unsent change"})),("description".into(),json!({"text":"Keep [@Alex](canopy-jira://node/0)","original":{"type":"doc","version":1,"content":[]}}))].into_iter().collect(),..Default::default()},baseline:None,warning:Some("Verify in Jira before retrying.".into())};
        let drafts = TaskDrafts::from([("connection:CAN:new-issue".into(), draft)]);
        db.save_task_drafts(drafts.clone()).await.unwrap();
        db.shutdown().await.unwrap();
        let db = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(db.load_integrations().await.unwrap(), config);
        assert_eq!(db.load_task_drafts().await.unwrap(), drafts);
        db.shutdown().await.unwrap();
    });
}
#[test]
fn deleting_issue_requires_exact_confirmation_and_preserves_subtasks_by_default() {
    block_on(async {
        let m = Mock::new(vec![response(204, Value::Null)]);
        let j = jira(m.clone());
        assert!(
            j.write(&write(JiraWrite::DeleteIssue {
                confirmation: "CAN-41".into(),
                subtasks: false
            }))
            .await
            .is_err()
        );
        assert!(m.sent.lock().unwrap().is_empty());
        let receipt = j
            .write(&write(JiraWrite::DeleteIssue {
                confirmation: "CAN-42".into(),
                subtasks: false,
            }))
            .await
            .unwrap();
        assert!(receipt.deleted_task.unwrap().same_task(&task()));
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].method, "DELETE");
        assert_eq!(
            sent[0].path,
            "/rest/api/3/issue/CAN-42?deleteSubtasks=false"
        );
    });
}
#[test]
fn file_transfers_use_jira_api_without_following_attachment_urls() {
    block_on(async {
        let m = Mock::new(vec![
            response(201, json!([{"id":"77"}])),
            response(200, issue()),
            response(200, issue()),
            Reply {
                status: 200,
                body: "file bytes".into(),
                rate: false,
            },
        ]);
        let j = jira(m.clone());
        j.write(&write(JiraWrite::Attach {
            name: "test.txt".into(),
            bytes: Arc::new(b"file bytes".to_vec()),
        }))
        .await
        .unwrap();
        assert_eq!(
            j.download_attachment(&task(), "77").await.unwrap(),
            b"file bytes"
        );
        let sent = m.sent.lock().unwrap();
        assert_eq!(sent[0].path, "/rest/api/3/issue/CAN-42/attachments");
        assert_eq!(
            sent[3].path,
            "/rest/api/3/attachment/content/77?redirect=false"
        );
    });
}
#[test]
fn scoped_cloud_id_must_match_the_configured_site() {
    block_on(async {
        let m = Mock::new(vec![response(
            200,
            json!({"baseUrl":"https://different.atlassian.net"}),
        )]);
        let j = Jira::new(
            m.clone(),
            "https://team.atlassian.net",
            "user@example.com",
            Some("12345678-1234-1234-1234-123456789abc"),
            "private-token".into(),
            String::new(),
        )
        .unwrap();
        assert!(j.verify().await.unwrap_err().contains("another Jira site"));
        assert_eq!(m.sent.lock().unwrap().len(), 1);
    });
}
#[test]
fn dates_validate_calendar_and_keep_explicit_timezone() {
    let field = JiraField {
        id: "duedate".into(),
        name: "Due date".into(),
        required: false,
        schema: json!({"type":"date"}),
        allowed: vec![],
        default: Value::Null,
    };
    assert!(field_value(&field, "2026-02-30").is_err());
    assert_eq!(field_value(&field, "2026-09-10").unwrap(), "2026-09-10");
    assert_eq!(
        jira::date_time("2026-09-10T14:30:00+02:00").unwrap(),
        "2026-09-10T14:30:00.000+0200"
    );
}
#[test]
fn markdown_reference_links_and_checklist_state_are_not_dropped() {
    let doc=adf::from_markdown("See [documentation][docs].\n\n[docs]: https://example.com/docs\n\n- [x] Done\n- [ ] Pending").unwrap();
    assert!(doc.to_string().contains("https://example.com/docs"));
    assert!(doc.to_string().contains("[x] "));
    assert!(doc.to_string().contains("[ ] "));
}
#[test]
fn custom_filter_is_intersected_with_project_and_does_not_inherit_active_filter() {
    block_on(async {
        let m = Mock::new(vec![
            response(200, json!({"issues":[],"isLast":true})),
            response(200, json!({"issues":[],"isLast":true})),
        ]);
        let j = jira(m.clone());
        j.list_context(
            &project(),
            TaskState::Open,
            None,
            &TaskQuery {
                text: String::new(),
                filter: Some("sprint in openSprints() AND assignee is EMPTY".into()),
                reconcile: vec![],
            },
        )
        .await
        .unwrap();
        j.list_context(
            &project(),
            TaskState::Open,
            None,
            &TaskQuery {
                text: String::new(),
                filter: Some(String::new()),
                reconcile: vec![],
            },
        )
        .await
        .unwrap();
        let sent = m.sent.lock().unwrap();
        assert_eq!(
            sent[0].body["jql"],
            "project = \"CAN\" AND (sprint in openSprints() AND assignee is EMPTY) ORDER BY updated DESC"
        );
        assert_eq!(
            sent[1].body["jql"],
            "project = \"CAN\" ORDER BY updated DESC"
        );
    });
}
#[test]
fn invalid_filter_never_reaches_the_api() {
    block_on(async {
        let m = Mock::new(vec![]);
        assert!(
            jira(m.clone())
                .list_context(
                    &project(),
                    TaskState::Open,
                    None,
                    &TaskQuery {
                        text: String::new(),
                        filter: Some(") OR project=OTHER".into()),
                        reconcile: vec![]
                    }
                )
                .await
                .is_err()
        );
        assert!(m.sent.lock().unwrap().is_empty());
    });
}
