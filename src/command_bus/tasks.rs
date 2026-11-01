//! Integration intentions retain provider keys and typed operations, never credentials.
use super::{CommandMetadata, Risk};
use crate::{
    host, integrations,
    settings::{Error, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
mod local;

/// Host-owned routing and immutable credential reference retained before admission.
/// The client intention, and therefore its fingerprint, stays independent of it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccountBinding {
    version: u8,
    intent: String,
    pub(crate) account: integrations::Account,
}

impl AccountBinding {
    pub(crate) fn bind(
        input: &mut crate::host::CommandInput,
        account: integrations::Account,
    ) -> Result<()> {
        let value = Self {
            version: 1,
            intent: String::from_utf8(input.arguments.clone())
                .map_err(|_| Error::InvalidRuntimeState)?,
            account,
        };
        value.validate()?;
        input.arguments = serde_json::to_vec(&value).map_err(|_| Error::InvalidRuntimeState)?;
        if input.arguments.len() > host::CONTROL_BYTES {
            return Err(Error::ResourceLimit);
        }
        Ok(())
    }

    pub(crate) fn read(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > host::CONTROL_BYTES {
            return Err(Error::NotAuthorized);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| Error::NotAuthorized)?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<()> {
        let (_, _, _, command, _): (String, String, u64, super::Command, super::Conditions) =
            serde_json::from_str(&self.intent).map_err(|_| Error::NotAuthorized)?;
        let integration = match command {
            super::Command::Task(
                TaskCommand::Mutate { integration, .. }
                | TaskCommand::Update { integration, .. }
                | TaskCommand::Worktree { integration, .. },
            ) => integration,
            _ => return Err(Error::NotAuthorized),
        };
        if self.version != 1 || self.account.id != integration || self.account.credential.is_empty()
        {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }

    pub(crate) fn intent(&self) -> &[u8] {
        self.intent.as_bytes()
    }
}

pub(crate) fn requires_account(method: &str) -> bool {
    matches!(method, "tasks.mutate" | "tasks.update" | "tasks.worktree")
}

pub(crate) fn same_arguments(saved: &[u8], input: &crate::host::CommandInput) -> bool {
    let Ok(saved) = AccountBinding::read(saved) else {
        return false;
    };
    let current = AccountBinding::read(&input.arguments).ok();
    saved.intent()
        == current
            .as_ref()
            .map_or(input.arguments.as_slice(), AccountBinding::intent)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskCommand {
    Mutate {
        integration: String,
        project: String,
        task: Option<String>,
        action: TaskAction,
    },
    Update {
        integration: String,
        project: String,
        task: String,
        changes: TaskChanges,
    },
    SaveFilter(integrations::filters::TaskFilter),
    RemoveFilter(String),
    Configure(TaskConfiguration),
    Worktree {
        integration: String,
        project: String,
        task: String,
        branch: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskConfiguration {
    SelectFilter(integrations::filters::FilterSelection),
    Repository(Option<integrations::ProjectTarget>),
    Links(Vec<integrations::TaskRef>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskAction {
    Create(TaskDraft),
    Field(TaskField),
    Fields(Value),
    Status(TaskStatus),
    Comment(TaskComment),
    Tag(TaskMembership),
    Assignee(TaskMembership),
    Milestone(Option<String>),
    Attachment(TaskAttachment),
    Link(TaskLink),
    Worklog(TaskWorklog),
    Watching(bool),
    Voted(bool),
    Delete(TaskDelete),
    Sprint(Option<String>),
}
impl TaskAction {
    pub(crate) fn requires_fresh_admission(&self) -> bool {
        matches!(self, Self::Delete(_))
            || matches!(self, Self::Comment(comment) if comment.delete)
            || matches!(self, Self::Attachment(attachment) if matches!(attachment.action, TaskAttachmentAction::DeleteId(_)))
            || matches!(self, Self::Link(link) if link.delete_link_id.is_some())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDraft {
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone_id: Option<String>,
    pub fields: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskField {
    pub field_id: String,
    pub value: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskStatus {
    pub field_id: String,
    pub choice_id: String,
    pub form_fields: Value,
    pub comment: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskComment {
    pub comment_id: Option<String>,
    pub body: String,
    pub rich_body: Option<Value>,
    pub delete: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskMembership {
    pub id: String,
    pub add: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskAttachment {
    pub name: String,
    pub action: TaskAttachmentAction,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskAttachmentAction {
    Attach(super::FileContent),
    DeleteId(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskLink {
    pub kind: String,
    pub other_task: String,
    pub outward: bool,
    pub delete_link_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskWorklog {
    pub seconds: u64,
    pub started: String,
    pub comment: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDelete {
    pub confirmation: String,
    pub subtasks: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskChanges {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub labels: Option<Vec<String>>,
    pub assignees: Option<Vec<String>>,
    pub milestone: Option<Option<String>>,
}

pub(super) static REGISTRY: &[CommandMetadata] = &[
    CommandMetadata::new("tasks.mutate", Risk::Delivery, host::TASKS),
    CommandMetadata::new("tasks.update", Risk::Delivery, host::TASKS),
    CommandMetadata::new("tasks.save_filter", Risk::Configuration, host::TASKS),
    CommandMetadata::new("tasks.remove_filter", Risk::Configuration, host::TASKS),
    CommandMetadata::new(
        "tasks.worktree",
        Risk::Delivery,
        host::TASKS | host::FILES_GIT,
    ),
    CommandMetadata::new("tasks.configure", Risk::Configuration, host::TASKS),
];
impl TaskCommand {
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[match self {
            Self::Mutate { .. } => 0,
            Self::Update { .. } => 1,
            Self::SaveFilter(_) => 2,
            Self::RemoveFilter(_) => 3,
            Self::Worktree { .. } => 4,
            Self::Configure(_) => 5,
        }]
    }
    pub(super) fn normalize(&mut self) -> Result<()> {
        match self {
            Self::SaveFilter(filter) => {
                filter.validate().map_err(|_| Error::InvalidRuntimeState)?
            }
            Self::RemoveFilter(id) if uuid::Uuid::parse_str(id).is_err() => {
                return Err(Error::InvalidRuntimeState);
            }
            Self::Configure(value) => match value {
                TaskConfiguration::SelectFilter(value)
                    if !value.project.valid()
                        || value.filter_id.is_empty()
                        || value.filter_id.len() > 256 =>
                {
                    return Err(Error::InvalidRuntimeState);
                }
                TaskConfiguration::Repository(Some(value)) if !value.valid() => {
                    return Err(Error::InvalidRuntimeState);
                }
                TaskConfiguration::Links(values)
                    if values.len() > 32 || values.iter().any(|v| !v.valid()) =>
                {
                    return Err(Error::InvalidRuntimeState);
                }
                _ => (),
            },
            Self::Mutate {
                integration,
                project,
                task,
                ..
            } => validate_keys(integration, project, task.as_deref())?,
            Self::Update {
                integration,
                project,
                task,
                ..
            }
            | Self::Worktree {
                integration,
                project,
                task,
                ..
            } => validate_keys(integration, project, Some(task))?,
            _ => (),
        }
        if serde_json::to_vec(self)
            .map_err(|_| Error::InvalidRuntimeState)?
            .len()
            > 1024 * 1024
        {
            return Err(Error::ResourceLimit);
        }
        Ok(())
    }
}
fn validate_keys(integration: &str, project: &str, task: Option<&str>) -> Result<()> {
    if integration.is_empty()
        || integration.len() > 256
        || project.is_empty()
        || project.len() > 4096
        || task.is_some_and(|t| t.is_empty() || t.len() > 4096)
    {
        return Err(Error::InvalidRuntimeState);
    }
    Ok(())
}
