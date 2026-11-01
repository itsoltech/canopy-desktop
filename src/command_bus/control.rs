//! Stable runtime intentions. Effects stay with the existing thread owner.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DecisionAnswer {
    Allow,
    AllowForThread,
    Deny(Option<String>),
    Answer(Vec<String>),
    ApprovePlan { accept_edits: bool },
    KeepPlanning(Option<String>),
    Option(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Control {
    Start,
    Interrupt {
        run: u64,
    },
    Restart {
        run: u64,
    },
    Steer {
        run: u64,
        message: String,
        draft: Option<DraftIntent>,
    },
    Respond {
        run: u64,
        item: u64,
        answer: DecisionAnswer,
    },
    SetOption {
        id: String,
        value: String,
    },
    SetAgent {
        tool: String,
        profile: String,
        model: Option<String>,
    },
    SetGoal {
        objective: String,
    },
    PauseGoal,
    ResumeGoal,
    ClearGoal,
    StopBackground {
        run: u64,
        call: String,
    },
    Edit {
        run: u64,
        history_epoch: u64,
        item: u64,
        draft: DraftIntent,
        restore_files: bool,
    },
    RunShell {
        run: u64,
        command: String,
    },
    StopShell {
        run: u64,
        history_epoch: u64,
        item: u64,
    },
    Fork {
        history_epoch: u64,
        item: u64,
        replacement: Option<DraftIntent>,
    },
}
impl Control {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Start => "chat.start",
            Self::Interrupt { .. } => "chat.interrupt",
            Self::Restart { .. } => "chat.restart",
            Self::Steer { .. } => "chat.steer",
            Self::Respond { .. } => "chat.respond",
            Self::SetOption { .. } => "chat.set_option",
            Self::SetAgent { .. } => "chat.set_agent",
            Self::SetGoal { .. } => "chat.set_goal",
            Self::PauseGoal => "chat.pause_goal",
            Self::ResumeGoal => "chat.resume_goal",
            Self::ClearGoal => "chat.clear_goal",
            Self::StopBackground { .. } => "chat.stop_background",
            Self::Edit { .. } => "chat.edit",
            Self::RunShell { .. } => "chat.shell",
            Self::StopShell { .. } => "chat.stop_shell",
            Self::Fork { .. } => "chat.fork",
        }
    }
    pub(super) fn draft(&self) -> Option<&DraftIntent> {
        match self {
            Self::Steer { draft, .. } => draft.as_ref(),
            Self::Edit { draft, .. } => Some(draft),
            Self::Fork { replacement, .. } => replacement.as_ref(),
            _ => None,
        }
    }
    pub(super) fn normalize(&mut self) -> Result<()> {
        match self {
            Self::Steer { message, draft, .. } => {
                if !host::valid_id(message) {
                    return Err(Error::InvalidRuntimeState);
                }
                if let Some(draft) = draft {
                    draft.normalize()?;
                }
            }
            Self::Edit { draft, .. } => draft.normalize()?,
            Self::Fork {
                replacement: Some(draft),
                ..
            } => draft.normalize()?,
            Self::SetGoal { objective }
                if objective.trim().is_empty() || objective.len() > host::CONTROL_BYTES =>
            {
                return Err(Error::InvalidRuntimeState);
            }
            Self::RunShell { command, .. } => {
                *command = command.trim().into();
                if command.is_empty()
                    || command.len() > host::CONTROL_BYTES
                    || command.contains('\0')
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            _ => (),
        }
        Ok(())
    }
}

pub(super) const REGISTRY: &[CommandMetadata] = &[
    metadata("chat.start", Risk::Delivery, host::CHAT),
    metadata("chat.interrupt", Risk::Delivery, host::CHAT),
    metadata("chat.restart", Risk::Delivery, host::CHAT),
    metadata("chat.steer", Risk::Delivery, host::CHAT),
    metadata("chat.respond", Risk::Delivery, host::APPROVE),
    metadata("chat.set_option", Risk::Configuration, host::CHAT),
    metadata("chat.set_agent", Risk::Configuration, host::CHAT),
    metadata("chat.set_goal", Risk::Delivery, host::CHAT),
    metadata("chat.pause_goal", Risk::Delivery, host::CHAT),
    metadata("chat.resume_goal", Risk::Delivery, host::CHAT),
    metadata("chat.clear_goal", Risk::Delivery, host::CHAT),
    metadata("chat.stop_background", Risk::Delivery, host::CHAT),
    metadata("chat.edit", Risk::Delivery, host::CHAT),
    metadata("chat.shell", Risk::Delivery, host::CHAT | host::TERMINAL),
    metadata(
        "chat.stop_shell",
        Risk::Delivery,
        host::CHAT | host::TERMINAL,
    ),
    metadata("chat.fork", Risk::Delivery, host::CHAT),
];
const fn metadata(name: &'static str, risk: Risk, scope: u32) -> CommandMetadata {
    CommandMetadata {
        name,
        risk,
        scope,
        workspace_attachment_scope: host::READ,
    }
}
