//! Private, temporary files for read-only attachment previews. Never workspace resources.
use std::{
    io::Write,
    path::{Path, PathBuf},
};
pub const PREVIEW_LIMIT: usize = 25 * 1024 * 1024;
pub struct PreviewFile {
    path: PathBuf,
    _directory: tempfile::TempDir,
}
impl PreviewFile {
    pub fn create(name: &str, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PREVIEW_LIMIT {
            return Err("Attachment exceeds the 25 MiB preview limit.".into());
        }
        let mut builder = tempfile::Builder::new();
        builder.prefix("canopy-attachment-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let parent = crate::platform::directories::data_dir()?.join("previews");
        crate::platform::directories::ensure_private_dir(&parent)?;
        let directory = builder.tempdir_in(parent).map_err(|e| e.to_string())?;
        let path = directory.path().join(preview_name(name));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o400))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            path,
            _directory: directory,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn save_to(&self, target: &Path) -> Result<(), String> {
        let parent = target.parent().ok_or("Invalid save destination.")?;
        let mut out = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let mut input = std::fs::File::open(&self.path).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut out).map_err(|e| e.to_string())?;
        out.as_file().sync_all().map_err(|e| e.to_string())?;
        out.persist(target).map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub fn preview_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or("attachment");
    let mut name = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    name.truncate(name.trim_end_matches([' ', '.']).len());
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches([' ', '.']);
    if windows_reserved_name(stem) {
        name.insert(0, '_');
    }
    let name = if name.len() > 180 {
        let ext = Path::new(&name)
            .extension()
            .and_then(|s| s.to_str())
            .filter(|s| s.len() <= 24)
            .unwrap_or("");
        let mut stem = String::new();
        for c in name.chars() {
            if stem.len() + c.len_utf8() > 150 {
                break;
            }
            stem.push(c);
        }
        if ext.is_empty() {
            stem
        } else {
            format!("{stem}.{ext}")
        }
    } else {
        name
    };
    if name.trim().is_empty() || matches!(name.as_str(), "." | "..") {
        "attachment".into()
    } else {
        name
    }
}

fn windows_reserved_name(stem: &str) -> bool {
    let stem = stem.to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|number| number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'9'))
}
