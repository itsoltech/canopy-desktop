use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
#[derive(Clone, Default, Debug)]
pub struct Dirty {
    pub tree: bool,
    pub git: bool,
    /// Empty means every listed level when tree is true (ignore/index changes or rescan).
    pub directories: BTreeSet<PathBuf>,
}
impl Dirty {
    fn merge(&mut self, next: Self) {
        if next.tree {
            if self.tree && self.directories.is_empty() || next.directories.is_empty() {
                self.directories.clear();
            } else {
                self.directories.extend(next.directories);
            }
            self.tree = true;
        }
        self.git |= next.git;
    }
}
#[derive(Default)]
pub struct WatchPaths {
    pub listed: BTreeSet<PathBuf>,
    pub tracked: BTreeSet<PathBuf>,
    pub untracked: BTreeSet<PathBuf>,
    pub ignored: BTreeSet<PathBuf>,
    pub git: BTreeSet<PathBuf>,
}
#[derive(Default)]
struct Scope {
    paths: WatchPaths,
    dirty: Dirty,
    error: Option<String>,
}
pub struct TreeWatch {
    watcher: notify::RecommendedWatcher,
    scope: Arc<Mutex<Scope>>,
    paths: BTreeMap<PathBuf, bool>,
    pub changes: async_channel::Receiver<()>,
}
fn metadata(path: &Path, roots: &BTreeSet<PathBuf>) -> bool {
    roots.iter().any(|root| {
        path.strip_prefix(root).is_ok_and(|p| {
            let first = p.components().next().map(|c| c.as_os_str());
            if first.is_some_and(|p| p == "objects" || p == "logs") {
                return false;
            }
            p.as_os_str().is_empty()
                || matches!(
                    p.file_name().and_then(|s| s.to_str()),
                    Some("HEAD" | "index" | "packed-refs" | "config" | "exclude")
                )
                || p.components().any(|c| c.as_os_str() == "refs")
        })
    })
}
fn classify(event: &Event, scope: &Scope) -> Dirty {
    if event.kind.is_access() {
        return Dirty::default();
    }
    let mut dirty = Dirty::default();
    for path in &event.paths {
        let listed = scope.paths.listed.contains(path)
            || path
                .parent()
                .is_some_and(|p| scope.paths.listed.contains(p));
        let tracked = scope.paths.tracked.contains(path)
            || path
                .parent()
                .is_some_and(|p| scope.paths.tracked.contains(p));
        let git = metadata(path, &scope.paths.git);
        let structure = !matches!(
            event.kind,
            EventKind::Modify(notify::event::ModifyKind::Data(_))
        );
        let ignore = path
            .file_name()
            .is_some_and(|s| s == ".gitignore" || s == "exclude" || s == "config");
        let ignored = path.ancestors().any(|p| scope.paths.ignored.contains(p));
        let untracked = path.ancestors().any(|p| scope.paths.untracked.contains(p));
        let tree = listed && (structure || ignore) || git;
        let mut directories = BTreeSet::new();
        if tree && !git && !ignore {
            if scope.paths.listed.contains(path) {
                directories.insert(path.clone());
            }
            if let Some(parent) = path.parent()
                && scope.paths.listed.contains(parent)
            {
                directories.insert(parent.to_owned());
            }
        }
        dirty.merge(Dirty {
            tree,
            git: git || !ignored && (listed || tracked || structure && untracked),
            directories,
        });
    }
    if event.need_rescan() {
        dirty = Dirty {
            tree: true,
            git: true,
            ..Default::default()
        };
    }
    dirty
}

fn record(scope: &mut Scope, event: notify::Result<Event>) -> bool {
    let dirty = match event {
        Ok(event) => {
            if event.need_rescan() {
                scope.error = Some(
                    "Filesystem events were dropped. Canopy performed a full rescan; use Refresh if the tree still looks stale."
                        .into(),
                );
            }
            classify(&event, scope)
        }
        Err(error) => {
            scope.error = Some(error.to_string());
            Dirty {
                tree: true,
                git: true,
                ..Default::default()
            }
        }
    };
    let changed = dirty.tree || dirty.git;
    scope.dirty.merge(dirty);
    changed
}

impl TreeWatch {
    pub fn new() -> Result<Self, String> {
        let (tx, changes) = async_channel::bounded(1);
        let scope = Arc::new(Mutex::new(Scope::default()));
        let state = scope.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let mut state = state.lock().unwrap();
            if record(&mut state, event) {
                let _ = tx.try_send(());
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(Self {
            watcher,
            scope,
            paths: Default::default(),
            changes,
        })
    }
    pub fn set(&mut self, paths: WatchPaths) -> Result<(), String> {
        let WatchPaths {
            listed,
            tracked,
            untracked,
            ignored,
            git,
        } = paths;
        let mut wanted = BTreeMap::new();
        #[cfg(target_os = "macos")]
        {
            // notify's FSEvents backend registers a path; it does not walk its descendants.
            // One stream avoids rebuilding FSEvents once per tracked directory. The callback
            // below still accepts only listed levels / tracked parents / relevant Git metadata.
            if let Some(root) = listed.iter().min_by_key(|p| p.components().count()) {
                wanted.insert(root.clone(), true);
            }
            for root in &git {
                if !wanted.keys().any(|parent| root.starts_with(parent)) {
                    wanted.insert(root.clone(), true);
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            for path in listed.iter().chain(tracked.iter()).chain(untracked.iter()) {
                wanted.insert(path.clone(), false);
            }
            for path in &git {
                wanted.insert(path.clone(), false);
                for (name, recursive) in [("refs", true), ("info", false)] {
                    let child = path.join(name);
                    if child.is_dir() {
                        wanted.insert(child, recursive);
                    }
                }
            }
        }
        {
            let mut s = self.scope.lock().unwrap();
            s.paths = WatchPaths {
                listed,
                tracked,
                untracked,
                ignored,
                git,
            };
        }
        for (path, recursive) in self.paths.clone() {
            if wanted.get(&path) != Some(&recursive) {
                let _ = self.watcher.unwatch(&path);
                self.paths.remove(&path);
            }
        }
        let mut error = None;
        for (path, recursive) in wanted {
            if self.paths.get(&path) == Some(&recursive) {
                continue;
            }
            match self.watcher.watch(
                &path,
                if recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            ) {
                Ok(()) => {
                    self.paths.insert(path, recursive);
                }
                Err(e) => error = Some(e.to_string()),
            }
        }
        error.map_or(Ok(()), Err)
    }
    pub fn take_dirty(&self) -> Dirty {
        std::mem::take(&mut self.scope.lock().unwrap().dirty)
    }
    pub fn error(&self) -> Option<String> {
        self.scope.lock().unwrap().error.clone()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unopened_dependency_descendants_do_not_invalidate_tree() {
        let scope = Scope {
            paths: WatchPaths {
                listed: BTreeSet::from([PathBuf::from("/repo")]),
                tracked: BTreeSet::from([PathBuf::from("/repo/src")]),
                git: BTreeSet::from([PathBuf::from("/repo/.git")]),
                untracked: BTreeSet::from([PathBuf::from("/repo/generated")]),
                ignored: BTreeSet::from([PathBuf::from("/repo/node_modules")]),
            },
            ..Default::default()
        };
        let event = |path: &str| {
            Event::new(EventKind::Create(notify::event::CreateKind::File)).add_path(path.into())
        };
        let d = classify(&event("/repo/node_modules/pkg/file.js"), &scope);
        assert!(!d.tree && !d.git);
        let d = classify(&event("/repo/src/main.rs"), &scope);
        assert!(!d.tree && d.git);
        let d = classify(&event("/repo/new.txt"), &scope);
        assert!(d.tree && d.git);
        let d = classify(&event("/repo/.git/objects/aa/bb"), &scope);
        assert!(!d.tree && !d.git);
        assert!(classify(&event("/repo/.git/index"), &scope).git);
        let d = classify(&event("/repo/generated/deep/file"), &scope);
        assert!(!d.tree && d.git);
        let scope = Scope {
            paths: WatchPaths {
                listed: BTreeSet::from([PathBuf::from("/repo/node_modules")]),
                ignored: BTreeSet::from([PathBuf::from("/repo/node_modules")]),
                ..Default::default()
            },
            ..Default::default()
        };
        let d = classify(&event("/repo/node_modules/new-package"), &scope);
        assert!(d.tree && !d.git);
        assert_eq!(
            d.directories,
            BTreeSet::from([PathBuf::from("/repo/node_modules")])
        );
    }
    #[test]
    fn broad_invalidation_is_not_lost_when_events_are_coalesced() {
        let mut dirty = Dirty::default();
        dirty.merge(Dirty {
            tree: true,
            git: false,
            directories: BTreeSet::from([PathBuf::from("/repo/src")]),
        });
        assert_eq!(dirty.directories.len(), 1);
        dirty.merge(Dirty {
            tree: true,
            git: true,
            ..Default::default()
        });
        dirty.merge(Dirty {
            tree: true,
            git: false,
            directories: BTreeSet::from([PathBuf::from("/repo/docs")]),
        });
        assert!(dirty.tree && dirty.git && dirty.directories.is_empty());
    }

    #[test]
    fn rescan_notice_invalidates_everything_and_keeps_a_visible_warning() {
        let event = Event::new(EventKind::Any).set_flag(notify::event::Flag::Rescan);
        let mut scope = Scope::default();
        assert!(record(&mut scope, Ok(event)));
        assert!(scope.dirty.tree && scope.dirty.git && scope.dirty.directories.is_empty());
        assert!(
            scope
                .error
                .as_deref()
                .is_some_and(|error| error.contains("dropped"))
        );
    }
}
