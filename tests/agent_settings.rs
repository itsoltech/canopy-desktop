use canopy_desktop::{
    state::{
        agent_settings::{Agent, AgentSettings},
        tools::ToolCatalog,
        workspace::Workspace,
    },
    terminal::{agent_config, environment::ShellEnvironment},
};
use std::collections::HashMap;
#[test]
fn maps_claude_fields_to_separate_arguments_and_environment() {
    let settings = AgentSettings {
        permission_mode: "plan".into(),
        effort_level: "high".into(),
        append_system_prompt: "Keep spaces; $(literal)".into(),
        provider: "vertex".into(),
        base_url: "https://example.invalid".into(),
        settings_json: r#"{"language":"polish"}"#.into(),
        ..Default::default()
    };
    settings.validate(Some(Agent::Claude)).unwrap();
    assert_eq!(
        settings.arguments(Agent::Claude),
        [
            "--permission-mode",
            "plan",
            "--effort",
            "high",
            "--append-system-prompt",
            "Keep spaces; $(literal)",
            "--settings",
            r#"{"language":"polish"}"#
        ]
    );
    let env = settings.environment(Agent::Claude);
    assert_eq!(env["ANTHROPIC_BASE_URL"], "https://example.invalid");
    assert_eq!(env["CLAUDE_CODE_USE_VERTEX"], "1");
    assert_eq!(env["CLAUDE_CODE_USE_BEDROCK"], "0");
}

#[cfg(windows)]
#[test]
fn profile_environment_replaces_inherited_case_variant() {
    let mut catalog = ToolCatalog::default();
    let tool = catalog
        .tools
        .iter_mut()
        .find(|tool| tool.id == "claude")
        .unwrap();
    tool.executable = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    tool.profiles[0]
        .settings
        .custom_env
        .insert("PATH".into(), "profile-path".into());
    let mut workspace = Workspace::new();
    workspace.open("Claude", "claude");
    workspace.set_default_cwd(std::env::current_dir().unwrap());
    catalog.bind_default(&mut workspace);
    let base = ShellEnvironment {
        shell: std::env::current_exe().unwrap(),
        vars: HashMap::from([("Path".into(), "inherited-path".into())]),
    };
    let prepared = agent_config::prepare(&catalog, &workspace.activation_plan()[0], &base).unwrap();
    assert_eq!(prepared.environment.value("path"), Some("profile-path"));
    assert_eq!(
        prepared
            .environment
            .vars
            .keys()
            .filter(|key| key.eq_ignore_ascii_case("PATH"))
            .count(),
        1
    );
}
#[test]
fn codex_shortcuts_have_explicit_precedence() {
    let mut s = AgentSettings {
        approval_mode: "never".into(),
        sandbox: "read-only".into(),
        config_profile: "review".into(),
        ..Default::default()
    };
    assert_eq!(
        s.arguments(Agent::Codex),
        [
            "--ask-for-approval",
            "never",
            "--sandbox",
            "read-only",
            "--profile",
            "review"
        ]
    );
    s.full_auto = true;
    assert_eq!(
        s.arguments(Agent::Codex),
        [
            "--sandbox",
            "workspace-write",
            "--ask-for-approval",
            "on-request",
            "--profile",
            "review"
        ]
    );
    s.bypass_approvals = true;
    assert_eq!(
        s.arguments(Agent::Codex),
        [
            "--dangerously-bypass-approvals-and-sandbox",
            "--profile",
            "review"
        ]
    );
}
#[test]
fn invalid_json_env_and_options_are_rejected() {
    let mut s = AgentSettings {
        settings_json: "[]".into(),
        ..Default::default()
    };
    assert!(s.validate(Some(Agent::Claude)).is_err());
    s.settings_json = "{broken".into();
    assert!(s.validate(Some(Agent::Claude)).is_err());
    s.settings_json = "{}".into();
    s.permission_mode = "invalid".into();
    assert!(s.validate(Some(Agent::Claude)).is_err());
    s.permission_mode.clear();
    s.custom_env.insert("BAD=NAME".into(), "x".into());
    assert!(s.validate(None).is_err());
}
#[test]
fn old_profile_json_still_loads() {
    let mut value = serde_json::to_value(ToolCatalog::default()).unwrap();
    value.as_object_mut().unwrap().remove("retired_credentials");
    for tool in value["tools"].as_array_mut().unwrap() {
        for profile in tool["profiles"].as_array_mut().unwrap() {
            profile.as_object_mut().unwrap().remove("settings");
        }
    }
    let catalog: ToolCatalog = serde_json::from_value(value).unwrap();
    catalog.validate().unwrap();
    assert_eq!(
        catalog.get("claude").unwrap().profiles[0].settings,
        AgentSettings::default()
    );
    assert!(catalog.retired_credentials.is_empty());
}
#[test]
fn sqlite_preserves_agent_fields_and_only_a_key_reference() {
    futures_lite::future::block_on(async {
        use canopy_desktop::settings::{Access, SettingsClient};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let mut catalog = ToolCatalog::default();
        let mut tool = catalog.get("claude").unwrap().clone();
        tool.profiles[0].settings = AgentSettings {
            effort_level: "high".into(),
            provider: "bedrock".into(),
            api_key_ref: Some(uuid::Uuid::new_v4().to_string()),
            custom_env: std::collections::BTreeMap::from([("EXAMPLE".into(), "value".into())]),
            ..Default::default()
        };
        catalog.upsert(tool).unwrap();
        client.save_tools(catalog.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
        assert_eq!(client.load_tools().await.unwrap(), catalog);
        client.shutdown().await.unwrap();
    });
}
#[test]
#[ignore = "uses macOS Keychain; run explicitly for local credential verification"]
fn keychain_roundtrip_for_a_disposable_test_entry() {
    use canopy_desktop::terminal::credentials;
    let id = uuid::Uuid::new_v4().to_string();
    credentials::store(&id, "canopy-test-not-a-real-key").unwrap();
    let read = credentials::load(&id);
    let _ = credentials::remove(&id);
    assert!(read.is_ok_and(|key| key == "canopy-test-not-a-real-key"));
    assert!(credentials::load(&id).is_err());
}

#[test]
fn codex_hooks_use_process_overrides_without_changing_auth_home() {
    let mut catalog = ToolCatalog::default();
    let mut tool = catalog.get("codex").unwrap().clone();
    tool.executable = "/usr/bin/printf".into();
    tool.profiles[0].settings.settings_json=r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"printf 'hello world'"}]}]}}"#.into();
    catalog.upsert(tool).unwrap();
    let mut workspace = Workspace::new();
    workspace.open("Codex", "codex");
    workspace.set_default_cwd("/tmp".into());
    catalog.bind_default(&mut workspace);
    let env = ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: HashMap::from([
            ("HOME".into(), "/tmp/example-home".into()),
            ("CODEX_HOME".into(), "/tmp/example-codex".into()),
        ]),
    };
    let prepared = agent_config::prepare(&catalog, &workspace.activation_plan()[0], &env).unwrap();
    assert_eq!(
        prepared.environment.vars["CODEX_HOME"],
        "/tmp/example-codex"
    );
    assert!(prepared.files.is_none());
    assert_eq!(prepared.spec.arguments[0], "--config");
    assert!(prepared.spec.arguments[1].starts_with("hooks={"));
    assert!(prepared.spec.arguments[1].contains("printf 'hello world'"));
}
#[test]
fn profile_environment_reaches_the_real_pty() {
    use canopy_desktop::terminal::session::{Session, Size, Status};
    use std::time::{Duration, Instant};
    let mut catalog = ToolCatalog::default();
    let mut tool = catalog.get("opencode").unwrap().clone();
    tool.executable = "/bin/sh".into();
    tool.arguments = vec!["-c".into(), "printf '%s' \"$CANOPY_TEST_ENV\"".into()];
    tool.profiles[0]
        .settings
        .custom_env
        .insert("CANOPY_TEST_ENV".into(), "literal $(echo no)".into());
    catalog.upsert(tool).unwrap();
    let mut workspace = Workspace::new();
    workspace.open("Test", "opencode");
    workspace.set_default_cwd("/tmp".into());
    catalog.bind_default(&mut workspace);
    let env = ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: Default::default(),
    };
    let p = agent_config::prepare(&catalog, &workspace.activation_plan()[0], &env).unwrap();
    let session =
        Session::start_with_config(p.spec, &p.environment, Size::bounded(80, 24), p.files).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.status() == Status::Running {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        session.status(),
        Status::Exited {
            code: Some(0),
            signal: None
        }
    );
    assert!(
        session
            .frame()
            .unwrap()
            .cells
            .iter()
            .map(|c| c.text.as_str())
            .collect::<String>()
            .contains("literal $(echo no)")
    );
}
