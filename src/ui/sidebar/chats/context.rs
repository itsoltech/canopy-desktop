//! Read-only checkout context from the existing project catalogue and Git watcher.
use crate::app_state::GitState;
use canopy_desktop::state::projects::Project;

pub(super) struct ChatContext {
    pub project: String,
    pub checkout: String,
    pub git: bool,
    pub detail: String,
}

impl ChatContext {
    pub fn new(project: Option<&Project>, git: &GitState) -> Self {
        let Some(project) = project else {
            return Self {
                project: "Without a project".into(),
                checkout: "Private folder".into(),
                git: false,
                detail: "Without a project · private chat folder".into(),
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
            git: repository.is_some(),
            detail: format!("Project: {}\nFolder: {}", root.display(), path.display()),
        };
        context.checkout = if let Some(worktree) =
            repository.and_then(|r| r.worktrees.iter().find(|w| w.path == *path))
        {
            let head = worktree.head.description();
            if path != root {
                let folder = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                context
                    .detail
                    .push_str(&format!("\nWorktree: {}\nHEAD: {head}", path.display()));
                format!("{head} · {folder}")
            } else {
                context
                    .detail
                    .push_str(&format!("\nMain checkout\nHEAD: {head}"));
                head
            }
        } else {
            match git.entries.get(path) {
                Some(Ok(None)) => "Folder without Git".into(),
                Some(Err(error)) => {
                    context.detail.push_str(&format!("\nGit: {error}"));
                    "Git unavailable".into()
                }
                Some(Ok(Some(_))) => "Git HEAD unavailable".into(),
                None => "Reading Git…".into(),
            }
        };
        context
    }
}
