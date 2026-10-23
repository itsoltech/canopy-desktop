//! Rich hover cards: several facts about the hovered object, beside it.
//!
//! One layer per window, owned by the workspace; triggers find it through the
//! window. The first card waits for intent, then eases in. While a card is open,
//! or just closed, the next trigger opens at once and the card glides there.
//! Leaving fades it out; a press or a scroll dismisses it.
use canopy_desktop::motion::{self, Presence, Transition, distance, presets};
use gpui_kit::base::{Align, ElementExt, Placement, Positioner};
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Same as GPUI's text tooltips, so a pointer crossing a list opens nothing.
const INTENT: Duration = Duration::from_millis(500);
/// How long after closing the next trigger still opens without the delay.
const GRACE: Duration = Duration::from_millis(300);
const PRIORITY: usize = 210;

type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Card {
    owner: SharedString,
    anchor: Rc<Cell<Bounds<Pixels>>>,
    placement: Placement,
    build: Build,
}

pub struct HoverCards {
    card: Option<Card>,
    /// The trigger under the pointer, also while a control inside it hides the card.
    hovered: Option<SharedString>,
    suppressed: bool,
    /// The card is showing or easing in; it may still be on screen while leaving.
    open: bool,
    presence: Presence,
    /// Offset from the card's place, carrying it over from the previous trigger.
    glide: (Transition, Transition),
    pending: Option<Task<()>>,
    warm: Option<Task<()>>,
}

#[derive(Default)]
struct Layers(HashMap<WindowId, WeakEntity<HoverCards>>);
impl Global for Layers {}

impl HoverCards {
    /// The layer of `window`; the caller owns it and renders it last.
    pub fn install(window: &Window, cx: &mut App) -> Entity<Self> {
        let layer = cx.new(|_| Self::new());
        let layers = &mut cx.default_global::<Layers>().0;
        layers.retain(|_, layer| layer.upgrade().is_some());
        layers.insert(window.window_handle().window_id(), layer.downgrade());
        layer
    }

    fn new() -> Self {
        let now = Instant::now();
        Self {
            card: None,
            hovered: None,
            suppressed: false,
            open: false,
            presence: Presence::new(false, presets::TOOLTIP, now),
            glide: (Transition::new(0., now), Transition::new(0., now)),
            pending: None,
            warm: None,
        }
    }

    fn of(window: &Window, cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<Layers>()?
            .0
            .get(&window.window_handle().window_id())?
            .upgrade()
    }

    fn show(&mut self, card: Card, cx: &mut Context<Self>) {
        self.hovered = Some(card.owner.clone());
        self.pending = None;
        if !self.open && self.warm.is_none() {
            self.pending = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(INTENT).await;
                let _ = this.update(cx, |this, cx| {
                    this.pending = None;
                    if this.hovered.as_ref() == Some(&card.owner) {
                        this.open(card, false, cx);
                    }
                });
            }));
            return;
        }
        // Still on screen, even if leaving: travel from where it is.
        let glide = self.presence.progress(Instant::now()) > 0.;
        self.open(card, glide, cx);
    }

    fn open(&mut self, card: Card, glide: bool, cx: &mut Context<Self>) {
        let now = Instant::now();
        let policy = motion::policy(cx);
        let from = match (&self.card, glide) {
            (Some(previous), true) if previous.owner != card.owner => {
                let (before, after) = (previous.anchor.get(), card.anchor.get());
                (
                    f32::from(before.origin.x - after.origin.x) + self.glide.0.value(now),
                    f32::from(before.origin.y - after.origin.y) + self.glide.1.value(now),
                )
            }
            _ => (0., 0.),
        };
        self.glide = (Transition::new(from.0, now), Transition::new(from.1, now));
        self.glide
            .0
            .retarget(0., presets::STATE_CHANGE, now, policy);
        self.glide
            .1
            .retarget(0., presets::STATE_CHANGE, now, policy);
        self.card = Some(card);
        self.warm = None;
        if !self.suppressed {
            self.open = true;
            self.presence.set_open(true, now, policy);
        }
        cx.notify();
    }

    fn hide(&mut self, owner: &SharedString, cx: &mut Context<Self>) {
        if self.hovered.as_ref() != Some(owner) {
            return;
        }
        self.hovered = None;
        self.pending = None;
        if self.close(cx) {
            self.warm = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(GRACE).await;
                let _ = this.update(cx, |this, _| this.warm = None);
            }));
        }
    }

    /// A press or a scroll: the next card waits for intent again.
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.pending = None;
        self.warm = None;
        self.close(cx);
    }

    /// Whether the card was open.
    fn close(&mut self, cx: &mut Context<Self>) -> bool {
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
        std::mem::take(&mut self.open)
    }

    fn suppress(&mut self, suppressed: bool, cx: &mut Context<Self>) {
        self.suppressed = suppressed;
        let open = !suppressed
            && self.hovered.is_some()
            && self.card.as_ref().map(|card| &card.owner) == self.hovered.as_ref();
        self.open = open;
        if self
            .presence
            .set_open(open, Instant::now(), motion::policy(cx))
        {
            cx.notify();
        }
    }
}

impl Render for HoverCards {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let active = self.presence.is_animating(now)
            || self.glide.0.is_animating(now)
            || self.glide.1.is_animating(now);
        motion::request_frame(window, active);
        let progress = self.presence.progress(now);
        let Some(card) = self.card.as_ref().filter(|_| progress > 0.) else {
            return div().into_any_element();
        };
        // The card leaves its trigger a few pixels out, on the side it opens toward.
        let rise = distance::MICRO * (1. - progress);
        let (x, y) = match card.placement {
            Placement::Right => (-rise, 0.),
            Placement::Left => (rise, 0.),
            Placement::Bottom => (0., -rise),
            Placement::Top => (0., rise),
        };
        let theme = cx.theme();
        let surface = div()
            .relative()
            .left(px(x + self.glide.0.value(now)))
            .top(px(y + self.glide.1.value(now)))
            .opacity(progress)
            .font_family(theme.font_family.clone())
            .bg(theme.tokens.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .rounded(theme.radius)
            .py_2()
            .px_2p5()
            .child((card.build)(window, cx));
        deferred(
            Positioner::side(card.anchor.get())
                .placement(card.placement)
                .align(Align::Start)
                .offset(px(distance::SMALL))
                .margin(px(distance::BASE))
                .child(surface),
        )
        .with_priority(PRIORITY)
        .into_any_element()
    }
}

/// An element that shows a hover card while the pointer rests on it.
pub trait HoverCardExt: StatefulInteractiveElement + ParentElement + Sized {
    /// `owner` names the trigger, unique among the window's triggers.
    fn hover_card(
        self,
        owner: impl Into<SharedString>,
        placement: Placement,
        build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        let owner = owner.into();
        let anchor = Rc::new(Cell::new(Bounds::default()));
        let build: Build = Rc::new(build);
        let bounds = anchor.clone();
        self.on_prepaint(move |rect, _, _| bounds.set(rect))
            .on_hover(move |hovered, window, cx| {
                let Some(layer) = HoverCards::of(window, cx) else {
                    return;
                };
                layer.update(cx, |layer, cx| {
                    if *hovered {
                        let card = Card {
                            owner: owner.clone(),
                            anchor: anchor.clone(),
                            placement,
                            build: build.clone(),
                        };
                        layer.show(card, cx);
                    } else {
                        layer.hide(&owner, cx);
                    }
                });
            })
            .on_mouse_down(MouseButton::Left, dismiss)
            .on_scroll_wheel(dismiss)
    }

    /// A control inside a trigger with a tooltip of its own: the card steps aside.
    fn hides_hover_card(self) -> Self {
        self.on_hover(|hovered, window, cx| {
            if let Some(layer) = HoverCards::of(window, cx) {
                layer.update(cx, |layer, cx| layer.suppress(*hovered, cx));
            }
        })
    }
}

impl<E: StatefulInteractiveElement + ParentElement> HoverCardExt for E {}

fn dismiss<E>(_: &E, window: &mut Window, cx: &mut App) {
    if let Some(layer) = HoverCards::of(window, cx) {
        layer.update(cx, |layer, cx| layer.dismiss(cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{Card, GRACE, HoverCards, INTENT};
    use gpui_kit::base::Placement;
    use gpui_kit::{AppContext, Bounds, Entity, IntoElement, TestAppContext, div};
    use std::{cell::Cell, rc::Rc};

    fn card(owner: &str) -> Card {
        Card {
            owner: owner.to_owned().into(),
            anchor: Rc::new(Cell::new(Bounds::default())),
            placement: Placement::Right,
            build: Rc::new(|_, _| div().into_any_element()),
        }
    }

    fn showing(layer: &Entity<HoverCards>, cx: &mut TestAppContext) -> Option<String> {
        layer.read_with(cx, |layer, _| {
            layer
                .card
                .as_ref()
                .filter(|_| layer.open)
                .map(|card| card.owner.to_string())
        })
    }

    #[gpui_kit::test]
    fn first_card_waits_for_intent_and_neighbours_open_at_once(cx: &mut TestAppContext) {
        let layer = cx.update(|cx| cx.new(|_| HoverCards::new()));
        layer.update(cx, |layer, cx| layer.show(card("a"), cx));
        cx.executor().advance_clock(INTENT / 2);
        cx.run_until_parked();
        assert_eq!(showing(&layer, cx), None);
        cx.executor().advance_clock(INTENT);
        cx.run_until_parked();
        assert_eq!(showing(&layer, cx).as_deref(), Some("a"));

        // Moving to the next row: the old trigger leaves, the new one opens at once,
        // in either order.
        layer.update(cx, |layer, cx| {
            layer.hide(&"a".into(), cx);
            layer.show(card("b"), cx);
        });
        assert_eq!(showing(&layer, cx).as_deref(), Some("b"));
        layer.update(cx, |layer, cx| {
            layer.show(card("c"), cx);
            layer.hide(&"b".into(), cx);
        });
        assert_eq!(showing(&layer, cx).as_deref(), Some("c"));

        // After the grace period the next card waits again.
        layer.update(cx, |layer, cx| layer.hide(&"c".into(), cx));
        cx.executor().advance_clock(GRACE);
        cx.run_until_parked();
        layer.update(cx, |layer, cx| layer.show(card("a"), cx));
        assert_eq!(showing(&layer, cx), None);
    }

    #[gpui_kit::test]
    fn a_press_dismisses_without_a_grace_period(cx: &mut TestAppContext) {
        let layer = cx.update(|cx| cx.new(|_| HoverCards::new()));
        layer.update(cx, |layer, cx| layer.show(card("a"), cx));
        cx.executor().advance_clock(INTENT);
        cx.run_until_parked();
        layer.update(cx, |layer, cx| {
            layer.dismiss(cx);
            layer.hide(&"a".into(), cx);
            layer.show(card("b"), cx);
        });
        assert_eq!(showing(&layer, cx), None);
    }

    #[gpui_kit::test]
    fn a_control_with_its_own_tooltip_hides_the_card_meanwhile(cx: &mut TestAppContext) {
        let layer = cx.update(|cx| cx.new(|_| HoverCards::new()));
        layer.update(cx, |layer, cx| layer.show(card("a"), cx));
        cx.executor().advance_clock(INTENT);
        cx.run_until_parked();
        layer.update(cx, |layer, cx| layer.suppress(true, cx));
        assert_eq!(showing(&layer, cx), None);
        layer.update(cx, |layer, cx| layer.suppress(false, cx));
        assert_eq!(showing(&layer, cx).as_deref(), Some("a"));
    }
}

/// Building blocks of a card's content, so every card reads the same: a title,
/// a status sentence, a rule, then one icon-led line per fact.
pub mod content {
    use super::super::{column, row};
    use crate::ui::theme as t;
    use gpui_kit::component::Icon;
    use gpui_kit::*;

    pub fn body() -> Div {
        column().w(t::hover_card_width()).gap_2()
    }

    pub fn title(text: impl Into<SharedString>) -> Div {
        div()
            .text_sm()
            .line_height(t::chat_title_line_height())
            .font_weight(FontWeight::MEDIUM)
            .text_color(t::text())
            .child(text.into())
    }

    /// A colored dot, then the label (in `color`) and its explanation flowing as
    /// one sentence.
    pub fn status(dot: Hsla, label: &str, detail: &str, color: Hsla) -> Div {
        let text = if detail.is_empty() {
            label.to_owned()
        } else {
            format!("{label}  {detail}")
        };
        row()
            .items_start()
            .gap_1p5()
            .text_xs()
            .line_height(t::chat_metadata_line_height())
            .child(
                div()
                    .flex_shrink_0()
                    .mt(rems(0.3125))
                    .size(rems(0.375))
                    .rounded_full()
                    .bg(dot),
            )
            .child(div().flex_1().min_w_0().text_color(t::muted()).child(
                StyledText::new(text).with_highlights([(
                    0..label.len(),
                    HighlightStyle {
                        color: Some(color),
                        font_weight: Some(FontWeight::MEDIUM),
                        ..Default::default()
                    },
                )]),
            ))
    }

    pub fn separator() -> Div {
        div().h_px().bg(t::border())
    }

    pub fn facts() -> Div {
        column()
            .gap_1()
            .text_xs()
            .line_height(t::chat_metadata_line_height())
            .text_color(t::secondary())
    }

    /// An icon-led line; the icon stands in for the fact's label.
    pub fn fact(icon: Icon) -> Div {
        row().w_full().min_w_0().gap_2().child(
            div()
                .flex_shrink_0()
                .text_color(t::faint())
                .child(icon.size_3()),
        )
    }

    pub fn error(text: impl Into<SharedString>) -> Div {
        fact(Icon::default().path("canopy/bang.svg").text_color(t::red()))
            .items_start()
            .text_color(t::red())
            .child(div().min_w_0().child(text.into()))
    }
}
