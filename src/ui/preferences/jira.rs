use crate::{
    app_state::{AppState, IntegrationsState},
    ui::{components::integrations::*, components::*, theme as t},
};
use canopy_desktop::integrations::{Account, AccountScope, Provider, jira};
use gpui_kit::{
    base::Disableable,
    component::{IconName, input::InputState},
    *,
};
pub struct JiraPreferences {
    state: Entity<IntegrationsState>,
    site: Entity<InputState>,
    email: Entity<InputState>,
    cloud: Entity<InputState>,
    token: Entity<InputState>,
    scoped: bool,
    open: bool,
    editing: Option<String>,
    accounts: Vec<Account>,
    error: Option<String>,
    connect_loading: ButtonLoading,
    verifying: std::collections::HashMap<String, ButtonLoading>,
    disconnecting: std::collections::HashMap<String, ButtonLoading>,
    _observer: Subscription,
}
impl JiraPreferences {
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
                .filter(|a| a.provider == Provider::Jira)
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
                .filter(|a| a.provider == Provider::Jira)
                .cloned()
                .collect(),
            state,
            site: cx
                .new(|cx| InputState::new(window, cx).placeholder("https://team.atlassian.net")),
            email: cx.new(|cx| InputState::new(window, cx).placeholder("you@company.com")),
            cloud: cx.new(|cx| InputState::new(window, cx).placeholder("Cloud ID")),
            token: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("Atlassian API token")
            }),
            scoped: false,
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
        self.scoped = false;
        for input in [&self.site, &self.email, &self.cloud, &self.token] {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.notify();
    }
    fn edit(&mut self, a: &Account, window: &mut Window, cx: &mut Context<Self>) {
        self.reset(window, cx);
        self.editing = Some(a.id.clone());
        self.open = true;
        if let AccountScope::Jira {
            site,
            email,
            cloud_id,
        } = &a.scope
        {
            self.site
                .update(cx, |s, cx| s.set_value(site.clone(), window, cx));
            self.email
                .update(cx, |s, cx| s.set_value(email.clone(), window, cx));
            self.cloud.update(cx, |s, cx| {
                s.set_value(cloud_id.clone().unwrap_or_default(), window, cx)
            });
            self.scoped = cloud_id.is_some();
        }
        cx.notify();
    }
    fn connect(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            let site = jira::normalize_site(&self.site.read(cx).value())?;
            let email = self.email.read(cx).value().trim().to_owned();
            let cloud_id = self
                .scoped
                .then(|| self.cloud.read(cx).value().trim().to_owned());
            jira::validate_connection(&site, &email, cloud_id.as_deref())?;
            Ok::<_, String>(AccountScope::Jira {
                site,
                email,
                cloud_id,
            })
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
            Err(e) => self.error = Some(e),
        }
        cx.notify();
    }
}
impl Render for JiraPreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.state.read(cx).ready
            || self.state.read(cx).busy
            || cx.global::<AppState>().settings.read(cx).quitting;
        column().gap(px(16.)).child(provider_heading(Provider::Jira,"Jira","Projects, workflow and conversations in one place."))
            .children(self.accounts.iter().map(|a|{
                let edit=a.clone();let verify=a.id.clone();let remove=a.id.clone();
                let (site,email)=match &a.scope {AccountScope::Jira {site,email,..}=>(site.clone(),email.clone()),_=>unreachable!()};
                row().gap(px(8.)).py(px(8.)).child(column().flex_1().gap(px(3.)).child(div().truncate().child(site)).child(div().truncate().text_size(px(11.)).text_color(t::muted()).child(email)))
                    .child(loading_icon_button(SharedString::from(format!("jira-verify-{verify}")),IconName::RotateCw,"Check connection",self.verifying.get(&verify)).disabled(disabled && !self.verifying.get(&verify).is_some_and(ButtonLoading::active)).on_click(cx.listener(move|this,_,_,cx|{this.state.update(cx,|s,cx|s.verify(&verify,cx));this.verifying.entry(verify.clone()).or_default().set(this.state.read(cx).busy,cx);cx.notify();})))
                    .child(icon_button(SharedString::from(format!("jira-edit-{}",a.id)),IconName::Settings,"Edit connection").disabled(disabled).on_click(cx.listener(move|this,_,w,cx|this.edit(&edit,w,cx))))
                    .child(loading_icon_button(SharedString::from(format!("jira-remove-{remove}")),IconName::Close,"Disconnect Jira",self.disconnecting.get(&remove)).disabled(disabled && !self.disconnecting.get(&remove).is_some_and(ButtonLoading::active)).on_click(cx.listener(move|this,_,_,cx|{this.state.update(cx,|s,cx|s.disconnect(&remove,cx));this.disconnecting.entry(remove.clone()).or_default().set(this.state.read(cx).busy,cx);cx.notify();})))
            }))
            .children((!self.open).then(||row().child(button("add-jira","Connect Jira").icon(icon(IconName::Plus)).disabled(disabled).on_click(cx.listener(|this,_,w,cx|{this.reset(w,cx);this.open=true;cx.notify();})))))
            .children(self.open.then(||column().gap(px(16.))
                .child(form_field("Jira site","The root address of your Jira Cloud instance.",input(&self.site).w_full().disabled(disabled)))
                .child(form_field("Email","Your Atlassian account email.",input(&self.email).w_full().disabled(disabled)))
                .child(row().gap(px(4.)).children([(false,"Token without scopes"),(true,"Token with scopes")].into_iter().map(|(value,label)|selection_button(label,label,self.scoped==value).disabled(disabled).on_click(cx.listener(move|this,_,_,cx|{this.scoped=value;cx.notify();})))))
                .children(self.scoped.then(||form_field("Cloud ID","Find this site's ID in Atlassian administration. Scoped tokens use the Atlassian API gateway.",input(&self.cloud).w_full().disabled(disabled))))
                .child(row().gap(px(8.)).child(div().flex_1().text_color(t::secondary()).child("API token")).child(button("jira-create-token","Create token").icon(icon(IconName::ExternalLink)).on_click(|_,_,cx|cx.open_url("https://id.atlassian.com/manage-profile/security/api-tokens"))))
                .child(input(&self.token).w_full().disabled(disabled))
                .child(div().text_size(px(11.)).text_color(t::muted()).child(format!("Stored in {}. For scoped tokens: read:jira-work, write:jira-work, read:jira-user. Sprint operations also need Jira Software board/sprint scopes. Your project permissions still apply.", canopy_desktop::platform::credentials::store_name())))
                .children(self.error.clone().map(|e|integration_message(e,true)))
                .child(row().gap(px(8.)).justify_end().child(button("jira-cancel","Cancel").disabled(disabled).on_click(cx.listener(|this,_,w,cx|{this.reset(w,cx);this.open=false;cx.notify();})))
                    .child(primary_loading_button("jira-connect","Verify and connect",&self.connect_loading).disabled(disabled && !self.connect_loading.active()).on_click(cx.listener(|this,_,_,cx|this.connect(cx)))))))
    }
}
