//! Device-local routes. Editing one never changes the paired host identity.
use super::*;
use crate::ui::components::{integrations::integration_message, modal::Confirmation};
use canopy_desktop::remote::client::store::{ConnectionRoute, SshRoute};
use gpui_kit::component::{IconName, button::Button};
use std::collections::HashMap;

impl RemoteWindow {
    fn edit_route(
        &mut self,
        route: Option<ConnectionRoute>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.route_ssh = route.as_ref().is_some_and(|r| r.ssh().is_some());
        let ssh = route.as_ref().and_then(ConnectionRoute::ssh);
        for (field, value) in [
            (
                "route_name",
                route
                    .as_ref()
                    .map(|r| r.name().to_owned())
                    .unwrap_or_default(),
            ),
            (
                "endpoint",
                route
                    .as_ref()
                    .map(|r| r.endpoint().to_owned())
                    .unwrap_or_default(),
            ),
            (
                "ssh_address",
                ssh.map(|s| s.address().to_owned()).unwrap_or_default(),
            ),
            ("ssh_port", ssh.map(|s| s.port()).unwrap_or(22).to_string()),
            (
                "ssh_user",
                ssh.map(|s| s.username().to_owned()).unwrap_or_default(),
            ),
            ("ssh_secret", String::new()),
            ("ssh_passphrase", String::new()),
        ] {
            self.fields[field].update(cx, |f, cx| f.set_value(value, window, cx));
        }
        self.ssh_fingerprint = ssh.map(|s| s.fingerprint().to_owned());
        self.route_edit = route;
        self.route_form = true;
        cx.notify();
    }
    fn save_connection_route(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let name = self.value("route_name", cx).trim().to_owned();
        let endpoint = self.value("endpoint", cx);
        let previous = self.route_edit.clone();
        let ssh = self.route_ssh;
        let address = self.value("ssh_address", cx).trim().to_owned();
        let port = self.value("ssh_port", cx);
        let username = self.value("ssh_user", cx).trim().to_owned();
        let fingerprint = self.ssh_fingerprint.clone();
        let secret = self.value("ssh_secret", cx);
        let passphrase = nonempty(self.value("ssh_passphrase", cx));
        let vault = self.registry.as_ref().unwrap().vault();
        self.run(
            async move {
                let endpoint = endpoint_address(&endpoint)?;
                let mut staged = None;
                let route = if ssh {
                    let port = if port.is_empty() {
                        22
                    } else {
                        port.parse::<u16>()
                            .map_err(|_| "Enter an SSH port between 1 and 65535.")?
                    };
                    let fingerprint =
                        fingerprint.ok_or("Check and verify the SSH server key first.")?;
                    let old = previous.as_ref().and_then(ConnectionRoute::ssh);
                    if secret.is_empty() {
                        let old = old
                            .filter(|old| {
                                old.address() == address
                                    && old.port() == port
                                    && old.username() == username
                                    && old.fingerprint() == fingerprint
                            })
                            .ok_or("Enter SSH credentials for this server and login.")?;
                        Some(old.clone())
                    } else {
                        let reference = uuid::Uuid::new_v4().to_string();
                        canopy_desktop::remote::client::ssh::save_credentials(
                            vault.as_ref(),
                            &reference,
                            &secret,
                            passphrase.as_deref(),
                        )?;
                        staged = Some(reference.clone());
                        Some(SshRoute::new(
                            address,
                            port,
                            username,
                            fingerprint,
                            reference,
                        ))
                    }
                } else {
                    None
                };
                let result = previous
                    .as_ref()
                    .map_or_else(
                        || ConnectionRoute::new(name.clone(), endpoint.clone(), route.clone()),
                        |old| old.edited(name.clone(), endpoint.clone(), route.clone()),
                    )
                    .and_then(|route| host.save_route(route, previous.as_ref()));
                if let Err(error) = result {
                    if let Some(reference) = staged {
                        let _ = canopy_desktop::remote::client::ssh::remove_credentials(
                            vault.as_ref(),
                            &reference,
                        );
                    }
                    return Err(error);
                }
                Ok(Update::RouteSaved)
            },
            cx,
        );
    }
    fn remove_route(
        &mut self,
        host: Arc<HostClient>,
        id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = self.window.window_id();
        if cx
            .try_global::<RouteConfirmations>()
            .is_some_and(|confirmations| confirmations.0.contains_key(&key))
        {
            return;
        }
        let owner = cx.entity();
        let target = owner.downgrade();
        let focus = window.focused(cx).unwrap_or_else(|| cx.focus_handle());
        let modal = cx.new(|cx| {
            let mut modal = Confirmation::new(
                format!("Remove “{name}”?"),
                "Other routes and the pairing with this computer stay. Removing the route in use disconnects this window.",
                "Remove",
                focus,
                move |cx| {
                    let (host, id) = (host.clone(), id.clone());
                    target
                        .update(cx, |this, cx| {
                            if this.busy {
                                return Err(SharedString::from(
                                    "Another action is still running. Try again in a moment.",
                                ));
                            }
                            this.run(
                                async move {
                                    host.remove_route(&id)?;
                                    Ok(Update::Done("Route removed.".into()))
                                },
                                cx,
                            );
                            Ok(())
                        })
                        .map_err(|_| SharedString::from("This window was closed."))?
                },
                window,
                cx,
            )
            .destructive();
            modal.wait_for(&owner, |this| (this.busy, this.error.clone()), cx);
            modal
        });
        cx.subscribe(&modal, move |_, _, _: &ModalDismissed, cx| {
            cx.default_global::<RouteConfirmations>().0.remove(&key);
            cx.notify();
        })
        .detach();
        cx.default_global::<RouteConfirmations>()
            .0
            .insert(key, modal);
        cx.notify();
    }
    pub(super) fn render_routes(&mut self, cx: &mut Context<Self>) -> Div {
        let host = self.host.clone().unwrap();
        let routes = host.routes();
        let active = host.active_route();
        let mut list = column().flex_shrink_0();
        for (index, route) in routes.iter().enumerate() {
            let in_use = active.as_deref() == Some(route.id());
            list = list.child(self.route_row(&host, route, index, routes.len(), in_use, cx));
        }
        let confirmation = cx
            .try_global::<RouteConfirmations>()
            .and_then(|confirmations| confirmations.0.get(&self.window.window_id()))
            .cloned();
        let body = section(
            "ROUTES",
            "Canopy tries these in order and skips networks that aren’t available here. The pairing stays the same.",
            vec![list.into_any_element()],
        )
        .children(confirmation);
        if self.route_form {
            return body.child(self.route_form(cx));
        }
        body.child(
            row().child(
                button("new-connection-route", "Add route…")
                    .icon(icon(IconName::Plus))
                    .disabled(self.busy || routes.len() >= 8)
                    .on_click(cx.listener(|this, _, window, cx| this.edit_route(None, window, cx))),
            ),
        )
        .children(
            self.error
                .clone()
                .map(|error| integration_message(error, true)),
        )
    }
    fn route_row(
        &self,
        host: &Arc<HostClient>,
        route: &ConnectionRoute,
        index: usize,
        count: usize,
        in_use: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = route.id().to_owned();
        let name = route.name().to_owned();
        let edit = route.clone();
        let (connect, up, down, remove) = (host.clone(), host.clone(), host.clone(), host.clone());
        let (connect_id, up_id, down_id, remove_id) =
            (id.clone(), id.clone(), id.clone(), id.clone());
        let detail = match route.ssh() {
            Some(ssh) => format!("Through SSH · {}@{}", ssh.username(), ssh.address()),
            None => display_endpoint(route.endpoint()),
        };
        row()
            .flex_shrink_0()
            .gap(px(12.))
            .py(px(12.))
            .border_t(px(if index == 0 { 0. } else { 1. }))
            .border_color(t::border())
            .child(
                if route.ssh().is_some() {
                    custom_icon("terminal")
                } else {
                    icon(IconName::Globe)
                }
                .flex_shrink_0(),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.))
                                    .child(name.clone()),
                            )
                            .children(in_use.then(|| badge("In use").flex_shrink_0())),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .line_height(px(14.85))
                            .text_color(t::muted())
                            .child(detail),
                    )
                    .children(host.route_error(route.id()).map(|error| {
                        div()
                            .text_size(px(11.))
                            .line_height(px(14.85))
                            .text_color(t::red())
                            .child(error)
                    })),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .child(
                        button(SharedString::from(format!("use-route-{id}")), "Connect")
                            .disabled(self.busy || in_use)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let host = connect.clone();
                                let id = connect_id.clone();
                                let ready = prepare_connection(cx);
                                this.run(
                                    async move {
                                        ready.await?;
                                        host.connect_via(id).await?;
                                        Ok(Update::Connected)
                                    },
                                    cx,
                                );
                            })),
                    )
                    .child(
                        icon_button(
                            SharedString::from(format!("up-route-{id}")),
                            IconName::ChevronUp,
                            "Move route up",
                        )
                        .disabled(self.busy || index == 0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let host = up.clone();
                            let id = up_id.clone();
                            this.run(
                                async move {
                                    host.move_route(&id, index - 1)?;
                                    Ok(Update::Done("Route order saved.".into()))
                                },
                                cx,
                            );
                        })),
                    )
                    .child(
                        icon_button(
                            SharedString::from(format!("down-route-{id}")),
                            IconName::ChevronDown,
                            "Move route down",
                        )
                        .disabled(self.busy || index + 1 == count)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let host = down.clone();
                            let id = down_id.clone();
                            this.run(
                                async move {
                                    host.move_route(&id, index + 1)?;
                                    Ok(Update::Done("Route order saved.".into()))
                                },
                                cx,
                            );
                        })),
                    )
                    .child(
                        icon_button_base(
                            SharedString::from(format!("edit-route-{id}")),
                            "Edit route",
                        )
                        .icon(custom_icon("pencil"))
                        .disabled(self.busy)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.edit_route(Some(edit.clone()), window, cx)
                            },
                        )),
                    )
                    .child(
                        icon_button(
                            SharedString::from(format!("remove-route-{id}")),
                            IconName::Close,
                            "Remove route",
                        )
                        .disabled(self.busy || count == 1)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.remove_route(
                                    remove.clone(),
                                    remove_id.clone(),
                                    name.clone(),
                                    window,
                                    cx,
                                )
                            },
                        )),
                    ),
            )
    }
    fn route_form(&self, cx: &mut Context<Self>) -> Div {
        let heading = self
            .route_edit
            .as_ref()
            .map(|route| format!("Edit “{}”", route.name()))
            .unwrap_or_else(|| "Add route".into());
        let input_for = |field: &'static str| input(&self.fields[field]).disabled(self.busy);
        let mut form = column()
            .flex_shrink_0()
            .gap(px(16.))
            .p(px(16.))
            .rounded(px(6.))
            .border_1()
            .border_color(t::border())
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(heading),
            )
            .child(form_field(
                "Connection",
                if self.route_ssh {
                    "Reach Canopy through an SSH server you can sign in to."
                } else {
                    "Reach Canopy directly on your network or VPN."
                },
                segmented([
                    segment("route-direct", "Local network / VPN", !self.route_ssh)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.route_ssh = false;
                            cx.notify();
                        })),
                    segment("route-ssh", "SSH tunnel", self.route_ssh)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.route_ssh = true;
                            cx.notify();
                        })),
                ]),
            ))
            .child(field_grid([
                form_field(
                    "Route name",
                    "For example Home, Office or Tailscale",
                    input_for("route_name").id("route-name-input"),
                ),
                form_field(
                    "Canopy address",
                    if self.route_ssh {
                        "As reached from the SSH server, usually 127.0.0.1:44330"
                    } else {
                        "Computer IP or VPN name, for example 192.168.1.25:44330"
                    },
                    input_for("endpoint").id("route-endpoint-input"),
                ),
            ]));
        if self.route_ssh {
            form = form
                .child(field_grid([
                    form_field(
                        "SSH server",
                        "Computer name or IP address",
                        input_for("ssh_address"),
                    ),
                    form_field("SSH port", "Leave empty for port 22", input_for("ssh_port")),
                    form_field(
                        "SSH username",
                        "Your login on that server",
                        input_for("ssh_user"),
                    ),
                    form_field(
                        "Private-key passphrase",
                        "Only for an encrypted private key",
                        input_for("ssh_passphrase"),
                    ),
                ]))
                .child(form_field(
                    "SSH password or private key",
                    if self.route_edit.as_ref().is_some_and(|r| r.ssh().is_some()) {
                        "Leave empty to keep the saved credential for this server and login."
                    } else {
                        "Stored securely on this device."
                    },
                    input_for("ssh_secret"),
                ))
                .child(self.server_key(cx));
        }
        form.children(
            self.error
                .clone()
                .map(|error| integration_message(error, true)),
        )
        .child(
            row()
                .gap(px(8.))
                .justify_end()
                .child(
                    button("cancel-connection-route", "Cancel")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.route_form = false;
                            this.route_edit = None;
                            cx.notify();
                        })),
                )
                .child(
                    primary_button(
                        "save-connection-route",
                        if self.route_ssh {
                            "Trust key and save"
                        } else {
                            "Save route"
                        },
                    )
                    .disabled(
                        self.busy
                            || self.value("route_name", cx).trim().is_empty()
                            || self.value("endpoint", cx).trim().is_empty(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.save_connection_route(cx))),
                ),
        )
    }
    /// The SSH host key must be checked and compared before credentials are saved.
    fn server_key(&self, cx: &mut Context<Self>) -> Div {
        column()
            .flex_shrink_0()
            .gap(px(8.))
            .child(
                row().child(
                    button("check-route-ssh", "Check server key")
                        .disabled(self.busy || self.value("ssh_address", cx).trim().is_empty())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let address = this.value("ssh_address", cx);
                            let port = this.value("ssh_port", cx);
                            this.run(
                                async move {
                                    let port = if port.is_empty() {
                                        22
                                    } else {
                                        port.parse::<u16>().map_err(|_| "Invalid SSH port.")?
                                    };
                                    Ok(Update::SshFingerprint(
                                        tokio::task::spawn_blocking(move || {
                                            canopy_desktop::remote::client::ssh::probe(
                                                &address, port,
                                            )
                                        })
                                        .await
                                        .map_err(|_| "SSH check stopped.")??,
                                    ))
                                },
                                cx,
                            );
                        })),
                ),
            )
            .children(self.ssh_fingerprint.as_ref().map(|pin| {
                column()
                    .gap(px(4.))
                    .p(px(10.))
                    .rounded(px(4.))
                    .bg(t::hover())
                    .child(
                        div().text_size(px(11.)).text_color(t::muted()).child(
                            "Server key. Confirm it with the computer’s owner before saving.",
                        ),
                    )
                    .child(
                        div()
                            .font_family(t::MONO)
                            .text_size(px(12.))
                            .text_color(t::text())
                            .child(pin.clone()),
                    )
            }))
    }
}
/// Route removal confirmations, one per remote window. The window's fields belong
/// to its owner module, so the modal lives here and renders with the route list.
#[derive(Default)]
struct RouteConfirmations(HashMap<WindowId, Entity<Confirmation>>);
impl Global for RouteConfirmations {}

/// One choice group sharing a single silhouette, as in Preferences.
fn segmented(segments: impl IntoIterator<Item = Button>) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(6.))
        .bg(t::hover())
        .children(segments)
}
fn segment(id: &'static str, label: &'static str, selected: bool) -> Button {
    selection_button(id, label, selected)
        .border_0()
        .text_color(if selected { t::text() } else { t::secondary() })
}
/// Saved endpoints are URLs; people recognise the host and port.
fn display_endpoint(endpoint: &str) -> String {
    endpoint
        .strip_prefix("https://")
        .unwrap_or(endpoint)
        .trim_end_matches('/')
        .to_owned()
}
fn endpoint_address(value: &str) -> Result<String, String> {
    let value = value.trim();
    let value = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let mut url = url::Url::parse(&value).map_err(|_| "Enter a computer address and port.")?;
    if url.port().is_none() && !value.ends_with(":443") && !value.ends_with(":443/") {
        url.set_port(Some(44330))
            .map_err(|_| "Invalid Canopy port.")?;
    }
    Ok(url.to_string())
}
