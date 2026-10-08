use crate::{
    app_state::{AppState, IntegrationsState},
    ui::{components::integrations::*, components::*, theme as t},
};
use canopy_desktop::integrations::{Account, AccountScope, Provider, youtrack};
use gpui_kit::{
    base::Disableable,
    component::{IconName, input::InputState},
    *,
};

pub struct YoutrackPreferences {
    state: Entity<IntegrationsState>,
    service: Entity<InputState>,
    token: Entity<InputState>,
    open: bool,
    editing: Option<String>,
    accounts: Vec<Account>,
    error: Option<String>,
    connect_loading: ButtonLoading,
    verifying: std::collections::HashMap<String, ButtonLoading>,
    disconnecting: std::collections::HashMap<String, ButtonLoading>,
    _observer: Subscription,
}

impl YoutrackPreferences {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let observer = cx.observe_in(&state, window, |this, state, window, cx| {
            if !state.read(cx).busy {
                this.connect_loading.set(false, cx);
                for feedback in this
                    .verifying
                    .values_mut()
                    .chain(this.disconnecting.values_mut())
                {
                    feedback.set(false, cx);
                }
                this.verifying
                    .retain(|id, _| state.read(cx).config.accounts.iter().any(|a| a.id == *id));
                this.disconnecting
                    .retain(|id, _| state.read(cx).config.accounts.iter().any(|a| a.id == *id));
            }
            let accounts = state
                .read(cx)
                .config
                .accounts
                .iter()
                .filter(|a| a.provider == Provider::Youtrack)
                .cloned()
                .collect::<Vec<_>>();
            if accounts != this.accounts {
                this.accounts = accounts;
                this.reset(window, cx);
                this.open = false;
            }
            cx.notify();
        });
        Self {
            accounts: state
                .read(cx)
                .config
                .accounts
                .iter()
                .filter(|a| a.provider == Provider::Youtrack)
                .cloned()
                .collect(),
            state,
            service: cx.new(|cx| {
                InputState::new(window, cx).placeholder("https://issues.example.com/youtrack")
            }),
            token: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("Permanent token")
            }),
            open: false,
            editing: None,
            error: None,
            connect_loading: ButtonLoading::default(),
            verifying: Default::default(),
            disconnecting: Default::default(),
            _observer: observer,
        }
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.error = None;
        self.service.update(cx, |s, cx| s.set_value("", window, cx));
        self.token.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    fn edit(&mut self, account: &Account, window: &mut Window, cx: &mut Context<Self>) {
        self.reset(window, cx);
        self.open = true;
        self.editing = Some(account.id.clone());
        if let AccountScope::Youtrack { service } = &account.scope {
            self.service
                .update(cx, |s, cx| s.set_value(service.clone(), window, cx));
        }
        cx.notify();
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            let service = youtrack::normalize_service(&self.service.read(cx).value())?;
            youtrack::validate_connection(&service)?;
            Ok::<_, String>(AccountScope::Youtrack { service })
        })();
        match result {
            Ok(scope) => {
                self.error = None;
                let token = self.token.read(cx).value().trim().to_owned();
                self.state.update(cx, |s, cx| {
                    s.connect(self.editing.clone(), scope, token, cx)
                });
                self.connect_loading.set(self.state.read(cx).busy, cx);
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn connections(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        column().children(self.accounts.iter().map(|account| {
            let edit = account.clone();
            let verify = account.id.clone();
            let disconnect = account.id.clone();
            let service = match &account.scope {
                AccountScope::Youtrack { service } => service.clone(),
                _ => String::new(),
            };
            row()
                .gap(px(8.))
                .py(px(8.))
                .child(
                    column()
                        .flex_1()
                        .gap(px(3.))
                        .child(div().truncate().child(service))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(t::muted())
                                .child(format!("@{} · YouTrack connection", account.login)),
                        ),
                )
                .child(
                    loading_icon_button(
                        SharedString::from(format!("youtrack-verify-{verify}")),
                        IconName::RotateCw,
                        "Check connection",
                        self.verifying.get(&verify),
                    )
                    .disabled(
                        disabled
                            && !self
                                .verifying
                                .get(&verify)
                                .is_some_and(ButtonLoading::active),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| s.verify(&verify, cx));
                        this.verifying
                            .entry(verify.clone())
                            .or_default()
                            .set(this.state.read(cx).busy, cx);
                        cx.notify();
                    })),
                )
                .child(
                    icon_button(
                        SharedString::from(format!("youtrack-edit-{}", account.id)),
                        IconName::Settings,
                        "Edit connection",
                    )
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, window, cx| this.edit(&edit, window, cx))),
                )
                .child(
                    loading_icon_button(
                        SharedString::from(format!("youtrack-remove-{disconnect}")),
                        IconName::Close,
                        "Disconnect YouTrack",
                        self.disconnecting.get(&disconnect),
                    )
                    .disabled(
                        disabled
                            && !self
                                .disconnecting
                                .get(&disconnect)
                                .is_some_and(ButtonLoading::active),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| s.disconnect(&disconnect, cx));
                        this.disconnecting
                            .entry(disconnect.clone())
                            .or_default()
                            .set(this.state.read(cx).busy, cx);
                        cx.notify();
                    })),
                )
        }))
    }
}

impl Render for YoutrackPreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let disabled =
            !state.ready || state.busy || cx.global::<AppState>().settings.read(cx).quitting;
        column()
            .gap(px(16.))
            .child(provider_heading(
                Provider::Youtrack,
                "YouTrack",
                "Projects, issues and conversations from a YouTrack service.",
            ))
            .children((!self.accounts.is_empty()).then(|| self.connections(disabled, cx)))
            .children((!self.open).then(|| {
                row().child(
                    button("add-youtrack", "Connect YouTrack")
                        .icon(icon(IconName::Plus))
                        .disabled(disabled)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.reset(window, cx);
                            this.open = true;
                            cx.notify();
                        })),
                )
            }))
            .children(self.open.then(|| {
                column()
                    .gap(px(16.))
                    .child(form_field(
                        "Service URL",
                        "Use the HTTPS address of your YouTrack service, including a context path such as /youtrack.",
                        input(&self.service).w_full().disabled(disabled),
                    ))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(div().flex_1().text_color(t::secondary()).child("Permanent token"))
                            .child(
                                button("youtrack-token-help", "Token help")
                                    .icon(icon(IconName::ExternalLink))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let service = this.service.read(cx).value().to_string();
                                        match youtrack::normalize_service(&service) {
                                            Ok(_) => cx.open_url(&AccountScope::Youtrack { service }.creation_url()),
                                            Err(error) => this.error = Some(error),
                                        }
                                        cx.notify();
                                    }))
                                    .disabled(disabled),
                            ),
                    )
                    .child(input(&self.token).w_full().disabled(disabled))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child(format!("Stored in {}. The token is sent only to this configured service.", canopy_desktop::platform::credentials::store_name())),
                    )
                    .children(self.error.clone().map(|error| integration_message(error, true)))
                    .child(
                        row()
                            .gap(px(8.))
                            .justify_end()
                            .child(
                                button("youtrack-cancel", "Cancel")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.reset(window, cx);
                                        this.open = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                primary_loading_button(
                                    "youtrack-connect",
                                    if self.editing.is_some() {
                                        "Replace token"
                                    } else {
                                        "Verify and connect"
                                    },
                                    &self.connect_loading,
                                )
                                .disabled(disabled && !self.connect_loading.active())
                                .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                            ),
                    )
            }))
    }
}
