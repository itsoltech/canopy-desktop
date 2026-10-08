use canopy_desktop::{
    integrations::{github::Github, *},
    settings::{Access, SettingsClient},
};
use futures_lite::io::AsyncReadExt;
use gpui_kit::http_client::{
    AsyncBody, HttpClient, RedirectPolicy, Request, Response, Url, http::HeaderValue,
};
use std::{
    collections::VecDeque,
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex},
};
#[test]
fn origin_detection_and_explicit_override_are_scoped() {
    let expected = ProjectTarget::github("owner/project").unwrap();
    for remote in [
        "git@github.com:owner/project.git",
        "https://github.com/owner/project.git",
        "ssh://git@github.com/owner/project.git",
        "ssh://git@ssh.github.com:443/owner/project.git",
    ] {
        assert_eq!(from_origin(remote), Some(expected.clone()));
    }
    for remote in [
        "https://evil.example/owner/project",
        "https://github.com.evil.example/owner/project",
        "https://github.com/owner/project/issues",
        "/local/repo",
        "git@github.com:../repo",
    ] {
        assert!(from_origin(remote).is_none());
    }
    let mut config = Config::default();
    let mut context = RepositoryContext {
        common: "/tmp/repo/.git".into(),
        repository: "/tmp/repo".into(),
        inferred: Some(expected.clone()),
    };
    assert_eq!(config.task_project(&context), Some(expected));
    let override_ = ProjectTarget::github("team/tasks").unwrap();
    config
        .overrides
        .insert(context.common.clone(), override_.clone());
    context.inferred = Some(ProjectTarget::github("fork/project").unwrap());
    assert_eq!(config.task_project(&context), Some(override_));
}
struct Mock {
    responses: Mutex<VecDeque<(u16, serde_json::Value, bool)>>,
    requests: Arc<Mutex<Vec<String>>>,
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
    ) -> Pin<
        Box<
            dyn Future<Output = gpui_kit::http_client::Result<Response<AsyncBody>>>
                + Send
                + 'static,
        >,
    > {
        assert!(
            request.method() == "GET"
                || (request.method() == "POST" && request.uri().path() == "/graphql")
        );
        assert_eq!(request.uri().host(), Some("api.github.com"));
        assert!(request.headers()["Authorization"].is_sensitive());
        assert_eq!(
            request.extensions().get::<RedirectPolicy>(),
            Some(&RedirectPolicy::NoFollow)
        );
        let requests = self.requests.clone();
        let graphql = request.method() == "POST";
        let uri = request.uri().to_string();
        let (status, value, more) = self.responses.lock().unwrap().pop_front().unwrap();
        Box::pin(async move {
            let mut body = Vec::new();
            request.into_body().read_to_end(&mut body).await.unwrap();
            if graphql {
                let body = String::from_utf8(body).unwrap();
                let json: serde_json::Value = serde_json::from_str(&body).unwrap();
                assert!(
                    json["query"]
                        .as_str()
                        .unwrap()
                        .starts_with("query CanopyIssue")
                );
                assert!(!json["query"].as_str().unwrap().contains("mutation"));
                requests.lock().unwrap().push(body);
            } else {
                requests.lock().unwrap().push(uri);
            }
            let mut response = Response::builder().status(status);
            if more {
                response = response.header("Link", "<https://api.github.com/page>; rel=\"next\"");
            }
            Ok(response.body(AsyncBody::from(value.to_string()))?)
        })
    }
}
fn graphql_page(first: u64, count: u64, total: usize, cursor: Option<&str>) -> serde_json::Value {
    let nodes: Vec<_> = (first..first + count)
        .map(|number| {
            serde_json::json!({
                "number":number,"title":format!("Issue {number}"),"body":"Details","state":"OPEN",
                "labels":{"nodes":[{"name":"bug"}]},"assignees":{"nodes":[{"login":"octocat"}]}
            })
        })
        .collect();
    serde_json::json!({"data":{"repository":{"issues":{"nodes":nodes,"totalCount":total,
        "pageInfo":{"hasNextPage":cursor.is_some(),"endCursor":cursor}}}}})
}
#[test]
fn github_verifies_identity_and_fetches_issue_only_cursor_pages() {
    futures_lite::future::block_on(async {
        let mock = Arc::new(Mock {
            responses: Mutex::new(VecDeque::from([
                (200, serde_json::json!({"login":"octocat"}), false),
                (
                    200,
                    graphql_page(1, 30, 31, Some("cursor-first-page")),
                    false,
                ),
                (200, graphql_page(31, 1, 31, None), false),
            ])),
            requests: Arc::new(Mutex::new(vec![])),
        });
        let github = Github::new(mock.clone(), "test-token".into());
        assert_eq!(github.verify().await.unwrap(), "octocat");
        let target = ProjectTarget::github("owner/repo").unwrap();
        let first = github.list(&target, TaskState::Open, None).await.unwrap();
        assert_eq!(first.tasks.len(), 30);
        assert_eq!(first.total_count, Some(31));
        assert_eq!(first.tasks[0].labels, vec!["bug"]);
        let second = github
            .list(&target, TaskState::Open, first.next_cursor.as_deref())
            .await
            .unwrap();
        assert_eq!(second.tasks.len(), 1);
        assert_eq!(second.tasks[0].reference.id, "31");
        assert!(second.next_cursor.is_none());
        let requests = mock.requests.lock().unwrap();
        let first_query: serde_json::Value = serde_json::from_str(&requests[1]).unwrap();
        assert!(
            first_query["query"]
                .as_str()
                .unwrap()
                .contains("issues(first: 30")
        );
        assert!(
            !first_query["query"]
                .as_str()
                .unwrap()
                .contains("pullRequests")
        );
        assert_eq!(
            first_query["variables"],
            serde_json::json!({"owner":"owner","name":"repo","states":["OPEN"],"after":null})
        );
        let second_query: serde_json::Value = serde_json::from_str(&requests[2]).unwrap();
        assert_eq!(second_query["variables"]["after"], "cursor-first-page");
    });
}
#[test]
fn small_issue_repository_is_complete_in_one_request() {
    futures_lite::future::block_on(async {
        let mock = Arc::new(Mock {
            responses: Mutex::new(VecDeque::from([(200, graphql_page(1, 6, 6, None), false)])),
            requests: Arc::new(Mutex::new(vec![])),
        });
        let page = Github::new(mock.clone(), "test-token".into())
            .list(
                &ProjectTarget::github("owner/repo").unwrap(),
                TaskState::Closed,
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.tasks.len(), 6);
        assert_eq!(page.total_count, Some(6));
        assert!(page.next_cursor.is_none());
        let queries = mock.requests.lock().unwrap();
        assert_eq!(queries.len(), 1);
        let query: serde_json::Value = serde_json::from_str(&queries[0]).unwrap();
        assert_eq!(query["variables"]["states"], serde_json::json!(["CLOSED"]));
    });
}
#[test]
fn graphql_errors_and_broken_cursors_never_publish_a_partial_page() {
    futures_lite::future::block_on(async {
        let mut partial = graphql_page(1, 1, 2, Some("next"));
        partial["errors"] = serde_json::json!([{"type":"FORBIDDEN","message":"private-token"}]);
        let rate =
            serde_json::json!({"errors":[{"type":"RATE_LIMITED","message":"private-token"}]});
        let mut missing_cursor = graphql_page(1, 1, 2, None);
        missing_cursor["data"]["repository"]["issues"]["pageInfo"]["hasNextPage"] =
            serde_json::json!(true);
        for response in [
            partial,
            rate,
            missing_cursor,
            graphql_page(1, 1, 2, Some("same")),
            serde_json::json!({"data":{"repository":null}}),
        ] {
            let mock = Arc::new(Mock {
                responses: Mutex::new(VecDeque::from([(200, response, false)])),
                requests: Arc::new(Mutex::new(vec![])),
            });
            let error = Github::new(mock, "private-token".into())
                .list(
                    &ProjectTarget::github("owner/repo").unwrap(),
                    TaskState::Open,
                    Some("same"),
                )
                .await
                .unwrap_err();
            assert!(!error.contains("private-token"));
        }
    });
}
#[test]
fn linked_pull_request_is_not_presented_as_a_task() {
    futures_lite::future::block_on(async {
        let mock = Arc::new(Mock {
            responses: Mutex::new(VecDeque::from([(
                200,
                serde_json::json!({"number":1,"title":"PR","body":"","state":"open","pull_request":{}}),
                false,
            )])),
            requests: Arc::new(Mutex::new(vec![])),
        });
        let error = Github::new(mock, "test-token".into())
            .task(&TaskRef {
                project: ProjectTarget::github("owner/repo").unwrap(),
                id: "1".into(),
                title: "PR".into(),
            })
            .await
            .unwrap_err();
        assert!(error.contains("not an issue"));
    });
}
#[test]
fn github_errors_do_not_expose_tokens_or_follow_redirects() {
    futures_lite::future::block_on(async {
        for status in [401, 403, 404, 429, 301, 500] {
            let mock = Arc::new(Mock {
                responses: Mutex::new(VecDeque::from([(
                    status,
                    serde_json::json!({"message":"test-token"}),
                    false,
                )])),
                requests: Arc::new(Mutex::new(vec![])),
            });
            let error = Github::new(mock.clone(), "test-token".into())
                .verify()
                .await
                .unwrap_err();
            assert!(!error.contains("test-token"));
            assert_eq!(mock.requests.lock().unwrap().len(), 1);
        }
    });
}
#[test]
fn integration_metadata_survives_sqlite_and_rejects_future_versions() {
    futures_lite::future::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let mut config = Config::default();
        let id = uuid::Uuid::new_v4().to_string();
        config.accounts.push(Account {
            scope: AccountScope::Default,
            id: id.clone(),
            provider: Provider::Github,
            login: "octocat".into(),
            credential: id,
        });
        let target = ProjectTarget::github("owner/repo").unwrap();
        config
            .overrides
            .insert("/tmp/repo/.git".into(), target.clone());
        config.links.insert(
            "/tmp/worktree".into(),
            vec![TaskRef {
                project: target,
                id: "42".into(),
                title: "A task".into(),
            }],
        );
        client.save_integrations(config.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(client.load_integrations().await.unwrap(), config);
        client.shutdown().await.unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("UPDATE _canopy_rust_integrations SET version=99", [])
            .unwrap();
        drop(db);
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert!(client.load_integrations().await.is_err());
        assert!(client.save_integrations(Config::default()).await.is_err());
        client.shutdown().await.unwrap();
    });
}
#[test]
fn linked_worktrees_share_origin_and_override_identity() {
    futures_lite::future::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "git@github.com:owner/project.git")
            .unwrap();
        let mut index = repo.index().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        let other = tempfile::tempdir().unwrap();
        let linked = other.path().join("linked");
        repo.worktree("linked", &linked, None).unwrap();
        let client = canopy_desktop::git::GitClient::start().unwrap();
        let a = client
            .task_repository(dir.path().into())
            .await
            .unwrap()
            .unwrap();
        let b = client.task_repository(linked).await.unwrap().unwrap();
        assert_eq!(a.common, b.common);
        assert_eq!(a.inferred, b.inferred);
        repo.remote_set_url("origin", "https://github.com/owner/changed.git")
            .unwrap();
        assert_eq!(
            client
                .task_repository(dir.path().into())
                .await
                .unwrap()
                .unwrap()
                .inferred
                .unwrap()
                .key,
            "owner/changed"
        );
        client.shutdown().await;
    });
}
#[test]
fn task_identity_does_not_depend_on_title() {
    let mut a = TaskRef {
        project: ProjectTarget::github("owner/repo").unwrap(),
        id: "42".into(),
        title: "Old".into(),
    };
    let b = a.clone();
    a.title = "New".into();
    assert!(a.same_task(&b));
    assert!(!Path::new(&a.project.key).is_absolute());
}

fn account(scope: AccountScope) -> Account {
    Account {
        id: uuid::Uuid::new_v4().to_string(),
        provider: Provider::Github,
        login: "octocat".into(),
        credential: uuid::Uuid::new_v4().to_string(),
        scope,
    }
}

#[test]
fn organization_credentials_route_before_default_without_crossing_owners() {
    let default = account(AccountScope::Default);
    let acme = account(AccountScope::owner(" Acme ").unwrap());
    let other = account(AccountScope::owner("Other-Org").unwrap());
    let mut config = Config::default()
        .with_account(default.clone())
        .unwrap()
        .with_account(acme.clone())
        .unwrap()
        .with_account(other.clone())
        .unwrap();
    for (repo, expected) in [
        ("ACME/repo", &acme),
        ("other-org/repo", &other),
        ("personal/repo", &default),
        ("third-org/repo", &default),
    ] {
        assert_eq!(
            config.account_for(&ProjectTarget::github(repo).unwrap()),
            Some(expected)
        );
    }
    config.accounts.retain(|a| a.scope != AccountScope::Default);
    assert!(
        config
            .account_for(&ProjectTarget::github("third-org/repo").unwrap())
            .is_none()
    );
    assert!(config.valid());
}

#[test]
fn replacing_a_token_preserves_other_organizations_and_rejects_ambiguous_routing() {
    let default = account(AccountScope::Default);
    let acme = account(AccountScope::owner("acme").unwrap());
    let config = Config::default()
        .with_account(default.clone())
        .unwrap()
        .with_account(acme.clone())
        .unwrap();
    let mut replacement = acme.clone();
    replacement.credential = uuid::Uuid::new_v4().to_string();
    let next = config.with_account(replacement.clone()).unwrap();
    assert_eq!(next.accounts, vec![default.clone(), replacement]);
    assert_eq!(next.retired_credentials, vec![acme.credential.clone()]);
    assert_eq!(config.accounts, vec![default, acme]);
    assert!(next.with_account(account(AccountScope::Default)).is_err());
    assert!(
        next.with_account(account(AccountScope::Owner("ACME".into())))
            .is_err()
    );
    let mut corrupt = next.clone();
    corrupt
        .accounts
        .push(account(AccountScope::Owner("Acme".into())));
    assert!(!corrupt.valid());
    let mut malformed = next;
    malformed.accounts[1].scope = AccountScope::Owner(" acme ".into());
    assert!(!malformed.valid());
}

#[test]
fn token_links_prefill_only_current_task_permissions_and_owner() {
    let classic = Url::parse(&AccountScope::Default.creation_url()).unwrap();
    assert_eq!(classic.host_str(), Some("github.com"));
    assert_eq!(classic.path(), "/settings/tokens/new");
    let query: std::collections::BTreeMap<_, _> = classic.query_pairs().collect();
    assert_eq!(query["scopes"], "repo");
    assert_eq!(query.len(), 2);
    let scoped = Url::parse(&AccountScope::owner("my-org").unwrap().creation_url()).unwrap();
    assert_eq!(scoped.host_str(), Some("github.com"));
    assert_eq!(scoped.path(), "/settings/personal-access-tokens/new");
    let query: std::collections::BTreeMap<_, _> = scoped.query_pairs().collect();
    assert_eq!(query["target_name"], "my-org");
    assert_eq!(query["issues"], "write");
    assert_eq!(query["metadata"], "read");
    assert_eq!(query.len(), 5);
    for owner in [
        "",
        "https://github.com/acme",
        "acme/repo",
        "acme&issues=write",
        "../",
        "-acme",
    ] {
        assert!(AccountScope::owner(owner).is_err(), "{owner}");
    }
}

#[test]
fn multi_org_configuration_restores_and_legacy_account_keeps_default_route() {
    let old = account(AccountScope::Default);
    let mut json = serde_json::to_value(&old).unwrap();
    json.as_object_mut().unwrap().remove("scope");
    let restored: Account = serde_json::from_value(json).unwrap();
    assert_eq!(restored, old);
    futures_lite::future::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multi-org.db");
        let config = Config::default()
            .with_account(restored)
            .unwrap()
            .with_account(account(AccountScope::owner("acme").unwrap()))
            .unwrap()
            .with_account(account(AccountScope::owner("another-org").unwrap()))
            .unwrap();
        let client = SettingsClient::create(&path).await.unwrap();
        client.save_integrations(config.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(client.load_integrations().await.unwrap(), config);
        client.shutdown().await.unwrap();
    });
}

fn comment_page(first: u64, count: u64, total: usize, cursor: Option<&str>) -> serde_json::Value {
    let nodes:Vec<_>=(first..first+count).map(|id|serde_json::json!({
        "id":format!("comment-{id}"),"body":"## Comment\n\n**Markdown** body","author":if id==1{serde_json::Value::Null}else{serde_json::json!({"login":"octocat"})},
        "createdAt":"2026-09-09T12:34:56Z","url":format!("https://github.com/owner/repo/issues/42#issuecomment-{id}")
    })).collect();
    serde_json::json!({"data":{"repository":{"issue":{"comments":{"totalCount":total,"nodes":nodes,"pageInfo":{"hasNextPage":cursor.is_some(),"endCursor":cursor}}}}}})
}
#[test]
fn comments_preserve_markdown_deleted_authors_and_pagination() {
    futures_lite::future::block_on(async {
        let mock = Arc::new(Mock {
            responses: Mutex::new(VecDeque::from([
                (200, comment_page(1, 30, 31, Some("comment-cursor")), false),
                (200, comment_page(31, 1, 31, None), false),
            ])),
            requests: Arc::new(Mutex::new(vec![])),
        });
        let task = TaskRef {
            project: ProjectTarget::github("owner/repo").unwrap(),
            id: "42".into(),
            title: "Task".into(),
        };
        let github = Github::new(mock.clone(), "test-token".into());
        let first = github.comments(&task, None).await.unwrap();
        assert_eq!(first.comments.len(), 30);
        assert_eq!(first.total_count, 31);
        assert_eq!(first.comments[0].author, "Deleted user");
        assert!(first.comments[0].body.contains("**Markdown**"));
        let second = github
            .comments(&task, first.next_cursor.as_deref())
            .await
            .unwrap();
        assert_eq!(second.comments[0].id, "comment-31");
        assert!(second.next_cursor.is_none());
        let queries = mock.requests.lock().unwrap();
        let query: serde_json::Value = serde_json::from_str(&queries[1]).unwrap();
        assert_eq!(
            query["variables"],
            serde_json::json!({"owner":"owner","name":"repo","number":42,"after":"comment-cursor"})
        );
    });
}
#[test]
fn comments_reject_partial_data_and_non_browser_urls() {
    futures_lite::future::block_on(async {
        let mut partial = comment_page(1, 1, 1, None);
        partial["errors"] = serde_json::json!([{"message":"private-token","type":"FORBIDDEN"}]);
        let mut unsafe_url = comment_page(1, 1, 1, None);
        unsafe_url["data"]["repository"]["issue"]["comments"]["nodes"][0]["url"] =
            serde_json::json!("file:///tmp/secret");
        for response in [
            partial,
            unsafe_url,
            serde_json::json!({"data":{"repository":{"issue":null}}}),
        ] {
            let mock = Arc::new(Mock {
                responses: Mutex::new(VecDeque::from([(200, response, false)])),
                requests: Arc::new(Mutex::new(vec![])),
            });
            let error = Github::new(mock, "private-token".into())
                .comments(
                    &TaskRef {
                        project: ProjectTarget::github("owner/repo").unwrap(),
                        id: "42".into(),
                        title: "Task".into(),
                    },
                    None,
                )
                .await
                .unwrap_err();
            assert!(!error.contains("private-token"));
        }
    });
}
