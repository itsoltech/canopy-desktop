//! Main-thread-only WKWebView pages (native/html_page.m): a page shown over a chat card,
//! and a page rendered off screen into an image. Neither owns the GPUI window.
use gpui_kit::*;

/// What a page reports while it is open.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum PageEvent {
    /// Height of its content, in points.
    Height(f32),
    /// A console line or uncaught error, prefixed by its level.
    Console(String),
}

/// A page rendered off screen: PNG bytes (at most the requested height) and the
/// content's full height.
pub struct Captured {
    pub png: Vec<u8>,
    pub height: f32,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{
        ffi::{CStr, CString, c_char, c_void},
        ptr::NonNull,
    };

    type EventCallback = extern "C" fn(*mut c_void, i32, *const c_char, f64);
    type ImageCallback = extern "C" fn(*mut c_void, *const u8, usize, f64);

    unsafe extern "C" {
        fn canopy_page_create(
            view: *mut c_void,
            html: *const c_char,
            css: *const c_char,
            max_height: f64,
            ctx: *mut c_void,
            on_event: EventCallback,
        ) -> *mut c_void;
        #[allow(clippy::too_many_arguments)]
        fn canopy_page_frame(
            page: *mut c_void,
            cx: f64,
            cy: f64,
            cw: f64,
            ch: f64,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        );
        fn canopy_page_hide(page: *mut c_void);
        fn canopy_page_set_active(page: *mut c_void, active: bool);
        fn canopy_page_destroy(page: *mut c_void);
        #[allow(clippy::too_many_arguments)]
        fn canopy_page_capture(
            html: *const c_char,
            css: *const c_char,
            width: f64,
            max_height: f64,
            scale: f64,
            ctx: *mut c_void,
            on_event: EventCallback,
            on_image: ImageCallback,
        ) -> *mut c_void;
    }

    /// Where the native side's callbacks go; freed after the page is destroyed.
    struct Sink {
        events: async_channel::Sender<PageEvent>,
        images: Option<async_channel::Sender<Captured>>,
    }

    extern "C" fn on_event(ctx: *mut c_void, kind: i32, text: *const c_char, value: f64) {
        // SAFETY: `ctx` is the page's live Sink; the native side drops its callbacks
        // before the Sink is freed.
        let sink = unsafe { &*(ctx as *const Sink) };
        let event = match kind {
            0 => PageEvent::Height(value as f32),
            _ if !text.is_null() => {
                // SAFETY: a NUL-terminated UTF-8 string valid for this call.
                PageEvent::Console(
                    unsafe { CStr::from_ptr(text) }
                        .to_string_lossy()
                        .into_owned(),
                )
            }
            _ => return,
        };
        let _ = sink.events.try_send(event);
    }

    extern "C" fn on_image(ctx: *mut c_void, bytes: *const u8, length: usize, height: f64) {
        // SAFETY: as in `on_event`.
        let sink = unsafe { &*(ctx as *const Sink) };
        let png = if bytes.is_null() || length == 0 {
            vec![]
        } else {
            // SAFETY: `length` bytes the native side keeps alive during the call.
            unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec()
        };
        if let Some(images) = &sink.images {
            let _ = images.try_send(Captured {
                png,
                height: height as f32,
            });
        }
    }

    fn text(value: &str) -> CString {
        CString::new(value.replace('\0', "")).expect("no NUL left")
    }

    /// A page shown over a chat card, clipped to the conversation.
    pub struct Page {
        ptr: NonNull<c_void>,
        sink: *mut Sink,
    }

    impl Page {
        pub fn new(
            html: &str,
            css: &str,
            max_height: f32,
            window: &Window,
        ) -> Result<(Self, async_channel::Receiver<PageEvent>), String> {
            let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return Err("Pages need macOS.".into());
            };
            let (events, receiver) = async_channel::unbounded();
            let sink = Box::into_raw(Box::new(Sink {
                events,
                images: None,
            }));
            let (html, css) = (text(html), text(css));
            // SAFETY: borrowed live NSView of this window, on its GPUI thread.
            let ptr = unsafe {
                canopy_page_create(
                    handle.ns_view.as_ptr(),
                    html.as_ptr(),
                    css.as_ptr(),
                    max_height as f64,
                    sink as *mut c_void,
                    on_event,
                )
            };
            match NonNull::new(ptr) {
                Some(ptr) => Ok((Self { ptr, sink }, receiver)),
                None => {
                    // SAFETY: never handed to a live page.
                    drop(unsafe { Box::from_raw(sink) });
                    Err("Could not create the page.".into())
                }
            }
        }

        /// Shows the page at `rect`, cut to `clip` (window coordinates).
        pub fn place(&self, clip: Bounds<Pixels>, rect: Bounds<Pixels>) {
            let f = |p: Pixels| f32::from(p) as f64;
            // SAFETY: live page, main thread.
            unsafe {
                canopy_page_frame(
                    self.ptr.as_ptr(),
                    f(clip.origin.x),
                    f(clip.origin.y),
                    f(clip.size.width),
                    f(clip.size.height),
                    f(rect.origin.x),
                    f(rect.origin.y),
                    f(rect.size.width),
                    f(rect.size.height),
                )
            }
        }

        pub fn hide(&self) {
            // SAFETY: live page, main thread.
            unsafe { canopy_page_hide(self.ptr.as_ptr()) }
        }

        /// Shown and taking the pointer, or only warming up: running, but transparent
        /// and letting clicks and scrolling through.
        pub fn set_active(&self, active: bool) {
            // SAFETY: live page, main thread.
            unsafe { canopy_page_set_active(self.ptr.as_ptr(), active) }
        }
    }

    impl Drop for Page {
        fn drop(&mut self) {
            // SAFETY: destroyed once; its callbacks are cleared before the Sink goes.
            unsafe {
                canopy_page_destroy(self.ptr.as_ptr());
                drop(Box::from_raw(self.sink));
            }
        }
    }

    /// A page rendering off screen; dropping it stops the render.
    pub struct Capture {
        ptr: NonNull<c_void>,
        sink: *mut Sink,
    }

    /// Renders `html` `width` wide off screen at `scale` pixels per point: the image
    /// arrives on the second receiver, console lines on the first.
    pub fn capture(
        html: &str,
        css: &str,
        width: f32,
        max_height: f32,
        scale: f32,
    ) -> Result<
        (
            Capture,
            async_channel::Receiver<PageEvent>,
            async_channel::Receiver<Captured>,
        ),
        String,
    > {
        let (events, event_receiver) = async_channel::unbounded();
        let (images, image_receiver) = async_channel::bounded(1);
        let sink = Box::into_raw(Box::new(Sink {
            events,
            images: Some(images),
        }));
        let (html, css) = (text(html), text(css));
        // SAFETY: main thread; the native side owns its off-screen window.
        let ptr = unsafe {
            canopy_page_capture(
                html.as_ptr(),
                css.as_ptr(),
                width as f64,
                max_height as f64,
                scale as f64,
                sink as *mut c_void,
                on_event,
                on_image,
            )
        };
        match NonNull::new(ptr) {
            Some(ptr) => Ok((Capture { ptr, sink }, event_receiver, image_receiver)),
            None => {
                // SAFETY: never handed to a live page.
                drop(unsafe { Box::from_raw(sink) });
                Err("Could not render the page.".into())
            }
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            // SAFETY: as for Page.
            unsafe {
                canopy_page_destroy(self.ptr.as_ptr());
                drop(Box::from_raw(self.sink));
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    pub struct Page;
    impl Page {
        pub fn new(
            _: &str,
            _: &str,
            _: f32,
            _: &Window,
        ) -> Result<(Self, async_channel::Receiver<PageEvent>), String> {
            Err("Pages need macOS.".into())
        }
        pub fn place(&self, _: Bounds<Pixels>, _: Bounds<Pixels>) {}
        pub fn hide(&self) {}
        pub fn set_active(&self, _: bool) {}
    }
    pub struct Capture;
    // Mirrors the macOS Capture, whose Drop tears the page down; callers drop it explicitly.
    impl Drop for Capture {
        fn drop(&mut self) {}
    }
    pub fn capture(
        _: &str,
        _: &str,
        _: f32,
        _: f32,
        _: f32,
    ) -> Result<
        (
            Capture,
            async_channel::Receiver<PageEvent>,
            async_channel::Receiver<Captured>,
        ),
        String,
    > {
        Err("Page previews need macOS.".into())
    }
}

pub use platform::{Page, capture};
