//! Host-discovered PR context is signed as a bounded, expiring capability for
//! that checkout. The signature authenticates data; filesystem and chat access
//! still require the current device grant at acceptance.
use crate::api::proto;
use hmac::{Hmac, Mac};
use prost::Message;
use sha2::Sha256;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tonic::Status;

#[derive(Clone)]
pub struct PullContexts(Arc<[u8; 32]>);
impl Default for PullContexts {
    fn default() -> Self {
        let mut key = [0; 32];
        key[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        key[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        Self(Arc::new(key))
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl PullContexts {
    fn mac(&self, root: &str, expires: u64, pull: &proto::PullAttachment) -> Hmac<Sha256> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.0.as_slice()).expect("HMAC key length");
        mac.update(b"canopy/pr-context/v1\0");
        mac.update(&(root.len() as u64).to_le_bytes());
        mac.update(root.as_bytes());
        mac.update(&expires.to_le_bytes());
        mac.update(&pull.encode_to_vec());
        mac
    }
    pub fn publish(&self, root: &str, mut pull: proto::PullAttachment) -> proto::PullAttachment {
        pull.context_id = None;
        let expires = now() + 15 * 60;
        let hash = self.mac(root, expires, &pull).finalize().into_bytes();
        let signature: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        pull.context_id = Some(format!("{expires}.{signature}"));
        pull
    }
    pub fn resolve(
        &self,
        root: &str,
        mut pull: proto::PullAttachment,
    ) -> Result<proto::PullAttachment, Status> {
        let token = pull.context_id.take().ok_or_else(|| {
            Status::failed_precondition("Select the pull request from the host's catalog.")
        })?;
        let invalid = || {
            Status::failed_precondition("Pull request context expired or changed. Select it again.")
        };
        if token.len() > 96 || pull.encoded_len() > 64 * 1024 {
            return Err(invalid());
        }
        let (expires, signature) = token.split_once('.').ok_or_else(invalid)?;
        let expires: u64 = expires.parse().map_err(|_| invalid())?;
        if expires < now() || expires > now() + 15 * 60 {
            return Err(invalid());
        }
        if signature.len() != 64 || !signature.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let hash: Result<Vec<u8>, _> = (0..signature.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&signature[i..i + 2], 16))
            .collect();
        self.mac(root, expires, &pull)
            .verify_slice(&hash.map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
        Ok(pull)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_context_rejects_changed_body_checkout_expiry_and_unicode() {
        let contexts = PullContexts::default();
        let original = proto::PullAttachment {
            number: 42,
            body: "source body".into(),
            ..Default::default()
        };
        let signed = contexts.publish("/project", original.clone());
        assert_eq!(
            contexts.resolve("/project", signed.clone()).unwrap(),
            original
        );
        assert!(contexts.resolve("/other", signed.clone()).is_err());
        let mut changed = signed.clone();
        changed.body.push('!');
        assert!(contexts.resolve("/project", changed).is_err());
        let mut expired = original.clone();
        let expiry = now().saturating_sub(1);
        let hash = contexts
            .mac("/project", expiry, &expired)
            .finalize()
            .into_bytes();
        expired.context_id = Some(format!(
            "{expiry}.{}",
            hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ));
        assert!(contexts.resolve("/project", expired).is_err());
        let mut malformed = signed;
        malformed.context_id = Some(format!("{}.{}", now() + 60, "é".repeat(32)));
        assert!(contexts.resolve("/project", malformed).is_err());
        assert!(contexts.resolve("/project", original).is_err());
    }
}
