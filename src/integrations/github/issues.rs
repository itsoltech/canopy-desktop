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
