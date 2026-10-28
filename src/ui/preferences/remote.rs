//! Connection setup. RemoteState owns the listener and enrollment; this retained
//! view owns disclosure, address choice and permission edits, never socket I/O.
use super::{CONTENT_X, SECTION_GAP};
use crate::{
    app_state::{AppState, project_commands, remote::RemoteState},
    ui::{
        components::{integrations::integration_message, modal::Confirmation, *},
        remote_client::{self, ConnectionSetup, ConnectionSetupEvent, Connections},
        theme as t,
    },
};
use canopy_desktop::{
    host::{ALL_SCOPES, DeviceGrant},
    motion,
    remote::{
        client::{HostClient, Phase},
        enrollment::EnrollmentRpc,
        network::{LocalAddress, local_addresses},
    },
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName, Sizable,
        button::{Button, ButtonVariants},
        input::{InputState, TextareaState},
        menu::{DropdownMenu, PopupMenuItem},
        scroll::ScrollableElement,
        spinner::Spinner,
        switch::Switch,
    },
    prelude::FluentBuilder,
    *,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
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
const PRESETS: [(u32, &str); 4] = [
    (ALL_SCOPES, "Full access"),
    (63, "Control"),
    (7, "Chat"),
    (1, "View only"),
];
const QR_SIZE: f32 = 160.;

/// The page region an error belongs to, so it renders next to what failed.
#[derive(Clone, Copy, PartialEq)]
enum Area {
    Access,
    Devices,
    Grant,
    Computers,
    Advanced,
}

pub(super) struct RemotePreferences {
    state: Entity<RemoteState>,
    connections: Option<Entity<Connections>>,
    /// Inline Add computer form at the end of Other computers.
    add_computer: Option<Entity<ConnectionSetup>>,
    /// Computer whose routes are expanded below its row.
    computer_routes: Option<(String, Entity<ConnectionSetup>)>,
    add_computer_events: Option<Subscription>,
    computer_routes_events: Option<Subscription>,
    address: Entity<InputState>,
    advertised: Entity<InputState>,
    roots: Entity<TextareaState>,
    scopes: u32,
    editing: Option<(String, u64)>,
    setup: bool,
    advanced: Disclosure,
    grant_advanced: Disclosure,
    address_picker: bool,
    addresses: Vec<LocalAddress>,
    addresses_loading: bool,
    address_loading: ButtonLoading,
    grant_loading: ButtonLoading,
    projects: Vec<project_commands::WorkspaceSummary>,
    qr: Option<Arc<qrcodegen::QrCode>>,
    invite_uri: Option<String>,
    copied: bool,
    error: Option<(Area, String)>,
    /// Where RemoteState's shared error is shown: the area of the last request.
    state_area: Area,
    address_error: Option<String>,
    focus: FocusHandle,
    confirmation: Option<Entity<Confirmation>>,
    confirmation_events: Option<Subscription>,
    scroll: ScrollHandle,
    qr_task: Option<Task<()>>,
    load_task: Option<Task<()>>,
    copied_task: Option<Task<()>>,
    _observers: Vec<Subscription>,
    seen_address: String,
    seen_pending: Option<String>,
}
impl RemotePreferences {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().remote.clone();
        let address = state.read(cx).preferences().address().to_string();
        let connections = remote_client::connections(cx);
        let connection_error = connections
            .as_ref()
            .err()
            .map(|error| (Area::Computers, error.to_string()));
        let connections = connections.ok();
        let mut observers = vec![cx.observe_in(&state, window, |this, state, window, cx| {
            let (address, pending, uri, editing_saved, busy) = {
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
                    state.busy(),
                )
            };
            if !busy {
                this.grant_loading.set(false, cx);
            }
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
                    this.scopes = ALL_SCOPES;
                    this.grant_advanced = Disclosure::new(false, Instant::now());
                    this.roots
                        .update(cx, |field, cx| field.set_value("*", window, cx));
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
                this.copied = false;
                this.qr = None;
                this.qr_task = uri.map(|uri| {
                    cx.spawn(async move |this, cx| {
                        let qr = cx
                            .background_spawn(async move {
                                qrcodegen::QrCode::encode_text(&uri, qrcodegen::QrCodeEcc::Medium)
                                    .map(Arc::new)
                            })
                            .await;
                        let _ =
                            this.update(cx, |this, cx| {
                                match qr {
                                    Ok(qr) => this.qr = Some(qr),
                                    Err(_) => this.error = Some((
                                        Area::Devices,
                                        "Couldn’t create the QR code. Copy the invitation instead."
                                            .into(),
                                    )),
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
        let now = Instant::now();
        let mut view = Self {
            state,
            connections,
            add_computer: None,
            computer_routes: None,
            add_computer_events: None,
            computer_routes_events: None,
            address: cx.new(|cx| InputState::new(window, cx).default_value(address.clone())),
            advertised: cx.new(|cx| InputState::new(window, cx).placeholder("Automatic")),
            roots: cx.new(|cx| TextareaState::new(window, cx).default_value("*")),
            scopes: ALL_SCOPES,
            editing: None,
            setup: false,
            advanced: Disclosure::new(false, now),
            grant_advanced: Disclosure::new(false, now),
            address_picker: false,
            addresses: vec![],
            addresses_loading: false,
            address_loading: ButtonLoading::default(),
            grant_loading: ButtonLoading::default(),
            projects: vec![],
            qr: None,
            invite_uri: None,
            copied: false,
            error: connection_error,
            state_area: Area::Access,
            address_error: None,
            focus: cx.focus_handle(),
            confirmation: None,
            confirmation_events: None,
            scroll: ScrollHandle::new(),
            qr_task: None,
            load_task: None,
            copied_task: None,
            _observers: observers,
            seen_address: address,
            seen_pending: None,
        };
        view.refresh_addresses(cx);
        view.state.update(cx, |state, cx| state.refresh_devices(cx));
        view
    }
    fn fail(&mut self, area: Area, error: impl ToString, cx: &mut Context<Self>) {
        self.error = Some((area, error.to_string()));
        cx.notify();
    }
    /// The local error, or RemoteState's error when its last request came from `area`.
    fn error_for(&self, area: Area, cx: &App) -> Option<String> {
        if let Some((owner, error)) = &self.error
            && *owner == area
        {
            return Some(error.clone());
        }
        let state = self.state.read(cx);
        let grant_visible = state.pending().is_some() || self.editing.is_some();
        let owner = match self.state_area {
            Area::Grant if !grant_visible => Area::Devices,
            owner => owner,
        };
        (owner == area).then(|| state.error.clone()).flatten()
    }
    fn refresh_addresses(&mut self, cx: &mut Context<Self>) {
        if self.addresses_loading {
            return;
        }
        self.addresses_loading = true;
        self.address_loading.set(true, cx);
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
                this.address_loading.set(false, cx);
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
        self.state_area = Area::Access;
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
            self.fail(Area::Devices, "No network address was found. Connect to Wi-Fi or VPN and refresh, or enter an address in Advanced settings.", cx);
            return;
        };
        self.error = None;
        self.state_area = Area::Devices;
        self.setup = true;
        self.state.update(cx, |state, cx| state.pair(address, cx));
    }
    fn copy_invitation(&mut self, uri: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(uri));
        self.copied = true;
        self.copied_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn test_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                self.fail(Area::Advanced, error, cx);
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
        self.state_area = Area::Advanced;
        self.state.update(cx, |state, cx| state.pair(address, cx));
        let uri = self
            .state
            .read(cx)
            .invite
            .as_ref()
            .map(EnrollmentRpc::invite_uri);
        if let Some(uri) = uri {
            self.open_add_computer(Some(uri), window, cx);
        }
    }
    /// Shows the pairing form inline under Other computers.
    fn open_add_computer(
        &mut self,
        invitation: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.add_computer.is_some() && invitation.is_none() {
            return;
        }
        self.error = None;
        match ConnectionSetup::add_computer(invitation, window, cx) {
            Ok(setup) => {
                self.add_computer_events = Some(cx.subscribe(
                    &setup,
                    |this, _, _: &ConnectionSetupEvent, cx| {
                        // Paired: the computer joins the list from the registry.
                        this.add_computer = None;
                        this.add_computer_events = None;
                        cx.notify();
                    },
                ));
                self.add_computer = Some(setup);
            }
            Err(error) => self.fail(Area::Computers, error, cx),
        }
        cx.notify();
    }
    /// Expands or collapses one computer's routes below its row.
    fn toggle_routes(
        &mut self,
        host: Arc<HostClient>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = host.id().as_str().to_owned();
        if self
            .computer_routes
            .as_ref()
            .is_some_and(|(open, _)| *open == id)
        {
            self.computer_routes = None;
            self.computer_routes_events = None;
            cx.notify();
            return;
        }
        self.error = None;
        match ConnectionSetup::routes(host, window, cx) {
            Ok(setup) => {
                self.computer_routes_events = Some(cx.subscribe(
                    &setup,
                    |this, _, _: &ConnectionSetupEvent, cx| {
                        this.computer_routes = None;
                        this.computer_routes_events = None;
                        cx.notify();
                    },
                ));
                self.computer_routes = Some((id, setup));
            }
            Err(error) => self.fail(Area::Computers, error, cx),
        }
        cx.notify();
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
            Err(error) => return self.fail(Area::Grant, error, cx),
        };
        if self.scopes & 1 == 0 {
            return self.fail(Area::Grant, "View access is needed to connect and load this computer. Select it before approving other permissions.", cx);
        }
        let scopes = self.scopes;
        self.state_area = Area::Grant;
        if let Some(id) = pending {
            self.state.update(cx, |state, cx| {
                state.resolve_pairing(id, Some((scopes, roots)), cx)
            });
        } else if let Some((id, revision)) = self.editing.clone() {
            self.state.update(cx, |state, cx| {
                state.update_device(id, Some((revision, scopes, roots)), cx)
            });
        }
        self.grant_loading.set(self.state.read(cx).busy(), cx);
        self.error = None;
        cx.notify();
    }
    fn edit_device(&mut self, device: &DeviceGrant, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = Some((device.id().into(), device.authorization_revision()));
        self.scopes = device.scopes();
        self.error = None;
        self.roots.update(cx, |field, cx| {
            field.set_value(device.roots().join("\n"), window, cx)
        });
        self.refresh_addresses(cx);
        cx.notify();
    }
    fn share_everything(&mut self, id: String, revision: u64, cx: &mut Context<Self>) {
        self.error = None;
        self.state_area = Area::Devices;
        self.state.update(cx, |state, cx| {
            state.update_device(id, Some((revision, ALL_SCOPES, vec!["*".into()])), cx)
        });
    }
    fn revoke(&mut self, id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirmation.is_some() {
            return;
        }
        self.error = None;
        self.state_area = Area::Devices;
        let state = self.state.clone();
        let progress = state.clone();
        let focus = self.focus.clone();
        let modal = cx.new(|cx| {
            let mut modal = Confirmation::new(
                format!("Remove “{name}”?"),
                "This device will need a new invitation to connect again. Work this computer already accepted continues.",
                "Remove",
                focus,
                move |cx| {
                    if state.read(cx).busy() {
                        return Err("Another change is still being saved. Try again in a moment.".into());
                    }
                    state.update(cx, |state, cx| state.update_device(id.clone(), None, cx));
                    Ok(())
                },
                window,
                cx,
            )
            .destructive();
            modal.wait_for(&progress, |state| (state.busy(), state.error.clone()), cx);
            modal
        });
        self.confirmation_events = Some(cx.subscribe(&modal, |this, _, _: &ModalDismissed, cx| {
            this.confirmation = None;
            this.confirmation_events = None;
            cx.notify();
        }));
        self.confirmation = Some(modal);
        cx.notify();
    }

    fn this_computer(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let enabled = state.preferences().enabled();
        let running = state.address().is_some();
        let network = !state.preferences().address().ip().is_loopback();
        let mut rows = vec![
            setting(
                "Remote access",
                "Allow approved devices to use this computer while Canopy is running.",
                true,
                Switch::new("remote-enabled")
                    .checked(enabled)
                    .disabled(busy)
                    .on_change(cx.listener(|this, enabled, _, cx| {
                        let address = this.state.read(cx).preferences().address().to_string();
                        this.error = None;
                        this.state_area = Area::Access;
                        this.state
                            .update(cx, |state, cx| state.configure(*enabled, &address, cx));
                    })),
            )
            .into_any_element(),
        ];
        if enabled {
            rows.push(
                setting(
                    "Reachable from",
                    if network {
                        "Devices on the same Wi-Fi, LAN or VPN. Every new device still needs your approval."
                    } else {
                        "Other computers connect through an SSH tunnel to this computer."
                    },
                    false,
                    segmented([
                        segment("network-access", "Local network / VPN", network)
                            .disabled(busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.choose_method(true, window, cx)
                                }),
                            ),
                        segment("ssh-access", "SSH only", !network)
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_method(false, window, cx)
                            })),
                    ]),
                )
                .into_any_element(),
            );
            let (help, control) = if busy {
                ("Applying your change.", status_busy("Updating…"))
            } else if running {
                (
                    if network {
                        "Approved devices can reach this computer on your network."
                    } else {
                        "Approved devices can reach this computer through SSH."
                    },
                    status(t::green(), "Available"),
                )
            } else {
                (
                    "Remote access couldn’t start. Check the listen address in Advanced settings, then try again.",
                    row()
                        .gap(px(8.))
                        .child(status(t::red(), "Not running"))
                        .child(button("retry-listener", "Retry").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.error = None;
                                this.state_area = Area::Access;
                                this.state.update(cx, |state, cx| state.retry(cx))
                            },
                        ))),
                )
            };
            rows.push(setting("Status", help, false, control).into_any_element());
        }
        section(
            "THIS COMPUTER",
            "Let your approved devices use Canopy on this computer",
            rows,
        )
        .children(
            self.error_for(Area::Access, cx)
                .map(|error| integration_message(error, true)),
        )
    }

    fn devices(&self, busy: bool, now: Instant, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let running = state.address().is_some();
        let pending = state.pending().is_some();
        let devices = state
            .devices
            .iter()
            .filter(|d| !d.revoked())
            .map(|d| (d.clone(), state.device_online(d.id())))
            .collect::<Vec<_>>();
        let mut list = column().flex_shrink_0();
        for (index, (device, online)) in devices.iter().enumerate() {
            list = list.child(self.device_row(device, *online, index == 0, busy, cx));
            if !pending
                && self
                    .editing
                    .as_ref()
                    .is_some_and(|(id, _)| id == device.id())
            {
                list = list.children(self.grant(busy, now, cx).map(|grant| grant.mb(px(12.))));
            }
        }
        let flow = if pending {
            self.grant(busy, now, cx)
        } else if self.setup && running {
            Some(self.pairing(busy, cx))
        } else {
            None
        };
        let adding = flow.is_some();
        let empty = devices.is_empty() && !adding;
        // A new device joins the end of the list, like a new profile in AI providers.
        let content = column()
            .flex_shrink_0()
            .gap(px(12.))
            .children((!devices.is_empty()).then_some(list))
            .children(flow)
            .children(empty.then(|| {
                muted(if running {
                    "No devices yet. Add a phone or another computer to use Canopy here."
                } else {
                    "Turn on remote access to add a phone or another computer."
                })
            }))
            .children((!adding).then(|| {
                row().child(
                    button("new-device", "Add device")
                        .icon(icon(IconName::Plus))
                        .disabled(busy || !running)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.setup = true;
                            this.create_invitation(cx);
                        })),
                )
            }))
            .children(
                self.error_for(Area::Devices, cx)
                    .map(|error| integration_message(error, true)),
            );
        section(
            "DEVICES",
            "Phones and computers that can use this computer. You approve each new device.",
            vec![content.into_any_element()],
        )
    }

    fn device_row(
        &self,
        device: &DeviceGrant,
        online: bool,
        first: bool,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = device.id().to_owned();
        let name = device.name().to_owned();
        let full = device.scopes() == ALL_SCOPES && device.roots().iter().any(|r| r == "*");
        let edit = device.clone();
        let owner = cx.entity().downgrade();
        let revision = device.authorization_revision();
        let (share_id, remove_id, remove_name) = (id.clone(), id.clone(), name.clone());
        let menu = icon_button(
            SharedString::from(format!("device-menu-{id}")),
            IconName::Ellipsis,
            format!("More actions for {name}"),
        )
        .disabled(busy)
        .dropdown_menu(move |menu, _, _| {
            let (share, remove) = (owner.clone(), owner.clone());
            let (share_id, remove_id, remove_name) =
                (share_id.clone(), remove_id.clone(), remove_name.clone());
            let menu = if full {
                menu
            } else {
                menu.item(
                    PopupMenuItem::new("Share everything").on_click(move |_, _, cx| {
                        let _ = share.update(cx, |this, cx| {
                            this.share_everything(share_id.clone(), revision, cx)
                        });
                    }),
                )
                .separator()
            };
            menu.item(
                PopupMenuItem::new("Remove access…").on_click(move |_, window, cx| {
                    let _ = remove.update(cx, |this, cx| {
                        this.revoke(remove_id.clone(), remove_name.clone(), window, cx)
                    });
                }),
            )
        });
        list_row(first)
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(div().truncate().text_size(px(13.)).child(name))
                    .child(
                        row()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(if online {
                                status(t::green(), "Connected")
                            } else {
                                status(t::faint(), "Offline")
                            })
                            .child(badge(access_level(device.scopes())))
                            .child(badge(shared_access(device.roots()))),
                    ),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .child(
                        button(SharedString::from(format!("edit-{id}")), "Permissions…")
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.edit_device(&edit, window, cx)
                            })),
                    )
                    .child(menu),
            )
    }

    fn pairing(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        let state = self.state.read(cx);
        let network = state.address().is_some_and(|a| !a.ip().is_loopback());
        let expires = state
            .invite
            .as_ref()
            .and_then(|invite| chrono::DateTime::from_timestamp(invite.expires_unix as i64, 0))
            .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string());
        let address = state
            .invite
            .as_ref()
            .map(|invite| invite.address.clone())
            .or_else(|| self.invitation_address(cx));
        let expired = self.invite_uri.is_none();
        let code: AnyElement = match (&self.qr, expired) {
            (Some(qr), false) => qr_view(qr.clone()).into_any_element(),
            (_, false) => skeleton_bar().size(px(QR_SIZE)).into_any_element(),
            (_, true) => row()
                .size(px(QR_SIZE))
                .justify_center()
                .rounded(px(4.))
                .bg(t::hover())
                .text_size(px(11.))
                .text_color(t::muted())
                .child("Invitation expired")
                .into_any_element(),
        };
        let heading = column()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(if expired {
                        "Invitation expired"
                    } else {
                        "Waiting for the other device"
                    }),
            )
            .child(if expired {
                status(t::faint(), "Create a new invitation to continue")
            } else {
                status(
                    t::yellow(),
                    expires
                        .map(|at| format!("Expires at {at}"))
                        .unwrap_or_else(|| "Expires in five minutes".into()),
                )
            });
        let instructions = if network {
            "Scan the code with Canopy on your phone, or paste the invitation in Connections → Add computer on your other computer. Then approve the device here."
        } else {
            "On the other computer, open Connections → Add computer, choose SSH tunnel and paste this invitation. Then approve the device here."
        };
        let copy = self.invite_uri.clone();
        let actions = row()
            .gap(px(8.))
            .child(if let Some(uri) = copy {
                button(
                    "copy-invitation",
                    if self.copied {
                        "Copied"
                    } else {
                        "Copy invitation"
                    },
                )
                .icon(icon(if self.copied {
                    IconName::Check
                } else {
                    IconName::Copy
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.copy_invitation(uri.clone(), cx)))
            } else {
                button("renew-invitation", "New invitation")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.create_invitation(cx)))
            })
            .children((!expired).then(|| {
                button("renew-invitation", "New invitation")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.create_invitation(cx)))
            }))
            .child(div().flex_1())
            .child(
                button("hide-invitation", "Done").on_click(cx.listener(|this, _, _, cx| {
                    this.setup = false;
                    this.address_picker = false;
                    cx.notify();
                })),
            );
        card()
            .child(
                row()
                    .items_start()
                    .gap(px(16.))
                    .child(div().flex_shrink_0().child(code))
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .gap(px(12.))
                            .child(heading)
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(17.))
                                    .text_color(t::secondary())
                                    .child(instructions),
                            )
                            .children(network.then(|| self.invitation_address_row(address, cx))),
                    ),
            )
            .children((network && self.address_picker).then(|| self.address_picker_list(busy, cx)))
            .child(actions)
    }

    fn invitation_address_row(&self, address: Option<String>, cx: &mut Context<Self>) -> Div {
        row()
            .gap(px(8.))
            .child(muted("Address"))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(t::MONO)
                    .text_size(px(12.))
                    .child(address.unwrap_or_else(|| "Not found".into())),
            )
            .child(
                button(
                    "choose-address",
                    if self.address_picker {
                        "Hide addresses"
                    } else {
                        "Change…"
                    },
                )
                .ghost()
                .border_0()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.address_picker = !this.address_picker;
                    cx.notify();
                })),
            )
    }

    fn address_picker_list(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        let chosen = self.invitation_address(cx);
        let mut list = column().flex_shrink_0().gap(px(2.));
        for address in &self.addresses {
            let ip = address.ip().to_string();
            let selected = chosen.as_deref() == Some(&ip);
            let label = format!("{} · {ip}", address.interface());
            list = list.child(
                row()
                    .id(SharedString::from(format!("network-{ip}")))
                    .keyboard(label)
                    .h(px(28.))
                    .px(px(8.))
                    .gap(px(8.))
                    .rounded(px(4.))
                    .when(selected, |row| row.bg(t::selected()))
                    .when(!selected && !busy, |row| {
                        row.hover(|style| style.bg(t::hover()))
                    })
                    .when(!busy, |row| {
                        row.on_click(cx.listener(move |this, _, window, cx| {
                            this.advertised
                                .update(cx, |field, cx| field.set_value(ip.clone(), window, cx));
                            this.create_invitation(cx);
                        }))
                    })
                    .child(
                        div().w(px(14.)).flex_shrink_0().children(
                            selected.then(|| icon(IconName::Check).text_color(t::accent())),
                        ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .child(address.interface().to_owned()),
                    )
                    .child(
                        div()
                            .font_family(t::MONO)
                            .text_size(px(12.))
                            .text_color(t::muted())
                            .child(address.ip().to_string()),
                    ),
            );
        }
        column()
            .flex_shrink_0()
            .gap(px(8.))
            .pt(px(12.))
            .border_t_1()
            .border_color(t::border())
            .children(
                (self.addresses_loading && self.addresses.is_empty())
                    .then(|| loading("Looking for network addresses…")),
            )
            .children(
                (!self.addresses_loading
                    && self.addresses.is_empty()
                    && self.address_error.is_none())
                .then(|| muted("No network addresses found. Connect to Wi-Fi or VPN and refresh.")),
            )
            .child(list)
            .children(
                self.address_error
                    .clone()
                    .map(|error| integration_message(error, true)),
            )
            .child(
                row().child(
                    loading_button(
                        "refresh-addresses",
                        "Refresh addresses",
                        &self.address_loading,
                    )
                    .disabled(self.addresses_loading)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_addresses(cx))),
                ),
            )
    }

    /// Approval of a pending device, or permission edits for an existing one.
    fn grant(&self, busy: bool, now: Instant, cx: &mut Context<Self>) -> Option<Div> {
        let state = self.state.read(cx);
        let pending = state.pending();
        let editing = self.editing.as_ref().and_then(|(id, _)| {
            state
                .devices
                .iter()
                .find(|device| device.id() == id)
                .map(|device| device.name().to_owned())
        });
        if pending.is_none() && self.editing.is_none() {
            return None;
        }
        let pending_id = pending.as_ref().map(|p| p.id().to_owned());
        let (title, subtitle) = match (&pending, editing) {
            (Some(pending), _) => (
                format!("“{}” wants to connect", pending.name()),
                "Choose what this device can use, then approve it.",
            ),
            (None, Some(name)) => (
                format!("Permissions for “{name}”"),
                "Choose what this device can use on this computer.",
            ),
            (None, None) => (
                "Device permissions".into(),
                "Choose what this device can use on this computer.",
            ),
        };
        let roots = self.roots(cx).unwrap_or_default();
        let all = roots.iter().any(|r| r == "*");
        let private_only = roots.len() == 1 && roots[0] == "private";
        let commit = if pending.is_some() && all && self.scopes == ALL_SCOPES {
            "Approve full access"
        } else if pending.is_some() && private_only {
            "Approve private chats only"
        } else if pending.is_some() {
            "Approve device"
        } else {
            "Save permissions"
        };
        let actions = row()
            .gap(px(8.))
            .justify_end()
            .child(
                button(
                    "cancel-permissions",
                    if pending.is_some() { "Deny" } else { "Cancel" },
                )
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    let pending = this.state.read(cx).pending().map(|p| p.id().to_owned());
                    this.error = None;
                    if let Some(id) = pending {
                        this.state_area = Area::Grant;
                        this.state
                            .update(cx, |state, cx| state.resolve_pairing(id, None, cx));
                    } else {
                        this.editing = None;
                        cx.notify();
                    }
                })),
            )
            .child(
                primary_loading_button("save-permissions", commit, &self.grant_loading)
                    .disabled(busy && !self.grant_loading.active())
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.save_grant(pending_id.clone(), cx)),
                    ),
            );
        Some(
            card()
                .child(
                    column()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .line_height(px(14.85))
                                .text_color(t::muted())
                                .child(subtitle),
                        ),
                )
                .child(form_field(
                    "Access",
                    match self.scopes {
                        1 => "Read history, files and Tasks. No changes or messages.",
                        7 => "Read, send messages and approve agent tools.",
                        63 => {
                            "Use chats, files, Git, terminals and Tasks. Host settings stay protected."
                        }
                        ALL_SCOPES => {
                            "Use all projects, chats, files, Git, terminals, Tasks and host settings."
                        }
                        _ => "Custom permissions, set in Advanced permissions.",
                    },
                    segmented(PRESETS.into_iter().map(|(scopes, label)| {
                        segment(
                            ("permission-preset", scopes as usize),
                            label,
                            self.scopes == scopes,
                        )
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.scopes = scopes;
                            if scopes == ALL_SCOPES {
                                this.roots
                                    .update(cx, |field, cx| field.set_value("*", window, cx));
                            }
                            cx.notify();
                        }))
                    })),
                ))
                .child(self.resources(&roots, all, private_only, busy, cx))
                .child(self.grant_details(busy, now, cx))
                .children(
                    self.error_for(Area::Grant, cx)
                        .map(|error| integration_message(error, true)),
                )
                .child(actions),
        )
    }

    fn resources(
        &self,
        roots: &[String],
        all: bool,
        private_only: bool,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut body = column()
            .flex_shrink_0()
            .gap(px(8.))
            .child(field_label("Projects and chats"))
            .child(
                checkbox("all-resource-access", all, "All projects and chats")
                    .label("All projects and chats")
                    .text_size(px(13.))
                    .disabled(busy)
                    .on_change(cx.listener(|this, checked, window, cx| {
                        this.roots.update(cx, |field, cx| {
                            field.set_value(if *checked { "*" } else { "private" }, window, cx)
                        });
                        this.error = None;
                        cx.notify();
                    })),
            );
        if all {
            return body.child(help(
                "Includes private chats and every project you open later. No need to share each one separately.",
            ));
        }
        body = body.child(
            checkbox(
                "private-chat-access",
                roots.iter().any(|r| r == "private"),
                "Private chats",
            )
            .text_size(px(13.))
            .disabled(busy)
            .on_change(cx.listener(|this, checked, window, cx| {
                this.toggle_root("private".into(), *checked, window, cx)
            }))
            .child(choice_text(
                "Private chats",
                "Private chats and global resources",
            )),
        );
        for project in &self.projects {
            let Some(root) = project.canonical_root.to_str() else {
                continue;
            };
            let root = root.to_owned();
            body = body.child(
                checkbox(
                    SharedString::from(format!("project-grant-{root}")),
                    roots.contains(&root),
                    project.name.clone(),
                )
                .text_size(px(13.))
                .disabled(busy)
                .on_change(cx.listener({
                    let root = root.clone();
                    move |this, checked, window, cx| {
                        this.toggle_root(root.clone(), *checked, window, cx)
                    }
                }))
                .child(choice_text(
                    project.name.clone(),
                    project.canonical_root.display().to_string(),
                )),
            );
        }
        if self.projects.is_empty() {
            body = body.child(help("Open a project on this computer to share it here."));
        }
        if private_only {
            body = body.child(help(
                "Only private chats are selected. Choose projects above, or allow all projects and chats.",
            ));
        }
        body
    }

    fn grant_details(&self, busy: bool, now: Instant, cx: &mut Context<Self>) -> Div {
        let mut scopes = column().flex_shrink_0().gap(px(8.));
        for (bit, label) in SCOPES {
            scopes = scopes.child(
                checkbox(("scope", bit as usize), self.scopes & bit != 0, label)
                    .label(label)
                    .text_size(px(13.))
                    .disabled(busy)
                    .on_change(cx.listener(move |this, checked, _, cx| {
                        if *checked {
                            this.scopes |= bit;
                        } else {
                            this.scopes &= !bit;
                        }
                        cx.notify();
                    })),
            );
        }
        column()
            .flex_shrink_0()
            .child(
                self.grant_advanced
                    .header("advanced-permissions", "ADVANCED PERMISSIONS", now, cx)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.grant_advanced.toggle(Instant::now(), cx);
                        cx.notify();
                    })),
            )
            .child(self.grant_advanced.measured_body(
                "advanced-permissions-body",
                column()
                    .flex_shrink_0()
                    .gap(px(16.))
                    .pb(px(4.))
                    .child(scopes)
                    .child(form_field(
                        "Allowed folders",
                        "One full folder path per line. private allows private chats; * allows every current and future project.",
                        textarea(&self.roots).h(px(100.)).w_full().disabled(busy),
                    )),
                now,
            ))
    }

    fn computers(&self, cx: &mut Context<Self>) -> Div {
        let mut content = column().flex_shrink_0().gap(px(12.));
        if let Some(connections) = &self.connections {
            let (hosts, loaded, error) = {
                let connections = connections.read(cx);
                (
                    connections.hosts(),
                    !connections.loading(),
                    connections.error().map(str::to_owned),
                )
            };
            if !loaded {
                content = content.child(loading("Loading saved computers…"));
            }
            if hosts.is_empty() && loaded && error.is_none() {
                content = content.child(muted(
                    "No other computers yet. After you add one, its projects and chats appear in the sidebar.",
                ));
            }
            content = content.children(error.map(|error| integration_message(error, true)));
            if !hosts.is_empty() {
                let mut list = column().flex_shrink_0();
                for (index, host) in hosts.into_iter().enumerate() {
                    let id = host.id().as_str().to_owned();
                    list = list.child(self.computer_row(host, index == 0, cx));
                    if let Some((_, routes)) = self
                        .computer_routes
                        .as_ref()
                        .filter(|(open, _)| *open == id)
                    {
                        list = list.child(card().mb(px(12.)).child(routes.clone()));
                    }
                }
                content = content.child(list);
            }
        }
        let adding = self.add_computer.clone().map(|setup| {
            card()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Add computer"),
                )
                .child(setup)
        });
        section(
            "OTHER COMPUTERS",
            "Computers you can open from here. They share all projects and chats unless their owner restricts this device.",
            vec![
                content
                    .children(adding)
                    .children(self.add_computer.is_none().then(|| {
                        row().child(
                            button("add-computer", "Add computer")
                                .icon(icon(IconName::Plus))
                                .disabled(self.connections.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_add_computer(None, window, cx)
                                })),
                        )
                    }))
                    .children(
                        self.error_for(Area::Computers, cx)
                            .map(|error| integration_message(error, true)),
                    )
                    .into_any_element(),
            ],
        )
    }

    fn computer_row(&self, host: Arc<HostClient>, first: bool, cx: &mut Context<Self>) -> Div {
        let id = host.id().as_str().to_owned();
        let name = host.name();
        let phase = host.phase();
        let active = host.active_route();
        let via = host
            .routes()
            .into_iter()
            .find(|route| Some(route.id()) == active.as_deref())
            .map(|route| route.name().to_owned());
        let catalog = match host.shared_catalog() {
            Some(catalog) if catalog.projects.is_empty() => {
                format!("No shared projects — open one on {name}")
            }
            Some(catalog) if catalog.projects.len() == 1 => "1 shared project".into(),
            Some(catalog) => format!("{} shared projects", catalog.projects.len()),
            None => "Open to load shared projects".into(),
        };
        let open = host.clone();
        let owner = cx.entity().downgrade();
        let menu_host = host.clone();
        let expanded = self
            .computer_routes
            .as_ref()
            .is_some_and(|(open, _)| *open == id);
        let menu = icon_button(
            SharedString::from(format!("computer-menu-{id}")),
            IconName::Ellipsis,
            format!("More actions for {name}"),
        )
        .dropdown_menu(move |menu, _, _| {
            let (routes, disconnect) = (owner.clone(), owner.clone());
            let (route_host, pause_host) = (menu_host.clone(), menu_host.clone());
            let menu = menu.item(
                PopupMenuItem::new(if expanded { "Hide routes" } else { "Routes" }).on_click(
                    move |_, window, cx| {
                        let _ = routes.update(cx, |this, cx| {
                            this.toggle_routes(route_host.clone(), window, cx)
                        });
                    },
                ),
            );
            if menu_host.phase() != Phase::Ready {
                return menu;
            }
            menu.separator()
                .item(PopupMenuItem::new("Disconnect").on_click(move |_, _, cx| {
                    if let Err(error) = pause_host.pause_connection() {
                        let _ =
                            disconnect.update(cx, |this, cx| this.fail(Area::Computers, error, cx));
                    }
                }))
        });
        let (dot, label) = match phase {
            Phase::Ready => (t::green(), "Connected"),
            Phase::Connecting => (t::yellow(), "Connecting…"),
            Phase::Offline => (t::faint(), "Offline"),
        };
        list_row(first)
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(div().min_w_0().truncate().text_size(px(13.)).child(name))
                            .children(host.is_mutual().then(|| badge("Two-way").flex_shrink_0())),
                    )
                    .child(
                        row()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(status(dot, label))
                            .children(
                                via.filter(|_| phase == Phase::Ready)
                                    .map(|via| muted(format!("via {via}"))),
                            )
                            .child(muted("·"))
                            .child(muted(catalog)),
                    )
                    .children(host.connection_error().map(|error| {
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
                        button(SharedString::from(format!("open-{id}")), "Open…").on_click(
                            cx.listener(move |this, _, _, cx| {
                                if let Err(error) = remote_client::open_host(open.clone(), cx) {
                                    this.fail(Area::Computers, error, cx);
                                }
                            }),
                        ),
                    )
                    .child(menu),
            )
    }

    fn advanced_settings(&self, busy: bool, now: Instant, cx: &mut Context<Self>) -> Div {
        let running = self.state.read(cx).address().is_some();
        let detected = if self.addresses_loading && self.addresses.is_empty() {
            "Looking for network addresses…".to_owned()
        } else if self.addresses.is_empty() {
            "No network addresses found.".to_owned()
        } else {
            self.addresses
                .iter()
                .map(|a| format!("{} · {}", a.interface(), a.ip()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let rows = column()
            .flex_shrink_0()
            .pt(px(8.))
            .child(setting(
                "Listen address",
                "Where this computer accepts connections. Default port: 44330.",
                true,
                row()
                    .gap(px(8.))
                    .child(input(&self.address).w(px(180.)).disabled(busy))
                    .child(
                        button("apply-listener", "Apply")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let address = this.address.read(cx).value().to_string();
                                let enabled = this.state.read(cx).preferences().enabled();
                                this.error = None;
                                this.state_area = Area::Advanced;
                                this.state
                                    .update(cx, |state, cx| state.configure(enabled, &address, cx));
                            })),
                    ),
            ))
            .child(setting(
                "Address in invitation",
                "Usually detected automatically. Enter an IP address or VPN name without https:// or a port.",
                false,
                input(&self.advertised).w(px(180.)).disabled(busy),
            ))
            .child(setting(
                "Network addresses",
                detected,
                false,
                loading_button("refresh-network", "Refresh", &self.address_loading)
                    .disabled(self.addresses_loading)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_addresses(cx))),
            ))
            .children(
                self.address_error
                    .clone()
                    .map(|error| integration_message(error, true).mb(px(12.))),
            )
            .child(setting(
                "Test on this computer",
                "Add this computer under Other computers to try a connection. It still asks for your approval.",
                false,
                button("test-here", "Test")
                    .disabled(
                        busy || !running
                            || self
                                .connections
                                .as_ref()
                                .is_none_or(|c| c.read(cx).loading()),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.test_here(window, cx))),
            ))
            .child(setting(
                "Device list",
                "Reload the devices that can use this computer.",
                false,
                button("refresh-devices", "Refresh")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.error = None;
                        this.state_area = Area::Advanced;
                        this.state.update(cx, |state, cx| state.refresh_devices(cx))
                    })),
            ))
            .child(setting(
                "Diagnostics",
                "Copy connection diagnostics for troubleshooting.",
                false,
                button("copy-diagnostics", "Copy diagnostics").on_click(|_, _, cx| {
                    if let Ok(json) = canopy_desktop::remote::metrics::snapshot_json() {
                        cx.write_to_clipboard(ClipboardItem::new_string(json));
                    }
                }),
            ))
            .children(
                self.error_for(Area::Advanced, cx)
                    .map(|error| integration_message(error, true)),
            );
        column()
            .flex_shrink_0()
            .child(
                self.advanced
                    .header("advanced-connections", "ADVANCED SETTINGS", now, cx)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.advanced.toggle(Instant::now(), cx);
                        cx.notify();
                    })),
            )
            .child(
                self.advanced
                    .measured_body("advanced-connections-body", rows, now),
            )
    }
}
impl Render for RemotePreferences {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        motion::request_frame(
            window,
            self.advanced.active(now) || self.grant_advanced.active(now),
        );
        let state = self.state.read(cx);
        let busy = state.busy() || state.quitting();
        column()
            .size_full()
            .track_focus(&self.focus)
            .child(page_header(
                "Connections",
                "Use this computer from your other devices, and open other computers here",
            ))
            .child(
                column()
                    .id("connections-preferences")
                    .flex_1()
                    .track_scroll(&self.scroll)
                    .overflow_y_scrollbar()
                    .px(px(CONTENT_X))
                    .py(px(20.))
                    .child(
                        column()
                            .flex_shrink_0()
                            .gap(px(SECTION_GAP))
                            .child(self.this_computer(busy, cx))
                            .child(self.devices(busy, now, cx))
                            .child(self.computers(cx))
                            .child(self.advanced_settings(busy, now, cx)),
                    ),
            )
            .children(self.confirmation.clone())
    }
}

/// Same header as General: title, one-line subtitle and a bottom hairline.
fn page_header(title: &'static str, subtitle: &'static str) -> Div {
    column()
        .flex_shrink_0()
        .px(px(CONTENT_X))
        .pt(px(20.))
        .pb(px(12.))
        .gap(px(2.))
        .border_b_1()
        .border_color(t::border())
        .child(
            div()
                .text_size(px(14.))
                .line_height(px(16.1))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_size(px(11.))
                .line_height(px(14.85))
                .text_color(t::muted())
                .child(subtitle),
        )
}
/// Bordered surface for a multi-step flow inside a section (pairing, approval).
fn card() -> Div {
    column()
        .flex_shrink_0()
        .gap(px(16.))
        .p(px(16.))
        .rounded(px(6.))
        .border_1()
        .border_color(t::border())
}
/// Row in a section list; hairlines separate rows, never precede the first.
fn list_row(first: bool) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(12.))
        .py(px(12.))
        .border_t(px(if first { 0. } else { 1. }))
        .border_color(t::border())
}
/// One choice group sharing a single silhouette, like the GitHub token kind.
fn segmented(segments: impl IntoIterator<Item = Button>) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(6.))
        .bg(t::hover())
        .children(segments)
}
fn segment(id: impl Into<ElementId>, label: &'static str, selected: bool) -> Button {
    selection_button(id, label, selected)
        .border_0()
        .text_color(if selected { t::text() } else { t::secondary() })
}
/// Status dot and its label; the label keeps state readable without color.
fn status(color: Hsla, label: impl Into<SharedString>) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(6.))
        .child(dot().size(px(6.)).bg(color))
        .child(
            div()
                .text_size(px(11.))
                .text_color(t::secondary())
                .child(label.into()),
        )
}
fn status_busy(label: &'static str) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(6.))
        .child(Spinner::new().xsmall().color(t::muted()))
        .child(
            div()
                .text_size(px(11.))
                .text_color(t::secondary())
                .child(label),
        )
}
fn loading(label: &'static str) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(8.))
        .child(Spinner::new().xsmall().color(t::muted()))
        .child(muted(label))
}
fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .line_height(px(14.85))
        .text_color(t::muted())
        .child(text.into())
}
fn help(text: &'static str) -> Div {
    muted(text).pl(px(24.))
}
fn field_label(text: &'static str) -> Div {
    div()
        .text_size(px(13.))
        .line_height(px(18.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t::secondary())
        .child(text)
}
/// Checkbox content with a secondary line, e.g. a project's folder.
fn choice_text(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Div {
    column()
        .min_w_0()
        .gap(px(2.))
        .child(div().text_size(px(13.)).child(title.into()))
        .child(muted(detail).truncate())
}
fn qr_view(qr: Arc<qrcodegen::QrCode>) -> impl IntoElement {
    div().p(px(8.)).rounded(px(6.)).bg(t::qr_light()).child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                // QR readers require a light quiet zone and dark modules in every theme.
                window.paint_quad(fill(bounds, t::qr_light()));
                let modules = qr.size() + 8;
                let side = f32::from(bounds.size.width);
                let unit = (side / modules as f32).floor().max(1.);
                let inset = ((side - unit * modules as f32) / 2.).max(0.).floor();
                for y in 0..qr.size() {
                    for x in 0..qr.size() {
                        if qr.get_module(x, y) {
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin
                                        + point(
                                            px(inset + (x + 4) as f32 * unit),
                                            px(inset + (y + 4) as f32 * unit),
                                        ),
                                    size(px(unit), px(unit)),
                                ),
                                t::qr_dark(),
                            ));
                        }
                    }
                }
            },
        )
        .size(px(QR_SIZE - 16.)),
    )
}
fn access_level(scopes: u32) -> &'static str {
    PRESETS
        .iter()
        .find(|(preset, _)| *preset == scopes)
        .map_or("Custom access", |(_, label)| label)
}
fn shared_access(roots: &[String]) -> String {
    let private = roots.iter().any(|root| root == "private");
    if roots.iter().any(|root| root == "*") {
        return "All projects and chats".into();
    }
    let count = roots
        .iter()
        .filter(|root| root.as_str() != "private")
        .count();
    match (count, private) {
        (0, true) => "Private chats only".into(),
        (0, false) => "Nothing shared".into(),
        (1, private) => format!("1 project{}", if private { " · private chats" } else { "" }),
        (count, private) => format!(
            "{count} projects{}",
            if private { " · private chats" } else { "" }
        ),
    }
}
