//! Pull requests of the worktree's branch (Electron: `PullRequestActions`,
//! `CreateTaskPRModal`, `PRDetailsModal`): the strip under Pull/Push in Changes and
//! the create and details dialogs. State and GitHub calls live in `PullRequests`.
use super::{components::*, markdown::MarkdownView, theme as t};
use crate::app_state::{
    AppState, Branch, Draft, GithubUnavailable, Lookup, Phase, PullRequestDialog, PullRequests,
};
use canopy_desktop::{
    integrations::github::PullRef,
    motion::{self, Presence, presets},
};
use gpui_kit::{
    base::Disableable,
    component::{
        Icon, IconName, IndexPath,
        input::{InputState, TextareaState},
        select::SelectState,
    },
    prelude::FluentBuilder,
    *,
};
use std::time::Instant;

/// Characters GitHub returns for a description; a longer one is cut there.
const BODY_LIMIT: usize = 1_500;

fn state_icon(state: &str) -> &'static str {
    match state {
        "merged" => "git-merge",
        "closed" => "git-pull-request-closed",
        "draft" => "git-pull-request-draft",
        _ => "git-pull-request",
    }
}

fn state_color(state: &str) -> Hsla {
    match state {
        "open" => t::green(),
        "merged" => t::skill(),
        "closed" => t::red(),
        _ => t::muted(),
    }
}

fn state_chip(state: &str) -> Div {
    let color = state_color(state);
    let label = match state {
        "open" => "Open",
        "draft" => "Draft",
        "merged" => "Merged",
        _ => "Closed",
    };
    row()
        .flex_shrink_0()
        .px(px(6.))
        .h(px(16.))
        .rounded(px(5.))
        .bg(color.opacity(0.14))
        .text_size(px(10.5))
        .text_color(color)
        .child(label)
}

fn branch_chip(name: impl Into<SharedString>) -> Div {
    div()
        .min_w_0()
        .truncate()
        .px(px(6.))
        .rounded(px(5.))
        .bg(t::hover())
        .font_family("JetBrains Mono")
        .text_size(px(11.))
        .text_color(t::secondary())
        .child(name.into())
}

fn show(dialog: PullRequestDialog, cx: &mut App) {
    cx.global::<AppState>()
        .pull_requests
        .clone()
        .update(cx, |pulls, cx| pulls.show(dialog, cx));
}

/// One line of the strip: icon, text and an optional action.
fn strip_row(icon: Icon, text: impl Into<SharedString>, color: Hsla) -> Div {
    row()
        .gap(px(8.))
        .min_h(px(24.))
        .text_size(px(12.))
        .text_color(color)
        .child(icon.text_color(color))
        .child(div().flex_1().min_w_0().child(text.into()))
}

fn create_button(branch: &Branch) -> Stateful<Div> {
    let branch = branch.clone();
    row()
        .id("create-pull-request")
        .keyboard("Create pull request")
        .gap(px(8.))
        .px(px(10.))
        .py(px(7.))
        .rounded(px(6.))
        .border_1()
        .border_dashed()
        .border_color(t::control_border())
        .cursor_pointer()
        .hover(|row| row.bg(t::hover()))
        .text_size(px(12.))
        .text_color(t::secondary())
        .child(custom_icon("git-pull-request").text_color(t::accent()))
        .child(div().flex_1().child("Create pull request"))
        .on_click(move |_, _, cx| show(PullRequestDialog::Create(branch.clone()), cx))
}

fn pull_card(pull: &PullRef) -> Stateful<Div> {
    let opened = pull.clone();
    column()
        .id("pull-request-card")
        .keyboard(format!("Pull request #{}: {}", pull.number, pull.title))
        .gap(px(4.))
        .px(px(10.))
        .py(px(7.))
        .rounded(px(6.))
        .border_1()
        .border_color(t::border())
        .bg(t::bg())
        .cursor_pointer()
        .hover(|card| card.bg(t::hover()))
        .on_click(move |_, _, cx| show(PullRequestDialog::Details(opened.clone()), cx))
        .child(
            row()
                .gap(px(8.))
                .text_size(px(12.))
                .child(custom_icon(state_icon(&pull.state)).text_color(state_color(&pull.state)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(t::text())
                        .child(format!("#{} {}", pull.number, pull.title)),
                )
                .child(state_chip(&pull.state)),
        )
        .child(
            row()
                .pl(px(22.))
                .gap(px(5.))
                .text_size(px(11.))
                .text_color(t::faint())
                .child(branch_chip(pull.head.clone()))
                .child("→")
                .child(branch_chip(pull.base.clone())),
        )
}

/// The pull request of the branch Changes shows, under Pull/Push. Nothing without
/// a branch or outside a Git repository.
pub(super) fn strip(cx: &App) -> Option<AnyElement> {
    let pulls = cx.global::<AppState>().pull_requests.read(cx);
    let branch = pulls.current.clone()?;
    if let Some((creating, phase)) = &pulls.creating
        && *creating == branch
    {
        let text = match phase {
            Phase::Pushing => format!("Pushing {}…", branch.name),
            Phase::Creating => "Creating pull request…".into(),
        };
        return Some(strip_row(icon(IconName::Loader), text, t::muted()).into_any_element());
    }
    let element = match pulls.lookup(&branch)? {
        Lookup::Checking => strip_row(
            icon(IconName::Loader),
            "Checking pull requests…",
            t::muted(),
        )
        .into_any_element(),
        Lookup::Found(None) => create_button(&branch).into_any_element(),
        Lookup::Found(Some(pull)) if matches!(pull.state.as_str(), "open" | "draft") => {
            pull_card(pull).into_any_element()
        }
        // Merged or closed: a new pull request can follow.
        Lookup::Found(Some(pull)) => column()
            .gap(px(6.))
            .child(pull_card(pull))
            .child(create_button(&branch))
            .into_any_element(),
        Lookup::Unavailable(GithubUnavailable::NotRepository) => return None,
        Lookup::Unavailable(GithubUnavailable::NotGithub) => div()
            .text_size(px(11.))
            .text_color(t::faint())
            .child("Pull requests are available for GitHub remotes.")
            .into_any_element(),
        Lookup::Unavailable(GithubUnavailable::NoAccount) => strip_row(
            custom_icon("git-pull-request"),
            "Connect GitHub to see pull requests.",
            t::muted(),
        )
        .child(
            button("pull-requests-connect", "Integrations").on_click(|_, _, cx| {
                let _ = super::preferences::open_integrations(cx);
            }),
        )
        .into_any_element(),
        Lookup::Unavailable(GithubUnavailable::Failed(error)) => {
            strip_row(icon(IconName::TriangleAlert), error.clone(), t::yellow())
                .child(
                    button("pull-requests-retry", "Retry").on_click(move |_, _, cx| {
                        let branch = branch.clone();
                        cx.global::<AppState>()
                            .pull_requests
                            .clone()
                            .update(cx, |pulls, cx| pulls.refresh(branch, cx));
                    }),
                )
                .into_any_element()
        }
    };
    Some(element)
}

/// The branch name as a title: its last segment, words separated.
fn title_from(branch: &str) -> String {
    let words = branch
        .rsplit('/')
        .next()
        .unwrap_or(branch)
        .replace(['-', '_'], " ");
    let mut chars = words.trim().chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Target branches of `branch`'s repository, the preselected one first: the
/// worktree's creation base, else a usual default branch.
fn targets(branch: &Branch, cx: &App) -> Vec<String> {
    let app = cx.global::<AppState>();
    let mut names: Vec<String> = app
        .git
        .read(cx)
        .repository(&branch.path)
        .map(|repository| repository.branches.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|name| *name != branch.name)
        .collect();
    let base = app
        .projects
        .read(cx)
        .catalog
        .items
        .iter()
        .filter(|project| {
            project.path == branch.path || project.worktree_path.as_ref() == Some(&branch.path)
        })
        .find_map(|project| project.worktree_base.as_ref().map(|b| b.reference.clone()));
    let preferred = base
        .into_iter()
        .chain(["main", "master", "develop"].map(str::to_owned))
        .find(|name| names.contains(name));
    if let Some(preferred) = preferred {
        names.retain(|name| *name != preferred);
        names.insert(0, preferred);
    }
    names
}

pub struct CreatePullRequest {
    branch: Branch,
    title: Entity<InputState>,
    body: Entity<TextareaState>,
    target: Entity<SelectState<Vec<SelectOption>>>,
    has_targets: bool,
    draft: bool,
    submitted: bool,
    error: Option<SharedString>,
    focus: FocusHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    _pulls: Subscription,
}
impl EventEmitter<ModalDismissed> for CreatePullRequest {}

impl CreatePullRequest {
    pub fn new(
        branch: Branch,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let options: Vec<_> = targets(&branch, cx)
            .into_iter()
            .map(|name| SelectOption::new(name.clone(), name))
            .collect();
        let has_targets = !options.is_empty();
        let target = cx.new(|cx| {
            SelectState::new(options, has_targets.then(|| IndexPath::new(0)), window, cx)
                .searchable(true)
        });
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Title"));
        let initial = title_from(&branch.name);
        title.update(cx, |input, cx| input.set_value(initial, window, cx));
        let body = cx
            .new(|cx| TextareaState::new(window, cx).placeholder("Describe the change (Markdown)"));
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        let focus = cx.focus_handle();
        let first = title.read(cx).focus_handle(cx);
        window.on_next_frame(move |window, cx| first.focus(window, cx));
        let pulls = cx.global::<AppState>().pull_requests.clone();
        Self {
            branch,
            title,
            body,
            target,
            has_targets,
            draft: false,
            submitted: false,
            error: None,
            focus,
            return_focus,
            presence,
            closing: false,
            scheduled: false,
            _pulls: cx.observe(&pulls, |this, pulls, cx| this.finished(pulls, cx)),
        }
    }

    fn busy(&self, cx: &App) -> bool {
        self.submitted
            && cx
                .global::<AppState>()
                .pull_requests
                .read(cx)
                .creating
                .as_ref()
                .is_some_and(|(branch, _)| *branch == self.branch)
    }

    /// Closes on success; a failure stays in the form so nothing typed is lost.
    fn finished(&mut self, pulls: Entity<PullRequests>, cx: &mut Context<Self>) {
        if !self.submitted || self.busy(cx) {
            return cx.notify();
        }
        let outcome = match &pulls.read(cx).outcome {
            Some((branch, outcome)) if *branch == self.branch => outcome.clone(),
            _ => return,
        };
        self.submitted = false;
        match outcome {
            Ok(_) => self.close(cx),
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
            }
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if self.busy(cx) {
            return;
        }
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.busy(cx) || self.presence.is_animating(Instant::now()) {
            return;
        }
        let title = self.title.read(cx).value().trim().to_owned();
        let Some(base) = self.target.read(cx).selected_value().cloned() else {
            self.error = Some("Choose a target branch.".into());
            return cx.notify();
        };
        if title.is_empty() {
            self.error = Some("Enter a title.".into());
            return cx.notify();
        }
        let draft = Draft {
            base: base.to_string(),
            title,
            body: self.body.read(cx).value().to_string(),
            draft: self.draft,
        };
        let branch = self.branch.clone();
        let result = cx
            .global::<AppState>()
            .pull_requests
            .clone()
            .update(cx, |pulls, cx| pulls.create(branch, draft, cx));
        match result {
            Ok(()) => {
                self.submitted = true;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
}

impl Render for CreatePullRequest {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animated = self.presence.is_animating(now);
        motion::request_frame(window, animated);
        if self.closing && !animated && !self.scheduled {
            self.scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.return_focus.focus(window, cx);
                    cx.emit(ModalDismissed);
                });
            });
        }
        let phase = self.submitted.then(|| {
            cx.global::<AppState>()
                .pull_requests
                .read(cx)
                .creating
                .as_ref()
                .map(|(_, phase)| *phase)
        });
        let busy = phase.flatten().is_some();
        let disabled = self.closing || animated || busy;
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(0., 560.);
        let panel = modal::modal_surface("create-pull-request-dialog")
            .absolute()
            .w(px(width))
            .left((window.viewport_size().width - px(width)) / 2.)
            .top(px(72. + motion::distance::BASE * (1. - progress)))
            .opacity(progress)
            .child(modal::modal_header(
                "Create pull request",
                icon_button("close-create-pull-request", IconName::Close, "Close dialog")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ))
            .child(
                row()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(t::faint())
                    .child("from")
                    .child(branch_chip(self.branch.name.clone())),
            )
            .child(modal::modal_field(
                "Title",
                input(&self.title).w_full().disabled(disabled),
                None,
            ))
            .child(modal::modal_field(
                "Description",
                textarea(&self.body).h(px(150.)).w_full().disabled(disabled),
                None,
            ))
            .child(modal::modal_field(
                "Target branch",
                dropdown(&self.target).w_full().disabled(disabled),
                (!self.has_targets)
                    .then(|| "This repository has no other local branch to merge into.".into()),
            ))
            .child(
                labeled_checkbox("pull-request-draft", self.draft, "Create as draft")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                        this.draft = *value;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child(format!(
                        "{} is pushed first; without an upstream it is published to origin.",
                        self.branch.name
                    )),
            )
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_size(px(12.)).text_color(t::red()).child(error)),
            )
            .child(modal::modal_actions(
                button("cancel-create-pull-request", "Cancel")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                primary_button(
                    "confirm-create-pull-request",
                    match phase.flatten() {
                        Some(Phase::Pushing) => "Pushing…",
                        Some(Phase::Creating) => "Creating…",
                        None => "Create pull request",
                    },
                )
                .disabled(disabled || !self.has_targets)
                .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
            ));
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(false)
            .on_cancel(move |e, w, cx| {
                cancel(e, w, cx);
                false
            })
            .backdrop(modal::modal_backdrop(progress))
            .popup(panel)
    }
}

pub struct PullRequestDetails {
    pull: PullRef,
    description: Entity<MarkdownView>,
    focus: FocusHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    _description: Subscription,
}
impl EventEmitter<ModalDismissed> for PullRequestDetails {}

impl PullRequestDetails {
    pub fn new(
        pull: PullRef,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let description = cx.new(MarkdownView::reading);
        description.update(cx, |view, cx| {
            view.set_content(pull.body.trim().into(), pull.url.clone(), true, cx)
        });
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        let focus = cx.focus_handle();
        let initial = focus.clone();
        window.on_next_frame(move |window, cx| initial.focus(window, cx));
        Self {
            _description: cx.observe(&description, |_, _, cx| cx.notify()),
            pull,
            description,
            focus,
            return_focus,
            presence,
            closing: false,
            scheduled: false,
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
}

impl Render for PullRequestDetails {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animated = self.presence.is_animating(now);
        motion::request_frame(window, animated);
        if self.closing && !animated && !self.scheduled {
            self.scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.return_focus.focus(window, cx);
                    cx.emit(ModalDismissed);
                });
            });
        }
        let pull = &self.pull;
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(0., 600.);
        let max_height = (f32::from(window.viewport_size().height) - 144.).max(240.);
        let url = pull.url.clone();
        let empty = pull.body.trim().is_empty();
        let cut = pull.body.chars().count() >= BODY_LIMIT;
        let panel =
            modal::modal_surface("pull-request-dialog")
                .absolute()
                .w(px(width))
                .max_h(px(max_height))
                .left((window.viewport_size().width - px(width)) / 2.)
                .top(px(72. + motion::distance::BASE * (1. - progress)))
                .opacity(progress)
                .child(
                    row()
                        .gap(px(10.))
                        .items_start()
                        .child(
                            custom_icon(state_icon(&pull.state))
                                .size(px(16.))
                                .mt(px(2.))
                                .text_color(state_color(&pull.state)),
                        )
                        .child(
                            column()
                                .flex_1()
                                .gap(px(4.))
                                .child(
                                    row()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(px(14.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(format!("#{} {}", pull.number, pull.title)),
                                        )
                                        .child(state_chip(&pull.state)),
                                )
                                .child(
                                    row()
                                        .gap(px(6.))
                                        .text_size(px(12.))
                                        .text_color(t::faint())
                                        .child("from")
                                        .child(branch_chip(pull.head.clone()))
                                        .child("into")
                                        .child(branch_chip(pull.base.clone())),
                                )
                                .when(!pull.author.is_empty(), |column| {
                                    column.child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(t::muted())
                                            .child(format!("Created by {}", pull.author)),
                                    )
                                }),
                        )
                        .child(
                            icon_button("close-pull-request", IconName::Close, "Close dialog")
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        ),
                )
                .child(caption("DESCRIPTION"))
                .child(
                    div()
                        .id("pull-request-description")
                        .flex_1()
                        .min_h(px(48.))
                        .overflow_y_scroll()
                        .px(px(12.))
                        .py(px(10.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(t::border())
                        .bg(t::input_bg())
                        .text_size(px(12.))
                        .map(|body| {
                            if empty {
                                body.text_color(t::faint()).child("No description.")
                            } else {
                                body.child(self.description.clone())
                            }
                        }),
                )
                .when(cut, |panel| {
                    panel.child(div().text_size(px(11.)).text_color(t::muted()).child(
                        "The description is shortened. Open it on GitHub to read all of it.",
                    ))
                })
                .child(modal::modal_actions(
                    button("close-pull-request-details", "Close")
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    primary_button("open-pull-request", "Open on GitHub")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                ));
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(false)
            .on_cancel(move |e, w, cx| {
                cancel(e, w, cx);
                false
            })
            .backdrop(modal::modal_backdrop(progress))
            .popup(panel)
    }
}
