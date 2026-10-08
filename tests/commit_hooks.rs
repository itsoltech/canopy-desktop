#![cfg(unix)]
use canopy_desktop::{
    git::changes::{self, Edit},
    terminal::environment::ShellEnvironment,
};
use git2::Repository;
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn fixture() -> (tempfile::TempDir, Repository, ShellEnvironment) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Hook Test").unwrap();
    config
        .set_str("user.email", "hook@example.invalid")
        .unwrap();
    config.set_bool("commit.gpgsign", false).unwrap();
    config.set_str("core.hooksPath", "custom hooks").unwrap();
    drop(config);
    std::fs::create_dir(dir.path().join("custom hooks")).unwrap();
    std::fs::write(dir.path().join("file"), "staged\n").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let env = ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: HashMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("GIT_DIR".into(), "/wrong/repository".into()),
        ]),
    };
    (dir, repo, env)
}
fn hook(root: &Path, name: &str, body: &str) {
    let path = root.join("custom hooks").join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
#[test]
fn hooks_run_in_order_and_can_edit_index_and_message() {
    let (dir, repo, env) = fixture();
    hook(
        dir.path(),
        "pre-commit",
        "#!/bin/sh\nset -eu\n[ $# = 0 ]\n[ \"$GIT_EDITOR\" = : ]\n[ \"$GIT_AUTHOR_NAME\" = 'Hook Test' ]\nprintf 'pre\\n' >> order\nprintf 'formatted\\n' > file\ngit add file\necho pre-output\n",
    );
    hook(
        dir.path(),
        "prepare-commit-msg",
        "#!/bin/sh\nset -eu\n[ $# = 2 ]\n[ \"$2\" = message ]\nprintf 'prepare\\n' >> order\nprintf 'prepared\\n' > \"$1\"\n",
    );
    hook(
        dir.path(),
        "commit-msg",
        "#!/bin/sh\nset -eu\n[ $# = 1 ]\nprintf 'message\\n' >> order\nprintf '\\nbody\\n' >> \"$1\"\n",
    );
    hook(
        dir.path(),
        "post-commit",
        "#!/bin/sh\nset -eu\n[ $# = 0 ]\n[ ! -e \"$GIT_INDEX_FILE.lock\" ]\nprintf 'post\\n' >> order\ngit log -1 --format=%s > published\n",
    );
    let result = changes::commit_with_hooks(
        dir.path(),
        "draft",
        None,
        &env,
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap();
    assert!(result.warning.is_none());
    assert_eq!(result.hooks.len(), 4);
    assert!(result.hooks[0].output.contains("pre-output"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("order")).unwrap(),
        "pre\nprepare\nmessage\npost\n"
    );
    let commit = repo.find_commit(result.id).unwrap();
    assert_eq!(commit.message().unwrap(), "prepared\n\nbody");
    let tree = commit.tree().unwrap();
    let blob = repo.find_blob(tree.get_name("file").unwrap().id()).unwrap();
    assert_eq!(blob.content(), b"formatted\n");
    assert!(tree.get_name("order").is_none());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("published")).unwrap(),
        "prepared\n"
    );
}
#[test]
fn failed_validation_stops_before_publication() {
    let (dir, repo, env) = fixture();
    hook(
        dir.path(),
        "commit-msg",
        "#!/bin/sh\necho 'message rejected' >&2\nexit 7\n",
    );
    hook(dir.path(), "post-commit", "#!/bin/sh\ntouch post-ran\n");
    let error =
        changes::commit(dir.path(), "draft", None, &env, &AtomicBool::new(false)).unwrap_err();
    assert!(error.contains("commit-msg"));
    assert!(error.contains("message rejected"));
    assert!(repo.head().is_err());
    assert!(!dir.path().join("post-ran").exists());
}
#[test]
fn post_commit_failure_is_success_with_a_warning() {
    let (dir, repo, env) = fixture();
    hook(
        dir.path(),
        "post-commit",
        "#!/bin/sh\necho 'notification failed' >&2\nexit 2\n",
    );
    let result = changes::commit_with_hooks(
        dir.path(),
        "draft",
        None,
        &env,
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap();
    assert_eq!(repo.head().unwrap().target(), Some(result.id));
    assert!(
        result
            .warning
            .unwrap()
            .contains("was created, but post-commit")
    );
}
#[test]
fn non_executable_hooks_are_ignored_and_shell_hooks_without_shebang_work() {
    let (dir, repo, env) = fixture();
    let path = dir.path().join("custom hooks/pre-commit");
    std::fs::write(&path, "exit 1\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    hook(dir.path(), "commit-msg", "printf 'from shell' > \"$1\"\n");
    let id = changes::commit(dir.path(), "draft", None, &env, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        repo.find_commit(id).unwrap().message().unwrap(),
        "from shell"
    );
}
#[test]
fn cancellation_stops_a_running_hook_and_preserves_staged_changes() {
    let (dir, repo, env) = fixture();
    hook(
        dir.path(),
        "pre-commit",
        "#!/bin/sh\ntouch started\nsleep 30\n",
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let root = dir.path().to_owned();
    let trigger = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("started").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        flag.store(true, Ordering::Release);
    });
    let start = Instant::now();
    let error = changes::commit(dir.path(), "draft", None, &env, &cancel).unwrap_err();
    trigger.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(6));
    assert!(error.contains("Cancelled"));
    assert!(repo.head().is_err());
    assert_eq!(repo.index().unwrap().len(), 1);
}

#[test]
fn successful_hook_does_not_wait_for_a_descendant_holding_output_pipes() {
    let (dir, repo, env) = fixture();
    hook(
        dir.path(),
        "pre-commit",
        "#!/bin/sh\n(sleep 30) &\necho parent-finished\n",
    );
    let started = Instant::now();
    let result = changes::commit_with_hooks(
        dir.path(),
        "draft",
        None,
        &env,
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(result.hooks[0].output.contains("parent-finished"));
    assert_eq!(repo.head().unwrap().target(), Some(result.id));
}
#[test]
fn head_changes_inside_hooks_are_not_overwritten() {
    let (dir, repo, env) = fixture();
    hook(
        dir.path(),
        "pre-commit",
        "#!/bin/sh\ngit -c core.hooksPath=/dev/null commit -m 'hook commit'\n",
    );
    let error =
        changes::commit(dir.path(), "draft", None, &env, &AtomicBool::new(false)).unwrap_err();
    assert!(error.contains("HEAD"));
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap(),
        "hook commit\n"
    );
}

#[test]
fn linked_worktree_uses_shared_hooks_and_its_own_index() {
    let (dir, repo, env) = fixture();
    let mut index = repo.index().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    let initial = repo
        .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("linked");
    repo.worktree("linked", &path, None).unwrap();
    let hooks = dir.path().join("custom hooks");
    repo.config()
        .unwrap()
        .set_str("core.hooksPath", hooks.to_str().unwrap())
        .unwrap();
    hook(
        dir.path(),
        "pre-commit",
        "#!/bin/sh\nset -eu\ncase \"$GIT_DIR\" in */worktrees/*) ;; *) exit 12;; esac\nprintf 'linked\\n' > file\ngit add file\n",
    );
    std::fs::write(path.join("file"), "change").unwrap();
    changes::edit(&path, Edit::Stage(None)).unwrap();
    let id = changes::commit(
        &path,
        "linked commit",
        Some(&initial.to_string()),
        &env,
        &AtomicBool::new(false),
    )
    .unwrap();
    let linked = Repository::open(&path).unwrap();
    assert_eq!(linked.head().unwrap().target(), Some(id));
    assert_eq!(repo.head().unwrap().target(), Some(initial));
    let tree = linked.find_commit(id).unwrap().tree().unwrap();
    assert_eq!(
        linked
            .find_blob(tree.get_name("file").unwrap().id())
            .unwrap()
            .content(),
        b"linked\n"
    );
}

#[test]
fn cancellation_after_publication_keeps_the_commit() {
    let (dir, repo, env) = fixture();
    hook(dir.path(), "post-commit", "#!/bin/sh\necho notification\n");
    let cancel = AtomicBool::new(false);
    let report = |phase: &str| {
        if phase.contains("post-commit") {
            cancel.store(true, Ordering::Release);
        }
    };
    let outcome =
        changes::commit_with_hooks(dir.path(), "published", None, &env, &cancel, &report).unwrap();
    assert_eq!(repo.head().unwrap().target(), Some(outcome.id));
    assert!(outcome.warning.unwrap().contains("Cancelled"));
}
