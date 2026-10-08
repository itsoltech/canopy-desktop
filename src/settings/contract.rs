use super::{Error, Result};
use serde::Serialize;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PreferenceKey {
    ReopenLastWorkspace,
    NotchEnabled,
    ResourceUsage,
    NewTabTool,
    NewWorktreeTool,
}
impl PreferenceKey {
    pub const ALL: [Self; 5] = [
        Self::ReopenLastWorkspace,
        Self::NotchEnabled,
        Self::ResourceUsage,
        Self::NewTabTool,
        Self::NewWorktreeTool,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReopenLastWorkspace => "reopenLastWorkspace",
            Self::NotchEnabled => "notch.enabled",
            Self::ResourceUsage => "perf.hud.enabled",
            Self::NewTabTool => "newTab.toolId",
            Self::NewWorktreeTool => "newWorktree.toolId",
        }
    }
}
impl FromStr for PreferenceKey {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|key| key.as_str() == value)
            .ok_or(Error::UnsupportedKey)
    }
}

/// Opaque ID, including custom Electron tools; availability is a separate concern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ToolId(String);
impl ToolId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(Error::InvalidToolId);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Default for ToolId {
    fn default() -> Self {
        Self("shell".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Preferences {
    pub reopen_last_workspace: bool,
    pub notch_enabled: bool,
    pub resource_usage: bool,
    pub new_tab_tool: ToolId,
    pub new_worktree_tool: ToolId,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            reopen_last_workspace: true,
            notch_enabled: false,
            resource_usage: false,
            new_tab_tool: ToolId::default(),
            new_worktree_tool: ToolId::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DecodeWarning {
    pub key: PreferenceKey,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Schema {
    RustPreferencesV1,
    Electron { migration: i64 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub preferences: Preferences,
    pub warnings: Vec<DecodeWarning>,
    pub schema: Schema,
}

#[derive(Clone, Debug)]
pub enum Change {
    ReopenLastWorkspace(bool),
    NotchEnabled(bool),
    ResourceUsage(bool),
    NewTabTool(ToolId),
    NewWorktreeTool(ToolId),
    Reset(PreferenceKey),
}
impl Change {
    pub fn parse(key: PreferenceKey, value: &str) -> Result<Self> {
        let boolean = || match value {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(Error::InvalidValue(key)),
        };
        Ok(match key {
            PreferenceKey::ReopenLastWorkspace => Self::ReopenLastWorkspace(boolean()?),
            PreferenceKey::NotchEnabled => Self::NotchEnabled(boolean()?),
            PreferenceKey::ResourceUsage => Self::ResourceUsage(boolean()?),
            PreferenceKey::NewTabTool => Self::NewTabTool(ToolId::new(value)?),
            PreferenceKey::NewWorktreeTool => Self::NewWorktreeTool(ToolId::new(value)?),
        })
    }
    pub(super) fn encoded(&self) -> (PreferenceKey, Option<String>) {
        match self {
            Self::ReopenLastWorkspace(v) => {
                (PreferenceKey::ReopenLastWorkspace, Some(v.to_string()))
            }
            Self::NotchEnabled(v) => (PreferenceKey::NotchEnabled, Some(v.to_string())),
            Self::ResourceUsage(v) => (PreferenceKey::ResourceUsage, Some(v.to_string())),
            Self::NewTabTool(v) => (PreferenceKey::NewTabTool, Some(v.0.clone())),
            Self::NewWorktreeTool(v) => (PreferenceKey::NewWorktreeTool, Some(v.0.clone())),
            Self::Reset(key) => (*key, None),
        }
    }
}

pub(super) fn decode(schema: Schema, values: Vec<(PreferenceKey, Option<String>)>) -> Snapshot {
    let mut preferences = Preferences::default();
    let mut warnings = Vec::new();
    for (key, raw) in values {
        let Some(raw) = raw else {
            continue;
        };
        match Change::parse(key, &raw) {
            Ok(Change::ReopenLastWorkspace(v)) => preferences.reopen_last_workspace = v,
            Ok(Change::NotchEnabled(v)) => preferences.notch_enabled = v,
            Ok(Change::ResourceUsage(v)) => preferences.resource_usage = v,
            Ok(Change::NewTabTool(v)) => preferences.new_tab_tool = v,
            Ok(Change::NewWorktreeTool(v)) => preferences.new_worktree_tool = v,
            _ => warnings.push(DecodeWarning { key }),
        }
    }
    Snapshot {
        preferences,
        warnings,
        schema,
    }
}
