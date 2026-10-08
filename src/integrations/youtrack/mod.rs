//! YouTrack REST integration.
//!
//! YouTrack identifies projects by a stable internal id but exposes a readable
//! `idReadable` issue key.  The adapter keeps both identities separate and
//! never treats YouTrack custom fields as Jira fields.
mod read;
mod schema;
mod status;
mod transport;
mod writes;

use super::*;
use gpui_kit::http_client::{HttpClient, Url};
use serde_json::Value;
use std::sync::Arc;

pub use schema::{YoutrackField, YoutrackFieldKind, YoutrackSchema, value_for_field};

#[derive(Clone, Debug)]
pub struct YoutrackAttachment {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: usize,
    pub url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct YoutrackTag {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct YoutrackDetails {
    pub complete: bool,
    pub internal_id: String,
    pub project_id: String,
    pub project_short_name: String,
    pub status: String,
    pub resolved: bool,
    pub issue_type: String,
    pub priority: String,
    pub reporter: String,
    pub fields: Value,
    pub custom_fields: Vec<YoutrackField>,
    pub tags: Vec<YoutrackTag>,
    pub attachments: Vec<YoutrackAttachment>,
}

#[derive(Clone, Debug)]
pub enum YoutrackWrite {
    /// Create using the shared issue draft and provider-native custom-field values.
    Create {
        draft: IssueDraft,
        fields: Value,
    },
    /// Update summary/description and/or custom fields. The value is sent as
    /// the exact YouTrack request object after validation.
    Fields {
        fields: Value,
    },
    /// Update one custom field at its provider-native endpoint.
    CustomField {
        id: String,
        field_type: String,
        value: Value,
    },
    /// Revalidate one status field and its choices immediately before writing.
    Status {
        id: String,
        choice: String,
    },
    /// Trigger a StateMachineIssueCustomField event.
    StateMachineEvent {
        id: String,
        event_id: String,
    },
    DeleteIssue {
        confirmation: String,
    },
    Tag {
        id: String,
        add: bool,
    },
    Attach {
        name: String,
        mime_type: String,
        bytes: Arc<Vec<u8>>,
    },
    DeleteAttachment {
        id: String,
    },
}

/// Normalize an explicitly configured YouTrack service URL while preserving a
/// deployment context path such as `/youtrack` and an explicit HTTPS port.
pub fn normalize_service(service: &str) -> Result<String, String> {
    let raw = service.trim();
    if raw.len() > 512 {
        return Err("Enter a YouTrack HTTPS service URL up to 512 characters.".into());
    }
    let raw_path = raw.split(['?', '#']).next().unwrap_or(raw);
    if raw_path
        .split('/')
        .any(|segment| matches!(segment, "." | ".."))
    {
        return Err("Use an HTTPS YouTrack service URL without traversal segments.".into());
    }
    let url = Url::parse(raw).map_err(|_| {
        "Enter the HTTPS address of your YouTrack service, for example https://issues.example.com/youtrack."
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url
            .path()
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
        || url.path().bytes().any(|b| b.is_ascii_control())
    {
        return Err(
            "Use an HTTPS YouTrack service URL without credentials, query or fragment.".into(),
        );
    }
    let host = url.host_str().ok_or("YouTrack service URL has no host.")?;
    let mut normalized = format!("https://{host}");
    if let Some(port) = url.port() {
        normalized.push(':');
        normalized.push_str(&port.to_string());
    }
    let path = url.path().trim_end_matches('/');
    if !path.is_empty() {
        normalized.push_str(path);
    }
    Ok(normalized)
}

pub fn validate_connection(service: &str) -> Result<(), String> {
    if normalize_service(service)?.as_str() != service {
        return Err("Use the normalized YouTrack service URL.".into());
    }
    Ok(())
}

pub fn valid_issue_key(key: &str) -> bool {
    let Some((project, number)) = key.rsplit_once('-') else {
        return false;
    };
    !project.is_empty()
        && project.len() <= 100
        && project
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        && number.parse::<u64>().is_ok_and(|n| n > 0)
}

pub fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || value == "."
        || value == ".."
        || value
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')))
    {
        return Err(format!("Invalid YouTrack {label}."));
    }
    Ok(())
}

pub fn query(path: &str, values: &[(&str, &str)]) -> String {
    let mut url = Url::parse(&format!("https://unused.invalid{path}")).unwrap();
    url.query_pairs_mut().extend_pairs(values.iter().copied());
    format!("{}?{}", url.path(), url.query().unwrap_or_default())
}

pub struct Youtrack {
    pub(super) http: Arc<dyn HttpClient>,
    pub(super) service: String,
    pub(super) api: String,
    pub(super) token: String,
    pub(super) user: String,
}

impl Youtrack {
    pub fn new(
        http: Arc<dyn HttpClient>,
        service: &str,
        token: String,
        user: String,
    ) -> Result<Self, String> {
        let service = normalize_service(service)?;
        if token.is_empty() || token.len() > 8192 || token.chars().any(char::is_control) {
            return Err("Enter a valid YouTrack permanent token.".into());
        }
        let api = format!("{service}/api");
        Ok(Self {
            http,
            service,
            api,
            token,
            user,
        })
    }

    pub(super) fn target(&self, target: &ProjectTarget) -> Result<(), String> {
        if target.provider != Provider::Youtrack
            || !target.valid()
            || target.site.as_deref() != Some(self.service.as_str())
        {
            Err("The YouTrack connection does not belong to this service.".into())
        } else {
            Ok(())
        }
    }

    pub async fn projects(&self, cursor: Option<&str>) -> Result<TaskOptions, String> {
        read::projects(self, cursor).await
    }

    pub async fn schema(&self, target: &ProjectTarget) -> Result<YoutrackSchema, String> {
        schema::load(self, target).await
    }

    pub async fn download_attachment(&self, task: &TaskRef, id: &str) -> Result<Vec<u8>, String> {
        transport::download_attachment(self, task, id).await
    }
}

impl TaskProvider for Youtrack {
    fn list_context<'a>(
        &'a self,
        target: &'a ProjectTarget,
        state: TaskState,
        cursor: Option<&'a str>,
        query: &'a TaskQuery,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<TaskPage, String>> + Send + 'a>> {
        Box::pin(read::list_context(self, target, state, cursor, query))
    }

    fn verify(
        &self,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(async move {
            let value = self.get("/users/me?fields=id,login,name,fullName").await?;
            value["login"]
                .as_str()
                .or_else(|| value["name"].as_str())
                .or_else(|| value["fullName"].as_str())
                .filter(|v| !v.is_empty() && v.len() <= 128)
                .map(str::to_owned)
                .ok_or("YouTrack returned no user identity.".into())
        })
    }

    fn task<'a>(
        &'a self,
        task: &'a TaskRef,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<TaskItem, String>> + Send + 'a>> {
        Box::pin(read::task(self, task))
    }

    fn list<'a>(
        &'a self,
        target: &'a ProjectTarget,
        state: TaskState,
        cursor: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<TaskPage, String>> + Send + 'a>> {
        Box::pin(read::list(self, target, state, cursor))
    }

    fn comments<'a>(
        &'a self,
        task: &'a TaskRef,
        cursor: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<TaskCommentPage, String>> + Send + 'a>> {
        Box::pin(read::comments(self, task, cursor))
    }

    fn options<'a>(
        &'a self,
        target: &'a ProjectTarget,
        kind: OptionKind,
        cursor: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<TaskOptions, String>> + Send + 'a>> {
        Box::pin(read::options(self, target, kind, cursor))
    }

    fn projects<'a>(
        &'a self,
        cursor: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<TaskOptions, String>> + Send + 'a>> {
        Box::pin(read::projects(self, cursor))
    }

    fn youtrack_schema<'a>(
        &'a self,
        target: &'a ProjectTarget,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<YoutrackSchema, String>> + Send + 'a>> {
        Box::pin(schema::load(self, target))
    }

    fn youtrack_status_fields<'a>(
        &'a self,
        task: &'a TaskRef,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<YoutrackField>, String>> + Send + 'a>>
    {
        Box::pin(status::load(self, task))
    }

    fn write<'a>(
        &'a self,
        command: &'a TaskWrite,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<WriteReceipt, WriteError>> + Send + 'a>> {
        Box::pin(writes::execute(self, command))
    }
}
