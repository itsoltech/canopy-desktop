//! Nonsecret configuration intent. Immutable versions belong to the existing vault.
use super::{CommandMetadata, Risk};
use crate::{host, remote::settings_rpc::SettingsCommand};

pub(super) static REGISTRY: &[CommandMetadata] = &[
    metadata("settings.agent.save"),
    metadata("settings.agent.remove"),
    metadata("settings.profile.save"),
    metadata("settings.profile.remove"),
    metadata("settings.integration.save"),
    metadata("settings.integration.remove"),
];
const fn metadata(name: &'static str) -> CommandMetadata {
    CommandMetadata::new(name, Risk::Configuration, host::ADMIN)
}
impl SettingsCommand {
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[match self {
            Self::Agent(_) => 0,
            Self::RemoveAgent(_) => 1,
            Self::Profile(_) => 2,
            Self::RemoveProfile(_) => 3,
            Self::Integration(_) => 4,
            Self::RemoveIntegration(_) => 5,
        }]
    }
}
