//! What a finished tool call did, in a shape the chat can draw: a command and its
//! output, a file read, or a file change as diff lines. Built from the tool input and
//! the CLI's structured result (`tool_use_result`); unknown shapes fall back to
//! `Generic`, because the CLI's results change between versions.
use super::transcript::{ToolCall, ToolState, Transcript, TurnId};
use crate::git::changes::DiffLine;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum ToolView {
    Shell {
        command: String,
        description: Option<String>,
        stdout: String,
        stderr: String,
        failed: bool,
    },
    Read {
        path: PathBuf,
        start: u32,
        shown: u32,
        total: u32,
        content: String,
    },
    Change(FileChange),
    Web {
        target: String,
        text: String,
    },
    Generic {
        input: String,
        output: String,
    },
}

/// What a turn-diff pane shows, stored as its resource: a chat pane's turn, optionally
/// scrolled to one file.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TurnDiffTarget {
    pub chat: crate::state::workspace::PaneId,
    /// Sessions the chat had before (Restart); turn numbers restart in each session.
    #[serde(default)]
    pub session: u64,
    pub turn: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
}

/// A file the agent changed: one or more hunks, or the whole content of a new file.
#[derive(Clone, Debug, PartialEq)]
pub struct FileChange {
    pub path: PathBuf,
    pub created: bool,
    pub added: usize,
    pub removed: usize,
    pub lines: Vec<DiffLine>,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub fn view(tool: &ToolCall) -> ToolView {
    let output = tool.output.as_ref();
    let structured = output.and_then(|o| o.structured.as_ref());
    let plain = output.map(|o| o.text.clone()).unwrap_or_default();
    let failed = tool.state == ToolState::Failed || output.is_some_and(|o| o.is_error);
    match tool.name.as_str() {
        "Bash" => {
            let command = text(&tool.input, "command").unwrap_or_default().to_owned();
            let description = text(&tool.input, "description").map(str::to_owned);
            match structured.filter(|s| s.is_object()) {
                Some(result) => ToolView::Shell {
                    command,
                    description,
                    stdout: text(result, "stdout").unwrap_or_default().to_owned(),
                    stderr: text(result, "stderr").unwrap_or_default().to_owned(),
                    failed,
                },
                // Failures arrive as plain text ("Error: Exit code 1\n…").
                None => ToolView::Shell {
                    command,
                    description,
                    stdout: if failed { String::new() } else { plain },
                    stderr: if failed {
                        output.map(|o| o.text.clone()).unwrap_or_default()
                    } else {
                        String::new()
                    },
                    failed,
                },
            }
        }
        "Read" => match structured.and_then(|s| s.get("file")) {
            Some(file) => ToolView::Read {
                path: text(file, "filePath")
                    .or_else(|| text(&tool.input, "file_path"))
                    .unwrap_or_default()
                    .into(),
                start: number(file, "startLine").unwrap_or(1),
                shown: number(file, "numLines").unwrap_or(0),
                total: number(file, "totalLines").unwrap_or(0),
                content: text(file, "content").unwrap_or_default().to_owned(),
            },
            None => generic(tool, plain),
        },
        "Edit" | "MultiEdit" | "Write" => match structured.and_then(change) {
            Some(change) => ToolView::Change(change),
            None => generic(tool, plain),
        },
        "WebFetch" | "WebSearch" => ToolView::Web {
            target: text(&tool.input, "url")
                .or_else(|| text(&tool.input, "query"))
                .unwrap_or_default()
                .to_owned(),
            text: plain,
        },
        _ => generic(tool, plain),
    }
}

fn number(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)?
        .as_u64()
        .map(|n| n.min(u32::MAX as u64) as u32)
}

fn generic(tool: &ToolCall, output: String) -> ToolView {
    ToolView::Generic {
        input: serde_json::to_string_pretty(&tool.input).unwrap_or_default(),
        output,
    }
}

/// A file change from an Edit/Write result: its patch, or a new file's content.
fn change(result: &Value) -> Option<FileChange> {
    let path: PathBuf = text(result, "filePath")?.into();
    let created = text(result, "type") == Some("create");
    let mut lines = vec![];
    if created {
        let content = text(result, "content").unwrap_or_default();
        lines.extend(content.lines().enumerate().map(|(i, line)| DiffLine {
            old: None,
            new: Some(i as u32 + 1),
            kind: '+',
            text: line.to_owned(),
        }));
    } else {
        for hunk in result.get("structuredPatch")?.as_array()? {
            let mut old = number(hunk, "oldStart").unwrap_or(1);
            let mut new = number(hunk, "newStart").unwrap_or(1);
            lines.push(DiffLine {
                old: None,
                new: None,
                kind: 'H',
                text: format!(
                    "@@ -{},{} +{},{} @@",
                    old,
                    number(hunk, "oldLines").unwrap_or(0),
                    new,
                    number(hunk, "newLines").unwrap_or(0)
                ),
            });
            for line in hunk.get("lines")?.as_array()? {
                let line = line.as_str().unwrap_or_default();
                let (kind, body) = match line.chars().next() {
                    Some(c @ ('+' | '-' | ' ')) => (c, &line[1..]),
                    // "\ No newline at end of file" belongs to no side.
                    Some('\\') => {
                        lines.push(DiffLine {
                            old: None,
                            new: None,
                            kind: ' ',
                            text: line.to_owned(),
                        });
                        continue;
                    }
                    _ => (' ', line),
                };
                let (o, n) = match kind {
                    '+' => (None, Some(new)),
                    '-' => (Some(old), None),
                    _ => (Some(old), Some(new)),
                };
                if kind != '+' {
                    old += 1;
                }
                if kind != '-' {
                    new += 1;
                }
                lines.push(DiffLine {
                    old: o,
                    new: n,
                    kind,
                    text: body.to_owned(),
                });
            }
        }
    }
    let added = lines.iter().filter(|l| l.kind == '+').count();
    let removed = lines.iter().filter(|l| l.kind == '-').count();
    Some(FileChange {
        path,
        created,
        added,
        removed,
        lines,
    })
}

/// One row of a turn's change tree: a folder (with its descendants' counts) or a file.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeRow<'a> {
    Folder {
        /// Full path from the tree's root, the key for folding.
        path: String,
        /// Shown name; folders holding only one folder are joined ("src/ui/chat").
        name: String,
        depth: usize,
        added: usize,
        removed: usize,
    },
    File {
        change: &'a FileChange,
        name: String,
        depth: usize,
    },
}

#[derive(Default)]
struct TreeNode<'a> {
    folders: std::collections::BTreeMap<String, TreeNode<'a>>,
    files: Vec<(String, &'a FileChange)>,
}

impl TreeNode<'_> {
    fn counts(&self) -> (usize, usize) {
        let own = self
            .files
            .iter()
            .fold((0, 0), |(a, r), (_, f)| (a + f.added, r + f.removed));
        self.folders.values().fold(own, |(a, r), folder| {
            let (fa, fr) = folder.counts();
            (a + fa, r + fr)
        })
    }
}

/// Files as a folder tree, rows in display order. `paths` are the files' paths as shown
/// (relative to the chat folder when inside it). Children of `collapsed` folders are left
/// out.
pub fn change_tree<'a>(
    files: impl IntoIterator<Item = (String, &'a FileChange)>,
    collapsed: &std::collections::HashSet<String>,
) -> Vec<TreeRow<'a>> {
    let mut root = TreeNode::default();
    for (path, change) in files {
        let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let name = parts.pop().unwrap_or_default().to_owned();
        let mut node = &mut root;
        for part in parts {
            node = node.folders.entry(part.to_owned()).or_default();
        }
        node.files.push((name, change));
    }
    let mut rows = vec![];
    flatten(&root, "", 0, collapsed, &mut rows);
    rows
}

fn flatten<'a>(
    node: &TreeNode<'a>,
    prefix: &str,
    depth: usize,
    collapsed: &std::collections::HashSet<String>,
    rows: &mut Vec<TreeRow<'a>>,
) {
    for (name, folder) in &node.folders {
        // Join single-folder chains into one row.
        let mut name = name.clone();
        let mut folder = folder;
        while folder.files.is_empty() && folder.folders.len() == 1 {
            let (child, next) = folder.folders.iter().next().expect("one folder");
            name = format!("{name}/{child}");
            folder = next;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let (added, removed) = folder.counts();
        rows.push(TreeRow::Folder {
            path: path.clone(),
            name,
            depth,
            added,
            removed,
        });
        if !collapsed.contains(&path) {
            flatten(folder, &path, depth + 1, collapsed, rows);
        }
    }
    let mut files: Vec<_> = node.files.iter().collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, change) in files {
        rows.push(TreeRow::File {
            change,
            name: name.clone(),
            depth,
        });
    }
}

/// What a tool asks to do, shown before the user allows it.
#[derive(Clone, Debug, PartialEq)]
pub enum Proposal {
    Command {
        command: String,
        description: Option<String>,
    },
    /// An edit (old → new text, without file line numbers) or a file to write.
    /// `created` is true for Write; whether it replaces a file is checked separately.
    Change(FileChange),
    Input(String),
}

/// Lines compared per side at most (after the common start and end); beyond that a
/// change shows as removed + added.
const DIFF_LINES: usize = 500;
/// Edits of one MultiEdit and lines of a Write shown before the user allows them.
const PROPOSED_EDITS: usize = 50;
const PROPOSED_LINES: usize = 2000;

pub fn proposal(tool: &str, input: &Value) -> Proposal {
    let path = || text(input, "file_path").map(PathBuf::from);
    match tool {
        "Bash" => Proposal::Command {
            command: text(input, "command").unwrap_or_default().to_owned(),
            description: text(input, "description").map(str::to_owned),
        },
        // A Codex patch, already shaped like an Edit/Write result.
        "apply_patch" => match change(&input["result"]) {
            Some(change) => Proposal::Change(change),
            None => Proposal::Input(serde_json::to_string_pretty(input).unwrap_or_default()),
        },
        "Edit" if path().is_some() => Proposal::Change(edit_change(
            path().expect("path"),
            &[(
                text(input, "old_string").unwrap_or_default(),
                text(input, "new_string").unwrap_or_default(),
            )],
        )),
        "MultiEdit" if path().is_some() => {
            let edits: Vec<(&str, &str)> = input["edits"]
                .as_array()
                .into_iter()
                .flatten()
                .take(PROPOSED_EDITS)
                .map(|edit| {
                    (
                        text(edit, "old_string").unwrap_or_default(),
                        text(edit, "new_string").unwrap_or_default(),
                    )
                })
                .collect();
            Proposal::Change(edit_change(path().expect("path"), &edits))
        }
        "Write" if path().is_some() => {
            let content = text(input, "content").unwrap_or_default();
            let lines: Vec<DiffLine> = content
                .lines()
                .take(PROPOSED_LINES)
                .enumerate()
                .map(|(i, line)| DiffLine {
                    old: None,
                    new: Some(i as u32 + 1),
                    kind: '+',
                    text: line.to_owned(),
                })
                .collect();
            Proposal::Change(FileChange {
                path: path().expect("path"),
                created: true,
                added: content.lines().count(),
                removed: 0,
                lines,
            })
        }
        _ => Proposal::Input(serde_json::to_string_pretty(input).unwrap_or_default()),
    }
}

/// Edits as removed/added lines, each edit after a separator when there are several.
fn edit_change(path: PathBuf, edits: &[(&str, &str)]) -> FileChange {
    let mut lines = vec![];
    for (index, (old, new)) in edits.iter().enumerate() {
        if edits.len() > 1 {
            lines.push(DiffLine {
                old: None,
                new: None,
                kind: 'H',
                text: format!("Edit {}", index + 1),
            });
        }
        lines.extend(line_diff(old, new));
    }
    FileChange {
        path,
        created: false,
        added: lines.iter().filter(|l| l.kind == '+').count(),
        removed: lines.iter().filter(|l| l.kind == '-').count(),
        lines,
    }
}

/// Line diff by longest common subsequence; positions are unknown, so no line numbers.
pub fn line_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let line = |kind: char, text: &str| DiffLine {
        old: None,
        new: None,
        kind,
        text: text.to_owned(),
    };
    // The unchanged start and end need no table.
    let start = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (&old[start..old.len() - end], &new[start..new.len() - end]);
    let mut lines: Vec<DiffLine> = old[..start].iter().map(|t| line(' ', t)).collect();
    lines.extend(middle_diff(a, b, &line));
    lines.extend(old[old.len() - end..].iter().map(|t| line(' ', t)));
    lines
}

fn middle_diff(a: &[&str], b: &[&str], line: &dyn Fn(char, &str) -> DiffLine) -> Vec<DiffLine> {
    if a.len() > DIFF_LINES || b.len() > DIFF_LINES {
        return a
            .iter()
            .map(|t| line('-', t))
            .chain(b.iter().map(|t| line('+', t)))
            .collect();
    }
    // lengths[i][j]: common subsequence of a[i..] and b[j..].
    let mut lengths = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lengths[i][j] = if a[i] == b[j] {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut lines) = (0, 0, vec![]);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            lines.push(line(' ', a[i]));
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lengths[i + 1][j] >= lengths[i][j + 1]) {
            // Removed lines come before the added ones, as in git.
            lines.push(line('-', a[i]));
            i += 1;
        } else {
            lines.push(line('+', b[j]));
            j += 1;
        }
    }
    lines
}

/// Files changed by Edit/Write calls of `turn` (its subagents included), one entry per
/// file in first-change order; later edits of a file add their hunks after earlier ones.
pub fn turn_changes(transcript: &Transcript, turn: TurnId) -> Vec<FileChange> {
    let mut files: Vec<FileChange> = vec![];
    for item in transcript.items().iter().filter(|item| item.turn == turn) {
        let super::transcript::Body::Tool(tool) = &item.body else {
            continue;
        };
        if tool.state != ToolState::Succeeded {
            continue;
        }
        let ToolView::Change(change) = view(tool) else {
            continue;
        };
        match files.iter_mut().find(|f| f.path == change.path) {
            Some(file) => {
                file.added += change.added;
                file.removed += change.removed;
                file.lines.extend(change.lines);
            }
            None => files.push(change),
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::transcript::ToolOutput;
    use serde_json::json;

    fn call(name: &str, input: Value, result: Option<Value>, text: &str) -> ToolCall {
        ToolCall {
            call_id: "t".into(),
            name: name.into(),
            input,
            input_partial: String::new(),
            state: ToolState::Succeeded,
            output: Some(ToolOutput {
                text: text.into(),
                is_error: false,
                structured: result,
            }),
            elapsed_seconds: None,
            activity: None,
            background: false,
        }
    }

    #[test]
    fn edit_patches_become_numbered_diff_lines() {
        let tool = call(
            "Edit",
            json!({"file_path": "/r/a.txt"}),
            Some(json!({"filePath": "/r/a.txt", "structuredPatch": [
                {"oldStart": 1, "oldLines": 3, "newStart": 1, "newLines": 3,
                 "lines": [" line one", "-line two", "+line 2", " line three"]}
            ]})),
            "updated",
        );
        let ToolView::Change(change) = view(&tool) else {
            panic!("change");
        };
        assert_eq!(
            (change.added, change.removed, change.created),
            (1, 1, false)
        );
        let numbered: Vec<_> = change
            .lines
            .iter()
            .map(|l| (l.kind, l.old, l.new, l.text.as_str()))
            .collect();
        assert_eq!(
            numbered,
            [
                ('H', None, None, "@@ -1,3 +1,3 @@"),
                (' ', Some(1), Some(1), "line one"),
                ('-', Some(2), None, "line two"),
                ('+', None, Some(2), "line 2"),
                (' ', Some(3), Some(3), "line three"),
            ]
        );
    }

    #[test]
    fn a_missing_final_newline_does_not_shift_numbers() {
        let tool = call(
            "Edit",
            json!({}),
            Some(json!({"filePath": "/r/a", "structuredPatch": [
                {"oldStart": 4, "oldLines": 1, "newStart": 4, "newLines": 1,
                 "lines": ["-end", "\\ No newline at end of file", "+end!"]}
            ]})),
            "",
        );
        let ToolView::Change(change) = view(&tool) else {
            panic!("change");
        };
        let numbers: Vec<_> = change.lines.iter().map(|l| (l.old, l.new)).collect();
        assert_eq!(
            numbers,
            [(None, None), (Some(4), None), (None, None), (None, Some(4))]
        );
    }

    #[test]
    fn created_files_are_all_additions() {
        let tool = call(
            "Write",
            json!({}),
            Some(
                json!({"type": "create", "filePath": "/r/b.txt", "content": "one\ntwo\n", "structuredPatch": []}),
            ),
            "created",
        );
        let ToolView::Change(change) = view(&tool) else {
            panic!("change");
        };
        assert!(change.created);
        assert_eq!(change.added, 2);
        assert_eq!(change.lines[1].new, Some(2));
    }

    #[test]
    fn shell_results_and_failures() {
        let ok = call(
            "Bash",
            json!({"command": "ls", "description": "List"}),
            Some(json!({"stdout": "a\nb", "stderr": ""})),
            "a\nb",
        );
        assert!(
            matches!(view(&ok), ToolView::Shell { stdout, failed: false, .. } if stdout == "a\nb")
        );
        let mut failed = call(
            "Bash",
            json!({"command": "ls x"}),
            Some(json!("Error: Exit code 1")),
            "Exit code 1\nls: x: No such file",
        );
        failed.state = ToolState::Failed;
        assert!(
            matches!(view(&failed), ToolView::Shell { stderr, failed: true, .. } if stderr.contains("No such file"))
        );
        assert!(matches!(
            view(&call("mcp__x__y", json!({"a": 1}), None, "out")),
            ToolView::Generic { .. }
        ));
    }

    #[test]
    fn proposals_show_what_a_tool_will_do() {
        let edit = proposal(
            "Edit",
            &json!({"file_path":"/p/a.rs","old_string":"a\nb\nc","new_string":"a\nB\nc\nd"}),
        );
        let Proposal::Change(change) = edit else {
            panic!("edit");
        };
        let kinds: String = change.lines.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, " -+ +");
        assert_eq!(
            (change.added, change.removed, change.created),
            (2, 1, false)
        );
        let Proposal::Change(write) =
            proposal("Write", &json!({"file_path":"/p/b","content":"x\ny"}))
        else {
            panic!("write");
        };
        assert!(write.created && write.added == 2);
        assert!(matches!(
            proposal("Bash", &json!({"command":"ls","description":"List"})),
            Proposal::Command { command, description: Some(_) } if command == "ls"
        ));
        assert!(matches!(proposal("Edit", &json!({})), Proposal::Input(_)));
    }

    #[test]
    fn changes_nest_by_folder_and_join_single_chains() {
        let change = |path: &str, added| FileChange {
            path: path.into(),
            created: false,
            added,
            removed: 1,
            lines: vec![],
        };
        let files = [
            change("src/ui/chat/view.rs", 3),
            change("src/ui/chat/rows.rs", 2),
            change("src/ui/theme.rs", 1),
            change("README.md", 4),
        ];
        let rows = change_tree(
            files.iter().map(|f| (f.path.display().to_string(), f)),
            &Default::default(),
        );
        let shape: Vec<String> = rows
            .iter()
            .map(|row| match row {
                TreeRow::Folder {
                    name, depth, added, ..
                } => format!("{depth}:{name}/ +{added}"),
                TreeRow::File { name, depth, .. } => format!("{depth}:{name}"),
            })
            .collect();
        assert_eq!(
            shape,
            [
                "0:src/ui/ +6",
                "1:chat/ +5",
                "2:rows.rs",
                "2:view.rs",
                "1:theme.rs",
                "0:README.md"
            ]
        );
        let folded = change_tree(
            files.iter().map(|f| (f.path.display().to_string(), f)),
            &["src/ui/chat".to_owned()].into(),
        );
        assert!(
            !folded
                .iter()
                .any(|row| matches!(row, TreeRow::File { name, .. } if name == "view.rs"))
        );
    }
}
