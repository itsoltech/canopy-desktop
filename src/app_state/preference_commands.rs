//! Preferences use the same bus and the existing app/SQLite owners for both paths.
use super::{AppState, SettingsState};
use canopy_desktop::{
    command_bus::{
        Command, CommandBus, CommandEnvelope, Conditions, PreparedCommand, Principal, ResourceRef,
    },
    host::CommandReceipt,
    remote::auth::storage_status,
    settings::Change,
    thread::{EnvironmentRef, HostId},
};
use futures_channel::oneshot;
use gpui_kit::*;
use tonic::Status;

type Completion = oneshot::Sender<Result<CommandReceipt, Status>>;
impl SettingsState {
    pub(super) fn bind_preference_owners(&mut self, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        self.preference_owners = vec![
            cx.observe(&app.tools, |this, _, cx| {
                this.settings_revision(cx);
                this.flush_remembered(cx);
            }),
            cx.observe(&app.integrations, |this, _, cx| {
                this.settings_revision(cx);
                this.flush_remembered(cx);
            }),
        ];
    }
    pub(crate) fn settings_revision(&mut self, cx: &App) -> u64 {
        let app = cx.global::<AppState>();
        self.settings_revision.observe(
            self.values.clone(),
            app.tools.read(cx).catalog.clone(),
            app.integrations.read(cx).config.clone(),
        )
    }
    pub(super) fn preferences_owner_busy(&self, cx: &App) -> bool {
        let app = cx.global::<AppState>();
        let tools = app.tools.read(cx);
        let integrations = app.integrations.read(cx);
        // A failed tools load has finished its publication. Local preferences
        // remain writable; remote CAS retains its existing ready-owner check.
        (!tools.ready && tools.error.is_none())
            || tools.saving
            || tools.quitting
            || !integrations.ready
            || integrations.busy
    }
    pub(super) fn start_local_preferences(&mut self, changes: Vec<Change>, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(host_id) = self.host_id().and_then(|id| HostId::new(id.into())) else {
            return;
        };
        let command = match Command::preferences(changes) {
            Ok(command) => command,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let envelope = CommandEnvelope {
            command_id: uuid::Uuid::new_v4(),
            resource: ResourceRef::Environment(EnvironmentRef { host_id }),
            command,
            conditions: Conditions::default(),
        };
        let bus = CommandBus::new(client);
        let prepare = cx.background_spawn(async move {
            bus.prepare(envelope, Principal::local_owner())
                .await
                .map_err(storage_status)
        });
        self.save_preferences(prepare, None, cx);
    }
    pub(crate) fn start_prepared_preferences(
        &mut self,
        prepared: PreparedCommand,
        completion: Completion,
        cx: &mut Context<Self>,
    ) {
        self.save_preferences(Task::ready(Ok(prepared)), Some(completion), cx);
    }
    fn save_preferences(
        &mut self,
        prepare: Task<Result<PreparedCommand, Status>>,
        completion: Option<Completion>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            if let Some(reply) = completion {
                let _ = reply.send(Err(Status::unavailable("Storage is unavailable.")));
            }
            return;
        };
        let revision = self.settings_revision(cx);
        self.saving = true;
        self.error = None;
        cx.notify();
        self._task = Some(cx.spawn(async move |this, cx| {
            let outcome = async {
                let prepared = prepare.await?;
                CommandBus::new(client.clone())
                    .commit_preferences(prepared, revision)
                    .await
                    .map_err(storage_status)
            }
            .await;
            let local_receipt = if completion.is_none() {
                outcome
                    .as_ref()
                    .ok()
                    .map(|(receipt, _)| receipt.key().clone())
            } else {
                None
            };
            let published = this.update(cx, |this, cx| {
                this.saving = false;
                let result = match outcome {
                    Ok((receipt, snapshot)) => {
                        if let Some(snapshot) = snapshot {
                            this.values = snapshot.preferences;
                            this.warnings = snapshot.warnings;
                        }
                        this.settings_revision(cx);
                        Ok(receipt)
                    }
                    Err(error) => {
                        this.error = Some(error.message().to_owned());
                        Err(error)
                    }
                };
                // Reply names the receipt of this commit, before a remembered local
                // write can change the combined snapshot again.
                if let Some(reply) = completion {
                    let _ = reply.send(result);
                }
                this.flush_remembered(cx);
                cx.notify();
            });
            if published.is_ok()
                && let Some(key) = local_receipt
            {
                let _ = client.acknowledge_runtime_command(key).await;
            }
        }));
    }
}
