use canopy_desktop::{
    git::changes::{self, Edit},
    terminal::environment::ShellEnvironment,
};
use git2::{Oid, Repository};
use std::{collections::HashMap, path::Path, sync::atomic::AtomicBool};
fn fixture() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Canopy Test").unwrap();
    config
        .set_str("user.email", "test@example.invalid")
        .unwrap();
    config.set_bool("commit.gpgsign", false).unwrap();
    config
        .set_str(
            "core.hooksPath",
            repo.path().join("hooks").to_str().unwrap(),
        )
        .unwrap();
    drop(config);
    (dir, repo)
}
fn env() -> ShellEnvironment {
    ShellEnvironment {
        shell: "/bin/sh".into(),
        vars: HashMap::from([("PATH".into(), "/usr/bin:/bin:/opt/homebrew/bin".into())]),
    }
}
fn commit(path: &Path, message: &str) -> Oid {
    let snapshot = changes::status(path).unwrap();
    changes::commit(
        path,
        message,
        snapshot.head.as_deref(),
        &env(),
        &AtomicBool::new(false),
    )
    .unwrap()
}
#[test]
fn e2e_stage_diff_commit_unstage_and_discard() {
    let (dir, repo) = fixture();
    let path = dir.path();
    std::fs::write(path.join("file.txt"), "one\ntwo\n").unwrap();
    let snapshot = changes::status(path).unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert!(!snapshot.files[0].staged);
    assert!(
        changes::diff(path, &snapshot.files[0])
            .unwrap()
            .lines
            .iter()
            .any(|l| l.kind == '+' && l.text == "one")
    );
    changes::edit(path, Edit::Stage(None)).unwrap();
    let first = commit(path, "Initial");
    assert_eq!(repo.head().unwrap().target(), Some(first));
    assert!(changes::status(path).unwrap().files.is_empty());
    std::fs::write(path.join("file.txt"), "one\nchanged\n").unwrap();
    let file = changes::status(path).unwrap().files[0].clone();
    changes::edit(path, Edit::Stage(Some(file))).unwrap();
    std::fs::write(path.join("file.txt"), "one\nnewer\n").unwrap();
    let snapshot = changes::status(path).unwrap();
    assert_eq!(snapshot.files.len(), 2);
    let staged = snapshot.files.iter().find(|f| f.staged).unwrap();
    let diff = changes::diff(path, staged).unwrap();
    assert!(
        diff.lines
            .iter()
            .any(|l| l.kind == '+' && l.text == "changed")
    );
    let second = commit(path, "Only staged");
    let commit = repo.find_commit(second).unwrap();
    assert_eq!(commit.parent_id(0).unwrap(), first);
    let tree = commit.tree().unwrap();
    let blob = repo
        .find_blob(tree.get_path(Path::new("file.txt")).unwrap().id())
        .unwrap();
    assert_eq!(blob.content(), b"one\nchanged\n");
    let file = changes::status(path).unwrap().files[0].clone();
    changes::edit(path, Edit::Discard(file)).unwrap();
    assert_eq!(
        std::fs::read_to_string(path.join("file.txt")).unwrap(),
        "one\nchanged\n"
    );
    std::fs::write(path.join("new.txt"), "local").unwrap();
    changes::edit(path, Edit::Stage(None)).unwrap();
    changes::edit(path, Edit::Unstage(None)).unwrap();
    let snapshot = changes::status(path).unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert!(!snapshot.files[0].staged);
    changes::edit(path, Edit::Discard(snapshot.files[0].clone())).unwrap();
    assert!(!path.join("new.txt").exists());
}
#[test]
fn staged_rename_and_deleted_file_unstage_preserve_working_tree() {
    let (dir, _) = fixture();
    let path = dir.path();
    std::fs::write(
        path.join("old.txt"),
        "enough identical text for rename detection\n",
    )
    .unwrap();
    changes::edit(path, Edit::Stage(None)).unwrap();
    commit(path, "Initial");
    std::fs::rename(path.join("old.txt"), path.join("new.txt")).unwrap();
    changes::edit(path, Edit::Stage(None)).unwrap();
    let snapshot = changes::status(path).unwrap();
    assert!(snapshot.files.iter().any(|f| f.kind == "R"));
    let file = snapshot
        .files
        .iter()
        .find(|f| f.kind == "R")
        .unwrap()
        .clone();
    changes::edit(path, Edit::Unstage(Some(file))).unwrap();
    assert!(!path.join("old.txt").exists());
    assert!(path.join("new.txt").exists());
    assert!(
        changes::status(path)
            .unwrap()
            .files
            .iter()
            .all(|f| !f.staged)
    );
}
#[test]
fn binary_diff_and_paths_are_guarded() {
    let (dir, _) = fixture();
    let path = dir.path();
    std::fs::write(path.join("image.bin"), b"\0\x01\x02").unwrap();
    let file = changes::status(path).unwrap().files[0].clone();
    assert!(changes::diff(path, &file).unwrap().binary);
    let mut invalid = file;
    invalid.path = "../outside".into();
    assert!(changes::edit(path, Edit::Discard(invalid.clone())).is_err());
    assert!(changes::diff(path, &invalid).is_err());
}
#[test]
fn empty_staging_cancellation_and_failed_signing_never_publish() {
    let (dir, repo) = fixture();
    let path = dir.path();
    assert!(changes::commit(path, "Empty", None, &env(), &AtomicBool::new(false)).is_err());
    std::fs::write(path.join("a"), "data").unwrap();
    changes::edit(path, Edit::Stage(None)).unwrap();
    assert!(changes::commit(path, "Cancel", None, &env(), &AtomicBool::new(true)).is_err());
    assert!(repo.head().is_err());
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "openpgp").unwrap();
    cfg.set_str("gpg.openpgp.program", "/usr/bin/false")
        .unwrap();
    drop(cfg);
    assert!(
        changes::commit(
            path,
            "Must be signed",
            None,
            &env(),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert!(repo.head().is_err());
    assert!(
        changes::status(path)
            .unwrap()
            .files
            .iter()
            .any(|f| f.staged)
    );
}
#[cfg(unix)]
#[test]
fn ssh_signed_commit_verifies_with_disposable_key() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let keys = tempfile::tempdir().unwrap();
    let key = keys.path().join("key");
    assert!(
        Command::new("/usr/bin/ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", ""])
            .arg("-f")
            .arg(&key)
            .status()
            .unwrap()
            .success()
    );
    let (dir, repo) = fixture();
    let path = dir.path();
    std::fs::write(path.join("a"), "signed\n").unwrap();
    changes::edit(path, Edit::Stage(None)).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "ssh").unwrap();
    cfg.set_str("gpg.ssh.program", "/usr/bin/ssh-keygen")
        .unwrap();
    cfg.set_str("user.signingkey", key.to_str().unwrap())
        .unwrap();
    drop(cfg);
    let oid = commit(path, "Signed SSH");
    let (signature, body) = repo.extract_signature(&oid, None).unwrap();
    let sigfile = keys.path().join("sig");
    std::fs::write(&sigfile, &*signature).unwrap();
    let allowed = keys.path().join("allowed");
    std::fs::write(
        &allowed,
        format!(
            "test@example.invalid {}",
            std::fs::read_to_string(key.with_extension("pub")).unwrap()
        ),
    )
    .unwrap();
    let mut verify = Command::new("/usr/bin/ssh-keygen")
        .args([
            "-Y",
            "verify",
            "-n",
            "git",
            "-I",
            "test@example.invalid",
            "-f",
        ])
        .arg(allowed)
        .arg("-s")
        .arg(sigfile)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    verify.stdin.take().unwrap().write_all(&body).unwrap();
    assert!(verify.wait().unwrap().success());
}

#[test]
fn literal_pathspec_does_not_change_neighboring_files() {
    let (dir, _) = fixture();
    let root = dir.path();
    for name in ["literal[1].txt", "literal1.txt"] {
        std::fs::write(root.join(name), "old").unwrap();
    }
    changes::edit(root, Edit::Stage(None)).unwrap();
    commit(root, "Initial");
    for name in ["literal[1].txt", "literal1.txt"] {
        std::fs::write(root.join(name), "new").unwrap();
    }
    changes::edit(root, Edit::Stage(None)).unwrap();
    let file = changes::status(root)
        .unwrap()
        .files
        .into_iter()
        .find(|f| f.path == Path::new("literal[1].txt"))
        .unwrap();
    changes::edit(root, Edit::Unstage(Some(file))).unwrap();
    let status = changes::status(root).unwrap();
    assert!(
        status
            .files
            .iter()
            .any(|f| f.path == Path::new("literal1.txt") && f.staged)
    );
    let file = status
        .files
        .into_iter()
        .find(|f| f.path == Path::new("literal[1].txt") && !f.staged)
        .unwrap();
    changes::edit(root, Edit::Discard(file)).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("literal[1].txt")).unwrap(),
        "old"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("literal1.txt")).unwrap(),
        "new"
    );
}
#[cfg(unix)]
#[test]
fn cancelling_a_waiting_signer_does_not_publish() {
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{Arc, atomic::Ordering},
        time::{Duration, Instant},
    };
    let signer = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(signer.path(), "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    std::fs::set_permissions(signer.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let (dir, repo) = fixture();
    std::fs::write(dir.path().join("file"), "content").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "openpgp").unwrap();
    cfg.set_str("gpg.openpgp.program", signer.path().to_str().unwrap())
        .unwrap();
    drop(cfg);
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        signal.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let result = changes::commit(dir.path(), "Cancel", None, &env(), &cancel);
    trigger.join().unwrap();
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(repo.head().is_err());
}
#[test]
#[ignore = "creates an isolated GPG test key; requires local gpg and gpgconf"]
fn openpgp_signed_commit_verifies_without_using_user_keys() {
    use std::process::{Command, Stdio};
    let home = tempfile::tempdir().unwrap();
    let gpg = "/opt/homebrew/bin/gpg";
    let gpgconf = "/opt/homebrew/bin/gpgconf";
    struct Cleanup<'a>(&'a Path, &'a str);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = Command::new(self.1)
                .arg("--homedir")
                .arg(self.0)
                .args(["--kill", "gpg-agent"])
                .status();
        }
    }
    let _cleanup = Cleanup(home.path(), gpgconf);
    assert!(
        Command::new(gpg)
            .arg("--homedir")
            .arg(home.path())
            .args([
                "--batch",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "",
                "--quick-generate-key",
                "Canopy Test <test@example.invalid>",
                "ed25519",
                "sign",
                "0"
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let (dir, repo) = fixture();
    std::fs::write(dir.path().join("a"), "signed").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "openpgp").unwrap();
    cfg.set_str("gpg.openpgp.program", gpg).unwrap();
    cfg.set_str("user.signingkey", "test@example.invalid")
        .unwrap();
    drop(cfg);
    let mut environment = env();
    environment.vars.insert(
        "GNUPGHOME".into(),
        home.path().to_string_lossy().into_owned(),
    );
    let oid = changes::commit(
        dir.path(),
        "GPG signed",
        None,
        &environment,
        &AtomicBool::new(false),
    )
    .unwrap();
    let (sig, body) = repo.extract_signature(&oid, None).unwrap();
    let signature = home.path().join("signature.asc");
    let message = home.path().join("body");
    std::fs::write(&signature, &*sig).unwrap();
    std::fs::write(&message, &*body).unwrap();
    assert!(
        Command::new(gpg)
            .arg("--homedir")
            .arg(home.path())
            .arg("--verify")
            .arg(signature)
            .arg(message)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
}

#[cfg(unix)]
#[test]
fn index_changed_while_signing_aborts_publication() {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    let control = tempfile::tempdir().unwrap();
    let ready = control.path().join("ready");
    let script = control.path().join("signer");
    std::fs::write(&script,format!("#!/bin/sh\n/usr/bin/touch '{}'\n/bin/sleep 0.3\nprintf '%s\\n' '-----BEGIN PGP SIGNATURE-----' 'test' '-----END PGP SIGNATURE-----'\n",ready.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (dir, repo) = fixture();
    std::fs::write(dir.path().join("file"), "before").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "openpgp").unwrap();
    cfg.set_str("gpg.openpgp.program", script.to_str().unwrap())
        .unwrap();
    drop(cfg);
    let path = dir.path().to_path_buf();
    let signing = std::thread::spawn(move || {
        changes::commit(
            &path,
            "Racing commit",
            None,
            &env(),
            &AtomicBool::new(false),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::write(dir.path().join("file"), "after").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let error = signing.join().unwrap().unwrap_err();
    assert!(error.contains("index changed"));
    assert!(repo.head().is_err());
}
#[test]
fn active_changes_watch_coalesces_and_skips_ignored_files() {
    use canopy_desktop::git::GitClient;
    use std::{
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };
    fn wait(mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !f() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let (dir, _) = fixture();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
    std::fs::create_dir(root.join("ignored")).unwrap();
    std::fs::write(root.join("file"), "0").unwrap();
    changes::edit(&root, Edit::Stage(None)).unwrap();
    commit(&root, "Initial");
    let client = GitClient::start().unwrap();
    client.watch(vec![root.clone()]).unwrap();
    client.watch_changes(Some(root.clone())).unwrap();
    wait(|| client.changes_snapshot(&root).is_some());
    std::thread::sleep(Duration::from_millis(600));
    let before = client.stats.status_scans.load(Ordering::Relaxed);
    for i in 0..100 {
        std::fs::write(root.join("ignored/build"), i.to_string()).unwrap();
    }
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(before, client.stats.status_scans.load(Ordering::Relaxed));
    for i in 0..100 {
        std::fs::write(root.join("file"), i.to_string()).unwrap();
    }
    wait(|| client.stats.status_scans.load(Ordering::Relaxed) > before);
    std::thread::sleep(Duration::from_millis(500));
    let scans = client.stats.status_scans.load(Ordering::Relaxed) - before;
    assert!(scans <= 3, "{scans} scans for one burst");
    client.watch_changes(None).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let before = client.stats.status_scans.load(Ordering::Relaxed);
    std::fs::write(root.join("file"), "hidden").unwrap();
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(before, client.stats.status_scans.load(Ordering::Relaxed));
    futures_lite::future::block_on(client.shutdown());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated encrypted GPG key and test pinentry; requires GnuPG"]
fn encrypted_gpg_key_uses_agent_pinentry() {
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let home = tempfile::tempdir().unwrap();
    let pinentry = home.path().join("pinentry-test");
    let marker = home.path().join("pin-requested");
    std::fs::write(&pinentry,format!("#!/bin/sh\nprintf 'OK\\n'\nwhile IFS= read -r command; do\n case \"$command\" in\n GETPIN*) /usr/bin/touch '{}'; printf 'D test-only-passphrase\\nOK\\n' ;;\n BYE*) printf 'OK\\n'; exit 0 ;;\n *) printf 'OK\\n' ;;\n esac\ndone\n",marker.display())).unwrap();
    std::fs::set_permissions(&pinentry, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        home.path().join("gpg-agent.conf"),
        format!("pinentry-program {}\n", pinentry.display()),
    )
    .unwrap();
    let gpg = "/opt/homebrew/bin/gpg";
    let gpgconf = "/opt/homebrew/bin/gpgconf";
    struct Cleanup<'a>(&'a Path, &'a str);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = Command::new(self.1)
                .arg("--homedir")
                .arg(self.0)
                .args(["--kill", "gpg-agent"])
                .status();
        }
    }
    let _cleanup = Cleanup(home.path(), gpgconf);
    assert!(
        Command::new(gpg)
            .arg("--homedir")
            .arg(home.path())
            .args([
                "--batch",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "test-only-passphrase",
                "--quick-generate-key",
                "Canopy Test <test@example.invalid>",
                "ed25519",
                "sign",
                "0"
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new(gpgconf)
            .arg("--homedir")
            .arg(home.path())
            .args(["--kill", "gpg-agent"])
            .status()
            .unwrap()
            .success()
    );
    let (dir, repo) = fixture();
    std::fs::write(dir.path().join("file"), "content").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "openpgp").unwrap();
    cfg.set_str("gpg.openpgp.program", gpg).unwrap();
    cfg.set_str("user.signingkey", "test@example.invalid")
        .unwrap();
    drop(cfg);
    let mut environment = env();
    environment.vars.insert(
        "GNUPGHOME".into(),
        home.path().to_string_lossy().into_owned(),
    );
    let oid = changes::commit(
        dir.path(),
        "Encrypted key",
        None,
        &environment,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(marker.exists());
    assert!(repo.extract_signature(&oid, None).is_ok());
}

#[cfg(unix)]
#[test]
fn encrypted_ssh_key_uses_configured_askpass() {
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let keys = tempfile::tempdir().unwrap();
    let key = keys.path().join("key");
    let marker = keys.path().join("asked");
    let askpass = keys.path().join("askpass");
    assert!(
        Command::new("/usr/bin/ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "test-only-passphrase"])
            .arg("-f")
            .arg(&key)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        &askpass,
        format!(
            "#!/bin/sh\n/usr/bin/touch '{}'\nprintf '%s\\n' 'test-only-passphrase'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&askpass, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (dir, repo) = fixture();
    std::fs::write(dir.path().join("file"), "data").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "ssh").unwrap();
    cfg.set_str("gpg.ssh.program", "/usr/bin/ssh-keygen")
        .unwrap();
    cfg.set_str("user.signingkey", key.to_str().unwrap())
        .unwrap();
    drop(cfg);
    let mut environment = env();
    environment
        .vars
        .insert("SSH_ASKPASS".into(), askpass.to_string_lossy().into_owned());
    let oid = changes::commit(
        dir.path(),
        "SSH askpass",
        None,
        &environment,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(marker.exists());
    let (sig, body) = repo.extract_signature(&oid, None).unwrap();
    let sigpath = keys.path().join("sig");
    let bodypath = keys.path().join("body");
    std::fs::write(&sigpath, &*sig).unwrap();
    std::fs::write(&bodypath, &*body).unwrap();
    let body = std::fs::File::open(bodypath).unwrap();
    assert!(
        Command::new("/usr/bin/ssh-keygen")
            .args(["-Y", "check-novalidate", "-n", "git", "-s"])
            .arg(sigpath)
            .stdin(body)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
}

#[cfg(unix)]
#[test]
fn ssh_public_key_signs_through_agent_and_agent_failure_is_not_unsigned() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let keys = tempfile::tempdir().unwrap();
    let key = keys.path().join("key");
    let socket = keys.path().join("agent.sock");
    assert!(
        Command::new("/usr/bin/ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", ""])
            .arg("-f")
            .arg(&key)
            .status()
            .unwrap()
            .success()
    );
    struct AgentGuard(std::process::Child);
    impl Drop for AgentGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut agent = AgentGuard(
        Command::new("/usr/bin/ssh-agent")
            .args(["-D", "-a"])
            .arg(&socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !socket.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        Command::new("/usr/bin/ssh-add")
            .arg(&key)
            .env("SSH_AUTH_SOCK", &socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let (dir, repo) = fixture();
    std::fs::write(dir.path().join("file"), "one").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let mut cfg = repo.config().unwrap();
    cfg.set_bool("commit.gpgsign", true).unwrap();
    cfg.set_str("gpg.format", "ssh").unwrap();
    cfg.set_str("gpg.ssh.program", "/usr/bin/ssh-keygen")
        .unwrap();
    cfg.set_str(
        "user.signingkey",
        &format!(
            "key::{}",
            std::fs::read_to_string(key.with_extension("pub"))
                .unwrap()
                .trim()
        ),
    )
    .unwrap();
    drop(cfg);
    let mut environment = env();
    environment.vars.insert(
        "SSH_AUTH_SOCK".into(),
        socket.to_string_lossy().into_owned(),
    );
    let oid = changes::commit(
        dir.path(),
        "Agent signed",
        None,
        &environment,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(repo.extract_signature(&oid, None).is_ok());
    agent.0.kill().unwrap();
    agent.0.wait().unwrap();
    std::fs::write(dir.path().join("file"), "two").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    assert!(
        changes::commit(
            dir.path(),
            "No agent",
            Some(&oid.to_string()),
            &environment,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    assert_eq!(repo.head().unwrap().target(), Some(oid));
}

#[test]
fn diff_metadata_survives_session_restore_without_starting_a_terminal() {
    use canopy_desktop::state::{
        layout::LayoutState,
        projects::Projects,
        session::SessionSnapshot,
        workspace::{PaneKind, PaneMetadata, Workspace},
    };
    let (dir, _) = fixture();
    std::fs::write(dir.path().join("file"), "data").unwrap();
    let file = changes::status(dir.path()).unwrap().files[0].clone();
    let mut projects = Projects::default();
    let id = projects.open(dir.path().canonicalize().unwrap());
    let mut workspace = Workspace::empty(id);
    let tab = workspace.open("file · diff", "diff");
    let pane = workspace.active().unwrap().focused;
    workspace
        .set_pane_metadata(
            tab,
            pane,
            PaneMetadata {
                kind: PaneKind::Diff,
                cwd: Some(dir.path().canonicalize().unwrap()),
                resource: Some(serde_json::to_string(&file).unwrap()),
                ..Default::default()
            },
        )
        .unwrap();
    let snapshot = SessionSnapshot {
        projects,
        workspaces: vec![workspace],
        layout: LayoutState::default(),
    };
    assert!(snapshot.valid());
    let restored: SessionSnapshot =
        serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
    assert_eq!(restored, snapshot);
    assert!(
        canopy_desktop::terminal::lifecycle::reconcile(
            &restored.workspaces,
            restored.projects.active,
            &Default::default()
        )
        .start
        .is_empty()
    );
}
#[test]
fn patch_headers_are_split_into_single_visual_lines() {
    let (dir, _) = fixture();
    std::fs::write(dir.path().join("file"), "data\n").unwrap();
    let file = changes::status(dir.path()).unwrap().files[0].clone();
    let diff = changes::diff(dir.path(), &file).unwrap();
    assert!(diff.lines.len() > 3);
    assert!(diff.lines.iter().all(|line| !line.text.contains('\n')));
}
#[cfg(unix)]
#[test]
fn rejecting_pre_commit_hook_aborts_publication() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = fixture();
    std::fs::write(dir.path().join("file"), "data").unwrap();
    changes::edit(dir.path(), Edit::Stage(None)).unwrap();
    let hook = repo.path().join("hooks/pre-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
    repo.config()
        .unwrap()
        .set_str("core.hooksPath", hook.parent().unwrap().to_str().unwrap())
        .unwrap();
    let error =
        changes::commit(dir.path(), "Hooks", None, &env(), &AtomicBool::new(false)).unwrap_err();
    assert!(error.contains("hook"));
    assert!(repo.head().is_err());
}
