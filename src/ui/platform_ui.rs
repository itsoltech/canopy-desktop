//! Small platform presentation choices; behavior remains with the owning views.

pub const fn shortcut(mac: &'static str, windows: &'static str) -> &'static str {
    if cfg!(target_os = "windows") {
        windows
    } else {
        mac
    }
}

pub const fn interface_font() -> &'static str {
    if cfg!(target_os = "windows") {
        "Segoe UI"
    } else {
        ".SystemUIFont"
    }
}
