use super::*;
use serde_json::json;
fn empty() -> WriteReceipt {
    WriteReceipt {
        deleted_task: None,
        task: None,
        comment: None,
        deleted_comment: None,
        notice: None,
    }
}
fn fields(value: &Value) -> Result<(), WriteError> {
    let values = value.as_object().ok_or("Jira fields must be an object.")?;
    if values.len() > 256
        || value.to_string().len() > 1024 * 1024
        || values.keys().any(|k| {
            k.is_empty()
                || k.len() > 128
                || !k.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
    {
        return Err("Invalid or too large Jira field values.".into());
    }
    Ok(())
}
async fn refreshed(j: &Jira, task: &TaskRef) -> WriteReceipt {
    match j.task(task).await {
        Ok(t) => WriteReceipt {
            task: Some(t),
            ..empty()
        },
        Err(_) => WriteReceipt {
            notice: Some("Change saved in Jira. Refresh to load the updated task.".into()),
            ..empty()
        },
    }
}
fn comment_path(task: &TaskRef, comment: &TaskComment) -> Result<String, WriteError> {
    ident(&comment.id)?;
    let expected = format!("{}?focusedCommentId={}", task.url(), comment.id);
    if comment.url != expected {
        return Err("This comment does not belong to the selected Jira task.".into());
    }
    Ok(format!("{}/comment/{}", issue_path(task)?, comment.id))
}
pub(super) async fn execute(j: &Jira, command: &TaskWrite) -> Result<WriteReceipt, WriteError> {
    j.target(command.project())?;
    match command {
        TaskWrite::AddComment { task, body } | TaskWrite::EditComment { task, body, .. } => {
            validate_body(body, true)?;
            let path = if let TaskWrite::EditComment { comment, .. } = command {
                if comment.can_edit == Some(false) {
                    return Err("You cannot edit this comment.".into());
                }
                if comment
                    .rich_body
                    .as_ref()
                    .is_some_and(|b| !adf::editable(b))
                {
                    return Err("This comment contains rich Jira content. Edit its original document in the rich content editor to preserve it.".into());
                }
                comment_path(task, comment)?
            } else {
                format!("{}/comment", issue_path(task)?)
            };
            let v = j
                .send(
                    if matches!(command, TaskWrite::EditComment { .. }) {
                        "PUT"
                    } else {
                        "POST"
                    },
                    &path,
                    Some(json!({"body":adf::from_markdown(body)?})),
                    true,
                )
                .await?;
            let comment=read::comment(&v,task,&j.user).map_err(|_|WriteError::Uncertain("The comment may be saved, but Jira returned an unreadable response. Refresh before retrying.".into()))?;
            Ok(WriteReceipt {
                comment: Some(comment),
                ..empty()
            })
        }
        TaskWrite::DeleteComment { task, comment } => {
            j.send("DELETE", &comment_path(task, comment)?, None, true)
                .await?;
            Ok(WriteReceipt {
                deleted_comment: Some(comment.id.clone()),
                ..empty()
            })
        }
        TaskWrite::Label { task, label, add } => {
            if label.is_empty() || label.len() > 255 || label.chars().any(char::is_whitespace) {
                return Err("Jira labels cannot contain spaces.".into());
            }
            j.send("PUT",&issue_path(task)?,Some(json!({"update":{"labels":[if *add {json!({"add":label})}else{json!({"remove":label})}]}})),true).await?;
            Ok(refreshed(j, task).await)
        }
        TaskWrite::Assignee { task, login, add } => {
            if *add {
                ident(login)?;
            }
            j.send(
                "PUT",
                &format!("{}/assignee", issue_path(task)?),
                Some(json!({"accountId":if *add {Some(login)}else{None}})),
                true,
            )
            .await?;
            Ok(refreshed(j, task).await)
        }
        TaskWrite::Jira {
            project,
            task,
            action,
        } => {
            if let Some(t) = task
                && (t.project != *project || !t.valid())
            {
                return Err("Invalid Jira task context.".into());
            }
            if let JiraWrite::Create { fields: input } = action {
                fields(input)?;
                let mut input = input.clone();
                input["project"] = json!({"key":project.key});
                if input["summary"]
                    .as_str()
                    .is_none_or(|s| s.trim().is_empty() || s.chars().count() > 255)
                    || input["issuetype"]["id"].as_str().is_none()
                {
                    return Err("Choose an issue type and enter a summary.".into());
                }
                let v = j
                    .send(
                        "POST",
                        "/rest/api/3/issue",
                        Some(json!({"fields":input})),
                        true,
                    )
                    .await?;
                let key=v["key"].as_str().filter(|s|valid_issue_key(s)).ok_or_else(||WriteError::Uncertain("Jira may have created the task but returned no key. Refresh before creating it again.".into()))?;
                let reference = TaskRef {
                    project: project.clone(),
                    id: key.into(),
                    title: input["summary"].as_str().unwrap_or("").into(),
                };
                let mut receipt = refreshed(j, &reference).await;
                if receipt.task.is_none() {
                    receipt.task = Some(read::task_item(
                        json!({"id":v["id"],"key":key,"fields":input}),
                        project,
                        false,
                    )?);
                }
                return Ok(receipt);
            }
            let task = task.as_ref().ok_or("Select a Jira task first.")?;
            let path = issue_path(task)?;
            match action {
                JiraWrite::Create { .. } => unreachable!(),
                JiraWrite::DeleteIssue {
                    confirmation,
                    subtasks,
                } => {
                    if confirmation != &task.id {
                        return Err("Type the exact issue key to confirm deletion.".into());
                    }
                    j.send(
                        "DELETE",
                        &format!("{path}?deleteSubtasks={subtasks}"),
                        None,
                        true,
                    )
                    .await?;
                    return Ok(WriteReceipt {
                        deleted_task: Some(task.clone()),
                        ..empty()
                    });
                }
                JiraWrite::Sprint { id } => {
                    let path = if let Some(id) = id {
                        ident(id)?;
                        format!("/rest/agile/1.0/sprint/{id}/issue")
                    } else {
                        "/rest/agile/1.0/backlog/issue".into()
                    };
                    j.send("POST", &path, Some(json!({"issues":[task.id]})), true)
                        .await?;
                }
                JiraWrite::CommentDocument { comment, document } => {
                    if document["type"] != "doc"
                        || document["version"] != 1
                        || document.to_string().len() > 1024 * 1024
                    {
                        return Err("Enter a valid Jira document.".into());
                    }
                    let value = j
                        .send(
                            "PUT",
                            &comment_path(task, comment)?,
                            Some(json!({"body":document})),
                            true,
                        )
                        .await?;
                    let comment = read::comment(&value, task, &j.user).map_err(|_| {
                        WriteError::Uncertain(
                            "The comment may be saved. Refresh before retrying.".into(),
                        )
                    })?;
                    return Ok(WriteReceipt {
                        comment: Some(comment),
                        ..empty()
                    });
                }
                JiraWrite::Fields { fields: input } => {
                    fields(input)?;
                    if input.as_object().is_none_or(|o| o.is_empty()) {
                        return Err("There are no changed fields to save.".into());
                    }
                    j.send("PUT", &path, Some(json!({"fields":input})), true)
                        .await?;
                }
                JiraWrite::QuickTransition { id } => {
                    ident(id)?;
                    let schema = schema::load(
                        j,
                        project,
                        &SchemaRequest::Transitions {
                            key: task.id.clone(),
                        },
                    )
                    .await?;
                    let transition = schema.transitions.iter().find(|transition| transition.id == *id)
                        .ok_or("This workflow transition is no longer available. Refresh the status choices.")?;
                    if transition.requires_form() {
                        return Err("This transition now requires additional fields. Refresh the status choices and select it again to open the form.".into());
                    }
                    j.send(
                        "POST",
                        &format!("{path}/transitions"),
                        Some(json!({"transition":{"id":id}})),
                        true,
                    )
                    .await?;
                }
                JiraWrite::Transition {
                    id,
                    fields: input,
                    comment,
                } => {
                    ident(id)?;
                    fields(input)?;
                    validate_body(comment, false)?;
                    let mut v = json!({"transition":{"id":id},"fields":input});
                    if !comment.trim().is_empty() {
                        v["update"] =
                            json!({"comment":[{"add":{"body":adf::from_markdown(comment)?}}]});
                    }
                    j.send("POST", &format!("{path}/transitions"), Some(v), true)
                        .await?;
                }
                JiraWrite::Attach { name, bytes } => {
                    if name.is_empty()
                        || name.len() > 240
                        || name
                            .chars()
                            .any(|c| c.is_control() || matches!(c, '/' | '\\' | '"'))
                        || bytes.len() > 25 * 1024 * 1024
                    {
                        return Err("Choose a file up to 25 MiB with a valid filename.".into());
                    }
                    let boundary = format!("canopy{}", uuid::Uuid::new_v4().simple());
                    let mut body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
                    body.extend_from_slice(bytes);
                    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
                    j.request(
                        "POST",
                        &format!("{path}/attachments"),
                        Some(body),
                        &format!("multipart/form-data; boundary={boundary}"),
                        true,
                    )
                    .await?;
                }
                JiraWrite::DeleteAttachment { id } => {
                    ident(id)?;
                    let fresh = j.task(task).await?;
                    if !fresh.jira.as_ref().is_some_and(|d| {
                        d.fields["attachment"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|a| a["id"].as_str() == Some(id))
                    }) {
                        return Err("Attachment no longer belongs to this task.".into());
                    }
                    j.send(
                        "DELETE",
                        &format!("/rest/api/3/attachment/{id}"),
                        None,
                        true,
                    )
                    .await?;
                }
                JiraWrite::Link {
                    kind,
                    other,
                    outward,
                } => {
                    if kind.is_empty() || kind.len() > 128 || !valid_issue_key(other) {
                        return Err("Choose a link type and enter an issue key.".into());
                    }
                    j.send("POST","/rest/api/3/issueLink",Some(json!({"type":{"name":kind},"outwardIssue":{"key":if *outward {&task.id}else{other}},"inwardIssue":{"key":if *outward {other}else{&task.id}}})),true).await?;
                }
                JiraWrite::DeleteLink { id } => {
                    ident(id)?;
                    let fresh = j.task(task).await?;
                    if !fresh.jira.as_ref().is_some_and(|d| {
                        d.fields["issuelinks"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|a| a["id"].as_str() == Some(id))
                    }) {
                        return Err("This link no longer belongs to the task.".into());
                    }
                    j.send("DELETE", &format!("/rest/api/3/issueLink/{id}"), None, true)
                        .await?;
                }
                JiraWrite::LogWork {
                    seconds,
                    started,
                    comment,
                } => {
                    if *seconds == 0
                        || *seconds > 31 * 24 * 3600
                        || started.len() > 40
                        || chrono::DateTime::parse_from_str(started, "%Y-%m-%dT%H:%M:%S%.3f%z")
                            .is_err()
                    {
                        return Err("Enter time spent and a start date with timezone.".into());
                    }
                    validate_body(comment, false)?;
                    let mut v = json!({"timeSpentSeconds":seconds,"started":started});
                    if !comment.is_empty() {
                        v["comment"] = adf::from_markdown(comment)?;
                    }
                    j.send(
                        "POST",
                        &format!("{path}/worklog?adjustEstimate=leave"),
                        Some(v),
                        true,
                    )
                    .await?;
                }
                JiraWrite::Watch { watching } => {
                    if j.user.is_empty() {
                        return Err("Verify your Jira connection first.".into());
                    }
                    let endpoint = if *watching {
                        format!("{path}/watchers")
                    } else {
                        query(&format!("{path}/watchers"), &[("accountId", &j.user)])
                    };
                    j.send(
                        if *watching { "POST" } else { "DELETE" },
                        &endpoint,
                        watching.then(|| json!(j.user)),
                        true,
                    )
                    .await?;
                }
                JiraWrite::Vote { voted } => {
                    j.send(
                        if *voted { "POST" } else { "DELETE" },
                        &format!("{path}/votes"),
                        None,
                        true,
                    )
                    .await?;
                }
            }
            Ok(refreshed(j, task).await)
        }
        _ => Err("Use Jira's issue fields and workflow transitions for this action.".into()),
    }
}
