use super::*;
use canopy_desktop::api::proto;
use canopy_desktop::integrations::AccountScope;
fn endpoint(value: &str) -> String {
    match url::Url::parse(value) {
        Ok(mut url) => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => String::new(),
    }
}
pub(super) fn snapshot(revision: u64, cx: &App) -> Result<proto::HostSettings, Status> {
    let app = cx.global::<AppState>();
    let settings = app.settings.read(cx);
    let tools = app.tools.read(cx);
    let integrations = app.integrations.read(cx);
    if !settings.ready || !tools.ready || !integrations.ready {
        return Err(busy("Host configuration is starting."));
    }
    let mut agents = Vec::new();
    let mut profiles = Vec::new();
    for tool in &tools.catalog.tools {
        let backend = if tool.acp {
            proto::BackendKind::Protocol
        } else {
            match tool.id.as_str() {
                "claude" => proto::BackendKind::NativeStream,
                "codex" => proto::BackendKind::NativeServer,
                "shell" => proto::BackendKind::Shell,
                _ => proto::BackendKind::Process,
            }
        };
        agents.push(proto::AgentDefinition {
            id: tool.id.clone(),
            name: tool.name.clone(),
            backend_kind: backend as i32,
            command: tool.executable.clone(),
            arguments: tool.arguments.clone(),
            environment: tool
                .env_refs
                .keys()
                .map(|name| proto::EnvironmentField {
                    name: name.clone(),
                    configured: true,
                })
                .collect(),
            enabled: Some(tool.enabled),
            default_profile_id: Some(tool.default_profile.clone().unwrap_or_default()),
        });
        for profile in &tool.profiles {
            let s = &profile.settings;
            let names: std::collections::BTreeSet<_> =
                s.custom_env.keys().chain(s.env_refs.keys()).collect();
            profiles.push(proto::Profile {
                id: profile.id.clone(),
                agent_id: tool.id.clone(),
                name: profile.name.clone(),
                environment: names
                    .into_iter()
                    .map(|name| proto::EnvironmentField {
                        name: name.clone(),
                        configured: true,
                    })
                    .collect(),
                credential_configured: s.api_key_ref.is_some(),
                model: Some(profile.model.clone()),
                arguments: Some(proto::StringValues {
                    values: profile.arguments.clone(),
                }),
                settings: Some(proto::ProfileAgentSettings {
                    permission_mode: Some(s.permission_mode.clone()),
                    effort_level: Some(s.effort_level.clone()),
                    base_url: Some(endpoint(&s.base_url)),
                    provider: Some(s.provider.clone()),
                    approval_mode: Some(s.approval_mode.clone()),
                    sandbox: Some(s.sandbox.clone()),
                    full_auto: Some(s.full_auto),
                    bypass_approvals: Some(s.bypass_approvals),
                    config_profile: Some(s.config_profile.clone()),
                    system_prompt_configured: s.prompt_ref.is_some()
                        || !s.append_system_prompt.is_empty(),
                    settings_override_configured: s.settings_json_ref.is_some()
                        || !s.settings_json.is_empty(),
                }),
            });
        }
    }
    let integrations = integrations
        .config
        .accounts
        .iter()
        .map(|account| {
            let (kind, url, owner, email, cloud_id) = match &account.scope {
                AccountScope::Default => (
                    "github",
                    "https://api.github.com".into(),
                    Some(String::new()),
                    None,
                    None,
                ),
                AccountScope::Owner(owner) => (
                    "github",
                    "https://api.github.com".into(),
                    Some(owner.clone()),
                    None,
                    None,
                ),
                AccountScope::Jira {
                    site,
                    email,
                    cloud_id,
                } => (
                    "jira",
                    endpoint(site),
                    None,
                    Some(email.clone()),
                    cloud_id.clone(),
                ),
                AccountScope::Youtrack { service } => {
                    ("youtrack", endpoint(service), None, None, None)
                }
            };
            proto::Integration {
                id: account.id.clone(),
                kind: kind.into(),
                name: account.login.clone(),
                endpoint: url,
                credential_configured: !account.credential.is_empty(),
                owner,
                email,
                cloud_id,
            }
        })
        .collect();
    let p = &settings.values;
    Ok(proto::HostSettings {
        environment: Some(proto::EnvironmentRef {
            host_id: settings.host_id().unwrap_or_default().into(),
        }),
        revision,
        agents,
        profiles,
        integrations,
        preferences: Some(proto::Preferences {
            reopen_last_workspace: p.reopen_last_workspace,
            notch_enabled: p.notch_enabled,
            new_tab_tool: p.new_tab_tool.as_str().into(),
            new_worktree_tool: p.new_worktree_tool.as_str().into(),
            chat_profile: p.chat.profile.clone(),
            chat_model: p.chat.model.clone(),
            chat_effort: p.chat.effort.map(|v| v.key().to_owned()),
            chat_mode: p.chat.mode.map(|v| v.key().to_owned()),
        }),
    })
}
