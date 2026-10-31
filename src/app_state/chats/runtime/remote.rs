//! What the remote bridge reads from and asks of one chat runtime. Reads are plain
//! projections (no inbox); each mutation is its own named handler with a typed
//! command, the caller's command identity and the durable preflight of §8: accepted
//! (authorized, deduplicated) before anything, `attempting` before the effect, then a
//! final state. A retried command returns its recorded state instead of acting again.
//! The local desktop calls the same runtime operations without a remote identity.
use super::{AgentOption, ChatRuntime, ChatStateChanged};
use crate::app_state::AppState;
use canopy_desktop::chat::session::{Draft, Rewind};
use canopy_desktop::{
    chat::{
        agent::Capabilities,
        backend_session::{BackendDecision, BackendSession},
        session::Response,
        transcript::{Body, ItemId, PermissionKind},
    },
    host::{ChatQueue, CommandInput, CommandKey, CommandReceipt, CommandState, QueueDraft},
    settings::SettingsClient,
};
use futures_channel::oneshot;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Scope bits checked for chat commands (§12): `chat`, and `approve` for decisions.
const CHAT: u32 = canopy_desktop::host::CHAT;
const APPROVE: u32 = canopy_desktop::host::APPROVE;
/// Composer shell commands run on the host like a terminal: both scopes.
const SHELL: u32 = canopy_desktop::host::CHAT | canopy_desktop::host::TERMINAL;
/// How long a file restore preview waits for the agent's dry-run answer.
const PREVIEW_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// One chat as a remote client reads it: no transcript (SYNC-05).
#[derive(Clone, Debug, PartialEq)]
pub struct ChatSnapshot {
    pub chat: String,
    pub tool: String,
    pub agent: String,
    pub profile: String,
    pub title: Option<String>,
    /// Run generation: decisions and late results of another run are refused.
    pub run: u64,
    /// The running session's number: `Items` events of this serial belong to it.
    pub serial: u64,
    pub history_epoch: u64,
    pub status: &'static str,
    pub working: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    /// Waiting for an agent and account choice before anything runs.
    pub recovery: Option<String>,
    pub account_locked: bool,
    pub capabilities: Capabilities,
    pub options: Vec<AgentOption>,
    pub decisions: Vec<PendingDecision>,
    pub queue: Option<ChatQueue>,
    pub observers: usize,
    /// The chat's goal and its accounting (native agents with the goal capability).
    pub goal: Option<canopy_desktop::chat::goal::Goal>,
    /// Why sent messages can't be edited now (`None`: they can).
    pub edit_unavailable: Option<String>,
    /// Why composer shell commands can't run now (`None`: they can).
    pub shell_unavailable: Option<String>,
}

/// What editing a sent message would do (the remote Edit's preview).
#[derive(Clone, Debug, PartialEq)]
pub struct EditPreview {
    /// `Ok(first)`: editable; `first`: the edit starts the conversation over (same
    /// chat). `Err`: why not, as `code: text`.
    pub editable: Result<bool, String>,
    /// The message as sent: text, context, goal and attachments (images by file).
    pub original: Option<Draft>,
    pub files: FilesPreview,
}

/// What restoring files to before the message would change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilesPreview {
    pub possible: bool,
    pub paths: Vec<String>,
    pub insertions: u64,
    pub deletions: u64,
    /// Why files can't be offered for restoring (when not `possible`).
    pub reason: Option<String>,
}

/// Remote commands waiting on what the session does next; each is answered from
/// `after_change` (or dropped with the runtime: the waiter sees it closed).
type PendingRewind = (ItemId, bool, oneshot::Sender<Result<Rewind, String>>);
#[derive(Default)]
pub(super) struct Waits {
    /// An edit's new branch: `Ok` once its message goes out, `Err` when it didn't start.
    edit: Option<oneshot::Sender<Result<(), String>>>,
    /// A file restore (or its dry run, `true`) of user message `ItemId`.
    rewind: Option<PendingRewind>,
}

/// One part of the conversation as the runtime holds it now: sessions put away
/// (Restart, Edit, restored history) and the running or restored one.
#[derive(Clone, Debug)]
#[allow(dead_code, reason = "read by the remote server")]
pub struct SnapshotEpoch {
    /// The runtime session number (`Items` events name it); `None`: read from saved
    /// history and not run in this runtime yet.
    pub serial: Option<u64>,
    pub history_epoch: u64,
    pub items: Vec<canopy_desktop::chat::transcript::Item>,
}

/// A request waiting on the user, bound to the run that asked it (CHAT-05).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDecision {
    pub run: u64,
    pub item: u64,
    pub kind: PermissionKind,
    pub tool: String,
    pub title: Option<String>,
    /// "Allow for this chat" is offered (Claude and Codex rules, CHAT-06).
    pub remember: bool,
    /// An ACP agent's own choices, IDs untouched (AGT-11).
    pub options: Vec<(String, String, String)>,
}

/// An answer to a pending decision, as a remote client sends it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DecisionAnswer {
    Allow,
    AllowForChat,
    Deny(Option<String>),
    Answer(Vec<String>),
    ApprovePlan {
        accept_edits: bool,
    },
    KeepPlanning(Option<String>),
    /// An ACP option ID (`None` cancels the request).
    Option(Option<String>),
}
impl DecisionAnswer {
    fn decision(self) -> BackendDecision {
        BackendDecision::Native(match self {
            Self::Allow => Response::Allow,
            Self::AllowForChat => Response::AllowForChat,
            Self::Deny(text) => Response::Deny(text),
            Self::Answer(answers) => Response::Answer(answers),
            Self::ApprovePlan { accept_edits } => Response::ApprovePlan { accept_edits },
            Self::KeepPlanning(text) => Response::KeepPlanning(text),
            Self::Option(option) => return BackendDecision::OptionId(option),
        })
    }
}

/// The chat commands a remote client sends, with their typed arguments. Each has its
/// own handler; this type only fixes their inbox method and fingerprint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChatCommand {
    Start,
    /// Interrupt and Restart name the run they were asked for (`ChatSnapshot.run`):
    /// another run by then refuses them with `stale_run`.
    Interrupt {
        run: u64,
    },
    Restart {
        run: u64,
    },
    /// Steer an accepted, queued message (its ID) into the running turn.
    Steer {
        message: String,
        run: u64,
    },
    Respond {
        run: u64,
        item: u64,
        answer: DecisionAnswer,
    },
    SetOption {
        id: String,
        value: String,
    },
    SetAgent {
        tool: String,
        profile: String,
        model: Option<String>,
    },
    /// The chat's goal (set by sending a goal message).
    PauseGoal,
    ResumeGoal,
    ClearGoal,
    /// Stops background work `call` (a background shell or subagent).
    StopBackground {
        run: u64,
        call: String,
    },
    /// Edits sent user message `item` of history part `history_epoch` in run `run`
    /// (CHAT-08): files restored first when asked, then the same chat continues from
    /// before it with `draft` as the new branch's first message.
    Edit {
        run: u64,
        history_epoch: u64,
        item: u64,
        draft: QueueDraft,
        restore_files: bool,
    },
    /// Runs a composer shell command (`!`) in run `run` (TERM-09).
    RunShell {
        run: u64,
        command: String,
    },
    /// Stops the running composer shell command of card `item` in run `run`.
    StopShell {
        run: u64,
        history_epoch: u64,
        item: u64,
    },
}
impl ChatCommand {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Start => "chat.start",
            Self::Interrupt { .. } => "chat.interrupt",
            Self::Restart { .. } => "chat.restart",
            Self::Steer { .. } => "chat.steer",
            Self::Respond { .. } => "chat.respond",
            Self::SetOption { .. } => "chat.set_option",
            Self::SetAgent { .. } => "chat.set_agent",
            Self::PauseGoal => "chat.pause_goal",
            Self::ResumeGoal => "chat.resume_goal",
            Self::ClearGoal => "chat.clear_goal",
            Self::StopBackground { .. } => "chat.stop_background",
            Self::Edit { .. } => "chat.edit",
            Self::RunShell { .. } => "chat.shell",
            Self::StopShell { .. } => "chat.stop_shell",
        }
    }
    pub fn scope(&self) -> u32 {
        match self {
            Self::Respond { .. } => APPROVE,
            Self::RunShell { .. } | Self::StopShell { .. } => SHELL,
            _ => CHAT,
        }
    }
    /// The inbox record of this command for chat `chat` under `key`.
    pub fn command(&self, key: CommandKey, chat: &str) -> Option<CommandInput> {
        uuid::Uuid::parse_str(chat).ok()?;
        let arguments = serde_json::to_vec(self).ok()?;
        let fingerprint = Sha256::digest(&arguments).to_vec();
        CommandInput::new(
            key,
            self.method().into(),
            chat.into(),
            arguments,
            fingerprint,
        )
    }
}

/// A command's identity as a handler receives it. A remote command carries the
/// fingerprint of its decoded wire request: a retry under the same ID matches it
/// regardless of how its targets resolve now, and changed arguments conflict.
#[derive(Clone, Debug)]
pub struct RemoteKey {
    pub key: CommandKey,
    pub fingerprint: Option<Vec<u8>>,
}
impl From<CommandKey> for RemoteKey {
    fn from(key: CommandKey) -> Self {
        Self {
            key,
            fingerprint: None,
        }
    }
}

/// How a command's effect ended.
enum Outcome {
    Succeeded,
    /// Succeeded with a result for the receipt (a shell card's key).
    SucceededWith(Vec<u8>),
    /// Concrete evidence nothing (or only what is said) happened.
    Failed(String),
    /// Part of it may have reached the agent.
    Unknown(String),
}

fn reason(text: &str) -> Option<Vec<u8>> {
    serde_json::to_vec(&serde_json::json!({ "reason": text })).ok()
}

/// The parts of a chat's remote state that change without its status: a change
/// of any of them is a new state for subscribed clients (goal accounting included).
#[derive(Clone, Debug, PartialEq)]
pub struct StateKey {
    goal: Option<canopy_desktop::chat::goal::Goal>,
    error: Option<String>,
    notice: Option<super::ChatNotice>,
    recovery: Option<String>,
    working: bool,
    options: Vec<AgentOption>,
    decisions: Vec<ItemId>,
    shells: usize,
}

impl ChatRuntime {
    // Reads.

    pub(super) fn state_key(&self) -> StateKey {
        StateKey {
            goal: self.native().and_then(|session| session.goal()).cloned(),
            error: self.error.clone(),
            notice: self.notice.clone(),
            recovery: self.recovery.clone(),
            working: self.is_working(),
            options: self.remote_options(),
            decisions: self.pending_decisions(),
            shells: self.shells.len(),
        }
    }

    fn remote_options(&self) -> Vec<AgentOption> {
        match &self.session {
            Some(BackendSession::Native(_)) => self
                .backend()
                .map(|session| session.options().iter().map(AgentOption::from).collect())
                .unwrap_or_default(),
            _ => self.agent_options(),
        }
    }

    pub fn snapshot(&self, cx: &App) -> ChatSnapshot {
        let options = self.remote_options();
        ChatSnapshot {
            chat: self.chat_id().unwrap_or_default(),
            tool: self.pane.tool.clone(),
            agent: self.agent_name(cx),
            profile: self.profile(cx),
            title: self.pane.metadata.title.clone(),
            run: self.generation,
            serial: self.serial,
            history_epoch: self.epoch,
            status: self.navigation_status(),
            working: self.is_working(),
            error: self.error.clone(),
            notice: self.notice.as_ref().map(|notice| notice.text.clone()),
            recovery: self.recovery.clone(),
            account_locked: self.account_locked(),
            capabilities: self.capabilities(cx),
            options,
            decisions: self.decisions(),
            queue: self.host_queue().cloned(),
            observers: self.observers,
            goal: self.native().and_then(|session| session.goal()).cloned(),
            edit_unavailable: self.edit_unavailable(cx),
            shell_unavailable: self.shell_unavailable(cx),
        }
    }

    // Edit and shell availability.

    /// Why sent messages can't be edited at all now (per message: `edit_target`).
    fn edit_unavailable(&self, cx: &App) -> Option<String> {
        let name = self.agent_name(cx);
        match &self.session {
            Some(BackendSession::Native(session)) if !session.status().ended() => {
                (!self.capabilities(cx).fork)
                    .then(|| format!("{name} can't continue a conversation from a message."))
            }
            Some(BackendSession::Native(_)) => Some("The agent isn't running.".into()),
            Some(BackendSession::Acp(_)) => Some(format!("{name} can't edit sent messages.")),
            None => match self.backend_kind(cx) {
                Some(super::BackendKind::Acp) | None => {
                    Some(format!("{name} can't edit sent messages."))
                }
                Some(_) => Some("Start the chat to edit its messages.".into()),
            },
        }
    }

    /// Why a composer shell command can't run now (the checks of `run_shell`).
    pub(super) fn shell_unavailable(&self, cx: &App) -> Option<String> {
        let name = self.agent_name(cx);
        if matches!(self.session, Some(BackendSession::Acp(_)))
            || self.session.is_none()
                && matches!(self.backend_kind(cx), Some(super::BackendKind::Acp) | None)
        {
            return Some(format!("{name} doesn't take `!` command results."));
        }
        if self.native().is_none_or(|session| session.status().ended()) {
            return Some("The agent isn't running.".into());
        }
        if self.cwd().is_none() {
            return Some("This chat has no folder to run commands in.".into());
        }
        match &cx.global::<AppState>().tools.read(cx).environment {
            None => Some("The shell environment is still loading.".into()),
            Some(Err(error)) => Some(error.clone()),
            Some(Ok(_)) => None,
        }
    }

    /// Sent user message `item` of history part `history_epoch` in run `run` as it can
    /// be edited now: the point the new branch continues after (`None`: the
    /// conversation starts over) and the message as sent. Errors are `code: text`.
    fn edit_target(
        &self,
        run: u64,
        history_epoch: u64,
        item: u64,
        cx: &App,
    ) -> Result<(Option<String>, Draft), String> {
        if run != self.generation {
            return Err("stale_run".into());
        }
        if let Some(reason) = self.edit_unavailable(cx) {
            return Err(format!("not_editable: {reason}"));
        }
        let session = self
            .native()
            .ok_or("not_editable: The agent isn't running.")?;
        if history_epoch != self.epoch {
            return Err(
                "not_editable: Only messages since the last restart or edit can be \
                 edited; fork from an earlier one instead."
                    .into(),
            );
        }
        if self.recovery.is_some()
            || self.switching
            || self.starting
            || self.is_working()
            || !self.pending_decisions().is_empty()
            || !self.queue_lines().is_empty()
            || self.send_on_start.is_some()
            || self.edit_recovery.is_some()
            || self.remote_waits.edit.is_some()
        {
            return Err(
                "conditions_changed: Wait until the agent is idle and nothing is queued.".into(),
            );
        }
        let fork = session.fork_point(ItemId(item)).ok_or(
            "not_editable: This message can't be edited: it isn't sent, or it continues \
             history this session doesn't hold.",
        )?;
        if fork.is_some() && self.pane.metadata.resume_id.is_none() {
            return Err("not_editable: This conversation can't be continued from there.".into());
        }
        let original = session
            .transcript()
            .items()
            .iter()
            .find_map(|i| match &i.body {
                Body::User(message) if i.id == ItemId(item) => Some(Draft {
                    text: message.text.clone(),
                    attachments: message.attachments.clone(),
                    context: message.context.clone(),
                    goal: message.goal,
                }),
                _ => None,
            })
            .ok_or("not_editable: This message is no longer in the conversation.")?;
        Ok((fork, original))
    }

    /// Answers the remote commands waiting on the session (from `after_change`).
    pub(super) fn settle_remote_waits(&mut self) {
        let Some((item, dry_run, _)) = &self.remote_waits.rewind else {
            return;
        };
        let (item, dry_run) = (*item, *dry_run);
        let ended = self
            .backend()
            .is_none_or(|session| session.status().ended());
        let outcome = self
            .native()
            .and_then(|session| session.rewind())
            .filter(|(at, outcome)| {
                *at == item
                    && match outcome {
                        Rewind::Preview(_) | Rewind::Unavailable(_) => dry_run,
                        Rewind::Done | Rewind::Failed(_) => !dry_run,
                    }
            })
            .map(|(_, outcome)| outcome.clone());
        let answer = match (outcome, ended) {
            (Some(outcome), _) => Ok(outcome),
            (None, true) => Err("The chat ended before the agent answered.".to_owned()),
            (None, false) => return,
        };
        if let Some((_, _, reply)) = self.remote_waits.rewind.take() {
            let _ = reply.send(answer);
        }
    }

    /// The remote edit waiting for its new branch learns how it went.
    pub(super) fn settle_edit(&mut self, result: Result<(), String>) {
        if let Some(reply) = self.remote_waits.edit.take() {
            let _ = reply.send(result);
        }
    }

    /// What editing message `item` would do: whether it can be edited, the message as
    /// sent, and (agents with file checkpoints) what restoring files would change,
    /// asked of the agent as a dry run and waited for up to `PREVIEW_WAIT`.
    pub fn remote_edit_preview(
        &mut self,
        run: u64,
        history_epoch: u64,
        item: u64,
        cx: &mut Context<Self>,
    ) -> Task<EditPreview> {
        let (fork, original) = match self.edit_target(run, history_epoch, item, cx) {
            Ok(target) => target,
            Err(reason) => {
                return Task::ready(EditPreview {
                    editable: Err(reason),
                    original: None,
                    files: FilesPreview::default(),
                });
            }
        };
        let preview = EditPreview {
            editable: Ok(fork.is_none()),
            original: Some(original),
            files: FilesPreview::default(),
        };
        let unavailable = |preview: EditPreview, reason: String| {
            Task::ready(EditPreview {
                files: FilesPreview {
                    reason: Some(reason),
                    ..Default::default()
                },
                ..preview
            })
        };
        if !self.capabilities(cx).rewind {
            let name = self.agent_name(cx);
            return unavailable(preview, format!("{name} keeps no file checkpoints."));
        }
        if self.remote_waits.rewind.is_some() {
            return unavailable(preview, "Another file restore is under way.".into());
        }
        if !self.rewind_files(ItemId(item), true, cx) {
            return unavailable(preview, "The agent can't check files now.".into());
        }
        let (reply, answer) = oneshot::channel();
        self.remote_waits.rewind = Some((ItemId(item), true, reply));
        let timer = cx.background_executor().timer(PREVIEW_WAIT);
        cx.spawn(async move |this, cx| {
            let answer = futures_lite::future::or(async { answer.await.ok() }, async {
                timer.await;
                None
            })
            .await;
            let files = match answer {
                Some(Ok(Rewind::Preview(p))) => FilesPreview {
                    possible: p.possible,
                    paths: p.files,
                    insertions: p.insertions,
                    deletions: p.deletions,
                    reason: (!p.possible).then(|| "Files can't be restored here.".into()),
                },
                Some(Ok(Rewind::Unavailable(reason))) | Some(Err(reason)) => FilesPreview {
                    reason: Some(reason),
                    ..Default::default()
                },
                Some(Ok(_)) | None => {
                    // No answer in time: the dry run's waiter goes, the edit stays.
                    let _ = this.update(cx, |this, _| {
                        if this
                            .remote_waits
                            .rewind
                            .as_ref()
                            .is_some_and(|(at, dry, _)| *at == ItemId(item) && *dry)
                        {
                            this.remote_waits.rewind = None;
                        }
                    });
                    FilesPreview {
                        reason: Some("The agent didn't say in time which files changed.".into()),
                        ..Default::default()
                    }
                }
            };
            EditPreview { files, ..preview }
        })
    }

    /// The conversation held now, oldest part first, with only the newest `limit`
    /// items in total copied (older ones stay in the runtime and saved history);
    /// `true` when some were left out. With `snapshot`, the starting point of a
    /// subscription: later `Items` events apply on top of it.
    /// Every part is listed, also when none of its items fit, so a later `Items`
    /// event of its serial still finds its history epoch.
    pub fn epochs_tail(&self, limit: usize) -> (Vec<SnapshotEpoch>, bool) {
        let mut parts: Vec<(Option<u64>, u64, &[canopy_desktop::chat::transcript::Item])> = self
            .archived
            .iter()
            .map(|archived| {
                (
                    archived.serial(),
                    archived.epoch,
                    archived.transcript.items(),
                )
            })
            .collect();
        match (&self.session, &self.restored) {
            (Some(session), _) => {
                parts.push((Some(self.serial), self.epoch, session.transcript().items()))
            }
            (None, Some(restored)) => parts.push((None, restored.index, &restored.items)),
            (None, None) => {}
        }
        let mut left = limit;
        let mut earlier = false;
        let mut epochs: Vec<_> = parts
            .into_iter()
            .rev()
            .map(|(serial, history_epoch, items)| {
                let take = items.len().min(left);
                left -= take;
                earlier |= take < items.len();
                SnapshotEpoch {
                    serial,
                    history_epoch,
                    items: items[items.len() - take..].to_vec(),
                }
            })
            .collect();
        epochs.reverse();
        (epochs, earlier)
    }

    /// What forking at turn end `item` of history part `history_epoch` takes (the
    /// same point the chat's Fork offers); `None` when that part has no saved session
    /// to branch, or the item is no turn end with a branch point.
    pub fn fork_request(&self, history_epoch: u64, item: u64) -> Option<super::super::ForkRequest> {
        if self.history_off {
            return None;
        }
        let source = self.chat_id()?;
        let (transcript, agent) = if history_epoch == self.epoch && self.session.is_some() {
            (self.transcript()?, self.epoch_meta.agent.clone())
        } else {
            let archived = self
                .archived
                .iter()
                .rev()
                .find(|archived| archived.epoch == history_epoch)?;
            (&archived.transcript, archived.agent.clone())
        };
        let found = transcript.items().iter().find(|i| i.id == ItemId(item))?;
        let Body::TurnEnd(end) = &found.body else {
            return None;
        };
        let at = end.resume_at.clone()?;
        let agent = agent?;
        let session = agent.session.clone()?;
        let provider = canopy_desktop::chat::provider::Provider::from_tool(&agent.tool)?;
        let mut settings = agent.settings.unwrap_or_else(|| self.settings.clone());
        settings.id = None;
        Some(super::super::ForkRequest {
            source,
            cut: canopy_desktop::chat::fork::Cut {
                epoch: history_epoch,
                end: found.id,
                at,
            },
            provider,
            profile: agent.profile,
            settings,
            session,
            title: self
                .pane
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| crate::ui::chat::NEW_CHAT_TITLE.to_owned()),
        })
    }

    /// Requests waiting on the user, oldest first, bound to this run.
    pub fn decisions(&self) -> Vec<PendingDecision> {
        let Some(session) = &self.session else {
            return vec![];
        };
        self.pending_decisions()
            .into_iter()
            .filter_map(|item| {
                let permission =
                    session
                        .transcript()
                        .items()
                        .iter()
                        .find_map(|i| match &i.body {
                            Body::Permission(p) if i.id == item => Some(p),
                            _ => None,
                        })?;
                Some(PendingDecision {
                    run: self.generation,
                    item: item.0,
                    kind: permission.kind,
                    tool: permission.tool.clone(),
                    title: permission.title.clone(),
                    remember: permission.remember.is_some(),
                    options: self
                        .decision_options(item)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|o| (o.option_id, o.name, o.kind))
                        .collect(),
                })
            })
            .collect()
    }

    // Named command handlers.

    pub fn remote_start(
        &mut self,
        key: RemoteKey,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        self.durable(key, ChatCommand::Start, cx, |this, cx| {
            if this.process.is_some() || this.starting {
                return Outcome::Succeeded;
            }
            if let Some(full) = this.process_limit(cx) {
                return Outcome::Failed(full);
            }
            this.start(cx);
            match &this.error {
                Some(error) if !this.starting => Outcome::Failed(error.clone()),
                _ => Outcome::Succeeded,
            }
        })
    }

    pub fn remote_interrupt(
        &mut self,
        key: RemoteKey,
        run: u64,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        self.durable(key, ChatCommand::Interrupt { run }, cx, move |this, cx| {
            if run != this.generation || this.session.is_none() {
                return Outcome::Failed("stale_run".into());
            }
            match this.interrupt_checked(cx) {
                Ok(()) => Outcome::Succeeded,
                Err((reason, false)) => Outcome::Failed(reason),
                Err((reason, true)) => Outcome::Unknown(reason),
            }
        })
    }

    pub fn remote_restart(
        &mut self,
        key: RemoteKey,
        run: u64,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        self.durable(key, ChatCommand::Restart { run }, cx, move |this, cx| {
            if run != this.generation {
                return Outcome::Failed("stale_run".into());
            }
            if !this.can_restart() {
                return Outcome::Failed("The chat can't restart now.".into());
            }
            if let Some(full) = this.process_limit(cx) {
                return Outcome::Failed(full);
            }
            this.restart(cx);
            Outcome::Succeeded
        })
    }

    /// Steers accepted message `message`: it keeps its ID and receipts (CHAT-03).
    pub fn remote_steer(
        &mut self,
        key: RemoteKey,
        message: String,
        run: u64,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let command = ChatCommand::Steer {
            message: message.clone(),
            run,
        };
        self.durable(key, command, cx, move |this, cx| {
            if this.generation != run {
                return Outcome::Failed("stale_run: the run has changed.".into());
            }
            if this.entry_draft(&message).is_none() {
                return Outcome::Failed("The message is no longer waiting.".into());
            }
            if !this.capabilities(cx).steer {
                return Outcome::Failed(format!("{} can't be steered.", this.agent_name(cx)));
            }
            if this.queue_paused() {
                return Outcome::Failed("The queue is held. Resume it first.".into());
            }
            this.steer_queued(message, cx);
            Outcome::Succeeded
        })
    }

    /// The first valid answer wins; a resolved request or another run's is refused
    /// with `already_resolved` / `stale_run` (CHAT-05).
    pub fn remote_respond(
        &mut self,
        key: RemoteKey,
        run: u64,
        item: u64,
        answer: DecisionAnswer,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let command = ChatCommand::Respond {
            run,
            item,
            answer: answer.clone(),
        };
        self.durable(key, command, cx, move |this, cx| {
            if run != this.generation {
                return Outcome::Failed("stale_run".into());
            }
            if !this.pending_decisions().contains(&ItemId(item)) {
                return Outcome::Failed("already_resolved".into());
            }
            let Some(session) = &mut this.session else {
                return Outcome::Failed("stale_run".into());
            };
            let outcome = match session.respond(ItemId(item), answer.decision()) {
                Ok(lines) => match this.write_dispatched(lines) {
                    Ok(()) => Outcome::Succeeded,
                    Err((reason, false)) => Outcome::Failed(reason),
                    Err((reason, true)) => Outcome::Unknown(reason),
                },
                Err(error) => Outcome::Failed(error),
            };
            this.after_change(cx);
            outcome
        })
    }

    pub fn remote_set_agent(
        &mut self,
        key: RemoteKey,
        tool: String,
        profile: String,
        model: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let command = ChatCommand::SetAgent {
            tool: tool.clone(),
            profile: profile.clone(),
            model: model.clone(),
        };
        self.durable(key, command, cx, move |this, cx| {
            if this.account_locked() || this.busy() || this.unsent() > 0 {
                return Outcome::Failed(
                    "The account can only change before the first message.".into(),
                );
            }
            let catalog = &cx
                .global::<crate::app_state::AppState>()
                .tools
                .read(cx)
                .catalog;
            if !catalog.get(&tool).is_some_and(|t| {
                t.enabled
                    && (t.profiles.iter().any(|p| p.id == profile)
                        || (t.profiles.is_empty() && t.id == profile))
            }) {
                return Outcome::Failed("The selected agent account is unavailable.".into());
            }
            this.set_account(tool, profile, model, cx);
            Outcome::Succeeded
        })
    }

    /// Chooses an option value as the agent offered it; a refusal keeps the previous
    /// choice (AGT-05).
    pub fn remote_set_option(
        &mut self,
        key: RemoteKey,
        id: String,
        value: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let command = ChatCommand::SetOption {
            id: id.clone(),
            value: value.clone(),
        };
        self.durable(key, command, cx, move |this, cx| match &mut this.session {
            Some(BackendSession::Acp(_)) => {
                let before = this.notice.clone();
                this.notice = None;
                this.set_agent_option(&id, &value, cx);
                match this.notice.take() {
                    Some(error) => Outcome::Failed(error.text),
                    None => {
                        this.notice = before;
                        Outcome::Succeeded
                    }
                }
            }
            Some(BackendSession::Native(_)) => {
                let Some(session) = this.native_mut() else {
                    return Outcome::Failed("The agent isn't running.".into());
                };
                let applied = match id.as_str() {
                    "model" => Some(session.set_model(Some(value.clone()))),
                    "mode" => canopy_desktop::chat::config::Mode::from_key(&value)
                        .map(|mode| session.set_mode(mode)),
                    "effort" => canopy_desktop::chat::config::Effort::from_key(&value)
                        .map(|effort| session.set_effort(Some(effort))),
                    _ => None,
                };
                match applied {
                    None => Outcome::Failed("The session does not offer this option value.".into()),
                    Some(lines) => {
                        let settings = session.settings().clone();
                        let written = this.write_dispatched(lines);
                        if settings != this.settings {
                            this.settings = settings;
                            this.persist(None, cx);
                        }
                        this.after_change(cx);
                        match written {
                            Ok(()) => Outcome::Succeeded,
                            Err((reason, false)) => Outcome::Failed(reason),
                            Err((reason, true)) => Outcome::Unknown(reason),
                        }
                    }
                }
            }
            None => Outcome::Failed("The agent isn't running.".into()),
        })
    }

    /// Pause, resume or clear the goal: Claude and Codex only (AGT-07: never emulated).
    pub fn remote_goal(
        &mut self,
        key: RemoteKey,
        command: ChatCommand,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let change = command.clone();
        self.durable(key, command, cx, move |this, cx| {
            if !this.capabilities(cx).goal {
                return Outcome::Failed(format!("{} has no goals.", this.agent_name(cx)));
            }
            let Some(session) = this.native() else {
                return Outcome::Failed("The agent isn't running.".into());
            };
            if session.goal().is_none() {
                return Outcome::Failed("This chat has no goal.".into());
            }
            match change {
                ChatCommand::PauseGoal => this.pause_goal(cx),
                ChatCommand::ResumeGoal => this.resume_goal(cx),
                ChatCommand::ClearGoal => this.clear_goal(cx),
                _ => return Outcome::Failed("Not a goal command.".into()),
            }
            Outcome::Succeeded
        })
    }

    /// Stops background work `call` of the running turn.
    pub fn remote_stop_background(
        &mut self,
        key: RemoteKey,
        run: u64,
        call: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let command = ChatCommand::StopBackground {
            run,
            call: call.clone(),
        };
        self.durable(key, command, cx, move |this, cx| {
            if run != this.generation {
                return Outcome::Failed("stale_run".into());
            }
            if !this.native().is_some_and(|s| s.can_stop_background(&call)) {
                return Outcome::Failed("That background work is no longer running.".into());
            }
            this.stop_background(&call, cx);
            Outcome::Succeeded
        })
    }

    /// Edits sent user message `item` (CHAT-08): the same chat continues from before
    /// it with `draft`, the earlier continuation kept above an "edited" boundary.
    /// Accepted with the draft's images retained, `attempting` before anything
    /// changes; files are restored first when asked (agents with checkpoints), and
    /// nothing is edited when that fails. Succeeds once the new branch started and
    /// its first message goes out; never sent into another run.
    pub fn remote_edit(
        &mut self,
        key: RemoteKey,
        edit: ChatCommand,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let ChatCommand::Edit {
            run,
            history_epoch,
            item,
            draft,
            restore_files,
        } = edit.clone()
        else {
            return Task::ready(Err("Not an edit.".into()));
        };
        let artifacts = draft.artifact_ids().to_vec();
        self.durable_async(
            key,
            edit,
            Accept::Retaining(artifacts),
            cx,
            move |this, cx| {
                if let Err(reason) = this.edit_target(run, history_epoch, item, cx) {
                    return Task::ready(Outcome::Failed(reason));
                }
                if restore_files && !this.capabilities(cx).rewind {
                    let name = this.agent_name(cx);
                    return Task::ready(Outcome::Failed(format!(
                        "files_not_restored: {name} keeps no file checkpoints."
                    )));
                }
                if this.remote_waits.rewind.is_some() {
                    return Task::ready(Outcome::Failed(
                        "conditions_changed: Another file restore is under way.".into(),
                    ));
                }
                let store = this.artifacts(cx);
                let chat = this.chat_id();
                cx.spawn(async move |this, cx| {
                    // Remote images become attachments of this chat before anything changes.
                    let loaded = cx
                        .background_spawn(async move {
                            super::queue::materialize(chat.as_deref(), draft, store)
                        })
                        .await;
                    let mut draft = match loaded {
                        Ok(draft) => draft,
                        Err((_, error)) => {
                            return Outcome::Failed(format!("not_editable: {error}"));
                        }
                    };
                    let restored = if restore_files {
                        let asked = this.update(cx, |this, cx| {
                            this.edit_target(run, history_epoch, item, cx)?;
                            if this.remote_waits.rewind.is_some()
                                || !this.rewind_files(ItemId(item), false, cx)
                            {
                                return Err(
                                    "files_not_restored: The agent can't restore files now."
                                        .to_owned(),
                                );
                            }
                            let (reply, answer) = oneshot::channel();
                            this.remote_waits.rewind = Some((ItemId(item), false, reply));
                            Ok(answer)
                        });
                        let answer = match asked {
                            Ok(Ok(answer)) => answer,
                            Ok(Err(reason)) => return Outcome::Failed(reason),
                            Err(_) => return Outcome::Failed("resource_deleted".into()),
                        };
                        match answer.await {
                            Ok(Ok(Rewind::Done)) => true,
                            // The agent says it restored nothing: nothing is edited.
                            Ok(Ok(Rewind::Failed(error))) => {
                                return Outcome::Failed(format!("files_not_restored: {error}"));
                            }
                            Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
                                return Outcome::Unknown(
                                    "The chat ended before the files were restored; some may be."
                                        .into(),
                                );
                            }
                        }
                    } else {
                        false
                    };
                    let after = if restored {
                        " The files were already restored."
                    } else {
                        ""
                    };
                    let started = this.update(cx, |this, cx| {
                        let (fork, original) = this
                            .edit_target(run, history_epoch, item, cx)
                            .map_err(|reason| format!("{reason}{after}"))?;
                        // The message's context (from an agent switch) goes again.
                        draft.context = original.context;
                        let (reply, answer) = oneshot::channel();
                        this.remote_waits.edit = Some(reply);
                        this.restart_edited(fork, draft, cx);
                        Ok::<_, String>(answer)
                    });
                    let answer = match started {
                        Ok(Ok(answer)) => answer,
                        Ok(Err(reason)) => return Outcome::Failed(reason),
                        Err(_) => return Outcome::Failed(format!("resource_deleted{after}")),
                    };
                    match answer.await {
                        Ok(Ok(())) => Outcome::Succeeded,
                        Ok(Err(reason)) => {
                            Outcome::Failed(format!("edit_not_started: {reason}{after}"))
                        }
                        // The chat closed during the restart: whether it went is unknown.
                        Err(_) => Outcome::Unknown(format!(
                            "The chat closed while the edit was starting.{after}"
                        )),
                    }
                })
            },
        )
    }

    /// Runs composer shell command `command` in run `run` (TERM-09): in the chat's
    /// folder and the host's environment, its card in the conversation and its result
    /// sent to the agent when it ends. The receipt names the card.
    pub fn remote_run_shell(
        &mut self,
        key: RemoteKey,
        run: u64,
        command: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let shell = ChatCommand::RunShell {
            run,
            command: command.clone(),
        };
        self.durable(key, shell, cx, move |this, cx| {
            if run != this.generation {
                return Outcome::Failed("stale_run".into());
            }
            if let Some(reason) = this.shell_unavailable(cx) {
                return Outcome::Failed(format!("shell_unavailable: {reason}"));
            }
            match this.run_shell(&command, cx) {
                Ok(id) => serde_json::to_vec(&serde_json::json!({
                    "history_epoch": this.epoch,
                    "item_id": id.0,
                }))
                .map(Outcome::SucceededWith)
                .unwrap_or(Outcome::Succeeded),
                Err(reason) => Outcome::Failed(format!("shell_unavailable: {reason}")),
            }
        })
    }

    /// Stop on running shell card `item` of run `run`: it ends, and the agent hears it
    /// was stopped.
    pub fn remote_stop_shell(
        &mut self,
        key: RemoteKey,
        run: u64,
        history_epoch: u64,
        item: u64,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let stop = ChatCommand::StopShell {
            run,
            history_epoch,
            item,
        };
        self.durable(key, stop, cx, move |this, cx| {
            if run != this.generation {
                return Outcome::Failed("stale_run".into());
            }
            if history_epoch != this.epoch {
                return Outcome::Failed(
                    "conditions_changed: That command belongs to another history part.".into(),
                );
            }
            if !this.shell_running(ItemId(item)) {
                return Outcome::Failed(
                    "conditions_changed: That command is no longer running.".into(),
                );
            }
            this.stop_shell(ItemId(item), cx);
            Outcome::Succeeded
        })
    }

    // Shared preflight.

    /// The chat's authority for command authorization: its canonical checkout or
    /// "private". `None` until its record has one (never guessed).
    fn command_root(&self, cx: &App) -> Option<String> {
        let chat = self.chat_id()?;
        let settings = cx.global::<AppState>().settings.read(cx);
        let authority = settings.chat_record(&chat)?.authority_root();
        (!authority.is_empty()).then(|| authority.to_owned())
    }

    /// Accept, then `attempting`, then `effect`, then the final state. A command seen
    /// before returns its recorded state; one already attempting elsewhere is not
    /// performed twice.
    fn durable(
        &mut self,
        key: RemoteKey,
        command: ChatCommand,
        cx: &mut Context<Self>,
        effect: impl FnOnce(&mut Self, &mut Context<Self>) -> Outcome + 'static,
    ) -> Task<Result<CommandReceipt, String>> {
        self.durable_async(key, command, Accept::Plain, cx, |this, cx| {
            Task::ready(effect(this, cx))
        })
    }

    /// `durable` for an effect that ends later (an edit waits for files and the new
    /// session). `accept` says what the acceptance retains with the command.
    fn durable_async(
        &mut self,
        key: RemoteKey,
        command: ChatCommand,
        accept: Accept,
        cx: &mut Context<Self>,
        effect: impl FnOnce(&mut Self, &mut Context<Self>) -> Task<Outcome> + 'static,
    ) -> Task<Result<CommandReceipt, String>> {
        let Some(client) = self.client(cx) else {
            return Task::ready(Err("Chat storage is unavailable.".into()));
        };
        let Some(chat) = self.chat_id() else {
            return Task::ready(Err("This chat has no identity.".into()));
        };
        // The same ID with other arguments is an idempotency conflict in the inbox.
        let RemoteKey { key, fingerprint } = key;
        let Some(input) = self.command_root(cx).and_then(|root| {
            let input = command.command(key, &chat)?.with_root(root)?;
            match fingerprint {
                Some(fingerprint) => input.with_fingerprint(fingerprint),
                None => Some(input),
            }
        }) else {
            return Task::ready(Err("This command is not valid for this chat.".into()));
        };
        let key = input.key().clone();
        let scope = command.scope();
        cx.spawn(async move |this, cx| {
            let receipt = match accept {
                Accept::Plain => client.accept_runtime_command(input, scope).await,
                // Images the command names are retained in its acceptance transaction:
                // none can be collected before the effect reads them.
                Accept::Retaining(artifacts) => {
                    client
                        .accept_runtime_chat_command(input, scope, chat, artifacts)
                        .await
                }
            }
            .map_err(|e| e.to_string())?;
            if receipt.state() != CommandState::Accepted {
                return Ok(receipt);
            }
            if this.read_with(cx, |this, _| this.closed).unwrap_or(true) {
                return transition(
                    &client,
                    &key,
                    CommandState::Accepted,
                    CommandState::Rejected,
                    reason("resource_deleted"),
                )
                .await;
            }
            // Recorded before the effect: a crash from here on is delivery unknown.
            if let Err(error) = client
                .transition_runtime_command(
                    key.clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
            {
                // A concurrent retry of the same command got there first.
                return match client.get_runtime_command(key.clone(), scope).await {
                    Ok(Some(receipt)) => Ok(receipt),
                    _ => Err(error.to_string()),
                };
            }
            let outcome = match this.update(cx, |this, cx| {
                let outcome = effect(this, cx);
                cx.emit(ChatStateChanged);
                cx.notify();
                outcome
            }) {
                Ok(outcome) => outcome.await,
                Err(_) => Outcome::Failed("resource_deleted".into()),
            };
            let _ = this.update(cx, |_, cx| cx.emit(ChatStateChanged));
            let (state, result) = match outcome {
                Outcome::Succeeded => (CommandState::Succeeded, None),
                Outcome::SucceededWith(result) => (CommandState::Succeeded, Some(result)),
                Outcome::Failed(text) => (CommandState::Failed, reason(&text)),
                Outcome::Unknown(text) => (CommandState::DeliveryUnknown, reason(&text)),
            };
            transition(&client, &key, CommandState::Attempting, state, result).await
        })
    }

    fn process_limit(&self, cx: &mut Context<Self>) -> Option<String> {
        let me = cx.entity_id();
        let live = cx
            .default_global::<super::LiveChats>()
            .0
            .borrow()
            .iter()
            .filter(|id| **id != me)
            .count();
        (live >= super::MAX_CHAT_PROCESSES).then(|| {
            format!(
                "resource_exhausted: {} chats are already running. Stop one, then retry.",
                super::MAX_CHAT_PROCESSES
            )
        })
    }
}

/// What a command's acceptance retains with it.
enum Accept {
    Plain,
    /// Host artifacts (images) the command's draft names, retained for the chat.
    Retaining(Vec<String>),
}

async fn transition(
    client: &SettingsClient,
    key: &CommandKey,
    from: CommandState,
    to: CommandState,
    result: Option<Vec<u8>>,
) -> Result<CommandReceipt, String> {
    client
        .transition_runtime_command(key.clone(), from, to, result)
        .await
        .map_err(|e| e.to_string())
}
