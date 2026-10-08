use crate::{
    app_state::{AppState, WriteFinished},
    ui::{
        components::{integrations::integration_message, task_detail as view, *},
        markdown::MarkdownView,
        task_edit::comment_editor::{CommentEditor, CommentEditorDone},
        theme as t,
    },
};
use canopy_desktop::integrations::{TaskComment, TaskRef, TaskWrite};
use gpui_kit::{base::Disableable, component::IconName, *};
use std::{cell::Cell, rc::Rc};
pub(super) struct CommentCard {
    task: TaskRef,
    comment: TaskComment,
    markdown: Entity<MarkdownView>,
    interactive: bool,
    index: Rc<Cell<usize>>,
    list: ListState,
    editor: Option<Entity<CommentEditor>>,
    edit_event: Option<Subscription>,
    edit_size: Option<Subscription>,
    confirm_delete: bool,
    pending_delete: Option<u64>,
    delete_loading: ButtonLoading,
    error: Option<String>,
    _events: Vec<Subscription>,
}
impl CommentCard {
    pub(super) fn new(
        task: TaskRef,
        comment: TaskComment,
        interactive: bool,
        index: usize,
        list: ListState,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown = cx.new(MarkdownView::reading);
        markdown.update(cx, |view, cx| {
            view.set_content(comment.body.clone(), comment.url.clone(), interactive, cx)
        });
        let index = Rc::new(Cell::new(index));
        let slot = index.clone();
        let measured = list.clone();
        let state = cx.global::<AppState>().integrations.clone();
        let events = vec![
            cx.observe(&markdown, move |_, _, cx| {
                let index = slot.get();
                if index < measured.item_count() {
                    measured.splice(index..index + 1, 1);
                }
                cx.notify();
            }),
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe(&state, |this, _, event: &WriteFinished, cx| {
                if this.pending_delete != Some(event.id) {
                    return;
                }
                this.pending_delete = None;
                this.delete_loading.set(false, cx);
                if let Err(error) = &event.result {
                    this.error = Some(error.to_string());
                }
                this.invalidate(cx);
            }),
        ];
        Self {
            task,
            comment,
            markdown,
            interactive,
            index,
            list,
            editor: None,
            edit_event: None,
            edit_size: None,
            confirm_delete: false,
            pending_delete: None,
            delete_loading: ButtonLoading::default(),
            error: None,
            _events: events,
        }
    }
    pub(super) fn reindex(&self, index: usize) {
        self.index.set(index);
    }
    fn invalidate(&self, cx: &mut Context<Self>) {
        let index = self.index.get();
        if index < self.list.item_count() {
            self.list.splice(index..index + 1, 1);
        }
        cx.notify();
    }
    pub(super) fn set_comment(
        &mut self,
        mut comment: TaskComment,
        interactive: bool,
        cx: &mut Context<Self>,
    ) {
        comment.can_edit = comment.can_edit.or(self.comment.can_edit);
        comment.can_delete = comment.can_delete.or(self.comment.can_delete);
        self.comment = comment;
        self.enable(interactive, cx);
    }
    pub(super) fn enable(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.interactive = enabled;
        self.markdown.update(cx, |view, cx| {
            view.set_content(
                self.comment.body.clone(),
                self.comment.url.clone(),
                enabled,
                cx,
            )
        });
        if let Some(editor) = &self.editor {
            editor.update(cx, |e, cx| e.enable(enabled, cx));
        }
        cx.notify();
    }
    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.interactive || cx.global::<AppState>().integrations.read(cx).busy {
            return;
        }
        let editor = cx.new(|cx| {
            CommentEditor::new(self.task.clone(), Some(self.comment.clone()), window, cx)
        });
        editor.update(cx, |e, cx| e.enable(true, cx));
        self.edit_event = Some(cx.subscribe(&editor, |this, _, _: &CommentEditorDone, cx| {
            this.editor = None;
            this.edit_event = None;
            this.edit_size = None;
            this.invalidate(cx);
        }));
        self.edit_size = Some(cx.observe(&editor, |this, _, cx| this.invalidate(cx)));
        self.editor = Some(editor);
        self.invalidate(cx);
    }
    fn delete(&mut self, cx: &mut Context<Self>) {
        if !self.interactive || self.pending_delete.is_some() {
            return;
        }
        match cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |s, cx| {
                let draft = s.draft_key(
                    &self.task.project,
                    &format!("issue/{}/comment/{}", self.task.id, self.comment.id),
                );
                s.submit(
                    TaskWrite::DeleteComment {
                        task: self.task.clone(),
                        comment: self.comment.clone(),
                    },
                    draft,
                    cx,
                )
            }) {
            Ok(id) => {
                self.pending_delete = Some(id);
                self.delete_loading.set(true, cx);
            }
            Err(e) => self.error = Some(e),
        }
        self.invalidate(cx);
    }
}
impl Render for CommentCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.interactive || cx.global::<AppState>().integrations.read(cx).busy;
        let url = self.comment.url.clone();
        let body = self
            .editor
            .as_ref()
            .map(|e| e.clone().into_any_element())
            .unwrap_or_else(|| self.markdown.clone().into_any_element());
        let actions = row()
            .gap(px(6.))
            .children((self.comment.can_edit != Some(false)).then(|| {
                icon_button(
                    SharedString::from(format!("edit-{}", self.comment.id)),
                    IconName::Replace,
                    "Edit comment",
                )
                .disabled(disabled)
                .on_click(cx.listener(|this, _, window, cx| this.edit(window, cx)))
            }))
            .children((self.comment.can_delete != Some(false)).then(|| {
                icon_button(
                    SharedString::from(format!("delete-{}", self.comment.id)),
                    IconName::Delete,
                    "Delete comment",
                )
                .disabled(disabled)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.confirm_delete = true;
                    this.invalidate(cx);
                }))
            }))
            .child(
                icon_button(
                    SharedString::from(format!("comment-{}", self.comment.id)),
                    IconName::ExternalLink,
                    "Open comment in browser",
                )
                .disabled(!self.interactive)
                .on_click(move |_, _, cx| cx.open_url(&url)),
            );
        view::comment_card(&self.comment, body, actions)
            .children(self.error.clone().map(|e| integration_message(e, true)))
            .children(self.confirm_delete.then(|| {
                column()
                    .gap(px(8.))
                    .child(integration_message(
                        "Delete this comment? This cannot be undone.",
                        false,
                    ))
                    .child(
                        row()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                button("keep-comment", "Cancel")
                                    .disabled(self.pending_delete.is_some())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_delete = false;
                                        this.invalidate(cx);
                                    })),
                            )
                            .child(
                                loading_button(
                                    "confirm-delete-comment",
                                    "Delete comment",
                                    &self.delete_loading,
                                )
                                .text_color(t::red())
                                .disabled(disabled && !self.delete_loading.active())
                                .on_click(cx.listener(|this, _, _, cx| this.delete(cx))),
                            ),
                    )
            }))
    }
}
