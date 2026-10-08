//! Read-only preview presentation; controllers own assets, playback and callbacks.
use super::{super::theme as t, column, row};
use gpui_kit::*;

pub fn preview_surface() -> Div {
    column().size_full().bg(t::bg())
}

pub fn fitted_image(source: impl Into<ImageSource>) -> Img {
    img(source).size_full().object_fit(ObjectFit::Contain)
}

pub fn media_seek_bar(id: impl Into<ElementId>, ratio: f32) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .h(px(8.))
        .flex_shrink_0()
        .bg(t::hover())
        .rounded(px(4.))
        .cursor_pointer()
        .child(
            div()
                .h_full()
                .w(relative(ratio.clamp(0., 1.)))
                .bg(t::accent())
                .rounded(px(4.)),
        )
}

pub fn playback_controls(
    back: impl IntoElement,
    play: impl IntoElement,
    forward: impl IntoElement,
    time: f64,
    duration: f64,
) -> Div {
    row()
        .gap(px(8.))
        .flex_shrink_0()
        .child(back)
        .child(play)
        .child(forward)
        .child(
            div()
                .text_size(px(11.))
                .text_color(t::secondary())
                .child(format!("{} / {}", time_label(time), time_label(duration))),
        )
}

fn time_label(time: f64) -> String {
    let seconds = time.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
