//! The pull request of the branch shown in Changes, and opening a new one: the
//! branch is pushed first, then the pull request is created on GitHub.
use super::{AppState, integrations::GithubUnavailable};
use canopy_desktop::{
    git::network::{Operation, Upstream},
    integrations::github::{NewPull, PullRef},
};
use gpui_kit::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// A lookup older than this is repeated when its branch is shown again.
const FRESH: Duration = Duration::from_secs(60);

/// A worktree's checked-out branch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Branch {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug)]
pub enum Lookup {
    Checking,
    /// The open pull request of the branch, else its latest; `None` without any.
    Found(Option<PullRef>),
    Unavailable(GithubUnavailable),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Pushing,
    Creating,
}

/// A pull request dialog to show; the workspace opens it.
#[derive(Clone)]
pub enum PullRequestDialog {
    Create(Branch),
    Details(PullRef),
}
impl EventEmitter<PullRequestDialog> for PullRequests {}

/// What to open; the source branch comes from the [`Branch`].
pub struct Draft {
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
}

pub struct PullRequests {
    /// The branch Changes shows.
    pub current: Option<Branch>,
    lookups: HashMap<Branch, (Instant, Lookup)>,
    lookup_tasks: HashMap<Branch, Task<()>>,
    /// The branch whose pull request is being opened, and the step.
    pub creating: Option<(Branch, Phase)>,
    /// The last attempt to open a pull request, for its dialog.
    pub outcome: Option<(Branch, Result<PullRef, String>)>,
    create_task: Option<Task<()>>,
    network_revision: u64,
    accounts: (bool, usize),
    _observers: Vec<Subscription>,
}

impl PullRequests {
    pub fn new() -> Self {
        Self {
            current: None,
            lookups: HashMap::new(),
            lookup_tasks: HashMap::new(),
            creating: None,
            outcome: None,
            create_task: None,
            network_revision: 0,
            accounts: (false, 0),
            _observers: vec![],
        }
    }

    pub fn bind(&mut self, app: &AppState, cx: &mut Context<Self>) {
        self._observers = vec![
            cx.observe(&app.changes, |this, _, cx| this.sync(cx)),
            cx.observe(&app.git, |this, _, cx| this.sync(cx)),
            cx.observe(&app.integrations, |this, _, cx| this.sync(cx)),
        ];
    }

    pub fn lookup(&self, branch: &Branch) -> Option<&Lookup> {
        self.lookups.get(branch).map(|(_, lookup)| lookup)
    }

    /// Follows the branch Changes shows; a push or a new GitHub connection asks again.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        let changes = app.changes.read(cx);
        let revision = changes.network_revision;
        let current = changes.path.clone().and_then(|path| {
            let name = app
                .git
                .read(cx)
                .repository(&path)?
                .worktrees
                .iter()
                .filter(|worktree| worktree.path == path)
                .find_map(|worktree| worktree.branch_name().map(str::to_owned))?;
            Some(Branch { path, name })
        });
        let integrations = app.integrations.read(cx);
        let accounts = (integrations.ready, integrations.config.accounts.len());
        if accounts != self.accounts {
            self.accounts = accounts;
            self.lookups.clear();
        }
        let pushed = revision != self.network_revision;
        self.network_revision = revision;
        if current != self.current {
            self.current = current.clone();
            cx.notify();
        }
        if let Some(branch) = current
            && (pushed
                || self
                    .lookups
                    .get(&branch)
                    .is_none_or(|(at, _)| at.elapsed() > FRESH))
            && !self.lookup_tasks.contains_key(&branch)
            && accounts.0
        {
            self.refresh(branch, cx);
        }
    }

    /// Asks GitHub for the branch's pull request.
    pub fn refresh(&mut self, branch: Branch, cx: &mut Context<Self>) {
        self.lookups
            .insert(branch.clone(), (Instant::now(), Lookup::Checking));
        let github = cx
            .global::<AppState>()
            .integrations
            .clone()
            .update(cx, |integrations, cx| {
                integrations.github(branch.path.clone(), cx)
            });
        let key = branch.clone();
        let slot = branch.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = async {
                let (client, owner, name) = github.await?;
                client
                    .branch_pull(&owner, &name, &branch.name)
                    .await
                    .map_err(GithubUnavailable::Failed)
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.lookup_tasks.remove(&key);
                let lookup = match result {
                    Ok(pull) => Lookup::Found(pull),
                    Err(unavailable) => Lookup::Unavailable(unavailable),
                };
                this.lookups.insert(key, (Instant::now(), lookup));
                cx.notify();
            });
        });
        self.lookup_tasks.insert(slot, task);
        cx.notify();
    }

    pub fn show(&mut self, dialog: PullRequestDialog, cx: &mut Context<Self>) {
        cx.emit(dialog);
    }

    /// Pushes the branch (publishing it to `origin` without an upstream), then
    /// opens the pull request. Runs to the end even if its dialog closes; only
    /// the push holds Git busy.
    pub fn create(
        &mut self,
        branch: Branch,
        draft: Draft,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        let app = cx.global::<AppState>().clone();
        if self.creating.is_some() {
            return Err("A pull request is already being created.".into());
        }
        if app.changes.read(cx).busy || app.git.read(cx).busy {
            return Err("Git is busy. Try again when it finishes.".into());
        }
        let git = app.git.read(cx).client().ok_or("Git worker unavailable.")?;
        let github = app.integrations.clone().update(cx, |integrations, cx| {
            integrations.github(branch.path.clone(), cx)
        });
        app.git
            .clone()
            .update(cx, |git, cx| git.set_operation_busy(true, cx));
        self.creating = Some((branch.clone(), Phase::Pushing));
        self.outcome = None;
        cx.notify();
        self.create_task = Some(cx.spawn(async move |this, cx| {
            // GitHub access is checked before anything is pushed.
            let pushed = async {
                let (client, owner, name) = github.await.map_err(|error| error.to_string())?;
                let plan = git.network_plan(branch.path.clone()).await?;
                if plan.branch != branch.name {
                    return Err("The worktree switched branches. Try again.".to_owned());
                }
                let (upstream, head) = match &plan.upstream {
                    Some(upstream) => (None, upstream.branch.clone()),
                    None => {
                        let remote = plan
                            .remotes
                            .iter()
                            .find(|remote| *remote == "origin")
                            .or(plan.remotes.first())
                            .ok_or("This repository has no remote to push the branch to.")?
                            .clone();
                        let target = Upstream {
                            remote,
                            branch: branch.name.clone(),
                        };
                        (Some(target), branch.name.clone())
                    }
                };
                git.network(
                    plan,
                    Operation::Push,
                    upstream,
                    Arc::new(AtomicBool::new(false)),
                )
                .await?;
                Ok::<_, String>((client, owner, name, head))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                cx.global::<AppState>()
                    .git
                    .clone()
                    .update(cx, |git, cx| git.set_operation_busy(false, cx));
                if pushed.is_ok() {
                    this.creating = Some((branch.clone(), Phase::Creating));
                }
                cx.notify();
            });
            let result = async {
                let (client, owner, name, head) = pushed?;
                let pull = NewPull {
                    head,
                    base: draft.base,
                    title: draft.title,
                    body: draft.body,
                    draft: draft.draft,
                };
                client
                    .create_pull(&owner, &name, &pull)
                    .await
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.creating = None;
                if let Ok(pull) = &result {
                    this.lookups.insert(
                        branch.clone(),
                        (Instant::now(), Lookup::Found(Some(pull.clone()))),
                    );
                    let message = format!("Created pull request #{}", pull.number);
                    cx.global::<AppState>()
                        .toasts
                        .clone()
                        .update(cx, |toasts, cx| toasts.show(message, cx));
                }
                this.outcome = Some((branch, result));
                cx.notify();
            });
        }));
        Ok(())
    }
}
