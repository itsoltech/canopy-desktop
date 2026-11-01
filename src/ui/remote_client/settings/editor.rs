use super::{controls::*, *};
use gpui_kit::component::{
    IndexPath,
    checkbox::Checkbox,
    select::{SelectEvent, SelectState},
};

#[derive(Clone)]
pub(super) enum Draft {
    Preferences(p::HostPreferences),
    Agent(p::AgentDefinition),
    Profile(p::Profile, bool),
    Integration(p::Integration),
}
#[expect(
    clippy::large_enum_variant,
    reason = "One settings save carries its owned profile request directly into the pending RPC; this is not an accumulated collection."
)]
pub(super) enum Write {
    Preferences(p::UpdatePreferencesRequest),
    Agent(p::SaveAgentDefinitionRequest),
    Profile(p::SaveProfileRequest),
    Integration(p::SaveIntegrationRequest),
}
impl Write {
    pub(super) async fn send(
        self,
        host: Arc<HostClient>,
        revision: u64,
    ) -> Result<p::CommandReceipt, String> {
        if let Self::Preferences(request) = self {
            return host.update_preferences(request, revision).await;
        }
        host.mutate(Some(revision), move |channel, command| async move {
            let mut client = p::settings_service_client::SettingsServiceClient::new(channel);
            match self {
                Self::Preferences(_) => unreachable!("preferences use the typed command client"),
                Self::Agent(mut r) => {
                    r.command = Some(command);
                    client.save_agent_definition(r).await
                }
                Self::Profile(mut r) => {
                    r.command = Some(command);
                    client.save_profile(r).await
                }
                Self::Integration(mut r) => {
                    r.command = Some(command);
                    client.save_integration(r).await
                }
            }
        })
        .await
    }
}
/// One selected record owns its inputs and write-only patches. Switching records
/// is explicit; refreshing metadata never overwrites an in-progress editor.
pub(super) struct Editor {
    draft: Draft,
    fields: BTreeMap<&'static str, Entity<InputState>>,
    labels: Vec<(&'static str, &'static str)>,
    flags: BTreeMap<&'static str, bool>,
    backend: usize,
    kind: usize,
    credential: Option<Entity<SecretField>>,
    prompt: Option<Entity<SecretField>>,
    settings: Option<Entity<SecretField>>,
    environment: Vec<(String, Entity<SecretField>)>,
    new_environment: Entity<InputState>,
    error: Option<String>,
    enabled: bool,
    selects: BTreeMap<&'static str, Entity<SelectState<Vec<SelectOption>>>>,
    subscriptions: Vec<Subscription>,
    catalog: p::HostSettings,
}
impl Editor {
    pub(super) fn new(
        draft: Draft,
        catalog: p::HostSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            draft: draft.clone(),
            fields: BTreeMap::new(),
            labels: vec![],
            flags: BTreeMap::new(),
            backend: 0,
            kind: 0,
            credential: None,
            prompt: None,
            settings: None,
            environment: vec![],
            new_environment: text("", false, window, cx),
            error: None,
            enabled: true,
            selects: BTreeMap::new(),
            subscriptions: vec![],
            catalog,
        };
        let mut env = vec![];
        match draft {
            Draft::Preferences(p) => {
                this.flags
                    .insert("Reopen last workspace", p.reopen_last_workspace);
                this.flags.insert("Notch", p.notch_enabled);
                for (key, label, val) in [
                    ("new_tab", "New tab tool", p.new_tab_tool),
                    ("new_worktree", "New worktree tool", p.new_worktree_tool),
                    (
                        "chat_profile",
                        "Chat profile",
                        p.chat_profile.unwrap_or_default(),
                    ),
                    ("chat_model", "Chat model", p.chat_model.unwrap_or_default()),
                    (
                        "chat_effort",
                        "Chat effort",
                        p.chat_effort.unwrap_or_default(),
                    ),
                    ("chat_mode", "Chat mode", p.chat_mode.unwrap_or_default()),
                ] {
                    this.add(key, label, val, window, cx);
                }
            }
            Draft::Agent(a) => {
                this.add("name", "Name", a.name, window, cx);
                this.add("command", "Command", a.command, window, cx);
                this.add(
                    "arguments",
                    "Arguments (shell quoted)",
                    shell_words::join(a.arguments),
                    window,
                    cx,
                );
                this.add(
                    "default_profile",
                    "Default profile",
                    a.default_profile_id.unwrap_or_default(),
                    window,
                    cx,
                );
                this.flags.insert("Enabled", a.enabled.unwrap_or(true));
                this.backend = a.backend_kind.saturating_sub(1) as usize;
                env = a.environment;
            }
            Draft::Profile(p, acp) => {
                this.add("name", "Name", p.name, window, cx);
                this.add("agent", "Agent ID", p.agent_id, window, cx);
                this.add("model", "Model", p.model.unwrap_or_default(), window, cx);
                this.add(
                    "arguments",
                    "Arguments (shell quoted)",
                    shell_words::join(p.arguments.unwrap_or_default().values),
                    window,
                    cx,
                );
                let s = p.settings.unwrap_or_default();
                for (key, label, v) in [
                    ("permission_mode", "Permission mode", s.permission_mode),
                    ("effort_level", "Effort", s.effort_level),
                    ("base_url", "API base URL", s.base_url),
                    ("provider", "Provider", s.provider),
                    ("approval_mode", "Approval mode", s.approval_mode),
                    ("sandbox", "Sandbox", s.sandbox),
                    ("config_profile", "Configuration profile", s.config_profile),
                ] {
                    this.add(key, label, v.unwrap_or_default(), window, cx);
                }
                this.flags.insert("Full auto", s.full_auto.unwrap_or(false));
                this.flags
                    .insert("Bypass approvals", s.bypass_approvals.unwrap_or(false));
                let reason = acp.then(|| "ACP does not support native API credentials, prompt or JSON overrides. Configure named environment variables and arguments instead.".into());
                this.credential = Some(cx.new(|cx| {
                    SecretField::new(
                        "API key",
                        p.credential_configured,
                        reason.clone(),
                        window,
                        cx,
                    )
                }));
                this.prompt = Some(cx.new(|cx| {
                    SecretField::new(
                        "Append system prompt",
                        s.system_prompt_configured,
                        reason.clone(),
                        window,
                        cx,
                    )
                }));
                this.settings = Some(cx.new(|cx| {
                    SecretField::new(
                        "Settings JSON override",
                        s.settings_override_configured,
                        reason,
                        window,
                        cx,
                    )
                }));
                env = p.environment;
            }
            Draft::Integration(i) => {
                this.kind = match i.kind.as_str() {
                    "jira" => 1,
                    "youtrack" => 2,
                    _ => 0,
                };
                for (key, label, v) in [
                    ("name", "Name", i.name),
                    ("endpoint", "Endpoint", i.endpoint),
                    (
                        "owner",
                        "GitHub owner / organization",
                        i.owner.unwrap_or_default(),
                    ),
                    ("email", "Jira email", i.email.unwrap_or_default()),
                    ("cloud_id", "Jira Cloud ID", i.cloud_id.unwrap_or_default()),
                ] {
                    this.add(key, label, v, window, cx);
                }
                this.credential = Some(cx.new(|cx| {
                    SecretField::new("Credential", i.credential_configured, None, window, cx)
                }));
            }
        }
        for e in env {
            let field = cx
                .new(|cx| SecretField::new(&e.name, e.configured, None, window, cx).allow_empty());
            this.environment.push((e.name, field));
        }
        let agents = this
            .catalog
            .agents
            .iter()
            .map(|a| SelectOption::new(a.id.clone(), a.name.clone()))
            .collect();
        if this.fields.contains_key("agent") {
            this.add_select("agent", agents, window, cx);
        }
        let profile_agent = match &this.draft {
            Draft::Agent(a) => Some(a.id.clone()),
            _ => None,
        };
        for key in ["default_profile", "chat_profile"] {
            if this.fields.contains_key(key) {
                let mut options = vec![SelectOption::new("", "Default")];
                options.extend(
                    this.catalog
                        .profiles
                        .iter()
                        .filter(|p| profile_agent.as_ref().is_none_or(|id| id == &p.agent_id))
                        .map(|p| SelectOption::new(p.id.clone(), p.name.clone())),
                );
                this.add_select(key, options, window, cx);
            }
        }
        for (key, pairs) in [
            (
                "permission_mode",
                vec![
                    ("", "Default"),
                    ("plan", "Plan"),
                    ("auto", "Auto"),
                    ("acceptEdits", "Accept edits"),
                    ("bypassPermissions", "Bypass permissions"),
                ],
            ),
            (
                "effort_level",
                vec![
                    ("", "Default"),
                    ("low", "Low"),
                    ("medium", "Medium"),
                    ("high", "High"),
                    ("xhigh", "Extra high"),
                    ("max", "Max"),
                ],
            ),
            (
                "provider",
                vec![
                    ("", "Default"),
                    ("bedrock", "AWS Bedrock"),
                    ("vertex", "Google Vertex AI"),
                    ("foundry", "Microsoft Foundry"),
                ],
            ),
            (
                "approval_mode",
                vec![
                    ("", "Default"),
                    ("on-request", "On request"),
                    ("never", "Never"),
                ],
            ),
            (
                "sandbox",
                vec![
                    ("", "Default"),
                    ("read-only", "Read only"),
                    ("workspace-write", "Workspace write"),
                    ("danger-full-access", "Full access"),
                ],
            ),
        ] {
            if this.fields.contains_key(key) {
                this.add_select(
                    key,
                    pairs
                        .into_iter()
                        .map(|(v, l)| SelectOption::new(v, l))
                        .collect(),
                    window,
                    cx,
                );
            }
        }
        this
    }
    fn add_select(
        &mut self,
        key: &'static str,
        mut options: Vec<SelectOption>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.get(key, cx);
        if !options.iter().any(|v| v.value.as_ref() == value) {
            options.push(SelectOption::new(value.clone(), value.clone()));
        }
        let selected = options
            .iter()
            .position(|v| v.value.as_ref() == value)
            .map(IndexPath::new);
        let select = cx.new(|cx| SelectState::new(options, selected, window, cx));
        self.subscriptions.push(cx.subscribe_in(&select,window,move|s,_,event:&SelectEvent<Vec<SelectOption>>,w,cx|{
            if let SelectEvent::Confirm(Some(value))=event{
                s.fields[key].update(cx,|v,cx|v.set_value(value.clone(),w,cx));
                if key=="agent"{let acp=s.catalog.agents.iter().find(|a|a.id==value.as_ref()).is_some_and(|a|a.backend_kind==3);if let Draft::Profile(_,is_acp)=&mut s.draft{*is_acp=acp;}
                    for f in [&s.credential,&s.prompt,&s.settings].into_iter().flatten(){f.update(cx,|f,cx|f.set_unsupported(acp.then(||"ACP uses named environment variables and arguments; native credentials, prompt and JSON overrides are unsupported.".into()),cx));}
                }cx.notify();
            }
        }));
        self.selects.insert(key, select);
    }
    fn add(
        &mut self,
        key: &'static str,
        label: &'static str,
        v: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fields.insert(key, text(v, false, window, cx));
        self.labels.push((key, label));
    }
    fn get(&self, key: &str, cx: &App) -> String {
        value(&self.fields[key], cx)
    }
    pub(super) fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        for field in [&self.credential, &self.prompt, &self.settings]
            .into_iter()
            .flatten()
            .chain(self.environment.iter().map(|(_, f)| f))
        {
            field.update(cx, |f, cx| f.enable(enabled, cx));
        }
        cx.notify();
    }
    pub(super) fn title(&self) -> &'static str {
        let new = match &self.draft {
            Draft::Preferences(_) => false,
            Draft::Agent(a) => !self.catalog.agents.iter().any(|v| v.id == a.id),
            Draft::Profile(p, _) => !self.catalog.profiles.iter().any(|v| v.id == p.id),
            Draft::Integration(i) => !self.catalog.integrations.iter().any(|v| v.id == i.id),
        };
        match (&self.draft, new) {
            (Draft::Preferences(_), _) => "Defaults on this computer",
            (Draft::Agent(_), true) => "New agent",
            (Draft::Agent(_), false) => "Edit agent",
            (Draft::Profile(_, _), true) => "New profile",
            (Draft::Profile(_, _), false) => "Edit profile",
            (Draft::Integration(_), true) => "Connect integration",
            (Draft::Integration(_), false) => "Edit integration",
        }
    }
    pub(super) fn write(&self, cx: &App) -> Result<Write, String> {
        let env = self
            .environment
            .iter()
            .map(|(name, field)| {
                Ok(p::EnvironmentUpdate {
                    name: name.clone(),
                    value: Some(field.read(cx).update(cx)?),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let secret =
            |f: &Option<Entity<SecretField>>| f.as_ref().map(|f| f.read(cx).update(cx)).transpose();
        Ok(match &self.draft {
            Draft::Preferences(base) => {
                let changed = |key: &str, old: &str| {
                    let v = self.get(key, cx);
                    (v != old).then_some(v)
                };
                Write::Preferences(p::UpdatePreferencesRequest {
                    reopen_last_workspace: (self.flags["Reopen last workspace"]
                        != base.reopen_last_workspace)
                        .then_some(self.flags["Reopen last workspace"]),
                    notch_enabled: (self.flags["Notch"] != base.notch_enabled)
                        .then_some(self.flags["Notch"]),
                    new_tab_tool: changed("new_tab", &base.new_tab_tool),
                    new_worktree_tool: changed("new_worktree", &base.new_worktree_tool),
                    chat_profile: changed(
                        "chat_profile",
                        base.chat_profile.as_deref().unwrap_or_default(),
                    ),
                    chat_model: changed(
                        "chat_model",
                        base.chat_model.as_deref().unwrap_or_default(),
                    ),
                    chat_effort: changed(
                        "chat_effort",
                        base.chat_effort.as_deref().unwrap_or_default(),
                    ),
                    chat_mode: changed("chat_mode", base.chat_mode.as_deref().unwrap_or_default()),
                    ..Default::default()
                })
            }
            Draft::Agent(base) => {
                let mut agent = base.clone();
                agent.name = self.get("name", cx);
                agent.command = self.get("command", cx);
                agent.arguments =
                    shell_words::split(&self.get("arguments", cx)).map_err(|e| e.to_string())?;
                agent.backend_kind = self.backend as i32 + 1;
                agent.enabled = Some(self.flags["Enabled"]);
                agent.default_profile_id = Some(self.get("default_profile", cx));
                if agent.name.trim().is_empty() || agent.command.trim().is_empty() {
                    return Err("Agent name and command are required.".into());
                }
                Write::Agent(p::SaveAgentDefinitionRequest {
                    agent: Some(agent),
                    environment: env,
                    ..Default::default()
                })
            }
            Draft::Profile(base, acp) => {
                let mut profile = base.clone();
                profile.name = self.get("name", cx);
                profile.agent_id = self.get("agent", cx);
                profile.model = Some(self.get("model", cx));
                let args =
                    shell_words::split(&self.get("arguments", cx)).map_err(|e| e.to_string())?;
                if args != base.arguments.clone().unwrap_or_default().values {
                    profile.arguments = Some(p::StringValues { values: args });
                }
                if profile.name.trim().is_empty() || profile.agent_id.trim().is_empty() {
                    return Err("Profile name and agent are required.".into());
                }
                let mut s = base.settings.clone().unwrap_or_default();
                macro_rules! option {
                    ($field:ident) => {
                        if self.get(stringify!($field), cx)
                            != s.$field.as_deref().unwrap_or_default()
                        {
                            s.$field = Some(self.get(stringify!($field), cx));
                        }
                    };
                }
                option!(permission_mode);
                option!(effort_level);
                option!(base_url);
                option!(provider);
                option!(approval_mode);
                option!(sandbox);
                option!(config_profile);
                if self.flags["Full auto"] != s.full_auto.unwrap_or(false) {
                    s.full_auto = Some(self.flags["Full auto"]);
                }
                if self.flags["Bypass approvals"] != s.bypass_approvals.unwrap_or(false) {
                    s.bypass_approvals = Some(self.flags["Bypass approvals"]);
                }
                let settings_json = secret(&self.settings)?;
                if let Some(p::SecretUpdate {
                    action: Some(p::secret_update::Action::Set(bytes)),
                }) = &settings_json
                {
                    serde_json::from_slice::<serde_json::Value>(bytes)
                        .map_err(|e| format!("Settings JSON: {e}"))?;
                }
                Write::Profile(p::SaveProfileRequest {
                    profile: Some(profile),
                    environment: env,
                    credential: secret(&self.credential)?,
                    settings: Some(p::ProfileSettingsUpdate {
                        options: (!*acp).then_some(s),
                        append_system_prompt: secret(&self.prompt)?,
                        settings_json,
                    }),
                    ..Default::default()
                })
            }
            Draft::Integration(base) => {
                let mut integration = base.clone();
                integration.kind = ["github", "jira", "youtrack"][self.kind].into();
                integration.name = self.get("name", cx);
                integration.endpoint = self.get("endpoint", cx);
                integration.owner = nonempty(self.get("owner", cx));
                integration.email = nonempty(self.get("email", cx));
                integration.cloud_id = nonempty(self.get("cloud_id", cx));
                if integration.name.trim().is_empty() {
                    return Err("Integration name is required.".into());
                }
                Write::Integration(p::SaveIntegrationRequest {
                    integration: Some(integration),
                    credential: secret(&self.credential)?,
                    ..Default::default()
                })
            }
        })
    }
}
fn flag_label(key: &str) -> &str {
    match key {
        "Notch" => "Show the notch",
        "Full auto" => "Full auto: run without asking",
        "Bypass approvals" => "Skip approval prompts",
        "Enabled" => "Available for new chats",
        other => other,
    }
}
impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = column().gap(px(16.));
        if matches!(self.draft, Draft::Agent(_)) {
            body = body.child(form_field(
                "Type",
                "Set when the agent is created.",
                row().child(badge(
                    ["Claude", "Codex", "ACP"]
                        .get(self.backend)
                        .copied()
                        .unwrap_or("ACP"),
                )),
            ));
        }
        if matches!(self.draft, Draft::Integration(_)) {
            body = body.child(form_field(
                "Provider",
                "Removing the credential disconnects this integration. Linked tasks and projects remain.",
                chrome::segmented(["GitHub", "Jira", "YouTrack"].into_iter().enumerate().map(
                    |(index, label)| {
                        chrome::segment(
                            SharedString::from(format!("provider:{index}")),
                            label,
                            self.kind == index,
                        )
                        .disabled(!self.enabled)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.kind = index;
                            cx.notify();
                        }))
                    },
                )),
            ));
        }
        let mut fields = vec![];
        for (key, label) in &self.labels {
            if matches!(self.draft, Draft::Profile(_, true))
                && [
                    "permission_mode",
                    "effort_level",
                    "base_url",
                    "provider",
                    "approval_mode",
                    "sandbox",
                    "config_profile",
                ]
                .contains(key)
            {
                continue;
            }
            fields.push(if let Some(select) = self.selects.get(key) {
                form_field(
                    *label,
                    "",
                    dropdown(select).w_full().disabled(!self.enabled),
                )
            } else {
                labeled(*label, &self.fields[key], !self.enabled)
            });
        }
        body = body.child(field_grid(fields));
        if !matches!(self.draft, Draft::Profile(_, true)) && !self.flags.is_empty() {
            body = body.child(column().gap(px(8.)).children(self.flags.iter().map(
                |(key, checked)| {
                    let key = *key;
                    Checkbox::new(key)
                        .label(flag_label(key).to_owned())
                        .checked(*checked)
                        .disabled(!self.enabled)
                        .on_change(cx.listener(move |s, v, _, cx| {
                            s.flags.insert(key, *v);
                            cx.notify();
                        }))
                },
            )));
        }
        let secrets = [&self.credential, &self.prompt, &self.settings]
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if !secrets.is_empty() {
            body = body.child(
                column()
                    .gap(px(12.))
                    .pt(px(16.))
                    .border_t_1()
                    .border_color(t::border())
                    .child(caption("SECRETS"))
                    .children(secrets),
            );
        }
        if matches!(self.draft, Draft::Agent(_) | Draft::Profile(_, _)) {
            body = body.child(
                column()
                    .gap(px(12.))
                    .pt(px(16.))
                    .border_t_1()
                    .border_color(t::border())
                    .child(
                        column()
                            .gap(px(4.))
                            .child(caption("ENVIRONMENT VARIABLES"))
                            .child(div().text_size(px(11.)).text_color(t::muted()).child(
                                "Values are write-only: saved values are never shown here.",
                            )),
                    )
                    .children(self.environment.iter().map(|(_, field)| field.clone()))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                input(&self.new_environment)
                                    .flex_1()
                                    .disabled(!self.enabled),
                            )
                            .child(
                                button("add-env", "Add variable")
                                    .icon(icon(gpui_kit::component::IconName::Plus))
                                    .disabled(!self.enabled)
                                    .on_click(cx.listener(|s, _, window, cx| {
                                        let name = value(&s.new_environment, cx).trim().to_owned();
                                        if name.is_empty()
                                            || name.contains('=')
                                            || name.contains('\0')
                                            || s.environment.iter().any(|(n, _)| n == &name)
                                        {
                                            s.error = Some(
                                                "Enter a unique variable name without = or NUL."
                                                    .into(),
                                            );
                                        } else {
                                            let field = cx.new(|cx| {
                                                SecretField::new(&name, false, None, window, cx)
                                                    .allow_empty()
                                            });
                                            s.environment.push((name, field));
                                            s.new_environment
                                                .update(cx, |v, cx| v.set_value("", window, cx));
                                            s.error = None;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }
        body.children(self.error.clone().map(chrome::failure))
    }
}

impl Editor {
    /// Reapply only user-edited metadata to a reviewed fresh snapshot. Secret
    /// patches remain explicit and never depend on secret readback.
    pub(super) fn write_reviewed(
        &self,
        latest: &p::HostSettings,
        cx: &App,
    ) -> Result<Write, String> {
        let removed = match &self.draft {
            Draft::Agent(old) => {
                self.catalog.agents.iter().any(|v| v.id == old.id)
                    && !latest.agents.iter().any(|v| v.id == old.id)
            }
            Draft::Profile(old, _) => {
                self.catalog.profiles.iter().any(|v| v.id == old.id)
                    && !latest.profiles.iter().any(|v| v.id == old.id)
            }
            Draft::Integration(old) => {
                self.catalog.integrations.iter().any(|v| v.id == old.id)
                    && !latest.integrations.iter().any(|v| v.id == old.id)
            }
            Draft::Preferences(_) => false,
        };
        if removed {
            return Err("This record was removed on the host. Close this draft and create a new record explicitly.".into());
        }
        let mut write = self.write(cx)?;
        macro_rules! preserve {($next:ident,$old:ident,$latest:ident,$($field:ident),+)=>{$(if $next.$field==$old.$field{$next.$field=$latest.$field.clone();})+};}
        match (&self.draft, &mut write) {
            (Draft::Agent(old), Write::Agent(r)) => {
                if let (Some(next), Some(latest)) =
                    (&mut r.agent, latest.agents.iter().find(|v| v.id == old.id))
                {
                    preserve!(
                        next,
                        old,
                        latest,
                        name,
                        command,
                        arguments,
                        backend_kind,
                        enabled,
                        default_profile_id
                    );
                }
            }
            (Draft::Profile(old, _), Write::Profile(r)) => {
                if let (Some(next), Some(latest)) = (
                    &mut r.profile,
                    latest.profiles.iter().find(|v| v.id == old.id),
                ) {
                    preserve!(next, old, latest, name, agent_id, model, arguments);
                    if let Some(options) = r.settings.as_mut().and_then(|s| s.options.as_mut()) {
                        let old = old.settings.clone().unwrap_or_default();
                        let latest = latest.settings.clone().unwrap_or_default();
                        preserve!(
                            options,
                            old,
                            latest,
                            permission_mode,
                            effort_level,
                            base_url,
                            provider,
                            approval_mode,
                            sandbox,
                            full_auto,
                            bypass_approvals,
                            config_profile
                        );
                    }
                }
            }
            (Draft::Integration(old), Write::Integration(r)) => {
                if let (Some(next), Some(latest)) = (
                    &mut r.integration,
                    latest.integrations.iter().find(|v| v.id == old.id),
                ) {
                    preserve!(
                        next, old, latest, name, kind, endpoint, owner, email, cloud_id
                    );
                }
            }
            _ => {}
        }
        Ok(write)
    }
}
