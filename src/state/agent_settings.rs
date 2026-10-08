//! Provider-specific preferences. Secrets are represented only by opaque Keychain references.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentSettings {
    pub permission_mode: String,
    pub effort_level: String,
    pub append_system_prompt: String,
    pub base_url: String,
    pub provider: String,
    pub approval_mode: String,
    pub sandbox: String,
    pub full_auto: bool,
    pub bypass_approvals: bool,
    pub config_profile: String,
    pub custom_env: BTreeMap<String, String>,
    pub settings_json: String,
    pub api_key_ref: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
    Gemini,
    OpenCode,
}
impl Agent {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }
    pub fn api_env(self) -> &'static str {
        match self {
            Self::Claude | Self::OpenCode => "ANTHROPIC_API_KEY",
            Self::Codex => "OPENAI_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
        }
    }
}
impl AgentSettings {
    pub fn validate(&self, agent: Option<Agent>) -> Result<(), String> {
        for s in [
            &self.permission_mode,
            &self.effort_level,
            &self.append_system_prompt,
            &self.base_url,
            &self.provider,
            &self.approval_mode,
            &self.sandbox,
            &self.config_profile,
            &self.settings_json,
        ] {
            if s.len() > 65536 || s.contains('\0') {
                return Err("An agent setting is too long or contains a NUL character.".into());
            }
        }
        if !self.settings_json.trim().is_empty() {
            let value: serde_json::Value = serde_json::from_str(&self.settings_json)
                .map_err(|_| "Settings override must be valid JSON.")?;
            if !value.is_object() {
                return Err("Settings override must be a JSON object.".into());
            }
        }
        if self
            .api_key_ref
            .as_ref()
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err("Invalid API key reference.".into());
        }
        if self.custom_env.len() > 128 {
            return Err("Too many environment variables.".into());
        }
        for (key, value) in &self.custom_env {
            validate_env(key, value)?;
        }
        let valid = match agent {
            Some(Agent::Claude) => {
                ["", "plan", "auto", "acceptEdits", "bypassPermissions"]
                    .contains(&self.permission_mode.as_str())
                    && ["", "low", "medium", "high", "xhigh", "max"]
                        .contains(&self.effort_level.as_str())
                    && ["", "bedrock", "vertex", "foundry"].contains(&self.provider.as_str())
            }
            Some(Agent::Codex) => {
                ["", "untrusted", "on-request", "never"].contains(&self.approval_mode.as_str())
                    && ["", "read-only", "workspace-write", "danger-full-access"]
                        .contains(&self.sandbox.as_str())
            }
            Some(Agent::Gemini) => {
                ["", "default", "auto_edit", "yolo", "plan"].contains(&self.approval_mode.as_str())
            }
            _ => true,
        };
        if agent == Some(Agent::Codex) {
            self.codex_hooks()?;
        }
        if !valid {
            return Err("Unsupported agent option.".into());
        }
        Ok(())
    }
    pub fn arguments(&self, agent: Agent) -> Vec<String> {
        let mut args = vec![];
        let mut push = |flag: &str, value: &str| {
            if !value.is_empty() {
                args.extend([flag.to_owned(), value.to_owned()]);
            }
        };
        match agent {
            Agent::Claude => {
                push("--permission-mode", &self.permission_mode);
                push("--effort", &self.effort_level);
                push("--append-system-prompt", &self.append_system_prompt);
                push("--settings", &self.settings_json);
            }
            Agent::Codex => {
                if self.bypass_approvals {
                    args.push("--dangerously-bypass-approvals-and-sandbox".into());
                } else if self.full_auto {
                    args.extend([
                        "--sandbox".into(),
                        "workspace-write".into(),
                        "--ask-for-approval".into(),
                        "on-request".into(),
                    ]);
                } else {
                    push("--ask-for-approval", &self.approval_mode);
                    push("--sandbox", &self.sandbox);
                }
                if let Ok(Some(hooks)) = self.codex_hooks() {
                    args.extend(["--config".into(), format!("hooks={hooks}")]);
                }
                if !self.config_profile.is_empty() {
                    args.extend(["--profile".into(), self.config_profile.clone()]);
                }
            }
            Agent::Gemini => push("--approval-mode", &self.approval_mode),
            Agent::OpenCode => {}
        }
        args
    }
    fn codex_hooks(&self) -> Result<Option<String>, String> {
        if self.settings_json.trim().is_empty() {
            return Ok(None);
        }
        let value: serde_json::Value =
            serde_json::from_str(&self.settings_json).map_err(|_| "Invalid settings JSON.")?;
        let object = value
            .as_object()
            .ok_or("Settings override must be a JSON object.")?;
        if object
            .keys()
            .any(|key| key != "hooks" && key != "description")
        {
            return Err("Codex settings override accepts hooks and description fields.".into());
        }
        object
            .get("hooks")
            .map(|hooks| {
                if !hooks.is_object() {
                    return Err("Codex hooks must be an object.".into());
                }
                json_toml(hooks)
            })
            .transpose()
    }
    pub fn environment(&self, agent: Agent) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        if !self.base_url.is_empty() {
            match agent {
                Agent::Claude => {
                    env.insert("ANTHROPIC_BASE_URL".into(), self.base_url.clone());
                }
                Agent::Codex => {
                    env.insert("OPENAI_BASE_URL".into(), self.base_url.clone());
                }
                _ => {}
            }
        }
        if agent == Agent::Claude && !self.provider.is_empty() {
            for (p, key) in [
                ("bedrock", "CLAUDE_CODE_USE_BEDROCK"),
                ("vertex", "CLAUDE_CODE_USE_VERTEX"),
                ("foundry", "CLAUDE_CODE_USE_FOUNDRY"),
            ] {
                env.insert(
                    key.into(),
                    if self.provider == p { "1" } else { "0" }.into(),
                );
            }
        }
        if agent == Agent::OpenCode && !self.settings_json.trim().is_empty() {
            env.insert("OPENCODE_CONFIG_CONTENT".into(), self.settings_json.clone());
        }
        env.extend(self.custom_env.clone());
        env
    }
}
pub fn validate_env(key: &str, value: &str) -> Result<(), String> {
    if key.is_empty()
        || key.len() > 256
        || !key
            .bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
        || value.len() > 65536
        || value.contains('\0')
    {
        return Err("Use a valid variable name and a value without NUL characters.".into());
    }
    Ok(())
}

fn json_toml(value: &serde_json::Value) -> Result<String, String> {
    Ok(match value {
        serde_json::Value::Null => {
            return Err("Codex hook settings cannot contain null values.".into());
        }
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        serde_json::Value::String(v) => {
            serde_json::to_string(v).map_err(|_| "Invalid hook string.")?
        }
        serde_json::Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(json_toml)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        serde_json::Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| Ok(format!(
                    "{}={}",
                    serde_json::to_string(key).map_err(|_| "Invalid hook key.")?,
                    json_toml(value)?
                )))
                .collect::<Result<Vec<_>, String>>()?
                .join(",")
        ),
    })
}
