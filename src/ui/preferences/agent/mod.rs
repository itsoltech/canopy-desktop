mod environment;
mod options;
mod state;
mod view;
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
pub(super) struct AgentForm {
    agent: Agent,
    reveal: Presence,
    texts: HashMap<&'static str, Entity<InputState>>,
    selects: HashMap<&'static str, Entity<SelectState<Vec<SelectOption>>>>,
    prompt: Entity<TextareaState>,
    json: Entity<TextareaState>,
    env: Entity<EnvironmentEditor>,
    full_auto: bool,
    bypass: bool,
    clear_key: bool,
    has_key: bool,
    disabled: bool,
    original: AgentSettings,
}
