//! Editing a sent message (DEC-043): the conversation continues from before it as a
//! new branch (the CLI forks the session there); the earlier continuation stays above
//! a separator. Files changed after the message can be restored first, when the CLI
//! kept their checkpoints.
use super::{render::centered, view::ChatView};
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{claude::wire::RewindPreview, session::Draft, transcript::ItemId};
use canopy_desktop::motion;
use gpui_kit::component::{IconName, Sizable, spinner::Spinner};
use gpui_kit::{prelude::FluentBuilder, *};

pub(super) struct Editing {
    pub item: ItemId,
    pub run: u64,
    /// Composer content from before the edit, back on Cancel.
    pub draft: Draft,
    /// What restoring files would change, once the CLI has answered.
    pub preview: Option<RewindPreview>,
    pub restore: bool,
    /// The edited message, waiting for the restore to finish (or, remotely, for the
    /// host to take the edit).
    pub sending: Option<Draft>,
    /// The host is checking the message again (after a restart, or Check again).
    pub checking: bool,
    pub error: Option<String>,
    /// The error offers Check again: the host's preview of the edit is gone or failed.
    pub recheck: bool,
}

impl ChatView {
    /// Puts a sent message into the composer to be edited and sent again.
    pub(super) fn start_edit(&mut self, item: ItemId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settled(cx) {
            return;
        }
        // One check at a time; the pencil shows it until the host answers.
        if self.edit_check.as_ref().is_some_and(|(_, l)| l.active()) {
            return;
        }
        let (original, preview) = self
            .runtime
            .update(cx, |runtime, cx| runtime.preview_message_edit(item, cx));
        let run = self.rt(cx).generation();
        let opened = original.is_some();
        if let Some(original) = original {
            self.open_message_edit(item, original, None, window, cx);
        } else {
            let mut loading = ButtonLoading::default();
            loading.set(true, cx);
            self.edit_check = Some((item, loading));
        }
        let revision = self.remote_revision;
        cx.notify();
        self.edit_preview = Some(cx.spawn_in(window, async move |this, cx| {
            let result = preview.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some((_, loading)) = &mut this.edit_check {
                    loading.set(false, cx);
                }
                if this.rt(cx).generation() != run
                    || (!opened && this.remote_revision != revision)
                    || (opened
                        && this
                            .editing
                            .as_ref()
                            .is_none_or(|edit| edit.item != item || edit.run != run))
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(preview) => {
                        let files = Some({
                            let f = preview.files;
                            canopy_desktop::chat::backend::RewindPreview {
                                possible: f.possible,
                                files: f.paths,
                                insertions: f.insertions,
                                deletions: f.deletions,
                            }
                        });
                        if opened {
                            let edit = this.editing.as_mut().unwrap();
                            if edit.sending.is_none() {
                                edit.preview = files;
                            }
                        } else {
                            this.open_message_edit(
                                item,
                                preview.original.unwrap_or_default(),
                                files,
                                window,
                                cx,
                            );
                        }
                    }
                    Err(error) => this.shell_error = Some(error),
                }
                cx.notify();
            });
        }));
    }

    fn open_message_edit(
        &mut self,
        item: ItemId,
        original: Draft,
        preview: Option<RewindPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.composer_draft(cx);
        let run = self.rt(cx).generation();
        self.runtime.update(cx, |runtime, cx| {
            runtime.save_message_edit(Some(previous.clone()), cx)
        });
        self.editing = Some(Editing {
            item,
            run,
            draft: previous,
            preview,
            restore: false,
            sending: None,
            checking: false,
            error: None,
            recheck: false,
        });
        self.set_composer_draft(original, window, cx);
    }

    pub(super) fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Restoring files is under way: the edit goes through once it finishes.
        if self.editing.as_ref().is_some_and(|e| e.sending.is_some()) {
            return;
        }
        let Some(editing) = self.editing.take() else {
            return;
        };
        self.edit_preview = None;
        self.runtime
            .update(cx, |runtime, cx| runtime.save_message_edit(None, cx));
        self.set_composer_draft(editing.draft, window, cx);
        cx.notify();
    }

    /// Sends the edited text: restores files first when asked, then forks.
    pub(super) fn send_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.composer_draft(cx);
        if self.attachments_blocked(cx)
            || (draft.text.trim().is_empty() && draft.attachments.is_empty())
            || self
                .editing
                .as_ref()
                .is_none_or(|e| e.sending.is_some() || e.checking)
        {
            return;
        }
        self.save_remote_draft(cx);
        let editing = self.editing.as_mut().unwrap();
        let (item, restore, run) = (editing.item, editing.restore, editing.run);
        editing.sending = Some(draft.clone());
        editing.error = None;
        editing.recheck = false;
        let revision = self.remote_revision;
        if let Err(error) = self.runtime.update(cx, |runtime, cx| {
            runtime.commit_message_edit(item, draft, restore, revision, run, cx)
        }) {
            let editing = self.editing.as_mut().unwrap();
            editing.sending = None;
            editing.error = Some(error);
            editing.recheck = true;
        }
        cx.notify();
    }

    /// The pencil's spinner while the host checks message `item` for editing.
    pub(super) fn edit_loading(&self, item: ItemId) -> Option<&ButtonLoading> {
        self.edit_check
            .as_ref()
            .filter(|(checked, _)| *checked == item)
            .map(|(_, loading)| loading)
    }

    /// Above the composer while editing: what happens on send, with Cancel. Without
    /// an edit, a saved remote request waiting for review takes the same place.
    pub(super) fn edit_bar(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let Some(editing) = self.editing.as_ref() else {
            return self.recovery_bar(cx);
        };
        let environment = self.rt(cx).environment_name();
        let files = editing
            .preview
            .as_ref()
            .filter(|p| p.possible && !p.files.is_empty());
        let sending = editing.sending.is_some();
        // Work under way is said with a spinner, never in the error's red.
        let progress = if editing.checking {
            Some(format!("Checking the message on {}…", self.host_name(cx)))
        } else if sending && editing.restore {
            Some("Restoring files…".to_owned())
        } else if let Some(environment) = environment.filter(|_| sending) {
            Some(format!("Sending the edit to {environment}…"))
        } else if sending {
            Some("Sending the edit…".to_owned())
        } else {
            None
        };
        let note = match (&editing.error, files) {
            _ if progress.is_some() => None,
            (Some(_), _) => None,
            (_, Some(_)) => None,
            (_, None) => {
                Some("The conversation continues from before this message; files stay as they are.")
            }
        };
        let error = editing
            .error
            .clone()
            .filter(|_| progress.is_none())
            .map(|error| {
                row()
                    .gap(px(6.))
                    .items_start()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(t::red())
                            .child(error),
                    )
                    .children(editing.recheck.then(|| {
                        super::decision::text_action("chat-edit-recheck", "Check again")
                            .on_click(cx.listener(|this, _, _, cx| this.check_remote_edit(cx)))
                    }))
            });
        let restore = files.filter(|_| !editing.checking).map(|preview| {
            let count = preview.files.len();
            let checked = editing.restore;
            row()
                .id("chat-edit-restore")
                .keyboard(format!(
                    "Also restore the files changed after this message, {}",
                    if checked { "on" } else { "off" }
                ))
                .gap(px(8.))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(t::secondary())
                .hover(|style| style.text_color(t::text()))
                .child(super::decision::checkbox(checked))
                .child(format!(
                    "Also restore {count} {} changed after this message (+{} −{})",
                    if count == 1 { "file" } else { "files" },
                    preview.insertions,
                    preview.deletions
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(editing) = &mut this.editing
                        && editing.sending.is_none()
                    {
                        editing.restore ^= true;
                        cx.notify();
                    }
                }))
        });
        // Once the edit is on its way it can't be called back.
        let cancel = div()
            .id("chat-edit-cancel")
            .flex_shrink_0()
            .px(px(6.))
            .h(px(20.))
            .flex()
            .items_center()
            .rounded(px(5.))
            .text_size(px(12.))
            .map(|cancel| {
                if sending {
                    cancel.text_color(t::secondary().opacity(0.5))
                } else {
                    cancel
                        .keyboard("Cancel editing")
                        .cursor_pointer()
                        .text_color(t::secondary())
                        .hover(|style| style.bg(t::hover()).text_color(t::text()))
                        .tooltip(|window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new("Cancel (Esc)")
                                .build(window, cx)
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.cancel_edit(window, cx)))
                }
            })
            .child("Cancel");
        Some(
            centered(
                column()
                    .mx(px(14.))
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(10.))
                    .gap(px(6.))
                    .rounded_t(px(12.))
                    .border_1()
                    .border_b_0()
                    .border_color(t::border())
                    .bg(t::composer_surface())
                    .child(
                        row()
                            .gap(px(8.))
                            .child(custom_icon("pencil").size(px(13.)).text_color(t::muted()))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .text_color(t::muted())
                                    .child("Editing message"),
                            )
                            .child(cancel),
                    )
                    .children(restore)
                    .children(progress.map(|label| progress_row(label, cx)))
                    .children(error)
                    .children(
                        note.map(|note| {
                            div().text_size(px(12.)).text_color(t::faint()).child(note)
                        }),
                    ),
            )
            .mb(px(-4.)),
        )
    }
}

/// Work under way, in a tray: an extra-small spinner and a muted line (a still mark
/// with Reduced Motion).
pub(super) fn progress_row(label: impl Into<SharedString>, cx: &App) -> Div {
    row().gap(px(8.)).child(busy_mark(cx)).child(
        div()
            .flex_1()
            .min_w_0()
            .text_size(px(12.))
            .text_color(t::muted())
            .child(label.into()),
    )
}

/// An extra-small spinner, or the loader's still mark with Reduced Motion.
pub(super) fn busy_mark(cx: &App) -> AnyElement {
    if motion::policy(cx) == motion::MotionPolicy::Reduced {
        icon(IconName::Loader)
            .size(px(12.))
            .text_color(t::muted())
            .into_any_element()
    } else {
        Spinner::new().xsmall().color(t::muted()).into_any_element()
    }
}
