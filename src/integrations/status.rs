//! Shared status selection with provider-native write semantics, independent of drafts/UI.
use super::{
    Provider, TaskItem, TaskOption, TaskProvider, TaskRef, TaskState, TaskWrite,
    jira::{JiraTransition, JiraWrite, SchemaRequest},
    youtrack::{YoutrackField, YoutrackFieldKind, YoutrackWrite},
};

#[derive(Clone, Debug)]
pub enum StatusField {
    Github(TaskState),
    Jira {
        current: String,
        transitions: Vec<JiraTransition>,
    },
    Youtrack(Box<YoutrackField>),
}

#[derive(Debug)]
pub enum StatusChange {
    Write(Box<TaskWrite>),
    JiraForm { transition: String },
}

pub fn from_task(task: &TaskItem) -> Vec<StatusField> {
    match task.reference.project.provider {
        Provider::Github => vec![StatusField::Github(task.state)],
        Provider::Jira => task
            .jira
            .as_ref()
            .map(|details| StatusField::Jira {
                current: details.status.clone(),
                transitions: vec![],
            })
            .into_iter()
            .collect(),
        Provider::Youtrack => task
            .youtrack
            .as_ref()
            .into_iter()
            .flat_map(|details| details.custom_fields.iter())
            .filter(|field| field.is_status())
            .map(|field| StatusField::Youtrack(Box::new(field.clone())))
            .collect(),
    }
}

pub async fn load(provider: &dyn TaskProvider, task: &TaskRef) -> Result<Vec<StatusField>, String> {
    if task.project.provider == Provider::Youtrack {
        return Ok(provider
            .youtrack_status_fields(task)
            .await?
            .into_iter()
            .map(|field| StatusField::Youtrack(Box::new(field)))
            .collect());
    }
    let current = provider.task(task).await?;
    if !current.reference.same_task(task) {
        return Err("The status response belongs to another task.".into());
    }
    let mut fields = from_task(&current);
    if task.project.provider == Provider::Jira {
        let schema = provider
            .schema(
                &task.project,
                &SchemaRequest::Transitions {
                    key: task.id.clone(),
                },
            )
            .await?;
        let Some(StatusField::Jira { transitions, .. }) = fields.first_mut() else {
            return Err("Jira returned no current status.".into());
        };
        *transitions = schema.transitions;
    }
    Ok(fields)
}

impl StatusField {
    pub fn id(&self) -> &str {
        match self {
            Self::Github(_) => "github-state",
            Self::Jira { .. } => "jira-status",
            Self::Youtrack(field) => &field.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Youtrack(field) => &field.name,
            _ => "Status",
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Github(TaskState::Open) => "Open".into(),
            Self::Github(TaskState::Closed) => "Closed".into(),
            Self::Jira { current, .. } => current.clone(),
            Self::Youtrack(field) => field.status_label(),
        }
    }

    /// Workflow actions have no selected value: their name is not the current status.
    pub fn selected(&self) -> Option<&str> {
        match self {
            Self::Github(TaskState::Open) => Some("open"),
            Self::Github(TaskState::Closed) => Some("closed"),
            Self::Youtrack(field) if field.kind == YoutrackFieldKind::State => {
                field.value["id"].as_str()
            }
            _ => None,
        }
    }

    pub fn choices(&self) -> Vec<TaskOption> {
        match self {
            Self::Github(_) => vec![
                TaskOption {
                    value: "open".into(),
                    label: "Open".into(),
                },
                TaskOption {
                    value: "closed".into(),
                    label: "Closed".into(),
                },
            ],
            Self::Jira { transitions, .. } => transitions
                .iter()
                .map(|transition| TaskOption {
                    value: transition.id.clone(),
                    label: transition.label(),
                })
                .collect(),
            Self::Youtrack(field) => field.status_choices(),
        }
    }

    pub fn read_only_reason(&self) -> Option<&'static str> {
        match self {
            Self::Youtrack(field) if field.read_only => Some("This status is read-only."),
            Self::Youtrack(field) if field.multi_value => {
                Some("This field has multiple states. Manage them in YouTrack.")
            }
            _ => None,
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Jira { .. } => Some("Choose a workflow transition. Required fields open a form."),
            Self::Youtrack(field) if field.kind == YoutrackFieldKind::StateMachine => {
                Some("Choose an available workflow transition.")
            }
            _ => None,
        }
    }

    pub fn choose(&self, task: &TaskRef, choice: &str) -> Result<Option<StatusChange>, String> {
        let command = match self {
            Self::Github(current) if task.project.provider == Provider::Github => {
                let state = match choice {
                    "open" => TaskState::Open,
                    "closed" => TaskState::Closed,
                    _ => return Err("Choose Open or Closed.".into()),
                };
                if state == *current {
                    return Ok(None);
                }
                TaskWrite::State {
                    task: task.clone(),
                    state,
                }
            }
            Self::Jira { transitions, .. } if task.project.provider == Provider::Jira => {
                let transition = transitions.iter().find(|transition| transition.id == choice)
                    .ok_or("This workflow transition is no longer available. Refresh the status choices.")?;
                if transition.requires_form() {
                    return Ok(Some(StatusChange::JiraForm {
                        transition: transition.id.clone(),
                    }));
                }
                TaskWrite::Jira {
                    project: task.project.clone(),
                    task: Some(task.clone()),
                    action: JiraWrite::QuickTransition {
                        id: transition.id.clone(),
                    },
                }
            }
            Self::Youtrack(field) if task.project.provider == Provider::Youtrack => {
                if field.status_update(choice)?.is_none() {
                    return Ok(None);
                }
                TaskWrite::Youtrack {
                    project: task.project.clone(),
                    task: Some(task.clone()),
                    action: YoutrackWrite::Status {
                        id: field.id.clone(),
                        choice: choice.to_owned(),
                    },
                }
            }
            _ => return Err("This status field belongs to another task provider.".into()),
        };
        Ok(Some(StatusChange::Write(Box::new(command))))
    }
}
