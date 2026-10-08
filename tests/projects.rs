use canopy_desktop::{
    settings::{Access, Error, SettingsClient},
    state::projects::{ProjectSnapshot, Projects, canonical_directory, restore},
};
use futures_lite::future::block_on;

#[test]
fn canonical_aliases_activate_one_project_and_closing_selects_neighbor() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let mut projects = Projects::default();
    let first = projects.open(canonical_directory(&a).unwrap());
    let second = projects.open(canonical_directory(&b).unwrap());
    let reopened = projects.open(canonical_directory(&a.join(".")).unwrap());
    assert_eq!(first, reopened);
    assert_eq!(projects.items.len(), 2);
    assert!(projects.close(first));
    assert_eq!(projects.active, Some(second));
    assert!(projects.close(second));
    assert!(projects.current().is_none());
    assert!(projects.snapshot().valid());
}

#[cfg(unix)]
#[test]
fn symlinks_resolve_to_the_same_project() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("alias");
    std::os::unix::fs::symlink(dir.path(), &link).unwrap();
    assert_eq!(
        canonical_directory(dir.path()).unwrap(),
        canonical_directory(&link).unwrap()
    );
}
#[test]
fn files_and_missing_folders_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, "").unwrap();
    assert!(canonical_directory(&file).is_err());
    assert!(canonical_directory(&dir.path().join("missing")).is_err());
}
#[test]
fn restore_skips_missing_folders_and_preserves_active_selection() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let saved = ProjectSnapshot {
        paths: vec![a.clone(), dir.path().join("missing"), b],
        active: Some(a.clone()),
    };
    let (projects, skipped) = restore(saved);
    assert_eq!(skipped, 1);
    assert_eq!(projects.items.len(), 2);
    assert_eq!(
        projects.current().unwrap().path,
        canonical_directory(&a).unwrap()
    );
}
#[test]
fn snapshot_rejects_duplicates_relative_paths_and_invalid_selection() {
    let a = std::path::PathBuf::from("/a");
    assert!(
        !ProjectSnapshot {
            paths: vec![a.clone(), a.clone()],
            active: Some(a.clone())
        }
        .valid()
    );
    assert!(
        !ProjectSnapshot {
            paths: vec!["relative".into()],
            active: Some("relative".into())
        }
        .valid()
    );
    assert!(
        !ProjectSnapshot {
            paths: vec![a.clone()],
            active: None
        }
        .valid()
    );
    assert!(
        !ProjectSnapshot {
            paths: vec![],
            active: Some(a)
        }
        .valid()
    );
}

#[test]
fn sqlite_project_selection_survives_restart_and_preserves_electron_data() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(include_str!("fixtures/electron-v11.sql"))
            .unwrap();
        db.execute("INSERT INTO preferences VALUES('opaque','untouched')", [])
            .unwrap();
        let snapshot = ProjectSnapshot {
            paths: vec![dir.path().to_owned()],
            active: Some(dir.path().to_owned()),
        };
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert_eq!(
            client.load_projects().await.unwrap(),
            ProjectSnapshot::default()
        );
        client.save_projects(snapshot.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
        assert_eq!(client.load_projects().await.unwrap(), snapshot);
        assert!(matches!(
            client.save_projects(ProjectSnapshot::default()).await,
            Err(Error::ReadOnly)
        ));
        let value: String = db
            .query_row(
                "SELECT value FROM preferences WHERE key='opaque'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, "untouched");
        let count: i64 = db
            .query_row("SELECT count(*) FROM _migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 11);
        client.shutdown().await.unwrap();
    });
}
#[test]
fn future_project_version_and_corrupt_state_cannot_be_overwritten() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let client = SettingsClient::create(&path).await.unwrap();
        client
            .save_projects(ProjectSnapshot::default())
            .await
            .unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("UPDATE _canopy_rust_projects SET version=2", [])
            .unwrap();
        assert!(matches!(
            client.load_projects().await,
            Err(Error::InvalidProjectState)
        ));
        assert!(matches!(
            client.save_projects(ProjectSnapshot::default()).await,
            Err(Error::InvalidProjectState)
        ));
        db.execute(
            "UPDATE _canopy_rust_projects SET version=1,payload='broken'",
            [],
        )
        .unwrap();
        assert!(matches!(
            client.load_projects().await,
            Err(Error::InvalidProjectState)
        ));
        assert!(matches!(
            client.save_projects(ProjectSnapshot::default()).await,
            Err(Error::InvalidProjectState)
        ));
        client.shutdown().await.unwrap();
    });
}
