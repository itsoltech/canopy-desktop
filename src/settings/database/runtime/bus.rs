//! BUS/CMD admission and DB-only effects, serialized by the existing writer.
use super::*;
use crate::command_bus::{
    Command, CommandEnvelope, Dispatch, PreparedCommand, Principal, ResourceRef,
};
#[cfg(test)]
mod tests;

impl Database {
    pub(crate) fn prepare_bus_catalog(
        &mut self,
        mut envelope: CommandEnvelope,
        principal: Principal,
        resolution: Option<(String, String)>,
    ) -> Result<Option<PreparedCommand>> {
        require_host(self)?;
        if envelope.resource.host_id().as_str() != self.runtime_host()?.id() {
            return Err(Error::NotFound);
        }
        match (&envelope.resource, &envelope.command) {
            (
                ResourceRef::Project(_) | ResourceRef::Environment(_),
                Command::Catalog(crate::command_bus::CatalogCommand::Create { .. }),
            )
            | (
                ResourceRef::Environment(_),
                Command::Catalog(crate::command_bus::CatalogCommand::OpenProject { .. }),
            ) => (),
            _ => return Err(Error::InvalidRuntimeState),
        }
        let scope = envelope.command.required_scope();
        let mut input = envelope.input(&principal, "catalog".into())?;
        authorize(&self.db, &input.key, scope, true)?;
        let saved: Option<(String,String,u32)> = self.db.query_row(
            "SELECT target,required_root,required_scope FROM _canopy_command_inbox WHERE device_id=?1 AND command_id=?2 AND resource=?3 AND executor_epoch=?4",
            params![input.key.device,input.key.id,input.key.resource,integer(input.key.executor_epoch)?], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let prior = if let Some((target, root, saved_scope)) = saved {
            authorize_root(&self.db, &input.key, saved_scope, &root, true)?;
            input.target = target;
            input.root = Some(root);
            existing(&self.db, &input)?
        } else if let Some((target, root)) = resolution {
            input.target = target;
            input.root = Some(root);
            authorize_root(&self.db, &input.key, scope, command_root(&input)?, true)?;
            None
        } else {
            return Ok(None);
        };
        Ok(Some(PreparedCommand {
            input,
            envelope,
            record: None,
            prior,
        }))
    }
    pub(crate) fn prepare_bus_command(
        &mut self,
        mut envelope: CommandEnvelope,
        principal: Principal,
    ) -> Result<PreparedCommand> {
        require_host(self)?;
        if envelope.resource.host_id().as_str() != self.runtime_host()?.id() {
            return Err(Error::NotFound);
        }
        let (target, record) = match &envelope.resource {
            ResourceRef::Thread { thread, .. } => {
                if matches!(
                    envelope.command,
                    Command::Preferences { .. }
                        | Command::Catalog(
                            crate::command_bus::CatalogCommand::Create { .. }
                                | crate::command_bus::CatalogCommand::OpenProject { .. }
                        )
                ) {
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
        let mut prior = existing(&self.db, &input)?;
        if prior.is_none()
            && let ResourceRef::Thread { executor_epoch, .. } = &envelope.resource
            && record
                .as_ref()
                .and_then(ChatRecord::thread)
                .is_some_and(|stored| stored.executor_epoch() != *executor_epoch)
        {
            let tx = self
                .db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            prior = Some(reject_conditions(&tx, &input, scope)?);
            tx.commit()?;
        }
        if prior.is_none() && matches!(envelope.resource, ResourceRef::Thread { .. }) {
            chat_commands::live_chat(&self.db, &input.target)?;
            if private_fork(&envelope.command, record.as_ref().ok_or(Error::NotFound)?) {
                let tx = self
                    .db
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                prior = Some(reject_private_fork(&tx, &input, scope)?);
                tx.commit()?;
            }
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
        if let Err(error) = validate_dispatch(&tx, &prepared) {
            if !matches!(error, Error::ResourceConflict) {
                return Err(error);
            }
            let receipt = reject_conditions(
                &tx,
                &prepared.input,
                prepared.envelope.command.required_scope(),
            )?;
            tx.commit()?;
            return Ok(receipt);
        }
        if let Some(prior) = existing(&tx, &prepared.input)? {
            return Ok(prior);
        }
        let input = prepared.input;
        let chat = input.target.clone();
        let scope = prepared.envelope.command.required_scope();
        // Queue validation can fail after bumping its revision. Roll back every
        // tentative effect before persisting a domain rejection in the same tx.
        tx.execute_batch("SAVEPOINT bus_effect")?;
        let result = (|| -> Result<CommandReceipt> {
            Ok(match dispatch {
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
                    accept_message(
                        &tx,
                        input.clone(),
                        chat,
                        message,
                        draft,
                        Some(intent),
                        scope,
                    )?
                }
                Dispatch::Queue { revision, mutation } => {
                    mutate_queue(&tx, input.clone(), chat, revision, mutation, scope)?
                }
            })
        })();
        let result = match result {
            Err(Error::ResourceConflict) => {
                tx.execute_batch("ROLLBACK TO bus_effect; RELEASE bus_effect")?;
                reject_conditions(&tx, &input, scope)?
            }
            result => {
                let receipt = result?;
                tx.execute_batch("RELEASE bus_effect")?;
                receipt
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
            let receipt = reject_conditions(
                &tx,
                &prepared.input,
                prepared.envelope.command.required_scope(),
            )?;
            tx.commit()?;
            return Ok((receipt, None));
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

fn reject_conditions(tx: &Transaction, input: &CommandInput, scope: u32) -> Result<CommandReceipt> {
    let result = br#"{"reason":"conditions_changed: The resource changed; review its current state before a new command."}"#;
    insert_command(tx, input, CommandState::Rejected, Some(result), scope)
}

fn private_fork(command: &Command, record: &ChatRecord) -> bool {
    matches!(
        command,
        Command::Control(crate::command_bus::Control::Fork { .. })
    ) && record.workspace().is_none()
}

fn reject_private_fork(
    tx: &Transaction,
    input: &CommandInput,
    scope: u32,
) -> Result<CommandReceipt> {
    insert_command(
        tx,
        input,
        CommandState::Rejected,
        Some(br#"{"reason":"unsupported: Fork requires a project thread."}"#),
        scope,
    )
}

/// Runtime handlers consume the bus's original input, then fence it again in
/// their acceptance transaction. No adapter can relax attachment permissions.
pub(super) fn runtime_admission(
    tx: &Transaction,
    input: &CommandInput,
    fallback_scope: u32,
) -> Result<Option<CommandReceipt>> {
    let intent = if input.key.resource.is_empty() {
        None
    } else {
        Some(serde_json::from_slice::<(String, String, u64, Command, crate::command_bus::Conditions)>(&input.arguments)
            .map_err(|_| Error::InvalidRuntimeState)?)
    };
    let scope = intent
        .as_ref()
        .map_or(fallback_scope, |(_, _, _, command, _)| {
            command.required_scope()
        });
    authorize_root(tx, &input.key, scope, command_root(input)?, true)?;
    if let Some(prior) = existing(tx, input)? {
        return Ok(Some(prior));
    }
    if let Some((method, resource, epoch, command, conditions)) = intent {
        if method != input.method
            || command.metadata().name != input.method
            || resource != input.key.resource
            || epoch != input.key.executor_epoch
        {
            return Err(Error::InvalidRuntimeState);
        }
        if matches!(
            command,
            Command::Catalog(
                crate::command_bus::CatalogCommand::Create { .. }
                    | crate::command_bus::CatalogCommand::OpenProject { .. }
            )
        ) {
            return Ok(None);
        }
        let record = load_chat(tx, "chat_id=?1 AND closed_at IS NULL", &input.target)?
            .ok_or(Error::NotFound)?;
        authorize_root(tx, &input.key, scope, record.authority_root(), true)?;
        let thread = record.thread().ok_or(Error::InvalidRuntimeState)?;
        if thread.executor_epoch() != epoch
            || record.authority_root() != command_root(input)?
            || conditions
                .expected_revision
                .is_some_and(|r| r != thread.revision())
        {
            return reject_conditions(tx, input, scope).map(Some);
        }
        if private_fork(&command, &record) {
            return reject_private_fork(tx, input, scope).map(Some);
        }
        chat_commands::live_chat(tx, &input.target)?;
    }
    Ok(None)
}

pub(super) fn runtime_scope(input: &CommandInput, fallback: u32) -> Result<u32> {
    if input.key.resource.is_empty() {
        return Ok(fallback);
    }
    let (_, _, _, command, _): (String, String, u64, Command, crate::command_bus::Conditions) =
        serde_json::from_slice(&input.arguments).map_err(|_| Error::InvalidRuntimeState)?;
    Ok(command.required_scope())
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
