const MAX_SECRET_BYTES: usize = 2_560;

fn operation_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

pub fn store_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else {
        "the system credential store"
    }
}

pub fn store(service: &str, id: &str, value: &str) -> Result<(), String> {
    validate(service, id, value.as_bytes())?;
    let _guard = operation_lock()
        .lock()
        .map_err(|_| "Credential service is unavailable.".to_owned())?;
    backend::store(service, id, value.as_bytes())
}

pub fn load(service: &str, id: &str) -> Result<String, String> {
    validate_target(service, id)?;
    let _guard = operation_lock()
        .lock()
        .map_err(|_| "Credential service is unavailable.".to_owned())?;
    let bytes = backend::load(service, id)?;
    String::from_utf8(bytes).map_err(|_| "Credential has invalid UTF-8 encoding.".into())
}

pub fn remove(service: &str, id: &str) -> Result<(), String> {
    validate_target(service, id)?;
    let _guard = operation_lock()
        .lock()
        .map_err(|_| "Credential service is unavailable.".to_owned())?;
    backend::remove(service, id)
}

fn validate(service: &str, id: &str, value: &[u8]) -> Result<(), String> {
    validate_target(service, id)?;
    if value.is_empty() {
        return Err("Credential cannot be empty.".into());
    }
    if value.len() > MAX_SECRET_BYTES {
        return Err("Credential exceeds the system store limit of 2560 bytes.".into());
    }
    Ok(())
}

fn validate_target(service: &str, id: &str) -> Result<(), String> {
    if service.is_empty()
        || id.is_empty()
        || service.contains('\0')
        || id.contains(['\0', '/', '\\'])
        || service.len() + id.len() + 1 > 32_767
    {
        Err("Invalid credential identifier.".into())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod backend {
    pub fn store(service: &str, id: &str, value: &[u8]) -> Result<(), String> {
        security_framework::passwords::set_generic_password(service, id, value)
            .map_err(|_| "Could not save the credential in macOS Keychain.".into())
    }
    pub fn load(service: &str, id: &str) -> Result<Vec<u8>, String> {
        security_framework::passwords::get_generic_password(service, id)
            .map_err(|_| "Credential is unavailable in macOS Keychain.".into())
    }
    pub fn remove(service: &str, id: &str) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(service, id) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => Err("Could not remove the credential from macOS Keychain.".into()),
        }
    }
}

#[cfg(target_os = "windows")]
mod backend {
    use windows_sys::Win32::{
        Foundation::{ERROR_NOT_FOUND, GetLastError},
        Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
            CredReadW, CredWriteW,
        },
    };

    pub fn store(service: &str, id: &str, value: &[u8]) -> Result<(), String> {
        let mut target = target(service, id);
        // SAFETY: zero initialization is valid before populating the documented fields.
        let mut credential: CREDENTIALW = unsafe { std::mem::zeroed() };
        credential.Type = CRED_TYPE_GENERIC;
        credential.TargetName = target.as_mut_ptr();
        credential.CredentialBlobSize = value.len() as u32;
        credential.CredentialBlob = value.as_ptr() as *mut u8;
        credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: target and value buffers remain alive for the synchronous write.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            Err("Could not save the credential in Windows Credential Manager.".into())
        } else {
            Ok(())
        }
    }

    pub fn load(service: &str, id: &str) -> Result<Vec<u8>, String> {
        let target = target(service, id);
        let mut credential = std::ptr::null_mut();
        // SAFETY: target is terminated and output storage is valid.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
            return Err("Credential is unavailable in Windows Credential Manager.".into());
        }
        // SAFETY: a successful read returns one valid CREDENTIALW allocation.
        let value = unsafe {
            let credential = &*credential;
            let length = credential.CredentialBlobSize as usize;
            if length == 0
                || length > super::MAX_SECRET_BYTES
                || credential.CredentialBlob.is_null()
            {
                Err("Credential data from Windows Credential Manager is invalid.".to_owned())
            } else {
                // SAFETY: the non-null blob belongs to the returned credential allocation and
                // its non-zero length was validated against Canopy's upper bound.
                Ok(std::slice::from_raw_parts(credential.CredentialBlob, length).to_vec())
            }
        };
        // SAFETY: CredReadW allocated this buffer and transfers ownership to the caller.
        unsafe { CredFree(credential.cast()) };
        value
    }

    pub fn remove(service: &str, id: &str) -> Result<(), String> {
        let target = target(service, id);
        // SAFETY: target is terminated and remains alive for the synchronous delete.
        let deleted = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0;
        let missing = !deleted && unsafe { GetLastError() } == ERROR_NOT_FOUND;
        if deleted || missing {
            Ok(())
        } else {
            Err("Could not remove the credential from Windows Credential Manager.".into())
        }
    }

    fn target(service: &str, id: &str) -> Vec<u16> {
        format!("{service}/{id}")
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod backend {
    pub fn store(_: &str, _: &str, _: &[u8]) -> Result<(), String> {
        Err("Secure credential storage is unavailable on this platform.".into())
    }
    pub fn load(_: &str, _: &str) -> Result<Vec<u8>, String> {
        Err("Secure credential storage is unavailable on this platform.".into())
    }
    pub fn remove(_: &str, _: &str) -> Result<(), String> {
        Err("Secure credential storage is unavailable on this platform.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_target_validation_precede_the_os_store() {
        assert!(validate("service", "id", b"secret").is_ok());
        assert!(validate("service", "id", &vec![b'x'; 2_561]).is_err());
        assert!(validate_target("service", "bad/id").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_store_roundtrip_update_remove_unicode_and_limit() {
        use windows_sys::Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
        };

        let service = "tech.itsol.canopy.rust.tests";
        let id = uuid::Uuid::new_v4().to_string();
        remove(service, &id).unwrap();
        store(service, &id, "zażółć 🔐").unwrap();
        assert_eq!(load(service, &id).unwrap(), "zażółć 🔐");
        store(service, &id, "updated").unwrap();
        assert_eq!(load(service, &id).unwrap(), "updated");
        remove(service, &id).unwrap();
        remove(service, &id).unwrap();
        assert!(load(service, &id).is_err());
        assert!(store(service, &id, &"x".repeat(MAX_SECRET_BYTES + 1)).is_err());

        let mut target: Vec<_> = format!("{service}/{id}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // Simulate an external application writing a credential Canopy itself rejects.
        let mut empty: CREDENTIALW = unsafe { std::mem::zeroed() };
        empty.Type = CRED_TYPE_GENERIC;
        empty.TargetName = target.as_mut_ptr();
        empty.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: target remains alive and terminated for this synchronous write.
        assert_ne!(unsafe { CredWriteW(&empty, 0) }, 0);
        assert!(load(service, &id).is_err());
        remove(service, &id).unwrap();
    }
}
