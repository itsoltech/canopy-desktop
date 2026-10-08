//! Chat pane: a conversation with a headless coding agent (Claude Code).
mod composer;
mod queue;
mod render;
mod rows;
mod subagent;
mod view;
pub use view::{ChatStateChanged, ChatView};

/// Tab title until the first message names the chat.
pub const NEW_CHAT_TITLE: &str = "New chat";

gpui_kit::actions!(chat_actions, [StopChat, AllowPending, DenyPending]);

pub(crate) fn init(cx: &mut gpui_kit::App) {
    cx.bind_keys([
        gpui_kit::KeyBinding::new("cmd-.", StopChat, Some("Chat")),
        // From the composer, cmd-enter arrives as a secondary PressEnter instead.
        gpui_kit::KeyBinding::new("cmd-enter", AllowPending, Some("Chat")),
        gpui_kit::KeyBinding::new("escape", DenyPending, Some("Chat")),
    ]);
}
