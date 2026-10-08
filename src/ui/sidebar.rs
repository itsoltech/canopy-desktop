mod projects;
mod tasks;
mod tools;
use super::components::file_tree::FileTree;
use super::components::files::file_message;
use super::{components::*, theme as t};
use gpui_kit::component::{Icon, IconName, button::Button, scroll::ScrollableElement};
use gpui_kit::*;
use std::time::Instant;

pub struct NewFile;
impl EventEmitter<NewFile> for Sidebar {}
pub struct Sidebar {
    files: Entity<FileTree>,
    tasks: Entity<tasks::TasksSection>,
    scroll: ScrollHandle,
    git: Entity<crate::app_state::GitState>,
    tools: Entity<crate::app_state::ToolsState>,
    terminals: Entity<crate::app_state::Terminals>,
    profiles: std::collections::HashMap<String, Disclosure>,
    project_groups: std::collections::HashMap<std::path::PathBuf, Disclosure>,
    hovered_project_item: Option<String>,
    stop_feedback:
        std::collections::HashMap<canopy_desktop::state::workspace::WorkspaceId, ButtonLoading>,
    _tool_events: Vec<Subscription>,
    projects: Entity<crate::app_state::ProjectsState>,
    _projects_observer: Subscription,
    sections: [Disclosure; 3],
    actions_hovered: [bool; 3],
    hovered_tool_label: Option<String>,
    focus: FocusHandle,
}
impl Sidebar {
    pub fn new(files: Entity<FileTree>, cx: &mut Context<Self>) -> Self {
        let scroll = ScrollHandle::new();
        files.update(cx, |files, _| files.use_sidebar_scroll(scroll.clone()));
        let projects = cx.global::<crate::app_state::AppState>().projects.clone();
        let observer = cx.observe(&projects, |this, _, cx| this.sync_project_groups(cx));
        let app = cx.global::<crate::app_state::AppState>().clone();
        let tools = app.tools.clone();
        let terminals = app.terminals.clone();
        let tool_events = vec![
            cx.observe(&app.files, |_, _, cx| cx.notify()),
            cx.observe(&app.git, |_, _, cx| cx.notify()),
            cx.observe(&tools, |this, _, cx| this.sync_tools(cx)),
            cx.observe(&terminals, |this, _, cx| {
                this.sync_stop_feedback(cx);
                cx.notify();
            }),
        ];
        let profiles = tools
            .read(cx)
            .catalog
            .tools
            .iter()
            .map(|t| (t.id.clone(), Disclosure::new(false, Instant::now())))
            .collect();
        let now = Instant::now();
        Self {
            git: app.git.clone(),
            tools,
            terminals,
            project_groups: projects
                .read(cx)
                .catalog
                .items
                .iter()
                .map(|p| {
                    (
                        p.repository_path.as_ref().unwrap_or(&p.path).clone(),
                        Disclosure::new(true, now),
                    )
                })
                .collect(),
            hovered_project_item: None,
            stop_feedback: Default::default(),
            profiles,
            _tool_events: tool_events,
            files,
            tasks: cx.new(tasks::TasksSection::new),
            scroll,
            projects,
            _projects_observer: observer,
            sections: std::array::from_fn(|_| Disclosure::new(true, now)),
            actions_hovered: [false; 3],
            hovered_tool_label: None,
            focus: cx.focus_handle(),
        }
    }
    fn action(&self, index: usize, cx: &mut Context<Self>) -> Button {
        let (id, label) = [
            ("attach-project", "Attach project"),
            ("new-file", "New file"),
            ("refresh-files", "Refresh files"),
        ][index];
        let button = quiet_button(id, label, self.actions_hovered[index], cx);
        let button = if index == 0 {
            button.child(div().text_size(px(10.)).child("+ attach"))
        } else {
            button.w(px(20.)).icon(Icon::new(if index == 1 {
                IconName::Plus
            } else {
                IconName::RotateCw
            }))
        };
        let button = button.on_click(cx.listener(move |this, _, window, cx| {
            if index == 1 {
                cx.emit(NewFile);
            }
            if index == 2 {
                cx.global::<crate::app_state::AppState>()
                    .files
                    .clone()
                    .update(cx, |files, cx| files.refresh(cx));
            }
            if index == 0 {
                this.projects
                    .update(cx, |state, cx| state.open_folder(window, cx));
            }
        }));
        button.on_hover(cx.listener(move |this, hovered, _, cx| {
            if this.actions_hovered[index] != *hovered {
                this.actions_hovered[index] = *hovered;
                cx.notify();
            }
        }))
    }
    fn header(
        &self,
        index: usize,
        label: &'static str,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.sections[index]
            .header(label, label, now, cx)
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if this.sections[index].hovered != *hovered {
                    this.sections[index].hovered = *hovered;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus.focus(window, cx);
                this.sections[index].toggle(Instant::now(), cx);
                cx.notify();
            }))
    }
}
impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let tree_height = self.files.read(cx).animated_height(now);
        canopy_desktop::motion::request_frame(
            window,
            self.sections.iter().any(|s| s.active(now))
                || self.project_groups.values().any(|s| s.active(now))
                || self.profiles.values().any(|d| d.active(now))
                || (self.sections[1].open && self.files.read(cx).is_animating(now)),
        );
        let (projects, projects_height) = self.projects_content(now, cx);
        let (tools, tools_height) = self.tools_content(now, cx);
        column()
            .size_full()
            .track_focus(&self.focus)
            .bg(t::sidebar())
            .border_r_1()
            .border_color(t::border())
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        column()
                            .id("sidebar-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(
                                column()
                                    .flex_shrink_0()
                                    .px(px(12.))
                                    .pt(px(8.))
                                    .pb(px(14.))
                                    .child(
                                        row()
                                            .h(px(36.))
                                            .flex_shrink_0()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(self.header(0, "PROJECTS", now, cx)),
                                            )
                                            .child(self.action(0, cx)),
                                    )
                                    .child(self.sections[0].body(projects_height, projects, now)),
                            )
                            .child(self.tasks.clone())
                            .child(
                                column()
                                    .flex_shrink_0()
                                    .border_t_1()
                                    .border_color(t::border())
                                    .px(px(12.))
                                    .py(px(10.))
                                    .child(
                                        row()
                                            .h(px(36.))
                                            .flex_shrink_0()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(self.header(1, "FILES", now, cx)),
                                            )
                                            .child(
                                                row()
                                                    .gap(px(t::SPACING_UNIT))
                                                    .child(self.action(1, cx))
                                                    .child(self.action(2, cx)),
                                            ),
                                    )
                                    .child(self.sections[1].body(
                                        tree_height,
                                        self.files.clone(),
                                        now,
                                    ))
                                    .children(
                                        cx.global::<crate::app_state::AppState>()
                                            .files
                                            .read(cx)
                                            .error
                                            .clone()
                                            .or(cx
                                                .global::<crate::app_state::AppState>()
                                                .files
                                                .read(cx)
                                                .index
                                                .warning
                                                .clone())
                                            .map(|error| {
                                                file_message(error, t::red())
                                                    .px(px(12.))
                                                    .text_size(px(11.))
                                            }),
                                    )
                                    .children(
                                        (cx.global::<crate::app_state::AppState>()
                                            .files
                                            .read(cx)
                                            .loading)
                                            .then(|| {
                                                file_message("Loading files…", t::muted())
                                                    .px(px(12.))
                                            }),
                                    ),
                            )
                            .child(
                                column()
                                    .flex_shrink_0()
                                    .border_t_1()
                                    .border_color(t::border())
                                    .px(px(12.))
                                    .py(px(10.))
                                    .child(self.header(2, "TOOLS", now, cx))
                                    .child(self.sections[2].body(tools_height, tools, now)),
                            ),
                    )
                    .vertical_scrollbar(&self.scroll),
            )
            .child(
                row()
                    .h(px(28.))
                    .flex_shrink_0()
                    .px(px(12.))
                    .border_t_1()
                    .border_color(t::border())
                    .text_size(px(10.))
                    .text_color(t::faint())
                    .child("V0.13.0-NEXT.28"),
            )
    }
}
