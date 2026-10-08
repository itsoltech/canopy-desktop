//! Scoped Win32 adapter for the nonactivating notch overlay.
use super::notch_geometry::{InputRegion, NativeEvent};
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{cell::Cell, ptr};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SetLastError, WPARAM},
    Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, GetMonitorInfoW,
        MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
        MonitorFromWindow, RGN_ERROR, RGN_OR, ScreenToClient, SetWindowRgn,
    },
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        HiDpi::GetDpiForWindow,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            CHILDID_SELF, CallNextHookEx, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_STATECHANGE,
            EVENT_SYSTEM_FOREGROUND, GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetCursorPos,
            GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HHOOK,
            HWND_TOPMOST, IsWindowVisible, MA_NOACTIVATE, MSLLHOOKSTRUCT, OBJID_WINDOW,
            PostMessageW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW,
            ShowWindow, UnhookWindowsHookEx, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT, WM_APP,
            WM_DISPLAYCHANGE, WM_DPICHANGED, WM_MOUSEACTIVATE, WM_NCDESTROY, WM_SETTINGCHANGE,
            WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_THICKFRAME,
        },
    },
};

const SUBCLASS_ID: usize = 0x4341_4e4f_5059;
const WM_NOTCH_FULLSCREEN: u32 = WM_APP + 0x3a;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhysicalRegion {
    left: i32,
    right: i32,
    height: i32,
    radius: i32,
}

fn scale_region(region: InputRegion, client: RECT, scale: f32) -> PhysicalRegion {
    if region.width <= 0. || region.height <= 0. {
        return PhysicalRegion {
            left: 0,
            right: 0,
            height: 0,
            radius: 0,
        };
    }
    let canvas_width = (client.right - client.left).max(0);
    let canvas_height = (client.bottom - client.top).max(0);
    let width = ((region.width * scale).round() as i32).clamp(0, canvas_width);
    let height = ((region.height * scale).round() as i32).clamp(0, canvas_height);
    let radius = (region.radius * scale).round() as i32;
    let left = (canvas_width - width) / 2;
    PhysicalRegion {
        left,
        right: left + width,
        height,
        radius: radius.min(width / 2).min(height).max(0),
    }
}

fn anchor_position(monitor: RECT, work: RECT, window_width: i32) -> POINT {
    let available_width = (work.right - work.left).max(0);
    POINT {
        x: work.left + (available_width - window_width).max(0) / 2,
        y: work.top.max(monitor.top),
    }
}

fn should_hide_for_foreground(
    bounds: RECT,
    monitor: RECT,
    style: u32,
    same_process: bool,
    same_monitor: bool,
    visible: bool,
) -> bool {
    visible
        && !same_process
        && same_monitor
        && bounds.left <= monitor.left
        && bounds.top <= monitor.top
        && bounds.right >= monitor.right
        && bounds.bottom >= monitor.bottom
        && style & (WS_CAPTION | WS_THICKFRAME) == 0
}

fn begin_coalesced_update(pending: &Cell<bool>) -> bool {
    !pending.replace(true)
}

struct NativeState {
    hwnd: HWND,
    region: Cell<InputRegion>,
    applied: Cell<Option<PhysicalRegion>>,
    inside: Cell<bool>,
    hidden_for_fullscreen: Cell<bool>,
    disabled: Cell<bool>,
    fullscreen_update_pending: Cell<bool>,
    destroyed: Cell<bool>,
    on_change: Box<dyn Fn(NativeEvent)>,
}

impl NativeState {
    fn scale(&self) -> f32 {
        // SAFETY: hwnd belongs to the live notch window while this state is active.
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 { 1. } else { dpi as f32 / 96. }
    }

    fn physical_region(&self) -> Result<PhysicalRegion, String> {
        let mut client: RECT = unsafe { std::mem::zeroed() };
        // SAFETY: client points to writable storage and hwnd is live.
        if unsafe { GetClientRect(self.hwnd, &mut client) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let region = self.region.get();
        Ok(scale_region(region, client, self.scale()))
    }

    fn apply_region(&self) -> Result<(), String> {
        if self.destroyed.get() {
            return Ok(());
        }
        let physical = self.physical_region()?;
        if self.applied.get() == Some(physical) {
            return Ok(());
        }
        let region = if physical.right <= physical.left || physical.height <= 0 {
            // SAFETY: coordinates describe an empty region.
            unsafe { CreateRectRgn(0, 0, 0, 0) }
        } else {
            // SAFETY: coordinates are bounded by the fixed notch client rectangle.
            let rounded = unsafe {
                CreateRoundRectRgn(
                    physical.left,
                    0,
                    physical.right,
                    physical.height,
                    physical.radius * 2,
                    physical.radius * 2,
                )
            };
            if rounded.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            // Unioning the top body keeps only the two bottom corners rounded.
            let top = unsafe {
                CreateRectRgn(
                    physical.left,
                    0,
                    physical.right,
                    (physical.height - physical.radius).max(0),
                )
            };
            if top.is_null() {
                unsafe { DeleteObject(rounded) };
                return Err(std::io::Error::last_os_error().to_string());
            }
            let combined = unsafe { CombineRgn(rounded, rounded, top, RGN_OR) };
            unsafe { DeleteObject(top) };
            if combined == RGN_ERROR {
                unsafe { DeleteObject(rounded) };
                return Err("Could not combine the Windows notch input region.".into());
            }
            rounded
        };
        if region.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SAFETY: SetWindowRgn takes ownership only on success.
        if unsafe { SetWindowRgn(self.hwnd, region, 1) } == 0 {
            unsafe { DeleteObject(region) };
            return Err(std::io::Error::last_os_error().to_string());
        }
        self.applied.set(Some(physical));
        Ok(())
    }

    fn update_pointer(&self, mut point: POINT) {
        if self.destroyed.get() || self.hidden_for_fullscreen.get() || self.disabled.get() {
            return;
        }
        // SAFETY: point is writable and hwnd is live.
        let inside = if unsafe { ScreenToClient(self.hwnd, &mut point) } == 0 {
            false
        } else {
            let scale = self.scale();
            self.region
                .get()
                .contains_xy(point.x as f32 / scale, point.y as f32 / scale)
        };
        if self.inside.replace(inside) != inside {
            (self.on_change)(NativeEvent::Pointer(inside));
        }
    }

    fn update_current_pointer(&self) {
        let mut point: POINT = unsafe { std::mem::zeroed() };
        // SAFETY: point is writable.
        if unsafe { GetCursorPos(&mut point) } != 0 {
            self.update_pointer(point);
        }
    }

    fn foreign_fullscreen(&self) -> bool {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null() || foreground == self.hwnd {
            return false;
        }
        let visible = unsafe { IsWindowVisible(foreground) } != 0;
        let mut process = 0;
        unsafe { GetWindowThreadProcessId(foreground, &mut process) };
        let same_process = process == std::process::id();
        let mut bounds: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(foreground, &mut bounds) } == 0 {
            return false;
        }
        let monitor = unsafe {
            MonitorFromPoint(
                POINT {
                    x: bounds.left,
                    y: bounds.top,
                },
                MONITOR_DEFAULTTONEAREST,
            )
        };
        if monitor.is_null() {
            return false;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return false;
        }
        let style = unsafe { GetWindowLongPtrW(foreground, GWL_STYLE) } as u32;
        let notch_monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
        should_hide_for_foreground(
            bounds,
            info.rcMonitor,
            style,
            same_process,
            monitor == notch_monitor,
            visible,
        )
    }

    fn update_visibility(&self) {
        if self.destroyed.get() {
            return;
        }
        if self.disabled.get() {
            let _ = self.recover();
            return;
        }
        let hidden = self.foreign_fullscreen();
        if self.hidden_for_fullscreen.replace(hidden) == hidden {
            return;
        }
        if hidden {
            if self.inside.replace(false) {
                (self.on_change)(NativeEvent::Pointer(false));
            }
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        } else {
            if anchor(self.hwnd).is_err() {
                self.fail("The Windows session notch could not be repositioned.");
                return;
            }
            self.applied.set(None);
            if self.apply_region().is_err() {
                self.fail("The Windows session notch input region could not be updated.");
                return;
            }
            unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
            self.update_current_pointer();
        }
    }

    fn fail(&self, message: &'static str) {
        let first = !self.disabled.replace(true);
        if self.inside.replace(false) {
            (self.on_change)(NativeEvent::Pointer(false));
        }
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        if first {
            (self.on_change)(NativeEvent::Failed(message));
        }
    }

    fn recover(&self) -> Result<(), String> {
        if self.destroyed.get() {
            return Err("The Windows session notch window was destroyed.".into());
        }
        self.disabled.set(false);
        if let Err(error) = anchor(self.hwnd) {
            self.disabled.set(true);
            return Err(error);
        }
        self.applied.set(None);
        if let Err(error) = self.apply_region() {
            self.disabled.set(true);
            return Err(error);
        }
        let hidden = self.foreign_fullscreen();
        self.hidden_for_fullscreen.set(hidden);
        if hidden {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        } else {
            unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
            self.update_current_pointer();
        }
        (self.on_change)(NativeEvent::Recovered);
        Ok(())
    }
}

thread_local! {
    static ACTIVE_STATE: Cell<*const NativeState> = const { Cell::new(ptr::null()) };
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        ACTIVE_STATE.with(|active| {
            let state = active.get();
            if !state.is_null() {
                // SAFETY: WH_MOUSE_LL supplies MSLLHOOKSTRUCT for this callback duration.
                let event = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
                // SAFETY: the adapter clears TLS only after unhooking this callback.
                unsafe { &*state }.update_pointer(event.pt);
            }
        });
    }
    // SAFETY: forwarding preserves all messages not consumed by this observation-only hook.
    unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn win_event_callback(
    _: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if matches!(
        event,
        EVENT_OBJECT_STATECHANGE | EVENT_OBJECT_LOCATIONCHANGE
    ) && (object != OBJID_WINDOW
        || child != CHILDID_SELF as i32
        || hwnd != unsafe { GetForegroundWindow() })
    {
        return;
    }
    ACTIVE_STATE.with(|active| {
        let state = active.get();
        if !state.is_null() {
            // SAFETY: TLS is cleared only after both native hooks are removed.
            let state = unsafe { &*state };
            if begin_coalesced_update(&state.fullscreen_update_pending) {
                // SAFETY: message targets only the live scoped notch subclass.
                if unsafe { PostMessageW(state.hwnd, WM_NOTCH_FULLSCREEN, 0, 0) } == 0 {
                    state.fullscreen_update_pending.set(false);
                    state.fail("The Windows session notch could not observe fullscreen changes.");
                }
            }
        }
    });
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    reference: usize,
) -> LRESULT {
    if message == WM_MOUSEACTIVATE {
        return MA_NOACTIVATE as LRESULT;
    }
    // SAFETY: reference points to the boxed state for the lifetime of the subclass.
    let state = unsafe { &*(reference as *const NativeState) };
    if message == WM_NOTCH_FULLSCREEN {
        state.fullscreen_update_pending.set(false);
        state.update_visibility();
        return 0;
    }
    if message == WM_NCDESTROY {
        state.destroyed.set(true);
        // SAFETY: remove only this adapter's scoped subclass.
        unsafe { RemoveWindowSubclass(hwnd, Some(subclass), subclass_id) };
    }
    // Let GPUI process DPI/display changes first, then restore the product anchor and region.
    let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    if matches!(message, WM_DPICHANGED | WM_DISPLAYCHANGE | WM_SETTINGCHANGE)
        && !state.destroyed.get()
    {
        let repositioned = anchor(hwnd);
        state.applied.set(None);
        let clipped = state.apply_region();
        if repositioned.is_err() || clipped.is_err() {
            state.fail("The Windows session notch could not adapt to the display change.");
        } else {
            state.update_current_pointer();
            state.update_visibility();
        }
    }
    result
}

fn hwnd(window: &Window) -> gpui_kit::Result<HWND> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(std::io::Error::other("Not a Win32 window").into());
    };
    Ok(handle.hwnd.get() as HWND)
}

fn configure_window(hwnd: HWND) -> Result<(), String> {
    // SAFETY: style access is scoped to the live notch HWND.
    let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let desired =
        current | WS_EX_NOACTIVATE as isize | WS_EX_TOOLWINDOW as isize | WS_EX_TOPMOST as isize;
    unsafe { SetLastError(0) };
    if unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired) } == 0
        && unsafe { GetLastError() } != 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: preserve size and move while applying only this window's nonactivation styles.
    if unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

fn anchor(hwnd: HWND) -> Result<(), String> {
    // Windows places the primary monitor at virtual-desktop origin (0, 0).
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    if monitor.is_null() {
        return Err("Primary Windows monitor is unavailable.".into());
    }
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut window: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let width = window.right - window.left;
    let position = anchor_position(info.rcMonitor, info.rcWork, width);
    if unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            position.x,
            position.y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

/// Owns the one notch subclass and low-level mouse hook on the UI thread.
pub struct PointerMonitor {
    state: Box<NativeState>,
    hook: HHOOK,
    foreground_hook: HWINEVENTHOOK,
    geometry_hook: HWINEVENTHOOK,
    subclass_installed: bool,
}

impl PointerMonitor {
    pub fn install(
        window: &Window,
        initial: InputRegion,
        on_change: impl Fn(NativeEvent) + 'static,
    ) -> gpui_kit::Result<Self> {
        let hwnd = hwnd(window)?;
        configure_window(hwnd).map_err(std::io::Error::other)?;
        anchor(hwnd).map_err(std::io::Error::other)?;
        let state = Box::new(NativeState {
            hwnd,
            region: Cell::new(initial),
            applied: Cell::new(None),
            inside: Cell::new(false),
            hidden_for_fullscreen: Cell::new(false),
            disabled: Cell::new(false),
            fullscreen_update_pending: Cell::new(false),
            destroyed: Cell::new(false),
            on_change: Box::new(on_change),
        });
        state.apply_region().map_err(std::io::Error::other)?;
        let pointer = state.as_ref() as *const NativeState;
        // SAFETY: pointer remains stable in the owned Box until subclass removal.
        if unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, pointer as usize) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let occupied = ACTIVE_STATE.with(|active| {
            if active.get().is_null() {
                active.set(pointer);
                false
            } else {
                true
            }
        });
        if occupied {
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID) };
            return Err(
                std::io::Error::other("A Windows notch mouse hook is already active").into(),
            );
        }
        // SAFETY: WH_MOUSE_LL callback lives in this module; the installing UI thread has a loop.
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), ptr::null_mut(), 0) };
        if hook.is_null() {
            ACTIVE_STATE.with(|active| active.set(ptr::null()));
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID) };
            return Err(std::io::Error::last_os_error().into());
        }
        let foreground_hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                ptr::null_mut(),
                Some(win_event_callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if foreground_hook.is_null() {
            unsafe {
                UnhookWindowsHookEx(hook);
                RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
            }
            ACTIVE_STATE.with(|active| active.set(ptr::null()));
            return Err(std::io::Error::last_os_error().into());
        }
        let geometry_hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_STATECHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                ptr::null_mut(),
                Some(win_event_callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if geometry_hook.is_null() {
            unsafe {
                UnhookWinEvent(foreground_hook);
                UnhookWindowsHookEx(hook);
                RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
            }
            ACTIVE_STATE.with(|active| active.set(ptr::null()));
            return Err(std::io::Error::last_os_error().into());
        }
        let monitor = Self {
            state,
            hook,
            foreground_hook,
            geometry_hook,
            subclass_installed: true,
        };
        monitor.state.update_visibility();
        if !monitor.state.hidden_for_fullscreen.get() {
            monitor.state.update_current_pointer();
        }
        Ok(monitor)
    }

    pub fn set_region(&self, region: InputRegion) {
        self.state.region.set(region);
        if self.state.disabled.get() {
            return;
        }
        if let Err(error) = self.state.apply_region() {
            eprintln!("Windows notch region failed: {error}");
            self.state
                .fail("The Windows session notch input region could not be updated.");
            return;
        }
        if !self.state.hidden_for_fullscreen.get() {
            self.state.update_current_pointer();
        }
    }

    pub fn retry(&self) -> gpui_kit::Result<()> {
        self.state.recover().map_err(std::io::Error::other)?;
        Ok(())
    }
}

impl Drop for PointerMonitor {
    fn drop(&mut self) {
        // Unhook first so the TLS pointer cannot be observed during state destruction.
        unsafe {
            UnhookWinEvent(self.geometry_hook);
            UnhookWinEvent(self.foreground_hook);
            UnhookWindowsHookEx(self.hook);
        }
        let pointer = self.state.as_ref() as *const NativeState;
        ACTIVE_STATE.with(|active| {
            if active.get() == pointer {
                active.set(ptr::null());
            }
        });
        if self.subclass_installed && !self.state.destroyed.get() {
            unsafe { RemoveWindowSubclass(self.state.hwnd, Some(subclass), SUBCLASS_ID) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_region_scales_and_centers_without_screen_origin_assumptions() {
        let logical = InputRegion {
            canvas_width: 480.,
            width: 295.,
            height: 39.,
            radius: 16.,
        };
        let physical = scale_region(
            logical,
            RECT {
                left: 0,
                top: 0,
                right: 600,
                bottom: 200,
            },
            1.25,
        );
        assert_eq!(physical.left, 115);
        assert_eq!(physical.right, 484);
        assert_eq!(physical.height, 49);
        assert_eq!(physical.radius, 20);
    }

    #[test]
    fn anchor_handles_negative_origin_top_taskbar_and_small_work_area() {
        let monitor = RECT {
            left: -1920,
            top: -200,
            right: 0,
            bottom: 880,
        };
        let work = RECT {
            left: -1920,
            top: -160,
            right: 0,
            bottom: 840,
        };
        let anchored = anchor_position(monitor, work, 600);
        assert_eq!((anchored.x, anchored.y), (-1260, -160));
        let clamped = anchor_position(
            monitor,
            RECT {
                right: -1520,
                ..work
            },
            600,
        );
        assert_eq!((clamped.x, clamped.y), (-1920, -160));
    }

    #[test]
    fn f11_enter_and_exit_on_same_hwnd_changes_visibility() {
        let monitor = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(!should_hide_for_foreground(
            monitor,
            monitor,
            WS_CAPTION | WS_THICKFRAME,
            false,
            true,
            true
        ));
        assert!(should_hide_for_foreground(
            monitor, monitor, 0, false, true, true
        ));
        assert!(!should_hide_for_foreground(
            monitor,
            monitor,
            WS_CAPTION | WS_THICKFRAME,
            false,
            true,
            true
        ));
        assert!(!should_hide_for_foreground(
            monitor, monitor, 0, true, true, true
        ));
        assert!(!should_hide_for_foreground(
            monitor, monitor, 0, false, true, false
        ));
    }

    #[test]
    fn fullscreen_on_another_monitor_does_not_hide_primary_notch() {
        let secondary = RECT {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1080,
        };
        assert!(!should_hide_for_foreground(
            secondary, secondary, 0, false, false, true
        ));
    }

    #[test]
    fn repeated_window_events_coalesce_until_private_message_is_handled() {
        let pending = Cell::new(false);
        assert!(begin_coalesced_update(&pending));
        assert!(!begin_coalesced_update(&pending));
        pending.set(false);
        assert!(begin_coalesced_update(&pending));
    }
}
