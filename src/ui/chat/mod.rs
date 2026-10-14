//! Chat pane: a conversation with a headless coding agent (Claude Code).
mod actions;
mod activity;
mod attachments;
mod code_blocks;
mod composer;
mod decision;
mod edit;
mod find;
mod fork;
mod goal;
mod handoff;
mod mentions;
mod model_picker;
mod navigator;
mod page;
mod queue;
mod quote;
mod recall;
mod render;
mod resting;
mod rows;
mod scroller;
mod shell;
mod subagent;
mod suggest;
mod tools;
mod turn_diff;
mod view;
pub(crate) use page::serve_previews;
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
        PreviousMessage,
        NextMessage,
        FindInChat,
        FindNext,
        FindPrevious,
        CloseFind
    ]
);

pub(crate) fn init(cx: &mut gpui_kit::App) {
    cx.bind_keys([
        gpui_kit::KeyBinding::new("cmd-.", StopChat, Some("Chat")),
        // From the composer, cmd-enter arrives as a secondary PressEnter instead.
        gpui_kit::KeyBinding::new("cmd-enter", AllowPending, Some("Chat")),
        gpui_kit::KeyBinding::new("escape", DenyPending, Some("Chat")),
        gpui_kit::KeyBinding::new("alt-cmd-up", PreviousMessage, Some("Chat")),
        gpui_kit::KeyBinding::new("alt-cmd-down", NextMessage, Some("Chat")),
        gpui_kit::KeyBinding::new("cmd-f", FindInChat, Some("Chat")),
        gpui_kit::KeyBinding::new("cmd-g", FindNext, Some("Chat")),
        gpui_kit::KeyBinding::new("cmd-shift-g", FindPrevious, Some("Chat")),
        gpui_kit::KeyBinding::new("escape", CloseFind, Some("ChatFind")),
    ]);
}
