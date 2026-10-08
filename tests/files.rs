use canopy_desktop::{
    files::{self, Document},
    state::workspace::{PaneKind, PaneMetadata, Workspace},
};
use std::path::Path;
#[test]
fn initial_listing_is_shallow_and_ignored_entries_stay_visible() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join(".gitignore"), "ignored/\n.env\n").unwrap();
    std::fs::create_dir_all(dir.path().join("src/deep")).unwrap();
    std::fs::create_dir_all(dir.path().join("ignored/pkg/deep")).unwrap();
    std::fs::write(dir.path().join("src/deep/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.path().join("ignored/pkg/deep/file"), "ignored").unwrap();
    std::fs::write(dir.path().join(".env"), "fixture").unwrap();
    let index = files::scan(dir.path()).unwrap();
    assert_eq!(index.entries.len(), 4);
    assert!(
        index
            .entries
            .iter()
            .all(|e| e.path.components().count() == 1)
    );
    assert!(index.entries.iter().all(|e| e.path != Path::new(".git")));
    for name in ["ignored", ".env"] {
        assert!(
            index
                .entries
                .iter()
                .find(|e| e.path == Path::new(name))
                .unwrap()
                .ignored
        );
    }
    assert!(
        !index
            .entries
            .iter()
            .find(|e| e.path == Path::new("src"))
            .unwrap()
            .ignored
    );
    let child = files::list_directory(
        dir.path(),
        Path::new("ignored"),
        files::MAX_DIRECTORY_ENTRIES,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(child.entries.len(), 1);
    assert_eq!(child.entries[0].path, Path::new("ignored/pkg"));
    assert!(child.entries[0].ignored);
}
#[test]
fn atomic_save_preserves_bom_crlf_and_rejects_external_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"\xef\xbb\xbfhello\r\nworld\r\n").unwrap();
    let document = Document::load(dir.path(), Path::new("file.txt")).unwrap();
    assert_eq!(document.text, "hello\nworld\n");
    let saved = document
        .save(dir.path(), Path::new("file.txt"), "changed\n")
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"\xef\xbb\xbfchanged\r\n");
    std::fs::write(&path, "agent change").unwrap();
    assert!(
        saved
            .save(dir.path(), Path::new("file.txt"), "my edits")
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "agent change");
}

#[cfg(windows)]
#[test]
fn windows_sharing_violation_preserves_original_and_allows_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("locked.txt");
    std::fs::write(&path, "original").unwrap();
    let document = Document::load(dir.path(), Path::new("locked.txt")).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .unwrap();
    assert!(
        document
            .save(dir.path(), Path::new("locked.txt"), "replacement")
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    drop(lock);
    document
        .save(dir.path(), Path::new("locked.txt"), "replacement")
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
}
#[test]
fn binary_large_missing_and_unsafe_paths_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("binary"), b"a\0b").unwrap();
    std::fs::write(dir.path().join("large"), vec![b'x'; files::MAX_TEXT + 1]).unwrap();
    for path in ["binary", "large", "missing", "../escape", "/tmp/outside"] {
        assert!(Document::load(dir.path(), Path::new(path)).is_err());
    }
    files::create(dir.path(), Path::new("new.txt")).unwrap();
    assert!(files::create(dir.path(), Path::new("new.txt")).is_err());
}
#[cfg(unix)]
#[test]
fn symlinks_are_not_followed_and_permissions_survive_save() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("file"), "outside").unwrap();
    symlink(outside.path(), dir.path().join("link")).unwrap();
    assert!(Document::load(dir.path(), Path::new("link/file")).is_err());
    assert!(files::scan(dir.path()).unwrap().entries.is_empty());
    let path = dir.path().join("script");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    Document::load(dir.path(), Path::new("script"))
        .unwrap()
        .save(dir.path(), Path::new("script"), "new")
        .unwrap();
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o755
    );
}
#[test]
fn editor_identity_and_empty_sibling_tabs_survive_restore() {
    let mut workspace = Workspace::new();
    let tab = workspace.open("main.rs", "editor");
    let pane = workspace.active().unwrap().focused;
    let metadata = PaneMetadata {
        kind: PaneKind::Editor,
        cwd: Some("/tmp/project".into()),
        resource: Some("src/main.rs".into()),
        ..Default::default()
    };
    workspace
        .set_pane_metadata(tab, pane, metadata.clone())
        .unwrap();
    let restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&workspace).unwrap()).unwrap();
    assert_eq!(restored.active().unwrap().focused, pane);
    assert_eq!(restored.active().unwrap().root.first().metadata, metadata);
    assert!(
        canopy_desktop::terminal::lifecycle::reconcile(
            std::slice::from_ref(&restored),
            Some(restored.id),
            &Default::default()
        )
        .start
        .is_empty()
    );
}

#[test]
fn file_index_runs_on_the_shared_git_worker() {
    futures_lite::future::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), "text").unwrap();
        let client = canopy_desktop::git::GitClient::start().unwrap();
        let index = client.files(dir.path().into()).await.unwrap();
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].path, Path::new("file.txt"));
        client.shutdown().await;
    });
}

#[test]
fn git_decorations_include_staged_unstaged_and_aggregate_folders() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let repo = git2::Repository::init(root).unwrap();
    std::fs::create_dir_all(root.join("src/deep")).unwrap();
    for path in ["src/deep/changed.txt", "src/deep/removed.txt", "clean.txt"] {
        std::fs::write(root.join(path), "initial\n").unwrap();
    }
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("Test", "test@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .unwrap();
    std::fs::write(root.join("src/deep/changed.txt"), "changed\n").unwrap();
    std::fs::remove_file(root.join("src/deep/removed.txt")).unwrap();
    std::fs::create_dir(root.join("added")).unwrap();
    std::fs::write(root.join("added/new.txt"), "new").unwrap();
    let decorations = files::decorations(root).unwrap();
    let scan = files::snapshot(
        &[
            std::path::PathBuf::new(),
            "src".into(),
            "src/deep".into(),
            "added".into(),
        ]
        .into_iter()
        .map(|p| {
            let listing = files::list_directory(
                root,
                &p,
                files::MAX_DIRECTORY_ENTRIES,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
            (p, listing)
        })
        .collect(),
        &decorations,
        Default::default(),
        Default::default(),
    );
    assert_eq!(
        scan.git_status.get(Path::new("src/deep/changed.txt")),
        Some(&'M')
    );
    assert_eq!(
        scan.git_status.get(Path::new("src/deep/removed.txt")),
        Some(&'D')
    );
    assert_eq!(scan.git_status.get(Path::new("src")), Some(&'M'));
    assert_eq!(scan.git_status.get(Path::new("src/deep")), Some(&'M'));
    assert_eq!(scan.git_status.get(Path::new("added")), Some(&'A'));
    assert_eq!(scan.git_status.get(Path::new("added/new.txt")), Some(&'A'));
    assert!(!scan.git_status.contains_key(Path::new("clean.txt")));
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let staged = files::decorations(root).unwrap();
    assert_eq!(staged.status.get(Path::new("added/new.txt")), Some(&'A'));
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "changes", &tree, &[&parent])
        .unwrap();
    assert!(files::decorations(root).unwrap().status.is_empty());
}

#[test]
fn subtree_and_non_git_decorations_are_scoped() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/file"), "text").unwrap();
    assert!(files::decorations(dir.path()).unwrap().status.is_empty());
    git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("outside"), "text").unwrap();
    let root = dir.path().join("sub");
    let scan = files::snapshot(
        &[(
            std::path::PathBuf::new(),
            files::list_directory(
                &root,
                Path::new(""),
                files::MAX_DIRECTORY_ENTRIES,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap(),
        )]
        .into_iter()
        .collect(),
        &files::decorations(&root).unwrap(),
        Default::default(),
        Default::default(),
    );
    assert_eq!(scan.git_status.get(Path::new("file")), Some(&'A'));
    assert!(!scan.git_status.contains_key(Path::new("outside")));
}

#[test]
fn images_route_separately_and_keep_binary_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let data = [0x89, b'P', b'N', b'G', 0, 0xff];
    std::fs::write(dir.path().join("photo.PNG"), data).unwrap();
    assert!(files::is_image(Path::new("photo.PNG")));
    assert!(files::is_image(Path::new("logo.svg")));
    assert!(!files::is_image(Path::new("video.mp4")));
    assert!(!files::is_image(Path::new("source.rs")));
    assert_eq!(
        files::image_bytes(dir.path(), Path::new("photo.PNG")).unwrap(),
        data
    );
    assert!(files::image_bytes(dir.path(), Path::new("../photo.PNG")).is_err());
    let mut workspace = Workspace::new();
    let tab = workspace.open("photo.PNG", "image");
    let pane = workspace.active().unwrap().focused;
    workspace
        .set_pane_metadata(
            tab,
            pane,
            PaneMetadata {
                kind: PaneKind::Image,
                cwd: Some(dir.path().to_owned()),
                resource: Some("photo.PNG".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&workspace).unwrap()).unwrap();
    assert_eq!(
        restored.active().unwrap().root.first().metadata.kind,
        PaneKind::Image
    );
    assert!(
        canopy_desktop::terminal::lifecycle::reconcile(
            std::slice::from_ref(&restored),
            Some(restored.id),
            &Default::default()
        )
        .start
        .is_empty()
    );
}

#[test]
fn font_and_video_routing_is_deliberately_small() {
    for file in ["face.ttf", "face.OTF", "face.woff", "face.woff2"] {
        assert!(files::is_font(Path::new(file)));
    }
    for file in ["clip.mp4", "clip.MOV", "clip.m4v", "clip.mkv"] {
        assert!(files::is_video(Path::new(file)));
    }
    for file in ["archive.zip", "clip.avi", "music.mp3", "source.rs"] {
        assert!(!files::is_video(Path::new(file)));
        assert!(!files::is_font(Path::new(file)));
    }
    assert!(canopy_desktop::font_preview::specimen(b"not a font", "#cccccc").is_err());
    let dir = tempfile::tempdir().unwrap();
    let mut header = vec![0; 48];
    header[..4].copy_from_slice(b"wOF2");
    header[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    std::fs::write(dir.path().join("bad.woff2"), header).unwrap();
    assert!(
        canopy_desktop::font_preview::load(dir.path(), Path::new("bad.woff2"), "#cccccc")
            .unwrap_err()
            .contains("limit")
    );
}

#[test]
#[ignore = "requires CANOPY_TEST_FONT pointing to a local font fixture"]
fn real_font_specimen() {
    let path = std::path::PathBuf::from(std::env::var("CANOPY_TEST_FONT").unwrap());
    let specimen = canopy_desktop::font_preview::load(
        path.parent().unwrap(),
        Path::new(path.file_name().unwrap()),
        "#cccccc",
    )
    .unwrap();
    assert!(specimen.glyphs > 0);
    assert!(specimen.svg.windows(5).any(|w| w == b"<path"));
    assert!(!specimen.family.is_empty());
}

#[test]
fn quick_open_searches_unopened_folders_on_demand_and_skips_ignored_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src/deep")).unwrap();
    std::fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
    std::fs::write(dir.path().join(".gitignore"), "node_modules/\n.env\n").unwrap();
    std::fs::write(dir.path().join("src/deep/main.rs"), "source").unwrap();
    std::fs::write(dir.path().join("node_modules/pkg/main.rs"), "ignored").unwrap();
    std::fs::write(dir.path().join(".env"), "fixture").unwrap();
    assert!(
        files::scan(dir.path())
            .unwrap()
            .entries
            .iter()
            .all(|e| e.path.components().count() == 1)
    );
    let found = files::search::find(
        dir.path(),
        "main.rs",
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        found.paths,
        vec![std::path::PathBuf::from("src/deep/main.rs")]
    );
    assert!(
        files::search::find(
            dir.path(),
            ".env",
            &std::sync::atomic::AtomicBool::new(false)
        )
        .unwrap()
        .paths
        .is_empty()
    );
}
#[test]
fn tracked_files_are_not_marked_ignored_just_because_a_rule_matches() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join(".gitignore"), ".env\nnode_modules/\n").unwrap();
    std::fs::write(dir.path().join(".env"), "fixture").unwrap();
    std::fs::write(dir.path().join(".visible"), "dot file").unwrap();
    std::fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
    std::fs::write(dir.path().join("node_modules/pkg/tracked"), "tracked").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(".env")).unwrap();
    index
        .add_path(Path::new("node_modules/pkg/tracked"))
        .unwrap();
    index.write().unwrap();
    let listing = files::scan(dir.path()).unwrap();
    for path in [".env", ".visible", "node_modules"] {
        assert!(
            !listing
                .entries
                .iter()
                .find(|e| e.path == Path::new(path))
                .unwrap()
                .ignored
        );
    }
}
#[test]
fn cancellation_and_unsafe_directory_paths_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let cancelled = std::sync::atomic::AtomicBool::new(true);
    assert!(
        files::list_directory(dir.path(), Path::new(""), 50_000, &cancelled)
            .unwrap_err()
            .contains("cancelled")
    );
    assert!(files::search::find(dir.path(), "", &cancelled).is_err());
    let guard = files::CancelGuard::default();
    let flag = guard.0.clone();
    drop(guard);
    assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
    for path in ["../outside", "/tmp/outside"] {
        assert!(
            files::list_directory(
                dir.path(),
                Path::new(path),
                50_000,
                &std::sync::atomic::AtomicBool::new(false)
            )
            .is_err()
        );
    }
}
#[test]
fn listing_budget_is_local_to_direct_children() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("large")).unwrap();
    for n in 0..120 {
        std::fs::write(dir.path().join(format!("large/{n}.txt")), "x").unwrap();
    }
    let root = files::scan(dir.path()).unwrap();
    assert_eq!(root.entries.len(), 1);
    assert!(root.warning.is_none());
    let listing = files::list_directory(
        dir.path(),
        Path::new("large"),
        10,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(listing.entries.len(), 10);
    assert!(listing.warning.is_some());
    let listing = files::list_directory(
        dir.path(),
        Path::new("large"),
        50_000,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(listing.entries.len(), 120);
    assert!(listing.warning.is_none());
}
#[cfg(unix)]
#[test]
fn lazy_expansion_does_not_follow_directory_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "fixture").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    assert!(files::scan(dir.path()).unwrap().entries.is_empty());
    assert!(
        files::list_directory(
            dir.path(),
            Path::new("link"),
            50_000,
            &std::sync::atomic::AtomicBool::new(false)
        )
        .is_err()
    );
    assert!(
        files::search::find(
            dir.path(),
            "secret",
            &std::sync::atomic::AtomicBool::new(false)
        )
        .unwrap()
        .paths
        .is_empty()
    );
}
#[test]
fn decorations_keep_untracked_folders_opaque_and_watch_tracked_ancestors() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join("src/deep")).unwrap();
    std::fs::write(dir.path().join("src/deep/a"), "tracked").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("src/deep/a")).unwrap();
    index.write().unwrap();
    std::fs::create_dir_all(dir.path().join("new/deep")).unwrap();
    for n in 0..30 {
        std::fs::write(dir.path().join(format!("new/deep/{n}")), "x").unwrap();
    }
    let status = files::decorations(dir.path()).unwrap();
    assert!(status.tracked_directories.contains(Path::new("src/deep")));
    assert!(status.tracked_directories.contains(Path::new("src")));
    assert!(status.untracked_directories.contains(Path::new("new")));
    assert!(!status.status.contains_key(Path::new("new/deep/0")));
}
