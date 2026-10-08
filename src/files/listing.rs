use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub const MAX_DIRECTORY_ENTRIES: usize = 50_000;
pub const MAX_VISIBLE_ENTRIES: usize = 100_000;
pub const MAX_OPEN_DIRECTORIES: usize = 512;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
    pub ignored: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirectoryListing {
    pub entries: Vec<Entry>,
    pub warning: Option<String>,
    pub git_directory: Option<PathBuf>,
    pub git_common_directory: Option<PathBuf>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decorations {
    pub status: BTreeMap<PathBuf, char>,
    pub untracked_directories: BTreeSet<PathBuf>,
    /// From the Git index, never a recursive walk through untracked/ignored directories.
    pub tracked_directories: BTreeSet<PathBuf>,
    pub git_directory: Option<PathBuf>,
    pub git_common_directory: Option<PathBuf>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    pub entries: Vec<Entry>,
    pub warning: Option<String>,
    pub git_status: BTreeMap<PathBuf, char>,
    pub git_directory: Option<PathBuf>,
    pub git_common_directory: Option<PathBuf>,
    pub loaded_directories: BTreeSet<PathBuf>,
    pub loading_directories: BTreeSet<PathBuf>,
    pub directory_errors: BTreeMap<PathBuf, String>,
}
/// Dropping a request owner also cancels work already queued on the background worker.
pub struct CancelGuard(pub Arc<AtomicBool>);
impl Default for CancelGuard {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}
impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
pub fn list_directory(
    root: &Path,
    relative: &Path,
    budget: usize,
    cancel: &AtomicBool,
) -> Result<DirectoryListing, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Directory read cancelled.".into());
    }
    if relative.components().count() > 64 {
        return Err("Folder nesting exceeds 64 levels.".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = if relative.as_os_str().is_empty() {
        root.clone()
    } else {
        super::resolve(&root, relative)?
    };
    let mut result = DirectoryListing::default();
    let repo = match git2::Repository::discover(&root) {
        Ok(repo) => Some(repo),
        Err(e) if e.code() == git2::ErrorCode::NotFound => None,
        Err(e) => {
            result.warning = Some(format!("Git ignore information unavailable: {e}"));
            None
        }
    };
    if let Some(repo) = &repo {
        result.git_directory = Some(repo.path().to_owned());
        result.git_common_directory = Some(repo.commondir().to_owned());
    }
    let limit = budget.min(MAX_DIRECTORY_ENTRIES);
    let mut git_index = None;
    for child in std::fs::read_dir(&path)
        .map_err(|e| format!("Could not read {}: {e}", relative.display()))?
    {
        if cancel.load(Ordering::Relaxed) {
            return Err("Directory read cancelled.".into());
        }
        let child = child.map_err(|e| e.to_string())?;
        if child.file_name() == ".git" {
            continue;
        }
        let relative = relative.join(child.file_name());
        if relative.to_str().is_none() {
            result.warning = Some("Files with non-UTF-8 names are omitted.".into());
            continue;
        }
        let kind = match child.file_type() {
            Ok(kind) => kind,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                result.warning = Some(format!("Some entries could not be read: {e}"));
                continue;
            }
        };
        if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
            continue;
        }
        if result.entries.len() == limit {
            result.warning = Some(if limit == MAX_DIRECTORY_ENTRIES {
                format!("This folder has more than {limit} direct entries; its listing is limited.")
            } else {
                "Visible file tree limit reached. Collapse other folders and refresh this folder."
                    .into()
            });
            break;
        }
        let mut ignored = match repo
            .as_ref()
            .map(|r| r.status_should_ignore(&root.join(&relative)))
            .transpose()
        {
            Ok(value) => value.unwrap_or(false),
            Err(e) => {
                result.warning = Some(format!("Git ignore information unavailable: {e}"));
                false
            }
        };
        if ignored && let Some(repo) = &repo {
            if git_index.is_none() {
                match repo.index() {
                    Ok(index) => git_index = Some(index),
                    Err(e) => {
                        result.warning = Some(format!("Git index unavailable: {e}"));
                        ignored = false;
                    }
                }
            }
            if let Some(index) = &git_index
                && let Some(base) = repo.workdir()
                && let Ok(path) = root.join(&relative).strip_prefix(base)
                && ((0..=3).any(|stage| index.get_path(path, stage).is_some())
                    || kind.is_dir()
                        && index
                            .find_prefix(format!("{}/", path.to_string_lossy()))
                            .is_ok())
            {
                ignored = false;
            }
        }
        result.entries.push(Entry {
            path: relative,
            directory: kind.is_dir(),
            ignored,
        });
    }
    result.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}
/// Initial Files read is shallow and deliberately does not run Git status.
pub fn scan(root: &Path) -> Result<Index, String> {
    let listing = list_directory(
        root,
        Path::new(""),
        MAX_DIRECTORY_ENTRIES,
        &AtomicBool::new(false),
    )?;
    Ok(snapshot(
        &BTreeMap::from([(PathBuf::new(), listing)]),
        &Decorations::default(),
        BTreeSet::new(),
        BTreeMap::new(),
    ))
}
pub fn snapshot(
    directories: &BTreeMap<PathBuf, DirectoryListing>,
    decorations: &Decorations,
    loading: BTreeSet<PathBuf>,
    errors: BTreeMap<PathBuf, String>,
) -> Index {
    let mut result = Index {
        loaded_directories: directories.keys().cloned().collect(),
        loading_directories: loading,
        directory_errors: errors,
        git_status: decorations.status.clone(),
        git_directory: decorations.git_directory.clone(),
        git_common_directory: decorations.git_common_directory.clone(),
        ..Default::default()
    };
    for (path, listing) in directories {
        result.entries.extend(listing.entries.iter().cloned());
        if let Some(warning) = &listing.warning {
            result
                .directory_errors
                .insert(path.clone(), warning.clone());
        }
        result.git_directory = result.git_directory.or(listing.git_directory.clone());
        result.git_common_directory = result
            .git_common_directory
            .or(listing.git_common_directory.clone());
    }
    result.entries.sort_by(|a, b| a.path.cmp(&b.path));
    for entry in &result.entries {
        if !entry.ignored
            && !result.git_status.contains_key(&entry.path)
            && entry
                .path
                .ancestors()
                .any(|p| decorations.untracked_directories.contains(p))
        {
            result.git_status.insert(entry.path.clone(), 'A');
        }
    }
    result.warning = result.directory_errors.get(Path::new("")).cloned();
    result
}
/// Query tracked changes without recursively enumerating new or ignored directory contents.
pub fn decorations(root: &Path) -> Result<Decorations, String> {
    use git2::Status as S;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let repo = match git2::Repository::discover(&root) {
        Ok(repo) => repo,
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(Decorations::default()),
        Err(e) => return Err(e.to_string()),
    };
    let base = repo
        .workdir()
        .ok_or("No working tree")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let prefix = root
        .strip_prefix(&base)
        .map_err(|_| "File root is outside the working tree.")?;
    let mut result = Decorations {
        git_directory: Some(repo.path().to_owned()),
        git_common_directory: Some(repo.commondir().to_owned()),
        ..Default::default()
    };
    let index = repo.index().map_err(|e| e.to_string())?;
    let mut parents = BTreeSet::new();
    for entry in index.iter() {
        let Ok(path) = std::str::from_utf8(&entry.path) else {
            continue;
        };
        let Ok(path) = Path::new(path).strip_prefix(prefix) else {
            continue;
        };
        if let Some(parent) = path.parent() {
            parents.insert(parent.to_owned());
        }
    }
    for mut path in parents {
        loop {
            let absolute = if path.as_os_str().is_empty() {
                Some(root.clone())
            } else {
                super::resolve(&root, &path).ok()
            };
            if absolute.is_some_and(|p| p.is_dir()) {
                result
                    .tracked_directories
                    .extend(path.ancestors().map(Path::to_owned));
                break;
            }
            if !path.pop() {
                break;
            }
        }
    }
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .recurse_ignored_dirs(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    if !prefix.as_os_str().is_empty() {
        options.pathspec(prefix.to_string_lossy().as_ref());
    }
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|e| e.to_string())?;
    for entry in statuses.iter() {
        let flags = entry.status();
        let status = if flags.contains(S::CONFLICTED) {
            'U'
        } else if flags.intersects(S::WT_DELETED | S::INDEX_DELETED) {
            'D'
        } else if flags.intersects(S::WT_RENAMED | S::INDEX_RENAMED) {
            'R'
        } else if flags.intersects(S::WT_NEW | S::INDEX_NEW) {
            'A'
        } else if flags
            .intersects(S::WT_MODIFIED | S::INDEX_MODIFIED | S::WT_TYPECHANGE | S::INDEX_TYPECHANGE)
        {
            'M'
        } else {
            continue;
        };
        let Some(path) = entry
            .index_to_workdir()
            .or_else(|| entry.head_to_index())
            .and_then(|d| d.new_file().path())
            .or_else(|| entry.path().ok().map(Path::new))
        else {
            continue;
        };
        let absolute = base.join(path);
        let Ok(relative) = absolute.strip_prefix(&root) else {
            continue;
        };
        if flags.contains(S::WT_NEW) && path.to_string_lossy().ends_with('/') {
            result.untracked_directories.insert(relative.to_owned());
        }
        result.status.insert(relative.to_owned(), status);
        let mut parent = relative.parent();
        while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
            result
                .status
                .entry(path.to_owned())
                .and_modify(|a| *a = aggregate_status(*a, status))
                .or_insert(status);
            parent = path.parent();
        }
    }
    Ok(result)
}
fn aggregate_status(a: char, b: char) -> char {
    if a == 'U' || b == 'U' {
        'U'
    } else if a == b {
        a
    } else {
        'M'
    }
}
