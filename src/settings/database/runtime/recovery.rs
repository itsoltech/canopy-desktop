use super::*;

impl Database {
    pub fn recover_runtime_chat(
        &mut self,
        mut input: CommandInput,
        chat: ChatRecord,
        expected_reason: String,
    ) -> Result<CommandReceipt> {
        require_host(self)?;
        let normalized = chat
            .recovery_command(input.key.clone(), &expected_reason)
            .ok_or(Error::InvalidRuntimeState)?;
        presence::same_intent(&input, &normalized)?;
        if ChatRecord::new(
            chat.id.clone(),
            chat.workspace.clone(),
            chat.agent.clone(),
            chat.metadata.clone(),
        )
        .is_none()
            || chat.metadata.kind != crate::state::workspace::PaneKind::Chat
            || chat.recovery_reason.is_some()
            || !host::valid_authority_root(&chat.authority_root)
            || chat.workspace.is_some() && chat.authority_root == "private"
        {
            return Err(Error::InvalidRuntimeState);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        input.root = Some(chat.authority_root.clone());
        authorize_root(&tx, &input.key, host::CHAT, command_root(&input)?, true)?;
        if let Some(receipt) = existing(&tx, &input)? {
            return Ok(receipt);
        }
        let old_root = chat_root(&tx, &chat.id, &input.key)?;
        authorize_root(&tx, &input.key, host::CHAT, &old_root, true)?;
        // Validate the explicitly selected account against the current catalog,
        // never resolve through default_profile during orphan recovery.
        let catalog = super::super::read_tools(&tx)?;
        let tool = catalog
            .get(&chat.agent)
            .filter(|tool| tool.enabled && tool.is_agent())
            .ok_or(Error::InvalidRuntimeState)?;
        let profile = chat
            .metadata
            .profile_id
            .as_deref()
            .ok_or(Error::InvalidRuntimeState)?;
        if !(tool.profiles.iter().any(|p| p.id == profile)
            || tool.profiles.is_empty() && profile == tool.id)
        {
            return Err(Error::InvalidRuntimeState);
        }
        let payload =
            serde_json::to_string(&chat.metadata).map_err(|_| Error::InvalidRuntimeState)?;
        if payload.len() > host::CONTROL_BYTES {
            return Err(Error::ResourceLimit);
        }
        let changed = tx.execute("UPDATE _canopy_runtime_chats SET workspace=?2,agent_id=?3,profile_id=?4,metadata=?5,authority_root=?8,recovery_reason=NULL WHERE chat_id=?1 AND recovery_reason=?6 AND history_epoch=?7 AND closed_at IS NULL", params![chat.id,chat.workspace,chat.agent,profile,payload,expected_reason,integer(chat.history_epoch)?,chat.authority_root])?;
        if changed == 0 {
            return Err(Error::ResourceConflict);
        }
        let receipt = insert_command(
            &tx,
            &input,
            CommandState::Succeeded,
            Some(chat.id.as_bytes()),
            host::CHAT,
        )?;
        tx.commit()?;
        Ok(receipt)
    }
}
