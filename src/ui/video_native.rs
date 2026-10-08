//! Main-thread-only AVPlayerLayer ownership. It never owns the GPUI host window.
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    ffi::{CString, c_void},
    ptr::NonNull,
};
unsafe extern "C" {
    fn canopy_video_create(view: *mut c_void, path: *const std::ffi::c_char) -> *mut c_void;
    fn canopy_video_frame(p: *mut c_void, x: f64, y: f64, w: f64, h: f64);
    fn canopy_video_hide(p: *mut c_void);
    fn canopy_video_play(p: *mut c_void, play: bool);
    fn canopy_video_seek(p: *mut c_void, time: f64);
    fn canopy_video_status(
        p: *mut c_void,
        time: *mut f64,
        duration: *mut f64,
        playing: *mut bool,
    ) -> i32;
    fn canopy_video_destroy(p: *mut c_void);
}
pub struct Player(NonNull<c_void>);
impl Player {
    pub fn new(path: &std::path::Path, window: &Window) -> Result<Self, String> {
        let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("Video preview requires macOS.".into());
        };
        let path = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: borrowed live NSView, called on its GPUI thread; native bridge retains its own layer.
        NonNull::new(unsafe { canopy_video_create(handle.ns_view.as_ptr(), path.as_ptr()) })
            .map(Self)
            .ok_or("Could not create the system video player.".into())
    }
    pub fn frame(&self, b: Bounds<Pixels>) {
        unsafe {
            canopy_video_frame(
                self.0.as_ptr(),
                f32::from(b.origin.x) as f64,
                f32::from(b.origin.y) as f64,
                f32::from(b.size.width) as f64,
                f32::from(b.size.height) as f64,
            )
        }
    }
    pub fn hide(&self) {
        unsafe { canopy_video_hide(self.0.as_ptr()) }
    }
    pub fn play(&self, playing: bool) {
        unsafe { canopy_video_play(self.0.as_ptr(), playing) }
    }
    pub fn seek(&self, time: f64) {
        unsafe { canopy_video_seek(self.0.as_ptr(), time) }
    }
    pub fn status(&self) -> (i32, f64, f64, bool) {
        let (mut time, mut duration, mut playing) = (0., 0., false);
        let status =
            unsafe { canopy_video_status(self.0.as_ptr(), &mut time, &mut duration, &mut playing) };
        (
            status,
            if time.is_finite() { time } else { 0. },
            if duration.is_finite() { duration } else { 0. },
            playing,
        )
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        unsafe { canopy_video_destroy(self.0.as_ptr()) }
    }
}
