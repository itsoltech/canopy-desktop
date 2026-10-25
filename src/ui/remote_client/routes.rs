//! Device-local routes. Editing one never changes the paired host identity.
use super::*;
use canopy_desktop::remote::client::store::{ConnectionRoute, SshRoute};
use gpui_kit::component::IconName;

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
    pub(super) fn render_routes(&mut self, cx: &mut Context<Self>) -> Div {
        let host = self.host.clone().unwrap();
        let routes = host.routes();
        let active = host.active_route();
        let mut body = column().gap_3()
            .child(div().font_weight(FontWeight::MEDIUM).child("Connection routes"))
            .child(div().text_color(t::muted()).child("One computer, one pairing. Canopy tries routes in this order and skips networks that are unavailable here."));
        for (index, route) in routes.iter().enumerate() {
            let id = route.id().to_owned();
            let edit = route.clone();
            let is_active = active.as_deref() == Some(id.as_str());
            let connect = host.clone();
            let connect_id = id.clone();
            let up = host.clone();
            let up_id = id.clone();
            let down = host.clone();
            let down_id = id.clone();
            let remove = host.clone();
            let remove_id = id.clone();
            let name = route.name().to_owned();
            body = body.child(column().gap_1().py_2().border_b_1().border_color(t::border())
                .child(row().gap_2().child(column().flex_1().gap_1()
                    .child(format!("{}{}", route.name(), if is_active { " · In use" } else { "" }))
                    .child(div().text_color(t::muted()).child(format!("{}{}", if route.ssh().is_some() { "SSH → " } else { "" }, route.endpoint()))))
                    .child(button(SharedString::from(format!("use-route-{id}")), "Connect").disabled(self.busy || is_active).on_click(cx.listener(move |this,_,_,cx|{
                        let host=connect.clone();let id=connect_id.clone();let ready=prepare_connection(cx);this.run(async move{ready.await?;host.connect_via(id).await?;Ok(Update::Connected)},cx);
                    })))
                    .child(button(SharedString::from(format!("edit-route-{id}")), "Edit…").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.edit_route(Some(edit.clone()),window,cx))))
                    .child(icon_button(SharedString::from(format!("up-route-{id}")), IconName::ChevronUp, "Move route up").disabled(self.busy || index==0).on_click(cx.listener(move|this,_,_,cx|{
                        let host=up.clone();let id=up_id.clone();this.run(async move{host.move_route(&id,index-1)?;Ok(Update::Done("Route order saved.".into()))},cx);
                    })))
                    .child(icon_button(SharedString::from(format!("down-route-{id}")), IconName::ChevronDown, "Move route down").disabled(self.busy || index+1==routes.len()).on_click(cx.listener(move|this,_,_,cx|{
                        let host=down.clone();let id=down_id.clone();this.run(async move{host.move_route(&id,index+1)?;Ok(Update::Done("Route order saved.".into()))},cx);
                    })))
                    .child(icon_button(SharedString::from(format!("remove-route-{id}")), IconName::Close, "Remove route").disabled(self.busy || routes.len()==1).on_click(cx.listener(move|_this,_,window,cx|{
                        let answer=window.prompt(PromptLevel::Warning,&format!("Remove route “{name}”?"),Some("Other routes, pairing and drafts are kept. Removing the route in use disconnects this window."),&[PromptButton::cancel("Cancel"),PromptButton::new("Remove route")],cx);
                        let host=remove.clone();let id=remove_id.clone();cx.spawn_in(window,async move|this,cx|{if answer.await==Ok(1){let _=this.update(cx,|this,cx|this.run(async move{host.remove_route(&id)?;Ok(Update::Done("Route removed.".into()))},cx));}}).detach();
                    }))))
                .children(host.route_error(route.id()).map(|error|div().text_color(t::muted()).child(error))));
        }
        if !self.route_form {
            return body.child(
                button("new-connection-route", "Add route…")
                    .disabled(self.busy || routes.len() >= 8)
                    .on_click(cx.listener(|this, _, window, cx| this.edit_route(None, window, cx))),
            );
        }
        body = body
            .child(form_field(
                "Route name",
                "For example Home, Office or Tailscale",
                input(&self.fields["route_name"])
                    .id("route-name-input")
                    .disabled(self.busy),
            ))
            .child(
                row()
                    .gap_2()
                    .child(
                        selection_button("route-direct", "Local network / VPN", !self.route_ssh)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.route_ssh = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        selection_button("route-ssh", "SSH tunnel", self.route_ssh)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.route_ssh = true;
                                cx.notify();
                            })),
                    ),
            )
            .child(form_field(
                "Canopy address",
                if self.route_ssh {
                    "Address as reached from the SSH server, usually 127.0.0.1:44330"
                } else {
                    "Computer IP or VPN name, for example 192.168.1.25:44330"
                },
                input(&self.fields["endpoint"])
                    .id("route-endpoint-input")
                    .disabled(self.busy),
            ));
        if self.route_ssh {
            body = body
                .child(self.field("ssh_address", "SSH server"))
                .child(self.field("ssh_port", "SSH port (default 22)"))
                .child(self.field("ssh_user", "SSH username"))
                .child(form_field(
                    "SSH password or private key",
                    if self.route_edit.as_ref().is_some_and(|r| r.ssh().is_some()) {
                        "Leave blank to keep the saved credential for this server and login."
                    } else {
                        "Stored securely on this device."
                    },
                    input(&self.fields["ssh_secret"]).disabled(self.busy),
                ))
                .child(self.field("ssh_passphrase", "Private-key passphrase (optional)"))
                .child(
                    button("check-route-ssh", "Check SSH server")
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
                )
                .children(self.ssh_fingerprint.as_ref().map(|pin| {
                    div()
                        .text_color(t::muted())
                        .child(format!("Verify with the computer’s owner: {pin}"))
                }));
        }
        body.child(
            row()
                .gap_2()
                .child(
                    button(
                        "save-connection-route",
                        if self.route_ssh {
                            "Trust SSH key and save route"
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
                )
                .child(
                    button("cancel-connection-route", "Cancel")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.route_form = false;
                            this.route_edit = None;
                            cx.notify();
                        })),
                ),
        )
    }
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
