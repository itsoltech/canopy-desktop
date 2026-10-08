use canopy_desktop::{
    integrations::{
        drafts::{TaskDraft, TaskDrafts},
        github::Github,
        status::{self, StatusChange, StatusField},
        *,
    },
    settings::{Access, SettingsClient},
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
        assert_eq!(request.uri().host(), Some("api.github.com"));
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
    ProjectTarget::github("owner/repo").unwrap()
}
fn task() -> TaskRef {
    TaskRef {
        project: project(),
        id: "42".into(),
        title: "Title".into(),
    }
}
fn issue() -> Value {
    json!({"number":42,"title":"Title","body":"Body","state":"open","labels":[{"name":"keep"}],"assignees":[{"login":"alex"}],"milestone":null})
}
fn comment() -> TaskComment {
    TaskComment {
        rich_body: None,
        id: "node-100".into(),
        author: "alex".into(),
        created_at: "2026-09-10T00:00:00Z".into(),
        body: "Before".into(),
        url: "https://github.com/owner/repo/issues/42#issuecomment-100".into(),
        can_edit: Some(true),
        can_delete: Some(true),
    }
}
fn comment_json() -> Value {
    json!({"node_id":"node-100","id":100,"body":"After","user":{"login":"alex"},"created_at":"2026-09-10T00:00:00Z","html_url":"https://github.com/owner/repo/issues/42#issuecomment-100"})
}
#[test]
fn shared_status_github_current_choice_is_noop_and_other_choice_is_targeted_state_write() {
    for (current, selected, other, expected) in [
        (TaskState::Open, "open", "closed", TaskState::Closed),
        (TaskState::Closed, "closed", "open", TaskState::Open),
    ] {
        let field = StatusField::Github(current);
        assert_eq!(field.selected(), Some(selected));
        assert_eq!(
            field
                .choices()
                .iter()
                .map(|c| (c.value.as_str(), c.label.as_str()))
                .collect::<Vec<_>>(),
            vec![("open", "Open"), ("closed", "Closed")]
        );
        assert!(field.choose(&task(), selected).unwrap().is_none());
        let Some(StatusChange::Write(command)) = field.choose(&task(), other).unwrap() else {
            panic!("expected targeted state write");
        };
        assert!(
            matches!(*command, TaskWrite::State { task: reference, state }
            if reference == task() && state == expected)
        );
        for invalid in ["", "Open", "Done", "31", "reopened"] {
            assert!(field.choose(&task(), invalid).is_err());
        }
        let foreign = TaskRef {
            project: ProjectTarget::jira("https://team.atlassian.net", "CAN").unwrap(),
            id: "CAN-42".into(),
            title: String::new(),
        };
        // Even an otherwise no-op choice must reject another provider.
        assert!(field.choose(&foreign, selected).is_err());
        assert!(field.choose(&foreign, other).is_err());
    }
}

#[test]
fn shared_status_github_load_uses_current_get_state_then_reopens_with_minimal_patch() {
    block_on(async {
        let mut closed = issue();
        closed["state"] = json!("closed");
        let mock = Mock::new(vec![response(200, closed), response(200, issue())]);
        let github = Github::new(mock.clone(), "private-token".into());
        let fields = status::load(&github, &task()).await.unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].label(), "Closed");
        assert_eq!(fields[0].selected(), Some("closed"));
        assert!(fields[0].choose(&task(), "closed").unwrap().is_none());
        assert_eq!(mock.sent.lock().unwrap().len(), 1);
        let Some(StatusChange::Write(command)) = fields[0].choose(&task(), "open").unwrap() else {
            panic!("a closed issue must be reopened, not treated as an open-state no-op");
        };
        let receipt = github.write(&command).await.unwrap();
        let reopened = receipt.task.unwrap();
        assert_eq!(reopened.state, TaskState::Open);
        let fresh_fields = status::from_task(&reopened);
        assert_eq!(fresh_fields.len(), 1);
        assert_eq!(fresh_fields[0].label(), "Open");
        assert!(fresh_fields[0].choose(&task(), "open").unwrap().is_none());
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].method, "GET");
        assert_eq!(sent[0].path, "/repos/owner/repo/issues/42");
        assert_eq!(sent[0].body, Value::Null);
        assert_eq!(sent[1].method, "PATCH");
        assert_eq!(sent[1].path, "/repos/owner/repo/issues/42");
        assert_eq!(
            sent[1].body,
            json!({"state":"open","state_reason":"reopened"})
        );
    });
}

#[test]
fn shared_status_github_load_rejects_response_for_another_task() {
    block_on(async {
        let mut foreign = issue();
        foreign["number"] = json!(43);
        let mock = Mock::new(vec![response(200, foreign)]);
        let github = Github::new(mock.clone(), "private-token".into());
        assert!(status::load(&github, &task()).await.is_err());
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].method, "GET");
    });
}

#[test]
fn creates_issue_and_reports_silently_ignored_metadata() {
    block_on(async {
        let mock = Mock::new(vec![response(201, issue())]);
        let github = Github::new(mock.clone(), "private-token".into());
        let result = github
            .write(&TaskWrite::Create {
                project: project(),
                draft: IssueDraft {
                    fields: Default::default(),
                    title: "Title".into(),
                    body: "Body".into(),
                    labels: vec!["requested".into()],
                    assignees: vec![],
                    milestone: Some(3),
                },
            })
            .await
            .unwrap();
        assert_eq!(result.task.unwrap().reference.id, "42");
        assert!(result.notice.unwrap().contains("Issue created"));
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].method, "POST");
        assert_eq!(sent[0].path, "/repos/owner/repo/issues");
        assert_eq!(sent[0].body["labels"], json!(["requested"]));
    });
}
#[test]
fn patch_sends_only_changed_text_fields_including_empty_body() {
    block_on(async {
        let mock = Mock::new(vec![response(200, issue())]);
        Github::new(mock.clone(), "private-token".into())
            .write(&TaskWrite::Edit {
                task: task(),
                title: None,
                body: Some("".into()),
                labels: None,
                assignees: None,
                milestone: None,
            })
            .await
            .unwrap();
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent[0].method, "PATCH");
        assert_eq!(sent[0].body, json!({"body":""}));
    });
}
#[test]
fn patch_sends_only_changed_metadata_and_preserves_clear_semantics() {
    block_on(async {
        let mock = Mock::new(vec![response(
            200,
            json!({
                "number":42,
                "title":"Title",
                "body":"Body",
                "state":"open",
                "labels":[],
                "assignees":[{"login":"sam"}],
                "milestone":{"number":7,"title":"Sprint 7"}
            }),
        )]);
        Github::new(mock.clone(), "private-token".into())
            .write(&TaskWrite::Edit {
                task: task(),
                title: None,
                body: None,
                labels: Some(vec![]),
                assignees: Some(vec!["sam".into()]),
                milestone: Some(Some(7)),
            })
            .await
            .unwrap();
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent[0].method, "PATCH");
        assert_eq!(sent[0].body["labels"], json!([]));
        assert_eq!(sent[0].body["assignees"], json!(["sam"]));
        assert_eq!(sent[0].body["milestone"], json!(7));
        assert!(sent[0].body.get("title").is_none());
        assert!(sent[0].body.get("body").is_none());
    });
}
#[test]
fn label_removal_escapes_names_and_keeps_success_when_refresh_fails() {
    block_on(async {
        let mock = Mock::new(vec![
            response(200, json!([{"name":"keep"}])),
            response(500, json!({})),
        ]);
        let result = Github::new(mock.clone(), "private-token".into())
            .write(&TaskWrite::Label {
                task: task(),
                label: "bug/UI #1".into(),
                add: false,
            })
            .await
            .unwrap();
        assert!(result.notice.unwrap().contains("change saved"));
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent[0].method, "DELETE");
        assert_eq!(
            sent[0].path,
            "/repos/owner/repo/issues/42/labels/bug%2FUI%20%231"
        );
        assert_eq!(sent[1].method, "GET");
    });
}
#[test]
fn metadata_commands_are_targeted_and_milestone_can_be_cleared() {
    block_on(async {
        let mock = Mock::new(vec![
            response(200, issue()),
            response(200, issue()),
            response(200, issue()),
        ]);
        let github = Github::new(mock.clone(), "private-token".into());
        github
            .write(&TaskWrite::Assignee {
                task: task(),
                login: "remove-me".into(),
                add: false,
            })
            .await
            .unwrap();
        github
            .write(&TaskWrite::Milestone {
                task: task(),
                number: None,
            })
            .await
            .unwrap();
        github
            .write(&TaskWrite::State {
                task: task(),
                state: TaskState::Closed,
            })
            .await
            .unwrap();
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent[0].body, json!({"assignees":["remove-me"]}));
        assert_eq!(sent[0].method, "DELETE");
        assert_eq!(sent[1].body, json!({"milestone":null}));
        assert_eq!(
            sent[2].body,
            json!({"state":"closed","state_reason":"completed"})
        );
    });
}
#[test]
fn comments_support_create_edit_and_delete_with_no_content_response() {
    block_on(async {
        let mock = Mock::new(vec![
            response(201, comment_json()),
            response(200, comment_json()),
            Reply {
                status: 204,
                body: String::new(),
                rate: false,
            },
        ]);
        let github = Github::new(mock.clone(), "private-token".into());
        assert_eq!(
            github
                .write(&TaskWrite::AddComment {
                    task: task(),
                    body: "After".into()
                })
                .await
                .unwrap()
                .comment
                .unwrap()
                .id,
            "node-100"
        );
        github
            .write(&TaskWrite::EditComment {
                task: task(),
                comment: comment(),
                body: "After".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            github
                .write(&TaskWrite::DeleteComment {
                    task: task(),
                    comment: comment()
                })
                .await
                .unwrap()
                .deleted_comment
                .as_deref(),
            Some("node-100")
        );
        let sent = mock.sent.lock().unwrap();
        assert_eq!(sent[0].path, "/repos/owner/repo/issues/42/comments");
        assert_eq!(sent[1].path, "/repos/owner/repo/issues/comments/100");
        assert_eq!(sent[2].method, "DELETE");
    });
}
#[test]
fn validation_and_comment_ownership_prevent_requests() {
    block_on(async {
        let mock = Mock::new(vec![]);
        let github = Github::new(mock.clone(), "private-token".into());
        assert!(
            github
                .write(&TaskWrite::AddComment {
                    task: task(),
                    body: "  ".into()
                })
                .await
                .is_err()
        );
        let mut foreign = comment();
        foreign.url = "https://github.com/other/repo/issues/42#issuecomment-100".into();
        assert!(
            github
                .write(&TaskWrite::DeleteComment {
                    task: task(),
                    comment: foreign
                })
                .await
                .is_err()
        );
        let mut forbidden = comment();
        forbidden.can_edit = Some(false);
        assert!(
            github
                .write(&TaskWrite::EditComment {
                    task: task(),
                    comment: forbidden,
                    body: "Edit".into()
                })
                .await
                .is_err()
        );
        assert!(mock.sent.lock().unwrap().is_empty());
    });
}
#[test]
fn uncertain_writes_are_never_automatically_retried_or_reported_as_rejected() {
    block_on(async {
        for reply in [
            response(0, json!({})),
            response(500, json!({"message":"private-token"})),
            Reply {
                status: 201,
                body: "not json".into(),
                rate: false,
            },
        ] {
            let mock = Mock::new(vec![reply]);
            let error = Github::new(mock.clone(), "private-token".into())
                .write(&TaskWrite::AddComment {
                    task: task(),
                    body: "Comment".into(),
                })
                .await
                .unwrap_err();
            assert!(matches!(error, WriteError::Uncertain(_)));
            assert!(!error.to_string().contains("private-token"));
            assert_eq!(mock.sent.lock().unwrap().len(), 1);
        }
    });
}
#[test]
fn write_rate_limits_and_permissions_have_distinct_messages() {
    block_on(async {
        let mock = Mock::new(vec![
            Reply {
                status: 403,
                body: "{}".into(),
                rate: true,
            },
            response(403, json!({})),
            response(422, json!({})),
        ]);
        let github = Github::new(mock, "private-token".into());
        let command = TaskWrite::AddComment {
            task: task(),
            body: "Comment".into(),
        };
        assert!(
            github
                .write(&command)
                .await
                .unwrap_err()
                .to_string()
                .contains("rate limit")
        );
        assert!(
            github
                .write(&command)
                .await
                .unwrap_err()
                .to_string()
                .contains("read and write")
        );
        assert!(matches!(
            github.write(&command).await.unwrap_err(),
            WriteError::Rejected(_)
        ));
    });
}
#[test]
fn task_drafts_restore_text_and_ambiguity_and_reject_future_schema() {
    block_on(async {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("drafts.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let drafts = TaskDrafts::from([(
            "account:repo:new".into(),
            TaskDraft {
                project: project(),
                value: IssueDraft {
                    title: "Unsent".into(),
                    body: "**Keep my text**".into(),
                    ..Default::default()
                },
                baseline: None,
                warning: Some("Check GitHub before retrying".into()),
            },
        )]);
        client.save_task_drafts(drafts.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(client.load_task_drafts().await.unwrap(), drafts);
        client.shutdown().await.unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("UPDATE _canopy_rust_task_drafts SET version=2", [])
            .unwrap();
        let before: String = db
            .query_row("SELECT payload FROM _canopy_rust_task_drafts", [], |r| {
                r.get(0)
            })
            .unwrap();
        drop(db);
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert!(client.save_task_drafts(Default::default()).await.is_err());
        client.shutdown().await.unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        let after: String = db
            .query_row("SELECT payload FROM _canopy_rust_task_drafts", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(before, after);
    });
}
