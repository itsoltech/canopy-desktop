//! Catalog-level chat operations of the remote bridge: summaries without transcripts,
//! creating a chat without a tab (OWN-07) and deleting one after a preflight whose
//! conditions the host checks again right before the effect (LIFE-03, LIFE-10). Each
//! mutation records the same durable states as the runtime's handlers.
use super::runtime::RemoteKey;
use super::*;
use canopy_desktop::host::{
    CHAT, ChatRecord, CommandInput, CommandKey, CommandReceipt, CommandState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A host chat in the catalog, as lists show it (SYNC-05: no transcript).
#[derive(Clone, Debug, PartialEq)]
pub struct ChatSummary {
    pub chat: String,
    /// The catalog workspace (`None`: Private chats).
    pub workspace: Option<String>,
    pub tool: String,
    pub title: Option<String>,
    pub status: &'static str,
    pub working: bool,
    pub decisions: usize,
    pub unsent: usize,
    pub observers: usize,
    /// Its agent and account must be chosen on the host first.
    pub recovery: Option<String>,
}

/// A new chat a remote client creates; the client picks its UUID, so a retried
/// command names the same chat.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewChat {
    pub chat: String,
    /// Catalog workspace (`None`: Private chats, in a folder of its own).
    pub workspace: Option<WorkspaceId>,
    pub tool: String,
    /// The account, chosen explicitly: no default account is picked for a remote chat.
    pub profile: String,
    pub model: Option<String>,
}
impl NewChat {
    /// The inbox record under `authority`: the project's canonical checkout or
    /// "private" (what device grants are checked against).
    fn command(&self, key: RemoteKey, authority: String) -> Option<CommandInput> {
        uuid::Uuid::parse_str(&self.chat).ok()?;
        inbox(key, "chat.create", self.chat.clone(), self, authority)
    }
}

/// The inbox record of a catalog command: typed arguments, and the fingerprint of the
/// wire request when it came from a device (else of the arguments).
fn inbox(
    key: RemoteKey,
    method: &str,
    target: String,
    arguments: &impl Serialize,
    root: String,
) -> Option<CommandInput> {
    let arguments = serde_json::to_vec(arguments).ok()?;
    let typed = Sha256::digest(&arguments).to_vec();
    let input = CommandInput::new(key.key, method.into(), target, arguments, typed)?;
    match key.fingerprint {
        Some(fingerprint) => input.with_fingerprint(fingerprint),
        None => Some(input),
    }?
    .with_root(root)
}

/// The canonical checkout of open project `id` as an authority root, resolved on a
/// worker; `None` when the project isn't open or its folder is gone.
async fn authority_of(id: Option<WorkspaceId>, cx: &mut AsyncApp) -> Option<String> {
    let Some(id) = id else {
        return Some("private".into());
    };
    let checkout = cx.update(|cx| project_cwd(id, cx))?;
    cx.background_spawn(async move { std::fs::canonicalize(checkout) })
        .await
        .ok()?
        .to_str()
        .map(str::to_owned)
}

/// A fork a remote client asks for: chat `source` continued after turn end `item` of
/// its history part `history_epoch`, as new chat `chat` (UUID picked by the client)
/// in `workspace` (`None`: Private chats). No tab opens on the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForkChat {
    pub source: String,
    pub history_epoch: u64,
    pub item: u64,
    pub chat: String,
    pub workspace: Option<WorkspaceId>,
    /// The fingerprint of the edited first message queued with the fork (an edit
    /// made as a fork): the store accepts exactly that message with it, or none.
    #[serde(default)]
    pub replacement_fingerprint: Option<Vec<u8>>,
}
impl ForkChat {
    fn command(&self, key: RemoteKey, authority: String) -> Option<CommandInput> {
        uuid::Uuid::parse_str(&self.chat).ok()?;
        inbox(key, "chat.fork", self.chat.clone(), self, authority)
    }
}

/// The edited message a fork starts with, sent under its own derived command (same
/// device and epoch as the fork). It is queued in the fork's completing transaction.
#[derive(Clone, Debug)]
pub struct ForkReplacement {
    pub key: CommandKey,
    pub message: String,
    pub draft: canopy_desktop::host::QueueDraft,
    /// The canonical folder of the private source its files were resolved in (a
    /// private fork only): the fork gets copies of them in its own folder.
    pub source_folder: Option<std::path::PathBuf>,
}
impl ForkReplacement {
    /// The fingerprint its send command carries, derived from the fork's own (wire)
    /// fingerprint so a retry of the fork names the same message.
    pub fn fingerprint(fork: &[u8]) -> Vec<u8> {
        let mut hash = Sha256::new();
        hash.update(b"canopy/fork-replacement/v1\0");
        hash.update(fork);
        hash.finalize().to_vec()
    }
}

/// The catalog record of fork `fork` made from `request` in folder `cwd`.
fn fork_record(
    fork: &ForkChat,
    request: &super::ForkRequest,
    cwd: std::path::PathBuf,
    authority: &str,
) -> Option<ChatRecord> {
    let mut settings = request.settings.clone();
    settings.id = Some(fork.chat.clone());
    // Permissions remembered for a chat stay with it.
    settings.allowed.clear();
    let metadata = PaneMetadata {
        cwd: Some(cwd),
        profile_id: request.profile.clone(),
        // The session to branch off; the new one replaces it once it has an id.
        resume_id: Some(request.session.clone()),
        title: Some(format!("{} (fork)", request.title)),
        kind: PaneKind::Chat,
        chat: Some(Box::new(settings)),
        ..Default::default()
    };
    let root = fork
        .workspace
        .and_then(|id| serde_json::to_string(&id).ok());
    let record = ChatRecord::new(
        fork.chat.clone(),
        root,
        request.provider.tool().into(),
        metadata,
    )?;
    match fork.workspace {
        Some(_) => record.with_authority_root(authority.to_owned()),
        None => Some(record),
    }
}

/// What deleting a chat loses now; the delete is refused when this changed since.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletePreflight {
    pub chat: String,
    pub conversation: bool,
    pub working: bool,
    pub unsent: usize,
    pub commands: usize,
    pub observers: usize,
    /// Why it can't be deleted now (a folder still in use), if anything.
    pub blocked: Option<String>,
}
impl DeletePreflight {
    pub fn command(&self, key: RemoteKey, root: String) -> Option<CommandInput> {
        inbox(key, "chat.delete", self.chat.clone(), self, root)
    }
}

/// Where private chat folders are made: the app's data directory (under test, one of
/// the test run's own).
fn chats_data() -> Result<std::path::PathBuf, String> {
    #[cfg(test)]
    {
        static DATA: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let data = DATA.get_or_init(|| tempfile::tempdir().expect("a test data directory"));
        Ok(data.path().to_owned())
    }
    #[cfg(not(test))]
    canopy_desktop::platform::directories::data_dir()
}

fn reason(text: &str) -> Option<Vec<u8>> {
    serde_json::to_vec(&serde_json::json!({ "reason": text })).ok()
}

impl Chats {
    /// Every catalog chat with its live state, with or without a pane or runtime.
    pub fn chat_summaries(&self, cx: &App) -> Vec<ChatSummary> {
        let settings = cx.global::<AppState>().settings.read(cx);
        let mut summaries: Vec<_> = settings
            .chat_records()
            .map(|record| {
                let runtime = self.runtimes.get(record.id()).map(|r| r.read(cx));
                ChatSummary {
                    chat: record.id().to_owned(),
                    workspace: record.workspace().map(str::to_owned),
                    tool: record.agent().to_owned(),
                    title: runtime
                        .and_then(|r| r.pane_ref().metadata.title.clone())
                        .or_else(|| record.metadata().title.clone()),
                    status: runtime.map_or("Not started", |r| r.navigation_status()),
                    working: runtime.is_some_and(|r| r.is_working()),
                    decisions: runtime.map_or(0, |r| r.pending_decisions().len()),
                    unsent: runtime.map_or(0, |r| r.unsent()),
                    observers: runtime.map_or(0, |r| r.observers()),
                    recovery: record.recovery_reason().map(str::to_owned),
                }
            })
            .collect();
        summaries.sort_by(|a, b| a.chat.cmp(&b.chat));
        summaries
    }

    /// Creates chat `new.chat` in the host catalog and its runtime, without a tab or
    /// window; the host sees it in its project or Private chats (OWN-07).
    pub fn create_chat(
        &mut self,
        key: RemoteKey,
        new: NewChat,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let app = cx.global::<AppState>().clone();
        let Some(client) = app.settings.read(cx).client() else {
            return Task::ready(Err("Chat storage is unavailable.".into()));
        };
        cx.spawn(async move |this, cx| {
            let authority = authority_of(new.workspace, cx)
                .await
                .ok_or("This project is not open on the host.")?;
            let input = new
                .command(key, authority.clone())
                .ok_or("This chat can't be created as asked.")?;
            let key = input.key().clone();
            let receipt = client
                .accept_runtime_command(input, CHAT)
                .await
                .map_err(|e| e.to_string())?;
            if receipt.state() != CommandState::Accepted {
                return Ok(receipt);
            }
            // Validated now, with the catalog as it is (CMD-07).
            let checked = this
                .update(cx, |this, cx| this.check_new_chat(&new, cx))
                .map_err(|e| e.to_string())?;
            let (tool, folder, mode) = match checked {
                Ok(checked) => checked,
                Err(error) => {
                    return client
                        .transition_runtime_command(
                            key,
                            CommandState::Accepted,
                            CommandState::Rejected,
                            reason(&error),
                        )
                        .await
                        .map_err(|e| e.to_string());
                }
            };
            if let Err(error) = client
                .transition_runtime_command(
                    key.clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
            {
                return match client.get_runtime_command(key, CHAT).await {
                    Ok(Some(receipt)) => Ok(receipt),
                    _ => Err(error.to_string()),
                };
            }
            let folder = match folder {
                Some(folder) => Ok(folder),
                None => {
                    cx.background_executor()
                        .spawn(async {
                            let data = canopy_desktop::platform::directories::data_dir()?;
                            canopy_desktop::chat::scratch::create(&data)
                        })
                        .await
                }
            };
            let record = folder.and_then(|cwd| {
                let settings = new_settings(&new, &tool, mode);
                let metadata = PaneMetadata {
                    cwd: Some(cwd),
                    profile_id: Some(new.profile.clone()),
                    title: Some(NEW_CHAT_TITLE.into()),
                    chat_title_state: Some(
                        canopy_desktop::state::workspace::ChatTitleState::Placeholder,
                    ),
                    kind: PaneKind::Chat,
                    chat: Some(Box::new(settings)),
                    ..Default::default()
                };
                let root = new.workspace.and_then(|id| serde_json::to_string(&id).ok());
                ChatRecord::new(new.chat.clone(), root, new.tool.clone(), metadata)
                    .and_then(|record| match new.workspace {
                        Some(_) => record.with_authority_root(authority.clone()),
                        None => Some(record),
                    })
                    .ok_or_else(|| "The chat record is not valid.".to_owned())
            });
            let saved = match record {
                Ok(record) => client
                    .save_runtime_chat(record.clone())
                    .await
                    .map(|()| record)
                    .map_err(|e| e.to_string()),
                Err(error) => Err(error),
            };
            let (state, result) = match saved {
                Ok(record) => {
                    let started = this.update(cx, |this, cx| {
                        let settings = cx.global::<AppState>().settings.clone();
                        settings.update(cx, |settings, cx| {
                            settings.record_saved(record.clone());
                            cx.notify();
                        });
                        this.ensure_runtime_for_chat(record.id(), cx).map(|_| ())
                    });
                    match started {
                        Ok(Ok(())) => {
                            (CommandState::Succeeded, Some(new.chat.clone().into_bytes()))
                        }
                        // The chat exists in the catalog; its runtime can start later.
                        Ok(Err(error)) => (CommandState::Succeeded, reason(&error)),
                        Err(error) => (CommandState::Succeeded, reason(&error.to_string())),
                    }
                }
                Err(error) => (CommandState::Failed, reason(&error)),
            };
            client
                .transition_runtime_command(key, CommandState::Attempting, state, result)
                .await
                .map_err(|e| e.to_string())
        })
    }

    /// Forks a conversation into a new chat without a tab (CHAT-08): its history up to
    /// the turn end is copied first (by the history owner), then one transaction
    /// publishes the chat, its inherited image references, the edited first message
    /// (`replacement`) and the command's success; nothing of the fork is listed before
    /// that, and a failure leaves no partial fork. The agent branches the session off
    /// at that point when it starts.
    pub fn fork_chat(
        &mut self,
        key: RemoteKey,
        fork: ForkChat,
        replacement: Option<ForkReplacement>,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let app = cx.global::<AppState>().clone();
        let Some(client) = app.settings.read(cx).client() else {
            return Task::ready(Err("Chat storage is unavailable.".into()));
        };
        if replacement.is_some() != fork.replacement_fingerprint.is_some() {
            return Task::ready(Err("This fork can't be made as asked.".into()));
        }
        cx.spawn(async move |this, cx| {
            let authority = authority_of(fork.workspace, cx)
                .await
                .ok_or("This project is not open on the host.")?;
            let input = fork
                .command(key, authority.clone())
                .ok_or("This fork can't be made as asked.")?;
            let key = input.key().clone();
            let receipt = client
                .accept_runtime_command(input, CHAT)
                .await
                .map_err(|e| e.to_string())?;
            if receipt.state() != CommandState::Accepted {
                return Ok(receipt);
            }
            let request = this
                .update(cx, |this, cx| {
                    let taken = app.settings.read(cx).chat_record(&fork.chat).is_some()
                        || this.runtimes.contains_key(&fork.chat);
                    if taken {
                        return Err("chat_exists".to_owned());
                    }
                    let source = this.ensure_runtime_for_chat(&fork.source, cx)?;
                    source
                        .read(cx)
                        .fork_request(fork.history_epoch, fork.item)
                        .ok_or_else(|| "There is no saved point to fork at here.".to_owned())
                })
                .map_err(|e| e.to_string())?;
            let request = match request {
                Ok(request) => request,
                Err(error) => {
                    return client
                        .transition_runtime_command(
                            key,
                            CommandState::Accepted,
                            CommandState::Rejected,
                            reason(&error),
                        )
                        .await
                        .map_err(|e| e.to_string());
                }
            };
            if let Err(error) = client
                .transition_runtime_command(
                    key.clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
            {
                return match client.get_runtime_command(key, CHAT).await {
                    Ok(Some(receipt)) => Ok(receipt),
                    _ => Err(error.to_string()),
                };
            }
            let copied = cx
                .update(|cx| {
                    app.chat_history.update(cx, |history, cx| {
                        history.fork(&request.source, &fork.chat, request.cut.clone(), cx)
                    })
                })
                .await;
            let mut created = None;
            let mut replacement = replacement;
            let folder = match (copied, fork.workspace) {
                (Err(error), _) => Err(format!("Could not fork the chat: {error}")),
                (Ok(()), Some(id)) => cx
                    .update(|cx| project_cwd(id, cx))
                    .ok_or_else(|| "This project is no longer open.".to_owned()),
                (Ok(()), None) => {
                    // The files the edited message attaches are the source's, checked
                    // against its folder as it is now.
                    let first = replacement.take();
                    let source = cx.update(|cx| {
                        let settings = app.settings.read(cx);
                        let source = settings.chat_record(&fork.source)?;
                        source
                            .workspace()
                            .is_none()
                            .then(|| source.metadata().cwd.clone())
                            .flatten()
                    });
                    let (made, carried) = cx
                        .background_executor()
                        .spawn(async move {
                            let made = chats_data()
                                .and_then(|data| canopy_desktop::chat::scratch::create(&data))
                                .map_err(|e| format!("Could not create the chat folder: {e}"));
                            let Ok(cwd) = &made else {
                                return (made, Ok(first));
                            };
                            let carried = first
                                .map(|mut first| {
                                    let source = match (&first.source_folder, source) {
                                        (None, _) => None,
                                        (Some(folder), Some(now))
                                            if std::fs::canonicalize(&now).ok().as_ref()
                                                == Some(folder) =>
                                        {
                                            Some(folder.as_path())
                                        }
                                        (Some(_), _) => {
                                            return Err("conditions_changed: The source chat's \
                                                        folder changed; fork again."
                                                .to_owned());
                                        }
                                    };
                                    first.draft = fork_files::carry(
                                        first.draft,
                                        source,
                                        cwd,
                                        fork_files::BOUND,
                                    )?;
                                    Ok(first)
                                })
                                .transpose();
                            (made, carried)
                        })
                        .await;
                    created = made.as_ref().ok().cloned();
                    made.and_then(|cwd| {
                        replacement = carried?;
                        Ok(cwd)
                    })
                }
            };
            let record = folder.and_then(|cwd| {
                fork_record(&fork, &request, cwd, &authority)
                    .ok_or_else(|| "The fork's record is not valid.".to_owned())
            });
            let completed = match record {
                Err(error) => Err(error),
                Ok(record) => {
                    let first = match (replacement, &fork.replacement_fingerprint) {
                        (None, _) => Ok(None),
                        (Some(first), Some(fingerprint)) => first
                            .draft
                            .command_for_message(
                                first.key,
                                &fork.chat,
                                first.message.clone(),
                                record.send_context(),
                            )
                            .and_then(|(input, draft)| {
                                Some((
                                    input.with_fingerprint(fingerprint.clone())?,
                                    first.message,
                                    draft,
                                ))
                            })
                            .map(Some)
                            .ok_or_else(|| "The edited message is not valid.".to_owned()),
                        (Some(_), None) => Err("The edited message is not valid.".to_owned()),
                    };
                    match first {
                        Err(error) => Err(error),
                        Ok(first) => client
                            .complete_runtime_fork(
                                key.clone(),
                                record.clone(),
                                fork.source.clone(),
                                first,
                            )
                            .await
                            .map(|receipt| (record, receipt))
                            .map_err(|error| match error {
                                canopy_desktop::settings::Error::ResourceConflict => {
                                    "conditions_changed: The conversation changed meanwhile; \
                                     fork again."
                                        .to_owned()
                                }
                                other => other.to_string(),
                            }),
                    }
                }
            };
            match completed {
                Ok((record, receipt)) => {
                    let _ = this.update(cx, |this, cx| {
                        app.settings.update(cx, |settings, cx| {
                            settings.record_saved(record.clone());
                            cx.notify();
                        });
                        let _ = this.ensure_runtime_for_chat(record.id(), cx);
                    });
                    Ok(receipt)
                }
                Err(error) => {
                    // Nothing was published: a private folder made for it goes now; the
                    // copied history of the unlisted chat goes with the next cleanup.
                    if let Some(cwd) = created {
                        cx.background_executor()
                            .spawn(async move {
                                if let Ok(data) = chats_data() {
                                    let _ = canopy_desktop::chat::scratch::remove(&data, &cwd);
                                }
                            })
                            .await;
                    }
                    client
                        .transition_runtime_command(
                            key,
                            CommandState::Attempting,
                            CommandState::Failed,
                            reason(&error),
                        )
                        .await
                        .map_err(|e| e.to_string())
                }
            }
        })
    }

    /// The agent's tool and the project folder (`None`: a private folder to create).
    fn check_new_chat(
        &self,
        new: &NewChat,
        cx: &App,
    ) -> Result<(String, Option<std::path::PathBuf>, Option<Mode>), String> {
        let app = cx.global::<AppState>();
        if app.settings.read(cx).quitting || self.stopping {
            return Err("Canopy is quitting.".into());
        }
        if app.settings.read(cx).chat_record(&new.chat).is_some()
            || self.runtimes.contains_key(&new.chat)
        {
            return Err("chat_exists".into());
        }
        let tools = app.tools.read(cx);
        let tool = tools
            .catalog
            .get(&new.tool)
            .filter(|tool| tool.enabled)
            .ok_or("This agent is not configured on the host.")?;
        if runtime::backend_of(&new.tool, cx).is_none() {
            return Err("This tool can't run a chat.".into());
        }
        let profile_ok = tool.profiles.iter().any(|p| p.id == new.profile)
            || (tool.profiles.is_empty() && new.profile == tool.id);
        if !profile_ok {
            return Err("This account is not configured for the agent.".into());
        }
        if new
            .model
            .as_deref()
            .is_some_and(|m| !canopy_desktop::chat::config::valid_model(m))
        {
            return Err("The model is not valid.".into());
        }
        let folder = match new.workspace {
            None => None,
            Some(id) => Some(project_cwd(id, cx).ok_or("This project is not open on the host.")?),
        };
        let mode = tool
            .profiles
            .iter()
            .find(|p| p.id == new.profile)
            .and_then(|p| profile_mode(Provider::from_tool(&new.tool)?, p));
        Ok((tool.id.clone(), folder, mode))
    }

    /// What deleting `chat` would lose now (the remote confirmation's content).
    pub fn delete_preflight(&self, chat: &str, cx: &App) -> Result<DeletePreflight, String> {
        let app = cx.global::<AppState>();
        if app.settings.read(cx).chat_record(chat).is_none() {
            return Err("resource_deleted".into());
        }
        let pane = self
            .known
            .iter()
            .find(|(id, (pane, _))| {
                self.panes.get(id).map(String::as_str) == Some(chat)
                    || pane.metadata.chat.as_ref().and_then(|c| c.id.as_deref()) == Some(chat)
            })
            .map(|(id, _)| *id);
        let blocked = match pane {
            Some(pane) => self.deletion_blocker(&[pane], cx),
            None => self.detached.get(chat).and_then(|detached| {
                let cwd = self
                    .runtimes
                    .get(chat)
                    .and_then(|r| r.read(cx).cwd())
                    .or_else(|| {
                        app.settings
                            .read(cx)
                            .chat_record(chat)?
                            .metadata()
                            .cwd
                            .clone()
                    })?;
                (detached.workspace == app.projects.read(cx).scratch_id(cx))
                    .then(|| self.directory_blocker(&cwd, &[detached.id], cx))
                    .flatten()
            }),
        };
        let runtime = self.runtimes.get(chat).map(|r| r.read(cx));
        Ok(DeletePreflight {
            chat: chat.to_owned(),
            conversation: runtime.is_none_or(|r| r.has_conversation() || r.history_uncertain()),
            working: runtime.is_some_and(|r| r.is_working()),
            unsent: runtime.map_or(0, |r| r.unsent()),
            commands: runtime.map_or(0, |r| r.running_commands()),
            observers: runtime.map_or(0, |r| r.observers()),
            blocked,
        })
    }

    /// Deletes `chat` as the user confirmed in `expected`: refused if what it loses
    /// changed. Succeeds only after its cleanup finished (LIFE-04).
    pub fn delete_chat(
        &mut self,
        key: RemoteKey,
        expected: DeletePreflight,
        cx: &mut Context<Self>,
    ) -> Task<Result<CommandReceipt, String>> {
        let app = cx.global::<AppState>().clone();
        let Some(client) = app.settings.read(cx).client() else {
            return Task::ready(Err("Chat storage is unavailable.".into()));
        };
        let root = app
            .settings
            .read(cx)
            .chat_record(&expected.chat)
            .map(|r| r.authority_root().to_owned())
            .filter(|root| !root.is_empty());
        let Some(input) = root.and_then(|root| expected.command(key, root)) else {
            return Task::ready(Err("resource_deleted".into()));
        };
        let key = input.key().clone();
        cx.spawn(async move |this, cx| {
            let receipt = client
                .accept_runtime_command(input, CHAT)
                .await
                .map_err(|e| e.to_string())?;
            if receipt.state() != CommandState::Accepted {
                return Ok(receipt);
            }
            let current = this
                .read_with(cx, |this, cx| this.delete_preflight(&expected.chat, cx))
                .map_err(|e| e.to_string())?;
            let refusal = match &current {
                Err(error) => Some(error.clone()),
                Ok(current) if current.blocked.is_some() => current.blocked.clone(),
                Ok(current) if *current != expected => Some("conditions_changed".into()),
                Ok(_) => None,
            };
            if let Some(refusal) = refusal {
                return client
                    .transition_runtime_command(
                        key,
                        CommandState::Accepted,
                        CommandState::Rejected,
                        reason(&refusal),
                    )
                    .await
                    .map_err(|e| e.to_string());
            }
            if let Err(error) = client
                .transition_runtime_command(
                    key.clone(),
                    CommandState::Accepted,
                    CommandState::Attempting,
                    None,
                )
                .await
            {
                return match client.get_runtime_command(key, CHAT).await {
                    Ok(Some(receipt)) => Ok(receipt),
                    _ => Err(error.to_string()),
                };
            }
            let cleanup = this
                .update(cx, |this, cx| this.delete_now(&expected.chat, cx))
                .map_err(|e| e.to_string())?;
            let (state, result) = match cleanup {
                Err(error) => (CommandState::Failed, reason(&error)),
                Ok(cleanup) => match cleanup.await {
                    Ok(()) => (CommandState::Succeeded, None),
                    // Kept with Retry cleanup on the host; what remains is not undone.
                    Err(error) => (CommandState::Failed, reason(&error)),
                },
            };
            client
                .transition_runtime_command(key, CommandState::Attempting, state, result)
                .await
                .map_err(|e| e.to_string())
        })
    }

    /// Closes the chat's pane (its tab's Delete) or deletes it without one; the
    /// returned task ends with its cleanup.
    fn delete_now(
        &mut self,
        chat: &str,
        cx: &mut Context<Self>,
    ) -> Result<Task<Result<(), String>>, String> {
        let pane = self
            .known
            .iter()
            .find(|(id, (pane, _))| {
                self.panes.get(id).map(String::as_str) == Some(chat)
                    || pane.metadata.chat.as_ref().and_then(|c| c.id.as_deref()) == Some(chat)
            })
            .map(|(id, (_, workspace))| (*id, *workspace));
        let pane_id = match pane {
            Some((pane, workspace)) => {
                let projects = cx.global::<AppState>().projects.clone();
                projects
                    .update(cx, |projects, cx| {
                        projects.close_chat_pane(workspace, pane, cx)
                    })
                    .map_err(|e| e.to_string())?;
                self.sync_model(cx);
                pane
            }
            None => {
                let id = self
                    .detached
                    .get(chat)
                    .map(|d| d.id)
                    .ok_or("resource_deleted")?;
                self.delete_detached(chat, cx);
                if self.detached.contains_key(chat) {
                    return Err(self
                        .error
                        .clone()
                        .unwrap_or_else(|| "The chat is in use.".into()));
                }
                id
            }
        };
        let retiring = self
            .retiring
            .iter()
            .find(|r| r.pane.id == pane_id)
            .ok_or("The chat's cleanup didn't start.")?;
        let (completion, result) = (retiring.completion.clone(), retiring.result.clone());
        Ok(cx.spawn(async move |_, _| {
            let _ = completion.recv().await;
            result
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .unwrap_or_else(|| Err("Chat cleanup result was lost.".into()))
        }))
    }
}

impl Chats {
    /// Gives a conversation the host kept without its launch context (history adopted
    /// without a pane) the agent, account and folder the user chose. Its ID, history
    /// and saved choices stay; nothing is picked by default. Afterwards it is listed
    /// like any chat without a tab.
    pub fn recover(
        &mut self,
        chat: String,
        tool: String,
        profile: String,
        workspace: Option<WorkspaceId>,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let app = cx.global::<AppState>().clone();
        let settings = app.settings.read(cx);
        let Some(client) = settings.client() else {
            return Task::ready(Err("Chat storage is unavailable.".into()));
        };
        let Some(record) = settings.chat_record(&chat).cloned() else {
            return Task::ready(Err(
                "This conversation is no longer waiting to be restored.".into(),
            ));
        };
        let Some(reason) = record.recovery_reason().map(str::to_owned) else {
            return Task::ready(Err(
                "This conversation is no longer waiting to be restored.".into(),
            ));
        };
        let folder = match workspace {
            Some(id) => match project_cwd(id, cx) {
                Some(cwd) => Some(cwd),
                None => return Task::ready(Err("This project is no longer open.".into())),
            },
            None => None,
        };
        cx.spawn(async move |_, cx| {
            let authority = authority_of(workspace, cx)
                .await
                .ok_or("This project is no longer open.")?;
            let created = folder.is_none();
            let cwd = match folder {
                Some(cwd) => cwd,
                None => cx
                    .background_executor()
                    .spawn(async {
                        let data = canopy_desktop::platform::directories::data_dir()?;
                        canopy_desktop::chat::scratch::create(&data)
                    })
                    .await
                    .map_err(|e| format!("Could not create the chat folder: {e}"))?,
            };
            let mut metadata = record.metadata().clone();
            metadata.kind = PaneKind::Chat;
            metadata.cwd = Some(cwd.clone());
            metadata.profile_id = Some(profile);
            let root = workspace.and_then(|id| serde_json::to_string(&id).ok());
            let recovered = ChatRecord::new(chat.clone(), root, tool, metadata)
                .and_then(|r| r.with_history_epoch(record.history_epoch()))
                .and_then(|r| match workspace {
                    Some(_) => r.with_authority_root(authority),
                    None => Some(r),
                });
            let key = CommandKey::local(uuid::Uuid::new_v4().to_string()).expect("UUID");
            let result = match recovered
                .as_ref()
                .and_then(|r| Some((r, r.recovery_command(key.clone(), &reason)?)))
            {
                None => Err("This agent, account or folder can't be used.".to_owned()),
                Some((record, input)) => client
                    .recover_runtime_chat(input, record.clone(), reason)
                    .await
                    .map_err(|error| match error {
                        canopy_desktop::settings::Error::ResourceConflict => {
                            "The conversation changed meanwhile. Close this and try again."
                                .to_owned()
                        }
                        canopy_desktop::settings::Error::InvalidRuntimeState => {
                            "This agent or account is not configured on this computer.".to_owned()
                        }
                        other => other.to_string(),
                    })
                    .map(|_| ()),
            };
            match (result, recovered) {
                (Ok(()), Some(record)) => {
                    let _ = client.acknowledge_runtime_command(key).await;
                    cx.update(|cx| {
                        app.settings.update(cx, |settings, cx| {
                            settings.record_saved(record);
                            cx.notify();
                        })
                    });
                    Ok(())
                }
                (result, _) => {
                    // A folder made for this attempt holds nothing yet.
                    if created {
                        cx.background_executor()
                            .spawn(async move {
                                if let Ok(data) = chats_data() {
                                    let _ = canopy_desktop::chat::scratch::remove(&data, &cwd);
                                }
                            })
                            .await;
                    }
                    result.and(Err("This agent, account or folder can't be used.".into()))
                }
            }
        })
    }
}

/// Choices of a new remote chat: the profile's mode and model unless asked otherwise.
fn new_settings(new: &NewChat, tool: &str, mode: Option<Mode>) -> ChatSettings {
    let mode = mode.unwrap_or(match Provider::from_tool(tool) {
        Some(Provider::Codex) => Mode::Auto,
        _ => Mode::Ask,
    });
    ChatSettings {
        model: new.model.clone(),
        effort: None,
        mode,
        allowed: vec![],
        id: Some(new.chat.clone()),
        agent_options: Default::default(),
    }
}

mod fork_files;
#[cfg(test)]
mod tests;
