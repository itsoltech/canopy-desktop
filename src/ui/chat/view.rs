//! Chat pane: presents one chat's runtime (`ChatRuntime`) for one window. The
//! runtime owns the agent; this view owns focus, the composer and its draft, scroll,
//! rows and motion, and sends the user's intents to the runtime.
use super::{
    decision::DecisionPanel,
    edit::Editing,
    queue::{QueueBar, Waiting},
    rows::Rows,
};
use crate::app_state::{ChatRuntime, ChatRuntimeEvent};
use crate::ui::components::ButtonLoading;
use canopy_desktop::chat::mentions::FileRef;
use canopy_desktop::{
    chat::{
        agent::Capabilities,
        presentation,
        provider::Provider,
        session::{ChatSession, Draft},
        transcript::{Body, Change, Transcript},
    },
    motion::{self, MotionPolicy, Transition, TransitionSpec},
};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

/// An opened subagent conversation (read-only), stacked for nested subagents.
pub(super) struct Thread {
    pub call: String,
    pub title: String,
    pub rows: Rows,
    /// From an earlier session: complete, nothing more arrives.
    pub archived: bool,
}

pub struct ChatView {
    pub(super) runtime: Entity<ChatRuntime>,
    pub(super) remote_revision: u64,
    pub(super) remote_anchor: Option<(u64, u64)>,
    /// Session (runtime serial) the rows show, and how many of its items they hold.
    synced: (u64, usize),
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
    /// Row key whose Copy shows a check mark, and since when.
    pub(super) copied: Option<(u64, Instant)>,
    pub(super) copied_reset: Option<Task<()>>,
    /// A sent message being edited in the composer.
    pub(super) editing: Option<Editing>,
    /// The remote message whose edit the host is checking, and its pencil's spinner.
    pub(super) edit_check: Option<(canopy_desktop::chat::transcript::ItemId, ButtonLoading)>,
    pub(super) edit_preview: Option<Task<()>>,
    pub(super) attachments: super::attachments::Attachments,
    pub(super) navigator: super::navigator::Navigator,
    /// Redraws while a retry or limit countdown is shown.
    ticker: Option<Task<()>>,
    /// The ⌘F bar and its matches (DEC-069).
    pub(super) find: Option<super::find::Find>,
    /// The source of a clicked quote, lit for a moment.
    pub(super) flash: Option<super::find::Flash>,
    /// Text views holding highlights, cleared when no longer lit.
    pub(super) lit: Vec<Entity<gpui_kit::base::TextViewState>>,
    /// Quote button for the selection in the conversation (DEC-070).
    pub(super) quote_offer: Option<super::quote::QuoteOffer>,
    /// A reply to one quote, written in its popover.
    pub(super) quote_reply: Option<super::quote::QuoteReply>,
    /// Suggestions for the word being typed (`/`, `@`, `#`, DEC-071).
    pub(super) suggest: Option<super::suggest::Suggest>,
    /// Esc closed the suggestions for the word starting here.
    pub(super) suggest_dismissed: Option<usize>,
    /// The model and reasoning popover of the composer.
    pub(super) model_picker: super::model_picker::ModelPicker,
    /// The composer shrinks to one line while the reader scrolls the conversation.
    pub(super) resting: super::resting::Resting,
    /// Room for the composer's pickers in the last frame; they shorten to fit it.
    pub(super) pickers_width: std::rc::Rc<std::cell::Cell<Pixels>>,
    /// Height of the composer and the trays over the conversation's bottom edge in the
    /// last frame, without what trays unfold over it: the list's bottom padding, so its
    /// end shows above them.
    pub(super) overlay_height: std::rc::Rc<std::cell::Cell<Pixels>>,
    /// Tray content drawn this frame that lies over the conversation instead of
    /// lifting it (`render::unfolds_over`); the overlay's canvas takes and clears it.
    pub(super) overlay_unfolded: std::rc::Rc<std::cell::Cell<Pixels>>,
    /// The overlay's height as drawn, also while the composer rests: Jump to latest
    /// sits on its top edge.
    pub(super) overlay_drawn: std::rc::Rc<std::cell::Cell<Pixels>>,
    /// Why the last `!` command couldn't run, under the composer.
    pub(super) shell_error: Option<String>,
    /// The runtime's notice under the field, kept while it leaves.
    pub(super) notice: super::composer::NoticeRow,
    /// Spinners of the remote chat's history and recovery actions.
    pub(super) remote_feedback: super::remote::RemoteFeedback,
    /// The composer runs shell commands: `!` typed into an empty field turned it on.
    pub(super) shell: bool,
    /// The composer sets the chat's goal: `/goal ` typed or picked turned it on.
    pub(super) goal_mode: bool,
    pub(super) goal_row: super::goal::GoalRow,
    /// The agent's task list and background work over the composer.
    pub(super) activity: super::activity::ActivityPanel,
    /// Code blocks in the composer, each with the text above it; the main field is
    /// under them all (DEC-088).
    pub(super) code_parts: Vec<super::code_blocks::CodePart>,
    /// The composer was empty before its latest change (a typed `!` starts shell mode).
    pub(super) composer_was_empty: bool,
    /// ↑ in the composer: which earlier message is shown, and its text (DEC-075).
    pub(super) recall: Option<super::recall::Recall>,
    /// Pages agents showed, by tool call (DEC-076).
    pub(super) pages: super::page::Pages,
    _subscriptions: Vec<Subscription>,
}

impl ChatView {
    /// A view of `pane`'s chat, through the registry's runtime for it.
    #[cfg(test)]
    pub fn new(
        pane: canopy_desktop::state::workspace::Pane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let chats = cx.global::<crate::app_state::AppState>().chats.clone();
        let runtime = chats.update(cx, |chats, cx| chats.adopt(pane, cx));
        Self::with_runtime(runtime, window, cx)
    }

    pub fn with_runtime(
        runtime: Entity<ChatRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .placeholder(super::decision::COMPOSER_PLACEHOLDER)
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
                InputEvent::Change => {
                    this.dismiss_quote_offer(cx);
                    this.shell_error = None;
                    this.composer_changed(window, cx);
                    this.wake_composer(cx);
                    this.update_suggest(cx);
                    this.end_recall(cx);
                    this.save_remote_draft(cx);
                    cx.notify()
                }
                _ => {}
            },
        );
        let model_picker = super::model_picker::ModelPicker::new(Instant::now(), window, cx);
        let search = cx.subscribe(&model_picker.search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.model_picker.highlighted = 0;
                cx.notify();
            }
        });
        let observed = cx.observe(&runtime, |this, _, cx| {
            this.follow_agent_terminals(cx);
            this.sync_notice(motion::policy(cx), cx);
            this.sync_remote_feedback(cx);
            cx.notify()
        });
        // The empty conversation names the project and branch Git reports (`details.rs`).
        let git = cx.observe(
            &cx.global::<crate::app_state::AppState>().git.clone(),
            |this, _, cx| {
                if this.rows.rows.is_empty() {
                    cx.notify()
                }
            },
        );
        let events = cx.subscribe(&runtime, |this, _, event: &ChatRuntimeEvent, cx| {
            this.on_runtime(event, cx)
        });
        let (serial, cwd) = {
            let runtime = runtime.read(cx);
            (runtime.serial(), runtime.cwd())
        };
        let rows = Rows::new(cx)
            .in_folder(cwd)
            .with_badges(super::attachments::badge_factory(cx.entity().downgrade()));
        let mut view = Self {
            runtime,
            remote_revision: 0,
            remote_anchor: None,
            synced: (serial, 0),
            rows,
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
            copied: None,
            copied_reset: None,
            editing: None,
            edit_check: None,
            edit_preview: None,
            attachments: Default::default(),
            navigator: Default::default(),
            ticker: None,
            find: None,
            flash: None,
            lit: vec![],
            quote_offer: None,
            quote_reply: None,
            suggest: None,
            suggest_dismissed: None,
            resting: super::resting::Resting::new(Instant::now()),
            model_picker,
            pickers_width: std::rc::Rc::new(std::cell::Cell::new(Pixels::MAX)),
            overlay_height: Default::default(),
            overlay_unfolded: Default::default(),
            overlay_drawn: Default::default(),
            shell_error: None,
            notice: Default::default(),
            remote_feedback: Default::default(),
            shell: false,
            goal_mode: false,
            goal_row: Default::default(),
            activity: Default::default(),
            code_parts: vec![],
            composer_was_empty: true,
            recall: None,
            pages: Default::default(),
            _subscriptions: vec![subscription, search, observed, git, events],
        };
        // What the runtime already holds shows at once, without replaying motion.
        view.catch_up_archived(cx);
        view.runtime_changed(serial, &[], MotionPolicy::Reduced, cx);
        view.sync_notice(MotionPolicy::Reduced, cx);
        view.sync_remote_feedback(cx);
        view
    }

    // The runtime, read for presentation.

    pub(super) fn rt<'a>(&self, cx: &'a App) -> &'a ChatRuntime {
        self.runtime.read(cx)
    }
    /// The Claude or Codex session, for their own features; `None` for other agents.
    pub(super) fn native<'a>(&self, cx: &'a App) -> Option<&'a ChatSession> {
        self.runtime.read(cx).native()
    }
    pub(super) fn transcript<'a>(&self, cx: &'a App) -> Option<&'a Transcript> {
        self.runtime.read(cx).transcript()
    }
    /// What the agent can do; features it lacks are offered nowhere (AGT-07).
    pub(super) fn capabilities(&self, cx: &App) -> Capabilities {
        self.rt(cx).capabilities(cx)
    }
    pub(super) fn agent_name(&self, cx: &App) -> String {
        self.rt(cx).agent_name(cx)
    }
    /// The chat's agent mark (Claude's, Codex's, else its tool's).
    pub(super) fn agent_mark(&self, cx: &App) -> AnyElement {
        super::composer::agent_icon(self.provider(cx), &self.agent_name(cx))
    }
    pub(super) fn pane<'a>(&self, cx: &'a App) -> &'a canopy_desktop::state::workspace::Pane {
        self.runtime.read(cx).pane_ref()
    }
    pub(super) fn settings<'a>(
        &self,
        cx: &'a App,
    ) -> &'a canopy_desktop::chat::config::ChatSettings {
        self.runtime.read(cx).settings()
    }
    pub fn cwd(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.rt(cx).cwd()
    }
    /// Claude or Codex; `None` for an ACP agent.
    pub fn provider(&self, cx: &App) -> Option<Provider> {
        self.rt(cx).provider()
    }
    pub fn chat_id(&self, cx: &App) -> Option<String> {
        self.rt(cx).chat_id()
    }
    pub fn is_working(&self, cx: &App) -> bool {
        self.rt(cx).is_working()
    }
    pub fn has_conversation(&self, cx: &App) -> bool {
        self.rt(cx).has_conversation() || self.transcript(cx).is_some_and(|t| !t.items().is_empty())
    }
    pub fn account_locked(&self, cx: &App) -> bool {
        self.rt(cx).account_locked()
    }
    pub(super) fn can_restart(&self, cx: &App) -> bool {
        self.rt(cx).can_restart()
    }
    pub(super) fn busy(&self, cx: &App) -> bool {
        self.rt(cx).busy()
    }

    pub fn set_title(
        &mut self,
        title: &str,
        state: Option<canopy_desktop::state::workspace::ChatTitleState>,
        cx: &mut Context<Self>,
    ) {
        self.runtime
            .update(cx, |runtime, cx| runtime.set_title(title, state, cx));
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

    // Following the runtime.

    fn on_runtime(&mut self, event: &ChatRuntimeEvent, cx: &mut Context<Self>) {
        match event {
            ChatRuntimeEvent::FileRequested(file) => cx.emit(super::ClientFileRequested {
                runtime: self.runtime.clone(),
                file: file.clone(),
            }),
            ChatRuntimeEvent::Changed { serial, changes } => {
                self.runtime_changed(*serial, changes, motion::policy(cx), cx)
            }
            ChatRuntimeEvent::Archived => {
                self.edit_preview = None;
                // A replaced session's subagents are no longer running; an open edit
                // belonged to it, and its draft comes back.
                self.threads.clear();
                if self
                    .editing
                    .as_ref()
                    .is_none_or(|edit| edit.sending.is_none())
                    && let Some(editing) = self.editing.take()
                {
                    self.restore_draft(editing.draft, cx);
                }
                self.catch_up_archived(cx);
                self.presentation_changed(cx);
            }
            ChatRuntimeEvent::Restored => {
                if self.remote_revision == 0
                    && let Some(draft) = self.rt(cx).saved_draft()
                {
                    self.restore_draft(draft, cx);
                }
                self.restore_remote_edit(cx);
                self.catch_up_archived(cx);
                self.presentation_changed(cx);
            }
            ChatRuntimeEvent::Reset => {
                self.edit_preview = None;
                if self.rt(cx).history_loading() {
                    let top = self.rows.scroller.read(cx).top_row();
                    self.remote_anchor = self.rows.rows.get(top).and_then(|r| {
                        let super::rows::Kind::Item(source) = r.kind else {
                            return None;
                        };
                        let epoch = match source {
                            super::rows::Source::Current(_) => self.synced.0.saturating_sub(1),
                            super::rows::Source::Archived { epoch, .. } => {
                                self.rows.archived.get(epoch)?.epoch
                            }
                        };
                        Some((epoch, self.rows.item(source, self.transcript(cx))?.id.0))
                    });
                }
                if self.editing.as_ref().is_none_or(|e| e.sending.is_none())
                    && let Some(editing) = self.editing.take()
                {
                    self.restore_draft(editing.draft, cx);
                }
                self.rows = Rows::new(cx)
                    .in_folder(self.cwd(cx))
                    .with_badges(super::attachments::badge_factory(cx.entity().downgrade()));
                self.threads.clear();
                self.queue.clear();
                self.presentation_changed(cx);
            }
            ChatRuntimeEvent::DraftReturned(draft) => {
                self.restore_draft(draft.clone(), cx);
                cx.notify();
            }
            ChatRuntimeEvent::DraftConsumed(revision) => {
                if self.remote_revision == *revision {
                    self.attachments.pending.clear();
                    self.code_parts.clear();
                    self.shell = false;
                    self.goal_mode = false;
                    self.decision.composer = Some(super::decision::ComposerChange {
                        text: Some(String::new().into()),
                        placeholder: super::decision::COMPOSER_PLACEHOLDER,
                    });
                    cx.notify();
                }
            }
            ChatRuntimeEvent::EditFinished { revision, result } => {
                match result {
                    Ok(()) => {
                        self.editing = None;
                        if self.remote_revision == *revision {
                            self.attachments.pending.clear();
                            self.decision.composer = Some(super::decision::ComposerChange {
                                text: Some(String::new().into()),
                                placeholder: super::decision::COMPOSER_PLACEHOLDER,
                            });
                        }
                    }
                    Err(error) => {
                        if let Some(editing) = &mut self.editing {
                            editing.error = Some(error.clone());
                            editing.sending = None;
                        }
                    }
                }
                cx.notify();
            }
            ChatRuntimeEvent::Stopping => self.drop_live_pages(),
        }
    }

    /// Brings the rows up to session `serial`: `changes` name updated items, and every
    /// item past those already shown is inserted. Repeated or late reports are
    /// harmless: an older session's items are read from where it was put away.
    fn runtime_changed(
        &mut self,
        serial: u64,
        changes: &[Change],
        policy: MotionPolicy,
        cx: &mut Context<Self>,
    ) {
        if serial < self.synced.0 {
            return;
        }
        if serial > self.synced.0 {
            self.synced = (serial, 0);
        }
        let runtime = self.runtime.clone();
        let synced = self.synced.1;
        let checked = &self.checked_tools;
        let rows = &mut self.rows;
        let threads = &mut self.threads;
        let queue = &mut self.queue;
        let read = runtime.update(cx, |runtime, cx| {
            let transcript = runtime.transcript_of(serial)?;
            let len = transcript.items().len();
            let mut fresh: Vec<Change> = changes
                .iter()
                .filter(|change| matches!(change, Change::Updated(index) if *index < synced))
                .copied()
                .collect();
            fresh.dedup();
            fresh.extend((synced..len).map(Change::Inserted));
            let current = runtime.serial() == serial && runtime.ready();
            let thinking = current
                && runtime
                    .status()
                    .is_some_and(|status| presentation::thinking(transcript, status));
            let files = file_tools(transcript, &fresh, checked);
            rows.sync(transcript, &fresh, thinking, policy, cx);
            let images = images_in(fresh.iter().filter_map(|change| match *change {
                Change::Inserted(index) | Change::Updated(index) => transcript.get(index),
            }));
            if current {
                for thread in threads.iter_mut().filter(|thread| !thread.archived) {
                    let thinking = thread_thinking(transcript, &thread.call);
                    thread.rows.sync(transcript, &fresh, thinking, policy, cx);
                }
                let waiting = if runtime.is_working() {
                    vec![]
                } else {
                    waiting_subagents(transcript, rows, runtime.cwd().as_deref())
                };
                queue.sync(&waiting, &runtime.queue_lines(), policy);
            }
            Some((len, images, files))
        });
        if let Some((len, images, files)) = read {
            self.synced.1 = len;
            self.load_previews(images, cx);
            self.resolve_tool_files(files, cx);
        }
        self.presentation_changed(cx);
        self.restore_remote_anchor(cx);
    }

    /// Rows for sessions the runtime put away since the rows last looked: the one they
    /// showed is frozen in place, saved ones are added whole.
    fn catch_up_archived(&mut self, cx: &mut Context<Self>) {
        let shown = self.synced.0;
        let archived = self.rt(cx).archived();
        let pending = archived[self.rows.archived.len().min(archived.len())..].to_vec();
        for archived in pending {
            if archived.serial() == Some(shown) {
                // The rows showed it running: the rest of it first, then freeze.
                self.runtime_changed(shown, &[], motion::policy(cx), cx);
                self.rows.archive(archived, cx);
            } else {
                self.load_previews(images_in(archived.transcript.items().iter()), cx);
                self.rows.restore(archived, cx);
            }
        }
    }

    /// Presentation that follows the runtime's state as a whole.
    fn presentation_changed(&mut self, cx: &mut Context<Self>) {
        let policy = motion::policy(cx);
        self.sync_decision(policy, cx);
        self.sync_activity(cx);
        self.tick(cx);
        let working = self.is_working(cx);
        self.action.retarget(
            if working { 1. } else { 0. },
            TransitionSpec::new(motion::duration::FAST, motion::Easing::EaseInOut),
            Instant::now(),
            policy,
        );
        self.refresh_find(cx);
        cx.notify();
    }

    /// Checks off the UI thread which tool paths are files Canopy can open.
    fn resolve_tool_files(&mut self, files: Vec<(String, String)>, cx: &mut Context<Self>) {
        if files.is_empty() {
            return;
        }
        self.checked_tools
            .extend(files.iter().map(|(call, _)| call.clone()));
        let resolve = self
            .runtime
            .update(cx, |runtime, cx| runtime.resolve_tool_paths(files, cx));
        cx.spawn(async move |this, cx| {
            let found = resolve.await;
            let _ = this.update(cx, |this, cx| {
                if !found.is_empty() {
                    this.tool_files.extend(found);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // Intents.

    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.goal_mode() {
            self.submit_goal(window, cx);
            return;
        }
        if self.shell_mode(cx) {
            let text = self.composer.read(cx).value().to_string();
            self.save_remote_draft(cx);
            let revision = self.remote_revision;
            match self.runtime.update(cx, |runtime, cx| {
                runtime.submit_composer_shell(text, revision, cx)
            }) {
                // One command per `!`, as in Claude Code.
                Ok(()) => {}
                Err(error) => self.shell_error = Some(error),
            }
            cx.notify();
            return;
        }
        let draft = self.composer_draft(cx);
        if self.attachments_blocked(cx)
            || (draft.text.trim().is_empty() && draft.attachments.is_empty())
            || !self.rt(cx).accepts_messages()
        {
            return;
        }
        self.save_remote_draft(cx);
        let revision = self.remote_revision;
        self.activity_sent();
        self.runtime.update(cx, |runtime, cx| {
            runtime.submit_composer_draft(draft, false, revision, cx)
        });
    }

    pub(super) fn send_draft(&mut self, draft: Draft, cx: &mut Context<Self>) {
        if !self.rt(cx).accepts_messages() {
            return;
        }
        self.activity_sent();
        self.runtime
            .update(cx, |runtime, cx| runtime.send_draft(draft, cx));
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
        let runtime = self.rt(cx);
        let epoch = epoch.unwrap_or(runtime.epoch());
        let chat = runtime.pane_ref().id;
        let target = canopy_desktop::chat::tools::TurnDiffTarget {
            chat,
            session: epoch,
            turn,
            file,
        };
        let Ok(resource) = serde_json::to_string(&target) else {
            return;
        };
        let cwd = runtime.cwd();
        let name = runtime
            .pane_ref()
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| super::NEW_CHAT_TITLE.into());
        let workspace = cx.global::<crate::app_state::AppState>().workspace.clone();
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
        let runtime = self.runtime.clone();
        let cwd = self.cwd(cx);
        let opened = runtime.update(cx, |runtime, cx| {
            let transcript = match (&earlier, runtime.transcript()) {
                (Some(archived), _) => &archived.transcript,
                (None, Some(transcript)) => transcript,
                (None, None) => return None,
            };
            let title = transcript
                .items()
                .iter()
                .find_map(|item| match &item.body {
                    Body::Tool(tool) if tool.call_id == call => {
                        Some(presentation::subagent_title(tool))
                    }
                    _ => None,
                })?;
            let rows = match &earlier {
                Some(archived) => Rows::archived_thread(&call, archived.clone(), cwd, cx),
                None => {
                    let thinking = thread_thinking(transcript, &call);
                    Rows::thread(&call, transcript, thinking, cwd, cx)
                }
            };
            Some((title, rows))
        });
        let Some((title, rows)) = opened else {
            return;
        };
        self.threads.push(Thread {
            call,
            title,
            rows,
            archived: archived.is_some(),
        });
        self.restart_drill(cx);
        self.refresh_find(cx);
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
        self.refresh_find(cx);
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
            // Steering is the agent's own feature; without it the message waits.
            if self.is_working(cx) && self.capabilities(cx).steer {
                self.save_remote_draft(cx);
                let revision = self.remote_revision;
                self.runtime.update(cx, |runtime, cx| {
                    runtime.submit_composer_draft(draft, true, revision, cx)
                });
            } else {
                self.submit(window, cx);
            }
            return;
        }
        let oldest = self.runtime.read(cx).first_queued();
        if let Some(id) = oldest.filter(|_| self.capabilities(cx).steer) {
            self.steer(id, cx);
        }
    }

    pub(super) fn resume_queue(&mut self, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.resume_queue(cx));
    }

    pub(super) fn steer(&mut self, id: String, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.steer_queued(id, cx));
    }

    /// Moves a queued message back into the message field; a draft there takes its
    /// place at the end of the queue, so nothing typed is lost.
    pub(super) fn edit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(cx);
        let revision = self.remote_revision;
        let taken = self
            .runtime
            .update(cx, |runtime, cx| runtime.take_queued(id, draft, cx));
        // The composer is cleared now and refilled once the queue gave the message up.
        cx.spawn_in(window, async move |this, cx| {
            if let Some(mut queued) = taken.await {
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.remote_revision != revision {
                        let mut current = this.composer_draft(cx);
                        if !current.text.is_empty() && !queued.text.is_empty() {
                            current.text.push_str("\n\n");
                        }
                        current.text.push_str(&queued.text);
                        current.attachments.append(&mut queued.attachments);
                        if let Some(context) = queued.context {
                            current.context = Some(match current.context {
                                Some(current) => format!("{current}\n\n{context}"),
                                None => context,
                            });
                        }
                        current.goal |= queued.goal;
                        queued = current;
                        this.shell_error = Some(
                            "The draft changed; the queued content was appended for review.".into(),
                        );
                    }
                    this.set_composer_draft(queued, window, cx)
                });
            }
        })
        .detach();
    }

    /// Drag and drop in the queue: `id` goes in front of `before`.
    pub(super) fn move_queued(&mut self, id: String, before: String, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.move_queued(id, before, cx));
    }

    pub(super) fn remove(&mut self, id: String, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.remove_queued(id, cx));
    }

    /// The user's choice for a message whose delivery is uncertain.
    pub(super) fn resolve_unknown(
        &mut self,
        id: String,
        resolution: super::queue::Resolution,
        cx: &mut Context<Self>,
    ) {
        use canopy_desktop::host::UnknownResolution;
        let resolution = match resolution {
            super::queue::Resolution::Delivered => UnknownResolution::Delivered,
            super::queue::Resolution::Again => UnknownResolution::Resend {
                message: uuid::Uuid::new_v4().to_string(),
            },
            super::queue::Resolution::Dismiss => UnknownResolution::Dismissed,
        };
        self.runtime.update(cx, |runtime, cx| {
            runtime.resolve_unknown(id, resolution, cx)
        });
    }

    pub(super) fn interrupt(&mut self, cx: &mut Context<Self>) {
        self.runtime.update(cx, |runtime, cx| runtime.interrupt(cx));
    }

    pub(super) fn restart(&mut self, cx: &mut Context<Self>) {
        self.runtime.update(cx, |runtime, cx| runtime.restart(cx));
    }

    /// Keeps a retry or limit countdown current: a redraw each second while seconds
    /// show, every half minute while minutes do; nothing once no countdown runs.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.countdown_left(cx).is_none() {
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
                        let left = this.countdown_left(cx);
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
    fn countdown_left(&self, cx: &App) -> Option<u64> {
        self.transcript(cx)?
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

    pub(super) fn remove_rule(&mut self, rule: &str, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.remove_rule(rule, cx));
    }
    pub(super) fn set_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.set_model(model, cx));
    }
    pub(super) fn set_effort(
        &mut self,
        effort: Option<canopy_desktop::chat::config::Effort>,
        cx: &mut Context<Self>,
    ) {
        self.runtime
            .update(cx, |runtime, cx| runtime.set_effort(effort, cx));
    }
    pub(super) fn set_mode(
        &mut self,
        mode: canopy_desktop::chat::config::Mode,
        cx: &mut Context<Self>,
    ) {
        self.runtime
            .update(cx, |runtime, cx| runtime.set_mode(mode, cx));
    }
    /// Switches the account before the first message (DEC-087).
    pub(super) fn set_account(
        &mut self,
        tool: String,
        profile: String,
        model: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.runtime.update(cx, |runtime, cx| {
            runtime.set_account(tool, profile, model, cx)
        });
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
    use canopy_desktop::chat::{
        backend::ToolKind,
        transcript::{Change, ToolState},
    };
    changes
        .iter()
        .filter_map(|change| {
            let (Change::Inserted(index) | Change::Updated(index)) = *change;
            match &transcript.get(index)?.body {
                Body::Tool(tool)
                    if tool.state == ToolState::Succeeded && !checked.contains(&tool.call_id) =>
                {
                    if !matches!(tool.semantic_kind(), ToolKind::Read | ToolKind::Edit) {
                        return None;
                    }
                    let path = tool.file_path()?.to_owned();
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

fn thread_thinking(transcript: &canopy_desktop::chat::transcript::Transcript, call: &str) -> bool {
    transcript.items().iter().any(|item| {
        matches!(&item.body, Body::Tool(tool)
            if tool.call_id == call && presentation::thread_thinking(transcript, call, tool))
    })
}
