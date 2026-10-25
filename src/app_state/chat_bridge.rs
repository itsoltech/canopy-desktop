//! The seam between the remote server and the chat runtimes it serves. The server
//! holds a `ChatBridge` (Send + Sync); every operation goes through a bounded channel
//! to the app thread, which calls the same typed handlers the desktop uses. Nothing
//! here owns network, auth or encoding: the caller authenticated the device and builds
//! command keys and inputs. Subscriptions stream a chat's changes to one client each,
//! independent of views; a slow client loses its stream (resync), never the runtime.
use super::project_commands::{self, ProjectClosePreflight, WorkspaceSummary};
use super::{
    AppState, ChatRuntime, ChatRuntimeEvent,
    chats::{
        ChatCommand, ChatSnapshot, ChatStateChanged, ChatSummary, DeletePreflight, EditPreview,
        ForkChat, ForkReplacement, NewChat, RemoteKey, SnapshotEpoch,
    },
};
use canopy_desktop::{
    chat::transcript::{Change, Item},
    host::{CommandInput, CommandReceipt, QueueDraft, QueueMutation},
    state::workspace::WorkspaceId,
};
use futures_channel::oneshot;
use gpui_kit::*;
use std::{collections::HashMap, sync::Arc};

/// Operations waiting for the app thread; beyond this the bridge answers busy.
const PENDING: usize = 64;
/// Events buffered per subscription before the client must resync.
const EVENTS: usize = 256;
/// Newest transcript items copied into a subscription's first snapshot; older ones
/// are read as history pages (SYNC-05), never cloned whole per subscriber.
pub const SNAPSHOT_ITEMS: usize = 100;

type Reply<T> = oneshot::Sender<Result<T, String>>;

/// What a subscribed client receives, in order.
#[derive(Clone, Debug)]
pub enum ChatEvent {
    /// First, once: the state and the newest `SNAPSHOT_ITEMS` of the conversation
    /// held at subscription (`true`: older items exist), taken in the same app turn
    /// the subscription starts, so no change falls between it and the following events.
    Snapshot(Box<(ChatSnapshot, Vec<SnapshotEpoch>, bool)>),
    /// The chat's state after each status change.
    State(Box<ChatSnapshot>),
    /// Items of running session `serial` inserted or updated, with their index.
    Items {
        serial: u64,
        items: Vec<(usize, Item)>,
    },
    /// The running session was replaced (Restart, Edit) or the conversation reset.
    SessionReplaced,
    /// The process stops; live presentation of it ends.
    Stopping,
    /// Saved history rows of this chat committed together with checkpoint
    /// `checkpoint` (sent only after the commit; item hashes are of the stored JSON).
    Committed(canopy_desktop::settings::HistoryCheckpoint),
}

/// The host's catalog as a client lists it: projects and chats, no transcripts.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogEvent {
    pub workspaces: Vec<WorkspaceSummary>,
    pub chats: Vec<ChatSummary>,
    /// The catalog records of `chats` (authority root, account, history epoch).
    pub records: Vec<canopy_desktop::host::ChatRecord>,
}

enum Op {
    Summaries(Reply<Vec<ChatSummary>>),
    Workspaces(Reply<Vec<WorkspaceSummary>>),
    WatchCatalog(CatalogSlot, Reply<()>),
    OpenProject(RemoteKey, std::path::PathBuf, Reply<CommandReceipt>),
    ClosePreflight(WorkspaceId, Reply<ProjectClosePreflight>),
    CloseProject(RemoteKey, ProjectClosePreflight, Reply<CommandReceipt>),
    Snapshot(String, Reply<ChatSnapshot>),
    /// Edit preview of chat, run, history part and user message.
    EditPreview(String, u64, u64, u64, Reply<EditPreview>),
    Subscribe(String, async_channel::Sender<ChatEvent>, Reply<u64>),
    Unsubscribe(u64),
    Observers(u64, usize),
    Send(
        String,
        CommandInput,
        String,
        QueueDraft,
        Reply<CommandReceipt>,
    ),
    ChangeQueue(
        String,
        CommandInput,
        u64,
        QueueMutation,
        Reply<CommandReceipt>,
    ),
    Command(String, RemoteKey, ChatCommand, Reply<CommandReceipt>),
    Steer(
        String,
        CommandInput,
        String,
        QueueDraft,
        u64,
        Reply<CommandReceipt>,
    ),
    Create(RemoteKey, NewChat, Reply<CommandReceipt>),
    Fork(
        RemoteKey,
        ForkChat,
        Option<ForkReplacement>,
        Reply<CommandReceipt>,
    ),
    DeletePreflight(String, Reply<DeletePreflight>),
    Delete(RemoteKey, DeletePreflight, Reply<CommandReceipt>),
}

/// The server's handle to the chat runtimes (clone freely, any thread).
#[derive(Clone)]
pub struct ChatBridge(async_channel::Sender<Op>);

/// One live subscription on the app thread.
struct Watch {
    runtime: Entity<ChatRuntime>,
    events: async_channel::Sender<ChatEvent>,
    /// Clients this subscription stands for (LIFE-09: they keep the process).
    observers: usize,
    _changes: Subscription,
    _state: Subscription,
    _committed: Subscription,
}

/// Owns the subscriptions; lives on the app thread.
struct Hub {
    next: u64,
    watches: HashMap<u64, Watch>,
    /// Catalog streams: each holds only the newest catalog (one shared `Arc`).
    catalogs: Vec<CatalogSlot>,
    /// The catalog last sent and the projects it was resolved from: an app change
    /// that leaves these unchanged (a streamed text token) builds and sends nothing.
    last: Option<(Arc<CatalogEvent>, Vec<project_commands::Listed>)>,
    refreshing: bool,
    stale: bool,
    git_changed: bool,
    _observers: Vec<Subscription>,
}

/// One catalog stream's mailbox: the newest catalog only, replaced (never queued) by
/// later ones, plus a coalesced wake. A slow reader skips intermediate versions.
#[derive(Clone)]
pub(crate) struct CatalogSlot {
    latest: Arc<std::sync::Mutex<Option<Arc<CatalogEvent>>>>,
    wake: async_channel::Sender<()>,
}
impl CatalogSlot {
    fn put(&self, event: &Arc<CatalogEvent>) {
        *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(event.clone());
        // Full: a wake is already pending and reads the replaced value.
        let _ = self.wake.try_send(());
    }
}

/// The receiving end of `ChatBridge::watch_catalog`.
pub struct CatalogWatch {
    latest: Arc<std::sync::Mutex<Option<Arc<CatalogEvent>>>>,
    wake: async_channel::Receiver<()>,
}
impl CatalogWatch {
    /// The newest catalog not read yet; waits for the next one. Fails when Canopy closes.
    pub async fn recv(&self) -> Result<Arc<CatalogEvent>, async_channel::RecvError> {
        loop {
            if let Some(event) = self.take() {
                return Ok(event);
            }
            self.wake.recv().await?;
        }
    }
    fn take(&self) -> Option<Arc<CatalogEvent>> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}
impl Drop for CatalogWatch {
    fn drop(&mut self) {
        self.wake.close();
    }
}

/// Sends the current catalog to every catalog stream when it changed; changes during
/// the send are coalesced into one more.
fn refresh_catalog(hub: Entity<Hub>, cx: &mut App) {
    let start = hub.update(cx, |hub, _| {
        hub.catalogs.retain(|slot| !slot.wake.is_closed());
        if hub.catalogs.is_empty() {
            hub.last = None;
            return false;
        }
        if hub.refreshing {
            hub.stale = true;
            return false;
        }
        hub.refreshing = true;
        true
    });
    if !start {
        return;
    }
    let chats = cx.global::<AppState>().chats.read(cx).chat_summaries(cx);
    let projects = project_commands::listed(cx);
    let git_changed = hub.update(cx, |hub, _| std::mem::take(&mut hub.git_changed));
    let unchanged = !git_changed
        && hub.read(cx).last.as_ref().is_some_and(|(last, listed)| {
            *listed == projects
                && last.chats == chats
                && cx
                    .global::<AppState>()
                    .settings
                    .read(cx)
                    .chat_records()
                    .eq(last.records.iter())
        });
    if unchanged {
        let again = hub.update(cx, |hub, _| {
            hub.refreshing = false;
            std::mem::take(&mut hub.stale)
        });
        if again {
            refresh_catalog(hub, cx);
        }
        return;
    }
    let records = cx
        .global::<AppState>()
        .settings
        .read(cx)
        .chat_records()
        .cloned()
        .collect();
    let listed = projects.clone();
    let resolve = cx.background_spawn(async move { project_commands::resolve_listed(projects) });
    cx.spawn(async move |cx| {
        let workspaces = resolve.await;
        let again = hub.update(cx, |hub, _| {
            let event = CatalogEvent {
                workspaces,
                chats,
                records,
            };
            // The same catalog again (a folder resolved as before) is not resent.
            if hub.last.as_ref().is_none_or(|(last, _)| **last != event) {
                let event = Arc::new(event);
                for slot in &hub.catalogs {
                    slot.put(&event);
                }
                hub.last = Some((event, listed));
            } else if let Some((_, last)) = &mut hub.last {
                *last = listed;
            }
            hub.refreshing = false;
            std::mem::take(&mut hub.stale)
        });
        if again {
            cx.update(|cx| refresh_catalog(hub, cx));
        }
    })
    .detach();
}
impl Hub {
    /// Drops subscription `id`: the runtime loses one observer, nothing else changes
    /// (LIFE-02: a lost client never interrupts or deletes).
    fn drop_watch(&mut self, id: u64, cx: &mut App) {
        if let Some(watch) = self.watches.remove(&id) {
            watch.events.close();
            watch.runtime.update(cx, |runtime, cx| {
                for _ in 0..watch.observers {
                    runtime.unobserve(cx);
                }
            });
        }
    }
    /// Subscription `id` now stands for `count` clients (a shared projection keeps one
    /// subscription for all its clients and none while it waits for a reconnect).
    fn set_observers(&mut self, id: u64, count: usize, cx: &mut App) {
        let Some(watch) = self.watches.get_mut(&id) else {
            return;
        };
        let before = std::mem::replace(&mut watch.observers, count);
        watch.runtime.update(cx, |runtime, cx| {
            for _ in count..before {
                runtime.unobserve(cx);
            }
            for _ in before..count {
                runtime.observe(cx);
            }
        });
    }
}

/// Starts the app-side owner of the bridge.
pub fn install(cx: &mut App) -> ChatBridge {
    let (sender, ops) = async_channel::bounded(PENDING);
    let app = cx.global::<AppState>().clone();
    let hub = cx.new(|cx| {
        // Catalog streams follow the projects, the chat registry (runtime status
        // included) and the catalog records; no polling.
        // Deferred: the refresh updates the hub itself.
        fn changed<T>(_: &mut Hub, _: Entity<T>, cx: &mut Context<Hub>) {
            let hub = cx.entity();
            cx.defer(move |cx| refresh_catalog(hub, cx));
        }
        let observers = vec![
            cx.observe(&app.git, |this: &mut Hub, _, cx| {
                this.git_changed = true;
                let hub = cx.entity();
                cx.defer(move |cx| refresh_catalog(hub, cx));
            }),
            cx.observe(&app.projects, changed),
            cx.observe(&app.chats, changed),
            cx.observe(&app.settings, changed),
            // Terminal panes and device terminals, their start, exit and cleanup.
            cx.observe(&app.terminals, changed),
        ];
        Hub {
            next: 0,
            watches: HashMap::new(),
            catalogs: vec![],
            last: None,
            refreshing: false,
            stale: false,
            git_changed: false,
            _observers: observers,
        }
    });
    cx.spawn(async move |cx| {
        while let Ok(op) = ops.recv().await {
            let hub = hub.clone();
            cx.update(|cx| serve(op, hub, cx));
        }
    })
    .detach();
    ChatBridge(sender)
}

fn runtime(chat: &str, cx: &mut App) -> Result<Entity<ChatRuntime>, String> {
    let chats = cx.global::<AppState>().chats.clone();
    chats.update(cx, |chats, cx| chats.ensure_runtime_for_chat(chat, cx))
}

/// Answers `reply` when `task` ends.
fn forward<T: 'static>(task: Task<Result<T, String>>, reply: Reply<T>, cx: &mut App) {
    cx.spawn(async move |_| {
        let _ = reply.send(task.await);
    })
    .detach();
}

fn serve(op: Op, hub: Entity<Hub>, cx: &mut App) {
    let chats = cx.global::<AppState>().chats.clone();
    match op {
        Op::Summaries(reply) => {
            let _ = reply.send(Ok(chats.read(cx).chat_summaries(cx)));
        }
        Op::Workspaces(reply) => {
            let projects = project_commands::listed(cx);
            cx.background_spawn(async move {
                let _ = reply.send(Ok(project_commands::resolve_listed(projects)));
            })
            .detach();
        }
        Op::WatchCatalog(slot, reply) => {
            hub.update(cx, |hub, _| {
                // A new stream starts from the catalog already built (shared, not
                // copied); the refresh below sends a newer one when it changed.
                if let Some((last, _)) = &hub.last {
                    slot.put(last);
                }
                hub.catalogs.push(slot);
            });
            let _ = reply.send(Ok(()));
            refresh_catalog(hub, cx);
        }
        Op::OpenProject(key, path, reply) => {
            let task = project_commands::open_project(key, path, cx);
            forward(task, reply, cx);
        }
        Op::ClosePreflight(id, reply) => {
            let _ = reply.send(project_commands::close_preflight(id, cx));
        }
        Op::CloseProject(key, preflight, reply) => {
            let task = project_commands::close_project(key, preflight, cx);
            forward(task, reply, cx);
        }
        Op::Snapshot(chat, reply) => {
            let _ = reply.send(runtime(&chat, cx).map(|r| r.read(cx).snapshot(cx)));
        }
        Op::EditPreview(chat, run, history_epoch, item, reply) => match runtime(&chat, cx) {
            Ok(runtime) => {
                let task = runtime.update(cx, |r, cx| {
                    r.remote_edit_preview(run, history_epoch, item, cx)
                });
                cx.spawn(async move |_| {
                    let _ = reply.send(Ok(task.await));
                })
                .detach();
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        },
        Op::Subscribe(chat, events, reply) => {
            let _ = reply.send(subscribe(&chat, events, hub, cx));
        }
        Op::Unsubscribe(id) => hub.update(cx, |hub, cx| hub.drop_watch(id, cx)),
        Op::Observers(id, count) => hub.update(cx, |hub, cx| hub.set_observers(id, count, cx)),
        Op::Send(chat, input, message, draft, reply) => match runtime(&chat, cx) {
            Ok(runtime) => {
                let task = runtime.update(cx, |r, cx| r.accept_message(input, message, draft, cx));
                forward(task, reply, cx);
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        },
        Op::ChangeQueue(chat, input, revision, mutation, reply) => match runtime(&chat, cx) {
            Ok(runtime) => {
                let task =
                    runtime.update(cx, |r, cx| r.change_queue(input, revision, mutation, cx));
                forward(task, reply, cx);
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        },
        Op::Steer(chat, input, message, draft, run, reply) => match runtime(&chat, cx) {
            Ok(runtime) => {
                let task =
                    runtime.update(cx, |r, cx| r.accept_steer(input, message, draft, run, cx));
                forward(task, reply, cx);
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        },
        Op::Command(chat, key, command, reply) => match runtime(&chat, cx) {
            Ok(runtime) => {
                let task = runtime.update(cx, |r, cx| match command {
                    ChatCommand::Start => r.remote_start(key, cx),
                    ChatCommand::Interrupt { run } => r.remote_interrupt(key, run, cx),
                    ChatCommand::Restart { run } => r.remote_restart(key, run, cx),
                    ChatCommand::ResumeQueue => r.remote_resume_queue(key, cx),
                    ChatCommand::Steer { message } => r.remote_steer(key, message, cx),
                    ChatCommand::Respond { run, item, answer } => {
                        r.remote_respond(key, run, item, answer, cx)
                    }
                    ChatCommand::SetOption { id, value } => r.remote_set_option(key, id, value, cx),
                    goal @ (ChatCommand::PauseGoal
                    | ChatCommand::ResumeGoal
                    | ChatCommand::ClearGoal) => r.remote_goal(key, goal, cx),
                    ChatCommand::StopBackground { run, call } => {
                        r.remote_stop_background(key, run, call, cx)
                    }
                    ChatCommand::Rename { title } => r.remote_rename(key, title, cx),
                    edit @ ChatCommand::Edit { .. } => r.remote_edit(key, edit, cx),
                    ChatCommand::RunShell { run, command } => {
                        r.remote_run_shell(key, run, command, cx)
                    }
                    ChatCommand::StopShell {
                        run,
                        history_epoch,
                        item,
                    } => r.remote_stop_shell(key, run, history_epoch, item, cx),
                });
                forward(task, reply, cx);
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        },
        Op::Create(key, new, reply) => {
            let task = chats.update(cx, |chats, cx| chats.create_chat(key, new, cx));
            forward(task, reply, cx);
        }
        Op::Fork(key, fork, replacement, reply) => {
            let task = chats.update(cx, |chats, cx| chats.fork_chat(key, fork, replacement, cx));
            forward(task, reply, cx);
        }
        Op::DeletePreflight(chat, reply) => {
            let _ = reply.send(chats.read(cx).delete_preflight(&chat, cx));
        }
        Op::Delete(key, preflight, reply) => {
            let task = chats.update(cx, |chats, cx| chats.delete_chat(key, preflight, cx));
            forward(task, reply, cx);
        }
    }
}

/// Starts streaming chat `chat` to `events`: its state first, then its changes. The
/// runtime counts the client as an observer (it doesn't sleep while watched).
fn subscribe(
    chat: &str,
    events: async_channel::Sender<ChatEvent>,
    hub: Entity<Hub>,
    cx: &mut App,
) -> Result<u64, String> {
    let runtime = runtime(chat, cx)?;
    let id = hub.update(cx, |hub, _| {
        hub.next += 1;
        hub.next
    });
    let initial = {
        let runtime = runtime.read(cx);
        let (epochs, earlier) = runtime.epochs_tail(SNAPSHOT_ITEMS);
        (runtime.snapshot(cx), epochs, earlier)
    };
    events
        .try_send(ChatEvent::Snapshot(Box::new(initial)))
        .map_err(|_| "The subscription closed.".to_owned())?;
    // A full or closed stream ends the subscription: the client resyncs from a
    // snapshot; the runtime goes on.
    let send = {
        let hub = hub.downgrade();
        move |event: ChatEvent, cx: &mut App| {
            let stalled = hub
                .read_with(cx, |hub, _| {
                    hub.watches
                        .get(&id)
                        .is_some_and(|watch| watch.events.try_send(event).is_err())
                })
                .unwrap_or(false);
            if stalled {
                let _ = hub.update(cx, |hub, cx| hub.drop_watch(id, cx));
            }
        }
    };
    let on_change = send.clone();
    let changes = cx.subscribe(&runtime, move |runtime, event: &ChatRuntimeEvent, cx| {
        let event = match event {
            ChatRuntimeEvent::Changed { serial, changes } => {
                let runtime = runtime.read(cx);
                let Some(transcript) = runtime.transcript_of(*serial) else {
                    return;
                };
                let items: Vec<_> = changes
                    .iter()
                    .filter_map(|change| {
                        let (Change::Inserted(index) | Change::Updated(index)) = *change;
                        Some((index, transcript.get(index)?.clone()))
                    })
                    .collect();
                if items.is_empty() {
                    return;
                }
                ChatEvent::Items {
                    serial: *serial,
                    items,
                }
            }
            ChatRuntimeEvent::Archived | ChatRuntimeEvent::Reset | ChatRuntimeEvent::Restored => {
                ChatEvent::SessionReplaced
            }
            ChatRuntimeEvent::Stopping => ChatEvent::Stopping,
            // A draft returned to a local composer is not a remote event.
            ChatRuntimeEvent::DraftReturned(_) => return,
        };
        on_change(event, cx);
    });
    let on_state = send.clone();
    let state = cx.subscribe(&runtime, move |runtime, _: &ChatStateChanged, cx| {
        let snapshot = runtime.read(cx).snapshot(cx);
        on_state(ChatEvent::State(Box::new(snapshot)), cx);
    });
    let history = cx.global::<AppState>().chat_history.clone();
    let chat_id = chat.to_owned();
    let committed = cx.subscribe(
        &history,
        move |_, event: &super::chat_history::HistoryCommitted, cx| {
            if event.0.chat() == chat_id {
                send(ChatEvent::Committed(event.0.clone()), cx);
            }
        },
    );
    runtime.update(cx, |runtime, cx| runtime.observe(cx));
    hub.update(cx, |hub, _| {
        hub.watches.insert(
            id,
            Watch {
                runtime,
                events,
                observers: 1,
                _changes: changes,
                _state: state,
                _committed: committed,
            },
        )
    });
    Ok(id)
}

impl ChatBridge {
    async fn ask<T>(&self, op: impl FnOnce(Reply<T>) -> Op) -> Result<T, String> {
        let (reply, answer) = oneshot::channel();
        self.0
            .try_send(op(reply))
            .map_err(|_| "resource_exhausted: the host is busy; retry.".to_owned())?;
        answer.await.map_err(|_| "Canopy is closing.".to_owned())?
    }

    pub async fn summaries(&self) -> Result<Vec<ChatSummary>, String> {
        self.ask(Op::Summaries).await
    }
    /// Open projects and worktrees with their canonical roots (read on a worker).
    pub async fn workspaces(&self) -> Result<Vec<WorkspaceSummary>, String> {
        self.ask(Op::Workspaces).await
    }
    /// The whole catalog now, then the newest one after each change. A slow reader
    /// skips versions (one shared catalog per stream, never a queue of them).
    pub async fn watch_catalog(&self) -> Result<CatalogWatch, String> {
        let latest = Arc::default();
        let (wake, receiver) = async_channel::bounded(1);
        let slot = CatalogSlot {
            latest: Arc::clone(&latest),
            wake,
        };
        self.ask(|reply| Op::WatchCatalog(slot, reply)).await?;
        Ok(CatalogWatch {
            latest,
            wake: receiver,
        })
    }
    /// Opens a folder as a project without changing the host's selection.
    pub async fn open_project(
        &self,
        key: RemoteKey,
        path: std::path::PathBuf,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::OpenProject(key, path, reply)).await
    }
    pub async fn close_project_preflight(
        &self,
        workspace: WorkspaceId,
    ) -> Result<ProjectClosePreflight, String> {
        self.ask(|reply| Op::ClosePreflight(workspace, reply)).await
    }
    pub async fn close_project(
        &self,
        key: RemoteKey,
        preflight: ProjectClosePreflight,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::CloseProject(key, preflight, reply))
            .await
    }
    pub async fn snapshot(&self, chat: String) -> Result<ChatSnapshot, String> {
        self.ask(|reply| Op::Snapshot(chat, reply)).await
    }
    /// What editing user message `item` of history part `history_epoch` in run `run`
    /// would do (waits for the agent's file dry run, bounded).
    pub async fn edit_preview(
        &self,
        chat: String,
        run: u64,
        history_epoch: u64,
        item: u64,
    ) -> Result<EditPreview, String> {
        self.ask(|reply| Op::EditPreview(chat, run, history_epoch, item, reply))
            .await
    }
    /// Streams chat `chat`; returns the subscription ID and its bounded receiver. The
    /// stream ends when the client falls behind (resync) or the chat goes away.
    pub async fn subscribe(
        &self,
        chat: String,
    ) -> Result<(u64, async_channel::Receiver<ChatEvent>), String> {
        let (events, receiver) = async_channel::bounded(EVENTS);
        let id = self.ask(|reply| Op::Subscribe(chat, events, reply)).await?;
        Ok((id, receiver))
    }
    /// Subscription `id` stands for `count` connected clients; the runtime counts
    /// them as observers (`0`: it may sleep while a projection waits for a reconnect).
    /// Waits for room like `unsubscribe`: the count must not be lost.
    pub async fn set_observers(&self, id: u64, count: usize) {
        let _ = self.0.send(Op::Observers(id, count)).await;
    }
    /// Ends a subscription (Detach, cancelled RPC). Waits for room instead of being
    /// refused as busy: a subscription must not outlive its client.
    pub async fn unsubscribe(&self, id: u64) {
        let _ = self.0.send(Op::Unsubscribe(id)).await;
    }
    pub async fn send(
        &self,
        chat: String,
        input: CommandInput,
        message: String,
        draft: QueueDraft,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::Send(chat, input, message, draft, reply))
            .await
    }
    pub async fn change_queue(
        &self,
        chat: String,
        input: CommandInput,
        revision: u64,
        mutation: QueueMutation,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::ChangeQueue(chat, input, revision, mutation, reply))
            .await
    }
    /// Steers new text into run `run`: `input` from `QueueDraft::command_for_steer`
    /// (one Send command bound to the run). Another run: `stale_run`, nothing accepted.
    pub async fn steer(
        &self,
        chat: String,
        input: CommandInput,
        message: String,
        draft: QueueDraft,
        run: u64,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::Steer(chat, input, message, draft, run, reply))
            .await
    }
    /// A named chat command under the device's command key.
    pub async fn command(
        &self,
        chat: String,
        key: RemoteKey,
        command: ChatCommand,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::Command(chat, key, command, reply))
            .await
    }
    pub async fn create(&self, key: RemoteKey, new: NewChat) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::Create(key, new, reply)).await
    }
    /// Forks a chat into a new one without a tab (its UUID picked by the client),
    /// with `replacement` as its first queued message (an edit made as a fork).
    pub async fn fork(
        &self,
        key: RemoteKey,
        fork: ForkChat,
        replacement: Option<ForkReplacement>,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::Fork(key, fork, replacement, reply))
            .await
    }
    pub async fn delete_preflight(&self, chat: String) -> Result<DeletePreflight, String> {
        self.ask(|reply| Op::DeletePreflight(chat, reply)).await
    }
    pub async fn delete(
        &self,
        key: RemoteKey,
        preflight: DeletePreflight,
    ) -> Result<CommandReceipt, String> {
        self.ask(|reply| Op::Delete(key, preflight, reply)).await
    }
}

/// A stand-in for the app side in tests of the bridge's clients: subscriptions are
/// handed to the test (which plays the runtime), observer counts and unsubscribes
/// are recorded, every other operation is refused as if Canopy were closing.
#[cfg(test)]
pub(crate) mod fake {
    use super::{ChatBridge, ChatEvent, Op};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq)]
    pub(crate) enum Call {
        Observers(u64, usize),
        Unsubscribe(u64),
    }
    pub(crate) struct FakeApp {
        pub subscribed: async_channel::Receiver<(u64, String, async_channel::Sender<ChatEvent>)>,
        pub calls: Arc<Mutex<Vec<Call>>>,
    }
    impl FakeApp {
        pub fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
    }
    pub(crate) fn bridge() -> (ChatBridge, FakeApp) {
        let (sender, ops) = async_channel::bounded(super::PENDING);
        let (handed, subscribed) = async_channel::unbounded();
        let calls = Arc::new(Mutex::new(vec![]));
        let record = calls.clone();
        std::thread::spawn(move || {
            futures_lite::future::block_on(async move {
                let mut next = 0;
                while let Ok(op) = ops.recv().await {
                    match op {
                        Op::Subscribe(chat, events, reply) => {
                            next += 1;
                            let _ = handed.try_send((next, chat, events));
                            let _ = reply.send(Ok(next));
                        }
                        Op::Observers(id, count) => {
                            record.lock().unwrap().push(Call::Observers(id, count))
                        }
                        Op::Unsubscribe(id) => record.lock().unwrap().push(Call::Unsubscribe(id)),
                        _ => {}
                    }
                }
            })
        });
        (ChatBridge(sender), FakeApp { subscribed, calls })
    }
}

#[cfg(test)]
mod tests {
    use super::{CatalogEvent, CatalogSlot, CatalogWatch, ChatEvent, install};
    use std::sync::Arc;

    #[test]
    fn a_slow_catalog_reader_gets_only_the_newest_shared_version() {
        let latest = Arc::default();
        let (wake, receiver) = async_channel::bounded(1);
        let slot = CatalogSlot {
            latest: Arc::clone(&latest),
            wake,
        };
        let watch = CatalogWatch {
            latest,
            wake: receiver,
        };
        let versions: Vec<_> = (0..300)
            .map(|_| {
                Arc::new(CatalogEvent {
                    workspaces: vec![],
                    chats: vec![],
                    records: vec![],
                })
            })
            .collect();
        for version in &versions {
            slot.put(version);
        }
        let read = futures_lite::future::block_on(watch.recv()).unwrap();
        assert!(
            Arc::ptr_eq(&read, versions.last().unwrap()),
            "shared, not copied"
        );
        assert!(
            watch.take().is_none(),
            "older versions were replaced, not queued"
        );
        drop(watch);
        assert!(slot.wake.is_closed(), "the host drops a closed stream");
    }
    use crate::app_state::AppState;
    use canopy_desktop::chat::{config::ChatSettings, session::ChatSession};
    use canopy_desktop::host::ChatRecord;
    use canopy_desktop::settings::{ChatHistoryBatch, SettingsClient};
    use canopy_desktop::state::workspace::{PaneKind, PaneMetadata, Workspace};
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    async fn a_subscription_starts_with_the_saved_conversation_and_counts_as_a_viewer(
        cx: &mut TestAppContext,
    ) {
        cx.background_executor.allow_parking();
        let folder = tempfile::tempdir().unwrap();
        let client = SettingsClient::create(&folder.path().join("settings.db"))
            .await
            .unwrap();
        let host = client.initialize_runtime().await.unwrap();
        let chat = ChatSettings::new_id();
        let (mut session, _) = ChatSession::with_settings(ChatSettings::default());
        session.note("Saved before the restart".into());
        let item = serde_json::to_string(&session.transcript().items()[0]).unwrap();
        client
            .save_chat_history(vec![ChatHistoryBatch {
                chat: chat.clone(),
                epochs: vec![(0, "{}".into())],
                items: vec![(0, 1, item)],
            }])
            .await
            .unwrap();
        let record = ChatRecord::new(
            chat.clone(),
            None,
            "claude".into(),
            PaneMetadata {
                kind: PaneKind::Chat,
                cwd: Some(folder.path().to_owned()),
                profile_id: Some("claude-default".into()),
                chat: Some(Box::new(ChatSettings {
                    id: Some(chat.clone()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )
        .unwrap();
        client.save_runtime_chat(record.clone()).await.unwrap();
        let (app, bridge) = cx.update(|cx| {
            let app = AppState::install_test(Workspace::new(), cx);
            app.projects.update(cx, |projects, _| projects.ready = true);
            app.settings.update(cx, |settings, _| {
                settings.install_test_client(client.clone(), host);
                settings.record_saved(record);
            });
            app.chat_history
                .update(cx, |history, cx| history.initialize(client.clone(), cx));
            (app, install(cx))
        });
        // History loads in the background; subscribing sees it once it is in.
        let runtime = cx.update(|cx| {
            app.chats
                .update(cx, |chats, cx| chats.ensure_runtime_for_chat(&chat, cx))
                .unwrap()
        });
        for _ in 0..500 {
            cx.run_until_parked();
            if !cx.update(|cx| runtime.read(cx).history_uncertain()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let subscribe = cx.background_executor.spawn({
            let bridge = bridge.clone();
            let chat = chat.clone();
            async move { bridge.subscribe(chat).await }
        });
        cx.run_until_parked();
        let (id, events) = subscribe.await.unwrap();
        let Ok(ChatEvent::Snapshot(initial)) = events.try_recv() else {
            panic!("a subscription starts with one snapshot");
        };
        let (state, epochs, earlier) = *initial;
        assert!(!earlier);
        assert_eq!(state.chat, chat);
        assert_eq!(epochs.len(), 1);
        assert_eq!(epochs[0].items.len(), 1, "the saved conversation is in it");
        assert_eq!(cx.update(|cx| runtime.read(cx).observers()), 1);
        // The catalog stream starts with the whole catalog, chats without a tab too.
        let watch = cx.background_executor.spawn({
            let bridge = bridge.clone();
            async move { bridge.watch_catalog().await }
        });
        cx.run_until_parked();
        let catalog = watch.await.unwrap();
        let mut first = None;
        for _ in 0..500 {
            cx.run_until_parked();
            if let Some(event) = catalog.take() {
                first = Some(event);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let first = first.expect("a catalog stream starts with the catalog");
        assert!(first.chats.iter().any(|summary| summary.chat == chat));
        // An app change that leaves the catalog as it was (a streamed token) sends
        // nothing; the stream holds no further version.
        cx.update(|cx| app.chats.update(cx, |_, cx| cx.notify()));
        for _ in 0..20 {
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            catalog.take().is_none(),
            "an unchanged catalog is not resent"
        );
        // A shared projection reports its true client count, also zero while it
        // waits for a reconnect (the chat may then sleep).
        for (count, expected) in [(3, 3), (0, 0), (2, 2)] {
            let set = cx.background_executor.spawn({
                let bridge = bridge.clone();
                async move { bridge.set_observers(id, count).await }
            });
            cx.run_until_parked();
            set.await;
            cx.run_until_parked();
            assert_eq!(cx.update(|cx| runtime.read(cx).observers()), expected);
        }
        let unsubscribe = cx
            .background_executor
            .spawn(async move { bridge.unsubscribe(id).await });
        cx.run_until_parked();
        unsubscribe.await;
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| runtime.read(cx).observers()),
            0,
            "a detached client leaves the chat as it was"
        );
        assert!(events.is_closed());
        client.shutdown().await.unwrap();
    }
}
