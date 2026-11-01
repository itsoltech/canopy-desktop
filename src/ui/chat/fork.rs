//! Fork under an agent reply in a destination offered by its environment.
use super::{rows::Source, view::ChatView};
use crate::app_state::{ChatForkRequested, HistoryPosition};
use gpui_kit::*;

impl EventEmitter<ChatForkRequested> for ChatView {}

impl ChatView {
    pub(super) fn fork_reply(&mut self, end: Source, cx: &mut Context<Self>) {
        let history_epoch = match end {
            Source::Current(_) => self.rt(cx).epoch(),
            Source::Archived { epoch, .. } => self.rows.archived[epoch].epoch,
        };
        let Some(item) = self.rows.item(end, self.transcript(cx)) else {
            return;
        };
        let position = HistoryPosition {
            history_epoch,
            item_id: item.id.0,
        };
        if self.rt(cx).can_fork_at(position, cx) {
            cx.emit(ChatForkRequested {
                runtime: self.runtime.clone(),
                position,
            });
        }
    }
}
