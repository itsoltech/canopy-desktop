use super::*;

impl ToolsPreferences {
    pub(super) fn add_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.capture_profile(cx) {
            return;
        }
        let profile = Profile {
            settings: Default::default(),
            id: new_id(),
            name: "New profile".into(),
            model: String::new(),
            arguments: vec![],
        };
        self.profile = Some(profile.id.clone());
        if self.draft.default_profile.is_none() {
            self.draft.default_profile = Some(profile.id.clone());
        }
        self.draft.profiles.push(profile);
        self.load_profile(window, cx);
        cx.notify();
    }
    fn capture_profile(&mut self, cx: &mut Context<Self>) -> bool {
        if let Err(error) = self.collect_profile(cx) {
            self.error = Some(error);
            cx.notify();
            return false;
        }
        true
    }
    pub(super) fn select_profile(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.capture_profile(cx) {
            return;
        }
        self.profile = Some(id.to_owned());
        self.load_profile(window, cx);
        cx.notify();
    }
    pub(super) fn remove_profile(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.capture_profile(cx) {
            return;
        }
        self.draft.profiles.retain(|p| p.id != id);
        if self.draft.default_profile.as_deref() == Some(id) {
            self.draft.default_profile = self.draft.profiles.first().map(|p| p.id.clone());
        }
        self.profile = self.draft.profiles.first().map(|p| p.id.clone());
        self.load_profile(window, cx);
        cx.notify();
    }
    fn profile_row(&self, profile: &Profile, disabled: bool, cx: &Context<Self>) -> Div {
        let id = profile.id.clone();
        let default_id = id.clone();
        let remove_id = id.clone();
        let is_default = self.draft.default_profile.as_ref() == Some(&id);
        row()
            .gap(px(8.))
            .child(
                selection_button(
                    SharedString::from(format!("edit-profile-{id}")),
                    profile.name.clone(),
                    self.profile.as_ref() == Some(&id),
                )
                .flex_1()
                .disabled(disabled)
                .on_click(
                    cx.listener(move |this, _, window, cx| this.select_profile(&id, window, cx)),
                ),
            )
            .child(
                button(
                    SharedString::from(format!("default-profile-{default_id}")),
                    if is_default {
                        "Default"
                    } else {
                        "Make default"
                    },
                )
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.draft.default_profile = Some(default_id.clone());
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    SharedString::from(format!("delete-profile-{remove_id}")),
                    IconName::Close,
                    "Delete profile",
                )
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.remove_profile(&remove_id, window, cx)
                })),
            )
    }
    fn profile_fields(&self, disabled: bool) -> Div {
        column().gap(px(12.))
            .child(form_field("Profile name", "Select this profile from the expanded tool in the sidebar.", input(&self.profile_name).disabled(disabled).w_full()))
            .children(self.draft.is_agent().then(|| form_field("Model", "Empty keeps the CLI default; passed with --model.", input(&self.model).disabled(disabled).w_full())))
            .child(form_field("Profile arguments", "Appended after the tool arguments. Configure permission mode, effort and other CLI options here.", input(&self.profile_args).disabled(disabled).w_full()))
    }
    pub(super) fn profiles(&self, disabled: bool, cx: &Context<Self>) -> Div {
        column()
            .gap(px(16.))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Profiles"),
                    )
                    .child(
                        button("add-profile", "Add profile")
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_profile(window, cx)),
                            ),
                    ),
            )
            .child(
                column().gap(px(4.)).children(
                    self.draft
                        .profiles
                        .iter()
                        .map(|profile| self.profile_row(profile, disabled, cx)),
                ),
            )
            .children(
                self.profile
                    .is_some()
                    .then(|| self.profile_fields(disabled)),
            )
    }
}
