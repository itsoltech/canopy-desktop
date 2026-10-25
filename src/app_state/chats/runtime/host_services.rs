//! What the host does for an ACP agent (AGT-10): read and write text files and run
//! terminals, within the chat's folder and through the host's own services. A file
//! open in Canopy's editor is read from its buffer; one with unsaved edits is never
//! written. Terminals belong to the agent's run: people may watch, never type.
use super::{ChatRuntime, ChatStateChanged};
use crate::app_state::AppState;
use canopy_desktop::{
    chat::{
        acp::{HostOperation, HostRequest, wire::RequestId},
        backend_session::BackendSession,
        shell::{Running, ShellEvent},
    },
    files::{self, Document},
};
use gpui_kit::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

/// Terminals run through the host's command runner: a process group on macOS and
/// Linux, a Job Object on Windows.
pub const TERMINALS: bool = cfg!(any(unix, windows));
/// Output kept per terminal when the agent sets no lower limit.
const OUTPUT_LIMIT: usize = 1024 * 1024;
/// Terminals one run may hold at once.
const MAX_TERMINALS: usize = 16;

struct Terminal {
    running: Running,
    output: String,
    truncated: bool,
    limit: usize,
    /// Set once it ended: exit code (`None` when a signal ended it).
    exit: Option<Option<i32>>,
    waiters: Vec<RequestId>,
    _pump: Task<()>,
}

/// The terminals an ACP agent created in the current run.
#[derive(Default)]
pub struct HostTerminals(HashMap<String, Terminal>);
impl HostTerminals {
    /// A terminal's command still runs.
    pub fn any_running(&self) -> bool {
        self.0.values().any(|terminal| terminal.exit.is_none())
    }
    /// What terminal `id` printed so far (its newest part), and whether it still runs.
    pub fn output(&self, id: &str) -> Option<(&str, bool)> {
        let terminal = self.0.get(id)?;
        Some((terminal.output.as_str(), terminal.exit.is_none()))
    }
    /// The run ended: every terminal stops; their handles go to the shared cleanup.
    pub fn release_all(&mut self) -> Vec<Running> {
        self.0
            .drain()
            .map(|(_, terminal)| {
                terminal.running.stop();
                terminal.running
            })
            .collect()
    }
}

impl ChatRuntime {
    /// Answers what the ACP agent asked of the host since the last call.
    pub(super) fn serve_host(&mut self, cx: &mut Context<Self>) {
        let Some(BackendSession::Acp(session)) = &mut self.session else {
            return;
        };
        for request in session.take_host_requests() {
            self.serve(request, cx);
        }
    }

    fn serve(&mut self, request: HostRequest, cx: &mut Context<Self>) {
        let id = request.id;
        match request.operation {
            HostOperation::ReadTextFile { path, line, limit } => {
                let (root, relative) = match self.scoped(&path) {
                    Ok(scope) => scope,
                    Err(error) => return self.reply(&id, Err(error)),
                };
                if let Some((text, _)) = editor_buffer(&root.join(&relative), cx) {
                    return self.reply(&id, Ok(json!({ "content": slice(&text, line, limit) })));
                }
                self.background(id, cx, move || {
                    let document = Document::load(&root, &relative)?;
                    Ok(json!({ "content": slice(&document.text, line, limit) }))
                });
            }
            HostOperation::WriteTextFile { path, content } => {
                let (root, relative) = match self.scoped(&path) {
                    Ok(scope) => scope,
                    Err(error) => return self.reply(&id, Err(error)),
                };
                self.write_checked(id, root, relative, content, cx);
            }
            HostOperation::CreateTerminal {
                command,
                args,
                env,
                cwd,
                output_byte_limit,
            } => {
                let created = self.create_terminal(command, args, env, cwd, output_byte_limit, cx);
                self.reply(
                    &id,
                    created.map(|terminal| json!({ "terminalId": terminal })),
                );
            }
            HostOperation::TerminalOutput { terminal_id } => {
                let output = self.terminals.0.get(&terminal_id).map(|terminal| {
                    json!({
                        "output": terminal.output,
                        "truncated": terminal.truncated,
                        "exitStatus": terminal.exit.map(exit_status),
                    })
                });
                self.reply(&id, output.ok_or_else(unknown_terminal));
            }
            HostOperation::WaitForExit { terminal_id } => {
                match self.terminals.0.get_mut(&terminal_id) {
                    None => self.reply(&id, Err(unknown_terminal())),
                    Some(Terminal {
                        exit: Some(exit), ..
                    }) => {
                        let exit = *exit;
                        self.reply(&id, Ok(exit_status(exit)))
                    }
                    Some(terminal) => terminal.waiters.push(id),
                }
            }
            HostOperation::KillTerminal { terminal_id } => {
                let killed = self.terminals.0.get(&terminal_id).map(|terminal| {
                    terminal.running.stop();
                    json!({})
                });
                self.reply(&id, killed.ok_or_else(unknown_terminal));
            }
            HostOperation::ReleaseTerminal { terminal_id } => {
                let Some(terminal) = self.terminals.0.remove(&terminal_id) else {
                    return self.reply(&id, Err(unknown_terminal()));
                };
                terminal.running.stop();
                // The common cleanup confirms the process group is gone (quit, delete).
                self.shell_resources.push(terminal.running);
                for waiter in terminal.waiters {
                    self.reply(&waiter, Err("The terminal was released.".into()));
                }
                self.reply(&id, Ok(json!({})));
            }
        }
    }

    /// An agent's write through the host's Save rules (APP-02/04): never over unsaved
    /// edits in Canopy's editor or another device's dirty buffer. The file is reserved
    /// against other Canopy writes first, then both are checked again right before the
    /// write. Other programs aren't excluded: no filesystem-wide compare-and-swap.
    fn write_checked(
        &mut self,
        id: RequestId,
        root: PathBuf,
        relative: PathBuf,
        content: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = local_dirty(&root, &relative, cx) {
            return self.reply(&id, Err(error));
        }
        let gate = crate::app_state::file_ops::gate(cx);
        let client = cx.global::<AppState>().settings.read(cx).client();
        let generation = self.generation;
        self.host_busy += 1;
        cx.spawn(async move |this, cx| {
            let (check_root, check_relative) = (root.clone(), relative.clone());
            let identity = cx
                .background_spawn(async move { file_identity(&check_root, &check_relative) })
                .await;
            // Reserved first, then every condition is read again before the write.
            let reserved = identity.and_then(|file| {
                let reservation = gate
                    .try_reserve_file(Path::new(file.root()), Path::new(file.relative_path()))
                    .ok_or_else(|| {
                        format!(
                            "{} is being written by Canopy; it was not written.",
                            relative.display()
                        )
                    })?;
                Ok((file, reservation))
            });
            let (remote, reservation) = match (reserved, client) {
                (Ok((file, reservation)), Some(client)) => {
                    let remote = match client.runtime_file_presence(file).await {
                        Ok(presence) => other_dirty(&presence, &relative),
                        Err(error) => Some(format!(
                            "Unsaved edits on other devices couldn't be checked ({error}); {} was not written.",
                            relative.display()
                        )),
                    };
                    (remote, Some(reservation))
                }
                // No host database (tests, startup): no other device can be dirty.
                (Ok((_, reservation)), None) => (None, Some(reservation)),
                (Err(error), _) => (Some(error), None),
            };
            let ready = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return None;
                }
                let refused = remote.or_else(|| local_dirty(&root, &relative, cx));
                if let Some(error) = refused {
                    this.host_busy = this.host_busy.saturating_sub(1);
                    this.reply(&id, Err(error));
                    this.after_host(cx);
                    return None;
                }
                Some((root, relative, content))
            });
            let Ok(Some((root, relative, content))) = ready else {
                return;
            };
            let written = cx
                .background_spawn(async move {
                    let written = write_text(&root, &relative, &content);
                    drop(reservation);
                    written
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.host_busy = this.host_busy.saturating_sub(1);
                this.reply(&id, written.map(|()| json!({})));
                this.after_host(cx);
            });
        })
        .detach();
    }

    /// `path` inside the chat's folder: that folder and the path relative to it.
    fn scoped(&self, path: &str) -> Result<(PathBuf, PathBuf), String> {
        let root = self
            .pane
            .metadata
            .cwd
            .clone()
            .ok_or("This chat has no folder.")?;
        let path = Path::new(path);
        if !path.is_absolute() {
            return Err("The path must be absolute.".into());
        }
        let relative = path
            .strip_prefix(&root)
            .ok()
            .map(Path::to_path_buf)
            .or_else(|| {
                let canonical = root.canonicalize().ok()?;
                path.strip_prefix(canonical).ok().map(Path::to_path_buf)
            })
            .ok_or("The path is outside this chat's folder.")?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err("The path is outside this chat's folder.".into());
        }
        Ok((root, relative))
    }

    /// Runs `work` off the UI thread; its result answers request `id` of this run only.
    fn background(
        &mut self,
        id: RequestId,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> Result<Value, String> + Send + 'static,
    ) {
        let generation = self.generation;
        self.host_busy += 1;
        let done = cx.background_spawn(async move { work() });
        cx.spawn(async move |this, cx| {
            let result = done.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.host_busy = this.host_busy.saturating_sub(1);
                this.reply(&id, result);
                this.after_host(cx);
            });
        })
        .detach();
    }

    fn reply(&mut self, id: &RequestId, result: Result<Value, String>) {
        let Some(BackendSession::Acp(session)) = &mut self.session else {
            return;
        };
        if let Ok(line) = session.complete_host_request(id, result) {
            self.write(vec![line]);
        }
    }

    fn after_host(&mut self, cx: &mut Context<Self>) {
        self.schedule_sleep(cx);
        cx.emit(ChatStateChanged);
        cx.notify();
    }

    fn create_terminal(
        &mut self,
        command: String,
        args: Vec<String>,
        env: Vec<canopy_desktop::chat::acp::EnvironmentVariable>,
        cwd: Option<String>,
        limit: Option<u64>,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        if !TERMINALS {
            return Err("Terminals for agents aren't supported on this platform.".into());
        }
        if self.terminals.0.len() >= MAX_TERMINALS {
            return Err("Too many terminals; release one first.".into());
        }
        if command.trim().is_empty() {
            return Err("The command is empty.".into());
        }
        let folder = self
            .pane
            .metadata
            .cwd
            .clone()
            .ok_or("This chat has no folder.")?;
        let cwd = match cwd {
            Some(cwd) if Path::new(&cwd) == folder => folder,
            Some(cwd) => {
                let (root, relative) = self.scoped(&cwd)?;
                root.join(relative)
            }
            None => folder,
        };
        let mut environment = cx
            .global::<AppState>()
            .tools
            .read(cx)
            .environment
            .clone()
            .ok_or("The shell environment is still loading.")??
            .as_ref()
            .clone();
        environment.extend_overrides(env.into_iter().map(|var| (var.name, var.value)));
        // Windows runs the program itself (argv isn't shell syntax there).
        #[cfg(windows)]
        let (running, events) = {
            let program = environment.resolve(&command)?;
            let (program, args) = environment.prepare_launch(program, args)?;
            Running::start_program(&program, &args, &cwd, &environment.vars)?
        };
        #[cfg(not(windows))]
        let (running, events) = {
            let line = shell_words::join(
                std::iter::once(command.as_str()).chain(args.iter().map(String::as_str)),
            );
            Running::start(&line, &cwd, &environment.shell, &environment.vars)?
        };
        let id = format!("canopy-terminal-{}", uuid::Uuid::new_v4());
        let generation = self.generation;
        let terminal = id.clone();
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let ended = matches!(event, ShellEvent::Exit { .. });
                let alive = this
                    .update(cx, |this, cx| {
                        if this.generation == generation {
                            this.on_terminal(&terminal, event, cx);
                        }
                    })
                    .is_ok();
                if ended || !alive {
                    break;
                }
            }
        });
        self.terminals.0.insert(
            id.clone(),
            Terminal {
                running,
                output: String::new(),
                truncated: false,
                limit: limit.map_or(OUTPUT_LIMIT, |limit| {
                    limit.min(OUTPUT_LIMIT as u64) as usize
                }),
                exit: None,
                waiters: vec![],
                _pump: pump,
            },
        );
        Ok(id)
    }

    fn on_terminal(&mut self, id: &str, event: ShellEvent, cx: &mut Context<Self>) {
        let Some(terminal) = self.terminals.0.get_mut(id) else {
            return;
        };
        match event {
            ShellEvent::Output(chunk) => {
                terminal.output.push_str(&chunk);
                if terminal.output.len() > terminal.limit {
                    // The newest output stays; the cut lands on a character boundary.
                    let mut cut = terminal.output.len() - terminal.limit;
                    while !terminal.output.is_char_boundary(cut) {
                        cut += 1;
                    }
                    terminal.output.drain(..cut);
                    terminal.truncated = true;
                }
                // Its tool card shows the output as it comes.
                cx.notify();
            }
            ShellEvent::Exit { code, .. } => {
                terminal.exit = Some(code);
                let waiters = std::mem::take(&mut terminal.waiters);
                for waiter in waiters {
                    self.reply(&waiter, Ok(exit_status(code)));
                }
                self.after_host(cx);
            }
        }
    }
}

/// The text of the editor showing `path`, and whether it has unsaved edits.
fn editor_buffer(path: &Path, cx: &App) -> Option<(String, bool)> {
    let editors = cx.global::<AppState>().editors.read(cx);
    editors.views.values().find_map(|view| {
        let view = view.read(cx);
        let open = view.root.join(&view.path);
        (open == path).then(|| (view.buffer(cx), view.dirty || view.saving))
    })
}

/// Unsaved edits of the file in Canopy's editor on this host.
fn local_dirty(root: &Path, relative: &Path, cx: &App) -> Option<String> {
    editor_buffer(&root.join(relative), cx)
        .is_some_and(|(_, dirty)| dirty)
        .then(|| {
            format!(
                "{} has unsaved edits in Canopy's editor; it was not written.",
                relative.display()
            )
        })
}

/// The file's host identity: canonical root and normalized relative path.
fn file_identity(
    root: &Path,
    relative: &Path,
) -> Result<canopy_desktop::host::FileIdentity, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("The chat's folder is unavailable: {error}"))?;
    let relative = relative
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join("/"));
    root.to_str()
        .zip(relative)
        .and_then(|(root, relative)| {
            canopy_desktop::host::FileIdentity::new(root.to_owned(), relative)
        })
        .ok_or_else(|| "This path can't be written by the agent.".to_owned())
}

/// Another device's unsaved buffer of the file: who and when it was last seen.
fn other_dirty(
    presence: &[canopy_desktop::host::DirtyPresence],
    relative: &Path,
) -> Option<String> {
    let dirty = presence
        .iter()
        .find(|p| p.device_id() != canopy_desktop::host::LOCAL_PRINCIPAL)?;
    Some(format!(
        "{} has unsaved edits on {} (last seen {}); it was not written.",
        relative.display(),
        dirty.device_name(),
        last_seen(dirty.last_seen()),
    ))
}

fn last_seen(at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    match now.saturating_sub(at) {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86_400),
    }
}

/// Lines `line` (1-based) onwards, at most `limit` of them.
fn slice(text: &str, line: Option<u32>, limit: Option<u32>) -> String {
    if line.is_none() && limit.is_none() {
        return text.to_owned();
    }
    let skip = line.map_or(0, |line| line.saturating_sub(1) as usize);
    let take = limit.map_or(usize::MAX, |limit| limit as usize);
    text.split_inclusive('\n').skip(skip).take(take).collect()
}

/// Writes `content` keeping the file's BOM, line endings and permissions; a new file is
/// created. A change on disk since it was read refuses the write (APP-02).
fn write_text(root: &Path, relative: &Path, content: &str) -> Result<(), String> {
    let content = content.replace("\r\n", "\n");
    let document = match Document::load(root, relative) {
        Ok(document) => document,
        Err(error) => {
            if files::resolve(root, relative)?.exists() {
                return Err(error);
            }
            files::create(root, relative)?;
            Document::load(root, relative)?
        }
    };
    document.save(root, relative, &content)?;
    Ok(())
}

fn exit_status(code: Option<i32>) -> Value {
    json!({ "exitCode": code, "signal": Value::Null })
}

fn unknown_terminal() -> String {
    "Unknown terminal.".into()
}

#[cfg(test)]
mod tests {
    use super::{slice, write_text};
    use std::path::Path;

    #[test]
    fn writes_keep_line_endings_and_refuse_to_leave_the_folder() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        std::fs::write(root.join("a.txt"), "one\r\ntwo\r\n").unwrap();
        write_text(root, Path::new("a.txt"), "uno\ndos\n").unwrap();
        assert_eq!(
            std::fs::read(root.join("a.txt")).unwrap(),
            b"uno\r\ndos\r\n"
        );
        write_text(root, Path::new("new.txt"), "fresh").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).unwrap(),
            "fresh"
        );
        assert!(write_text(root, Path::new("../escape.txt"), "x").is_err());
    }

    #[test]
    fn a_line_range_counts_from_one() {
        assert_eq!(slice("a\nb\nc\n", Some(2), Some(1)), "b\n");
        assert_eq!(slice("a\nb", None, None), "a\nb");
    }
}
