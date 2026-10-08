//! Only the selected working tree is watched. Callbacks never read Git/files.
use super::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub(super) struct ChangeWatch {
    pub path: PathBuf,
    pub deadline: Option<Instant>,
    dirty: Arc<AtomicBool>,
    candidates: Arc<Mutex<HashSet<PathBuf>>>,
    overflow: Arc<AtomicBool>,
    _watcher: Option<RecommendedWatcher>,
    pub failed: bool,
}
impl ChangeWatch {
    pub fn new(path: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let dirty = Arc::new(AtomicBool::new(false));
        let candidates = Arc::new(Mutex::new(HashSet::new()));
        let overflow = Arc::new(AtomicBool::new(false));
        let flag = dirty.clone();
        let paths = candidates.clone();
        let overflow_flag = overflow.clone();
        let root = path.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event {
                    let rescan = event.need_rescan();
                    if rescan {
                        overflow_flag.store(true, Ordering::Release);
                    }
                    if !rescan && matches!(event.kind, notify::EventKind::Access(_)) {
                        return;
                    }
                    for path in event.paths {
                        if let Ok(relative) = path.strip_prefix(&root) {
                            if relative.starts_with(".git") {
                                continue;
                            }
                            let mut paths = paths.lock().unwrap();
                            if paths.len() < 256 {
                                paths.insert(relative.to_path_buf());
                            } else {
                                overflow_flag.store(true, Ordering::Release);
                            }
                        }
                    }
                } else {
                    overflow_flag.store(true, Ordering::Release);
                }
                if !flag.swap(true, Ordering::AcqRel) {
                    wake();
                }
            })
            .ok();
        let failed = watcher
            .as_mut()
            .is_none_or(|w| w.watch(&path, RecursiveMode::Recursive).is_err());
        Self {
            path,
            deadline: None,
            dirty,
            candidates,
            overflow,
            _watcher: watcher,
            failed,
        }
    }
    pub fn schedule(&mut self) {
        if self.dirty.load(Ordering::Acquire) && self.deadline.is_none() {
            self.deadline = Some(Instant::now() + Duration::from_millis(300));
        }
    }
    pub fn due(&mut self) -> bool {
        if self.deadline.is_none_or(|d| Instant::now() < d) {
            return false;
        }
        self.deadline = None;
        self.dirty.store(false, Ordering::Release);
        let paths = std::mem::take(&mut *self.candidates.lock().unwrap());
        if self.overflow.swap(false, Ordering::AcqRel) {
            self.failed = true;
            return true;
        }
        let Ok(repo) = git2::Repository::open(&self.path) else {
            return true;
        };
        paths.iter().any(|p| {
            p.as_os_str().is_empty()
                || p.file_name()
                    .is_some_and(|n| n == ".gitignore" || n == ".gitattributes")
                || !repo.status_should_ignore(p).unwrap_or(false)
        })
    }
}
