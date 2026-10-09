//! Chat pane: owns the headless agent session and its process for one PaneId.
use super::{
    decision::DecisionPanel,
    edit::Editing,
    queue::{QueueBar, Waiting},
    rows::Rows,
};
use crate::app_state::AppState;
use canopy_desktop::chat::mentions::{self, FileRef};
use canopy_desktop::{
    agents::launch as hook_launch,
    chat::{
        claude::wire::{self, Inbound},
        config::{ChatSettings, Effort, Mode},
        history::{Boundary, EpochMeta, Saved, SavedEpoch},
        launch::{HookConfig, fork_at, prepare_claude},
        presentation,
        process::{Event, Process},
        session::{ChatSession, QueuedId, Status},
        transcript::{Body, Change, Transcript},
    },
    motion::{self, Transition, TransitionSpec},
    state::workspace::Pane,
};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

pub struct ChatStateChanged;

impl EventEmitter<ChatStateChanged> for ChatView {}

/// An opened subagent conversation (read-only), stacked for nested subagents.
pub(super) struct Thread {
    pub call: String,
    pub title: String,
    pub rows: Rows,
    /// From an earlier session: complete, nothing more arrives.
    pub archived: bool,
}

/// The chat's saved conversation, read once when the view is created.
pub(super) enum History {
    Loading(#[allow(dead_code)] Task<()>),
    Loaded,
}

/// One in-flight shutdown shared by every caller (pane close, Stop worktree, quit).
struct Closing {
    generation: u64,
    _task: Task<()>,
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
}

pub struct ChatView {
    pub(super) pane: Pane,
    pub(super) settings: ChatSettings,
    pub(super) session: Option<ChatSession>,
    process: Option<Arc<Process>>,
    /// Lines produced before the process exists (initialize, early messages).
    outbox: Vec<serde_json::Value>,
    pub(super) agent_run: Option<String>,
    generation: u64,
    pub(super) starting: bool,
    pub(super) error: Option<String>,
    start_task: Option<Task<()>>,
    pump: Option<Task<()>>,
    closing: Option<Closing>,
    pub(super) rows: Rows,
    /// Subagent conversations opened from their cards; the last one is shown.
    pub(super) threads: Vec<Thread>,
    /// Content transition when a subagent conversation opens or closes.
    pub(super) drill: Transition,
    pub(super) queue: QueueBar,
    /// Permission, question or plan waiting on the user, above the composer.
    pub(super) decision: DecisionPanel,
    /// Files read or written by finished tool calls that exist in the chat's folder.
    pub(super) tool_files: HashMap<String, FileRef>,
    checked_tools: HashSet<String>,
    pub(super) composer: Entity<TextareaState>,
    pub(super) focus: FocusHandle,
    visible: bool,
    focused: bool,
    needs_focus: bool,
    /// 0 = Send, 1 = Stop.
    pub(super) action: Transition,
    /// What the notch was last told this chat waits for.
    pub(super) reported_decision: Option<String>,
    /// Restored with a provider session: earlier messages exist but are not shown.
    pub(super) resumed: bool,
    pub(super) sent_any: bool,
    /// The registry closed this pane (or the app quits): never start again.
    pub(super) closed: bool,
    /// An account switch or an edit is restarting the process; ignore further switches.
    pub(super) switching: bool,
    /// Row key whose Copy shows a check mark, and since when.
    pub(super) copied: Option<(u64, Instant)>,
    pub(super) copied_reset: Option<Task<()>>,
    /// A sent message being edited in the composer.
    pub(super) editing: Option<Editing>,
    /// The next start forks the session after this message (an edit).
    pub(super) fork_at: Option<String>,
    /// Sent as soon as the next session starts (the edited message).
    pub(super) send_on_start: Option<canopy_desktop::chat::session::Draft>,
    pub(super) attachments: super::attachments::Attachments,
    pub(super) navigator: super::navigator::Navigator,
    /// Separator shown above the replaced session at the next start.
    pub(super) boundary: Boundary,
    /// Number of the running session in the chat's saved history.
    pub(super) epoch: u64,
    pub(super) history: History,
    /// The last saved session, continued by the first start.
    restored: Option<SavedEpoch>,
    /// Said in the conversation at the first start (history that couldn't be read).
    history_note: Option<String>,
    /// Restored items already saved: their insertion isn't written again.
    pub(super) history_skip: usize,
    /// The saved history couldn't be read: nothing is written over it.
    pub(super) history_off: bool,
    /// Ids of saved rows that couldn't be read (a newer version): new ones go after.
    item_floor: u64,
    /// `start` was asked for while the history was loading.
    start_waiting: bool,
    /// Redraws while a retry or limit countdown is shown.
    ticker: Option<Task<()>>,
    /// The edited message sent at start, until the session is up (back on failure).
    edit_recovery: Option<canopy_desktop::chat::session::Draft>,
    _subscriptions: Vec<Subscription>,
}

impl ChatView {
    pub fn new(pane: Pane, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .placeholder("Ask anything")
                .submit_on_enter(true)
        });
        let subscription = cx.subscribe_in(
            &composer,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.command_enter(window, cx),
                InputEvent::PressEnter { shift: false, .. } => {
                    if this.editing.is_some() {
                        this.send_edit(window, cx);
                    } else if this.decision.active().is_some() {
                        this.decision_enter(window, cx);
                    } else {
                        this.submit(window, cx);
                    }
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        );
        let mut settings = pane.metadata.chat.as_deref().cloned().unwrap_or_default();
        let resumed = pane.metadata.resume_id.is_some();
        let history = match settings.id.clone() {
            Some(chat) => {
                let load = cx
                    .global::<AppState>()
                    .chat_history
                    .clone()
                    .update(cx, |history, cx| history.load(&chat, cx));
                History::Loading(cx.spawn(async move |this, cx| {
                    let saved = load.await;
                    let _ = this.update(cx, |this, cx| this.restore_history(saved, cx));
                }))
            }
            None => {
                // A chat from before saved history: it gets an id now.
                settings.id = Some(ChatSettings::new_id());
                let this = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = this.update(cx, |this, cx| this.persist(None, cx));
                });
                History::Loaded
            }
        };
        Self {
            settings,
            session: None,
            process: None,
            outbox: vec![],
            agent_run: None,
            generation: 0,
            starting: false,
            error: None,
            start_task: None,
            pump: None,
            closing: None,
            rows: Rows::new(cx)
                .in_folder(files_root(&pane))
                .with_badges(super::attachments::badge_factory(cx.entity().downgrade())),
            threads: vec![],
            drill: Transition::new(1., Instant::now()),
            queue: QueueBar::default(),
            decision: DecisionPanel::default(),
            tool_files: HashMap::new(),
            checked_tools: HashSet::new(),
            composer,
            focus: cx.focus_handle(),
            visible: false,
            focused: false,
            needs_focus: false,
            action: Transition::new(0., Instant::now()),
            reported_decision: None,
            resumed,
            sent_any: false,
            closed: false,
            switching: false,
            copied: None,
            copied_reset: None,
            editing: None,
            fork_at: None,
            send_on_start: None,
            attachments: Default::default(),
            navigator: Default::default(),
            boundary: Boundary::Restarted,
            epoch: 0,
            history,
            restored: None,
            history_note: None,
            history_skip: 0,
            history_off: false,
            item_floor: 0,
            start_waiting: false,
            ticker: None,
            edit_recovery: None,
            _subscriptions: vec![subscription],
            pane,
        }
    }

    pub fn cwd(&self) -> Option<std::path::PathBuf> {
        self.pane.metadata.cwd.clone()
    }
    /// Identifies the chat's saved history.
    pub fn chat_id(&self) -> Option<String> {
        self.settings.id.clone()
    }
    /// Created only to show its history (a turn diff): the agent hasn't started.
    pub fn dormant(&self) -> bool {
        self.session.is_none() && !self.starting && !self.closed && self.error.is_none()
    }

    /// Shows the saved conversation: earlier sessions above their separators, the last
    /// one continued by the agent at start.
    fn restore_history(&mut self, saved: Result<Saved, String>, cx: &mut Context<Self>) {
        self.history = History::Loaded;
        let chat = self.settings.id.clone();
        let saved = match saved {
            Ok(saved) => saved,
            Err(error) => {
                // What is saved stays as it is; this run of the chat isn't saved.
                self.history_off = true;
                self.history_note = Some(format!(
                    "Earlier messages couldn't be loaded ({error}); this conversation isn't saved."
                ));
                Saved::default()
            }
        };
        self.item_floor = saved.floor.unwrap_or(0);
        if saved.lost > 0 {
            self.history_note = Some(format!(
                "{} saved {} couldn't be read.",
                saved.lost,
                if saved.lost == 1 { "entry" } else { "entries" }
            ));
        }
        let mut epochs = saved.epochs;
        let last = epochs.pop();
        let history = cx.global::<AppState>().chat_history.clone();
        let settle = |epoch: &mut SavedEpoch, cx: &mut Context<Self>| {
            let changed = canopy_desktop::chat::history::settle(&mut epoch.items);
            if let Some(chat) = &chat
                && !changed.is_empty()
            {
                let items: Vec<_> = changed.iter().map(|&i| epoch.items[i].clone()).collect();
                let index = epoch.index;
                history.update(cx, |history, cx| {
                    history.record(chat, index, None, items, false, cx)
                });
            }
        };
        for position in 0..epochs.len() {
            let below = epochs
                .get(position + 1)
                .or(last.as_ref())
                .and_then(|next| next.meta.before)
                .unwrap_or(Boundary::Restarted);
            let epoch = &mut epochs[position];
            settle(epoch, cx);
            let images = images_in(epoch.items.iter());
            self.load_previews(images, cx);
            let transcript = Transcript::restore(std::mem::take(&mut epoch.items));
            self.rows
                .restore(transcript, epoch.index, below.label(), cx);
        }
        if let Some(mut last) = last {
            settle(&mut last, cx);
            self.epoch = last.index;
            self.restored = Some(last);
            // The conversation is on screen; nothing is missing anymore.
            self.resumed = false;
        }
        // Sessions kept for a newer version: this run starts after them.
        if let Some(unread) = saved.last_epoch.filter(|&unread| unread >= self.epoch) {
            self.epoch = unread + 1;
            if let Some(last) = self.restored.take() {
                // Its rows stay saved, but it's not continued after a newer one.
                let transcript = Transcript::restore(last.items);
                self.rows
                    .restore(transcript, last.index, Boundary::Restarted.label(), cx);
            }
        }
        if std::mem::take(&mut self.start_waiting) {
            self.start(cx);
        }
        cx.emit(ChatStateChanged);
        cx.notify();
    }
    pub fn status(&self) -> Option<&Status> {
        self.session.as_ref().map(ChatSession::status)
    }
    /// A process is running or starting; Stop worktree has something to stop.
    pub fn is_live(&self) -> bool {
        self.starting || self.process.is_some() || !self.closing_done()
    }
    pub fn is_working(&self) -> bool {
        matches!(
            self.status(),
            Some(Status::Working | Status::NeedsDecision | Status::Interrupting)
        )
    }
    pub fn can_restart(&self) -> bool {
        !self.starting
            && self.closing_done()
            && (self.error.is_some() || self.status().is_some_and(Status::ended))
    }
    fn closing_done(&self) -> bool {
        self.closing
            .as_ref()
            .is_none_or(|closing| closing.completion.is_closed())
    }
    pub fn request_focus(&mut self, cx: &mut Context<Self>) {
        self.needs_focus = true;
        cx.notify();
    }
    pub(super) fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.needs_focus)
    }
    pub fn set_active(&mut self, visible: bool, focused: bool, cx: &mut Context<Self>) {
        if focused && !self.focused {
            self.needs_focus = true;
        }
        let changed = self.visible != visible || self.focused != focused;
        self.visible = visible;
        self.focused = focused;
        if changed {
            cx.notify();
        }
    }

    /// Starts (or restarts) the agent with the pane's current account and choices.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.closed || self.starting || !self.closing_done() || self.process.is_some() {
            return;
        }
        if matches!(self.history, History::Loading(_)) {
            self.start_waiting = true;
            return;
        }
        let app = cx.global::<AppState>().clone();
        // Fresh launch identity: resume id and account may have changed since creation.
        if let Some(pane) = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .into_iter()
            .flat_map(|workspace| workspace.all_panes())
            .find(|pane| pane.id == self.pane.id)
        {
            self.pane = pane;
        }
        let Some(env) = app.tools.read(cx).environment.clone() else {
            self.error = Some("The shell environment is still loading.".into());
            cx.notify();
            return;
        };
        let env = match env {
            Ok(env) => env,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        // A replaced session's subagents are no longer running.
        self.threads.clear();
        // Unsent messages survive Restart and wait for the user again.
        let mut queued = vec![];
        let replaced = self.session.is_some();
        if let Some(session) = &mut self.session {
            queued = session.take_queue();
            let transcript = session.take_transcript();
            self.rows
                .archive(transcript, self.epoch, self.boundary.label(), cx);
        }
        let boundary = std::mem::replace(&mut self.boundary, Boundary::Restarted);
        // An open edit belongs to the session being replaced; its draft comes back.
        if let Some(editing) = self.editing.take() {
            self.restore_draft(editing.draft, cx);
        }
        // Forking needs the session it continues.
        let fork = self
            .fork_at
            .take()
            .filter(|_| self.pane.metadata.resume_id.is_some());
        self.generation += 1;
        self.closing = None;
        self.error = None;
        self.starting = true;
        let (mut session, mut lines) = ChatSession::with_settings(self.settings.clone());
        session.restore_queue(queued);
        let continued = fork.is_some() || self.pane.metadata.resume_id.is_some();
        let restored = self.restored.take().filter(|_| !replaced);
        let meta = match &restored {
            // The saved session goes on: it keeps how it began.
            Some(epoch) => epoch.meta.clone(),
            None => {
                if replaced {
                    self.epoch += 1;
                }
                // New sessions start at item 1 again: no unread rows to land on.
                self.item_floor = 0;
                EpochMeta {
                    before: replaced.then_some(boundary),
                    continued,
                }
            }
        };
        session.set_base(meta.base());
        if let Some(epoch) = restored {
            self.history_skip = epoch.items.len();
            session.restore_history_after(epoch.items, self.item_floor);
        } else if let Some(chat) = self.settings.id.as_ref().filter(|_| !self.history_off) {
            let epoch = self.epoch;
            app.chat_history.update(cx, |history, cx| {
                history.record(chat, epoch, Some(meta), [], false, cx)
            });
        }
        if let Some(note) = self.history_note.take() {
            // Said once, not saved.
            self.history_skip = session.transcript().items().len() + 1;
            session.note(note);
        }
        if let Some(draft) = self.send_on_start.take() {
            // Held until initialize, then sent as the new branch's first message; kept
            // to give back if the start fails.
            lines.extend(session.submit_draft(draft.clone()));
            self.edit_recovery = Some(draft);
        }
        self.session = Some(session);
        self.outbox = lines;
        // Restored history shows before the agent answers anything.
        self.after_change(cx);
        let registration = match app
            .agents
            .update(cx, |agents, cx| agents.register(self.pane.clone(), cx))
        {
            Ok(registration) => registration,
            Err(error) => {
                self.fail_start(error, cx);
                return;
            }
        };
        self.agent_run = Some(registration.run.clone());
        // A new run starts without anything reported to the notch.
        self.reported_decision = None;
        let catalog = app.tools.read(cx).catalog.clone();
        let pane = self.pane.clone();
        let settings = self.settings.clone();
        let generation = self.generation;
        self.start_task = Some(cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                    let helper = hook_launch::helper_path(&executable);
                    hook_launch::validate_helper(&helper)?;
                    let mut launch = prepare_claude(
                        &catalog,
                        &pane,
                        &env,
                        &settings,
                        Some(HookConfig {
                            helper: &helper,
                            registration: &registration,
                        }),
                    )?;
                    if let Some(at) = &fork {
                        fork_at(&mut launch, at);
                    }
                    Process::start(
                        launch.spec,
                        launch.files,
                        wire::parse_line,
                        wire::MAX_LINE_BYTES,
                    )
                })
                .await;
            let current = this
                .update(cx, |this, _| this.generation == generation)
                .unwrap_or(false);
            if !current {
                // Superseded (closed or restarted meanwhile): never adopt the process,
                // and finish closing it inside this task so shutdown can await it.
                if let Ok((process, _)) = started {
                    let _ = cx
                        .background_executor()
                        .spawn(async move { process.close() })
                        .await;
                }
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.starting = false;
                match started {
                    Ok((process, events)) => {
                        let process = Arc::new(process);
                        for line in this.outbox.drain(..) {
                            let _ = process.send(&line);
                        }
                        this.process = Some(process);
                        this.pump = Some(this.pump(events, generation, cx));
                    }
                    Err(error) => this.fail_start(error, cx),
                }
                cx.emit(ChatStateChanged);
                cx.notify();
            });
        }));
        cx.emit(ChatStateChanged);
        cx.notify();
    }

    fn fail_start(&mut self, error: String, cx: &mut Context<Self>) {
        self.starting = false;
        self.error = Some(error);
        if let Some(draft) = self.edit_recovery.take() {
            self.restore_draft(draft, cx);
        }
        self.outbox.clear();
        // The session never started: end it so the composer stops accepting messages.
        if let Some(session) = &mut self.session {
            session.close_requested();
            session.on_event(Event::Exited(canopy_desktop::chat::process::Exit {
                code: None,
                signal: None,
                stderr: String::new(),
            }));
        }
        self.after_change(cx);
        self.finish_agent(true, true, cx);
        cx.emit(ChatStateChanged);
        cx.notify();
    }

    fn pump(
        &self,
        events: async_channel::Receiver<Event<Inbound>>,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Ok(first) = events.recv().await {
                // Everything already waiting becomes one update and one frame.
                let mut batch = vec![first];
                while batch.len() < 512
                    && let Ok(event) = events.try_recv()
                {
                    batch.push(event);
                }
                let alive = this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return false;
                        }
                        this.apply(batch, cx);
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
    }

    fn apply(&mut self, batch: Vec<Event<Inbound>>, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        let mut lines = vec![];
        let mut exited = false;
        for event in batch {
            exited |= matches!(event, Event::Exited(_));
            lines.extend(session.on_event(event));
        }
        self.write(lines);
        if exited {
            self.process = None;
            self.pump = None;
            let failed = matches!(self.status(), Some(Status::Failed(_)));
            self.finish_agent(failed, true, cx);
            cx.emit(ChatStateChanged);
        }
        self.after_change(cx);
    }

    /// Mirrors session state into rows, persisted choices and the Send/Stop control.
    pub(super) fn after_change(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        let changes = session.take_changes();
        let thinking = presentation::thinking(session.transcript(), session.status());
        let working = matches!(
            session.status(),
            Status::Working | Status::NeedsDecision | Status::Interrupting
        );
        let mut settings = session.settings().clone();
        // Rules remembered for the chat live in its settings, not the session's.
        settings.allowed = self.settings.allowed.clone();
        settings.id = self.settings.id.clone();
        settings.allow(session.take_remembered());
        let policy = motion::policy(cx);
        let session = self.session.as_ref().expect("session");
        let transcript = session.transcript();
        let files = file_tools(transcript, &changes, &self.checked_tools);
        let waiting = if working {
            vec![]
        } else {
            waiting_subagents(transcript, &self.rows, self.pane.metadata.cwd.as_deref())
        };
        self.rows.sync(transcript, &changes, thinking, policy, cx);
        for thread in self.threads.iter_mut().filter(|thread| !thread.archived) {
            let thinking = thread_thinking(transcript, &thread.call);
            thread.rows.sync(transcript, &changes, thinking, policy, cx);
        }
        let images = images_in(changes.iter().filter_map(|change| match *change {
            Change::Inserted(index) | Change::Updated(index) => transcript.get(index),
        }));
        self.queue.sync(&waiting, session.queue(), policy);
        self.save_history(&changes, cx);
        self.load_previews(images, cx);
        self.sync_decision(policy, cx);
        self.report_decision(cx);
        self.sync_edit(cx);
        self.tick(cx);
        if self
            .session
            .as_ref()
            .is_some_and(|s| !matches!(s.status(), Status::Starting))
        {
            self.edit_recovery = None;
        }
        self.action.retarget(
            if working { 1. } else { 0. },
            TransitionSpec::new(motion::duration::FAST, motion::Easing::EaseInOut),
            Instant::now(),
            policy,
        );
        if settings != self.settings {
            // The CLI may change the mode itself (e.g. leaving plan mode).
            self.settings = settings;
            self.persist(None, cx);
        }
        self.resolve_tool_files(files, cx);
        self.save_session(cx);
        cx.notify();
    }

    /// Hands changed items to the chat's saved history.
    fn save_history(&mut self, changes: &[Change], cx: &mut Context<Self>) {
        let skip = std::mem::take(&mut self.history_skip);
        let (Some(chat), Some(session)) = (&self.settings.id, &self.session) else {
            return;
        };
        if self.history_off {
            return;
        }
        let transcript = session.transcript();
        let items: Vec<_> = changes
            .iter()
            .filter_map(|change| match *change {
                Change::Inserted(index) if index < skip => None,
                Change::Inserted(index) | Change::Updated(index) => transcript.get(index).cloned(),
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let urgent = items
            .iter()
            .any(|item| matches!(item.body, Body::TurnEnd(_)));
        let epoch = self.epoch;
        cx.global::<AppState>()
            .chat_history
            .clone()
            .update(cx, |history, cx| {
                history.record(chat, epoch, None, items, urgent, cx)
            });
    }

    /// Records the provider session for resume once the conversation has a message;
    /// until then the CLI has not written it, and the account may still change.
    fn save_session(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .session
            .as_ref()
            .and_then(ChatSession::session_id)
            .filter(|_| self.sent_any)
            .map(str::to_owned)
        else {
            return;
        };
        if self.pane.metadata.resume_id.as_deref() == Some(id.as_str()) {
            return;
        }
        let pane = self.pane.clone();
        cx.global::<AppState>()
            .projects
            .clone()
            .update(cx, |projects, cx| {
                projects.record_agent_session(&pane, &id, cx)
            });
        self.pane.metadata.resume_id = Some(id);
    }

    /// Checks off the UI thread which tool paths are files Canopy can open.
    fn resolve_tool_files(&mut self, files: Vec<(String, String)>, cx: &mut Context<Self>) {
        let Some(cwd) = files_root(&self.pane).filter(|_| !files.is_empty()) else {
            return;
        };
        self.checked_tools
            .extend(files.iter().map(|(call, _)| call.clone()));
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    files
                        .into_iter()
                        .filter_map(|(call, path)| {
                            let mention = mentions::Mention {
                                range: 0..0,
                                label: String::new(),
                                path,
                                line: None,
                            };
                            Some((call, mentions::resolve(&cwd, &mention)?))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !found.is_empty() {
                    this.tool_files.extend(found);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn write(&mut self, lines: Vec<serde_json::Value>) {
        match &self.process {
            Some(process) => {
                for line in lines {
                    let _ = process.send(&line);
                }
            }
            None => self.outbox.extend(lines),
        }
    }

    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(cx);
        if self.attachments_blocked(cx)
            || (draft.text.trim().is_empty() && draft.attachments.is_empty())
            || self.session.as_ref().is_none_or(|s| s.status().ended())
        {
            return;
        }
        if canopy_desktop::chat::attachments::needs_data(&draft.attachments) {
            // Saved images are read again first; the composer empties once they are.
            self.with_images(draft, cx, |this, draft, cx| {
                this.attachments.pending.clear();
                this.decision.composer = Some(super::decision::ComposerChange {
                    text: Some(String::new().into()),
                    placeholder: super::decision::COMPOSER_PLACEHOLDER,
                });
                this.send_draft(draft, cx)
            });
        } else {
            self.clear_composer(window, cx);
            self.send_draft(draft, cx);
        }
    }

    fn send_draft(&mut self, draft: canopy_desktop::chat::session::Draft, cx: &mut Context<Self>) {
        let title = if draft.text.trim().is_empty() {
            "Image".to_owned()
        } else {
            presentation::title(&draft.text)
        };
        let Some(session) = &mut self.session else {
            return;
        };
        let lines = session.submit_draft(draft);
        self.write(lines);
        if !self.sent_any {
            self.sent_any = true;
            cx.global::<AppState>()
                .projects
                .clone()
                .update(cx, |projects, cx| {
                    projects.update_chat(self.pane.id, None, Some(&title), cx)
                });
            let pane = self.pane.id;
            cx.global::<AppState>()
                .agents
                .clone()
                .update(cx, |agents, cx| agents.rename(pane, &title, cx));
        }
        self.after_change(cx);
    }

    /// Files the agent changed in `turn` of session `session` (running or earlier).
    pub fn turn_changes(
        &self,
        session: u64,
        turn: u64,
    ) -> Option<Vec<canopy_desktop::chat::tools::FileChange>> {
        let restored;
        let transcript = if session == self.epoch {
            match (&self.session, &self.restored) {
                (Some(session), _) => session.transcript(),
                // Not started yet (shown for a turn diff): the saved session.
                (None, Some(saved)) => {
                    restored = Transcript::restore(saved.items.clone());
                    &restored
                }
                (None, None) => return None,
            }
        } else {
            &self
                .rows
                .archived
                .iter()
                .find(|archived| archived.epoch == session)?
                .transcript
        };
        let turn = canopy_desktop::chat::transcript::TurnId(turn);
        transcript
            .items()
            .iter()
            .any(|item| item.turn == turn)
            .then(|| canopy_desktop::chat::tools::turn_changes(transcript, turn))
    }

    /// Opens (or reuses) the pane with `turn`'s changes, scrolled to `file`.
    pub(super) fn open_turn_diff(
        &mut self,
        epoch: Option<u64>,
        turn: u64,
        file: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        use canopy_desktop::state::workspace::{PaneKind, PaneMetadata};
        let epoch = epoch.unwrap_or(self.epoch);
        let target = canopy_desktop::chat::tools::TurnDiffTarget {
            chat: self.pane.id,
            session: epoch,
            turn,
            file,
        };
        let Ok(resource) = serde_json::to_string(&target) else {
            return;
        };
        let chat = self.pane.id;
        let cwd = self.cwd();
        let name = self
            .pane
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| super::NEW_CHAT_TITLE.into());
        let workspace = cx.global::<AppState>().workspace.clone();
        workspace.update(cx, |w, cx| {
            let same_turn = |p: &canopy_desktop::state::workspace::Pane| {
                p.metadata.kind == PaneKind::TurnDiff
                    && p.metadata
                        .resource
                        .as_deref()
                        .and_then(|s| {
                            serde_json::from_str::<canopy_desktop::chat::tools::TurnDiffTarget>(s)
                                .ok()
                        })
                        .is_some_and(|t| t.chat == chat && t.session == epoch && t.turn == turn)
            };
            if let Some(pane) = w.all_panes().into_iter().find(|p| same_turn(p))
                && let Some(tab) = w
                    .tabs()
                    .iter()
                    .filter(|t| t.root.find(pane.id).is_some())
                    .map(|t| t.id)
                    .next()
            {
                let mut metadata = pane.metadata.clone();
                metadata.resource = Some(resource);
                let _ = w.set_pane_metadata(tab, pane.id, metadata);
                let _ = w.activate(tab);
                let _ = w.focus(tab, pane.id);
                cx.notify();
                return;
            }
            let title = format!("Changes · {name}");
            let tab = w.open(title.clone(), "turn-diff");
            let pane = w.active().expect("opened tab").focused;
            let _ = w.set_pane_metadata(
                tab,
                pane,
                PaneMetadata {
                    kind: PaneKind::TurnDiff,
                    cwd,
                    resource: Some(resource),
                    title: Some(title),
                    ..Default::default()
                },
            );
            cx.notify();
        });
    }

    /// Rows of the conversation on screen: the main chat or an opened subagent.
    pub(super) fn shown_rows(&self) -> &Rows {
        self.threads
            .last()
            .map_or(&self.rows, |thread| &thread.rows)
    }
    pub(super) fn shown_rows_mut(&mut self) -> &mut Rows {
        match self.threads.last_mut() {
            Some(thread) => &mut thread.rows,
            None => &mut self.rows,
        }
    }

    /// Opens the conversation of the subagent run by tool call `call`.
    /// `archived`: position of an earlier session in the shown rows.
    pub(super) fn open_thread(
        &mut self,
        call: String,
        archived: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let earlier = match archived {
            Some(position) => match self.shown_rows().archived.get(position) {
                Some(archived) => Some(archived.clone()),
                None => return,
            },
            None => None,
        };
        let transcript = match (&earlier, &self.session) {
            (Some(archived), _) => &archived.transcript,
            (None, Some(session)) => session.transcript(),
            (None, None) => return,
        };
        let Some(title) = transcript.items().iter().find_map(|item| match &item.body {
            Body::Tool(tool) if tool.call_id == call => Some(presentation::subagent_title(tool)),
            _ => None,
        }) else {
            return;
        };
        let cwd = files_root(&self.pane);
        let rows = match &earlier {
            Some(archived) => Rows::archived_thread(&call, archived.clone(), cwd, cx),
            None => {
                let thinking = thread_thinking(transcript, &call);
                Rows::thread(&call, transcript, thinking, cwd, cx)
            }
        };
        self.threads.push(Thread {
            call,
            title,
            rows,
            archived: archived.is_some(),
        });
        self.restart_drill(cx);
        // The composer is locked; Esc and ⌘. still reach the pane.
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Back to the conversation that opened the current subagent, where it was left.
    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.threads.pop().is_none() {
            return;
        }
        self.restart_drill(cx);
        if self.threads.is_empty() {
            let composer = self.composer.clone();
            composer.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    fn restart_drill(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.drill = Transition::new(0., now);
        self.drill
            .retarget(1., motion::presets::STATE_CHANGE, now, motion::policy(cx));
    }

    /// ⌘⏎: allow a pending prompt, else steer the typed message, else the oldest queued one.
    pub(super) fn command_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            self.send_edit(window, cx);
            return;
        }
        if self.decision.active().is_some() {
            self.decision_primary(window, cx);
            return;
        }
        let draft = self.composer_draft(cx);
        if !draft.text.trim().is_empty() || !draft.attachments.is_empty() {
            if self.attachments_blocked(cx) {
                return;
            }
            if self.is_working() {
                self.clear_composer(window, cx);
                self.with_images(draft, cx, |this, draft, cx| {
                    if let Some(session) = &mut this.session {
                        let lines = session.steer_draft(draft);
                        this.write(lines);
                    }
                    this.after_change(cx);
                });
            } else {
                self.submit(window, cx);
            }
            return;
        }
        let oldest = self.session.as_ref().and_then(|session| {
            session
                .queue()
                .iter()
                .filter(|q| q.steering.is_none())
                .map(|q| q.id)
                .next()
        });
        if let Some(id) = oldest {
            self.steer(id, cx);
        }
    }

    pub(super) fn resume_queue(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            let lines = session.resume_queue();
            self.write(lines);
            self.after_change(cx);
        }
    }

    pub(super) fn steer(&mut self, id: QueuedId, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            let lines = session.steer(id);
            self.write(lines);
            self.after_change(cx);
        }
    }

    /// Moves a queued message back into the message field; a draft there takes its
    /// place at the end of the queue, so nothing typed is lost.
    pub(super) fn edit(&mut self, id: QueuedId, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        let Some(queued) = session.edit(id) else {
            return;
        };
        session.enqueue_draft(draft);
        self.set_composer_draft(queued, window, cx);
        self.after_change(cx);
    }

    /// Drag and drop in the queue: `id` goes in front of `before`.
    pub(super) fn move_queued(&mut self, id: QueuedId, before: QueuedId, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session
            && session.move_queued(id, Some(before))
        {
            self.after_change(cx);
        }
    }

    pub(super) fn remove(&mut self, id: QueuedId, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session
            && session.remove(id)
        {
            self.after_change(cx);
        }
    }

    /// Messages that would be lost if the pane closed now.
    pub fn unsent(&self) -> usize {
        self.session.as_ref().map_or(0, ChatSession::unsent)
    }
    pub fn has_unsent(&self) -> bool {
        self.unsent() > 0
    }
    pub fn discard_queue(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            session.discard_unsent();
            self.after_change(cx);
        }
    }

    pub(super) fn interrupt(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            let lines = session.interrupt();
            self.write(lines);
            self.after_change(cx);
        }
    }

    /// The oldest permission still waiting for the user.
    /// Starting or closing: Restart waits for it.
    pub(super) fn busy(&self) -> bool {
        self.starting || !self.closing_done()
    }

    /// Keeps a retry or limit countdown current: a redraw each second while seconds
    /// show, every half minute while minutes do; nothing once no countdown runs.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.countdown_left().is_none() {
            self.ticker = None;
            return;
        }
        if self.ticker.is_some() {
            return;
        }
        self.ticker = Some(cx.spawn(async move |this, cx| {
            let mut wait = std::time::Duration::from_secs(1);
            loop {
                cx.background_executor().timer(wait).await;
                let left = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        let left = this.countdown_left();
                        if left.is_none() {
                            this.ticker = None;
                        }
                        left
                    })
                    .ok()
                    .flatten();
                let Some(left) = left else {
                    break;
                };
                wait = std::time::Duration::from_secs(if left < 90 { 1 } else { 30 });
            }
        }));
    }

    /// Seconds until the nearest retry or limit reset shown in the conversation.
    fn countdown_left(&self) -> Option<u64> {
        let session = self.session.as_ref()?;
        session
            .transcript()
            .items()
            .iter()
            .rev()
            .take(200)
            .filter_map(|item| match &item.body {
                Body::Notice(notice) => super::actions::seconds_left(notice.until),
                _ => None,
            })
            .min()
    }

    /// Forgets a rule remembered for this chat. The running process keeps it until Restart:
    /// the CLI cannot drop a session rule.
    pub(super) fn remove_rule(&mut self, rule: &str, cx: &mut Context<Self>) {
        self.settings.allowed.retain(|allowed| allowed != rule);
        self.persist(None, cx);
        cx.notify();
    }

    pub(super) fn set_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        self.change(|session| session.set_model(model), cx);
    }
    pub(super) fn set_effort(&mut self, effort: Option<Effort>, cx: &mut Context<Self>) {
        self.change(|session| session.set_effort(effort), cx);
    }
    pub(super) fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.change(|session| session.set_mode(mode), cx);
    }
    fn change(
        &mut self,
        f: impl FnOnce(&mut ChatSession) -> Vec<serde_json::Value>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &mut self.session else {
            return;
        };
        let lines = f(session);
        let settings = session.settings().clone();
        self.write(lines);
        if settings != self.settings {
            self.settings = settings;
            self.persist(None, cx);
            self.remember(cx);
        }
        self.after_change(cx);
    }

    /// Switches the account before the first message: a new process for the new profile.
    pub(super) fn set_account(&mut self, profile: String, cx: &mut Context<Self>) {
        if self.closed
            || self.switching
            || self.account_locked()
            || self.pane.metadata.profile_id.as_deref() == Some(&profile)
        {
            return;
        }
        self.switching = true;
        self.pane.metadata.profile_id = Some(profile.clone());
        // Models belong to the account; the new one starts from its default model.
        self.settings.model = None;
        self.persist(Some(profile), cx);
        self.remember(cx);
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            let _ = stop.await;
            let _ = this.update(cx, |this, cx| {
                this.switching = false;
                if this.closed {
                    return;
                }
                this.session = None;
                this.rows = Rows::new(cx)
                    .in_folder(files_root(&this.pane))
                    .with_badges(super::attachments::badge_factory(cx.entity().downgrade()));
                this.threads.clear();
                this.queue.clear();
                this.start(cx);
            });
        })
        .detach();
    }

    /// The account cannot change once the conversation has messages (stage 8).
    pub fn account_locked(&self) -> bool {
        self.resumed
            || self.sent_any
            || self.rows.has_items()
            || self.pane.metadata.resume_id.is_some()
    }

    fn persist(&mut self, profile: Option<String>, cx: &mut Context<Self>) {
        let pane = self.pane.id;
        let settings = self.settings.clone();
        cx.global::<AppState>()
            .projects
            .clone()
            .update(cx, |projects, cx| {
                if let Some(profile) = profile {
                    projects.set_chat_account(pane, profile, cx);
                }
                projects.update_chat(pane, Some(settings), None, cx)
            });
    }

    /// The latest choices become the defaults of the next new chat.
    fn remember(&self, cx: &mut Context<Self>) {
        use canopy_desktop::settings::{Change, PreferenceKey};
        let mut changes = vec![
            match &self.settings.model {
                Some(model) => Change::ChatModel(model.clone()),
                None => Change::Reset(PreferenceKey::ChatModel),
            },
            match self.settings.effort {
                Some(effort) => Change::ChatEffort(effort),
                None => Change::Reset(PreferenceKey::ChatEffort),
            },
        ];
        // Yolo applies only to the chat that chose it, never to the next new chat.
        if self.settings.mode != Mode::Yolo {
            changes.push(Change::ChatMode(self.settings.mode));
        }
        if let Some(profile) = &self.pane.metadata.profile_id {
            changes.push(Change::ChatProfile(profile.clone()));
        }
        cx.global::<AppState>()
            .settings
            .clone()
            .update(cx, |settings, cx| settings.remember(changes, cx));
    }

    /// Nothing runs, waits or is being edited, and there is a conversation to leave.
    pub fn can_new_session(&self) -> bool {
        self.session.is_some()
            && !self.starting
            && !self.switching
            && !self.closed
            && !self.is_working()
            && self.decision.active().is_none()
            && self.editing.is_none()
            && self.rows.has_items()
    }

    /// Starts a new conversation in this chat with the same account and choices. The
    /// agent no longer remembers the earlier one, which stays above a "New session"
    /// separator; unsent messages wait for the new conversation.
    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        if !self.can_new_session() {
            return;
        }
        self.switching = true;
        self.fork_at = None;
        let pane = self.pane.id;
        self.pane.metadata.resume_id = None;
        cx.global::<AppState>()
            .projects
            .clone()
            .update(cx, |projects, cx| projects.forget_chat_session(pane, cx));
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            let _ = stop.await;
            let _ = this.update(cx, |this, cx| {
                this.switching = false;
                if !this.closed {
                    this.boundary = Boundary::NewSession;
                    this.start(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn restart(&mut self, cx: &mut Context<Self>) {
        if self.can_restart() {
            self.process = None;
            self.start(cx);
        }
    }

    fn finish_agent(&mut self, failed: bool, natural: bool, cx: &mut Context<Self>) {
        if let Some(run) = self.agent_run.take() {
            let pane = self.pane.id;
            cx.global::<AppState>()
                .agents
                .clone()
                .update(cx, |agents, cx| {
                    agents.finish(pane, &run, failed, natural, cx)
                });
        }
    }

    /// The pane is gone or the app quits: stop for good.
    pub fn close(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.closed = true;
        self.shutdown(cx)
    }

    /// Stops the agent process; every caller joins the same bounded close.
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        if let Some(closing) = &self.closing
            && closing.generation == self.generation
        {
            return wait(closing.completion.clone(), closing.result.clone(), cx);
        }
        self.generation += 1;
        self.starting = false;
        self.pump = None;
        let start = self.start_task.take();
        self.finish_agent(false, false, cx);
        if let Some(session) = &mut self.session {
            session.close_requested();
        }
        let process = self.process.take();
        let (finished, completion) = async_channel::bounded::<()>(1);
        let result = Arc::new(std::sync::Mutex::new(None));
        let shared = result.clone();
        let task = cx.spawn(async move |this, cx| {
            // A start in flight sees the new generation and closes its own process.
            if let Some(start) = start {
                start.await;
            }
            let closed = match process {
                Some(process) => {
                    cx.background_executor()
                        .spawn(async move { process.close() })
                        .await
                }
                None => Ok(()),
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(session) = &mut this.session
                    && !session.status().ended()
                {
                    session.on_event(Event::Exited(canopy_desktop::chat::process::Exit {
                        code: None,
                        signal: None,
                        stderr: String::new(),
                    }));
                }
                this.after_change(cx);
                cx.emit(ChatStateChanged);
            });
            *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(closed);
            drop(finished);
        });
        self.closing = Some(Closing {
            generation: self.generation,
            _task: task,
            completion: completion.clone(),
            result: result.clone(),
        });
        cx.emit(ChatStateChanged);
        cx.notify();
        wait(completion, result, cx)
    }
}

/// Background subagents still running after the agent's turn ended.
fn waiting_subagents(
    transcript: &canopy_desktop::chat::transcript::Transcript,
    rows: &Rows,
    cwd: Option<&std::path::Path>,
) -> Vec<Waiting> {
    let cwd = cwd.unwrap_or(std::path::Path::new(""));
    transcript
        .items()
        .iter()
        .filter_map(|item| match &item.body {
            Body::Tool(tool)
                if tool.background && !tool.state.finished() && presentation::is_subagent(tool) =>
            {
                let last = rows
                    .children
                    .get(&tool.call_id)
                    .and_then(|children| children.last_tool)
                    .and_then(|index| match &transcript.get(index)?.body {
                        Body::Tool(child) => Some(child),
                        _ => None,
                    });
                Some(Waiting {
                    call: tool.call_id.clone(),
                    title: presentation::subagent_title(tool),
                    activity: presentation::subagent_preview(tool, last, cwd),
                })
            }
            _ => None,
        })
        .collect()
}

/// Finished file tools in `changes` whose path was not checked yet: (call id, path).
fn file_tools(
    transcript: &canopy_desktop::chat::transcript::Transcript,
    changes: &[canopy_desktop::chat::transcript::Change],
    checked: &HashSet<String>,
) -> Vec<(String, String)> {
    use canopy_desktop::chat::transcript::{Change, ToolState};
    changes
        .iter()
        .filter_map(|change| {
            let (Change::Inserted(index) | Change::Updated(index)) = *change;
            match &transcript.get(index)?.body {
                Body::Tool(tool)
                    if tool.state == ToolState::Succeeded && !checked.contains(&tool.call_id) =>
                {
                    let key = match tool.name.as_str() {
                        "Read" | "Write" | "Edit" | "MultiEdit" => "file_path",
                        "NotebookEdit" | "NotebookRead" => "notebook_path",
                        _ => return None,
                    };
                    let path = tool.input.get(key)?.as_str()?.to_owned();
                    Some((tool.call_id.clone(), path))
                }
                _ => None,
            }
        })
        .collect()
}

/// Images sent in user messages among `items`.
fn images_in<'a>(
    items: impl Iterator<Item = &'a canopy_desktop::chat::transcript::Item>,
) -> Vec<canopy_desktop::chat::transcript::Attachment> {
    items
        .filter_map(|item| match &item.body {
            Body::User(message) => Some(message.attachments.iter()),
            _ => None,
        })
        .flatten()
        .filter(|attachment| attachment.is_image())
        .cloned()
        .collect()
}

/// The folder file mentions are relative to: the chat's working directory.
fn files_root(pane: &Pane) -> Option<std::path::PathBuf> {
    pane.metadata.cwd.clone()
}

fn thread_thinking(transcript: &canopy_desktop::chat::transcript::Transcript, call: &str) -> bool {
    transcript.items().iter().any(|item| {
        matches!(&item.body, Body::Tool(tool)
            if tool.call_id == call && presentation::thread_thinking(transcript, call, tool))
    })
}

fn wait(
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    cx: &mut Context<ChatView>,
) -> Task<Result<(), String>> {
    cx.spawn(async move |_, _| {
        let _ = completion.recv().await;
        result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| Err("Chat cleanup result was lost.".into()))
    })
}
