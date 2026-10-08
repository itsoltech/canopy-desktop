use super::components::file_tree::FileNode;
use canopy_desktop::files::Index;
use std::{collections::BTreeMap, path::PathBuf};
#[derive(Default)]
struct Node {
    path: PathBuf,
    directory: bool,
    ignored: bool,
    loading: bool,
    error: Option<String>,
    git_status: Option<char>,
    children: BTreeMap<String, Node>,
}
pub fn nodes(index: &Index) -> Vec<FileNode> {
    let mut root = Node::default();
    for entry in &index.entries {
        let mut node = &mut root;
        let mut path = PathBuf::new();
        for part in entry.path.components() {
            let label = part.as_os_str().to_string_lossy().to_string();
            path.push(part);
            node = node.children.entry(label).or_default();
            node.path = path.clone();
            node.git_status = index.git_status.get(&path).copied();
        }
        node.directory = entry.directory;
        node.ignored = entry.ignored;
        node.loading = index.loading_directories.contains(&entry.path);
        node.error = index.directory_errors.get(&entry.path).cloned();
    }
    fn children(parent: Node) -> Vec<FileNode> {
        let mut entries: Vec<_> = parent.children.into_iter().collect();
        entries.sort_by(|(a, x), (b, y)| {
            y.directory
                .cmp(&x.directory)
                .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
        });
        entries
            .into_iter()
            .map(|(label, node)| {
                let id = node.path.to_string_lossy().to_string();
                let status = node.git_status;
                let ignored = node.ignored;
                let loading = node.loading;
                let error = node.error.clone();
                let mut file = if node.directory {
                    FileNode::folder(id, label, children(node))
                } else {
                    FileNode::file(id, label)
                };
                file.git_status = status;
                file.ignored = ignored;
                file.loading = loading;
                file.error = error;
                file
            })
            .collect()
    }
    children(root)
}
