//! Private working directories for chats without a project.
//!
//! Each such chat gets `<data dir>/chats/<uuid>` (0700). Removal is limited to those
//! directories: anything else — other paths, symlinks, non-UUID names — is refused.
use std::path::{Path, PathBuf};

pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join("chats")
}

/// Creates a fresh private directory and returns its canonical path.
pub fn create(data_dir: &Path) -> Result<PathBuf, String> {
    let root = root(data_dir);
    crate::platform::directories::ensure_private_dir(&root)?;
    let dir = root.join(uuid::Uuid::new_v4().to_string());
    crate::platform::directories::ensure_private_dir(&dir)?;
    std::fs::canonicalize(&dir).map_err(|error| error.to_string())
}

/// Whether `dir` is a chat directory this module created (it may no longer exist).
pub fn owns(data_dir: &Path, dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if uuid::Uuid::parse_str(name).is_err() {
        return false;
    }
    let Ok(root) = std::fs::canonicalize(root(data_dir)) else {
        return false;
    };
    // Compare the parent canonically, without following the final component.
    dir.parent()
        .and_then(|parent| std::fs::canonicalize(parent).ok())
        .is_some_and(|parent| parent == root)
}

/// Deletes a chat directory with its contents. `Ok(false)` when there is nothing to
/// delete; an error for anything this module does not own.
pub fn remove(data_dir: &Path, dir: &Path) -> Result<bool, String> {
    if !owns(data_dir, dir) {
        return Err("Refusing to delete a folder outside Canopy's chat storage.".into());
    }
    match std::fs::symlink_metadata(dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err("Refusing to delete a chat folder that is not a plain directory.".into())
        }
        Ok(_) => std::fs::remove_dir_all(dir)
            .map(|()| true)
            .map_err(|error| format!("Could not delete the chat folder: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not inspect the chat folder: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_private_dirs_and_removes_only_its_own() {
        let data = tempfile::tempdir().unwrap();
        let dir = create(data.path()).unwrap();
        assert!(dir.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
        std::fs::write(dir.join("note.txt"), "x").unwrap();
        assert!(owns(data.path(), &dir));
        assert_eq!(remove(data.path(), &dir), Ok(true));
        assert!(!dir.exists());
        assert_eq!(remove(data.path(), &dir), Ok(false), "already gone");

        let outside = tempfile::tempdir().unwrap();
        assert!(remove(data.path(), outside.path()).is_err());
        let not_uuid = root(data.path()).join("keep");
        std::fs::create_dir_all(&not_uuid).unwrap();
        assert!(remove(data.path(), &not_uuid).is_err());
        assert!(not_uuid.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_chat_dirs_are_never_followed() {
        let data = tempfile::tempdir().unwrap();
        let victim = tempfile::tempdir().unwrap();
        std::fs::write(victim.path().join("precious.txt"), "x").unwrap();
        create(data.path()).unwrap();
        let link = root(data.path()).join(uuid::Uuid::new_v4().to_string());
        std::os::unix::fs::symlink(victim.path(), &link).unwrap();
        assert!(remove(data.path(), &link).is_err());
        assert!(victim.path().join("precious.txt").exists());
    }
}
