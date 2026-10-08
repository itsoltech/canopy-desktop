use canopy_desktop::settings::{
    Access, Change, Error, PreferenceKey, Preferences, Schema, SettingsClient, ToolId,
};
use futures_lite::future::block_on;
use rusqlite::Connection;
use std::path::Path;

fn electron(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    db.execute_batch(include_str!("fixtures/electron-v11.sql"))
        .unwrap();
    db
}
fn raw(db: &Connection, key: &str) -> String {
    db.query_row("SELECT value FROM preferences WHERE key=?1", [key], |r| {
        r.get(0)
    })
    .unwrap()
}
#[test]
fn fresh_defaults_and_persistence_after_worker_restart() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native.db");
        let client = SettingsClient::create(&path).await.unwrap();
        assert_eq!(
            client.load().await.unwrap().preferences,
            Preferences::default()
        );
        let saved = client
            .apply(vec![
                Change::NotchEnabled(true),
                Change::NewTabTool(ToolId::new("custom-tool-42").unwrap()),
            ])
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let reopened = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
        assert_eq!(reopened.load().await.unwrap(), saved);
        assert!(matches!(
            reopened.apply(vec![Change::NotchEnabled(false)]).await,
            Err(Error::ReadOnly)
        ));
        reopened.shutdown().await.unwrap();
    });
}
#[test]
fn online_backup_includes_wal_and_preserves_unknown_and_secret_values() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("electron.db");
        let dest = dir.path().join("rust.db");
        let original = electron(&source);
        original.pragma_update(None, "journal_mode", "WAL").unwrap();
        original
            .pragma_update(None, "wal_autocheckpoint", 0)
            .unwrap();
        original
            .execute(
                "INSERT INTO preferences VALUES(?1,?2)",
                ["notch.enabled", "true"],
            )
            .unwrap();
        original
            .execute(
                "INSERT INTO preferences VALUES(?1,?2)",
                ["claude.apiKey", "opaque-encrypted-value"],
            )
            .unwrap();
        original
            .execute(
                "INSERT INTO preferences VALUES(?1,?2)",
                ["unknown.setting", " {\"keep\": [1, 2]} "],
            )
            .unwrap();
        assert!(source.with_extension("db-wal").exists());
        SettingsClient::import_electron(&source, &dest)
            .await
            .unwrap();
        let client = SettingsClient::open(&dest, Access::ReadWrite)
            .await
            .unwrap();
        let before = client.load().await.unwrap();
        assert_eq!(before.schema, Schema::Electron { migration: 11 });
        assert!(before.preferences.notch_enabled);
        assert!(
            !serde_json::to_string(&before)
                .unwrap()
                .contains("opaque-encrypted-value")
        );
        client
            .apply(vec![Change::NotchEnabled(false)])
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let copy = Connection::open(&dest).unwrap();
        assert_eq!(raw(&copy, "claude.apiKey"), raw(&original, "claude.apiKey"));
        assert_eq!(
            raw(&copy, "unknown.setting"),
            raw(&original, "unknown.setting")
        );
        assert_eq!(raw(&original, "notch.enabled"), "true");
        assert_eq!(raw(&copy, "notch.enabled"), "false");
        assert_eq!(
            copy.query_row("SELECT count(*) FROM _migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            11
        );
    });
}
#[test]
fn unsupported_schema_and_existing_destination_are_not_modified() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("future.db");
        let dest = dir.path().join("new.db");
        let db = electron(&source);
        for id in 12..=18 {
            db.execute("INSERT INTO _migrations(id) VALUES(?1)", [id])
                .unwrap();
        }
        drop(db);
        let before = std::fs::read(&source).unwrap();
        assert!(matches!(
            SettingsClient::import_electron(&source, &dest).await,
            Err(Error::UnsupportedSchema)
        ));
        assert!(matches!(
            SettingsClient::open(&source, Access::ReadWrite).await,
            Err(Error::UnsupportedSchema)
        ));
        assert_eq!(std::fs::read(&source).unwrap(), before);
        assert!(!dest.exists());
        let valid = dir.path().join("valid.db");
        drop(electron(&valid));
        std::fs::write(&dest, b"do not overwrite").unwrap();
        assert!(matches!(
            SettingsClient::import_electron(&valid, &dest).await,
            Err(Error::DestinationExists)
        ));
        assert_eq!(std::fs::read(&dest).unwrap(), b"do not overwrite");
        assert!(matches!(
            SettingsClient::import_electron(&valid, &valid).await,
            Err(Error::DestinationExists)
        ));
    });
}
#[test]
fn invalid_stored_values_fall_back_without_rewriting_them() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("electron.db");
        let db = electron(&path);
        db.execute("INSERT INTO preferences VALUES('notch.enabled','TRUE')", [])
            .unwrap();
        db.execute("INSERT INTO preferences VALUES('newTab.toolId','')", [])
            .unwrap();
        drop(db);
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        let snapshot = client.load().await.unwrap();
        assert_eq!(snapshot.warnings.len(), 2);
        assert!(!snapshot.preferences.notch_enabled);
        assert_eq!(snapshot.preferences.new_tab_tool.as_str(), "shell");
        client
            .apply(vec![Change::ResourceUsage(true)])
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let db = Connection::open(path).unwrap();
        assert_eq!(raw(&db, "notch.enabled"), "TRUE");
        assert_eq!(raw(&db, "newTab.toolId"), "");
    });
}
#[test]
fn batch_failure_rolls_back_all_earlier_changes_and_hides_trigger_message() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TRIGGER reject_notch BEFORE INSERT ON preferences WHEN NEW.key='notch.enabled' BEGIN SELECT RAISE(ABORT,'sensitive database message'); END;").unwrap();
        drop(db);
        let error = client
            .apply(vec![
                Change::ReopenLastWorkspace(false),
                Change::NotchEnabled(true),
            ])
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("sensitive"));
        assert!(
            client
                .load()
                .await
                .unwrap()
                .preferences
                .reopen_last_workspace
        );
        client.shutdown().await.unwrap();
    });
}
#[test]
fn cloned_handles_serialize_updates_and_reset_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.db");
    let client = block_on(SettingsClient::create(&path)).unwrap();
    let a = client.clone();
    let b = client.clone();
    let first =
        std::thread::spawn(move || block_on(a.apply(vec![Change::ReopenLastWorkspace(false)])));
    let second = std::thread::spawn(move || block_on(b.apply(vec![Change::NotchEnabled(true)])));
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    let p = block_on(client.load()).unwrap().preferences;
    assert!(!p.reopen_last_workspace);
    assert!(p.notch_enabled);
    assert!(
        block_on(client.apply(vec![Change::Reset(PreferenceKey::ReopenLastWorkspace)]))
            .unwrap()
            .preferences
            .reopen_last_workspace
    );
    block_on(client.shutdown()).unwrap();
    assert!(matches!(block_on(client.load()), Err(Error::WorkerStopped)));
}
#[test]
fn only_ordinary_known_keys_can_be_changed() {
    for key in [
        "claude.apiKey",
        "credential.secret.v2.x",
        "remote.trustedDevices",
        "unknown.setting",
    ] {
        assert!(key.parse::<PreferenceKey>().is_err());
    }
    assert!(Change::parse(PreferenceKey::NotchEnabled, "yes").is_err());
    assert!(ToolId::new(" ").is_err());
}

#[test]
fn malformed_preferences_and_incomplete_migrations_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.db");
    let db = Connection::open(&bad).unwrap();
    db.execute_batch("CREATE TABLE preferences(key INTEGER PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE _canopy_rust_meta(version); INSERT INTO _canopy_rust_meta VALUES(1);").unwrap();
    drop(db);
    assert!(matches!(
        block_on(SettingsClient::open(&bad, Access::ReadWrite)),
        Err(Error::InvalidSchema)
    ));
    let gap = dir.path().join("gap.db");
    let db = electron(&gap);
    db.execute("DELETE FROM _migrations WHERE id=3", [])
        .unwrap();
    drop(db);
    assert!(matches!(
        block_on(SettingsClient::open(&gap, Access::ReadWrite)),
        Err(Error::UnsupportedSchema)
    ));
}

#[test]
fn running_worker_rejects_external_schema_upgrade() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("electron.db");
        let external = electron(&path);
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        external
            .execute("INSERT INTO _migrations(id) VALUES(12)", [])
            .unwrap();
        assert!(matches!(client.load().await, Err(Error::UnsupportedSchema)));
        assert!(matches!(
            client.apply(vec![Change::NotchEnabled(true)]).await,
            Err(Error::UnsupportedSchema)
        ));
        let count: i64 = external
            .query_row("SELECT count(*) FROM preferences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        client.shutdown().await.unwrap();
    });
}
