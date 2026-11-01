//! Host administration uses the same admission and transactional writer as other commands.
use super::{CommandMetadata, Risk};
use crate::{
    host,
    settings::{Error, Result},
};
use serde::{Deserialize, Serialize};

pub(super) static REGISTRY: &[CommandMetadata] = &[CommandMetadata::new(
    "host.revoke",
    Risk::Configuration,
    host::ADMIN,
)];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AdminCommand {
    RevokeDevice { device: String },
}
impl AdminCommand {
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[0]
    }
    pub(super) fn normalize(&mut self) -> Result<()> {
        let Self::RevokeDevice { device } = self;
        *device = uuid::Uuid::parse_str(device)
            .map_err(|_| Error::InvalidRuntimeState)?
            .to_string();
        Ok(())
    }
}
