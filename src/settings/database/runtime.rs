//! The one runtime migration and receipts, on the existing SQLite worker.
use super::{Database, has_table};
use crate::remote::metrics::{self, Outcome, Stage};
use crate::{
    host::{
        self, ChatQueue, ChatRecord, CommandInput, CommandKey, CommandReceipt, CommandState,
        DeviceRegistration, PersistedThread, QueueDraft, QueueEntry, QueueMutation, QueueState,
        RuntimeHost, UnknownResolution,
    },
    settings::{Access, Error, Result},
    thread::{HostId, ThreadId, ThreadRef},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
mod authorization;
mod bus;
mod chat_commands;
mod files;
mod presence;
mod recovery;

const SCHEMA: &str = include_str!("runtime.sql");
const VERSION: i64 = 2;
fn publication() -> &'static str {
    if cfg!(feature = "published-runtime-schema") {
        "published"
    } else {
        "prerelease"
    }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs().min(i64::MAX as u64) as i64)
}
fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::InvalidRuntimeState)
}
fn unsigned(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::InvalidRuntimeState)
}

pub(super) fn validate_schema(db: &Connection) -> Result<()> {
    if !has_table(db, "_canopy_runtime_meta")? {
        let partial: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name IN ('_canopy_runtime_host','_canopy_runtime_chats','_canopy_thread_alias','_canopy_command_inbox'))",[],|r|r.get(0))?;
        return if partial {
            Err(Error::UnsupportedSchema)
        } else {
            Ok(())
        };
    }
    let rows = db
        .prepare("SELECT id,version,publication,fingerprint FROM _canopy_runtime_meta LIMIT 2")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 1
        || rows[0].0 != 1
        || rows[0].1 != VERSION
        || rows[0].2 != publication()
        || rows[0].3 != SCHEMA
    {
        return Err(Error::UnsupportedSchema);
    }
    Ok(())
}

fn require_host(db: &Database) -> Result<()> {
    if db.access == Access::ReadOnly {
        return Err(Error::ReadOnly);
    }
    if db.runtime_lease.is_none() {
        return Err(Error::InvalidRuntimeState);
    }
    validate_schema(&db.db)
}

fn authorize(db: &Connection, key: &CommandKey, scope: u32, check_epoch: bool) -> Result<()> {
    if key.device == host::LOCAL_PRINCIPAL {
        return if key.epoch == 0 {
            Ok(())
        } else {
            Err(Error::NotAuthorized)
        };
    }
    let device: Option<(i64, u32, Option<i64>)> = db
        .query_row(
            "SELECT command_epoch,scopes,revoked_at FROM _canopy_remote_devices WHERE device_id=?1",
            [&key.device],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((epoch, scopes, None)) = device else {
        return Err(Error::NotAuthorized);
    };
    if scopes & scope != scope {
        return Err(Error::NotAuthorized);
    }
    if check_epoch && integer(key.epoch)? != epoch {
        return Err(Error::CommandRetired);
    }
    Ok(())
}

fn authorize_root(
    db: &Connection,
    key: &CommandKey,
    scope: u32,
    root: &str,
    check_epoch: bool,
) -> Result<()> {
    authorize(db, key, scope, check_epoch)?;
    if key.device != host::LOCAL_PRINCIPAL && root.is_empty() {
        return Err(Error::NotAuthorized);
    }
    if key.device == host::LOCAL_PRINCIPAL {
        return Ok(());
    }
    let roots: String = db.query_row(
        "SELECT roots FROM _canopy_remote_devices WHERE device_id=?1",
        [&key.device],
        |r| r.get(0),
    )?;
    let roots: Vec<String> =
        serde_json::from_str(&roots).map_err(|_| Error::InvalidRuntimeState)?;
    if !roots.iter().any(|grant| grant == "*" || grant == root) {
        return Err(Error::NotAuthorized);
    }
    Ok(())
}
fn authorize_saved(db: &Connection, key: &CommandKey, check_epoch: bool) -> Result<()> {
    let (scope,root,method,arguments):(u32,String,String,Option<Vec<u8>>)=db.query_row("SELECT required_scope,required_root,method,arguments FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND command_epoch=?3 AND resource=?4 AND executor_epoch=?5",params![key.device,key.id,integer(key.epoch)?,key.resource,integer(key.executor_epoch)?],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?.ok_or(Error::NotFound)?;
    authorize_root(db, key, scope, &root, check_epoch)?;
    if method.starts_with("git.") {
        let saved = crate::command_bus::git::SavedAuthority::read(
            arguments.as_deref().ok_or(Error::NotAuthorized)?,
            &root,
        )?;
        for root in saved.roots {
            authorize_root(db, key, scope, &root, check_epoch)?;
        }
    }
    Ok(())
}
fn command_root(input: &CommandInput) -> Result<&str> {
    input
        .root
        .as_deref()
        .or_else(|| (input.key.device == host::LOCAL_PRINCIPAL).then_some("private"))
        .ok_or(Error::NotAuthorized)
}
fn chat_root(db: &Connection, chat: &str, key: &CommandKey) -> Result<String> {
    let root: Option<String> = db
        .query_row(
            "SELECT authority_root FROM _canopy_runtime_chats WHERE chat_id=?1",
            [chat],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(root) = root {
        return Ok(root);
    }
    // A retained receipt remains readable after the conversation was deleted.
    db.query_row("SELECT required_root FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND command_epoch=?3 AND resource=?4 AND executor_epoch=?5",params![key.device,key.id,integer(key.epoch)?,key.resource,integer(key.executor_epoch)?],|r|r.get(0)).optional()?.ok_or(Error::NotFound)
}

fn receipt(db: &Connection, key: &CommandKey) -> Result<Option<CommandReceipt>> {
    type ReceiptRow = (String, Option<Vec<u8>>, i64, bool, String, String);
    let row:Option<ReceiptRow>=db.query_row(
        "SELECT state,result,command_epoch,resolved_by_user,method,target FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND resource=?3 AND executor_epoch=?4",
        params![key.device,key.id,key.resource,integer(key.executor_epoch)?],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    row.map(|(state, result, epoch, resolved, method, target)| {
        if epoch != integer(key.epoch)? {
            return Err(Error::CommandConflict);
        }
        Ok(CommandReceipt {
            key: key.clone(),
            state: CommandState::parse(&state).ok_or(Error::InvalidRuntimeState)?,
            result,
            resolved,
            method,
            target,
        })
    })
    .transpose()
}

/// Legacy RPCs do not carry CMD-01's resource/executor key. Never select an
/// arbitrary match. Authorize every retained row before disclosing ambiguity.
fn legacy_command_key(db: &Connection, key: CommandKey, scope: u32) -> Result<CommandKey> {
    authorize(db, &key, scope, false)?;
    if !key.resource.is_empty() {
        let epoch: Option<i64> = db.query_row(
            "SELECT command_epoch FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND resource=?3 AND executor_epoch=?4",
            params![key.device,key.id,key.resource,integer(key.executor_epoch)?], |r| r.get(0),
        ).optional()?;
        if let Some(epoch) = epoch {
            let mut saved = key.clone();
            saved.epoch = unsigned(epoch)?;
            authorize_saved(db, &saved, false)?;
            if saved.epoch != key.epoch {
                return Err(Error::CommandConflict);
            }
        }
        return Ok(key);
    }
    let rows = db.prepare("SELECT resource,executor_epoch,command_epoch FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2")?
        .query_map(params![key.device,key.id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut matches = Vec::new();
    for (resource, executor_epoch, epoch) in rows {
        let mut saved = key.clone();
        saved.resource = resource;
        saved.executor_epoch = unsigned(executor_epoch)?;
        saved.epoch = unsigned(epoch)?;
        authorize_saved(db, &saved, false)?;
        matches.push(saved);
    }
    if matches.len() > 1 {
        return Err(Error::AmbiguousCommand);
    }
    if let Some(saved) = matches.pop() {
        if saved.epoch != key.epoch {
            return Err(Error::CommandConflict);
        }
        return Ok(saved);
    }
    Ok(key)
}

fn existing(db: &Connection, input: &CommandInput) -> Result<Option<CommandReceipt>> {
    type StoredIntent = (String, String, Vec<u8>, i64, Option<Vec<u8>>, String);
    let prior:Option<StoredIntent>=db.query_row(
        "SELECT method,target,fingerprint,command_epoch,arguments,required_root FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND resource=?3 AND executor_epoch=?4",
        params![input.key.device,input.key.id,input.key.resource,integer(input.key.executor_epoch)?],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    if let Some((method, target, fingerprint, epoch, arguments, root)) = prior {
        authorize_saved(db, &input.key, true)?;
        if root != command_root(input)?
            || if method.starts_with("git.") {
                !arguments
                    .as_deref()
                    .is_some_and(|saved| crate::command_bus::git::same_arguments(saved, input))
            } else if crate::command_bus::tasks::requires_account(&method) {
                !arguments
                    .as_deref()
                    .is_some_and(|saved| crate::command_bus::tasks::same_arguments(saved, input))
            } else {
                arguments.as_deref() != Some(input.arguments.as_slice())
            }
            || method != input.method
            || target != input.target
            || fingerprint != input.fingerprint
            || epoch != integer(input.key.epoch)?
        {
            return Err(Error::CommandConflict);
        }
        return receipt(db, &input.key);
    }
    Ok(None)
}

fn insert_command(
    tx: &Transaction,
    input: &CommandInput,
    state: CommandState,
    result: Option<&[u8]>,
    scope: u32,
) -> Result<CommandReceipt> {
    let (count,bytes):(i64,i64)=tx.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(resource AS BLOB))+length(CAST(method AS BLOB))+length(CAST(target AS BLOB))+length(CAST(required_root AS BLOB))+length(fingerprint)+coalesce(length(arguments),0)+coalesce(length(result),0)+512),0) FROM _canopy_command_inbox WHERE device_id=?1",[&input.key.device],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let added = (input.key.resource.len()
        + input.method.len()
        + input.target.len()
        + command_root(input)?.len()
        + input.fingerprint.len()
        + input.arguments.len()
        + result.map_or(0, |r| r.len())
        + 512) as u64;
    let limit = if input.key.device == host::LOCAL_PRINCIPAL {
        host::LOCAL_INBOX_BYTES
    } else {
        host::REMOTE_INBOX_BYTES
    };
    if unsigned(bytes)?.saturating_add(added) > limit
        || input.key.device != host::LOCAL_PRINCIPAL
            && unsigned(count)? >= host::REMOTE_INBOX_RECORDS
    {
        return Err(Error::ResourceLimit);
    }
    let at = now();
    tx.execute("INSERT INTO _canopy_command_inbox(device_id,command_id,command_epoch,method,target,fingerprint,arguments,state,result,created_at,updated_at,required_scope,required_root,resource,executor_epoch) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?12,?13,?14)",
        params![input.key.device,input.key.id,integer(input.key.epoch)?,input.method,input.target,input.fingerprint,input.arguments,state.key(),result,at,scope,command_root(input)?,input.key.resource,integer(input.key.executor_epoch)?])?;
    receipt(tx, &input.key)?.ok_or(Error::InvalidRuntimeState)
}

fn check_result_budget(db: &Connection, key: &CommandKey, result: Option<&[u8]>) -> Result<()> {
    let bytes:i64=db.query_row("SELECT coalesce(sum(length(CAST(resource AS BLOB))+length(CAST(method AS BLOB))+length(CAST(target AS BLOB))+length(CAST(required_root AS BLOB))+length(fingerprint)+coalesce(length(arguments),0)+CASE WHEN command_id=?2 AND resource=?3 AND executor_epoch=?4 THEN 0 ELSE coalesce(length(result),0) END+512),0) FROM _canopy_command_inbox WHERE device_id=?1",params![key.device,key.id,key.resource,integer(key.executor_epoch)?],|r|r.get(0))?;
    let limit = if key.device == host::LOCAL_PRINCIPAL {
        host::LOCAL_INBOX_BYTES
    } else {
        host::REMOTE_INBOX_BYTES
    };
    if unsigned(bytes)?.saturating_add(result.map_or(0, |v| v.len()) as u64) > limit {
        return Err(Error::ResourceLimit);
    }
    Ok(())
}

impl Database {
    pub fn initialize_runtime(&mut self) -> Result<RuntimeHost> {
        if self.access == Access::ReadOnly {
            return Err(Error::ReadOnly);
        }
        if self.runtime_lease.is_some() {
            return self.runtime_host();
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lease = options.open(self.path.with_extension("runtime-lock"))?;
        lease.try_lock().map_err(|_| Error::RuntimeInUse)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::schema(&tx)?;
        if !has_table(&tx, "_canopy_runtime_meta")? {
            tx.execute_batch(SCHEMA)?;
            tx.execute(
                "INSERT INTO _canopy_runtime_host(id,host_id) VALUES(1,?1)",
                [uuid::Uuid::new_v4().to_string()],
            )?;
            adopt(&tx)?;
            if has_table(&tx, "_canopy_rust_chat_epochs")? {
                tx.execute("UPDATE _canopy_runtime_chats SET history_epoch=coalesce((SELECT max(epoch) FROM _canopy_rust_chat_epochs WHERE chat=chat_id),0)", [])?;
            }
            if has_table(&tx, "_canopy_rust_chat_items")? {
                tx.execute("UPDATE _canopy_runtime_chats SET history_epoch=max(history_epoch,coalesce((SELECT max(epoch) FROM _canopy_rust_chat_items WHERE chat=chat_id),0))", [])?;
            }
            // Commit the marker in the same transaction as DDL and adoption.
            tx.execute(
                "INSERT INTO _canopy_runtime_meta VALUES(1,?1,?2,?3)",
                params![VERSION, publication(), SCHEMA],
            )?;
        }
        authorization::upgrade_default_sharing(&tx)?;
        tx.execute("UPDATE _canopy_command_inbox SET state='delivery_unknown',updated_at=?1 WHERE state='attempting'",[now()])?;
        // Catalog effects belong to the previous app owner and its ephemeral
        // preflight. They never crossed the attempt fence, so retire that intent
        // with a known refusal instead of leaving an ownerless Accepted forever.
        tx.execute(
            "UPDATE _canopy_command_inbox SET state='rejected',result=?1,updated_at=?2 WHERE state='accepted' AND method IN ('chat.create','chat.delete','projects.open','projects.close','terminal.close')",
            params![br#"{"reason":"conditions_changed: The host restarted before catalog dispatch; review the current state and submit a new command."}"#.as_slice(), now()],
        )?;
        tx.execute(
            "UPDATE _canopy_chat_queue SET state='delivery_unknown' WHERE state='attempting'",
            [],
        )?;
        // A persisted hint cannot target a run in a later host lifetime. The
        // original command receipt still binds its accepted steer intent.
        tx.execute(
            "UPDATE _canopy_chat_queue SET steer_run=NULL WHERE steer_run IS NOT NULL",
            [],
        )?;
        tx.execute(
            "UPDATE _canopy_runtime_chats SET queue_paused=1,queue_revision=queue_revision+1",
            [],
        )?;
        tx.commit()?;
        self.runtime_lease = Some(lease);
        self.runtime_host()
    }
    pub(super) fn runtime_host(&self) -> Result<RuntimeHost> {
        let (id, identity_ref): (String, Option<String>) = self.db.query_row(
            "SELECT host_id,tls_identity_ref FROM _canopy_runtime_host WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !host::valid_id(&id) || identity_ref.as_ref().is_some_and(|id| !host::valid_id(id)) {
            return Err(Error::InvalidRuntimeState);
        }
        Ok(RuntimeHost { id, identity_ref })
    }
    /// Stores `chat` and returns it as committed, with its thread identity.
    pub fn save_runtime_chat(&mut self, chat: ChatRecord) -> Result<ChatRecord> {
        require_host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let id = chat.id.clone();
        save_chat(&tx, chat)?;
        let saved = load_chat(&tx, "chat_id=?1", &id)?.ok_or(Error::InvalidRuntimeState)?;
        tx.commit()?;
        Ok(saved)
    }
    pub fn load_runtime_chats(&self) -> Result<Vec<ChatRecord>> {
        require_host(self)?;
        self.db
            .prepare(&format!(
                "SELECT {CHAT_COLUMNS} FROM _canopy_runtime_chats WHERE closed_at IS NULL ORDER BY chat_id"
            ))?
            .query_map([], chat_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(chat_record)
            .collect()
    }
    /// Keyset scan over live catalog rows; callers advance past hidden rows too.
    pub fn runtime_chats_page(&self, after: Option<String>) -> Result<Vec<ChatRecord>> {
        require_host(self)?;
        if after.as_ref().is_some_and(|id| !host::valid_id(id)) {
            return Err(Error::InvalidRuntimeState);
        }
        self.db
            .prepare(&format!("SELECT {CHAT_COLUMNS} FROM _canopy_runtime_chats WHERE closed_at IS NULL AND chat_id > ?1 ORDER BY chat_id LIMIT 64"))?
            .query_map([after.unwrap_or_default()], chat_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(chat_record)
            .collect()
    }
    /// A resource request must not materialize the whole host chat catalog.
    pub fn load_runtime_chat(&self, id: String) -> Result<Option<ChatRecord>> {
        require_host(self)?;
        if !host::valid_id(&id) {
            return Err(Error::InvalidRuntimeState);
        }
        load_chat(&self.db, "chat_id=?1 AND closed_at IS NULL", &id)
    }
    /// The open thread `thread`, if this host has it.
    pub fn load_runtime_thread(&self, thread: ThreadId) -> Result<Option<ChatRecord>> {
        require_host(self)?;
        load_chat(
            &self.db,
            "thread_id=?1 AND closed_at IS NULL",
            &thread.to_string(),
        )
    }
    /// The thread a legacy `(host_id, chat_id)` was given, also after it was deleted.
    pub fn runtime_thread_alias(&self, host: HostId, chat: String) -> Result<Option<ThreadId>> {
        require_host(self)?;
        let thread: Option<String> = self
            .db
            .query_row(
                "SELECT thread_id FROM _canopy_thread_alias WHERE legacy_host_id=?1 AND legacy_chat_id=?2",
                params![host.as_str(), chat],
                |r| r.get(0),
            )
            .optional()?;
        thread
            .map(|thread| ThreadId::parse(&thread).ok_or(Error::InvalidRuntimeState))
            .transpose()
    }
    pub fn register_runtime_device(&mut self, device: DeviceRegistration) -> Result<()> {
        require_host(self)?;
        let bound: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM _canopy_remote_devices WHERE public_credential=?1 OR device_id=?2)", params![device.credential, device.id], |r| r.get(0))?;
        if bound {
            return Err(Error::ResourceConflict);
        }
        let count: i64 = self.db.query_row(
            "SELECT count(*) FROM _canopy_remote_devices WHERE revoked_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        if count >= 32 {
            return Err(Error::ResourceLimit);
        }
        self.db.execute("INSERT INTO _canopy_remote_devices(device_id,name,public_credential,scopes,roots) VALUES(?1,?2,?3,?4,?5)",
            params![device.id,device.name,device.credential,device.scopes,serde_json::to_string(&device.roots).map_err(|_|Error::InvalidRuntimeState)?])?;
        Ok(())
    }
    pub fn revoke_runtime_device(&mut self, device: String) -> Result<()> {
        require_host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        revoke_device(&tx, &device)?;
        tx.commit()?;
        Ok(())
    }
    pub(in crate::settings) fn record_runtime_queue_wait(
        &self,
        input: &CommandInput,
        queued: Instant,
    ) {
        if let Ok(host) = self.runtime_host() {
            metrics::record(
                host.id(),
                &metrics::resource_id(&input.target),
                Some(input.key.id()),
                0,
                Stage::WorkerQueue,
                queued,
                Outcome::Ok,
            );
        }
    }
    pub fn accept_runtime_command(
        &mut self,
        input: CommandInput,
        scope: u32,
    ) -> Result<CommandReceipt> {
        let scope = bus::runtime_scope(&input, scope)?;
        let diagnostic_host = self.runtime_host().ok().map(|host| host.id);
        let diagnostic_key = input.key.clone();
        let diagnostic_resource = Some(metrics::resource_id(&input.target));
        let mut retried = false;
        let result: Result<CommandReceipt> = (|| {
            require_host(self)?;
            let tx = self
                .db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            authorize_root(&tx, &input.key, scope, command_root(&input)?, true)?;
            let result = match bus::runtime_admission(&tx, &input, scope)? {
                Some(result) => {
                    retried = true;
                    result
                }
                None => insert_command(&tx, &input, CommandState::Accepted, None, scope)?,
            };
            let commit_started = Instant::now();
            let committed = tx.commit();
            if let (Some(host), Some(resource)) = (&diagnostic_host, &diagnostic_resource) {
                metrics::record(
                    host,
                    resource,
                    Some(diagnostic_key.id()),
                    0,
                    Stage::Commit,
                    commit_started,
                    if committed.is_ok() {
                        Outcome::Ok
                    } else {
                        Outcome::Storage
                    },
                );
            }
            committed?;
            Ok(result)
        })();
        if let (Some(host), Some(resource)) = (&diagnostic_host, &diagnostic_resource) {
            let outcome = match &result {
                Ok(receipt) => {
                    if retried {
                        Outcome::Retry
                    } else {
                        Outcome::command(receipt.state)
                    }
                }
                Err(error) => Outcome::storage(error),
            };
            metrics::command_transition(host, resource, &diagnostic_key, outcome);
        }
        result
    }
    pub fn get_runtime_command(
        &self,
        key: CommandKey,
        scope: u32,
    ) -> Result<Option<CommandReceipt>> {
        require_host(self)?;
        let key = legacy_command_key(&self.db, key, scope)?;
        if let Some(result) = receipt(&self.db, &key)? {
            authorize_saved(&self.db, &key, false)?;
            return Ok(Some(result));
        }
        authorize(&self.db, &key, scope, true)?;
        Ok(None)
    }
    pub fn transition_runtime_command(
        &mut self,
        key: CommandKey,
        expected: CommandState,
        state: CommandState,
        result: Option<Vec<u8>>,
    ) -> Result<CommandReceipt> {
        let diagnostic_host = self.runtime_host().ok().map(|host| host.id);
        let diagnostic_key = key.clone();
        let diagnostic_resource = receipt(&self.db, &key)
            .ok()
            .flatten()
            .map(|r| metrics::resource_id(&r.target));
        let result: Result<CommandReceipt> = (|| {
            require_host(self)?;
            if result
                .as_ref()
                .is_some_and(|r| r.len() > host::CONTROL_BYTES)
            {
                return Err(Error::ResourceLimit);
            }
            let allowed = matches!(
                (expected, state),
                (
                    CommandState::Accepted,
                    CommandState::Attempting | CommandState::Rejected
                ) | (
                    CommandState::Attempting,
                    CommandState::Succeeded
                        | CommandState::Failed
                        | CommandState::DeliveryUnknown
                        | CommandState::Rejected
                )
            );
            if !allowed {
                return Err(Error::InvalidRuntimeState);
            }
            let tx = self
                .db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            // A host-owned effect may finish after its sender lost authority. Record
            // that known outcome; revoke still prevents dispatch and result disclosure.
            // This API is never exposed as a client-controlled state transition RPC.
            // Rejected after the attempt fence requires owner proof that no effect
            // occurred, such as an atomic revision check refusing a settings edit.
            if expected == CommandState::Accepted {
                authorize_saved(&tx, &key, true)?;
            }
            check_result_budget(&tx, &key, result.as_deref())?;
            let changed=tx.execute("UPDATE _canopy_command_inbox SET state=?3,result=?4,updated_at=?5 WHERE device_id=?1 AND command_id=?2 AND command_epoch=?6 AND state=?7 AND resource=?8 AND executor_epoch=?9",params![key.device,key.id,state.key(),result,now(),integer(key.epoch)?,expected.key(),key.resource,integer(key.executor_epoch)?])?;
            if changed == 0 {
                return Err(Error::ResourceConflict);
            }
            let result = receipt(&tx, &key)?.ok_or(Error::NotFound)?;
            let commit_started = Instant::now();
            let committed = tx.commit();
            if let (Some(host), Some(resource)) = (&diagnostic_host, &diagnostic_resource) {
                metrics::record(
                    host,
                    resource,
                    Some(diagnostic_key.id()),
                    0,
                    Stage::Commit,
                    commit_started,
                    if committed.is_ok() {
                        Outcome::Ok
                    } else {
                        Outcome::Storage
                    },
                );
            }
            committed?;
            Ok(result)
        })();
        if let (Some(host), Some(resource)) = (&diagnostic_host, &diagnostic_resource) {
            let outcome = match &result {
                Ok(receipt) => Outcome::command(receipt.state),
                Err(error) => Outcome::storage(error),
            };
            metrics::command_transition(host, resource, &diagnostic_key, outcome);
        }
        result
    }
    pub fn close_runtime_command_epoch(&mut self, device: String, expected: u64) -> Result<u64> {
        require_host(self)?;
        let probe = CommandKey::remote(device.clone(), expected, uuid::Uuid::new_v4().to_string())
            .ok_or(Error::InvalidRuntimeState)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        authorize(&tx, &probe, 0, false)?;
        let current = unsigned(tx.query_row::<i64, _, _>(
            "SELECT command_epoch FROM _canopy_remote_devices WHERE device_id=?1",
            [&device],
            |r| r.get(0),
        )?)?;
        if current == expected.saturating_add(1) {
            return Ok(current);
        }
        if current != expected {
            return Err(Error::CommandRetired);
        }
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _canopy_command_inbox WHERE device_id=?1 AND state IN ('accepted','attempting'))",[&device],|r|r.get(0))?;
        if pending {
            return Err(Error::OperationPending);
        }
        let next = expected
            .checked_add(1)
            .filter(|v| *v < i64::MAX as u64)
            .ok_or(Error::ResourceLimit)?;
        tx.execute(
            "UPDATE _canopy_remote_devices SET command_epoch=?2 WHERE device_id=?1",
            params![device, integer(next)?],
        )?;
        tx.execute("DELETE FROM _canopy_command_inbox WHERE device_id=?1 AND (state IN ('succeeded','failed','rejected') OR resolved_by_user=1)",[&device])?;
        tx.commit()?;
        Ok(next)
    }
    pub fn accept_runtime_message(
        &mut self,
        input: CommandInput,
        chat: String,
        message: String,
        draft: QueueDraft,
    ) -> Result<CommandReceipt> {
        let diagnostic_host = self.runtime_host().ok().map(|host| host.id);
        let diagnostic_key = input.key.clone();
        let diagnostic_resource = Some(metrics::resource_id(&input.target));
        let mut retried = false;
        let result: Result<CommandReceipt> = (|| {
            require_host(self)?;
            let tx = self
                .db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            retried = receipt(&tx, &input.key)?.is_some();
            let result = accept_message(&tx, input, chat, message, draft, None, host::CHAT)?;
            let commit_started = Instant::now();
            let committed = tx.commit();
            if let (Some(host), Some(resource)) = (&diagnostic_host, &diagnostic_resource) {
                metrics::record(
                    host,
                    resource,
                    Some(diagnostic_key.id()),
                    0,
                    Stage::Commit,
                    commit_started,
                    if committed.is_ok() {
                        Outcome::Ok
                    } else {
                        Outcome::Storage
                    },
                );
            }
            committed?;
            Ok(result)
        })();
        if let (Some(host), Some(resource)) = (&diagnostic_host, &diagnostic_resource) {
            let outcome = match &result {
                Ok(receipt) => {
                    if retried {
                        Outcome::Retry
                    } else {
                        Outcome::command(receipt.state)
                    }
                }
                Err(error) => Outcome::storage(error),
            };
            metrics::command_transition(host, resource, &diagnostic_key, outcome);
        }
        result
    }
    pub fn load_runtime_queue(&self, chat: String) -> Result<ChatQueue> {
        require_host(self)?;
        let (revision, paused): (i64, bool) = self
            .db
            .query_row(
                "SELECT queue_revision,queue_paused FROM _canopy_runtime_chats WHERE chat_id=?1",
                [&chat],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or(Error::NotFound)?;
        let rows=self.db.prepare("SELECT message_id,payload,state,resolved_by_user,steer_run FROM _canopy_chat_queue WHERE chat_id=?1 ORDER BY position")?
            .query_map([&chat],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,bool>(3)?,r.get::<_,Option<i64>>(4)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let entries = rows
            .into_iter()
            .map(|(id, payload, state, manual, steer_run)| {
                Ok(QueueEntry {
                    id,
                    draft: serde_json::from_str(&payload)
                        .map_err(|_| Error::InvalidRuntimeState)?,
                    state: QueueState::parse(&state).ok_or(Error::InvalidRuntimeState)?,
                    manual,
                    steer_run: steer_run.map(unsigned).transpose()?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(ChatQueue {
            revision: unsigned(revision)?,
            paused,
            entries,
        })
    }
    pub fn set_runtime_queue_paused(
        &mut self,
        chat: String,
        revision: u64,
        paused: bool,
    ) -> Result<ChatQueue> {
        let host_id = HostId::new(self.runtime_host()?.id).ok_or(Error::InvalidRuntimeState)?;
        let thread_id = self
            .runtime_thread_alias(host_id.clone(), chat.clone())?
            .ok_or(Error::NotFound)?;
        let prepared = self.prepare_bus_command(
            crate::command_bus::CommandEnvelope {
                command_id: uuid::Uuid::new_v4(),
                resource: crate::command_bus::ResourceRef::Thread {
                    thread: ThreadRef { host_id, thread_id },
                    executor_epoch: 0,
                },
                command: crate::command_bus::Command::Queue {
                    revision,
                    change: crate::command_bus::QueueChange::Pause { paused },
                },
                conditions: Default::default(),
            },
            crate::command_bus::Principal::local_owner(),
        )?;
        let receipt = self.dispatch_bus_command(
            prepared,
            crate::command_bus::Dispatch::Queue {
                revision,
                mutation: QueueMutation::Pause { paused },
            },
        )?;
        self.acknowledge_runtime_command(receipt.key)?;
        self.load_runtime_queue(chat)
    }

    pub fn transition_runtime_queue(
        &mut self,
        chat: String,
        message: String,
        revision: u64,
        expected: QueueState,
        state: QueueState,
    ) -> Result<ChatQueue> {
        require_host(self)?;
        if !matches!(
            (expected, state),
            (QueueState::Queued, QueueState::Attempting)
                | (
                    QueueState::Attempting,
                    QueueState::Delivered | QueueState::Failed | QueueState::DeliveryUnknown
                )
        ) {
            return Err(Error::InvalidRuntimeState);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let changed=tx.execute("UPDATE _canopy_runtime_chats SET queue_revision=queue_revision+1 WHERE chat_id=?1 AND queue_revision=?2 AND closed_at IS NULL",params![chat,integer(revision)?])?;
        if changed == 0 {
            return Err(Error::ResourceConflict);
        }
        if state == QueueState::Attempting {
            let blocked:bool=tx.query_row("SELECT queue_paused OR EXISTS(SELECT 1 FROM _canopy_chat_queue WHERE chat_id=?1 AND state IN ('attempting','delivery_unknown')) FROM _canopy_runtime_chats WHERE chat_id=?1",[&chat],|r|r.get(0))?;
            if blocked {
                return Err(Error::OperationPending);
            }
            let first:Option<String>=tx.query_row("SELECT message_id FROM _canopy_chat_queue WHERE chat_id=?1 AND state='queued' ORDER BY position LIMIT 1",[&chat],|r|r.get(0)).optional()?;
            if first.as_deref() != Some(&message) {
                return Err(Error::ResourceConflict);
            }
        }
        let changed=tx.execute("UPDATE _canopy_chat_queue SET state=?3 WHERE chat_id=?1 AND message_id=?2 AND state=?4",params![chat,message,state.key(),expected.key()])?;
        if changed == 0 {
            return Err(Error::ResourceConflict);
        }
        tx.commit()?;
        self.load_runtime_queue(chat)
    }
}

fn adopted_authority(path: &std::path::Path) -> Option<String> {
    let canonical = crate::state::projects::canonical_directory(path).ok()?;
    // A changed symlink or formerly noncanonical path needs an explicit choice;
    // migration must not silently broaden authority to a newly resolved target.
    let root = canonical.to_str()?;
    (Some(root) == path.to_str() && host::valid_authority_root(root)).then(|| root.to_owned())
}

fn adopt(tx: &Transaction) -> Result<()> {
    if let Some(snapshot) = super::read_session(tx)? {
        for workspace in snapshot.workspaces.iter().chain(snapshot.scratch.iter()) {
            let scratch = snapshot
                .scratch
                .as_ref()
                .is_some_and(|s| s.id == workspace.id);
            let authority = if scratch {
                Some("private".to_owned())
            } else {
                snapshot
                    .projects
                    .items
                    .iter()
                    .find(|p| p.workspace == workspace.id)
                    .and_then(|p| adopted_authority(p.worktree_path.as_deref().unwrap_or(&p.path)))
            };
            let recovery = authority
                .is_none()
                .then_some("Restore the canonical checkout directory before running");
            let authority = authority.unwrap_or_default();
            for pane in workspace.all_panes().into_iter().filter(|p| {
                p.metadata.kind == crate::state::workspace::PaneKind::Chat
                    && p.metadata.remote.is_none()
            }) {
                let Some(id) = pane.metadata.chat.as_ref().and_then(|c| c.id.clone()) else {
                    continue;
                };
                let metadata = serde_json::to_string(&pane.metadata)
                    .map_err(|_| Error::InvalidRuntimeState)?;
                let root = if snapshot
                    .scratch
                    .as_ref()
                    .is_some_and(|scratch| scratch.id == workspace.id)
                {
                    None
                } else {
                    Some(
                        serde_json::to_string(&workspace.id)
                            .map_err(|_| Error::InvalidRuntimeState)?,
                    )
                };
                let prior:Option<(String,String,Option<String>,String)>=tx.query_row("SELECT metadata,agent_id,workspace,authority_root FROM _canopy_runtime_chats WHERE chat_id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
                if let Some((prior, agent, prior_root, prior_authority)) = prior {
                    if prior != metadata
                        || agent != pane.tool
                        || prior_root != root
                        || prior_authority != authority
                    {
                        return Err(Error::InvalidRuntimeState);
                    }
                    continue;
                }
                let thread = insert_thread(tx, &id)?.to_string();
                tx.execute("INSERT INTO _canopy_runtime_chats(chat_id,thread_id,host_id,workspace,agent_id,profile_id,metadata,authority_root,recovery_reason) VALUES(?1,?8,(SELECT host_id FROM _canopy_runtime_host WHERE id=1),?2,?3,?4,?5,?6,?7)",params![id,root,pane.tool,pane.metadata.profile_id,metadata,authority,recovery,thread])?;
            }
        }
    }
    if has_table(tx, "_canopy_rust_closed_chats")? {
        let closed = tx
            .prepare(
                "SELECT chat,closed_at,payload FROM _canopy_rust_closed_chats WHERE version=1",
            )?
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, at, payload) in closed {
            if let Some(chat) = crate::chat::closed::ClosedChat::from_json(&payload) {
                if chat.chat_id() != Some(&id) {
                    return Err(Error::InvalidRuntimeState);
                }
                let present: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM _canopy_runtime_chats WHERE chat_id=?1)",
                    [&id],
                    |r| r.get(0),
                )?;
                if present {
                    return Err(Error::InvalidRuntimeState);
                }
                let authority = match &chat.project {
                    None => Some("private".to_owned()),
                    Some(p) => adopted_authority(p.worktree_path.as_deref().unwrap_or(&p.path)),
                };
                let recovery = authority
                    .is_none()
                    .then_some("Restore the canonical checkout directory before running");
                let thread = insert_thread(tx, &id)?.to_string();
                tx.execute("INSERT INTO _canopy_runtime_chats(chat_id,thread_id,host_id,agent_id,profile_id,metadata,closed_at,authority_root,recovery_reason) VALUES(?1,?8,(SELECT host_id FROM _canopy_runtime_host WHERE id=1),?2,?3,?4,?5,?6,?7)",params![id,chat.tool,chat.metadata.profile_id,serde_json::to_string(&chat.metadata).map_err(|_|Error::InvalidRuntimeState)?,at,authority.unwrap_or_default(),recovery,thread])?;
            } else {
                return Err(Error::InvalidRuntimeState);
            }
        }
    }
    adopt_orphans(tx)?;
    Ok(())
}

impl Database {
    /// Acknowledgement is local-owner cleanup, never a remote retry endpoint.
    pub fn acknowledge_runtime_command(&mut self, key: CommandKey) -> Result<()> {
        require_host(self)?;
        if key.device != host::LOCAL_PRINCIPAL {
            return Err(Error::NotAuthorized);
        }
        let key = legacy_command_key(&self.db, key, 0)?;
        let changed=self.db.execute("DELETE FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND command_epoch=?3 AND resource=?4 AND executor_epoch=?5 AND (state IN ('succeeded','failed','rejected') OR (state='delivery_unknown' AND resolved_by_user=1))",params![key.device,key.id,integer(key.epoch)?,key.resource,integer(key.executor_epoch)?])?;
        if changed == 0 && receipt(&self.db, &key)?.is_some() {
            return Err(Error::OperationPending);
        }
        Ok(())
    }
    /// Archives uncertainty without claiming evidence of the external effect.
    pub fn resolve_runtime_command(&mut self, key: CommandKey) -> Result<CommandReceipt> {
        require_host(self)?;
        let key = legacy_command_key(&self.db, key, 0)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        authorize_saved(&tx, &key, false)?;
        let changed=tx.execute("UPDATE _canopy_command_inbox SET resolved_by_user=1,updated_at=?4 WHERE device_id=?1 AND command_id=?2 AND command_epoch=?3 AND resource=?5 AND executor_epoch=?6 AND state='delivery_unknown'",params![key.device,key.id,integer(key.epoch)?,now(),key.resource,integer(key.executor_epoch)?])?;
        if changed == 0 {
            return Err(Error::ResourceConflict);
        }
        let result = receipt(&tx, &key)?.ok_or(Error::NotFound)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn mutate_runtime_queue(
        &mut self,
        input: CommandInput,
        chat: String,
        revision: u64,
        mutation: QueueMutation,
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        let normalized = mutation
            .command(input.key.clone(), &chat, revision)
            .ok_or(Error::InvalidRuntimeState)?;
        if input.method != normalized.method
            || input.target != normalized.target
            || input.arguments != normalized.arguments
            || input.fingerprint != normalized.fingerprint
        {
            return Err(Error::CommandConflict);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let result = mutate_queue(&tx, input, chat, revision, mutation, host::CHAT)?;
        tx.commit()?;
        Ok(result)
    }
}

// Local revoke calls this without inbox admission, including when its inbox is full.
fn revoke_device(tx: &Transaction, device: &str) -> Result<()> {
    let changed=tx.execute("UPDATE _canopy_remote_devices SET revoked_at=coalesce(revoked_at,?2),authorization_revision=authorization_revision+(revoked_at IS NULL) WHERE device_id=?1",params![device,now()])?;
    if changed == 0 {
        return Err(Error::NotFound);
    }
    tx.execute("UPDATE _canopy_command_inbox SET state='rejected',updated_at=?2 WHERE device_id=?1 AND state='accepted'",params![device,now()])?;
    tx.execute(
        "DELETE FROM _canopy_dirty_presence WHERE device_id=?1",
        [device],
    )?;
    Ok(())
}

fn mutate_queue(
    tx: &Transaction,
    mut input: CommandInput,
    chat: String,
    revision: u64,
    mutation: QueueMutation,
    scope: u32,
) -> Result<CommandReceipt> {
    input.root = Some(chat_root(tx, &chat, &input.key)?);
    authorize_root(tx, &input.key, scope, command_root(&input)?, true)?;
    if let Some(prior) = bus::runtime_admission(tx, &input, scope)? {
        return Ok(prior);
    }
    if let Some(result) = existing(tx, &input)? {
        return Ok(result);
    }
    let changed=tx.execute("UPDATE _canopy_runtime_chats SET queue_revision=queue_revision+1 WHERE chat_id=?1 AND queue_revision=?2 AND closed_at IS NULL",params![chat,integer(revision)?])?;
    if changed == 0 {
        return Err(Error::ResourceConflict);
    }
    match mutation {
        QueueMutation::Pause { paused } => {
            tx.execute(
                "UPDATE _canopy_runtime_chats SET queue_paused=?2 WHERE chat_id=?1",
                params![chat, paused],
            )?;
        }
        QueueMutation::Edit { message, draft } => {
            if !draft.valid() {
                return Err(Error::InvalidRuntimeState);
            }
            let previous:String=tx.query_row("SELECT payload FROM _canopy_chat_queue WHERE chat_id=?1 AND message_id=?2 AND state='queued'",params![chat,message],|r|r.get(0)).optional()?.ok_or(Error::ResourceConflict)?;
            let previous: host::QueueDraft =
                serde_json::from_str(&previous).map_err(|_| Error::InvalidRuntimeState)?;
            super::artifacts::retain_for_queue_edit(
                tx,
                &input.key.device,
                &chat,
                previous.artifact_ids(),
                draft.artifact_ids(),
            )?;
            let payload = serde_json::to_string(&draft).map_err(|_| Error::InvalidRuntimeState)?;
            if payload.len() > host::CONTROL_BYTES {
                return Err(Error::ResourceLimit);
            }
            let changed=tx.execute("UPDATE _canopy_chat_queue SET payload=?3 WHERE chat_id=?1 AND message_id=?2 AND state='queued'",params![chat,message,payload])?;
            if changed == 0 {
                return Err(Error::ResourceConflict);
            }
        }
        QueueMutation::Remove { message } => {
            let changed=tx.execute("UPDATE _canopy_chat_queue SET state='dismissed',resolved_by_user=1,resolution='removed' WHERE chat_id=?1 AND message_id=?2 AND state='queued'",params![chat,message])?;
            if changed == 0 {
                return Err(Error::ResourceConflict);
            }
        }
        QueueMutation::Move { message, before } => {
            let rows=tx.prepare("SELECT message_id,state FROM _canopy_chat_queue WHERE chat_id=?1 ORDER BY position,message_id")?.query_map([&chat],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            if !rows
                .iter()
                .any(|(id, state)| id == &message && state == "queued")
                || before.as_ref().is_some_and(|before| {
                    !rows
                        .iter()
                        .any(|(id, state)| id == before && state == "queued")
                })
            {
                return Err(Error::ResourceConflict);
            }
            let mut ids = rows.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
            let from = ids
                .iter()
                .position(|id| id == &message)
                .ok_or(Error::ResourceConflict)?;
            ids.remove(from);
            let to = match before {
                Some(id) => ids
                    .iter()
                    .position(|item| item == &id)
                    .ok_or(Error::ResourceConflict)?,
                None => ids.len(),
            };
            ids.insert(to, message);
            for (position, id) in ids.iter().enumerate() {
                tx.execute(
                    "UPDATE _canopy_chat_queue SET position=?3 WHERE chat_id=?1 AND message_id=?2",
                    params![chat, id, position as i64],
                )?;
            }
        }
        QueueMutation::ResolveUnknown {
            message,
            resolution,
        } => {
            let (payload,position):(String,i64)=tx.query_row("SELECT payload,position FROM _canopy_chat_queue WHERE chat_id=?1 AND message_id=?2 AND state='delivery_unknown'",params![chat,message],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or(Error::ResourceConflict)?;
            let (state, reason) = match &resolution {
                UnknownResolution::Delivered => ("delivered", "user_confirmed"),
                UnknownResolution::Dismissed => ("dismissed", "user_dismissed"),
                UnknownResolution::Resend { .. } => ("dismissed", "user_resent"),
            };
            tx.execute("UPDATE _canopy_chat_queue SET state=?3,resolved_by_user=1,resolution=?4 WHERE chat_id=?1 AND message_id=?2",params![chat,message,state,reason])?;
            if let UnknownResolution::Resend {
                message: new_message,
            } = resolution
            {
                if !host::valid_id(&new_message) || new_message == message {
                    return Err(Error::InvalidRuntimeState);
                }
                // This resolution command is the explicit new send intent; no second inbox.
                tx.execute("INSERT INTO _canopy_chat_queue(chat_id,message_id,position,payload,state,source_device) VALUES(?1,?2,?3,?4,'queued',?5)",params![chat,new_message,position,payload,input.key.device])?;
            }
        }
    }
    let result = serde_json::to_vec(&revision.checked_add(1).ok_or(Error::ResourceLimit)?)
        .map_err(|_| Error::InvalidRuntimeState)?;
    let receipt = insert_command(tx, &input, CommandState::Succeeded, Some(&result), scope)?;
    Ok(receipt)
}

// History may outlive every pane. Keep its identity and account references, but
// require explicit recovery of missing launch context rather than choosing defaults.
fn adopt_orphans(tx: &Transaction) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    for table in ["_canopy_rust_chat_epochs", "_canopy_rust_chat_items"] {
        if has_table(tx, table)? {
            ids.extend(
                tx.prepare(&format!("SELECT DISTINCT chat FROM {table}"))?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
    }
    for id in ids {
        let present: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _canopy_runtime_chats WHERE chat_id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        if present {
            continue;
        }
        if !host::valid_id(&id) {
            return Err(Error::InvalidRuntimeState);
        }
        let mut metadata = crate::state::workspace::PaneMetadata {
            kind: crate::state::workspace::PaneKind::Chat,
            ..Default::default()
        };
        let mut settings = crate::chat::config::ChatSettings::default();
        if has_table(tx, "_canopy_rust_chat_epochs")? {
            let raw:Option<(i64,String)>=tx.query_row("SELECT version,meta FROM _canopy_rust_chat_epochs WHERE chat=?1 ORDER BY epoch DESC LIMIT 1",[&id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((1, raw)) = raw
                && let Ok(epoch) = serde_json::from_str::<crate::chat::history::EpochMeta>(&raw)
                && let Some(agent) = epoch.agent
            {
                metadata.profile_id = agent.profile;
                metadata.resume_id = agent.session;
                settings = agent.settings.unwrap_or_default();
            }
        }
        settings.id = Some(id.clone());
        metadata.chat = Some(Box::new(settings));
        let payload = serde_json::to_string(&metadata).map_err(|_| Error::InvalidRuntimeState)?;
        let thread = insert_thread(tx, &id)?.to_string();
        tx.execute("INSERT INTO _canopy_runtime_chats(chat_id,thread_id,host_id,agent_id,profile_id,metadata,authority_root,recovery_reason) VALUES(?1,?4,(SELECT host_id FROM _canopy_runtime_host WHERE id=1),'unresolved',?2,?3,'','History retained without pane metadata; restore account and context before running')",params![id,metadata.profile_id,payload,thread])?;
    }
    Ok(())
}

fn save_chat(db: &Connection, mut chat: ChatRecord) -> Result<()> {
    // Explicit names have one writer: the bus. A delayed runtime/catalog save
    // must not restore a name captured before a committed rename.
    let metadata: Option<String> = db
        .query_row(
            "SELECT metadata FROM _canopy_runtime_chats WHERE chat_id=?1",
            [&chat.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(metadata) = metadata {
        let stored: crate::state::workspace::PaneMetadata =
            serde_json::from_str(&metadata).map_err(|_| Error::InvalidRuntimeState)?;
        if stored.chat_title_state == Some(crate::state::workspace::ChatTitleState::Named) {
            chat.metadata.title = stored.title;
            chat.metadata.chat_title_state = stored.chat_title_state;
        }
    }
    if ChatRecord::new(
        chat.id.clone(),
        chat.workspace.clone(),
        chat.agent.clone(),
        chat.metadata.clone(),
    )
    .is_none()
    {
        return Err(Error::InvalidRuntimeState);
    }
    if !host::valid_authority_root(&chat.authority_root)
        || chat.workspace.is_some() && chat.authority_root == "private"
    {
        return Err(Error::InvalidRuntimeState);
    }
    let payload = serde_json::to_string(&chat.metadata).map_err(|_| Error::InvalidRuntimeState)?;
    if payload.len() > host::CONTROL_BYTES {
        return Err(Error::ResourceLimit);
    }
    let stored: Option<(String, String)> = db
        .query_row(
            "SELECT host_id,thread_id FROM _canopy_runtime_chats WHERE chat_id=?1",
            [&chat.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    // A record read from storage names its thread; it never moves to another one,
    // and a deleted thread is not recreated under the same identity.
    if let Some(expected) = &chat.thread
        && stored.as_ref()
            != Some(&(
                expected.thread.host_id.to_string(),
                expected.thread.thread_id.to_string(),
            ))
    {
        return Err(Error::InvalidRuntimeState);
    }
    let changed = if stored.is_some() {
        db.execute("UPDATE _canopy_runtime_chats SET revision=revision+(workspace IS NOT ?2 OR authority_root IS NOT ?7 OR agent_id IS NOT ?3 OR profile_id IS NOT ?4 OR metadata IS NOT ?5 OR history_epoch<?6 OR closed_at IS NOT NULL),workspace=?2,authority_root=?7,agent_id=?3,profile_id=?4,metadata=?5,history_epoch=max(history_epoch,?6),closed_at=NULL WHERE chat_id=?1 AND recovery_reason IS NULL",
            params![chat.id,chat.workspace,chat.agent,chat.metadata.profile_id,payload,integer(chat.history_epoch)?,chat.authority_root])?
    } else {
        let thread = insert_thread(db, &chat.id)?;
        db.execute("INSERT INTO _canopy_runtime_chats(chat_id,thread_id,host_id,workspace,agent_id,profile_id,metadata,history_epoch,authority_root) VALUES(?1,?8,(SELECT host_id FROM _canopy_runtime_host WHERE id=1),?2,?3,?4,?5,?6,?7)",
            params![chat.id,chat.workspace,chat.agent,chat.metadata.profile_id,payload,integer(chat.history_epoch)?,chat.authority_root,thread.to_string()])?
    };
    if changed == 0 {
        return Err(Error::InvalidRuntimeState);
    }
    Ok(())
}

/// A new conversation's global thread and its one alias, in the caller's
/// transaction. Aliases outlive their thread, so a deleted chat ID is never
/// given a thread again (OWN-08).
fn insert_thread(db: &Connection, chat: &str) -> Result<ThreadId> {
    let thread = ThreadId::new();
    let added = db.execute(
        "INSERT INTO _canopy_thread_alias(legacy_host_id,legacy_chat_id,thread_id) SELECT host_id,?1,?2 FROM _canopy_runtime_host WHERE id=1 ON CONFLICT DO NOTHING",
        params![chat, thread.to_string()],
    )?;
    if added == 0 {
        return Err(Error::ResourceConflict);
    }
    Ok(thread)
}

const CHAT_COLUMNS: &str = "chat_id,workspace,agent_id,metadata,recovery_reason,history_epoch,authority_root,host_id,thread_id,revision,executor_epoch";
type ChatRow = (
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    i64,
    String,
    String,
    String,
    i64,
    i64,
);
fn chat_row(r: &rusqlite::Row) -> rusqlite::Result<ChatRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
    ))
}
fn load_chat(db: &Connection, filter: &str, key: &str) -> Result<Option<ChatRecord>> {
    db.query_row(
        &format!("SELECT {CHAT_COLUMNS} FROM _canopy_runtime_chats WHERE {filter}"),
        [key],
        chat_row,
    )
    .optional()?
    .map(chat_record)
    .transpose()
}
fn chat_record(
    (
        id,
        workspace,
        agent,
        payload,
        recovery_reason,
        history_epoch,
        authority_root,
        host_id,
        thread_id,
        revision,
        executor_epoch,
    ): ChatRow,
) -> Result<ChatRecord> {
    let mut record = ChatRecord::new(
        id,
        workspace,
        agent,
        serde_json::from_str(&payload).map_err(|_| Error::InvalidRuntimeState)?,
    )
    .ok_or(Error::InvalidRuntimeState)?;
    if !(host::valid_authority_root(&authority_root)
        || authority_root.is_empty() && recovery_reason.is_some())
        || record.workspace.is_some() && authority_root == "private"
    {
        return Err(Error::InvalidRuntimeState);
    }
    record.authority_root = authority_root;
    record.recovery_reason = recovery_reason;
    record.history_epoch = unsigned(history_epoch)?;
    record.thread = Some(PersistedThread {
        thread: ThreadRef {
            host_id: HostId::new(host_id).ok_or(Error::InvalidRuntimeState)?,
            thread_id: ThreadId::parse(&thread_id).ok_or(Error::InvalidRuntimeState)?,
        },
        revision: unsigned(revision)?,
        executor_epoch: unsigned(executor_epoch)?,
    });
    Ok(record)
}

fn accept_message(
    tx: &Transaction,
    mut input: CommandInput,
    chat: String,
    message: String,
    draft: QueueDraft,
    resolved_intent: Option<host::MessageIntent>,
    scope: u32,
) -> Result<CommandReceipt> {
    let scope = bus::runtime_scope(&input, scope)?;
    if !host::valid_id(&chat) || !host::valid_id(&message) || !draft.valid() {
        return Err(Error::InvalidRuntimeState);
    }
    if !matches!(
        input.method.as_str(),
        "chat.send" | "chat.steer" | "chat.set_goal"
    ) || input.target != chat
    {
        return Err(Error::InvalidRuntimeState);
    }
    let payload = serde_json::to_string(&draft).map_err(|_| Error::InvalidRuntimeState)?;
    if payload.len() > host::CONTROL_BYTES {
        return Err(Error::ResourceLimit);
    }
    input.root = Some(chat_root(tx, &chat, &input.key)?);
    authorize_root(tx, &input.key, scope, command_root(&input)?, true)?;
    if let Some(prior) = bus::runtime_admission(tx, &input, scope)? {
        return Ok(prior);
    }
    let intent = if let Some(intent) = resolved_intent {
        Some(intent)
    } else {
        match serde_json::from_slice::<host::MessageIntent>(&input.arguments) {
            Ok(intent) => {
                if intent.message_id != message || intent.draft != draft {
                    return Err(Error::CommandConflict);
                }
                let normalized =
                    serde_json::to_vec(&intent).map_err(|_| Error::InvalidRuntimeState)?;
                if normalized != input.arguments {
                    return Err(Error::CommandConflict);
                }
                Some(intent)
            }
            Err(_)
                if input.key.device == host::LOCAL_PRINCIPAL
                    && input.arguments == payload.as_bytes() =>
            {
                None
            }
            Err(_) if !input.key.resource.is_empty() => {
                let (_, _, _, command, _): (
                    String,
                    String,
                    u64,
                    crate::command_bus::Command,
                    crate::command_bus::Conditions,
                ) = serde_json::from_slice(&input.arguments).map_err(|_| Error::CommandConflict)?;
                let steer_run = match command {
                    crate::command_bus::Command::Control(crate::command_bus::Control::Steer {
                        run,
                        ..
                    }) => Some(run),
                    crate::command_bus::Command::Control(
                        crate::command_bus::Control::SetGoal { .. },
                    ) => None,
                    _ => return Err(Error::CommandConflict),
                };
                let record = load_chat(tx, "chat_id=?1 AND closed_at IS NULL", &chat)?
                    .ok_or(Error::NotFound)?;
                Some(host::MessageIntent {
                    message_id: message.clone(),
                    draft: draft.clone(),
                    context: record.send_context(),
                    steer_run,
                })
            }
            Err(_) => return Err(Error::CommandConflict),
        }
    };
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM _canopy_runtime_chats WHERE chat_id=?1 AND closed_at IS NULL)",
        [&chat],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(Error::NotFound);
    }
    let recovery: bool = tx.query_row("SELECT recovery_reason IS NOT NULL OR authority_root='' FROM _canopy_runtime_chats WHERE chat_id=?1", [&chat], |r| r.get(0))?;
    if recovery {
        return Err(Error::ResourceConflict);
    }
    let steer_run = intent
        .as_ref()
        .and_then(|intent| intent.steer_run)
        .map(integer)
        .transpose()?;
    if let Some(intent) = intent {
        let current: (String, Option<String>, Option<String>, i64, Option<String>) = tx.query_row(
                "SELECT agent_id,profile_id,workspace,history_epoch,recovery_reason FROM _canopy_runtime_chats WHERE chat_id=?1",
                [&chat], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        if current.4.is_some()
            || intent.context.agent != current.0
            || intent.context.profile != current.1
            || intent.context.workspace != current.2
            || integer(intent.context.history_epoch)? != current.3
        {
            return Err(Error::ResourceConflict);
        }
    }
    chat_commands::retain_images(tx, &input.key.device, &chat, draft.artifact_ids())?;
    let result = insert_command(
        tx,
        &input,
        CommandState::Succeeded,
        Some(message.as_bytes()),
        scope,
    )?;
    let position: i64 = tx.query_row(
        "SELECT coalesce(max(position)+1,0) FROM _canopy_chat_queue WHERE chat_id=?1",
        [&chat],
        |r| r.get(0),
    )?;
    tx.execute("INSERT INTO _canopy_chat_queue(chat_id,message_id,position,payload,state,source_device,steer_run) VALUES(?1,?2,?3,?4,'queued',?5,?6)",params![chat,message,position,payload,input.key.device,steer_run])?;
    tx.execute(
        "UPDATE _canopy_runtime_chats SET queue_revision=queue_revision+1 WHERE chat_id=?1",
        [&chat],
    )?;
    // A new explicit Send resumes an otherwise empty held queue. This belongs to
    // first acceptance, never to retry publication after a later user Pause.
    if !input.key.resource.is_empty() {
        tx.execute("UPDATE _canopy_runtime_chats SET queue_paused=0,queue_revision=queue_revision+1 WHERE chat_id=?1 AND queue_paused=1 AND NOT EXISTS(SELECT 1 FROM _canopy_chat_queue WHERE chat_id=?1 AND message_id!=?2 AND state IN ('queued','attempting','delivery_unknown'))", params![chat,message])?;
    }
    Ok(result)
}
