//! Composer island: message field, account/model, reasoning and mode pickers, Send/Stop.
use super::{render::centered, view::ChatView};
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{
    config::{Effort, Mode, ModelInfo},
    provider::Provider,
    transcript::PermissionKind,
};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::Textarea,
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::*;
use std::time::Instant;

/// Accounts are the Claude Code and Codex profiles configured in Preferences.
struct Account {
    provider: Provider,
    id: String,
    name: String,
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
        centered(
            column()
                .w_full()
                .p(px(10.))
                .gap(px(6.))
                .rounded(px(16.))
                .border_1()
                .border_color(t::control_border())
                .bg(t::input_bg())
                // Number keys pick an option and ⌘⌫ denies while a request waits.
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if this.decision_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .child({
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
                })
                .children(self.attachment_errors(cx))
                .child(
                    row()
                        .w_full()
                        .gap(px(2.))
                        .child(self.model_picker(cx))
                        .children(self.effort_picker(cx))
                        .child(self.mode_picker(cx))
                        .children(super::actions::context_ring(self))
                        .child(div().flex_1())
                        .child(
                            Button::new("chat-attach")
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
                                .on_click(cx.listener(|this, _, _, cx| this.pick_attachments(cx))),
                        )
                        .children(deciding.then(|| self.decision_buttons(empty, cx)))
                        // While a request waits, its own buttons answer it; ⌘. still stops.
                        .children((!deciding).then(|| {
                            self.action_button(
                                working,
                                empty || ended || self.attachments_blocked(cx),
                                cx,
                            )
                        })),
                ),
        )
        .pt(px(4.))
        .pb(px(16.))
    }

    /// Claude Code and Codex profiles, and the chat's own.
    fn accounts(&self, cx: &App) -> (Vec<Account>, String) {
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
    fn default_model_label(models: &[ModelInfo]) -> String {
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

    fn models(&self) -> Vec<ModelInfo> {
        self.session
            .as_ref()
            .map(|session| session.models().to_vec())
            .unwrap_or_default()
    }

    /// The model the choices apply to; `None` means the account's default entry.
    fn current_model(&self) -> Option<ModelInfo> {
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

    fn model_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (accounts, current) = self.accounts(cx);
        let models = self.models();
        let model = self.settings.model.clone();
        let (account_name, model_name) = self.selection_label(cx);
        let ongoing = self.account_locked();
        let can_switch = self.can_switch_agent();
        let view = cx.entity().downgrade();
        let picker = picker_button(
            "chat-model",
            Some(provider_icon(self.provider())),
            format!("{account_name} · {model_name}"),
            None,
            cx,
        );
        let picker = picker.tooltip("Account and model");
        picker.dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
            for (position, account) in accounts.iter().enumerate() {
                if position > 0 {
                    menu = menu.separator();
                }
                menu = menu.label(account.name.clone());
                if account.id == current {
                    let pick = |value: Option<String>| {
                        let view = view.clone();
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            let _ = view.update(cx, |this, cx| this.set_model(value.clone(), cx));
                        }
                    };
                    let default = Self::default_model_label(&models);
                    menu = menu.item(
                        PopupMenuItem::new(default)
                            .checked(model.is_none())
                            .on_click(pick(None)),
                    );
                    for info in models.iter().filter(|m| m.value != "default") {
                        menu = menu.item(
                            PopupMenuItem::new(info.label())
                                .checked(model.as_deref() == Some(info.value.as_str()))
                                .on_click(pick(Some(info.value.clone()))),
                        );
                    }
                } else {
                    let view = view.clone();
                    let id = account.id.clone();
                    let provider = account.provider;
                    menu = menu.item(
                        PopupMenuItem::new(match (ongoing, can_switch) {
                            (false, _) => format!("Use {} (restarts this chat)", account.name),
                            (true, true) => {
                                format!("Switch to {} — continues with context", account.name)
                            }
                            (true, false) => {
                                format!("Switch to {} once the agent is done", account.name)
                            }
                        })
                        .disabled(ongoing && !can_switch)
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.choose_account(provider, id.clone(), cx)
                            });
                        }),
                    );
                }
            }
            menu.min_w(px(240.)).max_h(px(360.)).scrollable(true)
        })
    }

    fn effort_picker(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let model = self.current_model()?;
        if !model.supports_effort() {
            return None;
        }
        let effort = self.settings.effort;
        let label = effort.map_or("Reasoning", Effort::label);
        let view = cx.entity().downgrade();
        Some(
            picker_button(
                "chat-effort",
                Some(custom_icon("brain").size(px(13.)).into_any_element()),
                label.to_owned(),
                None,
                cx,
            )
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                menu = menu.label("Reasoning");
                let choice = |value: Option<Effort>| {
                    let view = view.clone();
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        let _ = view.update(cx, |this, cx| this.set_effort(value, cx));
                    }
                };
                menu = menu.item(
                    PopupMenuItem::new("Default")
                        .checked(effort.is_none())
                        .on_click(choice(None)),
                );
                for level in &model.effort_levels {
                    menu = menu.item(
                        PopupMenuItem::new(level.label())
                            .checked(effort == Some(*level))
                            .on_click(choice(Some(*level))),
                    );
                }
                menu.min_w(px(160.))
            }),
        )
    }

    fn mode_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.settings.mode;
        let allowed = self.settings.allowed.clone();
        // Without the model list yet, every mode stays offered; the CLI validates.
        let codex = self.provider() == Provider::Codex;
        let auto = codex || self.current_model().is_none_or(|model| model.supports_auto);
        let view = cx.entity().downgrade();
        let warning = mode.unguarded().then(t::yellow);
        let options: &'static [Mode] = if codex { &Mode::CODEX } else { &Mode::CLAUDE };
        picker_button(
            "chat-mode",
            Some(
                mode_icon(mode)
                    .text_color(warning.unwrap_or_else(t::muted))
                    .into_any_element(),
            ),
            mode.label().to_owned(),
            warning,
            cx,
        )
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

/// Quiet composer control: optional mark, muted label and a chevron, highlighted on hover.
fn picker_button(
    id: &'static str,
    leading: Option<AnyElement>,
    label: String,
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
        .accessibility_label(label.clone())
        .child(
            row()
                .gap(px(5.))
                .text_size(px(12.))
                .text_color(foreground)
                .children(leading)
                .child(label)
                .child(
                    icon(IconName::ChevronDown)
                        .size(px(10.))
                        .text_color(foreground),
                ),
        )
}
