//! Per-chat choices (model, reasoning effort, work mode) and the models a provider offers.
use serde::{Deserialize, Serialize};

/// How much the agent may do without asking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    /// Prompts for anything not already allowed (Claude `default`).
    #[default]
    Ask,
    AcceptEdits,
    Plan,
    /// A classifier approves or denies prompts; only on models that support it.
    Auto,
    /// Skips every permission prompt (Claude `bypassPermissions`).
    Yolo,
    /// Codex: reads freely, asks before changing anything.
    ReadOnly,
    /// Codex: no sandbox and no prompts.
    FullAccess,
}
impl Mode {
    pub const ALL: [Self; 7] = [
        Self::Ask,
        Self::AcceptEdits,
        Self::Plan,
        Self::Auto,
        Self::Yolo,
        Self::ReadOnly,
        Self::FullAccess,
    ];
    pub const CLAUDE: [Self; 5] = [
        Self::Ask,
        Self::AcceptEdits,
        Self::Plan,
        Self::Auto,
        Self::Yolo,
    ];
    pub const CODEX: [Self; 4] = [Self::ReadOnly, Self::Ask, Self::Auto, Self::FullAccess];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::AcceptEdits => "Accept edits",
            Self::Plan => "Plan",
            Self::Auto => "Auto",
            Self::Yolo => "Yolo",
            Self::ReadOnly => "Read only",
            Self::FullAccess => "Full access",
        }
    }
    /// Runs everything without asking: shown as a warning, never a new chat's default.
    pub fn unguarded(self) -> bool {
        matches!(self, Self::Yolo | Self::FullAccess)
    }
    /// Codex approval policy and sandbox mode (`thread/start`).
    pub fn codex(self) -> (&'static str, &'static str) {
        match self {
            Self::ReadOnly => ("on-request", "read-only"),
            Self::Ask | Self::AcceptEdits | Self::Plan => ("untrusted", "workspace-write"),
            Self::Auto => ("on-request", "workspace-write"),
            Self::Yolo | Self::FullAccess => ("never", "danger-full-access"),
        }
    }
    /// The Codex mode of a profile's approval and sandbox settings.
    pub fn from_codex(approval: &str, sandbox: &str, full_auto: bool, bypass: bool) -> Self {
        match (approval, sandbox) {
            _ if bypass => Self::FullAccess,
            _ if full_auto => Self::Auto,
            (_, "danger-full-access") | ("never", _) => Self::FullAccess,
            (_, "read-only") => Self::ReadOnly,
            ("untrusted", _) => Self::Ask,
            _ => Self::Auto,
        }
    }
    /// The same intent among Codex's modes.
    pub fn for_codex(self) -> Self {
        match self {
            Self::AcceptEdits | Self::Plan => Self::Ask,
            Self::Yolo => Self::FullAccess,
            other => other,
        }
    }
    /// The same intent among Claude's modes.
    pub fn for_claude(self) -> Self {
        match self {
            Self::ReadOnly => Self::Ask,
            Self::FullAccess => Self::Yolo,
            other => other,
        }
    }
    /// Claude permission mode for `set_permission_mode`.
    pub fn claude(self) -> &'static str {
        match self.for_claude() {
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::Auto => "auto",
            Self::Yolo => "bypassPermissions",
            _ => "default",
        }
    }
    /// Claude profile setting at launch; empty keeps the CLI default.
    pub fn claude_profile(self) -> &'static str {
        match self {
            Self::Ask => "",
            other => other.claude(),
        }
    }
    pub fn from_claude(mode: &str) -> Option<Self> {
        match mode {
            "" | "default" => Some(Self::Ask),
            "acceptEdits" => Some(Self::AcceptEdits),
            "plan" => Some(Self::Plan),
            "auto" => Some(Self::Auto),
            "bypassPermissions" => Some(Self::Yolo),
            _ => None,
        }
    }
    /// Stable preference value.
    pub fn key(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::AcceptEdits => "accept-edits",
            Self::Plan => "plan",
            Self::Auto => "auto",
            Self::Yolo => "yolo",
            Self::ReadOnly => "read-only",
            Self::FullAccess => "full-access",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Effort {
    Low,
    Medium,
    High,
    XHigh,
    Max,
    /// Codex only, on models that offer it.
    Ultra,
}
impl Effort {
    pub const ALL: [Self; 6] = [
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
        Self::Ultra,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::XHigh => "Extra high",
            Self::Max => "Max",
            Self::Ultra => "Ultra",
        }
    }
    /// Claude `--effort` / `effortLevel` value; also the preference value.
    pub fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|effort| effort.key() == key)
    }
}

/// The choices a chat pane keeps across restarts. The account is the pane's profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatSettings {
    /// Provider model id; `None` uses the account's default model.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<Effort>,
    #[serde(default)]
    pub mode: Mode,
    /// Permission rules "Allow for this chat" remembered (`Bash(npm test *)`); passed to
    /// each start of the chat's process, never written to settings files (DEC-039).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed: Vec<String>,
    /// Identifies the chat's saved history (a UUID); stays the same across Restart,
    /// Edit and New session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}
impl ChatSettings {
    pub fn valid(&self) -> bool {
        self.model.as_deref().is_none_or(valid_model)
            && self
                .id
                .as_deref()
                .is_none_or(|id| uuid::Uuid::parse_str(id).is_ok())
    }

    /// A new chat id.
    pub fn new_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Remembers rules, skipping invalid ones and duplicates; true when any was added.
    pub fn allow(&mut self, rules: impl IntoIterator<Item = String>) -> bool {
        let before = self.allowed.len();
        for rule in rules {
            if valid_rule(&rule) && !self.allowed.contains(&rule) {
                self.allowed.push(rule);
            }
        }
        self.allowed.len() != before
    }
}

/// A permission rule as the CLI writes it: a tool name with an optional `(pattern)`.
pub fn valid_rule(rule: &str) -> bool {
    !rule.trim().is_empty() && rule.len() <= 1024 && !rule.chars().any(char::is_control)
}

/// Model ids come from the provider CLI and are passed back as one argv value.
pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.starts_with('-')
        && model.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

/// A command or skill the agent offers (`/name` in the composer), as its CLI reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    /// What it takes after its name (`<pr-number>`).
    pub hint: Option<String>,
    /// Where a Codex skill lives (sent with it).
    pub path: Option<std::path::PathBuf>,
    /// A skill (offered after `$`), not a built-in command.
    pub skill: bool,
}

/// Marks the commands named in the agent's skill list. Plugin skills are listed as
/// `plugin:name` and their commands by `name`.
pub fn mark_skills(commands: &mut [CommandInfo], skills: &[String]) {
    for command in commands {
        command.skill = skills.iter().any(|skill| {
            skill == &command.name || skill.rsplit(':').next() == Some(command.name.as_str())
        });
    }
}

/// Command names are typed after `/` and sent back as one word.
pub fn valid_command(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| !c.is_control() && !c.is_whitespace() && c != '[' && c != ']')
}

/// Sorted by name, one entry per name.
pub fn tidy_commands(mut commands: Vec<CommandInfo>) -> Vec<CommandInfo> {
    commands.sort_by_key(|command| command.name.to_lowercase());
    commands.dedup_by(|a, b| a.name == b.name);
    commands
}

/// A model the provider account offers, as reported by its CLI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelInfo {
    pub value: String,
    /// The concrete model an alias stands for (`claude-opus-5-5`, `…[1m]`).
    pub resolved: Option<String>,
    pub display_name: String,
    pub description: String,
    pub effort_levels: Vec<Effort>,
    pub supports_auto: bool,
    /// The effort the provider uses when none is chosen (Codex).
    pub default_effort: Option<Effort>,
    /// The account's default model (Codex reports it; Claude has a `default` entry).
    pub default: bool,
}
impl ModelInfo {
    pub fn supports_effort(&self) -> bool {
        !self.effort_levels.is_empty()
    }

    /// A specific name: "Opus 5.5", "Fable 5.1 · 1M context", "Default (Opus 5.5)".
    /// The CLI's display names are bare families ("Opus"); its descriptions start
    /// with the versioned name.
    pub fn label(&self) -> String {
        let lead = self
            .description
            .split(" · ")
            .next()
            .unwrap_or_default()
            .trim();
        let mut label = if self.value == "default" {
            match lead
                .split_once("(currently ")
                .and_then(|(_, rest)| rest.split_once(')'))
            {
                Some((current, _)) => format!("Default ({current})"),
                None => "Default".to_owned(),
            }
        } else {
            let family = self
                .display_name
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if !family.is_empty() && lead.starts_with(family) && lead.len() <= 40 {
                lead.to_owned()
            } else {
                self.display_name.clone()
            }
        };
        let wide = |name: &str| name.to_ascii_lowercase().contains("[1m]");
        if wide(&self.value) || self.resolved.as_deref().is_some_and(wide) {
            label.push_str(" · 1M context");
        }
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(value: &str, resolved: &str, name: &str, description: &str) -> ModelInfo {
        ModelInfo {
            value: value.into(),
            resolved: Some(resolved.into()),
            display_name: name.into(),
            description: description.into(),
            effort_levels: vec![],
            supports_auto: false,
            ..Default::default()
        }
    }

    #[test]
    fn model_labels_name_the_version_and_context() {
        let labels = [
            model(
                "default",
                "claude-opus-5-5",
                "Default (recommended)",
                "Use the default model (currently Opus 5.5) · $4/$20 per Mtok",
            ),
            model(
                "opus",
                "claude-opus-5-5",
                "Opus",
                "Opus 5.5 · Best for everyday, complex tasks",
            ),
            model(
                "claude-fable-5-1[1m]",
                "claude-fable-5-1[1m]",
                "Fable",
                "Fable 5.1 · Most capable",
            ),
            model("custom", "x", "Custom", "Something else entirely"),
        ]
        .map(|m| m.label());
        assert_eq!(
            labels,
            [
                "Default (Opus 5.5)",
                "Opus 5.5",
                "Fable 5.1 · 1M context",
                "Custom"
            ]
        );
    }

    #[test]
    fn modes_round_trip_through_claude_and_preference_values() {
        for mode in Mode::CLAUDE {
            assert_eq!(Mode::from_claude(mode.claude()), Some(mode));
        }
        for mode in Mode::ALL {
            assert_eq!(Mode::from_key(mode.key()), Some(mode));
        }
        // Codex modes read as their Claude counterparts, and the other way round.
        assert_eq!(Mode::ReadOnly.claude(), "default");
        assert_eq!(Mode::FullAccess.claude(), "bypassPermissions");
        assert_eq!(Mode::Yolo.for_codex(), Mode::FullAccess);
        assert_eq!(Mode::ReadOnly.codex(), ("on-request", "read-only"));
        assert_eq!(
            Mode::from_codex("", "", true, false),
            Mode::Auto,
            "Full auto"
        );
        assert_eq!(
            Mode::from_codex("untrusted", "workspace-write", false, false),
            Mode::Ask
        );
        assert_eq!(
            Mode::from_codex("on-request", "", false, true),
            Mode::FullAccess
        );
        assert_eq!(Mode::Ask.claude_profile(), "");
        assert_eq!(Mode::Yolo.claude_profile(), "bypassPermissions");
        assert_eq!(Mode::from_claude("dontAsk"), None);
    }

    #[test]
    fn settings_reject_unsafe_model_ids_and_unknown_fields() {
        assert!(ChatSettings::default().valid());
        for bad in ["", "--print", "a b", "x\ny"] {
            let settings = ChatSettings {
                model: Some(bad.into()),
                ..Default::default()
            };
            assert!(!settings.valid(), "{bad:?}");
        }
        assert!(serde_json::from_str::<ChatSettings>(r#"{"model":"opus","extra":1}"#).is_err());
        let with_id = ChatSettings {
            id: Some(ChatSettings::new_id()),
            ..Default::default()
        };
        assert!(with_id.valid());
        assert!(
            !ChatSettings {
                id: Some("../x".into()),
                ..Default::default()
            }
            .valid()
        );
        let restored: ChatSettings = serde_json::from_str(r#"{"effort":"XHigh"}"#).unwrap();
        assert_eq!(restored.effort, Some(Effort::XHigh));
        assert_eq!(restored.mode, Mode::Ask);
    }
}
