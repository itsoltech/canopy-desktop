//! Connection setup. RemoteState owns the listener and enrollment; this retained
//! view owns disclosure, address choice and permission edits, never socket I/O.
use crate::{
    app_state::{AppState, project_commands, remote::RemoteState},
    ui::{
        components::*,
        remote_client::{self, Connections},
        theme as t,
    },
};
use canopy_desktop::remote::{
    enrollment::EnrollmentRpc,
    network::{LocalAddress, local_addresses},
};
use gpui_kit::{
    base::Disableable,
    component::{
        input::{InputState, TextareaState},
        scroll::ScrollableElement,
        switch::Switch,
    },
    *,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

const SCOPES: [(u32, &str); 7] = [
    (1, "View projects and history"),
    (2, "Send messages"),
    (4, "Approve tools"),
    (8, "Change files and Git"),
    (16, "Use terminals"),
    (32, "Change Tasks"),
    (64, "Change host settings and devices"),
];
pub(super) struct RemotePreferences {
    state: Entity<RemoteState>,
    connections: Option<Entity<Connections>>,
    address: Entity<InputState>,
    advertised: Entity<InputState>,
    roots: Entity<TextareaState>,
    scopes: u32,
    editing: Option<(String, u64)>,
    setup: bool,
    advanced: bool,
    grant_advanced: bool,
    address_picker: bool,
    addresses: Vec<LocalAddress>,
    addresses_loading: bool,
    projects: Vec<project_commands::WorkspaceSummary>,
    qr: Option<Arc<qrcodegen::QrCode>>,
    invite_uri: Option<String>,
    error: Option<String>,
    address_error: Option<String>,
    scroll: ScrollHandle,
    qr_task: Option<Task<()>>,
    load_task: Option<Task<()>>,
    _observers: Vec<Subscription>,
    seen_address: String,
    seen_pending: Option<String>,
}
impl RemotePreferences {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().remote.clone();
        let address = state.read(cx).preferences().address().to_string();
        let connections = remote_client::connections(cx);
        let connection_error = connections.as_ref().err().map(ToString::to_string);
        let connections = connections.ok();
        let mut observers = vec![cx.observe_in(&state, window, |this, state, window, cx| {
            let (address, pending, uri, editing_saved) = {
                let state = state.read(cx);
                (
                    state.preferences().address().to_string(),
                    state.pending().map(|pending| pending.id().to_owned()),
                    state.invite.as_ref().map(EnrollmentRpc::invite_uri),
                    !state.busy()
                        && state.error.is_none()
                        && this.editing.as_ref().is_some_and(|(id, revision)| {
                            state
                                .devices
                                .iter()
                                .any(|d| d.id() == id && d.authorization_revision() != *revision)
                        }),
                )
            };
            if address != this.seen_address {
                if this.address.read(cx).value().as_str() == this.seen_address {
                    this.address
                        .update(cx, |field, cx| field.set_value(address.clone(), window, cx));
                }
                this.seen_address = address;
            }
            if pending != this.seen_pending {
                if pending.is_some() {
                    this.editing = None;
                    this.scopes = 7;
                    this.grant_advanced = false;
                    this.roots
                        .update(cx, |field, cx| field.set_value("private", window, cx));
                    this.refresh_addresses(cx);
                } else if this.seen_pending.is_some() && uri.is_none() {
                    this.setup = false;
                }
                this.seen_pending = pending;
            }
            if editing_saved {
                this.editing = None;
            }
            if uri != this.invite_uri {
                this.invite_uri = uri.clone();
                this.qr = None;
                this.qr_task = uri.map(|uri| {
                    cx.spawn(async move |this, cx| {
                        let qr = cx
                            .background_spawn(async move {
                                qrcodegen::QrCode::encode_text(&uri, qrcodegen::QrCodeEcc::Medium)
                                    .map(Arc::new)
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            match qr {
                                Ok(qr) => this.qr = Some(qr),
                                Err(_) => this.error = Some(
                                    "Could not create the QR code. Copy the invitation instead."
                                        .into(),
                                ),
                            }
                            cx.notify();
                        });
                    })
                });
            }
            cx.notify();
        })];
        if let Some(connections) = &connections {
            observers.push(cx.observe(connections, |_, _, cx| cx.notify()));
        }
        let mut view = Self {
            state,
            connections,
            address: cx.new(|cx| InputState::new(window, cx).default_value(address.clone())),
            advertised: cx.new(|cx| InputState::new(window, cx).placeholder("Automatic")),
            roots: cx.new(|cx| TextareaState::new(window, cx).default_value("private")),
            scopes: 7,
            editing: None,
            setup: false,
            advanced: false,
            grant_advanced: false,
            address_picker: false,
            addresses: vec![],
            addresses_loading: false,
            projects: vec![],
            qr: None,
            invite_uri: None,
            error: connection_error,
            address_error: None,
            scroll: ScrollHandle::new(),
            qr_task: None,
            load_task: None,
            _observers: observers,
            seen_address: address,
            seen_pending: None,
        };
        view.refresh_addresses(cx);
        view.state.update(cx, |state, cx| state.refresh_devices(cx));
        view
    }
    fn refresh_addresses(&mut self, cx: &mut Context<Self>) {
        if self.addresses_loading {
            return;
        }
        self.addresses_loading = true;
        self.address_error = None;
        let projects = project_commands::listed(cx);
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let (addresses, projects) = cx
                .background_spawn(async move {
                    (
                        local_addresses(),
                        project_commands::resolve_listed(projects),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.addresses_loading = false;
                this.projects = projects.into_iter().filter(|p| !p.closing).collect();
                match addresses {
                    Ok(addresses) => this.addresses = addresses,
                    Err(error) => this.address_error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn choose_method(&mut self, network: bool, window: &mut Window, cx: &mut Context<Self>) {
        let port = self.state.read(cx).preferences().address().port();
        let address = SocketAddr::new(
            if network {
                IpAddr::from([0, 0, 0, 0])
            } else {
                IpAddr::from([127, 0, 0, 1])
            },
            port,
        )
        .to_string();
        self.address
            .update(cx, |field, cx| field.set_value(address.clone(), window, cx));
        self.advertised
            .update(cx, |field, cx| field.set_value("", window, cx));
        self.setup = false;
        self.error = None;
        self.state.update(cx, |state, cx| {
            state.configure(state.preferences().enabled(), &address, cx)
        });
    }
    fn invitation_address(&self, cx: &App) -> Option<String> {
        let listener = self.state.read(cx).address()?;
        let selected = self.advertised.read(cx).value().trim().to_owned();
        if !selected.is_empty() {
            return Some(selected);
        }
        if !listener.ip().is_unspecified() {
            return Some(listener.ip().to_string());
        }
        self.addresses
            .first()
            .map(|address| address.ip().to_string())
    }
    fn create_invitation(&mut self, cx: &mut Context<Self>) {
        let Some(address) = self.invitation_address(cx) else {
            self.error=Some("No network address was found. Connect to Wi-Fi or VPN and refresh, or enter an address in Advanced settings.".into());
            cx.notify();
            return;
        };
        self.error = None;
        self.setup = true;
        self.state.update(cx, |state, cx| state.pair(address, cx));
    }
    fn test_here(&mut self, cx: &mut Context<Self>) {
        let id = cx
            .global::<AppState>()
            .settings
            .read(cx)
            .host_id()
            .map(str::to_owned);
        if let Some(host) = self.connections.as_ref().and_then(|c| {
            c.read(cx)
                .hosts()
                .into_iter()
                .find(|h| Some(h.id().as_str()) == id.as_deref())
        }) {
            if let Err(error) = remote_client::open_host(host, cx) {
                self.error = Some(error.to_string());
                cx.notify();
            }
            return;
        }
        let Some(listener) = self.state.read(cx).address() else {
            return;
        };
        let address = if listener.ip().is_unspecified() {
            if listener.is_ipv4() {
                "127.0.0.1"
            } else {
                "[::1]"
            }
            .to_owned()
        } else {
            listener.ip().to_string()
        };
        self.state.update(cx, |state, cx| state.pair(address, cx));
        let uri = self
            .state
            .read(cx)
            .invite
            .as_ref()
            .map(EnrollmentRpc::invite_uri);
        if let Some(uri) = uri
            && let Err(error) = remote_client::open_invitation(uri, cx)
        {
            self.error = Some(error.to_string());
            cx.notify();
        }
    }
    fn roots(&self, cx: &App) -> Result<Vec<String>, String> {
        let roots = self
            .roots
            .read(cx)
            .value()
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return Err("Select private chats or at least one project.".into());
        }
        if roots
            .iter()
            .any(|r| r != "private" && r != "*" && !std::path::Path::new(r).is_absolute())
        {
            return Err(
                "Choose projects from the list, or use full folder paths in Advanced permissions."
                    .into(),
            );
        }
        Ok(roots)
    }
    fn toggle_root(
        &mut self,
        root: String,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut roots = self.roots(cx).unwrap_or_default();
        roots.retain(|r| r != &root);
        if checked {
            roots.push(root);
        }
        self.roots.update(cx, |field, cx| {
            field.set_value(roots.join("\n"), window, cx)
        });
        cx.notify();
    }
    fn save_grant(&mut self, pending: Option<String>, cx: &mut Context<Self>) {
        let roots = match self.roots(cx) {
            Ok(roots) => roots,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        if self.scopes & 1 == 0 {
            self.error = Some("View access is needed to connect and load this computer. Select it before approving other permissions.".into());
            cx.notify();
            return;
        }
        let scopes = self.scopes;
        if let Some(id) = pending {
            self.state.update(cx, |state, cx| {
                state.resolve_pairing(id, Some((scopes, roots)), cx)
            });
        } else if let Some((id, revision)) = self.editing.clone() {
            self.state.update(cx, |state, cx| {
                state.update_device(id, Some((revision, scopes, roots)), cx)
            });
        }
        self.error = None;
        cx.notify();
    }
    fn revoke(&mut self, id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let answer=window.prompt(PromptLevel::Warning,&format!("Remove access for “{name}”?"),Some("This device will need a new invitation to connect again. Work already accepted by this computer continues."),&[PromptButton::cancel("Cancel"),PromptButton::new("Remove access")],cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |this, cx| {
                    this.state
                        .update(cx, |state, cx| state.update_device(id, None, cx))
                });
            }
        })
        .detach();
    }
    fn sharing(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let enabled = state.preferences().enabled();
        let running = state.address();
        let network = !state.preferences().address().ip().is_loopback();
        let status = if busy {
            "Updating access…"
        } else if running.is_some() {
            if network {
                "Available on your network"
            } else {
                "Available on this computer and through SSH"
            }
        } else if enabled {
            "Could not start remote access"
        } else {
            "Remote access is off"
        };
        let mut body = column()
            .gap_3()
            .child(setting(
                "Remote access",
                "Allow approved devices to use this computer while Canopy is running.",
                true,
                Switch::new("remote-enabled")
                    .checked(enabled)
                    .disabled(busy)
                    .on_change(cx.listener(|this, enabled, _, cx| {
                        let address = this.state.read(cx).preferences().address().to_string();
                        this.state
                            .update(cx, |state, cx| state.configure(*enabled, &address, cx));
                    })),
            ))
            .child(
                row()
                    .gap_2()
                    .child(div().text_color(t::muted()).child(status))
                    .children((enabled && running.is_none()).then(|| {
                        button("retry-listener", "Retry")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |state, cx| state.retry(cx))
                            }))
                    })),
            );
        if enabled {
            body=body.child(row().gap_2()
                .child(selection_button("network-access","Local network / VPN",network).disabled(busy).on_click(cx.listener(|this,_,window,cx|this.choose_method(true,window,cx))))
                .child(selection_button("ssh-access","SSH only",!network).disabled(busy).on_click(cx.listener(|this,_,window,cx|this.choose_method(false,window,cx)))))
                .child(div().text_color(t::muted()).child(if network{"Use the same Wi-Fi, LAN or VPN on both devices. Every new device still needs your approval."}else{"Other computers need an SSH tunnel to this computer. You can also test the connection here."}));
        }
        body
    }
    fn pairing(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        let running = self.state.read(cx).address();
        let network = running.is_some_and(|a| !a.ip().is_loopback());
        let mut body = column()
            .gap_3()
            .child(
                row()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Authorized devices"),
                    )
                    .child(
                        button("new-device", "Add device…")
                            .disabled(busy || running.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.setup = true;
                                this.create_invitation(cx);
                            })),
                    ),
            )
            .child(div().text_color(t::muted()).child(if running.is_none() {
                "Enable remote access to add a phone or another computer."
            } else {
                "Devices you approve can reconnect without another invitation."
            }));
        if self.setup && running.is_some() {
            body=body.child(column().gap_3()
                .child(div().font_weight(FontWeight::MEDIUM).child("1. Open Canopy on the other device"))
                .child(div().text_color(t::muted()).child(if network{"Scan this QR code on your phone, or copy the invitation into Connections → Add computer on another desktop."}else{"Choose SSH tunnel in Connections → Add computer on the other desktop. The tunnel connects to Canopy on this computer."}))
                .children(self.qr.clone().map(qr_view))
                .children(self.invite_uri.clone().map(|uri|button("copy-invitation","Copy invitation").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(uri.clone())))))
                .child(div().text_color(t::muted()).child("2. Return here to approve the device. Invitations expire after five minutes."))
                .children(self.invite_uri.is_none().then(||div().text_color(t::muted()).child("The invitation expired or is unavailable. Create a new one to continue.")))
                .child(row().gap_2().child(button("renew-invitation","New invitation").disabled(busy).on_click(cx.listener(|this,_,_,cx|this.create_invitation(cx))))
                    .child(button("hide-invitation","Done").on_click(cx.listener(|this,_,_,cx|{this.setup=false;cx.notify();})))));
            if network {
                body = body
                    .child(div().text_color(t::muted()).child(format!(
                            "Computer address: {}",
                            self.state
                                .read(cx)
                                .invite
                                .as_ref()
                                .map(|invite| invite.address.clone())
                                .or_else(|| self.invitation_address(cx))
                                .unwrap_or_else(|| "No address found".into())
                        )))
                    .child(
                        button(
                            "choose-address",
                            if self.address_picker {
                                "Hide addresses"
                            } else {
                                "Choose another address…"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.address_picker = !this.address_picker;
                            cx.notify();
                        })),
                    );
                if self.address_picker {
                    for address in &self.addresses {
                        let ip = address.ip().to_string();
                        let selected = self.invitation_address(cx).as_deref() == Some(&ip);
                        body = body.child(
                            selection_button(
                                SharedString::from(format!("network-{ip}")),
                                format!("{} · {ip}", address.interface()),
                                selected,
                            )
                            .disabled(busy)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.advertised.update(cx, |field, cx| {
                                        field.set_value(ip.clone(), window, cx)
                                    });
                                    this.create_invitation(cx);
                                },
                            )),
                        );
                    }
                    body = body.child(
                        button("refresh-addresses", "Refresh addresses")
                            .disabled(self.addresses_loading)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_addresses(cx))),
                    );
                }
            }
        }
        body
    }
    fn grant(&self, busy: bool, cx: &mut Context<Self>) -> Option<Div> {
        let state = self.state.read(cx);
        let pending = state.pending();
        if pending.is_none() && self.editing.is_none() {
            return None;
        }
        let pending_id = pending.as_ref().map(|p| p.id().to_owned());
        let title = pending
            .as_ref()
            .map(|p| format!("{} wants to connect", p.name()))
            .unwrap_or_else(|| "Device permissions".into());
        let mut body = column()
            .gap_3()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                row().gap_2().children(
                    [(1, "View only"), (7, "Chat"), (63, "Control")]
                        .into_iter()
                        .map(|(scopes, label)| {
                            selection_button(
                                ("permission-preset", scopes as usize),
                                label,
                                self.scopes == scopes,
                            )
                            .disabled(busy)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.scopes = scopes;
                                    cx.notify();
                                },
                            ))
                        }),
                ),
            )
            .child(div().text_color(t::muted()).child(match self.scopes {
                1 => "Read history, files and Tasks. No changes or messages.",
                7 => "Read, send messages and approve agent tools.",
                63 => "Use chats, files, Git, terminals and Tasks. Host settings stay protected.",
                _ => "Custom permissions",
            }))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Allow access to"),
            );
        let roots = self.roots(cx).unwrap_or_default();
        let all = roots.iter().any(|r| r == "*");
        body = body.child(
            row()
                .gap_2()
                .child(
                    checkbox(
                        "private-chat-access",
                        all || roots.iter().any(|r| r == "private"),
                        "Private chats",
                    )
                    .disabled(busy || all)
                    .on_change(cx.listener(|this, checked, window, cx| {
                        this.toggle_root("private".into(), *checked, window, cx)
                    })),
                )
                .child("Private chats and global resources"),
        );
        for project in &self.projects {
            let Some(root) = project.canonical_root.to_str() else {
                continue;
            };
            let root = root.to_owned();
            body = body.child(
                row()
                    .gap_2()
                    .child(
                        checkbox(
                            SharedString::from(format!("project-grant-{root}")),
                            all || roots.contains(&root),
                            project.name.clone(),
                        )
                        .disabled(busy || all)
                        .on_change(cx.listener(
                            move |this, checked, window, cx| {
                                this.toggle_root(root.clone(), *checked, window, cx)
                            },
                        )),
                    )
                    .child(
                        column().flex_1().gap_1().child(project.name.clone()).child(
                            div()
                                .text_color(t::muted())
                                .text_ellipsis()
                                .child(project.canonical_root.display().to_string()),
                        ),
                    ),
            );
        }
        if self.projects.is_empty() {
            body = body.child(
                div()
                    .text_color(t::muted())
                    .child("Open a project on this computer to share it here."),
            );
        }
        body = body.child(
            button(
                "advanced-permissions",
                if self.grant_advanced {
                    "Hide advanced permissions"
                } else {
                    "Advanced permissions…"
                },
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.grant_advanced = !this.grant_advanced;
                cx.notify();
            })),
        );
        if self.grant_advanced {
            body=body.children(SCOPES.into_iter().map(|(bit,label)|row().gap_2().child(checkbox(("scope",bit as usize),self.scopes&bit!=0,label).disabled(busy).on_change(cx.listener(move|this,checked,_,cx|{if *checked{this.scopes|=bit;}else{this.scopes&=!bit;}cx.notify();}))).child(label)))
                .child(form_field("Allowed folders", "One full folder path per line. private allows private chats; * allows every current and future project.", textarea(&self.roots).h(px(100.)).disabled(busy)));
        }
        body = body.child(
            row()
                .gap_2()
                .child(
                    button(
                        "save-permissions",
                        if pending.is_some() {
                            "Approve device"
                        } else {
                            "Save permissions"
                        },
                    )
                    .disabled(busy)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.save_grant(pending_id.clone(), cx)),
                    ),
                )
                .child(
                    button(
                        "cancel-permissions",
                        if pending.is_some() { "Deny" } else { "Cancel" },
                    )
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let pending = this.state.read(cx).pending().map(|p| p.id().to_owned());
                        if let Some(id) = pending {
                            this.state
                                .update(cx, |state, cx| state.resolve_pairing(id, None, cx));
                        } else {
                            this.editing = None;
                            cx.notify();
                        }
                    })),
                ),
        );
        Some(body)
    }
    fn devices(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let active = state
            .devices
            .iter()
            .filter(|d| !d.revoked())
            .collect::<Vec<_>>();
        let mut body = column().gap_2();
        if active.is_empty() {
            body = body.child(
                div()
                    .text_color(t::muted())
                    .child("No devices have access yet."),
            );
        }
        for device in active {
            let edit = device.clone();
            let id = device.id().to_owned();
            let name = device.name().to_owned();
            body = body.child(
                row()
                    .gap_2()
                    .py_2()
                    .border_b_1()
                    .border_color(t::border())
                    .child(
                        column()
                            .flex_1()
                            .gap_1()
                            .child(device.name().to_owned())
                            .child(div().text_color(t::muted()).child(
                                if state.device_online(device.id()) {
                                    "Connected"
                                } else {
                                    "Paired · offline"
                                },
                            )),
                    )
                    .child(
                        button(SharedString::from(format!("edit-{id}")), "Permissions…")
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.editing =
                                    Some((edit.id().into(), edit.authorization_revision()));
                                this.scopes = edit.scopes();
                                this.roots.update(cx, |field, cx| {
                                    field.set_value(edit.roots().join("\n"), window, cx)
                                });
                                this.refresh_addresses(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        button(SharedString::from(format!("remove-{id}")), "Remove access…")
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.revoke(id.clone(), name.clone(), window, cx)
                            })),
                    ),
            );
        }
        body
    }
    fn computers(&self, cx: &mut Context<Self>) -> Div {
        let body = row()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Other computers"),
            )
            .child(
                button("add-computer", "Add computer…").on_click(cx.listener(|this, _, _, cx| {
                    if let Err(error) = remote_client::add_computer(cx) {
                        this.error = Some(error.to_string());
                        cx.notify();
                    }
                })),
            )
            .into_any_element();
        let mut section = column().gap_3().child(body);
        if let Some(connections) = &self.connections {
            let connections = connections.read(cx);
            let hosts = connections.hosts();
            if connections.loading() {
                section = section.child(
                    div()
                        .text_color(t::muted())
                        .child("Loading saved connections…"),
                );
            }
            if let Some(error) = connections.error() {
                section = section.child(div().text_color(t::red()).child(error.to_owned()));
            }
            if hosts.is_empty() && !connections.loading() {
                section=section.child(div().text_color(t::muted()).child("Connect to a computer running Canopy. Its projects and agents appear in a separate remote window."));
            }
            for host in hosts {
                let status = match host.phase() {
                    canopy_desktop::remote::client::Phase::Ready => "Connected",
                    canopy_desktop::remote::client::Phase::Connecting => "Connecting…",
                    canopy_desktop::remote::client::Phase::Offline => "Offline",
                };
                let id = host.id().as_str().to_owned();
                let selected = host.clone();
                let routes = host.routes();
                let active = host.active_route();
                let via = routes
                    .iter()
                    .find(|route| Some(route.id()) == active.as_deref());
                let status = format!(
                    "{status}{} · {} routes",
                    via.map(|r| format!(" via {}", r.name()))
                        .unwrap_or_default(),
                    routes.len()
                );
                let route_host = host.clone();
                section =
                    section.child(
                        row()
                            .gap_2()
                            .py_2()
                            .border_b_1()
                            .border_color(t::border())
                            .child(
                                column()
                                    .flex_1()
                                    .gap_1()
                                    .child(host.name())
                                    .child(div().text_color(t::muted()).child(status))
                                    .children(
                                        host.connection_error()
                                            .map(|error| div().text_color(t::red()).child(error)),
                                    ),
                            )
                            .child(
                                button(SharedString::from(format!("open-{id}")), "Open…").on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        if let Err(error) =
                                            remote_client::open_host(selected.clone(), cx)
                                        {
                                            this.error = Some(error.to_string());
                                            cx.notify();
                                        }
                                    }),
                                ),
                            )
                            .child(
                                button(SharedString::from(format!("routes-{id}")), "Routes…")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Err(error) =
                                            remote_client::open_routes(route_host.clone(), cx)
                                        {
                                            this.error = Some(error.to_string());
                                            cx.notify();
                                        }
                                    })),
                            )
                            .children(
                                (host.phase() == canopy_desktop::remote::client::Phase::Ready)
                                    .then(|| {
                                        button(
                                            SharedString::from(format!("disconnect-{id}")),
                                            "Disconnect",
                                        )
                                        .on_click(move |_, _, _| host.disconnect())
                                    }),
                            ),
                    );
            }
        }
        section
    }
}
impl Render for RemotePreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let busy = state.busy() || state.quitting();
        let running = state.address().is_some();
        let error = self.error.clone().or_else(|| state.error.clone());
        let mut content = column()
            .id("connections-preferences")
            .track_scroll(&self.scroll)
            .overflow_y_scrollbar()
            .size_full()
            .p_6()
            .gap_6()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Connections"))
            .children(error.map(|error| div().text_color(t::red()).child(error)))
            .child(section(
                "THIS COMPUTER",
                "Run agents here and securely connect from your other devices.",
                vec![self.sharing(busy, cx).into_any_element()],
            ))
            .child(self.pairing(busy, cx))
            .children(self.grant(busy, cx))
            .child(self.devices(busy, cx))
            .child(self.computers(cx))
            .child(
                row()
                    .gap_2()
                    .child(
                        button("test-here", "Test on this computer…")
                            .disabled(
                                busy || !running
                                    || self
                                        .connections
                                        .as_ref()
                                        .is_none_or(|c| c.read(cx).loading()),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.test_here(cx))),
                    )
                    .child(
                        button(
                            "advanced-connections",
                            if self.advanced {
                                "Hide advanced settings"
                            } else {
                                "Advanced settings…"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.advanced = !this.advanced;
                            cx.notify();
                        })),
                    ),
            );
        if self.advanced {
            content=content.child(column().gap_3()
                .child(form_field("Listen address", "Changes where this computer accepts connections. Default port: 44330.", input(&self.address).disabled(busy)))
                .child(button("apply-listener","Apply address").disabled(busy).on_click(cx.listener(|this,_,_,cx|{let address=this.address.read(cx).value().to_string();let enabled=this.state.read(cx).preferences().enabled();this.state.update(cx,|state,cx|state.configure(enabled,&address,cx));})))
                .child(form_field("Address in invitation", "Usually detected automatically. Enter an IP address or VPN name without https:// or a port.", input(&self.advertised).disabled(busy)))
                .child(button("refresh-network","Refresh network addresses").disabled(self.addresses_loading).on_click(cx.listener(|this,_,_,cx|this.refresh_addresses(cx))))
                .children(self.address_error.clone().map(|e|div().text_color(t::red()).child(e)))
                .child(button("copy-diagnostics","Copy diagnostics").on_click(|_,_,cx|{if let Ok(json)=canopy_desktop::remote::metrics::snapshot_json(){cx.write_to_clipboard(ClipboardItem::new_string(json));}}))
                .child(button("refresh-devices","Refresh devices").disabled(busy).on_click(cx.listener(|this,_,_,cx|this.state.update(cx,|state,cx|state.refresh_devices(cx))))));
        }
        content
    }
}
fn qr_view(qr: Arc<qrcodegen::QrCode>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            // QR readers require a light quiet zone and dark modules in every theme.
            window.paint_quad(fill(bounds, t::qr_light()));
            let modules = qr.size() + 8;
            let unit = (f32::from(bounds.size.width) / modules as f32)
                .floor()
                .max(1.);
            for y in 0..qr.size() {
                for x in 0..qr.size() {
                    if qr.get_module(x, y) {
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin
                                    + point(px((x + 4) as f32 * unit), px((y + 4) as f32 * unit)),
                                size(px(unit), px(unit)),
                            ),
                            t::qr_dark(),
                        ));
                    }
                }
            }
        },
    )
    .size(px(320.))
}
