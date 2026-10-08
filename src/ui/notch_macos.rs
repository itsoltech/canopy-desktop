//! Narrow AppKit adapter for the overlay; never changes ordinary windows.
use super::notch_geometry::{InputRegion, NativeEvent};
use gpui_kit::Window;
use objc2_app_kit::{NSView, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn anchor_to_screen_top(window: &Window) -> gpui_kit::Result<()> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(std::io::Error::other("Not an AppKit window").into());
    };
    // SAFETY: GPUI supplies a live NSView for this borrowed Window. Called only
    // by the UI thread while opening the window; no raw pointer is retained.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let native = view
        .window()
        .ok_or_else(|| std::io::Error::other("Missing NSWindow"))?;
    let screen = native
        .screen()
        .ok_or_else(|| std::io::Error::other("Missing NSScreen"))?;
    let display = screen.frame();
    let size = native.frame().size;
    // GPUI's titlebar=None still includes NSTitledWindowMask, which AppKit
    // constrains below the menu bar. Removing that bit is essential here.
    native.setStyleMask(NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel);
    native.setHasShadow(false);
    let origin = NSPoint::new(
        display.origin.x + (display.size.width - size.width) / 2.,
        display.origin.y + display.size.height - size.height,
    );
    native.setFrame_display(NSRect::new(origin, size), true);
    let actual = native.frame();
    let top_gap = display.origin.y + display.size.height - actual.origin.y - actual.size.height;
    if std::env::var_os("CANOPY_NOTCH_PREVIEW").is_some() {
        eprintln!(
            "Notch: top_gap={top_gap:.2}pt, level={}, titled={}, size={}x{}",
            native.level(),
            native.styleMask().contains(NSWindowStyleMask::Titled),
            actual.size.width,
            actual.size.height
        );
    }
    if top_gap.abs() > 0.5 {
        return Err(std::io::Error::other(format!("Notch is {top_gap}pt below screen top")).into());
    }
    Ok(())
}

/// Owns only our AppKit monitors. No keyboard monitoring, polling or resize.
pub struct PointerMonitor {
    region: std::rc::Rc<std::cell::Cell<InputRegion>>,
    tokens: Vec<objc2::rc::Retained<objc2::runtime::AnyObject>>,
}
impl PointerMonitor {
    pub fn install(
        window: &Window,
        initial: InputRegion,
        on_change: impl Fn(NativeEvent) + 'static,
    ) -> gpui_kit::Result<Self> {
        anchor_to_screen_top(window)?;
        use objc2_app_kit::{NSEvent, NSEventMask};
        use std::{cell::Cell, ptr::NonNull, rc::Rc};
        let raw = HasWindowHandle::window_handle(window)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let RawWindowHandle::AppKit(raw) = raw.as_raw() else {
            return Err(std::io::Error::other("Not AppKit").into());
        };
        // SAFETY: live NSView supplied by GPUI on the main thread.
        let view = unsafe { raw.ns_view.cast::<NSView>().as_ref() };
        let native = view
            .window()
            .ok_or_else(|| std::io::Error::other("Missing NSWindow"))?;
        native.setIgnoresMouseEvents(true);
        let region = Rc::new(Cell::new(initial));
        let current = Rc::new(Cell::new(false));
        let check: Rc<dyn Fn()> = Rc::new({
            let region = region.clone();
            move || {
                let point = native.mouseLocationOutsideOfEventStream();
                let position = gpui_kit::point(
                    gpui_kit::px(point.x as f32),
                    gpui_kit::px((native.frame().size.height - point.y) as f32),
                );
                let inside = region.get().contains(position);
                if current.replace(inside) != inside {
                    native.setIgnoresMouseEvents(!inside);
                    on_change(NativeEvent::Pointer(inside));
                }
            }
        });
        let mut monitor = Self {
            region,
            tokens: Vec::new(),
        };
        let mask = NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDragged
            | NSEventMask::RightMouseDragged
            | NSEventMask::OtherMouseDragged;
        let global = block2::RcBlock::new({
            let check = check.clone();
            move |_: NonNull<NSEvent>| check()
        });
        monitor.tokens.push(
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global).ok_or_else(
                || std::io::Error::other("Cannot install notch global mouse monitor"),
            )?,
        );
        let local = block2::RcBlock::new(move |event: NonNull<NSEvent>| {
            check();
            event.as_ptr()
        });
        // SAFETY: the local monitor returns the exact event it receives, unchanged.
        monitor.tokens.push(
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) }
                .ok_or_else(|| std::io::Error::other("Cannot install notch local mouse monitor"))?,
        );
        Ok(monitor)
    }
    pub fn set_region(&self, region: InputRegion) {
        self.region.set(region);
    }
}
impl Drop for PointerMonitor {
    fn drop(&mut self) {
        for token in &self.tokens {
            // SAFETY: every token came from our NSEvent monitor registration.
            unsafe { objc2_app_kit::NSEvent::removeMonitor(token) };
        }
    }
}
