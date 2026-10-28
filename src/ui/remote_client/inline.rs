//! Connection setup embedded in Settings → Connections: Add computer and one
//! computer's routes. The owner is a `RemoteWindow` in inline mode, so pairing,
//! SSH key checks, route editing and their errors behave as in the device window.
use super::*;
use chrome::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Inline {
    AddComputer,
    Routes,
}
/// Result of an inline flow; the Settings owner removes the view on either event.
#[derive(Clone, Copy)]
pub enum ConnectionSetupEvent {
    /// The other computer approved this device; it is saved and connecting.
    Paired,
    /// The person closed the flow. A pairing request still waiting is cancelled.
    Closed,
}
impl EventEmitter<ConnectionSetupEvent> for RemoteWindow {}

pub struct ConnectionSetup {
    pub(super) view: Entity<RemoteWindow>,
    _events: Subscription,
}
impl EventEmitter<ConnectionSetupEvent> for ConnectionSetup {}
impl ConnectionSetup {
    /// Pairing form, optionally prefilled with an invitation.
    pub fn add_computer(
        invitation: Option<String>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<Self>> {
        ensure_global(cx)?;
        Ok(cx.new(|cx| {
            Self::wrap(
                cx.new(|cx| {
                    let mut view = RemoteWindow::new(window, cx);
                    view.embedded = true;
                    view.inline = Some(Inline::AddComputer);
                    view.opening_setup = true;
                    if let Some(invitation) = invitation {
                        view.fields["pair"]
                            .update(cx, |field, cx| field.set_value(invitation, window, cx));
                    }
                    view
                }),
                cx,
            )
        }))
    }
    /// Device-local routes and name of one paired computer.
    pub fn routes(
        host: Arc<HostClient>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<Self>> {
        ensure_global(cx)?;
        Ok(cx.new(|cx| {
            Self::wrap(
                cx.new(|cx| {
                    let mut view = RemoteWindow::new(window, cx);
                    view.embedded = true;
                    view.inline = Some(Inline::Routes);
                    view.routes_on_open = true;
                    view.fields["host_label"]
                        .update(cx, |field, cx| field.set_value(host.name(), window, cx));
                    view.host = Some(host);
                    view
                }),
                cx,
            )
        }))
    }
    fn wrap(view: Entity<RemoteWindow>, cx: &mut Context<Self>) -> Self {
        let events = cx.subscribe(&view, |_, _, event: &ConnectionSetupEvent, cx| {
            cx.emit(*event)
        });
        Self {
            view,
            _events: events,
        }
    }
}
impl Render for ConnectionSetup {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

impl RemoteWindow {
    pub(super) fn render_inline(&mut self, inline: Inline, cx: &mut Context<Self>) -> AnyElement {
        let content = match inline {
            Inline::AddComputer => self.render_pairing(cx),
            Inline::Routes => self.render_inline_routes(cx),
        };
        column()
            .track_focus(&self.focus)
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .text_size(px(13.))
            .text_color(t::text())
            .child(content)
            .children(self.confirmation.clone())
            .children(self.prompt.clone())
            .into_any_element()
    }
    /// Closes the inline flow; dropping the owner cancels a waiting pairing request.
    pub(super) fn close_inline(&mut self, cx: &mut Context<Self>) {
        self.cancelled.store(true, Ordering::Release);
        cx.emit(ConnectionSetupEvent::Closed);
    }
    fn render_inline_routes(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.host.is_none() {
            return loading("Loading routes…").into_any_element();
        }
        // `run` results without a control of their own, e.g. “Route order saved.”
        let info =
            (self.scope.is_none() && self.error.is_none() && !self.info.is_empty()).then(|| {
                if self.busy {
                    loading(self.info.clone())
                } else {
                    done(self.info.clone())
                }
            });
        column()
            .flex_shrink_0()
            .gap(px(20.))
            .child(self.render_routes(cx))
            .children(info)
            .child(self.host_label_form(cx))
            .child(
                row().justify_end().child(
                    button("close-computer-routes", "Done")
                        .on_click(cx.listener(|this, _, _, cx| this.close_inline(cx))),
                ),
            )
            .into_any_element()
    }
}
