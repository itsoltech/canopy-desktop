//! User-owned Jira filter expressions, always intersected with the selected project.
use super::{Config, ProjectTarget, Provider};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskFilter {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    pub expression: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterSelection {
    pub project: ProjectTarget,
    pub filter_id: String,
}
pub const DEFAULT_FILTER: &str = "jira:active";
pub const BUILTINS: [(&str, &str, &str); 7] = [
    (DEFAULT_FILTER, "Active tasks", "statusCategory != Done"),
    (
        "jira:mine",
        "My active tasks",
        "assignee = currentUser() AND statusCategory != Done",
    ),
    (
        "jira:unassigned",
        "Unassigned tasks",
        "assignee is EMPTY AND statusCategory != Done",
    ),
    ("jira:sprint", "Current sprint", "sprint in openSprints()"),
    (
        "jira:sprint-unassigned",
        "Unassigned · current sprint",
        "sprint in openSprints() AND assignee is EMPTY",
    ),
    ("jira:done", "Completed tasks", "statusCategory = Done"),
    ("jira:all", "All tasks", ""),
];
pub const YOUTRACK_BUILTINS: [(&str, &str, &str); 5] = [
    ("youtrack:active", "Active tasks", "#Unresolved"),
    (
        "youtrack:mine",
        "Assigned to me",
        "Assignee: me #Unresolved",
    ),
    (
        "youtrack:unassigned",
        "Unassigned tasks",
        "Assignee: Unassigned #Unresolved",
    ),
    ("youtrack:done", "Completed tasks", "#Resolved"),
    ("youtrack:all", "All tasks", ""),
];
pub fn builtin(id: &str) -> Option<TaskFilter> {
    let entry = BUILTINS
        .iter()
        .map(|entry| (entry, Provider::Jira))
        .chain(
            YOUTRACK_BUILTINS
                .iter()
                .map(|entry| (entry, Provider::Youtrack)),
        )
        .find(|(entry, _)| entry.0 == id)?;
    Some(TaskFilter {
        id: entry.0.0.into(),
        name: entry.0.1.into(),
        provider: entry.1,
        expression: entry.0.2.into(),
    })
}
impl TaskFilter {
    pub fn validate(&self) -> Result<(), String> {
        if uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("Invalid custom filter ID.".into());
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
        {
            return Err("Enter a filter name up to 80 characters.".into());
        }
        match self.provider {
            Provider::Jira => validate_expression(&self.expression),
            Provider::Youtrack => validate_youtrack_expression(&self.expression),
            Provider::Github => Err("Custom query filters are unavailable for GitHub.".into()),
        }
    }
}
/// Validate the composition boundary, not Jira's full grammar. Jira validates field names and functions.
pub fn validate_expression(text: &str) -> Result<(), String> {
    if text.len() > 4096
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("Use a JQL condition up to 4,096 bytes.".into());
    }
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0i32;
    let mut token = String::new();
    let mut tokens = vec![];
    for c in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
            quote = Some(c);
            continue;
        }
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth < 0 {
                return Err("JQL parentheses are not balanced.".into());
            }
        }
        if c.is_ascii_alphabetic() {
            token.push(c.to_ascii_uppercase());
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
        if c == ';' {
            return Err("Enter one JQL condition, without a statement separator.".into());
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    if quote.is_some() || escaped || depth != 0 {
        return Err("Close all quotes and parentheses in the JQL condition.".into());
    }
    if tokens.windows(2).any(|w| w == ["ORDER", "BY"]) {
        return Err("Omit ORDER BY. Tasks keeps the newest updates first.".into());
    }
    Ok(())
}
/// Validate only the composition boundary for a YouTrack query. YouTrack
/// validates field names and workflow-specific syntax on the server.
pub fn validate_youtrack_expression(text: &str) -> Result<(), String> {
    if text.len() > 4096
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("Use a YouTrack filter up to 4,096 bytes.".into());
    }
    let mut quote = None;
    let mut escaped = false;
    let mut delimiters = Vec::new();
    for c in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        // Braced YouTrack literals may contain apostrophes, for example
        // `{Won't fix}`. Treat quotes as delimiters only outside literals.
        if delimiters.last() == Some(&'{') {
            if c == '}' {
                delimiters.pop();
            }
        } else if matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if matches!(c, '(' | '{' | '[') {
            delimiters.push(c);
        } else if matches!(c, ')' | '}' | ']') {
            let expected = match c {
                ')' => '(',
                '}' => '{',
                ']' => '[',
                _ => unreachable!(),
            };
            if delimiters.pop() != Some(expected) {
                return Err("YouTrack filter delimiters are not balanced.".into());
            }
        } else if c == ';' {
            return Err("YouTrack filters cannot contain statement separators.".into());
        }
    }
    if quote.is_some() || escaped || !delimiters.is_empty() {
        return Err("Close all quotes and delimiters in the YouTrack filter.".into());
    }
    Ok(())
}
impl Config {
    pub fn filter(&self, id: &str) -> Option<TaskFilter> {
        builtin(id).or_else(|| self.task_filters.iter().find(|f| f.id == id).cloned())
    }
    pub fn filters_for(&self, provider: Provider) -> Vec<TaskFilter> {
        BUILTINS
            .iter()
            .filter_map(|(id, _, _)| builtin(id))
            .chain(
                YOUTRACK_BUILTINS
                    .iter()
                    .filter_map(|(id, _, _)| builtin(id)),
            )
            .chain(self.task_filters.iter().cloned())
            .filter(|f| f.provider == provider)
            .collect()
    }
    pub fn selected_filter(&self, project: &ProjectTarget) -> Option<TaskFilter> {
        if !matches!(project.provider, Provider::Jira | Provider::Youtrack) {
            return None;
        }
        self.filter_selections
            .iter()
            .find(|s| &s.project == project)
            .and_then(|s| self.filter(&s.filter_id))
            .or_else(|| {
                Some(match project.provider {
                    Provider::Jira => builtin(DEFAULT_FILTER),
                    Provider::Youtrack => builtin("youtrack:active"),
                    Provider::Github => None,
                })
                .flatten()
            })
    }
    pub fn select_filter(&self, project: &ProjectTarget, id: &str) -> Result<Self, String> {
        let filter = self.filter(id).ok_or("This filter no longer exists.")?;
        if !project.valid() || filter.provider != project.provider {
            return Err("This filter does not match the task provider.".into());
        }
        let mut next = self.clone();
        next.filter_selections.retain(|s| s.project != *project);
        next.filter_selections.push(FilterSelection {
            project: project.clone(),
            filter_id: id.into(),
        });
        if !next.valid() {
            return Err("Too many saved task filter selections.".into());
        }
        Ok(next)
    }
    pub fn save_filter(&self, filter: TaskFilter) -> Result<Self, String> {
        filter.validate()?;
        let mut next = self.clone();
        if next
            .task_filters
            .iter()
            .any(|f| f.id != filter.id && f.name.eq_ignore_ascii_case(filter.name.trim()))
        {
            return Err("A custom filter with this name already exists.".into());
        }
        if let Some(old) = next.task_filters.iter_mut().find(|f| f.id == filter.id) {
            *old = filter;
        } else {
            next.task_filters.push(filter);
        }
        if !next.valid() {
            return Err("Up to 64 custom filters are supported.".into());
        }
        Ok(next)
    }
    pub fn remove_filter(&self, id: &str) -> Self {
        let mut next = self.clone();
        next.task_filters.retain(|f| f.id != id);
        next.filter_selections.retain(|s| s.filter_id != id);
        next
    }
    pub(super) fn filters_valid(&self) -> bool {
        self.task_filters.len() <= 64
            && self.task_filters.iter().enumerate().all(|(i, f)| {
                f.validate().is_ok() && self.task_filters[..i].iter().all(|old| old.id != f.id)
            })
            && self.filter_selections.len() <= 256
            && self.filter_selections.iter().enumerate().all(|(i, s)| {
                s.project.valid()
                    && self
                        .filter(&s.filter_id)
                        .is_some_and(|f| f.provider == s.project.provider)
                    && self.filter_selections[..i]
                        .iter()
                        .all(|old| old.project != s.project)
            })
    }
}
