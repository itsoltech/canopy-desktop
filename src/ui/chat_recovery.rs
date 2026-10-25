//! "Restore chat…": a conversation the host kept without its agent and account (its
//! history outlived its tab metadata) gets them back, chosen by the user. Nothing is
//! preselected from defaults; the conversation's own saved account is offered.
use super::{components::*, theme as t};
use crate::app_state::AppState;
use canopy_desktop::{
    motion::{self, Presence, presets},
    state::workspace::WorkspaceId,
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName, IndexPath,
        select::{SelectEvent, SelectState},
    },
    *,
};
use std::time::Instant;

/// The folder value of a new private folder (the other values are workspace IDs).
const PRIVATE: &str = "private";

pub struct ChatRecoveryDialog {
    chat: String,
    title: SharedString,
    agent: Entity<SelectState<Vec<SelectOption>>>,
    account: Entity<SelectState<Vec<SelectOption>>>,
    folder: Entity<SelectState<Vec<SelectOption>>>,
    focus: FocusHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    restoring: bool,
    error: Option<SharedString>,
    _events: Subscription,
}
impl EventEmitter<ModalDismissed> for ChatRecoveryDialog {}

/// Chat agents of the catalog, as the user configured them.
fn agents(cx: &App) -> Vec<SelectOption> {
    cx.global::<AppState>()
        .tools
        .read(cx)
        .catalog
        .tools
        .iter()
        .filter(|tool| tool.enabled && crate::app_state::backend_of(&tool.id, cx).is_some())
        .map(|tool| SelectOption::new(tool.id.clone(), tool.name.clone()))
        .collect()
}

/// Accounts of agent `tool`; an ACP agent without profiles is its own account.
fn accounts(tool: &str, cx: &App) -> Vec<SelectOption> {
    let catalog = &cx.global::<AppState>().tools.read(cx).catalog;
    let Some(tool) = catalog.get(tool) else {
        return vec![];
    };
    if tool.profiles.is_empty() {
        return vec![SelectOption::new(tool.id.clone(), tool.name.clone())];
    }
    tool.profiles
        .iter()
        .map(|p| SelectOption::new(p.id.clone(), p.name.clone()))
        .collect()
}

impl ChatRecoveryDialog {
    pub fn new(
        chat: String,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app = cx.global::<AppState>().clone();
        let record = app.settings.read(cx).chat_record(&chat).cloned();
        let saved_profile = record
            .as_ref()
            .and_then(|r| r.metadata().profile_id.clone());
        let title = record
            .as_ref()
            .and_then(|r| r.metadata().title.clone())
            .unwrap_or_else(|| format!("Conversation {}", &chat[..chat.len().min(8)]));
        let agents = agents(cx);
        // The agent whose account this conversation last ran with, if it still exists.
        let agent_index = saved_profile.as_ref().and_then(|profile| {
            let catalog = &app.tools.read(cx).catalog;
            agents.iter().position(|option| {
                catalog
                    .get(&option.value)
                    .is_some_and(|tool| tool.profiles.iter().any(|p| &p.id == profile))
            })
        });
        let tool = agent_index.map(|i| agents[i].value.to_string());
        let account_items = tool
            .as_deref()
            .map(|tool| accounts(tool, cx))
            .unwrap_or_default();
        let account_index = saved_profile.as_ref().and_then(|profile| {
            account_items
                .iter()
                .position(|o| o.value.as_ref() == profile)
        });
        let mut folders = vec![SelectOption::new(PRIVATE, "New private folder")];
        let projects = app.projects.read(cx);
        folders.extend(projects.catalog.items.iter().filter_map(|project| {
            let value = serde_json::to_string(&project.workspace).ok()?;
            let path = project.worktree_path.as_deref().unwrap_or(&project.path);
            Some(SelectOption::new(
                value,
                format!("{} · {}", project.name, path.display()),
            ))
        }));
        let agent =
            cx.new(|cx| SelectState::new(agents, agent_index.map(IndexPath::new), window, cx));
        let account = cx.new(|cx| {
            SelectState::new(account_items, account_index.map(IndexPath::new), window, cx)
        });
        let folder = cx.new(|cx| SelectState::new(folders, None, window, cx));
        // Accounts follow the chosen agent; the previous account choice doesn't carry.
        let events = cx.subscribe_in(
            &agent,
            window,
            |this, _, event: &SelectEvent<Vec<SelectOption>>, window, cx| {
                let SelectEvent::Confirm(value) = event;
                let items = value
                    .as_deref()
                    .map(|tool| accounts(tool, cx))
                    .unwrap_or_default();
                this.account.update(cx, |account, cx| {
                    account.set_items(items, window, cx);
                    account.set_selected_index(None, window, cx);
                });
                this.error = None;
                cx.notify();
            },
        );
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        let focus = cx.focus_handle();
        let initial = focus.clone();
        window.on_next_frame(move |window, cx| initial.focus(window, cx));
        Self {
            chat,
            title: title.into(),
            agent,
            account,
            folder,
            focus,
            return_focus,
            presence,
            closing: false,
            scheduled: false,
            restoring: false,
            error: None,
            _events: events,
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if self.restoring {
            return;
        }
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.restoring || self.presence.is_animating(Instant::now()) {
            return;
        }
        let tool = self
            .agent
            .read(cx)
            .selected_value()
            .map(ToString::to_string);
        let profile = self
            .account
            .read(cx)
            .selected_value()
            .map(ToString::to_string);
        let folder = self
            .folder
            .read(cx)
            .selected_value()
            .map(ToString::to_string);
        let (Some(tool), Some(profile), Some(folder)) = (tool, profile, folder) else {
            self.error = Some("Choose the agent, account and folder.".into());
            cx.notify();
            return;
        };
        let workspace = if folder == PRIVATE {
            None
        } else {
            match serde_json::from_str::<WorkspaceId>(&folder) {
                Ok(id) => Some(id),
                Err(_) => return,
            }
        };
        self.restoring = true;
        self.error = None;
        cx.notify();
        let restore = cx
            .global::<AppState>()
            .chats
            .clone()
            .update(cx, |chats, cx| {
                chats.recover(self.chat.clone(), tool, profile, workspace, cx)
            });
        cx.spawn(async move |this, cx| {
            let result = restore.await;
            let _ = this.update(cx, |this, cx| {
                this.restoring = false;
                match result {
                    Ok(()) => this.close(cx),
                    Err(error) => {
                        this.error = Some(error.into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

impl Render for ChatRecoveryDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animated = self.presence.is_animating(now);
        motion::request_frame(window, animated);
        if self.closing && !animated && !self.scheduled {
            self.scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.return_focus.focus(window, cx);
                    cx.emit(ModalDismissed);
                });
            });
        }
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(0., 440.);
        let disabled = self.closing || animated || self.restoring;
        let panel = modal::modal_surface("chat-recovery-dialog")
            .absolute()
            .w(px(width))
            .left((window.viewport_size().width - px(width)) / 2.)
            .top(px(96. + motion::distance::BASE * (1. - progress)))
            .opacity(progress)
            .child(modal::modal_header(
                format!("Restore “{}”", self.title),
                icon_button("close-chat-recovery", IconName::Close, "Close dialog")
                    .disabled(self.restoring)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ))
            .child(
                div()
                    .text_color(t::secondary())
                    .child("Its history is kept, but the agent and account it ran with are unknown. Choose them to continue the conversation."),
            )
            .child(modal::modal_field(
                "Agent",
                dropdown(&self.agent).placeholder("Choose an agent").disabled(disabled),
                None,
            ))
            .child(modal::modal_field(
                "Account",
                dropdown(&self.account).placeholder("Choose an account").disabled(disabled),
                None,
            ))
            .child(modal::modal_field(
                "Folder",
                dropdown(&self.folder).placeholder("Choose a folder").disabled(disabled),
                None,
            ))
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(t::red()).child(error)),
            )
            .child(modal::modal_actions(
                button("cancel-chat-recovery", "Cancel")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                primary_button("confirm-chat-recovery", "Restore")
                    .disabled(self.closing || animated)
                    .loading(self.restoring)
                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
            ));
        let ok = cx.listener(|this, _, _, cx| this.submit(cx));
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(false)
            .on_ok(move |e, w, cx| {
                ok(e, w, cx);
                false
            })
            .on_cancel(move |e, w, cx| {
                cancel(e, w, cx);
                false
            })
            .backdrop(modal::modal_backdrop(progress))
            .popup(panel)
    }
}
