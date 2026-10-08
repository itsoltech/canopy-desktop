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
