//! Add computer (pairing) and the selected computer's connection page.
use super::*;
use canopy_desktop::remote::client::ssh;
use chrome::*;

const FORM_WIDTH: f32 = 560.;

impl RemoteWindow {
    pub(super) fn can_request(&self, cx: &App) -> bool {
        !self.busy
            && self.registry.is_some()
            && !self.value("pair", cx).trim().is_empty()
            && !self.value("name", cx).trim().is_empty()
            && (!self.pair_ssh
                || self.ssh_fingerprint.is_some()
                    && !self.value("ssh_user", cx).is_empty()
                    && !self.value("ssh_secret", cx).is_empty())
    }
    fn probe_ssh(&mut self, cx: &mut Context<Self>) {
        let address = self.value("ssh_address", cx);
        let port = self.value("ssh_port", cx).parse().unwrap_or(22);
        self.run_in(
            "ssh-probe",
            async move {
                Ok(Update::SshFingerprint(
                    tokio::task::spawn_blocking(move || ssh::probe(&address, port))
                        .await
                        .map_err(|_| "SSH check stopped.")??,
                ))
            },
            cx,
        );
        self.info = "Checking the SSH server…".into();
    }
    pub(super) fn render_hosts(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.opening_setup || self.host.is_none() {
            self.render_pairing(cx)
        } else {
            self.render_connection(cx)
        }
    }
    pub(super) fn render_pairing(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let now = Instant::now();
        let busy = self.busy;
        let ssh = self.pair_ssh;
        let mut form = column()
            .w_full()
            .max_w(px(FORM_WIDTH))
            .gap(px(24.))
            .child(
                column()
                    .gap(px(8.))
                    .child(caption("CONNECTION"))
                    .child(segmented([
                        segment("pair-direct-mode", "Local network / VPN", !ssh)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pair_ssh = false;
                                cx.notify();
                            })),
                        segment("pair-ssh-mode", "SSH tunnel", ssh)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pair_ssh = true;
                                cx.notify();
                            })),
                    ]))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t::secondary())
                            .child(if ssh {
                                "Canopy connects through SSH. SSH must already be enabled on the other computer."
                            } else {
                                "Use this when both computers share a network or VPN."
                            }),
                    ),
            )
            .child(hint(
                "On the other computer, open Connections → Add device and copy the invitation. Keep Canopy open on both computers.",
            ))
            .child(
                column()
                    .gap(px(16.))
                    .child(form_field(
                        "Invitation",
                        "Paste the invitation copied from the other computer.",
                        input(&self.fields["pair"]).w_full().disabled(busy),
                    ))
                    .child(form_field(
                        "This device’s name",
                        "Shown to the other computer’s owner when they approve access.",
                        input(&self.fields["name"]).w_full().disabled(busy),
                    )),
            );
        if ssh {
            let probing = self.pending == Some("ssh-probe");
            form = form.child(
                column()
                    .gap(px(16.))
                    .child(caption("SSH"))
                    .child(field_grid([
                        form_field(
                            "SSH server",
                            "",
                            input(&self.fields["ssh_address"]).w_full().disabled(busy),
                        ),
                        form_field(
                            "SSH username",
                            "Your login on that computer.",
                            input(&self.fields["ssh_user"]).w_full().disabled(busy),
                        ),
                    ]))
                    .child(form_field(
                        "Password or private key",
                        format!(
                            "Stored in {} on this device.",
                            canopy_desktop::platform::credentials::store_name()
                        ),
                        input(&self.fields["ssh_secret"]).w_full().disabled(busy),
                    ))
                    .child(
                        row().child(
                            button("probe-ssh", "Check SSH server")
                                .loading(probing)
                                .disabled(busy || self.value("ssh_address", cx).trim().is_empty())
                                .on_click(cx.listener(|this, _, _, cx| this.probe_ssh(cx))),
                        ),
                    )
                    .children(self.notice("ssh-probe"))
                    .children(self.ssh_fingerprint.clone().map(|pin| {
                        form_field(
                            "Server fingerprint",
                            "Confirm it with the other computer’s owner before you continue.",
                            mono_value(pin),
                        )
                    })),
            );
        }
        let mut advanced = column().gap(px(16.)).pb(px(8.)).child(form_field(
            "Canopy address",
            "Usually empty: the invitation already includes the address and port.",
            input(&self.fields["endpoint"]).w_full().disabled(busy),
        ));
        if ssh {
            advanced = advanced.child(field_grid([
                form_field(
                    "SSH port",
                    "Empty uses 22.",
                    input(&self.fields["ssh_port"]).w_full().disabled(busy),
                ),
                form_field(
                    "Key passphrase",
                    "Only for an encrypted private key.",
                    input(&self.fields["ssh_passphrase"])
                        .w_full()
                        .disabled(busy),
                ),
            ]));
        }
        form = form
            .child(
                column()
                    .child(
                        self.advanced
                            .header("show-connection-details", "ADVANCED", now, cx)
                            .on_hover(cx.listener(|this, hovered, _, cx| {
                                this.advanced.hovered = *hovered;
                                cx.notify();
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.advanced.toggle(Instant::now(), cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        self.advanced
                            .measured_body("connection-advanced", advanced, now),
                    ),
            )
            .children(self.notice("pair"))
            .child(
                row()
                    .justify_end()
                    .gap(px(8.))
                    .children((busy && self.inline.is_none()).then(|| {
                        button("cancel-pair", "Cancel request").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.cancelled.store(true, Ordering::Release);
                                cx.notify();
                            },
                        ))
                    }))
                    // Inline in Settings: one Cancel closes the form and any waiting request.
                    .children(self.inline.is_some().then(|| {
                        button("close-add-computer", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.close_inline(cx)))
                    }))
                    .child(
                        primary_button(
                            "pair-host",
                            if ssh {
                                "Trust key and request access"
                            } else {
                                "Request access"
                            },
                        )
                        .loading(self.pending == Some("pair"))
                        .disabled(!self.can_request(cx))
                        .on_click(cx.listener(|this, _, _, cx| this.pair_host(this.pair_ssh, cx))),
                    ),
            );
        form.into_any_element()
    }
    fn render_connection(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let now = Instant::now();
        let host = self.host.clone().unwrap();
        let phase = host.phase();
        let routes = host.routes();
        let via = host
            .active_route()
            .and_then(|id| routes.iter().find(|r| r.id() == id))
            .map(|route| format!("Using {}", route.name()))
            .unwrap_or_else(|| {
                format!(
                    "{} route{}",
                    routes.len(),
                    if routes.len() == 1 { "" } else { "s" }
                )
            });
        let status = column().gap(px(8.)).child(caption("STATUS")).child(
            row()
                .gap(px(8.))
                .child(status_dot(phase))
                .child(div().text_size(px(13.)).child(phase_label(phase)))
                .child(div().text_size(px(12.)).text_color(t::muted()).child(via)),
        );
        let label = self.host_label_form(cx);
        column()
            .w_full()
            .max_w(px(640.))
            .gap(px(24.))
            .child(status)
            .child(self.render_routes(cx))
            .child(
                column()
                    .child(
                        self.details
                            .header("show-route-details", "THIS DEVICE", now, cx)
                            .on_hover(cx.listener(|this, hovered, _, cx| {
                                this.details.hovered = *hovered;
                                cx.notify();
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.details.toggle(Instant::now(), cx);
                                cx.notify();
                            })),
                    )
                    .child(self.details.measured_body("connection-device", label, now)),
            )
            .into_any_element()
    }
    /// Device-local display name of the selected computer.
    pub(super) fn host_label_form(&self, cx: &mut Context<Self>) -> Div {
        column()
            .gap(px(12.))
            .pb(px(8.))
            .child(form_field(
                "Name on this device",
                "Only this device sees this name.",
                input(&self.fields["host_label"])
                    .w_full()
                    .disabled(self.busy),
            ))
            .child(
                row().child(
                    button("rename-host-label", "Save name")
                        .loading(self.pending == Some("host-label"))
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let Some(host) = this.host.clone() else {
                                return;
                            };
                            let name = this.value("host_label", cx);
                            this.run_in(
                                "host-label",
                                async move {
                                    host.set_display_name(name)?;
                                    Ok(Update::Done("Name saved on this device.".into()))
                                },
                                cx,
                            );
                        })),
                ),
            )
            .children(self.notice("host-label"))
    }
}
