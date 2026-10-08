mod comments;
mod graphql;
mod issues;
mod writes;
use super::*;
use futures_lite::io::AsyncReadExt;
use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt, RedirectPolicy, Request};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
pub struct Github {
    client: Arc<dyn HttpClient>,
    token: String,
}
impl Github {
    pub fn new(client: Arc<dyn HttpClient>, token: String) -> Self {
        Self { client, token }
    }
    async fn get(&self, path: &str) -> Result<serde_json::Value, String> {
        self.request(path, None).await
    }
    async fn request(
        &self,
        path: &str,
        json: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        self.send(
            if json.is_some() { "POST" } else { "GET" },
            path,
            json,
            false,
        )
        .await
        .map_err(|e| e.to_string())
    }
    async fn send(
        &self,
        method: &str,
        path: &str,
        json: Option<serde_json::Value>,
        writing: bool,
    ) -> Result<serde_json::Value, WriteError> {
        if self.token.is_empty()
            || self.token.len() > 8192
            || self.token.chars().any(char::is_control)
        {
            return Err("Enter a valid GitHub token.".into());
        }
        let mut authorization =
            gpui_kit::http_client::http::HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|_| "Invalid token header.")?;
        authorization.set_sensitive(true);
        let body = json
            .map(|json| AsyncBody::from(json.to_string()))
            .unwrap_or_else(AsyncBody::empty);
        let request = Request::builder()
            .method(method)
            .header("Content-Type", "application/json")
            .uri(format!("https://api.github.com{path}"))
            .header("Authorization", authorization)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Canopy-Desktop")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .follow_redirects(RedirectPolicy::NoFollow)
            .timeout(Duration::from_secs(20))
            .body(body)
            .map_err(|_| "Could not prepare GitHub request.")?;
        let response = self
            .client
            .send(request)
            .await
            .map_err(|_| if writing {WriteError::Uncertain("Could not confirm the GitHub change. Check GitHub before retrying; the change may already be saved.".into())} else {WriteError::Rejected("Could not reach GitHub. Check your connection and retry.".into())})?;
        let status = response.status().as_u16();
        if !(status == 200 || writing && matches!(status, 201 | 204)) {
            if matches!(status, 403 | 429)
                && (status == 429
                    || response
                        .headers()
                        .get("x-ratelimit-remaining")
                        .is_some_and(|v| v == "0")
                    || response.headers().contains_key("retry-after"))
            {
                return Err(WriteError::Rejected(
                    "GitHub rate limit reached. Wait before retrying.".into(),
                ));
            }
            if writing && status >= 500 {
                return Err(WriteError::Uncertain(format!(
                    "GitHub returned HTTP {status}. Check GitHub before retrying; the change may already be saved."
                )));
            }
            if writing && status == 403 {
                return Err(WriteError::Rejected("GitHub denied this change. The token needs Issues: read and write, and your account needs permission for this action. Check organization SSO as well.".into()));
            }
            if writing && status == 422 {
                return Err(WriteError::Rejected("GitHub rejected the fields. Check the title, labels, assignees, milestone and repository permissions.".into()));
            }
            return Err(match status{
            401=>"GitHub rejected the token. Reconnect in Preferences → Integrations.".into(),
            403|429 if response.headers().get("x-ratelimit-remaining").is_some_and(|v|v=="0")||status==429||response.headers().contains_key("retry-after")=>"GitHub rate limit reached. Wait before refreshing again.".into(),
            403=>"GitHub access denied. Check repository access, Issues read permission and organization SSO approval.".into(),
            404=>"Repository not found or not accessible with this token.".into(),
            301|302|307|308=>"GitHub redirected this repository. Update the origin or repository override.".into(),
            _=>format!("GitHub request failed (HTTP {status}). Try again later.").into()
        });
        }
        if status == 204 {
            return Ok(serde_json::Value::Null);
        }
        let mut bytes = Vec::new();
        response
            .into_body()
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| if writing {WriteError::Uncertain("The change may be saved, but the response could not be read. Check GitHub before retrying.".into())}else{WriteError::Rejected("Could not read GitHub response.".into())})?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(if writing {
                WriteError::Uncertain("The change may be saved, but GitHub returned too much data. Check GitHub before retrying.".into())
            } else {
                WriteError::Rejected("GitHub response exceeded the 4 MiB limit.".into())
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| if writing {WriteError::Uncertain("The change may be saved, but GitHub returned an invalid response. Check GitHub before retrying.".into())}else{WriteError::Rejected("GitHub returned an invalid response.".into())})
    }
    async fn graphql(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let value = self
            .request(
                "/graphql",
                Some(serde_json::json!({"query":query,"variables":variables})),
            )
            .await?;
        if let Some(errors) = value.get("errors")
            && errors.as_array().is_none_or(|errors| !errors.is_empty())
        {
            let limited = errors.as_array().is_some_and(|errors| {
                errors
                    .iter()
                    .any(|e| e.get("type").and_then(|v| v.as_str()) == Some("RATE_LIMITED"))
            });
            return Err(if limited{"GitHub rate limit reached. Wait before refreshing again."}else{"GitHub could not load task data. Check repository access, Issues read permission and organization SSO approval."}.into());
        }
        Ok(value)
    }
    pub async fn task(&self, reference: &TaskRef) -> Result<TaskItem, String> {
        ProjectTarget::github(&reference.project.key)?;
        let number = reference
            .id
            .parse::<u64>()
            .map_err(|_| "Invalid issue number")?;
        let value = self
            .get(&format!("/repos/{}/issues/{number}", reference.project.key))
            .await?;
        parse_page(serde_json::json!([value]), &reference.project)?
            .into_iter()
            .next()
            .ok_or("This item is not an issue.".into())
    }
    pub async fn verify(&self) -> Result<String, String> {
        let value = self.get("/user").await?;
        value
            .get("login")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .map(ToOwned::to_owned)
            .ok_or("GitHub returned no account identity.".into())
    }
}
impl TaskProvider for Github {
    fn verify(&self) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(Github::verify(self))
    }
    fn task<'a>(
        &'a self,
        task: &'a TaskRef,
    ) -> Pin<Box<dyn Future<Output = Result<TaskItem, String>> + Send + 'a>> {
        Box::pin(Github::task(self, task))
    }

    fn list<'a>(
        &'a self,
        target: &'a ProjectTarget,
        state: TaskState,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPage, String>> + Send + 'a>> {
        Box::pin(issues::list(self, target, state, cursor))
    }
    fn comments<'a>(
        &'a self,
        task: &'a TaskRef,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskCommentPage, String>> + Send + 'a>> {
        Box::pin(comments::list(self, task, cursor))
    }
    fn options<'a>(
        &'a self,
        project: &'a ProjectTarget,
        kind: OptionKind,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskOptions, String>> + Send + 'a>> {
        Box::pin(writes::options(self, project, kind, cursor))
    }
    fn write<'a>(
        &'a self,
        command: &'a TaskWrite,
    ) -> Pin<Box<dyn Future<Output = Result<WriteReceipt, WriteError>> + Send + 'a>> {
        Box::pin(writes::execute(self, command))
    }
}
#[derive(Deserialize)]
struct User {
    login: String,
}
#[derive(Deserialize)]
struct Label {
    name: String,
}
#[derive(Deserialize)]
struct Milestone {
    number: u64,
    title: String,
}
#[derive(Deserialize)]
struct Issue {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default)]
    assignees: Vec<User>,
    pull_request: Option<serde_json::Value>,
    milestone: Option<Milestone>,
}
pub fn parse_page(
    value: serde_json::Value,
    target: &ProjectTarget,
) -> Result<Vec<TaskItem>, String> {
    let rows: Vec<Issue> =
        serde_json::from_value(value).map_err(|_| "Invalid issue data from GitHub.")?;
    let tasks = rows
        .into_iter()
        .filter(|i| i.pull_request.is_none() && i.number > 0)
        .map(|i| task_item(i, target))
        .collect();
    Ok(tasks)
}
fn task_item(i: Issue, target: &ProjectTarget) -> TaskItem {
    TaskItem {
        youtrack: None,
        jira: None,
        milestone: i.milestone.map(|m| (m.number, m.title)),
        reference: TaskRef {
            project: target.clone(),
            id: i.number.to_string(),
            title: i.title.chars().take(4096).collect(),
        },
        body: i
            .body
            .unwrap_or_default()
            .chars()
            .take(65536)
            .collect::<String>()
            .into(),
        state: if i.state.eq_ignore_ascii_case("closed") {
            TaskState::Closed
        } else {
            TaskState::Open
        },
        labels: i
            .labels
            .into_iter()
            .map(|l| l.name.chars().take(100).collect())
            .collect(),
        assignees: i
            .assignees
            .into_iter()
            .take(16)
            .map(|u| u.login.chars().take(100).collect())
            .collect(),
        url: format!("https://github.com/{}/issues/{}", target.key, i.number),
    }
}
