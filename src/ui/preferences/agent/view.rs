use super::*;
impl AgentForm {
    fn text_row(&self, key: &str, label: &str, help: &str, first: bool) -> Div {
        stacked_setting(
            label.to_owned(),
            help.to_owned(),
            first,
            input(&self.texts[key]).disabled(self.disabled).w_full(),
        )
    }
    fn select_row(&self, key: &str, label: &str, help: &str, first: bool) -> Div {
        setting(
            label.to_owned(),
            help.to_owned(),
            first,
            dropdown(&self.selects[key])
                .disabled(self.disabled)
                .w(px(if key == "provider" { 220. } else { 200. })),
        )
    }
    fn model_section(&self, cx: &Context<Self>) -> Div {
        let help = match self.agent {
            Agent::Claude => "Short name (sonnet, opus, haiku) or full model ID",
            Agent::Codex => "Leave empty for Codex default",
            Agent::Gemini => "Leave empty for the Gemini CLI default",
            Agent::OpenCode => "Format: provider/model",
        };
        let mut rows = column().child(self.text_row("model", "Model", help, true));
        match self.agent {
            Agent::Claude => {
                rows=rows.child(self.select_row("permission","Permission mode","Controls what Claude can do without asking. Plan = read-only, Auto = full autonomy.",false)).child(self.select_row("effort","Effort level","Higher effort means more thorough but slower responses",false));
            }
            Agent::Codex => {
                rows = rows
                    .child(self.select_row(
                        "approval",
                        "Approval mode",
                        "Controls when Codex pauses for human approval",
                        false,
                    ))
                    .child(self.select_row("sandbox", "Sandbox", "Command execution policy", false))
                    .child(setting(
                        "Full auto",
                        "Workspace-write sandbox + on-request approvals",
                        false,
                        checkbox("agent-full-auto", self.full_auto, "Full auto")
                            .disabled(self.disabled)
                            .on_click(cx.listener(|this, value, _, cx| {
                                this.full_auto = *value;
                                cx.notify();
                            })),
                    ))
                    .child(setting(
                        "Bypass approvals and sandbox",
                        "Runs Codex without approval prompts or sandbox restrictions",
                        false,
                        checkbox("agent-bypass", self.bypass, "Bypass approvals and sandbox")
                            .disabled(self.disabled)
                            .on_click(cx.listener(|this, value, _, cx| {
                                this.bypass = *value;
                                cx.notify();
                            })),
                    ))
                    .child(self.text_row(
                        "config_profile",
                        "Profile",
                        "Named CLI configuration profile",
                        false,
                    ));
            }
            Agent::Gemini => {
                rows=rows.child(self.select_row("approval","Approval mode","Controls what Gemini can do without asking. YOLO = full autonomy, Plan = read-only.",false));
            }
            Agent::OpenCode => {}
        }
        preference_section(
            if self.agent == Agent::OpenCode {
                "MODEL"
            } else {
                "MODEL & BEHAVIOR"
            },
            rows,
        )
    }
    fn api_section(&self, cx: &Context<Self>) -> Div {
        let help = format!(
            "{} API key. Falls back to {}. Saved keys are kept in {}.",
            match self.agent {
                Agent::Claude => "Anthropic",
                Agent::Codex => "OpenAI",
                Agent::Gemini => "Google AI",
                Agent::OpenCode => "Anthropic",
            },
            self.agent.api_env(),
            canopy_desktop::platform::credentials::store_name()
        );
        let mut fields = column()
            .child(self.text_row("api_key", "API key", &help, true))
            .children(self.has_key.then(|| {
                setting(
                    "Remove saved API key",
                    "Leave the key field empty to keep the saved key",
                    false,
                    checkbox("clear-agent-key", self.clear_key, "Remove saved API key")
                        .disabled(self.disabled)
                        .on_click(cx.listener(|this, value, _, cx| {
                            this.clear_key = *value;
                            cx.notify();
                        })),
                )
            }));
        if matches!(self.agent, Agent::Claude | Agent::Codex) {
            fields = fields.child(self.text_row(
                "base_url",
                "Base URL",
                "Custom API endpoint or compatible proxy",
                false,
            ));
        }
        if self.agent == Agent::Claude {
            fields = fields.child(self.select_row(
                "provider",
                "Provider",
                "Cloud provider for the Claude API backend",
                false,
            ));
        }
        preference_section(
            if self.agent == Agent::Claude {
                "API & PROVIDER"
            } else {
                "API"
            },
            fields,
        )
    }
}
impl Render for AgentForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        motion::request_frame(window, self.reveal.is_animating(now));
        let help = match self.agent {
            Agent::Claude => "Passed as Claude settings JSON for this session",
            Agent::Codex => {
                "Per-session hooks override, e.g. {\"hooks\": {}}. Existing user configuration stays unchanged."
            }
            Agent::Gemini => "Merged into per-session .gemini/settings.json",
            Agent::OpenCode => "Passed via OPENCODE_CONFIG_CONTENT at session start",
        };
        column()
            .opacity(self.reveal.progress(now))
            .flex_shrink_0()
            .gap(px(28.))
            .child(self.model_section(cx))
            .child(self.api_section(cx))
            .children((self.agent == Agent::Claude).then(|| {
                preference_section(
                    "SYSTEM PROMPT",
                    stacked_setting(
                        "Append to system prompt",
                        "Extra instructions added after the default system prompt in every session",
                        true,
                        textarea(&self.prompt)
                            .disabled(self.disabled)
                            .h(px(72.))
                            .w_full(),
                    ),
                )
            }))
            .child(preference_section(
                "ENVIRONMENT VARIABLES",
                self.env.clone(),
            ))
            .child(preference_section(
                "ADVANCED",
                stacked_setting(
                    if self.agent == Agent::OpenCode {
                        "Config JSON override"
                    } else {
                        "Settings JSON override"
                    },
                    help,
                    true,
                    textarea(&self.json)
                        .disabled(self.disabled)
                        .h(px(96.))
                        .w_full(),
                ),
            ))
    }
}
