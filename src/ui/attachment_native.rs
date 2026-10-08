//! QLPreviewView ownership on the GPUI thread; its temporary file outlives native callbacks.
use canopy_desktop::integrations::attachments::PreviewFile;
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    ffi::{CString, c_char, c_void},
    ptr::NonNull,
    sync::Arc,
};
unsafe extern "C" {
    fn canopy_attachment_create(
        view: *mut c_void,
        path: *const c_char,
        context: *mut c_void,
        close: extern "C" fn(*mut c_void),
    ) -> *mut c_void;
    fn canopy_attachment_frame(view: *mut c_void, x: f64, y: f64, w: f64, h: f64, alpha: f64);
    fn canopy_attachment_destroy(view: *mut c_void);
}
pub struct NativePreview {
    view: NonNull<c_void>,
    close: Box<async_channel::Sender<()>>,
    _file: Arc<PreviewFile>,
}
extern "C" fn close(context: *mut c_void) {
    // SAFETY: the sender stays boxed until the native monitor is removed in Drop.
    let sender = unsafe { &*(context as *const async_channel::Sender<()>) };
    let _ = sender.try_send(());
}
impl NativePreview {
    pub fn new(
        file: Arc<PreviewFile>,
        sender: async_channel::Sender<()>,
        window: &Window,
    ) -> Result<Self, String> {
        let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("System preview requires macOS.".into());
        };
        let path =
            CString::new(file.path().as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        let mut sender = Box::new(sender);
        // SAFETY: live NSView, main thread; the bridge owns only its preview and monitor.
        let view = NonNull::new(unsafe {
            canopy_attachment_create(
                handle.ns_view.as_ptr(),
                path.as_ptr(),
                (&mut *sender as *mut async_channel::Sender<()>).cast(),
                close,
            )
        })
        .ok_or("Could not create the system preview.")?;
        Ok(Self {
            view,
            close: sender,
            _file: file,
        })
    }
    pub fn frame(&self, b: Bounds<Pixels>, alpha: f32) {
        unsafe {
            canopy_attachment_frame(
                self.view.as_ptr(),
                f32::from(b.origin.x) as f64,
                f32::from(b.origin.y) as f64,
                f32::from(b.size.width) as f64,
                f32::from(b.size.height) as f64,
                alpha as f64,
            )
        }
    }
}
impl Drop for NativePreview {
    fn drop(&mut self) {
        unsafe { canopy_attachment_destroy(self.view.as_ptr()) };
        self.close.close();
    }
}
