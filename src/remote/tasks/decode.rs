//! Decode once into the transport-independent integration intention.
use super::*;
use crate::command_bus::tasks as d;

fn project_key(value: proto::TaskProjectKey) -> Result<i::ProjectTarget, Status> {
    let result = i::ProjectTarget {
        provider: match value.provider.as_str() {
            "github" => i::Provider::Github,
            "jira" => i::Provider::Jira,
            "youtrack" => i::Provider::Youtrack,
            _ => return Err(invalid("Unknown task provider.")),
        },
        key: value.key,
        site: value.endpoint,
    };
    if !result.valid() {
        return Err(invalid("Invalid task project."));
    }
    Ok(result)
}
pub(super) fn configuration(
    value: proto::configure_task_project_request::Change,
) -> Result<d::TaskConfiguration, Status> {
    use proto::configure_task_project_request::Change;
    Ok(match value {
        Change::Filter(value) => d::TaskConfiguration::SelectFilter(i::filters::FilterSelection {
            project: project_key(required(value.project)?)?,
            filter_id: value.filter_id,
        }),
        Change::Repository(value) => {
            d::TaskConfiguration::Repository(value.project.map(project_key).transpose()?)
        }
        Change::Links(value) => d::TaskConfiguration::Links(
            value
                .tasks
                .into_iter()
                .map(|value| {
                    let task = i::TaskRef {
                        project: project_key(required(value.project)?)?,
                        id: value.task_id,
                        title: value.title,
                    };
                    if !task.valid() {
                        return Err(invalid("Invalid linked task."));
                    }
                    Ok(task)
                })
                .collect::<Result<_, Status>>()?,
        ),
    })
}

pub(super) fn status(s: proto::TaskStatusChoice) -> Result<d::TaskStatus, Status> {
    Ok(d::TaskStatus {
        field_id: s.field_id,
        choice_id: s.choice_id,
        form_fields: mapping::object(s.form_fields.as_ref())?,
        comment: s.comment,
    })
}
pub(super) fn changes(r: &proto::UpdateTaskRequest) -> Result<d::TaskChanges, Status> {
    Ok(d::TaskChanges {
        title: r.title.clone(),
        description: r.description.clone(),
        status: r.status.clone().map(status).transpose()?,
        labels: r.labels.as_ref().map(|v| v.values.clone()),
        assignees: r.assignees.as_ref().map(|v| v.values.clone()),
        milestone: r.milestone.as_ref().map(|v| v.id.clone()),
    })
}
pub(super) fn action(a: proto::apply_mutation_request::Action) -> Result<d::TaskAction, Status> {
    use d::TaskAction as D;
    use proto::apply_mutation_request::Action as A;
    Ok(match a {
        A::Create(v) => D::Create(d::TaskDraft {
            title: v.title,
            description: v.description,
            labels: v.labels,
            assignees: v.assignees,
            milestone_id: v.milestone_id,
            fields: mapping::object(v.fields.as_ref())?,
        }),
        A::Field(v) => D::Field(d::TaskField {
            field_id: v.field_id,
            value: mapping::json(&required(v.value)?)?,
        }),
        A::Fields(v) => D::Fields(mapping::object(Some(&v))?),
        A::Status(v) => D::Status(status(v)?),
        A::Comment(v) => D::Comment(d::TaskComment {
            comment_id: v.comment_id,
            body: v.body,
            rich_body: v.rich_body.as_ref().map(mapping::json).transpose()?,
            delete: v.delete,
        }),
        A::Tag(v) => D::Tag(d::TaskMembership {
            id: v.id,
            add: v.add,
        }),
        A::Assignee(v) => D::Assignee(d::TaskMembership {
            id: v.id,
            add: v.add,
        }),
        A::Milestone(v) => D::Milestone(v.id),
        A::Sprint(v) => D::Sprint(v.id),
        A::Attachment(v) => D::Attachment(d::TaskAttachment {
            name: v.name,
            action: match required(v.action)? {
                proto::task_attachment_write::Action::DeleteId(id) => {
                    d::TaskAttachmentAction::DeleteId(id)
                }
                proto::task_attachment_write::Action::Attach(v) => {
                    d::TaskAttachmentAction::Attach(crate::command_bus::FileContent {
                        host: api::decode::host(v.host_id)?,
                        artifact: v.artifact_id,
                        media_type: v.media_type,
                        size: v.size,
                        sha256: v
                            .sha256
                            .try_into()
                            .map_err(|_| invalid("Invalid attachment hash."))?,
                    })
                }
            },
        }),
        A::Link(v) => D::Link(d::TaskLink {
            kind: v.kind,
            other_task: v.other_task,
            outward: v.outward,
            delete_link_id: v.delete_link_id,
        }),
        A::Worklog(v) => D::Worklog(d::TaskWorklog {
            seconds: v.seconds,
            started: v.started,
            comment: v.comment,
        }),
        A::Watching(v) => D::Watching(v),
        A::Voted(v) => D::Voted(v),
        A::Delete(v) => D::Delete(d::TaskDelete {
            confirmation: v.confirmation,
            subtasks: v.subtasks,
        }),
    })
}
