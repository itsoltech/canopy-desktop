use canopy_desktop::git::{self, CreateWorktree, GitClient};
use git2::{BranchType, Repository, Signature};
use std::{
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
fn fixture() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path().join("main")).unwrap();
    repo.config()
        .unwrap()
        .set_bool("commit.gpgSign", false)
        .unwrap();
    std::fs::write(repo.workdir().unwrap().join("tracked"), "initial").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked")).unwrap();
    index.write().unwrap();
    let oid = index.write_tree().unwrap();
    let tree = repo.find_tree(oid).unwrap();
    let signature = Signature::now("Canopy Test", "test@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();
    drop(tree);
    (dir, repo)
}
fn request(root: &Path, path: &Path, name: &str) -> CreateWorktree {
    CreateWorktree {
        repository: root.into(),
        destination: path.into(),
        branch: name.into(),
        new_branch: true,
        base: "HEAD".into(),
    }
}
fn commit_file(repo: &Repository, name: &str, contents: &str, message: &str) -> git2::Oid {
    std::fs::write(repo.workdir().unwrap().join(name), contents).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(name)).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let signature = Signature::now("Canopy Test", "test@example.invalid").unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent],
    )
    .unwrap()
}
fn shell_environment() -> canopy_desktop::terminal::environment::ShellEnvironment {
    canopy_desktop::terminal::environment::ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: std::env::vars().collect(),
    }
}

#[cfg(unix)]
fn install_hook(repo: &Repository, name: &str, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    let directory = repo.path().join("canopy-test-hooks");
    std::fs::create_dir_all(&directory).unwrap();
    let hook = directory.join(name);
    std::fs::write(&hook, contents).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    repo.config()
        .unwrap()
        .set_str("core.hooksPath", directory.to_str().unwrap())
        .unwrap();
}
fn prune_request(root: &Path, path: &Path) -> git::RemoveWorktree {
    let info = git::inspect(root).unwrap().unwrap();
    let worktree = info.worktrees.iter().find(|w| w.path == path).unwrap();
    git::RemoveWorktree {
        repository: root.into(),
        path: path.into(),
        kind: git::RemovalKind::MissingRegistration {
            name: worktree.name.clone().unwrap(),
        },
    }
}

#[test]
fn missing_worktree_cleanup_removes_only_its_registration_and_keeps_branches() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("missing"), "missing-test")).unwrap();
    let survivor = git::create(&request(
        root,
        &dir.path().join("survivor"),
        "survivor-test",
    ))
    .unwrap();
    std::fs::write(survivor.join("untracked"), "keep").unwrap();
    let prune = prune_request(root, &path);
    std::fs::remove_dir_all(&path).unwrap();
    let info = git::inspect(root).unwrap().unwrap();
    let missing = info.worktrees.iter().find(|w| w.path == path).unwrap();
    assert!(missing.missing && !missing.available);
    assert_eq!(missing.checkout_branch(), Some("missing-test"));
    assert!(missing.tooltip().contains("directory is missing"));
    let mut duplicate = request(root, &dir.path().join("duplicate-missing"), "missing-test");
    duplicate.new_branch = false;
    assert!(git::create(&duplicate).is_err());
    assert!(!duplicate.destination.exists());
    assert_eq!(
        git::remove_worktree(&prune, false).unwrap(),
        git::RemovalOutcome::Removed
    );
    assert_eq!(git::inspect(root).unwrap().unwrap().worktrees.len(), 2);
    assert!(!path.exists());
    assert_eq!(
        std::fs::read_to_string(survivor.join("untracked")).unwrap(),
        "keep"
    );
    assert!(repo.find_branch("missing-test", BranchType::Local).is_ok());
    assert!(repo.find_branch("survivor-test", BranchType::Local).is_ok());
    // A stale UI entry remains dismissible after an external prune as well.
    assert_eq!(
        git::remove_worktree(&prune, false).unwrap(),
        git::RemovalOutcome::Removed
    );
}

#[test]
fn missing_cleanup_rechecks_paths_and_never_falls_back_to_deleting_files() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("reappeared"), "reappeared")).unwrap();
    let prune = prune_request(root, &path);
    assert!(git::remove_worktree(&prune, true).is_err());
    std::fs::remove_dir_all(&path).unwrap();
    // Even a newly created non-Git directory must not be removed by this action.
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("keep"), "replacement").unwrap();
    assert!(git::remove_worktree(&prune, true).is_err());
    assert_eq!(
        std::fs::read_to_string(path.join("keep")).unwrap(),
        "replacement"
    );
    let info = git::inspect(root).unwrap().unwrap();
    let replacement = info.worktrees.iter().find(|w| w.path == path).unwrap();
    assert!(!replacement.missing && !replacement.available);
    assert_eq!(repo.worktrees().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn dangling_symlink_is_not_classified_or_pruned_as_a_missing_directory() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("link"), "link")).unwrap();
    let prune = prune_request(root, &path);
    std::fs::remove_dir_all(&path).unwrap();
    std::os::unix::fs::symlink(dir.path().join("absent"), &path).unwrap();
    assert!(
        !git::inspect(root)
            .unwrap()
            .unwrap()
            .worktrees
            .iter()
            .find(|w| w.path == path)
            .unwrap()
            .missing
    );
    assert!(git::remove_worktree(&prune, true).is_err());
    assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
    assert_eq!(repo.worktrees().unwrap().len(), 1);
}

#[test]
fn missing_cleanup_respects_worktree_and_git_operation_locks() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(
        root,
        &dir.path().join("locked-missing"),
        "locked-missing",
    ))
    .unwrap();
    let prune = prune_request(root, &path);
    let git::RemovalKind::MissingRegistration { name } = &prune.kind else {
        unreachable!()
    };
    let wt = repo.find_worktree(name).unwrap();
    wt.lock(Some("offline worktree")).unwrap();
    std::fs::remove_dir_all(&path).unwrap();
    assert!(git::remove_worktree(&prune, true).is_err());
    wt.unlock().unwrap();
    let lock = repo.path().join("worktrees").join(name).join("index.lock");
    std::fs::write(&lock, "busy").unwrap();
    assert!(git::remove_worktree(&prune, true).is_err());
    assert_eq!(repo.worktrees().unwrap().len(), 1);
    std::fs::remove_file(lock).unwrap();
    assert_eq!(
        git::remove_worktree(&prune, false).unwrap(),
        git::RemovalOutcome::Removed
    );
}

#[cfg(unix)]
#[test]
fn missing_directory_keeps_its_canonical_workspace_path_through_a_parent_alias() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let parent = dir.path().join("real");
    std::fs::create_dir(&parent).unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&parent, &alias).unwrap();
    let path = git::create(&request(root, &parent.join("child"), "aliased")).unwrap();
    let prune = prune_request(root, &path);
    let git::RemovalKind::MissingRegistration { name } = &prune.kind else {
        unreachable!()
    };
    std::fs::write(
        repo.path().join("worktrees").join(name).join("gitdir"),
        alias.join("child/.git").to_string_lossy().as_bytes(),
    )
    .unwrap();
    std::fs::remove_dir_all(&path).unwrap();
    let info = git::inspect(root).unwrap().unwrap();
    let missing = info
        .worktrees
        .iter()
        .find(|w| w.name.as_ref() == Some(name))
        .unwrap();
    assert!(missing.missing);
    assert_eq!(missing.path, path);
    assert_eq!(
        git::remove_worktree(&prune, false).unwrap(),
        git::RemovalOutcome::Removed
    );
}

#[test]
fn cleanup_rejects_a_registration_that_now_points_to_another_worktree() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("old"), "old")).unwrap();
    let other = git::create(&request(root, &dir.path().join("other"), "other")).unwrap();
    let prune = prune_request(root, &path);
    std::fs::remove_dir_all(&path).unwrap();
    let git::RemovalKind::MissingRegistration { name } = &prune.kind else {
        unreachable!()
    };
    std::fs::write(
        repo.path().join("worktrees").join(name).join("gitdir"),
        other.join(".git").to_string_lossy().as_bytes(),
    )
    .unwrap();
    assert!(git::remove_worktree(&prune, false).is_err());
    assert_eq!(repo.worktrees().unwrap().len(), 2);
    assert!(other.join("tracked").exists());
}

#[test]
fn worker_refreshes_cached_worktree_list_after_missing_entry_cleanup() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap().canonicalize().unwrap();
    let path = git::create(&request(&root, &dir.path().join("cached"), "cached")).unwrap();
    let prune = prune_request(&root, &path);
    let client = GitClient::start().unwrap();
    client.watch(vec![root.clone()]).unwrap();
    wait(|| client.snapshots().contains_key(&root));
    std::fs::remove_dir_all(&path).unwrap();
    client.watch(vec![root.clone(), path.clone()]).unwrap();
    wait(|| client.stats.active_watch_failures.load(Ordering::Relaxed) > 0);
    futures_lite::future::block_on(client.remove(prune, false)).unwrap();
    wait(|| {
        client
            .snapshots()
            .get(&root)
            .and_then(|r| r.as_ref().ok())
            .and_then(|r| r.as_ref())
            .is_some_and(|info| info.worktrees.len() == 1)
    });
    // Closing the saved missing workspace removes its failed watcher. A past
    // failure must not keep the warning alive after the current watches recover.
    client.watch(vec![root]).unwrap();
    wait(|| client.stats.active_watch_failures.load(Ordering::Relaxed) == 0);
    assert!(client.stats.watch_failures.load(Ordering::Relaxed) > 0);
    futures_lite::future::block_on(client.shutdown());
}

#[test]
fn worker_releases_the_selected_change_watch_before_worktree_cleanup() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap().canonicalize().unwrap();
    let path = git::create(&request(
        &root,
        &dir.path().join("watched-removal"),
        "watched-removal",
    ))
    .unwrap();
    let client = GitClient::start().unwrap();
    client.watch_changes(Some(path.clone())).unwrap();
    wait(|| client.changes_snapshot(&path).is_some());
    let outcome = futures_lite::future::block_on(client.remove(
        git::RemoveWorktree {
            repository: root,
            path: path.clone(),
            kind: git::RemovalKind::Directory,
        },
        false,
    ))
    .unwrap();
    assert_eq!(outcome, git::RemovalOutcome::Removed);
    assert!(!path.exists());
    futures_lite::future::block_on(client.shutdown());
}

#[test]
fn worker_releases_and_restores_a_watched_merge_target() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap().canonicalize().unwrap();
    let target = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        &root,
        &dir.path().join("watched-target"),
        "watched-target",
    ))
    .unwrap();
    commit_file(
        &Repository::open(&path).unwrap(),
        "feature",
        "content",
        "feature",
    );
    let removal = git::RemoveWorktree {
        repository: root.clone(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let client = GitClient::start().unwrap();
    client.watch_changes(Some(root.clone())).unwrap();
    wait(|| client.changes_snapshot(&root).is_some());
    let analysis = futures_lite::future::block_on(
        client.analyze_worktree(removal.clone(), Some(target.clone())),
    )
    .unwrap();
    let outcome = futures_lite::future::block_on(client.execute_worktree_removal(
        removal,
        git::RemovalAction::Merge {
            target,
            delete_branch: false,
        },
        analysis,
        git::RemovalApproval::default(),
        None,
    ))
    .unwrap();
    assert!(matches!(
        outcome,
        git::WorktreeRemovalOutcome::Finished(ref result)
            if result.merge_performed && result.worktree_removed
    ));
    wait(|| client.changes_snapshot(&root).is_some());
    assert!(root.join("feature").exists());
    futures_lite::future::block_on(client.shutdown());
}
#[test]
fn discovers_main_and_linked_worktrees_without_cli() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("feature"), "feature/test")).unwrap();
    let info = git::inspect(&path).unwrap().unwrap();
    assert_eq!(info.root, root.canonicalize().unwrap());
    assert_eq!(info.worktrees.len(), 2);
    assert!(
        info.worktrees
            .iter()
            .any(|w| w.path == path && w.branch_name() == Some("feature/test"))
    );
    assert!(info.branches.contains(&"feature/test".to_owned()));
    assert_eq!(
        std::fs::read_to_string(path.join("tracked")).unwrap(),
        "initial"
    );
}
#[test]
fn existing_branch_and_duplicate_checkout_guards() {
    let (dir, repo) = fixture();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("existing", &head, false).unwrap();
    let mut req = request(
        repo.workdir().unwrap(),
        &dir.path().join("existing"),
        "existing",
    );
    req.new_branch = false;
    let path = git::create(&req).unwrap();
    req.destination = dir.path().join("duplicate");
    assert!(git::create(&req).is_err());
    assert!(!req.destination.exists());
    assert!(path.exists());
    req.branch = "../bad".into();
    assert!(git::create(&req).is_err());
}
#[test]
fn removal_preserves_branch_and_blocks_local_data() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("remove"), "remove-test")).unwrap();
    std::fs::write(path.join("untracked"), "keep").unwrap();
    assert!(git::remove(root, &path).is_err());
    assert!(path.join("untracked").exists());
    std::fs::remove_file(path.join("untracked")).unwrap();
    std::fs::write(path.join("tracked"), "changed").unwrap();
    assert!(git::remove(root, &path).is_err());
    std::fs::write(path.join("tracked"), "initial").unwrap();
    git::remove(root, &path).unwrap();
    assert!(!path.exists());
    assert!(repo.find_branch("remove-test", BranchType::Local).is_ok());
    assert!(git::remove(root, root).is_err());
}
#[test]
fn ignored_and_locked_worktrees_are_not_removed() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("locked"), "locked-test")).unwrap();
    std::fs::write(repo.path().join("info/exclude"), "ignored\n").unwrap();
    std::fs::write(path.join("ignored"), "keep").unwrap();
    assert!(git::remove(root, &path).is_err());
    std::fs::remove_file(path.join("ignored")).unwrap();
    let linked = Repository::open(&path).unwrap();
    let wt = git2::Worktree::open_from_repository(&linked).unwrap();
    wt.lock(Some("external task")).unwrap();
    assert!(git::remove(root, &path).is_err());
    assert!(path.exists());
    wt.unlock().unwrap();
    git::remove(root, &path).unwrap();
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(6);
    while !condition() {
        assert!(Instant::now() < deadline, "condition timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn worker_is_idle_without_events_and_coalesces_metadata_bursts() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap().canonicalize().unwrap();
    let client = GitClient::start().unwrap();
    client.watch(vec![root.clone()]).unwrap();
    wait(|| client.snapshots().contains_key(&root));
    std::thread::sleep(Duration::from_millis(350));
    let before = client.stats.scans.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(before, client.stats.scans.load(Ordering::Relaxed));
    // Ordinary worktree writes must not trigger Git metadata scans.
    for i in 0..100 {
        std::fs::write(root.join("tracked"), format!("{i}")).unwrap();
    }
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(before, client.stats.scans.load(Ordering::Relaxed));
    let commit = repo.head().unwrap().peel_to_commit().unwrap();
    for i in 0..30 {
        repo.branch(&format!("burst-{i}"), &commit, false).unwrap();
    }
    wait(|| {
        client
            .snapshots()
            .get(&root)
            .and_then(|r| r.as_ref().ok())
            .and_then(|r| r.as_ref())
            .is_some_and(|i| i.branches.contains(&"burst-29".into()))
    });
    let scans = client.stats.scans.load(Ordering::Relaxed) - before;
    assert!(scans <= 5, "metadata burst caused {scans} scans");
    eprintln!("metadata burst: 30 branches, {scans} scans; idle/worktree writes: 0 scans");
    futures_lite::future::block_on(client.shutdown());
    drop(dir);
}
#[test]
fn independent_worktree_workspaces_survive_sqlite_restore() {
    futures_lite::future::block_on(async {
        use canopy_desktop::{
            settings::{Access, SettingsClient},
            state::{
                layout::LayoutState, projects::Projects, session::SessionSnapshot,
                workspace::Workspace,
            },
        };
        let (dir, repo) = fixture();
        let root = repo.workdir().unwrap().canonicalize().unwrap();
        let path =
            git::create(&request(&root, &dir.path().join("persist"), "persist-test")).unwrap();
        let mut projects = Projects::default();
        let a = projects.open_worktree(root.clone(), root.clone());
        let b = projects.open_worktree(root, path.clone());
        let mut main = Workspace::new();
        main.id = a;
        main.open("main", "shell");
        let mut linked = Workspace::new();
        linked.id = b;
        linked.open("linked", "codex");
        linked.set_default_cwd(path.clone());
        let snapshot = SessionSnapshot {
            projects,
            workspaces: vec![main, linked],
            layout: LayoutState::default(),
        };
        assert!(snapshot.valid());
        let db = dir.path().join("state.db");
        let client = SettingsClient::create(&db).await.unwrap();
        client.save_session(snapshot.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&db, Access::ReadOnly).await.unwrap();
        let restored = client.load_session().await.unwrap().unwrap();
        assert_eq!(restored, snapshot);
        assert_eq!(restored.activation_plan()[0].metadata.cwd, Some(path));
        client.shutdown().await.unwrap();
    });
}

#[test]
fn multiple_open_worktrees_share_one_metadata_snapshot() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap().canonicalize().unwrap();
    let linked = git::create(&request(&root, &dir.path().join("shared"), "shared-test")).unwrap();
    let client = GitClient::start().unwrap();
    client.watch(vec![root.clone(), linked.clone()]).unwrap();
    wait(|| client.snapshots().len() == 2);
    assert_eq!(client.stats.scans.load(Ordering::Relaxed), 1);
    let entries = client.snapshots();
    let a = entries[&root].as_ref().unwrap().as_ref().unwrap();
    let b = entries[&linked].as_ref().unwrap().as_ref().unwrap();
    assert!(std::sync::Arc::ptr_eq(a, b));
    futures_lite::future::block_on(client.shutdown());
}

#[test]
fn fresh_empty_worktree_stays_empty_after_restore_and_starts_no_pty() {
    futures_lite::future::block_on(async {
        use canopy_desktop::{
            settings::{Access, SettingsClient},
            state::{
                layout::LayoutState, projects::Projects, session::SessionSnapshot,
                workspace::Workspace,
            },
            terminal::lifecycle,
        };
        let (dir, repo) = fixture();
        let root = repo.workdir().unwrap().canonicalize().unwrap();
        let path = git::create(&request(&root, &dir.path().join("empty"), "empty-test")).unwrap();
        let mut projects = Projects::default();
        let main_id = projects.open_worktree(root.clone(), root.clone());
        let empty_id = projects.open_worktree(root, path);
        let mut main = Workspace::empty(main_id);
        main.open("Existing tab", "shell");
        let empty = Workspace::empty(empty_id);
        let snapshot = SessionSnapshot {
            projects,
            workspaces: vec![main, empty],
            layout: LayoutState::default(),
        };
        assert!(snapshot.valid());
        assert!(snapshot.activation_plan().is_empty());
        let client = SettingsClient::create(&dir.path().join("empty.db"))
            .await
            .unwrap();
        client.save_session(snapshot.clone()).await.unwrap();
        client.shutdown().await.unwrap();
        let client = SettingsClient::open(&dir.path().join("empty.db"), Access::ReadOnly)
            .await
            .unwrap();
        let mut restored = client.load_session().await.unwrap().unwrap();
        assert_eq!(restored, snapshot);
        assert!(
            lifecycle::reconcile(
                &restored.workspaces,
                restored.projects.active,
                &Default::default()
            )
            .start
            .is_empty()
        );
        restored.projects.select(main_id);
        assert_eq!(restored.activation_plan().len(), 1);
        restored.projects.select(empty_id);
        assert!(restored.activation_plan().is_empty());
        client.shutdown().await.unwrap();
    });
}

#[test]
fn dirty_worktree_requires_explicit_discard_and_keeps_branch() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(root, &dir.path().join("confirm"), "confirm-test")).unwrap();
    std::fs::write(path.join("tracked"), "modified").unwrap();
    std::fs::write(path.join("untracked"), "local data").unwrap();
    std::fs::write(repo.path().join("info/exclude"), "ignored\n").unwrap();
    std::fs::write(path.join("ignored"), "ignored data").unwrap();
    assert!(matches!(
        git::remove_confirmed(root, &path, false).unwrap(),
        git::RemovalOutcome::NeedsDiscardConfirmation(3)
    ));
    // Cancelling is no mutation: all data and registration still exist.
    assert_eq!(
        std::fs::read_to_string(path.join("tracked")).unwrap(),
        "modified"
    );
    assert!(path.join("untracked").exists());
    assert!(path.join("ignored").exists());
    assert_eq!(
        git::remove_confirmed(root, &path, true).unwrap(),
        git::RemovalOutcome::Removed
    );
    assert!(!path.exists());
    assert!(repo.find_branch("confirm-test", BranchType::Local).is_ok());
}
#[test]
fn discard_confirmation_does_not_bypass_lock_or_main_tree_guards() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(
        root,
        &dir.path().join("still-locked"),
        "still-locked",
    ))
    .unwrap();
    std::fs::write(path.join("tracked"), "modified").unwrap();
    let linked = Repository::open(&path).unwrap();
    let wt = git2::Worktree::open_from_repository(&linked).unwrap();
    wt.lock(Some("protected")).unwrap();
    assert!(git::remove_confirmed(root, &path, true).is_err());
    assert!(path.exists());
    assert!(git::remove_confirmed(root, root, true).is_err());
}

#[test]
fn detached_worktrees_use_directory_context_and_keep_commit_identity() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let oid = repo.head().unwrap().target().unwrap();
    let mut paths = vec![];
    for (container, branch) in [("bb23", "scratch/one"), ("044b", "scratch/two")] {
        let parent = dir.path().join(container);
        std::fs::create_dir(&parent).unwrap();
        let path = git::create(&request(root, &parent.join("main"), branch)).unwrap();
        Repository::open(&path)
            .unwrap()
            .set_head_detached(oid)
            .unwrap();
        paths.push((path, container));
    }
    let info = git::inspect(root).unwrap().unwrap();
    assert!(info.worktrees[0].branch_name().is_some());
    for (path, container) in paths {
        let worktree = info.worktrees.iter().find(|w| w.path == path).unwrap();
        assert_eq!(worktree.branch_name(), None);
        assert_eq!(worktree.label, format!("main ({container})"));
        assert_eq!(worktree.head, git::WorktreeHead::Detached(oid.to_string()));
        assert!(worktree.tooltip().contains(&path.display().to_string()));
        assert!(worktree.tooltip().contains("Detached HEAD"));
        assert!(worktree.tooltip().contains(&oid.to_string()[..8]));
    }
    // A detached checkout does not reserve its former branch name.
    let mut req = request(root, &dir.path().join("reuse"), "scratch/one");
    req.new_branch = false;
    assert!(git::create(&req).is_ok());
}
#[test]
fn descriptive_detached_directory_and_unavailable_worktree_keep_useful_labels() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let oid = repo.head().unwrap().target().unwrap();
    let path = git::create(&request(
        root,
        &dir.path().join("review-materials"),
        "feature/review",
    ))
    .unwrap();
    Repository::open(&path)
        .unwrap()
        .set_head_detached(oid)
        .unwrap();
    let info = git::inspect(&path).unwrap().unwrap();
    let w = info.worktrees.iter().find(|w| w.path == path).unwrap();
    assert_eq!(w.label, "review-materials");
    std::fs::rename(&path, dir.path().join("moved-away")).unwrap();
    let info = git::inspect(root).unwrap().unwrap();
    let w = info.worktrees.iter().find(|w| w.path == path).unwrap();
    assert_eq!(w.label, "review-materials");
    assert_eq!(w.head, git::WorktreeHead::Unavailable);
    assert!(!w.available);
}
#[test]
fn unborn_main_and_detached_main_have_explicit_head_states() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("empty-project");
    let repo = Repository::init(&root).unwrap();
    repo.set_head("refs/heads/fresh-start").unwrap();
    let info = git::inspect(&root).unwrap().unwrap();
    assert_eq!(info.worktrees[0].label, "fresh-start");
    assert_eq!(
        info.worktrees[0].head,
        git::WorktreeHead::Unborn("fresh-start".into())
    );
    let (_dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let info = git::inspect(root).unwrap().unwrap();
    assert_eq!(info.worktrees[0].label, "main");
    assert_eq!(
        info.worktrees[0].head,
        git::WorktreeHead::Detached(oid.to_string())
    );
}

#[test]
fn managed_destination_is_stable_for_request_and_regenerates_after_collision() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let proposed = git::propose_worktree_path(root).unwrap();
    let name = proposed.file_name().unwrap().to_string_lossy();
    let repository_name = root.file_name().unwrap().to_string_lossy();
    assert!(name.starts_with(&format!("{repository_name}-")));
    assert_eq!(name.len(), repository_name.len() + 11);
    std::fs::create_dir(&proposed).unwrap();
    let created = git::create_with_metadata(&CreateWorktree {
        repository: root.into(),
        destination: proposed.clone(),
        branch: "collision-safe".into(),
        new_branch: true,
        base: "HEAD".into(),
    })
    .unwrap();
    assert_ne!(created.path, proposed);
    assert!(proposed.is_dir());
    assert!(created.path.is_dir());
    assert_eq!(
        created.base.unwrap().oid,
        repo.head().unwrap().target().unwrap().to_string()
    );
    drop(dir);
}

#[cfg(unix)]
#[test]
fn managed_destination_never_overwrites_a_symlink() {
    use std::os::unix::fs::symlink;
    let (dir, repo) = fixture();
    let proposed = git::propose_worktree_path(repo.workdir().unwrap()).unwrap();
    let target = dir.path().join("target");
    std::fs::create_dir(&target).unwrap();
    symlink(&target, &proposed).unwrap();
    let created = git::create_with_metadata(&CreateWorktree {
        repository: repo.workdir().unwrap().into(),
        destination: proposed.clone(),
        branch: "symlink-safe".into(),
        new_branch: true,
        base: "HEAD".into(),
    })
    .unwrap();
    assert_ne!(created.path, proposed);
    assert_eq!(std::fs::read_link(&proposed).unwrap(), target);
}

#[test]
fn branch_deletion_requires_named_target_confirmation_and_runs_after_cleanup() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(root, &dir.path().join("delete"), "delete-me")).unwrap();
    let linked = Repository::open(&path).unwrap();
    commit_file(&linked, "feature", "one", "feature");
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    assert_eq!(analysis.target.as_ref().unwrap().commits_not_in_target, 1);
    let action = git::RemovalAction::DeleteBranch { target: main };
    assert!(matches!(
        git::execute_worktree_removal(
            &removal,
            &action,
            &analysis,
            git::RemovalApproval::default(),
            None
        )
        .unwrap(),
        git::WorktreeRemovalOutcome::NeedsBranchConfirmation(_)
    ));
    assert!(path.exists());
    let outcome = git::execute_worktree_removal(
        &removal,
        &action,
        &analysis,
        git::RemovalApproval {
            delete_unmerged_branch: true,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::Finished(result) = outcome else {
        panic!("expected completion")
    };
    assert!(result.worktree_removed && result.branch_deleted && result.error.is_none());
    assert!(repo.find_branch("delete-me", BranchType::Local).is_err());
}

#[test]
fn fast_forward_merge_updates_checked_out_target_before_removal() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(root, &dir.path().join("ff"), "feature-ff")).unwrap();
    let linked = Repository::open(&path).unwrap();
    let source = commit_file(&linked, "feature", "fast-forward", "feature");
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    assert_eq!(
        analysis.target.as_ref().unwrap().merge,
        git::MergeKind::FastForward
    );
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main.clone(),
            delete_branch: true,
        },
        &analysis,
        git::RemovalApproval::default(),
        None,
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::Finished(result) = outcome else {
        panic!("expected completion")
    };
    assert!(result.merge_performed && result.worktree_removed && result.branch_deleted);
    assert!(repo.find_branch("feature-ff", BranchType::Local).is_err());
    assert_eq!(repo.head().unwrap().target(), Some(source));
    assert_eq!(
        std::fs::read_to_string(root.join("feature")).unwrap(),
        "fast-forward"
    );
}

#[test]
fn divergent_merge_creates_two_parent_commit_then_removes_worktree() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(root, &dir.path().join("merge"), "feature-merge")).unwrap();
    let linked = Repository::open(&path).unwrap();
    let source = commit_file(&linked, "source", "source", "source");
    let old_target = commit_file(&repo, "target", "target", "target");
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    assert_eq!(
        analysis.target.as_ref().unwrap().merge,
        git::MergeKind::MergeCommit
    );
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main.clone(),
            delete_branch: true,
        },
        &analysis,
        git::RemovalApproval::default(),
        None,
    )
    .unwrap();
    assert!(
        matches!(outcome, git::WorktreeRemovalOutcome::Finished(ref result) if result.merge_performed && result.worktree_removed)
    );
    let git::WorktreeRemovalOutcome::Finished(result) = outcome else {
        unreachable!()
    };
    let refreshed = Repository::open(root).unwrap();
    let merge_oid = git2::Oid::from_str(result.merge_target_oid.as_deref().unwrap()).unwrap();
    let merge = refreshed.find_commit(merge_oid).unwrap();
    assert_eq!(merge.parent_count(), 2);
    assert_eq!(merge.parent_id(0).unwrap(), old_target);
    assert_eq!(merge.parent_id(1).unwrap(), source);
    assert_eq!(
        refreshed
            .refname_to_id(&format!("refs/heads/{main}"))
            .unwrap(),
        merge.id()
    );
    assert!(
        refreshed
            .find_branch("feature-merge", BranchType::Local)
            .is_err()
    );
}

#[test]
fn conflicting_merge_analysis_does_not_mutate_refs_or_remove_worktree() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("conflict"),
        "feature-conflict",
    ))
    .unwrap();
    let linked = Repository::open(&path).unwrap();
    let source = commit_file(&linked, "tracked", "source", "source");
    let target = commit_file(&repo, "tracked", "target", "target");
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    assert!(
        matches!(analysis.target.as_ref().unwrap().merge, git::MergeKind::Conflicts(ref paths) if paths == &[Path::new("tracked").to_path_buf()])
    );
    assert!(
        git::execute_worktree_removal(
            &removal,
            &git::RemovalAction::Merge {
                target: main,
                delete_branch: true
            },
            &analysis,
            git::RemovalApproval::default(),
            None
        )
        .is_err()
    );
    assert_eq!(repo.head().unwrap().target(), Some(target));
    assert_eq!(linked.head().unwrap().target(), Some(source));
    assert!(path.exists());
}

#[test]
fn branch_oid_change_invalidates_analysis_before_any_mutation() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(root, &dir.path().join("race"), "feature-race")).unwrap();
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    commit_file(&Repository::open(&path).unwrap(), "late", "late", "late");
    assert!(matches!(
        git::execute_worktree_removal(
            &removal,
            &git::RemovalAction::DeleteBranch { target: main },
            &analysis,
            git::RemovalApproval {
                delete_unmerged_branch: true,
                ..Default::default()
            },
            None
        )
        .unwrap(),
        git::WorktreeRemovalOutcome::AnalysisChanged(_)
    ));
    assert!(path.exists());
    assert!(repo.find_branch("feature-race", BranchType::Local).is_ok());
}

#[test]
fn lock_change_after_analysis_blocks_before_merge_or_cleanup() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let target = repo.head().unwrap().target();
    let path = git::create(&request(
        root,
        &dir.path().join("partial"),
        "feature-partial",
    ))
    .unwrap();
    let _source = commit_file(
        &Repository::open(&path).unwrap(),
        "partial",
        "merged",
        "partial",
    );
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let info = git::inspect(root).unwrap().unwrap();
    let name = info
        .worktrees
        .iter()
        .find(|item| item.path == path)
        .unwrap()
        .name
        .as_ref()
        .unwrap();
    repo.find_worktree(name)
        .unwrap()
        .lock(Some("test"))
        .unwrap();
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main,
            delete_branch: false,
        },
        &analysis,
        git::RemovalApproval::default(),
        None,
    );
    assert!(outcome.is_err());
    assert_eq!(repo.head().unwrap().target(), target);
    assert!(path.exists());
}

#[test]
fn dirty_source_or_checked_out_target_blocks_merge_before_publication() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(root, &dir.path().join("dirty"), "feature-dirty")).unwrap();
    let linked = Repository::open(&path).unwrap();
    let source = commit_file(&linked, "feature", "feature", "feature");
    let target = repo.head().unwrap().target().unwrap();
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    std::fs::write(path.join("tracked"), "dirty source").unwrap();
    let changed = git::analyze_worktree(&removal, Some(&main)).unwrap();
    assert!(
        git::execute_worktree_removal(
            &removal,
            &git::RemovalAction::Merge {
                target: main.clone(),
                delete_branch: false
            },
            &changed,
            git::RemovalApproval {
                discard_changes: true,
                ..Default::default()
            },
            None
        )
        .is_err()
    );
    assert_eq!(repo.head().unwrap().target(), Some(target));
    std::fs::write(path.join("tracked"), "initial").unwrap();
    std::fs::write(root.join("target-local"), "untracked").unwrap();
    let refreshed = git::analyze_worktree(&removal, Some(&main)).unwrap();
    assert!(
        git::execute_worktree_removal(
            &removal,
            &git::RemovalAction::Merge {
                target: main,
                delete_branch: false
            },
            &refreshed,
            git::RemovalApproval::default(),
            None
        )
        .is_err()
    );
    assert_eq!(repo.head().unwrap().target(), Some(target));
    assert_eq!(linked.head().unwrap().target(), Some(source));
    assert!(path.exists());
    assert!(matches!(
        refreshed.target.as_ref().unwrap().merge,
        git::MergeKind::Blocked(_)
    ));
    assert_ne!(analysis, refreshed);
}

#[cfg(unix)]
#[test]
fn rejecting_pre_merge_hook_keeps_both_refs_and_worktree() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(root, &dir.path().join("hook"), "feature-hook")).unwrap();
    let linked = Repository::open(&path).unwrap();
    let source = commit_file(&linked, "source", "source", "source");
    let target = commit_file(&repo, "target", "target", "target");
    install_hook(&repo, "pre-merge-commit", "#!/bin/sh\nexit 1\n");
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let env = canopy_desktop::terminal::environment::ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: std::env::vars().collect(),
    };
    assert!(
        git::execute_worktree_removal(
            &removal,
            &git::RemovalAction::Merge {
                target: main,
                delete_branch: false
            },
            &analysis,
            git::RemovalApproval::default(),
            Some(&env)
        )
        .is_err()
    );
    assert_eq!(repo.head().unwrap().target(), Some(target));
    assert_eq!(linked.head().unwrap().target(), Some(source));
    assert!(path.exists());
    let target_repo = Repository::open(root).unwrap();
    assert_eq!(target_repo.state(), git2::RepositoryState::Merge);
    assert_eq!(
        std::fs::read_to_string(target_repo.path().join("MERGE_HEAD"))
            .unwrap()
            .trim(),
        source.to_string()
    );
    assert_eq!(
        std::fs::read_to_string(target_repo.path().join("ORIG_HEAD"))
            .unwrap()
            .trim(),
        target.to_string()
    );
}

#[cfg(unix)]
#[test]
fn post_merge_hook_failure_is_warning_after_published_merge() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("post-hook"),
        "feature-post-hook",
    ))
    .unwrap();
    let linked = Repository::open(&path).unwrap();
    commit_file(&linked, "source", "source", "source");
    commit_file(&repo, "target", "target", "target");
    let hook_dir = repo.path().join("canopy-test-hooks");
    std::fs::create_dir(&hook_dir).unwrap();
    let hook = hook_dir.join("post-merge");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    repo.config()
        .unwrap()
        .set_str("core.hooksPath", hook_dir.to_str().unwrap())
        .unwrap();
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let env = canopy_desktop::terminal::environment::ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: std::env::vars().collect(),
    };
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main,
            delete_branch: false,
        },
        &analysis,
        git::RemovalApproval::default(),
        Some(&env),
    )
    .unwrap();
    assert!(
        matches!(outcome, git::WorktreeRemovalOutcome::Finished(ref result) if result.merge_performed && result.worktree_removed && result.warning.is_some() && result.error.is_none())
    );
    assert!(!path.exists());
}

#[test]
fn advanced_keep_branch_path_requires_discard_and_preserves_branch() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(
        root,
        &dir.path().join("advanced-discard"),
        "keep-after-discard",
    ))
    .unwrap();
    std::fs::write(path.join("untracked"), "local").unwrap();
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, None).unwrap();
    assert!(matches!(
        git::execute_worktree_removal(
            &removal,
            &git::RemovalAction::KeepBranch,
            &analysis,
            git::RemovalApproval::default(),
            None
        )
        .unwrap(),
        git::WorktreeRemovalOutcome::NeedsDiscardConfirmation(1)
    ));
    assert!(path.join("untracked").exists());
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::KeepBranch,
        &analysis,
        git::RemovalApproval {
            discard_changes: true,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert!(
        matches!(outcome, git::WorktreeRemovalOutcome::Finished(ref result) if result.worktree_removed && !result.branch_deleted && result.error.is_none())
    );
    assert!(
        repo.find_branch("keep-after-discard", BranchType::Local)
            .is_ok()
    );
}

#[test]
fn branch_retry_refuses_a_branch_checked_out_again_elsewhere() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("retry-source"),
        "retry-source",
    ))
    .unwrap();
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main))
        .unwrap()
        .target
        .unwrap();
    git::remove_confirmed(root, &path, true).unwrap();
    let mut reopen = request(root, &dir.path().join("retry-other"), "retry-source");
    reopen.new_branch = false;
    let other = git::create(&reopen).unwrap();
    assert!(git::delete_branch_after_removal(root, &analysis).is_err());
    assert!(other.exists());
    assert!(repo.find_branch("retry-source", BranchType::Local).is_ok());
}

#[test]
fn merge_to_non_checked_out_branch_updates_only_that_local_ref() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let initial = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("integration", &initial, false).unwrap();
    let main_oid = initial.id();
    drop(initial);
    let path = git::create(&request(
        root,
        &dir.path().join("background-target"),
        "background-source",
    ))
    .unwrap();
    let source = commit_file(
        &Repository::open(&path).unwrap(),
        "source",
        "source",
        "source",
    );
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some("integration")).unwrap();
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: "integration".into(),
            delete_branch: false,
        },
        &analysis,
        git::RemovalApproval::default(),
        None,
    )
    .unwrap();
    assert!(
        matches!(outcome, git::WorktreeRemovalOutcome::Finished(ref result) if result.merge_performed && result.worktree_removed)
    );
    assert_eq!(
        repo.refname_to_id("refs/heads/integration").unwrap(),
        source
    );
    assert_eq!(repo.head().unwrap().target(), Some(main_oid));
}

#[test]
fn merge_to_linked_target_locks_and_preserves_that_worktrees_head() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let main_oid = repo.head().unwrap().target().unwrap();
    let target_path = git::create(&request(
        root,
        &dir.path().join("linked-target"),
        "integration-target",
    ))
    .unwrap();
    let source_path = git::create(&request(
        root,
        &dir.path().join("linked-source"),
        "integration-source",
    ))
    .unwrap();
    let source_oid = commit_file(
        &Repository::open(&source_path).unwrap(),
        "linked-result",
        "linked",
        "linked",
    );
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: source_path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some("integration-target")).unwrap();
    assert_eq!(
        analysis.target.as_ref().unwrap().merge,
        git::MergeKind::FastForward
    );
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: "integration-target".into(),
            delete_branch: false,
        },
        &analysis,
        git::RemovalApproval::default(),
        None,
    )
    .unwrap();
    assert!(
        matches!(outcome, git::WorktreeRemovalOutcome::Finished(ref result) if result.merge_performed && result.worktree_removed && result.error.is_none())
    );
    let target = Repository::open(&target_path).unwrap();
    assert_eq!(
        target.head().unwrap().shorthand().unwrap(),
        "integration-target"
    );
    assert_eq!(target.head().unwrap().target(), Some(source_oid));
    assert_eq!(
        std::fs::read_to_string(target_path.join("linked-result")).unwrap(),
        "linked"
    );
    let refreshed_main = Repository::open(root).unwrap();
    assert_eq!(refreshed_main.head().unwrap().shorthand().unwrap(), main);
    assert_eq!(refreshed_main.head().unwrap().target(), Some(main_oid));
}

#[cfg(unix)]
#[test]
fn signing_failure_never_publishes_unsigned_merge_or_runs_cleanup() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(root, &dir.path().join("signed"), "signed-source")).unwrap();
    let linked = Repository::open(&path).unwrap();
    let source = commit_file(&linked, "source", "source", "source");
    let target = commit_file(&repo, "target", "target", "target");
    install_hook(&repo, "pre-merge-commit", "#!/bin/sh\nexit 0\n");
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "unsupported-test").unwrap();
    drop(config);
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let env = canopy_desktop::terminal::environment::ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: std::env::vars().collect(),
    };
    assert!(
        git::execute_worktree_removal(
            &removal,
            &git::RemovalAction::Merge {
                target: main,
                delete_branch: false
            },
            &analysis,
            git::RemovalApproval::default(),
            Some(&env)
        )
        .is_err()
    );
    assert_eq!(repo.head().unwrap().target(), Some(target));
    assert_eq!(linked.head().unwrap().target(), Some(source));
    assert!(path.exists());
    let target_repo = Repository::open(root).unwrap();
    assert_eq!(target_repo.state(), git2::RepositoryState::Merge);
    assert_eq!(
        std::fs::read_to_string(target_repo.path().join("MERGE_HEAD"))
            .unwrap()
            .trim(),
        source.to_string()
    );
}

#[cfg(unix)]
#[test]
fn inaccessible_destination_parent_does_not_create_a_branch() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = fixture();
    let parent = dir.path().join("closed-parent");
    std::fs::create_dir(&parent).unwrap();
    let original = std::fs::metadata(&parent).unwrap().permissions();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
    let outcome = git::create(&CreateWorktree {
        repository: repo.workdir().unwrap().into(),
        destination: parent.join("worktree"),
        branch: "permission-safe".into(),
        new_branch: true,
        base: "HEAD".into(),
    });
    std::fs::set_permissions(&parent, original).unwrap();
    assert!(outcome.is_err());
    assert!(!parent.join("worktree").exists());
}

#[test]
fn discard_approval_is_invalidated_by_same_count_content_and_path_changes() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let path = git::create(&request(
        root,
        &dir.path().join("fingerprint"),
        "fingerprint-source",
    ))
    .unwrap();
    std::fs::write(path.join("draft-a"), "first").unwrap();
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let action = git::RemovalAction::KeepBranch;
    let initial = git::analyze_worktree(&removal, None).unwrap();
    assert!(matches!(
        git::execute_worktree_removal(
            &removal,
            &action,
            &initial,
            git::RemovalApproval::default(),
            None
        )
        .unwrap(),
        git::WorktreeRemovalOutcome::NeedsDiscardConfirmation(1)
    ));

    std::fs::write(path.join("draft-a"), "second").unwrap();
    let changed = git::execute_worktree_removal(
        &removal,
        &action,
        &initial,
        git::RemovalApproval {
            discard_changes: true,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::AnalysisChanged(changed) = changed else {
        panic!("content change must invalidate approval")
    };
    assert_eq!(changed.changed_entries, initial.changed_entries);
    assert_ne!(changed.cleanup_fingerprint, initial.cleanup_fingerprint);
    assert!(path.exists());

    std::fs::remove_file(path.join("draft-a")).unwrap();
    std::fs::write(path.join("draft-b"), "second").unwrap();
    let replaced = git::execute_worktree_removal(
        &removal,
        &action,
        &changed,
        git::RemovalApproval {
            discard_changes: true,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::AnalysisChanged(replaced) = replaced else {
        panic!("path replacement must invalidate approval")
    };
    assert_eq!(replaced.changed_entries, changed.changed_entries);
    assert_ne!(replaced.cleanup_fingerprint, changed.cleanup_fingerprint);
    assert!(path.exists());
}

#[cfg(unix)]
#[test]
fn merge_hook_sees_prepared_tree_and_staged_changes_enter_merge_commit() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("hook-tree"),
        "hook-tree-source",
    ))
    .unwrap();
    let linked = Repository::open(&path).unwrap();
    commit_file(&linked, "source-result", "source", "source");
    commit_file(&repo, "target-result", "target", "target");
    install_hook(
        &repo,
        "pre-merge-commit",
        "#!/bin/sh\ntest \"$(cat source-result)\" = source || exit 21\nprintf hook > hook-added\ngit add hook-added\n",
    );
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main.clone(),
            delete_branch: false,
        },
        &analysis,
        git::RemovalApproval::default(),
        Some(&shell_environment()),
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::Finished(result) = outcome else {
        panic!("expected completion")
    };
    assert!(result.merge_performed && result.worktree_removed && result.error.is_none());
    let refreshed = Repository::open(root).unwrap();
    let commit = refreshed
        .find_commit(
            refreshed
                .refname_to_id(&format!("refs/heads/{main}"))
                .unwrap(),
        )
        .unwrap();
    assert!(
        commit
            .tree()
            .unwrap()
            .get_path(Path::new("source-result"))
            .is_ok()
    );
    assert!(
        commit
            .tree()
            .unwrap()
            .get_path(Path::new("hook-added"))
            .is_ok()
    );
    assert_eq!(
        std::fs::read_to_string(root.join("hook-added")).unwrap(),
        "hook"
    );
}

#[cfg(unix)]
#[test]
fn source_change_during_merge_hook_preserves_worktree_after_publication() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("hook-source-race"),
        "hook-source-race",
    ))
    .unwrap();
    let linked = Repository::open(&path).unwrap();
    commit_file(&linked, "source-result", "source", "source");
    commit_file(&repo, "target-result", "target", "target");
    std::fs::write(path.join("draft"), "approved").unwrap();
    install_hook(
        &repo,
        "pre-merge-commit",
        &format!(
            "#!/bin/sh\nprintf changed > '{}'\n",
            path.join("draft").display()
        ),
    );
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main.clone(),
            delete_branch: true,
        },
        &analysis,
        git::RemovalApproval {
            discard_changes: true,
            ..Default::default()
        },
        Some(&shell_environment()),
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::Finished(result) = outcome else {
        panic!("expected partial result")
    };
    assert!(result.merge_performed && !result.worktree_removed && !result.branch_deleted);
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("files changed"))
    );
    assert!(path.exists());
    assert_eq!(
        std::fs::read_to_string(path.join("draft")).unwrap(),
        "changed"
    );
    let retry_analysis = result.retry_analysis.clone().unwrap();
    let retry = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main.clone(),
            delete_branch: true,
        },
        &retry_analysis,
        git::RemovalApproval::default(),
        Some(&shell_environment()),
    )
    .unwrap();
    assert!(matches!(
        retry,
        git::WorktreeRemovalOutcome::NeedsDiscardConfirmation(1)
    ));
    assert!(path.exists());
    assert_eq!(
        Repository::open(root)
            .unwrap()
            .refname_to_id(&format!("refs/heads/{main}"))
            .unwrap()
            .to_string(),
        result.merge_target_oid.unwrap()
    );
}

#[cfg(unix)]
#[test]
fn target_branch_switch_in_hook_is_detected_before_publication() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("head-race"),
        "head-race-source",
    ))
    .unwrap();
    let linked = Repository::open(&path).unwrap();
    commit_file(&linked, "source-result", "source", "source");
    let old_target = commit_file(&repo, "target-result", "target", "target");
    let target_commit = repo.find_commit(old_target).unwrap();
    repo.branch("other-target", &target_commit, false).unwrap();
    drop(target_commit);
    install_hook(
        &repo,
        "pre-merge-commit",
        "#!/bin/sh\ngit reset --hard -q\ngit checkout -q other-target\n",
    );
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main.clone(),
            delete_branch: false,
        },
        &analysis,
        git::RemovalApproval::default(),
        Some(&shell_environment()),
    );
    assert!(outcome.is_err());
    let refreshed = Repository::open(root).unwrap();
    assert_eq!(
        refreshed
            .refname_to_id(&format!("refs/heads/{main}"))
            .unwrap(),
        old_target
    );
    assert_eq!(
        refreshed.head().unwrap().shorthand().unwrap(),
        "other-target"
    );
    assert!(path.exists());
}

#[cfg(unix)]
#[test]
fn post_merge_setup_failure_is_warning_after_fast_forward_publication() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("post-setup"),
        "post-setup-source",
    ))
    .unwrap();
    let source = commit_file(
        &Repository::open(&path).unwrap(),
        "source-result",
        "source",
        "source",
    );
    install_hook(&repo, "post-merge", "#!/bin/sh\nexit 0\n");
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main,
            delete_branch: false,
        },
        &analysis,
        git::RemovalApproval::default(),
        None,
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::Finished(result) = outcome else {
        panic!("expected completion")
    };
    assert!(result.merge_performed && result.worktree_removed && result.error.is_none());
    assert!(
        result
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("post-merge"))
    );
    assert_eq!(
        Repository::open(root).unwrap().head().unwrap().target(),
        Some(source)
    );
}

#[cfg(unix)]
#[test]
fn analysis_failure_after_published_merge_returns_partial_result() {
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_owned();
    let path = git::create(&request(
        root,
        &dir.path().join("post-analysis"),
        "post-analysis-source",
    ))
    .unwrap();
    let source_repo = Repository::open(&path).unwrap();
    let source = commit_file(&source_repo, "source-result", "source", "source");
    let source_lock = source_repo.path().join("index.lock");
    install_hook(
        &repo,
        "post-merge",
        &format!("#!/bin/sh\n: > '{}'\n", source_lock.display()),
    );
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some(&main)).unwrap();
    let outcome = git::execute_worktree_removal(
        &removal,
        &git::RemovalAction::Merge {
            target: main.clone(),
            delete_branch: true,
        },
        &analysis,
        git::RemovalApproval::default(),
        Some(&shell_environment()),
    )
    .unwrap();
    let git::WorktreeRemovalOutcome::Finished(result) = outcome else {
        panic!("expected partial result")
    };
    assert!(result.merge_performed && !result.worktree_removed && !result.branch_deleted);
    assert_eq!(
        result.merge_target_oid.as_deref(),
        Some(source.to_string().as_str())
    );
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("could not be analyzed"))
    );
    assert!(path.exists());
    assert_eq!(
        Repository::open(root)
            .unwrap()
            .refname_to_id(&format!("refs/heads/{main}"))
            .unwrap(),
        source
    );
}

#[cfg(unix)]
#[test]
fn initially_unchecked_target_becoming_checked_out_during_signing_aborts_publication() {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    let (dir, repo) = fixture();
    let root = repo.workdir().unwrap();
    let target_path = git::create(&request(
        root,
        &dir.path().join("sign-target-prep"),
        "sign-target",
    ))
    .unwrap();
    let target_oid = commit_file(
        &Repository::open(&target_path).unwrap(),
        "target-result",
        "target",
        "target",
    );
    git::remove_confirmed(root, &target_path, true).unwrap();
    let source_path = git::create(&request(
        root,
        &dir.path().join("sign-source"),
        "sign-source",
    ))
    .unwrap();
    commit_file(
        &Repository::open(&source_path).unwrap(),
        "source-result",
        "source",
        "source",
    );
    let control = tempfile::tempdir().unwrap();
    let ready = control.path().join("ready");
    let signer = control.path().join("signer");
    std::fs::write(&signer, format!("#!/bin/sh\n: > '{}'\n/bin/sleep 0.3\nprintf '%s\\n' '-----BEGIN PGP SIGNATURE-----' 'test' '-----END PGP SIGNATURE-----'\n", ready.display())).unwrap();
    std::fs::set_permissions(&signer, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "openpgp").unwrap();
    config
        .set_str("gpg.openpgp.program", signer.to_str().unwrap())
        .unwrap();
    drop(config);
    let removal = git::RemoveWorktree {
        repository: root.into(),
        path: source_path.clone(),
        kind: git::RemovalKind::Directory,
    };
    let analysis = git::analyze_worktree(&removal, Some("sign-target")).unwrap();
    assert_eq!(
        analysis.target.as_ref().unwrap().merge,
        git::MergeKind::MergeCommit
    );
    let removal_thread = removal.clone();
    let operation = std::thread::spawn(move || {
        git::execute_worktree_removal(
            &removal_thread,
            &git::RemovalAction::Merge {
                target: "sign-target".into(),
                delete_branch: false,
            },
            &analysis,
            git::RemovalApproval::default(),
            Some(&shell_environment()),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut checkout = request(root, &dir.path().join("sign-target-race"), "sign-target");
    checkout.new_branch = false;
    let checkout_path = git::create(&checkout).unwrap();
    let error = operation.join().unwrap().unwrap_err();
    assert!(error.contains("became checked out"));
    assert_eq!(
        Repository::open(root)
            .unwrap()
            .refname_to_id("refs/heads/sign-target")
            .unwrap(),
        target_oid
    );
    assert_eq!(
        Repository::open(&checkout_path)
            .unwrap()
            .head()
            .unwrap()
            .target(),
        Some(target_oid)
    );
    assert!(source_path.exists());
}
