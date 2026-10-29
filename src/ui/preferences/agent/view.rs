use super::super::search::PageSearch;
use super::*;
impl AgentForm {
    fn search(&self, cx: &App) -> PageSearch {
        self.search.read(cx).current(cx).scroll(&self.scroll)
    }
    /// The text field of `entry.id`.
    fn text_row(
        &self,
        search: &PageSearch,
        entry: &Entry,
        help: impl Into<SharedString>,
        first: bool,
    ) -> Div {
        search.stacked_setting(
            entry,
            help,
            first,
            input(&self.texts[entry.id])
                .disabled(self.disabled)
                .w_full(),
        )
    }
    /// The dropdown of `entry.id`.
    fn select_row(&self, search: &PageSearch, entry: &Entry, first: bool) -> Div {
        search.setting(
            entry,
            first,
            dropdown(&self.selects[entry.id])
                .disabled(self.disabled)
                .w(px(if entry.id == "provider" { 220. } else { 200. })),
        )
    }
    fn model_section(&self, cx: &Context<Self>) -> Div {
        let search = self.search(cx);
        let help = match self.agent {
            Agent::Claude => "Short name (sonnet, opus, haiku) or full model ID",
            Agent::Codex => "Leave empty for Codex default",
            Agent::Gemini => "Leave empty for the Gemini CLI default",
            Agent::OpenCode => "Format: provider/model",
        };
        let mut rows = column().child(self.text_row(&search, &MODEL, help, true));
        match self.agent {
            Agent::Claude => {
                rows = rows
                    .child(self.select_row(&search, &PERMISSION, false))
                    .child(self.select_row(&search, &EFFORT, false));
            }
            Agent::Codex => {
                rows = rows
                    .child(self.select_row(&search, &CODEX_APPROVAL, false))
                    .child(self.select_row(&search, &SANDBOX, false))
                    .child(
                        search.setting(
                            &FULL_AUTO,
                            false,
                            checkbox("agent-full-auto", self.full_auto, FULL_AUTO.label)
                                .disabled(self.disabled)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.full_auto = *value;
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        search.setting(
                            &BYPASS,
                            false,
                            checkbox("agent-bypass", self.bypass, BYPASS.label)
                                .disabled(self.disabled)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.bypass = *value;
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(self.text_row(&search, &CONFIG_PROFILE, CONFIG_PROFILE.help, false));
            }
            Agent::Gemini => {
                rows = rows.child(self.select_row(&search, &GEMINI_APPROVAL, false));
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
        let search = self.search(cx);
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
            .child(self.text_row(&search, &API_KEY, help, true))
            .children(self.has_key.then(|| {
                search.mark(
                    &[API_KEY.id],
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
                    ),
                )
            }));
        if matches!(self.agent, Agent::Claude | Agent::Codex) {
            fields = fields.child(self.text_row(&search, &BASE_URL, BASE_URL.help, false));
        }
        if self.agent == Agent::Claude {
            fields = fields.child(self.select_row(&search, &PROVIDER, false));
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
        let search = self.search(cx);
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
                    search.stacked_setting(
                        &PROMPT,
                        if self.original.prompt_ref.is_some() {
                            "Saved securely. Leave empty to keep it, or enter a replacement."
                        } else {
                            PROMPT.help
                        },
                        true,
                        column()
                            .gap_2()
                            .child(
                                textarea(&self.prompt)
                                    .disabled(self.disabled || self.clear_prompt)
                                    .h(px(72.))
                                    .w_full(),
                            )
                            .children(self.original.prompt_ref.is_some().then(|| {
                                checkbox(
                                    "remove-saved-prompt",
                                    self.clear_prompt,
                                    "Remove saved system prompt",
                                )
                                .disabled(self.disabled)
                                .on_click(cx.listener(
                                    |this, value, _, cx| {
                                        this.clear_prompt = *value;
                                        cx.notify();
                                    },
                                ))
                            })),
                    ),
                )
            }))
            .child(search.mark(
                &[ENVIRONMENT.id],
                preference_section("ENVIRONMENT VARIABLES", self.env.clone()),
            ))
            .child(preference_section(
                "ADVANCED",
                search.stacked_setting(
                    if self.agent == Agent::OpenCode {
                        &CONFIG_JSON
                    } else {
                        &SETTINGS_JSON
                    },
                    if self.original.settings_json_ref.is_some() {
                        "Saved securely. Leave empty to keep it, or enter a replacement."
                    } else {
                        help
                    },
                    true,
                    column()
                        .gap_2()
                        .child(
                            textarea(&self.json)
                                .disabled(self.disabled || self.clear_json)
                                .h(px(96.))
                                .w_full(),
                        )
                        .children(self.original.settings_json_ref.is_some().then(|| {
                            checkbox(
                                "remove-saved-json",
                                self.clear_json,
                                "Remove saved JSON override",
                            )
                            .disabled(self.disabled)
                            .on_click(cx.listener(
                                |this, value, _, cx| {
                                    this.clear_json = *value;
                                    cx.notify();
                                },
                            ))
                        })),
                ),
            ))
    }
}
