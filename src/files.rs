//! Bounded filesystem access. Called on background executors, never from render.
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
pub const MAX_TEXT: usize = 2 * 1024 * 1024;
mod listing;
pub mod search;
mod tree_watch;
pub use listing::*;
pub use tree_watch::{Dirty, TreeWatch, WatchPaths};

fn is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
}

pub fn resolve(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Invalid file path.".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut path = root.clone();
    let mut components = relative.components();
    while let Some(component) = components.next() {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if is_link_or_reparse(&metadata) {
                    return Err("Links and reparse points are not editable in this version.".into());
                }
                let canonical = path.canonicalize().map_err(|e| e.to_string())?;
                if !canonical.starts_with(&root) {
                    return Err("File path leaves the selected folder.".into());
                }
                path = canonical;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                path.extend(components);
                return Ok(path);
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(path)
}
fn bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Not a regular file.".into());
    }
    let mut value = Vec::new();
    file.take((MAX_TEXT + 1) as u64)
        .read_to_end(&mut value)
        .map_err(|e| e.to_string())?;
    if value.len() > MAX_TEXT {
        return Err("Files larger than 2 MiB are not supported by this editor.".into());
    }
    Ok(value)
}

#[cfg(windows)]
fn persist_temp(temp: tempfile::NamedTempFile, path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = temp
        .path()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let destination: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn persist_temp(temp: tempfile::NamedTempFile, path: &Path) -> Result<(), String> {
    temp.persist(path).map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Clone, Debug)]
pub struct Document {
    pub text: String,
    pub original: Vec<u8>,
    crlf: bool,
    bom: bool,
}
impl Document {
    pub fn load(root: &Path, relative: &Path) -> Result<Self, String> {
        let original = bytes(&resolve(root, relative)?)?;
        let bom = original.starts_with(&[0xef, 0xbb, 0xbf]);
        let text = std::str::from_utf8(if bom { &original[3..] } else { &original })
            .map_err(|_| "Only UTF-8 text files are supported.")?;
        if text.contains('\0') {
            return Err("Binary files cannot be edited.".into());
        }
        if text.bytes().filter(|b| *b == b'\n').count() > 50_000 {
            return Err("Files with more than 50,000 lines are not supported.".into());
        }
        let crlf = text.contains("\r\n");
        Ok(Self {
            text: text.replace("\r\n", "\n"),
            original,
            crlf,
            bom,
        })
    }
    pub fn save(&self, root: &Path, relative: &Path, text: &str) -> Result<Self, String> {
        let path = resolve(root, relative)?;
        if bytes(&path)? != self.original {
            return Err(
                "File changed on disk. Reload it before saving; your edits are still here.".into(),
            );
        }
        let mut value = if self.bom {
            vec![0xef, 0xbb, 0xbf]
        } else {
            Vec::new()
        };
        value.extend(if self.crlf {
            text.replace('\n', "\r\n").into_bytes()
        } else {
            text.as_bytes().to_vec()
        });
        if value.len() > MAX_TEXT {
            return Err("File exceeds the 2 MiB editor limit.".into());
        }
        let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing parent")?)
            .map_err(|e| e.to_string())?;
        temp.as_file()
            .set_permissions(
                std::fs::metadata(&path)
                    .map_err(|e| e.to_string())?
                    .permissions(),
            )
            .map_err(|e| e.to_string())?;
        temp.write_all(&value).map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        if bytes(&path)? != self.original {
            return Err("File changed while saving. Your edits have been preserved.".into());
        }
        persist_temp(temp, &path)?;
        Ok(Self {
            text: text.into(),
            original: value,
            crlf: self.crlf,
            bom: self.bom,
        })
    }
}
pub fn create(root: &Path, relative: &Path) -> Result<(), String> {
    let path = resolve(root, relative)?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub struct Watch {
    _watcher: notify::RecommendedWatcher,
    pub changes: async_channel::Receiver<()>,
    error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}
impl Watch {
    pub fn add(&mut self, path: &Path) -> Result<(), String> {
        use notify::Watcher;
        self._watcher
            .watch(path, notify::RecursiveMode::Recursive)
            .map_err(|e| e.to_string())
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
    pub fn new(path: &Path, recursive: bool) -> Result<Self, String> {
        use notify::Watcher;
        let (tx, changes) = async_channel::bounded(1);
        let error = std::sync::Arc::new(std::sync::Mutex::new(None));
        let watch_error = error.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Err(error) = &event {
                    *watch_error.lock().unwrap() = Some(error.to_string());
                }
                if event.as_ref().is_ok_and(|e| e.kind.is_access()) {
                    return;
                }
                if event.as_ref().is_ok_and(|e| {
                    e.paths.iter().all(|p| {
                        p.components().any(|c| c.as_os_str() == ".git")
                            && !matches!(
                                p.file_name().and_then(|n| n.to_str()),
                                Some("index" | "HEAD" | "packed-refs" | "config")
                            )
                            && !p.components().any(|c| c.as_os_str() == "refs")
                    })
                }) {
                    return;
                }
                let _ = tx.try_send(());
            })
            .map_err(|e| e.to_string())?;
        watcher
            .watch(
                path,
                if recursive {
                    notify::RecursiveMode::Recursive
                } else {
                    notify::RecursiveMode::NonRecursive
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            _watcher: watcher,
            changes,
            error,
        })
    }
}

pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tif" | "tiff"
        )
    })
}
pub fn image_bytes(root: &Path, relative: &Path) -> Result<Vec<u8>, String> {
    let path = resolve(root, relative)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Not a regular image file.".into());
    }
    const LIMIT: usize = 20 * 1024 * 1024;
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Images larger than 20 MiB are not supported.".into());
    }
    Ok(bytes)
}

pub fn is_font(path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "ttf" | "otf" | "woff" | "woff2"
        )
    })
}
pub fn is_video(path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "mp4" | "m4v" | "mov" | "mkv"
        )
    })
}
