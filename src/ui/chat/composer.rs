//! Composer island: message field, account/model, reasoning and mode pickers, Send/Stop.
use super::suggest::SuggestKey;
use super::{render::centered, view::ChatView};
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{
    config::{Mode, ModelInfo},
    provider::Provider,
    transcript::PermissionKind,
};
use canopy_desktop::motion;
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{self, Textarea},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Instant;

/// Accounts are the Claude Code and Codex profiles configured in Preferences.
pub(super) struct Account {
    pub provider: Provider,
    pub id: String,
    pub name: String,
}

/// The mark of the agent a chat talks to.
pub(super) fn provider_icon(provider: Provider) -> AnyElement {
    match provider {
        Provider::Claude => tool_icon("Claude Code", IconName::Bot),
        Provider::Codex => custom_icon("openai")
            .text_color(t::text())
            .into_any_element(),
    }
}

impl ChatView {
    pub(super) fn composer_island(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.apply_composer_change(window, cx);
        let working = self.is_working();
        let deciding = self.decision.active().is_some();
        self.insert_badges(window, cx);
        let empty = self.composer.read(cx).value().trim().is_empty();
        let ended = self.status().is_some_and(|status| status.ended());
        // `!command`: the island turns burgundy and Enter runs it (DEC-072).
        let shell = self.shell_mode(cx);
        let now = Instant::now();
        let resting = self.resting.on;
        motion::request_frame(window, self.resting.height(now).is_some());
        let measured = self.resting.measured.clone();
        let field = {
            let view = cx.entity().downgrade();
            let badges = view.clone();
            let clicks = view.clone();
            Textarea::new(&self.composer)
                .appearance(false)
                .disabled(ended)
                .w_full()
                .text_size(px(14.))
                .px(px(4.))
                // Images and copied files become attachments, not text.
                .on_paste(move |item, _, cx| {
                    view.update(cx, |this, cx| this.paste(item, cx))
                        .unwrap_or(false)
                })
                // Attachments are badges in the text.
                .token(move |context, _, cx| {
                    badges
                        .upgrade()
                        .map(|view| view.read(cx).badge(context))
                        .unwrap_or_else(|| div().into_any_element())
                })
                .on_token_click(move |event, _, cx| {
                    let _ = clicks.update(cx, |this, cx| this.badge_clicked(event, cx));
                })
        };
        let attach = Button::new("chat-attach")
            .ghost()
            .compact()
            .size(px(26.))
            .min_w(px(26.))
            .max_w(px(26.))
            .min_h(px(26.))
            .max_h(px(26.))
            .p_0()
            .rounded(px(13.))
            .border_0()
            .accessibility_label("Attach images or files")
            .tooltip("Attach images or files")
            .disabled(ended)
            .child(custom_icon("paperclip").size(px(14.)))
            .on_click(cx.listener(|this, _, _, cx| this.pick_attachments(cx)));
        let action = (!deciding).then(|| {
            self.action_button(working, empty || ended || self.attachments_blocked(cx), cx)
        });
        centered(
            column()
                .w_full()
                .rounded(px(16.))
                .border_1()
                .border_color(if shell {
                    t::shell().opacity(0.7)
                } else {
                    t::control_border()
                })
                .bg(if shell {
                    t::bg().blend(t::shell().opacity(0.12))
                } else {
                    t::composer_surface()
                })
                // The field binds ↑/↓/Enter/Tab/Esc to its own actions, which run before
                // key listeners: the open suggestions (and ↑ in an empty field, recalling
                // earlier messages) take them first.
                .capture_action(cx.listener(|this, _: &input::MoveUp, window, cx| {
                    if this.suggest_key(SuggestKey::Up, window, cx) || this.recall(true, window, cx)
                    {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, _: &input::MoveDown, window, cx| {
                    if this.suggest_key(SuggestKey::Down, window, cx)
                        || this.recall(false, window, cx)
                    {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, enter: &input::Enter, window, cx| {
                    if !enter.secondary
                        && !enter.shift
                        && this.suggest_key(SuggestKey::Accept, window, cx)
                    {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, _: &input::IndentInline, window, cx| {
                    if this.suggest_key(SuggestKey::Accept, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, _: &input::Escape, window, cx| {
                    if this.suggest_key(SuggestKey::Close, window, cx)
                        || this.leave_shell(window, cx)
                    {
                        cx.stop_propagation();
                    }
                }))
                // Backspace in an empty field leaves shell mode.
                .capture_action(cx.listener(|this, _: &input::Backspace, window, cx| {
                    if this.leave_shell(window, cx) {
                        cx.stop_propagation();
                    }
                }))
                // Number keys pick an option and ⌘⌫ denies while a request waits.
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if this.decision_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .when_some(self.resting.height(now), |island, height| {
                    island.h(height + px(2.)).overflow_hidden()
                })
                .child(
                    div()
                        .relative()
                        .w_full()
                        .flex_shrink_0()
                        .px(px(10.))
                        .py(px(if resting { 6. } else { 10. }))
                        .child(if resting {
                            // One line: the start of the draft, Attach and Send.
                            row()
                                .w_full()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| this.wake_composer(cx)),
                                        )
                                        .child(field),
                                )
                                .child(attach)
                                .children(action)
                        } else {
                            column()
                                .w_full()
                                .gap(px(6.))
                                .children(shell.then(super::shell::shell_label))
                                .child(field)
                                .children(self.attachment_errors(cx))
                                .children(self.shell_error.clone().map(|error| {
                                    div()
                                        .px(px(4.))
                                        .text_size(px(12.))
                                        .text_color(t::red())
                                        .child(error)
                                }))
                                .child(
                                    row()
                                        .w_full()
                                        .gap(px(2.))
                                        .child(self.pickers(window, cx))
                                        .children(super::actions::context_ring(self))
                                        .child(attach)
                                        .children(
                                            deciding.then(|| self.decision_buttons(empty, cx)),
                                        )
                                        // While a request waits, its own buttons answer it; ⌘. still stops.
                                        .children(action),
                                )
                        })
                        .child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, _, _| measured.set(bounds.size.height),
                            )
                            .absolute()
                            .inset_0(),
                        ),
                ),
        )
        .pt(px(4.))
        .pb(px(16.))
    }

    /// Model (with reasoning) and mode pickers in the room the buttons leave: with full
    /// labels when they fit, then only the model named, then icons only.
    fn pickers(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (_, model) = self.selection_label(cx);
        let effort = self.effort_label();
        let mode = self.settings.mode.label();
        let font = window.text_style().font();
        let measure = |text: &str| -> f32 {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                ..Default::default()
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(text.to_owned().into(), px(12.), &[run], None)
                    .width,
            )
        };
        // Padding, mark, gaps and chevron of one picker; a little slack for rounding.
        const ICON_ONLY: f32 = 48.;
        const LABEL_EXTRA: f32 = 5.;
        let gaps = 2.;
        let effort_width = effort.as_deref().map_or(0., |e| LABEL_EXTRA + measure(e));
        let full =
            2. * (ICON_ONLY + LABEL_EXTRA) + measure(&model) + effort_width + measure(mode) + gaps;
        let model_only = 2. * ICON_ONLY + LABEL_EXTRA + measure(&model) + gaps;
        let available = f32::from(self.pickers_width.get());
        let density = if full <= available {
            Density::Full
        } else if model_only <= available {
            Density::Model
        } else {
            Density::Icons
        };
        let room = self.pickers_width.clone();
        row()
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .gap(px(2.))
            .child(self.model_picker(density, cx))
            .child(self.mode_picker(density, cx))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if room.get() != bounds.size.width {
                            room.set(bounds.size.width);
                            window.refresh();
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
    }

    /// Claude Code and Codex profiles, and the chat's own.
    pub(super) fn accounts(&self, cx: &App) -> (Vec<Account>, String) {
        let catalog = &cx.global::<AppState>().tools.read(cx).catalog;
        let mut accounts = vec![];
        for provider in Provider::ALL {
            let Some(tool) = catalog.get(provider.tool()).filter(|tool| tool.enabled) else {
                continue;
            };
            let short = match provider {
                Provider::Claude => "Claude",
                Provider::Codex => "Codex",
            };
            accounts.extend(tool.profiles.iter().map(|profile| Account {
                provider,
                id: profile.id.clone(),
                name: if tool.profiles.len() == 1 {
                    provider.name().into()
                } else {
                    format!("{short} {}", profile.name)
                },
            }));
        }
        let current = self
            .pane
            .metadata
            .profile_id
            .clone()
            .or_else(|| catalog.get(self.provider().tool())?.default_profile.clone())
            .unwrap_or_default();
        (accounts, current)
    }

    /// The entry for no chosen model: the account's default, named when known.
    pub(super) fn default_model_label(models: &[ModelInfo]) -> String {
        models
            .iter()
            .filter(|m| m.value == "default")
            .map(ModelInfo::label)
            .next()
            .or_else(|| {
                let model = models.iter().find(|m| m.default)?;
                Some(format!("Default ({})", model.label()))
            })
            .unwrap_or_else(|| "Default model".to_owned())
    }

    pub(super) fn models(&self) -> Vec<ModelInfo> {
        self.session
            .as_ref()
            .map(|session| session.models().to_vec())
            .unwrap_or_default()
    }

    /// The model the choices apply to; `None` means the account's default entry.
    pub(super) fn current_model(&self) -> Option<ModelInfo> {
        let models = self.models();
        match self.settings.model.as_deref() {
            Some(value) => models.into_iter().find(|m| m.value == value),
            // Claude lists a `default` entry; Codex marks its default model.
            None => {
                let default = models.iter().position(|m| m.value == "default");
                let index = default.or_else(|| models.iter().position(|m| m.default))?;
                models.into_iter().nth(index)
            }
        }
    }

    /// Account and model names shown in the picker and the empty state.
    pub(super) fn selection_label(&self, cx: &App) -> (String, String) {
        let (accounts, current) = self.accounts(cx);
        let account = accounts
            .into_iter()
            .find(|account| account.id == current)
            .map(|account| account.name)
            .unwrap_or_else(|| self.provider().name().into());
        let models = self.models();
        let model = match &self.settings.model {
            None => Self::default_model_label(&models),
            Some(value) => models
                .iter()
                .filter(|m| &m.value == value)
                .map(ModelInfo::label)
                .next()
                .unwrap_or_else(|| value.clone()),
        };
        (account, model)
    }

    /// The effort the trigger names: the chosen one, else the account's default when
    /// known; `None` for a model without reasoning levels.
    pub(super) fn effort_label(&self) -> Option<String> {
        let model = self.current_model().filter(ModelInfo::supports_effort)?;
        self.settings
            .effort
            .or(model.default_effort)
            .map(|effort| effort.label().to_owned())
    }

    fn mode_picker(&self, density: Density, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.settings.mode;
        let allowed = self.settings.allowed.clone();
        // Without the model list yet, every mode stays offered; the CLI validates.
        let codex = self.provider() == Provider::Codex;
        let auto = codex || self.current_model().is_none_or(|model| model.supports_auto);
        let view = cx.entity().downgrade();
        let warning = mode.unguarded().then(t::yellow);
        let options: &'static [Mode] = if codex { &Mode::CODEX } else { &Mode::CLAUDE };
        let picker = picker_button(
            "chat-mode",
            Some(
                mode_icon(mode)
                    .text_color(warning.unwrap_or_else(t::muted))
                    .into_any_element(),
            ),
            (density == Density::Full).then(|| mode.label().to_owned()),
            format!("Work mode: {}", mode.label()),
            warning,
            cx,
        );
        if density == Density::Full {
            picker
        } else {
            picker.tooltip(format!("Work mode: {}", mode.label()))
        }
        .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
            menu = menu.label("Work mode");
            for &option in options {
                if option == Mode::Auto && !auto {
                    continue;
                }
                let view = view.clone();
                let label = match option {
                    Mode::Yolo => "Yolo — runs everything without asking",
                    Mode::FullAccess => "Full access — no sandbox, never asks",
                    Mode::ReadOnly => "Read only — asks before any change",
                    Mode::Auto if codex => "Auto — works in this folder without asking",
                    Mode::Ask if codex => "Ask — asks before commands and edits",
                    other => other.label(),
                };
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .icon(mode_icon(option))
                        .checked(mode == option)
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| this.set_mode(option, cx));
                        }),
                );
            }
            if !allowed.is_empty() {
                menu = menu
                    .separator()
                    .label("Allowed in this chat · click to remove");
                for rule in &allowed {
                    let view = view.clone();
                    let remove = rule.clone();
                    menu = menu.item(
                        PopupMenuItem::new(rule.clone())
                            .icon(icon(IconName::Close).size(px(13.)))
                            .on_click(move |_, _, cx| {
                                let _ = view.update(cx, |this, cx| this.remove_rule(&remove, cx));
                            }),
                    );
                }
            }
            menu.min_w(px(240.))
        })
    }

    /// While a request waits: back to the previous question, then Next / Submit for
    /// questions, or Deny / Keep planning with the typed note.
    fn decision_buttons(&self, empty: bool, cx: &mut Context<Self>) -> Div {
        let Some(request) = self.decision.active() else {
            return row();
        };
        let last = request.step + 1 >= request.questions.len();
        let answered = !empty
            || request
                .chosen
                .get(request.step)
                .is_some_and(|chosen| !chosen.is_empty());
        let primary = match request.permission.kind {
            PermissionKind::Question => Some((if last { "Submit" } else { "Next" }, !answered)),
            PermissionKind::Tool if !empty => Some(("Deny", false)),
            PermissionKind::Plan if !empty => Some(("Keep planning", false)),
            _ => None,
        };
        row()
            .gap(px(6.))
            .children(
                (request.permission.kind == PermissionKind::Question && request.step > 0).then(
                    || {
                        Button::new("chat-decision-back")
                            .ghost()
                            .compact()
                            .size(px(28.))
                            .min_w(px(28.))
                            .max_w(px(28.))
                            .p_0()
                            .rounded(px(14.))
                            .border_1()
                            .border_color(t::control_border())
                            .icon(IconName::ChevronLeft)
                            .accessibility_label("Previous question")
                            .tooltip("Previous question")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.decision_back(window, cx)),
                            )
                    },
                ),
            )
            .children(primary.map(|(label, disabled)| {
                Button::new("chat-decision-primary")
                    .primary()
                    .compact()
                    .h(px(28.))
                    .px(px(14.))
                    .rounded(px(14.))
                    .label(label)
                    .accessibility_label(label)
                    .tooltip(match label {
                        "Next" | "Submit" => "Enter",
                        _ => "Send with your note (Enter)",
                    })
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.decision_enter(window, cx)))
            }))
    }

    fn action_button(&self, working: bool, disabled: bool, cx: &mut Context<Self>) -> Button {
        let progress = self.action.value(Instant::now());
        let label = if working { "Stop" } else { "Send" };
        let swap = |name: IconName, visible: f32| {
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .opacity(visible)
                .child(
                    gpui_kit::component::Icon::new(name)
                        .size(px(9. + 5. * visible))
                        .text_color(t::bg()),
                )
        };
        Button::new("chat-action")
            .primary()
            .compact()
            .size(px(28.))
            .min_w(px(28.))
            .max_w(px(28.))
            .min_h(px(28.))
            .max_h(px(28.))
            .p_0()
            .rounded(px(14.))
            .accessibility_label(label)
            .tooltip(if working {
                "Stop (⌘.)"
            } else {
                "Send (Enter)"
            })
            .disabled(!working && disabled)
            .child(
                div()
                    .relative()
                    .size(px(14.))
                    .child(swap(IconName::ArrowUp, 1. - progress))
                    .child(swap(IconName::Square, progress)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.is_working() {
                    this.interrupt(cx);
                } else if this.editing.is_some() {
                    this.send_edit(window, cx);
                } else {
                    this.submit(window, cx);
                }
            }))
    }
}

/// One mark per work mode, shown on the picker and in its menu.
fn mode_icon(mode: Mode) -> gpui_kit::component::Icon {
    match mode {
        Mode::Ask => custom_icon("shield"),
        Mode::AcceptEdits => custom_icon("pencil"),
        Mode::Plan => icon(IconName::Map),
        Mode::Auto => custom_icon("sparkles"),
        Mode::Yolo | Mode::FullAccess => icon(IconName::TriangleAlert),
        Mode::ReadOnly => icon(IconName::Search),
    }
    .size(px(13.))
}

/// How much of the pickers' labels fits beside the composer's buttons.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Density {
    Full,
    /// Only the model is named; reasoning and mode show their marks.
    Model,
    Icons,
}

/// Quiet composer control: optional mark, muted label and a chevron, highlighted on
/// hover. Without a label it is a mark and a chevron, named by `name` for assistive
/// technology.
fn picker_button(
    id: &'static str,
    leading: Option<AnyElement>,
    label: Option<String>,
    name: String,
    color: Option<Hsla>,
    cx: &App,
) -> Button {
    let foreground = color.unwrap_or_else(t::muted);
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(foreground)
                .hover(t::hover()),
        )
        .xsmall()
        .compact()
        .h(px(26.))
        .px(px(8.))
        .rounded(px(13.))
        .border_0()
        .accessibility_label(name)
        .child(
            row()
                .gap(px(5.))
                .text_size(px(12.))
                .text_color(foreground)
                .children(leading)
                .children(label)
                .child(
                    icon(IconName::ChevronDown)
                        .size(px(10.))
                        .text_color(foreground),
                ),
        )
}
