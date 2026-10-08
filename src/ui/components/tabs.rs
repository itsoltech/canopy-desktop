use super::super::theme as t;
use super::{dot, row};
use gpui_kit::*;

pub const TAB_WIDTH: f32 = 180.;

/// Scrollable tab strip; callers supply tabs, trailing actions and drop behavior.
pub fn tab_strip(id: impl Into<ElementId>) -> Stateful<Div> {
    row()
        .id(id)
        .overflow_x_scroll()
        .h(px(t::TABS))
        .flex_shrink_0()
        .bg(t::sidebar())
        .border_b_1()
        .border_color(t::border())
}

/// Presentational tab. Identity, content indicator and selection are independent.
pub fn workspace_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    indicator: bool,
) -> Stateful<Div> {
    row()
        .id(id)
        .w(px(TAB_WIDTH))
        .flex_shrink_0()
        .h_full()
        .px(px(8.))
        .gap(px(6.))
        .cursor_pointer()
        .border_b_1()
        .border_color(if active { t::accent() } else { t::border() })
        .text_size(px(11.))
        .text_color(t::secondary())
        .children(indicator.then(dot))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
}

/// Remaining strip space. Event payloads and actions belong to its caller.
pub fn tab_drop_slot(id: impl Into<ElementId>) -> Stateful<Div> {
    row().id(id).min_w(px(40.)).flex_1().h_full()
}

/// Fixed edge hints painted after scrolling/clamping. Canvas adds no hit targets.
pub fn tab_viewport(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    scroll: ScrollHandle,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .w_full()
        .h(px(t::TABS))
        .flex_shrink_0()
        .child(content)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let offset = -f32::from(scroll.offset().x);
                    let maximum = f32::from(scroll.max_offset().x);
                    let (left, right) = edge_strengths(offset, maximum);
                    let extent = px(20.).min(bounds.size.width / 2.);
                    for (strength, origin, angle) in [
                        (left, bounds.origin, 90.),
                        (right, point(bounds.right() - extent, bounds.top()), 270.),
                    ] {
                        if strength <= 0. {
                            continue;
                        }
                        window.paint_quad(fill(
                            Bounds::new(
                                origin,
                                size(extent, (bounds.size.height - px(1.)).max(px(0.))),
                            ),
                            linear_gradient(
                                angle,
                                linear_color_stop(t::scroll_edge_shadow().opacity(strength), 0.),
                                linear_color_stop(t::scroll_edge_shadow().opacity(0.), 1.),
                            ),
                        ));
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
}

fn edge_strengths(offset: f32, maximum: f32) -> (f32, f32) {
    let maximum = maximum.max(0.);
    let offset = offset.clamp(0., maximum);
    ((offset / 20.).min(1.), ((maximum - offset) / 20.).min(1.))
}

#[cfg(test)]
mod tests {
    use super::edge_strengths;
    #[test]
    fn hints_match_start_middle_end_and_no_overflow() {
        assert_eq!(edge_strengths(0., 100.), (0., 1.));
        assert_eq!(edge_strengths(50., 100.), (1., 1.));
        assert_eq!(edge_strengths(100., 100.), (1., 0.));
        assert_eq!(edge_strengths(0., 0.), (0., 0.));
        assert_eq!(edge_strengths(5., 100.), (0.25, 1.));
    }
}
