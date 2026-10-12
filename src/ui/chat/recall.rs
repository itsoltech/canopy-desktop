//! ↑ in an empty composer brings back earlier messages of the conversation with their
//! badges; ↓ goes forward again (DEC-075). Editing the text ends it.
use super::view::ChatView;
use canopy_desktop::chat::{
    session::Draft,
    transcript::{Attachment, Body, Item},
};
use gpui_kit::*;

pub(super) struct Recall {
    /// Position among the messages, newest first.
    index: usize,
    /// The composer's text as restored: any other text ends the recall.
    text: String,
}

impl ChatView {
    /// Messages the user wrote in the main conversation, newest first (earlier
    /// sessions included; `!` command results aren't messages).
    fn written(&self) -> Vec<Draft> {
        let transcript = self.session.as_ref().map(|s| s.transcript());
        let message = |item: &Item| match &item.body {
            Body::User(message)
                if item.parent.is_none()
                    && !message
                        .attachments
                        .iter()
                        .any(|a| matches!(a, Attachment::Shell { .. })) =>
            {
                Some(Draft {
                    text: message.text.clone(),
                    attachments: message.attachments.clone(),
                    context: None,
                    goal: false,
                })
            }
            _ => None,
        };
        let archived = self
            .rows
            .archived
            .iter()
            .flat_map(|archived| archived.transcript.items().iter());
        let current = transcript.into_iter().flat_map(|t| t.items().iter());
        let mut drafts: Vec<Draft> = archived.chain(current).filter_map(message).collect();
        drafts.reverse();
        drafts.dedup_by(|a, b| a.text == b.text && a.attachments == b.attachments);
        drafts
    }

    /// ↑ (`older`) / ↓ in the composer; `true` when it moved through earlier messages.
    pub(super) fn recall(
        &mut self,
        older: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shell || self.goal_mode {
            return false;
        }
        let key = if older { "up" } else { "down" };
        let text = self.composer.read(cx).value().to_string();
        let browsing = self.recall.as_ref().is_some_and(|r| r.text == text);
        if !(browsing || key == "up" && text.is_empty()) {
            return false;
        }
        let drafts = self.written();
        let next = match (&self.recall, key) {
            (Some(recall), "up") if browsing => recall.index + 1,
            (None, "up") | (Some(_), "up") => 0,
            (Some(recall), _) if recall.index > 0 => recall.index - 1,
            // Past the newest: back to an empty composer.
            _ => {
                self.recall = None;
                self.set_composer_draft(Draft::default(), window, cx);
                return true;
            }
        };
        let Some(draft) = drafts.get(next).cloned() else {
            // Nothing older: stay on the oldest.
            return browsing || !drafts.is_empty();
        };
        self.set_composer_draft(draft, window, cx);
        self.recall = Some(Recall {
            index: next,
            text: self.composer.read(cx).value().to_string(),
        });
        true
    }

    /// The composer changed: an edit of a recalled message makes it a new one.
    pub(super) fn end_recall(&mut self, cx: &App) {
        if self
            .recall
            .as_ref()
            .is_some_and(|recall| recall.text != self.composer.read(cx).value().as_ref())
        {
            self.recall = None;
        }
    }
}
