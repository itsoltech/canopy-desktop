use super::SecretVersion;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tonic::Status;
const SERVICE: &str = "tech.itsol.canopy.rust.remote-settings-staging";
/// Inject a fake in tests. Production access remains host-only and off the UI thread.
#[tonic::async_trait]
pub trait SettingsSecrets: Send + Sync + 'static {
    /// Stable keyed MAC. Losing the key MUST fail closed, never generate a new key.
    async fn fingerprint(&self, bytes: &[u8]) -> Result<[u8; 32], Status>;
    async fn stage(&self, version: &SecretVersion, value: &str) -> Result<(), Status>;
    async fn read(&self, version: &SecretVersion) -> Result<String, Status>;
    async fn remove(&self, version: &SecretVersion) -> Result<(), Status>;
}
pub struct PlatformSettingsSecrets {
    key: [u8; 32],
}
impl PlatformSettingsSecrets {
    /// The host loads/derives this key from its persisted vault identity. Do not
    /// use a random per-process key: retries must keep their original fingerprint.
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }
}
fn unavailable() -> Status {
    Status::failed_precondition(
        "needs_host_interaction: remote settings secret storage is unavailable; unlock the host credential store.",
    )
}
#[tonic::async_trait]
impl SettingsSecrets for PlatformSettingsSecrets {
    async fn fingerprint(&self, bytes: &[u8]) -> Result<[u8; 32], Status> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).map_err(|_| unavailable())?;
        mac.update(b"Canopy remote settings v1\0");
        mac.update(bytes);
        Ok(mac.finalize().into_bytes().into())
    }
    async fn stage(&self, version: &SecretVersion, value: &str) -> Result<(), Status> {
        let pieces = chunks(value);
        let ids = version.parts().map(str::to_owned).collect::<Vec<_>>();
        if pieces.len() != ids.len() {
            return Err(Status::invalid_argument("Invalid staged secret shape."));
        }
        let values = pieces
            .into_iter()
            .map(|s| {
                if s.is_empty() {
                    "remote-empty-value".into()
                } else {
                    s.to_owned()
                }
            })
            .collect::<Vec<String>>();
        tokio::task::spawn_blocking(move || {
            for (id, value) in ids.iter().zip(values) {
                crate::platform::credentials::store(SERVICE, id, &value)
                    .map_err(|_| unavailable())?;
            }
            Ok(())
        })
        .await
        .map_err(|_| unavailable())?
    }
    async fn read(&self, version: &SecretVersion) -> Result<String, Status> {
        let ids = version.parts().map(str::to_owned).collect::<Vec<_>>();
        let empty = version.is_empty();
        tokio::task::spawn_blocking(move || {
            let mut value = String::new();
            for id in ids {
                let part =
                    crate::platform::credentials::load(SERVICE, &id).map_err(|_| unavailable())?;
                if value.len() + part.len() > 65536 {
                    return Err(Status::resource_exhausted(
                        "Staged settings value is too large.",
                    ));
                }
                value.push_str(&part);
            }
            Ok(if empty { String::new() } else { value })
        })
        .await
        .map_err(|_| unavailable())?
    }
    async fn remove(&self, version: &SecretVersion) -> Result<(), Status> {
        let ids = version.parts().map(str::to_owned).collect::<Vec<_>>();
        tokio::task::spawn_blocking(move || {
            for id in ids {
                crate::platform::credentials::remove(SERVICE, &id).map_err(|_| unavailable())?;
            }
            Ok(())
        })
        .await
        .map_err(|_| unavailable())?
    }
}
/// OS credential limits apply to UTF8 bytes, never split inside a codepoint.
pub(super) fn chunks(mut value: &str) -> Vec<&str> {
    if value.is_empty() {
        return vec![value];
    }
    let mut result = Vec::new();
    while !value.is_empty() {
        let mut end = value.len().min(2560);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        let (head, tail) = value.split_at(end);
        result.push(head);
        value = tail;
    }
    result
}
