use super::{model::Edit, *};
use crate::host::{CommandInput, CommandKey, CommandState, DeviceGrant};
use prost::Message;
use std::collections::BTreeMap;

pub(super) enum Write {
    Preferences(proto::UpdatePreferencesRequest),
    Agent(proto::SaveAgentDefinitionRequest),
    RemoveAgent(proto::RemoveAgentDefinitionRequest),
    Profile(Box<proto::SaveProfileRequest>),
    RemoveProfile(proto::RemoveProfileRequest),
    Integration(proto::SaveIntegrationRequest),
    RemoveIntegration(proto::RemoveIntegrationRequest),
}
fn invalid() -> Status {
    Status::invalid_argument("Invalid settings update.")
}
fn text(s: &str, max: usize, empty: bool) -> Result<(), Status> {
    if (!empty && s.trim().is_empty()) || s.len() > max || s.contains('\0') {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn identity(s: &str) -> Result<(), Status> {
    text(s, 256, false)
}
fn environments(v: &mut [proto::EnvironmentUpdate]) -> Result<(), Status> {
    if v.len() > 128 {
        return Err(invalid());
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    for e in v.iter_mut() {
        if e.name.is_empty() || e.name.len() > 256 || e.name.contains(['=', '\0']) {
            return Err(invalid());
        }
        normalize_secret(&mut e.value, true, 2560)?;
    }
    if v.windows(2).any(|v| v[0].name == v[1].name) {
        return Err(invalid());
    }
    Ok(())
}
fn normalize_secret(
    v: &mut Option<proto::SecretUpdate>,
    allow_empty: bool,
    limit: usize,
) -> Result<(), Status> {
    use proto::secret_update::Action;
    match v.as_ref().and_then(|v| v.action.as_ref()) {
        None | Some(Action::Unchanged(_)) => {
            *v = Some(proto::SecretUpdate {
                action: Some(Action::Unchanged(proto::Empty {})),
            })
        }
        Some(Action::Set(b)) => {
            let s = std::str::from_utf8(b).map_err(|_| invalid())?;
            text(s, limit, allow_empty)?;
        }
        Some(Action::Remove(_)) => {}
    }
    Ok(())
}
pub(super) fn validate_url(value: &str) -> Result<(), Status> {
    if value.is_empty() {
        return Ok(());
    }
    let url = gpui_kit::http_client::Url::parse(value).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Status::invalid_argument(
            "Endpoint must be an HTTP URL without embedded credentials, query or fragment.",
        ));
    }
    Ok(())
}
fn validate_options(o: &mut proto::ProfileAgentSettings) -> Result<(), Status> {
    for s in [
        &o.permission_mode,
        &o.effort_level,
        &o.base_url,
        &o.provider,
        &o.approval_mode,
        &o.sandbox,
        &o.config_profile,
    ]
    .into_iter()
    .flatten()
    {
        text(s, 65536, true)?;
    }
    if let Some(url) = &o.base_url {
        validate_url(url)?;
    }
    o.system_prompt_configured = false;
    o.settings_override_configured = false;
    Ok(())
}
impl Write {
    pub(super) fn profile(request: proto::SaveProfileRequest) -> Self {
        Self::Profile(Box::new(request))
    }
    fn prepare(&mut self) -> Result<(proto::CommandMeta, &'static str, String, Vec<u8>), Status> {
        macro_rules! finish {
            ($r:expr,$method:expr,$target:expr) => {{
                let meta = $r.command.take().ok_or_else(invalid)?;
                Ok((meta, $method, $target, $r.encode_to_vec()))
            }};
        }
        match self {
            Self::Preferences(r) => {
                for s in [&r.new_tab_tool, &r.new_worktree_tool]
                    .into_iter()
                    .flatten()
                {
                    identity(s)?;
                }
                for value in [&r.chat_profile, &r.chat_model, &r.chat_effort, &r.chat_mode]
                    .into_iter()
                    .flatten()
                {
                    text(value, 256, true)?;
                }
                finish!(r, "settings.preferences", "private".into())
            }
            Self::Agent(r) => {
                let a = r.agent.as_mut().ok_or_else(invalid)?;
                identity(&a.id)?;
                text(&a.name, 256, false)?;
                text(&a.command, 16384, false)?;
                if !matches!(
                    proto::BackendKind::try_from(a.backend_kind),
                    Ok(proto::BackendKind::ClaudeStreamJson
                        | proto::BackendKind::CodexAppServer
                        | proto::BackendKind::Acp)
                ) {
                    return Err(invalid());
                }
                if a.arguments.len() > 256 {
                    return Err(invalid());
                }
                for s in &a.arguments {
                    text(s, 16384, true)?;
                }
                if let Some(id) = &a.default_profile_id {
                    text(id, 256, true)?;
                }
                a.environment.clear();
                let id = a.id.clone();
                environments(&mut r.environment)?;
                finish!(r, "settings.agent.save", id)
            }
            Self::RemoveAgent(r) => {
                identity(&r.agent_id)?;
                finish!(r, "settings.agent.remove", r.agent_id.clone())
            }
            Self::Profile(r) => {
                let p = r.profile.as_mut().ok_or_else(invalid)?;
                identity(&p.id)?;
                identity(&p.agent_id)?;
                text(&p.name, 256, false)?;
                if let Some(model) = &p.model {
                    text(model, 256, true)?;
                }
                if let Some(args) = &p.arguments {
                    if args.values.len() > 256 {
                        return Err(invalid());
                    }
                    for arg in &args.values {
                        text(arg, 16384, true)?;
                    }
                }
                // This is read metadata; writes are explicit in request.settings.
                p.settings = None;
                p.environment.clear();
                p.credential_configured = false;
                let id = p.id.clone();
                environments(&mut r.environment)?;
                normalize_secret(&mut r.credential, false, 2560)?;
                if let Some(settings) = &mut r.settings {
                    if let Some(options) = &mut settings.options {
                        validate_options(options)?;
                    }
                    normalize_secret(&mut settings.append_system_prompt, true, 65536)?;
                    normalize_secret(&mut settings.settings_json, true, 65536)?;
                    if let Some(proto::secret_update::Action::Set(value)) = settings
                        .settings_json
                        .as_ref()
                        .and_then(|v| v.action.as_ref())
                        && !std::str::from_utf8(value)
                            .map_err(|_| invalid())?
                            .trim()
                            .is_empty()
                        && !serde_json::from_slice::<serde_json::Value>(value)
                            .is_ok_and(|v| v.is_object())
                    {
                        return Err(invalid());
                    }
                }
                finish!(r, "settings.profile.save", id)
            }
            Self::RemoveProfile(r) => {
                identity(&r.profile_id)?;
                finish!(r, "settings.profile.remove", r.profile_id.clone())
            }
            Self::RemoveIntegration(r) => {
                identity(&r.integration_id)?;
                finish!(r, "settings.integration.remove", r.integration_id.clone())
            }
            Self::Integration(r) => {
                let i = r.integration.as_mut().ok_or_else(invalid)?;
                identity(&i.id)?;
                text(&i.name, 256, false)?;
                text(&i.endpoint, 16384, true)?;
                validate_url(&i.endpoint)?;
                for value in [&i.owner, &i.email, &i.cloud_id].into_iter().flatten() {
                    text(value, 4096, true)?;
                }
                if !matches!(i.kind.as_str(), "github" | "jira" | "youtrack") {
                    return Err(invalid());
                }
                i.credential_configured = false;
                let id = i.id.clone();
                normalize_secret(&mut r.credential, false, 2560)?;
                finish!(r, "settings.integration.save", id)
            }
        }
    }
    async fn edit(
        self,
        secrets: &dyn SettingsSecrets,
        key: &CommandKey,
        host: &str,
    ) -> Result<(Edit, Vec<(SecretVersion, String)>), Status> {
        let mut staged = Vec::new();
        let edit = match self {
            Self::Preferences(r) => Edit::Preferences(PreferencesPatch::from_request(r)),
            Self::RemoveAgent(r) => Edit::RemoveAgent(r.agent_id),
            Self::RemoveProfile(r) => Edit::RemoveProfile(r.profile_id),
            Self::RemoveIntegration(r) => Edit::RemoveIntegration(r.integration_id),
            Self::Agent(r) => {
                let a = r.agent.ok_or_else(invalid)?;
                let backend = match proto::BackendKind::try_from(a.backend_kind)
                    .map_err(|_| invalid())?
                {
                    proto::BackendKind::ClaudeStreamJson => crate::chat::agent::BackendKind::Claude,
                    proto::BackendKind::CodexAppServer => crate::chat::agent::BackendKind::Codex,
                    proto::BackendKind::Acp => crate::chat::agent::BackendKind::Acp,
                    _ => return Err(invalid()),
                };
                let env = env_changes(r.environment, secrets, key, host, &mut staged).await?;
                Edit::Agent(AgentEdit::new(a, backend, env))
            }
            Self::Profile(r) => {
                let env = env_changes(r.environment, secrets, key, host, &mut staged).await?;
                let credential =
                    secret_change(r.credential, "credential", secrets, key, host, &mut staged)
                        .await?;
                let settings = if let Some(s) = r.settings {
                    let prompt = secret_change(
                        s.append_system_prompt,
                        "system_prompt",
                        secrets,
                        key,
                        host,
                        &mut staged,
                    )
                    .await?;
                    let json = secret_change(
                        s.settings_json,
                        "settings_json",
                        secrets,
                        key,
                        host,
                        &mut staged,
                    )
                    .await?;
                    Some(ProfileSettingsEdit::new(s.options, prompt, json))
                } else {
                    None
                };
                Edit::Profile(Box::new(ProfileEdit::new(
                    r.profile.ok_or_else(invalid)?,
                    env,
                    credential,
                    settings,
                )))
            }
            Self::Integration(r) => {
                let credential =
                    secret_change(r.credential, "credential", secrets, key, host, &mut staged)
                        .await?;
                Edit::Integration(IntegrationEdit::new(
                    r.integration.ok_or_else(invalid)?,
                    credential,
                ))
            }
        };
        Ok((edit, staged))
    }
}
async fn env_changes(
    values: Vec<proto::EnvironmentUpdate>,
    secrets: &dyn SettingsSecrets,
    key: &CommandKey,
    host: &str,
    staged: &mut Vec<(SecretVersion, String)>,
) -> Result<BTreeMap<String, SecretChange>, Status> {
    let mut result = BTreeMap::new();
    for e in values {
        let change = secret_change(
            e.value,
            &format!("env:{}", e.name),
            secrets,
            key,
            host,
            staged,
        )
        .await?;
        result.insert(e.name, change);
    }
    Ok(result)
}
async fn secret_change(
    v: Option<proto::SecretUpdate>,
    slot: &str,
    secrets: &dyn SettingsSecrets,
    key: &CommandKey,
    host: &str,
    staged: &mut Vec<(SecretVersion, String)>,
) -> Result<SecretChange, Status> {
    use proto::secret_update::Action;
    Ok(match v.and_then(|v| v.action) {
        None | Some(Action::Unchanged(_)) => SecretChange::Unchanged,
        Some(Action::Remove(_)) => SecretChange::Remove,
        Some(Action::Set(bytes)) => {
            let input =
                serde_json::to_vec(&(host, key.device(), key.epoch(), key.id(), slot, &bytes))
                    .map_err(|_| invalid())?;
            let digest = secrets.fingerprint(&input).await?;
            let mut id = [0; 16];
            id.copy_from_slice(&digest[..16]);
            let value = String::from_utf8(bytes).map_err(|_| invalid())?;
            let version = SecretVersion::for_value(uuid::Uuid::from_bytes(id).to_string(), &value);
            staged.push((version.clone(), value));
            SecretChange::Set(version)
        }
    })
}
impl SettingsRpc {
    pub(super) async fn write(
        &self,
        grant: DeviceGrant,
        mut write: Write,
    ) -> Result<proto::CommandReceipt, Status> {
        let (meta, method, target, canonical) = write.prepare()?;
        if canonical.len() > crate::host::CONTROL_BYTES {
            return Err(Status::resource_exhausted(
                "Settings request exceeds control limit.",
            ));
        }
        let key = self.auth.key(&meta, &grant)?;
        let operation = self.locks.begin()?;
        let this = self.clone();
        tokio::spawn(async move {
            let _operation = operation;
            let _gate = this.gate.lock().await;
            this.auth.resource(grant.id(), ADMIN, "private").await?;
            let canonical =
                serde_json::to_vec(&(meta.expected_revision, canonical)).map_err(|_| invalid())?;
            let fingerprint = this.secrets.fingerprint(&canonical).await?;
            let (edit, staged) = write
                .edit(this.secrets.as_ref(), &key, this.auth.host_id())
                .await?;
            let args = serde_json::to_vec(&edit).map_err(|_| invalid())?;
            let input = CommandInput::new(
                key.clone(),
                method.into(),
                target,
                args,
                fingerprint.to_vec(),
            )
            .and_then(|i| i.with_root("private".into()))
            .ok_or_else(invalid)?;
            let existing = this
                .auth
                .client()
                .get_runtime_command(key.clone(), ADMIN)
                .await
                .map_err(storage_status)?;
            if existing.is_some() {
                // Dedupe still compares the normalized intent and the current authority.
                let receipt = this
                    .auth
                    .client()
                    .accept_runtime_command(input.clone(), ADMIN)
                    .await
                    .map_err(storage_status)?;
                if receipt.state() != CommandState::Accepted {
                    return Ok(this.auth.receipt(&receipt));
                }
            }
            for (version, value) in &staged {
                for id in version.parts() {
                    this.auth
                        .client()
                        .reserve_remote_secret(key.clone(), id.into(), fingerprint.to_vec())
                        .await
                        .map_err(storage_status)?;
                }
                this.secrets.stage(version, value).await?;
            }
            let accepted = this
                .auth
                .client()
                .accept_runtime_command(input, ADMIN)
                .await
                .map_err(storage_status)?;
            if accepted.state() != CommandState::Accepted {
                return Ok(this.auth.receipt(&accepted));
            }
            this.auth
                .client()
                .transition_runtime_command(
                    key.clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
                .map_err(storage_status)?;
            let host = this.host.clone();
            let outcome = tokio::spawn(async move {
                match edit {
                    Edit::Preferences(e) => {
                        host.update_preferences(e, meta.expected_revision).await
                    }
                    Edit::Agent(e) => host.save_agent(e, meta.expected_revision).await,
                    Edit::RemoveAgent(id) => host.remove_agent(id, meta.expected_revision).await,
                    Edit::Profile(e) => host.save_profile(*e, meta.expected_revision).await,
                    Edit::RemoveProfile(id) => {
                        host.remove_profile(id, meta.expected_revision).await
                    }
                    Edit::Integration(e) => host.save_integration(e, meta.expected_revision).await,
                    Edit::RemoveIntegration(id) => {
                        host.remove_integration(id, meta.expected_revision).await
                    }
                }
            })
            .await
            .unwrap_or_else(|_| Err(Status::unknown("Settings owner stopped during dispatch.")));
            let (state, result) = match outcome {
                Ok(()) => (CommandState::Succeeded, b"{}".to_vec()),
                Err(error) => {
                    let unknown = matches!(
                        error.code(),
                        tonic::Code::Unknown | tonic::Code::Unavailable | tonic::Code::Cancelled
                    );
                    let code = if unknown {
                        "delivery_unknown"
                    } else if error.message().starts_with("needs_host_interaction") {
                        "needs_host_interaction"
                    } else if error.code() == tonic::Code::Aborted {
                        "conflict"
                    } else if error.code() == tonic::Code::InvalidArgument {
                        "invalid_argument"
                    } else if error.code() == tonic::Code::ResourceExhausted {
                        "resource_limit"
                    } else {
                        "operation_failed"
                    };
                    // Host errors may include provider/config values. Return a bounded generic message.
                    (
                        if unknown {
                            CommandState::DeliveryUnknown
                        } else {
                            CommandState::Failed
                        },
                        proto::DomainError {
                            code: code.into(),
                            message: if unknown {
                                "Settings outcome requires host reconciliation."
                            } else if code == "needs_host_interaction" {
                                "Unlock or reconnect the credential store on the host."
                            } else {
                                "The host could not apply these settings."
                            }
                            .into(),
                            retryable: false,
                        }
                        .encode_to_vec(),
                    )
                }
            };
            let receipt = this
                .auth
                .client()
                .transition_runtime_command(key, CommandState::Attempting, state, Some(result))
                .await
                .map_err(storage_status)?;
            // Cleanup failure remains durably queued and cannot change the committed receipt.
            let _ = this.cleanup_locked().await;
            Ok(this.auth.receipt(&receipt))
        })
        .await
        .map_err(|_| Status::unavailable("Settings outcome requires reconciliation."))?
    }
}
