use crate::app_state::{AppState, IntegrationsState};
use crate::ui::{components::integrations::*, components::*, theme as t};
use canopy_desktop::integrations::{Account, AccountScope, Provider};
use gpui_kit::{
    base::Disableable,
    component::{IconName, input::InputState, scroll::ScrollableElement},
    *,
};

pub struct IntegrationsPreferences {
    state: Entity<IntegrationsState>,
    jira: Entity<super::jira::JiraPreferences>,
    youtrack: Entity<super::youtrack::YoutrackPreferences>,
    scroll: ScrollHandle,
    token: Entity<InputState>,
    owner: Entity<InputState>,
    editing: Option<String>,
    form_open: bool,
    per_owner: bool,
    accounts: Vec<Account>,
    form_error: Option<String>,
    connect_loading: ButtonLoading,
    verifying: std::collections::HashMap<String, ButtonLoading>,
    disconnecting: std::collections::HashMap<String, ButtonLoading>,
    _observer: Subscription,
    _git_observer: Subscription,
}
impl IntegrationsPreferences {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let token = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Paste the token from GitHub")
        });
        let owner =
            cx.new(|cx| InputState::new(window, cx).placeholder("Organization or username"));
        let accounts = state
            .read(cx)
            .config
            .accounts
            .iter()
            .filter(|a| a.provider == Provider::Github)
            .cloned()
            .collect::<Vec<_>>();
        let per_owner = accounts.iter().any(|a| a.scope == AccountScope::Default);
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
            let accounts = &state
                .read(cx)
                .config
                .accounts
                .iter()
                .filter(|a| a.provider == Provider::Github)
                .cloned()
                .collect::<Vec<_>>();
            if this.accounts != *accounts {
                this.accounts = accounts.clone();
                this.reset(window, cx);
            }
            cx.notify();
        });
        Self {
            state,
            jira: cx.new(|cx| super::jira::JiraPreferences::new(window, cx)),
            youtrack: cx.new(|cx| super::youtrack::YoutrackPreferences::new(window, cx)),
            scroll: ScrollHandle::new(),
            token,
            owner,
            editing: None,
            form_open: accounts.is_empty(),
            per_owner,
            accounts,
            form_error: None,
            connect_loading: ButtonLoading::default(),
            verifying: Default::default(),
            disconnecting: Default::default(),
            _observer: observer,
            _git_observer: cx.observe(&cx.global::<AppState>().git.clone(), |_, _, cx| cx.notify()),
        }
    }
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.form_open = self.accounts.is_empty();
        self.per_owner = self
            .accounts
            .iter()
            .any(|a| a.scope == AccountScope::Default);
        self.form_error = None;
        self.token
            .update(cx, |field, cx| field.set_value("", window, cx));
        self.owner
            .update(cx, |field, cx| field.set_value("", window, cx));
        cx.notify();
    }
    fn edit(&mut self, account: &Account, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
        self.form_open = true;
        self.editing = Some(account.id.clone());
        self.per_owner = matches!(account.scope, AccountScope::Owner(_));
        self.form_error = None;
        let owner = match &account.scope {
            AccountScope::Owner(owner) => owner.as_str(),
            AccountScope::Default | AccountScope::Jira { .. } | AccountScope::Youtrack { .. } => "",
        };
        self.owner
            .update(cx, |field, cx| field.set_value(owner, window, cx));
        self.token
            .update(cx, |field, cx| field.set_value("", window, cx));
        cx.notify();
    }
    fn scope(&self, cx: &App) -> Result<AccountScope, String> {
        if self.per_owner {
            AccountScope::owner(&self.owner.read(cx).value())
        } else {
            Ok(AccountScope::Default)
        }
    }
    fn connections(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        column().children(self.accounts.iter().enumerate().map(|(index, account)| {
            let edit = account.clone();
            let verify = account.id.clone();
            let disconnect = account.id.clone();
            let (name, detail) = match &account.scope {
                AccountScope::Default => ("All organizations".to_owned(), "Default connection"),
                AccountScope::Owner(owner) => (owner.clone(), "Organization connection"),
                AccountScope::Jira { site, .. } => (site.clone(), "Jira connection"),
                AccountScope::Youtrack { service } => (service.clone(), "YouTrack connection"),
            };
            row()
                .gap(px(12.))
                .py(px(12.))
                .border_t(px(if index == 0 { 0. } else { 1. }))
                .border_color(t::border())
                .child(
                    icon(if account.scope == AccountScope::Default {
                        IconName::Globe
                    } else {
                        IconName::Building2
                    })
                    .flex_shrink_0(),
                )
                .child(
                    column()
                        .flex_1()
                        .gap(px(3.))
                        .child(div().truncate().text_size(px(13.)).child(name))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(t::muted())
                                .child(format!("@{} · {detail}", account.login)),
                        ),
                )
                .child(
                    row()
                        .flex_shrink_0()
                        .gap(px(6.))
                        .child(
                            loading_icon_button(
                                SharedString::from(format!("verify-{}", account.id)),
                                IconName::RotateCw,
                                "Check token identity",
                                self.verifying.get(&account.id),
                            )
                            .disabled(
                                disabled
                                    && !self
                                        .verifying
                                        .get(&account.id)
                                        .is_some_and(ButtonLoading::active),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.state.update(cx, |state, cx| state.verify(&verify, cx));
                                    this.verifying
                                        .entry(verify.clone())
                                        .or_default()
                                        .set(this.state.read(cx).busy, cx);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            icon_button(
                                SharedString::from(format!("edit-{}", account.id)),
                                IconName::Replace,
                                "Replace token",
                            )
                            .disabled(disabled)
                            .on_click(
                                cx.listener(move |this, _, window, cx| {
                                    this.edit(&edit, window, cx)
                                }),
                            ),
                        )
                        .child(
                            loading_icon_button(
                                SharedString::from(format!("disconnect-{}", account.id)),
                                IconName::Close,
                                "Disconnect organization",
                                self.disconnecting.get(&account.id),
                            )
                            .disabled(
                                disabled
                                    && !self
                                        .disconnecting
                                        .get(&account.id)
                                        .is_some_and(ButtonLoading::active),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.state
                                        .update(cx, |state, cx| state.disconnect(&disconnect, cx));
                                    this.disconnecting
                                        .entry(disconnect.clone())
                                        .or_default()
                                        .set(this.state.read(cx).busy, cx);
                                    cx.notify();
                                },
                            )),
                        ),
                )
        }))
    }
    fn token_form(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        let heading = self
            .editing
            .as_ref()
            .and_then(|id| self.accounts.iter().find(|a| &a.id == id))
            .map(|a| {
                format!(
                    "Replace token · {}",
                    match &a.scope {
                        AccountScope::Default => "Default",
                        AccountScope::Owner(owner) => owner,
                        AccountScope::Jira { site, .. } => site,
                        AccountScope::Youtrack { service } => service,
                    }
                )
            })
            .unwrap_or_else(|| "Add a GitHub connection".into());
        column().gap(px(16.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(heading))
            .child(column().gap(px(8.))
                .child(row().gap(px(2.)).p(px(2.)).rounded(px(6.)).bg(t::hover())
                    .children([(false,"Classic"),(true,"Fine-grained")].into_iter().map(|(per_owner,label)|{
                        selection_button(label,label,self.per_owner==per_owner).flex_1().border_0().disabled(disabled)
                            .text_color(if self.per_owner==per_owner{t::text()}else{t::secondary()})
                            .on_click(cx.listener(move|this,_,window,cx|{
                                if this.per_owner!=per_owner{this.per_owner=per_owner;this.form_error=None;this.token.update(cx,|field,cx|field.set_value("",window,cx));cx.notify();}
                            }))
                    })))
                .child(div().text_size(px(12.)).text_color(t::secondary()).child(if self.per_owner{"A separate token for each organization or personal account."}else{"One token across your personal repositories and organizations."})))
            .children(self.per_owner.then(||form_field("Organization / owner","The owner in github.com/owner/repository.",input(&self.owner).w_full().disabled(disabled))))
            .child(column().gap(px(8.))
                .child(row().gap(px(8.))
                    .child(div().flex_1().text_size(px(12.)).text_color(t::secondary()).child("Personal access token"))
                    .child(button("create-github-token","Create token").h(px(20.)).border_0().bg(rgba(0)).px(px(4.)).icon(icon(IconName::ExternalLink))
                        .disabled(disabled).on_click(cx.listener(|this,_,_,cx|{
                            match this.scope(cx){Ok(scope)=>{this.form_error=None;cx.open_url(&scope.creation_url());},Err(error)=>this.form_error=Some(error)}cx.notify();
                        }))))
                .child(input(&self.token).w_full().disabled(disabled))
                .child(div().text_size(px(11.)).text_color(t::muted()).child(format!("Encrypted in {}. Other connections stay unchanged.", canopy_desktop::platform::credentials::store_name()))))
            .child(column().gap(px(8.))
                .child(row().gap(px(6.)).flex_wrap().child(div().text_size(px(11.)).text_color(t::muted()).child("Permissions"))
                    .children(if self.per_owner{vec![badge("Issues: write"),badge("Metadata: read")]}else{vec![badge("repo")]}))
                .child(div().text_size(px(11.)).line_height(px(16.)).text_color(t::muted()).child(if self.per_owner{
                    "Create token preselects Issues: read and write. Existing read-only tokens can browse; replace them to edit. Organization approval may be required."
                }else{
                    "Create token preselects repo, which also grants write access. Canopy can read and update issues. Authorize SSO for each organization if required."
                })))
            .children(self.form_error.clone().map(|error|integration_message(error,true)))
            .child(row().gap(px(8.)).justify_end()
                .children((!self.accounts.is_empty()).then(||button("cancel-github-edit","Cancel").disabled(disabled)
                    .on_click(cx.listener(|this,_,window,cx|this.reset(window,cx)))))
                .child(primary_loading_button("connect-github",if self.editing.is_some(){"Replace token"}else{"Connect GitHub"}, &self.connect_loading)
                    .disabled(disabled && !self.connect_loading.active()).on_click(cx.listener(|this,_,_,cx|{
                        match this.scope(cx){Ok(scope)=>{this.form_error=None;let token=this.token.read(cx).value().trim().to_owned();this.state.update(cx,|state,cx|state.connect(this.editing.clone(),scope,token,cx));this.connect_loading.set(this.state.read(cx).busy,cx);},Err(error)=>this.form_error=Some(error)}cx.notify();
                    }))))
    }
}
impl Render for IntegrationsPreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let disabled = state.busy || !state.ready || cx.global::<AppState>().git.read(cx).busy;
        let status = state.connection.clone();
        let error = state.error.clone();
        let cleanup = !state.config.retired_credentials.is_empty();
        let content=column().flex_shrink_0().w_full().p(px(28.)).gap(px(24.))
            .child(column().gap(px(6.))
                .child(div().text_size(px(18.)).font_weight(FontWeight::MEDIUM).child("Integrations"))
                .child(div().text_size(px(12.)).text_color(t::secondary()).child("Bring your team's issues into Canopy.")))
            .child(column().gap(px(16.))
                .child(row().gap(px(12.))
                    .child(github_heading("GitHub","Issues for your projects and worktrees.").flex_1())
                    .child(badge("Issues").flex_shrink_0()))
                .children((!self.accounts.is_empty()).then(||self.connections(disabled,cx)))
                .children((!self.form_open).then(||row().child(button("add-github-connection","Add connection").icon(icon(IconName::Plus)).on_click(cx.listener(|this,_,window,cx|{
                    this.reset(window,cx);this.form_open=true;this.scroll.scroll_to_bottom();cx.notify();
                })).disabled(disabled))))
                .children(self.form_open.then(||div().pt(px(16.)).border_t_1().border_color(t::border()).child(self.token_form(disabled,cx)))))
            .child(div().pt(px(24.)).border_t_1().border_color(t::border()).child(self.jira.clone()))
            .child(div().pt(px(24.)).border_t_1().border_color(t::border()).child(self.youtrack.clone()))
            .children(error.clone().map(|error|integration_message(error,true)))
            .children((error.is_none()&&status!="Not connected").then(||div().text_size(px(11.)).text_color(t::secondary()).child(status)))
            .children(cleanup.then(||button("cleanup-integration-credentials","Retry credential cleanup").disabled(disabled)
                .on_click(cx.listener(|this,_,_,cx|this.state.update(cx,|state,cx|state.cleanup_credentials(cx))))))
            .child(row().items_start().gap(px(8.)).pt(px(16.)).border_t_1().border_color(t::border())
                .child(custom_icon("git-branch").flex_shrink_0())
                .child(div().flex_1().min_w_0().text_size(px(11.)).line_height(px(16.)).text_color(t::muted()).child("Repositories are matched from origin. Organization connections take priority over the default token. Choose another repository in Tasks when needed.")));
        div()
            .relative()
            .size_full()
            .child(
                column()
                    .id("integrations-preferences")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(content),
            )
            .vertical_scrollbar(&self.scroll)
    }
}
