//! Hover cards of the sidebar tree: what a project, worktree or session is and
//! how its agents are doing, one icon-led line per fact.
use super::activity::{Activity, Tally, project_tile};
use super::chats::context::ChatContext;
use super::projects::tag;
use crate::ui::components::hover_card::content as card;
use crate::ui::{components::*, theme as t};
use canopy_desktop::state::workspace::{PaneId, WorkspaceId};
use gpui_kit::component::{Icon, IconName};
use gpui_kit::{prelude::FluentBuilder, *};
use std::cell::OnceCell;
use std::path::{Path, PathBuf};

pub(super) struct SessionCard {
    /// The local pane, read live; a remote session shows `activity` instead.
    pub id: Option<PaneId>,
    pub activity: Activity,
    pub title: String,
    pub mark: (Hsla, &'static str),
    pub agent: String,
    pub model: Option<String>,
    pub cli: bool,
    pub context: ChatContext,
    pub folder: Folder,
    /// The computer the session runs on, when it isn't this one.
    pub host: Option<String>,
}

impl SessionCard {
    pub fn render(&self, cx: &App) -> Div {
        // Read when shown, so the card follows the agent while it stays open.
        let activity = self.id.map_or(self.activity, |id| Activity::of(id, cx));
        let context = &self.context;
        let (mark, path) = self.mark;
        card::body()
            .child(
                column()
                    .gap_0p5()
                    .child(card::title(self.title.clone()))
                    .child(card::status(
                        activity.color(),
                        activity.label(),
                        activity.detail(),
                        activity.color(),
                    )),
            )
            .child(card::separator())
            .child(
                card::facts()
                    .child(
                        card::fact(Icon::default().path(path).text_color(mark))
                            .child(
                                row()
                                    .min_w_0()
                                    .gap_1()
                                    .child(div().flex_shrink_0().child(self.agent.clone()))
                                    .children(self.model.clone().map(|model| {
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_color(t::muted())
                                            .child(format!("· {model}"))
                                    })),
                            )
                            .when(self.cli, |line| {
                                line.child(
                                    row()
                                        .ml_auto()
                                        .flex_shrink_0()
                                        .gap_1()
                                        .text_color(t::faint())
                                        .child(
                                            Icon::default()
                                                .path("canopy/square-terminal.svg")
                                                .size_3(),
                                        )
                                        .child("Terminal"),
                                )
                            }),
                    )
                    .children(self.host.clone().map(host))
                    .child(match &context.folder {
                        None => card::fact(Icon::default().path("canopy/box.svg"))
                            .text_color(t::muted())
                            .child("Private chat, no project"),
                        Some(_) => card::fact(Icon::default().path("canopy/box.svg"))
                            .child(context.project.clone()),
                    })
                    .when(context.folder.is_some(), |facts| {
                        facts.child(match &context.head {
                            Some(head) => card::fact(Icon::default().path("canopy/git-branch.svg"))
                                .child(div().min_w_0().truncate().child(head.clone()))
                                .when(context.main_checkout, |line| line.child(tag("default"))),
                            None => card::fact(Icon::default().path("canopy/git-branch.svg"))
                                .text_color(t::muted())
                                .child(context.checkout.clone()),
                        })
                    })
                    .children(context.folder.is_some().then(|| self.folder.fact()))
                    .children(context.git_error.clone().map(card::error)),
            )
    }
}

pub(super) struct WorktreeCard {
    pub head: String,
    pub main: bool,
    pub current: bool,
    pub locked: bool,
    pub missing: bool,
    pub available: bool,
    pub project: String,
    pub folder: Folder,
    pub sessions: Sessions,
    pub host: Option<String>,
}

impl WorktreeCard {
    pub fn render(&self, cx: &App) -> Div {
        let state = if self.missing {
            Some((t::red(), "Missing", "The worktree folder no longer exists"))
        } else if !self.available {
            Some((t::faint(), "Unavailable", "Git cannot read this worktree"))
        } else if self.current {
            Some((t::accent(), "Current worktree", ""))
        } else {
            None
        };
        card::body()
            .child(
                column()
                    .gap_0p5()
                    .child(
                        row()
                            .items_start()
                            .gap_1p5()
                            .child(
                                div().flex_shrink_0().pt(rems(0.1875)).child(
                                    Icon::default()
                                        .path("canopy/git-branch.svg")
                                        .size_3p5()
                                        .text_color(if self.current {
                                            t::accent()
                                        } else {
                                            t::faint()
                                        }),
                                ),
                            )
                            .child(card::title(self.head.clone()).flex_1().min_w_0())
                            .when(self.main, |line| line.child(tag("default").mt_0p5()))
                            .when(self.locked, |line| line.child(tag("locked").mt_0p5())),
                    )
                    .children(state.map(|(color, label, detail)| {
                        card::status(color, label, detail, t::muted())
                    })),
            )
            .child(card::separator())
            .child(
                card::facts()
                    .child(
                        card::fact(Icon::default().path("canopy/box.svg"))
                            .child(self.project.clone()),
                    )
                    .children(self.host.clone().map(host))
                    .child(self.folder.fact())
                    .child(self.sessions.fact(cx)),
            )
    }
}

pub(super) struct ProjectCard {
    pub name: String,
    /// Worktrees and local branches; `Err` says why there is no repository.
    pub repository: Result<(usize, usize), &'static str>,
    pub folder: Folder,
    pub sessions: Sessions,
    /// Other computers sharing this project.
    pub hosts: Vec<String>,
    pub error: Option<String>,
}

impl ProjectCard {
    pub fn render(&self, cx: &App) -> Div {
        let summary = match self.repository {
            Ok((worktrees, branches)) => {
                let worktrees = format!(
                    "{worktrees} {}",
                    if worktrees == 1 {
                        "worktree"
                    } else {
                        "worktrees"
                    }
                );
                match branches {
                    0 => worktrees,
                    1 => format!("{worktrees} · 1 branch"),
                    _ => format!("{worktrees} · {branches} branches"),
                }
            }
            Err(reason) => reason.to_owned(),
        };
        card::body()
            .child(
                row().gap_2().child(project_tile(&self.name)).child(
                    column()
                        .flex_1()
                        .min_w_0()
                        .child(card::title(self.name.clone()).truncate())
                        .child(
                            div()
                                .text_xs()
                                .line_height(t::chat_metadata_line_height())
                                .text_color(t::muted())
                                .child(summary),
                        ),
                ),
            )
            .child(card::separator())
            .child(
                card::facts()
                    .child(self.folder.fact())
                    .when(!self.hosts.is_empty(), |facts| {
                        facts.child(host(self.hosts.join(", ")))
                    })
                    .child(self.sessions.fact(cx))
                    .children(self.error.clone().map(card::error)),
            )
    }
}

/// A folder shown as `~/…`, worked out the first time its card opens rather
/// than on every sidebar render.
pub(super) struct Folder {
    path: PathBuf,
    shown: OnceCell<String>,
}

impl Folder {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            shown: OnceCell::new(),
        }
    }

    fn fact(&self) -> Div {
        // The end of a path names the worktree: elide its middle instead.
        card::fact(Icon::new(IconName::Folder)).child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis_middle()
                .text_color(t::muted())
                .child(self.shown.get_or_init(|| home_relative(&self.path)).clone()),
        )
    }
}

/// Where a card counts its sessions from.
pub(super) enum Sessions {
    Local(Vec<WorkspaceId>),
    /// A remote catalog's chats, counted when the row rendered.
    Remote(Tally),
    /// A worktree never opened in Canopy.
    NotOpened,
}

impl Sessions {
    fn fact(&self, cx: &App) -> Div {
        match self {
            Self::Local(ids) => sessions(super::tally(ids, cx)),
            Self::Remote(tally) => sessions(*tally),
            Self::NotOpened => card::fact(Icon::new(IconName::Bot))
                .text_color(t::muted())
                .child("Not opened yet"),
        }
    }
}

/// The computer a remote row lives on.
fn host(name: String) -> Div {
    card::fact(Icon::default().path("canopy/monitor.svg")).child(format!("On {name}"))
}

/// How many agent sessions there are and how many work or wait for the user.
fn sessions(tally: Tally) -> Div {
    let line = card::fact(Icon::new(IconName::Bot));
    if tally.sessions == 0 {
        return line.text_color(t::muted()).child("No sessions");
    }
    let count = |color: Hsla, text: String| {
        row()
            .flex_shrink_0()
            .gap_1()
            .child(div().size(rems(0.375)).rounded_full().bg(color))
            .child(text)
    };
    line.child(format!(
        "{} {}",
        tally.sessions,
        if tally.sessions == 1 {
            "session"
        } else {
            "sessions"
        }
    ))
    .when(tally.working > 0, |line| {
        line.child(count(t::accent(), format!("{} working", tally.working)))
    })
    .when(tally.needs_you > 0, |line| {
        line.child(count(
            t::yellow(),
            format!(
                "{} {}",
                tally.needs_you,
                if tally.needs_you == 1 {
                    "needs you"
                } else {
                    "need you"
                }
            ),
        ))
    })
}

/// `~/…` for folders under the home directory.
fn home_relative(path: &Path) -> String {
    canopy_desktop::platform::directories::user_home()
        .ok()
        .and_then(|home| {
            let rest = path.strip_prefix(&home).ok()?;
            Some(if rest.as_os_str().is_empty() {
                "~".to_owned()
            } else {
                format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display())
            })
        })
        .unwrap_or_else(|| canopy_desktop::platform::paths::display(path))
}
