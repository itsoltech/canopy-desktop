//! Row model of one conversation thread — the main chat, or a subagent opened from its
//! card: which transcript items are shown, in what order, with per-row view state.
use super::scroller::ChatScrollerState;
use crate::ui::{components::Disclosure, markdown::MarkdownView};
use canopy_desktop::{
    chat::{
        presentation,
        tools::{self, FileChange, ToolView},
        transcript::{
            Body, Change, Decision, Delivery, Item, Outcome, Permission, PermissionKind, Transcript,
        },
    },
    motion::{BatchReveal, MotionPolicy},
};
use gpui_kit::*;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};

/// A session replaced by Restart, Edit or New session (or restored from history),
/// kept whole so its diffs and subagent conversations still open.
pub(super) struct Archived {
    /// Stable session number of the chat (saved history, turn diff targets).
    pub epoch: u64,
    pub transcript: Transcript,
}

/// Where a row's item lives: the running session or a session replaced by Restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Current(usize),
    Archived { epoch: usize, index: usize },
}

pub(super) enum Kind {
    Item(Source),
    /// Separator above the output of a replaced session ("Session restarted").
    Separator(&'static str),
    /// "Thinking…" while the agent works without writing; always the last row.
    Thinking,
    /// First row of a subagent thread: the task it was given.
    Prompt(String),
    /// Last row of a finished subagent thread: what it reported back.
    Report,
    /// Subagents started together, in one row that expands to a list.
    Subagents(Vec<Source>),
    /// Files a finished turn changed; `epoch` of an earlier session (`None`: running).
    Changes {
        turn: u64,
        files: Vec<FileChange>,
        epoch: Option<u64>,
    },
}

pub(super) struct Row {
    /// Stable identity for element ids and keyed state across splices.
    pub key: u64,
    pub kind: Kind,
    pub markdown: Option<Entity<MarkdownView>>,
    pub output: Option<Disclosure>,
    pub extra: RowExtra,
}

/// Per-row view state beyond text and disclosure.
#[derive(Default)]
pub(super) struct RowExtra {
    /// What a finished tool did, prepared once (with preview highlighting).
    pub card: Option<Arc<ToolCard>>,
    /// "Show more" of a long diff or preview.
    pub expanded: bool,
    /// When a thinking block started and stopped streaming, for "Thought for Ns".
    pub thought: Option<(Option<Instant>, Option<Instant>)>,
    /// Folders folded in a turn's change summary.
    pub collapsed: std::collections::HashSet<String>,
    /// The user opened or closed the row: thinking no longer folds on its own.
    pub touched: bool,
    /// First work row of a finished turn: "Worked for …" folds the work up to the reply.
    pub fold: Option<Fold>,
    /// Index of the row whose `fold` hides this one while it is closed.
    pub folded_by: Option<usize>,
    /// The final reply of an ended turn: transcript index of its turn end, for the
    /// message actions and the turn's numbers.
    pub turn_end: Option<usize>,
}

/// A completed turn's work (thinking, tools, interim text, subagents) under one header,
/// leaving its prompt, final reply and change summary in view.
pub(super) struct Fold {
    pub disclosure: Disclosure,
    pub duration_ms: Option<u64>,
    /// Row of the final reply; work rows are the header row up to this one.
    pub reply: usize,
}

pub(super) struct ToolCard {
    pub view: ToolView,
    /// Syntax highlighting of a Read preview, over its whole content.
    pub highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
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
    pub archived: Vec<Arc<Archived>>,
    pub scroller: Entity<ChatScrollerState>,
    pub reveal: BatchReveal,
    /// `None` for the main conversation, else the subagent's tool call id.
    pub thread: Option<String>,
    /// The chat's folder: finished replies link the files they mention inside it.
    pub cwd: Option<PathBuf>,
    pub calls: HashMap<String, Call>,
    pub children: HashMap<String, Children>,
    /// How a sent message draws its attachment badges (the main conversation).
    badges: Option<BadgeFactory>,
    /// Transcript index → row, for the running session.
    current: HashMap<usize, usize>,
    next_key: u64,
}

impl Rows {
    pub fn new(cx: &mut App) -> Self {
        Self {
            rows: vec![],
            archived: vec![],
            scroller: cx.new(|cx| ChatScrollerState::new(0, cx)),
            reveal: BatchReveal::default(),
            thread: None,
            cwd: None,
            calls: HashMap::new(),
            children: HashMap::new(),
            current: HashMap::new(),
            badges: None,
            next_key: 0,
        }
    }

    pub fn with_badges(mut self, badges: BadgeFactory) -> Self {
        self.badges = Some(badges);
        self
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
                extra: RowExtra::default(),
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
        rows.scroller = cx.new(|cx| ChatScrollerState::new(count, cx));
        rows
    }

    pub fn item<'a>(
        &'a self,
        source: Source,
        transcript: Option<&'a Transcript>,
    ) -> Option<&'a Item> {
        match source {
            Source::Current(index) => transcript?.get(index),
            Source::Archived { epoch, index } => self.archived.get(epoch)?.transcript.get(index),
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
                        self.refresh(transcript, index, row, cx);
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
            extra: RowExtra::default(),
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
            self.fold_turn(transcript, index, remeasure);
            self.summarize_turn(transcript, index);
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
            // Attachments mentioned in a sent message stay badges.
            Body::User(message) => self.badges.as_ref().and_then(|badges| {
                let source =
                    super::attachments::badge_markdown(&message.text, &message.attachments)?;
                let renderer = badges(&message.attachments);
                Some(cx.new(|cx| {
                    let mut view = MarkdownView::chat(cx);
                    view.set_badges(renderer);
                    view.set_content(source.into(), String::new(), true, cx);
                    view
                }))
            }),
            _ => None,
        };
        let now = Instant::now();
        let output = match &item.body {
            Body::Tool(_) => Some(Disclosure::new(false, now)),
            // Thinking is shown while it streams, folded once it is done.
            Body::Thinking(block) => Some(Disclosure::new(block.streaming, now)),
            _ => None,
        };
        let at = self.rows.len() - usize::from(self.thinking_shown());
        let key = self.key();
        self.rows.insert(
            at,
            Row {
                key,
                kind: Kind::Item(Source::Current(index)),
                markdown,
                output,
                extra: RowExtra::default(),
            },
        );
        self.current.insert(index, at);
        if let Body::Thinking(block) = &item.body {
            self.rows[at].extra.thought = Some((block.streaming.then_some(now), None));
        }
        self.refresh(transcript, index, at, cx);
        // A turn that stopped or failed shows its end; its reply still gets actions.
        self.fold_turn(transcript, index, remeasure);
        self.summarize_turn(transcript, index);
    }

    /// Folds a completed turn's work once it ends: rows between the message that
    /// started it and its final reply (steered messages included). Turns that stopped,
    /// failed, or end without a reply stay open.
    fn fold_turn(&mut self, transcript: &Transcript, index: usize, remeasure: &mut Vec<usize>) {
        let Some(item) = transcript.get(index) else {
            return;
        };
        let Body::TurnEnd(end) = &item.body else {
            return;
        };
        if self.thread.is_some() {
            return;
        }
        let turn = item.turn;
        let of_turn = |row: &Row| {
            let source = match &row.kind {
                Kind::Item(source) => *source,
                Kind::Subagents(sources) => *sources.first()?,
                _ => return None,
            };
            let Source::Current(index) = source else {
                return None;
            };
            transcript.get(index).filter(|item| item.turn == turn)
        };
        // The turn's last main-thread text is its reply: actions and numbers go under it.
        if let Some(reply) = self.rows.iter().rposition(|row| {
            of_turn(row)
                .is_some_and(|item| item.parent.is_none() && matches!(item.body, Body::Text(_)))
        }) {
            self.rows[reply].extra.turn_end = Some(index);
            remeasure.push(reply);
        }
        if end.outcome != Outcome::Completed {
            return;
        }
        let Some(first) = self.rows.iter().position(|row| of_turn(row).is_some()) else {
            return;
        };
        let Some(reply) = self.rows.iter().rposition(|row| of_turn(row).is_some()) else {
            return;
        };
        let started = of_turn(&self.rows[first]).is_some_and(
            |item| matches!(&item.body, Body::User(m) if m.delivery == Delivery::Sent),
        );
        let replied = of_turn(&self.rows[reply])
            .is_some_and(|item| item.parent.is_none() && matches!(item.body, Body::Text(_)));
        let start = first + 1;
        if !started
            || !replied
            || start >= reply
            || !self.rows[start..reply]
                .iter()
                .all(|row| of_turn(row).is_some())
        {
            return;
        }
        self.rows[start].extra.fold = Some(Fold {
            disclosure: Disclosure::new(false, Instant::now()),
            duration_ms: end.duration_ms,
            reply,
        });
        for row in &mut self.rows[start + 1..reply] {
            row.extra.folded_by = Some(start);
        }
        remeasure.extend(start..reply);
    }

    /// Whether the row is hidden inside a closed "Worked for …" fold.
    pub fn folded(&self, row: &Row) -> bool {
        row.extra
            .folded_by
            .and_then(|header| self.rows.get(header)?.extra.fold.as_ref())
            .is_some_and(|fold| !fold.disclosure.open)
    }

    pub fn toggle_fold(&mut self, key: u64, policy: MotionPolicy, cx: &mut App) {
        let now = Instant::now();
        let Some(start) = self.rows.iter().position(|row| row.key == key) else {
            return;
        };
        let Some(fold) = &mut self.rows[start].extra.fold else {
            return;
        };
        fold.disclosure.toggle(now, cx);
        let (open, reply) = (fold.disclosure.open, fold.reply);
        if open {
            self.reveal.begin(start, reply - start, now, policy);
        }
        self.scroller
            .update(cx, |state, cx| state.remeasure_items(start..reply, cx));
    }

    /// Keeps a row's prepared state in step with its item: tool cards once a call has
    /// finished, the end of a thinking block.
    fn refresh(&mut self, transcript: &Transcript, index: usize, row: usize, cx: &mut App) {
        let Some(item) = transcript.get(index) else {
            return;
        };
        let now = Instant::now();
        let cwd = self.cwd.clone();
        let entry = &mut self.rows[row];
        match &item.body {
            // An approved plan opens from its row; the panel showed it before.
            Body::Permission(p)
                if p.kind == PermissionKind::Plan
                    && p.decision == Decision::Allowed
                    && entry.markdown.is_none() =>
            {
                if let Some(plan) = p.input["plan"].as_str() {
                    let plan: Arc<str> = plan.into();
                    entry.markdown = Some(cx.new(|cx| {
                        let mut view = MarkdownView::document(cx);
                        view.stream(plan, cx);
                        view.finish(cwd, cx);
                        view
                    }));
                    entry.output = Some(Disclosure::new(false, now));
                }
            }
            Body::Tool(tool) if tool.state.finished() && entry.extra.card.is_none() => {
                let view = tools::view(tool);
                let highlights = match &view {
                    ToolView::Read { path, content, .. } => preview_highlights(path, content, cx),
                    _ => vec![],
                };
                // Changes are shown right away (DEC-034).
                if matches!(view, ToolView::Change(_)) {
                    entry.output = Some(Disclosure::new(true, now));
                }
                entry.extra.card = Some(Arc::new(ToolCard { view, highlights }));
            }
            Body::Thinking(block) if !block.streaming => {
                if let Some((started, ended @ None)) = &mut entry.extra.thought {
                    *ended = Some(now);
                    if started.is_some()
                        && !entry.extra.touched
                        && let Some(disclosure) = &mut entry.output
                        && disclosure.open
                    {
                        disclosure.toggle(now, cx);
                    }
                }
            }
            _ => {}
        }
    }

    /// A finished turn of the main conversation that changed files gets a summary row.
    fn summarize_turn(&mut self, transcript: &Transcript, index: usize) {
        let Some(item) = transcript.get(index) else {
            return;
        };
        if self.thread.is_some() || !matches!(item.body, Body::TurnEnd(_)) {
            return;
        }
        let files = tools::turn_changes(transcript, item.turn);
        if files.is_empty() {
            return;
        }
        let at = self.rows.len() - usize::from(self.thinking_shown());
        let key = self.key();
        self.rows.insert(
            at,
            Row {
                key,
                kind: Kind::Changes {
                    turn: item.turn.0,
                    files,
                    epoch: None,
                },
                markdown: None,
                output: None,
                extra: RowExtra::default(),
            },
        );
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
                extra: RowExtra::default(),
            },
        );
    }

    /// Freezes the replaced session's rows above a separator; new output follows.
    pub fn archive(
        &mut self,
        transcript: Transcript,
        epoch: u64,
        label: &'static str,
        cx: &mut App,
    ) {
        if self.thinking_shown() {
            let at = self.rows.len() - 1;
            self.rows.pop();
            self.scroller
                .update(cx, |state, cx| state.splice(at..at + 1, 0, cx));
        }
        self.freeze(Arc::new(Archived { epoch, transcript }));
        if self.has_items() {
            let key = self.key();
            self.rows.push(Row {
                key,
                kind: Kind::Separator(label),
                markdown: None,
                output: None,
                extra: RowExtra::default(),
            });
            let at = self.rows.len() - 1;
            self.scroller
                .update(cx, |state, cx| state.splice(at..at, 1, cx));
        }
    }

    /// Points the running session's rows at `archived` (its transcript, now kept here).
    fn freeze(&mut self, archived: Arc<Archived>) {
        let position = self.archived.len();
        let epoch = archived.epoch;
        self.archived.push(archived);
        let archive = |source: &mut Source| {
            if let Source::Current(index) = *source {
                *source = Source::Archived {
                    epoch: position,
                    index,
                };
            }
        };
        for row in &mut self.rows {
            match &mut row.kind {
                Kind::Item(source) => archive(source),
                Kind::Subagents(sources) => sources.iter_mut().for_each(archive),
                Kind::Changes { epoch: of, .. } if of.is_none() => *of = Some(epoch),
                _ => {}
            }
        }
        self.current.clear();
        self.calls.clear();
        self.children.clear();
    }

    /// Adds a saved session above the running one: its rows (turns folded as when it
    /// ran), then the separator `label`.
    pub fn restore(
        &mut self,
        mut transcript: Transcript,
        epoch: u64,
        label: &'static str,
        cx: &mut App,
    ) {
        let changes = transcript.take_changes();
        self.sync(&transcript, &changes, false, MotionPolicy::Reduced, cx);
        self.archive(transcript, epoch, label, cx);
    }

    /// The conversation of a subagent of an earlier session (read-only, nothing more
    /// arrives).
    pub fn archived_thread(
        call: &str,
        archived: Arc<Archived>,
        cwd: Option<PathBuf>,
        cx: &mut App,
    ) -> Self {
        let mut rows = Self::thread(call, &archived.transcript, false, cwd, cx);
        rows.freeze(archived);
        rows
    }

    pub fn toggle_expanded(&mut self, key: u64, cx: &mut App) {
        if let Some(index) = self.rows.iter().position(|row| row.key == key) {
            self.rows[index].extra.expanded ^= true;
            self.scroller
                .update(cx, |state, cx| state.remeasure_items(index..index + 1, cx));
        }
    }

    pub fn toggle_folder(&mut self, key: u64, folder: String, cx: &mut App) {
        if let Some(index) = self.rows.iter().position(|row| row.key == key) {
            let collapsed = &mut self.rows[index].extra.collapsed;
            if !collapsed.remove(&folder) {
                collapsed.insert(folder);
            }
            self.scroller
                .update(cx, |state, cx| state.remeasure_items(index..index + 1, cx));
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
            row.extra.touched = true;
            self.scroller
                .update(cx, |state, cx| state.remeasure_items(index..index + 1, cx));
        }
    }

    /// Whether row-local motion (entrance, disclosures) still needs frames.
    pub fn animating(&self, now: Instant) -> bool {
        self.reveal.is_animating(now)
            || self.rows.iter().any(|row| {
                row.output.as_ref().is_some_and(|o| o.active(now))
                    || row
                        .extra
                        .fold
                        .as_ref()
                        .is_some_and(|fold| fold.disclosure.active(now))
            })
    }

    fn key(&mut self) -> u64 {
        self.next_key += 1;
        self.next_key
    }
}

/// Builds a sent message's badge renderer from its attachments.
pub(super) type BadgeFactory = std::rc::Rc<
    dyn Fn(&[canopy_desktop::chat::transcript::Attachment]) -> crate::ui::markdown::BadgeRenderer,
>;

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

/// Highlighting for a file preview, by the file's extension.
fn preview_highlights(
    path: &std::path::Path,
    content: &str,
    cx: &App,
) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    // Without a theme (headless tests) there are no syntax colors to use.
    if !cx.has_global::<gpui_kit::component::Theme>() {
        return vec![];
    }
    let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
        return vec![];
    };
    let block = gpui_kit::base::text::CodeBlock::from_code(content.to_owned(), Some(extension));
    crate::ui::markdown::highlight_code(&block, cx)
}

#[cfg(test)]
mod card_tests {
    use super::*;
    use canopy_desktop::chat::{claude::wire, session::ChatSession};
    use core::prelude::v1::test;

    #[gpui_kit::test]
    fn finished_tools_get_cards_and_the_turn_a_summary(cx: &mut TestAppContext) {
        let (mut session, _) = ChatSession::new();
        session.submit("prompt");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/chat/claude-2.1.294/tools.jsonl"
        );
        for line in std::fs::read_to_string(path).unwrap().lines() {
            session.on_message(wire::parse_line(line.as_bytes()).unwrap().unwrap());
        }
        let changes = session.take_changes();
        let transcript = session.transcript();
        cx.update(|cx| {
            let mut rows = Rows::new(cx);
            rows.sync(transcript, &changes, false, MotionPolicy::Reduced, cx);
            let cards: Vec<&ToolView> = rows
                .rows
                .iter()
                .filter_map(|row| row.extra.card.as_deref().map(|card| &card.view))
                .collect();
            assert!(
                cards
                    .iter()
                    .any(|v| matches!(v, ToolView::Change(c) if !c.created))
            );
            assert!(cards.iter().any(|v| matches!(v, ToolView::Read { .. })));
            let summaries: Vec<usize> = rows
                .rows
                .iter()
                .filter_map(|row| match &row.kind {
                    Kind::Changes {
                        files, epoch: None, ..
                    } => Some(files.len()),
                    _ => None,
                })
                .collect();
            assert_eq!(summaries, [2]);
            let thoughts = rows
                .rows
                .iter()
                .filter(|row| row.extra.thought.is_some())
                .count();
            assert!(thoughts > 0, "thinking blocks have rows");
            // Diffs open by default.
            assert!(rows.rows.iter().any(|row| matches!(
                (row.extra.card.as_deref(), &row.output),
                (Some(ToolCard { view: ToolView::Change(_), .. }), Some(d)) if d.open
            )));
        });
    }

    #[gpui_kit::test]
    fn a_completed_turn_folds_its_work_above_the_reply(cx: &mut TestAppContext) {
        let (mut session, _) = ChatSession::new();
        session.submit("prompt");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/chat/claude-2.1.294/tools.jsonl"
        );
        for line in std::fs::read_to_string(path).unwrap().lines() {
            session.on_message(wire::parse_line(line.as_bytes()).unwrap().unwrap());
        }
        let changes = session.take_changes();
        let transcript = session.transcript();
        cx.update(|cx| {
            let mut rows = Rows::new(cx);
            rows.sync(transcript, &changes, false, MotionPolicy::Reduced, cx);
            let header = rows
                .rows
                .iter()
                .position(|row| row.extra.fold.is_some())
                .expect("a fold");
            let reply = rows.rows[header].extra.fold.as_ref().unwrap().reply;
            let body = |row: &Row| match row.kind {
                Kind::Item(source) => rows.item(source, Some(transcript)).map(|item| &item.body),
                _ => None,
            };
            assert!(matches!(body(&rows.rows[header - 1]), Some(Body::User(_))));
            assert!(matches!(body(&rows.rows[reply]), Some(Body::Text(_))));
            assert!(
                rows.rows[header + 1..reply]
                    .iter()
                    .all(|row| rows.folded(row))
            );
            assert!(!rows.folded(&rows.rows[reply]), "the reply stays in view");
            assert!(
                rows.rows[reply..]
                    .iter()
                    .any(|row| matches!(row.kind, Kind::Changes { .. })),
                "so does the change summary"
            );

            let key = rows.rows[header].key;
            rows.toggle_fold(key, MotionPolicy::Reduced, cx);
            assert!(!rows.rows.iter().any(|row| rows.folded(row)));
        });
    }
}

#[cfg(test)]
mod long_tests {
    use super::*;
    use core::prelude::v1::test;

    /// Row building for a long conversation (dev bench transcript). Timing only:
    /// `cargo test --release --locked long_transcript -- --ignored --nocapture`.
    #[gpui_kit::test]
    #[ignore = "timing"]
    fn long_transcript_rows_build_quickly(cx: &mut TestAppContext) {
        let mut session = crate::ui::chat::dev_bench::long_session();
        let changes = session.take_changes();
        let transcript = session.transcript();
        cx.update(|cx| {
            let started = Instant::now();
            let mut rows = Rows::new(cx);
            rows.sync(transcript, &changes, false, MotionPolicy::Reduced, cx);
            eprintln!(
                "long transcript: {} items -> {} rows in {:?}",
                transcript.items().len(),
                rows.rows.len(),
                started.elapsed()
            );
            assert!(rows.rows.len() > 1000);
        });
    }
}
