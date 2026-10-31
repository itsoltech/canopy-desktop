//! BUS/CMD admission and DB-only effects, serialized by the existing writer.
use super::*;
use crate::command_bus::{
    Command, CommandEnvelope, Dispatch, PreparedCommand, Principal, ResourceRef,
};
#[cfg(test)]
mod tests;

impl Database {
    pub(crate) fn prepare_bus_command(
        &self,
        mut envelope: CommandEnvelope,
        principal: Principal,
    ) -> Result<PreparedCommand> {
        require_host(self)?;
        if envelope.resource.host_id().as_str() != self.runtime_host()?.id() {
            return Err(Error::NotFound);
        }
        let (target, record) = match &envelope.resource {
            ResourceRef::Thread { thread, .. } => {
                if matches!(envelope.command, Command::Preferences { .. }) {
                    return Err(Error::InvalidRuntimeState);
                }
                let target: String = self.db.query_row("SELECT legacy_chat_id FROM _canopy_thread_alias WHERE legacy_host_id=?1 AND thread_id=?2", params![thread.host_id.as_str(),thread.thread_id.to_string()], |r| r.get(0)).optional()?.ok_or(Error::NotFound)?;
                let record = load_chat(&self.db, "chat_id=?1", &target)?;
                (target, record)
            }
            ResourceRef::Environment(_)
                if matches!(envelope.command, Command::Preferences { .. }) =>
            {
                ("private".into(), None)
            }
            _ => return Err(Error::InvalidRuntimeState),
        };
        let mut input = envelope.input(&principal, target)?;
        let scope = envelope.command.required_scope();
        // The current grant precedes saved-result or conflict disclosure. A saved
        // root is retained even when the target has been removed from the catalog.
        input.root = Some(
            if matches!(envelope.resource, ResourceRef::Environment(_)) {
                "private".into()
            } else {
                chat_root(&self.db, &input.target, &input.key)?
            },
        );
        authorize_root(&self.db, &input.key, scope, command_root(&input)?, true)?;
        let prior = existing(&self.db, &input)?;
        if prior.is_none()
            && let ResourceRef::Thread { executor_epoch, .. } = &envelope.resource
            && record
                .as_ref()
                .and_then(ChatRecord::thread)
                .is_some_and(|stored| stored.executor_epoch() != *executor_epoch)
        {
            return Err(Error::ResourceConflict);
        }
        if prior.is_none() && matches!(envelope.resource, ResourceRef::Thread { .. }) {
            chat_commands::live_chat(&self.db, &input.target)?;
        }
        Ok(PreparedCommand {
            input,
            envelope,
            record,
            prior,
        })
    }

    pub(crate) fn dispatch_bus_command(
        &mut self,
        prepared: PreparedCommand,
        dispatch: Dispatch,
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        validate_dispatch(&tx, &prepared)?;
        if let Some(prior) = existing(&tx, &prepared.input)? {
            return Ok(prior);
        }
        let input = prepared.input;
        let chat = input.target.clone();
        let scope = prepared.envelope.command.required_scope();
        let result = match dispatch {
            Dispatch::Rename(title) => {
                let mut record = load_chat(&tx, "chat_id=?1 AND closed_at IS NULL", &chat)?
                    .ok_or(Error::NotFound)?;
                record.metadata.title = Some(title);
                record.metadata.chat_title_state =
                    Some(crate::state::workspace::ChatTitleState::Named);
                let metadata = serde_json::to_string(&record.metadata)
                    .map_err(|_| Error::InvalidRuntimeState)?;
                tx.execute("UPDATE _canopy_runtime_chats SET metadata=?2,revision=revision+(metadata IS NOT ?2) WHERE chat_id=?1",params![chat,metadata])?;
                let record = load_chat(&tx, "chat_id=?1", &chat)?.ok_or(Error::NotFound)?;
                let result = serde_json::to_vec(&serde_json::json!({"title": record.metadata.title, "revision": record.thread().ok_or(Error::InvalidRuntimeState)?.revision()})).map_err(|_| Error::InvalidRuntimeState)?;
                insert_command(
                    &tx,
                    &input,
                    CommandState::Succeeded,
                    Some(&result),
                    prepared.envelope.command.required_scope(),
                )?
            }
            Dispatch::Send {
                message,
                draft,
                context,
                steer_run,
            } => {
                let intent = host::MessageIntent {
                    message_id: message.clone(),
                    draft: draft.clone(),
                    context,
                    steer_run,
                };
                accept_message(&tx, input, chat, message, draft, Some(intent), scope)?
            }
            Dispatch::Queue { revision, mutation } => {
                mutate_queue(&tx, input, chat, revision, mutation, scope)?
            }
        };
        tx.commit()?;
        Ok(result)
    }

    /// CMD-08/BUS-04: preference values, exact resulting owner revision and receipt
    /// commit together. The app reserves the combined owner until publication.
    pub(crate) fn commit_bus_preferences(
        &mut self,
        prepared: PreparedCommand,
        revision: u64,
    ) -> Result<(CommandReceipt, Option<crate::settings::Snapshot>)> {
        require_host(self)?;
        let Command::Preferences { changes } = &prepared.envelope.command else {
            return Err(Error::InvalidRuntimeState);
        };
        let changes = changes
            .iter()
            .map(crate::command_bus::PreferenceWrite::decode)
            .collect::<Result<Vec<_>>>()?;
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        validate_dispatch(&tx, &prepared)?;
        if let Some(prior) = existing(&tx, &prepared.input)? {
            return Ok((prior, None));
        }
        if prepared
            .envelope
            .conditions
            .expected_revision
            .is_some_and(|expected| expected != revision)
        {
            return Err(Error::ResourceConflict);
        }
        let before = super::super::read_snapshot(&tx, super::super::schema(&tx)?)?;
        let snapshot = super::super::apply_preferences(&tx, changes)?;
        let changed = crate::command_bus::preferences::shared_preferences(before.preferences)
            != crate::command_bus::preferences::shared_preferences(snapshot.preferences.clone());
        let revision = if changed {
            revision.wrapping_add(1).max(1)
        } else {
            revision
        };
        let result = serde_json::to_vec(
            &serde_json::json!({"values":snapshot.preferences,"revision":revision}),
        )
        .map_err(|_| Error::InvalidRuntimeState)?;
        let receipt = insert_command(
            &tx,
            &prepared.input,
            CommandState::Succeeded,
            Some(&result),
            prepared.envelope.command.required_scope(),
        )?;
        tx.commit()?;
        Ok((receipt, Some(snapshot)))
    }
}

fn validate_dispatch(db: &Connection, prepared: &PreparedCommand) -> Result<()> {
    let scope = prepared.envelope.command.required_scope();
    authorize_root(
        db,
        &prepared.input.key,
        scope,
        command_root(&prepared.input)?,
        true,
    )?;
    // Authorization is rechecked before both dedupe and the domain transaction.
    if existing(db, &prepared.input)?.is_some() {
        return Ok(());
    }
    if let ResourceRef::Thread { executor_epoch, .. } = &prepared.envelope.resource {
        let record = load_chat(
            db,
            "chat_id=?1 AND closed_at IS NULL",
            &prepared.input.target,
        )?
        .ok_or(Error::NotFound)?;
        authorize_root(
            db,
            &prepared.input.key,
            scope,
            record.authority_root(),
            true,
        )?;
        if record.authority_root() != command_root(&prepared.input)? {
            return Err(Error::ResourceConflict);
        }
        let thread = record.thread().ok_or(Error::InvalidRuntimeState)?;
        if thread.executor_epoch() != *executor_epoch
            || prepared
                .envelope
                .conditions
                .expected_revision
                .is_some_and(|revision| thread.revision() != revision)
        {
            return Err(Error::ResourceConflict);
        }
        chat_commands::live_chat(db, &prepared.input.target)?;
    }
    Ok(())
}
