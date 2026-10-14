//! Fork under an agent reply (DEC-092/093): the conversation up to it goes on in a new
//! chat, in a project the user picks, with the agent's session branched off there.
use super::{rows::Source, view::ChatView};
use crate::app_state::{ChatForkRequested, ForkRequest};
use canopy_desktop::chat::{fork::Cut, provider::Provider, transcript::Body};
use gpui_kit::*;

impl EventEmitter<ChatForkRequested> for ChatView {}

impl ChatView {
    /// What forking at the turn end `end` takes; `None` when that part has no saved
    /// session to branch, or the turn no point to branch at.
    pub(super) fn fork_request(&self, end: Source) -> Option<ForkRequest> {
        if self.history_off {
            return None;
        }
        let source = self.settings.id.clone()?;
        let transcript = self.session.as_ref().map(|session| session.transcript());
        let item = self.rows.item(end, transcript)?;
        let Body::TurnEnd(turn_end) = &item.body else {
            return None;
        };
        let at = turn_end.resume_at.clone()?;
        let (epoch, agent) = match end {
            Source::Current(_) => (self.epoch, self.epoch_meta.agent.clone()),
            Source::Archived { epoch, .. } => {
                let archived = self.rows.archived.get(epoch)?;
                (archived.epoch, archived.agent.clone())
            }
        };
        let agent = agent?;
        let session = agent.session.clone()?;
        let provider = Provider::from_tool(&agent.tool)?;
        let mut settings = agent.settings.unwrap_or_else(|| self.settings.clone());
        settings.id = None;
        Some(ForkRequest {
            source,
            cut: Cut {
                epoch,
                end: item.id,
                at,
            },
            provider,
            profile: agent.profile,
            settings,
            session,
            title: self
                .pane
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| super::NEW_CHAT_TITLE.to_owned()),
        })
    }

    /// Fork clicked: the workspace asks where the new chat goes.
    pub(super) fn fork_reply(&mut self, end: Source, cx: &mut Context<Self>) {
        if let Some(request) = self.fork_request(end) {
            cx.emit(ChatForkRequested(request));
        }
    }
}
