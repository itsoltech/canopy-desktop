//! Quoting the conversation (DEC-070): where a selection in the conversation ends,
//! "Quote" puts the selected text into the composer as a quote badge and "Reply" opens
//! a popover to answer just that quote.
use super::{attachments::AttachState, view::ChatView};
use crate::ui::{components::*, platform_ui::shortcut, theme as t};
use canopy_desktop::{
    chat::{quotes, session::Draft},
    motion::{self, Presence, presets},
};
use gpui_kit::base::{Disableable, TextSelection};
use gpui_kit::component::input::{self, InputEvent, Textarea, TextareaState};
use gpui_kit::*;
use std::time::Instant;

const REPLY_WIDTH: f32 = 380.;

/// How much of a quote its tooltip shows.
const TOOLTIP_CHARS: usize = 600;

/// The selected text and where the button goes, offered until the selection goes.
pub(super) struct QuoteOffer {
    text: String,
    position: Point<Pixels>,
    /// The conversation's scroll position then: scrolling ends the offer.
    scroll: (usize, Pixels),
    presence: Presence,
}

/// A reply to one quote being written in its popover, where the selection was.
pub(super) struct QuoteReply {
    text: String,
    position: Point<Pixels>,
    field: Entity<TextareaState>,
    presence: Presence,
    _events: Subscription,
}

impl ChatView {
    /// The left button came up in the conversation: offer what it selected, if anything.
    pub(super) fn offer_quote(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // After the selection layer has finished the drag.
        cx.defer_in(window, move |this, window, cx| {
            let text = TextSelection::selected_text(window, cx);
            if text.trim().is_empty() {
                this.dismiss_quote_offer(cx);
                return;
            }
            let now = Instant::now();
            let mut presence = Presence::new(false, presets::POPOVER, now);
            presence.set_open(true, now, motion::policy(cx));
            let scroll = this.shown_rows().scroller.read(cx).position();
            this.quote_offer = Some(QuoteOffer {
                text,
                position,
                scroll,
                presence,
            });
            cx.notify();
        });
    }

    pub(super) fn dismiss_quote_offer(&mut self, cx: &mut Context<Self>) -> bool {
        if self.quote_offer.take().is_none() {
            return false;
        }
        cx.notify();
        true
    }

    /// "Reply" on the offer: a popover with the quote and a field for the answer.
    fn open_quote_reply(
        &mut self,
        text: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .placeholder("Reply to this quote")
                .submit_on_enter(true)
        });
        let events =
            cx.subscribe_in(
                &field,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter {
                        secondary: true, ..
                    } => this.finish_quote_reply(false, window, cx),
                    InputEvent::PressEnter { shift: false, .. } => {
                        this.finish_quote_reply(true, window, cx)
                    }
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            );
        let focus = field.clone();
        window
            .on_next_frame(move |window, cx| focus.update(cx, |state, cx| state.focus(window, cx)));
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        self.quote_reply = Some(QuoteReply {
            text,
            position,
            field,
            presence,
            _events: events,
        });
        cx.notify();
    }

    pub(super) fn close_quote_reply(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.quote_reply.take().is_none() {
            return false;
        }
        self.request_composer_focus(window, cx);
        cx.notify();
        true
    }

    /// Sending answers right away and leaves the composer's draft alone; otherwise the
    /// quote and the reply go into the composer at its cursor.
    fn finish_quote_reply(&mut self, send: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reply) = self.quote_reply.as_ref() else {
            return;
        };
        let answer = reply.field.read(cx).value().trim().to_owned();
        if send && !self.can_send_quote_reply(&answer) {
            return;
        }
        let Some(quote) = quotes::quote(&reply.text) else {
            return;
        };
        let text = reply.text.clone();
        self.quote_reply = None;
        if send {
            let mention = quote.label().unwrap_or_default().to_owned();
            self.send_draft(
                Draft {
                    text: format!("{mention} {answer}"),
                    attachments: vec![quote],
                    context: None,
                },
                cx,
            );
            self.request_composer_focus(window, cx);
        } else {
            self.add_quote(&text, cx);
            self.insert_badges(window, cx);
            if !answer.is_empty() {
                // After the badge's own deferred insertion.
                cx.defer_in(window, move |this, window, cx| {
                    this.composer.update(cx, |state, cx| {
                        state.insert(&format!("{answer} "), window, cx);
                        state.focus(window, cx);
                    });
                });
            }
        }
        cx.notify();
    }

    /// A reply is sent like a message, so only what the composer could send now.
    fn can_send_quote_reply(&self, answer: &str) -> bool {
        !answer.is_empty()
            && self.decision.active().is_none()
            && self.editing.is_none()
            && self.session.as_ref().is_some_and(|s| !s.status().ended())
    }

    fn request_composer_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self.composer.clone();
        window.on_next_frame(move |window, cx| {
            composer.update(cx, |state, cx| state.focus(window, cx))
        });
        cx.notify();
    }

    /// The reply popover where the selection was, until it is sent, added or dismissed.
    pub(super) fn quote_reply_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let reply = self.quote_reply.as_ref()?;
        let now = Instant::now();
        let progress = reply.presence.progress(now);
        motion::request_frame(window, reply.presence.is_animating(now));
        let answer = reply.field.read(cx).value().trim().to_owned();
        let can_send = self.can_send_quote_reply(&answer);
        let width = (f32::from(window.viewport_size().width) - 16.).clamp(0., REPLY_WIDTH);
        let send_hint = "Send (Enter)";
        let add_hint = format!(
            "Add the quote and reply to your message ({})",
            shortcut("⌘⏎", "Ctrl+Enter")
        );
        Some(
            deferred(
                anchored()
                    .position(reply.position)
                    .offset(point(px(-30.), px(8. + 4. * (1. - progress))))
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        column()
                            .id("chat-quote-reply")
                            .occlude()
                            .w(px(width))
                            .p(px(10.))
                            .gap(px(8.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(t::control_border())
                            .bg(t::elevated())
                            .shadow_md()
                            .opacity(progress)
                            .text_size(px(13.))
                            .text_color(t::text())
                            .capture_action(cx.listener(|this, _: &input::Escape, window, cx| {
                                this.close_quote_reply(window, cx);
                                cx.stop_propagation();
                            }))
                            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                                this.close_quote_reply(window, cx);
                            }))
                            .child(
                                row()
                                    .items_start()
                                    .gap(px(8.))
                                    .pl(px(8.))
                                    .border_l_2()
                                    .border_color(t::accent().opacity(0.6))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_size(px(12.))
                                            .text_color(t::secondary())
                                            .line_clamp(3)
                                            .child(reply.text.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(t::control_border())
                                    .bg(t::input_bg())
                                    .px(px(6.))
                                    .py(px(4.))
                                    .child(
                                        Textarea::new(&reply.field)
                                            .appearance(false)
                                            .w_full()
                                            .text_size(px(13.)),
                                    ),
                            )
                            .child(
                                row()
                                    .gap(px(8.))
                                    .child(div().flex_1())
                                    .child(
                                        button("chat-quote-reply-add", "Add to message")
                                            .tooltip(add_hint)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.finish_quote_reply(false, window, cx)
                                            })),
                                    )
                                    .child(
                                        primary_button("chat-quote-reply-send", "Send")
                                            .tooltip(send_hint)
                                            .disabled(!can_send)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.finish_quote_reply(true, window, cx)
                                            })),
                                    ),
                            ),
                    ),
            )
            .with_priority(2),
        )
    }

    /// Puts `text` into the composer as a quote badge at the cursor.
    pub(super) fn add_quote(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(quote) = quotes::quote(text) else {
            return;
        };
        let mention = quote.label().unwrap_or_default().to_owned();
        let name = mention
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        self.add_pending(&name, mention, AttachState::Ready(quote), false);
        cx.notify();
    }

    /// The button over the end of the selection, while the selection and the scroll
    /// position last.
    pub(super) fn quote_button(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let offer = self.quote_offer.as_ref()?;
        let gone = !TextSelection::has_selection(window, cx)
            || self.shown_rows().scroller.read(cx).position() != offer.scroll;
        if gone {
            self.quote_offer = None;
            return None;
        }
        let now = Instant::now();
        let progress = offer.presence.progress(now);
        motion::request_frame(window, offer.presence.is_animating(now));
        let view = cx.entity().downgrade();
        let quote = view.clone();
        let position = offer.position;
        Some(
            deferred(
                anchored()
                    .position(position)
                    .offset(point(px(-30.), px(-40. + 4. * (1. - progress))))
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        row()
                            .id("chat-quote-selection")
                            .occlude()
                            .h(px(28.))
                            .p(px(2.))
                            .gap(px(2.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(t::control_border())
                            .bg(t::elevated())
                            .shadow_md()
                            .opacity(progress)
                            .text_size(px(13.))
                            .text_color(t::secondary())
                            .child(
                                selection_action("chat-quote-add", "quote", "Quote")
                                    // On press: releasing would be too late, the press
                                    // clears the selection the offer lives on.
                                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                        cx.stop_propagation();
                                        let _ = quote.update(cx, |this, cx| {
                                            if let Some(offer) = this.quote_offer.take() {
                                                this.add_quote(&offer.text, cx);
                                                this.insert_badges(window, cx);
                                            }
                                        });
                                        TextSelection::clear(window, cx);
                                    }),
                            )
                            .child(div().w(px(1.)).h(px(16.)).bg(t::border()))
                            .child(
                                selection_action("chat-quote-reply-open", "reply", "Reply")
                                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                        cx.stop_propagation();
                                        let _ = view.update(cx, |this, cx| {
                                            if let Some(offer) = this.quote_offer.take() {
                                                this.open_quote_reply(
                                                    offer.text,
                                                    offer.position,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        });
                                        TextSelection::clear(window, cx);
                                    }),
                            ),
                    ),
            )
            .with_priority(2),
        )
    }
}

/// One action of the selection toolbar: a mark and a label, lit on hover.
fn selection_action(id: &'static str, icon: &'static str, label: &'static str) -> Stateful<Div> {
    row()
        .id(id)
        .h_full()
        .px(px(8.))
        .gap(px(6.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|style| style.text_color(t::text()).bg(t::hover()))
        .child(custom_icon(icon).size(px(13.)))
        .child(label)
}

/// A quote's badge, in the composer and in a sent message: its first words, the whole
/// quote in a tooltip.
pub(super) fn quote_badge(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    text: &str,
    height: Pixels,
    selected: bool,
) -> Stateful<Div> {
    let mut tooltip: String = text.chars().take(TOOLTIP_CHARS).collect();
    if text.chars().count() > TOOLTIP_CHARS {
        tooltip.push('…');
    }
    let tooltip: SharedString = tooltip.into();
    let label: SharedString = label.into();
    row()
        .id(id)
        .h(height)
        .min_w_0()
        .gap(px(4.))
        .px(px(5.))
        .rounded(px(5.))
        .border_1()
        .border_color(if selected {
            t::accent()
        } else {
            t::accent().opacity(0.14)
        })
        .bg(t::accent().opacity(if selected { 0.2 } else { 0.08 }))
        .hover(|style| style.bg(t::accent().opacity(0.14)))
        .text_color(t::text())
        .cursor_pointer()
        .child(
            custom_icon("quote")
                .size(height - px(10.))
                .text_color(t::accent()),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(label.trim_start_matches("❝ ").to_owned()),
        )
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
}
