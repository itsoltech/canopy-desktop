use super::*;
impl ToolsPreferences {
    pub(super) fn agent_profiles(&self, cx: &Context<Self>) -> Div {
        let state = self.state.read(cx);
        let disabled = !state.ready || state.saving;
        let profiles = self
            .draft
            .profiles
            .iter()
            .map(|profile| {
                let id = profile.id.clone();
                let remove = id.clone();
                let active = self.profile.as_ref() == Some(&id);
                row()
                    .rounded(px(4.))
                    .bg(if active {
                        t::selected()
                    } else {
                        rgba(0).into()
                    })
                    .child(
                        list_button(
                            SharedString::from(format!("agent-profile-{id}")),
                            profile.name.clone(),
                            active,
                            cx,
                        )
                        .flex_1()
                        .min_w_0()
                        .px(px(8.))
                        .border_0()
                        .disabled(disabled)
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.select_profile(&id, window, cx),
                        )),
                    )
                    .child(
                        icon_button(
                            SharedString::from(format!("agent-remove-{remove}")),
                            IconName::Close,
                            format!("Delete {}", profile.name),
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.remove_profile(&remove, window, cx),
                        )),
                    )
            })
            .collect::<Vec<_>>();
        let profile_id = self.profile.clone();
        let content = column()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(self.profile.is_some().then(|| {
                row()
                    .items_end()
                    .gap(px(12.))
                    .pb(px(16.))
                    .mb(px(28.))
                    .border_b_1()
                    .border_color(t::border())
                    .flex_shrink_0()
                    .child(
                        column()
                            .flex_1()
                            .gap(px(4.))
                            .child(caption("PROFILE NAME"))
                            .child(input(&self.profile_name).disabled(disabled).w_full()),
                    )
                    .child(
                        button(
                            "agent-default",
                            if self.draft.default_profile == profile_id {
                                "Default"
                            } else {
                                "Make default"
                            },
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.draft.default_profile = profile_id.clone();
                            cx.notify();
                        })),
                    )
                    .child(
                        primary_loading_button("save-agent", "Save", &self.save_loading)
                            .disabled(disabled && !self.save_loading.active())
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
            }))
            .children(
                self.error
                    .clone()
                    .or_else(|| state.error.clone())
                    .map(|e| div().text_color(t::red()).pb(px(12.)).child(e)),
            )
            .child(
                column()
                    .id("agent-form-scroll")
                    .flex_1()
                    .overflow_y_scrollbar()
                    .pr(px(4.))
                    .py(px(8.))
                    .children(self.profile.is_some().then(|| self.agent_form.clone()))
                    .children(self.profile.is_none().then(|| {
                        div()
                            .text_color(t::muted())
                            .child("Select or create a profile to begin.")
                    })),
            );
        row()
            .size_full()
            .items_stretch()
            .p(px(28.))
            .gap(px(20.))
            .child(
                column()
                    .w(px(180.))
                    .flex_shrink_0()
                    .h_full()
                    .pr(px(12.))
                    .border_r_1()
                    .border_color(t::border())
                    .gap(px(8.))
                    .child(
                        row()
                            .justify_between()
                            .pl(px(4.))
                            .child(caption("PROFILES"))
                            .child(
                                icon_button("new-agent-profile", IconName::Plus, "New profile")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_profile(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        column()
                            .id("agent-profiles-scroll")
                            .flex_1()
                            .overflow_y_scrollbar()
                            .gap(px(4.))
                            .children(profiles),
                    ),
            )
            .child(content)
    }
}
