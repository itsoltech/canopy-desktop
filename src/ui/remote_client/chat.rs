use super::*;
use canopy_desktop::remote::client::rpc_error;
impl RemoteWindow {
    pub(super) fn save_context_attachments(&mut self, cx: &mut Context<Self>) {
        self.save_composer(cx);
    }

    pub(super) fn save_composer(&mut self, cx: &mut Context<Self>) {
        let (Some(host), Some(chat)) = (self.host.clone(), self.chat.clone()) else {
            return;
        };
        if self.draft_context.is_none() {
            self.draft_context = self.projection.summary.as_ref().map(|s| p::SendContext {
                agent_id: s.agent_id.clone(),
                profile_id: s.profile_id.clone(),
                history_epoch: s.history_epoch,
                workspace: s.workspace.clone(),
            });
        }
        self.composer_revision = HostClient::next_view_revision();
        let revision = self.composer_revision;
        let draft = p::MessageDraft {
            text: self.composer.read(cx).value().to_string(),
            attachments: self.attachments.clone(),
            ..Default::default()
        };
        let context = self.draft_context.clone();
        let images = self.local_images.clone();
        let generation = self.generation;
        let work = self.global.runtime.spawn_blocking(move || {
            host.save_composer_version(&chat.chat_id, draft, context, images, revision)
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Err(error)) = work.await {
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

    fn pick_image(&mut self, cx: &mut Context<Self>) {
        let generation = self.generation;
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Attach image".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let Some(path) = paths.into_iter().next() else {
                    return;
                };
                let _ = this.update(cx, |this, cx| {
                    if this.generation != generation || this.busy {
                        return;
                    }
                    let Some(host) = this.host.clone() else {
                        return;
                    };
                    let Some(chat) = this.chat.clone() else {
                        return;
                    };
                    this.run(
                        async move {
                            use std::io::Read;
                            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                            let mut bytes = Vec::new();
                            file.take(32 * 1024 * 1024 + 1)
                                .read_to_end(&mut bytes)
                                .map_err(|e| e.to_string())?;
                            if bytes.len() > 32 * 1024 * 1024 {
                                return Err("Image exceeds 32 MiB.".into());
                            }
                            let format = image::guess_format(&bytes)
                                .map_err(|_| "Select a supported image file.".to_owned())?;
                            let media = match format {
                                image::ImageFormat::Png => "image/png",
                                image::ImageFormat::Jpeg => "image/jpeg",
                                image::ImageFormat::WebP => "image/webp",
                                image::ImageFormat::Gif => "image/gif",
                                _ => return Err("Choose a PNG, JPEG, WebP or GIF image.".into()),
                            };
                            let id = host.prepare_attachment(
                                p::ResourceRef {
                                    resource: Some(p::resource_ref::Resource::Chat(chat)),
                                },
                                "chat-image",
                                media,
                                &bytes,
                            )?;
                            Ok(Update::LocalAttachment(id))
                        },
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    fn fork_at(
        &mut self,
        key: p::ItemKey,
        edited: bool,
        destination: Option<p::WorkspaceRef>,
        cx: &mut Context<Self>,
    ) {
        let host = self.host.clone().unwrap();
        let chat = self.chat.clone();
        let Some(summary) = self.projection.summary.as_ref() else {
            self.error = Some("Load the chat context before forking.".into());
            cx.notify();
            return;
        };
        let _ = summary;
        self.fork_request = None;
        let replacement = edited.then(|| p::MessageDraft {
            text: self.composer.read(cx).value().to_string(),
            attachments: self.attachments.clone(),
            ..Default::default()
        });
        let images = if edited {
            self.local_images.clone()
        } else {
            vec![]
        };
        self.run(
            async move {
                let mut replacement = replacement;
                if let Some(draft) = &mut replacement {
                    for image in images {
                        draft.attachments.push(p::Attachment {
                            label: "Image".into(),
                            content: Some(p::attachment::Content::Image(
                                host.resume_upload(&image).await?,
                            )),
                        });
                    }
                }
                let receipt = host
                    .submit_chat_mutation(canopy_desktop::remote::client::ChatMutation::Fork(
                        p::ForkChatRequest {
                            command: None,
                            chat,
                            history_epoch: key.history_epoch,
                            item_id: key.item_id,
                            destination,
                            replacement,
                        },
                    ))
                    .await?;
                Ok(Update::Done(
                    if receipt.state == p::OperationState::Succeeded as i32 {
                        "Fork created. Open it from the host catalog."
                    } else {
                        "Fork accepted. Reconcile the original request to confirm completion."
                    }
                    .into(),
                ))
            },
            cx,
        );
    }

    pub(super) fn open_chat(
        &mut self,
        chat: p::ChatRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.global.connections.update(cx, |_, cx| {
            cx.emit(super::connections::OpenChat { host, chat })
        });
        self.page = Page::Catalog;
        self.chat = None;
        if let Some(task) = self.stream.take() {
            task.abort();
        }
        if !self.embedded {
            window.remove_window();
        }
        cx.notify();
    }
    fn search_history(&mut self, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let chat = self.chat.clone();
        let text = self.history_query.clone();
        let page_token = self
            .history_matches
            .as_ref()
            .and_then(|m| m.next_page_token.clone());
        self.run(
            async move {
                Ok(Update::HistoryMatches(
                    p::chat_service_client::ChatServiceClient::new(host.channel()?)
                        .search_history(p::SearchHistoryRequest {
                            chat,
                            text,
                            page_token,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner(),
                ))
            },
            cx,
        );
    }
    pub(super) fn supports(&self, operation: &str) -> bool {
        self.options
            .as_ref()
            .and_then(|o| o.capabilities.iter().find(|c| c.operation == operation))
            .or_else(|| {
                self.agent_catalog
                    .as_ref()
                    .and_then(|c| c.capabilities.iter().find(|c| c.operation == operation))
            })
            .is_some_and(|c| c.supported)
    }

    pub(super) fn capability_reason(&self, operation: &str) -> String {
        self.options
            .as_ref()
            .and_then(|o| o.capabilities.iter().find(|c| c.operation == operation))
            .or_else(|| {
                self.agent_catalog
                    .as_ref()
                    .and_then(|o| o.capabilities.iter().find(|c| c.operation == operation))
            })
            .map(|c| {
                if c.supported {
                    String::new()
                } else {
                    if c.reason.is_empty() {
                        "The agent does not support this operation.".into()
                    } else {
                        c.reason.clone()
                    }
                }
            })
            .unwrap_or_else(|| "Refresh the agent catalog to discover this operation.".into())
    }

    pub(super) fn render_chat(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (Some(host), Some(chat)) = (self.host.clone(), self.chat.clone()) else {
            return div().child("Select a chat.").into_any_element();
        };
        let mut body = column()
            .gap_3()
            .child(
                button("detach-chat", "Close view")
                    .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Catalog, cx))),
            )
            .child(
                row()
                    .gap_2()
                    .child(
                        button("earlier", "Earlier messages")
                            .disabled(!self.ready())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let host = this.host.clone().unwrap();
                                let chat = this.chat.clone().unwrap();
                                let token = this.history_token.clone();
                                let before = if token.is_none() {
                                    this.projection.items.first().and_then(|i| i.key)
                                } else {
                                    None
                                };
                                this.run(
                                    async move {
                                        let (items, token) =
                                            host.history_page(chat, token, None, before).await?;
                                        Ok(Update::History(items, token))
                                    },
                                    cx,
                                );
                            })),
                    )
                    .child(
                        button("latest-chat", "Latest messages").on_click(cx.listener(
                            |this, _, _, cx| {
                                if !this.projection.items.is_empty() {
                                    this.chat_list
                                        .scroll_to_reveal_item(this.projection.items.len() - 1);
                                }
                                cx.notify();
                            },
                        )),
                    )
                    .child(input(&self.fields["query"]))
                    .child(
                        button("find-chat", "Find")
                            .disabled(!self.ready())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.history_query = this.value("query", cx);
                                this.history_matches = None;
                                this.search_history(cx);
                            })),
                    ),
            );
        if let Some(matches) = self.history_matches.clone() {
            if matches.matches.is_empty() {
                body = body.child("No matches in saved history.");
            }
            if matches.incomplete {
                body = body.child("Some history is unavailable on the host.");
            }
            for found in matches.matches {
                if let Some(key) = found.key {
                    body = body.child(
                        button(
                            SharedString::from(format!(
                                "match:{}:{}",
                                key.history_epoch, key.item_id
                            )),
                            found.excerpt,
                        )
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let chat = this.chat.clone().unwrap();
                            this.run(
                                async move {
                                    let (items, _) =
                                        host.history_page(chat, None, Some(key), None).await?;
                                    Ok(Update::HistoryJump(items, key))
                                },
                                cx,
                            );
                        })),
                    );
                }
            }
            if matches.next_page_token.is_some() {
                body = body.child(
                    button("more-matches", "More matches")
                        .disabled(!self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.search_history(cx))),
                );
            }
        }
        if let Some((key, edited)) = self.fork_request {
            let mut destinations = column().gap_2().child("Fork destination");
            if let Some(catalog) = host.catalog() {
                for project in catalog.projects {
                    for checkout in project.checkouts {
                        if checkout.closing.unwrap_or(false) {
                            continue;
                        }
                        if let Some(workspace) = checkout.workspace {
                            destinations = destinations.child(
                                button(
                                    SharedString::from(format!(
                                        "fork-to:{}",
                                        workspace.workspace_id
                                    )),
                                    format!("{} / {}", project.name, checkout.name),
                                )
                                .disabled(!self.ready())
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.fork_at(key, edited, Some(workspace.clone()), cx)
                                    },
                                )),
                            );
                        }
                    }
                }
            }
            destinations = destinations
                .child(
                    button("fork-private", "Private chats")
                        .disabled(!self.ready())
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.fork_at(key, edited, None, cx)),
                        ),
                )
                .child(
                    button("cancel-fork", "Cancel").on_click(cx.listener(|this, _, _, cx| {
                        this.fork_request = None;
                        cx.notify();
                    })),
                );
            body = body.child(destinations);
        }
        if self.activity_thread.is_some() {
            body = body.child(
                button("thread-back", "Back to conversation").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.activity_thread = None;
                        this.refresh_markdown(cx);
                        cx.notify();
                    },
                )),
            );
        }
        let owner = cx.entity();
        body = body.child(
            list(self.chat_list.clone(), move |index, _, cx| {
                owner.update(cx, |this, cx| this.render_chat_item(index, cx))
            })
            .h_96()
            .w_full(),
        );
        for (id, request, receipt) in host.outbox() {
            if request.chat.as_ref() == Some(&chat) {
                let label = request.draft.map(|d| d.text).unwrap_or_default();
                let id_send = id.clone();
                body = body
                    .child(div().child(format!(
                            "Outbox: {label} · {}",
                            receipt
                                .map(|r| format!("{:?}", r.state))
                                .unwrap_or("not sent".into())
                        )))
                    .child(
                        row()
                            .gap_2()
                            .child(
                                button(
                                    SharedString::from(format!("send:{id}")),
                                    "Send / reconcile",
                                )
                                .disabled(!self.ready())
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let host = this.host.clone().unwrap();
                                        let id = id_send.clone();
                                        this.run(
                                            async move {
                                                host.send_outbox(&id).await?;
                                                Ok(Update::Done("Message reconciled.".into()))
                                            },
                                            cx,
                                        );
                                    },
                                )),
                            )
                            .child(
                                button(
                                    SharedString::from(format!("dismiss:{id}")),
                                    "Dismiss result",
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let host = this.host.clone().unwrap();
                                        let id = id.clone();
                                        this.run(
                                            async move {
                                                host.dismiss_outbox(&id)?;
                                                Ok(Update::Done("Message dismissed.".into()))
                                            },
                                            cx,
                                        );
                                    },
                                )),
                            ),
                    );
            }
        }
        for (index, attachment) in self.attachments.clone().into_iter().enumerate() {
            body = body.child(
                row().gap_2().child(attachment.label).child(
                    button(
                        SharedString::from(format!("remove-context:{index}")),
                        "Remove attachment",
                    )
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if index < this.attachments.len() {
                            this.attachments.remove(index);
                        }
                        this.save_context_attachments(cx);
                        cx.notify();
                    })),
                ),
            );
        }
        for (index, id) in self.local_images.clone().into_iter().enumerate() {
            body = body.child(
                row().gap_2().child(format!("Image {}", index + 1)).child(
                    button(
                        SharedString::from(format!("remove-image:{id}")),
                        "Remove image",
                    )
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.local_images.retain(|image| image != &id);
                        this.save_composer(cx);
                        // Cleanup is reference-based; an accepted send may still use this image.
                    })),
                ),
            );
        }
        body = body.child(self.render_edit_controls(cx));
        body = body
            .child(textarea(&self.composer).h_32().disabled(self.busy && self.editing.is_some()))
            .child(div().text_color(t::muted()).child(format!(
                "{} attachments",
                self.attachments.len() + self.local_images.len()
            )))
            .child(
                button(
                    "send-message",
                    if self.shell_mode { "Run shell command" } else if self.editing.is_some() { "Send edit" } else if self.ready() {
                        "Send"
                    } else {
                        "Keep in outbox"
                    },
                )
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.shell_mode { this.run_shell(cx); return; }
                    if this.editing.is_some() { this.send_edit(cx); return; }
                    let host = this.host.clone().unwrap();
                    let chat = this.chat.clone().unwrap();
                    let summary = this.projection.summary.clone().or_else(|| {
                        host.catalog().and_then(|c| {
                            c.chats.into_iter().find(|c| c.chat.as_ref() == Some(&chat))
                        })
                    });
                    let Some(summary) = summary else {
                        this.error =
                            Some("Load the original chat context before composing.".into());
                        cx.notify();
                        return;
                    };
                    let draft = p::MessageDraft {
                        text: this.composer.read(cx).value().to_string(),
                        attachments: this.attachments.clone(),
                        ..Default::default()
                    };
                    if draft.text.trim().is_empty()
                        && draft.attachments.is_empty()
                        && this.local_images.is_empty()
                    {
                        return;
                    }
                    let context = this.draft_context.clone().unwrap_or(p::SendContext {
                        agent_id: summary.agent_id,
                        profile_id: summary.profile_id,
                        history_epoch: summary.history_epoch,
                        workspace: summary.workspace,
                    });
                    let ready = this.ready();
                    let uploads = this.local_images.clone();
                    let revision = this.composer_revision;
                    this.run(
                        async move {
                            host.save_composer_version(&chat.chat_id, draft.clone(), Some(context.clone()), uploads.clone(), revision)?;
                            let chat_id = chat.chat_id.clone();
                            let cleared;
                            if draft.attachments.iter().all(|a| {
                                matches!(a.content, Some(p::attachment::Content::Image(_)))
                            }) {
                                let (id, consumed) = host.enqueue(chat, context, draft, uploads, revision)?;
                                cleared = consumed;
                                if ready && let Err(error) = host.send_outbox(&id).await {
                                        return Ok(Update::ComposerStored(
                                            revision, cleared,
                                            format!("Message remains in outbox: {error}"),
                                        ));
                                }
                            } else {
                                if !ready { return Err("Contextual attachments require an online send; the draft is retained.".into()); }
                                let mut draft = draft;
                                for upload in uploads {
                                    let artifact = host.resume_upload(&upload).await?;
                                    draft.attachments.push(p::Attachment { label: "Image".into(), content: Some(p::attachment::Content::Image(artifact)) });
                                }
                                host.submit_chat_mutation(canopy_desktop::remote::client::ChatMutation::Send(p::SendMessageRequest {
                                    command: None, chat: Some(chat), message_id: uuid::Uuid::new_v4().to_string(),
                                    draft: Some(draft), expected_context: Some(context),
                                })).await?;
                                cleared = host.clear_composer_version(&chat_id, revision)?;
                            }
                            Ok(Update::ComposerStored(revision, cleared, "Message saved.".into()))
                        },
                        cx,
                    );
                })),
            )
            .child(
                button("attach-image", "Attach image…")
                    .disabled(self.busy || !self.supports("images"))
                    .on_click(cx.listener(|this, _, _, cx| this.pick_image(cx))),
            )
            .child(
                row()
                    .gap_2()
                    .child(
                        button("interrupt", "Stop")
                            .disabled(!self.ready() || !self.supports("cancel"))
                            .on_click(cx.listener(|this, _, _, cx| this.chat_command(false, cx))),
                    )
                    .child(
                        button("restart", "Restart")
                            .disabled(!self.ready())
                            .on_click(cx.listener(|this, _, _, cx| this.chat_command(true, cx))),
                    )
                    .child(
                        button("steer", "Steer")
                            .disabled(!self.ready() || !self.supports("steer"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let host = this.host.clone().unwrap();
                                let chat = this.chat.clone();
                                let run_id = this
                                    .projection
                                    .summary
                                    .as_ref()
                                    .and_then(|s| s.run_id.clone())
                                    .unwrap_or_default();
                                let text = this.composer.read(cx).value().to_string();
                                this.run(
                                    async move {
                                        host.mutate(None, move |channel, command| async move {
                                            p::chat_service_client::ChatServiceClient::new(channel)
                                                .steer(p::SteerRequest {
                                                    command: Some(command),
                                                    chat,
                                                    run_id,
                                                    message_id: uuid::Uuid::new_v4().to_string(),
                                                    draft: Some(p::MessageDraft {
                                                        text,
                                                        ..Default::default()
                                                    }),
                                                })
                                                .await
                                        })
                                        .await?;
                                        Ok(Update::Done("Steer accepted.".into()))
                                    },
                                    cx,
                                );
                            })),
                    ),
            )
            .child(self.field("goal", "Goal objective"))
            .child(
                button("set-goal", "Set goal")
                    .disabled(!self.ready() || !self.supports("goal"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let host = this.host.clone().unwrap();
                        let chat = this.chat.clone();
                        let objective = nonempty(this.value("goal", cx));
                        this.run(
                            async move {
                                host.mutate(None, move |channel, command| async move {
                                    p::chat_service_client::ChatServiceClient::new(channel)
                                        .set_goal(p::SetGoalRequest {
                                            command: Some(command),
                                            chat,
                                            objective,
                                            action: None,
                                        })
                                        .await
                                })
                                .await?;
                                Ok(Update::Done("Goal updated.".into()))
                            },
                            cx,
                        );
                    })),
            );
        if let Some(goal) = self
            .projection
            .summary
            .as_ref()
            .and_then(|summary| summary.goal.clone())
        {
            body = body
                .child(format!("{} · {}", goal.objective, goal.state))
                .child(format!(
                    "Turns: {} · Tokens: {} · Budget: {} · Elapsed: {}s · Checks: {}",
                    goal.turns,
                    goal.tokens
                        .map(|value| value.to_string())
                        .unwrap_or("—".into()),
                    goal.budget
                        .map(|value| value.to_string())
                        .unwrap_or("—".into()),
                    goal.elapsed_seconds
                        .map(|value| value.to_string())
                        .unwrap_or("—".into()),
                    goal.checks
                ));
            if let Some(note) = goal.note {
                body = body.child(note);
            }
            for (action, label) in [(1, "Pause goal"), (2, "Resume goal"), (3, "Clear goal")] {
                let state_allows = match action {
                    1 => matches!(goal.state.as_str(), "pursuing" | "set"),
                    2 => matches!(
                        goal.state.as_str(),
                        "paused" | "blocked" | "usage_limited" | "budget_limited"
                    ),
                    _ => true,
                };
                body = body.child(
                    button(label, label)
                        .disabled(!self.ready() || !self.supports("goal") || !state_allows)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let chat = this.chat.clone();
                            this.run(
                                async move {
                                    host.mutate(None, move |channel, command| async move {
                                        p::chat_service_client::ChatServiceClient::new(channel)
                                            .set_goal(p::SetGoalRequest {
                                                command: Some(command),
                                                chat,
                                                objective: None,
                                                action: Some(action),
                                            })
                                            .await
                                    })
                                    .await?;
                                    Ok(Update::Done("Goal updated.".into()))
                                },
                                cx,
                            );
                        })),
                );
            }
        }
        if let Some(queue) = self.projection.queue.clone() {
            body = body.child(
                button(
                    "pause-queue",
                    if queue.paused {
                        "Resume queue"
                    } else {
                        "Pause queue"
                    },
                )
                .disabled(!self.ready())
                .on_click(cx.listener(move |this, _, _, cx| {
                    let host = this.host.clone().unwrap();
                    let chat = this.chat.clone();
                    let revision = queue.revision;
                    let paused = !queue.paused;
                    this.run(
                        async move {
                            host.mutate(None, move |channel, command| async move {
                                p::chat_service_client::ChatServiceClient::new(channel)
                                    .change_queue(p::ChangeQueueRequest {
                                        command: Some(command),
                                        chat,
                                        queue_revision: revision,
                                        change: Some(p::change_queue_request::Change::Pause(
                                            p::QueuePause { paused },
                                        )),
                                    })
                                    .await
                            })
                            .await?;
                            Ok(Update::Done("Queue updated.".into()))
                        },
                        cx,
                    );
                })),
            );
            for entry in queue.entries {
                body = body.child(div().child(format!(
                        "Queued: {} · {}",
                        entry
                            .draft
                            .as_ref()
                            .map(|d| d.text.as_str())
                            .unwrap_or_default(),
                        entry.delivery
                    )));
                let actions = if entry.delivery == 6 {
                    vec![
                        (1, "Accept as delivered"),
                        (2, "Send again…"),
                        (3, "Dismiss unknown…"),
                    ]
                } else {
                    vec![
                        (4, "Edit from composer"),
                        (5, "Move to end"),
                        (6, "Remove queued message"),
                    ]
                };
                for (action, label) in actions {
                    let id = entry.message_id.clone();
                    let revision = queue.revision;
                    let original = entry.draft.clone().unwrap_or_default();
                    body=body.child(button(SharedString::from(format!("queue:{id}:{action}")),label).disabled(!self.ready()).on_click(cx.listener(move|this,_,_,cx|{let host=this.host.clone().unwrap();let chat=this.chat.clone();let id=id.clone();let text=this.composer.read(cx).value().to_string();let original=original.clone();let change=match action{1..=3=>p::change_queue_request::Change::Resolve(p::QueueResolve{message_id:id,resolution:action,new_message_id:(action==2).then(||uuid::Uuid::new_v4().to_string())}),4=>p::change_queue_request::Change::Edit(p::QueueEdit{message_id:id,draft:Some(p::MessageDraft{text,..original})}),5=>p::change_queue_request::Change::Move(p::QueueMove{message_id:id,before_message_id:None}),_=>p::change_queue_request::Change::Remove(p::QueueRemove{message_id:id})};this.run(async move{let operation:PendingOperation=Box::pin(async move{host.mutate(None,move|channel,command|async move{p::chat_service_client::ChatServiceClient::new(channel).change_queue(p::ChangeQueueRequest{command:Some(command),chat,queue_revision:revision,change:Some(change)}).await}).await?;Ok(Update::Done("Queue changed.".into()))});if action==2||action==3{Ok(Update::Confirm(label.into(),"The previous delivery is unknown. Inspect the transcript before deciding.".into(),operation))}else{operation.await}},cx);})));
                }
            }
        }
        if let Some(options) = self
            .options
            .clone()
            .filter(|o| !o.options.is_empty())
            .or_else(|| {
                self.agent_catalog.as_ref().map(|c| p::AgentOptions {
                    chat: self.chat.clone(),
                    options: c.options.clone(),
                    capabilities: c.capabilities.clone(),
                })
            })
        {
            for option in options.options {
                body = body.child(div().child(format!(
                    "{} · {}",
                    option.label,
                    match option.effect.as_str() {
                        "immediate" => "Applies immediately",
                        "next_turn" | "nextTurn" => "Applies next turn",
                        "restart" => "Requires restart",
                        _ => "Timing not reported by host",
                    }
                )));
                for choice in option.choices {
                    let id = option.id.clone();
                    let value = choice.id.clone();
                    body = body.child(
                        button(
                            SharedString::from(format!("option:{id}:{value}")),
                            format!(
                                "{}{}",
                                if option.current_value == value {
                                    "✓ "
                                } else {
                                    ""
                                },
                                choice.label
                            ),
                        )
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let host = this.host.clone().unwrap();
                            let chat = this.chat.clone();
                            let option_id = id.clone();
                            let value_id = value.clone();
                            this.run(
                                async move {
                                    let refresh_chat = chat.clone();
                                    host.mutate(None, move |channel, command| async move {
                                        p::chat_service_client::ChatServiceClient::new(channel)
                                            .set_agent_option(p::SetAgentOptionRequest {
                                                command: Some(command),
                                                chat,
                                                option_id,
                                                value_id,
                                            })
                                            .await
                                    })
                                    .await?;
                                    Ok(Update::Options(
                                        p::chat_service_client::ChatServiceClient::new(
                                            host.channel()?,
                                        )
                                        .get_agent_options(p::ChatRequest { chat: refresh_chat })
                                        .await
                                        .map_err(rpc_error)?
                                        .into_inner(),
                                    ))
                                },
                                cx,
                            );
                        })),
                    );
                }
            }
        }
        body = body
            .child(self.field("title", "Chat title"))
            .child(
                button("rename-chat", "Rename")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let host = this.host.clone().unwrap();
                        let chat = this.chat.clone().unwrap();
                        let title = this.value("title", cx);
                        this.run(
                            async move {
                                host.mutate(None, move |channel, command| async move {
                                    p::catalog_service_client::CatalogServiceClient::new(channel)
                                        .rename_resource(p::RenameResourceRequest {
                                            command: Some(command),
                                            resource: Some(p::ResourceRef {
                                                resource: Some(p::resource_ref::Resource::Chat(
                                                    chat,
                                                )),
                                            }),
                                            title,
                                        })
                                        .await
                                })
                                .await?;
                                Ok(Update::Done("Chat renamed.".into()))
                            },
                            cx,
                        );
                    })),
            )
            .child(
                button("delete-chat", "Delete chat…")
                    .disabled(!self.ready())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let host = this.host.clone().unwrap();
                        let resource = Some(p::ResourceRef {
                            resource: this.chat.clone().map(p::resource_ref::Resource::Chat),
                        });
                        this.run(
                            async move {
                                let plan = p::catalog_service_client::CatalogServiceClient::new(
                                    host.channel()?,
                                )
                                .preflight_close(p::PreflightCloseRequest {
                                    resource: resource.clone(),
                                    delete: true,
                                })
                                .await
                                .map_err(rpc_error)?
                                .into_inner();
                                if !plan.can_execute {
                                    return Err(plan.reason);
                                }
                                let detail = plan
                                    .risks
                                    .iter()
                                    .map(|r| {
                                        format!(
                                            "{}: {} queued, {} viewers, {} dirty buffers",
                                            r.title,
                                            r.queued,
                                            r.observers,
                                            r.dirty.len()
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                Ok(Update::Confirm(
                                    "Delete chat on the host?".into(),
                                    detail,
                                    Box::pin(async move {
                                        host.mutate(None, move |channel, command| async move {
                                            p::catalog_service_client::CatalogServiceClient::new(
                                                channel,
                                            )
                                            .close_resource(p::CloseResourceRequest {
                                                command: Some(command),
                                                resource,
                                                plan_id: plan.plan_id,
                                            })
                                            .await
                                        })
                                        .await?;
                                        Ok(Update::Done(
                                            "Chat deleted on the host. Close this view.".into(),
                                        ))
                                    }),
                                ))
                            },
                            cx,
                        );
                    })),
            )
            .child(self.field("path", "Host path relative to this chat’s workspace"))
            .child(
                button("attach-host-path", "Attach host path")
                    .disabled(
                        !self.supports("embedded_context")
                            || self
                                .projection
                                .summary
                                .as_ref()
                                .and_then(|s| s.workspace.as_ref())
                                .is_none(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        let path = this.value("path", cx);
                        let workspace = this
                            .projection
                            .summary
                            .as_ref()
                            .and_then(|s| s.workspace.clone());
                        this.attachments.push(p::Attachment {
                            label: path.clone(),
                            content: Some(p::attachment::Content::Path(p::FileRef {
                                workspace,
                                relative_path: path,
                            })),
                        });
                        this.save_context_attachments(cx);
                        cx.notify();
                    })),
            )
            .child(self.render_composer_catalog(cx));
        body.into_any_element()
    }
    pub(super) fn refresh_markdown(&mut self, cx: &mut Context<Self>) {
        let mut retained = std::collections::BTreeSet::new();
        for item in &self.projection.items {
            let Some(key) = item.key else {
                continue;
            };
            let key = format!("{}:{}", key.history_epoch, key.item_id);
            retained.insert(key.clone());
            let view = self
                .markdown
                .entry(key)
                .or_insert_with(|| cx.new(super::super::markdown::MarkdownView::chat));
            let text = item_text(item);
            view.update(cx, |view, cx| {
                view.set_content(Arc::from(text), String::new(), true, cx)
            });
        }
        self.markdown.retain(|key, _| retained.contains(key));
        let parents = self
            .projection
            .items
            .iter()
            .filter_map(|item| match (&item.key, &item.body) {
                (Some(key), Some(p::transcript_item::Body::Tool(tool))) => {
                    Some((key.history_epoch, tool.call_id.clone()))
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        self.activity_children = self
            .projection
            .items
            .iter()
            .filter_map(|item| Some((item.key?.history_epoch, item.parent_call_id.clone()?)))
            .collect();
        self.chat_indices = self
            .projection
            .items
            .iter()
            .enumerate()
            .filter_map(|(ix, item)| {
                super::activity::visible(item, self.activity_thread.as_ref(), &parents)
                    .then_some(ix)
            })
            .collect();
        let rows = self
            .chat_indices
            .iter()
            .map(|ix| {
                let item = &self.projection.items[*ix];
                let key = item.key.unwrap_or_default();
                (key.history_epoch, key.item_id, item.revision)
            })
            .collect::<Vec<_>>();
        let prefix = self
            .chat_rows
            .iter()
            .zip(&rows)
            .take_while(|(old, new)| old == new)
            .count();
        let suffix = self
            .chat_rows
            .iter()
            .rev()
            .zip(rows.iter().rev())
            .take(self.chat_rows.len().min(rows.len()).saturating_sub(prefix))
            .take_while(|(old, new)| old == new)
            .count();
        if prefix + suffix != self.chat_rows.len() || prefix + suffix != rows.len() {
            let follow_tail = self.chat_list.is_scrolled_to_end() == Some(true);
            self.chat_list.splice(
                prefix..self.chat_rows.len() - suffix,
                rows.len() - prefix - suffix,
            );
            if follow_tail && !rows.is_empty() {
                self.chat_list.scroll_to_reveal_item(rows.len() - 1);
            }
        }
        self.chat_rows = rows;
    }
    fn render_chat_item(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self
            .chat_indices
            .get(index)
            .and_then(|ix| self.projection.items.get(*ix))
            .cloned()
        else {
            return div().into_any_element();
        };
        let mut body = column().gap_2().pb_3();
        if item.trimmed {
            body = body.child(
                div()
                    .text_color(t::secondary())
                    .child("Incomplete saved content — this item was trimmed on the host."),
            );
        }
        let key = item
            .key
            .map(|k| format!("{}:{}", k.history_epoch, k.item_id))
            .unwrap_or_default();
        let text = item_text(&item);
        if let Some(p::transcript_item::Body::Tool(tool)) = &item.body {
            body = body.child(self.render_activity(tool, &key, cx));
        } else if let Some(markdown) = self.markdown.get(&key) {
            body = body.child(markdown.clone());
        } else {
            body = body.child(div().child(text.clone()));
        }
        if let Some(p::transcript_item::Body::Decision(decision)) = &item.body
            && let Some(view) = self
                .decisions
                .get(&format!("{}:{}", decision.run_id, decision.decision_id))
        {
            body = body.child(view.clone());
        }
        if !text.is_empty() {
            body = body.child(
                button(SharedString::from(format!("quote:{key}")), "Quote")
                    .disabled(!self.supports("embedded_context"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.attachments.push(p::Attachment {
                            label: "Quote".into(),
                            content: Some(p::attachment::Content::Quote(p::QuoteAttachment {
                                text: text.clone(),
                            })),
                        });
                        this.save_context_attachments(cx);
                        cx.notify();
                    })),
            );
        }
        if let Some(p::transcript_item::Body::Shell(shell)) = &item.body {
            let shell = shell.clone();
            if shell.running
                && let Some(key) = item.key
            {
                body = body.child(
                    button(
                        SharedString::from(format!("stop-shell:{key:?}")),
                        "Stop shell command",
                    )
                    .disabled(!self.ready())
                    .on_click(cx.listener(move |this, _, _, cx| this.stop_shell(key, cx))),
                );
            }
            body = body.child(
                button(
                    SharedString::from(format!("shell-context:{key}")),
                    "Attach command output",
                )
                .disabled(!self.supports("embedded_context") || shell.running)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.attachments.push(p::Attachment {
                        label: shell.command.clone(),
                        content: Some(p::attachment::Content::Shell(p::ShellAttachment {
                            command: shell.command.clone(),
                            output: shell.output.clone(),
                            stopped: shell.stopped,
                            exit_code: shell.exit_code,
                        })),
                    });
                    this.save_context_attachments(cx);
                    cx.notify();
                })),
            );
        }
        if matches!(&item.body,Some(p::transcript_item::Body::TurnEnd(end)) if end.fork_available == Some(true))
            && !item.trimmed
            && self.supports("fork")
            && let Some(key) = item.key
        {
            let edited_disabled = !self.ready()
                || (self.composer.read(cx).value().trim().is_empty()
                    && self.attachments.is_empty()
                    && self.local_images.is_empty());
            body = body.child(
                row()
                    .gap_2()
                    .child(
                        button(
                            SharedString::from(format!("fork:{}", key.item_id)),
                            "Fork here",
                        )
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.fork_request = Some((key, false));
                            cx.notify();
                        })),
                    )
                    .child(
                        button(
                            SharedString::from(format!("fork-edit:{}", key.item_id)),
                            "Fork with draft",
                        )
                        .disabled(edited_disabled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.fork_request = Some((key, true));
                            cx.notify();
                        })),
                    ),
            );
        }
        if let Some(p::transcript_item::Body::User(user)) = &item.body {
            if let Some(key) = item.key {
                body = body.child(
                    button(
                        SharedString::from(format!("edit:{}:{}", key.history_epoch, key.item_id)),
                        "Edit message…",
                    )
                    .tooltip(self.capability_reason("edit"))
                    .disabled(
                        !self.ready()
                            || !self.supports("edit")
                            || item.trimmed
                            || self.editing.is_some(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.preview_edit(key, cx))),
                );
            }
            if let Some(draft) = &user.draft {
                for attachment in &draft.attachments {
                    if let Some(p::attachment::Content::Image(artifact)) = &attachment.content {
                        let artifact = artifact.clone();
                        let name = attachment.label.clone();
                        body = body.child(button(SharedString::from(format!("image:{}", artifact.artifact_id)), format!("Preview {name}"))
                            .disabled(!self.ready()).on_click(cx.listener(move |this, _, _, cx| {
                                let host = this.host.clone().unwrap(); let artifact = artifact.clone(); let name = name.clone();
                                this.run(async move { let bytes = host.download(artifact.clone()).await?; let file = canopy_desktop::integrations::attachments::PreviewFile::create(&name, &bytes)?; Ok(Update::Preview(Arc::new(file), artifact.media_type)) }, cx);
                            })));
                    }
                }
            }
        }
        if let Some(p::transcript_item::Body::Tool(tool)) = &item.body
            && tool.background
        {
            let call_id = tool.call_id.clone();
            body = body.child(
                button(
                    SharedString::from(format!("stop:{call_id}")),
                    "Stop background task",
                )
                .disabled(
                    !self.ready()
                        || !matches!(tool.state.as_str(), "running" | "streaming")
                        || item.key.is_none_or(|key| {
                            self.projection
                                .summary
                                .as_ref()
                                .is_none_or(|s| s.history_epoch != key.history_epoch)
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let host = this.host.clone().unwrap();
                    let chat = this.chat.clone();
                    let run_id = this
                        .projection
                        .summary
                        .as_ref()
                        .and_then(|s| s.run_id.clone())
                        .unwrap_or_default();
                    let call_id = call_id.clone();
                    this.run(
                        async move {
                            host.mutate(None, move |channel, command| async move {
                                p::chat_service_client::ChatServiceClient::new(channel)
                                    .stop_background(p::StopBackgroundRequest {
                                        command: Some(command),
                                        chat,
                                        run_id,
                                        call_id,
                                    })
                                    .await
                            })
                            .await?;
                            Ok(Update::Done("Background task stopped.".into()))
                        },
                        cx,
                    );
                })),
            );
        }
        body.into_any_element()
    }
    fn chat_command(&mut self, restart: bool, cx: &mut Context<Self>) {
        let host = self.host.clone().unwrap();
        let chat = self.chat.clone();
        let run_id = self
            .projection
            .summary
            .as_ref()
            .and_then(|s| s.run_id.clone());
        self.run(
            async move {
                host.mutate(None, move |channel, command| async move {
                    let mut client = p::chat_service_client::ChatServiceClient::new(channel);
                    let request = p::ChatCommandRequest {
                        command: Some(command),
                        chat,
                        run_id,
                    };
                    if restart {
                        client.restart(request).await
                    } else {
                        client.interrupt(request).await
                    }
                })
                .await?;
                Ok(Update::Done("Command accepted.".into()))
            },
            cx,
        );
    }
}
fn item_text(item: &p::TranscriptItem) -> String {
    use p::transcript_item::Body;
    match &item.body {
        Some(Body::User(v)) => v.draft.as_ref().map(|d| d.text.clone()).unwrap_or_default(),
        Some(Body::Text(v) | Body::Thinking(v)) => v.text.clone(),
        Some(Body::Tool(v)) => format!("{}\n{}", v.title, v.output.as_deref().unwrap_or_default()),
        Some(Body::Decision(v)) => v.title.clone(),
        Some(Body::Notice(v)) => v.text.clone(),
        Some(Body::TurnEnd(v)) => v.error.clone().unwrap_or(v.outcome.clone()),
        Some(Body::Shell(v)) => v.output.clone(),
        Some(Body::Goal(v)) => v.objective.clone(),
        None => String::new(),
    }
}

pub(super) struct DecisionView {
    host: Arc<HostClient>,
    chat: p::ChatRef,
    decision: p::Decision,
    global: ClientGlobal,
    answers: Vec<Entity<InputState>>,
    chosen: Vec<Vec<String>>,
    note: Entity<InputState>,
    accept_edits: bool,
    busy: bool,
    error: Option<String>,
}
impl DecisionView {
    fn respond(&mut self, response: p::respond_decision_request::Response, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let host = self.host.clone();
        let chat = self.chat.clone();
        let decision = self.decision.clone();
        let task = self.global.runtime.spawn(async move {
            host.mutate(None, move |channel, command| async move {
                p::chat_service_client::ChatServiceClient::new(channel)
                    .respond_decision(p::RespondDecisionRequest {
                        command: Some(command),
                        chat: Some(chat),
                        run_id: decision.run_id,
                        decision_id: decision.decision_id,
                        response: Some(response),
                    })
                    .await
            })
            .await
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.error = match result {
                    Ok(Ok(_)) => None,
                    Ok(Err(e)) => Some(e),
                    Err(_) => Some("Decision worker stopped.".into()),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn native(&mut self, action: i32, cx: &mut Context<Self>) {
        let answers: Vec<String> = self
            .decision
            .questions
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let text = self.answers[i].read(cx).value().trim().to_owned();
                if q.multiple {
                    let mut picked = self.chosen[i].clone();
                    if !text.is_empty() {
                        picked.push(text);
                    }
                    picked.join(", ")
                } else if !text.is_empty() {
                    text
                } else {
                    self.chosen[i].join(", ")
                }
            })
            .collect();
        if action == 4 && answers.iter().any(|answer| answer.trim().is_empty()) {
            self.error = Some("Answer each question or decline the request.".into());
            cx.notify();
            return;
        }
        self.respond(
            p::respond_decision_request::Response::Native(p::NativeDecision {
                action,
                answers,
                note: nonempty(self.note.read(cx).value().to_string()),
                accept_edits: self.accept_edits,
            }),
            cx,
        );
    }
}
impl Render for DecisionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.busy || self.host.phase() != Phase::Ready;
        let mut body = column()
            .gap_2()
            .child(self.decision.description.clone().unwrap_or_default());
        for option in self.decision.options.clone() {
            let id = option.id;
            body = body.child(
                button(SharedString::from(id.clone()), option.label)
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.respond(
                            p::respond_decision_request::Response::OptionId(id.clone()),
                            cx,
                        )
                    })),
            );
        }
        for (i, question) in self.decision.questions.iter().enumerate() {
            body = body.child(question.question.clone());
            for option in &question.options {
                let label = option.label.clone();
                let multiple = question.multiple;
                body = body.child(
                    button(
                        SharedString::from(format!("{i}:{}", option.id)),
                        format!(
                            "{}{}",
                            if self.chosen[i].contains(&label) {
                                "✓ "
                            } else {
                                ""
                            },
                            label
                        ),
                    )
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !multiple {
                            this.chosen[i].clear();
                        }
                        if let Some(pos) = this.chosen[i].iter().position(|v| v == &label) {
                            this.chosen[i].remove(pos);
                        } else {
                            this.chosen[i].push(label.clone());
                        }
                        cx.notify();
                    })),
                );
            }
            body = body.child(input(&self.answers[i]));
        }
        if self.decision.native_kind.as_deref() == Some("plan") {
            body = body.child(
                button(
                    "accept-edits",
                    if self.accept_edits {
                        "✓ Accept edits"
                    } else {
                        "Accept edits"
                    },
                )
                .disabled(disabled)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.accept_edits = !this.accept_edits;
                    cx.notify();
                })),
            );
        }
        body = body.child(input(&self.note));
        let actions = match self.decision.native_kind.as_deref() {
            Some("tool") => {
                if self.decision.can_remember {
                    vec![(1, "Allow once"), (2, "Allow for this chat"), (3, "Deny")]
                } else {
                    vec![(1, "Allow once"), (3, "Deny")]
                }
            }
            Some("question") => vec![(4, "Submit answers"), (3, "Decline")],
            Some("plan") => vec![(5, "Approve plan"), (6, "Keep planning")],
            _ => vec![],
        };
        for (action, label) in actions {
            body = body.child(
                button(label, label)
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| this.native(action, cx))),
            );
        }
        body.child(self.error.clone().unwrap_or_default())
    }
}
