use super::{
    editor::{Draft, Editor, Write},
    *,
};
use gpui_kit::component::WindowExt;

pub(super) struct SettingsView {
    host: Arc<HostClient>,
    device: Entity<device::DevicePreferences>,
    runtime: Arc<tokio::runtime::Runtime>,
    settings: Option<p::HostSettings>,
    devices: Vec<p::Device>,
    editor: Option<Entity<Editor>>,
    edit_revision: u64,
    reviewed: bool,
    busy: bool,
    error: Option<String>,
    notice: String,
    pending: Option<Task<()>>,
    abort: Option<tokio::task::AbortHandle>,
}
impl SettingsView {
    pub(super) fn new(
        host: Arc<HostClient>,
        runtime: Arc<tokio::runtime::Runtime>,
        device: Entity<device::DevicePreferences>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            host,
            device,
            runtime,
            settings: None,
            devices: vec![],
            editor: None,
            edit_revision: 0,
            reviewed: false,
            busy: false,
            error: None,
            notice: String::new(),
            pending: None,
            abort: None,
        };
        this.refresh(cx);
        this
    }
    fn ready(&self) -> bool {
        !self.busy && self.host.phase() == Phase::Ready
    }
    fn run<F>(&mut self, future: F, close_editor: bool, cx: &mut Context<Self>)
    where
        F: std::future::Future<Output = Result<(p::HostSettings, p::Devices, String), String>>
            + Send
            + 'static,
    {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        if let Some(e) = &self.editor {
            e.update(cx, |e, cx| e.enable(false, cx));
        }
        let job = self.runtime.spawn(future);
        self.abort = Some(job.abort_handle());
        self.pending=Some(cx.spawn(async move|this,cx|{
            let result=job.await.map_err(|e|format!("Settings operation interrupted: {e}. Query operation results before retrying a write.")).and_then(|v|v);
            let _=this.update(cx,|s,cx|{s.busy=false;s.abort=None;match result{Ok((settings,devices,notice))=>{s.settings=Some(settings);s.devices=devices.devices;s.notice=notice;if close_editor{s.editor=None;}},Err(e)=>s.error=Some(e)}if let Some(e)=&s.editor{e.update(cx,|e,cx|e.enable(true,cx));}cx.notify();});
        }));
        cx.notify();
    }
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.ready() {
            return;
        }
        let host = self.host.clone();
        self.run(
            async move { snapshot(host, "Host settings refreshed.").await },
            false,
            cx,
        );
    }
    fn edit(&mut self, draft: Draft, window: &mut Window, cx: &mut Context<Self>) {
        self.reviewed = false;
        self.edit_revision = self.settings.as_ref().map_or(0, |s| s.revision);
        self.editor =
            Some(cx.new(|cx| {
                Editor::new(draft, self.settings.clone().unwrap_or_default(), window, cx)
            }));
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready() {
            return;
        }
        let Some(editor) = &self.editor else { return };
        let result = if self.reviewed {
            self.settings
                .as_ref()
                .ok_or_else(|| "Reload host settings before saving.".to_owned())
                .and_then(|latest| editor.read(cx).write_reviewed(latest, cx))
        } else {
            editor.read(cx).write(cx)
        };
        let write = match result {
            Ok(w) => w,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        if let Write::Integration(ref request) = write
            && matches!(
                request.credential.as_ref().and_then(|s| s.action.as_ref()),
                Some(p::secret_update::Action::Remove(_))
            )
        {
            if let Some(i) = &request.integration {
                self.confirm_remove("integration", i.id.clone(), i.name.clone(), window, cx);
            }
            return;
        }
        let host = self.host.clone();
        let revision = self.edit_revision;
        self.run(
            async move {
                let receipt = write.send(host.clone(), revision).await?;
                let notice = receipt
                    .operation_output
                    .unwrap_or_else(|| "Settings saved.".into());
                snapshot(host, &notice).await.map_err(|e|format!("Settings were saved, but refresh failed: {e}. Refresh to inspect the result; do not repeat the write."))
            },
            true,
            cx,
        );
    }
    fn confirm_remove(
        &mut self,
        kind: &'static str,
        id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = if kind == "integration" {
            format!("Disconnect integration “{name}”?")
        } else if kind == "device" {
            format!("Revoke device “{name}”?")
        } else {
            format!("Delete {kind} “{name}”?")
        };
        let help = match kind {
            "integration" => {
                "The host credential will be removed. Linked tasks and projects are preserved. Reconnect this integration to use them again."
            }
            "device" => {
                "This device loses access to this host immediately, including active streams."
            }
            _ => {
                "Existing sessions are retained. This configuration will no longer be available for new sessions."
            }
        };
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let id = id.clone();
            dialog
                .title(title.clone())
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default()
                        .ok_text(if kind == "integration" {
                            "Disconnect"
                        } else if kind == "device" {
                            "Revoke"
                        } else {
                            "Delete"
                        })
                        .show_cancel(true),
                )
                .child(help)
                .on_ok(move |_, _, cx| {
                    let _ = owner.update(cx, |s, cx| s.remove(kind, id.clone(), cx));
                    true
                })
        });
    }
    fn remove(&mut self, kind: &'static str, id: String, cx: &mut Context<Self>) {
        if !self.ready() {
            return;
        }
        let host = self.host.clone();
        let rev = self.settings.as_ref().map(|s| s.revision);
        self.run(
            async move {
                host.mutate(
                    if kind == "device" { None } else { rev },
                    move |channel, command| async move {
                        let mut client =
                            p::settings_service_client::SettingsServiceClient::new(channel.clone());
                        match kind {
                            "agent" => {
                                client
                                    .remove_agent_definition(p::RemoveAgentDefinitionRequest {
                                        command: Some(command),
                                        agent_id: id,
                                    })
                                    .await
                            }
                            "profile" => {
                                client
                                    .remove_profile(p::RemoveProfileRequest {
                                        command: Some(command),
                                        profile_id: id,
                                    })
                                    .await
                            }
                            "integration" => {
                                client
                                    .remove_integration(p::RemoveIntegrationRequest {
                                        command: Some(command),
                                        integration_id: id,
                                    })
                                    .await
                            }
                            _ => {
                                p::host_service_client::HostServiceClient::new(channel)
                                    .revoke_device(p::RevokeDeviceRequest {
                                        command: Some(command),
                                        device_id: id,
                                    })
                                    .await
                            }
                        }
                    },
                )
                .await?;
                snapshot(host, "Change applied.").await
            },
            true,
            cx,
        );
    }
    fn review_epoch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.entity().downgrade();
        window.open_dialog(cx,move|dialog,_,_|{let owner=owner.clone();dialog.title("Retire reviewed command epoch?").button_props(gpui_kit::component::dialog::DialogButtonProps::default().ok_text("Retire epoch").show_cancel(true))
            .child("Review every operation above first. Unknown outcomes remain unknown: retirement does not undo effects or prove failure. Old outcomes are preserved and commands will never be replayed. Active operations or incomplete uploads can still block retirement. Unsent messages remain available.")
            .on_ok(move|_,_,cx|{let _=owner.update(cx,|s,cx|{if !s.ready(){return;}let host=s.host.clone();s.run(async move{host.close_epoch_after_review().await?;snapshot(host,"Reviewed epoch retired. Unknown outcomes remain unknown.").await},false,cx)});true})});
    }
}
async fn snapshot(
    host: Arc<HostClient>,
    notice: &str,
) -> Result<(p::HostSettings, p::Devices, String), String> {
    let request = p::HostRequest {
        host_id: host.id().as_str().into(),
    };
    let settings = p::settings_service_client::SettingsServiceClient::new(host.channel()?)
        .get_settings(request.clone())
        .await
        .map_err(rpc_error)?
        .into_inner();
    let mut devices = p::Devices::default();
    let mut page_token = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    loop {
        let page = p::host_service_client::HostServiceClient::new(host.channel()?)
            .list_devices(p::DeviceListRequest {
                host_id: request.host_id.clone(),
                page_token,
                include_revoked: true,
            })
            .await
            .map_err(rpc_error)?
            .into_inner();
        bytes = bytes.saturating_add(prost::Message::encoded_len(&page));
        if bytes > 32 * 1024 * 1024 {
            return Err(
                "Device history exceeds the 32 MiB review budget. Narrow the registry on the host."
                    .into(),
            );
        }
        devices.devices.extend(page.devices);
        if devices.devices.len() > 4096 {
            return Err(
                "Device history exceeds the review limit. Narrow the registry on the host.".into(),
            );
        }
        page_token = page.next_page_token;
        match &page_token {
            Some(token) if !seen.insert(token.clone()) => {
                return Err("Device pagination repeated a cursor.".into());
            }
            None => break,
            _ => {}
        }
    }
    Ok((settings, devices, notice.into()))
}
impl Drop for SettingsView {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
    }
}
impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.ready();
        let mut body = column()
            .gap_4()
            .child(
                row()
                    .gap_2()
                    .child(div().flex_1().child("Host settings"))
                    .child(
                        button("refresh", "Refresh")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| s.refresh(cx))),
                    ),
            )
            .child(
                "Changes apply to this host. Device layout, navigation and zoom remain separate.",
            )
            .children(self.busy.then(|| div().child("Loading…")))
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_color(t::red()).child(e)),
            )
            .children(
                (!self.notice.is_empty())
                    .then(|| div().text_color(t::muted()).child(self.notice.clone())),
            );
        if let Some(editor) = &self.editor {
            if let Some(latest) = &self.settings
                && latest.revision != self.edit_revision
            {
                body=body.child(format!("Host changed from revision {} to {}. Review the current metadata before keeping your edits.",self.edit_revision,latest.revision));
                for a in &latest.agents {
                    body = body.child(format!("Agent {}: {} · {}", a.id, a.name, a.command));
                }
                for p in &latest.profiles {
                    body = body.child(format!(
                        "Profile {}: {} · model {}",
                        p.id,
                        p.name,
                        p.model.as_deref().unwrap_or("Default")
                    ));
                }
                for i in &latest.integrations {
                    body = body.child(format!("Integration {}: {} · {}", i.id, i.name, i.endpoint));
                }
                body=body.child(button("review-revision","Keep edits against reviewed revision").disabled(disabled).on_click(cx.listener(|s,_,_,cx|{if let Some(latest)=&s.settings{s.edit_revision=latest.revision;s.reviewed=true;s.notice="Only edited metadata will replace the reviewed host values. Secret actions remain explicit.".into();}cx.notify();})));
            }
            return body.child(editor.clone()).child(row().gap_2()
                .child(button("save","Save").disabled(disabled).on_click(cx.listener(|s,_,w,cx|s.save(w,cx))))
                .child(button("cancel","Cancel editing").disabled(self.busy).on_click(cx.listener(|s,_,_,cx|{s.editor=None;cx.notify();}))))
                .child("A revision conflict preserves this draft. Refresh and compare the host settings before saving again.");
        }
        if let Some(settings) = self.settings.clone() {
            if let Some(p) = settings.preferences {
                body = body.child(
                    button("preferences", "Edit host preferences…")
                        .disabled(disabled)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.edit(Draft::Preferences(p.clone()), w, cx)
                        })),
                );
            }
            body = body.child(
                row().gap_2().child("Agents").child(
                    button("add-agent", "Add agent…")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, w, cx| {
                            s.edit(
                                Draft::Agent(p::AgentDefinition {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    backend_kind: 3,
                                    enabled: Some(true),
                                    ..Default::default()
                                }),
                                w,
                                cx,
                            )
                        })),
                ),
            );
            for a in settings.agents.iter() {
                let a = a.clone();
                let remove = a.clone();
                body = body.child(
                    row()
                        .gap_2()
                        .child(
                            button(
                                SharedString::from(format!("agent:{}", a.id)),
                                format!("{} · {}", a.name, a.command),
                            )
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |s, _, w, cx| s.edit(Draft::Agent(a.clone()), w, cx),
                            )),
                        )
                        .child(
                            button(
                                SharedString::from(format!("remove-agent:{}", remove.id)),
                                "Delete…",
                            )
                            .disabled(disabled || remove.backend_kind != 3)
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    s.confirm_remove(
                                        "agent",
                                        remove.id.clone(),
                                        remove.name.clone(),
                                        w,
                                        cx,
                                    )
                                },
                            )),
                        ),
                );
            }
            body = body.child(
                row().gap_2().child("Profiles").child(
                    button("add-profile", "Add profile…")
                        .disabled(disabled || settings.agents.is_empty())
                        .on_click(cx.listener(|s, _, w, cx| {
                            if let Some(a) =
                                s.settings.as_ref().and_then(|s| s.agents.first()).cloned()
                            {
                                s.edit(
                                    Draft::Profile(
                                        p::Profile {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            agent_id: a.id,
                                            ..Default::default()
                                        },
                                        a.backend_kind == 3,
                                    ),
                                    w,
                                    cx,
                                );
                            }
                        })),
                ),
            );
            for p in settings.profiles {
                let acp = settings
                    .agents
                    .iter()
                    .find(|a| a.id == p.agent_id)
                    .is_some_and(|a| a.backend_kind == 3);
                let remove = p.clone();
                body = body.child(
                    row()
                        .gap_2()
                        .child(
                            button(
                                SharedString::from(format!("profile:{}", p.id)),
                                p.name.clone(),
                            )
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |s, _, w, cx| s.edit(Draft::Profile(p.clone(), acp), w, cx),
                            )),
                        )
                        .child(
                            button(
                                SharedString::from(format!("remove-profile:{}", remove.id)),
                                "Delete…",
                            )
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    s.confirm_remove(
                                        "profile",
                                        remove.id.clone(),
                                        remove.name.clone(),
                                        w,
                                        cx,
                                    )
                                },
                            )),
                        ),
                );
            }
            body = body.child(
                row().gap_2().child("Integrations").child(
                    button("add-integration", "Connect integration…")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, w, cx| {
                            s.edit(
                                Draft::Integration(p::Integration {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    kind: "github".into(),
                                    ..Default::default()
                                }),
                                w,
                                cx,
                            )
                        })),
                ),
            );
            for i in settings.integrations {
                let remove = i.clone();
                body = body.child(
                    row()
                        .gap_2()
                        .child(
                            button(
                                SharedString::from(format!("integration:{}", i.id)),
                                format!(
                                    "{} · {} · {}",
                                    i.name,
                                    i.kind,
                                    if i.credential_configured {
                                        "Connected"
                                    } else {
                                        "Disconnected"
                                    }
                                ),
                            )
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |s, _, w, cx| s.edit(Draft::Integration(i.clone()), w, cx),
                            )),
                        )
                        .child(
                            button(
                                SharedString::from(format!("disconnect:{}", remove.id)),
                                "Disconnect…",
                            )
                            .disabled(disabled || !remove.credential_configured)
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    s.confirm_remove(
                                        "integration",
                                        remove.id.clone(),
                                        remove.name.clone(),
                                        w,
                                        cx,
                                    )
                                },
                            )),
                        ),
                );
            }
        }
        body = body.child(self.device.clone()).child("Authorized devices");
        let current = self.host.snapshot().record.device_id().to_owned();
        for d in self.devices.clone() {
            body = body.child(
                column()
                    .gap_2()
                    .child(format!(
                        "{}{} · {}",
                        d.name,
                        if d.device_id == current {
                            " (this device)"
                        } else {
                            ""
                        },
                        if d.revoked { "Revoked" } else { "Active" }
                    ))
                    .child(format!("Scopes: {}", d.scopes.join(", ")))
                    .child(format!("Roots: {}", d.roots.join(", ")))
                    .child(
                        button(
                            SharedString::from(format!("revoke:{}", d.device_id)),
                            "Revoke device…",
                        )
                        .disabled(disabled || d.revoked)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.confirm_remove("device", d.device_id.clone(), d.name.clone(), w, cx)
                        })),
                    ),
            );
        }
        body = body.child("Operation recovery");
        for (id, result) in self.host.recovery() {
            let state = result
                .as_ref()
                .and_then(|r| p::OperationState::try_from(r.state).ok())
                .map(|s| s.as_str_name().to_owned())
                .unwrap_or_else(|| "Unknown — query the host; do not repeat the operation.".into());
            body = body.child(format!("{id}: {state}")).children(
                result
                    .and_then(|r| r.error)
                    .map(|e| div().text_color(t::red()).child(e.message)),
            );
        }
        body.child(
            row()
                .gap_2()
                .child(
                    button("reconcile", "Query operation results")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, _, cx| {
                            let host = s.host.clone();
                            s.run(
                                async move {
                                    host.reconcile().await?;
                                    snapshot(host, "Operation results refreshed.").await
                                },
                                false,
                                cx,
                            )
                        })),
                )
                .child(
                    button("review-epoch", "Retire reviewed epoch…")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, w, cx| s.review_epoch(w, cx))),
                ),
        )
    }
}
