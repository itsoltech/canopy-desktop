//! One host-owned reservation set for editor/ACP writes and Git operations.
//! Paths are host-canonical identities supplied by the owner; acquiring a gate
//! performs no I/O and is safe on the app thread. This does not lock other apps.
use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct RootReservations {
    exclusive: bool,
    files: HashSet<PathBuf>,
}
#[derive(Default)]
struct State {
    roots: Mutex<HashMap<PathBuf, RootReservations>>,
    released: tokio::sync::Notify,
}
#[derive(Clone, Default)]
pub struct Gate {
    state: Arc<State>,
}
pub struct Reservation {
    state: Arc<State>,
    root: PathBuf,
    file: Option<PathBuf>,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Ok(mut roots) = self.state.roots.lock()
            && let Some(entry) = roots.get_mut(&self.root)
        {
            if let Some(file) = &self.file {
                entry.files.remove(file);
            } else {
                entry.exclusive = false;
            }
            if !entry.exclusive && entry.files.is_empty() {
                roots.remove(&self.root);
            }
        }
        self.state.released.notify_waiters();
    }
}
fn valid_root(root: &Path) -> bool {
    root.is_absolute()
        && !root
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}
fn valid_file(file: &Path) -> bool {
    !file.as_os_str().is_empty() && file.components().all(|c| matches!(c, Component::Normal(_)))
}
impl Gate {
    pub fn try_reserve_file(&self, root: &Path, relative: &Path) -> Option<Reservation> {
        if !valid_root(root) || !valid_file(relative) {
            return None;
        }
        self.reserve(root, Some(relative))
    }
    pub fn try_reserve_root(&self, root: &Path) -> Option<Reservation> {
        if !valid_root(root) {
            return None;
        }
        self.reserve(root, None)
    }
    fn reserve(&self, root: &Path, file: Option<&Path>) -> Option<Reservation> {
        let mut roots = self.state.roots.lock().ok()?;
        // Different workspace roots can overlap. Compare absolute identities so
        // a nested workspace cannot bypass its parent's Git reservation.
        let absolute = file.map(|file| root.join(file));
        for (reserved_root, entry) in roots.iter() {
            let overlaps = match &absolute {
                Some(path) => {
                    entry.exclusive && path.starts_with(reserved_root)
                        || entry
                            .files
                            .iter()
                            .any(|file| reserved_root.join(file) == *path)
                }
                None => {
                    entry.exclusive
                        && (root.starts_with(reserved_root) || reserved_root.starts_with(root))
                        || entry
                            .files
                            .iter()
                            .any(|file| reserved_root.join(file).starts_with(root))
                }
            };
            if overlaps {
                return None;
            }
        }
        let entry = roots.entry(root.into()).or_default();
        if entry.exclusive
            || file.is_none() && !entry.files.is_empty()
            || file.is_some_and(|file| entry.files.contains(file))
        {
            return None;
        }
        if let Some(file) = file {
            entry.files.insert(file.into());
        } else {
            entry.exclusive = true;
        }
        Some(Reservation {
            state: self.state.clone(),
            root: root.into(),
            file: file.map(Path::to_owned),
        })
    }
    pub async fn acquire_file(&self, root: &Path, relative: &Path) -> Result<Reservation, String> {
        if !valid_root(root) || !valid_file(relative) {
            return Err("Invalid file reservation identity.".into());
        }
        self.acquire(root, Some(relative)).await
    }
    pub async fn acquire_root(&self, root: &Path) -> Result<Reservation, String> {
        if !valid_root(root) {
            return Err("Invalid root reservation identity.".into());
        }
        self.acquire(root, None).await
    }
    async fn acquire(&self, root: &Path, file: Option<&Path>) -> Result<Reservation, String> {
        loop {
            let notified = self.state.released.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(reservation) = self.reserve(root, file) {
                return Ok(reservation);
            }
            if self.state.roots.is_poisoned() {
                return Err("File reservations are unavailable.".into());
            }
            notified.await;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn local_file_and_remote_root_reservations_exclude_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let gate = Gate::default();
        let local = gate.try_reserve_file(&root, Path::new("a")).unwrap();
        assert!(gate.try_reserve_file(&root, Path::new("a")).is_none());
        assert!(gate.try_reserve_root(&root).is_none());
        let other = gate.try_reserve_file(&root, Path::new("b")).unwrap();
        let remote = tokio::spawn({
            let gate = gate.clone();
            let root = root.clone();
            async move { gate.acquire_root(&root).await.unwrap() }
        });
        drop(local);
        assert!(gate.try_reserve_root(&root).is_none());
        drop(other);
        let remote = tokio::time::timeout(std::time::Duration::from_secs(5), remote)
            .await
            .unwrap()
            .unwrap();
        assert!(gate.try_reserve_file(&root, Path::new("a")).is_none());
        assert!(
            gate.try_reserve_file(&root.join("nested"), Path::new("a"))
                .is_none()
        );
        drop(remote);
        assert!(gate.try_reserve_file(&root, Path::new("a")).is_some());
        let nested = gate
            .try_reserve_file(&root.join("nested"), Path::new("file"))
            .unwrap();
        assert!(
            gate.try_reserve_file(&root, Path::new("nested/file"))
                .is_none()
        );
        assert!(gate.try_reserve_root(&root).is_none());
        drop(nested);
    }
}
