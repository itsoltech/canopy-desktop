//! Row model of one conversation thread — the main chat, or a subagent opened from its
//! card: which transcript items are shown, in what order, with per-row view state.
use crate::ui::{components::Disclosure, markdown::MarkdownView};
use canopy_desktop::{
    chat::{
        presentation,
        transcript::{Body, Change, Item, Permission, Transcript},
    },
    motion::{BatchReveal, MotionPolicy},
};
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::*;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};

/// Where a row's item lives: the running session or a session replaced by Restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Current(usize),
    Archived { epoch: usize, index: usize },
}

pub(super) enum Kind {
    Item(Source),
    /// Separator above the output of a restarted session.
    Restarted,
    /// "Thinking…" while the agent works without writing; always the last row.
    Thinking,
    /// First row of a subagent thread: the task it was given.
    Prompt(String),
    /// Last row of a finished subagent thread: what it reported back.
    Report,
    /// Subagents started together, in one row that expands to a list.
    Subagents(Vec<Source>),
}

pub(super) struct Row {
    /// Stable identity for element ids and keyed state across splices.
    pub key: u64,
    pub kind: Kind,
    pub markdown: Option<Entity<MarkdownView>>,
    pub output: Option<Disclosure>,
}

/// A tool call of the running session, in any thread.
pub(super) struct Call {
    pub index: usize,
    pub parent: Option<String>,
}

/// What a subagent has done so far, for its card.
#[derive(Clone, Copy, Default)]
pub(super) struct Children {
    pub tools: usize,
    pub last_tool: Option<usize>,
}

pub(super) struct Rows {
    pub rows: Vec<Row>,
    pub archived: Vec<Vec<Item>>,
    pub scroller: Entity<MessageScrollerState>,
    pub reveal: BatchReveal,
    /// `None` for the main conversation, else the subagent's tool call id.
    pub thread: Option<String>,
    /// The chat's folder: finished replies link the files they mention inside it.
    pub cwd: Option<PathBuf>,
    pub calls: HashMap<String, Call>,
    pub children: HashMap<String, Children>,
    /// Transcript index → row, for the running session.
    current: HashMap<usize, usize>,
    next_key: u64,
}

impl Rows {
    pub fn new(cx: &mut App) -> Self {
        Self {
            rows: vec![],
            archived: vec![],
            scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            reveal: BatchReveal::default(),
            thread: None,
            cwd: None,
            calls: HashMap::new(),
            children: HashMap::new(),
            current: HashMap::new(),
            next_key: 0,
        }
    }

    pub fn in_folder(mut self, cwd: Option<PathBuf>) -> Self {
        self.cwd = cwd;
        self
    }

    /// The conversation of the subagent run by `call`, built from what already happened.
    pub fn thread(
        call: &str,
        transcript: &Transcript,
        thinking: bool,
        cwd: Option<PathBuf>,
        cx: &mut App,
    ) -> Self {
        let mut rows = Self::new(cx);
        rows.thread = Some(call.to_owned());
        rows.cwd = cwd;
        let prompt = transcript.items().iter().find_map(|item| match &item.body {
            Body::Tool(tool) if tool.call_id == call => {
                presentation::subagent_prompt(tool).map(str::to_owned)
            }
            _ => None,
        });
        if let Some(prompt) = prompt {
            let key = rows.key();
            rows.rows.push(Row {
                key,
                kind: Kind::Prompt(prompt),
                markdown: None,
                output: None,
            });
        }
        let mut ignored = vec![];
        for index in 0..transcript.items().len() {
            rows.insert(transcript, index, &mut ignored, cx);
        }
        rows.report(transcript, cx);
        if thinking {
            rows.push_thinking();
        }
        let count = rows.rows.len();
        rows.scroller = cx.new(|cx| MessageScrollerState::new(count, cx));
        rows
    }

    pub fn item<'a>(
        &'a self,
        source: Source,
        transcript: Option<&'a Transcript>,
    ) -> Option<&'a Item> {
        match source {
            Source::Current(index) => transcript?.get(index),
            Source::Archived { epoch, index } => self.archived.get(epoch)?.get(index),
        }
    }

    pub fn has_items(&self) -> bool {
        self.rows
            .iter()
            .any(|row| matches!(row.kind, Kind::Item(_) | Kind::Subagents(_)))
    }

    /// The subagent whose tool call asked for this permission, if any.
    pub fn permission_thread(&self, permission: &Permission) -> Option<&str> {
        let call = self.calls.get(permission.call_id.as_deref()?)?;
        call.parent.as_deref()
    }

    fn thinking_shown(&self) -> bool {
        matches!(self.rows.last().map(|row| &row.kind), Some(Kind::Thinking))
    }

    /// Applies transcript changes and the thinking indicator in one splice batch.
    pub fn sync(
        &mut self,
        transcript: &Transcript,
        changes: &[Change],
        thinking: bool,
        policy: MotionPolicy,
        cx: &mut App,
    ) {
        let before = self.rows.len() - usize::from(self.thinking_shown());
        let mut remeasure = vec![];
        for change in changes {
            match *change {
                Change::Inserted(index) => self.insert(transcript, index, &mut remeasure, cx),
                Change::Updated(index) => {
                    if let Some(&row) = self.current.get(&index) {
                        if let (Some(markdown), Some(Body::Text(block))) = (
                            self.rows[row].markdown.clone(),
                            transcript.get(index).map(|item| &item.body),
                        ) {
                            let text: Arc<str> = block.text.as_str().into();
                            let done = !block.streaming;
                            let cwd = self.cwd.clone();
                            markdown.update(cx, |view, cx| {
                                view.stream(text, cx);
                                if done {
                                    view.finish(cwd, cx);
                                }
                            });
                        }
                        remeasure.push(row);
                    }
                    // A subagent's progress changes its card.
                    if let Some(row) = transcript
                        .get(index)
                        .and_then(|item| item.parent.as_deref())
                        .and_then(|parent| self.card_row(parent))
                    {
                        remeasure.push(row);
                    }
                    if self
                        .thread
                        .as_ref()
                        .and_then(|call| self.calls.get(call))
                        .is_some_and(|call| call.index == index)
                    {
                        self.report(transcript, cx);
                    }
                }
            }
        }
        let inserted = self.rows.len() - usize::from(self.thinking_shown()) - before;
        if inserted > 0 {
            self.reveal.begin(before, inserted, Instant::now(), policy);
            self.scroller
                .update(cx, |state, cx| state.splice(before..before, inserted, cx));
        }
        if thinking != self.thinking_shown() {
            if thinking {
                self.push_thinking();
                let at = self.rows.len() - 1;
                self.scroller
                    .update(cx, |state, cx| state.splice(at..at, 1, cx));
            } else {
                // Thinking is last: inserted rows were placed before it.
                let at = self
                    .rows
                    .iter()
                    .rposition(|row| matches!(row.kind, Kind::Thinking))
                    .expect("thinking row");
                self.rows.remove(at);
                self.scroller
                    .update(cx, |state, cx| state.splice(at..at + 1, 0, cx));
            }
        }
        remeasure.sort_unstable();
        remeasure.dedup();
        for row in remeasure {
            self.scroller
                .update(cx, |state, cx| state.remeasure_items(row..row + 1, cx));
        }
    }

    fn push_thinking(&mut self) {
        let key = self.key();
        self.rows.push(Row {
            key,
            kind: Kind::Thinking,
            markdown: None,
            output: None,
        });
    }

    fn card_row(&self, call: &str) -> Option<usize> {
        let call = self.calls.get(call)?;
        self.current.get(&call.index).copied()
    }

    /// Only this thread's items get rows; every subagent's permission request also
    /// shows in the main chat, because it blocks the whole agent.
    fn belongs(&self, item: &Item) -> bool {
        match &item.body {
            Body::Permission(permission) => {
                self.thread.is_none()
                    || self.permission_thread(permission) == self.thread.as_deref()
            }
            _ => item.parent == self.thread,
        }
    }

    fn insert(
        &mut self,
        transcript: &Transcript,
        index: usize,
        remeasure: &mut Vec<usize>,
        cx: &mut App,
    ) {
        let Some(item) = transcript.get(index) else {
            return;
        };
        if let Body::Tool(tool) = &item.body {
            self.calls.insert(
                tool.call_id.clone(),
                Call {
                    index,
                    parent: item.parent.clone(),
                },
            );
            if let Some(parent) = item.parent.as_ref().filter(|_| presentation::visible(item)) {
                let children = self.children.entry(parent.clone()).or_default();
                children.tools += 1;
                children.last_tool = Some(index);
            }
        }
        if let Some(row) = item.parent.as_deref().and_then(|p| self.card_row(p)) {
            remeasure.push(row);
        }
        if !presentation::visible(item) || !self.belongs(item) {
            return;
        }
        if let Some(row) = self.join_subagents(transcript, index) {
            self.current.insert(index, row);
            remeasure.push(row);
            return;
        }
        let markdown = match &item.body {
            Body::Text(block) => Some(chat_markdown(
                block.text.as_str().into(),
                (!block.streaming).then(|| self.cwd.clone()),
                cx,
            )),
            _ => None,
        };
        let output =
            matches!(item.body, Body::Tool(_)).then(|| Disclosure::new(false, Instant::now()));
        let at = self.rows.len() - usize::from(self.thinking_shown());
        let key = self.key();
        self.rows.insert(
            at,
            Row {
                key,
                kind: Kind::Item(Source::Current(index)),
                markdown,
                output,
            },
        );
        self.current.insert(index, at);
    }

    /// Adds a subagent to the subagent row right above it (same turn and thread);
    /// returns that row.
    fn join_subagents(&mut self, transcript: &Transcript, index: usize) -> Option<usize> {
        let item = transcript.get(index)?;
        if !matches!(&item.body, Body::Tool(tool) if presentation::is_subagent(tool)) {
            return None;
        }
        let at = self
            .rows
            .len()
            .checked_sub(1 + usize::from(self.thinking_shown()))?;
        let joins = |source: &Source| match source {
            Source::Current(previous) => transcript.get(*previous).is_some_and(|other| {
                other.turn == item.turn
                    && other.parent == item.parent
                    && matches!(&other.body, Body::Tool(tool) if presentation::is_subagent(tool))
            }),
            Source::Archived { .. } => false,
        };
        let row = &mut self.rows[at];
        match &mut row.kind {
            Kind::Item(source) if joins(source) => {
                row.kind = Kind::Subagents(vec![*source, Source::Current(index)]);
            }
            Kind::Subagents(sources) if sources.last().is_some_and(joins) => {
                sources.push(Source::Current(index));
            }
            _ => return None,
        }
        Some(at)
    }

    /// Adds the subagent's report once its call has finished with output.
    fn report(&mut self, transcript: &Transcript, cx: &mut App) {
        let Some(Body::Tool(tool)) = self
            .thread
            .as_ref()
            .and_then(|call| self.calls.get(call))
            .and_then(|call| transcript.get(call.index))
            .map(|item| &item.body)
        else {
            return;
        };
        let Some(output) = tool.output.as_ref().filter(|_| tool.state.finished()) else {
            return;
        };
        if output.text.trim().is_empty() {
            return;
        }
        let text: Arc<str> = output.text.as_str().into();
        if let Some(markdown) = self
            .rows
            .iter()
            .filter(|row| matches!(row.kind, Kind::Report))
            .find_map(|row| row.markdown.clone())
        {
            markdown.update(cx, |view, cx| view.stream(text, cx));
            return;
        }
        let markdown = chat_markdown(text, Some(self.cwd.clone()), cx);
        let at = self.rows.len() - usize::from(self.thinking_shown());
        let key = self.key();
        self.rows.insert(
            at,
            Row {
                key,
                kind: Kind::Report,
                markdown: Some(markdown),
                output: None,
            },
        );
    }

    /// Freezes the replaced session's rows above a separator; new output follows.
    pub fn archive(&mut self, transcript: &Transcript, cx: &mut App) {
        if self.thinking_shown() {
            let at = self.rows.len() - 1;
            self.rows.pop();
            self.scroller
                .update(cx, |state, cx| state.splice(at..at + 1, 0, cx));
        }
        let epoch = self.archived.len();
        self.archived.push(transcript.items().to_vec());
        let archive = |source: &mut Source| {
            if let Source::Current(index) = *source {
                *source = Source::Archived { epoch, index };
            }
        };
        for row in &mut self.rows {
            match &mut row.kind {
                Kind::Item(source) => archive(source),
                Kind::Subagents(sources) => sources.iter_mut().for_each(archive),
                _ => {}
            }
        }
        self.current.clear();
        self.calls.clear();
        self.children.clear();
        if self.has_items() {
            let key = self.key();
            self.rows.push(Row {
                key,
                kind: Kind::Restarted,
                markdown: None,
                output: None,
            });
            let at = self.rows.len() - 1;
            self.scroller
                .update(cx, |state, cx| state.splice(at..at, 1, cx));
        }
    }

    pub fn toggle_output(&mut self, key: u64, cx: &mut App) {
        let now = Instant::now();
        if let Some((index, row)) = self
            .rows
            .iter_mut()
            .enumerate()
            .find(|(_, row)| row.key == key)
            && let Some(output) = &mut row.output
        {
            output.toggle(now, cx);
            self.scroller
                .update(cx, |state, cx| state.remeasure_items(index..index + 1, cx));
        }
    }

    /// Whether row-local motion (entrance, disclosures) still needs frames.
    pub fn animating(&self, now: Instant) -> bool {
        self.reveal.is_animating(now)
            || self
                .rows
                .iter()
                .any(|row| row.output.as_ref().is_some_and(|o| o.active(now)))
    }

    fn key(&mut self) -> u64 {
        self.next_key += 1;
        self.next_key
    }
}

/// `finished` carries the folder for file links once the text is final.
fn chat_markdown(
    text: Arc<str>,
    finished: Option<Option<PathBuf>>,
    cx: &mut App,
) -> Entity<MarkdownView> {
    cx.new(|cx| {
        let mut view = MarkdownView::chat(cx);
        view.stream(text, cx);
        if let Some(cwd) = finished {
            view.finish(cwd, cx);
        }
        view
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_desktop::chat::{claude::wire, session::ChatSession};
    use core::prelude::v1::test;

    /// The recorded background-subagent session (see tests/chat_claude_fixtures.rs).
    fn subagent_session() -> ChatSession {
        recorded("subagent")
    }

    fn recorded(name: &str) -> ChatSession {
        let (mut session, _) = ChatSession::new();
        session.submit("prompt");
        let path = format!(
            "{}/tests/fixtures/chat/claude-2.1.294/{name}.jsonl",
            env!("CARGO_MANIFEST_DIR")
        );
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let message = wire::parse_line(line.as_bytes()).unwrap().unwrap();
            session.on_message(message);
            // Allow the subagent's Read so it can finish.
            if let Some(index) = session.transcript().items().iter().position(|item| {
                matches!(&item.body, Body::Permission(p) if p.decision == canopy_desktop::chat::transcript::Decision::Pending)
            }) {
                let id = session.transcript().items()[index].id;
                session.decide(id, true);
            }
        }
        session
    }

    fn items<'a>(rows: &'a Rows, transcript: &'a Transcript) -> Vec<&'a Item> {
        rows.rows
            .iter()
            .filter_map(|row| match row.kind {
                Kind::Item(source) => rows.item(source, Some(transcript)),
                _ => None,
            })
            .collect()
    }

    #[gpui_kit::test]
    fn subagent_work_stays_out_of_the_main_chat(cx: &mut TestAppContext) {
        let mut session = subagent_session();
        let changes = session.take_changes();
        let transcript = session.transcript();
        let call = transcript
            .items()
            .iter()
            .find_map(|item| match &item.body {
                Body::Tool(tool) if presentation::is_subagent(tool) => Some(tool.call_id.clone()),
                _ => None,
            })
            .unwrap();
        cx.update(|cx| {
            let mut main = Rows::new(cx);
            main.sync(transcript, &changes, false, MotionPolicy::Reduced, cx);
            let shown = items(&main, transcript);
            assert!(shown.iter().all(|item| item.parent.is_none()));
            assert!(
                shown
                    .iter()
                    .any(|item| matches!(&item.body, Body::Tool(t) if t.call_id == call))
            );
            assert!(
                main.children[&call].tools >= 1,
                "the card counts the subagent's tools"
            );

            let thread = Rows::thread(&call, transcript, false, None, cx);
            assert!(matches!(
                thread.rows.first().map(|r| &r.kind),
                Some(Kind::Prompt(_))
            ));
            assert!(matches!(
                thread.rows.last().map(|r| &r.kind),
                Some(Kind::Report)
            ));
            let inner = items(&thread, transcript);
            assert!(!inner.is_empty());
            assert!(inner.iter().all(|item| match &item.body {
                Body::Permission(p) => thread.permission_thread(p) == Some(call.as_str()),
                _ => item.parent.as_deref() == Some(call.as_str()),
            }));
        });
    }

    #[gpui_kit::test]
    fn subagents_started_together_share_one_row(cx: &mut TestAppContext) {
        let mut session = recorded("parallel-subagents");
        let changes = session.take_changes();
        let transcript = session.transcript();
        cx.update(|cx| {
            let mut main = Rows::new(cx);
            main.sync(transcript, &changes, false, MotionPolicy::Reduced, cx);
            let groups: Vec<usize> = main
                .rows
                .iter()
                .filter_map(|row| match &row.kind {
                    Kind::Subagents(sources) => Some(sources.len()),
                    _ => None,
                })
                .collect();
            assert_eq!(groups, [2]);
            assert!(!main.rows.iter().any(|row| matches!(
                row.kind,
                Kind::Item(source) if main.item(source, Some(transcript)).is_some_and(|item| matches!(&item.body, Body::Tool(t) if presentation::is_subagent(t)))
            )));
        });
    }
}
