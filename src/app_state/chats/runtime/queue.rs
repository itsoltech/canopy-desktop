//! The chat's durable host queue (CHAT-01/02, §5.2). Accepted messages, their order
//! and delivery state live in the host database, never only in a session: the runtime
//! records `attempting` before it writes a message to the agent and settles it only on
//! the backend's evidence. Local UI and remote commands go through the same handlers
//! with their own command keys (device, epoch, command ID).
use super::{ChatRuntime, ChatRuntimeEvent, ChatStateChanged};
use crate::app_state::AppState;
use canopy_desktop::{
    chat::{
        backend::Receipt,
        backend_session::BackendSession,
        session::{Draft, RESUME_PROMPT},
    },
    host::{
        ChatQueue, ChatRecord, CommandInput, CommandKey, CommandReceipt, QueueDraft, QueueMutation,
        QueueState, UnknownResolution,
    },
    settings::SettingsClient,
};
use gpui_kit::*;

/// How often a revision-checked write is retried after a concurrent change.
const RETRIES: usize = 5;

#[derive(Default)]
pub(super) struct HostQueue {
    /// The durable queue as last read or written.
    state: Option<ChatQueue>,
    /// The catalog record this runtime last saved; Send waits for it.
    record: Option<ChatRecord>,
    saving_record: bool,
    /// Writes in flight that decide what goes next; nothing is dispatched meanwhile.
    busy: usize,
    /// The message this run wrote to the agent (and what was written, to give back
    /// on refusal), until its evidence settles it.
    sent: Option<(String, Draft)>,
    /// Stop cut work short: Resume first asks the agent to finish it (RESUME_PROMPT).
    stopped: bool,
    resume_prompt: bool,
    /// Sends waiting for the record of the current account and history part, and
    /// whether each is a Steer.
    held: Vec<(Draft, bool)>,
    /// Accepted messages the user asked to steer into the running turn (CHAT-03).
    /// Not durable: after a restart they wait as ordinary queued messages.
    steer: std::collections::HashSet<String>,
}

/// One visible queue line: durable entries and steers in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct QueueLine {
    pub id: String,
    pub text: String,
    pub attachments: Vec<canopy_desktop::chat::transcript::Attachment>,
    pub state: LineState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineState {
    Queued,
    /// Written to the agent, waiting for its evidence (or a steer being read).
    Sending,
    /// The run ended without evidence: the user decides (Delivered, Send again, Dismiss).
    Unknown,
}

fn error(error: canopy_desktop::settings::Error) -> String {
    use canopy_desktop::settings::Error;
    match error {
        Error::ResourceConflict => "The queue changed meanwhile; review it and try again.".into(),
        Error::OperationPending => "A message is still being delivered.".into(),
        Error::NotFound => "This chat is not in the host catalog.".into(),
        other => other.to_string(),
    }
}

impl ChatRuntime {
    pub(super) fn client(&self, cx: &App) -> Option<SettingsClient> {
        cx.global::<AppState>().settings.read(cx).client()
    }

    // Reading.

    /// Queue lines in order: what waits, what is being delivered, what is uncertain.
    pub fn queue_lines(&self) -> Vec<QueueLine> {
        if let Some(remote) = &self.remote {
            return remote.lines();
        }
        let mut lines: Vec<QueueLine> = self
            .queue
            .state
            .iter()
            .flat_map(|queue| queue.entries())
            .filter_map(|entry| {
                let state = match entry.state() {
                    QueueState::Queued => LineState::Queued,
                    QueueState::Attempting => LineState::Sending,
                    QueueState::DeliveryUnknown => LineState::Unknown,
                    _ => return None,
                };
                // Images sent from another device are host artifacts, not attachments.
                let images = entry.draft().artifact_ids().len();
                let text = match (entry.draft().text().trim().is_empty(), images) {
                    (true, 1) => "Image".to_owned(),
                    (true, n) if n > 1 => format!("{n} images"),
                    _ => entry.draft().text().to_owned(),
                };
                Some(QueueLine {
                    id: entry.id().to_owned(),
                    text,
                    attachments: entry.draft().attachments().to_vec(),
                    state,
                })
            })
            .collect();
        // Steers written into the running turn, until the agent reads them.
        if let Some(native) = self.native() {
            lines.extend(
                native
                    .queue()
                    .iter()
                    .filter(|queued| queued.steering.is_some())
                    .map(|queued| QueueLine {
                        id: format!("steer-{}", queued.id.0),
                        text: queued.text.clone(),
                        attachments: queued.attachments.clone(),
                        state: LineState::Sending,
                    }),
            );
        }
        lines
    }
    /// Waiting messages are held: after Stop or a restart of Canopy, until Resume.
    pub fn queue_paused(&self) -> bool {
        if let Some(remote) = &self.remote {
            return remote.lines().iter().any(|l| l.id.starts_with("device:"))
                || remote.projection.queue.as_ref().is_some_and(|q| q.paused);
        }
        self.queue.state.as_ref().is_some_and(|queue| {
            queue.paused()
                && queue
                    .entries()
                    .iter()
                    .any(|entry| entry.state() == QueueState::Queued)
        })
    }
    /// The durable queue (revision, pause, every entry) for remote snapshots.
    #[allow(dead_code, reason = "read by the remote bridge")]
    pub fn host_queue(&self) -> Option<&ChatQueue> {
        self.queue.state.as_ref()
    }
    /// Accepted messages not yet known to be delivered (closing a chat keeps them).
    pub(super) fn pending_messages(&self) -> usize {
        self.queue.held.len()
            + self.queue.state.as_ref().map_or(0, |queue| {
                queue
                    .entries()
                    .iter()
                    .filter(|entry| {
                        matches!(
                            entry.state(),
                            QueueState::Queued
                                | QueueState::Attempting
                                | QueueState::DeliveryUnknown
                        )
                    })
                    .count()
            })
    }
    /// Something waits to go out now (not held): the process is still needed.
    pub(super) fn queue_active(&self) -> bool {
        self.queue.sent.is_some()
            || self.queue.busy > 0
            || self.queue.resume_prompt
            || self.edit_recovery.is_some()
            || self.queue.state.as_ref().is_some_and(|queue| {
                !queue.paused()
                    && !self.queue_blocked()
                    && queue
                        .entries()
                        .iter()
                        .any(|entry| entry.state() == QueueState::Queued)
            })
    }
    /// A delivery is uncertain: nothing after it goes out until the user decides.
    fn queue_blocked(&self) -> bool {
        self.queue.state.as_ref().is_some_and(|queue| {
            queue.entries().iter().any(|entry| {
                matches!(
                    entry.state(),
                    QueueState::Attempting | QueueState::DeliveryUnknown
                )
            })
        })
    }

    // The chat's catalog record.

    /// Saves the chat's record (agent, account, folder, history part) when it changed.
    /// A record the host adopted without metadata needs the user's choice first and
    /// is never overwritten here.
    pub(super) fn save_record(&mut self, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            return;
        }
        let Some(chat) = self.settings.id.clone() else {
            return;
        };
        if matches!(self.history, super::History::Loading(_)) || self.queue.saving_record {
            return;
        }
        let settings = cx.global::<AppState>().settings.clone();
        if let Some(reason) = settings
            .read(cx)
            .chat_record(&chat)
            .and_then(ChatRecord::recovery_reason)
        {
            self.recovery = Some(reason.to_owned());
            return;
        }
        let Some(client) = self.client(cx) else {
            return;
        };
        let projects = cx.global::<AppState>().projects.read(cx);
        let scratch = projects.scratch_id(cx);
        let workspace = projects
            .runtime_workspaces(cx)
            .into_iter()
            .find(|workspace| workspace.all_panes().iter().any(|p| p.id == self.pane.id))
            .map(|workspace| workspace.id);
        let root = match (workspace, &self.catalog_root) {
            (Some(id), _) if id == scratch => None,
            (Some(id), _) => serde_json::to_string(&id).ok(),
            // A chat no pane shows keeps the workspace its record names.
            (None, Some(root)) => root.clone(),
            (None, None) => return,
        };
        // The authority remote grants are checked against: the project's canonical
        // checkout (resolved once, off this thread), or "private".
        let known = settings
            .read(cx)
            .chat_record(&chat)
            .filter(|saved| saved.workspace() == root.as_deref())
            .map(|saved| saved.authority_root().to_owned())
            .filter(|authority| !authority.is_empty());
        let checkout = root
            .as_deref()
            .and_then(|root| {
                serde_json::from_str::<canopy_desktop::state::workspace::WorkspaceId>(root).ok()
            })
            .and_then(|id| {
                projects
                    .catalog
                    .items
                    .iter()
                    .find(|project| project.workspace == id)
                    .map(|project| {
                        project
                            .worktree_path
                            .clone()
                            .unwrap_or_else(|| project.path.clone())
                    })
            });
        let project = root.is_some();
        let mut metadata = self.pane.metadata.clone();
        metadata.chat = Some(Box::new(self.settings.clone()));
        let Some(record) = ChatRecord::new(chat.clone(), root, self.pane.tool.clone(), metadata)
            .and_then(|record| record.with_history_epoch(self.epoch))
        else {
            return;
        };
        let record = match (project, known) {
            (false, _) => Ok(record),
            (true, Some(authority)) => match record.with_authority_root(authority) {
                Some(record) => Ok(record),
                None => return,
            },
            (true, None) => match checkout {
                Some(checkout) => Err((record, checkout)),
                // Its project isn't open: the record keeps what it has.
                None => return,
            },
        };
        if let Ok(record) = &record
            && self.queue.record.as_ref() == Some(record)
        {
            return;
        }
        self.queue.saving_record = true;
        cx.spawn(async move |this, cx| {
            let record = match record {
                Ok(record) => Some(record),
                Err((record, checkout)) => cx
                    .background_spawn(async move { std::fs::canonicalize(checkout) })
                    .await
                    .ok()
                    .and_then(|path| path.to_str().map(str::to_owned))
                    .and_then(|authority| record.with_authority_root(authority)),
            };
            let Some(record) = record else {
                let _ = this.update(cx, |this, cx| {
                    this.queue.saving_record = false;
                    this.notice = Some(
                        "This chat's project folder is unavailable; it couldn't be saved.".into(),
                    );
                    cx.notify();
                });
                return;
            };
            let saved = client.save_runtime_chat(record.clone()).await;
            let queue = match saved {
                Ok(()) => Some(client.load_runtime_queue(chat).await),
                Err(_) => None,
            };
            let _ = this.update(cx, |this, cx| {
                this.queue.saving_record = false;
                match (saved, queue) {
                    (Ok(()), Some(queue)) => {
                        settings.update(cx, |settings, _| settings.record_saved(record.clone()));
                        this.queue.record = Some(record);
                        match queue {
                            Ok(queue) => this.queue.state = Some(queue),
                            Err(e) => this.notice = Some(error(e)),
                        }
                        // Saved meanwhile under a newer account or history part.
                        this.save_record(cx);
                        if !this.queue.saving_record {
                            for (draft, steer) in std::mem::take(&mut this.queue.held) {
                                this.send_draft_as(draft, steer, cx);
                            }
                        }
                        this.dispatch(cx);
                    }
                    (Err(e), _) => {
                        this.notice = Some(format!("This chat couldn't be saved: {}", error(e)));
                    }
                    _ => {}
                }
                cx.emit(ChatStateChanged);
                cx.notify();
            });
        })
        .detach();
    }

    // Commands (local and remote alike).

    /// Accepts message `message` into the durable queue; the receipt is final once
    /// this resolves, and repeated identical commands return the same receipt.
    pub fn accept_message(
        &mut self,
        input: CommandInput,
        message: String,
        draft: QueueDraft,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let (Some(client), Some(chat)) = (self.client(cx), self.settings.id.clone()) else {
            return Task::ready(Err("Chat storage is unavailable.".into()));
        };
        if self.closed {
            return Task::ready(Err("This chat is closed.".into()));
        }
        self.queue.busy += 1;
        cx.spawn(async move |this, cx| {
            let accepted = client
                .accept_runtime_message(input, chat.clone(), message.clone(), draft)
                .await
                .map_err(error);
            let queue = client.load_runtime_queue(chat.clone()).await;
            // A held queue with nothing else waiting goes on with this message: it is
            // the user's explicit next send (a restart or Stop held only older ones).
            let resume = accepted.is_ok()
                && queue.as_ref().is_ok_and(|queue| {
                    queue.paused()
                        && queue.entries().iter().all(|entry| {
                            entry.id() == message
                                || !matches!(
                                    entry.state(),
                                    QueueState::Queued
                                        | QueueState::Attempting
                                        | QueueState::DeliveryUnknown
                                )
                        })
                });
            let queue = match (resume, queue) {
                (true, Ok(queue)) => mutate(
                    &client,
                    &chat,
                    queue.revision(),
                    QueueMutation::Pause { paused: false },
                )
                .await
                .or(Ok::<_, String>(queue)),
                (_, queue) => queue.map_err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.queue.busy -= 1;
                if let Ok(queue) = queue {
                    this.queue.state = Some(queue);
                }
                if accepted.is_ok() {
                    this.queue.stopped = false;
                    this.queue.resume_prompt = false;
                }
                this.dispatch(cx);
                cx.emit(ChatStateChanged);
                cx.notify();
            });
            accepted
        })
    }

    /// Applies a revision-checked queue change: edit, move, remove, pause or resolve.
    pub fn change_queue(
        &mut self,
        input: CommandInput,
        revision: u64,
        mutation: QueueMutation,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let (Some(client), Some(chat)) = (self.client(cx), self.settings.id.clone()) else {
            return Task::ready(Err("Chat storage is unavailable.".into()));
        };
        self.queue.busy += 1;
        cx.spawn(async move |this, cx| {
            let changed = client
                .mutate_runtime_queue(input, chat.clone(), revision, mutation)
                .await
                .map_err(error);
            let queue = client.load_runtime_queue(chat).await;
            let _ = this.update(cx, |this, cx| {
                this.queue.busy -= 1;
                if let Ok(queue) = queue {
                    this.queue.state = Some(queue);
                }
                this.dispatch(cx);
                cx.emit(ChatStateChanged);
                cx.notify();
            });
            changed
        })
    }

    /// A local queue change at the revision on screen; a conflict is said, not retried.
    fn change_locally(
        &mut self,
        mutation: QueueMutation,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let (Some(chat), Some(revision)) = (
            self.settings.id.clone(),
            self.queue.state.as_ref().map(ChatQueue::revision),
        ) else {
            return Task::ready(Err("The queue is still loading.".into()));
        };
        let key = CommandKey::local(uuid::Uuid::new_v4().to_string()).expect("UUID");
        let Some(input) = mutation.command(key.clone(), &chat, revision) else {
            return Task::ready(Err("This queue change is not valid.".into()));
        };
        let client = self.client(cx);
        let changed = self.change_queue(input, revision, mutation, cx);
        cx.spawn(async move |this, cx| {
            let result = changed.await.map(|_| ());
            if result.is_ok()
                && let Some(client) = client
            {
                // Local receipts are cleanup only; there is no remote retry to answer.
                let _ = client.acknowledge_runtime_command(key).await;
            }
            if let Err(error) = &result {
                let _ = this.update(cx, |this, cx| {
                    this.notice = Some(error.clone());
                    cx.notify();
                });
            }
            result
        })
    }

    // Local entry points (the chat view).

    /// Sends a message: accepted into the queue, delivered when the agent can take it.
    pub fn send_draft(&mut self, draft: Draft, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            self.send_remote(draft, false, cx);
            return;
        }
        self.send_draft_as(draft, false, cx)
    }
    /// `send_draft`; `steer`: once accepted it goes first and into the running turn.
    pub(super) fn send_draft_as(&mut self, draft: Draft, steer: bool, cx: &mut Context<Self>) {
        self.notice = None;
        if let Some(reason) = &self.recovery {
            self.notice = Some(reason.clone());
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            cx.notify();
            return;
        }
        let record = self
            .queue
            .record
            .clone()
            .filter(|record| !self.queue.saving_record && record.history_epoch() == self.epoch);
        let (Some(chat), Some(record)) = (self.settings.id.clone(), record) else {
            // Sent with the record of this account and history part, once it's saved.
            self.queue.held.push((draft, steer));
            self.save_record(cx);
            cx.notify();
            return;
        };
        let title = if draft.text.trim().is_empty() {
            "Image".to_owned()
        } else {
            canopy_desktop::chat::presentation::title(&draft.text)
        };
        let message = uuid::Uuid::new_v4().to_string();
        let key = CommandKey::local(uuid::Uuid::new_v4().to_string()).expect("UUID");
        // A Steer is the same Send bound to this run: its intent survives a retry.
        let command = if steer {
            QueueDraft::from(draft.clone()).command_for_steer(
                key.clone(),
                &chat,
                message.clone(),
                record.send_context(),
                self.generation,
            )
        } else {
            QueueDraft::from(draft.clone()).command_for_message(
                key.clone(),
                &chat,
                message.clone(),
                record.send_context(),
            )
        };
        let Some((input, queued)) = command else {
            self.notice = Some("This message can't be sent (empty or too large).".into());
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            cx.notify();
            return;
        };
        let client = self.client(cx);
        let accepted = self.accept_message(input, message.clone(), queued, cx);
        if !self.sent_any {
            self.sent_any = true;
            let pane = self.pane.id;
            cx.global::<AppState>()
                .projects
                .clone()
                .update(cx, |projects, cx| {
                    projects.update_chat(pane, None, Some(&title), cx)
                });
        }
        cx.spawn(async move |this, cx| match accepted.await {
            Ok(_) => {
                if steer {
                    let _ = this.update(cx, |this, cx| this.route_steer(message, cx));
                }
                if let Some(client) = client {
                    let _ = client.acknowledge_runtime_command(key).await;
                }
            }
            Err(error) => {
                let _ = this.update(cx, |this, cx| {
                    this.notice = Some(format!("The message wasn't accepted: {error}"));
                    cx.emit(ChatRuntimeEvent::DraftReturned(draft));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Lets the held queue go on: work Stop cut short is finished first.
    pub fn resume_queue(&mut self, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            self.client_resume(cx);
            return;
        }
        let resume = self.change_locally(QueueMutation::Pause { paused: false }, cx);
        cx.spawn(async move |this, cx| {
            if resume.await.is_ok() {
                let _ = this.update(cx, |this, cx| {
                    this.queue.resume_prompt = std::mem::take(&mut this.queue.stopped);
                    this.dispatch(cx);
                });
            }
        })
        .detach();
    }
    /// Stop holds the queue (CHAT-04).
    pub(super) fn hold_queue(&mut self, cx: &mut Context<Self>) {
        self.queue.stopped = true;
        self.queue.resume_prompt = false;
        if self
            .queue
            .state
            .as_ref()
            .is_some_and(|queue| !queue.paused())
        {
            self.change_locally(QueueMutation::Pause { paused: true }, cx)
                .detach();
        }
    }
    /// Drag and drop in the queue: `message` goes in front of `before`.
    pub fn move_queued(&mut self, message: String, before: String, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            match (
                message.strip_prefix("device:"),
                before.strip_prefix("device:"),
            ) {
                (Some(id), Some(before)) => {
                    if let Some(host) = self.remote_host() {
                        if let Err(error) = host.move_unsent(id, before) {
                            self.note(error, cx);
                        }
                        cx.notify();
                    }
                    return;
                }
                (Some(_), _) | (_, Some(_)) => {
                    self.note(
                        "Messages saved on this device can be reordered after they reach the host."
                            .into(),
                        cx,
                    );
                    return;
                }
                _ => {}
            }
            self.remote_queue(
                canopy_desktop::remote::proto::change_queue_request::Change::Move(
                    canopy_desktop::remote::proto::QueueMove {
                        message_id: message,
                        before_message_id: Some(before),
                    },
                ),
                cx,
            );
            return;
        }
        if message != before {
            self.change_locally(
                QueueMutation::Move {
                    message,
                    before: Some(before),
                },
                cx,
            )
            .detach();
        }
    }
    pub fn remove_queued(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(id) = message
            .strip_prefix("device:")
            .filter(|_| self.remote.is_some())
        {
            self.client_remove_saved(id.into(), cx);
            return;
        }
        if self.remote.is_some() {
            self.remote_queue(
                canopy_desktop::remote::proto::change_queue_request::Change::Remove(
                    canopy_desktop::remote::proto::QueueRemove {
                        message_id: message,
                    },
                ),
                cx,
            );
            return;
        }
        self.change_locally(QueueMutation::Remove { message }, cx)
            .detach();
    }
    /// Replaces a waiting message's content in place.
    #[allow(dead_code, reason = "called by the remote bridge")]
    pub fn edit_queued(
        &mut self,
        message: String,
        draft: Draft,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let draft = QueueDraft::from(draft);
        self.change_locally(QueueMutation::Edit { message, draft }, cx)
    }
    /// Takes a waiting message back into the composer; `replacement` (what was typed
    /// there) is sent in its place so nothing typed is lost.
    pub fn take_queued(
        &mut self,
        message: String,
        replacement: Draft,
        cx: &mut Context<Self>,
    ) -> Task<Option<Draft>> {
        if self.remote.is_some() {
            return self.client_take_queued(message, replacement, cx);
        }
        let Some(queued) = self.entry_draft(&message) else {
            return Task::ready(None);
        };
        let (chat, store) = (self.settings.id.clone(), self.artifacts(cx));
        let removed = self.change_locally(QueueMutation::Remove { message }, cx);
        cx.spawn(async move |this, cx| {
            removed.await.ok()?;
            if !replacement.text.trim().is_empty() || !replacement.attachments.is_empty() {
                let _ = this.update(cx, |this, cx| this.send_draft(replacement, cx));
            }
            // Its host images become the composer's attachments.
            let draft = cx
                .background_spawn(async move { materialize(chat.as_deref(), queued, store) })
                .await;
            Some(match draft {
                Ok(draft) => draft,
                Err((draft, error)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.notice = Some(error);
                        cx.notify();
                    });
                    draft
                }
            })
        })
    }
    /// The user's decision about a message whose delivery is uncertain.
    pub fn resolve_unknown(
        &mut self,
        message: String,
        resolution: UnknownResolution,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = message
            .strip_prefix("device:")
            .filter(|_| self.remote.is_some())
        {
            self.client_resolve_saved(
                id.into(),
                matches!(resolution, UnknownResolution::Resend { .. }),
                cx,
            );
            return;
        }
        if self.remote.is_some() {
            use canopy_desktop::remote::proto as p;
            let resolution = match resolution {
                UnknownResolution::Delivered => p::UnknownResolution::AcceptAsDelivered,
                UnknownResolution::Resend { .. } => p::UnknownResolution::SendAgain,
                UnknownResolution::Dismissed => p::UnknownResolution::Dismiss,
            };
            self.remote_queue(
                p::change_queue_request::Change::Resolve(p::QueueResolve {
                    message_id: message,
                    resolution: resolution as i32,
                    new_message_id: (resolution == p::UnknownResolution::SendAgain)
                        .then(|| uuid::Uuid::new_v4().to_string()),
                }),
                cx,
            );
            return;
        }
        self.change_locally(
            QueueMutation::ResolveUnknown {
                message,
                resolution,
            },
            cx,
        )
        .detach();
    }
    /// Sends a waiting message into the running turn now (CHAT-03). It stays the same
    /// durable message (ID, command and receipts): it moves first in line and goes out
    /// as a steer after `attempting` is recorded, never removed and sent around the
    /// queue. Without a turn to steer it simply goes next.
    pub fn steer_queued(&mut self, message: String, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            self.client_steer_queued(message, cx);
            return;
        }
        if self.entry_draft(&message).is_none() {
            return;
        }
        if !self.capabilities(cx).steer {
            self.notice = Some(format!(
                "{} can't take a message during its turn; it waits in the queue.",
                self.agent_name(cx)
            ));
            cx.notify();
            return;
        }
        self.steer_accepted_message(message, cx);
    }
    /// Accepts a Steer of new text (a remote SteerRequest): one Send command bound to
    /// run `run`. A Steer for another run is refused before anything is accepted;
    /// when the run changes while it is being accepted, the message waits with the
    /// queue held: it never goes into another run unasked.
    pub fn accept_steer(
        &mut self,
        input: CommandInput,
        message: String,
        draft: QueueDraft,
        run: u64,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        if run != self.generation || self.session.is_none() {
            return Task::ready(Err("stale_run".into()));
        }
        if !self.capabilities(cx).steer {
            return Task::ready(Err(format!("{} can't be steered.", self.agent_name(cx))));
        }
        let accepted = self.accept_message(input, message.clone(), draft, cx);
        cx.spawn(async move |this, cx| {
            let receipt = accepted.await?;
            let _ = this.update(cx, |this, cx| this.route_steer(message, cx));
            Ok(receipt)
        })
    }
    /// An accepted Steer goes first in this run, or waits held when the run changed.
    fn route_steer(&mut self, message: String, cx: &mut Context<Self>) {
        let run = self
            .queue
            .state
            .as_ref()
            .and_then(|queue| queue.entries().iter().find(|e| e.id() == message))
            .and_then(|entry| entry.steer_run());
        if run.is_some_and(|run| run != self.generation) {
            self.hold_queue(cx);
            self.notice = Some(
                "The agent restarted before the message was steered; it waits in the queue.".into(),
            );
            cx.notify();
            return;
        }
        self.steer_accepted_message(message, cx);
    }
    /// Marks accepted `message` for Steer and moves it first in line.
    fn steer_accepted_message(&mut self, message: String, cx: &mut Context<Self>) {
        if self.queue_paused() {
            self.notice = Some("The queue is held. Resume it to send this message.".into());
            cx.notify();
            return;
        }
        self.queue.steer.insert(message.clone());
        match self.first_queued() {
            Some(first) if first != message => {
                let moved = self.change_locally(
                    QueueMutation::Move {
                        message: message.clone(),
                        before: Some(first),
                    },
                    cx,
                );
                cx.spawn(async move |this, cx| {
                    if moved.await.is_err() {
                        let _ = this.update(cx, |this, _| this.queue.steer.remove(&message));
                    }
                })
                .detach();
            }
            _ => self.dispatch(cx),
        }
    }
    /// The oldest waiting message (⌘⏎ steers it).
    pub fn first_queued(&self) -> Option<String> {
        if let Some(source) = &self.remote {
            return source
                .lines()
                .into_iter()
                .find(|l| l.state == LineState::Queued)
                .map(|l| l.id);
        }
        self.queue.state.as_ref().and_then(|queue| {
            queue
                .entries()
                .iter()
                .find(|entry| entry.state() == QueueState::Queued)
                .map(|entry| entry.id().to_owned())
        })
    }
    pub(super) fn entry_draft(&self, message: &str) -> Option<QueueDraft> {
        self.queue
            .state
            .as_ref()?
            .entries()
            .iter()
            .find(|entry| entry.id() == message && entry.state() == QueueState::Queued)
            .map(|entry| entry.draft().clone())
    }
    pub(super) fn artifacts(
        &self,
        cx: &App,
    ) -> Option<std::sync::Arc<canopy_desktop::artifacts::ArtifactStore>> {
        cx.global::<AppState>().settings.read(cx).artifacts()
    }

    // Dispatch.

    /// Writes the first waiting message to the agent once it can take it, after the
    /// queue recorded `attempting`.
    pub(super) fn dispatch(&mut self, cx: &mut Context<Self>) {
        if self.closed
            || self.recovery.is_some()
            || self.queue.busy > 0
            || self.queue.sent.is_some()
            || self.queue_blocked()
        {
            return;
        }
        // The edited message is the new branch's first message (it was never queued).
        if self.edit_recovery.is_some()
            && self
                .session
                .as_ref()
                .is_some_and(BackendSession::ready_for_message)
        {
            let draft = self.edit_recovery.take().expect("checked");
            // The edit is through: its message goes out as the branch's first.
            self.settle_edit(Ok(()));
            self.send_draft(draft, cx);
            return;
        }
        let Some(queue) = &self.queue.state else {
            return;
        };
        if queue.paused() {
            return;
        }
        if self.queue.resume_prompt
            && let Some(session) = &mut self.session
            && session.ready_for_message()
        {
            // Not a user message: nothing to deliver durably, its receipt is ignored.
            self.queue.resume_prompt = false;
            if let Ok(lines) = session.submit(
                Draft::text(RESUME_PROMPT),
                &uuid::Uuid::new_v4().to_string(),
            ) {
                self.write(lines);
                self.after_change(cx);
            }
            return;
        }
        let Some(entry) = queue
            .entries()
            .iter()
            .find(|entry| entry.state() == QueueState::Queued)
            .cloned()
        else {
            return;
        };
        let revision = queue.revision();
        let steer = match &self.session {
            // Send starts a dormant or never-started chat (lazy start otherwise).
            None => {
                if self.dormant() {
                    self.start(cx);
                }
                return;
            }
            Some(session) if session.ready_for_message() => false,
            Some(BackendSession::Native(native))
                if !entry.draft().clone().into_draft().goal
                    && (native.goal_steer_open()
                        || (self.queue.steer.contains(entry.id())
                            || entry.steer_run() == Some(self.generation))
                            && native.can_steer()) =>
            {
                true
            }
            Some(_) => return,
        };
        self.queue.steer.remove(entry.id());
        let (Some(client), Some(chat)) = (self.client(cx), self.settings.id.clone()) else {
            return;
        };
        let generation = self.generation;
        let message = entry.id().to_owned();
        let queued = entry.draft().clone();
        let store = self.artifacts(cx);
        self.queue.busy += 1;
        cx.spawn(async move |this, cx| {
            let attempting = client
                .transition_runtime_queue(
                    chat.clone(),
                    message.clone(),
                    revision,
                    QueueState::Queued,
                    QueueState::Attempting,
                )
                .await;
            let attempting = match attempting {
                Ok(queue) => queue,
                Err(_) => {
                    // Changed meanwhile: read it again and decide again.
                    let queue = client.load_runtime_queue(chat).await;
                    let _ = this.update(cx, |this, cx| {
                        this.queue.busy -= 1;
                        if let Ok(queue) = queue {
                            this.queue.state = Some(queue);
                        }
                        this.dispatch(cx);
                        cx.notify();
                    });
                    return;
                }
            };
            let loaded = cx
                .background_spawn(async move { materialize(Some(&chat), queued, store) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.queue.busy -= 1;
                this.queue.state = Some(attempting);
                let draft = match &loaded {
                    Ok(draft) | Err((draft, _)) => draft.clone(),
                };
                let written = match (&loaded, &mut this.session) {
                    (Ok(draft), Some(session)) if this.generation == generation => {
                        if steer {
                            match session {
                                BackendSession::Native(native) => {
                                    native.steer_accepted(draft.clone(), &message)
                                }
                                BackendSession::Acp(_) => Err("Steering isn't supported.".into()),
                            }
                        } else {
                            session.submit(draft.clone(), &message)
                        }
                    }
                    (Err((_, error)), _) => Err(error.clone()),
                    _ => Err("The agent stopped before the message went out.".into()),
                };
                let first_text = (!steer && this.process.is_some()).then(|| {
                    (
                        std::time::Instant::now(),
                        this.generation,
                        this.session
                            .as_ref()
                            .map_or(0, |session| session.transcript().items().len()),
                    )
                });
                match written {
                    Ok(lines) => match this.write_dispatched(lines) {
                        Ok(()) => {
                            if first_text.is_some() {
                                this.first_text = first_text;
                            }
                            this.queue.sent = Some((message, draft));
                            this.after_change(cx);
                        }
                        // Refused before anything was enqueued for the process: the
                        // message failed and returns; the session already counts it
                        // as sent, so this run ends instead of waiting for it forever.
                        Err((reason, false)) => {
                            this.settle(message, QueueState::Failed, Some(draft), cx);
                            this.abort_run(reason, cx);
                        }
                        // Part of it may have reached the agent: never resent by itself.
                        Err((reason, true)) => {
                            this.settle(message, QueueState::DeliveryUnknown, None, cx);
                            this.abort_run(reason, cx);
                        }
                    },
                    Err(reason) => {
                        // Nothing was written: the message failed; it returns to the user.
                        this.notice = Some(reason);
                        this.settle(message, QueueState::Failed, Some(draft), cx);
                    }
                }
            });
        })
        .detach();
    }

    /// The agent's input refused what this run wrote: its session state no longer
    /// matches what the agent got. The run ends (the queue is held, the goal and the
    /// conversation stay for Restart); nothing is resent by itself.
    pub(super) fn abort_run(&mut self, reason: String, cx: &mut Context<Self>) {
        self.hold_queue(cx);
        let stop = self.shutdown(cx);
        self.error = Some(format!(
            "The agent couldn't take what was sent ({reason}). It was stopped; Restart to continue."
        ));
        cx.spawn(async move |this, cx| {
            let stopped = stop.await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = stopped {
                    this.error = Some(error);
                }
                cx.emit(ChatStateChanged);
                cx.notify();
            });
        })
        .detach();
        cx.emit(ChatStateChanged);
        cx.notify();
    }

    /// Writes a dispatched message's lines. Err: why, and whether any line was already
    /// enqueued for the process (then delivery is uncertain).
    pub(super) fn write_dispatched(
        &mut self,
        lines: Vec<serde_json::Value>,
    ) -> Result<(), (String, bool)> {
        let Some(process) = &self.process else {
            // Held for initialize: goes out with the process (never ready before it).
            self.outbox.extend(lines);
            return Ok(());
        };
        for (index, line) in lines.iter().enumerate() {
            process.send(line).map_err(|error| (error, index > 0))?;
        }
        Ok(())
    }

    /// Settles the dispatched message on the backend's evidence; called with every
    /// session change.
    pub(super) fn settle_receipts(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        let receipts = session.take_receipts();
        let unread = match session {
            BackendSession::Native(native) => native.take_unread_steers(),
            BackendSession::Acp(_) => vec![],
        };
        let ended = session.status().ended();
        for (id, receipt) in receipts {
            if self.queue.sent.as_ref().is_none_or(|(sent, _)| *sent != id) {
                continue;
            }
            let (_, draft) = self.queue.sent.take().expect("checked");
            let (state, returned) = match receipt {
                Receipt::Delivered => (QueueState::Delivered, None),
                Receipt::Refused => {
                    self.notice = Some("The agent didn't take this message.".into());
                    (QueueState::Failed, Some(draft))
                }
                Receipt::Unknown => (QueueState::DeliveryUnknown, None),
            };
            self.settle(id, state, returned, cx);
        }
        if ended && let Some((id, _)) = self.queue.sent.take() {
            self.settle(id, QueueState::DeliveryUnknown, None, cx);
        }
        for draft in unread {
            self.requeue_front(draft, cx);
        }
    }

    /// The run is replaced or stops: what it was delivering is uncertain.
    pub(super) fn abandon_dispatch(&mut self, cx: &mut Context<Self>) {
        self.settle_receipts(cx);
        if let Some((id, _)) = self.queue.sent.take() {
            self.settle(id, QueueState::DeliveryUnknown, None, cx);
        }
    }

    /// Records the end of an attempt; a failed message returns to the composer.
    fn settle(
        &mut self,
        message: String,
        state: QueueState,
        returned: Option<Draft>,
        cx: &mut Context<Self>,
    ) {
        let (Some(client), Some(chat)) = (self.client(cx), self.settings.id.clone()) else {
            return;
        };
        self.queue.busy += 1;
        cx.spawn(async move |this, cx| {
            let mut settled = Err("The queue couldn't be read.".to_owned());
            for _ in 0..RETRIES {
                let Ok(queue) = client.load_runtime_queue(chat.clone()).await else {
                    break;
                };
                settled = client
                    .transition_runtime_queue(
                        chat.clone(),
                        message.clone(),
                        queue.revision(),
                        QueueState::Attempting,
                        state,
                    )
                    .await
                    .map_err(error);
                if settled.is_ok() {
                    break;
                }
            }
            let queue = client.load_runtime_queue(chat).await;
            let _ = this.update(cx, |this, cx| {
                this.queue.busy -= 1;
                if let Ok(queue) = queue {
                    this.queue.state = Some(queue);
                }
                if let Err(error) = settled {
                    // Left attempting: the next start reports it as delivery unknown.
                    this.notice = Some(format!("Delivery state couldn't be saved: {error}"));
                }
                if let Some(draft) = returned {
                    cx.emit(ChatRuntimeEvent::DraftReturned(draft));
                }
                this.dispatch(cx);
                cx.emit(ChatStateChanged);
                cx.notify();
            });
        })
        .detach();
    }

    /// A steer the agent never read waits again, first in line.
    fn requeue_front(&mut self, draft: Draft, cx: &mut Context<Self>) {
        let (Some(chat), Some(record), Some(client)) = (
            self.settings.id.clone(),
            self.queue.record.clone(),
            self.client(cx),
        ) else {
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            return;
        };
        let message = uuid::Uuid::new_v4().to_string();
        let key = CommandKey::local(uuid::Uuid::new_v4().to_string()).expect("UUID");
        let Some((input, queued)) = QueueDraft::from(draft.clone()).command_for_message(
            key.clone(),
            &chat,
            message.clone(),
            record.send_context(),
        ) else {
            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            return;
        };
        let accepted = self.accept_message(input, message.clone(), queued, cx);
        cx.spawn(async move |this, cx| {
            if accepted.await.is_err() {
                let _ = this.update(cx, |_, cx| cx.emit(ChatRuntimeEvent::DraftReturned(draft)));
                return;
            }
            let _ = client.acknowledge_runtime_command(key).await;
            let first = this
                .read_with(cx, |this, _| this.first_queued())
                .ok()
                .flatten();
            if let Some(first) = first.filter(|first| *first != message) {
                let _ = this.update(cx, |this, cx| {
                    this.move_queued(message, first, cx);
                });
            }
        })
        .detach();
    }
}

/// One local revision-checked change, read back.
async fn mutate(
    client: &SettingsClient,
    chat: &str,
    revision: u64,
    mutation: QueueMutation,
) -> Result<ChatQueue, String> {
    let key = CommandKey::local(uuid::Uuid::new_v4().to_string()).expect("UUID");
    let input = mutation
        .command(key.clone(), chat, revision)
        .ok_or("This queue change is not valid.")?;
    client
        .mutate_runtime_queue(input, chat.to_owned(), revision, mutation)
        .await
        .map_err(error)?;
    let _ = client.acknowledge_runtime_command(key).await;
    client
        .load_runtime_queue(chat.to_owned())
        .await
        .map_err(error)
}

/// A queued draft as the agent gets it: host artifacts (images sent remotely) are
/// read verified, normalized like any attached image into the chat's attachment
/// folder, then every image's data is loaded. Blocking: background only.
pub(super) fn materialize(
    chat: Option<&str>,
    queued: QueueDraft,
    store: Option<std::sync::Arc<canopy_desktop::artifacts::ArtifactStore>>,
) -> Result<Draft, (Draft, String)> {
    let artifacts = queued.artifact_ids().to_vec();
    let mut draft = queued.into_draft();
    if !artifacts.is_empty() {
        let fail = |draft: Draft| {
            Err((
                draft,
                "The message's images aren't available on this host.".to_owned(),
            ))
        };
        let (Some(chat), Some(store), Ok(data)) = (
            chat,
            store,
            canopy_desktop::platform::directories::data_dir(),
        ) else {
            return fail(draft);
        };
        let dir = canopy_desktop::chat::history::attachments_dir(&data, chat);
        for id in artifacts {
            let image = store
                .read_retained_image(chat, &id)
                .map_err(|error| error.to_string())
                .and_then(|bytes| canopy_desktop::chat::attachments::prepare_image(&bytes, &dir));
            match image {
                Ok(image) => draft.attachments.push(image),
                Err(error) => return Err((draft, error)),
            }
        }
    }
    with_data(chat, draft)
}

/// `draft` with its images' data read from the chat's attachment folder (queued
/// drafts are stored without it, like saved history).
fn with_data(chat: Option<&str>, draft: Draft) -> Result<Draft, (Draft, String)> {
    use canopy_desktop::chat::attachments;
    if !attachments::needs_data(&draft.attachments) {
        return Ok(draft);
    }
    let dir = match (chat, canopy_desktop::platform::directories::data_dir()) {
        (Some(chat), Ok(data)) => canopy_desktop::chat::history::attachments_dir(&data, chat),
        _ => return Err((draft, "The chat's attachments can't be found.".into())),
    };
    match attachments::with_data(&dir, draft.attachments.clone()) {
        Ok(attachments) => Ok(Draft {
            attachments,
            ..draft
        }),
        Err(error) => Err((draft, error)),
    }
}
