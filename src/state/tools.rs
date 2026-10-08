//! Versioned tool definitions and profiles. No processes or credentials live here.
use crate::{
    state::workspace::Pane,
    terminal::{environment::ShellEnvironment, session::LaunchSpec},
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolKind {
    Shell,
    Claude,
    Codex,
    Custom,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub model: String,
    pub arguments: Vec<String>,
    #[serde(default)]
    pub settings: super::agent_settings::AgentSettings,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub kind: ToolKind,
    pub executable: String,
    pub arguments: Vec<String>,
    pub enabled: bool,
    pub profiles: Vec<Profile>,
    pub default_profile: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCatalog {
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub retired_credentials: Vec<String>,
}
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn format_argument_list(arguments: &[String], windows: bool) -> String {
    if !windows {
        return shell_words::join(arguments.iter().map(String::as_str));
    }
    arguments
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty() && !argument.chars().any(|c| c.is_whitespace() || c == '"') {
        return argument.to_owned();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        if character == '"' {
            quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
            quoted.push('"');
        } else {
            quoted.extend(std::iter::repeat_n('\\', backslashes));
            quoted.push(character);
        }
        backslashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

pub fn parse_argument_list(value: &str, windows: bool) -> Result<Vec<String>, String> {
    if !windows {
        return shell_words::split(value).map_err(|_| "Unclosed quote in arguments.".into());
    }
    let characters: Vec<char> = value.chars().collect();
    let mut arguments = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        while index < characters.len() && characters[index].is_whitespace() {
            index += 1;
        }
        if index == characters.len() {
            break;
        }
        let mut argument = String::new();
        let mut quoted = false;
        let mut started = false;
        while index < characters.len() {
            if !quoted && characters[index].is_whitespace() {
                break;
            }
            let mut backslashes = 0;
            while index < characters.len() && characters[index] == '\\' {
                backslashes += 1;
                index += 1;
            }
            started |= backslashes > 0;
            if index < characters.len() && characters[index] == '"' {
                argument.extend(std::iter::repeat_n('\\', backslashes / 2));
                if backslashes % 2 == 0 {
                    quoted = !quoted;
                } else {
                    argument.push('"');
                }
                started = true;
                index += 1;
                continue;
            }
            argument.extend(std::iter::repeat_n('\\', backslashes));
            if index < characters.len() && (quoted || !characters[index].is_whitespace()) {
                argument.push(characters[index]);
                started = true;
                index += 1;
            }
        }
        if quoted {
            return Err("Unclosed quote in arguments.".into());
        }
        if started {
            arguments.push(argument);
        }
    }
    Ok(arguments)
}
fn text_valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
fn args_valid(args: &[String]) -> bool {
    args.len() <= 128 && args.iter().all(|a| a.len() <= 16384 && !a.contains('\0'))
}
impl Default for ToolCatalog {
    fn default() -> Self {
        let tools = [
            ("shell", "Shell", ToolKind::Shell),
            ("claude", "Claude Code", ToolKind::Claude),
            ("codex", "Codex", ToolKind::Codex),
            ("gemini", "Gemini CLI", ToolKind::Custom),
            ("opencode", "OpenCode", ToolKind::Custom),
            ("lazygit", "LazyGit", ToolKind::Custom),
            ("droid", "Droid", ToolKind::Custom),
        ]
        .into_iter()
        .map(|(id, name, kind)| {
            let profiles = if super::agent_settings::Agent::from_id(id).is_some() {
                vec![Profile {
                    id: format!("{id}-default"),
                    name: "Default".into(),
                    model: String::new(),
                    arguments: vec![],
                    settings: Default::default(),
                }]
            } else {
                vec![]
            };
            ToolDefinition {
                id: id.into(),
                name: name.into(),
                kind,
                executable: if kind == ToolKind::Shell {
                    String::new()
                } else {
                    id.into()
                },
                arguments: if kind == ToolKind::Shell {
                    if cfg!(windows) {
                        vec![]
                    } else {
                        vec!["-l".into()]
                    }
                } else {
                    vec![]
                },
                enabled: true,
                default_profile: profiles.first().map(|p| p.id.clone()),
                profiles,
            }
        })
        .collect();
        Self {
            tools,
            retired_credentials: vec![],
        }
    }
}
impl ToolDefinition {
    pub fn is_agent(&self) -> bool {
        super::agent_settings::Agent::from_id(&self.id).is_some()
    }
    pub fn label(&self, profile: Option<&str>) -> String {
        profile
            .and_then(|id| self.profiles.iter().find(|p| p.id == id))
            .map(|p| format!("{} — {}", self.name, p.name))
            .unwrap_or_else(|| self.name.clone())
    }
}
impl ToolCatalog {
    pub(crate) fn migrate_platform_defaults(&mut self) {
        if !cfg!(windows) {
            return;
        }
        if let Some(shell) = self.tools.iter_mut().find(|tool| tool.id == "shell")
            && shell.name == "Shell"
            && shell.kind == ToolKind::Shell
            && shell.executable.is_empty()
            && shell.arguments == ["-l"]
            && shell.profiles.is_empty()
            && shell.default_profile.is_none()
        {
            shell.arguments.clear();
        }
    }

    pub fn get(&self, id: &str) -> Option<&ToolDefinition> {
        self.tools.iter().find(|t| t.id == id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.tools.len() > 128 {
            return Err("Too many tools.".into());
        }
        let active_credentials: std::collections::HashSet<_> = self
            .tools
            .iter()
            .flat_map(|tool| &tool.profiles)
            .filter_map(|profile| profile.settings.api_key_ref.as_deref())
            .collect();
        let mut retired = std::collections::HashSet::new();
        if self.retired_credentials.len() > 128 * 128
            || self.retired_credentials.iter().any(|id| {
                uuid::Uuid::parse_str(id).is_err()
                    || !retired.insert(id)
                    || active_credentials.contains(id.as_str())
            })
        {
            return Err("Invalid retired API key references.".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        let mut profile_ids = std::collections::HashSet::new();
        for tool in &self.tools {
            if !text_valid(&tool.id)
                || !text_valid(&tool.name)
                || !ids.insert(&tool.id)
                || !names.insert(tool.name.trim().to_lowercase())
            {
                return Err("Tool names and IDs must be non-empty and unique.".into());
            }
            if (tool.executable.is_empty() && tool.kind != ToolKind::Shell)
                || (!tool.executable.is_empty() && !text_valid(&tool.executable))
                || !args_valid(&tool.arguments)
            {
                return Err("Provide a valid executable and arguments.".into());
            }
            if tool.profiles.len() > 128 {
                return Err("Too many profiles.".into());
            }
            let mut profile_names = std::collections::HashSet::new();
            for p in &tool.profiles {
                p.settings
                    .validate(super::agent_settings::Agent::from_id(&tool.id))?;
                if !text_valid(&p.id)
                    || !text_valid(&p.name)
                    || !profile_ids.insert(&p.id)
                    || !profile_names.insert(p.name.trim().to_lowercase())
                {
                    return Err("Profile names must be unique within a tool.".into());
                }
                if !args_valid(&p.arguments)
                    || (!p.model.is_empty() && (!tool.is_agent() || !text_valid(&p.model)))
                {
                    return Err("Invalid profile model or arguments.".into());
                }
            }
            if tool
                .default_profile
                .as_ref()
                .is_some_and(|id| !tool.profiles.iter().any(|p| &p.id == id))
            {
                return Err("The default profile does not exist.".into());
            }
            let required = match tool.kind {
                ToolKind::Shell => Some("shell"),
                ToolKind::Claude => Some("claude"),
                ToolKind::Codex => Some("codex"),
                ToolKind::Custom => None,
            };
            if required.is_some_and(|id| tool.id != id) {
                return Err("Built-in tool identity cannot be changed.".into());
            }
        }
        for (id, kind) in [
            ("shell", ToolKind::Shell),
            ("claude", ToolKind::Claude),
            ("codex", ToolKind::Codex),
        ] {
            if !self.get(id).is_some_and(|t| t.kind == kind) {
                return Err("Built-in tools cannot be deleted.".into());
            }
        }
        Ok(())
    }
    pub fn upsert(&mut self, tool: ToolDefinition) -> Result<(), String> {
        let mut next = self.clone();
        if let Some(old) = next.tools.iter_mut().find(|t| t.id == tool.id) {
            *old = tool;
        } else {
            next.tools.push(tool);
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let tool = self.get(id).ok_or("Tool not found.")?;
        if tool.kind != ToolKind::Custom {
            return Err("Built-in tools can be disabled, not deleted.".into());
        }
        self.tools.retain(|t| t.id != id);
        Ok(())
    }
    /// Bind the selected default to a newly created pane, preserving its identity on restore.
    pub fn bind_default(&self, workspace: &mut crate::state::workspace::Workspace) {
        let Some(tab) = workspace.active() else {
            return;
        };
        let (tab_id, pane_id) = (tab.id, tab.focused);
        let Some(pane) = tab.root.find(pane_id) else {
            return;
        };
        let Some(tool) = self.get(&pane.tool) else {
            return;
        };
        let mut metadata = pane.metadata.clone();
        metadata.profile_id = tool.default_profile.clone();
        metadata.title = Some(tool.label(metadata.profile_id.as_deref()));
        let _ = workspace.set_pane_metadata(tab_id, pane_id, metadata);
    }
    pub fn launch(&self, pane: &Pane, env: &ShellEnvironment) -> Result<LaunchSpec, String> {
        let tool = self
            .get(&pane.tool)
            .ok_or("This tool was deleted or is no longer configured.")?;
        if !tool.enabled {
            return Err("This tool is disabled in Preferences.".into());
        }
        let profile_id = pane
            .metadata
            .profile_id
            .as_ref()
            .or(tool.default_profile.as_ref());
        let profile = profile_id
            .map(|id| {
                tool.profiles
                    .iter()
                    .find(|p| &p.id == id)
                    .ok_or("This profile no longer exists.")
            })
            .transpose()?;
        let mut arguments = tool.arguments.clone();
        if let Some(profile) = profile {
            if !profile.model.is_empty() {
                arguments.extend(["--model".into(), profile.model.clone()]);
            }
            if let Some(agent) = super::agent_settings::Agent::from_id(&tool.id) {
                arguments.extend(profile.settings.arguments(agent));
            }
            arguments.extend(profile.arguments.clone());
        }
        arguments.extend(pane.metadata.arguments.clone());
        if let Some(id) = &pane.metadata.resume_id {
            if !crate::agents::valid_session(id) || uuid::Uuid::parse_str(id).is_err() {
                return Err("Invalid saved agent session id.".into());
            }
            let conflict = arguments.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "--resume" | "--last" | "--continue" | "--fork-session" | "--session-id"
                ) || arg.starts_with("--resume=")
                    || (tool.id == "claude" && matches!(arg.as_str(), "-r" | "-c"))
            });
            if conflict {
                return Err("Saved session conflicts with custom resume arguments. Remove those arguments before restarting.".into());
            }
            match tool.id.as_str() {
                "claude" => arguments.extend(["--resume".into(), id.clone()]),
                "codex" => {
                    arguments.insert(0, "resume".into());
                    arguments.insert(1, id.clone());
                }
                _ => return Err("This tool does not support session resume.".into()),
            }
        }

        let program = if tool.kind == ToolKind::Shell && tool.executable.is_empty() {
            env.shell.clone()
        } else {
            env.resolve(&tool.executable)?
        };
        let (program, arguments) = env.prepare_launch(program, arguments)?;
        Ok(LaunchSpec {
            program,
            arguments,
            cwd: pane
                .metadata
                .cwd
                .clone()
                .ok_or("Pane has no working directory.")?,
        })
    }
}

#[cfg(test)]
mod argument_tests {
    use super::{format_argument_list, parse_argument_list};

    #[test]
    fn windows_argument_editor_round_trips_backslashes_spaces_quotes_and_empty_values() {
        let arguments = vec![
            r"C:\Users\Żaneta\project".to_owned(),
            r"C:\Program Files\tool\".to_owned(),
            r#"value\"with-quote"#.to_owned(),
            "".to_owned(),
            r"\".to_owned(),
            r"\\".to_owned(),
            "plain".to_owned(),
        ];
        let formatted = format_argument_list(&arguments, true);
        assert_eq!(parse_argument_list(&formatted, true).unwrap(), arguments);
    }

    #[test]
    fn windows_argument_editor_does_not_treat_backslashes_as_escapes() {
        assert_eq!(
            parse_argument_list(r#"--config C:\Users\name\settings.json "two words""#, true)
                .unwrap(),
            ["--config", r"C:\Users\name\settings.json", "two words"]
        );
        assert!(parse_argument_list("\"unfinished", true).is_err());
    }
}
