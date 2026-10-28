//! Presentation adapters: remote task DTOs rendered through the local task
//! components, plus the human duration/date formats used by local editors.
use super::*;
use canopy_desktop::integrations::{
    self as local, ProjectTarget, Provider, TaskItem, TaskState, jira::JiraDetails,
};
use serde_json::Value as Json;

pub(super) fn provider(name: &str) -> Provider {
    match name {
        "jira" => Provider::Jira,
        "youtrack" => Provider::Youtrack,
        _ => Provider::Github,
    }
}

/// Local task reference used only for display (row identity, labels, headers).
pub(super) fn reference(task: &p::Task, provider: Provider, project: &str) -> local::TaskRef {
    let r = task.task.clone().unwrap_or_default();
    local::TaskRef {
        project: ProjectTarget {
            provider,
            key: if project.is_empty() {
                r.project_id
            } else {
                project.to_owned()
            },
            site: None,
        },
        id: r.task_id,
        title: task.title.clone(),
    }
}

/// Status shown by local rows/badges: the provider workflow status when known.
pub(super) fn status_label(task: &p::Task) -> Option<String> {
    task.statuses
        .iter()
        .map(|s| s.label.trim())
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

fn field<'a>(task: &'a p::Task, id: &str) -> Option<&'a p::TaskField> {
    task.fields.iter().find(|f| f.id == id)
}

/// Human-readable value of a read-only provider field (names, never raw ids).
pub(super) fn field_text(task: &p::Task, id: &str) -> Option<String> {
    let value = super::fields::json(field(task, id)?.current_value.as_ref());
    display(&value)
}

pub(super) fn display(value: &Json) -> Option<String> {
    match value {
        Json::Null => None,
        Json::String(s) if s.trim().is_empty() => None,
        Json::String(s) => Some(s.clone()),
        Json::Bool(b) => Some(if *b { "Yes" } else { "No" }.into()),
        Json::Number(n) => Some(n.to_string()),
        Json::Array(values) => {
            let parts: Vec<_> = values.iter().filter_map(display).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        Json::Object(o) => [
            "displayName",
            "name",
            "value",
            "presentation",
            "text",
            "login",
        ]
        .iter()
        .filter_map(|k| o.get(*k))
        .find_map(display),
    }
}

/// Names for labels/assignees. Opaque provider ids are resolved through the
/// detail projection or loaded option lists; unresolved ids are not shown.
pub(super) fn people_and_labels(
    task: &p::Task,
    provider: Provider,
    names: &BTreeMap<String, String>,
) -> (Vec<String>, Vec<String>) {
    let labels = task
        .labels
        .iter()
        .filter_map(|id| match names.get(id) {
            Some(name) => Some(name.clone()),
            None if provider == Provider::Youtrack => None,
            None => Some(id.clone()),
        })
        .collect();
    let assignee_name = field_text(task, "assignee");
    let assignees = task
        .assignees
        .iter()
        .filter_map(|id| match names.get(id) {
            Some(name) => Some(name.clone()),
            None if provider == Provider::Jira => assignee_name.clone(),
            None => Some(id.clone()),
        })
        .collect();
    (labels, assignees)
}

pub(super) fn item(
    task: &p::Task,
    provider: Provider,
    project: &str,
    names: &BTreeMap<String, String>,
) -> TaskItem {
    let (labels, assignees) = people_and_labels(task, provider, names);
    let jira = (provider == Provider::Jira).then(|| {
        Arc::new(JiraDetails {
            complete: true,
            internal_id: None,
            status: status_label(task).unwrap_or_else(|| state_label(task).into()),
            category: String::new(),
            issue_type: String::new(),
            fields: Json::Null,
            names: Json::Null,
            description: Json::Null,
            description_editable: true,
        })
    });
    TaskItem {
        reference: reference(task, provider, project),
        body: Arc::from(""),
        state: if task.state == "closed" {
            TaskState::Closed
        } else {
            TaskState::Open
        },
        labels,
        assignees,
        url: task.url.clone(),
        milestone: None,
        jira,
        youtrack: None,
    }
}

pub(super) fn state_label(task: &p::Task) -> &'static str {
    if task.state == "closed" {
        "Closed"
    } else {
        "Open"
    }
}

pub(super) fn comment(comment: &p::TaskComment) -> local::TaskComment {
    local::TaskComment {
        id: comment.id.clone(),
        rich_body: None,
        can_edit: Some(comment.can_edit),
        can_delete: Some(comment.can_delete),
        author: comment.author.clone(),
        created_at: comment.created_at.clone(),
        body: Arc::from(comment.body.as_str()),
        url: comment.url.clone(),
    }
}

/// Parses `1h 30m`, `90m`, `1.5h` or a bare number of minutes.
pub(super) fn parse_duration(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    if text.is_empty() {
        return None;
    }
    if let Ok(minutes) = text.parse::<f64>() {
        return seconds(minutes * 60.);
    }
    let mut total = 0.;
    let mut number = String::new();
    let mut any = false;
    for c in text.chars() {
        match c {
            '0'..='9' | '.' | ',' => number.push(if c == ',' { '.' } else { c }),
            'h' | 'm' => {
                let value = number.parse::<f64>().ok()?;
                total += value * if c == 'h' { 3600. } else { 60. };
                number.clear();
                any = true;
            }
            ' ' => {}
            _ => return None,
        }
    }
    if !number.is_empty() || !any {
        return None;
    }
    seconds(total)
}

fn seconds(value: f64) -> Option<u64> {
    (value.is_finite() && value >= 1.).then(|| value.round() as u64)
}

pub(super) fn format_duration(seconds: u64) -> String {
    let minutes = seconds.div_ceil(60);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// Provider timestamps shown in the same `YYYY-MM-DD HH:MM` local form the
/// local editors use; unknown formats are left as provided.
pub(super) fn format_time(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .or_else(|_| chrono::DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.3f%z"))
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_owned())
}

pub(super) fn now_local() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn work_durations_accept_the_formats_people_type() {
        assert_eq!(parse_duration("1h 30m"), Some(5400));
        assert_eq!(parse_duration("1h30m"), Some(5400));
        assert_eq!(parse_duration("90m"), Some(5400));
        assert_eq!(parse_duration("1.5h"), Some(5400));
        assert_eq!(parse_duration("45"), Some(2700));
        assert_eq!(parse_duration("2H"), Some(7200));
        for invalid in ["", "0", "h", "1x", "1h 30", "-1h"] {
            assert_eq!(parse_duration(invalid), None, "{invalid}");
        }
        assert_eq!(format_duration(5400), "1h 30m");
        assert_eq!(format_duration(600), "10m");
        assert_eq!(format_duration(7200), "2h");
    }
}
