use super::search::{Entry, SettingsSearch};
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

const REOPEN: Entry = Entry::new(
    "reopen",
    "STARTUP",
    "Reopen last workspace on startup",
    "Restore the previous workspace tabs and layout when the app starts",
    "restore session launch",
);
const WIZARD: Entry = Entry::new(
    "wizard",
    "STARTUP",
    "Run setup wizard",
    "Walk through the first-launch setup again to reconfigure tools and integrations.",
    "onboarding first launch",
);
const NEW_TAB: Entry = Entry::new(
    "new-tab",
    "DEFAULTS",
    "New tab",
    "Default tool to open when creating a new tab",
    "tool default",
);
const NEW_WORKTREE: Entry = Entry::new(
    "new-worktree",
    "DEFAULTS",
    "New worktree",
    "Default tool to open in new worktree tabs.",
    "tool default",
);
const SHELL: Entry = Entry::new(
    "shell",
    "DEFAULTS",
    "Shell",
    "Resolved from $SHELL at launch — change with chsh, then restart.",
    "zsh bash fish login terminal",
);
const RESOURCES: Entry = Entry::new(
    "resources",
    "RESOURCE USAGE",
    "Show CPU and RAM usage",
    "Total CPU and memory of Canopy and the agents and terminals it started. Sampled every second while shown.",
    "performance memory processor sidebar",
);
/// Search entries, in page order.
pub(super) const SEARCH: &[Entry] = &[REOPEN, WIZARD, NEW_TAB, NEW_WORKTREE, SHELL, RESOURCES];

pub(super) struct PreferencesContent {
    settings: Entity<SettingsState>,
    search: Entity<SettingsSearch>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
    new_tab: Entity<SelectState<Vec<SelectOption>>>,
    new_worktree: Entity<SelectState<Vec<SelectOption>>>,
}
impl PreferencesContent {
    pub(super) fn new(
        search: Entity<SettingsSearch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
            cx.observe(&search, |_, _, cx| cx.notify()),
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
            search,
            scroll: ScrollHandle::new(),
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
        let search = self.search.read(cx).page("General").scroll(&self.scroll);
        column()
            .flex_shrink_0()
            .gap(px(SECTION_GAP))
            .children(
                state
                    .error
                    .clone()
                    .map(|error| div().text_color(t::red()).child(error)),
            )
            .children(
                (!state.ready && state.error.is_none()).then(|| div().child("Loading settings…")),
            )
            .children(
                (!state.warnings.is_empty())
                    .then(|| div().child("Some stored settings are invalid; defaults are shown.")),
            )
            .child(section(
                "STARTUP",
                "What happens when Canopy launches",
                vec![
                    search
                        .setting(
                            &REOPEN,
                            true,
                            checkbox("reopen", state.values.reopen_last_workspace, REOPEN.label)
                                .disabled(disabled)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.settings.update(cx, |state, cx| {
                                        state.change(Change::ReopenLastWorkspace(*value), cx)
                                    });
                                })),
                        )
                        .into_any_element(),
                    // No setup wizard exists in this build; the button has no action.
                    unavailable(
                        "setting-rerun-wizard",
                        search.setting_with(
                            &WIZARD,
                            format!("{} {NOT_AVAILABLE}.", WIZARD.help),
                            false,
                            button("rerun-wizard", "Re-run wizard").disabled(true),
                        ),
                    )
                    .into_any_element(),
                ],
            ))
            .child(section(
                "DEFAULTS",
                "Tools used when opening new tabs and worktrees",
                vec![
                    search
                        .setting_with(
                            &NEW_TAB,
                            super::super::keymap::with_shortcut(
                                NEW_TAB.help,
                                super::super::keymap::CommandId::NewTab,
                                cx,
                            ),
                            true,
                            dropdown(&self.new_tab).disabled(disabled).w(px(160.)),
                        )
                        .into_any_element(),
                    // Stored as newWorktree.toolId, but worktree creation picks its agent in the dialog.
                    unavailable(
                        "setting-new-worktree",
                        search.setting_with(
                            &NEW_WORKTREE,
                            format!("{} {NOT_AVAILABLE}.", NEW_WORKTREE.help),
                            false,
                            dropdown(&self.new_worktree).disabled(true).w(px(160.)),
                        ),
                    )
                    .into_any_element(),
                    search
                        .setting(
                            &SHELL,
                            false,
                            div()
                                .font_family(t::MONO)
                                .text_size(px(12.))
                                .text_color(t::muted())
                                .child("auto-detected"),
                        )
                        .into_any_element(),
                ],
            ))
            .child(section(
                "RESOURCE USAGE",
                "Shown at the bottom of the sidebar",
                vec![
                    search
                        .setting(
                            &RESOURCES,
                            true,
                            checkbox("performance", state.values.resource_usage, RESOURCES.label)
                                .disabled(disabled)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.settings.update(cx, |state, cx| {
                                        state.change(Change::ResourceUsage(*value), cx)
                                    });
                                })),
                        )
                        .into_any_element(),
                ],
            ))
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
                            .child("Startup behavior, default tools, and resource usage"),
                    ),
            )
            .child(
                column()
                    .id("prefs-content")
                    .flex_1()
                    .track_scroll(&self.scroll)
                    .overflow_y_scrollbar()
                    .px(px(CONTENT_X))
                    .py(px(20.))
                    .child(self.general(cx)),
            )
    }
}
