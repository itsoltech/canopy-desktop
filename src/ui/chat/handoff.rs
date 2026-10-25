//! What is left of switching agents mid-conversation, withdrawn by DEC-087 (the agent
//! is chosen before the first message only): each part of a conversation keeps the
//! agent that ran it, and messages sent with a switch's context still show it.
use crate::ui::{components::*, theme as t};
use gpui_kit::*;

/// "4.2 KB"
pub(super) fn size_label(text: &str) -> String {
    format!("{:.1} KB", text.len() as f32 / 1024.)
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
