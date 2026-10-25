//! Common runtime entry points. Native extension APIs remain on `ChatSession`;
//! ACP never pretends to be a legacy Provider or silently emulates an extension.
use super::{
    acp::{self, AcpSession},
    agent::{BackendKind, Capabilities, OptionChoice, OptionEffect, SessionOption},
    config::{Effort, Mode},
    process::Event,
    provider::Provider,
    session::{ChatSession, Draft, Incoming, Status},
    transcript::{Change, Transcript},
};
use serde_json::Value;

pub enum BackendSession {
    Native(Box<ChatSession>),
    Acp(Box<AcpSession>),
}
pub enum BackendDecision {
    Native(super::session::Response),
    /// Exact ACP option ID; None cancels the request.
    OptionId(Option<String>),
}
pub enum BackendIncoming {
    Native(Incoming),
    Acp(acp::wire::Message),
}
impl From<ChatSession> for BackendSession {
    fn from(session: ChatSession) -> Self {
        Self::Native(Box::new(session))
    }
}
impl From<AcpSession> for BackendSession {
    fn from(session: AcpSession) -> Self {
        Self::Acp(Box::new(session))
    }
}
impl BackendSession {
    pub fn kind(&self) -> BackendKind {
        match self {
            Self::Native(s) => match s.provider() {
                Provider::Claude => BackendKind::Claude,
                Provider::Codex => BackendKind::Codex,
            },
            Self::Acp(_) => BackendKind::Acp,
        }
    }
    /// Initialization metadata is complete, independently of readiness to send.
    pub fn catalog_ready(&self) -> bool {
        match self {
            Self::Native(s) => s.catalog_ready(),
            Self::Acp(s) => s.catalog_ready(),
        }
    }
    pub fn status(&self) -> &Status {
        match self {
            Self::Native(s) => s.status(),
            Self::Acp(s) => s.status(),
        }
    }
    pub fn transcript(&self) -> &Transcript {
        match self {
            Self::Native(s) => s.transcript(),
            Self::Acp(s) => s.transcript(),
        }
    }
    pub fn take_changes(&mut self) -> Vec<Change> {
        match self {
            Self::Native(s) => s.take_changes(),
            Self::Acp(s) => s.take_changes(),
        }
    }
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Native(s) => s.session_id(),
            Self::Acp(s) => s.session_id(),
        }
    }
    pub fn commands(&self) -> &[super::config::CommandInfo] {
        match self {
            Self::Native(s) => s.commands(),
            Self::Acp(s) => s.commands(),
        }
    }
    pub fn capabilities(&self) -> Capabilities {
        match self {
            Self::Acp(s) => s.capabilities(),
            Self::Native(s) => Capabilities {
                resume: true,
                images: true,
                embedded_context: true,
                cancel: true,
                steer: true,
                fork: true,
                rewind: s.provider() == Provider::Claude,
                goal: true,
            },
        }
    }
    pub fn options(&self) -> Vec<SessionOption> {
        match self {
            Self::Acp(s) => {
                let mut options = s.options().to_vec();
                for (id, label, category, value, choices) in [
                    (
                        "canopy:model",
                        "Model",
                        "model",
                        s.current_model(),
                        s.models(),
                    ),
                    ("canopy:mode", "Mode", "mode", s.current_mode(), s.modes()),
                ] {
                    if !choices.is_empty()
                        && !options
                            .iter()
                            .any(|o| o.category.as_deref() == Some(category))
                    {
                        options.push(SessionOption {
                            id: id.into(),
                            label: label.into(),
                            description: None,
                            category: Some(category.into()),
                            value: value.unwrap_or_default().into(),
                            choices: choices.to_vec(),
                            effect: OptionEffect::Immediate,
                        });
                    }
                }
                options
            }
            Self::Native(s) => {
                let effect = if s.provider() == Provider::Codex {
                    OptionEffect::NextTurn
                } else {
                    OptionEffect::Immediate
                };
                let modes = if s.provider() == Provider::Codex {
                    &Mode::CODEX[..]
                } else {
                    &Mode::CLAUDE[..]
                };
                let mut options = vec![SessionOption {
                    id: "mode".into(),
                    label: "Mode".into(),
                    description: None,
                    category: Some("mode".into()),
                    value: s.settings().mode.key().into(),
                    choices: modes
                        .iter()
                        .map(|mode| OptionChoice {
                            id: mode.key().into(),
                            label: mode.label().into(),
                            description: None,
                        })
                        .collect(),
                    effect,
                }];
                if !s.models().is_empty() {
                    options.push(SessionOption {
                        id: "model".into(),
                        label: "Model".into(),
                        description: None,
                        category: Some("model".into()),
                        value: s.settings().model.clone().unwrap_or_default(),
                        choices: s
                            .models()
                            .iter()
                            .map(|m| OptionChoice {
                                id: m.value.clone(),
                                label: m.label(),
                                description: Some(m.description.clone()),
                            })
                            .collect(),
                        effect,
                    });
                }
                if let Some(model) = s
                    .models()
                    .iter()
                    .find(|m| Some(m.value.as_str()) == s.settings().model.as_deref())
                    && !model.effort_levels.is_empty()
                {
                    options.push(SessionOption {
                        id: "effort".into(),
                        label: "Reasoning effort".into(),
                        description: None,
                        category: Some("thought_level".into()),
                        value: s
                            .settings()
                            .effort
                            .map(|e| e.key().into())
                            .unwrap_or_default(),
                        choices: model
                            .effort_levels
                            .iter()
                            .map(|e| OptionChoice {
                                id: e.key().into(),
                                label: e.label().into(),
                                description: None,
                            })
                            .collect(),
                        effect,
                    });
                }
                options
            }
        }
    }
    pub fn set_option(&mut self, id: &str, value: &str) -> Result<Vec<Value>, String> {
        if !self
            .options()
            .iter()
            .any(|o| o.id == id && o.choices.iter().any(|c| c.id == value))
        {
            return Err("The session does not offer this option value.".into());
        }
        match self {
            Self::Acp(s) => match id {
                "canopy:model" if !s.options().iter().any(|o| o.id == id) => s.set_model(value),
                "canopy:mode" if !s.options().iter().any(|o| o.id == id) => s.set_mode(value),
                _ => s.set_option(id, value),
            },
            Self::Native(s) => match id {
                "model" => Ok(s.set_model((!value.is_empty()).then(|| value.into()))),
                "mode" => Ok(s.set_mode(Mode::from_key(value).ok_or("Unknown mode.")?)),
                "effort" => {
                    Ok(s.set_effort(Some(Effort::from_key(value).ok_or("Unknown effort.")?)))
                }
                _ => Err("Unsupported option.".into()),
            },
        }
    }
    /// Dispatches one durably accepted message under its host message ID. The runtime
    /// owns ordering; this never queues. Err: nothing was written.
    pub fn submit(&mut self, draft: Draft, message_id: &str) -> Result<Vec<Value>, String> {
        match self {
            Self::Native(s) => s.submit_accepted(draft, message_id),
            Self::Acp(s) => s.submit(&draft, message_id),
        }
    }
    /// A durably accepted message can be dispatched now.
    pub fn ready_for_message(&self) -> bool {
        match self {
            Self::Native(s) => s.ready_for_message(),
            Self::Acp(s) => s.ready_for_message(),
        }
    }
    /// Evidence about dispatched messages: delivered, refused or uncertain.
    pub fn take_receipts(&mut self) -> Vec<(String, super::backend::Receipt)> {
        match self {
            Self::Native(s) => s.take_receipts(),
            Self::Acp(s) => s.take_receipts(),
        }
    }
    pub fn respond(
        &mut self,
        item: super::transcript::ItemId,
        decision: BackendDecision,
    ) -> Result<Vec<Value>, String> {
        match (self, decision) {
            (Self::Native(s), BackendDecision::Native(response)) => {
                if !s.pending_decisions().contains(&item) {
                    return Err("Permission is already resolved or belongs to another run.".into());
                }
                Ok(s.respond(item, response))
            }
            (Self::Acp(s), BackendDecision::OptionId(option)) => {
                Ok(vec![s.respond_permission(item, option.as_deref())?])
            }
            _ => Err("Decision does not match this backend's offered choices.".into()),
        }
    }
    pub fn interrupt(&mut self) -> Vec<Value> {
        match self {
            Self::Native(s) => s.interrupt(),
            Self::Acp(s) => s.interrupt(),
        }
    }
    pub fn steer(&mut self, draft: Draft) -> Result<Vec<Value>, String> {
        match self {
            Self::Native(s) => Ok(s.steer_draft(draft)),
            Self::Acp(_) => Err("ACP v1 does not support steering an active turn.".into()),
        }
    }
    pub fn on_event(&mut self, event: Event<BackendIncoming>) -> Result<Vec<Value>, String> {
        match (self, event) {
            (Self::Native(s), Event::Message(BackendIncoming::Native(m))) => {
                Ok(s.on_incoming(Event::Message(m)))
            }
            (Self::Acp(s), Event::Message(BackendIncoming::Acp(m))) => Ok(s.on_message(m)),
            (Self::Native(s), Event::Diagnostic(d)) => Ok(s.on_incoming(Event::Diagnostic(d))),
            (Self::Acp(s), Event::Diagnostic(d)) => Ok(s.on_event(Event::Diagnostic(d))),
            (Self::Native(s), Event::Exited(e)) => Ok(s.on_incoming(Event::Exited(e))),
            (Self::Acp(s), Event::Exited(e)) => Ok(s.on_event(Event::Exited(e))),
            _ => Err("Protocol event does not belong to this backend.".into()),
        }
    }
}
pub fn parse_claude(line: &[u8]) -> Result<Option<BackendIncoming>, String> {
    Ok(super::session::parse_claude(line)?.map(BackendIncoming::Native))
}
pub fn parse_codex(line: &[u8]) -> Result<Option<BackendIncoming>, String> {
    Ok(super::session::parse_codex(line)?.map(BackendIncoming::Native))
}
pub fn parse_acp(line: &[u8]) -> Result<Option<BackendIncoming>, String> {
    Ok(acp::wire::parse_line(line)?.map(BackendIncoming::Acp))
}
