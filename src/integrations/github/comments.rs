use super::{
    Github, User,
    graphql::{PageInfo, validate_cursor},
};
use crate::integrations::{ProjectTarget, TaskComment, TaskCommentPage, TaskRef};
use serde::Deserialize;
const QUERY: &str = r#"query CanopyIssueComments($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    issue(number: $number) {
      comments(first: 30, after: $after) {
        totalCount
        pageInfo { hasNextPage endCursor }
        nodes { id body createdAt url author { login } viewerCanUpdate viewerCanDelete }
      }
    }
  }
}"#;
pub(super) async fn list(
    github: &Github,
    task: &TaskRef,
    cursor: Option<&str>,
) -> Result<TaskCommentPage, String> {
    ProjectTarget::github(&task.project.key)?;
    validate_cursor(cursor)?;
    let number = task
        .id
        .parse::<i32>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or("Invalid issue number.")?;
    let (owner, name) = task.project.key.split_once('/').unwrap();
    let value = github
        .graphql(
            QUERY,
            serde_json::json!({"owner":owner,"name":name,"number":number,"after":cursor}),
        )
        .await?;
    let comments = value
        .pointer("/data/repository/issue/comments")
        .cloned()
        .ok_or("Issue not found or not accessible with this token.")?;
    let page: Connection =
        serde_json::from_value(comments).map_err(|_| "Invalid comments from GitHub.")?;
    if page.nodes.len() > 30
        || page
            .nodes
            .iter()
            .any(|c| c.id.is_empty() || c.id.len() > 256)
    {
        return Err("Invalid comment page from GitHub.".into());
    }
    let next_cursor = page.page_info.continuation(cursor)?;
    let mut items = Vec::new();
    for c in page.nodes {
        let url = gpui_kit::http_client::Url::parse(&c.url).map_err(|_| "Invalid comment URL.")?;
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Invalid comment URL.".into());
        }
        items.push(TaskComment {
            rich_body: None,
            id: c.id,
            can_edit: c.viewer_can_update,
            can_delete: c.viewer_can_delete,
            author: c
                .author
                .map(|u| u.login)
                .unwrap_or_else(|| "Deleted user".into()),
            created_at: c.created_at,
            body: c.body.chars().take(65_536).collect::<String>().into(),
            url: url.into(),
        });
    }
    Ok(TaskCommentPage {
        comments: items,
        next_cursor,
        total_count: page.total_count,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    nodes: Vec<Comment>,
    total_count: usize,
    page_info: PageInfo,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Comment {
    id: String,
    body: String,
    created_at: String,
    url: String,
    author: Option<User>,
    viewer_can_update: Option<bool>,
    viewer_can_delete: Option<bool>,
}
