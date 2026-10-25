//! A project is grouped by origin; every child retains its host-scoped identity.
use super::*;
use canopy_desktop::{
    git::RepositoryIdentity,
    remote::{
        client::{HostClient, Phase},
        proto as p,
    },
};
use gpui_kit::component::{Selectable, Sizable, button::ButtonVariants};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub enum Navigation {
    Local,
    Remote {
        host: Arc<HostClient>,
        resource: p::ResourceRef,
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
    pub(crate) fn local_selection(&mut self, cx: &mut Context<Self>) {
        self.remote_selection = None;
        cx.emit(Navigation::Local);
        cx.notify();
    }

    fn remote_row(
        &self,
        host: Arc<HostClient>,
        resource: p::ResourceRef,
        title: String,
        status: &str,
        nested: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let key = match &resource.resource {
            Some(p::resource_ref::Resource::Workspace(w)) => {
                format!("remote-worktree:{}:{}", w.host_id, w.workspace_id)
            }
            Some(p::resource_ref::Resource::Chat(c)) => {
                format!("remote-chat:{}:{}", c.host_id, c.chat_id)
            }
            Some(p::resource_ref::Resource::Terminal(t)) => {
                format!("remote-terminal:{}:{}", t.host_id, t.terminal_id)
            }
            _ => unreachable!("sidebar resources"),
        };
        let selected = self.remote_selection.as_ref() == Some(&resource);
        let phase = host.phase();
        let online = phase == Phase::Ready;
        let environment = host.name();
        let state = match phase {
            Phase::Ready => status,
            Phase::Connecting => "Connecting…",
            Phase::Offline => "Offline · cached",
        };
        Button::new(SharedString::from(key))
            .ghost()
            .small()
            .compact()
            .selected(selected)
            .w_full()
            .min_w_0()
            .h(if nested {
                t::chat_row_height()
            } else {
                rems(2.)
            })
            .pl(if nested { rems(2.) } else { rems(1.25) })
            .pr_2()
            .rounded(t::sidebar_row_radius())
            .accessibility_label(format!("{title}, {environment}, {state}"))
            .tooltip(format!("{title} · {environment} · {state}"))
            .child(
                row()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .child(
                        Icon::default()
                            .path(if nested {
                                "canopy/message-circle-plus.svg"
                            } else {
                                "canopy/git-branch.svg"
                            })
                            .size_3p5()
                            .text_color(t::faint()),
                    )
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .items_start()
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_sm()
                                    .text_color(t::text())
                                    .child(title),
                            )
                            .when(nested, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .truncate()
                                        .text_xs()
                                        .text_color(t::muted())
                                        .child(format!("{environment} · {state}")),
                                )
                            }),
                    )
                    .when(!nested, |d| {
                        d.child(
                            div()
                                .max_w(rems(7.))
                                .truncate()
                                .text_xs()
                                .text_color(t::muted())
                                .child(if online {
                                    environment
                                } else {
                                    format!(
                                        "{environment} · {}",
                                        if phase == Phase::Connecting {
                                            "Connecting…"
                                        } else {
                                            "Offline"
                                        }
                                    )
                                }),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.remote_selection = Some(resource.clone());
                cx.emit(Navigation::Remote {
                    host: host.clone(),
                    resource: resource.clone(),
                });
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn remote_catalogs(&self, cx: &App) -> Vec<(Arc<HostClient>, Arc<p::Catalog>)> {
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
            .filter_map(|host| {
                host.shared_catalog()
                    .filter(|c| c.host_id == host.id().as_str())
                    .map(|c| (host, c))
            })
            .collect()
    }

    pub(super) fn remote_projects(
        &self,
        groups: &mut ProjectRows,
        now: Instant,
        cx: &Context<Self>,
    ) {
        for (host, catalog) in self.remote_catalogs(cx) {
            let mut chats = std::collections::HashMap::<&str, Vec<&p::ChatSummary>>::new();
            for chat in &catalog.chats {
                if let Some(workspace) = &chat.workspace {
                    chats
                        .entry(workspace.workspace_id.as_str())
                        .or_default()
                        .push(chat);
                }
            }
            let mut terminals = std::collections::HashMap::<&str, Vec<&p::TerminalSummary>>::new();
            for terminal in &catalog.terminals {
                if let Some(workspace) = &terminal.workspace {
                    terminals
                        .entry(workspace.workspace_id.as_str())
                        .or_default()
                        .push(terminal);
                }
            }
            for project in &catalog.projects {
                let origin = project
                    .origin
                    .as_deref()
                    .and_then(RepositoryIdentity::from_origin);
                let key = project_key(origin.as_ref(), host.id().as_str(), &project.project_id);
                let Some(disclosure) = self.project_groups.get(&key) else {
                    continue;
                };
                let title = origin
                    .as_ref()
                    .map_or_else(|| project.name.clone(), |o| o.path().to_owned());
                let toggle = key.clone();
                let header = Button::new(SharedString::from(format!("project-{key}")))
                    .ghost()
                    .small()
                    .compact()
                    .w_full()
                    .min_w_0()
                    .h_8()
                    .accessibility_label(format!(
                        "{} {title}",
                        if disclosure.open {
                            "Collapse"
                        } else {
                            "Expand"
                        }
                    ))
                    .tooltip(
                        origin
                            .as_ref()
                            .map_or_else(|| title.clone(), RepositoryIdentity::canonical_url),
                    )
                    .child(
                        row()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .child(disclosure.chevron(now))
                            .child(super::activity::project_tile(&title))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.project_groups.get_mut(&toggle) {
                            d.toggle(Instant::now(), cx);
                        }
                        cx.notify();
                    }))
                    .into_any_element();
                let (_, rows) = groups.entry(key).or_insert_with(|| (header, vec![]));
                if !disclosure.open && !disclosure.active(now) {
                    continue;
                }
                for checkout in &project.checkouts {
                    let Some(workspace) = checkout
                        .workspace
                        .clone()
                        .filter(|w| w.host_id == host.id().as_str())
                    else {
                        continue;
                    };
                    let title = checkout
                        .branch
                        .clone()
                        .unwrap_or_else(|| checkout.name.clone());
                    let state = if checkout.closing.unwrap_or(false) {
                        "Closing"
                    } else {
                        "Worktree"
                    };
                    rows.push(self.remote_row(
                        host.clone(),
                        p::ResourceRef {
                            resource: Some(p::resource_ref::Resource::Workspace(workspace.clone())),
                        },
                        title,
                        state,
                        false,
                        cx,
                    ));
                    for chat in chats
                        .get(workspace.workspace_id.as_str())
                        .into_iter()
                        .flatten()
                        .filter(|chat| chat.workspace.as_ref() == Some(&workspace))
                    {
                        self.push_remote_chat(&host, chat, rows, cx);
                    }
                    for terminal in terminals
                        .get(workspace.workspace_id.as_str())
                        .into_iter()
                        .flatten()
                        .filter(|t| t.workspace.as_ref() == Some(&workspace))
                    {
                        if let Some(reference) = terminal
                            .terminal
                            .clone()
                            .filter(|t| t.host_id == host.id().as_str())
                        {
                            rows.push(self.remote_row(
                                host.clone(),
                                p::ResourceRef {
                                    resource: Some(p::resource_ref::Resource::Terminal(reference)),
                                },
                                terminal.title.clone(),
                                "Terminal",
                                true,
                                cx,
                            ));
                        }
                    }
                }
            }
        }
    }

    fn push_remote_chat(
        &self,
        host: &Arc<HostClient>,
        chat: &p::ChatSummary,
        rows: &mut Vec<AnyElement>,
        cx: &Context<Self>,
    ) {
        if let Some(reference) = chat
            .chat
            .clone()
            .filter(|c| c.host_id == host.id().as_str())
        {
            let status = match p::ChatState::try_from(chat.state).ok() {
                Some(p::ChatState::Working) => "Working",
                Some(p::ChatState::NeedsDecision) => "Needs approval",
                Some(p::ChatState::ChatFailed) => "Error",
                Some(p::ChatState::ChatStarting) => "Starting",
                _ => "Chat",
            };
            rows.push(self.remote_row(
                host.clone(),
                p::ResourceRef {
                    resource: Some(p::resource_ref::Resource::Chat(reference)),
                },
                chat.title.clone(),
                status,
                true,
                cx,
            ));
        }
    }

    pub(super) fn remote_private_chats(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let mut rows = vec![];
        for (host, catalog) in self.remote_catalogs(cx) {
            for chat in catalog.chats.iter().filter(|chat| chat.workspace.is_none()) {
                self.push_remote_chat(&host, chat, &mut rows, cx);
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
