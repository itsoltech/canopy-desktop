use super::*;

impl AgentForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let texts = [
            ("model", "Default"),
            ("base_url", "https://api.anthropic.com"),
            ("config_profile", "Default"),
            ("api_key", "Leave empty to keep the saved key"),
        ]
        .into_iter()
        .map(|(key, hint)| {
            (
                key,
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(hint)
                        .masked(key == "api_key")
                }),
            )
        })
        .collect();
        let selects = ["permission", "effort", "provider", "approval", "sandbox"]
            .into_iter()
            .map(|key| {
                (
                    key,
                    cx.new(|cx| {
                        SelectState::new(
                            options::choices(key, Agent::Claude),
                            Some(gpui_kit::component::IndexPath::new(0)),
                            window,
                            cx,
                        )
                    }),
                )
            })
            .collect();
        Self {
            agent: Agent::Claude,
            reveal: Presence::new(true, motion::presets::CONTENT_REVEAL, Instant::now()),
            texts,
            selects,
            prompt: cx.new(|cx| TextareaState::new(window, cx)),
            json: cx.new(|cx| TextareaState::new(window, cx)),
            env: cx.new(|cx| EnvironmentEditor::new(window, cx)),
            full_auto: false,
            bypass: false,
            clear_key: false,
            has_key: false,
            disabled: false,
            original: Default::default(),
        }
    }
    pub fn load(
        &mut self,
        agent: Agent,
        profile: Option<&Profile>,
        key_edit: Option<&String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        self.reveal = Presence::new(false, motion::presets::CONTENT_REVEAL, now);
        self.reveal.set_open(true, now, motion::policy(cx));
        self.agent = agent;
        self.texts["model"].update(cx, |s, cx| {
            s.set_placeholder(
                match agent {
                    Agent::Claude => "sonnet, opus, haiku, or model ID",
                    Agent::OpenCode => "provider/model",
                    _ => "Default",
                },
                window,
                cx,
            )
        });
        self.texts["base_url"].update(cx, |s, cx| {
            s.set_placeholder(
                if agent == Agent::Codex {
                    "https://api.openai.com"
                } else {
                    "https://api.anthropic.com"
                },
                window,
                cx,
            )
        });
        let settings = profile.map(|p| p.settings.clone()).unwrap_or_default();
        for (key, value) in [
            (
                "model",
                profile.map(|p| p.model.clone()).unwrap_or_default(),
            ),
            ("base_url", settings.base_url.clone()),
            ("config_profile", settings.config_profile.clone()),
            ("api_key", key_edit.cloned().unwrap_or_default()),
        ] {
            self.texts[key].update(cx, |s, cx| s.set_value(value, window, cx));
        }
        self.selects["approval"].update(cx, |s, cx| {
            s.set_items(options::choices("approval", agent), window, cx)
        });
        for (key, value) in [
            ("permission", &settings.permission_mode),
            ("effort", &settings.effort_level),
            ("provider", &settings.provider),
            ("approval", &settings.approval_mode),
            ("sandbox", &settings.sandbox),
        ] {
            self.selects[key].update(cx, |s, cx| {
                s.set_selected_value(&SharedString::from(value.clone()), window, cx)
            });
        }
        self.prompt.update(cx, |s, cx| {
            s.set_value(settings.append_system_prompt.clone(), window, cx)
        });
        self.json.update(cx, |s, cx| {
            s.set_value(settings.settings_json.clone(), window, cx)
        });
        self.env
            .update(cx, |s, cx| s.load(settings.custom_env.clone(), window, cx));
        self.full_auto = settings.full_auto;
        self.bypass = settings.bypass_approvals;
        self.has_key = settings.api_key_ref.is_some();
        self.clear_key = key_edit.is_some_and(|v| v.is_empty());
        self.original = settings;
        cx.notify();
    }
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        self.env.update(cx, |e, cx| {
            e.disabled = disabled;
            cx.notify();
        });
        cx.notify();
    }
    pub fn collect(&self, cx: &App) -> Result<(String, AgentSettings, Option<String>), String> {
        let get = |key| self.texts[key].read(cx).value().trim().to_owned();
        let select = |key| {
            self.selects[key]
                .read(cx)
                .selected_value()
                .map(|v| v.to_string())
                .unwrap_or_default()
        };
        let mut s = self.original.clone();
        s.permission_mode = select("permission");
        s.effort_level = select("effort");
        s.provider = select("provider");
        s.approval_mode = select("approval");
        s.sandbox = select("sandbox");
        s.base_url = get("base_url");
        s.config_profile = get("config_profile");
        s.full_auto = self.full_auto;
        s.bypass_approvals = self.bypass;
        s.append_system_prompt = self.prompt.read(cx).value().to_string();
        s.settings_json = self.json.read(cx).value().to_string();
        s.custom_env = self.env.read(cx).values.clone();
        s.validate(Some(self.agent))?;
        let key = get("api_key");
        let edit = if self.clear_key {
            Some(String::new())
        } else if !key.is_empty() {
            Some(key)
        } else {
            None
        };
        Ok((get("model"), s, edit))
    }
}
