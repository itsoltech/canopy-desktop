use super::*;
use serde_json::{Map, json};
use std::sync::Arc;

fn empty() -> WriteReceipt {
    WriteReceipt {
        deleted_task: None,
        task: None,
        comment: None,
        deleted_comment: None,
        notice: None,
    }
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, WriteError> {
    value
        .as_object()
        .filter(|value| value.len() <= 256)
        .ok_or_else(|| format!("YouTrack {label} must be an object.").into())
}

fn field_id(id: &str) -> Result<(), WriteError> {
    super::validate_identifier(id, "custom field ID").map_err(WriteError::Rejected)
}

fn custom_fields(value: &Value) -> Result<Vec<Value>, WriteError> {
    let fields = object(value, "custom fields")?;
    let mut result = Vec::with_capacity(fields.len());
    for (id, value) in fields {
        field_id(id)?;
        let (field_type, field_value) = if let Some(map) = value.as_object() {
            (
                map.get("$type")
                    .and_then(Value::as_str)
                    .unwrap_or("SimpleIssueCustomField"),
                map.get("value").cloned().unwrap_or_else(|| value.clone()),
            )
        } else {
            ("SimpleIssueCustomField", value.clone())
        };
        result.push(json!({"id":id,"$type":field_type,"value":field_value}));
    }
    Ok(result)
}

fn fields_body(value: &Value) -> Result<Value, WriteError> {
    let fields = object(value, "fields")?;
    if fields.is_empty() {
        return Err("There are no changed YouTrack fields to save.".into());
    }
    let mut body = Map::new();
    let mut custom = Map::new();
    for (id, value) in fields {
        if matches!(id.as_str(), "summary" | "description") {
            body.insert(id.clone(), value.clone());
        } else if id == "customFields" {
            if let Some(values) = value.as_array() {
                body.insert(id.clone(), Value::Array(values.clone()));
            } else {
                custom.extend(
                    value
                        .as_object()
                        .ok_or("YouTrack customFields must be an object or array.")?
                        .clone(),
                );
            }
        } else {
            field_id(id)?;
            custom.insert(id.clone(), value.clone());
        }
    }
    if !custom.is_empty() {
        body.insert(
            "customFields".into(),
            Value::Array(custom_fields(&Value::Object(custom))?),
        );
    }
    Ok(Value::Object(body))
}

async fn refreshed(y: &Youtrack, task: &TaskRef) -> WriteReceipt {
    match y.task(task).await {
        Ok(task) => WriteReceipt {
            task: Some(task),
            ..empty()
        },
        Err(_) => WriteReceipt {
            notice: Some("Change saved in YouTrack. Refresh to load the updated task.".into()),
            ..empty()
        },
    }
}

async fn current(
    y: &Youtrack,
    task: &TaskRef,
) -> Result<(TaskItem, Arc<YoutrackDetails>), WriteError> {
    let task = y.task(task).await.map_err(WriteError::Rejected)?;
    let details = task
        .youtrack
        .clone()
        .ok_or("YouTrack returned no issue details.")?;
    Ok((task, details))
}

fn attachment_name(name: &str) -> Result<(), WriteError> {
    if name.is_empty()
        || name.len() > 240
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | '"'))
    {
        Err("Choose an attachment with a valid filename.".into())
    } else {
        Ok(())
    }
}

async fn comment_write(
    y: &Youtrack,
    task: &TaskRef,
    path_suffix: &str,
    body: Value,
) -> Result<TaskComment, WriteError> {
    let (_, details) = current(y, task).await?;
    let path = format!(
        "{}/comments{path_suffix}?fields=id,text,created,updated,deleted,author(id,login,name,fullName)",
        read::issue_path(task, &details)?
    );
    let value = y.send("POST", &path, Some(body), true).await?;
    read::comment_value(&value, task, &y.user).map_err(|_| {
        WriteError::Uncertain(
            "The YouTrack comment may be saved, but its response was invalid. Refresh before retrying."
                .into(),
        )
    })
}

pub(super) async fn execute(y: &Youtrack, command: &TaskWrite) -> Result<WriteReceipt, WriteError> {
    y.target(command.project()).map_err(WriteError::Rejected)?;
    match command {
        TaskWrite::AddComment { task, body } => {
            validate_body(body, true)?;
            let comment = comment_write(y, task, "", json!({"text":body})).await?;
            Ok(WriteReceipt {
                comment: Some(comment),
                ..empty()
            })
        }
        TaskWrite::EditComment {
            task,
            comment,
            body,
        } => {
            validate_body(body, true)?;
            let id = read::validate_comment(task, comment)?;
            let (_, details) = current(y, task).await?;
            let value = y
                .send(
                    "POST",
                    &format!("{}/comments/{}?fields=id,text,created,updated,deleted,author(id,login,name,fullName)", read::issue_path(task, &details)?, transport::component(&id)),
                    Some(json!({"text":body})),
                    true,
                )
                .await?;
            let comment = read::comment_value(&value, task, &y.user).map_err(|_| {
                WriteError::Uncertain(
                    "The YouTrack comment may be saved, but its response was invalid. Refresh before retrying."
                        .into(),
                )
            })?;
            Ok(WriteReceipt {
                comment: Some(comment),
                ..empty()
            })
        }
        TaskWrite::DeleteComment { task, comment } => {
            let id = read::validate_comment(task, comment)?;
            let (_, details) = current(y, task).await?;
            y.send(
                "DELETE",
                &format!(
                    "{}/comments/{}",
                    read::issue_path(task, &details)?,
                    transport::component(&id)
                ),
                None,
                true,
            )
            .await?;
            Ok(WriteReceipt {
                deleted_comment: Some(id),
                ..empty()
            })
        }
        TaskWrite::Youtrack {
            project,
            task,
            action,
        } => {
            if let Some(task) = task
                && (task.project != *project || !task.valid())
            {
                return Err("Invalid YouTrack task context.".into());
            }
            match action {
                YoutrackWrite::Create { draft, fields } => {
                    draft.validate()?;
                    let project_id = read::project_id(y, project)
                        .await
                        .map_err(WriteError::Rejected)?;
                    let mut body = Map::new();
                    body.insert("project".into(), json!({"id":project_id}));
                    body.insert("summary".into(), json!(draft.title.trim()));
                    body.insert("description".into(), json!(draft.body));
                    if !fields.is_null() {
                        let custom = custom_fields(fields)?;
                        body.insert("customFields".into(), Value::Array(custom));
                    }
                    let value = y.send("POST", "/issues?fields=id,idReadable,summary,description,project(id,shortName,name)", Some(Value::Object(body)), true).await?;
                    let readable = value["idReadable"]
                        .as_str()
                        .filter(|key| valid_issue_key(key))
                        .ok_or_else(|| WriteError::Uncertain("YouTrack may have created the issue but returned no readable key. Refresh before creating it again.".into()))?;
                    let reference = TaskRef {
                        project: project.clone(),
                        id: readable.into(),
                        title: draft.title.trim().into(),
                    };
                    let mut receipt = refreshed(y, &reference).await;
                    if receipt.task.is_none() {
                        receipt.task = read::parse_task(value.clone(), project).ok();
                    }
                    if receipt.task.is_none() {
                        let details = YoutrackDetails {
                            complete: false,
                            internal_id: value["id"].as_str().unwrap_or(readable).into(),
                            project_id: value["project"]["id"].as_str().unwrap_or("").into(),
                            project_short_name: project.key.clone(),
                            status: "Open".into(),
                            resolved: false,
                            issue_type: "Issue".into(),
                            priority: String::new(),
                            reporter: y.user.clone(),
                            fields: value.clone(),
                            custom_fields: vec![],
                            tags: vec![],
                            attachments: vec![],
                        };
                        receipt.task = Some(TaskItem {
                            reference: reference.clone(),
                            body: draft.body.clone().into(),
                            state: TaskState::Open,
                            labels: vec![],
                            assignees: vec![],
                            url: reference.url(),
                            milestone: None,
                            jira: None,
                            youtrack: Some(Arc::new(details)),
                        });
                    }
                    receipt.notice = receipt.notice.or_else(|| {
                        Some(
                            "Issue created, but YouTrack did not return its refreshed details."
                                .into(),
                        )
                    });
                    Ok(receipt)
                }
                YoutrackWrite::Fields { fields } => {
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    let (_, details) = current(y, task).await?;
                    let body = fields_body(fields)?;
                    y.send("POST", &read::issue_path(task, &details)?, Some(body), true)
                        .await?;
                    Ok(refreshed(y, task).await)
                }
                YoutrackWrite::CustomField {
                    id,
                    field_type,
                    value,
                } => {
                    field_id(id)?;
                    if field_type.is_empty()
                        || field_type.len() > 128
                        || field_type.chars().any(char::is_control)
                    {
                        return Err("Invalid YouTrack custom field type.".into());
                    }
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    let (_, details) = current(y, task).await?;
                    y.send(
                        "POST",
                        &format!(
                            "{}/customFields/{}",
                            read::issue_path(task, &details)?,
                            transport::component(id)
                        ),
                        Some(json!({"id":id,"$type":field_type,"value":value})),
                        true,
                    )
                    .await?;
                    Ok(refreshed(y, task).await)
                }
                YoutrackWrite::Status { id, choice } => {
                    field_id(id)?;
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    let (current_task, details) = current(y, task).await?;
                    let mut field = details
                        .custom_fields
                        .iter()
                        .find(|field| field.id == *id && field.is_status())
                        .cloned()
                        .ok_or("This status field no longer belongs to the task. Refresh it.")?;
                    status::load_choices(y, &mut field)
                        .await
                        .map_err(WriteError::Rejected)?;
                    let Some(body) = field.status_update(choice).map_err(WriteError::Rejected)?
                    else {
                        return Ok(WriteReceipt {
                            task: Some(current_task),
                            ..empty()
                        });
                    };
                    y.send(
                        "POST",
                        &format!(
                            "{}/customFields/{}",
                            read::issue_path(task, &details)?,
                            transport::component(id)
                        ),
                        Some(body),
                        true,
                    )
                    .await?;
                    Ok(refreshed(y, task).await)
                }
                YoutrackWrite::StateMachineEvent { id, event_id } => {
                    field_id(id)?;
                    super::validate_identifier(event_id, "state event ID")
                        .map_err(WriteError::Rejected)?;
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    let (_, details) = current(y, task).await?;
                    y.send("POST", &format!("{}/customFields/{}", read::issue_path(task, &details)?, transport::component(id)), Some(json!({"id":id,"$type":"StateMachineIssueCustomField","event":{"id":event_id,"$type":"Event"}})), true).await?;
                    Ok(refreshed(y, task).await)
                }
                YoutrackWrite::DeleteIssue { confirmation } => {
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    if confirmation != &task.id {
                        return Err("Type the exact YouTrack issue key to confirm deletion.".into());
                    }
                    let (_, details) = current(y, task).await?;
                    y.send("DELETE", &read::issue_path(task, &details)?, None, true)
                        .await?;
                    Ok(WriteReceipt {
                        deleted_task: Some(task.clone()),
                        ..empty()
                    })
                }
                YoutrackWrite::Tag { id, add } => {
                    super::validate_identifier(id, "tag ID").map_err(WriteError::Rejected)?;
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    let (_, details) = current(y, task).await?;
                    if !details.tags.iter().any(|tag| tag.id == *id) && !*add {
                        return Err("This tag no longer belongs to the selected task.".into());
                    }
                    let path = if *add {
                        format!("{}/tags", read::issue_path(task, &details)?)
                    } else {
                        format!(
                            "{}/tags/{}",
                            read::issue_path(task, &details)?,
                            transport::component(id)
                        )
                    };
                    y.send(
                        if *add { "POST" } else { "DELETE" },
                        &path,
                        (*add).then(|| json!({"id":id})),
                        true,
                    )
                    .await?;
                    Ok(refreshed(y, task).await)
                }
                YoutrackWrite::Attach {
                    name,
                    mime_type,
                    bytes,
                } => {
                    attachment_name(name)?;
                    if mime_type.is_empty()
                        || mime_type.len() > 200
                        || mime_type.chars().any(char::is_control)
                        || bytes.len() > 25 * 1024 * 1024
                    {
                        return Err("Choose a file up to 25 MiB with a valid content type.".into());
                    }
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    let (_, details) = current(y, task).await?;
                    let boundary = format!("canopy{}", uuid::Uuid::new_v4().simple());
                    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: {mime_type}\r\n\r\n").into_bytes();
                    body.extend_from_slice(bytes);
                    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
                    y.request(
                        "POST",
                        &format!("{}/attachments", read::issue_path(task, &details)?),
                        Some(body),
                        &format!("multipart/form-data; boundary={boundary}"),
                        true,
                    )
                    .await?;
                    Ok(refreshed(y, task).await)
                }
                YoutrackWrite::DeleteAttachment { id } => {
                    super::validate_identifier(id, "attachment ID")
                        .map_err(WriteError::Rejected)?;
                    let task = task.as_ref().ok_or("Select a YouTrack issue first.")?;
                    let (_, details) = current(y, task).await?;
                    if !details
                        .attachments
                        .iter()
                        .any(|attachment| attachment.id == *id)
                    {
                        return Err("Attachment no longer belongs to this task.".into());
                    }
                    y.send(
                        "DELETE",
                        &format!(
                            "{}/attachments/{}",
                            read::issue_path(task, &details)?,
                            transport::component(id)
                        ),
                        None,
                        true,
                    )
                    .await?;
                    Ok(refreshed(y, task).await)
                }
            }
        }
        _ => Err("This operation is not supported by YouTrack.".into()),
    }
}
