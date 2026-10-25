use super::*;
use gpui_kit::component::{input::Input, radio::RadioGroup};

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
    form_field(label, "", Input::new(field).disabled(disabled))
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
impl Render for SecretField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        column()
            .gap_2()
            .child(format!(
                "{} · {}",
                self.label,
                if self.configured {
                    "•••••••• (configured)"
                } else {
                    "Not configured"
                }
            ))
            .child(
                RadioGroup::new("secret-action")
                    .children(["Unchanged", "Set", "Remove"])
                    .selected_index(Some(self.mode))
                    .disabled(!self.enabled)
                    .on_change(cx.listener(|s, ix, window, cx| {
                        if *ix == 1 && s.unsupported.is_some() {
                            return;
                        }
                        s.mode = *ix;
                        if *ix != 1 {
                            s.input.update(cx, |v, cx| v.set_value("", window, cx));
                            if let Some(m) = &s.multiline {
                                m.update(cx, |v, cx| v.set_value("", window, cx));
                            }
                        }
                        cx.notify();
                    })),
            )
            .children(
                self.unsupported
                    .clone()
                    .map(|reason| div().text_color(t::muted()).child(reason)),
            )
            .children((self.mode == 1).then(|| {
                if let Some(m) = &self.multiline {
                    textarea(m)
                        .h_40()
                        .disabled(!self.enabled)
                        .into_any_element()
                } else {
                    Input::new(&self.input)
                        .disabled(!self.enabled)
                        .into_any_element()
                }
            }))
    }
}
