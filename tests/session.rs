use canopy_desktop::{
    settings::{Access, Error, SettingsClient},
    state::{layout::LayoutState, projects::Projects, session::SessionSnapshot, workspace::*},
};
use futures_lite::future::block_on;
fn fixture() -> SessionSnapshot {
    let mut projects = Projects::default();
    let a = projects.open("/repo/a".into());
    let b = projects.open("/repo/b".into());
    projects.select(a);
    projects.items[0].worktree_path = Some("/repo/a-worktree".into());
    let mut first = Workspace::new();
    first.id = a;
    let tab = first.open("keep title", "codex");
    let p = first.active().unwrap().focused;
    let second = first.split(tab, p, Axis::Horizontal, "shell").unwrap();
    first.split(tab, second, Axis::Vertical, "claude").unwrap();
    first
        .set_pane_metadata(
            tab,
            p,
            PaneMetadata {
                task_prompt: Some("Task description\nComment\n@'/tmp/image one.png'".into()),
                cwd: Some("/repo/a-worktree/sub".into()),
                profile_id: Some("profile-a".into()),
                title: Some("Agent".into()),
                resource: Some("src/main.rs".into()),
                resume_id: Some("provider-session".into()),
                arguments: vec!["--example".into()],
                kind: PaneKind::Terminal,
            },
        )
        .unwrap();
    first.open("sleeping tab", "gemini");
    first.activate(tab).unwrap();
    first.set_default_cwd("/repo/a-worktree".into());
    let mut other = Workspace::new();
    other.id = b;
    other.open("other project", "shell");
    other.set_default_cwd("/repo/b".into());
    let mut layout = LayoutState::default();
    layout.widths.left = 250.;
    layout.widths.right = 310.;
    layout.sidebar_open = false;
    layout.inspector_changes = true;
    SessionSnapshot {
        projects,
        workspaces: vec![first, other],
        layout,
    }
}
#[test]
fn exact_roundtrip_preserves_order_identity_layout_and_pane_metadata() {
    let original = fixture();
    assert!(original.valid());
    let json = serde_json::to_string(&original).unwrap();
    let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(original, restored);
    assert_eq!(restored.activation_plan().len(), 3);
    assert!(
        restored
            .activation_plan()
            .iter()
            .all(|p| p.tool != "gemini")
    );
    assert_eq!(
        restored.activation_plan()[0].metadata.resume_id.as_deref(),
        Some("provider-session")
    );
}
#[test]
fn activation_is_lazy_and_changes_only_when_a_tab_is_selected() {
    let mut s = fixture();
    let dormant = s.workspaces[0].tabs()[1].id;
    s.workspaces[0].activate(dormant).unwrap();
    assert_eq!(s.activation_plan().len(), 1);
    assert_eq!(s.activation_plan()[0].tool, "gemini");
    s.projects.select(s.workspaces[1].id);
    assert_eq!(s.activation_plan()[0].tool, "shell");
}
#[test]
fn restored_ids_are_reserved_before_creating_new_objects() {
    let restored: PaneId = serde_json::from_str("9000000").unwrap();
    let next = PaneId::new();
    assert_ne!(next, restored);
    assert!(serde_json::to_value(next).unwrap().as_u64().unwrap() > 9000000);
    assert!(serde_json::from_str::<PaneId>("0").is_err());
}
#[test]
fn invalid_focus_duplicate_ids_ratios_and_metadata_are_rejected() {
    let s = fixture();
    let mut bad = serde_json::to_value(&s).unwrap();
    bad["workspaces"][0]["active"] = serde_json::json!(12345678);
    assert!(
        !serde_json::from_value::<SessionSnapshot>(bad)
            .unwrap()
            .valid()
    );
    let mut bad = s.clone();
    bad.workspaces.push(bad.workspaces[0].clone());
    assert!(!bad.valid());
    let mut bad = s.clone();
    bad.layout.widths.left = f32::NAN;
    assert!(!bad.valid());
    let mut bad = s.clone();
    bad.layout.widths.right = 801.;
    assert!(!bad.valid());
    let mut bad = serde_json::to_value(&s).unwrap();
    bad["workspaces"][0]["tabs"][0]["root"]["Split"]["ratio"] = serde_json::json!(1.1);
    assert!(
        !serde_json::from_value::<SessionSnapshot>(bad)
            .unwrap()
            .valid()
    );
}
#[test]
fn sqlite_restores_complete_session_after_worker_restart() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let client = SettingsClient::create(&path).await.unwrap();
        assert!(client.load_session().await.unwrap().is_none());
        let mut s = fixture();
        s.layout.widths.left = 800.;
        s.layout.widths.right = 720.;
        client.save_session(s.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
        assert_eq!(client.load_session().await.unwrap(), Some(s.clone()));
        assert_eq!(client.load_projects().await.unwrap(), s.projects.snapshot());
        assert!(matches!(client.save_session(s).await, Err(Error::ReadOnly)));
        client.shutdown().await.unwrap();
    });
}
#[test]
fn failed_atomic_write_keeps_previous_catalog_and_session() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let client = SettingsClient::create(&path).await.unwrap();
        let old = fixture();
        client.save_session(old.clone()).await.unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TRIGGER reject_catalog BEFORE UPDATE ON _canopy_rust_projects BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
        let mut new = old.clone();
        new.layout.inspector_open = false;
        assert!(client.save_session(new).await.is_err());
        assert_eq!(client.load_session().await.unwrap(), Some(old.clone()));
        assert_eq!(
            client.load_projects().await.unwrap(),
            old.projects.snapshot()
        );
        client.shutdown().await.unwrap();
    });
}
#[test]
fn future_version_and_corrupt_payload_are_not_overwritten() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let client = SettingsClient::create(&path).await.unwrap();
        client.save_session(fixture()).await.unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("UPDATE _canopy_rust_session SET version=2", [])
            .unwrap();
        assert!(client.load_session().await.is_err());
        assert!(client.save_session(fixture()).await.is_err());
        db.execute(
            "UPDATE _canopy_rust_session SET version=1,payload='broken'",
            [],
        )
        .unwrap();
        assert!(client.load_session().await.is_err());
        assert!(client.save_session(fixture()).await.is_err());
        client.shutdown().await.unwrap();
    });
}

#[test]
fn optional_worktree_base_roundtrips_and_old_sessions_default_to_none() {
    let mut snapshot = fixture();
    let project = snapshot.projects.items.first_mut().unwrap();
    project.worktree_base = Some(canopy_desktop::state::projects::WorktreeBase {
        reference: "main".into(),
        oid: "0123456789012345678901234567890123456789".into(),
    });
    let expected = project.worktree_base.clone();
    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.projects.items[0].worktree_base, expected);
    assert!(restored.valid());

    let mut legacy = serde_json::to_value(fixture()).unwrap();
    legacy["projects"]["items"][0]
        .as_object_mut()
        .unwrap()
        .remove("worktree_base");
    let restored: SessionSnapshot = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.projects.items[0].worktree_base, None);
    assert!(restored.valid());
}
