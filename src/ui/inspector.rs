use super::{components::*, theme as t};
mod transition;
use canopy_desktop::motion;
use gpui_kit::*;
use std::time::Instant;
use transition::InspectorTransition;
impl EventEmitter<super::changes_panel::DiscardRequest> for Inspector {}
impl EventEmitter<super::tasks_panel::TaskWorktreeRequest> for Inspector {}
impl EventEmitter<super::task_edit::TaskEditRequest> for Inspector {}
impl EventEmitter<super::task_detail::TaskDetailRequest> for Inspector {}
pub struct Inspector {
    changes_view: Entity<super::changes_panel::ChangesPanel>,
    session_view: Entity<super::session_inspector::SessionInspector>,
    tasks_view: Entity<super::tasks_panel::TasksPanel>,
    _changes_events: Subscription,
    _task_events: Subscription,
    _detail_events: Subscription,
    _edit_events: Subscription,
    _observers: Vec<Subscription>,
    layout: Entity<canopy_desktop::state::layout::LayoutState>,
    tabs: InspectorTransition,
}
fn page(layout: &canopy_desktop::state::layout::LayoutState) -> usize {
    if layout.inspector_tasks {
        2
    } else if layout.inspector_changes {
        1
    } else {
        0
    }
}
impl Inspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app = cx.global::<crate::app_state::AppState>().clone();
        let changes_view = cx.new(|cx| super::changes_panel::ChangesPanel::new(window, cx));
        let tasks_view = cx.new(|cx| super::tasks_panel::TasksPanel::new(window, cx));
        let changes_events = cx.subscribe(
            &changes_view,
            |_, _, event: &super::changes_panel::DiscardRequest, cx| cx.emit(event.clone()),
        );
        let task_events = cx.subscribe(
            &tasks_view,
            |_, _, event: &super::tasks_panel::TaskWorktreeRequest, cx| cx.emit(event.clone()),
        );
        let detail_events = cx.subscribe(
            &tasks_view,
            |_, _, event: &super::task_detail::TaskDetailRequest, cx| cx.emit(event.clone()),
        );
        let edit_events = cx.subscribe(
            &tasks_view,
            |_, _, event: &super::task_edit::TaskEditRequest, cx| cx.emit(event.clone()),
        );
        let current = page(app.layout.read(cx));
        let observers = vec![
            cx.observe(&app.changes, |_, _, cx| cx.notify()),
            cx.observe(&app.projects, |_, _, cx| cx.notify()),
            cx.observe(&app.layout, |this, layout, cx| {
                let next = page(layout.read(cx));
                if next != this.tabs.selected() {
                    this.animate(next, cx);
                }
            }),
        ];
        Self {
            changes_view,
            tasks_view,
            session_view: cx.new(super::session_inspector::SessionInspector::new),
            _changes_events: changes_events,
            _task_events: task_events,
            _detail_events: detail_events,
            _edit_events: edit_events,
            _observers: observers,
            layout: app.layout,
            tabs: InspectorTransition::new(current, Instant::now()),
        }
    }
    pub fn select(&mut self, changes: bool, cx: &mut Context<Self>) {
        self.select_page(usize::from(changes), cx);
    }
    pub fn focus_sessions(
        &mut self,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_view
            .update(cx, |view, cx| view.focus_sessions(return_focus, window, cx));
    }
    #[cfg(test)]
    pub fn session_focus_handle(&self, cx: &App) -> FocusHandle {
        self.session_view.read(cx).focus_handle()
    }
    fn select_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.layout.update(cx, |layout, cx| {
            layout.inspector_changes = page == 1;
            layout.inspector_tasks = page == 2;
            cx.notify();
        });
        self.animate(page, cx);
    }
    fn animate(&mut self, page: usize, cx: &mut Context<Self>) {
        self.tabs.select(page, Instant::now(), motion::policy(cx));
        cx.notify();
    }
}
impl Render for Inspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.tabs.indicator(now);
        motion::request_frame(window, self.tabs.is_animating(now));
        column()
            .size_full()
            .bg(t::sidebar())
            .border_l_1()
            .border_color(t::border())
            .px(px(12.))
            .child(
                row()
                    .relative()
                    .h(px(38.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(t::border())
                    .children(["Session", "Changes", "Tasks"].into_iter().enumerate().map(
                        |(page, label)| {
                            row()
                                .id(label)
                                .flex_1()
                                .h_full()
                                .justify_center()
                                .cursor_pointer()
                                .text_color(if self.tabs.selected() == page {
                                    t::text()
                                } else {
                                    t::faint()
                                })
                                .child(label)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.select_page(page, cx)),
                                )
                        },
                    ))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left(relative(progress / 3.))
                            .w(relative(1. / 3.))
                            .h(px(2.))
                            .bg(t::accent()),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .children(
                        [
                            self.session_view.clone().into_any_element(),
                            self.changes_view.clone().into_any_element(),
                            self.tasks_view.clone().into_any_element(),
                        ]
                        .into_iter()
                        .enumerate()
                        .filter_map(|(page, view)| {
                            let opacity = self.tabs.opacity(page, now);
                            (opacity > 0.).then(|| {
                                div()
                                    .id(("inspector-page", page))
                                    .absolute()
                                    .inset_0()
                                    .w_full()
                                    .opacity(opacity)
                                    .child(view)
                            })
                        }),
                    )
                    // A fading page must not receive clicks or scroll through
                    // the incoming page. The tab header remains interactive.
                    .children(self.tabs.content_is_animating(now).then(|| {
                        div()
                            .id("inspector-transition-shield")
                            .absolute()
                            .inset_0()
                            .occlude()
                    })),
            )
    }
}
