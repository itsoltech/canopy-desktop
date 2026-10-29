use super::ShortcutOverrides;
use super::{Error, Result};
use crate::appearance::{
    AppearanceMode, Background, BackgroundTarget, Backgrounds, ThemeFamily, ThemeVariant,
};
use crate::chat::config::{Effort, Mode, valid_model};
use serde::Serialize;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PreferenceKey {
    ReopenLastWorkspace,
    NotchEnabled,
    ResourceUsage,
    NewTabTool,
    NewWorktreeTool,
    /// Last chat choices, used as the defaults of a new chat.
    ChatProfile,
    ChatModel,
    ChatEffort,
    ChatMode,
    /// Device-local look: theme family, light/dark/auto and custom backgrounds.
    AppearanceTheme,
    AppearanceMode,
    Background(ThemeVariant, BackgroundTarget),
    /// Stored for the terminal font settings; not consumed by the terminal yet.
    TerminalFontFamily,
    TerminalFontSize,
    /// Device-local keyboard shortcut overrides (JSON, see `ShortcutOverrides`).
    Shortcuts,
}
impl PreferenceKey {
    pub const ALL: [Self; 18] = [
        Self::ReopenLastWorkspace,
        Self::NotchEnabled,
        Self::ResourceUsage,
        Self::NewTabTool,
        Self::NewWorktreeTool,
        Self::ChatProfile,
        Self::ChatModel,
        Self::ChatEffort,
        Self::ChatMode,
        Self::AppearanceTheme,
        Self::AppearanceMode,
        Self::Background(ThemeVariant::Dark, BackgroundTarget::Window),
        Self::Background(ThemeVariant::Dark, BackgroundTarget::Sidebar),
        Self::Background(ThemeVariant::Light, BackgroundTarget::Window),
        Self::Background(ThemeVariant::Light, BackgroundTarget::Sidebar),
        Self::TerminalFontFamily,
        Self::TerminalFontSize,
        Self::Shortcuts,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReopenLastWorkspace => "reopenLastWorkspace",
            Self::NotchEnabled => "notch.enabled",
            Self::ResourceUsage => "perf.hud.enabled",
            Self::NewTabTool => "newTab.toolId",
            Self::NewWorktreeTool => "newWorktree.toolId",
            Self::ChatProfile => "chat.profileId",
            Self::ChatModel => "chat.model",
            Self::ChatEffort => "chat.effort",
            Self::ChatMode => "chat.mode",
            Self::AppearanceTheme => "appearance.theme",
            Self::AppearanceMode => "appearance.mode",
            Self::Background(ThemeVariant::Dark, BackgroundTarget::Window) => {
                "appearance.background.dark.window"
            }
            Self::Background(ThemeVariant::Dark, BackgroundTarget::Sidebar) => {
                "appearance.background.dark.sidebar"
            }
            Self::Background(ThemeVariant::Light, BackgroundTarget::Window) => {
                "appearance.background.light.window"
            }
            Self::Background(ThemeVariant::Light, BackgroundTarget::Sidebar) => {
                "appearance.background.light.sidebar"
            }
            Self::TerminalFontFamily => "terminal.font_family",
            Self::TerminalFontSize => "terminal.font_size",
            Self::Shortcuts => "keyboard.shortcuts",
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
    pub chat: ChatDefaults,
    pub appearance: AppearancePreferences,
    pub terminal: TerminalPreferences,
    /// Device-local; not mirrored to remote clients.
    pub shortcuts: ShortcutOverrides,
}

/// The app's look on this device. Not mirrored to remote clients.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AppearancePreferences {
    pub theme: ThemeFamily,
    pub mode: AppearanceMode,
    pub backgrounds: Backgrounds,
}

pub const TERMINAL_FONT_SIZES: std::ops::RangeInclusive<u8> = 8..=24;
pub const DEFAULT_TERMINAL_FONT_FAMILY: &str = "JetBrains Mono";
pub const DEFAULT_TERMINAL_FONT_SIZE: u8 = 13;
/// Longest accepted font family list (a comma-separated fallback list is allowed).
const TERMINAL_FONT_FAMILY_LIMIT: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TerminalPreferences {
    pub font_family: String,
    /// Pixels, within `TERMINAL_FONT_SIZES`.
    pub font_size: u8,
}
impl Default for TerminalPreferences {
    fn default() -> Self {
        Self {
            font_family: DEFAULT_TERMINAL_FONT_FAMILY.into(),
            font_size: DEFAULT_TERMINAL_FONT_SIZE,
        }
    }
}
fn font_family(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= TERMINAL_FONT_FAMILY_LIMIT
        && !value.chars().any(char::is_control))
    .then(|| value.to_owned())
}
fn font_size(value: &str) -> Option<u8> {
    value
        .parse::<u8>()
        .ok()
        .filter(|size| TERMINAL_FONT_SIZES.contains(size))
}

/// What the last chat used; `None` falls back to the account profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ChatDefaults {
    pub profile: Option<String>,
    pub model: Option<String>,
    pub effort: Option<Effort>,
    pub mode: Option<Mode>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            reopen_last_workspace: true,
            // Defaults are never written, so a stored value is always the user's
            // choice (Electron's Notch page or a remote client) and is kept.
            notch_enabled: true,
            resource_usage: false,
            new_tab_tool: ToolId::default(),
            new_worktree_tool: ToolId::default(),
            chat: ChatDefaults::default(),
            appearance: AppearancePreferences::default(),
            terminal: TerminalPreferences::default(),
            shortcuts: ShortcutOverrides::default(),
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
    ChatProfile(String),
    ChatModel(String),
    ChatEffort(Effort),
    ChatMode(Mode),
    AppearanceTheme(ThemeFamily),
    AppearanceMode(AppearanceMode),
    Background(ThemeVariant, BackgroundTarget, Background),
    TerminalFontFamily(String),
    TerminalFontSize(u8),
    Shortcuts(ShortcutOverrides),
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
            PreferenceKey::ChatProfile => {
                if value.is_empty() || value.len() > 256 || value.contains('\0') {
                    return Err(Error::InvalidValue(key));
                }
                Self::ChatProfile(value.into())
            }
            PreferenceKey::ChatModel => {
                if !valid_model(value) {
                    return Err(Error::InvalidValue(key));
                }
                Self::ChatModel(value.into())
            }
            PreferenceKey::ChatEffort => {
                Self::ChatEffort(Effort::from_key(value).ok_or(Error::InvalidValue(key))?)
            }
            PreferenceKey::ChatMode => {
                Self::ChatMode(Mode::from_key(value).ok_or(Error::InvalidValue(key))?)
            }
            PreferenceKey::AppearanceTheme => {
                Self::AppearanceTheme(ThemeFamily::from_key(value).ok_or(Error::InvalidValue(key))?)
            }
            PreferenceKey::AppearanceMode => Self::AppearanceMode(
                AppearanceMode::from_key(value).ok_or(Error::InvalidValue(key))?,
            ),
            PreferenceKey::Background(variant, target) => Self::Background(
                variant,
                target,
                Background::decode(value).ok_or(Error::InvalidValue(key))?,
            ),
            PreferenceKey::TerminalFontFamily => {
                Self::TerminalFontFamily(font_family(value).ok_or(Error::InvalidValue(key))?)
            }
            PreferenceKey::TerminalFontSize => {
                Self::TerminalFontSize(font_size(value).ok_or(Error::InvalidValue(key))?)
            }
            PreferenceKey::Shortcuts => {
                Self::Shortcuts(ShortcutOverrides::decode(value).ok_or(Error::InvalidValue(key))?)
            }
        })
    }
    pub fn key(&self) -> PreferenceKey {
        self.encoded().0
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
            Self::ChatProfile(v) => (PreferenceKey::ChatProfile, Some(v.clone())),
            Self::ChatModel(v) => (PreferenceKey::ChatModel, Some(v.clone())),
            Self::ChatEffort(v) => (PreferenceKey::ChatEffort, Some(v.key().into())),
            Self::ChatMode(v) => (PreferenceKey::ChatMode, Some(v.key().into())),
            Self::AppearanceTheme(v) => (PreferenceKey::AppearanceTheme, Some(v.key().into())),
            Self::AppearanceMode(v) => (PreferenceKey::AppearanceMode, Some(v.key().into())),
            Self::Background(variant, target, v) => (
                PreferenceKey::Background(*variant, *target),
                Some(v.encode()),
            ),
            Self::TerminalFontFamily(v) => (
                PreferenceKey::TerminalFontFamily,
                Some(font_family(v).unwrap_or_else(|| DEFAULT_TERMINAL_FONT_FAMILY.into())),
            ),
            Self::TerminalFontSize(v) => (
                PreferenceKey::TerminalFontSize,
                Some(
                    v.clamp(TERMINAL_FONT_SIZES.start(), TERMINAL_FONT_SIZES.end())
                        .to_string(),
                ),
            ),
            // No overrides is the default: the row is removed, not stored as `{}`.
            Self::Shortcuts(v) => (
                PreferenceKey::Shortcuts,
                (!v.is_empty()).then(|| v.encode()),
            ),
            Self::Reset(key) => (*key, None),
        }
    }
}

/// Electron's app-wide theme and terminal font keys, read only as fallbacks for
/// the native keys and never written.
pub(super) const LEGACY_KEYS: [&str; 3] = ["theme", "fontFamily", "fontSize"];

pub(super) fn decode(
    schema: Schema,
    values: Vec<(PreferenceKey, Option<String>)>,
    legacy: Vec<(String, String)>,
) -> Snapshot {
    let mut preferences = Preferences::default();
    let mut warnings = Vec::new();
    // Electron applied its terminal theme to the whole app: carry it over until
    // the native keys are written. Native keys below take precedence.
    for (key, raw) in legacy {
        match key.as_str() {
            "theme" => {
                if let Some((family, variant)) = ThemeFamily::from_electron(&raw) {
                    preferences.appearance.theme = family;
                    preferences.appearance.mode = match variant {
                        ThemeVariant::Dark => AppearanceMode::Dark,
                        ThemeVariant::Light => AppearanceMode::Light,
                    };
                }
            }
            "fontFamily" => {
                if let Some(family) = font_family(&raw) {
                    preferences.terminal.font_family = family;
                }
            }
            "fontSize" => {
                if let Some(size) = font_size(raw.trim()) {
                    preferences.terminal.font_size = size;
                }
            }
            _ => {}
        }
    }
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
            Ok(Change::ChatProfile(v)) => preferences.chat.profile = Some(v),
            Ok(Change::ChatModel(v)) => preferences.chat.model = Some(v),
            Ok(Change::ChatEffort(v)) => preferences.chat.effort = Some(v),
            Ok(Change::ChatMode(v)) => preferences.chat.mode = Some(v),
            Ok(Change::AppearanceTheme(v)) => preferences.appearance.theme = v,
            Ok(Change::AppearanceMode(v)) => preferences.appearance.mode = v,
            Ok(Change::Background(variant, target, v)) => {
                *preferences.appearance.backgrounds.get_mut(variant, target) = v
            }
            Ok(Change::TerminalFontFamily(v)) => preferences.terminal.font_family = v,
            Ok(Change::TerminalFontSize(v)) => preferences.terminal.font_size = v,
            Ok(Change::Shortcuts(v)) => preferences.shortcuts = v,
            _ => warnings.push(DecodeWarning { key }),
        }
    }
    Snapshot {
        preferences,
        warnings,
        schema,
    }
}
