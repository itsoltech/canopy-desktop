//! Terminal lifecycle intent; PTY input remains a fenced streaming protocol.
use super::{CommandMetadata, Risk};
use crate::{
    host,
    settings::{Error, Result},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TerminalCommand {
    Open {
        tool: String,
        profile: Option<String>,
    },
    Acquire {
        terminal: String,
        take_over: bool,
    },
    Release {
        terminal: String,
        run: String,
        fencing_token: u64,
    },
    Stop {
        terminal: String,
        run: String,
    },
    Restart {
        terminal: String,
        run: String,
    },
}
pub(super) static REGISTRY: &[CommandMetadata] = &[
    metadata("terminal.open"),
    metadata("terminal.acquire"),
    metadata("terminal.release"),
    metadata("terminal.stop"),
    metadata("terminal.restart"),
];
const fn metadata(name: &'static str) -> CommandMetadata {
    CommandMetadata::new(name, Risk::Delivery, host::TERMINAL)
}
impl TerminalCommand {
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[match self {
            Self::Open { .. } => 0,
            Self::Acquire { .. } => 1,
            Self::Release { .. } => 2,
            Self::Stop { .. } => 3,
            Self::Restart { .. } => 4,
        }]
    }
    pub(super) fn normalize(&mut self) -> Result<()> {
        match self {
            Self::Open { tool, profile } => {
                if tool.is_empty()
                    || tool.len() > 256
                    || profile.as_ref().is_some_and(|p| p.len() > 256)
                {
                    return Err(Error::InvalidRuntimeState);
                }
            }
            Self::Acquire { terminal, .. } => validate_id(terminal)?,
            Self::Release { terminal, run, .. }
            | Self::Stop { terminal, run }
            | Self::Restart { terminal, run } => {
                validate_id(terminal)?;
                validate_id(run)?;
            }
        }
        Ok(())
    }
}
fn validate_id(value: &str) -> Result<()> {
    if !host::valid_id(value) {
        return Err(Error::InvalidRuntimeState);
    }
    Ok(())
}
