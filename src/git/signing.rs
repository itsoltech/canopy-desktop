//! External signature providers only; Git objects and refs remain libgit2 operations.
use super::Result;
use crate::terminal::environment::ShellEnvironment;
use git2::{Config, ErrorCode};
use std::{ffi::OsString, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningInfo {
    pub enabled: bool,
    pub format: String,
    pub program: String,
    pub key_configured: bool,
}
fn value(config: &Config, key: &str) -> Result<Option<String>> {
    match config.get_string(key) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e.message().into()),
    }
}
pub fn info(config: &Config) -> Result<SigningInfo> {
    let enabled = match config.get_bool("commit.gpgsign") {
        Ok(v) => v,
        Err(e) if e.code() == ErrorCode::NotFound => false,
        Err(e) => return Err(e.message().into()),
    };
    let format = value(config, "gpg.format")?.unwrap_or_else(|| "openpgp".into());
    let program = match format.as_str() {
        "openpgp" => value(config, "gpg.openpgp.program")?
            .or(value(config, "gpg.program")?)
            .unwrap_or_else(|| "gpg".into()),
        "ssh" => value(config, "gpg.ssh.program")?.unwrap_or_else(|| "ssh-keygen".into()),
        _ => String::new(),
    };
    Ok(SigningInfo {
        enabled,
        format,
        program,
        key_configured: value(config, "user.signingkey")?.is_some_and(|s| !s.is_empty()),
    })
}
pub fn sign(
    config: &Config,
    body: &[u8],
    email: &str,
    cwd: &std::path::Path,
    env: &ShellEnvironment,
    cancel: &AtomicBool,
) -> Result<String> {
    let info = info(config)?;
    if !matches!(info.format.as_str(), "openpgp" | "ssh") {
        return Err(format!(
            "Signing format '{}' is unsupported. No unsigned fallback was used.",
            info.format
        ));
    }
    let key = value(config, "user.signingkey")?.unwrap_or_else(|| email.into());
    let program = env.resolve(&info.program).map_err(|_| {
        format!(
            "Signing program '{}' is unavailable. No commit was created.",
            info.program
        )
    })?;
    #[cfg(windows)]
    if !matches!(
        program
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("exe" | "com")
    ) {
        return Err(
            "The signing program must resolve to a native EXE or COM file on Windows. No commit was created."
                .into(),
        );
    }
    let mut arguments: Vec<OsString> = Vec::new();
    let mut environment = env.vars.clone();
    #[cfg(windows)]
    environment.retain(|key, _| !key.eq_ignore_ascii_case("GPG_TTY"));
    #[cfg(not(windows))]
    environment.remove("GPG_TTY");
    let mut public_key = None;
    match info.format.as_str() {
        "openpgp" => {
            arguments.extend(
                ["--armor", "--detach-sign", "--status-fd=2", "--local-user"].map(OsString::from),
            );
            arguments.push(key.clone().into());
        }
        "ssh" => {
            if !info.key_configured {
                return Err(
                    "SSH signing requires user.signingKey. No unsigned fallback was used.".into(),
                );
            }
            let path = if let Some(key) = key
                .strip_prefix("key::")
                .or_else(|| key.starts_with("ssh-").then_some(key.as_str()))
            {
                let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
                file.write_all(key.as_bytes()).map_err(|e| e.to_string())?;
                let path = file.path().to_path_buf();
                public_key = Some(file);
                path
            } else if let Some(tail) = key.strip_prefix("~/") {
                env.value("HOME")
                    .map(PathBuf::from)
                    .or_else(|| crate::platform::directories::user_home().ok())
                    .ok_or("User home is unavailable for signing key resolution.")?
                    .join(tail)
            } else {
                PathBuf::from(&key)
            };
            arguments.extend(["-Y", "sign", "-n", "git", "-f"].map(OsString::from));
            arguments.push(path.into_os_string());
            if public_key.is_some() {
                arguments.push("-U".into());
            }
            if env.value("SSH_ASKPASS").is_some() {
                #[cfg(windows)]
                environment.retain(|key, _| !key.eq_ignore_ascii_case("SSH_ASKPASS_REQUIRE"));
                environment.insert("SSH_ASKPASS_REQUIRE".into(), "force".into());
            }
        }
        other => {
            return Err(format!(
                "Signing format '{other}' is not supported. No unsigned commit was created."
            ));
        }
    }
    let output = super::process::run(
        super::process::Spec {
            invocation: super::process::Invocation { program, arguments },
            fallback: None,
            cwd: cwd.to_owned(),
            environment,
            environment_os: vec![],
            input: body.to_vec(),
            timeout: Duration::from_secs(120),
            output_limit: 65_536,
            fail_on_output_limit: true,
            #[cfg(all(test, windows))]
            synchronize_before_read: false,
        },
        cancel,
    )?;
    if !output.success() {
        return Err("Signing failed, was cancelled, timed out, or exceeded output limits. Unlock the key in gpg-agent/ssh-agent, or configure graphical pinentry/SSH_ASKPASS. No unsigned fallback was used.".into());
    }
    let signature =
        String::from_utf8(output.stdout).map_err(|_| "Signer returned invalid text.")?;
    let (begin, end) = if info.format == "ssh" {
        (
            "-----BEGIN SSH SIGNATURE-----",
            "-----END SSH SIGNATURE-----",
        )
    } else {
        (
            "-----BEGIN PGP SIGNATURE-----",
            "-----END PGP SIGNATURE-----",
        )
    };
    if !signature.starts_with(begin)
        || !signature.trim_end().ends_with(end)
        || signature.contains('\0')
    {
        return Err(
            "Signer did not return a valid armored signature. Commit was not published.".into(),
        );
    }
    Ok(signature)
}
