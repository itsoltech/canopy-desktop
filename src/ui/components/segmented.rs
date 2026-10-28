//! Segmented control: a small, always visible set of mutually exclusive choices
//! (Light / Dark / Auto). Selection and its callback belong to the caller.
use super::super::theme as t;
use super::row;
use gpui_kit::{prelude::FluentBuilder, *};
use std::rc::Rc;

/// One silhouette: the selected segment is a raised fill inside the container's
/// inner corners. Each segment is a radio button reachable with Tab and
/// activated with Enter or Space; `label` names the group for screen readers.
pub fn segmented_control<T: Copy + PartialEq + 'static>(
    id: &'static str,
    label: impl Into<SharedString>,
    options: &[(T, &'static str)],
    selected: T,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let on_select = Rc::new(on_select);
    row()
        .id(id)
        .flex_shrink_0()
        .role(Role::RadioGroup)
        .aria_label(label)
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(6.))
        .border_1()
        .border_color(t::border())
        .bg(t::input_bg())
        .children(options.iter().enumerate().map(|(index, (value, name))| {
            let (value, active) = (*value, *value == selected);
            let on_select = on_select.clone();
            row()
                .id((id, index))
                .role(Role::RadioButton)
                .aria_label(*name)
                .aria_toggled(if active {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .tab_index(0)
                .h(px(24.))
                .px(px(10.))
                .rounded(px(4.))
                .border_1()
                .border_color(gpui_kit::transparent_black())
                .text_size(px(12.))
                .when(active, |segment| {
                    segment
                        .bg(t::elevated())
                        .text_color(t::text())
                        .shadow(vec![BoxShadow {
                            inset: false,
                            color: t::segment_shadow(),
                            offset: point(px(0.), px(1.)),
                            blur_radius: px(2.),
                            spread_radius: px(0.),
                        }])
                })
                .when(!active, |segment| {
                    segment
                        .text_color(t::secondary())
                        .hover(|style| style.bg(t::hover()).text_color(t::text()))
                })
                .focus_visible(|style| style.border_1().border_color(t::accent()))
                .on_click(move |_, window, cx| {
                    if !active {
                        on_select(value, window, cx);
                    }
                })
                .child(*name)
        }))
}
