use super::*;
use serde_json::Value;

const PAGE_SIZE: usize = 50;

pub(super) fn offset(cursor: Option<&str>) -> Result<usize, String> {
    cursor
        .unwrap_or("0")
        .parse::<usize>()
        .ok()
        .filter(|n| *n <= 1_000_000)
        .ok_or("Invalid YouTrack page offset.".into())
}

pub(super) async fn project_id(y: &Youtrack, target: &ProjectTarget) -> Result<String, String> {
    y.target(target)?;
    let mut skip = 0;
    loop {
        let value = y
            .get(&format!(
                "/admin/projects?fields=id,shortName,name,archived&$top={PAGE_SIZE}&$skip={skip}"
            ))
            .await?;
        let rows = project_rows(&value)?;
        let raw_count = rows.len();
        if let Some(project) = rows.iter().find(|project| {
            project["shortName"]
                .as_str()
                .is_some_and(|key| key.eq_ignore_ascii_case(&target.key))
        }) {
            return project["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or("YouTrack returned a project without an internal ID.".into());
        }
        if raw_count < PAGE_SIZE {
            break;
        }
        skip += raw_count;
        if skip > 1_000_000 {
            return Err("YouTrack returned too many projects.".into());
        }
    }
    Err("YouTrack project was not found or is not accessible with this token.".into())
}

fn project_rows(value: &Value) -> Result<Vec<Value>, String> {
    value
        .as_array()
        .or_else(|| value["values"].as_array())
        .cloned()
        .ok_or("YouTrack returned an invalid project list.".into())
}

pub(super) async fn projects(y: &Youtrack, cursor: Option<&str>) -> Result<TaskOptions, String> {
    let skip = offset(cursor)?;
    let value = y
        .get(&format!(
            "/admin/projects?fields=id,shortName,name,archived&$top={PAGE_SIZE}&$skip={skip}"
        ))
        .await?;
    let rows = project_rows(&value)?;
    let raw_count = rows.len();
    let items = rows
        .iter()
        .filter(|project| project["archived"] != true)
        .filter_map(|project| {
            Some(TaskOption {
                value: project["shortName"].as_str()?.to_owned(),
                label: format!(
                    "{} · {}",
                    project["shortName"].as_str()?,
                    project["name"].as_str().unwrap_or("")
                ),
            })
        })
        .collect();
    Ok(TaskOptions {
        items,
        next_cursor: (raw_count == PAGE_SIZE).then(|| (skip + raw_count).to_string()),
    })
}

fn compose_query(
    target: &ProjectTarget,
    state: TaskState,
    context: &TaskQuery,
) -> Result<String, String> {
    let built_in = context.filter.clone().unwrap_or_else(|| match state {
        TaskState::Open => "#Unresolved".into(),
        TaskState::Closed => "#Resolved".into(),
    });
    super::super::filters::validate_youtrack_expression(&built_in)?;
    let search = context.text.trim();
    if search.len() > 1000 || search.chars().any(char::is_control) {
        return Err("Search is too long.".into());
    }
    let mut parts = vec![format!("(project: {{{}}})", target.key)];
    if !built_in.trim().is_empty() {
        parts.push(format!("({built_in})"));
    }
    if !search.is_empty() {
        if valid_issue_key(search) {
            parts.push(format!("id: {search}"));
        } else {
            let escaped = search.replace(['{', '}', '"'], "");
            if !escaped.trim().is_empty() {
                parts.push(format!("summary: {{{escaped}}}"));
            }
        }
    }
    Ok(parts.join(" AND "))
}

fn values(value: &Value) -> Vec<Value> {
    if let Some(values) = value.as_array() {
        values.clone()
    } else if value.is_null() {
        vec![]
    } else {
        vec![value.clone()]
    }
}

fn display_value(value: &Value) -> String {
    if let Some(value) = [
        value["name"].as_str(),
        value["presentation"].as_str(),
        value["login"].as_str(),
        value["fullName"].as_str(),
        value["text"].as_str(),
        value.as_str(),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.is_empty())
    {
        value.to_owned()
    } else if let Some(minutes) = value["minutes"].as_i64() {
        format!("{minutes}m")
    } else {
        value.to_string()
    }
}

fn custom_field_name(value: &Value) -> String {
    value["name"]
        .as_str()
        .or(value["projectCustomField"]["field"]["name"].as_str())
        .unwrap_or("")
        .to_owned()
}

fn custom_field_type(value: &Value) -> String {
    value["$type"]
        .as_str()
        .filter(|kind| kind.contains("IssueCustomField"))
        .map(str::to_owned)
        .unwrap_or_else(|| schema::issue_field_type(&custom_field_type_id(value)))
}

fn custom_field_type_id(value: &Value) -> String {
    value["projectCustomField"]["field"]["fieldType"]["id"]
        .as_str()
        .or(value["projectCustomField"]["fieldType"]["id"].as_str())
        .unwrap_or("unsupported")
        .to_owned()
}

fn custom_field(value: &Value) -> Option<YoutrackField> {
    let id = value["id"].as_str()?.to_owned();
    let name = custom_field_name(value);
    let field_type = custom_field_type(value);
    let field_type_id = custom_field_type_id(value);
    let kind = schema::field_kind(&field_type_id, &field_type);
    let mut bundle = value["projectCustomField"]["bundle"]["values"]
        .as_array()
        .or_else(|| value["bundle"]["values"].as_array())
        .cloned()
        .unwrap_or_default();
    for key in ["aggregatedUsers", "users", "groups"] {
        if let Some(extra) = value["projectCustomField"]["bundle"][key].as_array() {
            bundle.extend(extra.iter().cloned());
        }
    }
    Some(YoutrackField {
        id,
        name,
        field_type,
        field_type_id,
        kind,
        required: value["projectCustomField"]["canBeEmpty"] == false,
        read_only: value["projectCustomField"]["readOnly"] == true || value["readOnly"] == true,
        multi_value: value["projectCustomField"]["field"]["fieldType"]["isMultiValue"] == true
            || value["projectCustomField"]["field"]["fieldType"]["id"]
                .as_str()
                .is_some_and(|id| id.ends_with("[*]") || id.ends_with("[]"))
            || value["value"].is_array(),
        bundle_id: value["projectCustomField"]["bundle"]["id"]
            .as_str()
            .or(value["bundle"]["id"].as_str())
            .map(str::to_owned),
        value: value["value"].clone(),
        default: Value::Null,
        allowed: bundle,
        events: value["possibleEvents"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|event| {
                Some(TaskOption {
                    value: event["id"].as_str()?.to_owned(),
                    label: event["presentation"]
                        .as_str()
                        .filter(|label| !label.is_empty())
                        .unwrap_or(event["id"].as_str()?)
                        .to_owned(),
                })
            })
            .collect(),
    })
}

fn parse_attachment(value: &Value) -> Option<YoutrackAttachment> {
    Some(YoutrackAttachment {
        id: value["id"].as_str()?.to_owned(),
        name: value["name"].as_str().unwrap_or("attachment").to_owned(),
        mime_type: value["mimeType"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_owned(),
        size: value["size"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize,
        url: value["url"].as_str().map(str::to_owned),
    })
}

fn task_item(value: Value, target: &ProjectTarget, complete: bool) -> Result<TaskItem, String> {
    let id = value["id"]
        .as_str()
        .ok_or("YouTrack returned an issue without an internal ID.")?;
    let readable = value["idReadable"]
        .as_str()
        .ok_or("YouTrack returned an issue without a readable key.")?;
    let project = &value["project"];
    let short_name = project["shortName"]
        .as_str()
        .ok_or("YouTrack returned an issue without project identity.")?;
    if !short_name.eq_ignore_ascii_case(&target.key) {
        return Err("YouTrack returned an issue outside the selected project.".into());
    }
    let reference = TaskRef {
        project: target.clone(),
        id: readable.to_owned(),
        title: value["summary"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(4096)
            .collect(),
    };
    if !reference.valid() {
        return Err("YouTrack returned an invalid issue key.".into());
    }
    let custom = value["customFields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(custom_field)
        .collect::<Vec<_>>();
    let state = custom.iter().find(|field| {
        matches!(
            field.kind,
            YoutrackFieldKind::State | YoutrackFieldKind::StateMachine
        ) || field.name.eq_ignore_ascii_case("state")
    });
    // A present `resolved: null` is authoritative. Only old/minimal responses
    // without the property fall back to the state bundle values.
    let resolved = if value.get("resolved").is_some() {
        !value["resolved"].is_null()
    } else {
        state.is_some_and(|field| values(&field.value).iter().any(|v| v["isResolved"] == true))
    };
    let status = state
        .map(|field| display_value(&field.value))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if resolved {
                "Resolved".into()
            } else {
                "Open".into()
            }
        });
    let issue_type = custom
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("type"))
        .map(|f| display_value(&f.value))
        .unwrap_or_else(|| "Issue".into());
    let priority = custom
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("priority"))
        .map(|f| display_value(&f.value))
        .unwrap_or_default();
    let assignees = custom
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("assignee"))
        .map(|field| {
            values(&field.value)
                .iter()
                .map(display_value)
                .filter(|s| !s.is_empty())
                .take(16)
                .collect()
        })
        .unwrap_or_default();
    let tags = value["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tag| {
            Some(YoutrackTag {
                id: tag["id"].as_str()?.into(),
                name: tag["name"].as_str().unwrap_or("").into(),
            })
        })
        .collect::<Vec<_>>();
    let attachments = value["attachments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_attachment)
        .collect::<Vec<_>>();
    let details = YoutrackDetails {
        complete,
        internal_id: id.into(),
        project_id: project["id"].as_str().unwrap_or("").into(),
        project_short_name: short_name.into(),
        status,
        resolved,
        issue_type,
        priority,
        reporter: value["reporter"]["fullName"]
            .as_str()
            .or(value["reporter"]["name"].as_str())
            .or(value["reporter"]["login"].as_str())
            .unwrap_or("Former user")
            .into(),
        fields: value.clone(),
        custom_fields: custom,
        tags,
        attachments,
    };
    Ok(TaskItem {
        reference,
        body: value["description"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(65536)
            .collect::<String>()
            .into(),
        state: if resolved {
            TaskState::Closed
        } else {
            TaskState::Open
        },
        labels: details
            .tags
            .iter()
            .map(|tag| tag.name.clone())
            .take(100)
            .collect(),
        assignees,
        url: format!(
            "{}/issue/{}",
            target.site.as_deref().unwrap_or(""),
            readable
        ),
        milestone: None,
        jira: None,
        youtrack: Some(Arc::new(details)),
    })
}

pub(super) async fn task(y: &Youtrack, task: &TaskRef) -> Result<TaskItem, String> {
    y.target(&task.project)?;
    if !task.valid() {
        return Err("Invalid YouTrack issue reference.".into());
    }
    let fields = issue_fields();
    let value = y
        .get(&format!(
            "/issues/{}?fields={}",
            super::transport::component(&task.id),
            super::transport::component(fields),
        ))
        .await?;
    let result = task_item(value, &task.project, true)?;
    if result.reference.id != task.id {
        return Err("YouTrack returned a different issue than requested.".into());
    }
    Ok(result)
}

pub(super) async fn list(
    y: &Youtrack,
    target: &ProjectTarget,
    state: TaskState,
    cursor: Option<&str>,
) -> Result<TaskPage, String> {
    list_context(y, target, state, cursor, &TaskQuery::default()).await
}

pub(super) async fn list_context(
    y: &Youtrack,
    target: &ProjectTarget,
    state: TaskState,
    cursor: Option<&str>,
    context: &TaskQuery,
) -> Result<TaskPage, String> {
    y.target(target)?;
    let skip = offset(cursor)?;
    let query = compose_query(target, state, context)?;
    let value = y
        .get(&format!(
            "/issues?query={}&fields={}&$top={PAGE_SIZE}&$skip={skip}",
            super::transport::component(&query),
            super::transport::component(list_issue_fields()),
        ))
        .await?;
    let rows = value
        .as_array()
        .ok_or("YouTrack returned an invalid issue list.")?;
    let raw_count = rows.len();
    let tasks = rows
        .iter()
        .cloned()
        .map(|row| task_item(row, target, false))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TaskPage {
        tasks,
        next_cursor: (raw_count == PAGE_SIZE).then(|| (skip + raw_count).to_string()),
        total_count: None,
    })
}

fn comment(value: &Value, task: &TaskRef, user: &str) -> Result<TaskComment, String> {
    let id = value["id"]
        .as_str()
        .ok_or("YouTrack returned a comment without an ID.")?;
    super::validate_identifier(id, "comment ID")?;
    let author_login = value["author"]["login"].as_str().unwrap_or("");
    Ok(TaskComment {
        id: id.into(),
        rich_body: None,
        can_edit: (!user.is_empty()).then_some(author_login == user),
        can_delete: (!user.is_empty()).then_some(author_login == user),
        author: value["author"]["fullName"]
            .as_str()
            .or(value["author"]["name"].as_str())
            .or(value["author"]["login"].as_str())
            .unwrap_or("Former user")
            .into(),
        created_at: value["created"]
            .as_i64()
            .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms).map(|v| v.to_rfc3339()))
            .unwrap_or_default(),
        body: value["text"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(65536)
            .collect::<String>()
            .into(),
        url: format!("{}?focusedCommentId={id}", task.url()),
    })
}

pub(super) async fn comments(
    y: &Youtrack,
    task_ref: &TaskRef,
    cursor: Option<&str>,
) -> Result<TaskCommentPage, String> {
    y.target(&task_ref.project)?;
    let skip = offset(cursor)?;
    let details = task(y, task_ref)
        .await?
        .youtrack
        .ok_or("YouTrack returned no issue details.")?;
    let value = y.get(&format!(
        "/issues/{}/comments?fields=id,text,created,updated,deleted,author(id,login,name,fullName)&$top={PAGE_SIZE}&$skip={skip}",
        super::transport::component(&details.internal_id)
    )).await?;
    let rows = value
        .as_array()
        .ok_or("YouTrack returned an invalid comment list.")?;
    let raw_count = rows.len();
    let comments = rows
        .iter()
        .filter(|v| v["deleted"] != true)
        .map(|v| comment(v, task_ref, &y.user))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TaskCommentPage {
        comments,
        next_cursor: (raw_count == PAGE_SIZE).then(|| (skip + raw_count).to_string()),
        total_count: skip + raw_count,
    })
}

pub(super) async fn options(
    y: &Youtrack,
    target: &ProjectTarget,
    kind: OptionKind,
    cursor: Option<&str>,
) -> Result<TaskOptions, String> {
    y.target(target)?;
    let skip = offset(cursor)?;
    match kind {
        OptionKind::Assignee => {
            let value = y
                .get(&format!(
                    "/users?fields=id,login,name,fullName&$top={PAGE_SIZE}&$skip={skip}"
                ))
                .await?;
            let rows = value
                .as_array()
                .ok_or("YouTrack returned an invalid user list.")?;
            Ok(TaskOptions {
                items: rows
                    .iter()
                    .filter_map(|v| {
                        Some(TaskOption {
                            value: v["login"].as_str()?.into(),
                            label: v["fullName"]
                                .as_str()
                                .or(v["name"].as_str())
                                .or(v["login"].as_str())
                                .unwrap_or("")
                                .into(),
                        })
                    })
                    .collect(),
                next_cursor: (rows.len() == PAGE_SIZE).then(|| (skip + rows.len()).to_string()),
            })
        }
        OptionKind::Label => {
            let value = y
                .get(&format!(
                    "/tags?fields=id,name&$top={PAGE_SIZE}&$skip={skip}"
                ))
                .await?;
            let rows = value
                .as_array()
                .or_else(|| value["values"].as_array())
                .ok_or("YouTrack returned an invalid tag list.")?;
            Ok(TaskOptions {
                items: rows
                    .iter()
                    .filter_map(|v| {
                        Some(TaskOption {
                            value: v["id"].as_str()?.into(),
                            label: v["name"].as_str().unwrap_or("").into(),
                        })
                    })
                    .collect(),
                next_cursor: (rows.len() == PAGE_SIZE).then(|| (skip + rows.len()).to_string()),
            })
        }
        OptionKind::Milestone => {
            Err("YouTrack does not use milestones in this integration.".into())
        }
    }
}

fn issue_fields() -> &'static str {
    "id,idReadable,summary,description,resolved,updated,project(id,shortName,name),reporter(id,login,name,fullName),tags(id,name),attachments(id,name,size,mimeType,url),customFields(id,name,$type,value(id,name,login,fullName,text,minutes,isResolved),projectCustomField(id,canBeEmpty,field(id,name,$type,fieldType(id,isMultiValue)),bundle(id,$type)),possibleEvents(id,presentation))"
}

fn list_issue_fields() -> &'static str {
    "id,idReadable,summary,description,resolved,updated,project(id,shortName,name),reporter(id,login,name,fullName),tags(id,name),customFields(id,name,$type,value(id,name,login,fullName,text,minutes,isResolved),projectCustomField(field(fieldType(id,isMultiValue))))"
}

pub(super) fn parse_task(value: Value, target: &ProjectTarget) -> Result<TaskItem, String> {
    task_item(value, target, true)
}

pub(super) fn comment_value(
    value: &Value,
    task: &TaskRef,
    user: &str,
) -> Result<TaskComment, String> {
    comment(value, task, user)
}

pub(super) fn issue_path(task: &TaskRef, details: &YoutrackDetails) -> Result<String, String> {
    if !task.valid() || details.project_short_name != task.project.key {
        return Err("Invalid YouTrack issue context.".into());
    }
    super::validate_identifier(&details.internal_id, "issue ID")?;
    Ok(format!(
        "/issues/{}",
        super::transport::component(&details.internal_id)
    ))
}

pub(super) fn validate_comment(
    task: &TaskRef,
    comment: &TaskComment,
) -> Result<String, WriteError> {
    super::validate_identifier(&comment.id, "comment ID")?;
    let expected = format!("{}?focusedCommentId={}", task.url(), comment.id);
    if comment.url != expected {
        return Err("This comment does not belong to the selected YouTrack task.".into());
    }
    Ok(comment.id.clone())
}
