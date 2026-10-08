//! Version 1 native session. Restoring data never starts tools or processes.
use super::{
    layout::LayoutState,
    projects::Projects,
    workspace::{Pane, Workspace},
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSnapshot {
    pub projects: Projects,
    pub workspaces: Vec<Workspace>,
    pub layout: LayoutState,
}
impl SessionSnapshot {
    pub fn valid(&self) -> bool {
        if !self.projects.snapshot().valid() || self.workspaces.len() != self.projects.items.len() {
            return false;
        }
        let mut project_ids = std::collections::HashSet::new();
        let mut workspace_ids = std::collections::HashSet::new();
        let widths = &self.layout.widths;
        if !widths.valid() {
            return false;
        }
        let mut tab_ids = std::collections::HashSet::new();
        let mut pane_ids = std::collections::HashSet::new();
        for project in &self.projects.items {
            if project
                .repository_path
                .as_ref()
                .is_some_and(|p| !p.is_absolute())
                || !project_ids.insert(project.id)
                || !workspace_ids.insert(project.workspace)
                || project
                    .worktree_path
                    .as_ref()
                    .is_some_and(|p| !p.is_absolute())
                || project.worktree_base.as_ref().is_some_and(|base| {
                    base.reference.is_empty()
                        || git2::Oid::from_str(&base.oid).is_err()
                        || !git2::Reference::is_valid_name(&format!(
                            "refs/heads/{}",
                            base.reference
                        ))
                })
            {
                return false;
            }
            let matches: Vec<_> = self
                .workspaces
                .iter()
                .filter(|w| w.id == project.workspace)
                .collect();
            if matches.len() != 1 || !matches[0].valid() {
                return false;
            }
            for tab in matches[0].tabs() {
                if !tab_ids.insert(tab.id) {
                    return false;
                }
                let mut copy = matches[0].clone();
                let _ = copy.activate(tab.id);
                for pane in copy.activation_plan() {
                    if !pane_ids.insert(pane.id) {
                        return false;
                    }
                }
            }
        }
        true
    }
    /// Only the selected project's active tab is eligible for future lazy startup.
    pub fn activation_plan(&self) -> Vec<Pane> {
        self.workspaces
            .iter()
            .find(|w| Some(w.id) == self.projects.active)
            .map(|w| w.activation_plan())
            .unwrap_or_default()
    }
}
