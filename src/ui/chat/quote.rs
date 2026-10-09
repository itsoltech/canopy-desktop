//! Quoting the conversation (DEC-070): a "Quote" button where a selection in the
//! conversation ends puts the selected text into the composer as a quote badge.
use super::{attachments::AttachState, view::ChatView};
use crate::ui::{components::*, theme as t};
use canopy_desktop::{
    chat::quotes,
    motion::{self, Presence, presets},
};
use gpui_kit::base::TextSelection;
use gpui_kit::*;
use std::time::Instant;

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
                            .px(px(10.))
                            .gap(px(6.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(t::control_border())
                            .bg(t::elevated())
                            .shadow_md()
                            .opacity(progress)
                            .text_size(px(13.))
                            .text_color(t::secondary())
                            .cursor_pointer()
                            .hover(|style| style.text_color(t::text()).bg(t::hover()))
                            .child(custom_icon("quote").size(px(13.)))
                            .child("Quote")
                            // On press: releasing would be too late, the press clears
                            // the selection the offer lives on.
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                cx.stop_propagation();
                                let _ = view.update(cx, |this, cx| {
                                    if let Some(offer) = this.quote_offer.take() {
                                        this.add_quote(&offer.text, cx);
                                        this.insert_badges(window, cx);
                                    }
                                });
                                TextSelection::clear(window, cx);
                            }),
                    ),
            )
            .with_priority(2),
        )
    }
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
