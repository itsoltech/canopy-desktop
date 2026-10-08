//! Presentation-only path formatting. Filesystem identity always keeps the original `Path`.
use std::path::Path;

/// Format documented Win32 extended paths without exposing their transport prefix in the UI.
/// Other device namespaces stay unchanged because they do not have a safe DOS/UNC equivalent.
pub fn display(path: &Path) -> String {
    display_text(&path.to_string_lossy())
}

fn display_text(path: &str) -> String {
    let Some(rest) = path.strip_prefix(r"\\?\") else {
        return path.to_owned();
    };
    if rest
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("UNC\\"))
    {
        let unc = &rest[4..];
        return format!(r"\\{unc}");
    }
    let bytes = rest.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
        return rest.to_owned();
    }
    path.to_owned()
}

#[cfg(test)]
mod tests {
    use super::display_text;

    #[test]
    fn extended_dos_and_unc_paths_have_readable_labels() {
        assert_eq!(
            display_text(r"\\?\C:\Users\damia\GIT\canopy-desktop-2"),
            r"C:\Users\damia\GIT\canopy-desktop-2"
        );
        assert_eq!(
            display_text(r"\\?\UNC\server\share\long\path"),
            r"\\server\share\long\path"
        );
        assert_eq!(
            display_text(r"\\?\unc\Server\Share\MixedCase"),
            r"\\Server\Share\MixedCase"
        );
    }

    #[test]
    fn device_and_volume_namespaces_are_not_rewritten() {
        for path in [
            r"\\.\C:\device",
            r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\folder",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1",
        ] {
            assert_eq!(display_text(path), path);
        }
    }
}
