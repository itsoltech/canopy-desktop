//! Per-process configuration overlays. User-owned configuration is never overwritten.
use super::{environment::ShellEnvironment, session::LaunchSpec};
use crate::state::{agent_settings::Agent, tools::ToolCatalog, workspace::Pane};
use std::{fs, path::Path};

pub struct Prepared {
    pub spec: LaunchSpec,
    pub environment: ShellEnvironment,
    pub files: Option<tempfile::TempDir>,
}
pub fn prepare(
    catalog: &ToolCatalog,
    pane: &Pane,
    base: &ShellEnvironment,
) -> Result<Prepared, String> {
    prepare_with_data_dir(catalog, pane, base, None)
}

fn prepare_with_data_dir(
    catalog: &ToolCatalog,
    pane: &Pane,
    base: &ShellEnvironment,
    data_dir: Option<&Path>,
) -> Result<Prepared, String> {
    let spec = catalog.launch(pane, base)?;
    let mut environment = base.clone();
    let mut files = None;
    let tool = catalog.get(&pane.tool).ok_or("Tool not found.")?;
    let profile = pane
        .metadata
        .profile_id
        .as_ref()
        .or(tool.default_profile.as_ref())
        .and_then(|id| tool.profiles.iter().find(|p| &p.id == id));
    if let (Some(agent), Some(profile)) = (Agent::from_id(&tool.id), profile) {
        let prefs = &profile.settings;
        prefs.validate(Some(agent))?;
        environment.extend_overrides(prefs.environment(agent));
        if let Some(id) = &prefs.api_key_ref {
            environment.insert_override(agent.api_env().into(), super::credentials::load(id)?);
        }
        if agent == Agent::Gemini && !prefs.settings_json.trim().is_empty() {
            #[cfg(windows)]
            let home = crate::platform::directories::user_home()?;
            #[cfg(not(windows))]
            let home = std::path::PathBuf::from(
                environment
                    .value("HOME")
                    .ok_or("HOME is unavailable for agent configuration.")?,
            );
            let original = environment
                .value("GEMINI_CLI_HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or(home)
                .join(".gemini");
            let parent = match data_dir {
                Some(path) => path.to_owned(),
                None => crate::platform::directories::data_dir()?,
            }
            .join("agent-config");
            crate::platform::directories::ensure_private_dir(&parent)?;
            let temp = tempfile::Builder::new()
                .prefix("canopy-agent-")
                .tempdir_in(parent)
                .map_err(|_| "Could not create agent configuration directory.")?;
            let target = temp.path().join(".gemini");
            fs::create_dir_all(&target)
                .map_err(|_| "Could not create agent configuration directory.")?;
            overlay(&original, &target, "settings.json", &prefs.settings_json)?;
            environment.insert_override(
                "GEMINI_CLI_HOME".into(),
                temp.path().to_string_lossy().into_owned(),
            );
            #[cfg(windows)]
            environment.insert_override(
                "USERPROFILE".into(),
                temp.path().to_string_lossy().into_owned(),
            );
            files = Some(temp);
        }
    }
    Ok(Prepared {
        spec,
        environment,
        files,
    })
}
fn overlay(original: &Path, target: &Path, filename: &str, json: &str) -> Result<(), String> {
    let patch: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "Invalid settings JSON.")?;
    let mut settings = match fs::read(original.join(filename)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "Existing agent settings contain invalid JSON; no files were changed.")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(_) => return Err("Could not read existing agent settings.".into()),
    };
    merge(&mut settings, patch);
    match fs::read_dir(original) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|_| "Could not read agent configuration directory.")?;
                if entry.file_name() == filename {
                    continue;
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(entry.path(), target.join(entry.file_name()))
                    .map_err(|_| "Could not link existing agent configuration.")?;
                #[cfg(windows)]
                if gemini_auth_file(&entry.file_name()) {
                    std::fs::hard_link(entry.path(), target.join(entry.file_name())).map_err(
                        |_| {
                            "Could not link Gemini authentication into the private profile. Keep the Canopy data directory on the same volume as the user profile."
                        },
                    )?;
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Could not read agent configuration directory.".into()),
    }
    let path = target.join(filename);
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "Could not create private agent settings.")?;
    file.write_all(
        serde_json::to_string_pretty(&settings)
            .map_err(|_| "Could not encode agent settings.")?
            .as_bytes(),
    )
    .map_err(|_| "Could not write agent settings.".into())
}

#[cfg(windows)]
fn gemini_auth_file(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            "oauth_creds.json"
                | "google_accounts.json"
                | "mcp-oauth-tokens.json"
                | "installation_id"
        )
    )
}
fn merge(target: &mut serde_json::Value, patch: serde_json::Value) {
    if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            merge(
                target.entry(key.clone()).or_insert(serde_json::Value::Null),
                value.clone(),
            );
        }
    } else {
        *target = patch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{tools::ToolCatalog, workspace::Workspace};
    use std::collections::HashMap;

    #[test]
    fn gemini_overrides_are_isolated_and_preserve_auth_and_original_settings() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let data = root.path().join("data");
        let original = home.join(".gemini");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::write(
            original.join("settings.json"),
            r#"{"ui":{"theme":"dark","showBanner":true},"hooks":{"Existing":[]}}"#,
        )
        .unwrap();
        std::fs::write(original.join("oauth_creds.json"), "test-placeholder").unwrap();

        let mut catalog = ToolCatalog::default();
        let tool = catalog
            .tools
            .iter_mut()
            .find(|tool| tool.id == "gemini")
            .unwrap();
        tool.executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        tool.profiles[0].settings.settings_json = r#"{"ui":{"showBanner":false}}"#.into();
        let mut workspace = Workspace::new();
        workspace.open("Gemini", "gemini");
        workspace.set_default_cwd(home.clone());
        catalog.bind_default(&mut workspace);
        let env = ShellEnvironment {
            shell: std::env::current_exe().unwrap(),
            vars: HashMap::from([
                ("HOME".into(), home.to_string_lossy().into_owned()),
                (
                    "GEMINI_CLI_HOME".into(),
                    home.to_string_lossy().into_owned(),
                ),
            ]),
        };
        let pane = &workspace.activation_plan()[0];
        let one = prepare_with_data_dir(&catalog, pane, &env, Some(&data)).unwrap();
        let mut second_catalog = catalog.clone();
        second_catalog
            .tools
            .iter_mut()
            .find(|tool| tool.id == "gemini")
            .unwrap()
            .profiles[0]
            .settings
            .settings_json = r#"{"ui":{"theme":"light","showBanner":true}}"#.into();
        let two = prepare_with_data_dir(&second_catalog, pane, &env, Some(&data)).unwrap();

        let target = std::path::PathBuf::from(&one.environment.vars["GEMINI_CLI_HOME"]);
        let second_target = std::path::PathBuf::from(&two.environment.vars["GEMINI_CLI_HOME"]);
        assert!(target.starts_with(data.join("agent-config")));
        assert_ne!(target, second_target);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(target.join(".gemini/settings.json")).unwrap())
                .unwrap();
        let second_value: serde_json::Value = serde_json::from_slice(
            &std::fs::read(second_target.join(".gemini/settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(value["ui"]["theme"], "dark");
        assert_eq!(value["ui"]["showBanner"], false);
        assert!(value["hooks"]["Existing"].is_array());
        assert_eq!(second_value["ui"]["theme"], "light");

        #[cfg(unix)]
        assert!(
            std::fs::symlink_metadata(target.join(".gemini/oauth_creds.json"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        #[cfg(windows)]
        {
            assert_eq!(one.environment.value("USERPROFILE"), target.to_str());
            let linked = target.join(".gemini/oauth_creds.json");
            assert!(
                !std::fs::symlink_metadata(&linked)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            std::fs::write(&linked, "refreshed-placeholder").unwrap();
            assert_eq!(
                std::fs::read_to_string(original.join("oauth_creds.json")).unwrap(),
                "refreshed-placeholder"
            );
        }

        drop(one);
        assert!(!target.exists());
        drop(two);
        assert!(!second_target.exists());
        let original_value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(original.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(original_value["ui"]["theme"], "dark");
        assert_eq!(original_value["ui"]["showBanner"], true);
    }
}
