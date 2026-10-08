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
}
impl Mode {
    pub const ALL: [Self; 5] = [
        Self::Ask,
        Self::AcceptEdits,
        Self::Plan,
        Self::Auto,
        Self::Yolo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::AcceptEdits => "Accept edits",
            Self::Plan => "Plan",
            Self::Auto => "Auto",
            Self::Yolo => "Yolo",
        }
    }
    /// Claude permission mode for `set_permission_mode`.
    pub fn claude(self) -> &'static str {
        match self {
            Self::Ask => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::Auto => "auto",
            Self::Yolo => "bypassPermissions",
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
}
impl Effort {
    pub const ALL: [Self; 5] = [Self::Low, Self::Medium, Self::High, Self::XHigh, Self::Max];

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::XHigh => "Extra high",
            Self::Max => "Max",
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
}
impl ChatSettings {
    pub fn valid(&self) -> bool {
        self.model.as_deref().is_none_or(valid_model)
    }
}

/// Model ids come from the provider CLI and are passed back as one argv value.
pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.starts_with('-')
        && model.chars().all(|c| !c.is_control() && !c.is_whitespace())
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
        for mode in Mode::ALL {
            assert_eq!(Mode::from_claude(mode.claude()), Some(mode));
            assert_eq!(Mode::from_key(mode.key()), Some(mode));
        }
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
        let restored: ChatSettings = serde_json::from_str(r#"{"effort":"XHigh"}"#).unwrap();
        assert_eq!(restored.effort, Some(Effort::XHigh));
        assert_eq!(restored.mode, Mode::Ask);
    }
}
