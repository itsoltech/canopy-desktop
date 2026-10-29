mod environment;
mod options;
mod state;
mod view;
use super::search::{Entry, SettingsSearch};
use super::*;
use crate::ui::components::*;
use canopy_desktop::state::{
    agent_settings::{Agent, AgentSettings},
    tools::Profile,
};
use environment::EnvironmentEditor;
use gpui_kit::{
    base::Disableable,
    component::{input::TextareaState, select::SelectState},
};
use std::collections::HashMap;
const PROFILES: Entry = Entry::new(
    "profiles",
    "PROFILES",
    "Profiles",
    "Each profile keeps its own model, keys and settings",
    "new add delete",
);
const PROFILE_NAME: Entry = Entry::new(
    "profile-name",
    "PROFILES",
    "Profile name",
    "Make default picks the profile for new sessions",
    "default rename",
);
const MODEL: Entry = Entry::new(
    "model",
    "MODEL & BEHAVIOR",
    "Model",
    "Model passed to the CLI",
    "",
);
const PERMISSION: Entry = Entry::new(
    "permission",
    "MODEL & BEHAVIOR",
    "Permission mode",
    "Controls what Claude can do without asking. Plan = read-only, Auto = full autonomy.",
    "accept edits bypass permissions",
);
const EFFORT: Entry = Entry::new(
    "effort",
    "MODEL & BEHAVIOR",
    "Effort level",
    "Higher effort means more thorough but slower responses",
    "low medium high extra max thinking reasoning",
);
const CODEX_APPROVAL: Entry = Entry::new(
    "approval",
    "MODEL & BEHAVIOR",
    "Approval mode",
    "Controls when Codex pauses for human approval",
    "on request never",
);
const GEMINI_APPROVAL: Entry = Entry::new(
    "approval",
    "MODEL & BEHAVIOR",
    "Approval mode",
    "Controls what Gemini can do without asking. YOLO = full autonomy, Plan = read-only.",
    "prompt auto edit",
);
const SANDBOX: Entry = Entry::new(
    "sandbox",
    "MODEL & BEHAVIOR",
    "Sandbox",
    "Command execution policy",
    "read only workspace write full access",
);
const FULL_AUTO: Entry = Entry::new(
    "full-auto",
    "MODEL & BEHAVIOR",
    "Full auto",
    "Workspace-write sandbox + on-request approvals",
    "",
);
const BYPASS: Entry = Entry::new(
    "bypass",
    "MODEL & BEHAVIOR",
    "Bypass approvals and sandbox",
    "Runs Codex without approval prompts or sandbox restrictions",
    "yolo dangerous",
);
const CONFIG_PROFILE: Entry = Entry::new(
    "config_profile",
    "MODEL & BEHAVIOR",
    "Profile",
    "Named CLI configuration profile",
    "config",
);
const API_KEY: Entry = Entry::new(
    "api_key",
    "API",
    "API key",
    "The provider's API key, kept in the system credential store",
    "token secret credentials environment keychain",
);
const BASE_URL: Entry = Entry::new(
    "base_url",
    "API",
    "Base URL",
    "Custom API endpoint or compatible proxy",
    "endpoint proxy",
);
const PROVIDER: Entry = Entry::new(
    "provider",
    "API & PROVIDER",
    "Provider",
    "Cloud provider for the Claude API backend",
    "aws bedrock google vertex microsoft foundry",
);
const PROMPT: Entry = Entry::new(
    "prompt",
    "SYSTEM PROMPT",
    "Append to system prompt",
    "Extra instructions added after the default system prompt in every session",
    "instructions",
);
const ENVIRONMENT: Entry = Entry::new(
    "env",
    "ENVIRONMENT VARIABLES",
    "Environment variables",
    "Variables set for this profile's sessions",
    "env",
);
const SETTINGS_JSON: Entry = Entry::new(
    "json",
    "ADVANCED",
    "Settings JSON override",
    "Settings passed to the agent for each session",
    "hooks config",
);
const CONFIG_JSON: Entry = Entry::new(
    "json",
    "ADVANCED",
    "Config JSON override",
    "Passed via OPENCODE_CONFIG_CONTENT at session start",
    "settings",
);
/// Search entries of an agent's page, in page order.
pub(super) fn search(agent: Agent) -> Vec<Entry> {
    let mut entries = vec![PROFILES, PROFILE_NAME, MODEL];
    entries.extend_from_slice(match agent {
        Agent::Claude => &[PERMISSION, EFFORT][..],
        Agent::Codex => &[CODEX_APPROVAL, SANDBOX, FULL_AUTO, BYPASS, CONFIG_PROFILE],
        Agent::Gemini => &[GEMINI_APPROVAL],
        Agent::OpenCode => &[],
    });
    entries.push(API_KEY);
    if matches!(agent, Agent::Claude | Agent::Codex) {
        entries.push(BASE_URL);
    }
    if agent == Agent::Claude {
        entries.extend([PROVIDER, PROMPT]);
    }
    entries.push(ENVIRONMENT);
    entries.push(if agent == Agent::OpenCode {
        CONFIG_JSON
    } else {
        SETTINGS_JSON
    });
    entries
}
/// Entries of the profile list, which the tools page draws around the form.
pub(super) const PROFILE_ENTRIES: [Entry; 2] = [PROFILES, PROFILE_NAME];

pub(super) struct AgentForm {
    agent: Agent,
    search: Entity<SettingsSearch>,
    scroll: ScrollHandle,
    _search: Subscription,
    reveal: Presence,
    texts: HashMap<&'static str, Entity<InputState>>,
    selects: HashMap<&'static str, Entity<SelectState<Vec<SelectOption>>>>,
    prompt: Entity<TextareaState>,
    json: Entity<TextareaState>,
    env: Entity<EnvironmentEditor>,
    full_auto: bool,
    bypass: bool,
    clear_key: bool,
    clear_prompt: bool,
    clear_json: bool,
    has_key: bool,
    disabled: bool,
    original: AgentSettings,
}
