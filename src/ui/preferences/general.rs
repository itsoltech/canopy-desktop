use super::*;
use crate::app_state::SettingsState;
use canopy_desktop::settings::{Change, ToolId};
use gpui_kit::{
    base::Disableable,
    component::{
        scroll::ScrollableElement,
        select::{SelectEvent, SelectState},
    },
};

fn tool_options(
    values: &canopy_desktop::settings::Preferences,
    catalog: &canopy_desktop::state::tools::ToolCatalog,
) -> Vec<SelectOption> {
    let mut options: Vec<_> = catalog
        .tools
        .iter()
        .filter(|t| t.enabled)
        .map(|t| SelectOption {
            value: t.id.clone().into(),
            label: t.name.clone().into(),
        })
        .collect();
    for id in [&values.new_tab_tool, &values.new_worktree_tool] {
        if !options.iter().any(|o| o.value.as_ref() == id.as_str()) {
            options.push(SelectOption {
                value: id.as_str().to_owned().into(),
                label: format!("{} (unavailable)", id.as_str()).into(),
            });
        }
    }
    options
}

pub(super) struct PreferencesContent {
    settings: Entity<SettingsState>,
    _subscriptions: Vec<Subscription>,
    new_tab: Entity<SelectState<Vec<SelectOption>>>,
    new_worktree: Entity<SelectState<Vec<SelectOption>>>,
}
impl PreferencesContent {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.global::<AppState>().settings.clone();
        let values = settings.read(cx).values.clone();
        let options = tool_options(&values, &cx.global::<AppState>().tools.read(cx).catalog);
        let new_tab = cx.new(|cx| {
            SelectState::new(
                options.clone(),
                options
                    .iter()
                    .position(|v| v.value.as_ref() == values.new_tab_tool.as_str())
                    .map(gpui_kit::component::IndexPath::new),
                window,
                cx,
            )
        });
        let new_worktree = cx.new(|cx| {
            SelectState::new(
                options.clone(),
                options
                    .iter()
                    .position(|v| v.value.as_ref() == values.new_worktree_tool.as_str())
                    .map(gpui_kit::component::IndexPath::new),
                window,
                cx,
            )
        });
        let tools = cx.global::<AppState>().tools.clone();
        let subscriptions = vec![
            cx.observe_in(&tools, window, |this, _, window, cx| {
                this.sync_tool_options(window, cx)
            }),
            cx.observe_in(&settings, window, |this, _, window, cx| {
                this.sync_tool_options(window, cx)
            }),
            cx.subscribe(
                &new_tab,
                |this, _, event: &SelectEvent<Vec<SelectOption>>, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    this.settings.update(cx, |state, cx| {
                        state.change(
                            Change::NewTabTool(
                                ToolId::new(value.as_ref()).expect("validated tool ID"),
                            ),
                            cx,
                        )
                    });
                },
            ),
            cx.subscribe(
                &new_worktree,
                |this, _, event: &SelectEvent<Vec<SelectOption>>, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    this.settings.update(cx, |state, cx| {
                        state.change(
                            Change::NewWorktreeTool(
                                ToolId::new(value.as_ref()).expect("validated tool ID"),
                            ),
                            cx,
                        )
                    });
                },
            ),
        ];
        Self {
            settings,
            _subscriptions: subscriptions,
            new_tab,
            new_worktree,
        }
    }
    fn sync_tool_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let values = self.settings.read(cx).values.clone();
        let options = tool_options(&values, &cx.global::<AppState>().tools.read(cx).catalog);
        for (select, id) in [
            (&self.new_tab, &values.new_tab_tool),
            (&self.new_worktree, &values.new_worktree_tool),
        ] {
            select.update(cx, |state, cx| {
                state.set_items(options.clone(), window, cx);
                state.set_selected_value(&SharedString::from(id.as_str().to_owned()), window, cx);
            });
        }
        cx.notify();
    }
    fn general(&self, cx: &mut Context<Self>) -> Div {
        let state = self.settings.read(cx);
        let disabled = !state.ready || state.saving;
        column().flex_shrink_0().gap(px(SECTION_GAP))
            .children(state.error.clone().map(|error|div().text_color(t::red()).child(error)))
            .children((!state.ready&&state.error.is_none()).then(||div().child("Loading settings…")))
            .children((!state.warnings.is_empty()).then(||div().child("Some stored settings are invalid; defaults are shown.")))
            .child(section("STARTUP","What happens when Canopy launches",vec![
                setting("Reopen last workspace on startup","Restore the previous workspace tabs and layout when the app starts",true,
                    checkbox("reopen", state.values.reopen_last_workspace, "Reopen last workspace on startup")
                        .disabled(disabled).on_click(cx.listener(|this,value,_,cx|{this.settings.update(cx,|state,cx|state.change(Change::ReopenLastWorkspace(*value),cx));}))).into_any_element(),
                setting("Run setup wizard","Walk through the first-launch setup again to reconfigure tools and integrations",false,
                    button("rerun-wizard", "Re-run wizard")).into_any_element(),
            ]))
            .child(section("DEFAULTS","Tools used when opening new tabs and worktrees",vec![
                setting("New tab",format!("Default tool to open when creating a new tab ({})", super::super::platform_ui::shortcut("⌘T", "Ctrl+Shift+T")),true,
                    dropdown(&self.new_tab).disabled(disabled).w(px(160.))).into_any_element(),
                setting("New worktree","Default tool to open in new worktree tabs",false,
                    dropdown(&self.new_worktree).disabled(disabled).w(px(160.))).into_any_element(),
                setting("Shell","Resolved from $SHELL at launch — change with chsh, then restart.",false,
                    div().font_family(t::MONO).text_size(px(12.)).text_color(t::muted()).child("auto-detected")).into_any_element(),
            ]))
            .child(section("STATUS BAR","Information shown in the bottom status bar",vec![
                setting("Show CPU and RAM usage","Aggregates total CPU and resident memory across all Canopy processes (main, renderer, GPU, utility). Sampled once per second; the sampler stops entirely when this toggle is off.",true,
                    checkbox("performance", state.values.resource_usage, "Show CPU and RAM usage")
                        .disabled(disabled).on_click(cx.listener(|this,value,_,cx|{this.settings.update(cx,|state,cx|state.change(Change::ResourceUsage(*value),cx));}))).into_any_element(),
            ]))
    }
}
impl Render for PreferencesContent {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        column()
            .size_full()
            .child(
                column()
                    .px(px(CONTENT_X))
                    .pt(px(20.))
                    .pb(px(12.))
                    .gap(px(2.))
                    .border_b_1()
                    .border_color(t::border())
                    .child(
                        div()
                            .text_size(px(14.))
                            .line_height(px(16.1))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("General"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .line_height(px(14.85))
                            .text_color(t::muted())
                            .child("Startup behavior, default tools, and status bar"),
                    ),
            )
            .child(
                column()
                    .id("prefs-content")
                    .flex_1()
                    .overflow_y_scrollbar()
                    .px(px(CONTENT_X))
                    .py(px(20.))
                    .child(self.general(cx)),
            )
    }
}
