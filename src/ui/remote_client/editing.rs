//! Sent-message editing retains the previous composer and an immutable source item.
use super::*;
use canopy_desktop::remote::client::{ChatMutation, rpc_error};
use prost::Message;

pub(super) struct Editing {
    preview: p::EditPreview,
    previous: p::MessageDraft,
    images: Vec<String>,
    context: Option<p::SendContext>,
    restore: bool,
    reviewed: bool,
}
impl RemoteWindow {
    pub(super) fn save_edit(&mut self, cx: &mut Context<Self>) {
        let (Some(host), Some(chat)) = (self.host.clone(), self.chat.clone()) else {
            return;
        };
        let value = self
            .editing
            .as_ref()
            .map(|e| {
                serde_json::to_string(&(
                    e.preview.encode_to_vec(),
                    e.previous.encode_to_vec(),
                    e.images.clone(),
                    e.context.as_ref().map(Message::encode_to_vec),
                ))
                .unwrap()
            })
            .unwrap_or_default();
        let revision = HostClient::next_view_revision();
        let generation = self.generation;
        let task = self.global.runtime.spawn_blocking(move || {
            host.save_view_version(format!("edit:{}", chat.chat_id), value, revision)
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Err(error)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    if this.generation == generation {
                        this.error = Some(error);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
    pub(super) fn preview_edit(&mut self, item: p::ItemKey, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let chat = self.chat.clone();
        let run_id = self
            .projection
            .summary
            .as_ref()
            .and_then(|s| s.run_id.clone())
            .unwrap_or_default();
        self.run(
            async move {
                let mut client = p::chat_service_client::ChatServiceClient::new(host.channel()?);
                let preview = tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    client.preview_edit(p::PreviewEditRequest {
                        chat: chat.clone(),
                        run_id,
                        item: Some(item),
                    }),
                )
                .await
                .map_err(|_| "Edit preview timed out. The draft was retained.".to_owned())?
                .map_err(rpc_error)?
                .into_inner();
                if preview.chat != chat || preview.item != Some(item) {
                    return Err("Edit preview identity changed.".into());
                }
                Ok(Update::EditPreview(preview))
            },
            cx,
        );
    }
    pub(super) fn install_edit(&mut self, preview: p::EditPreview, cx: &mut Context<Self>) {
        if !preview.editable {
            self.error = Some(preview.reason.clone());
            return;
        }
        if let Some(editing) = &mut self.editing {
            if editing.preview.item != preview.item {
                self.error = Some("Cancel the current edit before editing another message.".into());
                return;
            }
            editing.preview = preview;
            editing.reviewed = true;
            editing.restore = false;
            self.save_edit(cx);
            return;
        }
        let Some(original) = preview.original.clone() else {
            self.error = Some("The host did not return the original message.".into());
            return;
        };
        self.editing = Some(Editing {
            preview,
            previous: p::MessageDraft {
                text: self.composer.read(cx).value().to_string(),
                attachments: self.attachments.clone(),
                ..Default::default()
            },
            images: self.local_images.clone(),
            context: self.draft_context.clone(),
            restore: false,
            reviewed: true,
        });
        self.save_edit(cx);
        self.attachments = original.attachments;
        self.local_images.clear();
        self.shell_mode = false;
        self.draft_context = self.projection.summary.as_ref().map(|s| p::SendContext {
            agent_id: s.agent_id.clone(),
            profile_id: s.profile_id.clone(),
            history_epoch: s.history_epoch,
            workspace: s.workspace.clone(),
        });
        let input = self.composer.clone();
        let _ = self.window.update(cx, |_, window, cx| {
            input.update(cx, |s, cx| s.set_value(original.text, window, cx))
        });
        self.save_composer(cx);
    }
    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.take() else {
            return;
        };
        self.attachments = editing.previous.attachments;
        self.local_images = editing.images;
        self.draft_context = editing.context;
        self.save_edit(cx);
        let input = self.composer.clone();
        let _ = self.window.update(cx, |_, window, cx| {
            input.update(cx, |s, cx| s.set_value(editing.previous.text, window, cx))
        });
        self.save_composer(cx);
        cx.notify();
    }
    pub(super) fn finish_edit(
        &mut self,
        item: p::ItemKey,
        completion: Option<(u64, bool)>,
        cx: &mut Context<Self>,
    ) {
        if self
            .editing
            .as_ref()
            .is_some_and(|e| e.preview.item == Some(item))
        {
            if let Some((revision, cleared)) = completion {
                self.editing = None;
                self.save_edit(cx);
                self.apply(
                    Update::ComposerStored(revision, cleared, "Edited message accepted.".into()),
                    cx,
                );
            } else {
                self.info = "Original edit completed. Current draft retained; cancel edit to restore the previous message.".into();
            }
        } else {
            self.info = "Original edit completed. The current draft was retained.".into();
        }
    }
    pub(super) fn send_edit(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = &self.editing else {
            return;
        };
        if !self.ready() || !editing.reviewed {
            self.error = Some("Review the edit preview again before sending.".into());
            cx.notify();
            return;
        }
        if let (Some(host), Some(chat)) = (&self.host, &self.chat) {
            let pending = host.chat_mutations(chat).into_iter().any(|(_, intent, receipt)| {
                matches!(intent, ChatMutation::Edit(request) if request.item == editing.preview.item)
                    && receipt.as_ref().is_none_or(|r| ![3, 4, 5, 7].contains(&r.state))
            });
            if pending {
                self.error = Some("The original edit has no confirmed outcome. Reconcile that request below before sending another edit.".into());
                cx.notify();
                return;
            }
        }
        let mut draft = p::MessageDraft {
            text: self.composer.read(cx).value().to_string(),
            attachments: self.attachments.clone(),
            context: editing
                .preview
                .original
                .as_ref()
                .and_then(|d| d.context.clone()),
            goal: editing.preview.original.as_ref().is_some_and(|d| d.goal),
        };
        let request = p::EditMessageRequest {
            command: None,
            chat: self.chat.clone(),
            run_id: editing.preview.run_id.clone(),
            item: editing.preview.item,
            draft: None,
            restore_files: editing.restore,
        };
        let host = self.host.clone().unwrap();
        let images = self.local_images.clone();
        let revision = self.composer_revision;
        let chat_id = self.chat.as_ref().unwrap().chat_id.clone();
        self.run(
            async move {
                for id in images {
                    draft.attachments.push(p::Attachment {
                        label: "Image".into(),
                        content: Some(p::attachment::Content::Image(
                            host.resume_upload(&id).await?,
                        )),
                    });
                }
                let item=request.item.ok_or("Edit target is missing.")?;
                let receipt=host.submit_chat_mutation(ChatMutation::Edit(p::EditMessageRequest {
                    draft: Some(draft), ..request
                })).await?;
                if receipt.state == p::OperationState::Succeeded as i32 {
                    let cleared = host.clear_composer_version(&chat_id, revision)?;
                    Ok(Update::EditSent(item, Some((revision, cleared))))
                }
                else {Ok(Update::Done("Edit accepted on host. Reconcile the original request to confirm completion.".into()))}
            },
            cx,
        );
    }
    pub(super) fn run_shell(&mut self, cx: &mut Context<Self>) {
        if !self.ready() {
            self.error = Some(
                "Shell requires a connected host. The command remains in the composer.".into(),
            );
            cx.notify();
            return;
        }
        if !self.attachments.is_empty() || !self.local_images.is_empty() {
            self.error = Some(
                "Remove attachments or return to Message mode before running a shell command."
                    .into(),
            );
            cx.notify();
            return;
        }
        let command_line = self.composer.read(cx).value().to_string();
        if command_line.trim().is_empty() {
            return;
        }
        let host = self.host.clone().unwrap();
        let request = p::RunShellRequest {
            command: None,
            chat: self.chat.clone(),
            run_id: self
                .projection
                .summary
                .as_ref()
                .and_then(|s| s.run_id.clone())
                .unwrap_or_default(),
            command_line: command_line.clone(),
        };
        let revision = self.composer_revision;
        let chat_id = self.chat.as_ref().unwrap().chat_id.clone();
        self.run(
            async move {
                host.submit_chat_mutation(ChatMutation::RunShell(request))
                    .await?;
                let cleared = host.clear_composer_version(&chat_id, revision)?;
                Ok(Update::ComposerStored(
                    revision,
                    cleared,
                    "Shell command accepted on host.".into(),
                ))
            },
            cx,
        );
    }
    pub(super) fn stop_shell(&mut self, item: p::ItemKey, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let request = p::StopShellRequest {
            command: None,
            chat: self.chat.clone(),
            run_id: self
                .projection
                .summary
                .as_ref()
                .and_then(|s| s.run_id.clone())
                .unwrap_or_default(),
            item: Some(item),
        };
        self.run(
            async move {
                host.submit_chat_mutation(ChatMutation::StopShell(request))
                    .await?;
                Ok(Update::Done("Shell stop accepted.".into()))
            },
            cx,
        );
    }
    pub(super) fn render_edit_controls(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = column().gap_2();
        if let Some(editing) = &self.editing {
            body = body.child(if editing.preview.first_message {
                "Editing the first message — creates a fresh branch"
            } else {
                "Editing a sent message — continues from before this message"
            });
            if !editing.reviewed {
                body = body
                    .child("Restored edit draft. Review the current host state before sending.");
            }
            if let Some(files) = &editing.preview.files {
                body = body.child(format!(
                    "File restore: +{} −{}",
                    files.insertions, files.deletions
                ));
                for path in files.paths.iter().take(100) {
                    body = body.child(path.clone());
                }
                if !files.reason.is_empty() {
                    body = body.child(files.reason.clone());
                }
                body = body.child(
                    gpui_kit::component::checkbox::Checkbox::new("restore-edit-files")
                        .label("Restore files before sending")
                        .checked(editing.restore)
                        .disabled(self.busy || !editing.reviewed || !files.possible)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            if let Some(editing) = &mut this.editing {
                                editing.restore = *checked;
                            }
                            cx.notify();
                        })),
                );
            }
            if let Some(item) = editing.preview.item {
                body = body.child(
                    button("review-edit", "Refresh edit preview")
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |this, _, _, cx| this.preview_edit(item, cx))),
                );
            }
            body = body.child(
                button("cancel-edit", "Cancel edit")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_edit(cx))),
            );
            body=body.child("A failed edit keeps this draft. After files_not_restored, turn off restore to send a new request without changing files. After stale_run or conditions_changed, refresh the preview. Unknown outcomes require reconciliation below.");
        } else {
            body = body.child(
                button(
                    "shell-mode",
                    if self.shell_mode {
                        "Shell · return to Message"
                    } else {
                        "Shell !"
                    },
                )
                .disabled(self.busy || !self.supports("shell"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.shell_mode = !this.shell_mode;
                    cx.notify();
                })),
            );
            if !self.supports("shell") {
                body = body.child(self.capability_reason("shell"));
            }
            if self.shell_mode {
                body=body.child("Shell on host. Enter the command without !. Message mode sends ! as ordinary text.");
            }
        }
        if let (Some(host), Some(chat)) = (&self.host, &self.chat) {
            for (id, intent, receipt) in host.chat_mutations(chat) {
                let recovery = match &intent {
                    ChatMutation::Edit(request) => request
                        .item
                        .zip(request.draft.as_ref().map(|d| d.text.clone())),
                    _ => None,
                };
                let kind = match intent {
                    ChatMutation::Send(_) => "Send",
                    ChatMutation::Edit(_) => "Edit",
                    ChatMutation::Fork(_) => "Fork",
                    ChatMutation::RunShell(_) => "Shell",
                    ChatMutation::StopShell(_) => "Stop shell",
                };
                body = body.child(format!(
                    "{kind} · {}",
                    receipt
                        .as_ref()
                        .map(|r| mutation_state(r.state).to_owned())
                        .unwrap_or_else(|| "Outcome unknown".into())
                ));
                if let Some(error) = receipt.as_ref().and_then(|r| r.error.as_ref()) {
                    body = body.child(format!("{}: {}", error.code, error.message));
                }
                if receipt
                    .as_ref()
                    .is_none_or(|r| ![3, 4, 5, 7].contains(&r.state))
                {
                    body = body.child(
                        button(
                            SharedString::from(format!("retry-chat:{id}")),
                            "Reconcile original request",
                        )
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let id = id.clone();
                            let recovery = recovery.clone();
                            this.run(
                                async move {
                                    let receipt = host.retry_chat_mutation(&id).await?;
                                    if receipt.state == p::OperationState::Succeeded as i32
                                        && let Some((item, _text)) = recovery
                                    {
                                        return Ok(Update::EditSent(item, None));
                                    }
                                    Ok(Update::Done(format!(
                                        "Original request result: {}",
                                        mutation_state(receipt.state)
                                    )))
                                },
                                cx,
                            );
                        })),
                    );
                }
            }
        }
        body.into_any_element()
    }
}

fn mutation_state(state: i32) -> &'static str {
    match p::OperationState::try_from(state).ok() {
        Some(p::OperationState::Accepted) => "Accepted",
        Some(p::OperationState::Attempting) => "In progress",
        Some(p::OperationState::Succeeded) => "Completed",
        Some(p::OperationState::Failed) => "Failed",
        Some(p::OperationState::Rejected) => "Rejected",
        Some(p::OperationState::DeliveryUnknown) => "Outcome unknown",
        Some(p::OperationState::CommandRetired) => "Retired",
        Some(p::OperationState::NotFound) => "Not found on host",
        _ => "Outcome unknown",
    }
}
