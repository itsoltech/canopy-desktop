use crate::{
    app_state::{AppState, IntegrationsState},
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::{
    integrations::{
        Provider,
        filters::{BUILTINS, TaskFilter, YOUTRACK_BUILTINS},
    },
    motion::{self, Presence, presets},
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName,
        input::{InputState, TextareaState},
        scroll::ScrollableElement,
        select::SelectState,
    },
    *,
};
use std::time::Instant;
pub struct TaskFiltersPreferences {
    state: Entity<IntegrationsState>,
    scroll: ScrollHandle,
    name: Entity<InputState>,
    expression: Entity<TextareaState>,
    provider: Entity<SelectState<Vec<SelectOption>>>,
    items: Vec<TaskFilter>,
    editing: Option<String>,
    open: bool,
    error: Option<String>,
    pending: bool,
    save_loading: ButtonLoading,
    delete_loading: ButtonLoading,
    confirm: Option<String>,
    reveal: Presence,
    _observer: Subscription,
    _settings_observer: Subscription,
}
impl TaskFiltersPreferences {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let observer = cx.observe_in(&state, window, |s, state, window, cx| {
            let state = state.read(cx);
            s.items = state.config.task_filters.clone();
            if !state.busy {
                s.save_loading.set(false, cx);
                if s.delete_loading.active() {
                    s.delete_loading.set(false, cx);
                    s.error = state.error.clone();
                    if s.error.is_none() {
                        s.confirm = None;
                    }
                }
            }
            if s.pending && !state.busy {
                s.pending = false;
                s.error = state.error.clone();
                if s.error.is_none() {
                    s.close(cx);
                    s.name.update(cx, |f, cx| f.set_value("", window, cx));
                    s.expression.update(cx, |f, cx| f.set_value("", window, cx));
                }
            }
            cx.notify();
        });
        Self {
            items: state.read(cx).config.task_filters.clone(),
            state,
            scroll: ScrollHandle::new(),
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Filter name")),
            expression: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("assignee is EMPTY AND sprint in openSprints()")
            }),
            provider: cx.new(|cx| {
                SelectState::new(
                    vec![
                        SelectOption::new("jira", "Jira"),
                        SelectOption::new("youtrack", "YouTrack"),
                    ],
                    None,
                    window,
                    cx,
                )
            }),
            editing: None,
            open: false,
            error: None,
            pending: false,
            save_loading: ButtonLoading::default(),
            delete_loading: ButtonLoading::default(),
            confirm: None,
            reveal: Presence::new(false, presets::CONTENT_REVEAL, Instant::now()),
            _observer: observer,
            _settings_observer: cx
                .observe(&cx.global::<AppState>().settings.clone(), |_, _, cx| {
                    cx.notify()
                }),
        }
    }
    fn edit(
        &mut self,
        id: Option<String>,
        name: String,
        expression: String,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing = id;
        self.name.update(cx, |f, cx| f.set_value(name, window, cx));
        self.expression
            .update(cx, |f, cx| f.set_value(expression, window, cx));
        self.provider.update(cx, |s, cx| {
            let selected = if provider == Provider::Youtrack
                || self
                    .items
                    .iter()
                    .find(|filter| Some(filter.id.clone()) == self.editing)
                    .is_some_and(|filter| filter.provider == Provider::Youtrack)
            {
                "youtrack"
            } else {
                "jira"
            };
            s.set_selected_value(&selected.into(), window, cx)
        });
        self.error = None;
        self.open = true;
        self.scroll.scroll_to_bottom();
        self.reveal = Presence::new(false, presets::CONTENT_REVEAL, Instant::now());
        self.reveal
            .set_open(true, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.reveal
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        let filter = TaskFilter {
            id: self
                .editing
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name: self.name.read(cx).value().trim().into(),
            provider: if self
                .provider
                .read(cx)
                .selected_value()
                .is_some_and(|value| value.as_ref() == "youtrack")
            {
                Provider::Youtrack
            } else {
                Provider::Jira
            },
            expression: self.expression.read(cx).value().trim().into(),
        };
        self.error = self
            .state
            .update(cx, |s, cx| s.save_task_filter(filter, cx))
            .err();
        self.pending = self.error.is_none();
        self.save_loading
            .set(self.pending && self.state.read(cx).busy, cx);
        cx.notify();
    }
}
impl TaskFiltersPreferences {
    fn builtins(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        column()
            .flex_shrink_0()
            .gap(px(10.))
            .child(caption("BUILT-IN · JIRA"))
            .children(BUILTINS.into_iter().map(|(id, name, expression)| {
                let summary = if expression.is_empty() {
                    "Every task in the selected project"
                } else {
                    expression
                };
                row()
                    .gap(px(12.))
                    .py(px(6.))
                    .flex_shrink_0()
                    .child(
                        column()
                            .flex_1()
                            .gap(px(3.))
                            .child(div().child(name))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child(summary),
                            ),
                    )
                    .child(
                        icon_button(id, IconName::Plus, "Use as a custom filter")
                            .disabled(disabled)
                            .on_click(cx.listener(move |s, _, w, cx| {
                                s.edit(
                                    None,
                                    format!("{name} copy"),
                                    expression.into(),
                                    Provider::Jira,
                                    w,
                                    cx,
                                )
                            })),
                    )
            }))
            .child(caption("BUILT-IN · YOUTRACK"))
            .children(YOUTRACK_BUILTINS.into_iter().map(|(id, name, expression)| {
                row()
                    .gap(px(12.))
                    .py(px(6.))
                    .flex_shrink_0()
                    .child(
                        column()
                            .flex_1()
                            .gap(px(3.))
                            .child(div().child(name))
                            .child(div().text_size(px(11.)).text_color(t::muted()).child(
                                if expression.is_empty() {
                                    "Every task in the selected project"
                                } else {
                                    expression
                                },
                            )),
                    )
                    .child(
                        icon_button(id, IconName::Plus, "Use as a custom filter")
                            .disabled(disabled)
                            .on_click(cx.listener(move |s, _, w, cx| {
                                s.edit(
                                    None,
                                    format!("{name} copy"),
                                    expression.into(),
                                    Provider::Youtrack,
                                    w,
                                    cx,
                                )
                            })),
                    )
            }))
    }
    fn custom(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        let add = button("new-task-filter", "New filter")
            .icon(icon(IconName::Plus))
            .disabled(disabled)
            .on_click(cx.listener(|s, _, w, cx| {
                s.edit(None, String::new(), String::new(), Provider::Jira, w, cx)
            }));
        column()
            .flex_shrink_0()
            .gap(px(10.))
            .child(row().child(caption("CUSTOM · JIRA").flex_1()).child(add))
            .children(self.items.iter().map(|filter| {
                let name = filter.name.clone();
                let expression = filter.expression.clone();
                let filter = filter.clone();
                let delete = filter.id.clone();
                let edit = icon_button(
                    SharedString::from(format!("edit-filter-{}", filter.id)),
                    IconName::Settings2,
                    "Edit filter",
                )
                .disabled(disabled)
                .on_click(cx.listener(move |s, _, w, cx| {
                    s.edit(
                        Some(filter.id.clone()),
                        filter.name.clone(),
                        filter.expression.clone(),
                        filter.provider,
                        w,
                        cx,
                    )
                }));
                let remove = icon_button(
                    SharedString::from(format!("delete-filter-{delete}")),
                    IconName::Close,
                    "Delete filter",
                )
                .disabled(disabled)
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.confirm = Some(delete.clone());
                    cx.notify();
                }));
                row()
                    .gap(px(8.))
                    .py(px(8.))
                    .border_b_1()
                    .border_color(t::border())
                    .flex_shrink_0()
                    .child(
                        column()
                            .flex_1()
                            .gap(px(3.))
                            .child(div().child(name))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .line_clamp(2)
                                    .child(expression),
                            ),
                    )
                    .child(edit)
                    .child(remove)
            }))
            .children(self.items.is_empty().then(|| {
                div()
                    .text_color(t::muted())
                    .child("Your saved filters will appear here and in Tasks.")
            }))
    }
    fn editor(&self, disabled: bool, now: Instant, cx: &mut Context<Self>) -> Div {
        column().flex_shrink_0().gap(px(16.)).p(px(16.)).rounded(px(6.)).bg(t::hover()).opacity(self.reveal.progress(now))
            .child(form_field("Name","",input(&self.name).w_full().disabled(disabled||!self.open)))
            .child(form_field("Provider","The selected project remains the outer scope for this provider.",dropdown(&self.provider).w_full().disabled(disabled||!self.open)))
            .child(form_field("Query","The selected project is always applied. Leave blank for all tasks in the project.",textarea(&self.expression).w_full().h(px(120.)).disabled(disabled||!self.open)))
            .child(div().text_size(px(11.)).text_color(t::muted()).child("Jira uses JQL. YouTrack uses its native query language and built-in #Resolved / #Unresolved filters."))
            .child(row().gap(px(8.)).justify_end()
                .child(button("cancel-filter-edit","Cancel").disabled(disabled||!self.open).on_click(cx.listener(|s,_,_,cx|s.close(cx))))
                .child(primary_loading_button("save-task-filter","Save filter", &self.save_loading).disabled((disabled||!self.open) && !self.save_loading.active()).on_click(cx.listener(|s,_,_,cx|s.save(cx)))))
    }
}
impl Render for TaskFiltersPreferences {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let moving = self.reveal.is_animating(now);
        motion::request_frame(window, moving);
        let disabled = self.state.read(cx).busy
            || !self.state.read(cx).ready
            || cx.global::<AppState>().settings.read(cx).quitting;
        let heading = column()
            .flex_shrink_0()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::MEDIUM)
                    .child("Task filters"),
            )
            .child(
                div()
                    .text_color(t::secondary())
                    .child("Create reusable views for the Tasks dropdown."),
            );
        let content = column()
            .id("task-filter-preferences")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(px(28.))
            .gap(px(24.))
            .child(heading)
            .child(self.builtins(disabled, cx))
            .child(self.custom(disabled, cx))
            .children(self.confirm.clone().map(|id| {
                row()
                    .gap(px(8.))
                    .flex_shrink_0()
                    .child(div().flex_1().child(
                        "Delete this filter? Projects using it will return to Active tasks.",
                    ))
                    .child(
                        button("cancel-filter-delete", "Cancel")
                            .disabled(self.delete_loading.active())
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.confirm = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        loading_button("confirm-filter-delete", "Delete", &self.delete_loading)
                            .text_color(t::red())
                            .disabled(disabled && !self.delete_loading.active())
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.error = s
                                    .state
                                    .update(cx, |state, cx| state.delete_task_filter(&id, cx))
                                    .err();
                                s.delete_loading
                                    .set(s.error.is_none() && s.state.read(cx).busy, cx);
                                cx.notify();
                            })),
                    )
            }))
            .children((self.open || moving).then(|| self.editor(disabled, now, cx)))
            .children(
                self.error
                    .clone()
                    .or_else(|| self.state.read(cx).error.clone())
                    .map(|e| integration_message(e, true).flex_shrink_0()),
            );
        div()
            .relative()
            .size_full()
            .child(content)
            .vertical_scrollbar(&self.scroll)
    }
}
