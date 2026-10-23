//! Closed chats for `/resume` (DEC-094/095): what reopens one where it was — its tab
//! title, agent, choices, session and folder — kept 30 days after it closed.
use crate::state::workspace::PaneMetadata;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The command listing closed chats, typed as `/resume`.
pub const COMMAND: &str = "resume";

/// How long a closed chat waits to be reopened.
pub const KEEP_DAYS: i64 = 30;

/// The project a chat was in ("No project" when absent).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosedProject {
    pub name: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClosedChat {
    pub title: String,
    /// Agent tool id (`claude`, `codex`).
    pub tool: String,
    /// The pane as it was: folder, account, session to resume, choices and chat id.
    pub metadata: PaneMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ClosedProject>,
}

impl ClosedChat {
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    /// The saved conversation's id.
    pub fn chat_id(&self) -> Option<&str> {
        self.metadata.chat.as_ref()?.id.as_deref()
    }

    /// Whether it was in the project at `path` (its worktree or its root).
    pub fn in_project(&self, path: Option<&std::path::Path>) -> bool {
        match (&self.project, path) {
            (None, None) => true,
            (Some(project), Some(path)) => {
                project.worktree_path.as_deref().unwrap_or(&project.path) == path
            }
            _ => false,
        }
    }

    /// The search matches its title or project name, case-insensitively.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.title.to_lowercase().contains(&query)
            || self
                .project
                .as_ref()
                .is_some_and(|project| project.name.to_lowercase().contains(&query))
    }
}

/// The oldest close time a chat may have to be kept, for `now` (unix seconds).
pub fn keep_since(now: i64) -> i64 {
    now - KEEP_DAYS * 24 * 60 * 60
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::config::ChatSettings;
    use crate::state::workspace::PaneKind;

    fn closed(title: &str, project: Option<ClosedProject>) -> ClosedChat {
        ClosedChat {
            title: title.into(),
            tool: "claude".into(),
            metadata: PaneMetadata {
                kind: PaneKind::Chat,
                cwd: Some("/repo".into()),
                resume_id: Some("s1".into()),
                chat: Some(Box::new(ChatSettings {
                    id: Some("11111111-1111-4111-8111-111111111111".into()),
                    ..Default::default()
                })),
                ..Default::default()
            },
            project,
        }
    }

    #[test]
    fn a_closed_chat_round_trips_and_is_found() {
        let project = ClosedProject {
            name: "gakko".into(),
            path: "/repo".into(),
            worktree_path: Some("/repo-wt".into()),
        };
        let chat = closed("Fix the login form", Some(project));
        let back = ClosedChat::from_json(&chat.to_json().unwrap()).unwrap();
        assert_eq!(back, chat);
        assert_eq!(back.chat_id(), Some("11111111-1111-4111-8111-111111111111"));
        assert!(back.in_project(Some("/repo-wt".as_ref())));
        assert!(!back.in_project(Some("/repo".as_ref())));
        assert!(!back.in_project(None));
        assert!(closed("x", None).in_project(None));
        assert!(back.matches("login") && back.matches("GAKKO") && back.matches(" "));
        assert!(!back.matches("signup"));
        assert_eq!(keep_since(KEEP_DAYS * 86_400 + 5), 5);
    }
}
