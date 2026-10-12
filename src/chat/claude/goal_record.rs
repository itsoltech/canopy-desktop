//! What Claude Code recorded about a finished goal in its session file: whether the
//! condition was met and the goal's numbers. Its stream doesn't say; the file is read
//! best-effort, after the goal ended, and nothing depends on finding it.
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// The end of a session file is enough: the goal ended moments before.
const TAIL: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub met: bool,
    pub reason: Option<String>,
    pub tokens: Option<u64>,
    pub seconds: Option<u64>,
}

/// Claude's folders for this user: `CLAUDE_CONFIG_DIR` when set, then `~/.claude`.
pub fn config_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".claude"));
    }
    dirs
}

/// The latest verdict on `objective` in session `session` of the chat in `cwd`.
pub fn read(dirs: &[PathBuf], cwd: &Path, session: &str, objective: &str) -> Option<Record> {
    let file = format!("{session}.jsonl");
    let project: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    dirs.iter().find_map(|dir| {
        let path = dir.join("projects").join(&project).join(&file);
        let path = if path.is_file() {
            path
        } else {
            // The folder name mangles the path; any project holding the session will do.
            std::fs::read_dir(dir.join("projects"))
                .ok()?
                .filter_map(Result::ok)
                .map(|entry| entry.path().join(&file))
                .find(|path| path.is_file())?
        };
        latest(&tail(&path)?, objective)
    })
}

fn tail(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL)))
        .ok()?;
    let mut bytes = vec![];
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The last `goal_status` entry for `objective` in session lines.
pub fn latest(lines: &str, objective: &str) -> Option<Record> {
    lines
        .lines()
        .rev()
        .filter(|line| line.contains("\"goal_status\""))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|entry| {
            let status = entry.get("attachment")?;
            (status["type"] == "goal_status"
                && status["condition"].as_str().map(str::trim) == Some(objective.trim()))
            .then(|| Record {
                met: status["met"].as_bool().unwrap_or(false),
                reason: status["reason"].as_str().map(str::to_owned),
                tokens: status["tokens"].as_u64(),
                seconds: status["durationMs"].as_u64().map(|ms| ms / 1000),
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_verdict_on_the_goal_counts() {
        let lines = [
            r#"{"type":"attachment","attachment":{"type":"goal_status","met":false,"condition":"tests pass","reason":"2 fail"}}"#,
            r#"{"type":"user","message":{"content":"goal_status"}}"#,
            r#"{"type":"attachment","attachment":{"type":"goal_status","met":true,"condition":"tests pass","reason":"all pass","iterations":3,"durationMs":31211,"tokens":1637}}"#,
            r#"{"type":"attachment","attachment":{"type":"goal_status","met":false,"condition":"another goal"}}"#,
        ]
        .join("\n");
        assert_eq!(
            latest(&lines, "tests pass"),
            Some(Record {
                met: true,
                reason: Some("all pass".into()),
                tokens: Some(1637),
                seconds: Some(31),
            })
        );
        assert_eq!(latest(&lines, "unknown"), None);
    }
}
