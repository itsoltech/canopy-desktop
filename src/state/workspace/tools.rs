//! The worktree's tool presentation; pane trees and process identities stay in Workspace.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolPosition {
    #[default]
    Right,
    Bottom,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolPage {
    #[default]
    Tabs,
    Files,
    Changes,
    Tasks,
    Agent,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolPanel {
    open: Option<bool>,
    position: ToolPosition,
    page: ToolPage,
    width: f32,
    height: f32,
    maximized: bool,
    focused: bool,
}
impl Default for ToolPanel {
    fn default() -> Self {
        Self {
            open: None,
            position: ToolPosition::Right,
            page: ToolPage::Tabs,
            width: 400.,
            height: 280.,
            maximized: false,
            focused: false,
        }
    }
}
impl ToolPanel {
    pub fn position(&self) -> ToolPosition {
        self.position
    }
    pub fn page(&self) -> ToolPage {
        self.page
    }
    pub fn width(&self) -> f32 {
        self.width
    }
    pub fn height(&self) -> f32 {
        self.height
    }
    pub fn is_maximized(&self) -> bool {
        self.maximized && self.open != Some(false)
    }
    pub fn is_focused(&self) -> bool {
        self.focused
    }
    pub fn set_position(&mut self, value: ToolPosition) {
        self.position = value;
    }
    pub fn set_maximized(&mut self, value: bool) {
        self.maximized = value;
    }
    pub fn show(&mut self, page: ToolPage) {
        self.open = Some(true);
        self.page = page;
        self.focused = true;
    }
    pub fn resize(&mut self, value: f32) {
        if !value.is_finite() {
            return;
        }
        match self.position {
            ToolPosition::Right => self.width = value.clamp(180., 1600.),
            ToolPosition::Bottom => self.height = value.clamp(96., 1600.),
        }
    }
    pub(super) fn focus_conversation(&mut self) {
        self.focused = false;
        self.maximized = false;
    }
    pub(super) fn valid(&self) -> bool {
        self.width.is_finite()
            && (180. ..=1600.).contains(&self.width)
            && self.height.is_finite()
            && (96. ..=1600.).contains(&self.height)
    }
}
impl Workspace {
    pub fn primary_tab(&self) -> Option<&Tab> {
        self.active()
            .filter(|t| t.has_chat())
            .or_else(|| {
                self.tabs
                    .iter()
                    .find(|t| Some(t.id) == self.last_chat && t.has_chat())
            })
            .or_else(|| self.tabs.iter().rev().find(|t| t.has_chat()))
    }
    pub fn tool_tab(&self) -> Option<&Tab> {
        self.active()
            .filter(|t| !t.has_chat())
            .or_else(|| {
                self.tabs
                    .iter()
                    .find(|t| Some(t.id) == self.last_tool && !t.has_chat())
            })
            .or_else(|| self.tabs.iter().rev().find(|t| !t.has_chat()))
    }
    pub fn tool_tabs(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter().filter(|t| !t.has_chat())
    }
    pub fn tool_panel(&self) -> &ToolPanel {
        &self.panel
    }
    pub fn tool_panel_mut(&mut self) -> &mut ToolPanel {
        &mut self.panel
    }
    pub fn tools_open(&self) -> bool {
        self.panel.open.unwrap_or_else(|| self.tool_tab().is_some())
    }
    pub fn show_tools(&mut self, page: ToolPage) {
        if page == ToolPage::Tabs
            && let Some(id) = self.tool_tab().map(|tab| tab.id)
        {
            self.select(id);
        } else {
            self.panel.show(page);
        }
    }
    pub fn hide_tools(&mut self) {
        self.remember_active();
        self.panel.open = Some(false);
        self.panel.maximized = false;
        self.panel.focused = false;
        if let Some(tab) = self.primary_tab() {
            self.active = Some(tab.id);
        }
    }
    pub(super) fn remember_active(&mut self) {
        if let Some(tab) = self.active() {
            let (id, chat) = (tab.id, tab.has_chat());
            if chat {
                self.last_chat = Some(id);
            } else {
                self.last_tool = Some(id);
            }
        }
    }
    pub(super) fn select(&mut self, id: TabId) {
        self.remember_active();
        self.active = Some(id);
        if self.active().is_some_and(|t| !t.has_chat()) {
            self.panel.show(ToolPage::Tabs);
        } else {
            self.panel.focus_conversation();
        }
    }
}
