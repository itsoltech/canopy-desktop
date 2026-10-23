//! Read-only checkout context from the existing project catalogue and Git watcher.
use crate::app_state::GitState;
use canopy_desktop::state::projects::Project;
use std::path::PathBuf;

#[derive(Clone)]
pub(in crate::ui::sidebar) struct ChatContext {
    pub project: String,
    /// One-line checkout summary for the accessibility label.
    pub checkout: String,
    /// The folder the chat runs in; `None` for private chats.
    pub folder: Option<PathBuf>,
    /// The checked-out HEAD, when Git reported one.
    pub head: Option<String>,
    /// `head` belongs to the repository's own checkout, not a linked worktree.
    pub main_checkout: bool,
    pub git_error: Option<String>,
}

impl ChatContext {
    pub fn new(project: Option<&Project>, git: &GitState) -> Self {
        let Some(project) = project else {
            return Self {
                project: "Without a project".into(),
                checkout: "Private folder".into(),
                folder: None,
                head: None,
                main_checkout: false,
                git_error: None,
            };
        };
        let path = project.worktree_path.as_ref().unwrap_or(&project.path);
        let repository = git.repository(path);
        let root = repository
            .map(|r| &r.root)
            .or(project.repository_path.as_ref())
            .unwrap_or(&project.path);
        let name = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy();
        let mut context = Self {
            project: name.into_owned(),
            checkout: String::new(),
            folder: Some(path.clone()),
            head: None,
            main_checkout: false,
            git_error: None,
        };
        context.checkout = if let Some(worktree) =
            repository.and_then(|r| r.worktrees.iter().find(|w| w.path == *path))
        {
            let head = worktree.head.description();
            context.head = Some(head.clone());
            context.main_checkout = path == root;
            if path != root {
                let folder = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                format!("{head} · {folder}")
            } else {
                head
            }
        } else {
            match git.entries.get(path) {
                Some(Ok(None)) => "Folder without Git".into(),
                Some(Err(error)) => {
                    context.git_error = Some(error.to_string());
                    "Git unavailable".into()
                }
                Some(Ok(Some(_))) => "Git HEAD unavailable".into(),
                None => "Reading Git…".into(),
            }
        };
        context
    }
}
