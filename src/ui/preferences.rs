mod agent;
mod general;
mod integrations;
mod jira;
mod navigation;
mod remote;
mod task_filters;
mod tools;
mod youtrack;
use super::{components::*, theme as t};
use crate::app_state::AppState;
use canopy_desktop::motion::{self, Presence};
use general::PreferencesContent;
use gpui_kit::component::{IconName, Root, Sizable, input::InputState};
use gpui_kit::*;
use navigation::PreferencesNavigation;
use std::time::Instant;

actions!(preferences, [FocusSearch]);
use tools::ToolsPreferences;

// Direct mapping of PrefsHeader / PrefsSidebar / PrefsRow / PrefsSection in Electron.
const SIDEBAR: f32 = 208.;
const HEADER: f32 = 48.;
const CONTENT_X: f32 = 28.;
const SECTION_GAP: f32 = 28.;
/// Hint on Settings entries that are shown but not wired to the app yet.
const NOT_AVAILABLE: &str = "Not available yet";

/// A row the app does not consume yet: kept visible, greyed out and inert.
/// Its control must also be disabled by the caller.
fn unavailable(id: &'static str, row: Div) -> Stateful<Div> {
    row.id(id).text_color(t::faint()).tooltip(|window, cx| {
        gpui_kit::component::tooltip::Tooltip::new(NOT_AVAILABLE).build(window, cx)
    })
}
/// Page for a navigation entry without working settings.
fn unavailable_page(page: String) -> Div {
    column().size_full().child(
        column()
            .flex_shrink_0()
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
                    .text_color(t::muted())
                    .child(page),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .line_height(px(14.85))
                    .text_color(t::muted())
                    .child(NOT_AVAILABLE),
            ),
    )
}

pub fn open(cx: &mut App) -> Result<WindowHandle<Root>> {
    open_page("General", cx)
}
pub fn open_task_filters(cx: &mut App) -> Result<WindowHandle<Root>> {
    open_page("Task filters", cx)
}
pub fn open_integrations(cx: &mut App) -> Result<WindowHandle<Root>> {
    open_page("Integrations", cx)
}
fn open_page(initial: &'static str, cx: &mut App) -> Result<WindowHandle<Root>> {
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(920.), px(720.)),
                cx,
            ))),
            window_min_size: Some(size(px(720.), px(540.))),
            titlebar: Some(titlebar_options("Canopy — Preferences")),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| Preferences::new(initial, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
}
impl Render for Preferences {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        motion::request_frame(window, self.reveal.is_animating(now));
        let page = self.page.read(cx).clone();
        let content = if navigation::unavailable(&page) {
            unavailable_page(page.clone()).into_any_element()
        } else if page == "General" {
            self.content.clone().into_any_element()
        } else if page == "Task filters" {
            self.task_filters.clone().into_any_element()
        } else if page == "Integrations" {
            self.integrations.clone().into_any_element()
        } else if page == "Connections" {
            self.remote.clone().into_any_element()
        } else {
            self.tools_content.clone().into_any_element()
        };
        column()
            .key_context("Preferences")
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search
                    .update(cx, |search, cx| search.focus(window, cx));
            }))
            .relative()
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .text_size(px(13.))
            .child(
                window_titlebar("preferences-titlebar", window)
                    .h(px(40.))
                    .flex_shrink_0()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("Canopy — Preferences"),
            )
            .child(
                row()
                    .h(px(HEADER))
                    .flex_shrink_0()
                    .px(px(16.))
                    .gap(px(16.))
                    .border_b_1()
                    .border_color(t::border())
                    .child(
                        row()
                            .w(px(176.))
                            .flex_shrink_0()
                            .gap(px(8.))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("Settings"))
                            .child(div().text_color(t::faint()).child("/"))
                            .child(div().text_color(t::secondary()).child(page)),
                    )
                    .child(
                        // Search does not filter settings yet; the field stays visible but inert.
                        row().flex_1().justify_center().child(
                            row()
                                .id("prefs-search")
                                .tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(NOT_AVAILABLE)
                                        .build(window, cx)
                                })
                                .w_full()
                                .max_w(px(384.))
                                .h(px(28.))
                                .rounded(px(4.))
                                .px(px(8.))
                                .bg(t::input_bg())
                                .child(icon(IconName::Search).size(px(13.)))
                                .child(
                                    input(&self.search)
                                        .appearance(false)
                                        .bordered(false)
                                        .disabled(true)
                                        .small(),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(t::faint())
                                        .child(super::platform_ui::shortcut("⌘K", "Ctrl+K")),
                                ),
                        ),
                    )
                    .child(
                        icon_button("close-preferences", IconName::Close, "Close settings")
                            .size(px(28.))
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(
                        div().w(px(SIDEBAR)).flex_shrink_0().h_full().child(
                            self.navigation
                                .clone()
                                .cached(div().size_full().style().clone()),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .opacity(self.reveal.progress(now))
                            .child(content),
                    ),
            )
    }
}

// Definite view bounds let GPUI retain the sibling's layout and paint output.
// Descendant control notifications, resizing and inherited text-style changes
// invalidate the corresponding cache automatically.
struct Preferences {
    search: Entity<InputState>,
    navigation: Entity<PreferencesNavigation>,
    content: Entity<PreferencesContent>,
    tools_content: Entity<ToolsPreferences>,
    integrations: Entity<integrations::IntegrationsPreferences>,
    task_filters: Entity<task_filters::TaskFiltersPreferences>,
    remote: Entity<remote::RemotePreferences>,
    page: Entity<String>,
    reveal: Presence,
    _route: Subscription,
}
impl Preferences {
    fn new(initial: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.bind_keys([KeyBinding::new(
            "secondary-k",
            FocusSearch,
            Some("Preferences"),
        )]);
        let page = cx.new(|_| initial.to_owned());
        let tools_content = cx.new(|cx| ToolsPreferences::new(window, cx));
        let route = cx.observe_in(&page, window, |this, page, window, cx| {
            let id = match page.read(cx).as_str() {
                "Claude" => Some("claude"),
                "Codex" => Some("codex"),
                "Gemini" => Some("gemini"),
                "OpenCode" => Some("opencode"),
                "Terminal" => Some("shell"),
                _ => None,
            };
            this.tools_content.update(cx, |view, cx| {
                view.agent_page = id.is_some_and(|id| id != "shell");
                cx.notify();
            });
            if let Some(id) = id {
                this.tools_content
                    .update(cx, |view, cx| view.select_tool(id, window, cx));
            }
            let now = Instant::now();
            this.reveal = Presence::new(false, motion::presets::CONTENT_REVEAL, now);
            this.reveal.set_open(true, now, motion::policy(cx));
            cx.notify();
        });
        Self {
            navigation: cx.new(|cx| PreferencesNavigation::new(page.clone(), cx)),
            page,
            tools_content,
            task_filters: cx.new(|cx| task_filters::TaskFiltersPreferences::new(window, cx)),
            remote: cx.new(|cx| remote::RemotePreferences::new(window, cx)),
            integrations: cx.new(|cx| integrations::IntegrationsPreferences::new(window, cx)),
            _route: route,
            reveal: Presence::new(true, motion::presets::CONTENT_REVEAL, Instant::now()),
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search settings…")),
            content: cx.new(|cx| PreferencesContent::new(window, cx)),
        }
    }
}
