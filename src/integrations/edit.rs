//! Provider-neutral write commands and durable drafts. Nothing here contains credentials.
use super::{ProjectTarget, TaskComment, TaskItem, TaskRef, TaskState};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueDraft {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone: Option<u64>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub fields: std::collections::BTreeMap<String, serde_json::Value>,
}
impl IssueDraft {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() || self.title.chars().count() > 256 {
            return Err("Enter a title up to 256 characters.".into());
        }
        validate_body(&self.body, false)?;
        if self.labels.len() > 32
            || self
                .labels
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 200)
            || self.assignees.len() > 16
            || self.assignees.iter().any(|s| s.is_empty() || s.len() > 100)
        {
            return Err("Too many or invalid labels / assignees.".into());
        }
        if self.milestone == Some(0) {
            return Err("Invalid milestone.".into());
        }
        Ok(())
    }
}
pub fn validate_body(body: &str, required: bool) -> Result<(), String> {
    if (required && body.trim().is_empty()) || body.chars().count() > 65_536 {
        return Err("Enter text up to 65,536 characters.".into());
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    Label,
    Assignee,
    Milestone,
}
#[derive(Clone, Debug)]
pub struct TaskOption {
    pub value: String,
    pub label: String,
}
#[derive(Clone, Debug)]
pub struct TaskOptions {
    pub items: Vec<TaskOption>,
    pub next_cursor: Option<String>,
}
#[derive(Clone, Debug)]
pub enum TaskWrite {
    Jira {
        project: ProjectTarget,
        task: Option<TaskRef>,
        action: super::jira::JiraWrite,
    },
    Youtrack {
        project: ProjectTarget,
        task: Option<TaskRef>,
        action: super::youtrack::YoutrackWrite,
    },
    Create {
        project: ProjectTarget,
        draft: IssueDraft,
    },
    Edit {
        task: TaskRef,
        title: Option<String>,
        body: Option<String>,
        /// Optional metadata patch for GitHub issue editing. `None` means the
        /// field was not touched; an empty vector is an explicit clear.
        labels: Option<Vec<String>>,
        assignees: Option<Vec<String>>,
        milestone: Option<Option<u64>>,
    },
    State {
        task: TaskRef,
        state: TaskState,
    },
    Label {
        task: TaskRef,
        label: String,
        add: bool,
    },
    Assignee {
        task: TaskRef,
        login: String,
        add: bool,
    },
    Milestone {
        task: TaskRef,
        number: Option<u64>,
    },
    AddComment {
        task: TaskRef,
        body: String,
    },
    EditComment {
        task: TaskRef,
        comment: TaskComment,
        body: String,
    },
    DeleteComment {
        task: TaskRef,
        comment: TaskComment,
    },
}
impl TaskWrite {
    pub fn is_create(&self) -> bool {
        matches!(
            self,
            Self::Create { .. }
                | Self::Jira {
                    action: super::jira::JiraWrite::Create { .. },
                    ..
                }
                | Self::Youtrack {
                    action: super::youtrack::YoutrackWrite::Create { .. },
                    ..
                }
        )
    }
    pub fn project(&self) -> &ProjectTarget {
        match self {
            Self::Create { project, .. }
            | Self::Jira { project, .. }
            | Self::Youtrack { project, .. } => project,
            Self::Edit { task, .. }
            | Self::State { task, .. }
            | Self::Label { task, .. }
            | Self::Assignee { task, .. }
            | Self::Milestone { task, .. }
            | Self::AddComment { task, .. }
            | Self::EditComment { task, .. }
            | Self::DeleteComment { task, .. } => &task.project,
        }
    }
    pub fn task(&self) -> Option<&TaskRef> {
        match self {
            Self::Create { .. } => None,
            Self::Jira { task, .. } => task.as_ref(),
            Self::Youtrack { task, .. } => task.as_ref(),
            Self::Edit { task, .. }
            | Self::State { task, .. }
            | Self::Label { task, .. }
            | Self::Assignee { task, .. }
            | Self::Milestone { task, .. }
            | Self::AddComment { task, .. }
            | Self::EditComment { task, .. }
            | Self::DeleteComment { task, .. } => Some(task),
        }
    }
}
#[derive(Clone, Debug)]
pub struct WriteReceipt {
    pub deleted_task: Option<TaskRef>,
    pub task: Option<TaskItem>,
    pub comment: Option<TaskComment>,
    pub deleted_comment: Option<String>,
    pub notice: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    Rejected(String),
    Uncertain(String),
}
impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(s) | Self::Uncertain(s) => f.write_str(s),
        }
    }
}
impl From<String> for WriteError {
    fn from(s: String) -> Self {
        Self::Rejected(s)
    }
}
impl From<&str> for WriteError {
    fn from(s: &str) -> Self {
        Self::Rejected(s.into())
    }
}
