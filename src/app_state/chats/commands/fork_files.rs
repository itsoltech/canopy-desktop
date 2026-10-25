//! The files a private fork's edited message attaches from its source's folder. The
//! fork gets its own copies, made in its new folder before it is published, so deleting
//! the source can't take away context its message was accepted with. Only the attached
//! files and folders are copied, within a bound, without following links and without
//! reading anything but regular files.
use canopy_desktop::{chat::transcript::Attachment, host::QueueDraft};
use std::{
    fs::File,
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
};

/// How much one fork may copy: bytes of file content and files plus folders.
#[derive(Clone, Copy, Debug)]
pub(super) struct Bound {
    pub bytes: u64,
    pub entries: usize,
}
pub(super) const BOUND: Bound = Bound {
    bytes: 64 << 20,
    entries: 1_000,
};
/// Folder levels below an attached folder (each holds a descriptor while copied).
const MAX_DEPTH: usize = 32;

/// Copies the files of `draft` that lie in `source` (a private chat's canonical folder)
/// to the same places in `destination` (the fork's new, empty folder) and returns the
/// draft naming the copies. Any attached path outside `source`, a link, a special file
/// or more than `bound` fails the whole copy; the caller then removes `destination`.
pub(super) fn carry(
    draft: QueueDraft,
    source: Option<&Path>,
    destination: &Path,
    bound: Bound,
) -> Result<QueueDraft, String> {
    let artifacts = draft.artifact_ids().to_vec();
    let mut draft = draft.into_draft();
    let mut relatives = Vec::new();
    for attachment in &draft.attachments {
        if let Some(path) = attached(attachment) {
            let source = source.ok_or("The attached files are not available to this fork.")?;
            relatives.push(relative(source, path)?);
        }
    }
    if relatives.is_empty() {
        return QueueDraft::from(draft)
            .with_artifacts(artifacts)
            .ok_or_else(|| "The edited message is not valid.".into());
    }
    let source = source.expect("attached files have a source");
    // Re-resolve the root: a source folder replaced since it was checked is refused.
    if std::fs::canonicalize(source).ok().as_deref() != Some(source) {
        return Err("The source chat's folder changed; fork again.".into());
    }
    // Ancestors sort first, so a path inside a folder already copied comes with it;
    // the same path attached twice is copied once.
    let mut order = relatives.clone();
    order.sort();
    order.dedup();
    let mut budget = bound;
    let mut folders: Vec<&PathBuf> = Vec::new();
    for relative in &order {
        if folders.iter().any(|done| relative.starts_with(done)) {
            continue;
        }
        let from = Opened::root(source)?.walk(relative)?;
        let folder = matches!(from, Opened::Folder { .. });
        if let Some(parent) = relative.parent() {
            std::fs::create_dir_all(destination.join(parent)).map_err(copy_error)?;
        }
        from.copy_to(&destination.join(relative), &mut budget, 0)?;
        if folder {
            folders.push(relative);
        }
    }
    let mut relatives = relatives.into_iter();
    for attachment in &mut draft.attachments {
        if let Some(path) = attached_mut(attachment) {
            *path = destination.join(relatives.next().expect("one per attached path"));
        }
    }
    QueueDraft::from(draft)
        .with_artifacts(artifacts)
        .ok_or_else(|| "The edited message is not valid.".into())
}

fn attached(attachment: &Attachment) -> Option<&Path> {
    match attachment {
        Attachment::Path { path, .. } => Some(path),
        Attachment::Command {
            path: Some(path), ..
        } => Some(path),
        _ => None,
    }
}
fn attached_mut(attachment: &mut Attachment) -> Option<&mut PathBuf> {
    match attachment {
        Attachment::Path { path, .. } => Some(path),
        Attachment::Command {
            path: Some(path), ..
        } => Some(path),
        _ => None,
    }
}

/// `path` relative to `source`, as plain components.
fn relative(source: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = path
        .strip_prefix(source)
        .map_err(|_| "An attached file is outside the source chat's folder.".to_owned())?;
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("An attached path is not valid.".into());
    }
    Ok(relative.to_owned())
}

fn copy_error(error: std::io::Error) -> String {
    format!("Could not copy the attached files: {error}")
}

fn refused(name: &Path) -> String {
    format!(
        "{} is a link or a special file; only files and folders are copied.",
        name.display()
    )
}

/// An open file or folder of the source, reached one component at a time without
/// following links (on Unix each step is relative to the folder already open).
enum Opened {
    Folder {
        file: platform::Folder,
        path: PathBuf,
    },
    File {
        file: File,
        len: u64,
    },
}

impl Opened {
    fn root(source: &Path) -> Result<Self, String> {
        Ok(Self::Folder {
            file: platform::open_folder(source).map_err(copy_error)?,
            path: source.to_owned(),
        })
    }

    fn walk(self, relative: &Path) -> Result<Self, String> {
        let mut at = self;
        for name in relative.iter() {
            at = match at {
                Self::Folder { file, path } => Self::child(&file, &path, Path::new(name))?,
                Self::File { .. } => {
                    return Err("An attached path goes through a file.".into());
                }
            };
        }
        Ok(at)
    }

    fn child(folder: &platform::Folder, path: &Path, name: &Path) -> Result<Self, String> {
        let path = path.join(name);
        match platform::open_child(folder, &path, name).map_err(copy_error)? {
            Some(Child::Folder(file)) => Ok(Self::Folder { file, path }),
            Some(Child::File(file, len)) => Ok(Self::File { file, len }),
            None => Err(refused(name)),
        }
    }

    fn copy_to(self, to: &Path, budget: &mut Bound, depth: usize) -> Result<(), String> {
        // Each level keeps its folder open while its children are copied.
        if depth > MAX_DEPTH {
            return Err("The attached folders are nested too deeply to copy.".into());
        }
        budget.entries = budget
            .entries
            .checked_sub(1)
            .ok_or("Too many attached files to copy into the fork.")?;
        match self {
            Self::File { file, len } => {
                if len > budget.bytes {
                    return Err(too_large());
                }
                let mut out = File::create_new(to).map_err(copy_error)?;
                // The file may grow while it is read: never past what is left.
                let written = std::io::copy(&mut file.take(budget.bytes + 1), &mut out)
                    .map_err(copy_error)?;
                if written > budget.bytes {
                    return Err(too_large());
                }
                budget.bytes -= written;
                out.flush().map_err(copy_error)
            }
            Self::Folder { file, path } => {
                std::fs::create_dir(to).map_err(copy_error)?;
                // Names only: each is opened relative to the folder already open.
                let mut names = std::fs::read_dir(&path)
                    .map_err(copy_error)?
                    .map(|entry| entry.map(|entry| entry.file_name()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(copy_error)?;
                if names.len() > budget.entries {
                    return Err("Too many attached files to copy into the fork.".into());
                }
                names.sort();
                for name in names {
                    let name = Path::new(&name);
                    Self::child(&file, &path, name)?.copy_to(&to.join(name), budget, depth + 1)?;
                }
                Ok(())
            }
        }
    }
}

fn too_large() -> String {
    format!(
        "The attached files are larger than {} MB; attach less to the fork.",
        BOUND.bytes >> 20
    )
}

enum Child {
    Folder(platform::Folder),
    /// A regular file and its length.
    File(File, u64),
}

#[cfg(unix)]
mod platform {
    use super::Child;
    use std::{
        ffi::CString,
        fs::File,
        io,
        os::{
            fd::{AsRawFd as _, FromRawFd as _},
            unix::{ffi::OsStrExt as _, fs::OpenOptionsExt as _},
        },
        path::Path,
    };

    pub type Folder = File;

    pub fn open_folder(path: &Path) -> io::Result<Folder> {
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }

    /// `name` in `folder`, checked before it is opened (a link, FIFO or device is
    /// never opened) and again once open; `None` for anything but a folder or file.
    pub fn open_child(folder: &Folder, _path: &Path, name: &Path) -> io::Result<Option<Child>> {
        let name = CString::new(name.as_os_str().as_bytes())?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `name` is NUL-terminated and `stat` is written before it is read.
        let stated = unsafe {
            libc::fstatat(
                folder.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if stated != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fstatat succeeded and filled it.
        let kind = unsafe { stat.assume_init() }.st_mode & libc::S_IFMT;
        let flags = match kind {
            libc::S_IFDIR => libc::O_DIRECTORY,
            libc::S_IFREG => libc::O_NONBLOCK,
            _ => return Ok(None),
        };
        // SAFETY: plain openat of a NUL-terminated name relative to an open folder.
        let fd = unsafe {
            libc::openat(
                folder.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | flags,
            )
        };
        if fd < 0 {
            let error = io::Error::last_os_error();
            // Swapped for a link since it was checked.
            return match error.raw_os_error() {
                Some(libc::ELOOP) => Ok(None),
                _ => Err(error),
            };
        }
        // SAFETY: `fd` was just opened and is owned by nothing else.
        let file = unsafe { File::from_raw_fd(fd) };
        let metadata = file.metadata()?;
        Ok(if kind == libc::S_IFDIR && metadata.is_dir() {
            Some(Child::Folder(file))
        } else if kind == libc::S_IFREG && metadata.is_file() {
            Some(Child::File(file, metadata.len()))
        } else {
            None
        })
    }
}

#[cfg(not(unix))]
mod platform {
    use super::Child;
    use std::{fs::File, io, path::Path};

    /// Without `openat` folders are walked by path, each step checked on its own.
    pub type Folder = ();

    pub fn open_folder(path: &Path) -> io::Result<Folder> {
        if std::fs::symlink_metadata(path)?.is_dir() {
            Ok(())
        } else {
            Err(io::Error::other("not a folder"))
        }
    }

    pub fn open_child(_folder: &Folder, path: &Path, _name: &Path) -> io::Result<Option<Child>> {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Ok(None);
        }
        if metadata.is_dir() {
            return Ok(Some(Child::Folder(())));
        }
        if !metadata.is_file() {
            return Ok(None);
        }
        let file = File::open(path)?;
        let opened = file.metadata()?;
        Ok(opened.is_file().then(|| Child::File(file, opened.len())))
    }
}
