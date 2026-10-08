use super::{
    github_metadata,
    status::{StatusFormRequested, TaskStatus},
};
use crate::ui::components::column;
use crate::ui::jira::panel::{JiraAction, JiraPanel, RelatedRequested};
use crate::ui::youtrack::panel::{YoutrackEditRequested, YoutrackPanel};
use canopy_desktop::integrations::{Provider, TaskItem};
use gpui_kit::*;
enum Content {
    Github(Entity<github_metadata::MetadataEditor>),
    Jira(Entity<JiraPanel>),
    Youtrack(Entity<YoutrackPanel>),
}
pub struct MetadataEditor {
    content: Content,
    status: Entity<TaskStatus>,
    _events: Vec<Subscription>,
}
impl EventEmitter<JiraAction> for MetadataEditor {}
impl EventEmitter<StatusFormRequested> for MetadataEditor {}
impl EventEmitter<RelatedRequested> for MetadataEditor {}
impl EventEmitter<YoutrackEditRequested> for MetadataEditor {}
impl MetadataEditor {
    pub fn new(
        task: TaskItem,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let status = cx.new(|cx| TaskStatus::new(&task, path.clone(), window, cx));
        let mut events = vec![
            cx.subscribe(&status, |_, _, event: &StatusFormRequested, cx| {
                cx.emit(event.clone())
            }),
        ];
        let content = if task.reference.project.provider == Provider::Jira {
            let panel = cx.new(|cx| JiraPanel::new(task, path, cx));
            events.push(cx.subscribe(&panel, |_, _, event: &JiraAction, cx| {
                cx.emit(event.clone())
            }));
            events.push(cx.subscribe(&panel, |_, _, e: &RelatedRequested, cx| cx.emit(e.clone())));
            Content::Jira(panel)
        } else if task.reference.project.provider == Provider::Youtrack {
            let panel = cx.new(|cx| YoutrackPanel::new(task, path, window, cx));
            events.push(
                cx.subscribe(&panel, |_, _, event: &YoutrackEditRequested, cx| {
                    cx.emit(event.clone())
                }),
            );
            Content::Youtrack(panel)
        } else {
            Content::Github(cx.new(|cx| github_metadata::MetadataEditor::new(task, window, cx)))
        };
        Self {
            content,
            status,
            _events: events,
        }
    }
    pub fn set_task(&mut self, task: TaskItem, cx: &mut Context<Self>) {
        match &self.content {
            Content::Github(v) => v.update(cx, |s, cx| s.set_task(task, cx)),
            Content::Jira(v) => v.update(cx, |s, cx| s.set_task(task, cx)),
            Content::Youtrack(v) => v.update(cx, |s, cx| s.set_task(task, cx)),
        }
    }
    pub fn enable(&mut self, value: bool, cx: &mut Context<Self>) {
        self.status
            .update(cx, |status, cx| status.enable(value, cx));
        match &self.content {
            Content::Github(v) => v.update(cx, |s, cx| s.enable(value, cx)),
            Content::Jira(v) => v.update(cx, |s, cx| s.enable(value, cx)),
            Content::Youtrack(v) => v.update(cx, |s, cx| s.enable(value, cx)),
        }
    }
}
impl Render for MetadataEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column()
            .flex_shrink_0()
            .gap(px(12.))
            .child(self.status.clone())
            .child(match &self.content {
                Content::Github(v) => v.clone().into_any_element(),
                Content::Jira(v) => v.clone().into_any_element(),
                Content::Youtrack(v) => v.clone().into_any_element(),
            })
    }
}
