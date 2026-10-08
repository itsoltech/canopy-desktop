//! Bounded SSH credential fallback. Reads only bounded key metadata to classify encryption;
//! private-key contents and passphrases are never logged or returned.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

const KEY_METADATA_LIMIT: u64 = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyProtection {
    Encrypted,
    Unencrypted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct KeyCandidate {
    pub path: PathBuf,
    pub public_path: Option<PathBuf>,
    pub protection: KeyProtection,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Attempt {
    Agent,
    KeyFile(KeyCandidate),
}

pub(super) struct SshCredentials {
    attempts: std::vec::IntoIter<Attempt>,
    candidates: Vec<KeyCandidate>,
    config_present: bool,
}

impl SshCredentials {
    pub fn new(home: Option<&Path>) -> Self {
        let mut attempts = vec![Attempt::Agent];
        let mut candidates = Vec::new();
        if let Some(home) = home {
            // Security-key identities require an agent; libssh2 cannot perform
            // their hardware interaction. Custom IdentityFile entries are not
            // read from OpenSSH config and must also be loaded into an agent.
            for name in ["id_rsa", "id_ecdsa", "id_ed25519"] {
                let path = home.join(".ssh").join(name);
                if path.is_file() {
                    let public_path = path.with_file_name(format!("{name}.pub"));
                    let candidate = KeyCandidate {
                        protection: key_protection(&path),
                        public_path: public_path.is_file().then_some(public_path),
                        path,
                    };
                    attempts.push(Attempt::KeyFile(candidate.clone()));
                    candidates.push(candidate);
                }
            }
        }
        let config_present = home.is_some_and(|home| home.join(".ssh/config").is_file());
        Self {
            attempts: attempts.into_iter(),
            candidates,
            config_present,
        }
    }

    pub fn next(&mut self) -> Option<Attempt> {
        self.attempts.next()
    }

    pub fn candidates(&self) -> &[KeyCandidate] {
        &self.candidates
    }

    pub fn config_present(&self) -> bool {
        self.config_present
    }
}

fn key_protection(path: &Path) -> KeyProtection {
    let Ok(file) = std::fs::File::open(path) else {
        return KeyProtection::Unknown;
    };
    let Ok(metadata) = file.metadata() else {
        return KeyProtection::Unknown;
    };
    if metadata.len() > KEY_METADATA_LIMIT {
        return KeyProtection::Unknown;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    if file
        .take(KEY_METADATA_LIMIT + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > KEY_METADATA_LIMIT
    {
        return KeyProtection::Unknown;
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return KeyProtection::Unknown;
    };
    if text.contains("BEGIN ENCRYPTED PRIVATE KEY") || text.contains("Proc-Type: 4,ENCRYPTED") {
        return KeyProtection::Encrypted;
    }
    if text.contains("BEGIN OPENSSH PRIVATE KEY") {
        return openssh_protection(text);
    }
    if [
        "BEGIN PRIVATE KEY",
        "BEGIN RSA PRIVATE KEY",
        "BEGIN EC PRIVATE KEY",
        "BEGIN DSA PRIVATE KEY",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        return KeyProtection::Unencrypted;
    }
    KeyProtection::Unknown
}

fn openssh_protection(text: &str) -> KeyProtection {
    let encoded = text
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();
    let Ok(decoded) = STANDARD.decode(encoded) else {
        return KeyProtection::Unknown;
    };
    const MAGIC: &[u8] = b"openssh-key-v1\0";
    let Some(payload) = decoded.strip_prefix(MAGIC) else {
        return KeyProtection::Unknown;
    };
    let Some(length) = payload.get(..4) else {
        return KeyProtection::Unknown;
    };
    let length = u32::from_be_bytes(length.try_into().unwrap()) as usize;
    let Some(cipher) = payload.get(4..4 + length) else {
        return KeyProtection::Unknown;
    };
    if cipher == b"none" {
        KeyProtection::Unencrypted
    } else {
        KeyProtection::Encrypted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tries_agent_then_each_existing_identity_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".ssh")).unwrap();
        for name in [
            "id_rsa",
            "id_rsa.pub",
            "id_ed25519",
            "custom",
            "id_ecdsa.pub",
        ] {
            std::fs::write(dir.path().join(".ssh").join(name), "test placeholder").unwrap();
        }
        let mut credentials = SshCredentials::new(Some(dir.path()));
        assert_eq!(credentials.next(), Some(Attempt::Agent));
        assert_eq!(
            credentials.next(),
            Some(Attempt::KeyFile(KeyCandidate {
                path: dir.path().join(".ssh/id_rsa"),
                public_path: Some(dir.path().join(".ssh/id_rsa.pub")),
                protection: KeyProtection::Unknown,
            }))
        );
        assert_eq!(
            credentials.next(),
            Some(Attempt::KeyFile(KeyCandidate {
                path: dir.path().join(".ssh/id_ed25519"),
                public_path: None,
                protection: KeyProtection::Unknown,
            }))
        );
        assert_eq!(credentials.next(), None);
        assert_eq!(credentials.next(), None);
    }

    #[test]
    fn missing_home_or_keys_does_not_retry_agent_forever() {
        let mut credentials = SshCredentials::new(None);
        assert_eq!(credentials.next(), Some(Attempt::Agent));
        assert_eq!(credentials.next(), None);
    }

    #[test]
    fn classifies_pem_and_openssh_encryption_without_exposing_key_material() {
        let dir = tempfile::tempdir().unwrap();
        let encrypted = dir.path().join("encrypted");
        std::fs::write(
            &encrypted,
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nfixture\n",
        )
        .unwrap();
        assert_eq!(key_protection(&encrypted), KeyProtection::Encrypted);

        for (name, cipher, expected) in [
            ("plain", b"none".as_slice(), KeyProtection::Unencrypted),
            (
                "protected",
                b"aes256-ctr".as_slice(),
                KeyProtection::Encrypted,
            ),
        ] {
            let mut payload = b"openssh-key-v1\0".to_vec();
            payload.extend_from_slice(&(cipher.len() as u32).to_be_bytes());
            payload.extend_from_slice(cipher);
            let path = dir.path().join(name);
            std::fs::write(
                &path,
                format!(
                    "-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----\n",
                    STANDARD.encode(payload)
                ),
            )
            .unwrap();
            assert_eq!(key_protection(&path), expected);
        }
    }
}
