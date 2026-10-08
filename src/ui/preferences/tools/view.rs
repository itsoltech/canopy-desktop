use super::*;
impl ToolsPreferences {
    fn header(&self, disabled: bool, cx: &Context<Self>) -> Div {
        row()
            .h(px(60.))
            .flex_shrink_0()
            .px(px(28.))
            .gap(px(8.))
            .border_b_1()
            .border_color(t::border())
            .child(
                div()
                    .flex_1()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Tools & profiles"),
            )
            .child(
                loading_button("refresh-tools", "Refresh", &self.refresh_loading)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |state, cx| state.refresh(cx))
                    })),
            )
            .child(
                primary_loading_button("save-tool-header", "Save changes", &self.save_loading)
                    .disabled(disabled && !self.save_loading.active())
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            )
            .child(
                button("add-tool", "Add tool")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.load(
                            ToolDefinition {
                                id: new_id(),
                                name: "New tool".into(),
                                kind: ToolKind::Custom,
                                executable: String::new(),
                                arguments: vec![],
                                enabled: true,
                                profiles: vec![],
                                default_profile: None,
                            },
                            window,
                            cx,
                        );
                    })),
            )
    }
    fn tool_selector(&self, disabled: bool, cx: &Context<Self>) -> Div {
        let selected = self.draft.id.clone();
        let catalog_tools = self.state.read(cx).catalog.tools.clone();
        row()
            .flex_wrap()
            .gap(px(4.))
            .children(catalog_tools.into_iter().map(|tool| {
                let id = tool.id.clone();
                selection_button(
                    SharedString::from(format!("edit-tool-{id}")),
                    tool.name,
                    id == selected,
                )
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, window, cx| this.select_tool(&id, window, cx)))
            }))
    }
    fn tool_fields(&self, disabled: bool, cx: &Context<Self>) -> Div {
        let availability = self
            .state
            .read(cx)
            .availability
            .get(&self.draft.id)
            .map(|result| match result {
                Ok(path) => format!("Installed: {}", path.display()),
                Err(e) => e.clone(),
            })
            .unwrap_or_else(|| "Save changes to check the executable.".into());
        column()
            .gap(px(16.))
            .child(form_field(
                "Name",
                "Name shown in the sidebar.",
                input(&self.name).disabled(disabled).w_full(),
            ))
            .child(form_field(
                "Executable",
                "Program or absolute path. Empty uses your login shell for Shell.",
                input(&self.executable).disabled(disabled).w_full(),
            ))
            .child(form_field(
                "Arguments",
                "Argument list with platform quoting. Backslashes are preserved; no shell interpolation.",
                input(&self.arguments).disabled(disabled).w_full(),
            ))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child(availability),
            )
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        checkbox("tool-enabled", self.draft.enabled, "Enable tool")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, value, _, cx| {
                                this.draft.enabled = *value;
                                cx.notify();
                            })),
                    )
                    .child("Enable tool"),
            )
    }
    fn footer(&self, disabled: bool, cx: &Context<Self>) -> Div {
        let selected = self.draft.id.clone();
        let error = self
            .error
            .clone()
            .or_else(|| self.state.read(cx).error.clone());
        column()
            .gap(px(16.))
            .children(error.map(|e| div().text_color(t::red()).child(e)))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        primary_loading_button("save-tool", "Save changes", &self.save_loading)
                            .disabled(disabled && !self.save_loading.active())
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
                    .child(button("cancel-tool", "Revert").disabled(disabled).on_click(
                        cx.listener(|this, _, window, cx| {
                            let id = if this.state.read(cx).catalog.get(&this.draft.id).is_some() {
                                this.draft.id.clone()
                            } else {
                                "shell".into()
                            };
                            this.select_tool(&id, window, cx);
                        }),
                    ))
                    .children(
                        (self.draft.kind == ToolKind::Custom
                            && self.state.read(cx).catalog.get(&selected).is_some())
                        .then(|| {
                            button("delete-tool", "Delete tool")
                                .disabled(disabled)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let mut catalog = this.state.read(cx).catalog.clone();
                                    let result = catalog.remove(&this.draft.id).and_then(|_| {
                                        this.state.update(cx, |state, cx| state.save(catalog, cx))
                                    });
                                    match result {
                                        Ok(()) => this.awaiting_save = true,
                                        Err(e) => this.error = Some(e),
                                    }
                                    cx.notify();
                                }))
                        }),
                    ),
            )
    }
}
impl Render for ToolsPreferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.agent_page {
            return self.agent_profiles(cx);
        }
        let state = self.state.read(cx);
        let disabled = !state.ready || state.saving;
        column().size_full().child(self.header(disabled, cx)).child(
            column()
                .id("tools-preferences-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .px(px(28.))
                .py(px(20.))
                .child(
                    column()
                        .flex_shrink_0()
                        .gap(px(16.))
                        .child(self.tool_selector(disabled, cx))
                        .child(self.tool_fields(disabled, cx))
                        .child(self.profiles(disabled, cx))
                        .child(self.footer(disabled, cx)),
                ),
        )
    }
}
