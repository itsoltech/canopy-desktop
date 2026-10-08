//! Pure lazy-start/reconciliation policy, independent of GPUI and PTY handles.
use crate::state::workspace::{Pane, PaneId, PaneKind, Workspace, WorkspaceId};
use std::collections::HashSet;
pub struct Plan {
    pub start: Vec<Pane>,
    pub remove: Vec<PaneId>,
    pub visible: HashSet<PaneId>,
    pub focused: Option<PaneId>,
}
pub fn reconcile(
    workspaces: &[Workspace],
    active: Option<WorkspaceId>,
    started: &HashSet<PaneId>,
) -> Plan {
    let alive: HashSet<_> = workspaces
        .iter()
        .flat_map(|w| w.all_panes())
        .map(|p| p.id)
        .collect();
    let workspace = workspaces.iter().find(|w| Some(w.id) == active);
    let panes = workspace.map(|w| w.activation_plan()).unwrap_or_default();
    let visible = panes.iter().map(|p| p.id).collect();
    Plan {
        start: panes
            .into_iter()
            .filter(|p| p.metadata.kind == PaneKind::Terminal && !started.contains(&p.id))
            .collect(),
        remove: started.difference(&alive).copied().collect(),
        visible,
        focused: workspace.and_then(|w| w.active()).map(|t| t.focused),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::workspace::Axis;
    #[test]
    fn restore_starts_only_active_tab_then_lazily_starts_other_tabs_once() {
        let mut a = Workspace::new();
        let first = a.open("first", "shell");
        let first_pane = a.active().unwrap().focused;
        a.split(first, first_pane, Axis::Horizontal, "codex")
            .unwrap();
        let second = a.open("second", "shell");
        a.activate(first).unwrap();
        let mut b = Workspace::new();
        b.open("other project", "shell");
        let mut workspaces = vec![a, b];
        let first_plan = reconcile(&workspaces, Some(workspaces[0].id), &HashSet::new());
        assert_eq!(first_plan.start.len(), 2);
        let started = first_plan.start.iter().map(|p| p.id).collect();
        assert!(
            reconcile(&workspaces, Some(workspaces[0].id), &started)
                .start
                .is_empty()
        );
        workspaces[0].activate(second).unwrap();
        let next = reconcile(&workspaces, Some(workspaces[0].id), &started);
        assert_eq!(next.start.len(), 1);
        assert!(next.remove.is_empty());
        workspaces[0].close(first).unwrap();
        assert_eq!(
            reconcile(&workspaces, Some(workspaces[0].id), &started)
                .remove
                .len(),
            2
        );
    }
    #[test]
    fn moving_a_pane_does_not_restart_it() {
        let mut w = Workspace::new();
        let a = w.open("a", "shell");
        let pane = w.active().unwrap().focused;
        let b = w.open("b", "shell");
        let target = w.active().unwrap().focused;
        let started = HashSet::from([pane, target]);
        w.dock_pane(a, pane, b, target, Axis::Horizontal, false)
            .unwrap();
        let plan = reconcile(&[w.clone()], Some(w.id), &started);
        assert!(plan.start.is_empty());
        assert!(plan.remove.is_empty());
        assert_eq!(plan.visible.len(), 2);
    }
}
