use super::*;
use serde_json::{Value, json};
fn empty() -> WriteReceipt {
    WriteReceipt {
        deleted_task: None,
        task: None,
        comment: None,
        deleted_comment: None,
        notice: None,
    }
}
fn issue_url(task: &TaskRef) -> Result<String, WriteError> {
    ProjectTarget::github(&task.project.key)?;
    if !task.id.parse::<u64>().is_ok_and(|n| n > 0) {
        return Err("Invalid issue number.".into());
    }
    Ok(format!("/repos/{}/issues/{}", task.project.key, task.id))
}
fn segment(s: &str) -> Result<String, WriteError> {
    if s.is_empty() || s.len() > 200 || matches!(s, "." | "..") {
        return Err("Invalid label name.".into());
    }
    Ok(s.bytes()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'~') {
                (c as char).to_string()
            } else {
                format!("%{c:02X}")
            }
        })
        .collect())
}
fn comment_id(task: &TaskRef, c: &TaskComment) -> Result<u64, WriteError> {
    let url =
        gpui_kit::http_client::Url::parse(&c.url).map_err(|_| "Invalid comment reference.")?;
    let expected = format!("/{}/issues/{}", task.project.key, task.id);
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.path().eq_ignore_ascii_case(&expected)
    {
        return Err("Comment does not belong to this task.".into());
    }
    url.fragment()
        .and_then(|f| f.strip_prefix("issuecomment-"))
        .and_then(|s| s.parse().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| "Invalid comment reference.".into())
}
fn parse_issue(v: Value, target: &ProjectTarget) -> Result<WriteReceipt, WriteError> {
    let task=super::parse_page(json!([v]),target).map_err(|_|WriteError::Uncertain("Change accepted, but the updated issue could not be read. Check GitHub before retrying.".into()))?.into_iter().next().ok_or_else(||WriteError::Uncertain("Change accepted, but GitHub returned no issue. Check GitHub before retrying.".into()))?;
    Ok(WriteReceipt {
        task: Some(task),
        ..empty()
    })
}
fn parse_comment(v: Value) -> Result<TaskComment, WriteError> {
    let bad = || {
        WriteError::Uncertain(
            "Comment may be saved, but its response was invalid. Check GitHub before retrying."
                .into(),
        )
    };
    let url = v["html_url"].as_str().ok_or_else(bad)?.to_owned();
    let parsed = gpui_kit::http_client::Url::parse(&url).map_err(|_| bad())?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(bad());
    }
    Ok(TaskComment {
        rich_body: None,
        id: v["node_id"].as_str().ok_or_else(bad)?.into(),
        author: v["user"]["login"].as_str().unwrap_or("Deleted user").into(),
        created_at: v["created_at"].as_str().ok_or_else(bad)?.into(),
        body: v["body"].as_str().ok_or_else(bad)?.into(),
        url,
        can_edit: None,
        can_delete: None,
    })
}
pub(super) async fn execute(g: &Github, command: &TaskWrite) -> Result<WriteReceipt, WriteError> {
    ProjectTarget::github(&command.project().key)?;
    match command {
        TaskWrite::Jira { .. } | TaskWrite::Youtrack { .. } => {
            Err("This operation requires GitHub.".into())
        }
        TaskWrite::Create { project, draft } => {
            draft.validate()?;
            let v=g.send("POST",&format!("/repos/{}/issues",project.key),Some(json!({"title":draft.title.trim(),"body":draft.body,"labels":draft.labels,"assignees":draft.assignees,"milestone":draft.milestone})),true).await?;
            let mut result = parse_issue(v, project)?;
            let task = result.task.as_ref().unwrap();
            if draft.labels.iter().any(|l| !task.labels.contains(l))
                || draft.assignees.iter().any(|a| !task.assignees.contains(a))
                || draft.milestone.is_some()
                    && task.milestone.as_ref().map(|m| m.0) != draft.milestone
            {
                result.notice=Some("Issue created, but GitHub did not apply all labels, assignees or milestone. These fields may require additional repository permissions.".into());
            }
            Ok(result)
        }
        TaskWrite::Edit {
            task,
            title,
            body,
            labels,
            assignees,
            milestone,
        } => {
            let mut patch = serde_json::Map::new();
            if let Some(title) = title {
                if title.trim().is_empty() || title.chars().count() > 256 {
                    return Err("Enter a title up to 256 characters.".into());
                }
                patch.insert("title".into(), json!(title.trim()));
            }
            if let Some(body) = body {
                validate_body(body, false)?;
                patch.insert("body".into(), json!(body));
            }
            if let Some(labels) = labels {
                if labels.len() > 32
                    || labels
                        .iter()
                        .any(|label| label.trim().is_empty() || label.len() > 200)
                {
                    return Err("Too many or invalid GitHub labels.".into());
                }
                patch.insert("labels".into(), json!(labels));
            }
            if let Some(assignees) = assignees {
                if assignees.len() > 16
                    || assignees.iter().any(|login| {
                        login.is_empty()
                            || login.len() > 100
                            || !login
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    })
                {
                    return Err("Too many or invalid GitHub assignees.".into());
                }
                patch.insert("assignees".into(), json!(assignees));
            }
            if let Some(milestone) = milestone {
                if milestone.is_some_and(|number| number == 0) {
                    return Err("Invalid milestone.".into());
                }
                patch.insert("milestone".into(), json!(milestone));
            }
            if patch.is_empty() {
                return Err("There are no changes to save.".into());
            }
            let v = g
                .send("PATCH", &issue_url(task)?, Some(Value::Object(patch)), true)
                .await?;
            let mut result = parse_issue(v, &task.project)?;
            if let Some(expected) = labels
                && result.task.as_ref().is_some_and(|updated| {
                    expected.iter().any(|label| !updated.labels.contains(label))
                })
            {
                result.notice =
                    Some("GitHub did not apply all labels. Check repository permissions.".into());
            }
            if let Some(expected) = assignees
                && result.task.as_ref().is_some_and(|updated| {
                    expected
                        .iter()
                        .any(|login| !updated.assignees.contains(login))
                })
            {
                result.notice = Some(
                    "GitHub did not apply all assignees. Check repository permissions.".into(),
                );
            }
            if let Some(expected) = milestone
                && result.task.as_ref().is_some_and(|updated| {
                    updated.milestone.as_ref().map(|value| value.0) != *expected
                })
            {
                result.notice = Some(
                    "GitHub did not apply the milestone. Check repository permissions.".into(),
                );
            }
            Ok(result)
        }
        TaskWrite::State { task, state } => {
            let v=g.send("PATCH",&issue_url(task)?,Some(json!({"state":if *state==TaskState::Open{"open"}else{"closed"},"state_reason":if *state==TaskState::Open{"reopened"}else{"completed"}})),true).await?;
            parse_issue(v, &task.project)
        }
        TaskWrite::Milestone { task, number } => {
            if *number == Some(0) {
                return Err("Invalid milestone.".into());
            }
            let v = g
                .send(
                    "PATCH",
                    &issue_url(task)?,
                    Some(json!({"milestone":number})),
                    true,
                )
                .await?;
            let mut result = parse_issue(v, &task.project)?;
            if result
                .task
                .as_ref()
                .unwrap()
                .milestone
                .as_ref()
                .map(|m| m.0)
                != *number
            {
                result.notice = Some(
                    "GitHub did not apply the milestone. Check repository permissions.".into(),
                );
            }
            Ok(result)
        }
        TaskWrite::Assignee { task, login, add } => {
            if login.is_empty()
                || login.len() > 100
                || !login
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            {
                return Err("Invalid GitHub username.".into());
            }
            let v = g
                .send(
                    if *add { "POST" } else { "DELETE" },
                    &format!("{}/assignees", issue_url(task)?),
                    Some(json!({"assignees":[login]})),
                    true,
                )
                .await?;
            let mut result = parse_issue(v, &task.project)?;
            if result.task.as_ref().unwrap().assignees.contains(login) != *add {
                result.notice=Some("GitHub did not apply this assignee. Check repository permissions and whether the user is assignable.".into());
            }
            Ok(result)
        }
        TaskWrite::Label { task, label, add } => {
            let encoded = segment(label)?;
            let path = format!("{}/labels", issue_url(task)?);
            g.send(
                if *add { "POST" } else { "DELETE" },
                &if *add {
                    path
                } else {
                    format!("{path}/{encoded}")
                },
                if *add {
                    Some(json!({"labels":[label]}))
                } else {
                    None
                },
                true,
            )
            .await?;
            let mut receipt = empty();
            match g.task(task).await{
                Ok(updated)=>{if updated.labels.contains(label)!=*add{receipt.notice=Some("GitHub did not apply this label. Check repository permissions.".into());}receipt.task=Some(updated);}
                Err(_)=>receipt.notice=Some("Label change saved. The issue could not be reloaded; refresh to see its current state.".into()),
            }
            Ok(receipt)
        }
        TaskWrite::AddComment { task, body } => {
            validate_body(body, true)?;
            let v = g
                .send(
                    "POST",
                    &format!("{}/comments", issue_url(task)?),
                    Some(json!({"body":body})),
                    true,
                )
                .await?;
            Ok(WriteReceipt {
                comment: Some(parse_comment(v)?),
                ..empty()
            })
        }
        TaskWrite::EditComment {
            task,
            comment,
            body,
        } => {
            validate_body(body, true)?;
            if comment.can_edit == Some(false) {
                return Err("You cannot edit this comment.".into());
            }
            let id = comment_id(task, comment)?;
            let v = g
                .send(
                    "PATCH",
                    &format!("/repos/{}/issues/comments/{id}", task.project.key),
                    Some(json!({"body":body})),
                    true,
                )
                .await?;
            Ok(WriteReceipt {
                comment: Some(parse_comment(v)?),
                ..empty()
            })
        }
        TaskWrite::DeleteComment { task, comment } => {
            if comment.can_delete == Some(false) {
                return Err("You cannot delete this comment.".into());
            }
            let id = comment_id(task, comment)?;
            g.send(
                "DELETE",
                &format!("/repos/{}/issues/comments/{id}", task.project.key),
                None,
                true,
            )
            .await?;
            Ok(WriteReceipt {
                deleted_comment: Some(comment.id.clone()),
                ..empty()
            })
        }
    }
}
pub(super) async fn options(
    g: &Github,
    project: &ProjectTarget,
    kind: OptionKind,
    cursor: Option<&str>,
) -> Result<TaskOptions, String> {
    ProjectTarget::github(&project.key)?;
    let page = cursor
        .unwrap_or("1")
        .parse::<u32>()
        .ok()
        .filter(|p| *p > 0 && *p <= 1000)
        .ok_or("Invalid options page.")?;
    let endpoint = match kind {
        OptionKind::Label => "labels",
        OptionKind::Assignee => "assignees",
        OptionKind::Milestone => "milestones",
    };
    let state = if kind == OptionKind::Milestone {
        "&state=all"
    } else {
        ""
    };
    let value = g
        .get(&format!(
            "/repos/{}/{endpoint}?per_page=100&page={page}{state}",
            project.key
        ))
        .await?;
    let rows = value.as_array().ok_or("Invalid repository options.")?;
    let mut items = vec![];
    for row in rows {
        let pair = match kind {
            OptionKind::Label => row["name"].as_str().map(|s| (s.to_owned(), s.to_owned())),
            OptionKind::Assignee => row["login"]
                .as_str()
                .map(|s| (s.to_owned(), format!("@{s}"))),
            OptionKind::Milestone => {
                row["number"]
                    .as_u64()
                    .zip(row["title"].as_str())
                    .map(|(n, s)| {
                        (
                            n.to_string(),
                            format!(
                                "{s}{}",
                                if row["state"] == "closed" {
                                    " (closed)"
                                } else {
                                    ""
                                }
                            ),
                        )
                    })
            }
        };
        if let Some((value, label)) = pair {
            items.push(TaskOption { value, label });
        }
    }
    Ok(TaskOptions {
        items,
        next_cursor: (rows.len() == 100 && page < 1000).then(|| (page + 1).to_string()),
    })
}
