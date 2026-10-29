mod agent;
mod appearance;
mod general;
mod integrations;
mod jira;
mod navigation;
mod notch;
mod remote;
mod search;
mod shortcuts;
mod task_filters;
mod tools;
mod youtrack;
use super::{components::*, theme as t};
use crate::app_state::AppState;
use canopy_desktop::motion::{self, Presence};
use general::PreferencesContent;
use gpui_kit::component::{
    IconName, Root, Sizable,
    input::{Escape, InputEvent, InputState, MoveDown, MoveUp},
};
use gpui_kit::*;
use navigation::PreferencesNavigation;
use search::SettingsSearch;
use std::time::Instant;

actions!(preferences, [FocusSearch]);
use tools::ToolsPreferences;

// Direct mapping of PrefsHeader / PrefsSidebar / PrefsRow / PrefsSection in Electron.
const SIDEBAR: f32 = 208.;
/// One top row: window controls, then the search field centered in the window.
const TITLEBAR: f32 = 40.;
const SEARCH_WIDTH: f32 = 384.;
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
        let search = self.search.read(cx);
        let nothing_found = search.nothing_found().then(|| search.text().clone());
        let content = if let Some(text) = nothing_found {
            self.nothing_found(text, cx).into_any_element()
        } else if navigation::unavailable(&page) {
            unavailable_page(page.clone()).into_any_element()
        } else if page == "General" {
            self.content.clone().into_any_element()
        } else if page == "Appearance" {
            self.appearance.clone().into_any_element()
        } else if page == "Task filters" {
            self.task_filters.clone().into_any_element()
        } else if page == "Integrations" {
            self.integrations.clone().into_any_element()
        } else if page == "Notch" {
            self.notch.clone().into_any_element()
        } else if page == "Shortcuts" {
            self.shortcuts.clone().into_any_element()
        } else if page == "Connections" {
            self.remote.clone().into_any_element()
        } else {
            self.tools_content.clone().into_any_element()
        };
        column()
            .key_context("Preferences")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search_input.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
            }))
            .relative()
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .text_size(px(13.))
            .child(self.titlebar(window, cx))
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

/// Empty space of the titlebar: drags the window; a double click zooms it on macOS.
fn drag_area() -> Div {
    div()
        .h_full()
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            #[cfg(target_os = "macos")]
            if event.click_count == 2 {
                window.titlebar_double_click();
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (event, window);
        })
}

impl Preferences {
    /// The only top row. The search field sits between two drag areas, which
    /// stay clear of it (a drag area under the field would take its clicks):
    /// equal flexible areas center it in the window, the leading one keeps it
    /// off the traffic lights, and on Windows a fixed area mirrors the caption
    /// buttons' lane.
    fn titlebar(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let trailing = titlebar_trailing_inset();
        workspace_titlebar("preferences-titlebar", window)
            .h(px(TITLEBAR))
            .flex_shrink_0()
            .pr(px(trailing))
            .border_b_1()
            .border_color(t::border())
            .children((trailing > 0.).then(|| drag_area().w(px(trailing)).flex_shrink_0()))
            .child(
                drag_area()
                    .flex_1()
                    .min_w(px(titlebar_leading_inset() + 12.)),
            )
            .child(self.search_field(window, cx))
            .child(drag_area().flex_1().min_w(px(12.)))
    }

    fn search_field(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let input_state = self.search_input.read(cx);
        let focused = input_state.focus_handle(cx).is_focused(window);
        let empty = input_state.value().is_empty();
        row()
            .id("prefs-search")
            // ↑/↓ move between matching pages instead of the caret.
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| this.step_search(true, cx)))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| this.step_search(false, cx)))
            // The field lets Escape through when it has nothing of its own to close.
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                if this.search_input.read(cx).value().is_empty() {
                    window.focus(&this.focus, cx);
                } else {
                    this.clear_search(window, cx);
                }
            }))
            .w(px(SEARCH_WIDTH))
            .min_w(px(160.))
            .h(px(28.))
            .rounded(px(4.))
            .px(px(8.))
            .border_1()
            .border_color(if focused {
                t::accent()
            } else {
                transparent_black()
            })
            .bg(t::input_bg())
            .child(icon(IconName::Search).size(px(13.)))
            .child(
                input(&self.search_input)
                    .appearance(false)
                    .bordered(false)
                    .cleanable(true)
                    .small(),
            )
            // The clear button takes the hint's place while there is a query.
            .children(empty.then(|| {
                div()
                    .text_size(px(10.))
                    .text_color(t::faint())
                    .children(super::keymap::shortcut(
                        super::keymap::CommandId::SearchSettings,
                        cx,
                    ))
            }))
    }

    fn step_search(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self
            .search
            .update(cx, |search, cx| search.step(forward, cx))
        {
            cx.stop_propagation();
        }
    }

    /// Empties the query; the page shown stays.
    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.search
            .update(cx, |search, cx| search.set_query("", cx));
    }

    fn nothing_found(&self, text: SharedString, cx: &mut Context<Self>) -> Div {
        column()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .px(px(CONTENT_X))
            .child(
                div()
                    .max_w_full()
                    .truncate()
                    .text_color(t::secondary())
                    .child(format!("No settings match “{text}”")),
            )
            .child(
                button("clear-settings-search", "Clear search").on_click(cx.listener(
                    |this, _, window, cx| {
                        this.clear_search(window, cx);
                        this.search_input
                            .update(cx, |input, cx| input.focus(window, cx));
                    },
                )),
            )
    }
}

fn transparent_black() -> Hsla {
    rgba(0).into()
}

// Definite view bounds let GPUI retain the sibling's layout and paint output.
// Descendant control notifications, resizing and inherited text-style changes
// invalidate the corresponding cache automatically.
struct Preferences {
    search_input: Entity<InputState>,
    search: Entity<SettingsSearch>,
    focus: FocusHandle,
    navigation: Entity<PreferencesNavigation>,
    content: Entity<PreferencesContent>,
    appearance: Entity<appearance::AppearancePreferences>,
    tools_content: Entity<ToolsPreferences>,
    integrations: Entity<integrations::IntegrationsPreferences>,
    task_filters: Entity<task_filters::TaskFiltersPreferences>,
    remote: Entity<remote::RemotePreferences>,
    shortcuts: Entity<shortcuts::ShortcutsPreferences>,
    notch: Entity<notch::NotchPreferences>,
    page: Entity<String>,
    reveal: Presence,
    _subscriptions: [Subscription; 3],
}
impl Preferences {
    fn new(initial: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        super::keymap::init(cx);
        let page = cx.new(|_| initial.to_owned());
        let search = cx.new(|_| SettingsSearch::new(page.clone()));
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search settings…"));
        let tools_content = cx.new(|cx| ToolsPreferences::new(search.clone(), window, cx));
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
        let typing = cx.subscribe(
            &search_input,
            |this, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let text = input.read(cx).value();
                    this.search
                        .update(cx, |search, cx| search.set_query(&text, cx));
                }
                InputEvent::PressEnter { .. } => {
                    this.search.update(cx, |search, cx| search.step(true, cx));
                }
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
            },
        );
        // Focus inside the window keeps its key context, so ⌘K works at once.
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            navigation: cx.new(|cx| PreferencesNavigation::new(page.clone(), search.clone(), cx)),
            page,
            tools_content,
            task_filters: cx
                .new(|cx| task_filters::TaskFiltersPreferences::new(search.clone(), window, cx)),
            remote: cx.new(|cx| remote::RemotePreferences::new(search.clone(), window, cx)),
            shortcuts: cx
                .new(|cx| shortcuts::ShortcutsPreferences::new(search.clone(), window, cx)),
            notch: cx.new(|cx| notch::NotchPreferences::new(search.clone(), cx)),
            integrations: cx
                .new(|cx| integrations::IntegrationsPreferences::new(search.clone(), window, cx)),
            _subscriptions: [route, typing, cx.observe(&search, |_, _, cx| cx.notify())],
            reveal: Presence::new(true, motion::presets::CONTENT_REVEAL, Instant::now()),
            focus,
            search_input,
            content: cx.new(|cx| PreferencesContent::new(search.clone(), window, cx)),
            appearance: cx
                .new(|cx| appearance::AppearancePreferences::new(search.clone(), window, cx)),
            search,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext};

    #[gpui_kit::test]
    fn search_moves_between_matching_pages_and_escape_clears_it(cx: &mut TestAppContext) {
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            t::init(cx);
            AppState::install_test(canopy_desktop::state::workspace::Workspace::new(), cx);
            cx.open_window(Default::default(), |window, cx| {
                let view = cx.new(|cx| Preferences::new("General", window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let view = cx.update(|window, cx| {
            window
                .root::<Root>()
                .flatten()
                .unwrap()
                .read(cx)
                .view()
                .clone()
                .downcast::<Preferences>()
                .unwrap()
        });
        let page = |cx: &mut VisualTestContext| view.read_with(cx, |v, cx| v.page.read(cx).clone());
        let query = |cx: &mut VisualTestContext| {
            view.read_with(cx, |v, cx| v.search_input.read(cx).value().to_string())
        };
        let input_focused = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                view.read(cx)
                    .search_input
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            })
        };
        cx.dispatch_action(FocusSearch);
        assert!(input_focused(&mut cx));

        // The page shown has no match: search moves to the best one. Pages
        // without a match fade, matching ones show their number of matches.
        cx.simulate_input("API key");
        cx.run_until_parked();
        assert_eq!(page(&mut cx), "Claude");
        view.read_with(&cx, |v, cx| {
            let search = v.search.read(cx);
            assert_eq!(search.navigation("General", false), (true, 0));
            assert_eq!(search.navigation("Codex", false), (false, 1));
            assert_eq!(search.navigation("Claude", true), (false, 1));
            assert!(search.current(cx).hit("api_key"));
            assert!(!search.current(cx).hit("model"));
        });

        // ↓ and Enter go to the next matching page, ↑ back.
        cx.simulate_keystrokes("down");
        assert_eq!(page(&mut cx), "Gemini");
        cx.simulate_keystrokes("enter");
        assert_eq!(page(&mut cx), "OpenCode");
        cx.simulate_keystrokes("up");
        assert_eq!(page(&mut cx), "Gemini");
        assert_eq!(query(&mut cx), "API key", "arrows do not edit the query");

        // Escape clears the query and keeps the page; a second one leaves the field.
        cx.simulate_keystrokes("escape");
        assert_eq!(query(&mut cx), "");
        assert_eq!(page(&mut cx), "Gemini");
        assert!(view.read_with(&cx, |v, cx| !v.search.read(cx).is_active()));
        assert!(input_focused(&mut cx));
        cx.simulate_keystrokes("escape");
        assert!(!input_focused(&mut cx));

        // Nothing matches: the page is replaced by the empty result.
        cx.dispatch_action(FocusSearch);
        cx.simulate_input("qwertyuiop");
        cx.run_until_parked();
        assert!(view.read_with(&cx, |v, cx| v.search.read(cx).nothing_found()));
    }
}
