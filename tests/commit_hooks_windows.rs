#![cfg(windows)]

use canopy_desktop::{
    git::changes::{self, Edit},
    terminal::environment::ShellEnvironment,
};
use git2::Repository;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn fixture() -> (tempfile::TempDir, Repository, ShellEnvironment) {
    let directory = tempfile::tempdir().unwrap();
    let repo = Repository::init(directory.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Windows Hook Test").unwrap();
    config
        .set_str("user.email", "hook@example.invalid")
        .unwrap();
    config.set_bool("commit.gpgsign", false).unwrap();
    config.set_str("core.hooksPath", "custom hooks").unwrap();
    drop(config);
    std::fs::create_dir(directory.path().join("custom hooks")).unwrap();
    std::fs::write(directory.path().join("file"), "staged\n").unwrap();
    changes::edit(directory.path(), Edit::Stage(None)).unwrap();
    let environment = ShellEnvironment::load().unwrap();
    (directory, repo, environment)
}

fn hook(root: &Path, name: &str, body: &str) {
    std::fs::write(root.join("custom hooks").join(name), body).unwrap();
}

#[test]
fn git_for_windows_shebang_runs_hooks_and_preserves_editor_contract() {
    let (directory, repo, environment) = fixture();
    hook(
        directory.path(),
        "pre-commit",
        "#!/bin/sh\nset -eu\n[ \"$GIT_EDITOR\" = : ]\nprintf 'pre\\n' >> order\n",
    );
    hook(
        directory.path(),
        "commit-msg",
        "#!/usr/bin/env sh\nset -eu\nprintf 'message\\n' >> order\nprintf '\\nwindows body\\n' >> \"$1\"\n",
    );
    let result = changes::commit_with_hooks(
        directory.path(),
        "draft",
        None,
        &environment,
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(directory.path().join("order")).unwrap(),
        "pre\nmessage\n"
    );
    assert_eq!(
        repo.find_commit(result.id).unwrap().message().unwrap(),
        "draft\n\nwindows body"
    );
}

#[test]
fn missing_windows_hook_interpreter_stops_before_publication() {
    let (directory, repo, environment) = fixture();
    hook(
        directory.path(),
        "pre-commit",
        "#!/usr/bin/env canopy-missing-interpreter\nexit 0\n",
    );
    let error = changes::commit(
        directory.path(),
        "draft",
        None,
        &environment,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.contains("interpreter"));
    assert!(repo.head().is_err());
}

#[test]
fn cancellation_stops_windows_hook_descendants_and_preserves_index() {
    let (directory, repo, environment) = fixture();
    hook(
        directory.path(),
        "pre-commit",
        "#!/bin/sh\ntouch started\n(sleep 30) &\nwait\n",
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let root = directory.path().to_owned();
    let trigger = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("started").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        signal.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let error =
        changes::commit(directory.path(), "draft", None, &environment, &cancel).unwrap_err();
    trigger.join().unwrap();
    assert!(error.contains("Cancelled"));
    assert!(started.elapsed() < Duration::from_secs(6));
    assert!(repo.head().is_err());
    assert_eq!(repo.index().unwrap().len(), 1);
}
