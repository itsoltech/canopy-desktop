use crate::{
    app_state::AppState,
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::integrations::{
    OptionKind, ProjectTarget, Provider, TaskOption, client, credentials,
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName,
        input::{InputEvent, InputState},
    },
    *,
};
#[derive(Clone)]
pub struct OptionChanged {
    pub value: String,
    pub selected: bool,
}
impl EventEmitter<OptionChanged> for OptionPicker {}
pub struct OptionPicker {
    project: ProjectTarget,
    kind: OptionKind,
    options: Vec<TaskOption>,
    selected: Vec<String>,
    filter: Entity<InputState>,
    open: bool,
    loading: bool,
    more_loading: ButtonLoading,
    loaded: bool,
    enabled: bool,
    next: Option<String>,
    error: Option<String>,
    read: Option<Task<()>>,
    _filter: Subscription,
}
impl OptionPicker {
    pub fn new(
        project: ProjectTarget,
        kind: OptionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter available values…"));
        let observer = cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify());
        Self {
            project,
            kind,
            options: vec![],
            selected: vec![],
            filter,
            open: false,
            loading: false,
            more_loading: ButtonLoading::default(),
            loaded: false,
            enabled: true,
            next: None,
            error: None,
            read: None,
            _filter: observer,
        }
    }
    pub fn seed_current(&mut self, option: TaskOption, cx: &mut Context<Self>) {
        if let Some(old) = self.options.iter_mut().find(|o| o.value == option.value) {
            *old = option;
        } else {
            self.options.push(option);
        }
        cx.notify();
    }
    pub fn selected(&self) -> Vec<String> {
        self.selected.clone()
    }
    pub fn set_selected(&mut self, selected: Vec<String>, cx: &mut Context<Self>) {
        self.selected = selected;
        cx.notify();
    }
    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.enabled != enabled {
            self.enabled = enabled;
            cx.notify();
        }
    }
    fn load(&mut self, more: bool, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(account) = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .config
            .account_for(&self.project)
            .cloned()
        else {
            self.error = Some("Connect this provider in Preferences to load values.".into());
            cx.notify();
            return;
        };
        let project = self.project.clone();
        let kind = self.kind;
        let cursor = if more { self.next.clone() } else { None };
        let http = cx.http_client();
        self.loading = true;
        self.more_loading.set(more, cx);
        self.error = None;
        cx.notify();
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let token = cx
                    .background_executor()
                    .spawn({
                        let credential = account.credential.clone();
                        async move { credentials::load(&credential) }
                    })
                    .await?;
                client(http, &account, token)?
                    .options(&project, kind, cursor.as_deref())
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                this.more_loading.set(false, cx);
                match result {
                    Ok(page) => {
                        this.loaded = true;
                        if !more {
                            // Keep seeded current values even when the first
                            // provider page does not contain them anymore.
                            this.options.retain(|o| this.selected.contains(&o.value));
                        }
                        for option in page.items {
                            if !this.options.iter().any(|o| o.value == option.value) {
                                this.options.push(option);
                            }
                        }
                        this.next = page.next_cursor;
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
    }
    fn toggle(&mut self, value: String, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        let selected = !self.selected.contains(&value);
        if selected {
            if self.kind == OptionKind::Milestone {
                self.selected.clear();
                self.open = false;
            }
            self.selected.push(value.clone());
        } else {
            self.selected.retain(|v| v != &value);
        }
        cx.emit(OptionChanged { value, selected });
        cx.notify();
    }
}
impl Render for OptionPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = match (self.project.provider, self.kind) {
            (Provider::Youtrack, OptionKind::Label) => "Tags",
            (_, OptionKind::Label) => "Labels",
            (_, OptionKind::Assignee) => "Assignees",
            (_, OptionKind::Milestone) => "Milestone",
        };
        let query = self.filter.read(cx).value().to_lowercase();
        let choices: Vec<_> = self
            .options
            .iter()
            .filter(|o| o.label.to_lowercase().contains(&query))
            .cloned()
            .collect();
        column()
            .gap(px(6.))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t::secondary())
                            .child(label),
                    )
                    .child(
                        icon_button(
                            label,
                            if self.open {
                                IconName::ChevronUp
                            } else {
                                IconName::Plus
                            },
                            format!("Choose {label}"),
                        )
                        .disabled(!self.enabled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open = !this.open;
                            if this.open && !this.loaded {
                                this.load(false, cx);
                            }
                            cx.notify();
                        })),
                    ),
            )
            .child(
                row()
                    .flex_wrap()
                    .gap(px(4.))
                    .children(self.selected.iter().map(|value| {
                        let value = value.clone();
                        let label = self
                            .options
                            .iter()
                            .find(|o| o.value == value)
                            .map(|o| o.label.clone())
                            .unwrap_or_else(|| value.clone());
                        choice_chip(
                            SharedString::from(format!("selected-{value}")),
                            format!("{label} ×"),
                        )
                        .disabled(!self.enabled)
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle(value.clone(), cx)))
                    }))
                    .children(self.selected.is_empty().then(|| {
                        div()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child("None")
                    })),
            )
            .children(self.open.then(|| {
                column()
                    .gap(px(6.))
                    .p(px(8.))
                    .rounded(px(4.))
                    .bg(t::hover())
                    .child(input(&self.filter).w_full().disabled(!self.enabled))
                    .child(if choices.is_empty() {
                        if self.loading {
                            div()
                                .h(px(64.))
                                .flex()
                                .items_center()
                                .text_color(t::muted())
                                .child("Loading available values…")
                        } else if self.error.is_some() {
                            div()
                                .h(px(64.))
                                .flex()
                                .items_center()
                                .text_color(t::muted())
                                .child("Values could not be loaded. Retry below.")
                        } else {
                            div()
                                .h(px(64.))
                                .flex()
                                .items_center()
                                .text_color(t::muted())
                                .child(if query.is_empty() {
                                    "No values are available."
                                } else {
                                    "No values match this search."
                                })
                        }
                        .into_any_element()
                    } else {
                        uniform_list(
                            label,
                            choices.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|i| {
                                        let choice = choices[i].clone();
                                        selection_button(
                                            SharedString::from(format!("choice-{}", choice.value)),
                                            choice.label,
                                            this.selected.contains(&choice.value),
                                        )
                                        .w_full()
                                        .h(px(28.))
                                        .disabled(!this.enabled)
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.toggle(choice.value.clone(), cx)
                                            }),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .h(px(140.))
                        .into_any_element()
                    })
                    .children(self.error.clone().map(|e| integration_message(e, true)))
                    .children(self.error.as_ref().map(|_| {
                        button("retry-options", "Retry")
                            .disabled(!self.enabled || self.loading)
                            .on_click(cx.listener(|this, _, _, cx| this.load(false, cx)))
                    }))
                    .children((self.loading).then(|| {
                        div()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child("Loading…")
                    }))
                    .children(self.next.as_ref().map(|_| {
                        loading_button("more-options", "Load more values", &self.more_loading)
                            .disabled(
                                (self.loading || !self.enabled) && !self.more_loading.active(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.load(true, cx)))
                    }))
            }))
    }
}
