//! User keyboard shortcut overrides (`keyboard.shortcuts`), device-local.
//!
//! Stored as a JSON object from a command ID to its keystrokes, e.g.
//! `{"workspace.new_chat":["cmd-shift-k"],"chat.stop":[]}`. An empty list means
//! the command has no shortcut; a missing ID keeps the command's default.
//! Unknown IDs are kept (a newer build may know them) and ignored by the keymap.
use serde::Serialize;
use std::collections::BTreeMap;

/// Longest accepted stored value.
const LIMIT: usize = 16 * 1024;
const MAX_COMMANDS: usize = 256;
const MAX_KEYSTROKES: usize = 4;
const MAX_ID: usize = 64;
const MAX_KEYSTROKE: usize = 64;
const MODIFIERS: [&str; 8] = [
    "ctrl",
    "alt",
    "shift",
    "fn",
    "cmd",
    "super",
    "win",
    "secondary",
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ShortcutOverrides(BTreeMap<String, Vec<String>>);

impl ShortcutOverrides {
    /// The stored keystrokes of `id`; `None` keeps its default.
    pub fn get(&self, id: &str) -> Option<&[String]> {
        self.0.get(id).map(Vec::as_slice)
    }
    /// Replaces the keystrokes of `id`. Invalid values are rejected unchanged.
    pub fn set(&mut self, id: &str, keystrokes: Vec<String>) -> bool {
        if !valid_id(id)
            || keystrokes.len() > MAX_KEYSTROKES
            || !keystrokes.iter().all(|k| valid_keystroke(k))
            || (!self.0.contains_key(id) && self.0.len() >= MAX_COMMANDS)
        {
            return false;
        }
        self.0.insert(id.to_owned(), keystrokes);
        true
    }
    /// Back to the default of `id`.
    pub fn remove(&mut self, id: &str) {
        self.0.remove(id);
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.0
            .iter()
            .map(|(id, keys)| (id.as_str(), keys.as_slice()))
    }
    pub fn encode(&self) -> String {
        serde_json::to_string(&self.0).expect("shortcut serialization")
    }
    /// Strict decoding: one invalid entry rejects the whole value.
    pub fn decode(value: &str) -> Option<Self> {
        if value.len() > LIMIT {
            return None;
        }
        let map: BTreeMap<String, Vec<String>> = serde_json::from_str(value).ok()?;
        let mut overrides = Self::default();
        for (id, keystrokes) in map {
            if !overrides.set(&id, keystrokes) {
                return None;
            }
        }
        Some(overrides)
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

/// One chord in GPUI syntax: `[modifier-]*key`, e.g. `cmd-shift-k`, `ctrl--`, `f5`.
/// The UI parses it with GPUI before binding; this only rejects malformed data.
fn valid_keystroke(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_KEYSTROKE
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let (modifiers, key) = match value.strip_suffix("--") {
        Some(prefix) => (prefix, "-"),
        None if value == "-" => ("", "-"),
        None => match value.rsplit_once('-') {
            Some((prefix, key)) => (prefix, key),
            None => ("", value),
        },
    };
    !key.is_empty()
        && (modifiers.is_empty()
            || modifiers
                .split('-')
                .all(|m| MODIFIERS.iter().any(|known| m.eq_ignore_ascii_case(known))))
}
