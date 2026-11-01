use super::*;
use canopy_desktop::{
    integrations as i,
    remote::settings_rpc::ToolBackend,
    state::{
        agent_settings::AgentSettings,
        tools::{Profile, ToolDefinition, ToolKind},
    },
};
use std::collections::{BTreeMap, HashMap};

#[expect(
    clippy::large_enum_variant,
    reason = "Profile edits carry their complete settings DTO through the 64-slot control queue; boxing would add an allocation per edit."
)]
pub(super) enum Edit {
    Agent(AgentEdit, BTreeMap<String, Value>),
    Profile(ProfileEdit, BTreeMap<String, Value>, Value, Value, Value),
    RemoveAgent(String),
    RemoveProfile(String),
    Integration(IntegrationEdit, Value),
    RemoveIntegration(String),
}
pub(super) enum Value {
    Unchanged,
    Set(String),
    Remove,
}
impl ControlBridge {
    async fn value(&self, value: &SecretChange) -> Result<Value, Status> {
        match value {
            SecretChange::Unchanged => Ok(Value::Unchanged),
            SecretChange::Remove => Ok(Value::Remove),
            SecretChange::Set(version) => self.secrets.read(version).await.map(Value::Set),
        }
    }
    async fn environment(
        &self,
        values: &BTreeMap<String, SecretChange>,
    ) -> Result<BTreeMap<String, Value>, Status> {
        let mut resolved = BTreeMap::new();
        for (key, value) in values {
            resolved.insert(key.clone(), self.value(value).await?);
        }
        Ok(resolved)
    }
}
#[tonic::async_trait]
impl SettingsHost for ControlBridge {
    async fn snapshot(&self) -> Result<canopy_desktop::api::proto::HostSettings, Status> {
        self.request(Op::Snapshot).await
    }
    async fn update_preferences(
        &self,
        envelope: canopy_desktop::command_bus::CommandEnvelope,
        principal: canopy_desktop::command_bus::Principal,
    ) -> Result<canopy_desktop::host::CommandReceipt, Status> {
        let client = self
            .client
            .clone()
            .ok_or_else(|| busy("Storage is unavailable."))?;
        let prepared = canopy_desktop::command_bus::CommandBus::new(client)
            .prepare(envelope, principal)
            .await
            .map_err(canopy_desktop::remote::auth::storage_status)?;
        if let Some(prior) = prepared.prior() {
            return Ok(prior.clone());
        }
        self.request(|reply| Op::Preferences(prepared, reply)).await
    }
    async fn save_agent(&self, edit: AgentEdit, expected: Option<u64>) -> Result<(), Status> {
        let env = self.environment(edit.environment()).await?;
        self.edit(Edit::Agent(edit, env), expected).await
    }
    async fn save_profile(&self, edit: ProfileEdit, expected: Option<u64>) -> Result<(), Status> {
        let env = self.environment(edit.environment()).await?;
        let key = self.value(edit.credential()).await?;
        let (prompt, json) = match edit.settings() {
            Some(s) => (
                self.value(s.append_system_prompt()).await?,
                self.value(s.settings_json()).await?,
            ),
            None => (Value::Unchanged, Value::Unchanged),
        };
        self.edit(Edit::Profile(edit, env, key, prompt, json), expected)
            .await
    }
    async fn remove_agent(&self, id: String, expected: Option<u64>) -> Result<(), Status> {
        self.edit(Edit::RemoveAgent(id), expected).await
    }
    async fn remove_profile(&self, id: String, expected: Option<u64>) -> Result<(), Status> {
        self.edit(Edit::RemoveProfile(id), expected).await
    }
    async fn remove_integration(&self, id: String, expected: Option<u64>) -> Result<(), Status> {
        self.edit(Edit::RemoveIntegration(id), expected).await
    }
    async fn save_integration(
        &self,
        edit: IntegrationEdit,
        expected: Option<u64>,
    ) -> Result<(), Status> {
        let credential = self.value(edit.credential()).await?;
        self.edit(Edit::Integration(edit, credential), expected)
            .await
    }
}
fn chunks(mut value: &str, pending: &mut Vec<(String, String)>) -> Vec<String> {
    let mut ids = Vec::new();
    while !value.is_empty() {
        let mut end = value.len().min(2560);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        let id = uuid::Uuid::new_v4().to_string();
        pending.push((id.clone(), value[..end].to_owned()));
        ids.push(id);
        value = &value[end..];
    }
    ids
}
pub(super) fn environment(
    refs: &mut BTreeMap<String, Vec<String>>,
    legacy: &mut BTreeMap<String, String>,
    values: BTreeMap<String, Value>,
    pending: &mut Vec<(String, String)>,
) -> Result<(), Status> {
    for (key, value) in values {
        canopy_desktop::state::agent_settings::validate_env(&key, "").map_err(invalid)?;
        match value {
            Value::Unchanged => {}
            Value::Remove => {
                refs.remove(&key);
                legacy.remove(&key);
            }
            Value::Set(value) => {
                if value.len() > 2560 {
                    return Err(invalid("Environment value is too large."));
                }
                canopy_desktop::state::agent_settings::validate_env(&key, &value)
                    .map_err(invalid)?;
                legacy.remove(&key);
                refs.insert(key, chunks(&value, pending));
            }
        }
    }
    Ok(())
}
pub(super) fn secret_field(
    active: &mut Option<Vec<String>>,
    legacy: &mut String,
    value: Value,
    pending: &mut Vec<(String, String)>,
) -> Result<(), Status> {
    match value {
        Value::Unchanged => {}
        Value::Remove => {
            *active = None;
            legacy.clear();
        }
        Value::Set(value) => {
            if value.len() > 65536 || value.contains('\0') {
                return Err(invalid("Invalid secret setting."));
            }
            *active = Some(chunks(&value, pending));
            legacy.clear();
        }
    }
    Ok(())
}
fn options(settings: &mut AgentSettings, options: &ProfileOptions) {
    macro_rules! set { ($($field:ident),*) => { $(if let Some(value) = options.$field() { settings.$field = value.to_owned(); })* }; }
    set!(
        permission_mode,
        effort_level,
        base_url,
        provider,
        approval_mode,
        sandbox,
        config_profile
    );
    if let Some(value) = options.full_auto() {
        settings.full_auto = value;
    }
    if let Some(value) = options.bypass_approvals() {
        settings.bypass_approvals = value;
    }
}
pub(super) fn apply(edit: Edit, cx: &mut App) -> Result<Committed, Status> {
    let app = cx.global::<AppState>().clone();
    let (tx, rx) = oneshot::channel();
    let mut catalog = app.tools.read(cx).catalog.clone();
    let mut pending = Vec::new();
    let mut keys = HashMap::new();
    match edit {
        Edit::Agent(edit, env) => {
            let kind = match edit.backend() {
                ToolBackend::NativeStream => ToolKind::Claude,
                ToolBackend::NativeServer => ToolKind::Codex,
                ToolBackend::Protocol | ToolBackend::Process => ToolKind::Custom,
                ToolBackend::Shell => ToolKind::Shell,
            };
            if (kind == ToolKind::Claude && edit.id() != "claude")
                || (kind == ToolKind::Codex && edit.id() != "codex")
                || (matches!(edit.backend(), ToolBackend::Protocol | ToolBackend::Process)
                    && matches!(edit.id(), "shell" | "claude" | "codex"))
                || (kind == ToolKind::Shell && edit.id() != "shell")
            {
                return Err(invalid(
                    "Backend does not match the built-in agent identity.",
                ));
            }
            let index = catalog.tools.iter().position(|t| t.id == edit.id());
            let mut tool =
                index
                    .map(|i| catalog.tools[i].clone())
                    .unwrap_or_else(|| ToolDefinition {
                        id: edit.id().into(),
                        name: edit.name().into(),
                        kind,
                        executable: edit.command().into(),
                        arguments: edit.arguments().to_vec(),
                        enabled: true,
                        profiles: vec![],
                        default_profile: None,
                        acp: edit.backend() == ToolBackend::Protocol,
                        env_refs: BTreeMap::new(),
                    });
            tool.name = edit.name().into();
            tool.kind = kind;
            tool.acp = edit.backend() == ToolBackend::Protocol;
            tool.executable = edit.command().into();
            tool.arguments = edit.arguments().to_vec();
            if let Some(enabled) = edit.enabled() {
                tool.enabled = enabled;
            }
            if let Some(default) = edit.default_profile_id() {
                tool.default_profile = (!default.is_empty()).then(|| default.to_owned());
            }
            environment(&mut tool.env_refs, &mut BTreeMap::new(), env, &mut pending)?;
            match index {
                Some(index) => catalog.tools[index] = tool,
                None => catalog.tools.push(tool),
            }
        }
        Edit::RemoveAgent(id) => {
            let tool = catalog
                .get(&id)
                .ok_or_else(|| Status::not_found("Agent was removed."))?;
            if tool.kind != ToolKind::Custom {
                return Err(invalid(
                    "Built-in tools cannot be removed; disable the tool instead.",
                ));
            }
            catalog.tools.retain(|t| t.id != id);
        }
        Edit::Profile(edit, env, key, prompt, json) => {
            let tool = catalog
                .tools
                .iter_mut()
                .find(|t| t.id == edit.agent_id())
                .ok_or_else(|| Status::not_found("Agent was removed."))?;
            if tool.acp
                && (matches!(&key, Value::Set(_))
                    || matches!(&prompt, Value::Set(_))
                    || matches!(&json, Value::Set(_))
                    || edit.settings().and_then(|s| s.options()).is_some())
            {
                return Err(invalid(
                    "ACP profile secrets use named environment variables; native profile settings are unavailable for this backend.",
                ));
            }
            let index = tool.profiles.iter().position(|p| p.id == edit.id());
            let mut profile = index
                .map(|i| tool.profiles[i].clone())
                .unwrap_or_else(|| Profile {
                    id: edit.id().into(),
                    name: edit.name().into(),
                    model: String::new(),
                    arguments: Vec::new(),
                    settings: AgentSettings::default(),
                });
            profile.name = edit.name().into();
            if let Some(model) = edit.model() {
                profile.model = model.into();
            }
            if let Some(arguments) = edit.arguments() {
                profile.arguments = arguments.to_vec();
            }
            if let Some(options_) = edit.settings().and_then(|s| s.options()) {
                options(&mut profile.settings, options_);
            }
            environment(
                &mut profile.settings.env_refs,
                &mut profile.settings.custom_env,
                env,
                &mut pending,
            )?;
            // Validate resolved inputs before replacing them with opaque references.
            // The validation copy never reaches the configuration writer.
            let mut validation = profile.settings.clone();
            if let Value::Set(value) = &json {
                validation.settings_json = value.clone();
            }
            if let Value::Set(value) = &prompt {
                validation.append_system_prompt = value.clone();
            }
            if matches!(&json, Value::Remove) {
                validation.settings_json.clear();
            }
            if matches!(&prompt, Value::Remove) {
                validation.append_system_prompt.clear();
            }
            validation
                .validate(canopy_desktop::state::agent_settings::Agent::from_id(
                    &tool.id,
                ))
                .map_err(invalid)?;
            secret_field(
                &mut profile.settings.prompt_ref,
                &mut profile.settings.append_system_prompt,
                prompt,
                &mut pending,
            )?;
            secret_field(
                &mut profile.settings.settings_json_ref,
                &mut profile.settings.settings_json,
                json,
                &mut pending,
            )?;
            match key {
                Value::Unchanged => {}
                Value::Set(value) => {
                    keys.insert(profile.id.clone(), value);
                }
                Value::Remove => {
                    keys.insert(profile.id.clone(), String::new());
                }
            }
            match index {
                Some(index) => tool.profiles[index] = profile,
                None => tool.profiles.push(profile),
            }
        }
        Edit::RemoveProfile(id) => {
            if !catalog
                .tools
                .iter()
                .any(|t| t.profiles.iter().any(|p| p.id == id))
            {
                return Err(Status::not_found("Profile was removed."));
            }
            for tool in &mut catalog.tools {
                tool.profiles.retain(|p| p.id != id);
                if tool.default_profile.as_ref() == Some(&id) {
                    tool.default_profile = None;
                }
            }
        }
        Edit::Integration(edit, credential) => {
            if matches!(credential, Value::Remove) {
                return apply(Edit::RemoveIntegration(edit.id().to_owned()), cx);
            }
            let config = app.integrations.read(cx).config.clone();
            let old = config.accounts.iter().find(|a| a.id == edit.id());
            let scope = match edit.kind() {
                "github" => match edit.owner() {
                    Some("") => i::AccountScope::Default,
                    Some(owner) => i::AccountScope::owner(owner).map_err(invalid)?,
                    None => old.map(|a| a.scope.clone()).unwrap_or_default(),
                },
                "jira" => i::AccountScope::Jira {
                    site: edit.endpoint().into(),
                    email: edit
                        .email()
                        .map(str::to_owned)
                        .or_else(|| {
                            old.and_then(|a| match &a.scope {
                                i::AccountScope::Jira { email, .. } => Some(email.clone()),
                                _ => None,
                            })
                        })
                        .unwrap_or_default(),
                    cloud_id: edit
                        .cloud_id()
                        .map(str::to_owned)
                        .or_else(|| {
                            old.and_then(|a| match &a.scope {
                                i::AccountScope::Jira { cloud_id, .. } => cloud_id.clone(),
                                _ => None,
                            })
                        })
                        .filter(|s| !s.is_empty()),
                },
                "youtrack" => i::AccountScope::Youtrack {
                    service: edit.endpoint().into(),
                },
                _ => return Err(invalid("Unsupported integration.")),
            };
            let provider = match edit.kind() {
                "github" => i::Provider::Github,
                "jira" => i::Provider::Jira,
                _ => i::Provider::Youtrack,
            };
            let (reference, write) = match credential {
                Value::Unchanged => (
                    old.ok_or_else(|| invalid("A new integration requires a credential."))?
                        .credential
                        .clone(),
                    None,
                ),
                Value::Set(value)
                    if !value.is_empty() && value.len() <= 2560 && !value.contains('\0') =>
                {
                    let id = uuid::Uuid::new_v4().to_string();
                    (id.clone(), Some((id, value)))
                }
                _ => {
                    return Err(invalid(
                        "Disconnect the integration to remove its credential.",
                    ));
                }
            };
            let config = config
                .with_account(i::Account {
                    id: edit.id().into(),
                    provider,
                    login: edit.name().into(),
                    credential: reference,
                    scope,
                })
                .map_err(invalid)?;
            app.integrations
                .update(cx, |state, cx| {
                    state.save_config_committed(
                        config,
                        "Integration",
                        write,
                        Some(edit.id().to_owned()),
                        Some(tx),
                        cx,
                    )
                })
                .map_err(busy)?;
            return Ok(rx);
        }
        Edit::RemoveIntegration(id) => {
            let mut config = app.integrations.read(cx).config.clone();
            let old = config
                .accounts
                .iter()
                .find(|a| a.id == id)
                .ok_or_else(|| Status::not_found("Integration was removed."))?;
            config.retired_credentials.push(old.credential.clone());
            config.accounts.retain(|a| a.id != id);
            app.integrations
                .update(cx, |state, cx| {
                    state.save_config_committed(config, "Integration", None, None, Some(tx), cx)
                })
                .map_err(busy)?;
            return Ok(rx);
        }
    }
    app.tools
        .update(cx, |state, cx| {
            state.save_with_completion(catalog, keys, pending, Some(tx), cx)
        })
        .map_err(invalid)?;
    Ok(rx)
}
