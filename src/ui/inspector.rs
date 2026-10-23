use super::components::file_tree::FileTree;
use super::{components::*, theme as t};
use canopy_desktop::state::workspace::ToolPage;
use gpui_kit::component::scroll::ScrollableElement;
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
    files: Entity<FileTree>,
    files_scroll: ScrollHandle,
    tabs: InspectorTransition,
}
fn page(cx: &App) -> usize {
    match cx
        .global::<crate::app_state::AppState>()
        .workspace
        .read(cx)
        .tool_panel()
        .page()
    {
        ToolPage::Changes => 1,
        ToolPage::Tasks => 2,
        ToolPage::Files => 3,
        _ => 0,
    }
}
impl Inspector {
    pub fn new(files: Entity<FileTree>, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        let current = page(cx);
        let files_scroll = ScrollHandle::new();
        files.update(cx, |files, _| {
            files.use_sidebar_scroll(files_scroll.clone())
        });
        let observers = vec![
            cx.observe(&app.changes, |_, _, cx| cx.notify()),
            cx.observe(&app.files, |_, _, cx| cx.notify()),
            cx.observe(&app.projects, |this, _, cx| {
                this.sync_context(cx);
                cx.notify();
            }),
            cx.observe(&app.workspace, |this, _, cx| {
                this.sync_context(cx);
                cx.notify();
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
            files,
            files_scroll,
            tabs: InspectorTransition::new(current, Instant::now()),
        }
    }
    pub fn select(&mut self, changes: bool, cx: &mut Context<Self>) {
        self.select_page(usize::from(changes), cx);
    }
    fn sync_context(&mut self, cx: &mut Context<Self>) {
        let next = page(cx);
        if next != self.tabs.selected() {
            self.animate(next, cx);
        }
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
        if page == 1
            && cx
                .global::<crate::app_state::AppState>()
                .projects
                .read(cx)
                .catalog
                .active
                .is_none()
        {
            return;
        }
        cx.global::<crate::app_state::AppState>()
            .workspace
            .clone()
            .update(cx, |workspace, cx| {
                workspace.show_tools(match page {
                    1 => ToolPage::Changes,
                    2 => ToolPage::Tasks,
                    3 => ToolPage::Files,
                    _ => ToolPage::Agent,
                });
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
        let has_project = cx
            .global::<crate::app_state::AppState>()
            .projects
            .read(cx)
            .catalog
            .active
            .is_some();
        motion::request_frame(window, self.tabs.is_animating(now));
        column().size_full().bg(t::sidebar()).child(
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
                        self.files_panel(cx).into_any_element(),
                    ]
                    .into_iter()
                    .enumerate()
                    .filter_map(|(page, view)| {
                        if page == 1 && !has_project {
                            return None;
                        }
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

impl Inspector {
    fn files_panel(&self, cx: &App) -> Div {
        use gpui_kit::component::{Sizable, button::Button, button::ButtonVariants};
        let app = cx.global::<crate::app_state::AppState>();
        let files = app.files.read(cx);
        column()
            .size_full()
            .child(
                row()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .child(div().flex_1().text_sm().child("Files"))
                    .child(
                        Button::new("files-new")
                            .ghost()
                            .small()
                            .icon(gpui_kit::component::IconName::Plus)
                            .tooltip("New file…")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(super::NewFile), cx)
                            }),
                    )
                    .child(
                        Button::new("files-refresh")
                            .ghost()
                            .small()
                            .icon(gpui_kit::component::IconName::RotateCw)
                            .tooltip("Refresh files")
                            .on_click(|_, _, cx| {
                                cx.global::<crate::app_state::AppState>()
                                    .files
                                    .clone()
                                    .update(cx, |f, cx| f.refresh(cx))
                            }),
                    ),
            )
            .children(
                files
                    .error
                    .clone()
                    .or(files.index.warning.clone())
                    .map(|e| div().px_3().text_color(t::red()).child(e)),
            )
            .children(
                files
                    .root
                    .is_none()
                    .then(|| div().px_3().child("Select a project to browse files.")),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        column()
                            .id("inspector-files-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.files_scroll)
                            .child(self.files.clone()),
                    )
                    .vertical_scrollbar(&self.files_scroll),
            )
    }
}
