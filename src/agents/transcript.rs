//! Narrow Claude fallback: watch only a pending question's structured tool result.
use notify::Watcher;
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub fn rejected_question(bytes: &[u8], session: &str, call: &str) -> bool {
    bytes
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .any(|value| {
            value.get("sessionId").and_then(|v| v.as_str()) == Some(session)
                && value.get("isSidechain").and_then(|v| v.as_bool()) != Some(true)
                && value
                    .get("message")
                    .and_then(|v| v.get("content"))
                    .and_then(|v| v.as_array())
                    .is_some_and(|items| {
                        items.iter().any(|item| {
                            item.get("type").and_then(|v| v.as_str()) == Some("tool_result")
                                && item.get("tool_use_id").and_then(|v| v.as_str()) == Some(call)
                                && item.get("is_error").and_then(|v| v.as_bool()) == Some(true)
                                && item.get("content").and_then(|v| v.as_str()).is_some_and(
                                    |text| {
                                        text.starts_with(
                                            "The user doesn't want to proceed with this tool use.",
                                        )
                                    },
                                )
                        })
                    })
        })
}

pub struct QuestionWatch {
    _watcher: notify::RecommendedWatcher,
    changes: async_channel::Receiver<()>,
}
impl QuestionWatch {
    pub fn new(path: &Path) -> Result<Self, String> {
        let (tx, changes) = async_channel::bounded(1);
        let target = path.to_owned();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event
                    .as_ref()
                    .map_or(true, |e| e.paths.iter().any(|p| p == &target))
                {
                    let _ = tx.try_send(());
                }
            })
            .map_err(|e| e.to_string())?;
        watcher
            .watch(
                path.parent().ok_or("Missing transcript directory")?,
                notify::RecursiveMode::NonRecursive,
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            _watcher: watcher,
            changes,
        })
    }
    pub async fn changed(&self) -> bool {
        self.changes.recv().await.is_ok()
    }
}
pub fn read_rejection(path: &Path, session: &str, call: &str) -> Result<bool, String> {
    // Never read a full long-lived conversation; no content leaves this parser.
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(2 * 1024 * 1024)))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(rejected_question(&bytes, session, call))
}
