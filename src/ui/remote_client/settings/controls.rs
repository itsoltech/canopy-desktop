use super::*;

pub(super) fn text(
    value: impl Into<SharedString>,
    secret: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let value = value.into();
    cx.new(|cx| {
        let mut state = InputState::new(window, cx).masked(secret);
        state.set_value(value, window, cx);
        state
    })
}
pub(super) fn value(field: &Entity<InputState>, cx: &App) -> String {
    field.read(cx).value().to_string()
}
pub(super) fn labeled(
    label: impl Into<SharedString>,
    field: &Entity<InputState>,
    disabled: bool,
) -> Div {
    form_field(label.into(), "", input(field).w_full().disabled(disabled))
}
/// Fixed configured marker; no secret is ever supplied by a settings snapshot.
pub(super) struct SecretField {
    label: String,
    configured: bool,
    mode: usize,
    input: Entity<InputState>,
    unsupported: Option<String>,
    enabled: bool,
    allow_empty: bool,
    multiline: Option<Entity<TextareaState>>,
}
impl SecretField {
    pub(super) fn new(
        label: &str,
        configured: bool,
        unsupported: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            label: label.into(),
            configured,
            mode: 0,
            input: text("", true, window, cx),
            unsupported,
            enabled: true,
            allow_empty: false,
            multiline: matches!(label, "Append system prompt" | "Settings JSON override")
                .then(|| cx.new(|cx| TextareaState::new(window, cx))),
        }
    }
    pub(super) fn allow_empty(mut self) -> Self {
        self.allow_empty = true;
        self
    }
    pub(super) fn set_unsupported(&mut self, reason: Option<String>, cx: &mut Context<Self>) {
        self.unsupported = reason;
        cx.notify();
    }
    pub(super) fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }
    pub(super) fn update(&self, cx: &App) -> Result<p::SecretUpdate, String> {
        use p::secret_update::Action;
        let action = match self.mode {
            1 => {
                if let Some(reason) = &self.unsupported {
                    return Err(reason.clone());
                }
                let v = self
                    .multiline
                    .as_ref()
                    .map(|s| s.read(cx).value().to_string())
                    .unwrap_or_else(|| value(&self.input, cx));
                if v.is_empty() && !self.allow_empty {
                    return Err(format!("Enter {} or choose Unchanged.", self.label));
                }
                Action::Set(v.into_bytes())
            }
            2 => Action::Remove(p::Empty {}),
            _ => Action::Unchanged(p::Empty {}),
        };
        Ok(p::SecretUpdate {
            action: Some(action),
        })
    }
}
impl SecretField {
    fn choose(&mut self, mode: usize, window: &mut Window, cx: &mut Context<Self>) {
        if mode == 1 && self.unsupported.is_some() {
            return;
        }
        self.mode = mode;
        if mode != 1 {
            self.input.update(cx, |v, cx| v.set_value("", window, cx));
            if let Some(m) = &self.multiline {
                m.update(cx, |v, cx| v.set_value("", window, cx));
            }
        }
        cx.notify();
    }
}
/// Write-only secret: the saved value is never shown, only whether one exists.
impl Render for SecretField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let choices = [
            if self.configured {
                "Keep"
            } else {
                "Leave unset"
            },
            if self.configured { "Replace" } else { "Set" },
            "Remove",
        ];
        column()
            .gap(px(8.))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t::secondary())
                            .child(self.label.clone()),
                    )
                    .child(badge(if self.configured { "Saved" } else { "Not set" })),
            )
            .child(chrome::segmented(choices.into_iter().enumerate().map(
                |(index, label)| {
                    chrome::segment(
                        SharedString::from(format!("secret:{}:{index}", self.label)),
                        label,
                        self.mode == index,
                    )
                    .disabled(!self.enabled || (index == 1 && self.unsupported.is_some()))
                    .on_click(cx.listener(move |s, _, window, cx| s.choose(index, window, cx)))
                },
            )))
            .children(self.unsupported.clone().map(|reason| {
                div()
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child(reason)
            }))
            .children((self.mode == 1).then(|| {
                if let Some(m) = &self.multiline {
                    textarea(m)
                        .h_40()
                        .disabled(!self.enabled)
                        .into_any_element()
                } else {
                    input(&self.input)
                        .w_full()
                        .disabled(!self.enabled)
                        .into_any_element()
                }
            }))
    }
}
