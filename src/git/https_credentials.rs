//! Exact, host-scoped HTTPS credential lookup. Tracker tokens are never consulted.
use super::Result;

#[derive(Debug, PartialEq, Eq)]
struct Lookup {
    target: String,
    host: String,
    port: Option<u16>,
    path: String,
    url_username: Option<String>,
}

fn lookup(url: &str, use_http_path: bool) -> Result<Lookup> {
    let parsed = url::Url::parse(url).map_err(|_| "Remote HTTPS URL is invalid.")?;
    if parsed.scheme() != "https"
        || parsed.host().is_none()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Remote credential lookup requires a plain HTTPS URL.".into());
    }
    let host = parsed.host_str().ok_or("Remote HTTPS host is missing.")?;
    let authority = match parsed.host().unwrap() {
        url::Host::Ipv6(address) => format!("[{address}]"),
        _ => host.to_owned(),
    };
    let port = parsed.port();
    let authority = match port {
        Some(port) => format!("{authority}:{port}"),
        None => authority,
    };
    let path = if use_http_path {
        parsed.path().to_owned()
    } else {
        String::new()
    };
    let target_path = path.trim_matches('/');
    let target = if target_path.is_empty() {
        format!("git:https://{authority}")
    } else {
        format!("git:https://{authority}/{target_path}")
    };
    let url_username = (!parsed.username().is_empty()).then(|| parsed.username().to_owned());
    Ok(Lookup {
        target,
        host: host.to_owned(),
        port,
        path,
        url_username,
    })
}

pub(super) fn load(
    url: &str,
    preferred_username: Option<&str>,
    use_http_path: bool,
) -> Result<(String, String)> {
    let lookup = lookup(url, use_http_path)?;
    backend::load(&lookup, preferred_username)
}

#[cfg(any(windows, test))]
fn matched_username(stored: Option<String>, requested: Option<&str>) -> Result<String> {
    let stored = stored
        .filter(|value| !value.is_empty())
        .ok_or("The matching HTTPS credential has no username.")?;
    if requested.is_some_and(|requested| requested != stored) {
        return Err("The matching HTTPS credential belongs to a different username.".into());
    }
    Ok(stored)
}

#[cfg(windows)]
pub(super) fn target_name(url: &str, use_http_path: bool) -> Option<String> {
    lookup(url, use_http_path).ok().map(|lookup| lookup.target)
}

#[cfg(target_os = "macos")]
mod backend {
    use super::*;
    use security_framework::{
        os::macos::passwords::{SecAuthenticationType, SecProtocolType},
        passwords::get_internet_password,
    };

    pub fn load(lookup: &Lookup, preferred_username: Option<&str>) -> Result<(String, String)> {
        let username = preferred_username
            .map(str::to_owned)
            .or_else(|| lookup.url_username.clone())
            .filter(|value| !value.is_empty())
            .ok_or("HTTPS credentials need a username in the remote URL or credential.username.")?;
        let password = get_internet_password(
            &lookup.host,
            None,
            &username,
            &lookup.path,
            lookup.port,
            SecProtocolType::HTTPS,
            SecAuthenticationType::Default,
        )
        .map_err(|_| "No matching HTTPS credential was found in macOS Keychain.")?;
        let password = String::from_utf8(password)
            .map_err(|_| "The matching HTTPS credential has invalid encoding.")?;
        if password.is_empty() || password.contains('\0') {
            Err("The matching HTTPS credential is empty or invalid.".into())
        } else {
            Ok((username, password))
        }
    }
}

#[cfg(windows)]
mod backend {
    use super::*;
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
    };

    pub fn load(lookup: &Lookup, preferred_username: Option<&str>) -> Result<(String, String)> {
        let target: Vec<_> = lookup.target.encode_utf16().chain(Some(0)).collect();
        let mut raw: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: target is terminated and output storage is valid.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0
            || raw.is_null()
        {
            return Err(
                "No matching HTTPS credential was found in Windows Credential Manager.".into(),
            );
        }
        // SAFETY: a successful CredReadW returns one CREDENTIALW allocation.
        let result = (|| -> Result<(String, String)> {
            // SAFETY: raw is a non-null CREDENTIALW returned by CredReadW.
            let credential = unsafe { &*raw };
            // SAFETY: UserName is either null or a NUL-terminated field in the same allocation.
            let stored_username = unsafe { wide_string(credential.UserName) }?;
            let requested = preferred_username.or(lookup.url_username.as_deref());
            let username = super::matched_username(stored_username, requested)?;
            let length = credential.CredentialBlobSize as usize;
            if length == 0 || length > 2_560 || credential.CredentialBlob.is_null() {
                Err("The matching HTTPS credential is empty or invalid.".to_owned())
            } else {
                // SAFETY: the non-null blob belongs to the CredReadW allocation and length is bounded.
                let bytes =
                    unsafe { std::slice::from_raw_parts(credential.CredentialBlob, length) };
                decode_secret(bytes).map(|password| (username, password))
            }
        })();
        // SAFETY: ownership of the returned allocation belongs to this caller.
        unsafe { CredFree(raw.cast()) };
        result
    }

    unsafe fn wide_string(value: *mut u16) -> Result<Option<String>> {
        if value.is_null() {
            return Ok(None);
        }
        let mut length = 0;
        // SAFETY: the pointer is a NUL-terminated field in the CredReadW allocation.
        while length < 32_768 && unsafe { *value.add(length) } != 0 {
            length += 1;
        }
        if length == 32_768 {
            return Err("The matching HTTPS credential username is invalid.".into());
        }
        // SAFETY: the loop found the terminating NUL within the allocation's documented field.
        let units = unsafe { std::slice::from_raw_parts(value, length) };
        String::from_utf16(units)
            .map(Some)
            .map_err(|_| "The matching HTTPS credential username has invalid encoding.".into())
    }

    fn decode_secret(bytes: &[u8]) -> Result<String> {
        if let Ok(value) = std::str::from_utf8(bytes)
            && !value.is_empty()
            && !value.contains('\0')
        {
            return Ok(value.to_owned());
        }
        if !bytes.len().is_multiple_of(2) {
            return Err("The matching HTTPS credential has invalid encoding.".into());
        }
        let mut units: Vec<_> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        if units.last() == Some(&0) {
            units.pop();
        }
        if units.is_empty() || units.contains(&0) {
            return Err("The matching HTTPS credential is empty or invalid.".into());
        }
        String::from_utf16(&units)
            .map_err(|_| "The matching HTTPS credential has invalid encoding.".into())
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod backend {
    use super::*;
    pub fn load(_: &Lookup, _: Option<&str>) -> Result<(String, String)> {
        Err("HTTPS credential storage is unavailable on this platform.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_target_is_exact_and_path_is_opt_in() {
        assert_eq!(
            lookup("https://user@example.com:8443/owner/repo.git", false)
                .unwrap()
                .target,
            "git:https://example.com:8443"
        );
        assert_eq!(
            lookup("https://user@example.com:8443/owner/repo.git", true)
                .unwrap()
                .target,
            "git:https://example.com:8443/owner/repo.git"
        );
        assert!(lookup("http://example.com/repo", false).is_err());
        assert!(lookup("https://user:secret@example.com/repo", false).is_err());
        assert_eq!(
            matched_username(Some("account-a".into()), Some("account-a")).unwrap(),
            "account-a"
        );
        assert!(matched_username(Some("account-a".into()), Some("account-b")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_reads_gcm_compatible_utf16_credential() {
        use windows_sys::Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredWriteW,
        };
        let id = uuid::Uuid::new_v4().to_string();
        let url = format!("https://canopy-{id}.invalid/repo.git");
        let lookup = lookup(&url, false).unwrap();
        let mut target: Vec<_> = lookup.target.encode_utf16().chain(Some(0)).collect();
        let mut username: Vec<_> = "git-user".encode_utf16().chain(Some(0)).collect();
        let mut secret = "test-token".encode_utf16().collect::<Vec<_>>();
        let mut credential: CREDENTIALW = unsafe { std::mem::zeroed() };
        credential.Type = CRED_TYPE_GENERIC;
        credential.TargetName = target.as_mut_ptr();
        credential.UserName = username.as_mut_ptr();
        credential.CredentialBlob = secret.as_mut_ptr().cast();
        credential.CredentialBlobSize = (secret.len() * 2) as u32;
        credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: all buffers remain valid for these synchronous credential calls.
        assert_ne!(unsafe { CredWriteW(&credential, 0) }, 0);
        assert_eq!(
            load(&url, None, false).unwrap(),
            ("git-user".into(), "test-token".into())
        );
        assert!(load(&url, Some("different-user"), false).is_err());
        assert_ne!(
            unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) },
            0
        );
    }
}
