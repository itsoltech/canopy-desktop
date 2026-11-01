//! Atomic chat acceptance and fork completion on the existing settings writer.
use super::*;

/// Existing references belong to the chat, including images inherited by a fork.
/// Only new references need the original upload owner's authority.
pub(super) fn retain_images(
    tx: &Transaction,
    owner: &str,
    chat: &str,
    images: &[String],
) -> Result<()> {
    if images.len() > 10 {
        return Err(Error::ResourceLimit);
    }
    for image in images {
        let retained: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _canopy_artifact_refs r JOIN _canopy_artifacts a ON a.artifact_id=r.artifact_id JOIN _canopy_uploads u ON u.artifact_id=a.artifact_id WHERE r.owner_kind='chat' AND r.owner_id=?1 AND r.artifact_id=?2 AND u.purpose=?3 AND a.size<=?4)",
            params![chat, image, serde_json::to_string(&crate::artifacts::Purpose::Image).map_err(|_| Error::InvalidRuntimeState)?, crate::artifacts::MAX_IMAGE_BYTES as i64], |r| r.get(0))?;
        if !retained {
            super::super::artifacts::retain_for_chat(tx, owner, image, chat)?;
        }
    }
    Ok(())
}

impl Database {
    /// Checks current grants before disclosing even a retry conflict.
    pub fn get_runtime_command_matching(
        &self,
        key: CommandKey,
        scope: u32,
        method: String,
        target: String,
        fingerprint: Vec<u8>,
    ) -> Result<Option<CommandReceipt>> {
        require_host(self)?;
        authorize(&self.db, &key, scope, false)?;
        let saved: Option<(u32, String, String, String, Vec<u8>, i64)> = self.db.query_row(
            "SELECT required_scope,required_root,method,target,fingerprint,command_epoch FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND resource=?3 AND executor_epoch=?4",
            params![key.device,key.id,key.resource,integer(key.executor_epoch)?], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        let Some((saved_scope, root, saved_method, saved_target, saved_fingerprint, epoch)) = saved
        else {
            authorize(&self.db, &key, scope, true)?;
            return Ok(None);
        };
        authorize_root(&self.db, &key, saved_scope, &root, false)?;
        if method != saved_method
            || target != saved_target
            || fingerprint != saved_fingerprint
            || integer(key.epoch)? != epoch
        {
            return Err(Error::CommandConflict);
        }
        receipt(&self.db, &key)
    }

    pub fn accept_runtime_chat_command(
        &mut self,
        mut input: CommandInput,
        scope: u32,
        chat: String,
        artifacts: Vec<String>,
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        let scope = bus::runtime_scope(&input, scope)?;
        let diagnostic_host = self.runtime_host()?.id;
        if input.target != chat {
            return Err(Error::CommandConflict);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        input.root = Some(chat_root(&tx, &chat, &input.key)?);
        authorize_root(&tx, &input.key, scope, command_root(&input)?, true)?;
        if let Some(prior) = bus::runtime_admission(&tx, &input, scope)? {
            metrics::command_transition(
                &diagnostic_host,
                &metrics::resource_id(&input.target),
                &input.key,
                Outcome::Retry,
            );
            tx.commit()?;
            return Ok(prior);
        }
        live_chat(&tx, &chat)?;
        retain_images(&tx, &input.key.device, &chat, &artifacts)?;
        let result = insert_command(&tx, &input, CommandState::Accepted, None, scope)?;
        let started = Instant::now();
        tx.commit()?;
        metrics::record(
            &diagnostic_host,
            &metrics::resource_id(&input.target),
            Some(input.key.id()),
            0,
            Stage::Commit,
            started,
            Outcome::Ok,
        );
        metrics::command_transition(
            &diagnostic_host,
            &metrics::resource_id(&input.target),
            &input.key,
            Outcome::Accepted,
        );
        Ok(result)
    }

    /// The caller first copies history using the existing history owner. This
    /// transaction publishes the catalog entry, references and replacement together.
    pub fn complete_runtime_fork(
        &mut self,
        key: CommandKey,
        record: ChatRecord,
        source_chat: String,
        replacement: Option<(CommandInput, String, QueueDraft)>,
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        let diagnostic_host = self.runtime_host()?.id;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        authorize_saved(&tx, &key, true)?;
        let prior = receipt(&tx, &key)?.ok_or(Error::NotFound)?;
        if prior.method != "chat.fork"
            || (if key.resource.is_empty() {
                prior.target != record.id
            } else {
                prior.target != source_chat
            })
            || source_chat == record.id
        {
            return Err(Error::CommandConflict);
        }
        let (root, arguments, fingerprint): (String, Vec<u8>, Vec<u8>) = tx.query_row(
            "SELECT required_root,arguments,fingerprint FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND resource=?3 AND executor_epoch=?4",
            params![key.device,key.id,key.resource,integer(key.executor_epoch)?], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let mut intent: serde_json::Value =
            serde_json::from_slice(&arguments).map_err(|_| Error::InvalidRuntimeState)?;
        if !key.resource.is_empty() {
            use crate::command_bus::{Command, Conditions, Control};
            let (_, _, _, command, _): (String, String, u64, Command, Conditions) =
                serde_json::from_slice(&arguments).map_err(|_| Error::InvalidRuntimeState)?;
            let Command::Control(Control::Fork {
                history_epoch,
                replacement,
                ..
            }) = command
            else {
                return Err(Error::CommandConflict);
            };
            let source = load_chat(&tx, "chat_id=?1 AND closed_at IS NULL", &source_chat)?
                .ok_or(Error::NotFound)?;
            if source.workspace != record.workspace || source.workspace.is_none() {
                return Err(Error::ResourceConflict);
            }
            use sha2::Digest;
            let replacement_fingerprint = replacement.map(|_| {
                let mut hash = sha2::Sha256::new();
                hash.update(b"canopy/fork-replacement/v1\0");
                hash.update(&fingerprint);
                hash.finalize().to_vec()
            });
            intent = serde_json::json!({"source":source_chat,"history_epoch":history_epoch,"replacement_fingerprint":replacement_fingerprint});
        }
        if intent.get("source").and_then(|v| v.as_str()) != Some(&source_chat)
            || root != record.authority_root
        {
            return Err(Error::CommandConflict);
        }
        let expected_fingerprint: Option<Vec<u8>> = intent
            .get("replacement_fingerprint")
            .filter(|value| !value.is_null())
            .map(|value| {
                serde_json::from_value(value.clone()).map_err(|_| Error::InvalidRuntimeState)
            })
            .transpose()?;
        if expected_fingerprint.as_deref()
            != replacement
                .as_ref()
                .map(|(input, _, _)| input.fingerprint.as_slice())
        {
            return Err(Error::CommandConflict);
        }
        let epoch = intent
            .get("history_epoch")
            .and_then(|v| v.as_u64())
            .ok_or(Error::InvalidRuntimeState)?;
        if prior.state == CommandState::Succeeded {
            metrics::command_transition(
                &diagnostic_host,
                &metrics::resource_id(record.id()),
                &key,
                Outcome::Retry,
            );
            return Ok(prior);
        }
        if prior.state != CommandState::Attempting {
            return Err(Error::ResourceConflict);
        }
        live_chat(&tx, &source_chat)?;
        let mut retained: bool = tx.query_row(
            "SELECT history_epoch=?2 FROM _canopy_runtime_chats WHERE chat_id=?1",
            params![source_chat, integer(epoch)?],
            |r| r.get(0),
        )?;
        for table in ["_canopy_rust_chat_epochs", "_canopy_rust_chat_items"] {
            if !retained && has_table(&tx, table)? {
                retained = tx.query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE chat=?1 AND epoch=?2)"),
                    params![source_chat, integer(epoch)?],
                    |r| r.get(0),
                )?;
            }
        }
        if !retained {
            return Err(Error::ResourceConflict);
        }

        let source_root = chat_root(&tx, &source_chat, &key)?;
        authorize_root(&tx, &key, 2, &source_root, true)?;
        let occupied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _canopy_runtime_chats WHERE chat_id=?1)",
            [&record.id],
            |r| r.get(0),
        )?;
        if occupied {
            return Err(Error::ResourceConflict);
        }
        let chat = record.id.clone();
        save_chat(&tx, record)?;
        tx.execute("INSERT INTO _canopy_artifact_refs(artifact_id,owner_kind,owner_id) SELECT artifact_id,'chat',?2 FROM _canopy_artifact_refs WHERE owner_kind='chat' AND owner_id=?1", params![source_chat,chat])?;
        let replacement_key = replacement.as_ref().map(|(input, _, _)| input.key.clone());
        if let Some((input, message, draft)) = replacement {
            if input.key.device != key.device
                || input.key.epoch != key.epoch
                || input.key.id == key.id
            {
                return Err(Error::CommandConflict);
            }
            accept_message(&tx, input, chat.clone(), message, draft, None, host::CHAT)?;
        }
        check_result_budget(&tx, &key, Some(chat.as_bytes()))?;
        tx.execute("UPDATE _canopy_command_inbox SET state='succeeded',result=?3,updated_at=?4 WHERE device_id=?1 AND command_id=?2 AND resource=?5 AND executor_epoch=?6 AND command_epoch=?7", params![key.device,key.id,chat.as_bytes(),now(),key.resource,integer(key.executor_epoch)?,integer(key.epoch)?])?;
        let result = receipt(&tx, &key)?.ok_or(Error::InvalidRuntimeState)?;
        let started = Instant::now();
        tx.commit()?;
        metrics::record(
            &diagnostic_host,
            &chat,
            Some(key.id()),
            0,
            Stage::Commit,
            started,
            Outcome::Ok,
        );
        metrics::command_transition(&diagnostic_host, &chat, &key, Outcome::Succeeded);
        if let Some(key) = replacement_key {
            metrics::command_transition(&diagnostic_host, &chat, &key, Outcome::Accepted);
        }
        Ok(result)
    }
}

pub(super) fn live_chat(db: &Connection, chat: &str) -> Result<()> {
    let recovery: Option<bool> = db.query_row("SELECT recovery_reason IS NOT NULL OR authority_root='' FROM _canopy_runtime_chats WHERE chat_id=?1 AND closed_at IS NULL", [chat], |r| r.get(0)).optional()?;
    match recovery {
        Some(false) => Ok(()),
        Some(true) => Err(Error::ResourceConflict),
        None => Err(Error::NotFound),
    }
}

#[cfg(test)]
mod tests;
