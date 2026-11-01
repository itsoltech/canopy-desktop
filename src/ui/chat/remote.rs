//! The ordinary composer keeps its device draft while the host accepts a send.
use super::{render::centered, view::ChatView};
use crate::app_state::{PendingOutcome as Pending, PendingRequest as SavedRequest, Recovering};
use crate::ui::{components::*, theme as t};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::*;
#[derive(Clone)]
pub(crate) struct ClientFileRequested {
    pub runtime: Entity<crate::app_state::ChatRuntime>,
    pub file: crate::app_state::ThreadFile,
}
impl EventEmitter<ClientFileRequested> for ChatView {}
impl ChatView {
    pub(super) fn restore_remote_edit(&mut self, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return;
        }
        let Some(saved) = self.runtime.update(cx, |r, _| r.take_saved_message_edit()) else {
            return;
        };
        self.editing = Some(super::edit::Editing {
            target: saved.target,
            draft: saved.previous,
            preview: None,
            restore: false,
            sending: None,
            checking: false,
            error: None,
            recheck: false,
        });
        if !saved.current {
            if let Some(edit) = &mut self.editing {
                edit.error = Some(
                    "The original run has changed. Review the saved request or cancel this edit."
                        .into(),
                );
            }
            return;
        }
        self.check_remote_edit(cx);
    }

    /// Asks the host again what editing the message would change (after a restart,
    /// or Check again); the edit tray shows the check under way.
    pub(super) fn check_remote_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.editing.as_mut().filter(|e| !e.checking) else {
            return;
        };
        let target = edit.target.clone();
        edit.checking = true;
        edit.error = None;
        edit.recheck = false;
        let selection = self
            .runtime
            .update(cx, |r, cx| r.preview_message_edit(target.item, cx));
        let Some(task) = selection.and_then(|selection| selection.checking) else {
            self.editing.as_mut().unwrap().checking = false;
            return;
        };
        cx.notify();
        self.edit_preview = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let Some(edit) = this.editing.as_mut().filter(|e| e.target == target) else {
                    return;
                };
                edit.checking = false;
                match result {
                    Ok(preview) => {
                        edit.preview = Some({
                            let f = preview.files;
                            canopy_desktop::chat::backend::RewindPreview {
                                possible: f.possible,
                                files: f.paths,
                                insertions: f.insertions,
                                deletions: f.deletions,
                            }
                        });
                    }
                    Err(error) => {
                        edit.error = Some(error);
                        edit.recheck = true;
                    }
                }
                cx.notify();
            });
        }));
    }

    /// The computer a remote chat runs on, by its name.
    pub(super) fn host_name(&self, cx: &App) -> String {
        self.rt(cx)
            .environment_name()
            .unwrap_or_else(|| "The other computer".into())
    }

    /// Spinners follow the remote session's history page and recovery action.
    pub(super) fn sync_remote_feedback(&mut self, cx: &App) {
        let runtime = self.rt(cx);
        let (history, recovering) = (runtime.history_loading(), runtime.recovering());
        let feedback = &mut self.remote_feedback;
        for (loading, active) in [
            (&mut feedback.history, history),
            (
                &mut feedback.check,
                recovering == Some(Recovering::Checking),
            ),
            (&mut feedback.again, recovering == Some(Recovering::Sending)),
        ] {
            if loading.active() != active {
                loading.set(active, cx);
            }
        }
    }

    /// A request saved on this device whose outcome the host hasn't confirmed: a tray
    /// above the composer, in the queue's place, naming the request and the computer,
    /// with what can be done about it.
    pub(super) fn recovery_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        use crate::ui::components::hover_card::content as card;
        let saved = self.rt(cx).pending_request()?;
        let id = saved.id.clone();
        let recovering = self.rt(cx).recovering();
        let host_name = self.host_name(cx);
        let state = saved.state;
        let line = state.line(&saved, &host_name);
        let (label, detail) = state.card(&host_name);
        let dot = state.dot();
        let feedback = &self.remote_feedback;
        let checking = recovering == Some(Recovering::Checking);
        let check = state.can_check().then(|| {
            tray_action("chat-check-request", "Check status", &feedback.check)
                .disabled(recovering == Some(Recovering::Sending))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runtime.update(cx, |r, cx| r.check_pending_request(cx))
                }))
        });
        // Hidden while a check runs: its answer decides whether sending again is safe.
        let again = (state.can_send_again() && !checking).then(|| {
            tray_action("chat-retry-request", "Send again", &feedback.again).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.runtime
                        .update(cx, |r, cx| r.retry_pending_request(id.clone(), cx))
                },
            ))
        });
        let owner = format!("recovery-{}", cx.entity_id());
        let text = saved.text.map(|text| {
            let line = canopy_desktop::chat::presentation::single_line(&text, 400);
            div()
                .id("chat-recovery-text")
                .pl(px(14.))
                .min_w_0()
                .truncate()
                .text_size(px(13.))
                .text_color(t::text())
                .hover_card(owner, gpui_kit::base::Placement::Top, move |_, _| {
                    card::body()
                        .child(
                            div()
                                .text_sm()
                                .line_height(t::chat_title_line_height())
                                .text_color(t::text())
                                .line_clamp(8)
                                .child(text.clone()),
                        )
                        .child(card::separator())
                        .child(card::status(dot, label, &detail, dot))
                        .into_any_element()
                })
                .child(line)
        });
        Some(
            centered(
                column()
                    .mx(px(14.))
                    .px(px(14.))
                    .py(px(10.))
                    .gap(px(6.))
                    .rounded_t(px(12.))
                    .border_1()
                    .border_b_0()
                    .border_color(t::border())
                    .bg(t::composer_surface())
                    .child(
                        row()
                            .gap(px(8.))
                            .child(div().size(px(6.)).flex_shrink_0().rounded_full().bg(dot))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(t::muted())
                                    .child(line),
                            )
                            .child(
                                row()
                                    .flex_shrink_0()
                                    .gap(px(2.))
                                    .children(check)
                                    .children(again),
                            ),
                    )
                    .children(text),
            )
            .mb(px(-4.)),
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

    /// Over the top of the conversation while the host holds earlier messages; a
    /// failed page turns it into Retry.
    pub(super) fn history_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let runtime = self.rt(cx);
        if !runtime.has_earlier_history() {
            return None;
        }
        let loading = &self.remote_feedback.history;
        let button = match runtime.history_error() {
            Some(_) if !runtime.history_loading() => loading_button_with_icon(
                "chat-earlier",
                "Couldn't load · Retry",
                icon(IconName::TriangleAlert).text_color(t::red()),
                loading,
            )
            .accessibility_label("Earlier messages couldn't load. Retry"),
            _ => loading_button("chat-earlier", "Earlier messages", loading),
        };
        Some(
            row()
                .absolute()
                .top_2()
                .left_0()
                .right_0()
                .justify_center()
                .child(
                    // Opaque, with a shadow: it lies over the messages.
                    button.shadow_md().on_click(cx.listener(|this, _, _, cx| {
                        this.runtime.update(cx, |r, cx| r.load_history(None, cx))
                    })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn save_remote_draft(&mut self, cx: &mut Context<Self>) {
        let draft = self.composer_draft(cx);
        self.remote_revision = self
            .runtime
            .update(cx, |runtime, cx| runtime.save_composer_draft(draft, cx));
    }
}

/// Spinners of the remote chat's own requests, following its session.
#[derive(Default)]
pub(super) struct RemoteFeedback {
    history: ButtonLoading,
    check: ButtonLoading,
    again: ButtonLoading,
}

impl Pending {
    fn dot(self) -> Hsla {
        match self {
            Self::Unconfirmed { .. } | Self::Unknown => t::yellow(),
            Self::Processing => t::accent(),
            Self::Missing => t::red(),
        }
    }
    /// The tray's line: the request ("it" when its text shows under the line) and
    /// the computer.
    fn line(self, saved: &SavedRequest, host: &str) -> String {
        let (title, noun) = saved.kind;
        let it = if saved.text.is_some() {
            "it".to_owned()
        } else {
            format!("the {noun}")
        };
        match self {
            Self::Unconfirmed { .. } => format!("{title} not confirmed by {host}"),
            Self::Processing => format!("{host} is processing {it}"),
            Self::Unknown => format!("{host} couldn't confirm {it}"),
            Self::Missing => format!("{host} has no record of {it}"),
        }
    }
    /// The hover card's status: label and detail.
    fn card(self, host: &str) -> (&'static str, String) {
        match self {
            Self::Unconfirmed { .. } => (
                "Not confirmed",
                format!("saved here before {host} answered"),
            ),
            Self::Processing => ("Processing", format!("{host} is still working on it")),
            Self::Unknown => (
                "Delivery unknown",
                format!("{host} may have run it, so it isn't sent again"),
            ),
            Self::Missing => ("Not received", format!("{host} never got it")),
        }
    }
}

/// A quiet action of the tray (ghost, 22 px, as in the queue), with its spinner.
fn tray_action(id: &'static str, label: &'static str, loading: &ButtonLoading) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .compact()
        .h(px(22.))
        .px(px(8.))
        .rounded(px(6.))
        .text_size(px(12.))
        .text_color(t::secondary())
        .accessibility_label(if loading.active() {
            SharedString::from(format!("{label}, in progress"))
        } else {
            label.into()
        })
        .loading(loading.active())
        .child(loading.label(label))
}
