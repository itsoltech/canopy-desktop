//! Native window gestures without imposing the caller's visual styling.
#[cfg(any(target_os = "windows", test))]
use super::icon;
use super::row;
#[cfg(any(target_os = "windows", test))]
use crate::ui::theme as t;
#[cfg(any(target_os = "windows", test))]
use gpui_kit::component::IconName;
use gpui_kit::{base::InteractiveElementExt, *};

pub fn titlebar_options(title: impl Into<SharedString>) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title.into()),
        appears_transparent: true,
        #[cfg(target_os = "macos")]
        traffic_light_position: Some(point(px(12.), px(12.))),
        #[cfg(not(target_os = "macos"))]
        traffic_light_position: None,
    }
}

pub const fn titlebar_leading_inset() -> f32 {
    if cfg!(target_os = "macos") { 80. } else { 12. }
}

pub const fn titlebar_trailing_inset() -> f32 {
    if cfg!(target_os = "windows") {
        138.
    } else {
        0.
    }
}

#[cfg(any(target_os = "windows", test))]
const CAPTION_BUTTON_WIDTH: f32 = 46.;
#[cfg(any(target_os = "windows", test))]
const CAPTION_CONTROLS_WIDTH: f32 = CAPTION_BUTTON_WIDTH * 3.;
#[cfg(any(target_os = "windows", test))]
const APP_CONTROL_LANE: f32 = 44.;

#[cfg(any(target_os = "windows", test))]
fn windows_drag_insets(workspace_controls: bool) -> (f32, f32) {
    if workspace_controls {
        (APP_CONTROL_LANE, CAPTION_CONTROLS_WIDTH + APP_CONTROL_LANE)
    } else {
        (0., CAPTION_CONTROLS_WIDTH)
    }
}

#[cfg(any(target_os = "windows", test))]
#[cfg_attr(test, allow(dead_code))]
fn caption_button(
    id: &'static str,
    glyph: IconName,
    area: WindowControlArea,
    label: &'static str,
    close: bool,
) -> Stateful<Div> {
    row()
        .id(id)
        .w(px(CAPTION_BUTTON_WIDTH))
        .h_full()
        .justify_center()
        .aria_label(label)
        .window_control_area(area)
        .hover(move |style| {
            if close {
                style.bg(t::red()).text_color(t::text())
            } else {
                style.bg(t::hover()).text_color(t::text())
            }
        })
        .active(move |style| {
            if close {
                style.bg(t::red().opacity(0.8)).text_color(t::text())
            } else {
                style.bg(t::selected()).text_color(t::text())
            }
        })
        .child(icon(glyph).size(px(12.)))
}

#[cfg(any(target_os = "windows", test))]
#[cfg_attr(test, allow(dead_code))]
fn windows_titlebar(
    id: impl Into<ElementId>,
    window: &Window,
    workspace_controls: bool,
) -> Stateful<Div> {
    let (left, right) = windows_drag_insets(workspace_controls);
    row()
        .id(id)
        .relative()
        // GPUI selects the first matching window-control hitbox. A Drag area on
        // this parent would cover every child and turn their clicks into HTCAPTION.
        .child(
            div()
                .id("windows-titlebar-drag")
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(left))
                .right(px(right))
                .window_control_area(WindowControlArea::Drag),
        )
        .child(
            row()
                .id("windows-caption-controls")
                .absolute()
                .right_0()
                .top_0()
                .h_full()
                .child(caption_button(
                    "window-minimize",
                    IconName::WindowMinimize,
                    WindowControlArea::Min,
                    "Minimize window",
                    false,
                ))
                .child(caption_button(
                    "window-maximize",
                    if window.is_maximized() {
                        IconName::WindowRestore
                    } else {
                        IconName::WindowMaximize
                    },
                    WindowControlArea::Max,
                    if window.is_maximized() {
                        "Restore window"
                    } else {
                        "Maximize window"
                    },
                    false,
                ))
                .child(caption_button(
                    "window-close",
                    IconName::WindowClose,
                    WindowControlArea::Close,
                    "Close window",
                    true,
                )),
        )
}

fn titlebar(id: impl Into<ElementId>, window: &Window, workspace_controls: bool) -> Stateful<Div> {
    #[cfg(not(target_os = "windows"))]
    let _ = workspace_controls;
    #[cfg(not(target_os = "windows"))]
    let titlebar = row().id(id).window_control_area(WindowControlArea::Drag);
    #[cfg(target_os = "windows")]
    let titlebar = windows_titlebar(id, window, workspace_controls);
    #[cfg(target_os = "macos")]
    let titlebar = titlebar.on_double_click(|_, window, _| window.titlebar_double_click());
    #[cfg(not(target_os = "windows"))]
    let _ = window;
    titlebar
}

pub fn window_titlebar(id: impl Into<ElementId>, window: &Window) -> Stateful<Div> {
    titlebar(id, window, false)
}

pub fn workspace_titlebar(id: impl Into<ElementId>, window: &Window) -> Stateful<Div> {
    titlebar(id, window, true)
}

#[cfg(test)]
mod tests {
    use super::{CAPTION_BUTTON_WIDTH, CAPTION_CONTROLS_WIDTH, windows_drag_insets};
    use core::prelude::v1::test;

    #[test]
    fn windows_workspace_drag_area_does_not_overlap_any_button_lane() {
        let width = 800.;
        let (left, right) = windows_drag_insets(true);
        let drag = left..width - right;
        let left_sidebar_center = 22.;
        let right_sidebar_center = width - CAPTION_CONTROLS_WIDTH - 12. - 10.;
        let minimize_center = width - CAPTION_CONTROLS_WIDTH + 23.;
        let maximize_center = minimize_center + CAPTION_BUTTON_WIDTH;
        let close_center = maximize_center + CAPTION_BUTTON_WIDTH;
        for control in [
            left_sidebar_center,
            right_sidebar_center,
            minimize_center,
            maximize_center,
            close_center,
        ] {
            assert!(!drag.contains(&control), "control at {control}");
        }
        assert!(drag.contains(&(width / 2.)));
    }

    #[test]
    fn secondary_window_keeps_caption_controls_outside_its_drag_area() {
        let width = 800.;
        let (left, right) = windows_drag_insets(false);
        assert_eq!(left, 0.);
        assert_eq!(right, CAPTION_CONTROLS_WIDTH);
        assert!((left..width - right).contains(&(width / 2.)));
        assert!(!(left..width - right).contains(&(width - 23.)));
    }
}
