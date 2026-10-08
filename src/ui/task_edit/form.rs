use super::{
    TaskEditRequest,
    issue_editor::{IssueDeleted, IssueEditor, IssueSaved},
};
use crate::ui::jira::form::JiraForm;
use crate::ui::youtrack::form::YoutrackForm;
use canopy_desktop::integrations::Provider;
use gpui_kit::*;
enum Editor {
    Github(Entity<IssueEditor>),
    Jira(Entity<JiraForm>),
    Youtrack(Entity<YoutrackForm>),
}
pub struct TaskForm {
    editor: Editor,
    _events: Vec<Subscription>,
}
impl EventEmitter<IssueSaved> for TaskForm {}
impl EventEmitter<IssueDeleted> for TaskForm {}
impl TaskForm {
    pub fn new(request: TaskEditRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (editor, event) = if request.project().provider == Provider::Jira {
            let view = cx.new(|cx| JiraForm::new(request, window, cx));
            let e = cx.subscribe(&view, |_, _, e: &IssueSaved, cx| cx.emit(e.clone()));
            {
                let deleted =
                    cx.subscribe(&view, |_, _, _: &IssueDeleted, cx| cx.emit(IssueDeleted));
                (Editor::Jira(view), vec![e, deleted])
            }
        } else if request.project().provider == Provider::Youtrack {
            let view = cx.new(|cx| YoutrackForm::new(request, window, cx));
            let e = cx.subscribe(&view, |_, _, e: &IssueSaved, cx| cx.emit(e.clone()));
            (Editor::Youtrack(view), vec![e])
        } else {
            let view = cx.new(|cx| IssueEditor::new(request, window, cx));
            let e = cx.subscribe(&view, |_, _, e: &IssueSaved, cx| cx.emit(e.clone()));
            (Editor::Github(view), vec![e])
        };
        Self {
            editor,
            _events: event,
        }
    }
    pub fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        match &self.editor {
            Editor::Github(v) => v.update(cx, |s, cx| s.enable(enabled, cx)),
            Editor::Jira(v) => v.update(cx, |s, cx| s.enable(enabled, cx)),
            Editor::Youtrack(v) => v.update(cx, |s, cx| s.enable(enabled, cx)),
        }
    }
}
impl Render for TaskForm {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match &self.editor {
            Editor::Github(v) => v.clone().into_any_element(),
            Editor::Jira(v) => v.clone().into_any_element(),
            Editor::Youtrack(v) => v.clone().into_any_element(),
        }
    }
}
