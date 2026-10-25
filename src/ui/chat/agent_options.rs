//! The composer's control for an ACP agent: its own options (configuration, model,
//! mode) exactly as the agent offers them, and before the first message the other
//! chat agents to switch to. Claude and Codex keep their model and mode pickers.
use super::{
    composer::{Density, agent_icon, picker_button},
    view::ChatView,
};
use crate::app_state::AgentOption;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::*;

impl ChatView {
    pub(super) fn agent_options(
        &self,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let runtime = self.rt(cx);
        let options = runtime.agent_options();
        let started = runtime.backend().is_some_and(|s| {
            !matches!(s.status(), canopy_desktop::chat::session::Status::Starting)
                && !s.status().ended()
        });
        let agent = self.agent_name(cx);
        // The model is what people look for first; else the first option the agent lists.
        let shown = options
            .iter()
            .find(|option| option.category.as_deref() == Some("model"))
            .or(options.first())
            .and_then(|option| {
                option
                    .choices
                    .iter()
                    .find(|choice| choice.id == option.value)
                    .map(|choice| choice.label.clone())
            });
        let name = match &shown {
            Some(value) => format!("{agent} · {value}"),
            None => agent.clone(),
        };
        let (accounts, current) = self.accounts(cx);
        let locked = self.account_locked(cx);
        let view = cx.entity().downgrade();
        let trigger = picker_button(
            "chat-agent-options",
            Some(agent_icon(None, &agent)),
            (density != Density::Icons).then(|| shown.clone().unwrap_or(agent.clone())),
            format!("Agent options: {name}"),
            None,
            cx,
        );
        let switch: Vec<_> = accounts
            .into_iter()
            .filter(|_| !locked)
            .map(|account| {
                (
                    account.tool,
                    account.id.clone(),
                    account.name,
                    account.id == current,
                )
            })
            .collect();
        trigger.tooltip(name).dropdown_menu_with_anchor(
            Anchor::BottomLeft,
            move |mut menu, _, _| {
                for (position, option) in options.iter().enumerate() {
                    if position > 0 {
                        menu = menu.separator();
                    }
                    menu = menu.label(option_heading(option));
                    for choice in &option.choices {
                        let (view, id, value) =
                            (view.clone(), option.id.clone(), choice.id.clone());
                        menu = menu.item(
                            PopupMenuItem::new(choice.label.clone())
                                .checked(choice.id == option.value)
                                .disabled(!started)
                                .on_click(move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.runtime.update(cx, |runtime, cx| {
                                            runtime.set_agent_option(&id, &value, cx)
                                        })
                                    });
                                }),
                        );
                    }
                }
                if options.is_empty() {
                    menu = menu.label(if started {
                        "This agent offers no options"
                    } else {
                        "Its options show once it starts"
                    });
                }
                if !switch.is_empty() {
                    menu = menu.separator().label("Agent");
                    for (tool, profile, label, chosen) in &switch {
                        let (view, tool, profile) = (view.clone(), tool.clone(), profile.clone());
                        menu =
                            menu.item(PopupMenuItem::new(label.clone()).checked(*chosen).on_click(
                                move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.set_account(tool.clone(), profile.clone(), None, cx)
                                    });
                                },
                            ));
                    }
                }
                menu.min_w(px(220.)).max_h(px(420.)).scrollable(true)
            },
        )
    }
}

/// An option's name, and when its choice takes effect if not at once.
fn option_heading(option: &AgentOption) -> String {
    use canopy_desktop::chat::agent::OptionEffect;
    match option.effect {
        OptionEffect::Restart => format!("{} · applies after Restart", option.label),
        OptionEffect::NextTurn => format!("{} · applies from the next turn", option.label),
        OptionEffect::Immediate => option.label.clone(),
    }
}
