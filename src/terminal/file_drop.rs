//! Prepare native file-drop paths for insertion into a running terminal.
use super::{
    environment::{ShellKind, windows_absolute},
    input::BRACKETED_PASTE_FRAME_BYTES,
    session::INPUT_LIMIT_BYTES,
};
use std::{fmt, path::PathBuf};

const MAX_DROP_TEXT_BYTES: usize = INPUT_LIMIT_BYTES - BRACKETED_PASTE_FRAME_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDropError {
    Empty,
    RelativePath,
    NonUtf8Path,
    ControlCharacter,
    UnsafeForShell,
    TooLarge,
}

impl fmt::Display for FileDropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            FileDropError::Empty => "No files were dropped.",
            FileDropError::RelativePath => "Dropped files must have absolute paths.",
            FileDropError::NonUtf8Path => "Dropped file paths must be valid UTF-8.",
            FileDropError::ControlCharacter => {
                "Dropped file paths cannot contain control characters."
            }
            FileDropError::UnsafeForShell => {
                "A dropped path cannot be represented safely for this shell."
            }
            FileDropError::TooLarge => "Too many file paths were dropped at once.",
        };
        f.write_str(message)
    }
}

impl std::error::Error for FileDropError {}

fn quote(path: &str, shell: ShellKind, windows: bool) -> Result<String, FileDropError> {
    match shell {
        ShellKind::Posix => Ok(shell_words::quote(path).into_owned()),
        ShellKind::PowerShell => Ok(format!("'{}'", path.replace('\'', "''"))),
        ShellKind::Cmd => {
            if path.contains(['"', '%', '!']) {
                return Err(FileDropError::UnsafeForShell);
            }
            Ok(format!("\"{path}\""))
        }
        ShellKind::Other if windows => Err(FileDropError::UnsafeForShell),
        ShellKind::Other => Ok(shell_words::quote(path).into_owned()),
    }
}

pub fn prepare_file_drop(
    paths: &[PathBuf],
    shell: ShellKind,
    windows: bool,
) -> Result<String, FileDropError> {
    if paths.is_empty() {
        return Err(FileDropError::Empty);
    }

    let mut payload = String::new();
    for path in paths {
        let path = path.to_str().ok_or(FileDropError::NonUtf8Path)?;
        if if windows {
            !windows_absolute(path)
        } else {
            !PathBuf::from(path).is_absolute()
        } {
            return Err(FileDropError::RelativePath);
        }
        if path.chars().any(char::is_control) {
            return Err(FileDropError::ControlCharacter);
        }
        if path.len() > MAX_DROP_TEXT_BYTES {
            return Err(FileDropError::TooLarge);
        }

        let quoted = quote(path, shell, windows)?;
        let next_len = payload
            .len()
            .checked_add(quoted.len())
            .and_then(|len| len.checked_add(1))
            .ok_or(FileDropError::TooLarge)?;
        if next_len > MAX_DROP_TEXT_BYTES {
            return Err(FileDropError::TooLarge);
        }
        payload.push_str(&quoted);
        payload.push(' ');
    }

    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    fn parsed(payload: &str) -> Vec<String> {
        shell_words::split(payload).unwrap()
    }

    #[test]
    fn prepares_cleanshot_unicode_and_multiple_paths_in_order() {
        let paths = [
            path(
                "/Users/nix/Library/Application Support/CleanShot/media/media_Qy86bBcZZh/CleanShot 2026-09-11 at 09.34.36.png",
            ),
            path("/tmp/zażółć/emoji-😀 image.webp"),
        ];
        let payload = prepare_file_drop(&paths, ShellKind::Posix, false).unwrap();
        assert!(payload.ends_with(' '));
        assert_eq!(
            parsed(&payload),
            paths
                .iter()
                .map(|p| p.to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn quotes_shell_syntax_without_changing_paths() {
        let paths = [
            path(
                "/tmp/quote' double\" slash\\ dollar$ command$(x) tick` semi; amp& parens() glob*.png",
            ),
            path("/tmp/hash#bang?brace[1].png"),
        ];
        assert_eq!(
            parsed(&prepare_file_drop(&paths, ShellKind::Posix, false).unwrap()),
            paths
                .iter()
                .map(|p| p.to_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_empty_relative_control_and_oversized_input() {
        assert_eq!(
            prepare_file_drop(&[], ShellKind::Posix, false).unwrap_err(),
            FileDropError::Empty
        );
        assert_eq!(
            prepare_file_drop(&[path("relative.png")], ShellKind::Posix, false).unwrap_err(),
            FileDropError::RelativePath
        );
        for name in [
            "/tmp/carriage\rreturn.png",
            "/tmp/new\nline.png",
            "/tmp/tab\tname.png",
            "/tmp/escape\u{1b}.png",
            "/tmp/nul\u{0}.png",
            "/tmp/delete\u{7f}.png",
        ] {
            assert_eq!(
                prepare_file_drop(&[path(name)], ShellKind::Posix, false).unwrap_err(),
                FileDropError::ControlCharacter
            );
        }
        let oversized = format!("/{}", "a".repeat(MAX_DROP_TEXT_BYTES));
        assert_eq!(
            prepare_file_drop(&[path(&oversized)], ShellKind::Posix, false).unwrap_err(),
            FileDropError::TooLarge
        );
    }

    #[test]
    fn accepts_payload_that_fits_with_bracketed_paste_framing() {
        let path = format!("/{}", "a".repeat(MAX_DROP_TEXT_BYTES - 2));
        let payload = prepare_file_drop(&[PathBuf::from(path)], ShellKind::Posix, false).unwrap();
        assert_eq!(payload.len(), MAX_DROP_TEXT_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_unix_paths() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let path = PathBuf::from(OsString::from_vec(b"/tmp/not-utf8-\xff.png".to_vec()));
        assert_eq!(
            prepare_file_drop(&[path], ShellKind::Posix, false).unwrap_err(),
            FileDropError::NonUtf8Path
        );
    }

    #[test]
    fn windows_shell_dialects_preserve_backslashes_spaces_and_metacharacters() {
        let paths = [
            path(r"C:\Users\Żaneta\a b&c.txt"),
            path(r"\\server\share\x'y.txt"),
        ];
        let powershell = prepare_file_drop(&paths, ShellKind::PowerShell, true).unwrap();
        assert_eq!(
            powershell,
            "'C:\\Users\\Żaneta\\a b&c.txt' '\\\\server\\share\\x''y.txt' "
        );
        let cmd = prepare_file_drop(&paths[..1], ShellKind::Cmd, true).unwrap();
        assert_eq!(cmd, r#""C:\Users\Żaneta\a b&c.txt" "#);
    }

    #[test]
    fn cmd_rejects_expansion_characters_instead_of_reinterpreting_paths() {
        for path in [r"C:\tmp\%TOKEN%.txt", r"C:\tmp\bang!.txt"] {
            assert_eq!(
                prepare_file_drop(&[PathBuf::from(path)], ShellKind::Cmd, true).unwrap_err(),
                FileDropError::UnsafeForShell
            );
        }
        assert_eq!(
            prepare_file_drop(&[PathBuf::from(r"C:\tmp\file.txt")], ShellKind::Other, true)
                .unwrap_err(),
            FileDropError::UnsafeForShell
        );
    }

    #[cfg(windows)]
    #[test]
    fn cmd_interprets_a_metacharacter_path_as_one_quoted_value() {
        let path = r"C:\tmp\a&echo CANOPY_INJECTED.txt";
        let payload = prepare_file_drop(&[PathBuf::from(path)], ShellKind::Cmd, true).unwrap();
        let command = format!("for %A in ({}) do @echo(%~A", payload.trim());
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/v:off", "/c", &command])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim_end(), path);
    }
}
