use super::*;
use crate::{integrations::Config, settings::Preferences, state::tools::ToolCatalog};
use std::collections::BTreeMap;

/// BUS-04's nonsecret typed patch; omitted keys are preserved, None resets one key.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreferenceWrite {
    pub key: String,
    pub value: Option<String>,
}
impl PreferenceWrite {
    pub(super) fn from_changes(changes: Vec<Change>) -> Result<Vec<Self>> {
        let mut writes = changes
            .into_iter()
            .map(|change| {
                let (key, value) = change.encoded();
                Self {
                    key: key.as_str().into(),
                    value,
                }
            })
            .collect();
        Self::normalize(&mut writes)?;
        Ok(writes)
    }
    pub(crate) fn decode(&self) -> Result<Change> {
        let key = self.key.parse()?;
        self.value
            .as_ref()
            .map_or(Ok(Change::Reset(key)), |value| Change::parse(key, value))
    }
    pub(super) fn normalize(writes: &mut Vec<Self>) -> Result<()> {
        if writes.len() > 64 {
            return Err(Error::BatchTooLarge);
        }
        let mut canonical = BTreeMap::new();
        for write in writes.iter() {
            let (key, value) = write.decode()?.encoded();
            canonical.insert(key.as_str().to_owned(), value);
        }
        *writes = canonical
            .into_iter()
            .map(|(key, value)| Self { key, value })
            .collect();
        Ok(())
    }
}
/// The existing startup-random combined settings revision, shared by local writes
/// and the control bridge. Device-local appearance, terminal and shortcuts retain
/// their existing exclusion. Cleanup bookkeeping does not change the revision.
pub struct SettingsRevision {
    revision: u64,
    snapshot: Option<(Preferences, ToolCatalog, Config)>,
}
impl Default for SettingsRevision {
    fn default() -> Self {
        Self {
            revision: u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap())
                .max(1),
            snapshot: None,
        }
    }
}
pub(crate) fn shared_preferences(mut values: Preferences) -> Preferences {
    values.appearance = Default::default();
    values.terminal = Default::default();
    values.shortcuts = Default::default();
    values
}
impl SettingsRevision {
    pub fn observe(
        &mut self,
        preferences: Preferences,
        mut tools: ToolCatalog,
        mut integrations: Config,
    ) -> u64 {
        tools.retired_credentials.clear();
        integrations.retired_credentials.clear();
        let current = (shared_preferences(preferences), tools, integrations);
        if self.snapshot.as_ref().is_some_and(|old| old != &current) {
            self.revision = self.revision.wrapping_add(1).max(1);
        }
        self.snapshot = Some(current);
        self.revision
    }
}
