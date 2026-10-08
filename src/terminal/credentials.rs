//! Blocking credential operations: invoke only from a worker/background executor.
const SERVICE: &str = "tech.itsol.canopy.rust.agent-api-keys";

pub fn store(id: &str, value: &str) -> Result<(), String> {
    crate::platform::credentials::store(SERVICE, id, value).map_err(|_| {
        format!(
            "Could not save the API key in {}.",
            crate::platform::credentials::store_name()
        )
    })
}

pub fn load(id: &str) -> Result<String, String> {
    crate::platform::credentials::load(SERVICE, id).map_err(|_| {
        format!(
            "API key is unavailable in {}. Save it again in Preferences.",
            crate::platform::credentials::store_name()
        )
    })
}

pub fn remove(id: &str) -> Result<(), String> {
    crate::platform::credentials::remove(SERVICE, id).map_err(|_| {
        format!(
            "Could not remove the API key from {}.",
            crate::platform::credentials::store_name()
        )
    })
}
