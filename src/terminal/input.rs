//! Legacy xterm key encoding; printable text and IME use the native input handler.
use super::session::INPUT_LIMIT_BYTES;

pub const BRACKETED_PASTE_START: &str = "\x1b[200~";
pub const BRACKETED_PASTE_END: &str = "\x1b[201~";
pub const BRACKETED_PASTE_FRAME_BYTES: usize =
    BRACKETED_PASTE_START.len() + BRACKETED_PASTE_END.len();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardMode {
    pub app_cursor: bool,
}

pub fn preserve_native_text_input(
    key: &str,
    control: bool,
    alt: bool,
    key_char: Option<&str>,
    prefer_character_input: bool,
    windows: bool,
) -> bool {
    if key.chars().count() != 1 || !alt {
        return false;
    }
    if windows {
        // GPUI derives this flag by comparing ToUnicode with and without
        // modifiers, and sets it directly for dead keys. Plain Alt+ASCII
        // navigation therefore remains available to the terminal encoder.
        return prefer_character_input && key_char.is_some_and(|value| !value.is_empty());
    }
    !control && key_char.is_none_or(|value| !value.is_ascii())
}

pub fn paste_bytes(text: &str, bracketed: bool) -> Option<Vec<u8>> {
    if bracketed {
        let text = text.replace('\x1b', "");
        let len = BRACKETED_PASTE_FRAME_BYTES.checked_add(text.len())?;
        if len > INPUT_LIMIT_BYTES {
            return None;
        }
        Some(format!("{BRACKETED_PASTE_START}{text}{BRACKETED_PASTE_END}").into_bytes())
    } else if text.len() <= INPUT_LIMIT_BYTES {
        Some(text.as_bytes().to_vec())
    } else {
        None
    }
}

/// Insert a task draft immediately, without waiting for terminal mode or hooks.
/// Always frame multiline content as a paste; never append a submit key.
pub fn task_prompt_bytes(text: &str) -> Option<Vec<u8>> {
    paste_bytes(text, true)
}

pub fn key(
    key: &str,
    control: bool,
    alt: bool,
    shift: bool,
    mode: KeyboardMode,
) -> Option<Vec<u8>> {
    if control && key.chars().count() == 1 {
        let c = key.chars().next()?.to_ascii_lowercase();
        let byte = match c {
            'a'..='z' => (c as u8) - b'a' + 1,
            ' ' | '@' => 0,
            '[' => 27,
            '\\' => 28,
            ']' => 29,
            '^' => 30,
            '_' | '/' => 31,
            '?' => 127,
            _ => return None,
        };
        let mut out = vec![byte];
        if alt {
            out.insert(0, 27);
        }
        return Some(out);
    }
    if alt && !control && key.chars().count() == 1 {
        let mut out = vec![27];
        out.extend_from_slice(key.as_bytes());
        return Some(out);
    }
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
    let arrow = match key {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    };
    if let Some(code) = arrow {
        return Some(
            if modifier > 1 {
                format!("\x1b[1;{modifier}{code}")
            } else if mode.app_cursor {
                format!("\x1bO{code}")
            } else {
                format!("\x1b[{code}")
            }
            .into_bytes(),
        );
    }
    let bytes = match key {
        "enter" if shift && !control && !alt => b"\x1b[13;2u".to_vec(),
        "enter" if alt => b"\x1b\r".to_vec(),
        "enter" => b"\r".to_vec(),
        "escape" if alt => b"\x1b\x1b".to_vec(),
        "escape" => vec![27],
        "backspace" if alt && control => b"\x1b\x08".to_vec(),
        "backspace" if alt => b"\x1b\x7f".to_vec(),
        "backspace" if control => vec![8],
        "backspace" => vec![127],
        "tab" if alt && shift => b"\x1b\x1b[Z".to_vec(),
        "tab" if alt => b"\x1b\t".to_vec(),
        "tab" if shift => b"\x1b[Z".to_vec(),
        "tab" => vec![9],
        "insert" | "delete" | "pageup" | "pagedown" => {
            let n = match key {
                "insert" => 2,
                "delete" => 3,
                "pageup" => 5,
                _ => 6,
            };
            if modifier > 1 {
                format!("\x1b[{n};{modifier}~")
            } else {
                format!("\x1b[{n}~")
            }
            .into_bytes()
        }
        "f1" | "f2" | "f3" | "f4" => format!(
            "\x1bO{}",
            match key {
                "f1" => 'P',
                "f2" => 'Q',
                "f3" => 'R',
                _ => 'S',
            }
        )
        .into_bytes(),
        "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" => {
            let n = match key {
                "f5" => 15,
                "f6" => 17,
                "f7" => 18,
                "f8" => 19,
                "f9" => 20,
                "f10" => 21,
                "f11" => 23,
                _ => 24,
            };
            format!("\x1b[{n}~").into_bytes()
        }
        _ => return None,
    };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn legacy() -> KeyboardMode {
        KeyboardMode::default()
    }

    #[test]
    fn keys() {
        assert_eq!(key("tab", false, false, false, legacy()), Some(vec![9]));
        assert_eq!(
            key("tab", false, false, true, legacy()),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(
            key("enter", false, false, true, legacy()),
            Some(b"\x1b[13;2u".to_vec())
        );
        assert_eq!(
            key("enter", false, false, false, legacy()),
            Some(vec![b'\r'])
        );
        assert_eq!(key("c", true, false, false, legacy()), Some(vec![3]));
        assert_eq!(key("s", true, false, false, legacy()), Some(vec![19]));
        assert_eq!(
            key(
                "left",
                false,
                false,
                false,
                KeyboardMode { app_cursor: true }
            )
            .unwrap(),
            b"\x1bOD"
        );
        assert_eq!(
            key("left", true, false, false, legacy()).unwrap(),
            b"\x1b[1;5D"
        );
    }

    #[test]
    fn paste_payload_uses_one_bracketed_frame_and_strips_escape() {
        assert_eq!(
            paste_bytes("one\ntwo\x1b", true).unwrap(),
            b"\x1b[200~one\ntwo\x1b[201~".to_vec()
        );
        assert_eq!(paste_bytes("one\ntwo\x1b", false).unwrap(), b"one\ntwo\x1b");
    }

    #[test]
    fn paste_payload_respects_the_session_input_limit() {
        let fits = "a".repeat(INPUT_LIMIT_BYTES - BRACKETED_PASTE_FRAME_BYTES);
        assert_eq!(paste_bytes(&fits, true).unwrap().len(), INPUT_LIMIT_BYTES);
        let too_large = "a".repeat(INPUT_LIMIT_BYTES - BRACKETED_PASTE_FRAME_BYTES + 1);
        assert!(paste_bytes(&too_large, true).is_none());
        assert!(paste_bytes(&"a".repeat(INPUT_LIMIT_BYTES + 1), false).is_none());
    }

    #[test]
    fn task_draft_is_framed_without_readiness_signals_or_submit_key() {
        assert_eq!(
            task_prompt_bytes("Task\nComment\n@'/tmp/image.png'").unwrap(),
            b"\x1b[200~Task\nComment\n@'/tmp/image.png'\x1b[201~"
        );
    }

    #[test]
    fn task_draft_strips_escape_and_respects_the_input_limit() {
        assert_eq!(
            task_prompt_bytes("Task\x1b\nComment").unwrap(),
            b"\x1b[200~Task\nComment\x1b[201~"
        );
        assert!(task_prompt_bytes(&"a".repeat(INPUT_LIMIT_BYTES)).is_none());
    }

    #[test]
    fn modified_legacy_control_keys_match_terminal_conventions() {
        assert_eq!(
            key("left", false, true, false, legacy()).unwrap(),
            b"\x1b[1;3D"
        );
        assert_eq!(
            key("right", true, true, false, legacy()).unwrap(),
            b"\x1b[1;7C"
        );
        assert_eq!(
            key("backspace", true, false, false, legacy()),
            Some(vec![8])
        );
        assert_eq!(
            key("backspace", false, true, false, legacy()),
            Some(b"\x1b\x7f".to_vec())
        );
        assert_eq!(
            key("backspace", true, true, false, legacy()),
            Some(b"\x1b\x08".to_vec())
        );
        assert_eq!(
            key("enter", false, true, false, legacy()),
            Some(b"\x1b\r".to_vec())
        );
    }

    #[test]
    fn windows_altgr_dead_keys_and_alt_navigation_use_distinct_paths() {
        assert!(preserve_native_text_input(
            "a",
            true,
            true,
            Some("ą"),
            true,
            true
        ));
        assert!(preserve_native_text_input(
            "q",
            true,
            true,
            Some("@"),
            true,
            true
        ));
        assert!(preserve_native_text_input(
            "`",
            false,
            true,
            Some("`"),
            true,
            true
        ));
        assert!(!preserve_native_text_input(
            "c", true, true, None, false, true
        ));
        assert!(!preserve_native_text_input(
            "c",
            true,
            true,
            Some(""),
            true,
            true
        ));
        assert!(!preserve_native_text_input(
            "b",
            false,
            true,
            Some("b"),
            false,
            true
        ));
        assert!(!preserve_native_text_input(
            "f",
            false,
            true,
            Some("f"),
            false,
            true
        ));
        assert_eq!(
            key("b", false, true, false, legacy()),
            Some(b"\x1bb".to_vec())
        );
        assert_eq!(
            key("f", false, true, false, legacy()),
            Some(b"\x1bf".to_vec())
        );
        assert!(!preserve_native_text_input(
            "c",
            true,
            false,
            Some("c"),
            false,
            true
        ));
        assert!(!preserve_native_text_input(
            "c",
            true,
            true,
            Some("c"),
            true,
            false
        ));
    }
}
