//! Provider-neutral Tasks contracts; account credentials never enter this model.
pub mod attachments;
pub mod client;
pub mod credentials;
pub mod drafts;
pub mod edit;
pub mod filters;
pub mod github;
pub mod jira;
pub mod status;
pub mod task_context;
pub mod youtrack;
pub use client::client;
pub use edit::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, future::Future, path::PathBuf, pin::Pin};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Github,
    Jira,
    Youtrack,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub provider: Provider,
    pub login: String,
    pub credential: String,
    /// Routing scope, not a claim about the permissions granted by GitHub.
    #[serde(default)]
    pub scope: AccountScope,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountScope {
    #[default]
    Default,
    Owner(String),
    Jira {
        site: String,
        email: String,
        #[serde(default)]
        cloud_id: Option<String>,
    },
    Youtrack {
        service: String,
    },
}
impl AccountScope {
    pub fn owner(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty()
            || value.len() > 100
            || value.starts_with('-')
            || value.ends_with('-')
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err("Enter the GitHub organization or username, without a URL.".into());
        }
        Ok(Self::Owner(value.to_ascii_lowercase()))
    }
    pub fn label(&self) -> String {
        match self {
            Self::Default => "Default · all accessible organizations".into(),
            Self::Owner(owner) => owner.clone(),
            Self::Jira { site, .. } => site.clone(),
            Self::Youtrack { service } => service.clone(),
        }
    }
    fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Default, Self::Default) => true,
            (Self::Owner(a), Self::Owner(b)) => a.eq_ignore_ascii_case(b),
            (Self::Jira { site: a, .. }, Self::Jira { site: b, .. }) => a == b,
            (Self::Youtrack { service: a }, Self::Youtrack { service: b }) => a == b,
            _ => false,
        }
    }
    /// Only permissions used by the current Tasks reader. Never contains a token.
    pub fn creation_url(&self) -> String {
        use gpui_kit::http_client::Url;
        match self {
            Self::Default => {
                let mut url = Url::parse("https://github.com/settings/tokens/new").unwrap();
                url.query_pairs_mut()
                    .append_pair("description", "Canopy Tasks")
                    .append_pair("scopes", "repo");
                url.into()
            }
            Self::Jira { .. } => {
                "https://id.atlassian.com/manage-profile/security/api-tokens".into()
            }
            Self::Youtrack { .. } => {
                "https://www.jetbrains.com/help/youtrack/standalone/Manage-Permanent-Token.html"
                    .into()
            }
            Self::Owner(owner) => {
                let mut url =
                    Url::parse("https://github.com/settings/personal-access-tokens/new").unwrap();
                url.query_pairs_mut()
                    .append_pair("name", "Canopy Tasks")
                    .append_pair("description", "Read and update GitHub Issues in Canopy")
                    .append_pair("target_name", owner)
                    .append_pair("issues", "write")
                    .append_pair("metadata", "read");
                url.into()
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTarget {
    pub provider: Provider,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
}
impl ProjectTarget {
    pub fn valid(&self) -> bool {
        match self.provider {
            Provider::Github => self.site.is_none() && Self::github(&self.key).is_ok(),
            Provider::Jira => self
                .site
                .as_ref()
                .is_some_and(|site| Self::jira(site, &self.key).is_ok_and(|t| t == *self)),
            Provider::Youtrack => self.site.as_ref().is_some_and(|service| {
                Self::youtrack(service, &self.key).is_ok_and(|t| t == *self)
            }),
        }
    }
    pub fn jira(site: &str, key: &str) -> Result<Self, String> {
        let site = jira::normalize_site(site)?;
        let key = key.trim().to_ascii_uppercase();
        if key.is_empty()
            || key.len() > 100
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !key.as_bytes()[0].is_ascii_alphabetic()
        {
            return Err("Enter a Jira project key, for example CAN.".into());
        }
        Ok(Self {
            provider: Provider::Jira,
            site: Some(site),
            key,
        })
    }
    pub fn youtrack(service: &str, key: &str) -> Result<Self, String> {
        let service = youtrack::normalize_service(service)?;
        // YouTrack's readable issue ids use the canonical project short name
        // prefix. Uppercasing keeps manually entered `can` and API `CAN-42`
        // references equivalent while preserving the service identity.
        let key = key.trim().to_ascii_uppercase();
        if key.is_empty()
            || key.len() > 100
            || !key
                .bytes()
                .enumerate()
                .all(|(i, c)| c.is_ascii_alphanumeric() || c == b'_' || (c == b'-' && i > 0))
            || !key.as_bytes()[0].is_ascii_alphanumeric()
        {
            return Err("Enter a YouTrack project short name, for example CAN.".into());
        }
        Ok(Self {
            provider: Provider::Youtrack,
            site: Some(service),
            key,
        })
    }
    pub fn github(key: &str) -> Result<Self, String> {
        let parts: Vec<_> = key.trim().split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|p| {
                p.is_empty()
                    || p.len() > 100
                    || matches!(*p, "." | "..")
                    || !p
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            })
        {
            return Err("Use owner/repository, for example octocat/Hello-World.".into());
        }
        Ok(Self {
            provider: Provider::Github,
            key: parts.join("/"),
            site: None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRef {
    pub project: ProjectTarget,
    pub id: String,
    pub title: String,
}
impl TaskRef {
    pub fn valid(&self) -> bool {
        self.project.valid()
            && self.title.len() <= 4096
            && match self.project.provider {
                Provider::Github => self.id.parse::<u64>().is_ok_and(|n| n > 0),
                Provider::Jira => {
                    jira::valid_issue_key(&self.id)
                        && self.id.starts_with(&format!("{}-", self.project.key))
                }
                Provider::Youtrack => {
                    youtrack::valid_issue_key(&self.id)
                        && self.id.starts_with(&format!("{}-", self.project.key))
                }
            }
    }
    pub fn label(&self) -> String {
        match self.project.provider {
            Provider::Github => format!("#{}", self.id),
            Provider::Jira => self.id.clone(),
            Provider::Youtrack => self.id.clone(),
        }
    }
    pub fn url(&self) -> String {
        match self.project.provider {
            Provider::Github => {
                format!("https://github.com/{}/issues/{}", self.project.key, self.id)
            }
            Provider::Jira => format!(
                "{}/browse/{}",
                self.project.site.as_deref().unwrap_or(""),
                self.id
            ),
            Provider::Youtrack => format!(
                "{}/issue/{}",
                self.project.site.as_deref().unwrap_or(""),
                self.id
            ),
        }
    }
    pub fn same_task(&self, other: &Self) -> bool {
        self.project == other.project && self.id == other.id
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub task_filters: Vec<filters::TaskFilter>,
    #[serde(default)]
    pub filter_selections: Vec<filters::FilterSelection>,
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub retired_credentials: Vec<String>,
    pub overrides: BTreeMap<PathBuf, ProjectTarget>,
    pub links: BTreeMap<PathBuf, Vec<TaskRef>>,
}
impl Config {
    pub fn valid(&self) -> bool {
        self.filters_valid()
            && self.accounts.len() <= 16
            && self.accounts.iter().enumerate().all(|(i, a)| {
                self.accounts[..i].iter().all(|b| {
                    a.id != b.id
                        && a.credential != b.credential
                        && (a.provider != b.provider || !a.scope.overlaps(&b.scope))
                })
            })
            && self.retired_credentials.len() <= 32
            && self.retired_credentials.iter().all(|key| {
                uuid::Uuid::parse_str(key).is_ok()
                    && !self.accounts.iter().any(|a| &a.credential == key)
            })
            && self.overrides.len() <= 256
            && self.links.len() <= 4096
            && self.accounts.iter().all(|a| {
                uuid::Uuid::parse_str(&a.id).is_ok()
                    && uuid::Uuid::parse_str(&a.credential).is_ok()
                    && !a.login.is_empty()
                    && a.login.len() <= 128
                    && match &a.scope {
                        AccountScope::Default => a.provider == Provider::Github,
                        AccountScope::Jira {
                            site,
                            email,
                            cloud_id,
                        } => {
                            a.provider == Provider::Jira
                                && jira::validate_connection(site, email, cloud_id.as_deref())
                                    .is_ok()
                        }
                        AccountScope::Youtrack { service } => {
                            a.provider == Provider::Youtrack
                                && youtrack::validate_connection(service).is_ok()
                        }
                        AccountScope::Owner(owner) => {
                            a.provider == Provider::Github
                                && AccountScope::owner(owner)
                                    .is_ok_and(|scope| scope.overlaps(&a.scope))
                        }
                    }
            })
            && self
                .overrides
                .iter()
                .all(|(p, t)| p.is_absolute() && t.valid())
            && self.links.iter().all(|(p, links)| {
                p.is_absolute() && links.len() <= 32 && links.iter().all(|t| t.valid())
            })
    }
    pub fn task_project(&self, repo: &RepositoryContext) -> Option<ProjectTarget> {
        self.overrides
            .get(&repo.common)
            .cloned()
            .or(repo.inferred.clone())
    }
    pub fn github_account(&self) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.provider == Provider::Github)
    }
    pub fn account_for(&self, target: &ProjectTarget) -> Option<&Account> {
        if target.provider == Provider::Jira {
            return self.accounts.iter().find(|a| a.provider == Provider::Jira && matches!(&a.scope, AccountScope::Jira {site,..} if Some(site) == target.site.as_ref()));
        }
        if target.provider == Provider::Youtrack {
            return self.accounts.iter().find(|a| {
                a.provider == Provider::Youtrack
                    && matches!(&a.scope, AccountScope::Youtrack { service } if Some(service) == target.site.as_ref())
            });
        }
        let (owner, _) = target.key.split_once('/')?;
        let accounts = || {
            self.accounts
                .iter()
                .filter(|a| a.provider == target.provider)
        };
        accounts().find(|a| matches!(&a.scope, AccountScope::Owner(name) if name.eq_ignore_ascii_case(owner)))
            .or_else(|| accounts().find(|a| a.scope == AccountScope::Default))
    }
    /// Replacement preserves the connection ID and retires only its old secret.
    pub fn with_account(&self, account: Account) -> Result<Self, String> {
        if self.accounts.iter().any(|a| {
            a.id != account.id && a.provider == account.provider && a.scope.overlaps(&account.scope)
        }) {
            return Err("A connection already exists for this scope. Edit that connection to replace its token.".into());
        }
        let mut next = self.clone();
        if let Some(old) = next.accounts.iter_mut().find(|a| a.id == account.id) {
            if old.credential != account.credential {
                next.retired_credentials.push(old.credential.clone());
            }
            *old = account;
        } else {
            next.accounts.push(account);
        }
        if !next.valid() {
            return Err("Cannot save this connection. Up to 16 connections are supported; retry pending credential cleanup if needed.".into());
        }
        Ok(next)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Open,
    Closed,
}
#[derive(Clone, Debug)]
pub struct TaskItem {
    pub reference: TaskRef,
    pub body: std::sync::Arc<str>,
    pub state: TaskState,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub url: String,
    pub milestone: Option<(u64, String)>,
    pub jira: Option<std::sync::Arc<jira::JiraDetails>>,
    pub youtrack: Option<std::sync::Arc<youtrack::YoutrackDetails>>,
}
#[derive(Clone, Debug)]
pub struct TaskPage {
    pub tasks: Vec<TaskItem>,
    pub next_cursor: Option<String>,
    pub total_count: Option<usize>,
}
#[derive(Clone, Debug)]
pub struct TaskComment {
    pub id: String,
    pub rich_body: Option<serde_json::Value>,
    pub can_edit: Option<bool>,
    pub can_delete: Option<bool>,
    pub author: String,
    pub created_at: String,
    pub body: std::sync::Arc<str>,
    pub url: String,
}
#[derive(Clone, Debug)]
pub struct TaskCommentPage {
    pub comments: Vec<TaskComment>,
    pub next_cursor: Option<String>,
    pub total_count: usize,
}
#[derive(Clone, Default, Debug)]
pub struct TaskQuery {
    pub text: String,
    pub filter: Option<String>,
    pub reconcile: Vec<u64>,
}
pub trait TaskProvider: Send + Sync {
    fn list_context<'a>(
        &'a self,
        target: &'a ProjectTarget,
        state: TaskState,
        cursor: Option<&'a str>,
        query: &'a TaskQuery,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPage, String>> + Send + 'a>> {
        let _ = query;
        self.list(target, state, cursor)
    }
    fn verify(&self) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>>;
    fn task<'a>(
        &'a self,
        task: &'a TaskRef,
    ) -> Pin<Box<dyn Future<Output = Result<TaskItem, String>> + Send + 'a>>;
    fn schema<'a>(
        &'a self,
        target: &'a ProjectTarget,
        request: &'a jira::SchemaRequest,
    ) -> Pin<Box<dyn Future<Output = Result<jira::JiraSchema, String>> + Send + 'a>> {
        let _ = (target, request);
        Box::pin(async { Err("This provider does not use Jira field schemas.".into()) })
    }
    fn youtrack_schema<'a>(
        &'a self,
        target: &'a ProjectTarget,
    ) -> Pin<Box<dyn Future<Output = Result<youtrack::YoutrackSchema, String>> + Send + 'a>> {
        let _ = target;
        Box::pin(async { Err("This provider does not use YouTrack field schemas.".into()) })
    }
    fn youtrack_status_fields<'a>(
        &'a self,
        task: &'a TaskRef,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<youtrack::YoutrackField>, String>> + Send + 'a>>
    {
        let _ = task;
        Box::pin(async { Err("This provider does not use YouTrack status fields.".into()) })
    }
    fn projects<'a>(
        &'a self,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskOptions, String>> + Send + 'a>> {
        let _ = cursor;
        Box::pin(async { Err("This provider does not provide project browsing.".into()) })
    }

    fn list<'a>(
        &'a self,
        target: &'a ProjectTarget,
        state: TaskState,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPage, String>> + Send + 'a>>;
    fn comments<'a>(
        &'a self,
        task: &'a TaskRef,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskCommentPage, String>> + Send + 'a>>;
    fn options<'a>(
        &'a self,
        project: &'a ProjectTarget,
        kind: OptionKind,
        cursor: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<TaskOptions, String>> + Send + 'a>>;
    fn write<'a>(
        &'a self,
        command: &'a TaskWrite,
    ) -> Pin<Box<dyn Future<Output = Result<WriteReceipt, WriteError>> + Send + 'a>>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryContext {
    pub common: PathBuf,
    pub repository: PathBuf,
    pub inferred: Option<ProjectTarget>,
}
pub fn from_origin(origin: &str) -> Option<ProjectTarget> {
    let raw = if let Some(path) = origin.strip_prefix("git@github.com:") {
        path.to_owned()
    } else {
        let url = gpui_kit::http_client::Url::parse(origin).ok()?;
        if !matches!(url.host_str(), Some("github.com" | "ssh.github.com"))
            || !matches!(url.scheme(), "https" | "ssh")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        url.path().trim_start_matches('/').to_owned()
    };
    ProjectTarget::github(
        raw.trim_end_matches('/')
            .strip_suffix(".git")
            .unwrap_or(raw.trim_end_matches('/')),
    )
    .ok()
}
