//! Local configuration diffs use the same semantic edits as RPCs. Plaintext
//! credentials and untouched values from older catalogs never enter the inbox.
use super::*;
use crate::state::tools::{ToolCatalog, ToolDefinition, ToolKind};

fn secret(ids: Option<&[String]>, old: Option<&[String]>) -> SecretChange {
    if ids == old {
        return SecretChange::Unchanged;
    }
    match ids {
        None => SecretChange::Remove,
        Some([]) => SecretChange::Set(SecretVersion {
            id: uuid::Uuid::new_v4().to_string(),
            empty: true,
            rest: Vec::new(),
        }),
        Some([id, rest @ ..]) => SecretChange::Set(SecretVersion {
            id: id.clone(),
            empty: false,
            rest: rest.to_vec(),
        }),
    }
}
fn legacy_secret(
    ids: Option<&[String]>,
    old: Option<&[String]>,
    removed_plaintext: bool,
) -> SecretChange {
    if ids.is_none() && removed_plaintext {
        SecretChange::Remove
    } else {
        secret(ids, old)
    }
}
fn environment(
    current: &BTreeMap<String, Vec<String>>,
    old: Option<&BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, SecretChange> {
    let mut result = BTreeMap::new();
    for (key, ids) in current {
        result.insert(
            key.clone(),
            secret(Some(ids), old.and_then(|v| v.get(key)).map(Vec::as_slice)),
        );
    }
    if let Some(old) = old {
        for key in old.keys().filter(|key| !current.contains_key(*key)) {
            result.insert(key.clone(), SecretChange::Remove);
        }
    }
    result
}
fn backend(tool: &ToolDefinition) -> ToolBackend {
    match tool.kind {
        ToolKind::Claude => ToolBackend::NativeStream,
        ToolKind::Codex => ToolBackend::NativeServer,
        ToolKind::Shell => ToolBackend::Shell,
        ToolKind::Custom if tool.acp => ToolBackend::Protocol,
        ToolKind::Custom => ToolBackend::Process,
    }
}
impl Edit {
    pub fn tool_changes(previous: &ToolCatalog, current: &ToolCatalog) -> Vec<Self> {
        let mut edits = Vec::new();
        for tool in &current.tools {
            let old = previous.get(&tool.id);
            if old.is_none_or(|old| {
                (
                    &old.name,
                    old.kind,
                    &old.executable,
                    &old.arguments,
                    old.enabled,
                    &old.default_profile,
                    old.acp,
                    &old.env_refs,
                ) != (
                    &tool.name,
                    tool.kind,
                    &tool.executable,
                    &tool.arguments,
                    tool.enabled,
                    &tool.default_profile,
                    tool.acp,
                    &tool.env_refs,
                )
            }) {
                edits.push(Self::Agent(AgentEdit {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    backend: backend(tool),
                    command: tool.executable.clone(),
                    arguments: tool.arguments.clone(),
                    enabled: Some(tool.enabled),
                    default_profile_id: Some(tool.default_profile.clone().unwrap_or_default()),
                    environment: environment(&tool.env_refs, old.map(|o| &o.env_refs)),
                }));
            }
            for profile in &tool.profiles {
                let old = old.and_then(|o| o.profiles.iter().find(|p| p.id == profile.id));
                if old == Some(profile) {
                    continue;
                }
                let s = &profile.settings;
                let old_settings = old.map(|p| &p.settings);
                let mut env = environment(&s.env_refs, old_settings.map(|s| &s.env_refs));
                if let Some(old) = old_settings {
                    for key in old
                        .custom_env
                        .keys()
                        .filter(|k| !s.custom_env.contains_key(*k) && !s.env_refs.contains_key(*k))
                    {
                        env.insert(key.clone(), SecretChange::Remove);
                    }
                }
                edits.push(Self::Profile(Box::new(ProfileEdit {
                    id: profile.id.clone(),
                    agent_id: tool.id.clone(),
                    name: profile.name.clone(),
                    model: Some(profile.model.clone()),
                    arguments: Some(profile.arguments.clone()),
                    environment: env,
                    credential: secret(
                        s.api_key_ref.as_ref().map(std::slice::from_ref),
                        old_settings
                            .and_then(|s| s.api_key_ref.as_ref())
                            .map(std::slice::from_ref),
                    ),
                    settings: Some(ProfileSettingsEdit {
                        options: Some(ProfileOptions {
                            permission_mode: Some(s.permission_mode.clone()),
                            effort_level: Some(s.effort_level.clone()),
                            base_url: Some(s.base_url.clone()),
                            provider: Some(s.provider.clone()),
                            approval_mode: Some(s.approval_mode.clone()),
                            sandbox: Some(s.sandbox.clone()),
                            full_auto: Some(s.full_auto),
                            bypass_approvals: Some(s.bypass_approvals),
                            config_profile: Some(s.config_profile.clone()),
                        }),
                        append_system_prompt: legacy_secret(
                            s.prompt_ref.as_deref(),
                            old_settings.and_then(|s| s.prompt_ref.as_deref()),
                            s.append_system_prompt.is_empty()
                                && old_settings.is_some_and(|s| !s.append_system_prompt.is_empty()),
                        ),
                        settings_json: legacy_secret(
                            s.settings_json_ref.as_deref(),
                            old_settings.and_then(|s| s.settings_json_ref.as_deref()),
                            s.settings_json.is_empty()
                                && old_settings.is_some_and(|s| !s.settings_json.is_empty()),
                        ),
                    }),
                })));
            }
            if let Some(old) = old {
                for profile in old
                    .profiles
                    .iter()
                    .filter(|p| !tool.profiles.iter().any(|n| n.id == p.id))
                {
                    edits.push(Self::RemoveProfile(profile.id.clone()));
                }
            }
        }
        for old in previous
            .tools
            .iter()
            .filter(|p| current.get(&p.id).is_none())
        {
            edits.push(Self::RemoveAgent(old.id.clone()));
        }
        edits
    }

    pub fn integration_changes(
        previous: &crate::integrations::Config,
        current: &crate::integrations::Config,
    ) -> Vec<Self> {
        use crate::integrations::{AccountScope, Provider};
        let mut edits = Vec::new();
        for a in &current.accounts {
            let old = previous.accounts.iter().find(|p| p.id == a.id);
            if old == Some(a) {
                continue;
            }
            let (endpoint, owner, email, cloud_id) = match &a.scope {
                AccountScope::Default => (String::new(), None, None, None),
                AccountScope::Owner(v) => (String::new(), Some(v.clone()), None, None),
                AccountScope::Jira {
                    site,
                    email,
                    cloud_id,
                } => (site.clone(), None, Some(email.clone()), cloud_id.clone()),
                AccountScope::Youtrack { service } => (service.clone(), None, None, None),
            };
            edits.push(Self::Integration(IntegrationEdit {
                id: a.id.clone(),
                kind: match a.provider {
                    Provider::Github => "github",
                    Provider::Jira => "jira",
                    Provider::Youtrack => "youtrack",
                }
                .into(),
                name: a.login.clone(),
                endpoint,
                owner,
                email,
                cloud_id,
                credential: secret(
                    Some(std::slice::from_ref(&a.credential)),
                    old.map(|a| std::slice::from_ref(&a.credential)),
                ),
            }));
        }
        for a in previous
            .accounts
            .iter()
            .filter(|a| !current.accounts.iter().any(|c| c.id == a.id))
        {
            edits.push(Self::RemoveIntegration(a.id.clone()));
        }
        edits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_legacy_overrides_and_setting_empty_environment_keep_distinct_intents() {
        let mut previous = ToolCatalog::default();
        let old = previous
            .tools
            .iter_mut()
            .find(|tool| !tool.profiles.is_empty())
            .unwrap();
        old.profiles[0].settings.append_system_prompt = "private previous prompt".into();
        old.profiles[0].settings.settings_json = "private previous override".into();
        old.profiles[0]
            .settings
            .custom_env
            .insert("REMOVED".into(), "private previous value".into());
        let tool_id = old.id.clone();
        let mut current = previous.clone();
        let profile = &mut current
            .tools
            .iter_mut()
            .find(|tool| tool.id == tool_id)
            .unwrap()
            .profiles[0];
        profile.settings.append_system_prompt.clear();
        profile.settings.settings_json.clear();
        profile.settings.custom_env.clear();
        profile.settings.env_refs.insert("EMPTY".into(), Vec::new());
        let changes = Edit::tool_changes(&previous, &current);
        let encoded = serde_json::to_string(&changes).unwrap();
        assert!(
            !encoded.contains("private previous"),
            "removed plaintext cannot enter the inbox"
        );
        let Edit::Profile(profile) = &changes[0] else {
            panic!("profile edit");
        };
        let overrides = profile.settings.as_ref().unwrap();
        assert_eq!(overrides.append_system_prompt, SecretChange::Remove);
        assert_eq!(overrides.settings_json, SecretChange::Remove);
        assert_eq!(profile.environment["REMOVED"], SecretChange::Remove);
        assert!(
            matches!(&profile.environment["EMPTY"], SecretChange::Set(value) if value.is_empty())
        );
    }
}
