use super::AppState;
use canopy_desktop::{
    git::{
        GitClient,
        changes::{ChangesSnapshot, Edit, FileChange},
    },
    state::workspace::{PaneKind, PaneMetadata},
};
use gpui_kit::*;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Clone)]
pub struct UpstreamRequest {
    pub plan: canopy_desktop::git::network::Plan,
    pub operation: canopy_desktop::git::network::Operation,
}
impl EventEmitter<UpstreamRequest> for ChangesState {}
pub struct ChangesState {
    pub path: Option<PathBuf>,
    pub data: Option<Arc<ChangesSnapshot>>,
    pub error: Option<String>,
    pub last_commit: Option<(PathBuf, String)>,
    pub busy: bool,
    pub committing: bool,
    pub commit_phase: Option<String>,
    pub commit_warning: Option<(PathBuf, String)>,
    pub commit_output: Option<(PathBuf, String)>,
    commit_progress: Option<Task<()>>,
    pub networking: bool,
    pub network_operation: Option<(PathBuf, canopy_desktop::git::network::Operation)>,
    pub network_revision: u64,
    pub client: Option<GitClient>,
    watching: Option<PathBuf>,
    observers: Vec<Subscription>,
    task: Option<Task<()>>,
    cancel: Arc<AtomicBool>,
    quitting: bool,
}
impl ChangesState {
    pub fn new() -> Self {
        Self {
            path: None,
            data: None,
            error: None,
            last_commit: None,
            busy: false,
            committing: false,
            commit_phase: None,
            commit_warning: None,
            commit_output: None,
            commit_progress: None,
            networking: false,
            network_operation: None,
            network_revision: 0,
            client: None,
            watching: None,
            observers: vec![],
            task: None,
            cancel: Arc::new(AtomicBool::new(false)),
            quitting: false,
        }
    }
    pub fn bind(&mut self, app: &AppState, cx: &mut Context<Self>) {
        self.client = app.git.read(cx).client();
        self.observers = vec![
            cx.observe(&app.projects, |this, _, cx| this.sync(cx)),
            cx.observe(&app.workspace, |this, _, cx| this.sync(cx)),
            cx.observe(&app.layout, |this, _, cx| this.sync(cx)),
            cx.observe(&app.git, |this, _, cx| this.sync(cx)),
        ];
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        let app = cx.global::<AppState>();
        let path = app
            .projects
            .read(cx)
            .catalog
            .current()
            .map(|p| p.path.clone());
        let changed = self.path != path;
        if changed {
            self.path = path.clone();
            self.data = None;
            self.error = None;
        }
        let layout = app.layout.read(cx);
        let visible = layout.inspector_open && layout.inspector_changes
            || app
                .workspace
                .read(cx)
                .activation_plan()
                .iter()
                .any(|p| p.metadata.kind == PaneKind::Diff);
        let watching = path
            .as_ref()
            .filter(|path| visible && app.git.read(cx).repository(path).is_some())
            .cloned();
        if watching != self.watching
            && let Some(client) = &self.client
        {
            if let Err(error) = client.watch_changes(watching.clone()) {
                self.error = Some(error);
            } else {
                self.watching = watching;
            }
        }
        let mut updated = changed;
        if let (Some(path), Some(client)) = (&path, &self.client)
            && let Some(result) = client.changes_snapshot(path)
        {
            match result {
                Ok(data) => {
                    if self
                        .data
                        .as_ref()
                        .is_none_or(|old| old.revision != data.revision)
                    {
                        self.data = Some(data);
                        updated = true;
                    }
                }
                Err(error) => {
                    if self.error.as_ref() != Some(&error) {
                        self.error = Some(error);
                        updated = true;
                    }
                }
            }
        }
        if updated {
            cx.notify();
        }
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.client
            && let Err(error) = client.refresh()
        {
            self.error = Some(error);
        }
        cx.notify();
    }
    pub fn open_diff(&mut self, file: FileChange, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let Ok(resource) = serde_json::to_string(&file) else {
            self.error = Some("This filename encoding cannot be stored in a diff pane.".into());
            cx.notify();
            return;
        };
        let workspace = cx.global::<AppState>().workspace.clone();
        workspace.update(cx, |w, cx| {
            if let Some(pane) = w.all_panes().into_iter().find(|p| {
                p.metadata.kind == PaneKind::Diff
                    && p.metadata
                        .resource
                        .as_ref()
                        .and_then(|s| serde_json::from_str::<FileChange>(s).ok())
                        .is_some_and(|target| {
                            target.path == file.path && target.staged == file.staged
                        })
                    && p.metadata.cwd.as_ref() == Some(&path)
            }) && let Some(tab) = w.tabs().iter().find(|t| t.root.find(pane.id).is_some())
            {
                let id = tab.id;
                let mut metadata = pane.metadata.clone();
                metadata.resource = Some(resource.clone());
                let _ = w.set_pane_metadata(id, pane.id, metadata);
                let _ = w.activate(id);
                let _ = w.focus(id, pane.id);
                cx.notify();
                return;
            }
            let title = format!(
                "{}{}",
                file.path.file_name().unwrap_or_default().to_string_lossy(),
                if file.staged {
                    " · staged"
                } else {
                    " · diff"
                }
            );
            let tab = w.open(title.clone(), "diff");
            let pane = w.active().unwrap().focused;
            let _ = w.set_pane_metadata(
                tab,
                pane,
                PaneMetadata {
                    cwd: Some(path),
                    resource: Some(resource),
                    kind: PaneKind::Diff,
                    title: Some(title),
                    ..Default::default()
                },
            );
            cx.notify();
        });
    }
    pub fn edit(
        &mut self,
        path: PathBuf,
        edit: Edit,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if self.busy || self.quitting || cx.global::<AppState>().git.read(cx).busy {
            return Err("Git is busy.".into());
        }
        let client = self.client.clone().ok_or("Git worker unavailable.")?;
        self.busy = true;
        self.error = None;
        cx.global::<AppState>()
            .git
            .clone()
            .update(cx, |g, cx| g.set_operation_busy(true, cx));
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = client.edit(path.clone(), edit).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if let Err(error) = result
                    && this.path.as_ref() == Some(&path)
                {
                    this.error = Some(error);
                }
                cx.global::<AppState>()
                    .git
                    .clone()
                    .update(cx, |g, cx| g.set_operation_busy(false, cx));
                this.sync(cx);
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn commit(&mut self, message: String, cx: &mut Context<Self>) -> Result<(), SharedString> {
        if self.busy || self.quitting || cx.global::<AppState>().git.read(cx).busy {
            return Err("Git is busy.".into());
        }
        let path = self.path.clone().ok_or("Select a worktree.")?;
        let data = self.data.clone().ok_or("Wait for Git changes to load.")?;
        if !data.files.iter().any(|f| f.staged) {
            return Err("Stage changes before committing.".into());
        }
        let env = cx
            .global::<AppState>()
            .tools
            .read(cx)
            .environment
            .clone()
            .ok_or("Shell environment is loading.")?
            .map_err(SharedString::from)?;
        let client = self.client.clone().ok_or("Git worker unavailable.")?;
        self.busy = true;
        self.committing = true;
        self.commit_phase = Some("Preparing commit…".into());
        self.commit_warning = None;
        self.commit_output = None;
        let (progress_tx, progress_rx) = async_channel::bounded(8);
        self.commit_progress = Some(cx.spawn(async move |this, cx| {
            while let Ok(phase) = progress_rx.recv().await {
                let _ = this.update(cx, |this, cx| {
                    if this.committing {
                        this.commit_phase = Some(phase);
                        cx.notify();
                    }
                });
            }
        }));
        self.error = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        cx.global::<AppState>()
            .git
            .clone()
            .update(cx, |g, cx| g.set_operation_busy(true, cx));
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = client
                .commit_with_hooks(canopy_desktop::git::CommitRequest {
                    path: path.clone(),
                    message,
                    head: data.head.clone(),
                    env: (*env).clone(),
                    cancel,
                    progress: Some(progress_tx),
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.committing = false;
                this.commit_phase = None;
                this.commit_progress = None;
                match result {
                    Ok(outcome) => {
                        let id = outcome.id.to_string();
                        this.commit_warning =
                            outcome.warning.map(|warning| (path.clone(), warning));
                        let output = outcome
                            .hooks
                            .into_iter()
                            .filter(|r| !r.output.is_empty())
                            .map(|r| format!("{}\n{}", r.name, r.output))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        this.commit_output = (!output.is_empty()).then(|| (path.clone(), output));
                        this.last_commit = Some((path.clone(), id.clone()));
                        let message = format!(
                            "Committed {} in {}{}",
                            &id[..8],
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            if this.commit_warning.is_some() {
                                " · post-commit needs attention"
                            } else {
                                ""
                            }
                        );
                        if !this.quitting {
                            cx.global::<AppState>()
                                .toasts
                                .clone()
                                .update(cx, |toasts, cx| toasts.show(message, cx));
                        }
                    }
                    Err(error) => {
                        this.error = Some(if this.path.as_ref() == Some(&path) {
                            error
                        } else {
                            format!("Commit in {} failed: {error}", path.display())
                        })
                    }
                }
                cx.global::<AppState>()
                    .git
                    .clone()
                    .update(cx, |g, cx| g.set_operation_busy(false, cx));
                this.sync(cx);
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn network(
        &mut self,
        operation: canopy_desktop::git::network::Operation,
        prepared: Option<(
            canopy_desktop::git::network::Plan,
            canopy_desktop::git::network::Upstream,
        )>,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if self.busy || self.quitting || cx.global::<AppState>().git.read(cx).busy {
            return Err("Git is busy.".into());
        }
        let path = self.path.clone().ok_or("Select a worktree.")?;
        if prepared.as_ref().is_some_and(|(plan, _)| plan.path != path) {
            return Err("The selected worktree changed. Try again.".into());
        }
        let client = self.client.clone().ok_or("Git worker unavailable.")?;
        self.busy = true;
        self.networking = true;
        self.network_operation = Some((path.clone(), operation));
        self.error = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        cx.global::<AppState>()
            .git
            .clone()
            .update(cx, |g, cx| g.set_operation_busy(true, cx));
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result: Result<Result<String, UpstreamRequest>, String> = async {
                let (plan, upstream) = match prepared {
                    Some((plan, target)) => (plan, Some(target)),
                    None => (client.network_plan(path.clone()).await?, None),
                };
                if cancel.load(Ordering::Acquire) { return Err("Operation cancelled.".into()); }
                if plan.upstream.is_none() && upstream.is_none() {
                    if plan.remotes.is_empty() { return Err("This repository has no remote. Add a remote before pulling or pushing.".into()); }
                    return Ok(Err(UpstreamRequest { plan, operation }));
                }
                client.network(plan, operation, upstream, cancel).await.map(Ok)
            }.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.networking = false;
                this.network_operation = None;
                this.network_revision += 1;
                cx.global::<AppState>().git.clone().update(cx, |g, cx| g.set_operation_busy(false, cx));
                this.sync(cx);
                match result {
                    Ok(Ok(message)) => {
                        if !this.quitting {
                            let message = if this.path.as_ref() == Some(&path) { message }
                                else { format!("{}: {message}", path.display()) };
                            cx.global::<AppState>().toasts.clone().update(cx, |toasts, cx| toasts.show(message, cx));
                        }
                    },
                    Ok(Err(request)) if !this.quitting && this.path.as_ref() == Some(&path) => cx.emit(request),
                    Ok(Err(_)) => {},
                    Err(error) => this.error = Some(if this.path.as_ref() == Some(&path) { error } else { format!("Git operation in {}: {error}", path.display()) }),
                }
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn begin_quit(&mut self) -> Option<Task<()>> {
        self.quitting = true;
        self.cancel();
        self.task.take()
    }
    pub fn cancel_quit(&mut self) {
        self.quitting = false;
    }
}
