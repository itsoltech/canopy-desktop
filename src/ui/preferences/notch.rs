//! Settings → Notch: whether the session overlay is shown (`notch.enabled`).
//! The overlay itself follows the setting (`ui::notch::bind`).
use super::search::{Entry, SettingsSearch};
use super::*;
use crate::app_state::SettingsState;
use canopy_desktop::settings::Change;
use gpui_kit::{base::Disableable, component::scroll::ScrollableElement};

/// Platforms with a native overlay adapter.
const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));
const OVERLAY: Entry = Entry::new(
    "overlay",
    "NOTCH OVERLAY",
    "Show session status near the notch",
    "A floating overlay near the top of the screen shows active session info — handy when Canopy is in the background.",
    "agents working attention background menu bar",
);
/// Search entries, in page order.
pub(super) const SEARCH: &[Entry] = &[OVERLAY];

pub(super) struct NotchPreferences {
    settings: Entity<SettingsState>,
    search: Entity<SettingsSearch>,
    scroll: ScrollHandle,
    _subscriptions: [Subscription; 3],
}
impl NotchPreferences {
    pub(super) fn new(search: Entity<SettingsSearch>, cx: &mut Context<Self>) -> Self {
        let app = cx.global::<AppState>().clone();
        Self {
            _subscriptions: [
                cx.observe(&app.settings, |_, _, cx| cx.notify()),
                cx.observe(&app.notch_status, |_, _, cx| cx.notify()),
                cx.observe(&search, |_, _, cx| cx.notify()),
            ],
            settings: app.settings,
            search,
            scroll: ScrollHandle::new(),
        }
    }
}
impl Render for NotchPreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.settings.read(cx);
        let disabled = !SUPPORTED || !state.ready || state.saving;
        let enabled = state.values.notch_enabled;
        let settings_error = state.error.clone();
        let overlay_error = cx
            .global::<AppState>()
            .notch_status
            .read(cx)
            .error
            .clone()
            .filter(|_| enabled);
        let search = self.search.read(cx).page("Notch").scroll(&self.scroll);
        let help = if SUPPORTED {
            OVERLAY.help
        } else {
            "The notch overlay is available on macOS and Windows."
        };
        column()
            .size_full()
            .child(super::remote::page_header(
                "Notch",
                "Session status at the top of the screen",
            ))
            .child(
                column()
                    .id("prefs-notch")
                    .flex_1()
                    .track_scroll(&self.scroll)
                    .overflow_y_scrollbar()
                    .px(px(CONTENT_X))
                    .py(px(20.))
                    .child(
                        column()
                            .flex_shrink_0()
                            .gap(px(SECTION_GAP))
                            .children(
                                settings_error.map(|error| div().text_color(t::red()).child(error)),
                            )
                            .child(section(
                                "NOTCH OVERLAY",
                                "Agent sessions that are working or need attention",
                                vec![
                                    search
                                        .setting_with(
                                            &OVERLAY,
                                            help,
                                            true,
                                            checkbox("notch-enabled", enabled, OVERLAY.label)
                                                .disabled(disabled)
                                                .on_click(cx.listener(|this, value, _, cx| {
                                                    this.settings.update(cx, |state, cx| {
                                                        state.change(
                                                            Change::NotchEnabled(*value),
                                                            cx,
                                                        )
                                                    });
                                                })),
                                        )
                                        .into_any_element(),
                                ],
                            ))
                            .children(overlay_error.map(|error| {
                                div()
                                    .text_size(px(11.))
                                    .line_height(px(14.85))
                                    .text_color(t::red())
                                    .child(error)
                            })),
                    ),
            )
    }
}
