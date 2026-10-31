//! Cmd+P performs an on-demand, cancellable search, independent from the displayed tree.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Default)]
pub struct SearchResult {
    pub paths: Vec<PathBuf>,
    pub warning: Option<String>,
}
pub fn find(root: &Path, query: &str, cancel: &AtomicBool) -> Result<SearchResult, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Search cancelled.".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let words = query.to_lowercase();
    let words = words.split_whitespace().collect::<Vec<_>>();
    let mut result = SearchResult::default();
    let mut walk = ignore::WalkBuilder::new(&root);
    walk.hidden(false)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(65))
        .filter_entry(|entry| entry.file_name() != ".git");
    for (visited, entry) in walk.build().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Search cancelled.".into());
        }
        if visited >= 200_000 {
            result.warning =
                Some("Search stopped after 200,000 entries. Some matches may not be shown.".into());
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                result.warning = Some("Some folders could not be searched.".into());
                continue;
            }
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(path) = entry.path().strip_prefix(&root) else {
            continue;
        };
        let Some(text) = path.to_str() else {
            continue;
        };
        let text = text.to_lowercase();
        if words.iter().all(|w| text.contains(w)) {
            result.paths.push(path.to_owned());
            if result.paths.len() == 100 {
                result.warning=Some("Showing the first 100 matches. Narrow your search to see more specific results.".into());
                break;
            }
        }
    }
    result.paths.sort();
    Ok(result)
}

/// A file or folder offered after `@` in the chat composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    /// Relative to the searched folder.
    pub path: PathBuf,
    pub folder: bool,
}

/// Files and folders under `root` whose path contains every word of `query`, best
/// first: a match in the name before one only in the path, then shorter paths.
/// Ignore rules apply as in Cmd+P; at most `limit` results.
pub fn mentions(
    root: &Path,
    query: &str,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<Vec<Mention>, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let query = query.to_lowercase();
    let words = query.split_whitespace().collect::<Vec<_>>();
    let mut walk = ignore::WalkBuilder::new(&root);
    walk.hidden(false)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(65))
        .filter_entry(|entry| entry.file_name() != ".git");
    // (name misses the query, depth, path) sorts best first.
    let mut found: Vec<(bool, usize, Mention)> = vec![];
    for (visited, entry) in walk.build().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Search cancelled.".into());
        }
        if visited >= 200_000 {
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let Some(kind) = entry.file_type() else {
            continue;
        };
        let Ok(path) = entry.path().strip_prefix(&root) else {
            continue;
        };
        let Some(text) = path.to_str().filter(|text| !text.is_empty()) else {
            continue;
        };
        let text = text.to_lowercase();
        if !words.iter().all(|word| text.contains(word)) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_lowercase();
        let in_name = words.last().is_none_or(|word| name.contains(word));
        found.push((
            !in_name,
            path.components().count(),
            Mention {
                path: path.to_owned(),
                folder: kind.is_dir(),
            },
        ));
        // Enough to rank from without walking a whole monorepo for a short query.
        if found.len() >= limit * 40 {
            break;
        }
    }
    found.sort_by(|a, b| (a.0, a.1, &a.2.path).cmp(&(b.0, b.1, &b.2.path)));
    Ok(found
        .into_iter()
        .take(limit)
        .map(|(_, _, mention)| mention)
        .collect())
}

#[cfg(test)]
mod mention_tests {
    use super::*;

    #[test]
    fn mentions_rank_name_matches_and_shallow_paths_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/chat")).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("src/chat/view.rs"), "").unwrap();
        std::fs::write(root.join("docs/chat.md"), "").unwrap();
        std::fs::write(root.join("chat.rs"), "").unwrap();
        let cancel = AtomicBool::new(false);
        let found = mentions(root, "chat", 10, &cancel).unwrap();
        let paths: Vec<_> = found
            .iter()
            .map(|m| m.path.to_str().unwrap().replace('\\', "/"))
            .collect();
        assert_eq!(
            paths,
            ["chat.rs", "docs/chat.md", "src/chat", "src/chat/view.rs"]
        );
        assert!(found[2].folder && !found[0].folder);
        assert_eq!(mentions(root, "", 2, &cancel).unwrap().len(), 2);
    }
}
