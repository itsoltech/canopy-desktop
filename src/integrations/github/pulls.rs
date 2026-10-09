//! Pull requests of a repository for `#` in the chat composer (DEC-074): the open ones
//! by recent activity, or a search by title and number.
use super::Github;
use serde_json::{Value, json};

/// Characters of the description kept for the agent.
const BODY: usize = 1_500;

/// A pull request as offered and sent to the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRef {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub head: String,
    pub base: String,
    /// "open", "draft", "merged" or "closed".
    pub state: String,
    pub author: String,
    pub body: String,
}

const FIELDS: &str = "number title url headRefName baseRefName state isDraft author { login } body";

impl Github {
    /// Pull requests of `owner/name`: the open ones by recent activity without a
    /// query, else a search by its words (a number also finds that pull request).
    pub async fn pulls(
        &self,
        owner: &str,
        name: &str,
        query: &str,
    ) -> Result<Vec<PullRef>, String> {
        let query = query.trim();
        if [owner, name]
            .iter()
            .any(|part| part.is_empty() || part.len() > 100 || part.contains(['/', ' ', '"']))
            || query.len() > 200
        {
            return Err("Invalid repository or search.".into());
        }
        let number = query
            .trim_start_matches('#')
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0);
        let (text, variables) = if query.is_empty() {
            (
                format!(
                    "query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{
                       pullRequests(first: 20, states: [OPEN], orderBy: {{field: UPDATED_AT, direction: DESC}}) {{
                         nodes {{ {FIELDS} }} }} }} }}"
                ),
                json!({"owner": owner, "name": name}),
            )
        } else {
            let words = query.replace('"', " ");
            (
                format!(
                    "query($owner: String!, $name: String!, $search: String!, $number: Int!, $byNumber: Boolean!) {{
                       repository(owner: $owner, name: $name) {{
                         pullRequest(number: $number) @include(if: $byNumber) {{ {FIELDS} }} }}
                       search(query: $search, type: ISSUE, first: 20) {{
                         nodes {{ ... on PullRequest {{ {FIELDS} }} }} }} }}"
                ),
                json!({
                    "owner": owner,
                    "name": name,
                    "search": format!("repo:{owner}/{name} is:pr {words} sort:updated-desc"),
                    "number": number.unwrap_or(1),
                    "byNumber": number.is_some(),
                }),
            )
        };
        let value = self
            .request(
                "/graphql",
                Some(json!({"query": text, "variables": variables})),
            )
            .await
            .map_err(|error| {
                if error.starts_with("GitHub access denied") {
                    PERMISSION.to_owned()
                } else {
                    error
                }
            })?;
        parse(&value)
    }
}

const PERMISSION: &str = "GitHub denied access to pull requests. The token needs Pull requests: read for this repository (and organization SSO approval).";

fn parse(value: &Value) -> Result<Vec<PullRef>, String> {
    if let Some(errors) = value["errors"]
        .as_array()
        .filter(|errors| !errors.is_empty())
    {
        let kind = |kind: &str| errors.iter().any(|e| e["type"].as_str() == Some(kind));
        return Err(if kind("RATE_LIMITED") {
            "GitHub rate limit reached. Wait before searching again.".into()
        } else if kind("FORBIDDEN") || kind("INSUFFICIENT_SCOPES") {
            PERMISSION.into()
        } else if kind("NOT_FOUND") {
            "Repository not found or not accessible with this token.".into()
        } else {
            "GitHub could not list pull requests. Check repository access and the token's Pull requests: read permission.".into()
        });
    }
    let data = &value["data"];
    if data["repository"].is_null() && data["search"].is_null() {
        return Err("Repository not found or not accessible with this token.".into());
    }
    let mut pulls: Vec<PullRef> = vec![];
    let nodes = std::iter::once(&data["repository"]["pullRequest"])
        .chain(
            data["repository"]["pullRequests"]["nodes"]
                .as_array()
                .into_iter()
                .flatten(),
        )
        .chain(data["search"]["nodes"].as_array().into_iter().flatten());
    for node in nodes {
        let Some(pull) = pull(node) else {
            continue;
        };
        if !pulls.iter().any(|known| known.number == pull.number) {
            pulls.push(pull);
        }
    }
    Ok(pulls)
}

fn pull(node: &Value) -> Option<PullRef> {
    let number = node["number"].as_u64().filter(|n| *n > 0)?;
    let text = |key: &str, limit: usize| -> String {
        node[key]
            .as_str()
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(limit)
            .collect()
    };
    let url = text("url", 512);
    if !url.starts_with("https://") {
        return None;
    }
    let state = match (node["state"].as_str(), node["isDraft"].as_bool()) {
        (Some("MERGED"), _) => "merged",
        (Some("CLOSED"), _) => "closed",
        (_, Some(true)) => "draft",
        _ => "open",
    };
    Some(PullRef {
        number,
        title: text("title", 300),
        url,
        head: text("headRefName", 256),
        base: text("baseRefName", 256),
        state: state.into(),
        author: node["author"]["login"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(64)
            .collect(),
        body: text("body", BODY),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(number: u64, state: &str, draft: bool) -> Value {
        json!({"number": number, "title": format!("PR {number}"), "url": format!("https://github.com/o/r/pull/{number}"),
               "headRefName": "feat", "baseRefName": "main", "state": state, "isDraft": draft,
               "author": {"login": "kuba"}, "body": "Body\u{7}text"})
    }

    #[test]
    fn pull_requests_come_once_with_their_state() {
        let value = json!({"data": {
            "repository": {"pullRequest": node(7, "OPEN", true)},
            "search": {"nodes": [node(7, "OPEN", true), node(3, "MERGED", false), {}]}
        }});
        let pulls = parse(&value).unwrap();
        assert_eq!(
            pulls
                .iter()
                .map(|p| (p.number, p.state.as_str()))
                .collect::<Vec<_>>(),
            [(7, "draft"), (3, "merged")]
        );
        assert_eq!(pulls[0].body, "Bodytext");
        let open =
            json!({"data": {"repository": {"pullRequests": {"nodes": [node(9, "OPEN", false)]}}}});
        assert_eq!(parse(&open).unwrap()[0].state, "open");
    }

    #[test]
    fn errors_say_what_to_fix() {
        let forbidden = json!({"errors": [{"type": "FORBIDDEN", "message": "no"}]});
        assert!(
            parse(&forbidden)
                .unwrap_err()
                .contains("Pull requests: read")
        );
        let missing = json!({"data": {"repository": null}});
        assert!(parse(&missing).unwrap_err().contains("not found"));
    }
}
