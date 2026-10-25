use super::proto;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An immutable entry in the remote staging namespace, never an active config key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretVersion {
    id: String,
    #[serde(default)]
    empty: bool,
    #[serde(default)]
    rest: Vec<String>,
}
impl SecretVersion {
    pub(super) fn new(id: String) -> Self {
        Self {
            id,
            empty: false,
            rest: Vec::new(),
        }
    }
    pub(super) fn for_value(id: String, value: &str) -> Self {
        use sha2::{Digest, Sha256};
        let rest = (1..super::secrets::chunks(value).len())
            .map(|index| {
                let digest = Sha256::digest(format!("Canopy staged chunk v1:{id}:{index}"));
                let mut bytes = [0; 16];
                bytes.copy_from_slice(&digest[..16]);
                uuid::Uuid::from_bytes(bytes).to_string()
            })
            .collect();
        Self {
            id,
            empty: value.is_empty(),
            rest,
        }
    }
    pub(super) fn parts(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str()).chain(self.rest.iter().map(String::as_str))
    }
    pub(super) fn is_empty(&self) -> bool {
        self.empty
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretChange {
    Unchanged,
    Set(SecretVersion),
    Remove,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PreferencesPatch {
    reopen_last_workspace: Option<bool>,
    notch_enabled: Option<bool>,
    new_tab_tool: Option<String>,
    new_worktree_tool: Option<String>,
    chat_profile: Option<String>,
    chat_model: Option<String>,
    chat_effort: Option<String>,
    chat_mode: Option<String>,
}
impl PreferencesPatch {
    pub fn chat_profile(&self) -> Option<&str> {
        self.chat_profile.as_deref()
    }
    pub fn chat_model(&self) -> Option<&str> {
        self.chat_model.as_deref()
    }
    pub fn chat_effort(&self) -> Option<&str> {
        self.chat_effort.as_deref()
    }
    pub fn chat_mode(&self) -> Option<&str> {
        self.chat_mode.as_deref()
    }
    pub fn reopen_last_workspace(&self) -> Option<bool> {
        self.reopen_last_workspace
    }
    pub fn notch_enabled(&self) -> Option<bool> {
        self.notch_enabled
    }
    pub fn new_tab_tool(&self) -> Option<&str> {
        self.new_tab_tool.as_deref()
    }
    pub fn new_worktree_tool(&self) -> Option<&str> {
        self.new_worktree_tool.as_deref()
    }
    pub(super) fn from_request(r: proto::UpdatePreferencesRequest) -> Self {
        Self {
            reopen_last_workspace: r.reopen_last_workspace,
            notch_enabled: r.notch_enabled,
            new_tab_tool: r.new_tab_tool,
            new_worktree_tool: r.new_worktree_tool,
            chat_profile: r.chat_profile,
            chat_model: r.chat_model,
            chat_effort: r.chat_effort,
            chat_mode: r.chat_mode,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentEdit {
    id: String,
    name: String,
    backend: crate::chat::agent::BackendKind,
    command: String,
    arguments: Vec<String>,
    enabled: Option<bool>,
    environment: BTreeMap<String, SecretChange>,
    default_profile_id: Option<String>,
}
impl AgentEdit {
    pub fn enabled(&self) -> Option<bool> {
        self.enabled
    }
    pub fn default_profile_id(&self) -> Option<&str> {
        self.default_profile_id.as_deref()
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn backend(&self) -> crate::chat::agent::BackendKind {
        self.backend
    }
    pub fn command(&self) -> &str {
        &self.command
    }
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    pub fn environment(&self) -> &BTreeMap<String, SecretChange> {
        &self.environment
    }
    pub(super) fn new(
        a: proto::AgentDefinition,
        backend: crate::chat::agent::BackendKind,
        environment: BTreeMap<String, SecretChange>,
    ) -> Self {
        Self {
            id: a.id,
            name: a.name,
            backend,
            command: a.command,
            arguments: a.arguments,
            enabled: a.enabled,
            default_profile_id: a.default_profile_id,
            environment,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileEdit {
    id: String,
    agent_id: String,
    name: String,
    environment: BTreeMap<String, SecretChange>,
    credential: SecretChange,
    model: Option<String>,
    arguments: Option<Vec<String>>,
    settings: Option<ProfileSettingsEdit>,
}
impl ProfileEdit {
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }
    pub fn arguments(&self) -> Option<&[String]> {
        self.arguments.as_deref()
    }
    pub fn settings(&self) -> Option<&ProfileSettingsEdit> {
        self.settings.as_ref()
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn environment(&self) -> &BTreeMap<String, SecretChange> {
        &self.environment
    }
    pub fn credential(&self) -> &SecretChange {
        &self.credential
    }
    pub(super) fn new(
        p: proto::Profile,
        environment: BTreeMap<String, SecretChange>,
        credential: SecretChange,
        settings: Option<ProfileSettingsEdit>,
    ) -> Self {
        Self {
            id: p.id,
            agent_id: p.agent_id,
            name: p.name,
            model: p.model,
            arguments: p.arguments.map(|v| v.values),
            settings,
            environment,
            credential,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntegrationEdit {
    id: String,
    kind: String,
    name: String,
    endpoint: String,
    credential: SecretChange,
    owner: Option<String>,
    email: Option<String>,
    cloud_id: Option<String>,
}
impl IntegrationEdit {
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }
    pub fn cloud_id(&self) -> Option<&str> {
        self.cloud_id.as_deref()
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn credential(&self) -> &SecretChange {
        &self.credential
    }
    pub(super) fn new(i: proto::Integration, credential: SecretChange) -> Self {
        Self {
            id: i.id,
            kind: i.kind,
            name: i.name,
            endpoint: i.endpoint,
            owner: i.owner,
            email: i.email,
            cloud_id: i.cloud_id,
            credential,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum Edit {
    Preferences(PreferencesPatch),
    Agent(AgentEdit),
    RemoveAgent(String),
    Profile(Box<ProfileEdit>),
    RemoveProfile(String),
    Integration(IntegrationEdit),
    RemoveIntegration(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileOptions {
    permission_mode: Option<String>,
    effort_level: Option<String>,
    base_url: Option<String>,
    provider: Option<String>,
    approval_mode: Option<String>,
    sandbox: Option<String>,
    full_auto: Option<bool>,
    bypass_approvals: Option<bool>,
    config_profile: Option<String>,
}
impl ProfileOptions {
    pub fn permission_mode(&self) -> Option<&str> {
        self.permission_mode.as_deref()
    }
    pub fn effort_level(&self) -> Option<&str> {
        self.effort_level.as_deref()
    }
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }
    pub fn approval_mode(&self) -> Option<&str> {
        self.approval_mode.as_deref()
    }
    pub fn sandbox(&self) -> Option<&str> {
        self.sandbox.as_deref()
    }
    pub fn full_auto(&self) -> Option<bool> {
        self.full_auto
    }
    pub fn bypass_approvals(&self) -> Option<bool> {
        self.bypass_approvals
    }
    pub fn config_profile(&self) -> Option<&str> {
        self.config_profile.as_deref()
    }
    pub(super) fn new(o: proto::ProfileAgentSettings) -> Self {
        Self {
            permission_mode: o.permission_mode,
            effort_level: o.effort_level,
            base_url: o.base_url,
            provider: o.provider,
            approval_mode: o.approval_mode,
            sandbox: o.sandbox,
            full_auto: o.full_auto,
            bypass_approvals: o.bypass_approvals,
            config_profile: o.config_profile,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileSettingsEdit {
    options: Option<ProfileOptions>,
    append_system_prompt: SecretChange,
    settings_json: SecretChange,
}
impl ProfileSettingsEdit {
    pub fn options(&self) -> Option<&ProfileOptions> {
        self.options.as_ref()
    }
    pub fn append_system_prompt(&self) -> &SecretChange {
        &self.append_system_prompt
    }
    pub fn settings_json(&self) -> &SecretChange {
        &self.settings_json
    }
    pub(super) fn new(
        options: Option<proto::ProfileAgentSettings>,
        append_system_prompt: SecretChange,
        settings_json: SecretChange,
    ) -> Self {
        Self {
            options: options.map(ProfileOptions::new),
            append_system_prompt,
            settings_json,
        }
    }
}
