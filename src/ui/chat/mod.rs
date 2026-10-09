//! Chat pane: a conversation with a headless coding agent (Claude Code).
mod actions;
mod attachments;
mod composer;
mod decision;
mod dev_bench; // dev_bench: remove together with dev_bench.rs
mod edit;
mod navigator;
mod queue;
mod render;
mod rows;
mod scroller;
mod subagent;
mod tools;
mod turn_diff;
mod view;
pub use turn_diff::TurnDiffView;
pub use view::{ChatStateChanged, ChatView};

/// Tab title until the first message names the chat.
pub const NEW_CHAT_TITLE: &str = "New chat";

gpui_kit::actions!(
    chat_actions,
    [
        StopChat,
        AllowPending,
        DenyPending,
        NewSession,
        PreviousMessage,
        NextMessage
    ]
);

pub(crate) fn init(cx: &mut gpui_kit::App) {
    cx.bind_keys([
        gpui_kit::KeyBinding::new("cmd-.", StopChat, Some("Chat")),
        // From the composer, cmd-enter arrives as a secondary PressEnter instead.
        gpui_kit::KeyBinding::new("cmd-enter", AllowPending, Some("Chat")),
        gpui_kit::KeyBinding::new("escape", DenyPending, Some("Chat")),
        gpui_kit::KeyBinding::new("cmd-k", NewSession, Some("Chat")),
        gpui_kit::KeyBinding::new("alt-cmd-up", PreviousMessage, Some("Chat")),
        gpui_kit::KeyBinding::new("alt-cmd-down", NextMessage, Some("Chat")),
    ]);
}
