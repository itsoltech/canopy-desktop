//! Bounded, HEAD-anchored history pages, read only on the Git worker.
use super::Result;
use git2::{ErrorCode, Oid, Repository, Sort};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};
pub const PAGE_SIZE: usize = 50;
pub const MAX_COMMITS: usize = 10_000;
#[derive(Clone, Debug)]
pub struct CommitEntry {
    pub id: String,
    pub subject: String,
    pub message: String,
    pub author: String,
    pub seconds: i64,
    pub parents: Vec<String>,
    pub references: Vec<ReferenceLabel>,
    pub local_only: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReferenceKind {
    Tag,
    LocalBranch,
    RemoteBranch,
}
#[derive(Clone, Debug)]
pub struct ReferenceLabel {
    pub name: String,
    pub kind: ReferenceKind,
}
#[derive(Clone, Debug)]
pub struct HistorySnapshot {
    pub head: String,
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub note: String,
    references: HashMap<Oid, Vec<ReferenceLabel>>,
    local_only: HashSet<Oid>,
    local_complete: bool,
}
impl HistorySnapshot {
    fn capture(repo: &Repository) -> Result<Option<Self>> {
        let reference = match repo.head() {
            Ok(reference) => reference,
            Err(e) if matches!(e.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
                return Ok(None);
            }
            Err(e) => return Err(super::err(e)),
        };
        let head = reference.peel_to_commit().map_err(super::err)?.id();
        let branch = if reference.is_branch() {
            reference.shorthand().unwrap_or("HEAD").to_owned()
        } else {
            "Detached HEAD".into()
        };
        let mut snapshot = Self {
            head: head.to_string(),
            branch,
            upstream: None,
            ahead: 0,
            behind: 0,
            note: "No upstream configured".into(),
            references: HashMap::new(),
            local_only: HashSet::new(),
            local_complete: true,
        };
        // Snapshot refs once per refresh, not once per row or page.
        for entry in repo.branches(None).map_err(super::err)?.take(4096) {
            let (branch, kind) = entry.map_err(super::err)?;
            if let Some(id) = branch.get().target() {
                snapshot
                    .references
                    .entry(id)
                    .or_default()
                    .push(ReferenceLabel {
                        name: branch
                            .name()
                            .map_err(super::err)?
                            .unwrap_or("Unknown branch")
                            .to_owned(),
                        kind: if kind == git2::BranchType::Remote {
                            ReferenceKind::RemoteBranch
                        } else {
                            ReferenceKind::LocalBranch
                        },
                    });
            }
        }
        for reference in repo
            .references_glob("refs/tags/*")
            .map_err(super::err)?
            .take(4096)
        {
            let reference = reference.map_err(super::err)?;
            // Peel annotated (including nested) tags to their final object.
            // Tags targeting trees/blobs do not belong to a commit timeline.
            let object = reference.peel(git2::ObjectType::Any).map_err(super::err)?;
            if object.as_commit().is_none() {
                continue;
            }
            let name = reference
                .name()
                .map_err(super::err)?
                .strip_prefix("refs/tags/")
                .unwrap_or_default();
            snapshot
                .references
                .entry(object.id())
                .or_default()
                .push(ReferenceLabel {
                    name: name.to_owned(),
                    kind: ReferenceKind::Tag,
                });
        }
        for labels in snapshot.references.values_mut() {
            labels.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
        }
        if !reference.is_branch() {
            snapshot.note = "Detached HEAD · no upstream comparison".into();
            return Ok(Some(snapshot));
        }
        let branch = repo
            .find_branch(&snapshot.branch, git2::BranchType::Local)
            .map_err(super::err)?;
        let upstream = match branch.upstream() {
            Ok(upstream) => upstream,
            Err(e) if e.code() == ErrorCode::NotFound => {
                snapshot.note = "No upstream available locally".into();
                return Ok(Some(snapshot));
            }
            Err(e) => return Err(super::err(e)),
        };
        let upstream_id = upstream.get().peel_to_commit().map_err(super::err)?.id();
        snapshot.upstream = Some(
            upstream
                .name()
                .map_err(super::err)?
                .unwrap_or("upstream")
                .to_owned(),
        );
        (snapshot.ahead, snapshot.behind) = repo
            .graph_ahead_behind(head, upstream_id)
            .map_err(super::err)?;
        snapshot.note = "Based on local tracking refs · no fetch".into();
        let mut unpublished = repo.revwalk().map_err(super::err)?;
        unpublished.push(head).map_err(super::err)?;
        unpublished.hide(upstream_id).map_err(super::err)?;
        for (index, id) in unpublished.take(MAX_COMMITS + 1).enumerate() {
            let id = id.map_err(super::err)?;
            if index == MAX_COMMITS {
                snapshot.local_complete = false;
                break;
            }
            snapshot.local_only.insert(id);
        }
        Ok(Some(snapshot))
    }
}
#[derive(Clone, Debug, Default)]
pub struct HistoryPage {
    pub head: Option<String>,
    pub snapshot: Option<Arc<HistorySnapshot>>,
    pub commits: Vec<CommitEntry>,
    pub more: bool,
}
pub fn read(
    path: &Path,
    snapshot: Option<Arc<HistorySnapshot>>,
    offset: usize,
) -> Result<HistoryPage> {
    if offset >= MAX_COMMITS {
        return Err("History preview is limited to 10,000 commits.".into());
    }
    let repo = Repository::open(path).map_err(super::err)?;
    let snapshot = match snapshot {
        Some(snapshot) => snapshot,
        None => match HistorySnapshot::capture(&repo)? {
            Some(snapshot) => Arc::new(snapshot),
            None => return Ok(HistoryPage::default()),
        },
    };
    let head = Oid::from_str(&snapshot.head).map_err(super::err)?;
    let mut walk = repo.revwalk().map_err(super::err)?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
        .map_err(super::err)?;
    walk.push(head).map_err(super::err)?;
    let mut page = HistoryPage {
        head: Some(head.to_string()),
        snapshot: Some(snapshot.clone()),
        ..Default::default()
    };
    for (index, oid) in walk.skip(offset).take(PAGE_SIZE + 1).enumerate() {
        let oid = oid.map_err(super::err)?;
        if index == PAGE_SIZE {
            page.more = offset + PAGE_SIZE < MAX_COMMITS;
            break;
        }
        let commit = repo.find_commit(oid).map_err(super::err)?;
        page.commits.push(CommitEntry {
            id: oid.to_string(),
            references: snapshot.references.get(&oid).cloned().unwrap_or_default(),
            local_only: snapshot.upstream.as_ref().and_then(|_| {
                if snapshot.local_only.contains(&oid) {
                    Some(true)
                } else if snapshot.local_complete {
                    Some(false)
                } else {
                    None
                }
            }),
            subject: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or("Untitled commit")
                .chars()
                .take(512)
                .collect(),
            message: commit
                .message()
                .unwrap_or("Non-UTF-8 commit message")
                .chars()
                .take(8192)
                .collect(),
            author: commit
                .author()
                .name()
                .unwrap_or("Unknown author")
                .chars()
                .take(256)
                .collect(),
            seconds: commit.time().seconds(),
            parents: commit.parent_ids().map(|id| id.to_string()).collect(),
        });
    }
    Ok(page)
}
