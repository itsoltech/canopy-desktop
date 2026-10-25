//! App-owned forwarding. No subprocess, ssh config, agent forwarding or known_hosts mutation.
use super::store::{CredentialStore, SshRoute};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

pub struct SshTunnel {
    address: String,
    cancelled: Arc<AtomicBool>,
}
impl SshTunnel {
    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn open(
        route: SshRoute,
        endpoint: &str,
        vault: Arc<dyn CredentialStore>,
    ) -> Result<Self, String> {
        let target = url::Url::parse(endpoint).map_err(|_| "Invalid tunnel destination.")?;
        let host = target
            .host_str()
            .ok_or("Missing tunnel destination.")?
            .to_owned();
        let port = target
            .port_or_known_default()
            .ok_or("Missing tunnel port.")?;
        // Verify trust and credentials before advertising a working local route.
        let session = authenticate(&route, vault.as_ref())?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = format!(
            "https://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        std::thread::Builder::new()
            .name("canopy-client-ssh".into())
            .spawn(move || {
                let active = Arc::new(AtomicUsize::new(0));
                let mut first = Some(session);
                while !stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((socket, _)) if active.load(Ordering::Acquire) < 8 => {
                            active.fetch_add(1, Ordering::AcqRel);
                            let active = active.clone();
                            let stop = stop.clone();
                            let route = route.clone();
                            let vault = vault.clone();
                            let host = host.clone();
                            let initial = first.take();
                            std::thread::spawn(move || {
                                let session = initial
                                    .map(Ok)
                                    .unwrap_or_else(|| authenticate(&route, vault.as_ref()));
                                if let Ok(session) = session {
                                    let _ = forward(session, socket, &host, port, &stop);
                                }
                                active.fetch_sub(1, Ordering::AcqRel);
                            });
                        }
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { address, cancelled })
    }
}
impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
fn handshake(address: &str, port: u16) -> Result<ssh2::Session, String> {
    let address = (address, port)
        .to_socket_addrs()
        .map_err(|_| "SSH address could not be resolved.")?
        .next()
        .ok_or("SSH address could not be resolved.")?;
    let socket = TcpStream::connect_timeout(&address, Duration::from_secs(10))
        .map_err(|_| "SSH connection failed.")?;
    let mut session = ssh2::Session::new().map_err(|_| "SSH initialization failed.")?;
    session.set_tcp_stream(socket);
    session.set_timeout(10000);
    session.handshake().map_err(|_| "SSH handshake failed.")?;
    Ok(session)
}
fn fingerprint(session: &ssh2::Session) -> Result<String, String> {
    let (key, _) = session
        .host_key()
        .ok_or("SSH server did not provide a host key.")?;
    Ok(format!(
        "SHA256:{}",
        STANDARD_NO_PAD.encode(Sha256::digest(key))
    ))
}
/// A probe returns public material only. The UI must obtain explicit trust before saving it.
pub fn probe(address: &str, port: u16) -> Result<String, String> {
    fingerprint(&handshake(address, port)?)
}
/// Store bounded, chunked key material in the OS vault; route records contain only its reference.
pub fn save_credentials(
    vault: &dyn CredentialStore,
    reference: &str,
    secret: &str,
    passphrase: Option<&str>,
) -> Result<(), String> {
    let bytes =
        serde_json::to_vec(&(secret, passphrase)).map_err(|_| "Invalid SSH credentials.")?;
    if bytes.len() > 65536 {
        return Err("SSH credentials exceed 64 KiB.".into());
    }
    let encoded = STANDARD_NO_PAD.encode(bytes);
    let chunks = encoded.as_bytes().chunks(2000).collect::<Vec<_>>();
    for (index, chunk) in chunks.iter().enumerate() {
        if vault
            .save(
                &format!("{reference}:ssh:{index}"),
                std::str::from_utf8(chunk).unwrap(),
            )
            .is_err()
        {
            for cleanup in 0..=index {
                let _ = vault.remove(&format!("{reference}:ssh:{cleanup}"));
            }
            return Err("Could not save SSH credentials in the device vault.".into());
        }
    }
    if vault
        .save(reference, &format!("canopy-ssh-v1:{}", chunks.len()))
        .is_err()
    {
        for index in 0..chunks.len() {
            let _ = vault.remove(&format!("{reference}:ssh:{index}"));
        }
        let _ = vault.remove(reference);
        return Err("Could not save SSH credential manifest.".into());
    }
    Ok(())
}
pub fn remove_credentials(vault: &dyn CredentialStore, reference: &str) -> Result<(), String> {
    let manifest = vault.load(reference)?;
    let mut failed = false;
    if let Some(count) = manifest
        .strip_prefix("canopy-ssh-v1:")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n <= 44)
    {
        for index in 0..count {
            failed |= vault.remove(&format!("{reference}:ssh:{index}")).is_err();
        }
    }
    if failed {
        return Err("Could not remove every SSH credential slot.".into());
    }
    vault.remove(reference)
}
fn load_credentials(
    vault: &dyn CredentialStore,
    reference: &str,
) -> Result<(String, Option<String>), String> {
    let manifest = vault.load(reference)?;
    let Some(count) = manifest.strip_prefix("canopy-ssh-v1:") else {
        return Ok((manifest, None));
    };
    let count = count
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=44).contains(n))
        .ok_or("Invalid SSH credential manifest.")?;
    let mut encoded = String::new();
    for index in 0..count {
        let chunk = vault.load(&format!("{reference}:ssh:{index}"))?;
        if chunk.len() > 2000 {
            return Err("Invalid SSH credential slot.".into());
        }
        encoded.push_str(&chunk);
    }
    let bytes = STANDARD_NO_PAD
        .decode(encoded)
        .map_err(|_| "Invalid SSH credentials.")?;
    serde_json::from_slice(&bytes).map_err(|_| "Invalid SSH credentials.".into())
}
fn authenticate(route: &SshRoute, vault: &dyn CredentialStore) -> Result<ssh2::Session, String> {
    let session = handshake(&route.address, route.port)?;
    if fingerprint(&session)? != route.fingerprint {
        return Err(
            "SSH host key changed. Verify its fingerprint before trusting the new key.".into(),
        );
    }
    let (secret, passphrase) = load_credentials(vault, &route.credential_ref)?;
    if secret.starts_with("-----BEGIN") {
        session
            .userauth_pubkey_memory(&route.username, None, &secret, passphrase.as_deref())
            .map_err(|_| "SSH private-key authentication failed.")?;
    } else {
        session
            .userauth_password(&route.username, &secret)
            .map_err(|_| "SSH password authentication failed.")?;
    }
    if !session.authenticated() {
        return Err("SSH authentication failed.".into());
    }
    Ok(session)
}
fn forward(
    session: ssh2::Session,
    mut socket: TcpStream,
    host: &str,
    port: u16,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let mut channel = session
        .channel_direct_tcpip(host, port, None)
        .map_err(|_| "SSH server refused forwarding.")?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    session.set_blocking(false);
    let mut to_remote = Vec::new();
    let mut to_local = Vec::new();
    let mut buffer = [0; 16384];
    let mut local_eof = false;
    let mut remote_eof = false;
    let mut eof_sent = false;
    while !cancelled.load(Ordering::Acquire) {
        for (reader, pending, eof) in [
            (&mut socket as &mut dyn Read, &mut to_remote, &mut local_eof),
            (
                &mut channel as &mut dyn Read,
                &mut to_local,
                &mut remote_eof,
            ),
        ] {
            if !*eof && pending.len() < 65536 {
                match reader.read(&mut buffer) {
                    Ok(0) => *eof = true,
                    Ok(n) => pending.extend_from_slice(&buffer[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Ok(()),
                }
            }
        }
        for (writer, pending) in [
            (&mut channel as &mut dyn Write, &mut to_remote),
            (&mut socket as &mut dyn Write, &mut to_local),
        ] {
            if !pending.is_empty() {
                match writer.write(pending) {
                    Ok(0) => return Ok(()),
                    Ok(n) => {
                        pending.drain(..n);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Ok(()),
                }
            }
        }
        if local_eof && to_remote.is_empty() && !eof_sent {
            match channel.send_eof() {
                Ok(()) => eof_sent = true,
                Err(e) if e.code() == ssh2::ErrorCode::Session(-37) => {}
                Err(_) => break,
            }
        }
        if remote_eof && to_local.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    let _ = channel.close();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Mutex};
    #[derive(Default)]
    struct Vault {
        values: Mutex<BTreeMap<String, String>>,
        fail_slot: Option<usize>,
    }
    impl CredentialStore for Vault {
        fn load(&self, id: &str) -> Result<String, String> {
            self.values
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or("Missing fake credential.".into())
        }
        fn save(&self, id: &str, value: &str) -> Result<(), String> {
            assert!(value.len() <= 2560);
            self.values.lock().unwrap().insert(id.into(), value.into());
            if self
                .fail_slot
                .is_some_and(|slot| id.ends_with(&format!(":ssh:{slot}")))
            {
                Err("Injected failure after write.".into())
            } else {
                Ok(())
            }
        }
        fn remove(&self, id: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(id);
            Ok(())
        }
    }
    #[test]
    fn long_encrypted_keys_fit_vault_slots_and_partial_writes_are_removed() {
        let key = format!(
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\n{}",
            "sample".repeat(1200)
        );
        let vault = Vault::default();
        save_credentials(&vault, "key", &key, Some("test passphrase")).unwrap();
        let loaded = load_credentials(&vault, "key").unwrap();
        assert!(loaded.0 == key && loaded.1.as_deref() == Some("test passphrase"));
        remove_credentials(&vault, "key").unwrap();
        assert!(vault.values.lock().unwrap().is_empty());
        let vault = Vault {
            fail_slot: Some(2),
            ..Default::default()
        };
        assert!(save_credentials(&vault, "key", &key, None).is_err());
        assert!(vault.values.lock().unwrap().is_empty());
    }
}
