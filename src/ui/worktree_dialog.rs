use super::{components::*, theme as t};
use crate::app_state::{AppState, GitState, RemovalConfirmation};
use canopy_desktop::{
    git::{
        CreateWorktree, MergeKind, RemovalAction, RemovalApproval, RemoveWorktree, RepositoryInfo,
        WorktreeAnalysis, worktree_path_proposal,
    },
    motion::{self, Presence, presets},
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName, IndexPath,
        button::ButtonVariants,
        input::InputState,
        scroll::ScrollableElement,
        select::{SelectEvent, SelectState},
    },
    *,
};
use std::{path::PathBuf, sync::Arc, time::Instant};

#[derive(Clone)]
pub enum WorktreeRequest {
    Create(Arc<RepositoryInfo>),
    Remove(RemoveWorktree),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RemoveChoice {
    #[default]
    Keep,
    Delete,
    Merge,
}

pub struct WorktreeDialog {
    request: WorktreeRequest,
    task: Option<canopy_desktop::integrations::TaskRef>,
    git: Entity<GitState>,
    branch: Entity<InputState>,
    destination: PathBuf,
    source: Entity<SelectState<Vec<SelectOption>>>,
    new_branch: bool,
    options: Disclosure,
    agent: Entity<SelectState<Vec<SelectOption>>>,
    agent_choices: Vec<(String, crate::app_state::WorktreeAgent)>,
    remove_choice: RemoveChoice,
    delete_after_merge: bool,
    analysis: Option<WorktreeAnalysis>,
    analyzing: bool,
    created: bool,
    focus: FocusHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    dismissed: bool,
    error: Option<SharedString>,
    submitted: bool,
    submit_loading: ButtonLoading,
    confirmation: Option<RemovalConfirmation>,
    stop_approved: bool,
    discard_approved: bool,
    branch_approved: bool,
    _events: Vec<Subscription>,
}

impl EventEmitter<ModalDismissed> for WorktreeDialog {}

impl WorktreeDialog {
    pub fn new(
        request: WorktreeRequest,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let git = cx.global::<AppState>().git.clone();
        let repo = match &request {
            WorktreeRequest::Create(repo) => Some(repo.clone()),
            WorktreeRequest::Remove(remove) => git.read(cx).repository(&remove.repository).cloned(),
        };
        let source_branch = match (&request, &repo) {
            (WorktreeRequest::Remove(remove), Some(repo)) => repo
                .worktrees
                .iter()
                .find(|item| item.path == remove.path)
                .and_then(|item| item.branch_name())
                .map(str::to_owned),
            _ => None,
        };
        let options: Vec<_> = repo
            .as_ref()
            .map(|repo| {
                repo.branches
                    .iter()
                    .filter(|branch| source_branch.as_deref() != Some(branch.as_str()))
                    .map(|branch| SelectOption::new(branch.clone(), branch.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let preferred = match (&request, &repo) {
            (WorktreeRequest::Create(repo), _) => {
                let current = cx.global::<AppState>().projects.read(cx).catalog.current();
                current
                    .filter(|project| {
                        project.repository_path.as_deref() == Some(repo.root.as_path())
                            || repo.worktrees.iter().any(|item| item.path == project.path)
                    })
                    .and_then(|project| {
                        repo.worktrees.iter().find(|item| item.path == project.path)
                    })
                    .and_then(|item| item.branch_name())
                    .or_else(|| repo.worktrees.first().and_then(|item| item.branch_name()))
                    .map(str::to_owned)
            }
            (WorktreeRequest::Remove(remove), Some(repo)) => {
                let saved = cx
                    .global::<AppState>()
                    .projects
                    .read(cx)
                    .catalog
                    .items
                    .iter()
                    .find(|project| project.path == remove.path)
                    .and_then(|project| project.worktree_base.as_ref())
                    .map(|base| base.reference.clone());
                saved.filter(|branch| {
                    repo.branches.contains(branch) && source_branch.as_ref() != Some(branch)
                })
            }
            _ => None,
        };
        let selected = preferred
            .as_ref()
            .and_then(|value| {
                options
                    .iter()
                    .position(|option| option.value.as_ref() == value)
            })
            .map(IndexPath::new);
        let source = cx.new(|cx| SelectState::new(options, selected, window, cx));
        let branch = cx.new(|cx| InputState::new(window, cx).placeholder("feature/my-change"));
        let focus = cx.focus_handle();
        let initial = if matches!(request, WorktreeRequest::Create(_)) {
            branch.read(cx).focus_handle(cx)
        } else {
            focus.clone()
        };
        window.on_next_frame(move |window, cx| initial.focus(window, cx));
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        let destination = match &request {
            WorktreeRequest::Create(repo) => worktree_path_proposal(&repo.root)
                .unwrap_or_else(|_| repo.root.with_extension("worktree")),
            WorktreeRequest::Remove(remove) => remove.path.clone(),
        };
        let mut agent_options = vec![SelectOption::new("none", "No agent")];
        let mut agent_choices = Vec::new();
        for tool in &cx.global::<AppState>().tools.read(cx).catalog.tools {
            if !tool.enabled || !matches!(tool.id.as_str(), "claude" | "codex") {
                continue;
            }
            if tool.profiles.is_empty() {
                agent_options.push(SelectOption::new(tool.id.clone(), tool.name.clone()));
                agent_choices.push((
                    tool.id.clone(),
                    crate::app_state::WorktreeAgent {
                        tool: tool.id.clone(),
                        profile: None,
                    },
                ));
            } else {
                for profile in &tool.profiles {
                    let key = format!("{}:{}", tool.id, profile.id);
                    agent_options.push(SelectOption::new(
                        key.clone(),
                        format!("{} · {}", tool.name, profile.name),
                    ));
                    agent_choices.push((
                        key,
                        crate::app_state::WorktreeAgent {
                            tool: tool.id.clone(),
                            profile: Some(profile.id.clone()),
                        },
                    ));
                }
            }
        }
        let git_events = cx.observe(&git, |this, git, cx| {
            this.submit_loading
                .set((this.submitted || this.analyzing) && git.read(cx).busy, cx);
            if this.analyzing && !git.read(cx).busy {
                this.analyzing = false;
                this.analysis = git.read(cx).worktree_analysis.clone();
                this.error = git.read(cx).error.clone().map(Into::into);
                cx.notify();
                return;
            }
            if this.submitted && !git.read(cx).busy {
                this.submitted = false;
                this.created = matches!(this.request, WorktreeRequest::Create(_))
                    && git.read(cx).created_path.is_some();
                if matches!(this.request, WorktreeRequest::Remove(_)) {
                    if let Some(step) = git.read(cx).removal_confirmation.clone() {
                        this.confirmation = Some(step);
                        this.error = None;
                        let now = Instant::now();
                        this.presence = Presence::new(false, presets::POPOVER, now);
                        this.presence.set_open(true, now, motion::policy(cx));
                        cx.notify();
                    } else if let Some(result) = &git.read(cx).removal_result {
                        this.analysis = git.read(cx).worktree_analysis.clone();
                        if result.retry_analysis.is_some() {
                            this.confirmation = None;
                            this.discard_approved = false;
                            this.branch_approved = false;
                        }
                        this.error = result.error.clone().map(Into::into);
                        if result.worktree_removed && result.error.is_none() {
                            this.close(cx);
                        }
                        cx.notify();
                    } else if let Some(error) = &git.read(cx).error {
                        this.analysis = git.read(cx).worktree_analysis.clone();
                        this.confirmation = None;
                        this.discard_approved = false;
                        this.branch_approved = false;
                        this.error = Some(error.clone().into());
                        cx.notify();
                    } else if this.registration_only() {
                        this.close(cx);
                    }
                } else if let Some(error) = &git.read(cx).error {
                    this.error = Some(error.clone().into());
                    cx.notify();
                } else {
                    this.close(cx);
                }
            }
        });
        let source_events = cx.subscribe(
            &source,
            |this, _, event: &SelectEvent<Vec<SelectOption>>, cx| {
                if matches!(event, SelectEvent::Confirm(Some(_)))
                    && matches!(this.request, WorktreeRequest::Remove(_))
                {
                    this.refresh_analysis(cx);
                }
            },
        );
        let confirmation = None;
        let mut dialog = Self {
            request,
            task: None,
            git,
            branch,
            destination,
            source,
            new_branch: true,
            options: Disclosure::new(false, now),
            agent: cx
                .new(|cx| SelectState::new(agent_options, Some(IndexPath::new(0)), window, cx)),
            agent_choices,
            remove_choice: RemoveChoice::Keep,
            delete_after_merge: true,
            analysis: None,
            analyzing: false,
            created: false,
            focus,
            return_focus,
            presence,
            closing: false,
            dismissed: false,
            error: None,
            submitted: false,
            submit_loading: ButtonLoading::default(),
            confirmation,
            stop_approved: false,
            discard_approved: false,
            branch_approved: false,
            _events: vec![git_events, source_events],
        };
        if matches!(dialog.request, WorktreeRequest::Remove(_)) && !dialog.registration_only() {
            dialog.refresh_analysis(cx);
        }
        dialog
    }

    fn registration_only(&self) -> bool {
        matches!(&self.request, WorktreeRequest::Remove(request) if request.registration_only())
    }

    pub fn attach_task(
        &mut self,
        task: canopy_desktop::integrations::TaskRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let slug = task
            .title
            .chars()
            .take(40)
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let branch = format!("issue-{}-{}", task.id, slug.trim_matches('-'));
        self.branch
            .update(cx, |field, cx| field.set_value(branch, window, cx));
        self.task = Some(task);
    }

    fn selected_target(&self, cx: &App) -> Option<String> {
        self.source
            .read(cx)
            .selected_value()
            .map(ToString::to_string)
    }

    fn refresh_analysis(&mut self, cx: &mut Context<Self>) {
        let WorktreeRequest::Remove(request) = &self.request else {
            return;
        };
        if request.registration_only() || self.git.read(cx).busy {
            return;
        }
        let target = (self.remove_choice != RemoveChoice::Keep)
            .then(|| self.selected_target(cx))
            .flatten();
        self.analysis = None;
        self.analyzing = true;
        self.error = None;
        self.confirmation = None;
        self.discard_approved = false;
        self.branch_approved = false;
        if let Err(error) = self.git.update(cx, |git, cx| {
            git.analyze_removal(request.clone(), target, cx)
        }) {
            self.analyzing = false;
            self.error = Some(error);
        }
        cx.notify();
    }

    fn choose_removal(&mut self, choice: RemoveChoice, cx: &mut Context<Self>) {
        self.remove_choice = choice;
        self.confirmation = None;
        self.discard_approved = false;
        self.branch_approved = false;
        self.refresh_analysis(cx);
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if (self.submitted || self.analyzing) && self.git.read(cx).busy {
            return;
        }
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }

    fn removal_action(&self, cx: &App) -> Option<RemovalAction> {
        match self.remove_choice {
            RemoveChoice::Keep => Some(RemovalAction::KeepBranch),
            RemoveChoice::Delete => self
                .selected_target(cx)
                .map(|target| RemovalAction::DeleteBranch { target }),
            RemoveChoice::Merge => self.selected_target(cx).map(|target| RemovalAction::Merge {
                target,
                delete_branch: self.delete_after_merge,
            }),
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.closing
            || self.submitted
            || self.analyzing
            || self.presence.is_animating(Instant::now())
        {
            return;
        }
        if self.created {
            self.close(cx);
            return;
        }
        if let WorktreeRequest::Remove(request) = &self.request
            && let Some(result) = &self.git.read(cx).removal_result
            && result.worktree_removed
            && !result.branch_deleted
            && result.error.is_some()
            && let Some(analysis) = result
                .retry_analysis
                .as_ref()
                .and_then(|value| value.target.clone())
        {
            let outcome = self.git.update(cx, |git, cx| {
                git.retry_branch_deletion(request.repository.clone(), analysis, cx)
            });
            match outcome {
                Ok(()) => {
                    self.submitted = true;
                    self.submit_loading.set(true, cx);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
            cx.notify();
            return;
        }
        let result = match &self.request {
            WorktreeRequest::Create(repo) => {
                let source = self.selected_target(cx).unwrap_or_default();
                let branch = if self.new_branch {
                    self.branch.read(cx).value().trim().to_owned()
                } else {
                    source.clone()
                };
                if branch.is_empty() {
                    self.error = Some(
                        if self.new_branch {
                            "Enter a branch name."
                        } else {
                            "Choose an existing branch."
                        }
                        .into(),
                    );
                    cx.notify();
                    return;
                }
                if self.new_branch && source.is_empty() {
                    self.error = Some("Choose a local branch to start from.".into());
                    cx.notify();
                    return;
                }
                let selected = self
                    .agent
                    .read(cx)
                    .selected_value()
                    .map(|value| value.as_ref())
                    .unwrap_or("none");
                let agent = self
                    .agent_choices
                    .iter()
                    .find(|(key, _)| key == selected)
                    .map(|(_, agent)| agent.clone());
                self.git.update(cx, |git, cx| {
                    git.create(
                        CreateWorktree {
                            repository: repo.root.clone(),
                            destination: self.destination.clone(),
                            branch,
                            new_branch: self.new_branch,
                            base: source,
                        },
                        self.task.clone(),
                        agent,
                        cx,
                    )
                })
            }
            WorktreeRequest::Remove(request) if request.registration_only() => {
                self.git.update(cx, |git, cx| {
                    if matches!(self.confirmation, Some(RemovalConfirmation::Processes)) {
                        self.stop_approved = true;
                    }
                    git.remove(request.clone(), self.stop_approved, false, cx)
                })
            }
            WorktreeRequest::Remove(request) => {
                let Some(action) = self.removal_action(cx) else {
                    self.error = Some("Choose a comparison or merge target branch.".into());
                    cx.notify();
                    return;
                };
                if matches!(self.confirmation, Some(RemovalConfirmation::Processes)) {
                    self.stop_approved = true;
                }
                if matches!(self.confirmation, Some(RemovalConfirmation::Changes(_))) {
                    self.discard_approved = true;
                }
                if matches!(self.confirmation, Some(RemovalConfirmation::Branch(_))) {
                    self.branch_approved = true;
                }
                let approval = RemovalApproval {
                    discard_changes: self.discard_approved,
                    delete_unmerged_branch: self.branch_approved,
                };
                self.git.update(cx, |git, cx| {
                    git.execute_removal(request.clone(), action, self.stop_approved, approval, cx)
                })
            }
        };
        match result {
            Ok(()) => {
                self.submitted = true;
                self.submit_loading.set(self.git.read(cx).busy, cx);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn creation_occupied(&self, cx: &App) -> bool {
        let WorktreeRequest::Create(repo) = &self.request else {
            return false;
        };
        !self.new_branch
            && self.selected_target(cx).is_some_and(|branch| {
                repo.worktrees
                    .iter()
                    .any(|item| item.checkout_branch() == Some(branch.as_str()))
            })
    }

    fn creation_content(&self, disabled: bool, now: Instant, cx: &mut Context<Self>) -> Div {
        let WorktreeRequest::Create(repo) = &self.request else {
            return div();
        };
        let selected = self.selected_target(cx);
        let occupied = (!self.new_branch)
            .then(|| {
                selected.as_ref().and_then(|branch| {
                    repo.worktrees
                        .iter()
                        .find(|item| item.checkout_branch() == Some(branch.as_str()))
                })
            })
            .flatten();
        column()
            .gap(px(12.))
            .children(self.new_branch.then(|| {
                modal::modal_field(
                    "Branch name",
                    input(&self.branch).disabled(disabled).w_full(),
                    None,
                )
            }))
            .children(self.new_branch.then(|| {
                modal::modal_field(
                    "Start from",
                    dropdown(&self.source).disabled(disabled).w_full(),
                    None,
                )
            }))
            .children((!self.new_branch).then(|| {
                modal::modal_field(
                    "Existing branch",
                    dropdown(&self.source).disabled(disabled).w_full(),
                    None,
                )
            }))
            .child(
                button(
                    "worktree-existing-mode",
                    if self.new_branch {
                        "Use existing branch"
                    } else {
                        "Create a new branch"
                    },
                )
                .disabled(disabled)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.new_branch = !this.new_branch;
                    this.error = None;
                    cx.notify();
                })),
            )
            .children(occupied.map(|item| {
                let root = repo.root.clone();
                let path = item.path.clone();
                row()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_color(t::secondary())
                            .child(if item.available {
                                "This branch is already open in a worktree."
                            } else {
                                "This branch belongs to an unavailable worktree. Remove its stale registration before reusing it."
                            }),
                    )
                    .children(item.available.then(|| {
                        button("open-existing-worktree", "Open worktree")
                            .disabled(disabled)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.git
                                    .update(cx, |git, cx| git.open(root.clone(), path.clone(), cx));
                                this.close(cx);
                            }))
                    }))
            }))
            .child(
                self.options
                    .header("worktree-options", "Options", now, cx)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.options.toggle(Instant::now(), cx);
                        cx.notify();
                    })),
            )
            .child(self.options.measured_body(
                "worktree-options-body",
                modal::modal_field(
                    "Agent / profile",
                    dropdown(&self.agent).disabled(disabled).w_full(),
                    Some("No agent creates an empty workspace.".into()),
                ),
                now,
            ))
            .child(modal::modal_field(
                "Worktree directory",
                row()
                    .gap(px(6.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(t::secondary())
                            .child(canopy_desktop::platform::paths::display(&self.destination)),
                    )
                    .child(
                        icon_button("copy-worktree-path", IconName::Copy, "Copy path")
                            .disabled(disabled)
                            .on_click({
                                let path = self.destination.display().to_string();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(path.clone()))
                                }
                            }),
                    ),
                None,
            ))
    }

    fn analysis_summary(&self) -> Div {
        let Some(analysis) = &self.analysis else {
            return div().text_color(t::muted()).child(if self.analyzing {
                "Analyzing worktree…"
            } else {
                "Analysis is unavailable."
            });
        };
        let mut content = column()
            .gap(px(4.))
            .child(format!("Branch: {}", analysis.branch))
            .child(format!(
                "Local entries to review: {}",
                analysis.changed_entries
            ));
        if let Some(target) = &analysis.target {
            content = content.child(format!(
                "{} → {} · {} commit(s) not in target",
                target.source, target.target, target.commits_not_in_target
            ));
            let message = match &target.merge {
                MergeKind::AlreadyIntegrated => "Already integrated".to_owned(),
                MergeKind::FastForward => "Fast-forward".to_owned(),
                MergeKind::MergeCommit => "Merge commit".to_owned(),
                MergeKind::Conflicts(paths) => format!(
                    "Conflicts: {}",
                    paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                MergeKind::Blocked(reason) => format!("Blocked: {reason}"),
            };
            content = content.child(
                div()
                    .text_color(
                        if matches!(
                            target.merge,
                            MergeKind::Conflicts(_) | MergeKind::Blocked(_)
                        ) {
                            t::red()
                        } else {
                            t::secondary()
                        },
                    )
                    .child(message),
            );
        }
        content.text_size(px(12.)).text_color(t::secondary())
    }

    fn removal_content(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        let WorktreeRequest::Remove(request) = &self.request else {
            return div();
        };
        if let Some(confirmation) = &self.confirmation {
            let (title, detail) = match confirmation {
                RemovalConfirmation::Processes if request.registration_only() => ("Close processes and remove this missing entry?".to_owned(), "Running panes will stop. Existing files and branches remain.".to_owned()),
                RemovalConfirmation::Processes => ("Close all processes in this worktree?".to_owned(), "Canopy will wait for every pane before checking files again.".to_owned()),
                RemovalConfirmation::Changes(count) => (format!("Permanently remove {count} changed, untracked or ignored entries?"), "The selected worktree directory will be deleted. This cannot be undone.".to_owned()),
                RemovalConfirmation::Branch(analysis) => (format!("Delete local branch '{}' with {} commit(s) not in '{}'?", analysis.source, analysis.commits_not_in_target, analysis.target), "The commits are not reachable from the selected comparison branch. Git object retention is not guaranteed.".to_owned()),
            };
            return column()
                .gap(px(12.))
                .child(title)
                .child(
                    div()
                        .text_color(t::secondary())
                        .child(canopy_desktop::platform::paths::display(&request.path)),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t::muted())
                        .child(detail),
                );
        }
        if request.registration_only() {
            return column().gap(px(12.)).child("Remove this missing worktree entry and its saved layout?")
                .child(
                    div()
                        .text_color(t::secondary())
                        .child(canopy_desktop::platform::paths::display(&request.path)),
                )
                .child(div().text_size(px(12.)).text_color(t::muted()).child("Only the stale Git registration and saved workspace are removed. The branch remains."));
        }
        let target_worktree = self
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.target.as_ref())
            .filter(|target| {
                matches!(
                    target.merge,
                    MergeKind::Conflicts(_) | MergeKind::Blocked(_)
                )
            })
            .and_then(|target| {
                self.git
                    .read(cx)
                    .repository(&request.repository)
                    .and_then(|repository| {
                        repository
                            .worktrees
                            .iter()
                            .find(|item| {
                                item.available
                                    && item.checkout_branch() == Some(target.target.as_str())
                            })
                            .map(|item| (repository.root.clone(), item.path.clone()))
                    })
            });
        column()
            .gap(px(12.))
            .child(
                self.analysis
                    .as_ref()
                    .map(|analysis| analysis.branch.clone())
                    .unwrap_or_else(|| "Worktree".into()),
            )
            .child(
                div()
                    .text_color(t::secondary())
                    .child(canopy_desktop::platform::paths::display(&request.path)),
            )
            .child(self.analysis_summary())
            .child(
                column()
                    .gap(px(4.))
                    .child(
                        selection_button(
                            "remove-keep",
                            "Keep branch",
                            self.remove_choice == RemoveChoice::Keep,
                        )
                        .w_full()
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|this, _, _, cx| {
                                this.choose_removal(RemoveChoice::Keep, cx)
                            }),
                        ),
                    )
                    .child(
                        selection_button(
                            "remove-delete",
                            "Delete local branch",
                            self.remove_choice == RemoveChoice::Delete,
                        )
                        .w_full()
                        .disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.choose_removal(RemoveChoice::Delete, cx)
                        })),
                    )
                    .child(
                        selection_button(
                            "remove-merge",
                            "Merge into another branch, then remove",
                            self.remove_choice == RemoveChoice::Merge,
                        )
                        .w_full()
                        .disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.choose_removal(RemoveChoice::Merge, cx)
                        })),
                    ),
            )
            .children((self.remove_choice != RemoveChoice::Keep).then(|| {
                modal::modal_field(
                    if self.remove_choice == RemoveChoice::Merge {
                        "Merge into"
                    } else {
                        "Compare with"
                    },
                    dropdown(&self.source).w_full().disabled(disabled),
                    None,
                )
            }))
            .children((self.remove_choice == RemoveChoice::Merge).then(|| {
                row()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        checkbox(
                            "delete-after-merge",
                            self.delete_after_merge,
                            "Delete local branch after merge",
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(|this, value, _, cx| {
                            this.delete_after_merge = *value;
                            cx.notify();
                        })),
                    )
                    .child("Delete local branch after merge")
            }))
            .children(
                self.analysis
                    .as_ref()
                    .filter(|analysis| {
                        self.remove_choice == RemoveChoice::Merge && analysis.tracked_changes > 0
                    })
                    .map(|_| {
                        button("open-source-changes", "Open Changes")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let WorktreeRequest::Remove(request) = &this.request {
                                    let projects = cx.global::<AppState>().projects.clone();
                                    let workspace = projects
                                        .read(cx)
                                        .catalog
                                        .items
                                        .iter()
                                        .find(|project| project.path == request.path)
                                        .map(|project| project.workspace);
                                    if let Some(workspace) = workspace {
                                        projects.update(cx, |projects, cx| {
                                            projects.select(workspace, cx)
                                        });
                                    }
                                }
                                let layout = cx.global::<AppState>().layout.clone();
                                layout.update(cx, |layout, cx| {
                                    layout.inspector_open = true;
                                    layout.inspector_changes = true;
                                    layout.inspector_tasks = false;
                                    cx.notify();
                                });
                                this.close(cx);
                            }))
                    }),
            )
            .children(target_worktree.map(|(root, path)| {
                button("open-merge-target", "Open target worktree")
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.git
                            .update(cx, |git, cx| git.open(root.clone(), path.clone(), cx));
                        this.close(cx);
                    }))
            }))
    }

    fn submit_label(&self, cx: &App) -> &'static str {
        if self.created {
            return "Close";
        }
        if self
            .git
            .read(cx)
            .removal_result
            .as_ref()
            .is_some_and(|result| {
                result.worktree_removed && !result.branch_deleted && result.error.is_some()
            })
        {
            return "Retry branch deletion";
        }
        if matches!(self.request, WorktreeRequest::Create(_)) {
            return "Create worktree";
        }
        if let Some(confirmation) = &self.confirmation {
            return match confirmation {
                RemovalConfirmation::Processes => "Close processes and continue",
                RemovalConfirmation::Changes(_) => "Discard and continue",
                RemovalConfirmation::Branch(_) => "Delete branch and continue",
            };
        }
        if self.registration_only() {
            return "Remove entry";
        }
        match self.remove_choice {
            RemoveChoice::Keep => "Remove worktree",
            RemoveChoice::Delete => "Remove worktree and branch",
            RemoveChoice::Merge => "Merge and remove",
        }
    }

    fn action_blocked(&self, cx: &App) -> bool {
        if self.remove_choice != RemoveChoice::Keep && self.selected_target(cx).is_none() {
            return true;
        }
        if self.remove_choice != RemoveChoice::Merge {
            return false;
        }
        self.analysis.as_ref().is_some_and(|analysis| {
            analysis.tracked_changes > 0
                || analysis.target.as_ref().is_none_or(|target| {
                    matches!(
                        target.merge,
                        MergeKind::Conflicts(_) | MergeKind::Blocked(_)
                    )
                })
        })
    }
}

impl Render for WorktreeDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animating = self.presence.is_animating(now) || self.options.active(now);
        motion::request_frame(window, animating);
        if self.closing && !animating && !self.dismissed {
            self.dismissed = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.return_focus.focus(window, cx);
                    cx.emit(ModalDismissed);
                });
            });
        }
        let creating = matches!(self.request, WorktreeRequest::Create(_));
        let disabled = self.closing || self.git.read(cx).busy || animating || self.analyzing;
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(0., 480.);
        let max_content = (f32::from(window.viewport_size().height) - 220.).max(120.);
        let content = if creating {
            self.creation_content(disabled, now, cx)
        } else {
            self.removal_content(disabled, cx)
        };
        let submit = primary_loading_button(
            "submit-worktree",
            self.submit_label(cx),
            &self.submit_loading,
        )
        .disabled(
            (disabled
                || self.creation_occupied(cx)
                || self.action_blocked(cx)
                || (!creating && self.analysis.is_none() && !self.registration_only()))
                && !self.submit_loading.active(),
        )
        .on_click(cx.listener(|this, _, _, cx| this.submit(cx)));
        let submit = if creating { submit } else { submit.danger() };
        let panel = modal::modal_surface("worktree-dialog")
            .absolute()
            .left((window.viewport_size().width - px(width)) / 2.)
            .top(px(60. + motion::distance::BASE * (1. - progress)))
            .w(px(width))
            .opacity(progress)
            .child(modal::modal_header(
                if creating {
                    "Create worktree"
                } else if self.registration_only() {
                    "Remove stale worktree entry"
                } else {
                    "Remove worktree"
                },
                icon_button("close-worktree-dialog", IconName::Close, "Close dialog")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ))
            .child(
                div()
                    .max_h(px(max_content))
                    .overflow_y_scrollbar()
                    .child(content),
            )
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(t::red()).child(error)),
            )
            .child(modal::modal_actions(
                button("cancel-worktree", "Cancel")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                submit,
            ));
        let ok = cx.listener(|this, _, _, cx| this.submit(cx));
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(false)
            .on_ok(move |event, window, cx| {
                ok(event, window, cx);
                false
            })
            .on_cancel(move |event, window, cx| {
                cancel(event, window, cx);
                false
            })
            .backdrop(modal::modal_backdrop(progress))
            .popup(panel)
    }
}
