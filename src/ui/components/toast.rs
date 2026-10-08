//! Presentational toast parts. Host supplies motion values and event handlers.
use super::{super::theme as t, icon_button, row};
use gpui_kit::{
    base::{Disableable, Toast},
    component::{IconName, button::Button},
    *,
};

const TOAST_RADIUS: f32 = 8.;

fn surface<T: Styled>(element: T) -> T {
    element
        .p(px(12.))
        .rounded(px(TOAST_RADIUS))
        .bg(t::elevated())
        .border_1()
        .border_color(t::control_border())
}

pub fn card(message: SharedString, elapsed: f32, close_action: impl IntoElement) -> Toast {
    surface(Toast::new("result-toast"))
        .gap(px(8.))
        .child(
            div()
                .size(px(6.))
                .flex_shrink_0()
                .rounded_full()
                .bg(t::accent()),
        )
        .child(
            div()
                .id("toast-message")
                .flex_1()
                .min_w_0()
                .max_h(px(120.))
                .overflow_y_scroll()
                .text_size(px(12.))
                .text_color(t::text())
                .child(message),
        )
        .child(close_action)
        .child(progress_bar(elapsed))
}

pub fn back_card(inset: f32, visibility: f32) -> Div {
    surface(row())
        .absolute()
        .top(px(-inset))
        .bottom(px(inset))
        .left(px(inset))
        .right(px(inset))
        .opacity(visibility * 0.8)
        .overflow_hidden()
}

pub fn progress_bar(elapsed: f32) -> Div {
    // GPUI overflow masks are rectangular. Paint a rounded surface through a
    // bottom-strip mask so the bar follows the toast's inner corner exactly.
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let strip = Bounds::new(
                    point(bounds.left(), bounds.bottom() - px(2.)),
                    size(bounds.size.width * elapsed.clamp(0., 1.), px(2.)),
                );
                window.with_content_mask(Some(ContentMask { bounds: strip }), |window| {
                    window
                        .paint_quad(fill(bounds, t::accent()).corner_radii(px(TOAST_RADIUS - 1.)));
                });
            },
        )
        .size_full(),
    )
}

pub fn close_button(hovered: bool, open: bool) -> Button {
    icon_button("dismiss-toast", IconName::Close, "Dismiss notification")
        .opacity(if hovered { 1. } else { 0. })
        .disabled(!hovered || !open)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}
