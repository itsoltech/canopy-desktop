//! `/resume` (DEC-094): a closed chat comes back. The chat asks the workspace for the
//! list; an empty chat gives way to the one chosen.
use super::view::ChatView;
use crate::app_state::ChatResumeRequested;
use gpui_kit::*;

impl EventEmitter<ChatResumeRequested> for ChatView {}

impl ChatView {
    pub(super) fn request_resume(&mut self, cx: &mut Context<Self>) {
        let empty = !self.has_conversation() && !self.is_working();
        cx.emit(ChatResumeRequested {
            replace: empty.then_some(self.pane.id),
        });
    }
}
