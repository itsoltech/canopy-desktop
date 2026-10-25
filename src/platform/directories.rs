use std::path::{Path, PathBuf};

pub fn data_dir() -> Result<PathBuf, String> {
    select_data_dir(std::env::var_os("CANOPY_DATA_DIR"), platform_data_dir)
}

/// Unreleased runtime schemas must never be tried on the normal user database.
/// This check does not create directories or open SQLite for writing.
pub fn runtime_data_dir() -> Result<PathBuf, String> {
    if cfg!(feature = "published-runtime-schema") {
        return data_dir();
    }
    let selected = std::env::var_os("CANOPY_DATA_DIR")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or("This unpublished runtime build requires CANOPY_DATA_DIR pointing to an isolated data directory.")?;
    validate_runtime_directory(&selected, &platform_data_dir()?)?;
    Ok(selected)
}

fn validate_runtime_directory(selected: &Path, default: &Path) -> Result<(), String> {
    let selected = canonical_candidate(selected)?;
    let default = canonical_candidate(default)?;
    if selected.starts_with(&default) {
        return Err("An unpublished runtime build cannot use the normal Canopy data directory. Select an isolated copy with CANOPY_DATA_DIR.".into());
    }
    Ok(())
}

fn canonical_candidate(path: &Path) -> Result<PathBuf, String> {
    let mut existing = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| "Current directory is unavailable.")?
            .join(path)
    };
    let mut tail = Vec::new();
    let root = loop {
        match existing.canonicalize() {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing
                    .file_name()
                    .ok_or("Data directory cannot be resolved.")?;
                tail.push(name.to_owned());
                if !existing.pop() {
                    return Err("Data directory cannot be resolved.".into());
                }
            }
            Err(_) => return Err("Data directory cannot be resolved.".into()),
        }
    };
    let mut resolved = root;
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn select_data_dir(
    override_path: Option<std::ffi::OsString>,
    fallback: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<PathBuf, String> {
    match override_path {
        Some(path) if path.is_empty() => Err("CANOPY_DATA_DIR cannot be empty.".into()),
        Some(path) => Ok(PathBuf::from(path)),
        None => fallback(),
    }
}

pub fn user_home() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    return known_folder(windows_sys::Win32::UI::Shell::FOLDERID_Profile);
    #[cfg(not(target_os = "windows"))]
    std::env::var_os("HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "User home directory is unavailable.".to_owned())
}

pub fn ensure_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "windows")]
    protect_windows_path(path)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_data_dir() -> Result<PathBuf, String> {
    user_home().map(|home| home.join("Library/Application Support/Canopy Rust"))
}

#[cfg(target_os = "windows")]
fn platform_data_dir() -> Result<PathBuf, String> {
    known_folder(windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData)
        .map(|path| path.join("Canopy Rust"))
}

#[cfg(target_os = "linux")]
fn platform_data_dir() -> Result<PathBuf, String> {
    linux_data_dir(std::env::var_os("XDG_DATA_HOME"), user_home)
}

#[cfg(target_os = "linux")]
fn linux_data_dir(
    xdg_data_home: Option<std::ffi::OsString>,
    home: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<PathBuf, String> {
    // XDG requires absolute paths; empty and relative values use the HOME fallback.
    let base = match xdg_data_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        Some(path) => path,
        None => {
            let home = home()?;
            if !home.is_absolute() {
                return Err("User home directory must be absolute.".into());
            }
            home.join(".local/share")
        }
    };
    Ok(base.join("canopy-rust"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_data_dir() -> Result<PathBuf, String> {
    Err("Application data directory is unavailable on this platform.".into())
}

#[cfg(target_os = "windows")]
fn known_folder(id: windows_sys::core::GUID) -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{System::Com::CoTaskMemFree, UI::Shell::SHGetKnownFolderPath};
    let mut raw = std::ptr::null_mut();
    // SAFETY: id and output storage are valid; null token selects the current user.
    let result = unsafe { SHGetKnownFolderPath(&id, 0, std::ptr::null_mut(), &mut raw) };
    if result < 0 || raw.is_null() {
        return Err("Windows Known Folder lookup failed.".into());
    }
    let mut length = 0usize;
    // SAFETY: successful SHGetKnownFolderPath returns a NUL-terminated allocation.
    unsafe {
        while length < 32_768 && *raw.add(length) != 0 {
            length += 1;
        }
    }
    let path = if length == 32_768 {
        Err("Windows Known Folder path is too long.".to_owned())
    } else {
        // SAFETY: length was bounded by the terminating NUL in the returned allocation.
        let value = unsafe { std::slice::from_raw_parts(raw, length) };
        Ok(PathBuf::from(std::ffi::OsString::from_wide(value)))
    };
    // SAFETY: the allocation is owned by the caller after a successful lookup.
    unsafe { CoTaskMemFree(raw.cast()) };
    path
}

#[cfg(target_os = "windows")]
fn protect_windows_path(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
        },
    };
    let encoded: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let sddl: Vec<_> = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;OW)"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: SDDL is terminated and descriptor points to writable storage.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("Could not create the Windows private-directory ACL.".into());
    }
    // SAFETY: path and descriptor remain valid for the synchronous ACL update.
    let updated = unsafe {
        SetFileSecurityW(
            encoded.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    // SAFETY: the conversion function allocated this descriptor with LocalAlloc.
    unsafe { LocalFree(descriptor) };
    if updated == 0 {
        Err("Could not protect the Windows application directory.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_data_directory_is_shared_without_rewriting_it() {
        let path = select_data_dir(Some("relative-test-data".into()), || {
            panic!("override must win")
        })
        .unwrap();
        assert_eq!(
            path.join("canopy.db"),
            PathBuf::from("relative-test-data/canopy.db")
        );
        assert!(select_data_dir(Some("".into()), || unreachable!()).is_err());
    }

    #[test]
    fn unpublished_runtime_rejects_default_data_without_creating_anything() {
        let root = tempfile::tempdir().unwrap();
        let default = root.path().join("user-data");
        assert!(validate_runtime_directory(&default, &default).is_err());
        assert!(validate_runtime_directory(&default.join("nested"), &default).is_err());
        assert!(
            !default.exists(),
            "preflight must not create the user directory"
        );
        assert!(validate_runtime_directory(&root.path().join("test-copy"), &default).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn unpublished_runtime_rejects_a_symlink_to_default_data() {
        let root = tempfile::tempdir().unwrap();
        let default = root.path().join("user-data");
        std::fs::create_dir(&default).unwrap();
        let link = root.path().join("test-copy");
        std::os::unix::fs::symlink(&default, &link).unwrap();
        assert!(validate_runtime_directory(&link, &default).is_err());
    }

    #[test]
    fn private_directory_is_created() {
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("private");
        ensure_private_dir(&path).unwrap();
        assert!(path.is_dir());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_data_directory_uses_absolute_xdg_without_requiring_home() {
        let path = linux_data_dir(Some("/custom/data".into()), || {
            Err("HOME unavailable".into())
        })
        .unwrap();
        assert_eq!(path, PathBuf::from("/custom/data/canopy-rust"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_data_directory_defaults_for_missing_empty_or_relative_xdg() {
        for xdg in [None, Some("".into()), Some("relative/data".into())] {
            let path = linux_data_dir(xdg, || Ok(PathBuf::from("/home/test"))).unwrap();
            assert_eq!(path, PathBuf::from("/home/test/.local/share/canopy-rust"));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_data_directory_requires_a_valid_home_when_xdg_is_unavailable() {
        assert!(linux_data_dir(None, || Err("HOME unavailable".into())).is_err());
        assert!(linux_data_dir(None, || Ok(PathBuf::from("relative/home"))).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_known_folders_do_not_depend_on_home_environment() {
        assert!(platform_data_dir().unwrap().is_absolute());
        assert!(user_home().unwrap().is_absolute());
    }
}
