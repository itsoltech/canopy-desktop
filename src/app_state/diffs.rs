use super::AppState;
use crate::ui::diff::DiffView;
use canopy_desktop::state::workspace::{PaneId, PaneKind};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
pub struct Diffs {
    pub views: HashMap<PaneId, Entity<DiffView>>,
    observers: Vec<Subscription>,
}
impl Diffs {
    pub fn new() -> Self {
        Self {
            views: HashMap::new(),
            observers: vec![],
        }
    }
    pub fn bind(&mut self, app: &AppState, cx: &mut Context<Self>) {
        self.observers = vec![
            cx.observe(&app.workspace, |this, _, cx| this.sync(cx)),
            cx.observe(&app.projects, |this, _, cx| this.sync(cx)),
            cx.observe(&app.changes, |this, _, cx| this.sync(cx)),
        ];
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>();
        if !app.projects.read(cx).ready {
            return;
        }
        let all = app.projects.read(cx).runtime_workspaces(cx);
        let alive: HashSet<_> = all
            .iter()
            .flat_map(|w| w.all_panes())
            .filter(|p| p.metadata.kind == PaneKind::Diff)
            .map(|p| p.id)
            .collect();
        self.views.retain(|id, _| alive.contains(id));
        let active: Vec<_> = app
            .workspace
            .read(cx)
            .activation_plan()
            .into_iter()
            .filter(|p| p.metadata.kind == PaneKind::Diff)
            .collect();
        let revision = app
            .changes
            .read(cx)
            .data
            .as_ref()
            .map(|d| d.revision)
            .unwrap_or(0);
        for pane in &active {
            self.views
                .entry(pane.id)
                .or_insert_with(|| cx.new(|_| DiffView::new(pane.clone())));
        }
        for (id, view) in &self.views {
            view.update(cx, |view, cx| {
                if let Some(pane) = active.iter().find(|p| p.id == *id) {
                    view.update_pane(pane, cx);
                }
                view.update_visibility(active.iter().any(|p| p.id == *id), revision, cx)
            });
        }
        cx.notify();
    }
}
