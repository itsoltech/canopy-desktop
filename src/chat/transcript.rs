//! Provider-neutral conversation model. Items are only appended or updated in place,
//! so a view can splice/remeasure exactly the rows reported by `take_changes`.
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(pub u64);

/// One user request and everything the agent produced while handling it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub turn: TurnId,
    /// Tool call that spawned this item (subagent output); `None` for the main thread.
    pub parent: Option<String>,
    pub body: Body,
    /// The saved copy left out parts too large to keep (long outputs, big inputs).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trimmed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Body {
    User(UserMessage),
    Text(TextBlock),
    Thinking(TextBlock),
    Tool(ToolCall),
    Permission(Permission),
    Notice(Notice),
    TurnEnd(TurnEnd),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserMessage {
    pub text: String,
    pub delivery: Delivery,
    /// The uuid it was sent with: files are restored and conversations forked by it.
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
}

/// Something sent along with a user message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Attachment {
    /// A picture the model sees. `file` names it in the chat's attachment folder; the
    /// encoded data is held only in memory, for sending.
    Image {
        file: String,
        media_type: String,
        width: u32,
        height: u32,
        #[serde(skip)]
        data: Option<Arc<str>>,
        /// Its mention in the message text (`[shot.png (120 KB)]`), shown as a badge.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// A file or folder the agent reads itself.
    Path {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}
impl Attachment {
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }
    /// Its mention in the message text, if it has one.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Image { label, .. } | Self::Path { label, .. } => label.as_deref(),
        }
    }
    pub fn with_label(mut self, text: impl Into<String>) -> Self {
        match &mut self {
            Self::Image { label, .. } | Self::Path { label, .. } => *label = Some(text.into()),
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Delivery {
    /// Started its own turn.
    Sent,
    /// Steered into a running turn: the agent read it after a tool finished.
    Steered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
    pub streaming: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub input: Value,
    /// Raw input JSON while the model is still writing it.
    pub input_partial: String,
    pub state: ToolState,
    pub output: Option<ToolOutput>,
    pub elapsed_seconds: Option<f64>,
    /// Latest progress line of a long-running or background tool (e.g. a subagent).
    pub activity: Option<String>,
    /// Runs in the background: the call stays `Running` until the task reports an end.
    pub background: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolState {
    Streaming,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
impl ToolState {
    pub fn finished(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
    /// Provider-specific structured result (e.g. a file patch), kept for richer views.
    pub structured: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Permission {
    pub request_id: String,
    pub call_id: Option<String>,
    pub tool: String,
    pub input: Value,
    pub title: Option<String>,
    pub description: Option<String>,
    pub kind: PermissionKind,
    pub decision: Decision,
    /// What "Allow for this chat" would remember, when the provider offers it.
    #[serde(default)]
    pub remember: Option<Remember>,
    /// How the user answered, for the conversation's record.
    #[serde(default)]
    pub resolution: Option<Resolution>,
}

/// A permission rule like `Bash(npm test *)`: a tool and an optional pattern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub tool: String,
    pub content: Option<String>,
}
impl Rule {
    pub fn text(&self) -> String {
        match &self.content {
            Some(content) => format!("{}({content})", self.tool),
            None => self.tool.clone(),
        }
    }
}

/// Remembering a permission for the rest of the chat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Remember {
    /// Calls matching these rules no longer ask.
    Rules(Vec<Rule>),
    /// File edits no longer ask (the chat switches to accepting edits).
    AcceptEdits,
    /// The same request no longer asks until the agent restarts (Codex sessions).
    Session,
}

/// The user's answer to a request, beyond allowed/denied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    Allowed,
    Remembered(Remember),
    /// Denied, with what the user told the agent to do instead.
    Denied(Option<String>),
    /// Question → answer, in the order asked.
    Answered(Vec<(String, String)>),
    PlanApproved {
        accept_edits: bool,
    },
    KeepPlanning(Option<String>),
    /// Never answered: the chat ended (Canopy closed) while it waited.
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionKind {
    Tool,
    /// The agent asks the user to choose answers (Claude AskUserQuestion).
    Question,
    /// The agent presents a plan for approval before acting (Claude ExitPlanMode).
    Plan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Pending,
    Allowed,
    Denied,
    /// Withdrawn by the provider, interrupted, or the session ended first.
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    /// When a limit resets or a retry happens (unix seconds), for a live countdown.
    #[serde(default)]
    pub until: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeKind {
    ApiRetry,
    RateLimit,
    Compacted,
    Diagnostic,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnEnd {
    pub outcome: Outcome,
    pub usage: Option<Usage>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    /// The turn's last main-thread message: a later message edited forks the
    /// conversation here.
    #[serde(default)]
    pub resume_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Completed,
    Interrupted,
    Failed(String),
    /// A configured turn/budget limit ended the turn.
    Limit(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Inserted(usize),
    Updated(usize),
}

#[derive(Default)]
pub struct Transcript {
    items: Vec<Item>,
    next_item: u64,
    next_turn: u64,
    current: TurnId,
    revision: u64,
    changes: Vec<Change>,
}
impl Transcript {
    /// A transcript holding saved items (ids ascending); new items and turns continue
    /// after them. Every item is reported as inserted.
    pub fn restore(items: Vec<Item>) -> Self {
        Self::restore_after(items, 0)
    }
    /// `restore`, with new item ids also after `floor` (ids of rows kept but unread).
    pub fn restore_after(items: Vec<Item>, floor: u64) -> Self {
        let next_item = items
            .iter()
            .map(|item| item.id.0)
            .max()
            .unwrap_or(0)
            .max(floor);
        let next_turn = items.iter().map(|item| item.turn.0).max().unwrap_or(0);
        let changes = (0..items.len()).map(Change::Inserted).collect();
        Self {
            current: items.last().map(|item| item.turn).unwrap_or_default(),
            revision: items.len() as u64,
            items,
            next_item,
            next_turn,
            changes,
        }
    }
    pub fn items(&self) -> &[Item] {
        &self.items
    }
    pub fn get(&self, index: usize) -> Option<&Item> {
        self.items.get(index)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn current_turn(&self) -> TurnId {
        self.current
    }
    pub fn allocate_turn(&mut self) -> TurnId {
        self.next_turn += 1;
        TurnId(self.next_turn)
    }
    /// Later provider output belongs to `turn` until another turn becomes current.
    pub fn set_current_turn(&mut self, turn: TurnId) {
        self.current = turn;
    }
    pub fn push(&mut self, parent: Option<String>, body: Body) -> usize {
        self.push_in(self.current, parent, body)
    }
    pub fn push_in(&mut self, turn: TurnId, parent: Option<String>, body: Body) -> usize {
        self.next_item += 1;
        self.items.push(Item {
            id: ItemId(self.next_item),
            turn,
            parent,
            body,
            trimmed: false,
        });
        let index = self.items.len() - 1;
        self.record(Change::Inserted(index));
        index
    }
    /// Mutates one item; reports a change only when the body actually changed.
    pub fn update(&mut self, index: usize, f: impl FnOnce(&mut Body)) {
        let Some(item) = self.items.get_mut(index) else {
            return;
        };
        let before = item.body.clone();
        f(&mut item.body);
        if item.body != before {
            self.record(Change::Updated(index));
        }
    }
    pub fn position(&self, id: ItemId) -> Option<usize> {
        // Ids are monotonic, so the item index is found by binary search.
        self.items
            .binary_search_by_key(&id.0, |item| item.id.0)
            .ok()
    }
    pub fn rposition(&self, predicate: impl Fn(&Item) -> bool) -> Option<usize> {
        self.items.iter().rposition(predicate)
    }
    pub fn take_changes(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.changes)
    }
    fn record(&mut self, change: Change) {
        self.revision += 1;
        let inserted_same = matches!(
            (self.changes.last(), change),
            (Some(Change::Inserted(a)), Change::Updated(b)) if *a == b
        );
        if !inserted_same && self.changes.last() != Some(&change) {
            self.changes.push(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Body {
        Body::Text(TextBlock {
            text: s.into(),
            streaming: true,
        })
    }

    #[test]
    fn changes_coalesce_repeated_updates_of_the_same_row() {
        let mut transcript = Transcript::default();
        let index = transcript.push(None, text("a"));
        for suffix in ["b", "c"] {
            transcript.update(index, |body| {
                if let Body::Text(block) = body {
                    block.text.push_str(suffix);
                }
            });
        }
        assert_eq!(transcript.take_changes(), [Change::Inserted(0)]);
        transcript.update(index, |_| {});
        assert!(transcript.take_changes().is_empty());
        transcript.update(index, |body| {
            if let Body::Text(block) = body {
                block.streaming = false;
            }
        });
        assert_eq!(transcript.take_changes(), [Change::Updated(0)]);
    }

    #[test]
    fn a_restored_transcript_continues_its_ids() {
        let mut saved = Transcript::default();
        let turn = saved.allocate_turn();
        saved.push_in(turn, None, text("a"));
        saved.push_in(turn, None, text("b"));
        let mut restored = Transcript::restore(saved.items().to_vec());
        assert_eq!(
            restored.take_changes(),
            [Change::Inserted(0), Change::Inserted(1)]
        );
        assert_eq!(restored.allocate_turn(), TurnId(2));
        let index = restored.push(None, text("c"));
        assert_eq!(restored.items()[index].id, ItemId(3));
        assert_eq!(restored.position(ItemId(3)), Some(2));
    }

    #[test]
    fn items_are_found_by_stable_id() {
        let mut transcript = Transcript::default();
        transcript.push(None, text("a"));
        let second = transcript.push(None, text("b"));
        let id = transcript.items()[second].id;
        assert_eq!(transcript.position(id), Some(second));
    }
}
