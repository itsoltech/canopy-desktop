//! Host controls. The app-owned RemoteState owns all I/O and listener lifetime.
use crate::{
    app_state::{AppState, remote::RemoteState},
    ui::{components::*, theme as t},
};
use canopy_desktop::remote::enrollment::EnrollmentRpc;
use gpui_kit::{
    base::Disableable,
    component::{
        input::{InputState, TextareaState},
        scroll::ScrollableElement,
    },
    *,
};
use std::sync::Arc;

const SCOPES: [(u32, &str); 7] = [
    (1, "Read"),
    (2, "Send messages"),
    (4, "Approve tools"),
    (8, "Files and Git"),
    (16, "Terminal input"),
    (32, "Tasks"),
    (64, "Host settings and devices"),
];
pub(super) struct RemotePreferences {
    state: Entity<RemoteState>,
    address: Entity<InputState>,
    advertised: Entity<InputState>,
    roots: Entity<TextareaState>,
    scopes: u32,
    editing: Option<(String, u64)>,
    qr: Option<Arc<qrcodegen::QrCode>>,
    invite_uri: Option<String>,
    error: Option<String>,
    scroll: ScrollHandle,
    qr_task: Option<Task<()>>,
    _observer: Subscription,
    seen_address: String,
}
impl RemotePreferences {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().remote.clone();
        let address = state.read(cx).preferences().address().to_string();
        let observer=cx.observe_in(&state,window,|this,state,window,cx|{
            let address=state.read(cx).preferences().address().to_string();
            if address!=this.seen_address {
                if this.address.read(cx).value().as_str()==this.seen_address { this.address.update(cx,|field,cx|field.set_value(address.clone(),window,cx)); }
                this.seen_address=address;
            }
            let uri=state.read(cx).invite.as_ref().map(EnrollmentRpc::invite_uri);
            if uri != this.invite_uri {
                this.invite_uri=uri.clone(); this.qr=None;
                this.qr_task=uri.map(|uri|cx.spawn(async move|this,cx|{
                    let qr=cx.background_spawn(async move{qrcodegen::QrCode::encode_text(&uri,qrcodegen::QrCodeEcc::Medium).map(Arc::new)}).await;
                    let _=this.update(cx,|this,cx|{match qr{Ok(qr)=>this.qr=Some(qr),Err(_)=>this.error=Some("Could not encode the pairing code. Copy the invitation instead.".into())}cx.notify();});
                }));
            }
            cx.notify();
        });
        Self {
            state,
            seen_address: address.clone(),
            address: cx.new(|cx| InputState::new(window, cx).default_value(address)),
            advertised: cx.new(|cx| {
                InputState::new(window, cx).placeholder("LAN / VPN IP or SSH forward address")
            }),
            roots: cx.new(|cx| TextareaState::new(window, cx).default_value("private")),
            scopes: 7,
            editing: None,
            qr: None,
            invite_uri: None,
            error: None,
            scroll: ScrollHandle::new(),
            qr_task: None,
            _observer: observer,
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
            return Err("Choose at least one checkout or private chats.".into());
        }
        if roots
            .iter()
            .any(|r| r != "private" && r != "*" && !std::path::Path::new(r).is_absolute())
        {
            return Err(
                "Use canonical absolute checkout paths, private, or * for all roots.".into(),
            );
        }
        Ok(roots)
    }
    fn save_grant(&mut self, pending: Option<String>, cx: &mut Context<Self>) {
        let roots = match self.roots(cx) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        let scopes = self.scopes;
        if scopes == 0 {
            self.error = Some("Select at least one permission.".into());
            cx.notify();
            return;
        }
        if let Some(id) = pending {
            self.state
                .update(cx, |s, cx| s.resolve_pairing(id, Some((scopes, roots)), cx));
        } else if let Some((id, revision)) = self.editing.take() {
            self.state.update(cx, |s, cx| {
                s.update_device(id, Some((revision, scopes, roots)), cx)
            });
        }
        self.error = None;
        cx.notify();
    }
    fn revoke(&mut self, id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let answer=window.prompt(PromptLevel::Warning,&format!("Revoke access for “{name}”?"),Some("This device loses access to this host. Work already accepted by the host continues."),&[PromptButton::cancel("Cancel"),PromptButton::new("Revoke access")],cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |this, cx| {
                    this.state.update(cx, |s, cx| s.update_device(id, None, cx))
                });
            }
        })
        .detach();
    }
}
impl Render for RemotePreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let busy = state.busy() || state.quitting();
        let running = state.address();
        let enabled = state.preferences().enabled();
        let pending = state.pending();
        let devices = state.devices.clone();
        let error = self.error.clone().or_else(|| state.error.clone());
        let pending_id = pending.as_ref().map(|p| p.id().to_owned());
        let mut content=column().id("remote-preferences").track_scroll(&self.scroll).overflow_y_scrollbar()
            .size_full().p(px(CONTENT_INSET)).gap(px(24.))
            .child(row().gap(px(12.)).child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child("Remote access"))
                .child(button("copy-remote-diagnostics","Copy diagnostics").on_click(|_,_,cx|{
                    if let Ok(json)=canopy_desktop::remote::metrics::snapshot_json(){cx.write_to_clipboard(ClipboardItem::new_string(json));}
                }))
                .child(button("open-remote-client","Connect to a host…").on_click(|_,_,cx|{let _=crate::ui::remote_client::open(cx);})))
            .children(error.map(|e|div().text_color(t::red()).child(e)))
            .child(section("HOST","Available while Canopy is running. Use a loopback listener with SSH, or bind a LAN/VPN interface.",vec![
                form_field("Listener address","Example: 127.0.0.1:44330 for an SSH tunnel. Select 0.0.0.0:44330 to listen on all IPv4 interfaces.",input(&self.address).disabled(busy)).into_any_element(),
                row().gap(px(8.)).child(button("save-listener",if enabled{"Apply listener"}else{"Enable remote access"}).disabled(busy)
                    .on_click(cx.listener(|this,_,_,cx|{let address=this.address.read(cx).value().to_string();this.state.update(cx,|s,cx|s.configure(true,&address,cx));})))
                    .children(enabled.then(||button("stop-listener","Disable remote access").disabled(busy).on_click(cx.listener(|this,_,_,cx|{
                        let address=this.address.read(cx).value().to_string();this.state.update(cx,|s,cx|s.configure(false,&address,cx));}))))
                    .children((running.is_none()&&enabled).then(||button("retry-listener","Retry").disabled(busy).on_click(cx.listener(|this,_,_,cx|this.state.update(cx,|s,cx|s.retry(cx)))))).into_any_element(),
                div().text_color(t::muted()).child(if busy{"Updating remote access…".into()}else{running.map_or_else(||"Listener stopped".into(),|a|format!("Listening on {a}"))}).into_any_element(),
            ]));
        if running.is_some() {
            content=content.child(section("PAIR DEVICE","The invitation expires after five minutes. Approve the device below after it requests access.",vec![
                form_field("Address reached by the client","Use the host LAN/Tailscale IP, or 127.0.0.1 when the client opens an SSH forward.",input(&self.advertised).disabled(busy)).into_any_element(),
                button("new-invitation","Create invitation").disabled(busy).on_click(cx.listener(|this,_,_,cx|{let address=this.advertised.read(cx).value().trim().to_owned();this.state.update(cx,|s,cx|s.pair(address,cx));})).into_any_element(),
            ]));
            if let Some(qr) = self.qr.clone() {
                content = content.child(qr_view(qr));
            }
            if let Some(uri) = self.invite_uri.clone() {
                content = content.child(button("copy-invitation", "Copy invitation").on_click(
                    move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(uri.clone())),
                ));
            }
        }
        if let Some(pending) = &pending {
            let deny = pending.id().to_owned();
            content =
                content.child(
                    column()
                        .gap(px(6.))
                        .child(caption("ACCESS REQUEST"))
                        .child(format!("{} wants to connect", pending.name()))
                        .child(div().font_family(t::MONO).text_size(px(11.)).child(format!(
                                "Key: {}",
                                pending
                                    .csr_fingerprint()
                                    .iter()
                                    .map(|b| format!("{b:02x}"))
                                    .collect::<String>()
                            )))
                        .child(button("deny-pairing", "Deny").disabled(busy).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.state
                                    .update(cx, |s, cx| s.resolve_pairing(deny.clone(), None, cx));
                            }),
                        )),
                );
        }
        if pending.is_some() || self.editing.is_some() {
            content=content.child(column().gap(px(8.)).child(caption("PERMISSIONS"))
                .children(SCOPES.into_iter().map(|(bit,label)|row().gap(px(8.)).child(checkbox(("grant-scope",bit as usize),self.scopes&bit!=0,label).disabled(busy).on_click(cx.listener(move|this,checked,_,cx|{if *checked{this.scopes|=bit;}else{this.scopes&=!bit;}cx.notify();}))).child(label)))
                .child(form_field("Allowed roots","One canonical checkout path per line. private grants private chats and global settings. * grants all present and future roots.",textarea(&self.roots).h(px(100.)).disabled(busy)))
                .child(button("save-grant",if pending.is_some(){"Approve device"}else{"Save permissions"}).disabled(busy).on_click(cx.listener(move|this,_,_,cx|this.save_grant(pending_id.clone(),cx)))));
        }
        content.child(section(
            "DEVICES",
            "Revoking a device closes its streams and prevents further commands.",
            vec![
                button("refresh-devices", "Refresh devices")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |s, cx| s.refresh_devices(cx))
                    }))
                    .into_any_element(),
                column()
                    .children(devices.into_iter().map(|device| {
                        let edit = device.clone();
                        let revoke = device.id().to_owned();
                        let name = device.name().to_owned();
                        column()
                            .gap(px(6.))
                            .py(px(8.))
                            .border_b_1()
                            .border_color(t::border())
                            .child(
                                row()
                                    .gap(px(8.))
                                    .child(div().flex_1().child(device.name().to_owned()))
                                    .children(device.revoked().then(|| badge("Revoked")))
                                    .children((!device.revoked()).then(|| {
                                        row()
                                            .gap(px(8.))
                                            .child(
                                                button(
                                                    SharedString::from(format!(
                                                        "grant-{}",
                                                        device.id()
                                                    )),
                                                    "Edit permissions…",
                                                )
                                                .disabled(busy)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.editing = Some((
                                                            edit.id().into(),
                                                            edit.authorization_revision(),
                                                        ));
                                                        this.scopes = edit.scopes();
                                                        this.roots.update(cx, |s, cx| {
                                                            s.set_value(
                                                                edit.roots().join("\n"),
                                                                window,
                                                                cx,
                                                            )
                                                        });
                                                        cx.notify();
                                                    },
                                                )),
                                            )
                                            .child(
                                                button(
                                                    SharedString::from(format!(
                                                        "revoke-{}",
                                                        device.id()
                                                    )),
                                                    "Revoke access…",
                                                )
                                                .disabled(busy)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.revoke(
                                                            revoke.clone(),
                                                            name.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    },
                                                )),
                                            )
                                    })),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child(device.roots().join(", ")),
                            )
                    }))
                    .into_any_element(),
            ],
        ))
    }
}
const CONTENT_INSET: f32 = 28.;
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
