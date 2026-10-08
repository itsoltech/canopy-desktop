use super::relay::Registration;
use crate::{
    state::{
        tools::{Profile, ToolCatalog},
        workspace::Pane,
    },
    terminal::environment::ShellEnvironment,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
pub const HOOK_HELPER_VERSION: &str = "1";
const HOOK_TIMEOUT_SECONDS: u64 = 2;
const CODEX_WINDOWS_HOOK_TIMEOUT_SECONDS: u64 = 3;
const COMMON: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
];
/// Add our observer to a private in-memory copy of the selected profile.
/// User hooks at every existing layer remain intact; commands stay stable between runs.
pub fn augment(
    catalog: &mut ToolCatalog,
    pane: &mut Pane,
    helper: &std::path::Path,
) -> Result<(), String> {
    augment_for_platform(catalog, pane, helper, cfg!(windows))
}

fn augment_for_platform(
    catalog: &mut ToolCatalog,
    pane: &mut Pane,
    helper: &std::path::Path,
    windows: bool,
) -> Result<(), String> {
    let command = hook_command(helper, windows)?;
    let tool = catalog
        .tools
        .iter_mut()
        .find(|t| t.id == pane.tool)
        .ok_or("Tool unavailable.")?;
    let id = pane
        .metadata
        .profile_id
        .clone()
        .or(tool.default_profile.clone());
    let index = match id {
        Some(id) => tool
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or("Agent profile unavailable.")?,
        None => {
            tool.profiles.push(Profile {
                id: "canopy-runtime".into(),
                name: "Runtime".into(),
                model: String::new(),
                arguments: vec![],
                settings: Default::default(),
            });
            pane.metadata.profile_id = Some("canopy-runtime".into());
            tool.profiles.len() - 1
        }
    };
    let settings = &mut tool.profiles[index].settings.settings_json;
    let mut value: serde_json::Value = if settings.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(settings).map_err(|_| "Invalid agent settings JSON.")?
    };
    let object = value
        .as_object_mut()
        .ok_or("Agent settings must be an object.")?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("Hooks must be an object.")?;
    let events: Vec<_> = COMMON
        .iter()
        .copied()
        .chain(if tool.id == "claude" {
            vec![
                "Notification",
                "PostToolUseFailure",
                "PermissionDenied",
                "StopFailure",
            ]
        } else {
            vec!["Interrupt", "PostCompact"]
        })
        .collect();
    for event in events {
        let timeout = if windows && tool.id == "codex" {
            CODEX_WINDOWS_HOOK_TIMEOUT_SECONDS
        } else {
            HOOK_TIMEOUT_SECONDS
        };
        let mut handler = serde_json::json!({
            "type": "command",
            "command": command,
            "timeout": timeout,
        });
        if windows && tool.id == "codex" {
            handler["commandWindows"] = serde_json::Value::String(codex_windows_command(helper)?);
        }
        hooks
            .entry(event)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or("Hook event must contain an array.")?
            .push(serde_json::json!({"hooks":[handler]}));
    }
    *settings = value.to_string();
    if tool.id == "codex" {
        tool.arguments.extend(["--enable".into(), "hooks".into()]);
    }
    Ok(())
}

fn codex_windows_command(helper: &std::path::Path) -> Result<String, String> {
    let helper = helper.to_str().ok_or("Hook helper path is not UTF-8.")?;
    if helper.contains(['\0', '\r', '\n']) {
        return Err("Hook helper path contains a control character.".into());
    }
    let script = format!("& '{}'", helper.replace('\'', "''"));
    let mut utf16le = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        utf16le.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(format!(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
        STANDARD.encode(utf16le)
    ))
}

pub fn validate_helper(helper: &std::path::Path) -> Result<(), String> {
    if !helper.is_file() {
        return Err(format!(
            "Agent hook helper is missing next to Canopy: {}",
            helper.display()
        ));
    }
    let mut command = std::process::Command::new(helper);
    command.arg("--canopy-hook-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|error| format!("Could not start the agent hook helper: {error}"))?;
    let version_matches = output.stdout == format!("{HOOK_HELPER_VERSION}\n").as_bytes()
        || output.stdout == format!("{HOOK_HELPER_VERSION}\r\n").as_bytes();
    if !output.status.success() || !version_matches || !output.stderr.is_empty() {
        return Err("The agent hook helper does not match this Canopy build.".into());
    }
    Ok(())
}

pub fn helper_path(gui_executable: &std::path::Path) -> std::path::PathBuf {
    if cfg!(windows) {
        gui_executable.with_file_name("canopy-agent-hook.exe")
    } else {
        gui_executable.to_owned()
    }
}

fn hook_command(helper: &std::path::Path, windows: bool) -> Result<String, String> {
    let helper = helper.to_str().ok_or("Hook helper path is not UTF-8.")?;
    if windows {
        if helper.contains('"') {
            return Err("Hook helper path contains an invalid quote.".into());
        }
        Ok(format!("\"{helper}\""))
    } else {
        Ok(format!("'{}' --agent-hook", helper.replace('\'', "'\\''")))
    }
}

pub fn environment(env: &mut ShellEnvironment, registration: &Registration) {
    env.insert_override("CANOPY_AGENT_RUN".into(), registration.run.clone());
    env.insert_override("CANOPY_AGENT_TOKEN".into(), registration.token.clone());
    env.insert_override(
        "CANOPY_AGENT_ENDPOINT_KIND".into(),
        registration.endpoint.kind().into(),
    );
    env.insert_override(
        "CANOPY_AGENT_ENDPOINT".into(),
        registration.endpoint.address(),
    );
    if let super::relay::Endpoint::UnixSocket(socket) = &registration.endpoint {
        env.insert_override(
            "CANOPY_AGENT_SOCKET".into(),
            socket.to_string_lossy().into_owned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_commands_quote_required_platform_paths() {
        let helper = std::path::Path::new("/tmp/Canopy's App/canopy");
        assert_eq!(
            hook_command(helper, false).unwrap(),
            "'/tmp/Canopy'\\''s App/canopy' --agent-hook"
        );
        let helper = std::path::Path::new(r"C:\Program Files\Canopy's App\canopy-agent-hook.exe");
        assert_eq!(
            hook_command(helper, true).unwrap(),
            r#""C:\Program Files\Canopy's App\canopy-agent-hook.exe""#
        );
    }

    #[test]
    fn codex_windows_uses_an_encoded_powershell_invocation() {
        let helper = std::path::Path::new(r"C:\Program Files\Canopy's App\canopy-agent-hook.exe");
        let command = codex_windows_command(helper).unwrap();
        let encoded = command.split_whitespace().last().unwrap();
        let bytes = STANDARD.decode(encoded).unwrap();
        let units = bytes
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        assert_eq!(
            String::from_utf16(&units).unwrap(),
            r"& 'C:\Program Files\Canopy''s App\canopy-agent-hook.exe'"
        );
    }

    #[test]
    fn windows_overlays_keep_user_hooks_and_only_codex_gets_the_command_override() {
        let mut catalog = ToolCatalog::default();
        catalog
            .tools
            .iter_mut()
            .find(|tool| tool.id == "codex")
            .unwrap()
            .profiles[0]
            .settings
            .settings_json =
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"user-hook"}]}]}}"#.into();
        let mut workspace = crate::state::workspace::Workspace::new();
        workspace.open("Codex", "codex");
        catalog.bind_default(&mut workspace);
        let mut pane = workspace.activation_plan()[0].clone();
        augment_for_platform(
            &mut catalog,
            &mut pane,
            std::path::Path::new(r"C:\Program Files\Canopy\canopy-agent-hook.exe"),
            true,
        )
        .unwrap();
        let tool = catalog.get("codex").unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&tool.profiles[0].settings.settings_json).unwrap();
        let stop = value["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0]["hooks"][0]["command"], "user-hook");
        assert!(
            stop[1]["hooks"][0]["commandWindows"]
                .as_str()
                .unwrap()
                .contains("-EncodedCommand")
        );
        assert_eq!(stop[1]["hooks"][0]["timeout"], 3);

        let mut catalog = ToolCatalog::default();
        let mut workspace = crate::state::workspace::Workspace::new();
        workspace.open("Claude", "claude");
        catalog.bind_default(&mut workspace);
        let mut pane = workspace.activation_plan()[0].clone();
        augment_for_platform(
            &mut catalog,
            &mut pane,
            std::path::Path::new(r"C:\Program Files\Canopy\canopy-agent-hook.exe"),
            true,
        )
        .unwrap();
        let tool = catalog.get("claude").unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&tool.profiles[0].settings.settings_json).unwrap();
        let stop = value["hooks"]["Stop"].as_array().unwrap();
        assert!(stop[0]["hooks"][0].get("commandWindows").is_none());
        assert_eq!(stop[0]["hooks"][0]["timeout"], 2);
    }
}
