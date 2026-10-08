//! Scoped WM_SETTINGCHANGE observation for cached Windows UI preferences.
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{WM_NCDESTROY, WM_SETTINGCHANGE},
    },
};

const SUBCLASS_ID: usize = 0x4341_4e4f_4d54;

struct State {
    callback: Box<dyn Fn()>,
    destroyed: std::cell::Cell<bool>,
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    reference: usize,
) -> LRESULT {
    // SAFETY: reference points to the boxed state while this subclass is installed.
    let state = unsafe { &*(reference as *const State) };
    if message == WM_SETTINGCHANGE {
        (state.callback)();
    } else if message == WM_NCDESTROY {
        state.destroyed.set(true);
        unsafe { RemoveWindowSubclass(hwnd, Some(subclass), id) };
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub struct SettingsMonitor {
    hwnd: HWND,
    state: Box<State>,
}

impl SettingsMonitor {
    pub fn install(window: &Window, callback: impl Fn() + 'static) -> gpui_kit::Result<Self> {
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(std::io::Error::other("Not a Win32 window").into());
        };
        let hwnd = handle.hwnd.get() as HWND;
        let state = Box::new(State {
            callback: Box::new(callback),
            destroyed: std::cell::Cell::new(false),
        });
        let pointer = state.as_ref() as *const State;
        if unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, pointer as usize) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self { hwnd, state })
    }
}

impl Drop for SettingsMonitor {
    fn drop(&mut self) {
        if !self.state.destroyed.get() {
            unsafe { RemoveWindowSubclass(self.hwnd, Some(subclass), SUBCLASS_ID) };
        }
    }
}
