//! GPUI's layout inspector, picked like a browser's (development builds only). GPUI picks
//! the frontmost element under the pointer, and gpui-kit's `Root` always draws invisible
//! full-window layers over the app (notifications, tooltips, menus), so every first pick
//! was such a layer. This panel replaces gpui-kit's: while picking, a full-window
//! element of gpui-kit under the pointer is stepped past — with the same wheel step GPUI
//! uses for its pick depth — so hovering highlights what is visible there. The style
//! editors are still gpui-kit's (`render_inspector_states`).
use super::{components::*, theme as t};
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
    clipboard::Clipboard,
};
use gpui_kit::*;
use std::{cell::RefCell, collections::HashSet};

/// GPUI's wheel step between pick depths, in pixels.
const PICK_STEP: f32 = 36.;

/// Frames to wait for an element's bounds before taking it as content (an element
/// without a div's inspector state never reports them).
const TRIES: u8 = 3;
/// More overlays than this over one point would be a loop.
const MAX_STEPS: usize = 32;

thread_local! {
    /// How many frames the element being checked waited for its bounds.
    static WAITED: RefCell<u8> = const { RefCell::new(0) };
    /// Overlays and content found so far, each checked once.
    static LAYERS: RefCell<HashSet<InspectorElementId>> = RefCell::new(HashSet::new());
    static CONTENT: RefCell<HashSet<InspectorElementId>> = RefCell::new(HashSet::new());
    /// The element the pick depth was last set for. GPUI draws the highlight at the pick
    /// depth and resets it to the top whenever the pointer moves onto another element,
    /// so a new element under the pointer gets its depth set again at once.
    static LANDED: RefCell<Option<InspectorElementId>> = const { RefCell::new(None) };
    /// The depth was just stepped: the element it reached is where it landed.
    static STEPPED: RefCell<bool> = const { RefCell::new(false) };
}

pub fn init(cx: &mut App) {
    cx.set_inspector_renderer(Box::new(render));
}

/// An element the pointer passes through to what is seen: an absolutely positioned one
/// that paints nothing itself (drop zones, shades, gpui-kit's layers — their children
/// are picked before them), or a gpui-kit element covering the app area (its layers).
fn is_overlay(id: &InspectorElementId, window: &mut Window, cx: &mut App) -> Option<bool> {
    let (bounds, invisible) = window
        .with_inspector_state::<DivInspectorState, _>(Some(id), cx, |state, _| {
            state.as_ref().map(|state| {
                let style = &state.base_style;
                let border = [
                    style.border_widths.top,
                    style.border_widths.right,
                    style.border_widths.bottom,
                    style.border_widths.left,
                ]
                .into_iter()
                .flatten()
                .any(|width| width != AbsoluteLength::Pixels(px(0.)));
                let invisible = style.position == Some(Position::Absolute)
                    && style.background.is_none()
                    && !border
                    && style.box_shadow.as_ref().is_none_or(Vec::is_empty);
                (state.bounds, invisible)
            })
        })
        .flatten()?;
    // GPUI and every gpui-kit crate (gpui-pre, gpui-base, gpui-component, …).
    let kit = id.path.source_location.file().contains("/gpui-");
    // The inspector takes the window's right side: the app area is narrower.
    let viewport = window.viewport_size();
    let covers = bounds.origin.x <= px(0.)
        && bounds.origin.y <= px(0.)
        && bounds.size.width >= viewport.width / 2.
        && bounds.size.height >= viewport.height - px(1.);
    Some(invisible || (kit && covers))
}

/// One pick depth deeper, at the pointer.
fn step_deeper(window: &mut Window, cx: &mut App) {
    let event = ScrollWheelEvent {
        position: window.mouse_position(),
        delta: ScrollDelta::Pixels(point(px(0.), px(PICK_STEP))),
        modifiers: window.modifiers(),
        touch_phase: TouchPhase::Moved,
    };
    window.dispatch_event(PlatformInput::ScrollWheel(event), cx);
}

/// Whether the inspector is on a known overlay now (a step changes it at once).
fn on_overlay(window: &mut Window, cx: &mut App) -> bool {
    let known: Vec<InspectorElementId> =
        LAYERS.with(|layers| layers.borrow().iter().cloned().collect());
    known.iter().any(|id| {
        window
            .with_inspector_state::<DivInspectorState, _>(Some(id), cx, |_, _| ())
            .is_some()
    })
}

/// Steps past every known overlay in one go, without waiting a frame for each; the
/// highlight is redrawn at the depth reached.
fn step_past_overlays(window: &mut Window, cx: &mut App) {
    STEPPED.with(|stepped| *stepped.borrow_mut() = true);
    for _ in 0..MAX_STEPS {
        if !on_overlay(window, cx) {
            break;
        }
        step_deeper(window, cx);
    }
    window.refresh();
}

/// Content under the pointer: where the depth was stepped to, or one the pointer moved
/// onto, whose depth GPUI reset to the top — stepped down again from there.
fn reached(id: InspectorElementId, window: &mut Window, cx: &mut App) {
    if STEPPED.with(|stepped| std::mem::take(&mut *stepped.borrow_mut())) {
        LANDED.with(|landed| *landed.borrow_mut() = Some(id));
    } else if LANDED.with(|landed| landed.borrow().as_ref() != Some(&id)) {
        step_deeper(window, cx);
        step_past_overlays(window, cx);
    }
}

fn render(
    inspector: &mut Inspector,
    window: &mut Window,
    cx: &mut Context<Inspector>,
) -> AnyElement {
    let picking = inspector.is_picking();
    let active = inspector.active_element_id().cloned();
    if picking && let Some(id) = active.clone() {
        let overlay = LAYERS.with(|layers| layers.borrow().contains(&id));
        let content = CONTENT.with(|content| content.borrow().contains(&id));
        window.defer(cx, move |window, cx| {
            if overlay {
                step_past_overlays(window, cx);
                return;
            }
            if content {
                reached(id, window, cx);
                return;
            }
            // Its bounds are known once it was painted as the active element.
            let waited = WAITED.with(|waited| *waited.borrow());
            match is_overlay(&id, window, cx) {
                None if waited < TRIES => {
                    WAITED.with(|waited| *waited.borrow_mut() += 1);
                    window.refresh();
                }
                Some(true) => {
                    WAITED.with(|waited| *waited.borrow_mut() = 0);
                    LAYERS.with(|layers| layers.borrow_mut().insert(id));
                    step_past_overlays(window, cx);
                }
                _ => {
                    WAITED.with(|waited| *waited.borrow_mut() = 0);
                    CONTENT.with(|content| content.borrow_mut().insert(id.clone()));
                    reached(id, window, cx);
                }
            }
        });
    }
    let source = active
        .as_ref()
        .map(|id| SharedString::from(id.path.source_location.to_string()));
    let global = active.as_ref().map(|id| id.path.global_id.to_string());
    column()
        .id("dev-inspector")
        .size_full()
        .bg(t::bg())
        .border_l_1()
        .border_color(t::border())
        .text_color(t::text())
        .child(
            row()
                .h(px(36.))
                .px(px(8.))
                .gap(px(6.))
                .border_b_1()
                .border_color(t::border())
                .child(
                    Button::new("dev-inspector-pick")
                        .icon(custom_icon("braces"))
                        .small()
                        .ghost()
                        .tooltip("Pick an element")
                        .on_click(cx.listener(|inspector, _, window, _| {
                            LANDED.with(|landed| *landed.borrow_mut() = None);
                            inspector.start_picking();
                            window.refresh();
                        })),
                )
                .child(div().flex_1().text_size(px(13.)).child(if picking {
                    "Inspector — hover an element, click to select"
                } else {
                    "Inspector"
                }))
                .child(
                    icon_button(
                        "dev-inspector-close",
                        gpui_kit::component::IconName::Close,
                        "Close the inspector",
                    )
                    .on_click(|_, window, cx| window.toggle_inspector(cx)),
                ),
        )
        .child(
            column()
                .id("dev-inspector-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(12.))
                .gap(px(12.))
                .text_size(px(12.))
                .children(source.map(|source| {
                    row()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family(t::MONO)
                                .text_color(t::secondary())
                                .child(source.clone()),
                        )
                        .child(Clipboard::new("dev-inspector-copy").value(source))
                }))
                .children(global.map(|global| {
                    div()
                        .font_family(t::MONO)
                        .text_size(px(11.))
                        .text_color(t::muted())
                        .child(global)
                }))
                .children(inspector.render_inspector_states(window, cx)),
        )
        .into_any_element()
}
