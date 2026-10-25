//! What a chat keeps across app restarts: its conversation split into sessions
//! (epochs) at Restart and Edit. No I/O — the owner stores the JSON
//! produced here and hands loaded rows back.
use super::session::Base;
use super::transcript::{Body, Decision, Item, Notice, NoticeKind, Outcome, Resolution, TurnEnd};
use crate::settings::ChatHistoryRows;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Tool output kept in full up to this size; longer output keeps its start and end.
const OUTPUT_LIMIT: usize = 64 * 1024;
const OUTPUT_HEAD: usize = 32 * 1024;
const OUTPUT_TAIL: usize = 16 * 1024;
/// A structured result (patches, file contents) larger than this is not kept.
const STRUCTURED_LIMIT: usize = 128 * 1024;
/// Text in a tool's input (e.g. a written file) is cut at this size.
const INPUT_LIMIT: usize = 64 * 1024;
/// A saved item stays below this; anything larger has every long text cut.
const ITEM_LIMIT: usize = 512 * 1024;
/// Each text of an item still over `ITEM_LIMIT` keeps this much.
const LAST_RESORT: usize = 16 * 1024;

/// Why a new session started below the previous one; shown as the separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Boundary {
    Restarted,
    Edited,
    /// Kept so histories saved with it still load (New session was removed).
    NewSession,
    /// The conversation went on with another agent; kept so histories saved with it
    /// still load (switching mid-conversation was withdrawn, DEC-087).
    SwitchedToClaude,
    SwitchedToCodex,
}
impl Boundary {
    pub fn label(self) -> &'static str {
        match self {
            Self::Restarted => "Session restarted",
            Self::Edited => "Edited — earlier version above",
            Self::NewSession => "New session",
            Self::SwitchedToClaude => "Switched to Claude Code",
            Self::SwitchedToCodex => "Switched to Codex",
        }
    }
}

/// The agent that ran a part of the conversation, to come back to it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRef {
    /// Tool id (`claude`, `codex`).
    pub tool: String,
    #[serde(default)]
    pub profile: Option<String>,
    /// Its provider session, to resume.
    #[serde(default)]
    pub session: Option<String>,
    /// Its model, effort and mode in this conversation.
    #[serde(default)]
    pub settings: Option<super::config::ChatSettings>,
}

/// How a session began.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochMeta {
    /// The separator above it (none for the first session).
    #[serde(default)]
    pub before: Option<Boundary>,
    /// It continues a conversation it doesn't hold (resumed or forked).
    #[serde(default)]
    pub continued: bool,
    /// Who ran it (none in histories from before agents could be switched); labels the
    /// parts of conversations saved with a switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRef>,
    /// The chat's goal while one is set; it comes back without running on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<crate::chat::goal::Goal>,
    /// A forked chat (DEC-093): its agent's session branches off `agent.session` at this
    /// point when it starts; cleared once the new session has its own id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_at: Option<String>,
}
impl EpochMeta {
    pub fn base(&self) -> Base {
        if self.continued {
            Base::Continued
        } else {
            Base::Fresh
        }
    }
}

/// One saved session, oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedEpoch {
    pub index: u64,
    pub meta: EpochMeta,
    pub items: Vec<Item>,
}

/// A chat's history as loaded; `lost` counts rows that couldn't be read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub epochs: Vec<SavedEpoch>,
    pub lost: usize,
    /// Highest item id and session among rows that couldn't be read: new rows go after
    /// them, so they never land on (and lose to) a row kept for a newer version.
    pub floor: Option<u64>,
    pub last_epoch: Option<u64>,
}

pub fn meta_json(meta: &EpochMeta) -> String {
    serde_json::to_string(meta).unwrap_or_else(|_| "{}".into())
}

/// The JSON stored for `item`, with oversized parts left out; always below `ITEM_LIMIT`
/// (a pasted log or a huge reply is cut rather than blocking the chat's saving).
pub fn item_json(item: &Item) -> Option<String> {
    let json = serde_json::to_string(&stored(item)).ok()?;
    if json.len() <= ITEM_LIMIT {
        return Some(json);
    }
    let mut value: Value = serde_json::from_str(&json).ok()?;
    cut_all(&mut value, LAST_RESORT);
    if let Value::Object(map) = &mut value {
        map.insert("trimmed".into(), Value::Bool(true));
    }
    let json = serde_json::to_string(&value).ok()?;
    if json.len() <= ITEM_LIMIT {
        return Some(json);
    }
    // Many individually small fields can exceed the limit even after trimming.
    // Keep this item's identity and an explicit loss marker so later items and
    // TurnEnd can still be saved. Never pretend the complete live item was stored.
    serde_json::to_string(&Item {
        id: item.id,
        turn: item.turn,
        parent: item
            .parent
            .as_ref()
            .filter(|p| p.len() <= LAST_RESORT)
            .cloned(),
        body: Body::Notice(Notice {
            kind: NoticeKind::Diagnostic,
            text: "Content omitted from saved history because it exceeded the storage limit."
                .into(),
            until: None,
        }),
        trimmed: true,
    })
    .ok()
}

fn cut_all(value: &mut Value, limit: usize) {
    match value {
        Value::String(text) if text.len() > limit => {
            let end = floor_boundary(text, limit);
            text.truncate(end);
            text.push_str("\n… trimmed in saved history");
        }
        Value::Array(values) => values.iter_mut().for_each(|v| cut_all(v, limit)),
        Value::Object(map) => map.values_mut().for_each(|v| cut_all(v, limit)),
        _ => {}
    }
}

/// A copy of `item` fit for saving: long outputs and inputs trimmed (`trimmed` set).
pub fn stored(item: &Item) -> Item {
    let mut item = item.clone();
    if let Body::Tool(call) = &mut item.body {
        let mut trimmed = false;
        if let Some(output) = &mut call.output {
            if output.text.len() > OUTPUT_LIMIT {
                output.text = cut(&output.text);
                trimmed = true;
            }
            if output
                .structured
                .as_ref()
                .is_some_and(|value| value.to_string().len() > STRUCTURED_LIMIT)
            {
                output.structured = None;
                trimmed = true;
            }
        }
        trimmed |= shorten_strings(&mut call.input);
        // Only meaningful while the input streams; finished calls have `input`.
        call.input_partial.clear();
        item.trimmed |= trimmed;
    }
    item
}

/// Start and end of a long text, with a marker between.
fn cut(text: &str) -> String {
    let head = floor_boundary(text, OUTPUT_HEAD);
    let tail = ceil_boundary(text, text.len().saturating_sub(OUTPUT_TAIL));
    format!(
        "{}\n… {} KB trimmed in saved history …\n{}",
        &text[..head],
        (tail - head) / 1024,
        &text[tail..]
    )
}

fn shorten_strings(value: &mut Value) -> bool {
    match value {
        Value::String(text) if text.len() > INPUT_LIMIT => {
            let end = floor_boundary(text, INPUT_LIMIT);
            text.truncate(end);
            text.push_str("\n… trimmed in saved history");
            true
        }
        Value::Array(values) => values.iter_mut().fold(false, |t, v| shorten_strings(v) | t),
        Value::Object(map) => map.values_mut().fold(false, |t, v| shorten_strings(v) | t),
        _ => false,
    }
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
fn ceil_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// Rebuilds the epochs from stored rows; unreadable rows are counted, not fatal.
pub fn parse(rows: ChatHistoryRows) -> Saved {
    let mut saved = Saved {
        epochs: vec![],
        lost: rows.skipped,
        floor: rows.skipped_keys.iter().map(|(_, item)| *item).max(),
        last_epoch: rows.skipped_keys.iter().map(|(epoch, _)| *epoch).max(),
    };
    let epoch = |saved: &mut Saved, index: u64| -> usize {
        match saved.epochs.iter().position(|e| e.index == index) {
            Some(position) => position,
            None => {
                saved.epochs.push(SavedEpoch {
                    index,
                    ..Default::default()
                });
                saved.epochs.len() - 1
            }
        }
    };
    for (index, meta) in rows.epochs {
        let position = epoch(&mut saved, index);
        match serde_json::from_str(&meta) {
            Ok(meta) => saved.epochs[position].meta = meta,
            Err(_) => saved.lost += 1,
        }
    }
    for (index, _, payload) in rows.items {
        match serde_json::from_str::<Item>(&payload) {
            Ok(item) => {
                let position = epoch(&mut saved, index);
                saved.epochs[position].items.push(item);
            }
            Err(_) => saved.lost += 1,
        }
    }
    saved.epochs.sort_by_key(|epoch| epoch.index);
    for epoch in &mut saved.epochs {
        epoch.items.sort_by_key(|item| item.id.0);
    }
    saved
}

/// Settles what was still in progress when the chat was saved: streams end, running
/// tools count as cancelled, unanswered requests expire, and a turn left open ends as
/// interrupted (with a note). Returns the indices of items changed or added.
pub fn settle(items: &mut Vec<Item>) -> Vec<usize> {
    let mut changed = vec![];
    for (index, item) in items.iter_mut().enumerate() {
        let before = item.body.clone();
        match &mut item.body {
            Body::Text(block) | Body::Thinking(block) => block.streaming = false,
            Body::Tool(call) if !call.state.finished() => {
                call.state = super::transcript::ToolState::Cancelled;
            }
            Body::Permission(permission) if permission.decision == Decision::Pending => {
                permission.decision = Decision::Cancelled;
                permission.resolution.get_or_insert(Resolution::Expired);
            }
            _ => {}
        }
        if item.body != before {
            changed.push(index);
        }
    }
    // Only a turn something happened in (a message, a reply, work) can be left open.
    let last_turn = items
        .iter()
        .rev()
        .filter(|item| item.parent.is_none())
        .find(|item| {
            matches!(
                item.body,
                Body::User(_)
                    | Body::Text(_)
                    | Body::Thinking(_)
                    | Body::Tool(_)
                    | Body::TurnEnd(_)
            )
        })
        .map(|item| item.turn);
    let ended = last_turn.is_none_or(|turn| {
        items
            .iter()
            .any(|item| item.turn == turn && matches!(item.body, Body::TurnEnd(_)))
    });
    if let (Some(turn), false) = (last_turn, ended) {
        let mut next = items.iter().map(|item| item.id.0).max().unwrap_or(0);
        for body in [
            Body::Notice(Notice {
                kind: NoticeKind::Diagnostic,
                text: "Canopy closed during this turn.".into(),
                until: None,
            }),
            Body::TurnEnd(TurnEnd {
                outcome: Outcome::Interrupted,
                usage: None,
                cost_usd: None,
                duration_ms: None,
                resume_at: None,
                quiet: false,
            }),
        ] {
            next += 1;
            items.push(Item {
                id: super::transcript::ItemId(next),
                turn,
                parent: None,
                body,
                trimmed: false,
            });
            changed.push(items.len() - 1);
        }
    }
    changed
}

/// Where a chat keeps its attached images.
pub fn attachments_dir(data_dir: &Path, chat: &str) -> PathBuf {
    data_dir.join("chat-attachments").join(chat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::transcript::{
        Delivery, ItemId, ToolCall, ToolOutput, ToolState, TurnId, UserMessage,
    };
    use serde_json::json;

    fn item(id: u64, turn: u64, body: Body) -> Item {
        Item {
            id: ItemId(id),
            turn: TurnId(turn),
            parent: None,
            body,
            trimmed: false,
        }
    }
    fn tool(state: ToolState, output: &str, input: Value) -> Body {
        Body::Tool(ToolCall {
            kind: Default::default(),
            call_id: "toolu_1".into(),
            name: "Bash".into(),
            input,
            input_partial: "{".into(),
            state,
            output: Some(ToolOutput {
                text: output.into(),
                is_error: false,
                structured: Some(json!({"big": "x".repeat(STRUCTURED_LIMIT)})),
            }),
            elapsed_seconds: None,
            activity: None,
            background: false,
        })
    }
    fn user(text: &str) -> Body {
        Body::User(UserMessage {
            text: text.into(),
            delivery: Delivery::Sent,
            uuid: Some("u1".into()),
            attachments: vec![],
            context: None,
            goal: false,
        })
    }

    #[test]
    fn long_outputs_keep_start_and_end_in_the_saved_copy() {
        let output = format!("{}{}", "a".repeat(100_000), "ż".repeat(20_000));
        let input = json!({"content": "c".repeat(INPUT_LIMIT + 10), "path": "x"});
        let original = item(1, 1, tool(ToolState::Succeeded, &output, input));
        let saved = stored(&original);
        assert!(saved.trimmed && !original.trimmed);
        let Body::Tool(call) = &saved.body else {
            panic!("tool")
        };
        let text = &call.output.as_ref().expect("output").text;
        assert!(text.starts_with("aaa") && text.ends_with("żż"));
        assert!(text.contains("trimmed in saved history"));
        assert!(text.len() < OUTPUT_LIMIT);
        assert!(call.output.as_ref().expect("output").structured.is_none());
        assert!(call.input["content"].as_str().expect("content").len() < INPUT_LIMIT + 64);
        assert_eq!(call.input["path"], "x");
        assert!(call.input_partial.is_empty());
        let small = item(2, 1, tool(ToolState::Succeeded, "ok", json!({})));
        let Body::Tool(call) = &stored(&small).body else {
            panic!("tool")
        };
        assert_eq!(call.output.as_ref().expect("output").text, "ok");
    }

    #[test]
    fn rows_become_ordered_epochs_and_bad_rows_are_counted() {
        let one = item_json(&item(1, 1, user("a"))).expect("json");
        let two = item_json(&item(2, 1, user("b"))).expect("json");
        let saved = parse(ChatHistoryRows {
            epochs: vec![(
                1,
                meta_json(&EpochMeta {
                    before: Some(Boundary::NewSession),
                    continued: false,
                    agent: None,
                    ..Default::default()
                }),
            )],
            items: vec![
                (1, 2, two),
                (0, 1, one.clone()),
                (1, 1, one),
                (1, 3, "{".into()),
            ],
            skipped: 1,
            skipped_keys: vec![(2, 9)],
        });
        assert_eq!(saved.lost, 2);
        assert_eq!((saved.floor, saved.last_epoch), (Some(9), Some(2)));
        assert_eq!(
            saved.epochs.iter().map(|e| e.index).collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(saved.epochs[1].meta.before, Some(Boundary::NewSession));
        assert_eq!(
            saved.epochs[1]
                .items
                .iter()
                .map(|i| i.id.0)
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn oversized_items_are_cut_to_fit() {
        let huge = item(1, 1, user(&"x".repeat(2 * ITEM_LIMIT)));
        let json = item_json(&huge).expect("saved");
        assert!(json.len() <= ITEM_LIMIT);
        let back: Item = serde_json::from_str(&json).expect("readable");
        assert!(back.trimmed);
        let notes_only = &mut vec![item(
            1,
            0,
            Body::Notice(Notice {
                kind: NoticeKind::Diagnostic,
                text: "note".into(),
                until: None,
            }),
        )];
        assert!(settle(notes_only).is_empty(), "no turn to end");
    }

    #[test]
    fn many_small_fields_keep_a_loss_marker_and_allow_the_turn_to_finish() {
        let original = item(
            7,
            3,
            tool(ToolState::Succeeded, "ok", json!(vec!["field"; 100_000])),
        );
        let payload = item_json(&original).expect("bounded placeholder");
        assert!(payload.len() <= ITEM_LIMIT);
        let saved = parse(ChatHistoryRows {
            items: vec![
                (0, 7, payload),
                (
                    0,
                    8,
                    item_json(&item(
                        8,
                        3,
                        Body::TurnEnd(TurnEnd {
                            outcome: Outcome::Completed,
                            usage: None,
                            cost_usd: None,
                            duration_ms: None,
                            resume_at: None,
                            quiet: false,
                        }),
                    ))
                    .unwrap(),
                ),
            ],
            ..Default::default()
        });
        assert_eq!(saved.lost, 0);
        let restored = &saved.epochs[0].items;
        assert!(matches!(&restored[0].body, Body::Notice(n) if n.text.contains("omitted")));
        assert!(restored[0].trimmed);
        assert_eq!(
            (restored[0].id, restored[0].turn),
            (original.id, original.turn)
        );
        assert!(
            matches!(&restored[1].body, Body::TurnEnd(end) if end.outcome == Outcome::Completed)
        );
        assert!(
            matches!(&original.body, Body::Tool(_)),
            "live content stays intact"
        );
    }

    #[test]
    fn an_open_turn_settles_as_interrupted() {
        let mut items = vec![
            item(1, 1, user("go")),
            item(2, 1, tool(ToolState::Running, "", json!({}))),
        ];
        let changed = settle(&mut items);
        assert_eq!(changed, [1, 2, 3]);
        assert!(matches!(
            &items[3].body,
            Body::TurnEnd(TurnEnd {
                outcome: Outcome::Interrupted,
                ..
            })
        ));
        assert_eq!(items[3].id, ItemId(4));
        let mut finished = items.clone();
        assert!(settle(&mut finished).is_empty(), "settled once");
    }
}
