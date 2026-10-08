//! Jira Cloud REST v3. Site identity, accountId and issue keys are never interchangeable.
pub mod adf;
mod read;
mod schema;
mod transport;
mod writes;
use super::*;
use gpui_kit::http_client::{HttpClient, Url};
pub use schema::*;
use serde_json::Value;
use std::sync::Arc;
pub struct Jira {
    http: Arc<dyn HttpClient>,
    site: String,
    api: String,
    email: String,
    token: String,
    user: String,
}
#[derive(Clone, Debug)]
pub struct JiraDetails {
    pub complete: bool,
    pub internal_id: Option<u64>,
    pub status: String,
    pub category: String,
    pub issue_type: String,
    pub fields: Value,
    pub names: Value,
    pub description: Value,
    pub description_editable: bool,
}
#[derive(Clone, Debug)]
pub enum JiraWrite {
    DeleteIssue {
        confirmation: String,
        subtasks: bool,
    },
    Sprint {
        id: Option<String>,
    },
    CommentDocument {
        comment: TaskComment,
        document: Value,
    },
    Create {
        fields: Value,
    },
    Fields {
        fields: Value,
    },
    /// Status-selector action: revalidate the workflow, never submit field defaults.
    QuickTransition {
        id: String,
    },
    Transition {
        id: String,
        fields: Value,
        comment: String,
    },
    Attach {
        name: String,
        bytes: Arc<Vec<u8>>,
    },
    DeleteAttachment {
        id: String,
    },
    Link {
        kind: String,
        other: String,
        outward: bool,
    },
    DeleteLink {
        id: String,
    },
    LogWork {
        seconds: u64,
        started: String,
        comment: String,
    },
    Watch {
        watching: bool,
    },
    Vote {
        voted: bool,
    },
}
pub fn normalize_site(site: &str) -> Result<String, String> {
    let url = Url::parse(site.trim())
        .map_err(|_| "Enter the Jira site URL, for example https://team.atlassian.net.")?;
    if site.len() > 253
        || url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err("Use the HTTPS root address of your Jira Cloud site, without a path, port or credentials.".into());
    }
    // Only a site the user explicitly configures receives credentials; no repo-owned URL.
    Ok(format!("https://{}", url.host_str().unwrap()))
}
pub fn validate_connection(site: &str, email: &str, cloud_id: Option<&str>) -> Result<(), String> {
    if normalize_site(site)? != site {
        return Err("Use the normalized Jira site address.".into());
    }
    if !email.contains('@')
        || email.len() > 254
        || email
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ':')
    {
        return Err("Enter your Atlassian account email.".into());
    }
    if cloud_id.is_some_and(|id| uuid::Uuid::parse_str(id).is_err()) {
        return Err("Cloud ID must be a UUID for a token with scopes.".into());
    }
    Ok(())
}
pub fn valid_issue_key(key: &str) -> bool {
    key.split_once('-').is_some_and(|(p, n)| {
        !p.is_empty()
            && p.len() <= 100
            && p.bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            && n.parse::<u64>().is_ok_and(|n| n > 0)
    })
}
fn ident(id: &str) -> Result<&str, String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':'))
    {
        Err("Invalid Jira identifier.".into())
    } else {
        Ok(id)
    }
}
fn issue_path(task: &TaskRef) -> Result<String, String> {
    if !task.valid() || task.project.provider != Provider::Jira {
        return Err("Invalid Jira issue reference.".into());
    }
    Ok(format!("/rest/api/3/issue/{}", task.id))
}
pub fn query(path: &str, values: &[(&str, &str)]) -> String {
    let mut u = Url::parse(&format!("https://unused.invalid{path}")).unwrap();
    u.query_pairs_mut().extend_pairs(values.iter().copied());
    format!("{}?{}", u.path(), u.query().unwrap_or(""))
}
impl Jira {
    pub fn new(
        http: Arc<dyn HttpClient>,
        site: &str,
        email: &str,
        cloud_id: Option<&str>,
        token: String,
        user: String,
    ) -> Result<Self, String> {
        validate_connection(site, email, cloud_id)?;
        if token.is_empty() || token.len() > 8192 || token.chars().any(char::is_control) {
            return Err("Enter a valid Atlassian API token.".into());
        }
        Ok(Self {
            http,
            site: site.into(),
            api: cloud_id
                .map(|id| format!("https://api.atlassian.com/ex/jira/{id}"))
                .unwrap_or_else(|| site.into()),
            email: email.into(),
            token,
            user,
        })
    }
    fn target(&self, target: &ProjectTarget) -> Result<(), String> {
        if target.provider != Provider::Jira
            || !target.valid()
            || target.site.as_ref() != Some(&self.site)
        {
            Err("The Jira connection does not belong to this site.".into())
        } else {
            Ok(())
        }
    }
    pub async fn projects(&self, cursor: Option<&str>) -> Result<TaskOptions, String> {
        read::projects(self, cursor).await
    }
}
impl TaskProvider for Jira {
    fn list_context<'a>(
        &'a self,
        target: &'a ProjectTarget,
        state: TaskState,
        cursor: Option<&'a str>,
        query: &'a TaskQuery,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPage, String>> + Send + 'a>> {
        Box::pin(read::list_context(self, target, state, cursor, query))
    }
    fn verify(&self) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(async {
            if self.api != self.site {
                let info = self.get("/rest/api/3/serverInfo").await?;
                let site = info["baseUrl"]
                    .as_str()
                    .ok_or("Jira returned no site identity for this Cloud ID.")?;
                if normalize_site(site)? != self.site {
                    return Err("This Cloud ID belongs to another Jira site. Check the site address and Cloud ID.".into());
                }
            }
            let v = self.get("/rest/api/3/myself").await?;
            v["accountId"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .map(str::to_owned)
                .ok_or("Jira returned no account identity.".into())
        })
    }
    fn task<'a>(
        &'a self,
        task: &'a TaskRef,
    ) -> Pin<Box<dyn Future<Output = Result<TaskItem, String>> + Send + 'a>> {
        Box::pin(read::task(self, task))
    }
    fn list<'a>(
        &'a self,
        target: &'a ProjectTarget,
        state: TaskState,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPage, String>> + Send + 'a>> {
        Box::pin(read::list(self, target, state, cursor))
    }
    fn comments<'a>(
        &'a self,
        task: &'a TaskRef,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskCommentPage, String>> + Send + 'a>> {
        Box::pin(read::comments(self, task, cursor))
    }
    fn options<'a>(
        &'a self,
        target: &'a ProjectTarget,
        kind: OptionKind,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskOptions, String>> + Send + 'a>> {
        Box::pin(read::options(self, target, kind, cursor))
    }
    fn projects<'a>(
        &'a self,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskOptions, String>> + Send + 'a>> {
        Box::pin(read::projects(self, cursor))
    }
    fn schema<'a>(
        &'a self,
        target: &'a ProjectTarget,
        request: &'a SchemaRequest,
    ) -> Pin<Box<dyn Future<Output = Result<JiraSchema, String>> + Send + 'a>> {
        Box::pin(schema::load(self, target, request))
    }
    fn write<'a>(
        &'a self,
        command: &'a TaskWrite,
    ) -> Pin<Box<dyn Future<Output = Result<WriteReceipt, WriteError>> + Send + 'a>> {
        Box::pin(writes::execute(self, command))
    }
}
