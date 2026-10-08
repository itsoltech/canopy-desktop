use canopy_desktop::state::workspace::*;

#[test]
fn tabs_have_independent_trees_and_focus() {
    let mut ws = Workspace::new();
    let a = ws.open("shell", "shell");
    let original = ws.active().unwrap().focused;
    let second = ws.split(a, original, Axis::Horizontal, "codex").unwrap();
    let b = ws.open("other", "shell");
    ws.activate(a).unwrap();
    assert_eq!(ws.active().unwrap().focused, second);
    assert!(matches!(ws.active().unwrap().root, SplitTree::Split { .. }));
    ws.close(b).unwrap();
    assert_eq!(ws.active().unwrap().id, a);
    ws.close_pane(a, second).unwrap();
    assert_eq!(ws.active().unwrap().focused, original);
    assert!(matches!(ws.active().unwrap().root, SplitTree::Leaf(_)));
    ws.close_pane(a, original).unwrap();
    assert!(ws.tabs().is_empty());
    assert!(ws.active().is_none());
}
#[test]
fn nested_removal_preserves_sibling_identity_and_focus() {
    let mut ws = Workspace::new();
    let tab = ws.open("a", "shell");
    let a = ws.active().unwrap().focused;
    let b = ws.split(tab, a, Axis::Horizontal, "codex").unwrap();
    let c = ws.split(tab, b, Axis::Vertical, "shell").unwrap();
    ws.close_pane(tab, b).unwrap();
    assert_eq!(ws.active().unwrap().focused, c);
    assert!(ws.active().unwrap().root.find(a).is_some());
    assert!(ws.active().unwrap().root.find(c).is_some());
    ws.close_pane(tab, a).unwrap();
    assert_eq!(ws.active().unwrap().root.first().id, c);
}
#[test]
fn failed_operations_do_not_mutate_the_tree() {
    let mut ws = Workspace::new();
    let tab = ws.open("a", "shell");
    let pane = ws.active().unwrap().focused;
    let before = ws.active().unwrap().clone();
    assert_eq!(
        ws.split(tab, PaneId::new(), Axis::Horizontal, "shell"),
        Err(StateError::MissingPane)
    );
    assert_eq!(
        ws.close_pane(tab, PaneId::new()),
        Err(StateError::MissingPane)
    );
    assert_eq!(ws.focus(tab, PaneId::new()), Err(StateError::MissingPane));
    assert_eq!(ws.active().unwrap(), &before);
    for _ in 0..3 {
        ws.split(tab, pane, Axis::Horizontal, "shell").unwrap();
    }
    let before = ws.active().unwrap().clone();
    assert_eq!(
        ws.split(tab, pane, Axis::Vertical, "shell"),
        Err(StateError::DepthLimit)
    );
    assert_eq!(ws.active().unwrap(), &before);
}
#[test]
fn moving_tabs_preserves_split_tree_and_chooses_source_fallback() {
    let mut source = Workspace::new();
    let mut dest = Workspace::new();
    let first = source.open("first", "shell");
    let last = source.open("last", "codex");
    let pane = source.active().unwrap().focused;
    source.split(last, pane, Axis::Vertical, "shell").unwrap();
    let expected = source.active().unwrap().clone();
    source.move_tab(last, &mut dest).unwrap();
    assert_eq!(dest.active().unwrap(), &expected);
    assert_eq!(source.active().unwrap().id, first);
    assert_eq!(
        source.move_tab(last, &mut dest),
        Err(StateError::MissingTab)
    );
    assert_eq!(dest.tabs().len(), 1);
}
#[test]
fn ratios_are_bounded_and_nan_is_rejected() {
    let mut ws = Workspace::new();
    let tab = ws.open("a", "shell");
    ws.split(tab, ws.active().unwrap().focused, Axis::Horizontal, "shell")
        .unwrap();
    let SplitTree::Split { id, .. } = ws.active().unwrap().root else {
        panic!()
    };
    ws.set_ratio(tab, id, 4.).unwrap();
    let SplitTree::Split { ratio, .. } = ws.active().unwrap().root else {
        panic!()
    };
    assert_eq!(ratio, 0.9);
    assert_eq!(
        ws.set_ratio(tab, id, f32::NAN),
        Err(StateError::InvalidRatio)
    );
    assert_eq!(
        ws.set_ratio(tab, SplitId::new(), 0.5),
        Err(StateError::MissingSplit)
    );
}

#[test]
fn moving_a_session_is_atomic_and_removes_an_empty_source_tab() {
    let mut ws = Workspace::new();
    let source = ws.open("source", "codex");
    let pane = ws.active().unwrap().focused;
    let dest = ws.open("dest", "shell");
    let target = ws.active().unwrap().focused;
    assert_eq!(
        ws.move_pane(source, pane, dest, PaneId::new(), Axis::Horizontal),
        Err(StateError::MissingPane)
    );
    assert_eq!(ws.tabs().len(), 2);
    ws.move_pane(source, pane, dest, target, Axis::Vertical)
        .unwrap();
    assert_eq!(ws.tabs().len(), 1);
    assert_eq!(ws.active().unwrap().focused, pane);
    assert_eq!(ws.active().unwrap().root.find(pane).unwrap().tool, "codex");
}

#[test]
fn tab_to_panes_and_back_preserves_every_identity() {
    let mut ws = Workspace::new();
    let a = ws.open("a", "codex");
    let pane = ws.active().unwrap().focused;
    let extra = ws.split(a, pane, Axis::Horizontal, "shell").unwrap();
    let b = ws.open("b", "shell");
    let target = ws.active().unwrap().focused;
    ws.dock_tab(a, b, target, Axis::Vertical, true).unwrap();
    assert_eq!(ws.tabs().len(), 1);
    assert!(ws.active().unwrap().root.find(pane).is_some());
    assert!(ws.active().unwrap().root.find(extra).is_some());
    let detached = ws.detach_pane(b, pane).unwrap();
    assert_eq!(ws.tabs().len(), 2);
    assert_eq!(ws.active().unwrap().focused, pane);
    assert_eq!(ws.detach_pane(detached, pane).unwrap(), detached);
    assert_eq!(ws.tabs().len(), 2);
}
#[test]
fn swaps_work_inside_one_tree_and_across_tabs() {
    let mut ws = Workspace::new();
    let tab = ws.open("a", "codex");
    let a = ws.active().unwrap().focused;
    let b = ws.split(tab, a, Axis::Horizontal, "shell").unwrap();
    ws.swap_panes(tab, a, tab, b).unwrap();
    assert_eq!(ws.active().unwrap().root.first().id, b);
    let second = ws.open("b", "gemini");
    let c = ws.active().unwrap().focused;
    ws.swap_panes(tab, a, second, c).unwrap();
    assert_eq!(ws.active().unwrap().root.first().id, a);
    ws.activate(tab).unwrap();
    assert!(ws.active().unwrap().root.find(c).is_some());
    assert!(
        ws.active()
            .unwrap()
            .root
            .find(ws.active().unwrap().focused)
            .is_some()
    );
}
#[test]
fn redocking_within_tab_collapses_old_split_and_rejects_bad_drops() {
    let mut ws = Workspace::new();
    let tab = ws.open("a", "codex");
    let a = ws.active().unwrap().focused;
    let b = ws.split(tab, a, Axis::Horizontal, "shell").unwrap();
    ws.dock_pane(tab, b, tab, a, Axis::Vertical, true).unwrap();
    assert!(matches!(
        ws.active().unwrap().root,
        SplitTree::Split {
            axis: Axis::Vertical,
            ..
        }
    ));
    assert_eq!(ws.active().unwrap().root.first().id, b);
    let original = ws.active().unwrap().clone();
    assert_eq!(
        ws.dock_tab(tab, tab, a, Axis::Horizontal, false),
        Err(StateError::SameTab)
    );
    assert!(
        ws.dock_pane(tab, b, TabId::new(), a, Axis::Horizontal, false)
            .is_err()
    );
    assert_eq!(ws.active().unwrap(), &original);
}

#[test]
fn rename_targets_inactive_tab_without_changing_focus_or_panes() {
    let mut ws = Workspace::new();
    let first = ws.open("first", "shell");
    let pane = ws.active().unwrap().focused;
    ws.split(first, pane, Axis::Horizontal, "codex").unwrap();
    let tree = ws.active().unwrap().root.clone();
    let second = ws.open("second", "shell");
    ws.rename(first, "  Moja sesja  ").unwrap();
    assert_eq!(ws.active().unwrap().id, second);
    assert_eq!(ws.tabs()[0].title, "Moja sesja");
    assert_eq!(ws.tabs()[0].root, tree);
    assert_eq!(ws.rename(first, "  "), Err(StateError::InvalidTitle));
    assert_eq!(ws.tabs()[0].title, "Moja sesja");
    ws.close(first).unwrap();
    assert_eq!(ws.tabs().len(), 1);
    assert_eq!(ws.active().unwrap().id, second);
}
