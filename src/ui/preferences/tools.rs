//! Tool/profile editor. Drafts are local; Save publishes one validated catalog.
mod agent_page;
mod profiles;
mod view;
use crate::{
    app_state::{AppState, ToolsState},
    ui::{components::*, theme as t},
};
use canopy_desktop::state::tools::{Profile, ToolDefinition, ToolKind, new_id};
use gpui_kit::{
    base::Disableable,
    component::{IconName, input::InputState, scroll::ScrollableElement},
    *,
};
pub(super) struct ToolsPreferences {
    state: Entity<ToolsState>,
    pub(super) agent_page: bool,
    agent_form: Entity<super::agent::AgentForm>,
    key_edits: std::collections::HashMap<String, String>,
    draft: ToolDefinition,
    name: Entity<InputState>,
    executable: Entity<InputState>,
    arguments: Entity<InputState>,
    profile: Option<String>,
    profile_name: Entity<InputState>,
    model: Entity<InputState>,
    profile_args: Entity<InputState>,
    initialized: bool,
    awaiting_save: bool,
    save_loading: ButtonLoading,
    refresh_loading: ButtonLoading,
    error: Option<String>,
    _observer: Subscription,
}
impl ToolsPreferences {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().tools.clone();
        let draft = state.read(cx).catalog.tools[0].clone();
        let observer = cx.observe_in(&state, window, |this, state, window, cx| {
            this.save_loading
                .set(this.awaiting_save && state.read(cx).saving, cx);
            this.refresh_loading.set(state.read(cx).discovering, cx);
            if state.read(cx).ready
                && (!this.initialized
                    || this.awaiting_save
                        && !state.read(cx).saving
                        && state.read(cx).error.is_none())
            {
                this.initialized = true;
                this.awaiting_save = false;
                let selected = state
                    .read(cx)
                    .catalog
                    .get(&this.draft.id)
                    .cloned()
                    .unwrap_or_else(|| state.read(cx).catalog.tools[0].clone());
                let selected_profile = this.profile.clone();
                this.load(selected, window, cx);
                if selected_profile
                    .as_ref()
                    .is_some_and(|id| this.draft.profiles.iter().any(|p| &p.id == id))
                {
                    this.profile = selected_profile;
                    this.load_profile(window, cx);
                }
            }
            let disabled = !state.read(cx).ready || state.read(cx).saving;
            this.agent_form
                .update(cx, |form, cx| form.set_disabled(disabled, cx));
            cx.notify();
        });
        let mut view = Self {
            agent_page: false,
            agent_form: cx.new(|cx| super::agent::AgentForm::new(window, cx)),
            key_edits: Default::default(),
            state,
            draft: draft.clone(),
            name: cx.new(|cx| InputState::new(window, cx).default_value(draft.name)),
            executable: cx
                .new(|cx| InputState::new(window, cx).placeholder("Program name or absolute path")),
            arguments: cx.new(|cx| InputState::new(window, cx).default_value("-l")),
            profile: None,
            profile_name: cx.new(|cx| InputState::new(window, cx)),
            model: cx.new(|cx| InputState::new(window, cx).placeholder("CLI default")),
            profile_args: cx
                .new(|cx| InputState::new(window, cx).placeholder("e.g. --permission-mode plan")),
            initialized: false,
            awaiting_save: false,
            save_loading: ButtonLoading::default(),
            refresh_loading: ButtonLoading::default(),
            error: None,
            _observer: observer,
        };
        if view.state.read(cx).ready {
            view.select_tool("shell", window, cx);
        }
        view
    }
    pub fn select_tool(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let tool = self.state.read(cx).catalog.get(id).cloned();
        if let Some(tool) = tool {
            self.initialized = true;
            self.load(tool, window, cx);
        }
    }
    fn load(&mut self, tool: ToolDefinition, window: &mut Window, cx: &mut Context<Self>) {
        self.name
            .update(cx, |s, cx| s.set_value(tool.name.clone(), window, cx));
        self.executable
            .update(cx, |s, cx| s.set_value(tool.executable.clone(), window, cx));
        self.arguments.update(cx, |s, cx| {
            s.set_value(
                canopy_desktop::state::tools::format_argument_list(&tool.arguments, cfg!(windows)),
                window,
                cx,
            )
        });
        self.profile = tool
            .default_profile
            .clone()
            .or_else(|| tool.profiles.first().map(|p| p.id.clone()));
        self.draft = tool;
        self.key_edits.clear();
        self.error = None;
        self.load_profile(window, cx);
        cx.notify();
    }
    fn load_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let p = self
            .profile
            .as_ref()
            .and_then(|id| self.draft.profiles.iter().find(|p| &p.id == id))
            .cloned();
        if let Some(agent) = canopy_desktop::state::agent_settings::Agent::from_id(&self.draft.id) {
            let edit = p.as_ref().and_then(|p| self.key_edits.get(&p.id));
            self.agent_form.update(cx, |form, cx| {
                form.load(agent, p.as_ref(), edit, window, cx)
            });
        }
        let (name, model, args) = p
            .map(|p| {
                (
                    p.name.clone(),
                    p.model.clone(),
                    canopy_desktop::state::tools::format_argument_list(&p.arguments, cfg!(windows)),
                )
            })
            .unwrap_or_default();
        self.profile_name
            .update(cx, |s, cx| s.set_value(name, window, cx));
        self.model
            .update(cx, |s, cx| s.set_value(model, window, cx));
        self.profile_args
            .update(cx, |s, cx| s.set_value(args, window, cx));
    }
    fn collect_profile(&mut self, cx: &App) -> Result<(), String> {
        if let Some(profile) = self
            .profile
            .as_ref()
            .and_then(|id| self.draft.profiles.iter_mut().find(|p| &p.id == id))
        {
            profile.name = self.profile_name.read(cx).value().trim().to_owned();
            if self.agent_page {
                let (model, settings, key) = self.agent_form.read(cx).collect(cx)?;
                profile.model = model;
                profile.settings = settings;
                if let Some(key) = key {
                    self.key_edits.insert(profile.id.clone(), key);
                }
            } else {
                profile.model = self.model.read(cx).value().trim().to_owned();
            }
            profile.arguments = canopy_desktop::state::tools::parse_argument_list(
                &self.profile_args.read(cx).value(),
                cfg!(windows),
            )
            .map_err(|_| "Unclosed quote in profile arguments.")?;
        }
        Ok(())
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            self.collect_profile(cx)?;
            let mut draft = self.draft.clone();
            draft.name = self.name.read(cx).value().trim().to_owned();
            draft.executable = self.executable.read(cx).value().trim().to_owned();
            draft.arguments = canopy_desktop::state::tools::parse_argument_list(
                &self.arguments.read(cx).value(),
                cfg!(windows),
            )
            .map_err(|_| "Unclosed quote in tool arguments.")?;
            let mut catalog = self.state.read(cx).catalog.clone();
            catalog.upsert(draft)?;
            self.state.update(cx, |state, cx| {
                state.save_with_keys(catalog, self.key_edits.clone(), cx)
            })
        })();
        match result {
            Ok(()) => {
                self.awaiting_save = true;
                self.save_loading.set(self.state.read(cx).saving, cx);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        cx.notify();
    }
}
