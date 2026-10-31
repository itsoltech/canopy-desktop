//! App-owned settings and Tasks callbacks. The bounded queue crosses threads;
//! configuration checks and owner reservations happen in one app update.
mod edits;
mod projection;
mod tasks;
#[cfg(test)]
mod tests;
use crate::app_state::AppState;
use canopy_desktop::remote::{proto, resources::ResourceLocks, settings_rpc::*};
use futures_channel::oneshot;
use gpui_kit::*;
use std::sync::Arc;
use tonic::Status;

type Reply<T> = oneshot::Sender<Result<T, Status>>;
type Committed = oneshot::Receiver<Result<(), String>>;
#[derive(Clone)]
pub struct ControlBridge {
    sender: async_channel::Sender<Op>,
    secrets: Arc<dyn SettingsSecrets>,
    client: Option<canopy_desktop::settings::SettingsClient>,
}
#[expect(
    clippy::large_enum_variant,
    reason = "The control queue is bounded to 64 operations and carries owned settings edits without an extra allocation."
)]
enum Op {
    Preferences(
        canopy_desktop::command_bus::PreparedCommand,
        Reply<canopy_desktop::host::CommandReceipt>,
    ),
    Snapshot(Reply<proto::HostSettings>),
    Edit(edits::Edit, Option<u64>, Reply<()>),
    Filter(
        Option<canopy_desktop::integrations::filters::TaskFilter>,
        String,
        Reply<()>,
    ),
    Worktree(
        proto::WorkspaceRef,
        canopy_desktop::integrations::TaskRef,
        String,
        tokio::runtime::Handle,
        Reply<String>,
    ),
}
struct ControlState {
    observers: Vec<Subscription>,
}
impl ControlState {
    fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }
    fn revision(&mut self, cx: &mut App) -> u64 {
        let settings = cx.global::<AppState>().settings.clone();
        settings.update(cx, |settings, cx| settings.settings_revision(cx))
    }
    fn check(&mut self, expected: Option<u64>, cx: &mut App) -> Result<(), Status> {
        let revision = self.revision(cx);
        if expected.is_some_and(|expected| expected != revision) {
            return Err(Status::aborted("Settings changed; reload before saving."));
        }
        let app = cx.global::<AppState>();
        let settings = app.settings.read(cx);
        let tools = app.tools.read(cx);
        let integrations = app.integrations.read(cx);
        // One opaque revision covers all three owners. An in-flight local commit
        // must not slip past a CAS against its previous snapshot.
        if !settings.ready
            || settings.saving
            || settings.quitting
            || !tools.ready
            || tools.saving
            || tools.quitting
            || !integrations.ready
            || integrations.busy
        {
            return Err(Status::resource_exhausted("Host configuration is busy."));
        }
        Ok(())
    }
}
impl ControlBridge {
    pub fn install(secrets: Arc<dyn SettingsSecrets>, locks: ResourceLocks, cx: &mut App) -> Self {
        let client = cx.global::<AppState>().settings.read(cx).client();
        let (sender, receiver) = async_channel::bounded(64);
        let state = cx.new(|cx: &mut Context<ControlState>| {
            let app = cx.global::<AppState>().clone();
            let mut state = ControlState::new();
            state.revision(cx);
            state.observers = vec![
                cx.observe(&app.settings, |state, _, cx| {
                    state.revision(cx);
                }),
                cx.observe(&app.tools, |state, _, cx| {
                    state.revision(cx);
                }),
                cx.observe(&app.integrations, |state, _, cx| {
                    state.revision(cx);
                }),
            ];
            state
        });
        cx.spawn(async move |cx| {
            while let Ok(op) = receiver.recv().await {
                cx.update(|cx| match op {
                    Op::Preferences(prepared, reply) => {
                        // Reserve the live owners here. The bus checks CAS after
                        // dedupe in its transaction: a concurrent identical retry
                        // may have committed since this request was prepared.
                        let result = state.update(cx, |state, cx| state.check(None, cx));
                        match result {
                            Err(error) => {
                                let _ = reply.send(Err(error));
                            }
                            Ok(()) => {
                                let settings = cx.global::<AppState>().settings.clone();
                                settings.update(cx, |settings, cx| {
                                    settings.start_prepared_preferences(prepared, reply, cx)
                                });
                            }
                        }
                    }
                    Op::Snapshot(reply) => {
                        let revision = state.update(cx, |state, cx| state.revision(cx));
                        let _ = reply.send(projection::snapshot(revision, cx));
                    }
                    Op::Edit(edit, expected, reply) => {
                        let result = state
                            .update(cx, |state, cx| state.check(expected, cx))
                            .and_then(|_| edits::apply(edit, cx));
                        finish(result, reply, cx);
                    }
                    Op::Filter(filter, id, reply) => {
                        finish(tasks::filter(filter, id, cx), reply, cx)
                    }
                    Op::Worktree(workspace, task, branch, runtime, reply) => {
                        tasks::worktree(workspace, task, branch, runtime, locks.clone(), reply, cx)
                    }
                });
            }
        })
        .detach();
        Self {
            sender,
            secrets,
            client,
        }
    }
    async fn request<T>(&self, op: impl FnOnce(Reply<T>) -> Op) -> Result<T, Status> {
        let (tx, rx) = oneshot::channel();
        self.sender.try_send(op(tx)).map_err(|_| {
            Status::resource_exhausted("Host control queue is unavailable or full.")
        })?;
        rx.await
            .map_err(|_| Status::unknown("Host owner stopped before reporting the outcome."))?
    }
    async fn edit(&self, edit: edits::Edit, expected: Option<u64>) -> Result<(), Status> {
        self.request(|reply| Op::Edit(edit, expected, reply)).await
    }
}
fn finish(result: Result<Committed, Status>, reply: Reply<()>, cx: &mut App) {
    match result {
        Err(error) => {
            let _ = reply.send(Err(error));
        }
        Ok(done) => {
            cx.spawn(async move |_| {
                let result = done
                    .await
                    .map_err(|_| Status::unknown("Configuration outcome requires reconciliation."))?
                    .map_err(|_| {
                        Status::failed_precondition("Host configuration could not be committed.")
                    });
                let _ = reply.send(result);
                Ok::<_, Status>(())
            })
            .detach();
        }
    }
}
fn invalid(error: impl ToString) -> Status {
    Status::invalid_argument(error.to_string())
}
fn busy(error: impl ToString) -> Status {
    Status::resource_exhausted(error.to_string())
}
