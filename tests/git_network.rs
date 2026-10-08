use canopy_desktop::git::network::{self, Operation, Upstream};
use git2::{Repository, Signature};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
fn commit(repo: &Repository, text: &str) -> git2::Oid {
    std::fs::write(repo.workdir().unwrap().join("file.txt"), text).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("file.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = Signature::now("Test", "test@example.invalid").unwrap();
    let parents: Vec<_> = repo
        .head()
        .ok()
        .map(|h| h.peel_to_commit().unwrap())
        .into_iter()
        .collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        text,
        &tree,
        &parents.iter().collect::<Vec<_>>(),
    )
    .unwrap()
}
fn run(path: &Path, operation: Operation, upstream: Option<Upstream>) -> Result<String, String> {
    network::execute(
        network::prepare(path.to_owned())?,
        operation,
        upstream,
        Arc::new(AtomicBool::new(false)),
    )
}
fn fixture() -> (tempfile::TempDir, Repository, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let bare = Repository::init_bare(dir.path().join("remote.git")).unwrap();
    bare.set_head("refs/heads/main").unwrap();
    let repo = Repository::init(dir.path().join("local")).unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.remote("origin", bare.path().to_str().unwrap())
        .unwrap();
    commit(&repo, "initial");
    (dir, repo, bare)
}
#[test]
fn publish_custom_upstream_push_and_fast_forward_pull() {
    let (dir, repo, remote) = fixture();
    let path = repo.workdir().unwrap();
    let target = Upstream {
        remote: "origin".into(),
        branch: "published".into(),
    };
    assert!(
        network::prepare(path.to_owned())
            .unwrap()
            .upstream
            .is_none()
    );
    run(path, Operation::Push, Some(target.clone())).unwrap();
    assert_eq!(
        network::prepare(path.to_owned()).unwrap().upstream,
        Some(target)
    );
    assert_eq!(
        remote.refname_to_id("refs/heads/published").unwrap(),
        repo.head().unwrap().target().unwrap()
    );
    remote.set_head("refs/heads/published").unwrap();
    let peer = Repository::clone(remote.path().to_str().unwrap(), dir.path().join("peer")).unwrap();
    let next = commit(&peer, "remote advance");
    run(peer.workdir().unwrap(), Operation::Push, None).unwrap();
    run(path, Operation::Pull, None).unwrap();
    assert_eq!(repo.head().unwrap().target(), Some(next));
    assert_eq!(
        std::fs::read_to_string(path.join("file.txt")).unwrap(),
        "remote advance"
    );
    assert!(repo.statuses(None).unwrap().is_empty());
    run(path, Operation::Pull, None).unwrap();
}
#[test]
fn rejected_push_does_not_set_upstream_and_dirty_pull_preserves_files() {
    let (dir, repo, remote) = fixture();
    let path = repo.workdir().unwrap();
    run(
        path,
        Operation::Push,
        Some(Upstream {
            remote: "origin".into(),
            branch: "main".into(),
        }),
    )
    .unwrap();
    let peer = Repository::clone(remote.path().to_str().unwrap(), dir.path().join("peer")).unwrap();
    commit(&peer, "peer change");
    run(peer.workdir().unwrap(), Operation::Push, None).unwrap();
    std::fs::write(path.join("file.txt"), "uncommitted").unwrap();
    assert!(
        run(path, Operation::Pull, None)
            .unwrap_err()
            .contains("stash")
    );
    assert_eq!(
        std::fs::read_to_string(path.join("file.txt")).unwrap(),
        "uncommitted"
    );
    commit(&repo, "local diverged");
    assert!(
        run(path, Operation::Pull, None)
            .unwrap_err()
            .contains("diverged")
    );
    Repository::open(path)
        .unwrap()
        .find_branch("main", git2::BranchType::Local)
        .unwrap()
        .set_upstream(None)
        .unwrap();
    assert!(
        run(
            path,
            Operation::Push,
            Some(Upstream {
                remote: "origin".into(),
                branch: "main".into()
            })
        )
        .is_err()
    );
    assert!(
        network::prepare(path.to_owned())
            .unwrap()
            .upstream
            .is_none()
    );
    assert_eq!(
        std::fs::read_to_string(path.join("file.txt")).unwrap(),
        "local diverged"
    );
}
#[test]
fn stale_plan_cancellation_and_invalid_target_never_publish() {
    let (_dir, repo, remote) = fixture();
    let path = repo.workdir().unwrap();
    let plan = network::prepare(path.to_owned()).unwrap();
    let target = Some(Upstream {
        remote: "origin".into(),
        branch: "main".into(),
    });
    assert!(
        network::execute(
            plan.clone(),
            Operation::Push,
            target.clone(),
            Arc::new(AtomicBool::new(true))
        )
        .is_err()
    );
    commit(&repo, "later");
    assert!(
        network::execute(
            plan,
            Operation::Push,
            target,
            Arc::new(AtomicBool::new(false))
        )
        .unwrap_err()
        .contains("changed")
    );
    assert!(
        run(
            path,
            Operation::Push,
            Some(Upstream {
                remote: "origin".into(),
                branch: "invalid:ref".into()
            })
        )
        .is_err()
    );
    assert!(remote.find_reference("refs/heads/main").is_err());
}

#[test]
fn publish_extends_single_branch_fetch_mapping() {
    let (_dir, repo, _remote) = fixture();
    let path = repo.workdir().unwrap();
    repo.config()
        .unwrap()
        .set_str(
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/main",
        )
        .unwrap();
    run(
        path,
        Operation::Push,
        Some(Upstream {
            remote: "origin".into(),
            branch: "new-feature".into(),
        }),
    )
    .unwrap();
    let reopened = Repository::open(path).unwrap();
    assert_eq!(
        reopened
            .find_branch("main", git2::BranchType::Local)
            .unwrap()
            .upstream()
            .unwrap()
            .get()
            .target(),
        repo.head().unwrap().target()
    );
}

#[test]
fn remote_change_and_active_hook_are_not_silently_accepted() {
    let (_dir, repo, remote) = fixture();
    let path = repo.workdir().unwrap();
    let plan = network::prepare(path.to_owned()).unwrap();
    repo.remote_set_url("origin", "https://example.invalid/changed.git")
        .unwrap();
    let target = Some(Upstream {
        remote: "origin".into(),
        branch: "main".into(),
    });
    assert!(
        network::execute(
            plan,
            Operation::Push,
            target.clone(),
            Arc::new(AtomicBool::new(false))
        )
        .unwrap_err()
        .contains("URL changed")
    );
    repo.remote_set_url("origin", remote.path().to_str().unwrap())
        .unwrap();
    {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        let hook = repo.path().join("hooks/pre-push");
        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            run(path, Operation::Push, target)
                .unwrap_err()
                .contains("pre-push")
        );
        assert!(remote.find_reference("refs/heads/main").is_err());
    }
}

#[test]
fn pull_does_not_overwrite_ignored_files() {
    let (dir, repo, remote) = fixture();
    let path = repo.workdir().unwrap();
    std::fs::write(path.join(".gitignore"), "*.cache\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(".gitignore")).unwrap();
    index.write().unwrap();
    drop(index);
    commit(&repo, "ignore cache");
    run(
        path,
        Operation::Push,
        Some(Upstream {
            remote: "origin".into(),
            branch: "main".into(),
        }),
    )
    .unwrap();
    let peer = Repository::clone(remote.path().to_str().unwrap(), dir.path().join("peer")).unwrap();
    std::fs::write(
        peer.workdir().unwrap().join("local.cache"),
        "remote contents",
    )
    .unwrap();
    let mut index = peer.index().unwrap();
    index.add_path(Path::new("local.cache")).unwrap();
    index.write().unwrap();
    drop(index);
    commit(&peer, "track cache file");
    run(peer.workdir().unwrap(), Operation::Push, None).unwrap();
    let original = repo.head().unwrap().target();
    std::fs::write(path.join("local.cache"), "private local contents").unwrap();
    assert!(run(path, Operation::Pull, None).is_err());
    assert_eq!(
        std::fs::read_to_string(path.join("local.cache")).unwrap(),
        "private local contents"
    );
    assert_eq!(repo.head().unwrap().target(), original);
}
