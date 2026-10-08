//! File mentions in agent replies. Paths that name an existing file inside the chat's
//! folder become `canopy-file:` links the chat view renders as pills and opens in the
//! editor. Detection is textual; `link_files` checks existence, so call it off the UI thread.
use markdown_parser::{ParseOptions, mdast::Node};
use std::{
    ops::Range,
    path::{Component, Path, PathBuf},
};

pub const SCHEME: &str = "canopy-file:";
/// Bounds the file-system checks for one block of text.
const MAX_MENTIONS: usize = 200;
const MAX_PATH: usize = 300;

/// A path-like token in Markdown source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    /// Source bytes to replace (for code spans, including the backticks).
    pub range: Range<usize>,
    /// What the reader saw, used as the pill's text.
    pub label: String,
    pub path: String,
    pub line: Option<u32>,
}

/// An existing file, as a folder to open it in and a path inside that folder: the
/// chat's folder for its own files, the file's parent folder for anything else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    pub root: PathBuf,
    pub relative: PathBuf,
    pub line: Option<u32>,
    /// Outside the chat's folder: links carry the absolute path.
    pub outside: bool,
}
impl FileRef {
    pub fn path(&self) -> PathBuf {
        self.root.join(&self.relative)
    }
}

/// Where a `canopy-file:` link points: relative to the chat's folder, or absolute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub line: Option<u32>,
}

/// Rewrites mentions of existing files in `markdown` into `canopy-file:` links.
pub fn link_files(markdown: &str, cwd: &Path) -> String {
    let Ok(canonical) = cwd.canonicalize() else {
        return markdown.to_owned();
    };
    let mut output = markdown.to_owned();
    for mention in candidates(markdown).into_iter().rev() {
        if let Some(file) = resolve_in(cwd, &canonical, &mention) {
            output.replace_range(mention.range.clone(), &link(&mention.label, &file));
        }
    }
    output
}

/// Path-like code spans and words of plain text, in source order. Code blocks,
/// links and raw HTML are left alone.
pub fn candidates(markdown: &str) -> Vec<Mention> {
    let Ok(tree) = markdown_parser::to_mdast(markdown, &ParseOptions::gfm()) else {
        return vec![];
    };
    let mut found = vec![];
    visit(&tree, markdown, &mut found);
    found.truncate(MAX_MENTIONS);
    found
}

fn visit(node: &Node, source: &str, found: &mut Vec<Mention>) {
    if found.len() >= MAX_MENTIONS {
        return;
    }
    match node {
        Node::InlineCode(code) => {
            if let (Some((path, line)), Some(position)) = (path_like(&code.value), node.position())
            {
                found.push(Mention {
                    range: position.start.offset..position.end.offset,
                    label: code.value.trim().to_owned(),
                    path,
                    line,
                });
            }
        }
        Node::Text(text) => {
            let Some(position) = node.position() else {
                return;
            };
            let start = position.start.offset;
            // Escapes and entities make the value differ from the source; skip those.
            if source.get(start..position.end.offset) != Some(text.value.as_str()) {
                return;
            }
            for (offset, word) in words(&text.value) {
                if let Some((path, line)) = path_like(word) {
                    found.push(Mention {
                        range: start + offset..start + offset + word.len(),
                        label: word.to_owned(),
                        path,
                        line,
                    });
                }
            }
        }
        Node::Code(_)
        | Node::Link(_)
        | Node::LinkReference(_)
        | Node::Image(_)
        | Node::ImageReference(_)
        | Node::Html(_)
        | Node::Definition(_) => {}
        _ => {
            for child in node.children().into_iter().flatten() {
                visit(child, source, found);
            }
        }
    }
}

/// Whitespace-separated words with surrounding punctuation trimmed, and their offsets.
fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    const EDGES: &[char] = &[
        '(', ')', '[', ']', '{', '}', '<', '>', '"', '\'', ',', ';', '!', '?', '*',
    ];
    text.split_whitespace().filter_map(move |raw| {
        let offset = raw.as_ptr() as usize - text.as_ptr() as usize;
        let leading = raw.len() - raw.trim_start_matches(EDGES).len();
        // A sentence may end right after the path: "see src/main.rs."
        let word = raw.trim_matches(EDGES).trim_end_matches(['.', ':']);
        (!word.is_empty()).then_some((offset + leading, word))
    })
}

/// `src/main.rs`, `src/main.rs:42`, `src/main.rs:42:7`, `main.rs#L42`, `/abs/file.toml`.
fn path_like(text: &str) -> Option<(String, Option<u32>)> {
    let text = text.trim();
    if text.is_empty()
        || text.len() > MAX_PATH
        || text.chars().any(char::is_whitespace)
        || text.contains("://")
        || text.contains('@')
        || text.starts_with('~')
    {
        return None;
    }
    let (path, line) = split_line(text);
    let name = path.rsplit('/').next().unwrap_or_default();
    let has_extension = name.rsplit_once('.').is_some_and(|(stem, extension)| {
        (!stem.is_empty() || name.len() > 1)
            && (1..=10).contains(&extension.len())
            && extension.chars().all(|c| c.is_ascii_alphanumeric())
            && extension.chars().any(|c| c.is_ascii_alphabetic())
    });
    let plausible = path
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | '@'));
    // A bare word needs an extension; a path with a slash names a file inside folders.
    let named = has_extension || (path.contains('/') && !name.is_empty());
    (plausible && named && !name.is_empty()).then(|| (path.to_owned(), line))
}

fn split_line(text: &str) -> (&str, Option<u32>) {
    if let Some((path, line)) = text.rsplit_once("#L") {
        return (path, line.parse().ok().filter(|line| *line > 0));
    }
    let mut parts = text.rsplitn(3, ':');
    let last = parts.next().unwrap_or_default();
    let middle = parts.next();
    let rest = parts.next();
    let number = |s: &str| s.parse::<u32>().ok().filter(|line| *line > 0);
    match (rest, middle) {
        // path:line:column
        (Some(path), Some(line)) if number(last).is_some() && number(line).is_some() => {
            (path, number(line))
        }
        _ => match middle {
            Some(_) if number(last).is_some() => {
                let path = &text[..text.len() - last.len() - 1];
                (path, number(last))
            }
            _ => (text, None),
        },
    }
}

/// An existing regular file the mention names, relative to the chat folder `root` or
/// absolute. Files of the folder open in it; any other file opens in its parent folder.
pub fn resolve(root: &Path, mention: &Mention) -> Option<FileRef> {
    resolve_in(root, &root.canonicalize().ok()?, mention)
}

fn resolve_in(root: &Path, canonical_root: &Path, mention: &Mention) -> Option<FileRef> {
    let path = Path::new(&mention.path);
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        canonical_root.join(path)
    };
    let canonical = absolute.canonicalize().ok()?;
    if !canonical.metadata().ok()?.is_file() {
        return None;
    }
    let normal = |relative: &Path| {
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
    };
    if let Ok(relative) = canonical.strip_prefix(canonical_root)
        && normal(relative)
        && crate::files::resolve(canonical_root, relative).is_ok()
    {
        return Some(FileRef {
            root: root.to_owned(),
            relative: relative.to_owned(),
            line: mention.line,
            outside: false,
        });
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if private_to_user(&canonical, home.as_deref()) {
        return None;
    }
    let parent = canonical.parent()?;
    let name = PathBuf::from(canonical.file_name()?);
    crate::files::resolve(parent, &name).ok()?;
    Some(FileRef {
        root: parent.to_owned(),
        relative: name,
        line: mention.line,
        outside: true,
    })
}

/// Inside a hidden folder of the user's home (`~/.ssh`, `~/.aws`, …): credentials
/// and keys, which a reply should not turn into one-click editors.
fn private_to_user(path: &Path, home: Option<&Path>) -> bool {
    let Some(rest) = home.and_then(|home| path.strip_prefix(home).ok()) else {
        return false;
    };
    rest.components()
        .any(|c| matches!(c, Component::Normal(name) if name.to_string_lossy().starts_with('.')))
}

/// A Markdown link Canopy recognizes, with `label` escaped as literal text.
pub fn link(label: &str, file: &FileRef) -> String {
    let mut escaped = String::with_capacity(label.len());
    for c in label.chars() {
        if c.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    format!("[{escaped}]({})", url(file))
}

pub fn url(file: &FileRef) -> String {
    let mut url = String::from(SCHEME);
    let path = if file.outside {
        file.path()
    } else {
        file.relative.clone()
    };
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    if let Some(line) = file.line {
        url.push_str(&format!("#L{line}"));
    }
    url
}

/// The file a `canopy-file:` link points at.
pub fn parse_url(url: &str) -> Option<Target> {
    let rest = url.strip_prefix(SCHEME)?;
    let (path, line) = match rest.rsplit_once("#L") {
        Some((path, line)) => (path, line.parse().ok()),
        None => (rest, None),
    };
    let mut bytes = vec![];
    let mut input = path.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = (input.next()? as char).to_digit(16)?;
            let low = (input.next()? as char).to_digit(16)?;
            bytes.push((high * 16 + low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    let path = PathBuf::from(String::from_utf8(bytes).ok()?);
    let safe = path.components().next().is_some()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::RootDir));
    safe.then_some(Target { path, line })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(markdown: &str) -> Vec<(String, Option<u32>)> {
        candidates(markdown)
            .into_iter()
            .map(|m| (m.path, m.line))
            .collect()
    }

    #[test]
    fn finds_paths_in_code_spans_and_prose() {
        assert_eq!(
            paths("Edit `src/main.rs:42` and see src/lib.rs, then Cargo.toml."),
            [
                ("src/main.rs".into(), Some(42)),
                ("src/lib.rs".into(), None),
                ("Cargo.toml".into(), None)
            ]
        );
        assert_eq!(paths("(`a/b.ts:3:9`)"), [("a/b.ts".into(), Some(3))]);
        assert_eq!(paths("see main.rs#L7"), [("main.rs".into(), Some(7))]);
        assert_eq!(paths("dotfile `.env`"), [(".env".into(), None)]);
    }

    #[test]
    fn ignores_things_that_only_look_like_paths() {
        for text in [
            "version 1.2.3",
            "https://example.com/a.rs",
            "mail me@host.com",
            "```\nsrc/main.rs\n```",
            "[link](src/main.rs)",
            "plain words only",
            "`cargo test --all`",
            "~/notes.md",
        ] {
            assert!(paths(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn links_only_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("x.rs"), "").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("x.rs"), root.join("link.rs")).unwrap();

        let absolute = root.join("src/main.rs").display().to_string();
        let markdown = format!("See `src/main.rs:2`, {absolute} and src/missing.rs, src.");
        let linked = link_files(&markdown, &root);
        let code = link(
            "src/main.rs:2",
            &FileRef {
                root: root.clone(),
                relative: "src/main.rs".into(),
                line: Some(2),
                outside: false,
            },
        );
        assert!(linked.starts_with(&format!("See {code}, [")), "{linked}");
        assert_eq!(
            linked.matches("](canopy-file:src/main.rs)").count(),
            1,
            "{linked}"
        );
        assert!(linked.ends_with(" and src/missing.rs, src."), "{linked}");
    }

    #[test]
    fn files_outside_the_folder_open_in_their_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let other = outside.path().canonicalize().unwrap();
        std::fs::write(other.join("x.rs"), "").unwrap();
        let mention = |path: String| Mention {
            range: 0..0,
            label: String::new(),
            path,
            line: Some(4),
        };
        let file = resolve(&root, &mention(other.join("x.rs").display().to_string())).unwrap();
        assert_eq!(
            (file.root.as_path(), file.relative.as_path()),
            (other.as_path(), Path::new("x.rs"))
        );
        assert!(file.outside);
        assert_eq!(
            parse_url(&url(&file)),
            Some(Target {
                path: other.join("x.rs"),
                line: Some(4)
            })
        );
        assert!(
            resolve(
                &root,
                &mention(other.join("missing.rs").display().to_string())
            )
            .is_none()
        );
        assert!(
            resolve(&root, &mention(other.display().to_string())).is_none(),
            "folders stay text"
        );
    }

    #[test]
    fn hidden_folders_in_home_are_private() {
        let home = Some(Path::new("/Users/me"));
        assert!(private_to_user(Path::new("/Users/me/.ssh/config"), home));
        assert!(private_to_user(
            Path::new("/Users/me/.aws/credentials"),
            home
        ));
        assert!(!private_to_user(Path::new("/Users/me/code/main.rs"), home));
        assert!(!private_to_user(Path::new("/private/var/.tmp1/x.rs"), home));
        assert!(!private_to_user(Path::new("/Users/me/.ssh/config"), None));
    }

    #[test]
    fn urls_round_trip_and_reject_escapes() {
        let file = FileRef {
            root: "/repo".into(),
            relative: "dir with space/ż.rs".into(),
            line: Some(3),
            outside: false,
        };
        assert_eq!(
            parse_url(&url(&file)),
            Some(Target {
                path: file.relative.clone(),
                line: Some(3)
            })
        );
        assert_eq!(parse_url("canopy-file:..%2Fsecret"), None);
        assert_eq!(parse_url("canopy-file:/a/../b"), None);
        assert_eq!(parse_url("https://x"), None);
    }
}
