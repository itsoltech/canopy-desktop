//! Explicit branch transport operations. Git CLI and shells are never invoked.
use super::{Result, err};
use git2::{build::CheckoutBuilder, *};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct SshDiagnostics {
    username: Option<String>,
    offered_keys: Vec<super::ssh_credentials::KeyCandidate>,
    candidates: Vec<super::ssh_credentials::KeyCandidate>,
    config_present: bool,
}

impl SshDiagnostics {
    fn authentication_message(&self) -> String {
        use super::ssh_credentials::KeyProtection;

        let username = self.username.as_deref().unwrap_or("git");
        let mut message = if self.candidates.is_empty() {
            format!(
                "SSH authentication failed for user '{username}'. Windows OpenSSH agent/Pageant did not provide an accepted key, and no default id_rsa, id_ecdsa or id_ed25519 key was found. Start an agent, load the required key with ssh-add, and retry."
            )
        } else if self
            .candidates
            .iter()
            .any(|key| key.protection == KeyProtection::Encrypted)
            && !self
                .candidates
                .iter()
                .any(|key| key.protection == KeyProtection::Unencrypted)
        {
            let names = key_names(&self.candidates);
            format!(
                "SSH authentication failed for user '{username}'. The available default key ({names}) is encrypted, and Canopy cannot request its passphrase through libgit2. Load it into Windows OpenSSH agent or Pageant and retry."
            )
        } else if self
            .offered_keys
            .iter()
            .any(|key| key.protection == KeyProtection::Unencrypted)
        {
            let names = key_names(&self.offered_keys);
            format!(
                "SSH authentication failed for user '{username}'. The server rejected the agent and available default key ({names}); verify the SSH username and repository access."
            )
        } else {
            let names = key_names(&self.candidates);
            format!(
                "SSH authentication failed for user '{username}'. The agent and default key ({names}) were unavailable, unsupported or rejected. Load the intended key into Windows OpenSSH agent or Pageant and retry."
            )
        };
        if self.config_present {
            message.push_str(
                " Canopy uses libgit2/libssh2 and does not apply Host, User or IdentityFile from ~/.ssh/config; use the real host and username in the remote URL and load custom keys into the agent.",
            );
        }
        message
    }
}

fn key_names(candidates: &[super::ssh_credentials::KeyCandidate]) -> String {
    let mut names = candidates
        .iter()
        .filter_map(|candidate| candidate.path.file_name()?.to_str().map(str::to_owned))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    if names.is_empty() {
        "unknown format".into()
    } else {
        names.join(", ")
    }
}

fn is_ssh_url(url: &str) -> bool {
    url.get(..6)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("ssh://"))
        || !url.contains("://") && url.rsplit_once(':').is_some()
}

fn ssh_username(user: Option<&str>) -> &str {
    user.filter(|user| !user.is_empty()).unwrap_or("git")
}

fn selected_username<'a>(
    url: &str,
    callback_user: Option<&'a str>,
    configured_user: Option<&'a str>,
) -> &'a str {
    if is_ssh_url(url) {
        ssh_username(callback_user)
    } else {
        callback_user.or(configured_user).unwrap_or("git")
    }
}

fn transport_error(error: Error, diagnostics: &Arc<Mutex<SshDiagnostics>>) -> String {
    let raw = error.message().trim().trim_end_matches(':').trim();
    let authentication = error.code() == ErrorCode::Auth
        || error.class() == ErrorClass::Ssh && raw.to_ascii_lowercase().contains("authenticat");
    if authentication {
        let diagnostics = diagnostics.lock().unwrap();
        let mut message = diagnostics.authentication_message();
        if !raw.is_empty()
            && raw != "failed to authenticate SSH session"
            && raw != "remote rejected authentication"
            && raw != message
        {
            message.push_str(&format!(" Transport: {raw}."));
        }
        message.push_str(&format!(
            " [git2 code={:?}, class={:?}]",
            error.code(),
            error.class()
        ));
        message
    } else if raw.is_empty() {
        format!(
            "Git transport failed [git2 code={:?}, class={:?}].",
            error.code(),
            error.class()
        )
    } else {
        raw.to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    pub remote: String,
    pub branch: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Pull,
    Push,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
    pub remotes: Vec<String>,
    pub upstream: Option<Upstream>,
    endpoints: HashMap<String, (String, Option<String>)>,
}
pub fn prepare(path: PathBuf) -> Result<Plan> {
    let repo = Repository::open(&path).map_err(err)?;
    let head = repo
        .head()
        .map_err(|_| "Create a commit on a local branch first.".to_owned())?;
    if !head.is_branch() {
        return Err("Select a local branch; HEAD is detached.".into());
    }
    let branch = head.shorthand().map_err(err)?.to_owned();
    let config = repo.config().map_err(err)?;
    let remote = config.get_string(&format!("branch.{branch}.remote")).ok();
    let merge = config.get_string(&format!("branch.{branch}.merge")).ok();
    let upstream = match (remote, merge) {
        (Some(remote), Some(merge)) => Some(Upstream {
            remote,
            branch: merge
                .strip_prefix("refs/heads/")
                .ok_or("Upstream must refer to a branch.")?
                .to_owned(),
        }),
        _ => None,
    };
    let mut remotes = Vec::new();
    for name in repo.remotes().map_err(err)?.iter() {
        if let Some(name) = name.map_err(err)? {
            remotes.push(name.to_owned());
        }
    }
    let mut endpoints = HashMap::new();
    for name in &remotes {
        let remote = repo.find_remote(name).map_err(err)?;
        endpoints.insert(
            name.clone(),
            (
                remote.url().map_err(err)?.to_owned(),
                remote.pushurl().map_err(err)?.map(str::to_owned),
            ),
        );
    }
    remotes.sort_by_key(|name| (name != "origin", name.clone()));
    Ok(Plan {
        path,
        branch,
        head: head.peel_to_commit().map_err(err)?.id().to_string(),
        remotes,
        upstream,
        endpoints,
    })
}
fn unchanged(repo: &Repository, plan: &Plan) -> Result<()> {
    let head = repo.head().map_err(err)?;
    if head.name().map_err(err)? != format!("refs/heads/{}", plan.branch)
        || head.target().map(|id| id.to_string()).as_ref() != Some(&plan.head)
    {
        return Err("The branch changed during this operation. Refresh and try again.".into());
    }
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the current Git operation first.".into());
    }
    Ok(())
}
fn active_hook(repo: &Repository, name: &str) -> Result<()> {
    let directory = repo
        .config()
        .map_err(err)?
        .get_path("core.hooksPath")
        .ok()
        .map(|p| {
            if p.is_absolute() {
                p
            } else {
                repo.workdir().unwrap_or(repo.path()).join(p)
            }
        })
        .unwrap_or_else(|| repo.commondir().join("hooks"));
    if let Ok(metadata) = std::fs::metadata(directory.join(name)) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                return Err(format!(
                    "An active {name} hook requires terminal execution; it will not be skipped."
                ));
            }
        }
        #[cfg(windows)]
        if metadata.is_file() {
            return Err(format!(
                "An active {name} hook requires terminal execution; it will not be skipped."
            ));
        }
    }
    Ok(())
}
fn callbacks(
    cancel: Arc<AtomicBool>,
    username: Option<String>,
    use_http_path: bool,
) -> (RemoteCallbacks<'static>, Arc<Mutex<SshDiagnostics>>) {
    let mut callbacks = RemoteCallbacks::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    let stopped = move || cancel.load(Ordering::Acquire) || Instant::now() > deadline;
    let transfer_stop = stopped.clone();
    callbacks.transfer_progress(move |_| !transfer_stop());
    let sideband_stop = stopped.clone();
    callbacks.sideband_progress(move |_| !sideband_stop());
    let negotiation_stop = stopped.clone();
    callbacks.push_negotiation(move |_| {
        if negotiation_stop() {
            Err(Error::from_str("Operation cancelled or timed out."))
        } else {
            Ok(())
        }
    });
    callbacks.push_update_reference(|_, status| match status {
        Some(reason) => Err(Error::from_str(&format!("Remote rejected push: {reason}"))),
        None => Ok(()),
    });
    let home = crate::platform::directories::user_home().ok();
    let mut ssh = super::ssh_credentials::SshCredentials::new(home.as_deref());
    let ssh_diagnostics = Arc::new(Mutex::new(SshDiagnostics {
        candidates: ssh.candidates().to_vec(),
        config_present: ssh.config_present(),
        ..Default::default()
    }));
    let credential_diagnostics = ssh_diagnostics.clone();
    let mut attempts = 0;
    callbacks.credentials(move |url, user, allowed| {
        attempts += 1;
        if stopped() { return Err(Error::from_str("Authentication cancelled or timed out.")); }
        if attempts > 8 { return Err(Error::from_str("Authentication rejected all available credentials.")); }
        let callback_user = user;
        if allowed.contains(CredentialType::SSH_KEY) {
            use super::ssh_credentials::Attempt;
            let user = selected_username(url, callback_user, username.as_deref());
            credential_diagnostics.lock().unwrap().username = Some(user.to_owned());
            while let Some(attempt) = ssh.next() {
                let credential = match attempt {
                    Attempt::Agent => Cred::ssh_key_from_agent(user),
                    Attempt::KeyFile(candidate) => {
                        credential_diagnostics
                            .lock()
                            .unwrap()
                            .offered_keys
                            .push(candidate.clone());
                        Cred::ssh_key(
                            user,
                            candidate.public_path.as_deref(),
                            &candidate.path,
                            None,
                        )
                    }
                };
                if let Ok(credential) = credential { return Ok(credential); }
            }
            let message = credential_diagnostics
                .lock()
                .unwrap()
                .authentication_message();
            return Err(Error::new(ErrorCode::Auth, ErrorClass::Ssh, message));
        }
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            let user = callback_user.or(username.as_deref());
            if let Ok((username, password)) =
                super::https_credentials::load(url, user, use_http_path)
            {
                return Cred::userpass_plaintext(&username, &password);
            }
            #[cfg(windows)]
            {
                let target = super::https_credentials::target_name(url, use_http_path)
                    .unwrap_or_else(|| "git:https://<host>".into());
                return Err(Error::from_str(&format!("HTTPS needs a Windows Generic Credential named '{target}' with its username field set. Configure it through Git Credential Manager or Windows Credential Manager, then retry.")));
            }
            #[cfg(target_os = "macos")]
            return Err(Error::from_str("HTTPS needs a matching macOS internet-password item and a username in the remote URL or credential.username. Alternatively use an SSH remote with ssh-agent."));
            #[cfg(not(any(windows, target_os = "macos")))]
            return Err(Error::from_str("HTTPS credential storage is unavailable on this platform. Use an SSH remote with ssh-agent."));
        }
        if allowed.contains(CredentialType::USERNAME) {
            let user = selected_username(url, callback_user, username.as_deref());
            return Cred::username(user);
        }
        Err(Error::from_str("No supported credentials. Load your SSH key into ssh-agent."))
    });
    (callbacks, ssh_diagnostics)
}

pub fn execute(
    plan: Plan,
    operation: Operation,
    selected: Option<Upstream>,
    cancel: Arc<AtomicBool>,
) -> Result<String> {
    let repo = Repository::open(&plan.path).map_err(err)?;
    unchanged(&repo, &plan)?;
    if cancel.load(Ordering::Acquire) {
        return Err("Operation cancelled.".into());
    }
    let target = selected
        .or_else(|| plan.upstream.clone())
        .ok_or("Choose an upstream first.")?;
    if !plan.remotes.contains(&target.remote) {
        return Err("Choose a configured remote.".into());
    }
    if !Reference::is_valid_name(&format!("refs/heads/{}", target.branch))
        || target.branch.is_empty()
    {
        return Err("Enter a valid remote branch name.".into());
    }
    let mut remote = repo.find_remote(&target.remote).map_err(err)?;
    let endpoint = (
        remote.url().map_err(err)?.to_owned(),
        remote.pushurl().map_err(err)?.map(str::to_owned),
    );
    if plan.endpoints.get(&target.remote) != Some(&endpoint) {
        return Err("Remote URL changed since the operation was requested. Try again.".into());
    }
    if operation == Operation::Push && endpoint.1.as_ref().is_some_and(|url| url != &endpoint.0) {
        return Err("This remote has a separate push URL. Use the terminal until separate push/fetch destinations are supported.".into());
    }
    let username = repo
        .config()
        .map_err(err)?
        .get_string("credential.username")
        .ok();
    let use_http_path = repo
        .config()
        .map_err(err)?
        .get_bool("credential.useHttpPath")
        .unwrap_or(false);
    let local_ref = format!("refs/heads/{}", plan.branch);
    let remote_ref = format!("refs/heads/{}", target.branch);
    let mapping = remote
        .refspecs()
        .find(|spec| spec.direction() == Direction::Fetch && spec.src_matches(&remote_ref));
    let add_mapping = mapping.is_none();
    let tracking_ref = match mapping {
        Some(spec) => spec
            .transform(&remote_ref)
            .map_err(err)?
            .as_str()
            .map_err(err)?
            .to_owned(),
        None => format!("refs/remotes/{}/{}", target.remote, target.branch),
    };
    if !tracking_ref.starts_with("refs/remotes/") {
        return Err(
            "Remote fetch mapping must target remote-tracking refs, not local branches.".into(),
        );
    }
    match operation {
        Operation::Push => {
            active_hook(&repo, "pre-push")?;
            let mut options = PushOptions::new();
            let (remote_callbacks, diagnostics) =
                callbacks(cancel.clone(), username, use_http_path);
            options
                .remote_callbacks(remote_callbacks)
                .packbuilder_parallelism(1);
            // Pin the source OID: a concurrent external commit must not change what is sent.
            remote
                .push(&[format!("{}:{remote_ref}", plan.head)], Some(&mut options))
                .map_err(|error| transport_error(error, &diagnostics))?;
            // A successful push remains successful even if cancellation arrived after acceptance.
            repo.reference(
                &tracking_ref,
                Oid::from_str(&plan.head).map_err(err)?,
                true,
                "Canopy push",
            )
            .map_err(|e| {
                format!(
                    "Push succeeded, but tracking update failed: {}",
                    e.message()
                )
            })?;
            unchanged(&repo, &plan).map_err(|e| format!("Push succeeded. {e}"))?;
        }
        Operation::Pull => {
            active_hook(&repo, "post-merge")?;
            let mut fetch = FetchOptions::new();
            let (remote_callbacks, diagnostics) =
                callbacks(cancel.clone(), username, use_http_path);
            fetch
                .remote_callbacks(remote_callbacks)
                .download_tags(AutotagOption::None);
            remote
                .fetch(
                    &[format!("+{remote_ref}:{tracking_ref}")],
                    Some(&mut fetch),
                    Some("Canopy pull"),
                )
                .map_err(|error| transport_error(error, &diagnostics))?;
            if cancel.load(Ordering::Acquire) {
                return Err("Fetch completed; pull cancelled before updating the worktree.".into());
            }
            unchanged(&repo, &plan)?;
            let original = Oid::from_str(&plan.head).map_err(err)?;
            let target_id = repo.refname_to_id(&tracking_ref).map_err(err)?;
            if original != target_id
                && !repo.graph_descendant_of(original, target_id).map_err(err)?
            {
                if !repo.graph_descendant_of(target_id, original).map_err(err)? {
                    return Err("Branches have diverged. Fetch completed; merge or rebase is required before pulling.".into());
                }
                let mut status = StatusOptions::new();
                status.include_untracked(true).recurse_untracked_dirs(true);
                if !repo.statuses(Some(&mut status)).map_err(err)?.is_empty() {
                    return Err(
                        "Commit or stash working-tree changes before pulling. Fetch completed."
                            .into(),
                    );
                }
                let object = repo.find_object(target_id, None).map_err(err)?;
                // Lock both HEAD and its branch during checkout/ref publication.
                let mut tx = repo.transaction().map_err(err)?;
                tx.lock_ref("HEAD").map_err(err)?;
                tx.lock_ref(&local_ref).map_err(err)?;
                unchanged(&repo, &plan)?;
                let mut checkout = CheckoutBuilder::new();
                checkout.safe().overwrite_ignored(false);
                repo.checkout_tree(&object, Some(&mut checkout))
                    .map_err(err)?;
                tx.set_target(&local_ref, target_id, None, "Canopy pull: fast-forward")
                    .map_err(err)?;
                tx.commit().map_err(|e| {
                    format!(
                        "Files were updated but the branch could not be advanced: {}",
                        e.message()
                    )
                })?;
            }
        }
    }
    if add_mapping {
        repo.remote_add_fetch(&target.remote, &format!("+{remote_ref}:{tracking_ref}"))
            .map_err(|e| {
                format!(
                    "Transfer succeeded, but fetch mapping could not be saved: {}",
                    e.message()
                )
            })?;
    }
    if plan.upstream.as_ref() != Some(&target) {
        let mut config = repo.config().map_err(|e| {
            format!(
                "Transfer succeeded, but upstream configuration is unavailable: {}",
                e.message()
            )
        })?;
        config
            .set_str(&format!("branch.{}.remote", plan.branch), &target.remote)
            .map_err(|e| {
                format!(
                    "Transfer succeeded, but upstream could not be saved: {}",
                    e.message()
                )
            })?;
        config
            .set_str(&format!("branch.{}.merge", plan.branch), &remote_ref)
            .map_err(|e| {
                format!(
                    "Transfer succeeded, but upstream could not be saved completely: {}",
                    e.message()
                )
            })?;
    }
    Ok(format!(
        "{} {}/{}",
        if operation == Operation::Push {
            "Pushed to"
        } else {
            "Pulled from"
        },
        target.remote,
        target.branch
    ))
}

#[cfg(test)]
mod ssh_diagnostic_tests {
    use super::super::ssh_credentials::{KeyCandidate, KeyProtection};
    use super::*;

    fn auth_error(diagnostics: SshDiagnostics) -> String {
        transport_error(
            Error::new(
                ErrorCode::Auth,
                ErrorClass::Ssh,
                "failed to authenticate SSH session:",
            ),
            &Arc::new(Mutex::new(diagnostics)),
        )
    }

    fn candidate(name: &str, protection: KeyProtection) -> KeyCandidate {
        KeyCandidate {
            path: PathBuf::from(name),
            public_path: None,
            protection,
        }
    }

    #[test]
    fn ssh_username_never_uses_the_https_credential_username() {
        assert_eq!(
            selected_username("ssh://example.test/repo", None, Some("web-account")),
            "git"
        );
        assert_eq!(
            selected_username("git@example.test:repo", Some("deploy"), Some("web-account")),
            "deploy"
        );
        assert_eq!(
            selected_username("https://example.test/repo", None, Some("web-account")),
            "web-account"
        );
    }

    #[test]
    fn missing_agent_and_keys_has_an_actionable_error_without_an_empty_colon() {
        let error = auth_error(SshDiagnostics {
            username: Some("git".into()),
            ..Default::default()
        });
        assert!(error.contains("OpenSSH agent/Pageant"));
        assert!(error.contains("no default"));
        assert!(error.contains("code=Auth"));
        assert!(!error.ends_with(':'));
    }

    #[test]
    fn encrypted_and_server_rejected_keys_have_distinct_messages() {
        let encrypted = candidate("id_ed25519", KeyProtection::Encrypted);
        let error = auth_error(SshDiagnostics {
            username: Some("git".into()),
            candidates: vec![encrypted.clone()],
            offered_keys: vec![encrypted],
            ..Default::default()
        });
        assert!(error.contains("encrypted"));
        assert!(error.contains("cannot request its passphrase"));

        let plain = candidate("id_ed25519", KeyProtection::Unencrypted);
        let error = auth_error(SshDiagnostics {
            username: Some("deploy".into()),
            candidates: vec![plain.clone()],
            offered_keys: vec![plain],
            ..Default::default()
        });
        assert!(error.contains("server rejected"));
        assert!(error.contains("user 'deploy'"));
        assert!(!error.contains("passphrase"));
    }

    #[test]
    fn openssh_config_limitation_is_explicit() {
        let error = auth_error(SshDiagnostics {
            config_present: true,
            ..Default::default()
        });
        assert!(error.contains("does not apply Host, User or IdentityFile"));
    }
}

#[cfg(test)]
mod ssh_probe {
    use super::*;
    #[test]
    #[ignore = "Read-only SSH handshake against CANOPY_TEST_SSH_REPO; uses the user's SSH identity only when explicitly requested"]
    fn ssh_connection_probe() {
        let path = std::env::var("CANOPY_TEST_SSH_REPO").expect("explicit test repository");
        let repo = Repository::open(path).unwrap();
        let mut remote = repo.find_remote("origin").unwrap();
        let (remote_callbacks, diagnostics) =
            callbacks(Arc::new(AtomicBool::new(false)), None, false);
        let connection = remote
            .connect_auth(Direction::Fetch, Some(remote_callbacks), None)
            .map_err(|error| transport_error(error, &diagnostics))
            .expect("read-only SSH handshake");
        assert!(!connection.list().unwrap().is_empty());
        // Dropping the connection disconnects: no fetch, index, checkout or ref updates.
    }
}
