//! Shared motion tokens, recipes and interruptible native transitions.
//! No timers, polling, rendering or I/O are owned by the interpolation types.
mod batch_reveal;
pub use batch_reveal::BatchReveal;
mod tokens;
mod transition;
pub use tokens::*;
pub use transition::{Presence, Transition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionPolicy {
    Full,
    Reduced,
}

static SYSTEM_REDUCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn init_system_policy() {
    let _ = refresh_system_policy();
}

pub fn refresh_system_policy() -> bool {
    #[cfg(target_os = "windows")]
    let reduced = windows::reduced_motion().unwrap_or_else(|| system_reduced());
    #[cfg(not(target_os = "windows"))]
    let reduced = false;
    SYSTEM_REDUCED.swap(reduced, std::sync::atomic::Ordering::AcqRel) != reduced
}

pub(crate) fn system_reduced() -> bool {
    SYSTEM_REDUCED.load(std::sync::atomic::Ordering::Acquire)
}

/// Resolve at a state change, outside render. Never changes system settings.
pub fn policy(cx: &gpui_kit::App) -> MotionPolicy {
    #[cfg(target_os = "macos")]
    let system_reduced =
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
    #[cfg(target_os = "windows")]
    let system_reduced = system_reduced();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let system_reduced = false;
    if cx.reduce_motion() || system_reduced {
        MotionPolicy::Reduced
    } else {
        MotionPolicy::Full
    }
}

#[cfg(target_os = "windows")]
mod windows;

/// Call from render with the combined activity of this view's lanes.
pub fn request_frame(window: &gpui_kit::Window, active: bool) {
    if active {
        window.request_animation_frame();
    }
}

/// Project the shared scale into GPUI Kit's available theme fields.
/// Component-specific spring mechanics remain owned by GPUI Kit.
pub fn apply_to_theme(theme: &mut gpui_kit::component::Theme) {
    use gpui_kit::base::Easing as NativeEasing;
    let (x1, y1, x2, y2) = Easing::SmoothOut.control_points().expect("surface curve");
    let curve = NativeEasing::cubic_bezier(x1, y1, x2, y2).expect("valid motion token");
    theme.motion.duration_instant = duration::INSTANT;
    theme.motion.duration_fast = duration::QUICK;
    theme.motion.duration_normal = duration::FAST;
    theme.motion.duration_slow = duration::SLOW;
    theme.motion.easing_enter = curve.clone();
    theme.motion.easing_exit = curve.clone();
    theme.motion.easing_move = curve;
    theme.motion.distance_short = gpui_kit::rems(distance::MICRO / 16.);
    theme.motion.distance_medium = gpui_kit::rems(distance::BASE / 16.);
}
