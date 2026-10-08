//! A bounded login-shell environment probe. Run only on a background executor.
#[cfg(unix)]
use std::ffi::CStr;
#[cfg(unix)]
use std::os::unix::{io::AsRawFd, process::CommandExt};
#[cfg(any(unix, test))]
use std::process::Command;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};
#[cfg(unix)]
use std::{io::Read, process::Stdio, time::Instant};
// Color decisions belong to the new PTY, not the process launching the GUI.
const LAUNCHER_COLOR_FLAGS: [&str; 4] = ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "FORCE_COLOR"];
const CAPABILITIES: [(&str, &str); 3] = [
    ("TERM", "xterm-256color"),
    ("COLORTERM", "truecolor"),
    ("TERM_PROGRAM", "Canopy"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    Posix,
    PowerShell,
    Cmd,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsProgramKind {
    Native,
    CommandScript,
    PowerShellScript,
    NodeScript,
}

pub fn shell_kind(path: &Path) -> ShellKind {
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match name.as_str() {
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" => ShellKind::Posix,
        "pwsh" | "powershell" => ShellKind::PowerShell,
        "cmd" => ShellKind::Cmd,
        _ => ShellKind::Other,
    }
}

/// Establish the desktop terminal environment before GPUI or any workers start.
///
/// Alacritty's Command inherits the host environment before applying Options.env;
/// merely deleting NO_COLOR from the captured map would leave the host value.
///
/// # Safety
/// Call only at process entry, before creating threads or invoking libraries that
/// may concurrently read the process environment.
pub unsafe fn prepare_desktop_environment() {
    for key in LAUNCHER_COLOR_FLAGS {
        unsafe {
            std::env::remove_var(key);
        }
    }
    for (key, value) in CAPABILITIES {
        unsafe {
            std::env::set_var(key, value);
        }
    }
}
#[cfg(any(unix, test))]
fn configure_probe(command: &mut Command) {
    for key in LAUNCHER_COLOR_FLAGS {
        command.env_remove(key);
    }
    command.envs(CAPABILITIES);
}

#[derive(Clone)]
pub struct ShellEnvironment {
    pub shell: PathBuf,
    pub vars: HashMap<String, String>,
}
impl ShellEnvironment {
    pub fn load() -> Result<Self, String> {
        let shell = user_shell()?;
        let vars = probe(&shell, Duration::from_secs(5))?;
        Ok(Self { shell, vars })
    }

    pub fn shell_kind(&self) -> ShellKind {
        shell_kind(&self.shell)
    }

    pub fn extend_overrides(&mut self, overrides: impl IntoIterator<Item = (String, String)>) {
        merge_environment(&mut self.vars, overrides, cfg!(windows));
    }

    pub fn insert_override(&mut self, key: String, value: String) {
        self.extend_overrides([(key, value)]);
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        environment_value_for_platform(&self.vars, key, cfg!(windows))
    }

    pub fn default_shell_arguments(&self) -> Vec<String> {
        match self.shell_kind() {
            ShellKind::Posix => vec!["-l".into()],
            ShellKind::PowerShell | ShellKind::Cmd | ShellKind::Other => Vec::new(),
        }
    }

    pub fn resolve(&self, tool: &str) -> Result<PathBuf, String> {
        if tool == "shell" {
            return Ok(self.shell.clone());
        }
        resolve_for_platform(tool, &self.vars, cfg!(windows))
    }

    pub fn prepare_launch(
        &self,
        program: PathBuf,
        arguments: Vec<String>,
    ) -> Result<(PathBuf, Vec<String>), String> {
        #[cfg(not(windows))]
        {
            Ok((program, arguments))
        }
        #[cfg(windows)]
        {
            match windows_program_kind(&program)? {
                WindowsProgramKind::Native => Ok((program, arguments)),
                WindowsProgramKind::PowerShellScript => {
                    let interpreter = self
                        .resolve("pwsh.exe")
                        .or_else(|_| self.resolve("powershell.exe"))?;
                    let mut prepared = vec![
                        "-NoLogo".into(),
                        "-NoProfile".into(),
                        "-File".into(),
                        program.to_string_lossy().into_owned(),
                    ];
                    prepared.extend(arguments);
                    Ok((interpreter, prepared))
                }
                WindowsProgramKind::NodeScript => {
                    let interpreter = self.resolve("node.exe")?;
                    let mut prepared = vec![program.to_string_lossy().into_owned()];
                    prepared.extend(arguments);
                    Ok((interpreter, prepared))
                }
                WindowsProgramKind::CommandScript => {
                    let interpreter = environment_value(&self.vars, "ComSpec")
                        .map(PathBuf::from)
                        .filter(|path| path.is_absolute() && executable(path))
                        .or_else(|| self.resolve("cmd.exe").ok())
                        .ok_or("Could not find cmd.exe for the command script.")?;
                    let command = windows_batch_command(&program, &arguments)?;
                    Ok((
                        interpreter,
                        vec![
                            "/d".into(),
                            "/s".into(),
                            "/v:off".into(),
                            "/c".into(),
                            command,
                        ],
                    ))
                }
            }
        }
    }
}

#[cfg(windows)]
fn environment_value<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    environment_value_for_platform(vars, key, true)
}

fn environment_value_for_platform<'a>(
    vars: &'a HashMap<String, String>,
    key: &str,
    windows: bool,
) -> Option<&'a str> {
    if windows {
        vars.iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    } else {
        vars.get(key).map(String::as_str)
    }
}

fn merge_environment(
    vars: &mut HashMap<String, String>,
    overrides: impl IntoIterator<Item = (String, String)>,
    windows: bool,
) {
    for (key, value) in overrides {
        if windows {
            vars.retain(|candidate, _| !candidate.eq_ignore_ascii_case(&key));
        }
        vars.insert(key, value);
    }
}

fn resolve_for_platform(
    name: &str,
    vars: &HashMap<String, String>,
    windows: bool,
) -> Result<PathBuf, String> {
    let path = Path::new(name);
    let absolute = if windows {
        windows_absolute(name)
    } else {
        path.is_absolute()
    };
    if absolute {
        return resolve_candidate(path, vars, windows)
            .ok_or_else(|| format!("Executable '{name}' is unavailable."));
    }
    if name.contains('/') || (windows && (name.contains('\\') || windows_drive_relative(name))) {
        return Err("Tool must name an executable or use an absolute path.".into());
    }
    let path_value = if windows {
        vars.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
            .map(|(_, value)| value.as_str())
            .unwrap_or_default()
    } else {
        vars.get("PATH").map(String::as_str).unwrap_or_default()
    };
    let paths: Vec<PathBuf> = if windows {
        path_value
            .split(';')
            .map(|value| value.trim().trim_matches('"'))
            .filter(|value| {
                !value.is_empty() && (windows_absolute(value) || Path::new(value).is_absolute())
            })
            .map(PathBuf::from)
            .collect()
    } else {
        std::env::split_paths(path_value).collect()
    };
    paths
        .into_iter()
        .find_map(|directory| resolve_candidate(&directory.join(name), vars, windows))
        .ok_or_else(|| format!("Executable '{name}' was not found in the shell PATH."))
}

fn resolve_candidate(
    path: &Path,
    vars: &HashMap<String, String>,
    windows: bool,
) -> Option<PathBuf> {
    if !windows {
        return executable(path).then(|| path.to_owned());
    }
    if path.extension().is_some() {
        return windows_program_kind(path)
            .is_ok_and(|_| executable(path))
            .then(|| path.to_owned());
    }
    windows_path_extensions(vars)
        .into_iter()
        .map(|extension| path.with_extension(extension.trim_start_matches('.')))
        .find(|candidate| executable(candidate))
}

fn windows_path_extensions(vars: &HashMap<String, String>) -> Vec<String> {
    vars.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("PATHEXT"))
        .map(|(_, value)| value.as_str())
        .unwrap_or(".COM;.EXE;.BAT;.CMD")
        .split(';')
        .filter_map(|extension| {
            let extension = extension.trim();
            (!extension.is_empty()
                && extension.starts_with('.')
                && extension[1..]
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()))
            .then(|| extension.to_ascii_lowercase())
        })
        .collect()
}

fn windows_program_kind(path: &Path) -> Result<WindowsProgramKind, String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "exe" | "com" => Ok(WindowsProgramKind::Native),
        "cmd" | "bat" => Ok(WindowsProgramKind::CommandScript),
        "ps1" => Ok(WindowsProgramKind::PowerShellScript),
        "js" | "mjs" | "cjs" => Ok(WindowsProgramKind::NodeScript),
        _ => Err("Windows tools must be an EXE, COM, CMD, BAT, PowerShell or Node script.".into()),
    }
}

#[cfg(any(windows, test))]
fn windows_batch_command(program: &Path, arguments: &[String]) -> Result<String, String> {
    let mut values = Vec::with_capacity(arguments.len() + 1);
    values.push(program.to_string_lossy().into_owned());
    values.extend(arguments.iter().cloned());
    let quoted: Result<Vec<_>, _> = values
        .iter()
        .map(|value| {
            if value.contains(['\0', '\r', '\n', '"', '%']) {
                Err("CMD script paths and arguments cannot contain NUL, newlines, quotes or '%'.")
            } else {
                Ok(format!("\"{value}\""))
            }
        })
        .collect();
    Ok(format!("\"{}\"", quoted?.join(" ")))
}

pub(super) fn windows_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with("\\\\")
        || value.starts_with("//")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

fn windows_drive_relative(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
pub fn user_shell() -> Result<PathBuf, String> {
    if let Some(shell) = std::env::var_os("SHELL").map(PathBuf::from).filter(|path| {
        path.is_absolute()
            && executable(path)
            && (!cfg!(windows) || windows_program_kind(path).is_ok())
    }) {
        return Ok(shell);
    }
    #[cfg(windows)]
    {
        let vars = windows_environment();
        for candidate in ["pwsh.exe", "powershell.exe"] {
            if let Ok(shell) = resolve_for_platform(candidate, &vars, true) {
                return Ok(shell);
            }
        }
        if let Some(shell) = environment_value(&vars, "ComSpec")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && executable(path))
        {
            return Ok(shell);
        }
        Err("Could not find pwsh.exe, Windows PowerShell or ComSpec.".into())
    }
    #[cfg(unix)]
    unsafe {
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut buffer = vec![0u8; 16384];
        let mut result = std::ptr::null_mut();
        if libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        ) == 0
            && !result.is_null()
            && !entry.pw_shell.is_null()
        {
            let shell = PathBuf::from(
                CStr::from_ptr(entry.pw_shell)
                    .to_string_lossy()
                    .into_owned(),
            );
            if executable(&shell) {
                return Ok(shell);
            }
        }
        Err("Could not find the user's login shell.".into())
    }
    #[cfg(not(any(unix, windows)))]
    Err("Could not find a supported user shell.".into())
}
pub fn probe(shell: &Path, timeout: Duration) -> Result<HashMap<String, String>, String> {
    #[cfg(windows)]
    {
        let _ = (shell, timeout);
        Ok(windows_environment())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (shell, timeout);
        Err("Shell environment capture is unavailable on this platform.".into())
    }
    #[cfg(unix)]
    {
        let nonce = format!(
            "CANOPY_ENV_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let begin = format!("{nonce}_BEGIN");
        let end = format!("{nonce}_END");
        let script = format!("printf '\\0{begin}\\0'; /usr/bin/env -0; printf '\\0{end}\\0'");
        let mut command = Command::new(shell);
        configure_probe(&mut command);
        let mut child = command
            .args(["-ilc", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|_| "Could not start the shell environment probe.")?;
        let mut output = child.stdout.take().expect("piped stdout");
        unsafe {
            libc::fcntl(output.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK);
        }
        let deadline = Instant::now() + timeout;
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        let result = loop {
            match output.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.len() > 1_048_576 {
                        break Err("Shell environment output exceeded 1 MiB.".into());
                    }
                    continue;
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => break Err("Could not read the shell environment.".into()),
            }
            if let Ok(Some(status)) = child.try_wait() {
                if !status.success() {
                    break Err("Shell environment probe failed.".into());
                }
                // Drain bytes written between the last nonblocking read and try_wait.
                while let Ok(n) = output.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.len() > 1_048_576 {
                        break;
                    }
                }
                if bytes.len() > 1_048_576 {
                    break Err("Shell environment output exceeded 1 MiB.".into());
                }
                break parse(&bytes, &begin, &end);
            }
            if Instant::now() >= deadline {
                break Err("Shell environment probe timed out.".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if result.is_err() {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }
}

#[cfg(windows)]
fn windows_environment() -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = std::env::vars().collect();
    for key in LAUNCHER_COLOR_FLAGS {
        vars.retain(|candidate, _| !candidate.eq_ignore_ascii_case(key));
    }
    for (key, value) in CAPABILITIES {
        vars.retain(|candidate, _| !candidate.eq_ignore_ascii_case(key));
        vars.insert(key.into(), value.into());
    }
    vars
}
#[cfg(any(unix, test))]
fn parse(bytes: &[u8], begin: &str, end: &str) -> Result<HashMap<String, String>, String> {
    let parts: Vec<_> = bytes.split(|b| *b == 0).collect();
    let start = parts
        .iter()
        .position(|p| *p == begin.as_bytes())
        .ok_or("Shell did not return an environment.")?
        + 1;
    let finish = parts[start..]
        .iter()
        .position(|p| *p == end.as_bytes())
        .map(|i| start + i)
        .ok_or("Incomplete shell environment.")?;
    let mut vars = HashMap::new();
    for part in &parts[start..finish] {
        if part.is_empty() {
            continue;
        }
        let value = std::str::from_utf8(part).map_err(|_| "Shell environment is not UTF-8.")?;
        if let Some((key, value)) = value.split_once('=') {
            vars.insert(key.into(), value.into());
        }
    }
    if !vars.contains_key("PATH") {
        return Err("Shell environment is missing PATH.".into());
    }
    vars.extend(CAPABILITIES.map(|(key, value)| (key.to_owned(), value.to_owned())));
    Ok(vars)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_removes_launcher_color_flags_and_advertises_its_terminal() {
        let mut command = Command::new("/bin/sh");
        command
            .env("NO_COLOR", "1")
            .env("CLICOLOR", "0")
            .env("TERM", "dumb")
            .env("KEEP_ME", "yes");
        configure_probe(&mut command);
        let changes: HashMap<_, _> = command.get_envs().collect();
        for key in LAUNCHER_COLOR_FLAGS {
            assert_eq!(changes[std::ffi::OsStr::new(key)], None);
        }
        assert_eq!(
            changes[std::ffi::OsStr::new("TERM")],
            Some(std::ffi::OsStr::new("xterm-256color"))
        );
        assert_eq!(
            changes[std::ffi::OsStr::new("KEEP_ME")],
            Some(std::ffi::OsStr::new("yes"))
        );
    }
    #[test]
    fn explicit_shell_color_preference_is_preserved_after_capture() {
        let vars = parse(b"B\0PATH=/bin\0NO_COLOR=user-choice\0E\0", "B", "E").unwrap();
        assert_eq!(vars["NO_COLOR"], "user-choice");
    }
    #[test]
    fn ignores_shell_chatter_and_preserves_multiline_values() {
        let vars = parse(b"hello\0B\0PATH=/bin\0MULTI=a\nb=c\0\0E\0after", "B", "E").unwrap();
        assert_eq!(vars["MULTI"], "a\nb=c");
        assert_eq!(vars["TERM"], "xterm-256color");
    }
    #[test]
    fn shell_kind_controls_only_known_posix_login_arguments() {
        assert_eq!(shell_kind(Path::new("/bin/zsh")), ShellKind::Posix);
        assert_eq!(shell_kind(Path::new("pwsh.exe")), ShellKind::PowerShell);
        assert_eq!(shell_kind(Path::new("CMD.EXE")), ShellKind::Cmd);
        let posix = ShellEnvironment {
            shell: "/bin/zsh".into(),
            vars: HashMap::new(),
        };
        let powershell = ShellEnvironment {
            shell: "pwsh.exe".into(),
            vars: HashMap::new(),
        };
        assert_eq!(posix.default_shell_arguments(), ["-l"]);
        assert!(powershell.default_shell_arguments().is_empty());
    }
    #[test]
    fn windows_resolve_uses_case_insensitive_path_and_pathext() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("tool.exe");
        std::fs::write(&executable, b"fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let vars = HashMap::from([
            ("Path".into(), dir.path().to_string_lossy().into_owned()),
            ("pathext".into(), ".EXE;.CMD".into()),
        ]);
        assert_eq!(
            resolve_for_platform("tool", &vars, true).unwrap(),
            executable
        );
        assert!(resolve_for_platform(r"folder\tool", &vars, true).is_err());
        assert!(resolve_for_platform(r"C:tool", &vars, true).is_err());
    }
    #[test]
    fn command_script_adapter_quotes_metacharacters_and_rejects_expansion() {
        assert_eq!(
            windows_batch_command(
                Path::new(r"C:\Program Files\tool.cmd"),
                &["literal&value".into(), "two words".into()]
            )
            .unwrap(),
            r#"""C:\Program Files\tool.cmd" "literal&value" "two words"""#
        );
        assert!(windows_batch_command(Path::new(r"C:\tool.cmd"), &["%PATH%".into()]).is_err());
    }
    #[test]
    fn windows_environment_overrides_replace_all_case_variants() {
        let mut vars = HashMap::from([
            ("Path".into(), "inherited".into()),
            ("OTHER".into(), "kept".into()),
        ]);
        merge_environment(&mut vars, [("PATH".into(), "profile".into())], true);
        assert_eq!(vars.len(), 2);
        assert_eq!(
            environment_value_for_platform(&vars, "path", true),
            Some("profile")
        );
        assert_eq!(vars["OTHER"], "kept");
    }
    #[cfg(unix)]
    #[test]
    fn probe_timeout_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shell");
        std::fs::write(&file, "#!/bin/sh\nsleep 10\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let start = Instant::now();
        assert!(probe(&file, Duration::from_millis(80)).is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
