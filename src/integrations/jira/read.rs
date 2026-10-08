use super::*;
use serde_json::json;
pub(super) fn task_item(
    v: Value,
    target: &ProjectTarget,
    complete: bool,
) -> Result<TaskItem, String> {
    let key = v["key"]
        .as_str()
        .ok_or("Jira returned an issue without a key.")?
        .to_owned();
    let title = v["fields"]["summary"].as_str().unwrap_or("").to_owned();
    let reference = TaskRef {
        project: target.clone(),
        id: key,
        title,
    };
    if !reference.valid() {
        return Err("Jira returned an issue outside the selected project.".into());
    }
    let fields = &v["fields"];
    let description = fields["description"].clone();
    let category = fields["status"]["statusCategory"]["key"]
        .as_str()
        .unwrap_or("new")
        .to_owned();
    Ok(TaskItem {
        youtrack: None,
        url: reference.url(),
        reference,
        body: adf::to_markdown(&description).into(),
        state: if category == "done" {
            TaskState::Closed
        } else {
            TaskState::Open
        },
        labels: fields["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        assignees: fields["assignee"]["displayName"]
            .as_str()
            .map(|s| vec![s.to_owned()])
            .unwrap_or_default(),
        milestone: None,
        jira: Some(Arc::new(JiraDetails {
            complete,
            internal_id: v["id"].as_str().and_then(|id| id.parse().ok()),
            status: fields["status"]["name"]
                .as_str()
                .unwrap_or("Unknown")
                .into(),
            category,
            issue_type: fields["issuetype"]["name"]
                .as_str()
                .unwrap_or("Issue")
                .into(),
            names: v["names"].clone(),
            fields: fields.clone(),
            description_editable: adf::editable(&description),
            description,
        })),
    })
}
pub(super) async fn task(j: &Jira, task: &TaskRef) -> Result<TaskItem, String> {
    j.target(&task.project)?;
    let v = j
        .get(&format!("{}?fields=*all&expand=names", issue_path(task)?))
        .await?;
    task_item(v, &task.project, true)
}
pub(super) async fn list(
    j: &Jira,
    target: &ProjectTarget,
    state: TaskState,
    cursor: Option<&str>,
) -> Result<TaskPage, String> {
    list_context(j, target, state, cursor, &TaskQuery::default()).await
}
pub(super) async fn list_context(
    j: &Jira,
    target: &ProjectTarget,
    state: TaskState,
    cursor: Option<&str>,
    context: &TaskQuery,
) -> Result<TaskPage, String> {
    j.target(target)?;
    if cursor.is_some_and(|c| c.len() > 8192) {
        return Err("Invalid Jira page cursor.".into());
    }
    let query = &context.text;
    let reconcile = &context.reconcile;
    if query.len() > 1000 {
        return Err("Search is too long.".into());
    }
    let expression = context.filter.clone().unwrap_or_else(|| {
        if state == TaskState::Closed {
            "statusCategory = Done".into()
        } else {
            "statusCategory != Done".into()
        }
    });
    super::super::filters::validate_expression(&expression)?;
    let mut jql = format!("project = \"{}\"", target.key);
    if !expression.trim().is_empty() {
        jql.push_str(&format!(" AND ({expression})"));
    }
    if !query.trim().is_empty() {
        let key = query.trim().to_ascii_uppercase();
        if valid_issue_key(&key) {
            jql.push_str(&format!(" AND key = \"{key}\""));
        } else {
            let escaped = query
                .trim()
                .replace('\\', "\\\\")
                .replace('\"', "\\\"")
                .replace(['\n', '\r'], " ");
            jql.push_str(&format!(" AND text ~ \"{escaped}\""));
        }
    }
    jql.push_str(" ORDER BY updated DESC");
    let mut body = json!({"jql":jql,"maxResults":30,"fields":["summary","description","status","issuetype","assignee","labels","priority","parent","updated"]});
    if !reconcile.is_empty() {
        body["reconcileIssues"] = json!(reconcile.iter().copied().take(32).collect::<Vec<_>>());
    }
    if let Some(c) = cursor {
        body["nextPageToken"] = json!(c);
    }
    let v = j
        .send("POST", "/rest/api/3/search/jql", Some(body), false)
        .await
        .map_err(|e| e.to_string())?;
    let issues = v["issues"]
        .as_array()
        .ok_or("Jira returned an invalid issue list.")?;
    let tasks = issues
        .iter()
        .cloned()
        .map(|v| task_item(v, target, false))
        .collect::<Result<Vec<_>, _>>()?;
    let next = v["nextPageToken"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if next.as_deref() == cursor && next.is_some() {
        return Err("Jira repeated the page cursor; refresh the list.".into());
    }
    Ok(TaskPage {
        tasks,
        next_cursor: if v["isLast"] == true { None } else { next },
        total_count: None,
    })
}
pub(super) fn comment(v: &Value, task: &TaskRef, _user: &str) -> Result<TaskComment, String> {
    let id = v["id"]
        .as_str()
        .ok_or("Jira returned a comment without an ID.")?;
    ident(id)?;
    Ok(TaskComment {
        id: id.into(),
        author: v["author"]["displayName"]
            .as_str()
            .unwrap_or("Former user")
            .into(),
        created_at: v["created"].as_str().unwrap_or("").into(),
        body: adf::to_markdown(&v["body"]).into(),
        rich_body: Some(v["body"].clone()),
        url: format!("{}?focusedCommentId={id}", task.url()),
        can_edit: None,
        can_delete: None,
    })
}
pub(super) fn offset(cursor: Option<&str>) -> Result<usize, String> {
    cursor
        .unwrap_or("0")
        .parse::<usize>()
        .ok()
        .filter(|n| *n <= 1_000_000)
        .ok_or("Invalid Jira page cursor.".into())
}
pub(super) async fn comments(
    j: &Jira,
    task: &TaskRef,
    cursor: Option<&str>,
) -> Result<TaskCommentPage, String> {
    j.target(&task.project)?;
    let start = offset(cursor)?;
    let v = j
        .get(&format!(
            "{}/comment?startAt={start}&maxResults=30&orderBy=created",
            issue_path(task)?
        ))
        .await?;
    let items = v["comments"]
        .as_array()
        .ok_or("Invalid Jira comments response.")?;
    let total = v["total"].as_u64().unwrap_or(items.len() as u64) as usize;
    Ok(TaskCommentPage {
        comments: items
            .iter()
            .map(|v| comment(v, task, &j.user))
            .collect::<Result<Vec<_>, _>>()?,
        next_cursor: (start + items.len() < total && !items.is_empty())
            .then(|| (start + items.len()).to_string()),
        total_count: total,
    })
}
pub(super) fn options_page(
    v: &Value,
    start: usize,
    kind: OptionKind,
) -> Result<TaskOptions, String> {
    let arr = v
        .as_array()
        .or_else(|| v["values"].as_array())
        .ok_or("Invalid Jira options response.")?;
    let items = arr
        .iter()
        .filter_map(|v| {
            let (id, label) = match kind {
                OptionKind::Assignee => (v["accountId"].as_str()?, v["displayName"].as_str()?),
                _ => (v.as_str()?, v.as_str()?),
            };
            Some(TaskOption {
                value: id.into(),
                label: label.into(),
            })
        })
        .collect();
    let more = if v.is_array() {
        arr.len() == 100
    } else {
        v["isLast"] != true
            && v["total"]
                .as_u64()
                .is_none_or(|n| start + arr.len() < n as usize)
    };
    Ok(TaskOptions {
        items,
        next_cursor: (more && !arr.is_empty()).then(|| (start + arr.len()).to_string()),
    })
}
pub(super) async fn options(
    j: &Jira,
    target: &ProjectTarget,
    kind: OptionKind,
    cursor: Option<&str>,
) -> Result<TaskOptions, String> {
    j.target(target)?;
    let start = offset(cursor)?;
    let path = match kind {
        OptionKind::Assignee => format!(
            "/rest/api/3/user/assignable/search?project={}&startAt={start}&maxResults=100",
            target.key
        ),
        OptionKind::Label => format!("/rest/api/3/label?startAt={start}&maxResults=100"),
        OptionKind::Milestone => return Err("Jira uses fix versions instead of milestones.".into()),
    };
    options_page(&j.get(&path).await?, start, kind)
}
pub(super) async fn projects(j: &Jira, cursor: Option<&str>) -> Result<TaskOptions, String> {
    let start = offset(cursor)?;
    let v = j
        .get(&format!(
            "/rest/api/3/project/search?startAt={start}&maxResults=100&orderBy=name"
        ))
        .await?;
    let values = v["values"].as_array().ok_or("Jira returned no projects.")?;
    Ok(TaskOptions {
        items: values
            .iter()
            .filter_map(|p| {
                Some(TaskOption {
                    value: p["key"].as_str()?.into(),
                    label: format!(
                        "{} · {}",
                        p["key"].as_str()?,
                        p["name"].as_str().unwrap_or("")
                    ),
                })
            })
            .collect(),
        next_cursor: (v["isLast"] != true
            && !values.is_empty()
            && start + values.len() < v["total"].as_u64().unwrap_or(u64::MAX) as usize)
            .then(|| (start + values.len()).to_string()),
    })
}
