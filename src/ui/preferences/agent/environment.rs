use super::*;
use crate::ui::components::Disclosure;
use canopy_desktop::motion;
use std::collections::{BTreeMap, HashSet};
pub(super) struct EnvironmentEditor {
    pub values: BTreeMap<String, String>,
    pub disabled: bool,
    revealed: HashSet<String>,
    key: Entity<InputState>,
    value: Entity<InputState>,
    form: Disclosure,
    error: Option<String>,
}
impl EnvironmentEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            values: Default::default(),
            disabled: false,
            revealed: Default::default(),
            key: cx.new(|cx| InputState::new(window, cx).placeholder("VARIABLE_NAME")),
            value: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("value")
                    .masked(true)
            }),
            form: Disclosure::new(false, Instant::now()),
            error: None,
        }
    }
    pub fn load(
        &mut self,
        values: BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.values = values;
        self.revealed.clear();
        self.form = Disclosure::new(false, Instant::now());
        self.error = None;
        self.key.update(cx, |s, cx| s.set_value("", window, cx));
        self.value.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }
    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.key.read(cx).value().trim().to_owned();
        let value = self.value.read(cx).value().to_string();
        if let Err(error) = canopy_desktop::state::agent_settings::validate_env(&key, &value) {
            self.error = Some(error);
            cx.notify();
            return;
        }
        self.values.insert(key.clone(), value);
        self.revealed.remove(&key);
        self.error = None;
        self.key.update(cx, |s, cx| s.set_value("", window, cx));
        self.value.update(cx, |s, cx| s.set_value("", window, cx));
        self.form.toggle(Instant::now(), cx);
        cx.notify();
    }
}
impl EnvironmentEditor {
    fn env_row(&self, key: &str, value: &str, cx: &Context<Self>) -> Div {
        let reveal = key.to_owned();
        let remove = key.to_owned();
        let shown = self.revealed.contains(key);
        row()
            .h(px(30.))
            .gap(px(6.))
            .px(px(10.))
            .rounded(px(4.))
            .bg(t::input_bg())
            .border_1()
            .border_color(t::border())
            .child(
                div()
                    .font_family(t::MONO)
                    .text_size(px(12.))
                    .text_color(t::accent())
                    .child(key.to_owned()),
            )
            .child(div().text_color(t::faint()).child("="))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .font_family(t::MONO)
                    .text_size(px(12.))
                    .text_color(if shown { t::secondary() } else { t::faint() })
                    .child(if shown {
                        value.to_owned()
                    } else {
                        "•".repeat(value.chars().count().min(12))
                    }),
            )
            .child(
                icon_button(
                    SharedString::from(format!("reveal-env-{key}")),
                    if shown {
                        IconName::EyeOff
                    } else {
                        IconName::Eye
                    },
                    format!("{} {key}", if shown { "Hide" } else { "Show" }),
                )
                .disabled(self.disabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.revealed.remove(&reveal) {
                        this.revealed.insert(reveal.clone());
                    }
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    SharedString::from(format!("remove-env-{key}")),
                    IconName::Close,
                    format!("Remove {key}"),
                )
                .disabled(self.disabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.values.remove(&remove);
                    this.revealed.remove(&remove);
                    cx.notify();
                })),
            )
    }
    fn add_form(&self, cx: &Context<Self>) -> Div {
        column()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(4.))
            .border_1()
            .border_color(t::border())
            .bg(t::input_bg())
            .child(input(&self.key).disabled(self.disabled).w_full())
            .child(
                input(&self.value)
                    .mask_toggle()
                    .disabled(self.disabled)
                    .w_full(),
            )
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_size(px(11.)).text_color(t::red()).child(e)),
            )
            .child(
                row()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("cancel-env", "Cancel")
                            .disabled(self.disabled)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.form.toggle(Instant::now(), cx);
                                this.error = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        primary_button("add-env", "Add")
                            .disabled(self.disabled)
                            .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
                    ),
            )
    }
}
impl Render for EnvironmentEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        motion::request_frame(window, self.form.active(now));
        let rows = self
            .values
            .iter()
            .map(|(key, value)| self.env_row(key, value, cx));
        let form = self.add_form(cx);
        column().gap(px(8.)).child(div().text_size(px(11.)).text_color(t::muted()).child("Extra environment variables for this profile. Values are stored in SQLite; use API key for credentials."))
            .child(column().gap(px(4.)).children(rows))
            .children((!self.form.open).then(||row().child(button("new-env","+ Add variable").disabled(self.disabled).on_click(cx.listener(|this,_,_,cx|{this.form.toggle(Instant::now(),cx);cx.notify();})))))
            .child(self.form.body(if self.error.is_some(){160.}else{132.},form,now))
    }
}
