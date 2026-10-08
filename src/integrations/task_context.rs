//! Complete task handoff, prepared before creating a worktree. No UI or shell I/O.
use super::*;
use futures_lite::io::AsyncReadExt;
use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt, RedirectPolicy, Request, Url};
use std::{collections::HashSet, io::Write, path::PathBuf, sync::Arc, time::Duration};

const TEXT_LIMIT: usize = 512 * 1024;
const TOTAL_LIMIT: usize = 100 * 1024 * 1024;

pub struct TaskContext {
    pub prompt: String,
    directory: tempfile::TempDir,
}
impl TaskContext {
    /// Retain the private bundle for subsequent provider-session resume.
    pub fn retain(self) {
        let _ = self.directory.keep();
    }
}

fn append(out: &mut String, text: &str) -> Result<(), String> {
    if out.len() + text.len() > TEXT_LIMIT {
        return Err("Task and comments exceed the 512 KiB handoff limit.".into());
    }
    // Remote text is data, never terminal control input.
    out.extend(
        text.chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t')),
    );
    Ok(())
}

fn github_asset(raw: &str) -> bool {
    Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
            && match u.host_str() {
                Some("github.com") => {
                    u.path().starts_with("/user-attachments/") || u.path().starts_with("/files/")
                }
                Some("user-images.githubusercontent.com") => true,
                _ => false,
            }
    })
}

fn github_assets(text: &str) -> Result<Vec<String>, String> {
    fn walk(node: &markdown_parser::mdast::Node, found: &mut Vec<String>) {
        use markdown_parser::mdast::Node;
        let url = match node {
            Node::Image(v) => Some(v.url.as_str()),
            Node::Link(v) => Some(v.url.as_str()),
            Node::Definition(v) => Some(v.url.as_str()),
            // GitHub also uses HTML img/video tags and bare attachment URLs.
            Node::Html(v) => {
                for word in v.value.split(['"', '\'', ' ', '\n', '<', '>']) {
                    if github_asset(word) {
                        found.push(word.into());
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(url) = url.filter(|u| github_asset(u)) {
            found.push(url.into());
        }
        if let Some(children) = node.children() {
            for child in children {
                walk(child, found);
            }
        }
    }
    let ast = markdown_parser::to_mdast(text, &markdown_parser::ParseOptions::gfm())
        .map_err(|_| "Could not read task attachment links.")?;
    let mut found = Vec::new();
    walk(&ast, &mut found);
    let mut seen = HashSet::new();
    found.retain(|v| seen.insert(v.clone()));
    Ok(found)
}

async fn github_download(
    http: &Arc<dyn HttpClient>,
    url: &str,
    token: &str,
) -> Result<Vec<u8>, String> {
    if !github_asset(url) {
        return Err("Unsupported GitHub attachment URL.".into());
    }
    let mut current = Url::parse(url).map_err(|_| "Invalid attachment URL.")?;
    let mut response = None;
    for _ in 0..5 {
        // Authenticate only the exact GitHub attachment origin; never a CDN redirect.
        let mut request = Request::builder()
            .uri(current.as_str())
            .header("User-Agent", "Canopy-Desktop")
            .follow_redirects(RedirectPolicy::NoFollow)
            .timeout(Duration::from_secs(60));
        if current.host_str() == Some("github.com") && github_asset(current.as_str()) {
            let mut auth =
                gpui_kit::http_client::http::HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|_| "Could not prepare GitHub attachment credentials.")?;
            auth.set_sensitive(true);
            request = request.header("Authorization", auth);
        }
        let request = request
            .body(AsyncBody::empty())
            .map_err(|_| "Invalid attachment request.")?;
        let reply = http
            .send(request)
            .await
            .map_err(|_| "Could not download GitHub attachment.")?;
        if reply.status().as_u16() == 200 {
            response = Some(reply);
            break;
        }
        if matches!(reply.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            let next = reply
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| current.join(v).ok())
                .ok_or("Invalid GitHub attachment redirect.")?;
            let allowed = github_asset(next.as_str())
                || (next.scheme() == "https"
                    && next.username().is_empty()
                    && next.password().is_none()
                    && next.port().is_none()
                    && matches!(
                        next.host_str(),
                        Some(
                            "private-user-images.githubusercontent.com"
                                | "github-production-user-asset-6210df.s3.amazonaws.com"
                                | "github-production-user-asset-6210df.s3.us-east-1.amazonaws.com"
                        )
                    ));
            if !allowed {
                return Err(
                    "GitHub attachment redirected outside its supported asset hosts.".into(),
                );
            }
            current = next;
        } else {
            return Err(format!(
                "GitHub attachment download failed (HTTP {}). The attachment may require browser authentication.",
                reply.status()
            ));
        }
    }
    let response = response.ok_or("Too many GitHub attachment redirects.")?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .take(attachments::PREVIEW_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "Could not read GitHub attachment.")?;
    if bytes.len() > attachments::PREVIEW_LIMIT {
        return Err("Attachment exceeds 25 MiB.".into());
    }
    Ok(bytes)
}

/// Called on the background executor; TempDir removes incomplete bundles on any error.
pub async fn prepare(
    http: Arc<dyn HttpClient>,
    account: Account,
    task: TaskRef,
    agent: String,
) -> Result<TaskContext, String> {
    let token = credentials::load(&account.credential)?;
    let parent = crate::platform::directories::data_dir()?.join("task-context");
    prepare_in(http, account, task, token, parent, &agent, cfg!(windows)).await
}

async fn prepare_in(
    http: Arc<dyn HttpClient>,
    account: Account,
    task: TaskRef,
    token: String,
    parent: PathBuf,
    agent: &str,
    windows: bool,
) -> Result<TaskContext, String> {
    let provider = client(http.clone(), &account, token.clone())?;
    let item = provider.task(&task).await?;
    if item.body.chars().count() >= 65536 {
        return Err("Task description reaches the provider reader limit; a complete handoff cannot be guaranteed.".into());
    }
    let mut prompt = String::new();
    append(
        &mut prompt,
        &format!(
            "Implement the following task. Use its description, comments and attached files as task context.\n\n# {} — {}\n{}\n\n## Description\n{}\n\n## Comments\n",
            item.reference.label(),
            item.reference.title,
            item.url,
            item.body
        ),
    )?;
    let mut cursor = None;
    let mut cursors = HashSet::new();
    let mut comments = HashSet::new();
    loop {
        let page = provider.comments(&task, cursor.as_deref()).await?;
        for comment in page.comments {
            if comment.body.chars().count() >= 65536 {
                return Err("A comment reaches the provider reader limit; a complete handoff cannot be guaranteed.".into());
            }
            if comments.insert(comment.id) {
                append(
                    &mut prompt,
                    &format!(
                        "\n### {} — {}\n{}\n",
                        comment.author, comment.created_at, comment.body
                    ),
                )?;
            }
        }
        cursor = page.next_cursor;
        let Some(next) = &cursor else { break };
        if !cursors.insert(next.clone()) || cursors.len() > 100 {
            return Err("Could not retrieve all comment pages.".into());
        }
    }
    let files: Vec<(String, String)> = match task.project.provider {
        Provider::Jira => item
            .jira
            .as_ref()
            .ok_or("Jira task details are missing.")?
            .fields["attachment"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| {
                Ok((
                    v["id"]
                        .as_str()
                        .ok_or("Invalid Jira attachment ID.")?
                        .into(),
                    v["filename"].as_str().unwrap_or("attachment").into(),
                ))
            })
            .collect::<Result<_, String>>()?,
        Provider::Youtrack => item
            .youtrack
            .as_ref()
            .ok_or("YouTrack task details are missing.")?
            .attachments
            .iter()
            .map(|v| (v.id.clone(), v.name.clone()))
            .collect(),
        Provider::Github => github_assets(&prompt)?
            .into_iter()
            .map(|url| {
                let name = Url::parse(&url)
                    .ok()
                    .and_then(|v| v.path_segments()?.next_back().map(str::to_owned))
                    .unwrap_or("attachment".into());
                (url, name)
            })
            .collect(),
    };
    if files.len() > 64 {
        return Err("Task has more than 64 attachments.".into());
    }
    crate::platform::directories::ensure_private_dir(&parent)?;
    let mut builder = tempfile::Builder::new();
    builder.prefix("task-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    let directory = builder.tempdir_in(parent).map_err(|e| e.to_string())?;
    let mut total = 0;
    append(&mut prompt, "\n## Local attachments\n")?;
    for (index, (id, name)) in files.into_iter().enumerate() {
        let bytes = match &account.scope {
            AccountScope::Jira {
                site,
                email,
                cloud_id,
            } => {
                jira::Jira::new(
                    http.clone(),
                    site,
                    email,
                    cloud_id.as_deref(),
                    token.clone(),
                    account.login.clone(),
                )?
                .download_attachment(&task, &id)
                .await?
            }
            AccountScope::Youtrack { service } => {
                youtrack::Youtrack::new(
                    http.clone(),
                    service,
                    token.clone(),
                    account.login.clone(),
                )?
                .download_attachment(&task, &id)
                .await?
            }
            _ => github_download(&http, &id, &token).await?,
        };
        total += bytes.len();
        if total > TOTAL_LIMIT {
            return Err("Task attachments exceed 100 MiB in total.".into());
        }
        let path = directory
            .path()
            .join(format!("{index}-{}", attachments::preview_name(&name)));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&path)
            .and_then(|mut file| file.write_all(&bytes))
            .map_err(|e| e.to_string())?;
        let reference = attachment_reference(
            agent,
            path.to_str().ok_or("Attachment path is not UTF-8.")?,
            windows,
        )?;
        append(
            &mut prompt,
            &format!("\n- {}: {reference}\n", attachments::preview_name(&name)),
        )?;
    }
    Ok(TaskContext { prompt, directory })
}

fn attachment_reference(agent: &str, path: &str, windows: bool) -> Result<String, String> {
    if path.chars().any(char::is_control) || path.contains('"') {
        return Err("Attachment path cannot be represented safely for the agent.".into());
    }
    if !matches!(agent, "claude" | "codex") {
        return Err("This agent does not support task attachment references.".into());
    }
    if windows {
        if path.chars().any(char::is_whitespace) {
            Ok(format!("@\"{path}\""))
        } else {
            Ok(format!("@{path}"))
        }
    } else {
        Ok(format!("@{}", shell_words::quote(path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::http_client::{Response, http::HeaderValue};
    use serde_json::json;
    use std::{collections::VecDeque, sync::Mutex};

    struct Mock(Mutex<VecDeque<(u16, String, Option<String>)>>);
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
            if matches!(request.uri().host(), Some("api.github.com" | "github.com")) {
                assert!(request.headers()["Authorization"].is_sensitive());
            } else {
                assert!(request.headers().get("Authorization").is_none());
            }
            assert_eq!(
                request.extensions().get::<RedirectPolicy>(),
                Some(&RedirectPolicy::NoFollow)
            );
            let (status, body, location) = self.0.lock().unwrap().pop_front().unwrap();
            Box::pin(async move {
                let mut response = Response::builder().status(status);
                if let Some(location) = location {
                    response = response.header("Location", location);
                }
                Ok(response.body(AsyncBody::from(body))?)
            })
        }
    }
    fn page(id: &str, text: &str, next: Option<&str>) -> String {
        json!({"data":{"repository":{"issue":{"comments":{
            "totalCount":2,"pageInfo":{"hasNextPage":next.is_some(),"endCursor":next},
            "nodes":[{"id":id,"body":text,"createdAt":"2026-09-11","url":"https://github.com/a/b/issues/1","author":{"login":"author"}}]
        }}}}}).to_string()
    }
    fn fixture(status: u16) -> (Arc<dyn HttpClient>, Account, TaskRef) {
        let mock = Mock(Mutex::new(VecDeque::from([
            (
                200,
                json!({"number":1,"title":"Actual title","body":"Description","state":"open"})
                    .to_string(),
                None,
            ),
            (200, page("one", "First comment", Some("cursor")), None),
            (
                200,
                page(
                    "two",
                    "Last comment ![image](https://github.com/user-attachments/assets/a.png)",
                    None,
                ),
                None,
            ),
            (
                302,
                String::new(),
                Some(
                    "https://private-user-images.githubusercontent.com/1/a.png?signature=test"
                        .into(),
                ),
            ),
            (status, "image-bytes".into(), None),
        ])));
        let account = Account {
            id: "test".into(),
            provider: Provider::Github,
            login: "author".into(),
            credential: "unused".into(),
            scope: AccountScope::Default,
        };
        let task = TaskRef {
            project: ProjectTarget::github("a/b").unwrap(),
            id: "1".into(),
            title: "Old title".into(),
        };
        (Arc::new(mock), account, task)
    }
    #[test]
    fn handoff_loads_every_comment_page_and_downloads_comment_attachments() {
        futures_lite::future::block_on(async {
            let parent = tempfile::tempdir().unwrap();
            let (http, account, task) = fixture(200);
            let context = prepare_in(
                http,
                account,
                task,
                "test-token".into(),
                parent.path().into(),
                "claude",
                false,
            )
            .await
            .unwrap();
            assert!(context.prompt.contains("Actual title"));
            assert!(context.prompt.contains("First comment"));
            assert!(context.prompt.contains("Last comment"));
            let file = context.directory.path().join("0-a.png");
            assert_eq!(std::fs::read(&file).unwrap(), b"image-bytes");
            assert!(context.prompt.contains(file.to_str().unwrap()));
            drop(context);
            assert!(!file.exists(), "failed/uncommitted handoffs clean up files");
        });
    }
    #[test]
    fn failed_download_cleans_bundle_and_never_returns_partial_prompt() {
        futures_lite::future::block_on(async {
            let parent = tempfile::tempdir().unwrap();
            let (http, account, task) = fixture(403);
            assert!(
                prepare_in(
                    http,
                    account,
                    task,
                    "test-token".into(),
                    parent.path().into(),
                    "claude",
                    false,
                )
                .await
                .is_err()
            );
            assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
        });
    }
    #[test]
    fn hostile_attachment_redirect_is_rejected_before_requesting_it() {
        futures_lite::future::block_on(async {
            let http: Arc<dyn HttpClient> = Arc::new(Mock(Mutex::new(VecDeque::from([(
                302,
                String::new(),
                Some("https://attacker.invalid/file".into()),
            )]))));
            let error = github_download(
                &http,
                "https://github.com/user-attachments/assets/a.png",
                "test-token",
            )
            .await
            .unwrap_err();
            assert!(error.contains("outside"));
        });
    }
    #[test]
    fn only_github_attachment_hosts_are_downloaded() {
        let urls = github_assets("![a](https://github.com/user-attachments/assets/abc)\n[x](https://evil.test/a.png)\n<img src=\"https://user-images.githubusercontent.com/1/a.png\">\n![again](https://github.com/user-attachments/assets/abc)").unwrap();
        assert_eq!(urls.len(), 2);
        assert!(!github_asset(
            "https://github.com.evil.test/user-attachments/a"
        ));
        assert!(!github_asset(
            "https://user:pass@github.com/user-attachments/a"
        ));
    }
    #[test]
    fn prompt_removes_terminal_controls_and_refuses_truncation() {
        let mut text = String::new();
        append(&mut text, "hello\x1b\0\r\nworld").unwrap();
        assert_eq!(text, "hello\nworld");
        assert!(append(&mut text, &"a".repeat(TEXT_LIMIT)).is_err());
    }

    #[test]
    fn agent_attachment_references_preserve_windows_paths_without_shell_parsing() {
        for agent in ["claude", "codex"] {
            assert_eq!(
                attachment_reference(agent, r"C:\Users\Żaneta\task image.png", true).unwrap(),
                r#"@"C:\Users\Żaneta\task image.png""#
            );
            assert_eq!(
                attachment_reference(agent, r"\\server\share\task.png", true).unwrap(),
                r"@\\server\share\task.png"
            );
        }
        assert!(attachment_reference("other", r"C:\task.png", true).is_err());
        assert!(attachment_reference("claude", "C:\\bad\"name", true).is_err());
    }
}
