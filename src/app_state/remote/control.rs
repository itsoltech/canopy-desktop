//! App-owned settings and Tasks callbacks. The bounded queue crosses threads;
//! configuration checks and owner reservations happen in one app update.
mod edits;
mod projection;
mod tasks;
#[cfg(test)]
mod tests;
use crate::app_state::{AppState, SettingsState};
use canopy_desktop::{
    remote::{proto, resources::ResourceLocks, settings_rpc::*},
    settings::{Change, PreferenceKey},
};
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
}
#[expect(
    clippy::large_enum_variant,
    reason = "The control queue is bounded to 64 operations and carries owned settings edits without an extra allocation."
)]
enum Op {
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
    revision: u64,
    snapshot: Option<(
        canopy_desktop::settings::Preferences,
        canopy_desktop::state::tools::ToolCatalog,
        canopy_desktop::integrations::Config,
    )>,
}
impl ControlState {
    fn new() -> Self {
        Self {
            observers: Vec::new(),
            revision: u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap())
                .max(1),
            snapshot: None,
        }
    }
    fn revision(&mut self, cx: &App) -> u64 {
        let app = cx.global::<AppState>();
        let mut tools = app.tools.read(cx).catalog.clone();
        let mut integrations = app.integrations.read(cx).config.clone();
        // Credential cleanup bookkeeping is not an editable configuration change.
        tools.retired_credentials.clear();
        integrations.retired_credentials.clear();
        // Appearance and terminal font are device-local and not part of the
        // remote settings snapshot: a local theme change must not fail remote CAS.
        let mut preferences = app.settings.read(cx).values.clone();
        preferences.appearance = Default::default();
        preferences.terminal = Default::default();
        let current = (preferences, tools, integrations);
        if self.snapshot.as_ref().is_some_and(|old| old != &current) {
            self.revision = self.revision.wrapping_add(1).max(1);
        }
        self.snapshot = Some(current);
        self.revision
    }
    fn check(&mut self, expected: Option<u64>, cx: &App) -> Result<(), Status> {
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
        Self { sender, secrets }
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

impl SettingsState {
    fn remote_preferences(
        &mut self,
        patch: PreferencesPatch,
        cx: &mut Context<Self>,
    ) -> Result<Committed, Status> {
        if !self.ready || self.saving || self.quitting {
            return Err(busy("Preferences are busy."));
        }
        let client = self
            .client
            .clone()
            .ok_or_else(|| busy("Storage is unavailable."))?;
        let mut changes = Vec::new();
        if let Some(v) = patch.reopen_last_workspace() {
            changes.push(Change::ReopenLastWorkspace(v));
        }
        if let Some(v) = patch.notch_enabled() {
            changes.push(Change::NotchEnabled(v));
        }
        for (key, value) in [
            (PreferenceKey::NewTabTool, patch.new_tab_tool()),
            (PreferenceKey::NewWorktreeTool, patch.new_worktree_tool()),
            (PreferenceKey::ChatProfile, patch.chat_profile()),
            (PreferenceKey::ChatModel, patch.chat_model()),
            (PreferenceKey::ChatEffort, patch.chat_effort()),
            (PreferenceKey::ChatMode, patch.chat_mode()),
        ] {
            if let Some(value) = value {
                changes.push(if value.is_empty() {
                    Change::Reset(key)
                } else {
                    Change::parse(key, value).map_err(invalid)?
                });
            }
        }
        let (tx, rx) = oneshot::channel();
        self.saving = true;
        self.error = None;
        cx.notify();
        self._task = Some(cx.spawn(async move |this, cx| {
            let result = client.apply(changes).await;
            let outcome = result.as_ref().map(|_| ()).map_err(ToString::to_string);
            let published = this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok(snapshot) => {
                        this.values = snapshot.preferences;
                        this.warnings = snapshot.warnings;
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                this.flush_remembered(cx);
                cx.notify();
            });
            let _ = tx.send(
                published
                    .map_err(|_| "Preferences owner stopped.".to_owned())
                    .and(outcome),
            );
        }));
        Ok(rx)
    }
}
