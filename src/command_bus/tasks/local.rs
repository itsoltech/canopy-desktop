//! Local provider writes enter the same typed intention registry. Attachment
//! bytes stay in the bulk owner; only finished immutable references are retained.
use super::*;
use integrations::{TaskWrite as W, jira::JiraWrite as J, youtrack::YoutrackWrite as Y};

fn youtrack_fields(fields: &Value) -> Value {
    fields
        .as_object()
        .map(|fields| {
            Value::Object(
                fields
                    .iter()
                    .map(|(id, field)| {
                        (
                            id.clone(),
                            field.get("value").cloned().unwrap_or_else(|| field.clone()),
                        )
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| fields.clone())
}

impl TaskCommand {
    pub fn from_write(
        integration: String,
        write: &integrations::TaskWrite,
        stage: impl Fn(&str, &[u8]) -> std::result::Result<super::super::FileContent, String>,
    ) -> std::result::Result<Self, String> {
        use TaskAction as A;
        let draft = |d: &integrations::IssueDraft, fields| TaskDraft {
            title: d.title.clone(),
            description: d.body.clone(),
            labels: d.labels.clone(),
            assignees: d.assignees.clone(),
            milestone_id: d.milestone.map(|v| v.to_string()),
            fields,
        };
        let status = |field: &str, choice: &str, form_fields, comment| {
            A::Status(TaskStatus {
                field_id: field.into(),
                choice_id: choice.into(),
                form_fields,
                comment,
            })
        };
        let comment = |id, body, rich_body, delete| {
            A::Comment(TaskComment {
                comment_id: id,
                body,
                rich_body,
                delete,
            })
        };
        let attachment =
            |name: &str, media: &str, bytes: &[u8]| -> std::result::Result<A, String> {
                Ok(A::Attachment(TaskAttachment {
                    name: name.into(),
                    action: TaskAttachmentAction::Attach(stage(media, bytes)?),
                }))
            };
        let deletion = |id: &str| {
            A::Attachment(TaskAttachment {
                name: String::new(),
                action: TaskAttachmentAction::DeleteId(id.into()),
            })
        };
        let action = match write {
            W::Edit {
                task,
                title,
                body,
                labels,
                assignees,
                milestone,
            } => {
                return Ok(Self::Update {
                    integration,
                    project: task.project.key.clone(),
                    task: task.id.clone(),
                    changes: TaskChanges {
                        title: title.clone(),
                        description: body.clone(),
                        status: None,
                        labels: labels.clone(),
                        assignees: assignees.clone(),
                        milestone: milestone.map(|v| v.map(|n| n.to_string())),
                    },
                });
            }
            W::Create { draft: d, .. } => A::Create(draft(
                d,
                serde_json::to_value(&d.fields).map_err(|e| e.to_string())?,
            )),
            W::State { state, .. } => status(
                "github-state",
                if *state == integrations::TaskState::Open {
                    "open"
                } else {
                    "closed"
                },
                serde_json::json!({}),
                None,
            ),
            W::Label { label, add, .. } => A::Tag(TaskMembership {
                id: label.clone(),
                add: *add,
            }),
            W::Assignee { login, add, .. } => A::Assignee(TaskMembership {
                id: login.clone(),
                add: *add,
            }),
            W::Milestone { number, .. } => A::Milestone(number.map(|n| n.to_string())),
            W::AddComment { body, .. } => comment(None, body.clone(), None, false),
            W::EditComment {
                comment: c, body, ..
            } => comment(Some(c.id.clone()), body.clone(), None, false),
            W::DeleteComment { comment: c, .. } => {
                comment(Some(c.id.clone()), String::new(), None, true)
            }
            W::Jira { action, .. } => match action {
                J::Create { fields } => A::Create(TaskDraft {
                    title: fields
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    description: String::new(),
                    labels: Vec::new(),
                    assignees: Vec::new(),
                    milestone_id: None,
                    fields: fields.clone(),
                }),
                J::Fields { fields } => A::Fields(fields.clone()),
                J::DeleteIssue {
                    confirmation,
                    subtasks,
                } => A::Delete(TaskDelete {
                    confirmation: confirmation.clone(),
                    subtasks: *subtasks,
                }),
                J::Sprint { id } => A::Sprint(id.clone()),
                J::CommentDocument {
                    comment: c,
                    document,
                } => comment(
                    Some(c.id.clone()),
                    String::new(),
                    Some(document.clone()),
                    false,
                ),
                J::QuickTransition { id } => status("jira-status", id, serde_json::json!({}), None),
                J::Transition {
                    id,
                    fields,
                    comment,
                } => status("jira-status", id, fields.clone(), Some(comment.clone())),
                J::Attach { name, bytes } => attachment(name, "application/octet-stream", bytes)?,
                J::DeleteAttachment { id } => deletion(id),
                J::Link {
                    kind,
                    other,
                    outward,
                } => A::Link(TaskLink {
                    kind: kind.clone(),
                    other_task: other.clone(),
                    outward: *outward,
                    delete_link_id: None,
                }),
                J::DeleteLink { id } => A::Link(TaskLink {
                    kind: String::new(),
                    other_task: String::new(),
                    outward: false,
                    delete_link_id: Some(id.clone()),
                }),
                J::LogWork {
                    seconds,
                    started,
                    comment,
                } => A::Worklog(TaskWorklog {
                    seconds: *seconds,
                    started: started.clone(),
                    comment: comment.clone(),
                }),
                J::Watch { watching } => A::Watching(*watching),
                J::Vote { voted } => A::Voted(*voted),
            },
            W::Youtrack { action, .. } => match action {
                Y::Create { draft: d, fields } => A::Create(draft(d, youtrack_fields(fields))),
                Y::Fields { fields } => A::Fields(youtrack_fields(fields)),
                Y::CustomField {
                    id,
                    field_type: _,
                    value,
                } => A::Field(TaskField {
                    field_id: id.clone(),
                    value: value.clone(),
                }),
                Y::Status { id, choice } => status(id, choice, serde_json::json!({}), None),
                Y::StateMachineEvent { id, event_id } => {
                    status(id, event_id, serde_json::json!({}), None)
                }
                Y::DeleteIssue { confirmation } => A::Delete(TaskDelete {
                    confirmation: confirmation.clone(),
                    subtasks: false,
                }),
                Y::Tag { id, add } => A::Tag(TaskMembership {
                    id: id.clone(),
                    add: *add,
                }),
                Y::Attach {
                    name,
                    mime_type,
                    bytes,
                } => attachment(name, mime_type, bytes)?,
                Y::DeleteAttachment { id } => deletion(id),
            },
        };
        Ok(Self::Mutate {
            integration,
            project: write.project().key.clone(),
            task: write.task().map(|t| t.id.clone()),
            action,
        })
    }
}
