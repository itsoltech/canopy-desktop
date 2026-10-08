use canopy_desktop::git::history::{self, PAGE_SIZE, ReferenceKind};
use git2::{Oid, Repository, Signature};
fn commit(repo: &Repository, reference: &str, parent: Option<Oid>, message: &str) -> Oid {
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("History Test", "history@example.invalid").unwrap();
    let parents: Vec<_> = parent
        .map(|id| repo.find_commit(id).unwrap())
        .into_iter()
        .collect();
    repo.commit(
        Some(reference),
        &signature,
        &signature,
        message,
        &tree,
        &parents.iter().collect::<Vec<_>>(),
    )
    .unwrap()
}
#[test]
fn current_branch_only_and_anchored_pagination() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    assert!(
        history::read(dir.path(), None, 0)
            .unwrap()
            .commits
            .is_empty()
    );
    let first = commit(&repo, "refs/heads/main", None, "Root");
    repo.set_head("refs/heads/main").unwrap();
    let other = commit(&repo, "refs/heads/other", Some(first), "Other branch only");
    let mut tip = first;
    for i in 0..PAGE_SIZE + 3 {
        tip = commit(&repo, "refs/heads/main", Some(tip), &format!("Commit {i}"));
    }
    let page = history::read(dir.path(), None, 0).unwrap();
    assert_eq!(page.commits.len(), PAGE_SIZE);
    assert!(page.more);
    assert_eq!(page.head.as_deref(), Some(tip.to_string().as_str()));
    let newer = commit(
        &repo,
        "refs/heads/main",
        Some(tip),
        "New tip after first page",
    );
    let tail = history::read(dir.path(), page.snapshot.clone(), PAGE_SIZE).unwrap();
    assert_eq!(tail.commits.len(), 4);
    assert!(!tail.more);
    assert!(
        !page
            .commits
            .iter()
            .chain(&tail.commits)
            .any(|c| c.id == other.to_string() || c.id == newer.to_string())
    );
    repo.set_head("refs/heads/other").unwrap();
    let branch = history::read(dir.path(), None, 0).unwrap();
    assert_eq!(branch.commits.len(), 2);
    assert_eq!(branch.commits[0].id, other.to_string());
    repo.set_head_detached(tip).unwrap();
    assert_eq!(history::read(dir.path(), None, 0).unwrap().head, page.head);
    assert!(history::read(dir.path(), None, history::MAX_COMMITS).is_err());
}

#[test]
fn graph_tracks_merge_edges_across_pages_and_converges_at_common_parent() {
    use canopy_desktop::git::history_graph::HistoryGraph;
    let mut graph = HistoryGraph::default();
    let merge = graph.append("merge", &["main".into(), "feature".into()]);
    assert_eq!(merge.width, 2);
    assert_eq!(merge.edges.len(), 2);
    assert_eq!(merge.edges[0].to, 0);
    assert_eq!(merge.edges[1].to, 1);
    let main = graph.append("main", &["root".into()]);
    assert_eq!(main.lane, 0);
    // A new page retains the same graph cursor, including its pending feature lane.
    let feature = graph.append("feature", &["root".into()]);
    assert_eq!(feature.lane, 1);
    assert!(
        feature
            .edges
            .iter()
            .any(|edge| edge.from == 1 && edge.to == 0 && edge.end == 1.)
    );
    let root = graph.append("root", &[]);
    assert_eq!(root.width, 1);
    assert_eq!(root.edges.len(), 1);
    assert_eq!(root.edges[0].end, 0.5);
}

#[test]
fn graph_colors_follow_branch_identity_when_columns_move() {
    use canopy_desktop::git::history_graph::HistoryGraph;
    let mut graph = HistoryGraph::default();
    let merge = graph.append("merge", &["main".into(), "side".into()]);
    assert_ne!(merge.edges[0].color, merge.edges[1].color);
    let main = graph.append("main", &[]);
    assert_eq!(main.color, merge.color);
    let side = graph.append("side", &["side-parent".into()]);
    assert_eq!(side.lane, 0); // moved left after the other lane ended
    assert_eq!(side.color, merge.edges[1].color);
    let parent = graph.append("side-parent", &[]);
    assert_eq!(parent.color, side.color);
    assert!(side.edges.iter().all(|edge| edge.color == side.color));
}

#[test]
fn tracking_distinguishes_diverged_commits_and_stays_anchored() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let root = commit(&repo, "refs/heads/main", None, "Shared root");
    repo.set_head("refs/heads/main").unwrap();
    let local = commit(&repo, "refs/heads/main", Some(root), "Local change");
    let remote = commit(
        &repo,
        "refs/heads/remote-fixture",
        Some(root),
        "Remote change",
    );
    repo.remote("origin", "https://example.invalid/repo.git")
        .unwrap();
    repo.reference("refs/remotes/origin/main", remote, true, "fixture")
        .unwrap();
    repo.find_branch("main", git2::BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/main"))
        .unwrap();
    let page = history::read(dir.path(), None, 0).unwrap();
    let snapshot = page.snapshot.as_ref().unwrap();
    assert_eq!(snapshot.upstream.as_deref(), Some("origin/main"));
    assert_eq!((snapshot.ahead, snapshot.behind), (1, 1));
    let local_row = page
        .commits
        .iter()
        .find(|c| c.id == local.to_string())
        .unwrap();
    assert_eq!(local_row.local_only, Some(true));
    assert!(
        local_row
            .references
            .iter()
            .any(|r| r.name == "main" && r.kind == ReferenceKind::LocalBranch)
    );
    assert_eq!(
        page.commits
            .iter()
            .find(|c| c.id == root.to_string())
            .unwrap()
            .local_only,
        Some(false)
    );
    assert!(!page.commits.iter().any(|c| c.id == remote.to_string()));
    repo.reference("refs/remotes/origin/main", local, true, "fixture sync")
        .unwrap();
    let anchored = history::read(dir.path(), page.snapshot.clone(), 0).unwrap();
    assert_eq!(anchored.commits[0].local_only, Some(true));
    let synced = history::read(dir.path(), None, 0).unwrap();
    let snapshot = synced.snapshot.as_ref().unwrap();
    assert_eq!((snapshot.ahead, snapshot.behind), (0, 0));
    assert_eq!(synced.commits[0].local_only, Some(false));
    assert!(
        synced.commits[0]
            .references
            .iter()
            .any(|r| r.kind == ReferenceKind::RemoteBranch && r.name == "origin/main")
    );
    repo.find_branch("main", git2::BranchType::Local)
        .unwrap()
        .set_upstream(None)
        .unwrap();
    let untracked = history::read(dir.path(), None, 0).unwrap();
    assert!(untracked.snapshot.unwrap().upstream.is_none());
    assert!(untracked.commits.iter().all(|c| c.local_only.is_none()));
}

#[test]
fn lightweight_and_annotated_tags_label_the_target_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let id = commit(&repo, "refs/heads/main", None, "Release");
    repo.set_head("refs/heads/main").unwrap();
    let object = repo.find_object(id, None).unwrap();
    let signature = Signature::now("Test", "test@example.invalid").unwrap();
    repo.tag_lightweight("v1-light", &object, false).unwrap();
    let annotated = repo
        .tag("v1", &object, &signature, "Release notes", false)
        .unwrap();
    repo.tag(
        "nested",
        &repo.find_object(annotated, None).unwrap(),
        &signature,
        "Nested tag",
        false,
    )
    .unwrap();
    let blob = repo.blob(b"not a commit").unwrap();
    repo.tag_lightweight("blob-tag", &repo.find_object(blob, None).unwrap(), false)
        .unwrap();
    let page = history::read(dir.path(), None, 0).unwrap();
    assert_eq!(page.commits.len(), 1);
    assert_eq!(page.commits[0].id, id.to_string());
    let labels = &page.commits[0].references;
    let tags: Vec<_> = labels
        .iter()
        .filter(|r| r.kind == ReferenceKind::Tag)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(tags, ["nested", "v1", "v1-light"]);
    assert_eq!(labels[0].kind, ReferenceKind::Tag);
    assert!(
        labels
            .iter()
            .any(|r| r.kind == ReferenceKind::LocalBranch && r.name == "main")
    );
}
