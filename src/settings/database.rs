use super::{Change, Error, PreferenceKey, Result, Schema, Snapshot, contract};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::state::{projects::ProjectSnapshot, session::SessionSnapshot};

const MAX_MIGRATION: i64 = 11;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    ReadWrite,
}
#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub source_schema: Schema,
    pub destination: PathBuf,
}

fn connection(path: &Path, access: Access) -> Result<Connection> {
    let flags = if access == Access::ReadOnly {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let db = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    db.busy_timeout(Duration::from_secs(2))?;
    Ok(db)
}
fn has_table(db: &Connection, table: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}
fn schema(db: &Connection) -> Result<Schema> {
    if !has_table(db, "preferences")? {
        return Err(Error::InvalidSchema);
    }
    let columns = db
        .prepare("PRAGMA table_info(preferences)")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if columns.len() != 2
        || columns[0].0 != "key"
        || !columns[0].1.eq_ignore_ascii_case("TEXT")
        || columns[0].3 != 1
        || columns[1].0 != "value"
        || !columns[1].1.eq_ignore_ascii_case("TEXT")
        || columns[1].2 != 1
        || columns[1].3 != 0
    {
        return Err(Error::InvalidSchema);
    }
    if has_table(db, "_migrations")? {
        let ids = db
            .prepare("SELECT id FROM _migrations ORDER BY id LIMIT 12")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let Some(&latest) = ids.last() else {
            return Err(Error::UnsupportedSchema);
        };
        if !(1..=MAX_MIGRATION).contains(&latest) || ids != (1..=latest).collect::<Vec<_>>() {
            return Err(Error::UnsupportedSchema);
        }
        Ok(Schema::Electron { migration: latest })
    } else if has_table(db, "_canopy_rust_meta")? {
        let versions = db
            .prepare("SELECT version FROM _canopy_rust_meta LIMIT 2")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if versions != [1] {
            return Err(Error::UnsupportedSchema);
        }
        Ok(Schema::RustPreferencesV1)
    } else {
        Err(Error::UnsupportedSchema)
    }
}
fn verify_integrity(db: &Connection) -> Result<()> {
    let result: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if result != "ok" {
        return Err(Error::InvalidSchema);
    }
    Ok(())
}
fn temporary_destination(path: &Path) -> Result<tempfile::NamedTempFile> {
    if path.exists() {
        return Err(Error::DestinationExists);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut directory = std::fs::DirBuilder::new();
    directory.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(parent)?;
    Ok(tempfile::NamedTempFile::new_in(parent)?)
}
fn publish(file: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::DestinationExists
        } else {
            Error::Io(e.error)
        }
    })?;
    Ok(())
}

pub(super) fn create(path: &Path) -> Result<()> {
    let file = temporary_destination(path)?;
    let db = Connection::open(file.path())?;
    db.execute_batch("BEGIN; CREATE TABLE preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE _canopy_rust_meta(version INTEGER NOT NULL); INSERT INTO _canopy_rust_meta VALUES(1); COMMIT;")?;
    db.close().map_err(|(_, e)| Error::Sqlite(e))?;
    publish(file, path)
}
pub(super) fn import(source: &Path, destination: &Path) -> Result<ImportReport> {
    // Source is always opened read-only; no pragma or migration writes to it.
    let source_db = connection(source, Access::ReadOnly)?;
    let source_schema = schema(&source_db)?;
    if !matches!(source_schema, Schema::Electron { .. }) {
        return Err(Error::UnsupportedSchema);
    }
    verify_integrity(&source_db)?;
    let file = temporary_destination(destination)?;
    let mut copy = Connection::open(file.path())?;
    {
        let backup = Backup::new(&source_db, &mut copy)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if Instant::now() > deadline {
                return Err(Error::BackupTimeout);
            }
            match backup.step(128)? {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(25))
                }
                _ => return Err(Error::InvalidSchema),
            }
        }
    }
    verify_integrity(&copy)?;
    schema(&copy)?;
    // Publish a self-contained file, not a database with a temporary-name WAL.
    copy.pragma_update(None, "journal_mode", "DELETE")?;
    copy.close().map_err(|(_, e)| Error::Sqlite(e))?;
    publish(file, destination)?;
    Ok(ImportReport {
        source_schema,
        destination: destination.to_owned(),
    })
}

pub(super) struct Database {
    db: Connection,
    access: Access,
}
impl Database {
    pub fn open(path: &Path, access: Access) -> Result<Self> {
        let preflight = connection(path, Access::ReadOnly)?;
        schema(&preflight)?;
        verify_integrity(&preflight)?;
        drop(preflight);
        let db = connection(path, access)?;
        schema(&db)?;
        if access == Access::ReadWrite {
            db.pragma_update(None, "journal_mode", "WAL")?;
            db.pragma_update(None, "foreign_keys", true)?;
        }
        Ok(Self { db, access })
    }
    pub fn load(&self) -> Result<Snapshot> {
        let transaction = self.db.unchecked_transaction()?;
        let snapshot = read_snapshot(&transaction, schema(&transaction)?)?;
        transaction.commit()?;
        Ok(snapshot)
    }
    pub fn load_task_drafts(&self) -> Result<crate::integrations::drafts::TaskDrafts> {
        schema(&self.db)?;
        read_task_drafts(&self.db)
    }
    pub fn save_task_drafts(
        &mut self,
        drafts: crate::integrations::drafts::TaskDrafts,
    ) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if !crate::integrations::drafts::valid(&drafts) {
            return Err(Error::InvalidTaskDrafts);
        }
        let json = serde_json::to_string(&drafts).map_err(|_| Error::InvalidTaskDrafts)?;
        if json.len() > 8 * 1024 * 1024 {
            return Err(Error::InvalidTaskDrafts);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        schema(&tx)?;
        read_task_drafts(&tx)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS _canopy_rust_task_drafts(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,payload TEXT NOT NULL)")?;
        tx.execute("INSERT INTO _canopy_rust_task_drafts VALUES(1,1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[json])?;
        tx.commit()?;
        Ok(())
    }
    pub fn load_integrations(&self) -> Result<crate::integrations::Config> {
        schema(&self.db)?;
        read_integrations(&self.db)
    }
    pub fn save_integrations(&mut self, config: crate::integrations::Config) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if !config.valid() {
            return Err(Error::InvalidIntegrations);
        }
        let json = serde_json::to_string(&config).map_err(|_| Error::InvalidIntegrations)?;
        if json.len() > 2_097_152 {
            return Err(Error::InvalidIntegrations);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        schema(&tx)?;
        read_integrations(&tx)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS _canopy_rust_integrations(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,payload TEXT NOT NULL)")?;
        tx.execute("INSERT INTO _canopy_rust_integrations VALUES(1,1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[json])?;
        tx.commit()?;
        Ok(())
    }
    pub fn load_tools(&self) -> Result<crate::state::tools::ToolCatalog> {
        let tx = self.db.unchecked_transaction()?;
        schema(&tx)?;
        let catalog = read_tools(&tx)?;
        tx.commit()?;
        Ok(catalog)
    }
    pub fn save_tools(&mut self, catalog: crate::state::tools::ToolCatalog) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        catalog.validate().map_err(|_| Error::InvalidToolCatalog)?;
        let json = serde_json::to_string(&catalog).map_err(|_| Error::InvalidToolCatalog)?;
        if json.len() > 2_097_152 {
            return Err(Error::InvalidToolCatalog);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        schema(&tx)?;
        read_tools(&tx)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS _canopy_rust_tools(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,payload TEXT NOT NULL)")?;
        tx.execute("INSERT INTO _canopy_rust_tools VALUES(1,1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[json])?;
        tx.commit()?;
        Ok(())
    }
    pub fn load_session(&self) -> Result<Option<SessionSnapshot>> {
        let tx = self.db.unchecked_transaction()?;
        schema(&tx)?;
        let result = read_session(&tx)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn save_session(&mut self, snapshot: SessionSnapshot) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if !snapshot.valid() {
            return Err(Error::InvalidProjectState);
        }
        let json = serde_json::to_string(&snapshot).map_err(|_| Error::InvalidProjectState)?;
        if json.len() > 8_388_608 {
            return Err(Error::InvalidProjectState);
        }
        let projects = serde_json::to_string(&snapshot.projects.snapshot())
            .map_err(|_| Error::InvalidProjectState)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        schema(&tx)?;
        read_session(&tx)?;
        read_projects(&tx)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS _canopy_rust_session(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS _canopy_rust_projects(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL,payload TEXT NOT NULL);")?;
        tx.execute("INSERT INTO _canopy_rust_session VALUES(1,1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[json])?;
        tx.execute("INSERT INTO _canopy_rust_projects VALUES(1,1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[projects])?;
        tx.commit()?;
        Ok(())
    }
    pub fn load_projects(&self) -> Result<ProjectSnapshot> {
        let tx = self.db.unchecked_transaction()?;
        schema(&tx)?;
        let snapshot = read_projects(&tx)?;
        tx.commit()?;
        Ok(snapshot)
    }
    pub fn save_projects(&mut self, snapshot: ProjectSnapshot) -> Result<()> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if !snapshot.valid() {
            return Err(Error::InvalidProjectState);
        }
        let json = serde_json::to_string(&snapshot).map_err(|_| Error::InvalidProjectState)?;
        if json.len() > 1_048_576 {
            return Err(Error::InvalidProjectState);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        schema(&tx)?;
        // Once a full session exists, catalog-only writes would desynchronize it.
        if has_table(&tx, "_canopy_rust_session")? {
            return Err(Error::InvalidProjectState);
        }
        // Validate a pre-existing version before any write.
        read_projects(&tx)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS _canopy_rust_projects (id INTEGER PRIMARY KEY CHECK(id=1), version INTEGER NOT NULL, payload TEXT NOT NULL)")?;
        tx.execute("INSERT INTO _canopy_rust_projects(id,version,payload) VALUES(1,1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[json])?;
        tx.commit()?;
        Ok(())
    }
    pub fn apply(&mut self, changes: Vec<Change>) -> Result<Snapshot> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if changes.len() > 64 {
            return Err(Error::BatchTooLarge);
        }
        let transaction = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current_schema = schema(&transaction)?;
        for change in changes {
            let (key, value) = change.encoded();
            if let Some(value) = value {
                transaction.execute("INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[key.as_str(),value.as_str()])?;
            } else {
                transaction.execute("DELETE FROM preferences WHERE key=?1", [key.as_str()])?;
            }
        }
        let snapshot = read_snapshot(&transaction, current_schema)?;
        transaction.commit()?;
        Ok(snapshot)
    }
}

fn read_snapshot(db: &Connection, schema: Schema) -> Result<Snapshot> {
    let mut statement =
        db.prepare("SELECT key,value FROM preferences WHERE key IN (?1,?2,?3,?4,?5)")?;
    let rows = statement.query_map(
        rusqlite::params_from_iter(PreferenceKey::ALL.map(PreferenceKey::as_str)),
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    let mut values = Vec::new();
    for row in rows {
        let (key, value) = row?;
        values.push((key.parse::<PreferenceKey>()?, Some(value)));
    }
    Ok(contract::decode(schema, values))
}

fn read_projects(db: &Connection) -> Result<ProjectSnapshot> {
    if !has_table(db, "_canopy_rust_projects")? {
        return Ok(ProjectSnapshot::default());
    }
    let rows = db
        .prepare("SELECT id,version,payload FROM _canopy_rust_projects LIMIT 2")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(ProjectSnapshot::default());
    }
    if rows.len() != 1 || rows[0].0 != 1 || rows[0].1 != 1 || rows[0].2.len() > 1_048_576 {
        return Err(Error::InvalidProjectState);
    }
    let snapshot: ProjectSnapshot =
        serde_json::from_str(&rows[0].2).map_err(|_| Error::InvalidProjectState)?;
    if !snapshot.valid() {
        return Err(Error::InvalidProjectState);
    }
    Ok(snapshot)
}

fn read_session(db: &Connection) -> Result<Option<SessionSnapshot>> {
    if !has_table(db, "_canopy_rust_session")? {
        return Ok(None);
    }
    let rows = db
        .prepare("SELECT id,version,payload FROM _canopy_rust_session LIMIT 2")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() != 1 || rows[0].0 != 1 || rows[0].1 != 1 || rows[0].2.len() > 8_388_608 {
        return Err(Error::InvalidProjectState);
    }
    let snapshot: SessionSnapshot =
        serde_json::from_str(&rows[0].2).map_err(|_| Error::InvalidProjectState)?;
    if !snapshot.valid() {
        return Err(Error::InvalidProjectState);
    }
    Ok(Some(snapshot))
}

fn read_tools(db: &Connection) -> Result<crate::state::tools::ToolCatalog> {
    if !has_table(db, "_canopy_rust_tools")? {
        return Ok(Default::default());
    }
    let rows = db
        .prepare("SELECT id,version,payload FROM _canopy_rust_tools LIMIT 2")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 1 || rows[0].0 != 1 || rows[0].1 != 1 || rows[0].2.len() > 2_097_152 {
        return Err(Error::InvalidToolCatalog);
    }
    let mut catalog: crate::state::tools::ToolCatalog =
        serde_json::from_str(&rows[0].2).map_err(|_| Error::InvalidToolCatalog)?;
    catalog.migrate_platform_defaults();
    catalog.validate().map_err(|_| Error::InvalidToolCatalog)?;
    Ok(catalog)
}

fn read_integrations(db: &Connection) -> Result<crate::integrations::Config> {
    if !has_table(db, "_canopy_rust_integrations")? {
        return Ok(Default::default());
    }
    let rows = db
        .prepare("SELECT id,version,payload FROM _canopy_rust_integrations LIMIT 2")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(Default::default());
    }
    if rows.len() != 1 || rows[0].0 != 1 || rows[0].1 != 1 || rows[0].2.len() > 2_097_152 {
        return Err(Error::InvalidIntegrations);
    }
    let config: crate::integrations::Config =
        serde_json::from_str(&rows[0].2).map_err(|_| Error::InvalidIntegrations)?;
    if !config.valid() {
        return Err(Error::InvalidIntegrations);
    }
    Ok(config)
}

fn read_task_drafts(db: &Connection) -> Result<crate::integrations::drafts::TaskDrafts> {
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='_canopy_rust_task_drafts')",[],|row|row.get(0))?;
    if !exists {
        return Ok(Default::default());
    }
    let mut statement = db.prepare("SELECT id,version,payload FROM _canopy_rust_task_drafts")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(Default::default());
    }
    if rows.len() != 1 || rows[0].0 != 1 || rows[0].1 != 1 || rows[0].2.len() > 8 * 1024 * 1024 {
        return Err(Error::InvalidTaskDrafts);
    }
    let drafts = serde_json::from_str(&rows[0].2).map_err(|_| Error::InvalidTaskDrafts)?;
    if !crate::integrations::drafts::valid(&drafts) {
        return Err(Error::InvalidTaskDrafts);
    }
    Ok(drafts)
}
