//! Builds the headless Claude launch from the same profile path as the terminal variant.
use super::{config::ChatSettings, process::Spec};
use crate::{
    agents::{launch as hooks, relay::Registration},
    state::{
        agent_settings::Agent,
        tools::{Profile, ToolCatalog},
        workspace::Pane,
    },
    terminal::{agent_config, environment::ShellEnvironment},
};

/// Protocol flags Canopy owns; the SDK passes the same set.
pub const HEADLESS_ARGUMENTS: [&str; 8] = [
    "--output-format",
    "stream-json",
    "--verbose",
    "--input-format",
    "stream-json",
    "--include-partial-messages",
    "--permission-prompt-tool",
    "stdio",
];

/// User arguments that would change the protocol or pick the session behind Canopy's back.
const RESERVED: [&str; 14] = [
    "-p",
    "--print",
    "--output-format",
    "--input-format",
    "--permission-prompt-tool",
    "--replay-user-messages",
    "--sdk-url",
    "-c",
    "--continue",
    "-r",
    "--resume",
    "--session-id",
    "--fork-session",
    "--resume-session-at",
];

/// Environment names whose values are masked in diagnostics.
const SECRET_NAME_PARTS: [&str; 5] = ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL"];

/// Canopy hook observers, injected exactly like the terminal variant.
pub struct HookConfig<'a> {
    pub helper: &'a std::path::Path,
    pub registration: &'a Registration,
}

pub struct Launch {
    pub spec: Spec,
    /// Private profile files that must outlive the process.
    pub files: Option<tempfile::TempDir>,
}

/// Lets the chat switch to Yolo later; on its own it bypasses nothing.
pub const ALLOW_YOLO: &str = "--allow-dangerously-skip-permissions";

pub fn prepare_claude(
    catalog: &ToolCatalog,
    pane: &Pane,
    env: &ShellEnvironment,
    settings: &ChatSettings,
    hooks: Option<HookConfig<'_>>,
) -> Result<Launch, String> {
    if pane.tool != "claude" {
        return Err("Chat currently supports Claude Code only.".into());
    }
    if !settings.valid() {
        return Err("The selected model is not valid.".into());
    }
    let mut catalog = catalog.clone();
    let mut pane = pane.clone();
    apply_settings(&mut catalog, &mut pane, settings)?;
    if let Some(config) = &hooks {
        hooks::augment(&mut catalog, &mut pane, config.helper)?;
    }
    let mut prepared = agent_config::prepare(&catalog, &pane, env)?;
    // ToolCatalog::launch appends Canopy's own `--resume <uuid>` last; only user
    // arguments before it are checked.
    let user_arguments = match &pane.metadata.resume_id {
        Some(id)
            if prepared
                .spec
                .arguments
                .ends_with(&["--resume".into(), id.clone()]) =>
        {
            &prepared.spec.arguments[..prepared.spec.arguments.len() - 2]
        }
        _ => &prepared.spec.arguments[..],
    };
    if let Some(argument) = reserved_argument(user_arguments) {
        return Err(format!(
            "The argument {argument} conflicts with Canopy chat. Remove it from the Claude Code tool or profile arguments to use chat."
        ));
    }
    prepared
        .spec
        .arguments
        .extend(HEADLESS_ARGUMENTS.map(str::to_owned));
    prepared.spec.arguments.push(ALLOW_YOLO.into());
    if let Some(config) = &hooks {
        hooks::environment(&mut prepared.environment, config.registration);
    }
    let mut environment = prepared.environment.vars;
    // Same as the SDK: a host NODE_OPTIONS must not alter the CLI runtime.
    environment.retain(|key, _| !key.eq_ignore_ascii_case("NODE_OPTIONS"));
    environment.insert("CLAUDE_CODE_SDK_READS_SESSION_STATE".into(), "1".into());
    let api_env = Agent::Claude.api_env();
    let secrets = environment
        .iter()
        .filter(|(key, _)| {
            let key = key.to_ascii_uppercase();
            key == api_env || SECRET_NAME_PARTS.iter().any(|part| key.contains(part))
        })
        .map(|(_, value)| value.clone())
        .collect();
    Ok(Launch {
        spec: Spec {
            program: prepared.spec.program,
            arguments: prepared.spec.arguments,
            cwd: prepared.spec.cwd,
            environment,
            secrets,
        },
        files: prepared.files,
    })
}

/// The chat's choices win over the account profile; `None` keeps the profile's value.
/// Works on a private copy of the catalog, so Preferences never change.
fn apply_settings(
    catalog: &mut ToolCatalog,
    pane: &mut Pane,
    settings: &ChatSettings,
) -> Result<(), String> {
    let tool = catalog
        .tools
        .iter_mut()
        .find(|tool| tool.id == pane.tool)
        .ok_or("Claude Code is not configured.")?;
    let profile = match pane
        .metadata
        .profile_id
        .clone()
        .or(tool.default_profile.clone())
    {
        Some(id) => tool
            .profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or("This account profile no longer exists.")?,
        None => {
            tool.profiles.push(Profile {
                id: "canopy-chat".into(),
                name: "Chat".into(),
                model: String::new(),
                arguments: vec![],
                settings: Default::default(),
            });
            pane.metadata.profile_id = Some("canopy-chat".into());
            tool.profiles.last_mut().expect("profile just added")
        }
    };
    if let Some(model) = &settings.model {
        profile.model = model.clone();
    }
    if let Some(effort) = settings.effort {
        profile.settings.effort_level = effort.key().into();
    }
    profile.settings.permission_mode = settings.mode.claude_profile().into();
    Ok(())
}

fn reserved_argument(arguments: &[String]) -> Option<&str> {
    arguments.iter().map(String::as_str).find(|argument| {
        RESERVED.iter().any(|reserved| {
            argument == reserved
                || (reserved.starts_with("--")
                    && argument
                        .strip_prefix(reserved)
                        .is_some_and(|rest| rest.starts_with('=')))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::workspace::{PaneId, PaneMetadata};

    fn pane(arguments: &[&str]) -> Pane {
        Pane {
            id: PaneId::new(),
            tool: "claude".into(),
            metadata: PaneMetadata {
                cwd: Some(std::env::temp_dir()),
                arguments: arguments.iter().map(|a| (*a).into()).collect(),
                ..Default::default()
            },
        }
    }

    fn env() -> ShellEnvironment {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join("claude");
        std::fs::write(&claude, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = dir.keep();
        ShellEnvironment {
            shell: "/bin/sh".into(),
            vars: [
                ("PATH".to_owned(), path.to_string_lossy().into_owned()),
                ("NODE_OPTIONS".to_owned(), "--inspect".to_owned()),
                (
                    "ANTHROPIC_API_KEY".to_owned(),
                    "sk-ant-secret-value".to_owned(),
                ),
            ]
            .into(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn headless_flags_follow_profile_arguments_and_resume() {
        let mut pane = pane(&["--add-dir", "/tmp"]);
        pane.metadata.resume_id = Some("4f1c2b8e-1d2a-4c5b-9e7f-0a1b2c3d4e5f".into());
        let launch = prepare_claude(
            &ToolCatalog::default(),
            &pane,
            &env(),
            &ChatSettings::default(),
            None,
        )
        .unwrap();
        let args = &launch.spec.arguments;
        let resume = args.iter().position(|a| a == "--resume").unwrap();
        let protocol = args.iter().position(|a| a == "--output-format").unwrap();
        assert!(args.iter().position(|a| a == "--add-dir").unwrap() < resume);
        assert!(resume < protocol);
        let mut tail = HEADLESS_ARGUMENTS.map(str::to_owned).to_vec();
        tail.push(ALLOW_YOLO.into());
        assert!(args.ends_with(&tail));
        assert!(!launch.spec.environment.contains_key("NODE_OPTIONS"));
        assert_eq!(
            launch.spec.environment["CLAUDE_CODE_SDK_READS_SESSION_STATE"],
            "1"
        );
        assert_eq!(launch.spec.secrets, ["sk-ant-secret-value"]);
        let mut profile_resume = pane.clone();
        profile_resume
            .metadata
            .arguments
            .push("--session-id=other".into());
        assert!(
            prepare_claude(
                &ToolCatalog::default(),
                &profile_resume,
                &env(),
                &ChatSettings::default(),
                None,
            )
            .err()
            .unwrap()
            .contains("--session-id=other")
        );
    }

    #[cfg(unix)]
    #[test]
    fn chat_choices_override_the_profile_and_allow_yolo() {
        let mut catalog = ToolCatalog::default();
        let claude = catalog.tools.iter_mut().find(|t| t.id == "claude").unwrap();
        claude.profiles[0].model = "sonnet".into();
        claude.profiles[0].settings.effort_level = "low".into();
        let value = |args: &[String], flag: &str| {
            let at = args.iter().position(|a| a == flag).unwrap();
            args[at + 1].clone()
        };
        let keep = prepare_claude(&catalog, &pane(&[]), &env(), &ChatSettings::default(), None)
            .unwrap()
            .spec
            .arguments;
        assert_eq!(value(&keep, "--model"), "sonnet");
        assert_eq!(value(&keep, "--effort"), "low");
        assert!(!keep.iter().any(|a| a == "--permission-mode"));
        assert!(keep.iter().any(|a| a == ALLOW_YOLO));
        let chosen = ChatSettings {
            model: Some("opus".into()),
            effort: Some(crate::chat::config::Effort::High),
            mode: crate::chat::config::Mode::Yolo,
        };
        let args = prepare_claude(&catalog, &pane(&[]), &env(), &chosen, None)
            .unwrap()
            .spec
            .arguments;
        assert_eq!(value(&args, "--model"), "opus");
        assert_eq!(value(&args, "--effort"), "high");
        assert_eq!(value(&args, "--permission-mode"), "bypassPermissions");
        let mut missing = pane(&[]);
        missing.metadata.profile_id = Some("deleted".into());
        assert!(
            prepare_claude(&catalog, &missing, &env(), &ChatSettings::default(), None)
                .err()
                .unwrap()
                .contains("no longer exists")
        );
    }

    #[test]
    fn conflicting_user_arguments_are_rejected_by_name() {
        for (args, name) in [
            (&["-p"][..], "-p"),
            (&["--output-format=json"][..], "--output-format=json"),
            (&["--continue"][..], "--continue"),
            (&["--input-format", "text"][..], "--input-format"),
        ] {
            assert_eq!(
                reserved_argument(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>()),
                Some(name)
            );
        }
        assert_eq!(
            reserved_argument(&["--output-formatting".into(), "--printer".into()]),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn conflicting_profile_argument_stops_the_launch() {
        let error = prepare_claude(
            &ToolCatalog::default(),
            &pane(&["--print"]),
            &env(),
            &ChatSettings::default(),
            None,
        )
        .err()
        .unwrap();
        assert!(error.contains("--print"));
    }

    #[test]
    fn only_claude_is_supported() {
        let mut pane = pane(&[]);
        pane.tool = "codex".into();
        assert!(
            prepare_claude(
                &ToolCatalog::default(),
                &pane,
                &env(),
                &ChatSettings::default(),
                None
            )
            .is_err()
        );
    }
}
