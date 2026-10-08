use crate::{
    app_state::{AppState, IntegrationsState},
    ui::{components::*, theme as t},
};
use gpui_kit::{component::IconName, *};
use std::time::Instant;
pub struct TasksSection {
    state: Entity<IntegrationsState>,
    disclosure: Disclosure,
    _observer: Subscription,
}
impl TasksSection {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().integrations.clone();
        let observer = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            state,
            disclosure: Disclosure::new(true, Instant::now()),
            _observer: observer,
        }
    }
    fn open(cx: &mut App) {
        cx.global::<AppState>()
            .layout
            .clone()
            .update(cx, |layout, cx| {
                layout.inspector_open = true;
                layout.inspector_tasks = true;
                layout.inspector_changes = false;
                cx.notify();
            });
    }
}
impl Render for TasksSection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        canopy_desktop::motion::request_frame(window, self.disclosure.active(now));
        let state = self.state.read(cx);
        let tasks = state
            .path
            .as_ref()
            .and_then(|p| state.config.links.get(p))
            .cloned()
            .unwrap_or_default();
        let body = column()
            .children(tasks.is_empty().then(|| {
                row()
                    .h(px(28.))
                    .px(px(4.))
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child("No linked tasks")
            }))
            .children(tasks.iter().map(|task| {
                let linked = task.clone();
                row()
                    .id(SharedString::from(format!(
                        "linked-{}-{}",
                        task.project.key, task.id
                    )))
                    .h(px(28.))
                    .cursor_pointer()
                    .px(px(4.))
                    .rounded(px(4.))
                    .hover(|style| style.bg(t::hover()))
                    .text_size(px(12.))
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(t::MONO)
                            .text_size(px(10.))
                            .text_color(t::muted())
                            .child(task.label()),
                    )
                    .child(div().truncate().child(task.title.clone()))
                    .on_click(move |_, _, cx| {
                        Self::open(cx);
                        cx.global::<AppState>()
                            .integrations
                            .clone()
                            .update(cx, |state, cx| state.open_linked(linked.clone(), cx));
                    })
            }))
            .child(
                button(
                    "browse-tasks",
                    if state.config.github_account().is_some() {
                        "Browse issues"
                    } else {
                        "Configure integrations"
                    },
                )
                .border_0()
                .bg(rgba(0))
                .icon(icon(IconName::Inbox))
                .on_click({
                    let connected = state.config.github_account().is_some();
                    move |_, _, cx| {
                        if connected {
                            Self::open(cx);
                        } else {
                            let _ = crate::ui::preferences::open_integrations(cx);
                        }
                    }
                }),
            );
        column()
            .flex_shrink_0()
            .px(px(12.))
            .py(px(10.))
            .border_t_1()
            .border_color(t::border())
            .child(
                self.disclosure
                    .header("tasks-section", "TASKS", now, cx)
                    .h(px(36.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.disclosure.toggle(Instant::now(), cx);
                        cx.notify();
                    })),
            )
            .child(self.disclosure.body(
                (tasks.len() as f32 + 1. + if tasks.is_empty() { 1. } else { 0. }) * 28.,
                body,
                now,
            ))
    }
}
