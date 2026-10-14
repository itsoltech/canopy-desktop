//! What is left of switching agents mid-conversation, withdrawn by DEC-087 (the agent
//! is chosen before the first message only): each part of a conversation keeps the
//! agent that ran it, and messages sent with a switch's context still show it.
use super::view::ChatView;
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::history::AgentRef;
use gpui_kit::*;

/// "4.2 KB"
pub(super) fn size_label(text: &str) -> String {
    format!("{:.1} KB", text.len() as f32 / 1024.)
}

impl ChatView {
    /// Who runs the current session, as saved with it (DEC-065).
    pub(super) fn agent_ref(&self) -> AgentRef {
        let mut settings = self.settings.clone();
        // The chat's id and rules stay with the chat, not with one agent.
        settings.id = None;
        settings.allowed.clear();
        AgentRef {
            tool: self.pane.tool.clone(),
            profile: self.pane.metadata.profile_id.clone(),
            session: self.pane.metadata.resume_id.clone(),
            settings: Some(settings),
        }
    }

    /// Saves the running session's agent again once its session id or choices change.
    pub(super) fn record_meta(&mut self, cx: &mut Context<Self>) {
        if self.history_off
            || self
                .session
                .as_ref()
                .is_none_or(|session| session.status().ended())
        {
            return;
        }
        let agent = Some(self.agent_ref());
        let goal = self.session.as_ref().and_then(|s| s.goal()).cloned();
        if self.epoch_meta.agent == agent && self.epoch_meta.goal == goal {
            return;
        }
        self.epoch_meta.agent = agent;
        self.epoch_meta.goal = goal;
        let Some(chat) = self.settings.id.clone() else {
            return;
        };
        let (epoch, meta) = (self.epoch, self.epoch_meta.clone());
        cx.global::<AppState>()
            .chat_history
            .clone()
            .update(cx, |history, cx| {
                history.record(&chat, epoch, Some(meta), [], false, cx)
            });
    }
}

/// The context exactly as the agent gets it, scrollable.
pub(super) fn context_text(id: impl Into<ElementId>, text: String) -> NestedScroller {
    div()
        .id(id)
        .w_full()
        .max_h(px(240.))
        .overflow_y_scroll()
        .p(px(8.))
        .rounded(px(8.))
        .bg(t::bg())
        .text_size(px(12.))
        .text_color(t::secondary())
        .child(text)
        .nested_scroll()
}
