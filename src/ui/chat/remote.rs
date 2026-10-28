//! The ordinary composer keeps its device draft while the host accepts a send.
use super::{render::centered, view::ChatView};
use crate::app_state::Recovering;
use crate::ui::{components::*, theme as t};
use canopy_desktop::remote::{client::ChatMutation, proto as p};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
};
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
        self.editing = Some(super::edit::Editing {
            item: canopy_desktop::chat::transcript::ItemId(item.item_id),
            fork: None,
            draft: previous,
            preview: None,
            restore: false,
            sending: None,
            checking: false,
            error: None,
            recheck: false,
            context: saved.original.and_then(|d| d.context),
        });
        if !current {
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
        let item = edit.item;
        edit.checking = true;
        edit.error = None;
        edit.recheck = false;
        let task = self.runtime.update(cx, |r, cx| r.client_preview(item, cx));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let Some(edit) = this.editing.as_mut().filter(|e| e.item == item) else {
                    return;
                };
                edit.checking = false;
                match result {
                    Ok(preview) => {
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
                    Err(error) => {
                        edit.error = Some(error);
                        edit.recheck = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The computer a remote chat runs on, by its name.
    pub(super) fn host_name(&self, cx: &App) -> String {
        self.rt(cx)
            .remote_source()
            .and_then(|s| s.host.as_ref())
            .map(|host| host.name())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "The other computer".into())
    }

    /// Spinners follow the remote session's history page and recovery action.
    pub(super) fn sync_remote_feedback(&mut self, cx: &App) {
        let Some(source) = self.rt(cx).remote_source() else {
            return;
        };
        let (history, recovering) = (source.history_loading, source.recovering);
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
        let source = self.rt(cx).remote_source()?;
        let host = source.host.as_ref()?;
        let (id, mutation, receipt) =
            host.chat_mutations(&source.chat())
                .into_iter()
                .find(|(_, _, r)| {
                    r.as_ref()
                        .is_none_or(|r| !r.resolved_by_user && ![3, 4, 5, 7].contains(&r.state))
                })?;
        let recovering = source.recovering;
        let host_name = self.host_name(cx);
        let saved = SavedRequest::of(&mutation);
        let state = Pending::of(receipt.as_ref().map(|r| r.state));
        let line = state.line(&saved, &host_name);
        let (label, detail) = state.card(&host_name);
        let dot = state.dot();
        let feedback = &self.remote_feedback;
        let checking = recovering == Some(Recovering::Checking);
        let check = state.can_check().then(|| {
            tray_action("chat-check-request", "Check status", &feedback.check)
                .disabled(recovering == Some(Recovering::Sending))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runtime.update(cx, |r, cx| r.client_reconcile(cx))
                }))
        });
        // Hidden while a check runs: its answer decides whether sending again is safe.
        let again = (state.can_send_again() && !checking).then(|| {
            tray_action("chat-retry-request", "Send again", &feedback.again).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.runtime
                        .update(cx, |r, cx| r.client_retry_saved(id.clone(), cx))
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
        let source = self.rt(cx).remote_source()?;
        if !source.projection.has_earlier {
            return None;
        }
        let loading = &self.remote_feedback.history;
        let button = match &source.history_error {
            Some(_) if !source.history_loading => loading_button_with_icon(
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

/// Spinners of the remote chat's own requests, following its session.
#[derive(Default)]
pub(super) struct RemoteFeedback {
    history: ButtonLoading,
    check: ButtonLoading,
    again: ButtonLoading,
}

/// What a saved request was, for its tray.
struct SavedRequest {
    /// Its kind as the tray names it: capitalized, and within a sentence.
    kind: (&'static str, &'static str),
    /// What was asked, when it has text: the message, the edit, the command.
    text: Option<String>,
}
impl SavedRequest {
    fn of(mutation: &ChatMutation) -> Self {
        let draft = |draft: Option<&p::MessageDraft>| {
            draft
                .map(|d| d.text.trim().to_owned())
                .filter(|text| !text.is_empty())
        };
        let (kind, text) = match mutation {
            ChatMutation::Send(r) => (("Message", "message"), draft(r.draft.as_ref())),
            ChatMutation::Edit(r) => (("Edit", "edit"), draft(r.draft.as_ref())),
            ChatMutation::Fork(r) => (("Fork", "fork"), draft(r.replacement.as_ref())),
            ChatMutation::RunShell(r) => (
                ("Command", "command"),
                Some(format!("!{}", r.command_line.trim())),
            ),
            ChatMutation::StopShell(_) => (("Stop request", "stop request"), None),
        };
        Self { kind, text }
    }
}

/// Where a saved request stands, by its receipt (`OperationState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// No receipt (saved before the host answered), or one without a state.
    Unconfirmed { replayable: bool },
    /// ACCEPTED / ATTEMPTING: the host is still working on it.
    Processing,
    /// DELIVERY_UNKNOWN: it may have run; it is never sent again by itself.
    Unknown,
    /// NOT_FOUND: the host never got it.
    Missing,
}
impl Pending {
    fn of(state: Option<i32>) -> Self {
        match state {
            None => Self::Unconfirmed { replayable: true },
            Some(1 | 2) => Self::Processing,
            Some(6) => Self::Unknown,
            Some(8) => Self::Missing,
            Some(_) => Self::Unconfirmed { replayable: false },
        }
    }
    fn can_check(self) -> bool {
        self != Self::Missing
    }
    fn can_send_again(self) -> bool {
        matches!(self, Self::Unconfirmed { replayable: true } | Self::Missing)
    }
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

#[cfg(test)]
mod tests {
    use super::Pending;
    use core::prelude::v1::test;

    #[test]
    fn sending_again_waits_while_the_host_may_have_the_request() {
        assert!(Pending::of(None).can_send_again(), "never confirmed");
        assert!(
            Pending::of(Some(8)).can_send_again(),
            "the host has no record"
        );
        assert!(!Pending::of(Some(8)).can_check());
        for state in [1, 2, 6] {
            assert!(!Pending::of(Some(state)).can_send_again(), "state {state}");
            assert!(Pending::of(Some(state)).can_check());
        }
    }
}
