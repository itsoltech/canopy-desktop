use super::{
    editor::{Draft, Editor, Write},
    *,
};
use canopy_desktop::integrations::Provider;
use canopy_desktop::{api::proto as api, remote::client::SettingsMutation};
use gpui_kit::component::{IconName, button::ButtonVariants};

pub(super) struct SettingsView {
    host: Arc<HostClient>,
    device: Entity<device::DevicePreferences>,
    runtime: Arc<tokio::runtime::Runtime>,
    settings: Option<api::HostSettings>,
    devices: Vec<api::Device>,
    editor: Option<Entity<Editor>>,
    edit_revision: u64,
    reviewed: bool,
    busy: bool,
    error: Option<String>,
    notice: String,
    pending: Option<Task<()>>,
    abort: Option<tokio::task::AbortHandle>,
    focus: FocusHandle,
    diagnostics: Disclosure,
    confirmation: Option<Entity<modal::Confirmation>>,
    confirmation_events: Option<Subscription>,
}
/// A settings record row: icon, name and detail; clicking edits it.
fn record_row(
    id: impl Into<ElementId>,
    glyph: impl IntoElement,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    first: bool,
) -> Stateful<Div> {
    let title: SharedString = title.into();
    row()
        .id(id)
        .keyboard(format!("Edit {title}"))
        .gap(px(12.))
        .py(px(10.))
        .px(px(8.))
        .rounded(px(4.))
        .border_t(px(if first { 0. } else { 1. }))
        .border_color(t::border())
        .cursor_pointer()
        .hover(|style| style.bg(t::hover()))
        .child(div().flex_shrink_0().child(glyph))
        .child(
            column()
                .flex_1()
                .min_w_0()
                .gap(px(3.))
                .child(div().truncate().text_size(px(13.)).child(title))
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.))
                        .text_color(t::muted())
                        .child(detail.into()),
                ),
        )
}
fn operation_state(state: Option<&p::CommandReceipt>) -> &'static str {
    match state
        .and_then(|r| p::OperationState::try_from(r.state).ok())
        .map(|s| s.as_str_name())
    {
        Some("ACCEPTED") => "Accepted",
        Some("ATTEMPTING") => "In progress",
        Some("SUCCEEDED") => "Succeeded",
        Some("FAILED") => "Failed",
        Some("REJECTED") => "Rejected",
        Some("DELIVERY_UNKNOWN") => "Outcome unknown",
        Some("COMMAND_RETIRED") => "Cleared",
        Some("NOT_FOUND") => "Not found on the computer",
        _ => "Unknown. Check results; do not repeat the operation.",
    }
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
            focus: cx.focus_handle(),
            diagnostics: Disclosure::new(false, Instant::now()),
            confirmation: None,
            confirmation_events: None,
        };
        this.refresh(cx);
        this
    }
    fn ready(&self) -> bool {
        !self.busy && self.host.phase() == Phase::Ready
    }
    fn run<F>(&mut self, future: F, close_editor: bool, cx: &mut Context<Self>)
    where
        F: std::future::Future<Output = Result<(api::HostSettings, api::Devices, String), String>>
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
        self.pending = Some(cx.spawn(async move |this, cx| {
            let result = job
                .await
                .map_err(|e| {
                    format!(
                        "Settings operation interrupted: {e}. Check operation results before retrying a change."
                    )
                })
                .and_then(|v| v);
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                s.abort = None;
                match result {
                    Ok((settings, devices, notice)) => {
                        s.settings = Some(settings);
                        s.devices = devices.devices;
                        s.notice = notice;
                        if close_editor {
                            s.editor = None;
                        }
                    }
                    Err(e) => s.error = Some(e),
                }
                if let Some(e) = &s.editor {
                    e.update(cx, |e, cx| e.enable(true, cx));
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.ready() {
            return;
        }
        let host = self.host.clone();
        self.run(async move { snapshot(host, "").await }, false, cx);
    }
    fn edit(&mut self, draft: Draft, window: &mut Window, cx: &mut Context<Self>) {
        self.reviewed = false;
        self.notice.clear();
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
                Some(api::secret_update::Action::Remove(_))
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
                let notice = match receipt.result {
                    Some(api::command_receipt::Result::Preferences(_)) => "Preferences saved.",
                    _ => "Settings saved.",
                };
                snapshot(host, notice).await.map_err(|e| {
                    format!(
                        "Settings were saved, but refreshing failed: {e}. Refresh to check the result; do not repeat the change."
                    )
                })
            },
            true,
            cx,
        );
    }
    fn open_confirmation(
        &mut self,
        title: String,
        message: &'static str,
        label: &'static str,
        accept: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation.is_some() {
            return;
        }
        let owner = cx.entity().downgrade();
        let focus = self.focus.clone();
        let modal = cx.new(|cx| {
            modal::Confirmation::new(
                title,
                message,
                label,
                focus,
                move |cx| {
                    owner
                        .update(cx, |s, cx| {
                            if !s.ready() {
                                return Err(SharedString::from(
                                    "Wait for the current operation to finish, or reconnect.",
                                ));
                            }
                            accept(s, cx);
                            Ok(())
                        })
                        .unwrap_or_else(|_| Err("Settings were closed.".into()))
                },
                window,
                cx,
            )
            .destructive()
        });
        self.confirmation_events = Some(cx.subscribe(&modal, |s, _, _: &ModalDismissed, cx| {
            s.confirmation = None;
            s.confirmation_events = None;
            cx.notify();
        }));
        self.confirmation = Some(modal);
        cx.notify();
    }
    fn confirm_remove(
        &mut self,
        kind: &'static str,
        id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = match kind {
            "integration" => format!("Disconnect integration “{name}”?"),
            "device" => format!("Revoke device “{name}”?"),
            _ => format!("Delete {kind} “{name}”?"),
        };
        let (help, label) = match kind {
            "integration" => (
                "The credential is removed from the other computer. Linked tasks and projects are kept; reconnect to use them again.",
                "Disconnect",
            ),
            "device" => (
                "This device loses access to the computer immediately, including open sessions.",
                "Revoke",
            ),
            _ => (
                "Existing sessions keep running. New sessions can no longer use it.",
                "Delete",
            ),
        };
        self.open_confirmation(
            title,
            help,
            label,
            move |s, cx| s.remove(kind, id.clone(), cx),
            window,
            cx,
        );
    }
    fn remove(&mut self, kind: &'static str, id: String, cx: &mut Context<Self>) {
        if !self.ready() {
            return;
        }
        let host = self.host.clone();
        let rev = self.settings.as_ref().map(|s| s.revision);
        self.run(
            async move {
                if kind == "device" {
                    host.revoke_device(id).await?;
                } else {
                    let request = match kind {
                        "agent" => {
                            SettingsMutation::RemoveAgent(api::RemoveAgentDefinitionRequest {
                                agent_id: id,
                                ..Default::default()
                            })
                        }
                        "profile" => SettingsMutation::RemoveProfile(api::RemoveProfileRequest {
                            profile_id: id,
                            ..Default::default()
                        }),
                        _ => SettingsMutation::RemoveIntegration(api::RemoveIntegrationRequest {
                            integration_id: id,
                            ..Default::default()
                        }),
                    };
                    host.update_settings(request, rev.ok_or("Settings revision is unavailable.")?)
                        .await?;
                }
                snapshot(host, "Change applied.").await
            },
            true,
            cx,
        );
    }
    fn review_epoch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_confirmation(
            "Clear reviewed operations?".into(),
            "Review the operations above first. An unknown outcome stays unknown: clearing does not undo its effects or prove it failed, and nothing is sent again. Running operations or unfinished uploads can prevent this. Unsent messages stay available.",
            "Clear operations",
            |s, cx| {
                let host = s.host.clone();
                s.run(
                    async move {
                        host.close_epoch_after_review().await?;
                        snapshot(
                            host,
                            "Reviewed operations cleared. Unknown outcomes stay unknown.",
                        )
                        .await
                    },
                    false,
                    cx,
                )
            },
            window,
            cx,
        );
    }
    fn status_line(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        row()
            .gap(px(8.))
            .min_h(px(28.))
            .child(div().flex_1().min_w_0().children(if self.busy {
                Some(chrome::loading("Loading settings…").into_any_element())
            } else if !self.notice.is_empty() {
                Some(chrome::done(self.notice.clone()).into_any_element())
            } else if self.host.phase() != Phase::Ready {
                Some(
                    div()
                        .text_size(px(12.))
                        .text_color(t::muted())
                        .child("Connect to change settings.")
                        .into_any_element(),
                )
            } else {
                None
            }))
            .child(
                icon_button("refresh", IconName::RotateCw, "Refresh settings")
                    .loading(self.busy)
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| s.refresh(cx))),
            )
    }
    fn render_editor(&self, editor: &Entity<Editor>, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let conflict = self
            .settings
            .as_ref()
            .filter(|latest| latest.revision != self.edit_revision)
            .map(|latest| {
                let mut lines = vec![
                    "These settings changed on the other computer while you were editing. Review the current values before keeping your edits.".to_owned(),
                ];
                lines.extend(
                    latest
                        .agents
                        .iter()
                        .map(|a| format!("• Agent {} — {}", a.name, a.command)),
                );
                lines.extend(latest.profiles.iter().map(|p| {
                    format!(
                        "• Profile {} — model {}",
                        p.name,
                        p.model.as_deref().filter(|m| !m.is_empty()).unwrap_or("default")
                    )
                }));
                lines.extend(
                    latest
                        .integrations
                        .iter()
                        .map(|i| format!("• Integration {} — {}", i.name, i.endpoint)),
                );
                column()
                    .gap(px(8.))
                    .child(chrome::hint(lines.join("\n")))
                    .child(
                        row().child(
                            button("review-revision", "Keep my edits")
                                .disabled(disabled)
                                .on_click(cx.listener(|s, _, _, cx| {
                                    if let Some(latest) = &s.settings {
                                        s.edit_revision = latest.revision;
                                        s.reviewed = true;
                                        s.notice = "Only the fields you edited replace the current values. Secrets change only when you choose so.".into();
                                    }
                                    cx.notify();
                                })),
                        ),
                    )
            });
        column()
            .gap(px(16.))
            .p(px(20.))
            .rounded(px(8.))
            .border_1()
            .border_color(t::border())
            .bg(t::panel())
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(editor.read(cx).title()),
            )
            .children(conflict)
            .child(editor.clone())
            .children(self.error.clone().map(chrome::failure))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child("If someone else changes these settings first, your draft is kept for review."),
                    )
                    .child(
                        button("cancel", "Cancel")
                            .disabled(self.busy)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.editor = None;
                                s.error = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        primary_button("save", "Save")
                            .loading(self.busy)
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, w, cx| s.save(w, cx))),
                    ),
            )
    }
}
async fn snapshot(
    host: Arc<HostClient>,
    notice: &str,
) -> Result<(api::HostSettings, api::Devices, String), String> {
    let request = api::GetInfoRequest {
        environment: Some(api::EnvironmentRef {
            host_id: host.id().as_str().into(),
        }),
    };
    let settings = api::settings_service_client::SettingsServiceClient::new(host.channel()?)
        .get_settings(api::GetSettingsRequest {
            environment: request.environment.clone(),
        })
        .await
        .map_err(rpc_error)?
        .into_inner();
    let mut devices = api::Devices::default();
    let mut page_token = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    loop {
        let page = api::host_service_client::HostServiceClient::new(host.channel()?)
            .list_devices(api::ListDevicesRequest {
                environment: request.environment.clone(),
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        canopy_desktop::motion::request_frame(window, self.diagnostics.active(now));
        let disabled = !self.ready();
        let mut body = column()
            .track_focus(&self.focus)
            .w_full()
            .max_w(px(720.))
            .gap(px(24.))
            .child(self.status_line(cx))
            .children(self.confirmation.clone());
        if let Some(editor) = self.editor.clone() {
            return body.child(self.render_editor(&editor, cx));
        }
        body = body.children(self.error.clone().map(chrome::failure));
        let Some(settings) = self.settings.clone() else {
            if !self.busy {
                body = body.child(chrome::empty(
                    "Settings not loaded",
                    if self.host.phase() == Phase::Ready {
                        "Refresh to load this computer’s settings."
                    } else {
                        "Connect to this computer to view and change its settings."
                    },
                ));
            }
            return body.child(self.device.clone());
        };
        if let Some(prefs) = settings.preferences.clone() {
            body = body.child(preference_section(
                "PREFERENCES",
                setting(
                    "Defaults on this computer",
                    format!(
                        "New tab: {} · New worktree: {} · Notch {}",
                        if prefs.new_tab_tool.is_empty() {
                            "default"
                        } else {
                            &prefs.new_tab_tool
                        },
                        if prefs.new_worktree_tool.is_empty() {
                            "default"
                        } else {
                            &prefs.new_worktree_tool
                        },
                        if prefs.notch_enabled { "on" } else { "off" }
                    ),
                    true,
                    button("preferences", "Edit…")
                        .disabled(disabled)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.edit(Draft::Preferences(prefs.clone()), w, cx)
                        })),
                ),
            ));
        }
        let mut agents = column();
        for (index, a) in settings.agents.iter().enumerate() {
            let edit = a.clone();
            let remove = a.clone();
            agents = agents.child(
                record_row(
                    SharedString::from(format!("agent:{}", a.id)),
                    icon(IconName::Bot),
                    a.name.clone(),
                    format!(
                        "{}{}",
                        a.command,
                        if a.enabled == Some(false) {
                            " · Disabled"
                        } else {
                            ""
                        }
                    ),
                    index == 0,
                )
                .when(!disabled, |row| {
                    row.on_click(
                        cx.listener(move |s, _, w, cx| s.edit(Draft::Agent(edit.clone()), w, cx)),
                    )
                })
                .child(
                    icon_button(
                        SharedString::from(format!("remove-agent:{}", remove.id)),
                        IconName::Close,
                        if matches!(remove.backend_kind, 3 | 5) {
                            "Delete agent…"
                        } else {
                            "Built-in agents cannot be deleted"
                        },
                    )
                    .disabled(disabled || !matches!(remove.backend_kind, 3 | 5))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        cx.stop_propagation();
                        s.confirm_remove("agent", remove.id.clone(), remove.name.clone(), w, cx)
                    })),
                ),
            );
        }
        body = body.child(
            column()
                .gap(px(8.))
                .child(
                    chrome::section_title("AGENTS").child(
                        button("add-agent", "Add agent…")
                            .icon(icon(IconName::Plus))
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.edit(
                                    Draft::Agent(api::AgentDefinition {
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
                )
                .child(agents),
        );
        let mut profiles = column();
        if settings.profiles.is_empty() {
            profiles = profiles.child(
                div()
                    .px(px(8.))
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("No profiles. Add one to save a model, credentials and options."),
            );
        }
        for (index, profile) in settings.profiles.iter().enumerate() {
            let agent = settings.agents.iter().find(|a| a.id == profile.agent_id);
            let acp = agent.is_some_and(|a| a.backend_kind == 3);
            let edit = profile.clone();
            let remove = profile.clone();
            profiles = profiles.child(
                record_row(
                    SharedString::from(format!("profile:{}", profile.id)),
                    icon(IconName::CircleUser),
                    profile.name.clone(),
                    format!(
                        "{} · model {}",
                        agent.map(|a| a.name.as_str()).unwrap_or(&profile.agent_id),
                        profile
                            .model
                            .as_deref()
                            .filter(|m| !m.is_empty())
                            .unwrap_or("default")
                    ),
                    index == 0,
                )
                .when(!disabled, |row| {
                    row.on_click(cx.listener(move |s, _, w, cx| {
                        s.edit(Draft::Profile(edit.clone(), acp), w, cx)
                    }))
                })
                .children(
                    profile
                        .credential_configured
                        .then(|| badge("Credential saved")),
                )
                .child(
                    icon_button(
                        SharedString::from(format!("remove-profile:{}", remove.id)),
                        IconName::Close,
                        "Delete profile…",
                    )
                    .disabled(disabled)
                    .on_click(cx.listener(move |s, _, w, cx| {
                        cx.stop_propagation();
                        s.confirm_remove("profile", remove.id.clone(), remove.name.clone(), w, cx)
                    })),
                ),
            );
        }
        body = body.child(
            column()
                .gap(px(8.))
                .child(
                    chrome::section_title("PROFILES").child(
                        button("add-profile", "Add profile…")
                            .icon(icon(IconName::Plus))
                            .disabled(disabled || settings.agents.is_empty())
                            .on_click(cx.listener(|s, _, w, cx| {
                                if let Some(a) =
                                    s.settings.as_ref().and_then(|s| s.agents.first()).cloned()
                                {
                                    s.edit(
                                        Draft::Profile(
                                            api::Profile {
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
                )
                .child(profiles),
        );
        let mut integrations = column();
        if settings.integrations.is_empty() {
            integrations = integrations.child(
                div()
                    .px(px(8.))
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("No integrations. Connect GitHub, Jira or YouTrack to browse tasks."),
            );
        }
        for (index, integration) in settings.integrations.iter().enumerate() {
            let provider = match integration.kind.as_str() {
                "jira" => Provider::Jira,
                "youtrack" => Provider::Youtrack,
                _ => Provider::Github,
            };
            let edit = integration.clone();
            let remove = integration.clone();
            integrations = integrations.child(
                record_row(
                    SharedString::from(format!("integration:{}", integration.id)),
                    integrations::provider_icon(provider),
                    integration.name.clone(),
                    if integration.endpoint.is_empty() {
                        integration.kind.clone()
                    } else {
                        integration.endpoint.clone()
                    },
                    index == 0,
                )
                .when(!disabled, |row| {
                    row.on_click(cx.listener(move |s, _, w, cx| {
                        s.edit(Draft::Integration(edit.clone()), w, cx)
                    }))
                })
                .child(badge(if integration.credential_configured {
                    "Connected"
                } else {
                    "Not connected"
                }))
                .child(
                    icon_button(
                        SharedString::from(format!("disconnect:{}", remove.id)),
                        IconName::Close,
                        "Disconnect…",
                    )
                    .disabled(disabled || !remove.credential_configured)
                    .on_click(cx.listener(move |s, _, w, cx| {
                        cx.stop_propagation();
                        s.confirm_remove(
                            "integration",
                            remove.id.clone(),
                            remove.name.clone(),
                            w,
                            cx,
                        )
                    })),
                ),
            );
        }
        body = body.child(
            column()
                .gap(px(8.))
                .child(
                    chrome::section_title("INTEGRATIONS").child(
                        button("add-integration", "Connect integration…")
                            .icon(icon(IconName::Plus))
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.edit(
                                    Draft::Integration(api::Integration {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        kind: "github".into(),
                                        ..Default::default()
                                    }),
                                    w,
                                    cx,
                                )
                            })),
                    ),
                )
                .child(integrations),
        );
        body = body.child(self.device.clone());
        let current = self.host.snapshot().record.device_id().to_owned();
        let mut devices = column();
        for (index, d) in self.devices.clone().into_iter().enumerate() {
            let this_device = d.device_id == current;
            devices = devices.child(
                row()
                    .gap(px(12.))
                    .py(px(10.))
                    .px(px(8.))
                    .border_t(px(if index == 0 { 0. } else { 1. }))
                    .border_color(t::border())
                    .when(d.revoked, |row| row.opacity(0.6))
                    .child(icon(IconName::HardDrive).flex_shrink_0())
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .gap(px(3.))
                            .child(
                                row()
                                    .gap(px(8.))
                                    .child(
                                        div().truncate().text_size(px(13.)).child(d.name.clone()),
                                    )
                                    .children(this_device.then(|| badge("This device"))),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child(if d.revoked { "Revoked" } else { "Has access" }),
                            ),
                    )
                    .child(
                        button(
                            SharedString::from(format!("revoke:{}", d.device_id)),
                            "Revoke…",
                        )
                        .ghost()
                        .border_0()
                        .text_color(t::red())
                        .disabled(disabled || d.revoked)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.confirm_remove("device", d.device_id.clone(), d.name.clone(), w, cx)
                        })),
                    ),
            );
        }
        body = body.child(preference_section("AUTHORIZED DEVICES", devices));
        let recovery = self.host.recovery();
        let mut operations = column().gap(px(4.));
        if recovery.is_empty() {
            operations = operations.child(
                div()
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("No operations need review."),
            );
        }
        for (id, result) in recovery {
            operations = operations.child(
                column()
                    .gap(px(2.))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(t::MONO)
                                    .text_size(px(11.))
                                    .text_color(t::faint())
                                    .child(id.chars().take(8).collect::<String>()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(12.))
                                    .text_color(t::secondary())
                                    .child(operation_state(result.as_ref())),
                            ),
                    )
                    .children(result.and_then(|r| r.error).map(|e| {
                        div()
                            .text_size(px(11.))
                            .text_color(t::red())
                            .child(e.message)
                    })),
            );
        }
        let access = column().gap(px(4.)).children(self.devices.iter().map(|d| {
            div()
                .text_size(px(11.))
                .text_color(t::muted())
                .child(format!(
                    "{} — access: {}; folders: {}",
                    d.name,
                    if d.scopes.is_empty() {
                        "none".to_owned()
                    } else {
                        d.scopes
                            .iter()
                            .filter_map(|scope| api::Scope::try_from(*scope).ok())
                            .map(|scope| {
                                scope.as_str_name().trim_start_matches("SCOPE_").to_owned()
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                    if d.roots.is_empty() {
                        "all".to_owned()
                    } else {
                        d.roots.join(", ")
                    }
                ))
        }));
        let diagnostics = column()
            .gap(px(16.))
            .pb(px(8.))
            .child(
                column()
                    .gap(px(8.))
                    .child(caption("UNCERTAIN OPERATIONS"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child("Changes this device sent while the connection was unreliable. Check them before repeating anything."),
                    )
                    .child(operations)
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                button("reconcile", "Check results")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        let host = s.host.clone();
                                        s.run(
                                            async move {
                                                host.reconcile().await?;
                                                snapshot(host, "Operation results updated.").await
                                            },
                                            false,
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                button("review-epoch", "Clear reviewed operations…")
                                    .ghost()
                                    .border_0()
                                    .text_color(t::red())
                                    .disabled(disabled)
                                    .on_click(cx.listener(|s, _, w, cx| s.review_epoch(w, cx))),
                            ),
                    ),
            )
            .child(
                column()
                    .gap(px(8.))
                    .child(caption("DEVICE ACCESS"))
                    .child(access),
            );
        body.child(
            column()
                .border_t_1()
                .border_color(t::border())
                .child(
                    self.diagnostics
                        .header("settings-diagnostics", "DIAGNOSTICS", now, cx)
                        .on_hover(cx.listener(|s, hovered, _, cx| {
                            s.diagnostics.hovered = *hovered;
                            cx.notify();
                        }))
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.diagnostics.toggle(Instant::now(), cx);
                            cx.notify();
                        })),
                )
                .child(self.diagnostics.measured_body(
                    "settings-diagnostics-body",
                    diagnostics,
                    now,
                )),
        )
    }
}
