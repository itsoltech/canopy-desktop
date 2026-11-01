//! Local controls retain their existing runtime effects behind typed admission.
use super::*;
use canopy_desktop::command_bus::{Command, CommandBus, Control, ResourceRef};
use canopy_desktop::host::CommandState;

pub(super) struct LocalOutcome {
    pub state: CommandState,
    pub result: Result<(), String>,
}
impl LocalOutcome {
    fn success() -> Self {
        Self {
            state: CommandState::Succeeded,
            result: Ok(()),
        }
    }
    fn error(state: CommandState, error: impl Into<String>) -> Self {
        Self {
            state,
            result: Err(error.into()),
        }
    }
}

impl ChatRuntime {
    pub(in crate::app_state::chats::runtime) fn local_control(
        &mut self,
        command: Control,
        cx: &mut Context<Self>,
        effect: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        if self.applying_command {
            effect(self, cx);
            return;
        }
        self.local_effect(command, cx, move |this, cx| {
            let before_notice = this.notice.clone();
            let before_error = this.error.clone();
            effect(this, cx);
            let error = (this.error != before_error)
                .then(|| this.error.clone())
                .flatten()
                .or_else(|| {
                    (this.notice != before_notice)
                        .then(|| {
                            this.notice
                                .as_ref()
                                .filter(|n| n.kind == ChatNoticeKind::Error)
                                .map(|n| n.text.clone())
                        })
                        .flatten()
                });
            Task::ready(match error {
                Some(error) => LocalOutcome::error(CommandState::DeliveryUnknown, error),
                None => LocalOutcome::success(),
            })
        });
    }

    fn local_effect(
        &mut self,
        command: Control,
        cx: &mut Context<Self>,
        effect: impl FnOnce(&mut Self, &mut Context<Self>) -> Task<LocalOutcome> + 'static,
    ) {
        let starting = matches!(command, Control::Start);
        if starting && self.local_start_pending {
            return;
        }
        let Some(client) = self.client(cx) else {
            self.notice = Some(ChatNotice::error("Thread storage is unavailable."));
            cx.notify();
            return;
        };
        let edit = match &command {
            Control::Edit {
                draft: canopy_desktop::command_bus::DraftIntent::Ready(draft),
                restore_files,
                ..
            } => Some((
                self.local_draft_revision,
                draft.clone().into_draft(),
                *restore_files,
            )),
            _ => None,
        };
        if starting {
            self.local_start_pending = true;
        }
        let record = self.persisted_record(cx);
        let id = uuid::Uuid::new_v4();
        self.local_controls.insert(
            id,
            cx.spawn(async move |this, cx| {
                let result = async {
                    let record = record.await?;
                    let thread = record
                        .thread()
                        .ok_or("The thread identity is unavailable.")?;
                    let bus = CommandBus::new(client);
                    let run = match &command {
                        Control::Interrupt { run }
                        | Control::Restart { run }
                        | Control::Respond { run, .. }
                        | Control::StopBackground { run, .. }
                        | Control::RunShell { run, .. }
                        | Control::StopShell { run, .. }
                        | Control::Edit { run, .. } => Some(*run),
                        _ => None,
                    };
                    let input = bus
                        .begin_local_resource(
                            ResourceRef::Thread {
                                thread: thread.thread().clone(),
                                executor_epoch: thread.executor_epoch(),
                            },
                            record.authority_root().into(),
                            Command::Control(command),
                            Vec::new(),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    let outcome = match this.update(cx, |this, cx| {
                        if this.closed || run.is_some_and(|run| run != this.generation) {
                            return Task::ready(LocalOutcome::error(
                                CommandState::Rejected,
                                "conditions_changed: The thread run changed before dispatch.",
                            ));
                        }
                        this.applying_command = true;
                        let outcome = effect(this, cx);
                        this.applying_command = false;
                        outcome
                    }) {
                        Ok(task) => task.await,
                        Err(_) => LocalOutcome::error(CommandState::Rejected, "resource_deleted"),
                    };
                    let detail = outcome.result.as_ref().err().map(|error| {
                        serde_json::to_vec(&serde_json::json!({"reason":error}))
                            .expect("string JSON")
                    });
                    bus.complete_local_resource(input, outcome.state, detail)
                        .await?;
                    outcome.result
                }
                .await;
                let _ = this.update(cx, |this, cx| {
                    this.local_controls.remove(&id);
                    if starting {
                        this.local_start_pending = false;
                    }
                    if let Some((revision, draft, restore)) = edit
                        && this.local_edit_pending
                    {
                        this.local_edit_pending = false;
                        cx.emit(ChatRuntimeEvent::EditFinished {
                            revision,
                            result: result.clone(),
                        });
                        if result.is_err() && !restore {
                            cx.emit(ChatRuntimeEvent::DraftReturned(draft));
                        }
                    }
                    if let Err(error) = result {
                        this.notice = Some(ChatNotice::error(error));
                    }
                    cx.emit(ChatStateChanged);
                    cx.notify();
                });
            }),
        );
    }

    pub(super) fn local_shell(&mut self, command: String, cx: &mut Context<Self>) {
        let revision = self.local_draft_revision;
        self.local_effect(
            Control::RunShell {
                run: self.generation,
                command: command.clone(),
            },
            cx,
            move |this, cx| {
                Task::ready(match this.run_shell(&command, cx) {
                    Ok(_) => {
                        cx.emit(ChatRuntimeEvent::DraftConsumed(revision));
                        LocalOutcome::success()
                    }
                    Err(error) => LocalOutcome::error(CommandState::Failed, error),
                })
            },
        );
    }

    pub(super) fn local_edit(
        &mut self,
        target: MessageEdit,
        fork: Option<String>,
        draft: Draft,
        restore: bool,
        revision: u64,
        cx: &mut Context<Self>,
    ) {
        if self.local_edit_pending {
            return;
        }
        if self.client(cx).is_none() {
            let error = "Thread storage is unavailable.".to_owned();
            cx.emit(ChatRuntimeEvent::EditFinished {
                revision,
                result: Err(error.clone()),
            });
            if !restore {
                cx.emit(ChatRuntimeEvent::DraftReturned(draft));
            }
            self.notice = Some(ChatNotice::error(error));
            cx.notify();
            return;
        }
        self.local_edit_pending = true;
        let recovery = draft.clone();
        self.local_effect(Control::Edit {
            run: target.run, history_epoch: target.epoch, item: target.item.0,
            draft: canopy_desktop::command_bus::DraftIntent::Ready(draft.clone().into()),
            restore_files: restore,
        }, cx, move |_, cx| cx.spawn(async move |this, cx| {
            let outcome = async {
                if restore {
                    let answer = this.update(cx, |this, cx| {
                        if this.generation != target.run || this.epoch != target.epoch || this.closed {
                            return Err("The chat restarted since; open the message again.".to_owned());
                        }
                        if this.remote_waits.rewind.is_some() || !this.rewind_files(target.item, false, cx) {
                            return Err("Files can't be restored now.".to_owned());
                        }
                        let (reply, answer) = futures_channel::oneshot::channel();
                        this.remote_waits.rewind = Some((target.item, false, reply));
                        Ok(answer)
                    }).map_err(|_| LocalOutcome::error(CommandState::Rejected, "resource_deleted"))?
                        .map_err(|e| LocalOutcome::error(CommandState::Rejected, e))?;
                    match answer.await {
                        Ok(Ok(canopy_desktop::chat::session::Rewind::Done)) => (),
                        Ok(Ok(canopy_desktop::chat::session::Rewind::Failed(error))) => return Err(LocalOutcome::error(CommandState::Failed, format!("Couldn't restore the files: {error}"))),
                        _ => return Err(LocalOutcome::error(CommandState::DeliveryUnknown, "The chat ended before the files were restored; some may be.")),
                    }
                }
                let answer = this.update(cx, |this, cx| {
                    if this.generation != target.run || this.epoch != target.epoch || this.closed {
                        return Err("The chat restarted since; open the message again.".to_owned());
                    }
                    if fork.is_some() && this.pane.metadata.resume_id.is_none() {
                        return Err("This conversation can't be continued from there.".into());
                    }
                    let (reply, answer) = futures_channel::oneshot::channel();
                    this.remote_waits.edit = Some(reply);
                    // Restore and restart belong to this one admitted edit.
                    this.applying_command = true;
                    this.restart_edited(fork, draft, cx);
                    this.applying_command = false;
                    cx.emit(ChatRuntimeEvent::EditFinished { revision, result: Ok(()) });
                    Ok(answer)
                }).map_err(|_| LocalOutcome::error(CommandState::Rejected, "resource_deleted"))?
                    .map_err(|e| LocalOutcome::error(if restore { CommandState::Failed } else { CommandState::Rejected }, e))?;
                match answer.await {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(error)) => Err(LocalOutcome::error(CommandState::Failed, error)),
                    Err(_) => Err(LocalOutcome::error(CommandState::DeliveryUnknown, "The chat closed while the edit was starting.")),
                }
            }.await;
            let _ = this.update(cx, |this, cx| {
                this.local_edit_pending = false;
                if let Err(outcome) = &outcome {
                    cx.emit(ChatRuntimeEvent::EditFinished { revision, result: outcome.result.clone() });
                    if !restore { cx.emit(ChatRuntimeEvent::DraftReturned(recovery)); }
                }
                cx.notify();
            });
            outcome.err().unwrap_or_else(LocalOutcome::success)
        }));
    }
}
