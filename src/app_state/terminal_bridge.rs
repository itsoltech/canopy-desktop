//! The remote TerminalService's handle to the host's terminals. Operations cross a
//! bounded channel to the app thread and call the same typed handlers as the
//! desktop (start, shutdown with confirmed cleanup, restart). Live projection,
//! input and leases never cross it: the RPC reads the run (`terminal::control::Run`)
//! directly, the object the host view itself writes through.
use super::AppState;
use crate::ui::terminal::TerminalView;
use canopy_desktop::{
    remote::terminal_rpc::{OpenTerminal, TerminalHost, TerminalInfo},
    state::workspace::WorkspaceId,
    terminal::control::Holder,
};
use futures_channel::oneshot;
use gpui_kit::*;
use std::{path::PathBuf, time::Duration};
use tonic::Status;

/// Operations waiting for the app thread; beyond this the bridge answers busy.
const PENDING: usize = 64;
/// Longest wait for a started process to report running or failed.
const START_WAIT: Duration = Duration::from_secs(30);

type Reply<T> = oneshot::Sender<Result<T, Status>>;

enum Op {
    List(Reply<Vec<TerminalInfo>>),
    Find(String, Reply<Option<TerminalInfo>>),
    Open(OpenTerminal, Reply<TerminalInfo>),
    Stop(String, String, Reply<()>),
    Restart(String, String, Holder, Reply<String>),
    Close(String, Reply<()>),
}

/// Clone freely, any thread.
#[derive(Clone)]
pub struct TerminalBridge(async_channel::Sender<Op>);

/// A terminal's catalog workspace ID, the same JSON number Open Project returns.
pub fn workspace_ref(id: WorkspaceId) -> String {
    serde_json::to_string(&id).expect("workspace ID serializes")
}

/// Starts the app-side owner of the bridge.
pub fn install(cx: &mut App) -> TerminalBridge {
    let (sender, ops) = async_channel::bounded(PENDING);
    cx.spawn(async move |cx| {
        while let Ok(op) = ops.recv().await {
            cx.update(|cx| serve(op, cx));
        }
    })
    .detach();
    TerminalBridge(sender)
}

/// What the app thread knows; the canonical root is resolved on a worker.
struct Row {
    workspace: WorkspaceId,
    // Keep the view alive while its canonical root is resolved off the app thread.
    _view: Entity<TerminalView>,
    id: String,
    cwd: Option<PathBuf>,
    title: String,
}
fn row(workspace: WorkspaceId, view: Entity<TerminalView>, cx: &App) -> Row {
    let terminal = view.read(cx);
    let pane = terminal.pane();
    Row {
        workspace,
        id: terminal.terminal_id().to_owned(),
        cwd: pane.metadata.cwd.clone(),
        title: pane
            .metadata
            .title
            .clone()
            .unwrap_or_else(|| pane.tool.clone()),
        _view: view.clone(),
    }
}
fn resolve(
    rows: Vec<(
        Row,
        Option<std::sync::Arc<canopy_desktop::terminal::control::Run>>,
    )>,
) -> Vec<TerminalInfo> {
    rows.into_iter()
        .filter_map(|(row, run)| {
            // A terminal whose folder is gone cannot be granted to a device.
            let root = std::fs::canonicalize(row.cwd?).ok()?;
            Some(
                TerminalInfo::new(row.id, root, row.title, run)
                    .with_workspace(workspace_ref(row.workspace)),
            )
        })
        .collect()
}
fn rows(
    cx: &App,
    only: Option<&str>,
) -> Vec<(
    Row,
    Option<std::sync::Arc<canopy_desktop::terminal::control::Run>>,
)> {
    let app = cx.global::<AppState>();
    let projects = &app.projects.read(cx).catalog;
    app.terminals
        .read(cx)
        .all()
        .into_iter()
        .filter(|(workspace, _)| projects.items.iter().any(|p| p.workspace == *workspace))
        .filter(|(_, view)| only.is_none_or(|id| view.read(cx).terminal_id() == id))
        .map(|(workspace, view)| {
            let run = view.read(cx).run();
            (row(workspace, view, cx), run)
        })
        .collect()
}
fn not_found() -> Status {
    Status::not_found("The terminal is no longer available.")
}
fn find(id: &str, cx: &App) -> Result<Entity<TerminalView>, Status> {
    let app = cx.global::<AppState>();
    let projects = &app.projects.read(cx).catalog;
    app.terminals
        .read(cx)
        .all()
        .into_iter()
        .find(|(workspace, view)| {
            projects.items.iter().any(|p| p.workspace == *workspace)
                && view.read(cx).terminal_id() == id
        })
        .map(|(_, view)| view)
        .ok_or_else(not_found)
}
/// The run a command names must still be the current one.
fn current(view: &Entity<TerminalView>, run: &str, cx: &App) -> Result<(), Status> {
    if view
        .read(cx)
        .run()
        .is_some_and(|current| current.id() == run)
    {
        Ok(())
    } else {
        Err(Status::aborted(
            "The terminal run changed. Refresh the terminal.",
        ))
    }
}

/// Confirmed Stop: the same shutdown the workspace Stop uses, then the view shows
/// the stopped (or failed cleanup) state.
fn stop(view: Entity<TerminalView>, cx: &mut App) -> Task<Result<(), Status>> {
    let cleanup = view.update(cx, |view, cx| view.shutdown(cx));
    cx.spawn(async move |cx| {
        let result = cleanup.await;
        view.update(cx, |view, cx| match &result {
            Ok(()) => view.mark_stopped(cx),
            Err(error) => view.mark_stop_failed(error.clone(), cx),
        });
        result.map_err(Status::failed_precondition)
    })
}
/// Waits until the started process runs or reports why it could not.
async fn started(view: Entity<TerminalView>, cx: &mut AsyncApp) -> Result<String, Status> {
    let deadline = std::time::Instant::now() + START_WAIT;
    loop {
        let state = view.read_with(cx, |view, _| {
            (
                view.is_starting(),
                view.start_error().map(str::to_owned),
                view.run().map(|run| run.id().to_owned()),
            )
        });
        match state {
            (false, Some(error), _) => return Err(Status::failed_precondition(error)),
            (false, None, Some(run)) => return Ok(run),
            (false, None, None) => {
                return Err(Status::failed_precondition(
                    "The host did not start the terminal.",
                ));
            }
            (true, ..) if std::time::Instant::now() >= deadline => {
                return Err(Status::unknown(
                    "The terminal is still starting; check the terminal list.",
                ));
            }
            (true, ..) => {
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await
            }
        }
    }
}

fn serve(op: Op, cx: &mut App) {
    match op {
        Op::List(reply) => {
            let rows = rows(cx, None);
            cx.background_spawn(async move {
                let _ = reply.send(Ok(resolve(rows)));
            })
            .detach();
        }
        Op::Find(id, reply) => {
            let rows = rows(cx, Some(&id));
            cx.background_spawn(async move {
                let _ = reply.send(Ok(resolve(rows).into_iter().next()));
            })
            .detach();
        }
        Op::Open(open, reply) => {
            let projects: Vec<_> = super::project_commands::catalog(cx);
            cx.spawn(async move |cx| {
                let result = open_terminal(open, projects, cx).await;
                let _ = reply.send(result);
            })
            .detach();
        }
        Op::Stop(id, run, reply) => {
            let task = find(&id, cx).and_then(|view| {
                current(&view, &run, cx)?;
                Ok(stop(view, cx))
            });
            cx.spawn(async move |_| {
                let _ = reply.send(match task {
                    Ok(task) => task.await,
                    Err(error) => Err(error),
                });
            })
            .detach();
        }
        Op::Restart(id, run, holder, reply) => {
            let view = find(&id, cx).and_then(|view| {
                current(&view, &run, cx)?;
                Ok(view)
            });
            cx.spawn(async move |cx| {
                let result = async {
                    let view = view?;
                    cx.update(|cx| stop(view.clone(), cx)).await?;
                    view.update(cx, |view, cx| view.start_for(holder, cx));
                    started(view, cx).await
                }
                .await;
                let _ = reply.send(result);
            })
            .detach();
        }
        Op::Close(id, reply) => {
            let terminals = cx.global::<AppState>().terminals.clone();
            let task = if terminals.read(cx).is_detached(&id) {
                Ok(terminals.update(cx, |terminals, cx| terminals.close_detached(&id, cx)))
            } else if terminals.read(cx).find(&id, cx).is_some() {
                Err(Status::failed_precondition(
                    "Close this terminal's pane on the host.",
                ))
            } else {
                Err(not_found())
            };
            cx.spawn(async move |_| {
                let _ = reply.send(match task {
                    Ok(task) => task.await.map_err(Status::failed_precondition),
                    Err(error) => Err(error),
                });
            })
            .detach();
        }
    }
}

/// The workspace must be the open project whose canonical checkout is `root`.
async fn open_terminal(
    open: OpenTerminal,
    projects: Vec<super::project_commands::ProjectListing>,
    cx: &mut AsyncApp,
) -> Result<TerminalInfo, Status> {
    let workspace: WorkspaceId = serde_json::from_str(&open.workspace)
        .map_err(|_| Status::not_found("Workspace is no longer available."))?;
    let root = open.root.clone();
    let matches = cx
        .background_spawn(async move {
            super::project_commands::resolve(projects)
                .into_iter()
                .any(|summary| summary.workspace == workspace && summary.canonical_root == root)
        })
        .await;
    if !matches {
        return Err(Status::not_found("Workspace is no longer available."));
    }
    let (id, root, title) = (open.terminal.clone(), open.root.clone(), open.tool.clone());
    let view = cx.update(|cx| {
        let terminals = cx.global::<AppState>().terminals.clone();
        terminals.update(cx, |terminals, cx| {
            terminals.open_detached(
                open.terminal,
                workspace,
                open.root,
                open.tool,
                open.profile,
                open.holder,
                cx,
            )
        })
    });
    let view = view.map_err(Status::failed_precondition)?;
    started(view.clone(), cx).await?;
    let run = view.read_with(cx, |view, _| view.run());
    Ok(TerminalInfo::new(id, root, title, run).with_workspace(workspace_ref(workspace)))
}

impl TerminalBridge {
    async fn ask<T>(&self, op: impl FnOnce(Reply<T>) -> Op) -> Result<T, Status> {
        let (reply, answer) = oneshot::channel();
        self.0
            .try_send(op(reply))
            .map_err(|_| Status::resource_exhausted("The host is busy; retry."))?;
        answer
            .await
            .map_err(|_| Status::unavailable("Canopy is closing."))?
    }
    /// Stops and forgets a terminal a device opened. Pane terminals close on the host.
    pub async fn close(&self, terminal: String) -> Result<(), Status> {
        self.ask(|reply| Op::Close(terminal, reply)).await
    }
}

#[tonic::async_trait]
impl TerminalHost for TerminalBridge {
    async fn terminals(&self) -> Result<Vec<TerminalInfo>, Status> {
        self.ask(Op::List).await
    }
    async fn terminal(&self, id: &str) -> Result<Option<TerminalInfo>, Status> {
        let id = id.to_owned();
        self.ask(|reply| Op::Find(id, reply)).await
    }
    async fn open(&self, request: OpenTerminal) -> Result<TerminalInfo, Status> {
        self.ask(|reply| Op::Open(request, reply)).await
    }
    async fn stop(&self, terminal: &str, run: &str) -> Result<(), Status> {
        let (terminal, run) = (terminal.to_owned(), run.to_owned());
        self.ask(|reply| Op::Stop(terminal, run, reply)).await
    }
    async fn restart(&self, terminal: &str, run: &str, holder: Holder) -> Result<String, Status> {
        let (terminal, run) = (terminal.to_owned(), run.to_owned());
        self.ask(|reply| Op::Restart(terminal, run, holder, reply))
            .await
    }
}
