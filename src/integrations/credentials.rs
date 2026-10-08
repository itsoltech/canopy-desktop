//! Blocking system credential operations for integrations, on background executors only.
const SERVICE: &str = "tech.itsol.canopy.rust.integrations";

pub fn store(id: &str, token: &str) -> Result<(), String> {
    crate::platform::credentials::store(SERVICE, id, token).map_err(|_| {
        format!(
            "Could not save the integration credential in {}.",
            crate::platform::credentials::store_name()
        )
    })
}

pub fn load(id: &str) -> Result<String, String> {
    crate::platform::credentials::load(SERVICE, id).map_err(|_| {
        format!(
            "Integration credential is unavailable in {}. Reconnect in Preferences → Integrations.",
            crate::platform::credentials::store_name()
        )
    })
}

pub fn remove(id: &str) -> Result<(), String> {
    crate::platform::credentials::remove(SERVICE, id).map_err(|_| {
        format!(
            "Could not remove the integration credential from {}.",
            crate::platform::credentials::store_name()
        )
    })
}
