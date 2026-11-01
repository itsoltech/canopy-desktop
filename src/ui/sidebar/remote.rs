//! A project is grouped by origin; every child retains its host-scoped identity.
use super::activity::{self, Activity, Tally};
use super::chats::{SessionRow, context::ChatContext};
use super::projects::{ProjectHeader, WorktreeRow};
use super::*;
use crate::ui::components::hover_card::content as card;
use crate::ui::remote_client::ResourceView;
use canopy_desktop::api::proto as p;
use canopy_desktop::chat::provider::Provider;
use canopy_desktop::{
    git::RepositoryIdentity,
    remote::client::{HostClient, Phase},
};
use gpui_kit::component::{Sizable, button::ButtonVariants};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

#[derive(Clone)]
#[expect(
    clippy::large_enum_variant,
    reason = "Navigation carries one selected checkout by value for an application event."
)]
pub enum Navigation {
    Local,
    Remote {
        host: Arc<HostClient>,
        resource: ResourceView,
    },
}
impl EventEmitter<Navigation> for Sidebar {}

pub(super) type ProjectRows = BTreeMap<String, (AnyElement, Vec<AnyElement>)>;

pub(super) fn project_key(
    origin: Option<&RepositoryIdentity>,
    host: &str,
    project: &str,
) -> String {
    origin.map_or_else(
        || format!("checkout:{host}:{project}"),
        |o| format!("origin:{}", o.canonical_url()),
    )
}

impl Sidebar {
    pub(super) fn sync_chat_selection(&mut self, cx: &mut Context<Self>) {
        let workspace = cx.global::<crate::app_state::AppState>().workspace.read(cx);
        let pane = workspace
            .active()
            .and_then(|tab| tab.root.find(tab.focused));
        let id = pane.map(|pane| pane.id);
        if self.selected_pane != id {
            self.selected_pane = id;
            self.remote_selection = pane
                .and_then(|pane| pane.metadata.remote.as_ref())
                .map(|chat| ResourceView::Thread(chat.as_ref().clone()));
            cx.emit(Navigation::Local);
        }
        cx.notify();
    }

    pub(crate) fn local_selection(&mut self, cx: &mut Context<Self>) {
        self.remote_selection = None;
        cx.emit(Navigation::Local);
        cx.notify();
    }

    pub(super) fn remote_catalogs(&self, cx: &App) -> Vec<(Arc<HostClient>, Arc<p::Catalog>)> {
        self.remote_hosts(cx)
            .into_iter()
            .filter_map(|host| {
                host.shared_catalog()
                    .filter(|c| {
                        c.environment
                            .as_ref()
                            .is_some_and(|e| e.host_id == host.id().as_str())
                    })
                    .map(|c| (host, c))
            })
            .collect()
    }

    pub(super) fn remote_hosts(&self, cx: &App) -> Vec<Arc<HostClient>> {
        let local = cx
            .global::<crate::app_state::AppState>()
            .settings
            .read(cx)
            .host_id();
        self.connections
            .as_ref()
            .map(|connections| connections.read(cx).hosts())
            .unwrap_or_default()
            .into_iter()
            .filter(|host| Some(host.id().as_str()) != local)
            .collect()
    }

    /// Hosts without shared projects to list: one compact row each, the
    /// details in its hover card.
    pub(super) fn remote_connection_messages(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let mut rows = vec![];
        for host in self.remote_hosts(cx) {
            let phase = host.phase();
            let catalog = host.shared_catalog();
            if phase == Phase::Ready && catalog.as_ref().is_some_and(|c| !c.projects.is_empty()) {
                continue;
            }
            let name = host.name();
            let (state, color, detail) = match phase {
                Phase::Offline => (
                    "Offline",
                    t::faint(),
                    "Connect to see the projects it shares.",
                ),
                Phase::Connecting => ("Connecting…", t::muted(), "Loading shared projects."),
                Phase::Ready if catalog.is_none() => {
                    ("Loading…", t::muted(), "Loading shared projects.")
                }
                Phase::Ready => (
                    "No projects",
                    t::muted(),
                    "Open a project there, or check this device's permissions in Connections.",
                ),
            };
            let id = host.id().as_str().to_owned();
            let error = host.connection_error();
            let key = format!("remote-host-{id}");
            let group = SharedString::from(format!("group-{key}"));
            let connect = host.clone();
            let card_name = name.clone();
            rows.push(
                row()
                    .id(SharedString::from(key.clone()))
                    .hover_card(
                        key.clone(),
                        gpui_kit::base::Placement::Right,
                        move |_, _| {
                            card::body()
                                .child(
                                    column()
                                        .gap_0p5()
                                        .child(card::title(card_name.clone()))
                                        .child(card::status(color, state, detail, t::muted())),
                                )
                                .children(
                                    error
                                        .clone()
                                        .map(|error| card::facts().child(card::error(error))),
                                )
                                .into_any_element()
                        },
                    )
                    .group(group.clone())
                    .pl(rems(0.375))
                    .pr_1()
                    .gap_1p5()
                    .h_8()
                    .rounded(t::sidebar_row_radius())
                    .hover(|s| s.bg(t::hover()))
                    .child(
                        Icon::default()
                            .path("canopy/monitor.svg")
                            .size_3p5()
                            .flex_shrink_0()
                            .text_color(t::faint()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(t::secondary())
                            .child(name.clone()),
                    )
                    .child(
                        row()
                            .flex_shrink_0()
                            .gap_1()
                            .text_xs()
                            .text_color(t::muted())
                            .child(div().size(rems(0.375)).rounded_full().bg(
                                if host.connection_error().is_some() {
                                    t::red()
                                } else {
                                    color
                                },
                            ))
                            .child(state),
                    )
                    .when(phase == Phase::Offline, |row| {
                        row.child(
                            div()
                                .id(SharedString::from(format!("aside-{key}")))
                                .flex_shrink_0()
                                .hides_hover_card()
                                .child(
                                    Button::new(SharedString::from(format!("remote-connect-{id}")))
                                        .ghost()
                                        .xsmall()
                                        .compact()
                                        .label("Connect")
                                        .accessibility_label(format!("Connect to {name}"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.remote_error =
                                                super::super::remote_client::open_host(
                                                    connect.clone(),
                                                    cx,
                                                )
                                                .err()
                                                .map(|error| error.to_string());
                                            cx.notify();
                                        })),
                                ),
                        )
                    })
                    .into_any_element(),
            );
        }
        if let Some(error) = &self.remote_error {
            rows.push(
                div()
                    .px_2()
                    .text_xs()
                    .text_color(t::red())
                    .child(error.clone())
                    .into_any_element(),
            );
        }
        rows
    }

    fn navigate(&mut self, host: Arc<HostClient>, resource: ResourceView, cx: &mut Context<Self>) {
        self.remote_selection = Some(resource.clone());
        cx.emit(Navigation::Remote { host, resource });
        cx.notify();
    }

    pub(super) fn remote_projects(
        &self,
        groups: &mut ProjectRows,
        now: Instant,
        cx: &Context<Self>,
    ) {
        for (host, catalog) in self.remote_catalogs(cx) {
            let mut chats = std::collections::HashMap::<&str, Vec<&p::CatalogThread>>::new();
            for chat in &catalog.threads {
                if let Some(workspace) = &chat.project_catalog_id {
                    chats.entry(workspace.as_str()).or_default().push(chat);
                }
            }
            let mut terminals = std::collections::HashMap::<&str, Vec<&p::TerminalSummary>>::new();
            for terminal in &catalog.terminals {
                if !terminal.project_catalog_id.is_empty() {
                    terminals
                        .entry(terminal.project_catalog_id.as_str())
                        .or_default()
                        .push(terminal);
                }
            }
            let name = host.name();
            for project in &catalog.projects {
                let origin = project
                    .origin
                    .as_deref()
                    .and_then(RepositoryIdentity::from_origin);
                let key = project_key(origin.as_ref(), host.id().as_str(), &project.path);
                let Some(disclosure) = self.project_groups.get(&key) else {
                    continue;
                };
                let title = origin
                    .as_ref()
                    .map_or_else(|| project.name.clone(), |o| o.path().to_owned());
                let shared = |workspace: &p::ProjectCheckout| -> Vec<&p::CatalogThread> {
                    chats
                        .get(workspace.catalog_id.as_str())
                        .into_iter()
                        .flatten()
                        .copied()
                        .collect()
                };
                let checkouts: Vec<_> = project
                    .checkouts
                    .iter()
                    .filter_map(|c| {
                        c.project
                            .as_ref()
                            .filter(|w| w.host_id == host.id().as_str())?;
                        Some((c, c.clone()))
                    })
                    .collect();
                let tally = Tally::count(
                    checkouts
                        .iter()
                        .flat_map(|(_, w)| shared(w))
                        .filter_map(|chat| chat.summary.as_ref())
                        .map(|chat| Activity::remote(chat.state, chat.goal.is_some())),
                );
                let header = self.project_header(
                    ProjectHeader {
                        key: format!("project-{key}"),
                        group: key.clone(),
                        label: title.clone(),
                        tally,
                        card: Rc::new(super::cards::ProjectCard {
                            name: title.clone(),
                            repository: Ok((checkouts.len(), 0)),
                            folder: super::cards::Folder::new(project.path.clone().into()),
                            sessions: super::cards::Sessions::Remote(tally),
                            hosts: vec![name.clone()],
                            error: host.connection_error(),
                        }),
                        menu: None,
                        blocked: false,
                    },
                    disclosure,
                    now,
                    cx,
                );
                let (_, rows) = groups.entry(key).or_insert_with(|| (header, vec![]));
                if !disclosure.open && !disclosure.active(now) {
                    continue;
                }
                for (checkout, workspace) in checkouts {
                    let label = checkout
                        .branch
                        .clone()
                        .unwrap_or_else(|| checkout.name.clone());
                    let main = checkout.repository_path.as_deref() == Some(checkout.path.as_str());
                    let context = ChatContext {
                        project: title.clone(),
                        checkout: label.clone(),
                        folder: Some(checkout.path.clone().into()),
                        head: Some(label.clone()),
                        main_checkout: main,
                        git_error: None,
                    };
                    let mut sessions = vec![];
                    for chat in shared(&workspace) {
                        sessions.extend(self.remote_chat(
                            &host,
                            chat,
                            &context,
                            sessions.len(),
                            cx,
                        ));
                    }
                    for terminal in terminals
                        .get(workspace.catalog_id.as_str())
                        .into_iter()
                        .flatten()
                        .filter(|t| t.project_catalog_id == workspace.catalog_id)
                    {
                        sessions.extend(self.remote_terminal(
                            &host,
                            terminal,
                            &context,
                            sessions.len(),
                            cx,
                        ));
                    }
                    let tally = Tally::count(
                        shared(&workspace)
                            .into_iter()
                            .filter_map(|chat| chat.summary.as_ref())
                            .map(|chat| Activity::remote(chat.state, chat.goal.is_some())),
                    );
                    let resource = ResourceView::Project(workspace.clone());
                    let selected = self.remote_selection.as_ref() == Some(&resource);
                    let target = host.clone();
                    rows.push(self.worktree_row(
                        WorktreeRow {
                            key: format!(
                                "worktree-remote-{}-{}",
                                host.id().as_str(),
                                workspace.catalog_id
                            ),
                            group: format!(
                                "remote:{}:{}",
                                host.id().as_str(),
                                workspace.catalog_id
                            ),
                            label: label.clone(),
                            main,
                            selected,
                            disabled: checkout.closing,
                            host: Some(name.clone()),
                            card: Rc::new(super::cards::WorktreeCard {
                                head: label,
                                main,
                                current: selected,
                                locked: false,
                                missing: false,
                                available: host.phase() == Phase::Ready,
                                project: title.clone(),
                                folder: super::cards::Folder::new(checkout.path.clone().into()),
                                sessions: super::cards::Sessions::Remote(tally),
                                host: Some(name.clone()),
                            }),
                            on_click: Rc::new(move |this, cx| {
                                this.navigate(target.clone(), resource.clone(), cx)
                            }),
                            terminal: None,
                            menu: None,
                            tally: Some(tally),
                            chats: Some(super::chats::session_rail(sessions)),
                        },
                        now,
                        cx,
                    ));
                }
            }
        }
    }

    /// What a remote session's row says while its computer isn't connected.
    fn remote_status(host: &HostClient, activity: Activity) -> (&'static str, Hsla) {
        match host.phase() {
            Phase::Ready => (activity.label(), activity.color()),
            Phase::Connecting => ("Connecting…", t::muted()),
            Phase::Offline => ("Offline", t::faint()),
        }
    }

    fn remote_chat(
        &self,
        host: &Arc<HostClient>,
        catalog_thread: &p::CatalogThread,
        context: &ChatContext,
        index: usize,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let reference = catalog_thread
            .thread
            .clone()
            .filter(|c| c.host_id == host.id().as_str())?;
        let chat = catalog_thread.summary.as_ref()?;
        let provider = Provider::from_tool(&chat.agent_id);
        let agent = provider.map_or(chat.agent_id.clone(), |p| p.name().to_owned());
        let activity = Activity::remote(chat.state, chat.goal.is_some());
        let name = host.name();
        let resource = ResourceView::Thread(canopy_desktop::state::workspace::RemoteChat::new(
            reference.host_id.clone(),
            catalog_thread.catalog_id.clone(),
        )?);
        let selected = self.remote_selection.as_ref() == Some(&resource);
        let status = Self::remote_status(host, activity);
        let target = host.clone();
        Some(self.session_row(
            SessionRow {
                key: format!("remote-{}-{}", reference.host_id, catalog_thread.catalog_id),
                title: chat.title.clone(),
                mark: activity::provider_mark(provider),
                activity,
                status,
                detail: Some(name.clone()),
                cli: false,
                host: true,
                selected,
                enabled: true,
                index,
                label: format!(
                    "{}, {}, {}, {agent}, {}, on {name}{}",
                    context.project,
                    chat.title,
                    context.checkout,
                    status.0,
                    if selected { ", selected" } else { "" }
                ),
                card: Rc::new(super::cards::SessionCard {
                    id: None,
                    activity,
                    title: chat.title.clone(),
                    mark: activity::provider_mark(provider),
                    agent,
                    model: None,
                    cli: false,
                    folder: super::cards::Folder::new(context.folder.clone().unwrap_or_default()),
                    context: context.clone(),
                    host: Some(name),
                }),
                on_click: Rc::new(move |this, cx| {
                    this.navigate(target.clone(), resource.clone(), cx)
                }),
                menu: None,
            },
            cx,
        ))
    }

    fn remote_terminal(
        &self,
        host: &Arc<HostClient>,
        terminal: &p::TerminalSummary,
        context: &ChatContext,
        index: usize,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let reference = terminal
            .terminal
            .clone()
            .filter(|t| t.host_id == host.id().as_str())?;
        let activity = if terminal.running {
            Activity::Idle
        } else {
            Activity::Exited
        };
        let status = match host.phase() {
            Phase::Ready if terminal.running => ("Running", t::secondary()),
            Phase::Ready => ("Exited", t::faint()),
            _ => Self::remote_status(host, activity),
        };
        let name = host.name();
        let resource = ResourceView::Terminal(reference.clone());
        let selected = self.remote_selection.as_ref() == Some(&resource);
        let target = host.clone();
        Some(self.session_row(
            SessionRow {
                key: format!(
                    "remote-terminal-{}-{}",
                    reference.host_id, reference.terminal_id
                ),
                title: terminal.title.clone(),
                mark: activity::terminal_mark(),
                activity,
                status,
                detail: Some(name.clone()),
                cli: false,
                host: true,
                selected,
                enabled: true,
                index,
                label: format!(
                    "{}, {}, terminal, {}, on {name}{}",
                    context.project,
                    terminal.title,
                    status.0,
                    if selected { ", selected" } else { "" }
                ),
                card: Rc::new(super::cards::SessionCard {
                    id: None,
                    activity,
                    title: terminal.title.clone(),
                    mark: activity::terminal_mark(),
                    agent: "Terminal".into(),
                    model: None,
                    cli: false,
                    folder: super::cards::Folder::new(context.folder.clone().unwrap_or_default()),
                    context: context.clone(),
                    host: Some(name),
                }),
                on_click: Rc::new(move |this, cx| {
                    this.navigate(target.clone(), resource.clone(), cx)
                }),
                menu: None,
            },
            cx,
        ))
    }

    /// Private chats of other computers, listed after this computer's own.
    pub(super) fn remote_private_chats(&self, first: usize, cx: &Context<Self>) -> Vec<AnyElement> {
        let private = ChatContext {
            project: "Without a project".into(),
            checkout: "Private folder".into(),
            folder: None,
            head: None,
            main_checkout: false,
            git_error: None,
        };
        let mut rows = vec![];
        for (host, catalog) in self.remote_catalogs(cx) {
            for chat in catalog
                .threads
                .iter()
                .filter(|chat| chat.project_catalog_id.is_none())
            {
                rows.extend(self.remote_chat(&host, chat, &private, first + rows.len(), cx));
            }
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::project_key;
    use canopy_desktop::git::RepositoryIdentity;
    #[test]
    fn matching_origins_merge_only_the_project_not_its_host_scoped_children() {
        let a =
            RepositoryIdentity::from_origin("git@github.com:itsoltech/canopy-desktop.git").unwrap();
        let b =
            RepositoryIdentity::from_origin("https://github.com/itsoltech/canopy-desktop").unwrap();
        assert_eq!(
            project_key(Some(&a), "home", "/a"),
            project_key(Some(&b), "office", "/b")
        );
        assert_ne!(
            project_key(None, "home", "/a"),
            project_key(None, "office", "/a")
        );
    }
}
