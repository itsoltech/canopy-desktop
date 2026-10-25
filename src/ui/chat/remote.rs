//! The ordinary composer keeps its device draft while the host accepts a send.
use super::view::ChatView;
use gpui_kit::*;
#[derive(Clone)]
pub(crate) struct ClientFileRequested {
    pub runtime: Entity<crate::app_state::ChatRuntime>,
    pub file: canopy_desktop::remote::proto::FileRef,
}
impl EventEmitter<ClientFileRequested> for ChatView {}
impl ChatView {
    pub(super) fn restore_remote_edit(&mut self, cx: &mut Context<Self>) {
        if self.editing.is_some() || !self.rt(cx).is_remote() {
            return;
        }
        let Some((saved, previous)) = self.runtime.update(cx, |r, _| r.take_client_edit()) else {
            return;
        };
        let Some(item) = saved.item else {
            return;
        };
        let current = self
            .rt(cx)
            .remote_source()
            .is_some_and(|s| s.run() == saved.run_id)
            && self.rt(cx).epoch() == item.history_epoch;
        let draft = self.composer_draft(cx);
        self.editing = Some(super::edit::Editing {
            item: canopy_desktop::chat::transcript::ItemId(item.item_id),
            fork: None,
            draft: previous,
            preview: None,
            restore: false,
            sending: Some(draft),
            error: Some("Checking the original edit point…".into()),
            context: saved.original.and_then(|d| d.context),
        });
        if !current {
            if let Some(edit) = &mut self.editing {
                edit.sending = None;
                edit.error = Some(
                    "The original run has changed. Review the saved request or cancel this edit."
                        .into(),
                );
            }
            return;
        }
        let task = self.runtime.update(cx, |r, cx| {
            r.client_preview(canopy_desktop::chat::transcript::ItemId(item.item_id), cx)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let Some(edit) = this.editing.as_mut().filter(|e| e.item.0 == item.item_id) else {
                    return;
                };
                edit.sending = None;
                match result {
                    Ok(preview) => {
                        edit.error = None;
                        edit.preview =
                            preview
                                .files
                                .map(|f| canopy_desktop::chat::backend::RewindPreview {
                                    possible: f.possible,
                                    files: f.paths,
                                    insertions: f.insertions,
                                    deletions: f.deletions,
                                });
                    }
                    Err(error) => edit.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn recovery_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::ui::components::*;
        let source = self.rt(cx).remote_source()?;
        let host = source.host.as_ref()?;
        let pending = host
            .chat_mutations(&source.chat())
            .into_iter()
            .find(|(_, _, r)| {
                r.as_ref()
                    .is_none_or(|r| !r.resolved_by_user && ![3, 4, 5, 7].contains(&r.state))
            })?;
        let retry = pending.2.as_ref().is_none_or(|r| r.state == 8);
        let id = pending.0;
        Some(
            row()
                .gap_2()
                .px_1()
                .child("A saved request needs review.")
                .child(
                    button("chat-check-request", "Check status").on_click(cx.listener(
                        |this, _, _, cx| this.runtime.update(cx, |r, cx| r.client_reconcile(cx)),
                    )),
                )
                .children(retry.then(|| {
                    button("chat-retry-request", "Retry original request").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.runtime
                                .update(cx, |r, cx| r.client_retry_saved(id.clone(), cx))
                        },
                    ))
                }))
                .into_any_element(),
        )
    }
    pub(super) fn restore_remote_anchor(&mut self, cx: &mut Context<Self>) {
        let Some((epoch, id)) = self.remote_anchor else {
            return;
        };
        let transcript = self.transcript(cx);
        let row = self.rows.rows.iter().position(|row| {
            let super::rows::Kind::Item(source) = row.kind else {
                return false;
            };
            let part = match source {
                super::rows::Source::Current(_) => self.rt(cx).epoch(),
                super::rows::Source::Archived { epoch, .. } => self.rows.archived[epoch].epoch,
            };
            part == epoch
                && self
                    .rows
                    .item(source, transcript)
                    .is_some_and(|i| i.id.0 == id)
        });
        if let Some(row) = row {
            self.remote_anchor = None;
            self.rows.scroller.update(cx, |s, cx| s.reveal_row(row, cx));
        }
    }
    pub(super) fn history_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::ui::components::*;
        use gpui_kit::component::{Disableable, button::ButtonVariants};
        let source = self.rt(cx).remote_source()?;
        if !source.projection.has_earlier {
            return None;
        }
        Some(
            row()
                .absolute()
                .top_2()
                .left_0()
                .right_0()
                .justify_center()
                .child(
                    button("chat-earlier", "Earlier messages")
                        .ghost()
                        .disabled(source.history_loading)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.runtime.update(cx, |r, cx| r.client_history(None, cx))
                        })),
                )
                .into_any_element(),
        )
    }
    pub(super) fn save_remote_draft(&mut self, cx: &mut Context<Self>) {
        if !self.rt(cx).is_remote() {
            return;
        }
        let draft = self.composer_draft(cx);
        let revision = canopy_desktop::remote::client::HostClient::next_view_revision();
        self.remote_revision = revision;
        self.runtime.update(cx, |runtime, cx| {
            runtime.save_client_draft(draft, revision, cx)
        });
    }
}
