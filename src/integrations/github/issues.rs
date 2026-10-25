//! Repository.issues contains issues only, so PRs never consume a page slot.
use super::{
    Github, Issue, Label, User,
    graphql::{PageInfo, validate_cursor},
    task_item,
};
use crate::integrations::{ProjectTarget, TaskPage, TaskState};
use serde::Deserialize;

const QUERY: &str = r#"query CanopyIssues($owner: String!, $name: String!, $states: [IssueState!], $after: String) {
  repository(owner: $owner, name: $name) {
    issues(first: 30, after: $after, states: $states, orderBy: {field: UPDATED_AT, direction: DESC}) {
      totalCount
      pageInfo { hasNextPage endCursor }
      nodes {
        number title body state
        milestone { number title }
        labels(first: 100) { nodes { name } }
        assignees(first: 16) { nodes { login } }
      }
    }
  }
}"#;
pub(super) async fn list(
    github: &Github,
    target: &ProjectTarget,
    state: TaskState,
    cursor: Option<&str>,
) -> Result<TaskPage, String> {
    ProjectTarget::github(&target.key)?;
    validate_cursor(cursor)?;
    let (owner, name) = target.key.split_once('/').unwrap();
    let value=github.graphql(QUERY,serde_json::json!({"owner":owner,"name":name,"states":[if state==TaskState::Open{"OPEN"}else{"CLOSED"}],"after":cursor})).await?;
    let repository = value
        .get("data")
        .and_then(|data| data.get("repository"))
        .filter(|repo| !repo.is_null())
        .ok_or("Repository not found or not accessible with this token.")?;
    let connection: IssueConnection =
        serde_json::from_value(repository.get("issues").cloned().unwrap_or_default())
            .map_err(|_| "Invalid issue data from GitHub.")?;
    if connection.nodes.len() > 30 || connection.nodes.iter().any(|issue| issue.number == 0) {
        return Err("Invalid issue page from GitHub.".into());
    }
    let next_cursor = connection.page_info.continuation(cursor)?;
    let tasks = connection
        .nodes
        .into_iter()
        .map(|issue| {
            task_item(
                Issue {
                    number: issue.number,
                    title: issue.title,
                    body: issue.body,
                    state: issue.state,
                    labels: issue.labels.nodes,
                    assignees: issue.assignees.nodes,
                    pull_request: None,
                    milestone: issue.milestone,
                },
                target,
            )
        })
        .collect();
    Ok(TaskPage {
        tasks,
        next_cursor,
        total_count: Some(connection.total_count),
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueConnection {
    total_count: usize,
    page_info: PageInfo,
    nodes: Vec<GraphqlIssue>,
}
#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
}
#[derive(Deserialize)]
struct GraphqlIssue {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    milestone: Option<super::Milestone>,
    labels: Nodes<Label>,
    assignees: Nodes<User>,
}

/// Search remains bounded by the selected repository and state; user text is a
/// quoted phrase, never additional GitHub search qualifiers.
pub(super) async fn search(
    github: &Github,
    target: &ProjectTarget,
    state: TaskState,
    cursor: Option<&str>,
    text: &str,
) -> Result<TaskPage, String> {
    if text.trim().is_empty() {
        return list(github, target, state, cursor).await;
    }
    ProjectTarget::github(&target.key)?;
    validate_cursor(cursor)?;
    if text.len() > 1000 || text.chars().any(char::is_control) {
        return Err("Invalid GitHub search text.".into());
    }
    let phrase = text.trim().replace('\\', "\\\\").replace('"', "\\\"");
    let query = format!(
        "repo:{} is:issue is:{} sort:updated-desc \"{}\"",
        target.key,
        if state == TaskState::Open {
            "open"
        } else {
            "closed"
        },
        phrase
    );
    let response=github.graphql(r#"query CanopyIssueSearch($query:String!,$after:String){search(query:$query,type:ISSUE,first:30,after:$after){issueCount pageInfo{hasNextPage endCursor} nodes{... on Issue{number title body state repository{nameWithOwner} milestone{number title} labels(first:100){nodes{name}} assignees(first:16){nodes{login}}}}}}"#,serde_json::json!({"query":query,"after":cursor})).await?;
    let connection = &response["data"]["search"];
    let rows = connection["nodes"]
        .as_array()
        .ok_or("Invalid GitHub search result.")?;
    if rows.len() > 30 {
        return Err("GitHub returned too many search results.".into());
    }
    let page_info: PageInfo = serde_json::from_value(connection["pageInfo"].clone())
        .map_err(|_| "Invalid GitHub search cursor.")?;
    let mut tasks = vec![];
    for row in rows {
        if !row["repository"]["nameWithOwner"]
            .as_str()
            .is_some_and(|repo| repo.eq_ignore_ascii_case(&target.key))
        {
            return Err("GitHub search returned a task outside this repository.".into());
        }
        let issue: GraphqlIssue = serde_json::from_value(row.clone())
            .map_err(|_| "Invalid GitHub issue in search result.")?;
        if issue.number == 0 {
            return Err("Invalid issue number.".into());
        }
        tasks.push(task_item(
            Issue {
                number: issue.number,
                title: issue.title,
                body: issue.body,
                state: issue.state,
                labels: issue.labels.nodes,
                assignees: issue.assignees.nodes,
                pull_request: None,
                milestone: issue.milestone,
            },
            target,
        ));
    }
    Ok(TaskPage {
        tasks,
        next_cursor: page_info.continuation(cursor)?,
        total_count: connection["issueCount"].as_u64().map(|n| n as usize),
    })
}
