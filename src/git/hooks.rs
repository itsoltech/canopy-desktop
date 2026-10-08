//! Commit hooks are user executables; Git object/ref operations stay in libgit2.
use super::Result;
use crate::terminal::environment::ShellEnvironment;
use git2::{ErrorCode, Repository, Signature};
use std::{
    ffi::{OsStr, OsString},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct HookReport {
    pub name: String,
    pub output: String,
}
pub struct Hooks<'a> {
    directory: PathBuf,
    root: PathBuf,
    git_dir: PathBuf,
    index: PathBuf,
    env: &'a ShellEnvironment,
    cancel: &'a AtomicBool,
    progress: &'a dyn Fn(&str),
    pub reports: Vec<HookReport>,
}
impl<'a> Hooks<'a> {
    pub fn new(
        repo: &Repository,
        env: &'a ShellEnvironment,
        cancel: &'a AtomicBool,
        progress: &'a dyn Fn(&str),
    ) -> Result<Self> {
        let root = repo
            .workdir()
            .ok_or("Commit hooks require a working tree.")?
            .to_owned();
        let configured = match repo
            .config()
            .map_err(|e| e.to_string())?
            .get_path("core.hooksPath")
        {
            Ok(path) => Some(path),
            Err(e) if e.code() == ErrorCode::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        let directory = configured
            .map(|p| if p.is_absolute() { p } else { root.join(p) })
            .unwrap_or_else(|| repo.commondir().join("hooks"));
        Ok(Self {
            directory,
            root,
            git_dir: repo.path().to_owned(),
            index: repo
                .index()
                .map_err(|e| e.to_string())?
                .path()
                .ok_or("Index has no disk path.")?
                .to_owned(),
            env,
            cancel,
            progress,
            reports: vec![],
        })
    }
    pub fn run(
        &mut self,
        name: &str,
        args: &[&std::ffi::OsStr],
        author: &Signature<'_>,
    ) -> Result<()> {
        let hook = self.directory.join(name);
        let metadata = match std::fs::metadata(&hook) {
            Ok(v) => v,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(());
            }
            Err(e) => return Err(format!("Cannot inspect {name} hook: {e}")),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
                return Ok(());
            }
        }
        #[cfg(not(unix))]
        if !metadata.is_file() {
            return Ok(());
        }
        (self.progress)(&format!("Running {name}…"));
        let (invocation, fallback) = hook_invocation(&hook, args, self.env)?;
        let (environment, environment_os) =
            hook_environment(self.env, &self.root, &self.git_dir, &self.index, author);
        let output = super::process::run(
            super::process::Spec {
                invocation,
                fallback,
                cwd: self.root.clone(),
                environment,
                environment_os,
                input: vec![],
                timeout: Duration::from_secs(300),
                output_limit: 65_536,
                fail_on_output_limit: false,
                #[cfg(all(test, windows))]
                synchronize_before_read: false,
            },
            self.cancel,
        )
        .map_err(|e| format!("{name} hook: {e}"))?;
        let success = output.success();
        let status = output.status;
        let failure = output.failure;
        let output_text = clean_output(output.stdout, output.stderr, output.truncated);
        if let Some(error) = failure {
            return Err(format!(
                "{name} hook: {error}{}",
                if output_text.is_empty() {
                    String::new()
                } else {
                    format!("\n{output_text}")
                }
            ));
        }
        self.reports.push(HookReport {
            name: name.into(),
            output: output_text.clone(),
        });
        if !success {
            return Err(format!(
                "{name} hook failed ({}).{}",
                status,
                if output_text.is_empty() {
                    String::new()
                } else {
                    format!("\n{output_text}")
                }
            ));
        }
        Ok(())
    }
}
fn clean_output(stdout: Vec<u8>, stderr: Vec<u8>, truncated: bool) -> String {
    let clean = |bytes: Vec<u8>| {
        String::from_utf8_lossy(&bytes)
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
            .collect::<String>()
    };
    let mut output = [clean(stdout), clean(stderr)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if truncated {
        output.push_str("\n[Hook output truncated]");
    }
    output
}

fn hook_environment(
    source: &ShellEnvironment,
    root: &Path,
    git_dir: &Path,
    index: &Path,
    author: &Signature<'_>,
) -> (
    std::collections::HashMap<String, String>,
    Vec<(OsString, OsString)>,
) {
    let mut environment = source.vars.clone();
    for key in [
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_PREFIX",
        "GIT_NAMESPACE",
        "GIT_SHALLOW_FILE",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ] {
        #[cfg(windows)]
        environment.retain(|candidate, _| !candidate.eq_ignore_ascii_case(key));
        #[cfg(not(windows))]
        environment.remove(key);
    }
    environment.insert("GIT_EDITOR".into(), ":".into());
    let when = author.when();
    let offset = when.offset_minutes();
    environment.insert(
        "GIT_AUTHOR_NAME".into(),
        author.name().unwrap_or_default().into(),
    );
    environment.insert(
        "GIT_AUTHOR_EMAIL".into(),
        author.email().unwrap_or_default().into(),
    );
    environment.insert(
        "GIT_AUTHOR_DATE".into(),
        format!(
            "@{} {}{:02}{:02}",
            when.seconds(),
            if offset < 0 { "-" } else { "+" },
            offset.abs() / 60,
            offset.abs() % 60
        ),
    );
    let environment_os = vec![
        ("GIT_DIR".into(), git_dir.as_os_str().to_owned()),
        ("GIT_WORK_TREE".into(), root.as_os_str().to_owned()),
        ("GIT_INDEX_FILE".into(), index.as_os_str().to_owned()),
    ];
    (environment, environment_os)
}

fn hook_invocation(
    hook: &Path,
    args: &[&OsStr],
    env: &ShellEnvironment,
) -> Result<(
    super::process::Invocation,
    Option<super::process::Invocation>,
)> {
    #[cfg(windows)]
    {
        let mut arguments = Vec::new();
        let mut interpreted = false;
        let program = if let Some(mut shebang) = read_shebang(hook)? {
            interpreted = true;
            let first = shebang.remove(0);
            let interpreter = Path::new(&first)
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or("Hook shebang has no interpreter.")?;
            if interpreter.eq_ignore_ascii_case("env")
                || interpreter.eq_ignore_ascii_case("env.exe")
            {
                if shebang.first().is_some_and(|value| value == "-S") {
                    shebang.remove(0);
                }
                if shebang.is_empty() {
                    return Err("Hook env shebang has no interpreter.".into());
                }
                let name = shebang.remove(0);
                let program = resolve_windows_interpreter(&name, env).map_err(|_| {
                    format!("Hook interpreter '{name}' is unavailable. Install it or update the hook shebang.")
                })?;
                arguments.extend(shebang.into_iter().map(OsString::from));
                program
            } else {
                let requested = if Path::new(&first).is_absolute() {
                    first.as_str()
                } else {
                    interpreter
                };
                let program = resolve_windows_interpreter(requested, env).map_err(|_| {
                    format!("Hook interpreter '{interpreter}' is unavailable. Install it or update the hook shebang.")
                })?;
                arguments.extend(shebang.into_iter().map(OsString::from));
                program
            }
        } else {
            hook.to_owned()
        };
        if interpreted {
            arguments.push(hook.as_os_str().to_owned());
        }
        arguments.extend(args.iter().map(|value| (*value).to_owned()));
        Ok((super::process::Invocation { program, arguments }, None))
    }
    #[cfg(not(windows))]
    {
        let arguments = args.iter().map(|value| (*value).to_owned()).collect();
        let mut fallback_arguments = vec![hook.as_os_str().to_owned()];
        fallback_arguments.extend(args.iter().map(|value| (*value).to_owned()));
        let _ = env;
        Ok((
            super::process::Invocation {
                program: hook.to_owned(),
                arguments,
            },
            Some(super::process::Invocation {
                program: "/bin/sh".into(),
                arguments: fallback_arguments,
            }),
        ))
    }
}

#[cfg(any(windows, test))]
fn resolve_windows_interpreter(requested: &str, env: &ShellEnvironment) -> Result<PathBuf> {
    if let Ok(program) = env.resolve(requested) {
        return Ok(program);
    }
    let requested_path = Path::new(requested);
    if requested_path.is_absolute() {
        return Err("Configured hook interpreter is unavailable.".into());
    }
    if requested_path
        .extension()
        .is_some_and(|extension| !extension.to_string_lossy().eq_ignore_ascii_case("exe"))
    {
        return Err("Hook interpreter is unavailable.".into());
    }
    let name = requested_path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(requested)
        .to_owned();
    if !matches!(name.to_ascii_lowercase().as_str(), "sh" | "bash") {
        return Err("Hook interpreter is unavailable.".into());
    }
    let executable = format!("{name}.exe");
    let mut candidates = Vec::new();
    if let Ok(git) = env.resolve("git.exe")
        && let Some(parent) = git.parent()
    {
        candidates.push(parent.join(&executable));
        if let Some(root) = parent.parent() {
            candidates.push(root.join("bin").join(&executable));
        }
    }
    for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(root) = env.value(key) {
            candidates.push(Path::new(root).join("Git/bin").join(&executable));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "Git for Windows hook interpreter is unavailable.".into())
}

#[cfg(any(windows, test))]
fn read_shebang(path: &Path) -> Result<Option<Vec<String>>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if !bytes.starts_with(b"#!") {
        return Ok(None);
    }
    let end = bytes[2..]
        .iter()
        .position(|byte| *byte == b'\n' || *byte == b'\r')
        .map(|position| position + 2)
        .unwrap_or(bytes.len());
    if end > 4096 {
        return Err("Hook shebang exceeds 4096 bytes.".into());
    }
    let line = std::str::from_utf8(&bytes[2..end])
        .map_err(|_| "Hook shebang must be UTF-8.")?
        .trim();
    let parts = shell_words::split(line).map_err(|_| "Hook shebang is invalid.")?;
    if parts.is_empty() {
        Err("Hook shebang has no interpreter.".into())
    } else {
        Ok(Some(parts))
    }
}
pub fn read_message(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Hook commit message exceeds 1 MiB.".into());
    }
    let message = String::from_utf8(bytes).map_err(|_| "Hook commit message must be UTF-8.")?;
    if message.trim().is_empty() || message.contains('\0') {
        return Err("Hook produced an empty or invalid commit message.".into());
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn windows_shebang_parser_is_bounded_and_preserves_arguments() {
        let directory = tempfile::tempdir().unwrap();
        let hook = directory.path().join("pre-commit");
        std::fs::write(&hook, "#!/usr/bin/env -S sh -eu\necho test\n").unwrap();
        assert_eq!(
            read_shebang(&hook).unwrap().unwrap(),
            ["/usr/bin/env", "-S", "sh", "-eu"]
        );
        std::fs::write(&hook, format!("#!{}\n", "x".repeat(4096))).unwrap();
        assert!(read_shebang(&hook).is_err());
    }

    #[test]
    fn windows_shell_fallback_finds_git_bin_without_running_git() {
        let directory = tempfile::tempdir().unwrap();
        let shell = directory.path().join("Git/bin/sh.exe");
        std::fs::create_dir_all(shell.parent().unwrap()).unwrap();
        std::fs::write(&shell, "fixture").unwrap();
        let environment = ShellEnvironment {
            shell: shell.clone(),
            vars: HashMap::from([(
                "ProgramFiles".into(),
                directory.path().to_string_lossy().into_owned(),
            )]),
        };
        assert_eq!(
            resolve_windows_interpreter("sh", &environment).unwrap(),
            shell
        );
    }
}
