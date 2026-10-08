use canopy_desktop::{
    settings::{Access, Error, SettingsClient},
    state::{
        tools::{Profile, ToolCatalog, new_id},
        workspace::{Axis, Workspace},
    },
    terminal::environment::ShellEnvironment,
};
use futures_lite::future::block_on;
fn configured() -> ToolCatalog {
    let mut catalog = ToolCatalog::default();
    let mut tool = catalog.get("codex").unwrap().clone();
    tool.executable = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    tool.arguments = vec!["%s\\n".into()];
    tool.profiles.push(Profile {
        settings: Default::default(),
        id: new_id(),
        name: "Review".into(),
        model: "example-model".into(),
        arguments: vec!["--literal=$(touch unwanted)".into(), "two words".into()],
    });
    tool.default_profile = Some(tool.profiles.last().unwrap().id.clone());
    catalog.upsert(tool).unwrap();
    catalog
}
#[test]
fn pending_task_draft_never_becomes_an_auto_submitted_cli_argument() {
    for tool in ["claude", "codex"] {
        let mut catalog = ToolCatalog::default();
        let mut definition = catalog.get(tool).unwrap().clone();
        definition.executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        catalog.upsert(definition).unwrap();
        let mut workspace = Workspace::new();
        workspace.open("Task", tool);
        workspace.set_default_cwd("/tmp".into());
        let mut pane = workspace.activation_plan()[0].clone();
        let env = ShellEnvironment {
            shell: "/bin/sh".into(),
            vars: Default::default(),
        };
        let before = catalog.launch(&pane, &env).unwrap().arguments;
        pane.metadata.task_prompt = Some("Task\nComment\n@'/tmp/file.png'".into());
        assert_eq!(catalog.launch(&pane, &env).unwrap().arguments, before);
        pane.metadata.resume_id = Some(uuid::Uuid::new_v4().to_string());
        assert!(
            !catalog
                .launch(&pane, &env)
                .unwrap()
                .arguments
                .iter()
                .any(|v| v.contains("Comment"))
        );
    }
}
#[test]
fn profiles_bind_to_panes_and_build_literal_argv() {
    let mut catalog = configured();
    let mut workspace = Workspace::new();
    let tab = workspace.open("Test", "codex");
    workspace.set_default_cwd("/tmp".into());
    catalog.bind_default(&mut workspace);
    let pane = workspace.activation_plan()[0].clone();
    let env = ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: Default::default(),
    };
    let spec = catalog.launch(&pane, &env).unwrap();
    assert_eq!(
        spec.arguments,
        vec![
            "%s\\n",
            "--model",
            "example-model",
            "--literal=$(touch unwanted)",
            "two words"
        ]
    );
    let selected = pane.metadata.profile_id.clone();
    let mut tool = catalog.get("codex").unwrap().clone();
    tool.default_profile = Some("codex-default".into());
    catalog.upsert(tool).unwrap();
    assert_eq!(
        catalog.launch(&pane, &env).unwrap().arguments,
        spec.arguments
    );
    let split = workspace
        .split(tab, pane.id, Axis::Horizontal, "codex")
        .unwrap();
    catalog.bind_default(&mut workspace);
    assert_eq!(
        workspace
            .active()
            .unwrap()
            .root
            .find(split)
            .unwrap()
            .metadata
            .profile_id
            .as_deref(),
        Some("codex-default")
    );
    let restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&workspace).unwrap()).unwrap();
    assert_eq!(
        restored
            .active()
            .unwrap()
            .root
            .find(pane.id)
            .unwrap()
            .metadata
            .profile_id,
        selected
    );
    let mut tool = catalog.get("codex").unwrap().clone();
    tool.profiles.retain(|p| Some(&p.id) != selected.as_ref());
    catalog.upsert(tool).unwrap();
    assert!(
        catalog
            .launch(&pane, &env)
            .err()
            .unwrap()
            .contains("profile")
    );
}
#[test]
fn invalid_edits_are_atomic_and_builtins_protected() {
    let mut catalog = configured();
    let before = catalog.clone();
    let mut tool = catalog.get("codex").unwrap().clone();
    tool.profiles[1].name = "Default".into();
    assert!(catalog.upsert(tool).is_err());
    assert_eq!(catalog, before);
    let mut tool = catalog.get("codex").unwrap().clone();
    tool.default_profile = Some("missing".into());
    assert!(catalog.upsert(tool).is_err());
    assert_eq!(catalog, before);
    assert!(catalog.remove("codex").is_err());
    let mut custom = catalog.get("gemini").unwrap().clone();
    custom.id = new_id();
    custom.profiles.clear();
    custom.default_profile = None;
    custom.name = "My command".into();
    let id = custom.id.clone();
    catalog.upsert(custom).unwrap();
    catalog.remove(&id).unwrap();
    assert_eq!(catalog, before);
}
#[test]
fn disabled_and_deleted_tools_never_fall_back_to_shell() {
    let mut catalog = ToolCatalog::default();
    let mut workspace = Workspace::new();
    workspace.open("Test", "gemini");
    workspace.set_default_cwd("/tmp".into());
    let env = ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: Default::default(),
    };
    let pane = workspace.activation_plan()[0].clone();
    let mut tool = catalog.get("gemini").unwrap().clone();
    tool.enabled = false;
    catalog.upsert(tool).unwrap();
    assert!(
        catalog
            .launch(&pane, &env)
            .err()
            .unwrap()
            .contains("disabled")
    );
    catalog.remove("gemini").unwrap();
    assert!(catalog.launch(&pane, &env).is_err());
}
#[test]
fn tools_roundtrip_and_corrupt_version_is_not_overwritten() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools.db");
        let client = SettingsClient::create(&path).await.unwrap();
        assert_eq!(client.load_tools().await.unwrap(), ToolCatalog::default());
        let mut catalog = configured();
        catalog
            .retired_credentials
            .push(uuid::Uuid::new_v4().to_string());
        client.save_tools(catalog.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
        assert_eq!(client.load_tools().await.unwrap(), catalog);
        assert!(matches!(
            client.save_tools(catalog.clone()).await,
            Err(Error::ReadOnly)
        ));
        client.shutdown().await.unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("UPDATE _canopy_rust_tools SET version=99", [])
            .unwrap();
        let client = SettingsClient::open(&path, Access::ReadWrite)
            .await
            .unwrap();
        assert!(matches!(
            client.load_tools().await,
            Err(Error::InvalidToolCatalog)
        ));
        assert!(matches!(
            client.save_tools(ToolCatalog::default()).await,
            Err(Error::InvalidToolCatalog)
        ));
        assert_eq!(
            db.query_row("SELECT version FROM _canopy_rust_tools", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            99
        );
        client.shutdown().await.unwrap();
    });
}

#[test]
fn cleanup_state_write_failure_keeps_loaded_catalog_available_for_retry() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let mut catalog = ToolCatalog::default();
        let retired = uuid::Uuid::new_v4().to_string();
        catalog.retired_credentials.push(retired.clone());
        client.save_tools(catalog.clone()).await.unwrap();
        client.shutdown().await.unwrap();

        let client = SettingsClient::open(&path, Access::ReadOnly).await.unwrap();
        let loaded = client.load_tools().await.unwrap();
        let (available, warning) = client.persist_tool_cleanup(loaded, vec![]).await;
        assert_eq!(available, catalog);
        assert_eq!(available.retired_credentials, [retired]);
        assert!(warning.unwrap().contains("retry state could not be saved"));
        assert_eq!(client.load_tools().await.unwrap(), catalog);
        client.shutdown().await.unwrap();
    });
}

#[cfg(windows)]
#[test]
fn legacy_default_shell_login_argument_is_migrated_without_touching_custom_argv() {
    block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools.db");
        let client = SettingsClient::create(&path).await.unwrap();
        let mut legacy = ToolCatalog::default();
        legacy
            .tools
            .iter_mut()
            .find(|tool| tool.id == "shell")
            .unwrap()
            .arguments = vec!["-l".into()];
        client.save_tools(legacy).await.unwrap();
        let migrated = client.load_tools().await.unwrap();
        assert!(migrated.get("shell").unwrap().arguments.is_empty());
        client.save_tools(migrated).await.unwrap();

        let mut custom = ToolCatalog::default();
        custom
            .tools
            .iter_mut()
            .find(|tool| tool.id == "shell")
            .unwrap()
            .arguments = vec!["-l".into(), "custom".into()];
        client.save_tools(custom.clone()).await.unwrap();
        assert_eq!(client.load_tools().await.unwrap(), custom);
        client.shutdown().await.unwrap();
    });
}

#[cfg(unix)]
#[test]
fn configured_profile_runs_directly_in_a_real_pty() {
    use canopy_desktop::terminal::session::{Session, Size, Status};
    use std::time::{Duration, Instant};
    let mut catalog = ToolCatalog::default();
    let mut tool = catalog.get("gemini").unwrap().clone();
    tool.executable = "/usr/bin/printf".into();
    tool.arguments = vec!["%s".into()];
    tool.profiles = vec![Profile {
        settings: Default::default(),
        id: new_id(),
        name: "Literal".into(),
        model: String::new(),
        arguments: vec!["profile works; $(echo literal)".into()],
    }];
    tool.default_profile = Some(tool.profiles[0].id.clone());
    catalog.upsert(tool).unwrap();
    let mut workspace = Workspace::new();
    workspace.open("Test", "gemini");
    workspace.set_default_cwd("/tmp".into());
    catalog.bind_default(&mut workspace);
    let env = ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: Default::default(),
    };
    let session = Session::start(
        catalog
            .launch(&workspace.activation_plan()[0], &env)
            .unwrap(),
        &env,
        Size::bounded(80, 24),
    )
    .unwrap();
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
    let output = session
        .frame()
        .unwrap()
        .cells
        .iter()
        .map(|c| c.text.as_str())
        .collect::<String>();
    assert!(output.contains("profile works; $(echo literal)"));
}
