//! Reads the system pasteboard where GPUI's clipboard reading falls short.

/// The image on the pasteboard (PNG or TIFF bytes), also when text is there too: a
/// browser's "Copy Image" adds the image's address as text, and GPUI then returns only
/// the text.
#[cfg(target_os = "macos")]
pub fn image() -> Option<Vec<u8>> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeTIFF};
    let pasteboard = NSPasteboard::generalPasteboard();
    // SAFETY: the type constants are immutable AppKit statics.
    let types = unsafe { [NSPasteboardTypePNG, NSPasteboardTypeTIFF] };
    types
        .into_iter()
        .find_map(|kind| pasteboard.dataForType(kind))
        .map(|data| data.to_vec())
        .filter(|bytes| !bytes.is_empty())
}

#[cfg(not(target_os = "macos"))]
pub fn image() -> Option<Vec<u8>> {
    None
}
