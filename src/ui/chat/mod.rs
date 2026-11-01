//! Chat pane: a conversation with a headless coding agent (Claude Code).
mod actions;
mod activity;
mod agent_options;
mod attachments;
mod code_blocks;
mod composer;
mod decision;
mod details;
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
mod remote;
mod render;
mod resting;
mod resume;
mod rows;
mod scroller;
mod shell;
mod subagent;
mod suggest;
mod tools;
mod turn_diff;
mod view;
pub(crate) use page::serve_previews;
pub(crate) use page::theme_css;
pub(crate) use remote::ClientFileRequested;
pub use turn_diff::TurnDiffView;
pub use view::ChatView;

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

/// Chat shortcuts come from the command keymap (Settings → Shortcuts).
pub(crate) fn init(cx: &mut gpui_kit::App) {
    crate::ui::keymap::init(cx);
}
