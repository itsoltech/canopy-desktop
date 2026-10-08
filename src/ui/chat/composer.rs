//! Composer island: message field, account/model, reasoning and mode pickers, Send/Stop.
use super::{render::centered, view::ChatView};
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::config::{Effort, Mode, ModelInfo};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::Textarea,
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::*;
use std::time::Instant;

const ACCOUNT_LOCKED: &str = "Switching accounts in an ongoing chat isn't available yet.";

/// Accounts are the Claude Code profiles configured in Preferences.
struct Account {
    id: String,
    name: String,
}

impl ChatView {
    pub(super) fn composer_island(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let _ = window;
        let working = self.is_working();
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
                .child(
                    Textarea::new(&self.composer)
                        .appearance(false)
                        .disabled(ended)
                        .w_full()
                        .text_size(px(14.))
                        .px(px(4.)),
                )
                .child(
                    row()
                        .w_full()
                        .gap(px(2.))
                        .child(self.model_picker(cx))
                        .children(self.effort_picker(cx))
                        .child(self.mode_picker(cx))
                        .child(div().flex_1())
                        .child(super::dev_bench::button(cx, ended)) // dev_bench
                        .child(self.action_button(working, empty || ended, cx)),
                ),
        )
        .pt(px(4.))
        .pb(px(16.))
    }

    fn accounts(&self, cx: &App) -> (Vec<Account>, String) {
        let catalog = &cx.global::<AppState>().tools.read(cx).catalog;
        let Some(tool) = catalog.get("claude") else {
            return (vec![], String::new());
        };
        let accounts: Vec<_> = tool
            .profiles
            .iter()
            .map(|profile| Account {
                id: profile.id.clone(),
                name: if tool.profiles.len() == 1 {
                    "Claude Code".into()
                } else {
                    format!("Claude {}", profile.name)
                },
            })
            .collect();
        let current = self
            .pane
            .metadata
            .profile_id
            .clone()
            .or(tool.default_profile.clone())
            .unwrap_or_default();
        (accounts, current)
    }

    fn models(&self) -> Vec<ModelInfo> {
        self.session
            .as_ref()
            .map(|session| session.models().to_vec())
            .unwrap_or_default()
    }

    /// The model the choices apply to; `None` means the account's default entry.
    fn current_model(&self) -> Option<ModelInfo> {
        let value = self.settings.model.as_deref().unwrap_or("default");
        self.models().into_iter().find(|model| model.value == value)
    }

    /// Account and model names shown in the picker and the empty state.
    pub(super) fn selection_label(&self, cx: &App) -> (String, String) {
        let (accounts, current) = self.accounts(cx);
        let account = accounts
            .into_iter()
            .find(|account| account.id == current)
            .map(|account| account.name)
            .unwrap_or_else(|| "Claude Code".into());
        let models = self.models();
        let model = match &self.settings.model {
            None => models
                .iter()
                .filter(|m| m.value == "default")
                .map(ModelInfo::label)
                .next()
                .unwrap_or_else(|| "Default model".to_owned()),
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
        let locked = self.account_locked();
        let view = cx.entity().downgrade();
        let picker = picker_button(
            "chat-model",
            // Codex accounts get their own mark in stage 7.
            Some(tool_icon("Claude Code", IconName::Bot)),
            format!("{account_name} · {model_name}"),
            None,
            cx,
        );
        let picker = if locked && accounts.len() > 1 {
            picker.tooltip(ACCOUNT_LOCKED)
        } else {
            picker.tooltip("Account and model")
        };
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
                    let default = models
                        .iter()
                        .filter(|m| m.value == "default")
                        .map(ModelInfo::label)
                        .next()
                        .unwrap_or_else(|| "Default model".to_owned());
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
                    menu = menu.item(
                        PopupMenuItem::new(if locked {
                            ACCOUNT_LOCKED.to_owned()
                        } else {
                            format!("Use {} (restarts this chat)", account.name)
                        })
                        .disabled(locked)
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| this.set_account(id.clone(), cx));
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
        // Without the model list yet, every mode stays offered; the CLI validates.
        let auto = self.current_model().is_none_or(|model| model.supports_auto);
        let view = cx.entity().downgrade();
        let warning = (mode == Mode::Yolo).then(t::yellow);
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
            for option in Mode::ALL {
                if option == Mode::Auto && !auto {
                    continue;
                }
                let view = view.clone();
                let label = match option {
                    Mode::Yolo => "Yolo — runs everything without asking",
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
            menu.min_w(px(240.))
        })
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
        Mode::Yolo => icon(IconName::TriangleAlert),
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
