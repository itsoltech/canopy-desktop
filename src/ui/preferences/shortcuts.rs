//! Settings → Shortcuts: every command of the keymap, grouped, with its current
//! shortcut. Clicking a shortcut records a new one; the keymap owns the result.
use super::super::components::modal::{Confirmation, ModalDismissed};
use super::super::keymap::{self, COMMANDS, Command, CommandId, Group, Keymap, Overlap};
use super::search::{Entry, PageSearch, SettingsSearch};
use super::*;
use crate::app_state::SettingsState;
use canopy_desktop::settings::ShortcutOverrides;
use gpui_kit::{
    base::Disableable,
    component::{
        button::{Button, ButtonVariants},
        input::{InputEvent, InputState},
        scroll::ScrollableElement,
    },
    prelude::FluentBuilder,
};

const NOT_RECORDABLE: &str = "Use ⌘, ⌃ or ⌥ with a key, or a function key.";
const NOT_RECORDABLE_PC: &str = "Use Ctrl or Alt with a key, or a function key.";

/// Search entries: every command of the keymap, by name, in page order.
pub(super) fn entries() -> Vec<Entry> {
    Group::ALL
        .into_iter()
        .flat_map(|group| COMMANDS.iter().filter(move |c| c.group == group))
        .map(|command| Entry::new(command.key, command.group.label(), command.name, "", ""))
        .collect()
}

/// A recorded shortcut that another command already uses.
struct PendingConflict {
    command: CommandId,
    keystroke: String,
    others: Vec<(CommandId, Overlap)>,
}

pub(super) struct ShortcutsPreferences {
    settings: Entity<SettingsState>,
    search: Entity<SettingsSearch>,
    scroll: ScrollHandle,
    filter: Entity<InputState>,
    focus: FocusHandle,
    recording: Option<CommandId>,
    /// Why the last recorded keystroke was not accepted.
    hint: Option<CommandId>,
    conflict: Option<PendingConflict>,
    /// Takes keystrokes before any binding while recording.
    _recorder: Option<Subscription>,
    confirmation: Option<Entity<Confirmation>>,
    confirmation_events: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl ShortcutsPreferences {
    pub(super) fn new(
        search: Entity<SettingsSearch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        keymap::init(cx);
        let settings = cx.global::<AppState>().settings.clone();
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter by command or shortcut"));
        let subscriptions = vec![
            cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.observe_global::<Keymap>(|_, cx| cx.notify()),
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&search, |_, _, cx| cx.notify()),
        ];
        Self {
            settings,
            search,
            scroll: ScrollHandle::new(),
            filter,
            focus: cx.focus_handle(),
            recording: None,
            hint: None,
            conflict: None,
            _recorder: None,
            confirmation: None,
            confirmation_events: None,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn start_recording(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recording = Some(id);
        self.hint = None;
        self.conflict = None;
        let page = cx.entity().downgrade();
        let handle = window.window_handle();
        self._recorder = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != handle {
                return;
            }
            let Some(page) = page.upgrade() else {
                return;
            };
            if page.update(cx, |page, cx| page.record(&event.keystroke, cx)) {
                cx.stop_propagation();
            }
        }));
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording.take().is_some() {
            self._recorder = None;
            cx.notify();
        }
    }

    /// One keystroke while recording; true when it was consumed.
    fn record(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.recording else {
            return false;
        };
        if keymap::is_modifier_only(keystroke) {
            return true;
        }
        let m = keystroke.modifiers;
        let plain = !m.modified();
        match keystroke.key.as_str() {
            "escape" if plain => {
                self.stop_recording(cx);
                return true;
            }
            "backspace" | "delete" if plain => {
                self.stop_recording(cx);
                let (platform, overrides) = current(cx);
                self.apply(
                    keymap::with_keystrokes(&overrides, platform, id.command(), vec![]),
                    cx,
                );
                return true;
            }
            // Tab keeps moving focus; recording ends with it.
            "tab" if !(m.control || m.alt || m.platform) => {
                self.stop_recording(cx);
                return false;
            }
            _ => {}
        }
        if !keymap::recordable(keystroke) {
            self.hint = Some(id);
            cx.notify();
            return true;
        }
        let keystroke = Keystroke {
            modifiers: m,
            key: keystroke.key.clone(),
            key_char: None,
        }
        .unparse();
        self.stop_recording(cx);
        let (platform, overrides) = current(cx);
        let command = id.command();
        let others: Vec<_> = keymap::conflicts(&overrides, platform, command, &keystroke)
            .into_iter()
            .map(|conflict| (conflict.command.id, conflict.overlap))
            .collect();
        if others.is_empty() {
            self.apply(
                keymap::assign(&overrides, platform, command, &keystroke, &[]),
                cx,
            );
        } else {
            self.conflict = Some(PendingConflict {
                command: id,
                keystroke,
                others,
            });
            cx.notify();
        }
        true
    }

    /// Ends a conflict: `take` moves the shortcut, `keep` assigns it to both.
    fn resolve(&mut self, take: bool, keep: bool, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        if take || keep {
            let (platform, overrides) = current(cx);
            let others: Vec<CommandId> = if take {
                conflict.others.iter().map(|(id, _)| *id).collect()
            } else {
                vec![]
            };
            self.apply(
                keymap::assign(
                    &overrides,
                    platform,
                    conflict.command.command(),
                    &conflict.keystroke,
                    &others,
                ),
                cx,
            );
        }
        cx.notify();
    }

    fn reset(&mut self, id: CommandId, cx: &mut Context<Self>) {
        let (platform, overrides) = current(cx);
        let command = id.command();
        if self.conflict.as_ref().is_some_and(|c| c.command == id) {
            self.conflict = None;
        }
        self.apply(
            keymap::with_keystrokes(&overrides, platform, command, command.defaults(platform)),
            cx,
        );
    }

    fn apply(&mut self, overrides: ShortcutOverrides, cx: &mut Context<Self>) {
        self.hint = None;
        keymap::set_overrides(overrides, cx);
        cx.notify();
    }

    fn confirm_reset_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirmation.is_some() {
            return;
        }
        self.stop_recording(cx);
        self.conflict = None;
        let modal = cx.new(|cx| {
            Confirmation::new(
                "Reset all shortcuts?",
                "Every command goes back to its default shortcut, and your custom shortcuts are removed.",
                "Reset all",
                self.focus.clone(),
                |cx| {
                    keymap::set_overrides(ShortcutOverrides::default(), cx);
                    Ok(())
                },
                window,
                cx,
            )
            .destructive()
        });
        self.confirmation_events = Some(cx.subscribe(&modal, |this, _, _: &ModalDismissed, cx| {
            this.confirmation = None;
            this.confirmation_events = None;
            cx.notify();
        }));
        self.confirmation = Some(modal);
        cx.notify();
    }

    fn row(
        &self,
        command: &'static Command,
        first: bool,
        platform: keymap::Platform,
        overrides: &ShortcutOverrides,
        search: &PageSearch,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = command.id;
        let keystrokes = command.keystrokes(platform, overrides);
        let modified = command.modified(platform, overrides);
        let recording = self.recording == Some(id);
        let conflict = self.conflict.as_ref().filter(|c| c.command == id);
        // Two commands that both apply at once: only one of them can run.
        let clashes: Vec<&'static str> = keystrokes
            .iter()
            .flat_map(|k| keymap::conflicts(overrides, platform, command, k))
            .filter(|c| c.overlap == Overlap::Same)
            .map(|c| c.command.name)
            .collect();
        let key = command.key;
        let note = |text: String, color: Hsla| {
            div()
                .text_size(px(11.))
                .line_height(px(14.85))
                .text_color(color)
                .child(text)
        };
        let mut details = Vec::new();
        if recording {
            details.push(
                note(
                    if self.hint == Some(id) {
                        if cfg!(target_os = "macos") {
                            NOT_RECORDABLE
                        } else {
                            NOT_RECORDABLE_PC
                        }
                        .to_owned()
                    } else {
                        format!(
                            "Press the new shortcut. {} cancels, {} removes it.",
                            keymap::label("escape"),
                            keymap::label("backspace"),
                        )
                    },
                    if self.hint == Some(id) {
                        t::yellow()
                    } else {
                        t::muted()
                    },
                )
                .into_any_element(),
            );
        } else if let Some(conflict) = conflict {
            let names = conflict
                .others
                .iter()
                .map(|(other, _)| format!("“{}”", other.command().name))
                .collect::<Vec<_>>()
                .join(", ");
            let nested = conflict.others.iter().all(|(_, o)| *o == Overlap::Nested);
            let shortcut = keymap::label(&conflict.keystroke);
            let message = if nested {
                // One scope is inside the other; the inner command wins while focused.
                let other = conflict.others[0].0.command();
                if command.scope == keymap::Scope::Workspace {
                    format!(
                        "{shortcut} is also used by {names}, which wins {}.",
                        other.scope.label()
                    )
                } else {
                    format!(
                        "{shortcut} is also used by {names}; this one wins {}.",
                        command.scope.label()
                    )
                }
            } else {
                format!("{shortcut} is used by {names}.")
            };
            details.push(
                column()
                    .gap(px(6.))
                    .child(note(message, t::yellow()))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                button(SharedString::from(format!("{key}-reassign")), "Reassign")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.resolve(true, false, cx)),
                                    ),
                            )
                            .when(nested, |row| {
                                row.child(
                                    button(SharedString::from(format!("{key}-keep")), "Keep both")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.resolve(false, true, cx)
                                        })),
                                )
                            })
                            .child(
                                button(SharedString::from(format!("{key}-cancel")), "Cancel")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.resolve(false, false, cx)
                                        }),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
        } else if !clashes.is_empty() {
            details.push(
                note(
                    format!(
                        "Also assigned to “{}”; only one of them runs.",
                        clashes.join("”, “")
                    ),
                    t::yellow(),
                )
                .into_any_element(),
            );
        }

        let shortcut = Button::new(SharedString::from(format!("{key}-shortcut")))
            .ghost()
            .xsmall()
            .compact()
            .h(px(28.))
            .px(px(4.))
            .rounded(px(4.))
            .when(recording, |button| {
                button
                    .border_1()
                    .border_color(t::accent())
                    .bg(t::selected())
            })
            .accessibility_label(SharedString::from(format!(
                "{}: {}. Change shortcut",
                command.name,
                if keystrokes.is_empty() {
                    "no shortcut".to_owned()
                } else {
                    keystrokes
                        .iter()
                        .map(|k| keymap::label(k))
                        .collect::<Vec<_>>()
                        .join(" or ")
                }
            )))
            .child(if recording {
                div()
                    .px(px(4.))
                    .text_size(px(12.))
                    .text_color(t::accent())
                    .child("Press keys…")
                    .into_any_element()
            } else if keystrokes.is_empty() {
                div()
                    .px(px(4.))
                    .text_size(px(12.))
                    .text_color(t::faint())
                    .child("Not assigned")
                    .into_any_element()
            } else {
                row()
                    .gap(px(8.))
                    .children(keystrokes.iter().map(|k| keycaps(k)))
                    .into_any_element()
            })
            .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                if this.recording == Some(id) {
                    this.stop_recording(cx);
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.recording == Some(id) {
                    this.stop_recording(cx);
                } else {
                    this.start_recording(id, window, cx);
                }
            }));

        let row = row()
            .id(SharedString::from(format!("{key}-row")))
            .items_start()
            .flex_shrink_0()
            .gap(px(16.))
            .py(px(8.))
            .border_t(px(if first { 0. } else { 1. }))
            .border_color(t::border())
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .pt(px(5.))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(17.55))
                                    .truncate()
                                    .child(search.text(key, command.name)),
                            )
                            .children(modified.then(|| badge("Modified"))),
                    )
                    .children(details),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(4.))
                    // The slot stays when empty, so shortcuts keep one right edge.
                    .child(
                        div()
                            .size(px(20.))
                            .flex_shrink_0()
                            .children(modified.then(|| {
                                icon_button_base(
                                    SharedString::from(format!("{key}-reset")),
                                    format!(
                                        "Reset to default ({})",
                                        command
                                            .defaults(platform)
                                            .first()
                                            .map(|k| keymap::label(k))
                                            .unwrap_or_else(|| "none".to_owned())
                                    ),
                                )
                                .icon(custom_icon("rotate-ccw"))
                                .on_click(cx.listener(move |this, _, _, cx| this.reset(id, cx)))
                            })),
                    )
                    .child(shortcut),
            );
        search.mark(&[key], row)
    }
}

fn current(cx: &App) -> (keymap::Platform, ShortcutOverrides) {
    let keymap = cx.global::<Keymap>();
    (keymap.platform(), keymap.overrides().clone())
}

/// One shortcut as separate keycaps, like the Electron list.
fn keycaps(keystroke: &str) -> Div {
    row()
        .gap(px(4.))
        .children(keymap::keycaps(keystroke).into_iter().map(|cap| {
            row()
                .justify_center()
                .min_w(px(22.))
                .h(px(22.))
                .px(px(6.))
                .rounded(px(3.))
                .border_1()
                .border_color(t::border())
                .bg(t::input_bg())
                .text_size(px(11.))
                .text_color(t::secondary())
                .child(cap)
        }))
}

fn matches(command: &Command, keystrokes: &[String], query: &str) -> bool {
    query.is_empty()
        || command.name.to_lowercase().contains(query)
        || keystrokes.iter().any(|k| {
            k.contains(query)
                || keymap::label(k).to_lowercase().contains(query)
                || keymap::keycaps(k).concat().to_lowercase().contains(query)
        })
}

impl Render for ShortcutsPreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (platform, overrides) = current(cx);
        let query = self.filter.read(cx).value().trim().to_lowercase();
        let search = self.search.read(cx).page("Shortcuts").scroll(&self.scroll);
        let state = self.settings.read(cx);
        let error = state.error.clone();
        let loading = !state.ready && error.is_none();
        let any_modified = COMMANDS.iter().any(|c| c.modified(platform, &overrides));
        let mut sections = Vec::new();
        for group in Group::ALL {
            let commands: Vec<&'static Command> = COMMANDS
                .iter()
                .filter(|c| c.group == group)
                .filter(|c| matches(c, &c.keystrokes(platform, &overrides), &query))
                .collect();
            if commands.is_empty() {
                continue;
            }
            let rows = commands
                .into_iter()
                .enumerate()
                .map(|(index, command)| {
                    self.row(command, index == 0, platform, &overrides, &search, cx)
                        .into_any_element()
                })
                .collect::<Vec<_>>();
            sections.push(preference_section(group.label(), column().children(rows)));
        }
        let empty = sections.is_empty().then(|| {
            format!(
                "No commands match “{}”.",
                self.filter.read(cx).value().trim()
            )
        });
        column()
            .size_full()
            .track_focus(&self.focus)
            .child(
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
                            .child("Shortcuts"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .line_height(px(14.85))
                            .text_color(t::muted())
                            .child("Click a shortcut to record a new one"),
                    ),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .px(px(CONTENT_X))
                    .pt(px(16.))
                    .gap(px(12.))
                    .child(
                        row()
                            .flex_1()
                            .max_w(px(320.))
                            .h(px(28.))
                            .rounded(px(4.))
                            .px(px(8.))
                            .gap(px(6.))
                            .bg(t::input_bg())
                            .child(icon(IconName::Search).size(px(13.)))
                            .child(
                                input(&self.filter)
                                    .appearance(false)
                                    .bordered(false)
                                    .cleanable(true),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        button("reset-all-shortcuts", "Reset all…")
                            .disabled(!any_modified)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_reset_all(window, cx)
                            })),
                    ),
            )
            .child(
                column()
                    .id("prefs-shortcuts")
                    .flex_1()
                    .track_scroll(&self.scroll)
                    .overflow_y_scrollbar()
                    .px(px(CONTENT_X))
                    .py(px(20.))
                    .child(
                        column()
                            .flex_shrink_0()
                            .gap(px(SECTION_GAP))
                            .children(error.map(|error| div().text_color(t::red()).child(error)))
                            .children(loading.then(|| div().child("Loading settings…")))
                            .children(sections)
                            .children(empty.map(|text| div().text_color(t::muted()).child(text))),
                    ),
            )
            .children(self.confirmation.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext, component::Root};

    fn keys(id: CommandId, cx: &mut VisualTestContext) -> Vec<String> {
        cx.update(|_, cx| cx.global::<Keymap>().keystrokes(id))
    }

    #[gpui_kit::test]
    fn recording_assigns_cancels_clears_and_resolves_conflicts(cx: &mut TestAppContext) {
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            t::init(cx);
            AppState::install_test(canopy_desktop::state::workspace::Workspace::new(), cx);
            cx.open_window(Default::default(), |window, cx| {
                let search = SettingsSearch::detached("Shortcuts", cx);
                let page = cx.new(|cx| ShortcutsPreferences::new(search, window, cx));
                cx.new(|cx| Root::new(page, window, cx))
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let page = cx.update(|window, cx| {
            window
                .root::<Root>()
                .flatten()
                .unwrap()
                .read(cx)
                .view()
                .clone()
                .downcast::<ShortcutsPreferences>()
                .unwrap()
        });
        let record = |id, cx: &mut VisualTestContext| {
            page.update_in(cx, |page, window, cx| page.start_recording(id, window, cx));
        };
        let custom = keymap::canonical("ctrl-alt-k").unwrap();
        let new_chat = keys(CommandId::NewChat, &mut cx);

        // Escape cancels; a key without a modifier keeps recording with a hint.
        record(CommandId::NewChat, &mut cx);
        cx.simulate_keystrokes("a");
        assert!(page.read_with(&cx, |p, _| p.recording.is_some() && p.hint.is_some()));
        cx.simulate_keystrokes("escape");
        assert!(page.read_with(&cx, |p, _| p.recording.is_none()));
        assert_eq!(keys(CommandId::NewChat, &mut cx), new_chat);

        // A new shortcut replaces the default; Backspace removes it.
        record(CommandId::NewTab, &mut cx);
        cx.simulate_keystrokes(&custom);
        assert_eq!(
            keys(CommandId::NewTab, &mut cx),
            std::slice::from_ref(&custom)
        );
        record(CommandId::NewChat, &mut cx);
        cx.simulate_keystrokes("backspace");
        assert!(keys(CommandId::NewChat, &mut cx).is_empty());

        // Used by another command in the same scope: nothing changes until resolved.
        record(CommandId::GoToFile, &mut cx);
        cx.simulate_keystrokes(&custom);
        assert!(page.read_with(&cx, |p, _| p.conflict.is_some()));
        assert_eq!(
            keys(CommandId::NewTab, &mut cx),
            std::slice::from_ref(&custom)
        );
        page.update(&mut cx, |page, cx| page.resolve(true, false, cx));
        assert_eq!(
            keys(CommandId::GoToFile, &mut cx),
            std::slice::from_ref(&custom)
        );
        assert!(keys(CommandId::NewTab, &mut cx).is_empty());

        // Reset brings the default back.
        page.update(&mut cx, |page, cx| page.reset(CommandId::NewChat, cx));
        assert_eq!(keys(CommandId::NewChat, &mut cx), new_chat);
    }
}
