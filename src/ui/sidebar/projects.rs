use super::*;
use crate::ui::worktree_dialog::WorktreeRequest;
use canopy_desktop::git::{RemovalKind, RemoveWorktree};
use gpui_kit::base::{Disableable, Placement};
use gpui_kit::component::{
    Sizable,
    button::{ButtonCustomVariant, ButtonVariants},
    menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem},
};
use std::rc::Rc;
impl EventEmitter<WorktreeRequest> for Sidebar {}
impl Sidebar {
    fn stop_workspace(id: canopy_desktop::state::workspace::WorkspaceId, cx: &mut App) {
        let app = cx.global::<crate::app_state::AppState>().clone();
        app.terminals
            .update(cx, |terminals, cx| terminals.request_stop_workspace(id, cx));
        app.chats
            .update(cx, |chats, cx| chats.stop_workspace(id, cx))
            .detach();
    }

    pub(super) fn sync_project_groups(&mut self, cx: &mut Context<Self>) {
        let mut roots: std::collections::HashSet<_> = self
            .projects
            .read(cx)
            .catalog
            .items
            .iter()
            .map(|p| {
                let root = p.repository_path.as_ref().unwrap_or(&p.path);
                let origin = self
                    .git
                    .read(cx)
                    .repository(root)
                    .and_then(|r| r.origin.as_ref());
                super::remote::project_key(origin, "local", &root.to_string_lossy())
            })
            .collect();
        for (host, catalog) in self.remote_catalogs(cx) {
            for project in &catalog.projects {
                let origin = project
                    .origin
                    .as_deref()
                    .and_then(canopy_desktop::git::RepositoryIdentity::from_origin);
                roots.insert(super::remote::project_key(
                    origin.as_ref(),
                    host.id().as_str(),
                    &project.project_id,
                ));
            }
        }
        self.project_groups.retain(|root, _| roots.contains(root));
        for root in roots {
            self.project_groups
                .entry(root)
                .or_insert_with(|| Disclosure::new(true, Instant::now()));
        }
        let catalog = &self.projects.read(cx).catalog;
        if self.selected_workspace != catalog.active {
            self.selected_workspace = catalog.active;
            if let Some(project) = catalog.current() {
                if let Some(group) = self.worktree_groups.get_mut(&project.path)
                    && !group.open
                {
                    group.toggle(Instant::now(), cx);
                }
                let root = project.repository_path.as_ref().unwrap_or(&project.path);
                let origin = self
                    .git
                    .read(cx)
                    .repository(root)
                    .and_then(|r| r.origin.as_ref());
                let key = super::remote::project_key(origin, "local", &root.to_string_lossy());
                if let Some(group) = self.project_groups.get_mut(&key)
                    && !group.open
                {
                    group.toggle(Instant::now(), cx);
                }
            }
        }
        cx.notify();
    }
    pub(super) fn projects_content(&self, now: Instant, cx: &Context<Self>) -> Div {
        let state = self.projects.read(cx);
        let git = self.git.read(cx);
        let app = cx.global::<crate::app_state::AppState>();
        let running = app.running_workspaces(cx);
        let mut seen = std::collections::HashSet::new();
        let mut trees = super::remote::ProjectRows::new();
        for project in &state.catalog.items {
            let root = project
                .repository_path
                .as_ref()
                .unwrap_or(&project.path)
                .clone();
            if !seen.insert(root.clone()) {
                continue;
            }
            let info = git.repository(&root).cloned();
            let identity = super::remote::project_key(
                info.as_ref().and_then(|r| r.origin.as_ref()),
                "local",
                &root.to_string_lossy(),
            );
            let Some(disclosure) = self.project_groups.get(&identity) else {
                continue;
            };
            let label = info
                .as_ref()
                .and_then(|r| r.origin.as_ref())
                .map(|o| o.path().to_owned())
                .unwrap_or_else(|| {
                    root.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
            let key = format!("project-{identity}");
            let toggle_root = identity.clone();
            let close_root = root.clone();
            let ids: Vec<_> = state
                .catalog
                .items
                .iter()
                .filter(|p| p.repository_path.as_ref().unwrap_or(&p.path) == &root)
                .map(|p| p.workspace)
                .collect();
            let tally = (!disclosure.open)
                .then(|| super::tally(&ids, cx).pips(&format!("tally-{key}"), false))
                .flatten();
            let owner = cx.entity().downgrade();
            let repo = info.clone();
            let blocked = git.busy || state.busy || !self.sections[0].open;
            let items = Rc::new(
                move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                    let create = owner.clone();
                    let refresh = owner.clone();
                    let close = owner.clone();
                    let repo = repo.clone();
                    let root = close_root.clone();
                    menu.item(
                        PopupMenuItem::new("New worktree…")
                            .disabled(repo.is_none())
                            .on_click(move |_, _, cx| {
                                if let Some(repo) = repo.clone() {
                                    let _ = create
                                        .update(cx, |_, cx| cx.emit(WorktreeRequest::Create(repo)));
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Refresh Git metadata").on_click(move |_, _, cx| {
                            let _ = refresh
                                .update(cx, |s, cx| s.git.update(cx, |git, cx| git.refresh(cx)));
                        }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("Close project").on_click(move |_, _, cx| {
                            let _ = close.update(cx, |s, cx| {
                                s.projects.update(cx, |p, cx| p.close_repository(&root, cx))
                            });
                        }),
                    )
                },
            );
            let project_menu = Button::new(SharedString::from(format!("menu-{key}")))
                .ghost()
                .small()
                .compact()
                .size_6()
                .p_0()
                .icon(IconName::Ellipsis)
                .accessibility_label(format!("Actions for {label}"))
                .tooltip("Project actions on this computer")
                .disabled(blocked)
                .dropdown_menu({
                    let items = items.clone();
                    move |menu, window, cx| items(menu, window, cx)
                });
            let entry = git.entries.get(&root);
            let card = Rc::new(super::cards::ProjectCard {
                name: label.clone(),
                repository: match (&info, entry) {
                    (Some(info), _) => Ok((info.worktrees.len(), info.branches.len())),
                    (None, Some(Ok(None))) => Err("Folder without Git"),
                    (None, Some(Err(_))) => Err("Git unavailable"),
                    (None, _) => Err("Reading Git…"),
                },
                folder: super::cards::Folder::new(root.clone()),
                workspaces: ids.clone(),
                error: entry.and_then(|entry| entry.as_ref().err().cloned()),
            });
            let group = SharedString::from(format!("group-{key}"));
            let header = row()
                .id(SharedString::from(format!("header-{key}")))
                .hover_card(key.clone(), Placement::Right, move |_, cx| {
                    card.render(cx).into_any_element()
                })
                .group(group.clone())
                .gap_1()
                .pr_1()
                .min_h(rems(2.))
                .rounded(t::sidebar_row_radius())
                .hover(|s| s.bg(t::hover()))
                .child(
                    Button::new(SharedString::from(key.clone()))
                        .custom(quiet_row(cx))
                        .small()
                        .compact()
                        .min_h(rems(2.))
                        .flex_1()
                        .min_w_0()
                        .pl_1p5()
                        .pr_0()
                        .border_0()
                        .accessibility_label(format!(
                            "{} {label}",
                            if disclosure.open {
                                "Collapse"
                            } else {
                                "Expand"
                            }
                        ))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if let Some(group) = s.project_groups.get_mut(&toggle_root) {
                                group.toggle(Instant::now(), cx);
                                cx.notify();
                            }
                        }))
                        .child(
                            row()
                                .w_full()
                                .gap_2()
                                .child(div().text_color(t::faint()).child(disclosure.chevron(now)))
                                .child(super::activity::project_tile(&label))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(t::text())
                                        .truncate()
                                        .child(label.clone()),
                                ),
                        ),
                )
                .children(tally.map(|pips| aside(&format!("tally-{key}"), pips)))
                .child(reveal_on_hover(&key, &group, false, project_menu));
            let header = if blocked {
                header.into_any_element()
            } else {
                header
                    .context_menu(move |menu, window, cx| items(menu, window, cx))
                    .into_any_element()
            };
            let mut rows = vec![];
            if let Some(info) = info {
                for wt in &info.worktrees {
                    let path = wt.path.clone();
                    let repository = info.root.clone();
                    let workspace = state
                        .catalog
                        .items
                        .iter()
                        .find(|p| p.path == path)
                        .map(|p| p.workspace);
                    let key = format!("worktree-{}", path.display());
                    let selected = self.remote_selection.is_none()
                        && workspace.is_some()
                        && workspace == state.catalog.active;
                    let main = path == info.root;
                    let disclosure = self.worktree_groups.get(&path);
                    let expanded = disclosure.is_none_or(|d| d.open);
                    let collapse_path = path.clone();
                    let open_root = repository.clone();
                    let open_path = path.clone();
                    let terminal_root = repository.clone();
                    let terminal_path = path.clone();
                    let chat_root = repository.clone();
                    let chat_path = path.clone();
                    let menu_root = repository.clone();
                    let menu_path = path.clone();
                    let disabled = blocked || !wt.available;
                    let can_remove = !blocked && !wt.locked && (wt.available || wt.missing);
                    let remove = wt.name.as_ref().map(|name| RemoveWorktree {
                        repository: repository.clone(),
                        path: path.clone(),
                        kind: if wt.missing {
                            RemovalKind::MissingRegistration { name: name.clone() }
                        } else {
                            RemovalKind::Directory
                        },
                    });
                    let stopping =
                        workspace.is_some_and(|id| self.terminals.read(cx).workspace_stopping(id));
                    let active = workspace.is_some_and(|id| running.contains(&id));
                    let owner = cx.entity().downgrade();
                    let items = Rc::new(
                        move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                            let chat = owner.clone();
                            let terminal = owner.clone();
                            let remove_owner = owner.clone();
                            let root = chat_root.clone();
                            let path = chat_path.clone();
                            let tr = menu_root.clone();
                            let tp = menu_path.clone();
                            let request = remove.clone();
                            menu.item(PopupMenuItem::new("New chat").disabled(disabled).on_click(
                                move |_, _, cx| {
                                    let _ = chat.update(cx, |s, cx| {
                                        s.git.update(cx, |g, cx| {
                                            g.open_chat(root.clone(), path.clone(), cx)
                                        })
                                    });
                                },
                            ))
                            .item(
                                PopupMenuItem::new("Open terminal")
                                    .disabled(disabled)
                                    .on_click(move |_, _, cx| {
                                        let _ = terminal.update(cx, |s, cx| {
                                            s.git.update(cx, |g, cx| {
                                                g.open_terminal(tr.clone(), tp.clone(), cx)
                                            })
                                        });
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new(if stopping {
                                    "Stopping processes…"
                                } else {
                                    "Stop processes"
                                })
                                .disabled(blocked || !active || stopping)
                                .on_click(move |_, _, cx| {
                                    if let Some(id) = workspace {
                                        Self::stop_workspace(id, cx);
                                    }
                                }),
                            )
                            .item(
                                PopupMenuItem::new("Remove worktree…")
                                    .disabled(!can_remove || request.is_none())
                                    .on_click(move |_, _, cx| {
                                        if let Some(request) = request.clone() {
                                            let _ = remove_owner.update(cx, |_, cx| {
                                                cx.emit(WorktreeRequest::Remove(request))
                                            });
                                        }
                                    }),
                            )
                        },
                    );
                    let menu = Button::new(SharedString::from(format!("menu-{key}")))
                        .ghost()
                        .small()
                        .compact()
                        .size_6()
                        .p_0()
                        .icon(IconName::Ellipsis)
                        .accessibility_label(format!("Actions for {}", wt.label))
                        .tooltip("Worktree actions")
                        .dropdown_menu({
                            let items = items.clone();
                            move |menu, window, cx| items(menu, window, cx)
                        });
                    let card = Rc::new(super::cards::WorktreeCard {
                        head: if wt.missing {
                            wt.checkout_branch().unwrap_or(&wt.label).to_owned()
                        } else {
                            wt.head.description()
                        },
                        main,
                        current: selected,
                        locked: wt.locked,
                        missing: wt.missing,
                        available: wt.available,
                        project: label.clone(),
                        folder: super::cards::Folder::new(path.clone()),
                        workspace,
                    });
                    let action = Button::new(SharedString::from(key.clone()))
                        .custom(quiet_row(cx))
                        .small()
                        .compact()
                        .h_8()
                        .flex_1()
                        .min_w_0()
                        .px_0()
                        .border_0()
                        .disabled(disabled)
                        .accessibility_label(format!(
                            "{}{}",
                            wt.label,
                            if selected { ", current worktree" } else { "" }
                        ))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.local_selection(cx);
                            s.git.update(cx, |g, cx| {
                                g.open(open_root.clone(), open_path.clone(), cx)
                            })
                        }))
                        .child(
                            row()
                                .w_full()
                                .min_w_0()
                                .gap_1p5()
                                .child(
                                    Icon::default()
                                        .path("canopy/git-branch.svg")
                                        .size_3p5()
                                        .flex_shrink_0()
                                        .text_color(if selected {
                                            t::accent()
                                        } else {
                                            t::faint()
                                        }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_left()
                                        .text_sm()
                                        .text_color(if selected {
                                            t::text()
                                        } else {
                                            t::secondary()
                                        })
                                        .child(branch_label(&wt.label)),
                                )
                                .when(main, |row| row.child(tag("default")))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(t::muted())
                                        .child("This computer"),
                                ),
                        );
                    let group = SharedString::from(format!("group-{key}"));
                    let row = row()
                        .id(SharedString::from(format!("row-{key}")))
                        .hover_card(key.clone(), Placement::Right, move |_, cx| {
                            card.render(cx).into_any_element()
                        })
                        .group(group.clone())
                        .pl_0p5()
                        .pr_1()
                        .gap_0p5()
                        .h_8()
                        .rounded(t::sidebar_row_radius())
                        .hover(|s| s.bg(t::hover()))
                        .child(
                            Button::new(SharedString::from(format!(
                                "expand-worktree-{}",
                                path.display()
                            )))
                            .ghost()
                            .small()
                            .compact()
                            .size_5()
                            .p_0()
                            .icon(
                                disclosure
                                    .map_or_else(
                                        || {
                                            Icon::new(IconName::ChevronRight)
                                                .rotate(radians(std::f32::consts::FRAC_PI_2))
                                        },
                                        |d| d.chevron(now),
                                    )
                                    .size_3()
                                    .text_color(t::faint()),
                            )
                            .accessibility_label(format!(
                                "{} {}",
                                if expanded { "Collapse" } else { "Expand" },
                                wt.label
                            ))
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    let now = Instant::now();
                                    s.worktree_groups
                                        .entry(collapse_path.clone())
                                        .or_insert_with(|| Disclosure::new(true, now))
                                        .toggle(now, cx);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(action)
                        .children(workspace.filter(|_| !expanded).and_then(|id| {
                            super::tally(&[id], cx)
                                .pips(&format!("tally-{key}"), false)
                                .map(|pips| aside(&format!("tally-{key}"), pips))
                        }))
                        .child(reveal_on_hover(
                            &key,
                            &group,
                            selected,
                            row()
                                .gap_0p5()
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "terminal-worktree-{}",
                                        path.display()
                                    )))
                                    .ghost()
                                    .small()
                                    .compact()
                                    .size_6()
                                    .p_0()
                                    .icon(
                                        Icon::default()
                                            .path("canopy/square-terminal.svg")
                                            .size_3p5(),
                                    )
                                    .accessibility_label("Open worktree terminal")
                                    .tooltip("Open worktree terminal")
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.git.update(cx, |g, cx| {
                                            g.open_terminal(
                                                terminal_root.clone(),
                                                terminal_path.clone(),
                                                cx,
                                            )
                                        })
                                    })),
                                )
                                .child(menu),
                        ))
                        .context_menu(move |menu, window, cx| items(menu, window, cx));
                    // Same motion as a project group; without a disclosure yet the
                    // worktree has never been toggled and is simply open.
                    let chats = workspace.map(|id| self.workspace_chats(id, cx));
                    rows.push(
                        column()
                            .child(row)
                            .children(chats.map(|chats| {
                                match disclosure {
                                    Some(disclosure) => disclosure
                                        .measured_body(
                                            SharedString::from(format!("worktree-body-{key}")),
                                            chats,
                                            now,
                                        )
                                        .into_any_element(),
                                    None => chats.into_any_element(),
                                }
                            }))
                            .into_any_element(),
                    );
                }
            } else {
                let id = project.workspace;
                let message = match git.entries.get(&project.path) {
                    Some(Ok(None)) => "Local folder",
                    Some(Err(_)) => "Git unavailable",
                    _ => "Reading Git…",
                };
                let projects = self.projects.clone();
                let items = Rc::new(
                    move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                        let chat = projects.clone();
                        let terminal = projects.clone();
                        menu.item(PopupMenuItem::new("New chat").disabled(blocked).on_click(
                            move |_, _, cx| chat.update(cx, |p, cx| p.create_chat_in(id, cx)),
                        ))
                        .item(
                            PopupMenuItem::new("Open terminal")
                                .disabled(blocked)
                                .on_click(move |_, _, cx| {
                                    terminal.update(cx, |p, cx| p.show_terminal(id, cx))
                                }),
                        )
                    },
                );
                let group = SharedString::from(format!("group-folder-{key}"));
                rows.push(
                    row()
                        .id(SharedString::from(format!("row-folder-{key}")))
                        .group(group.clone())
                        .pl(rems(1.375))
                        .pr_1()
                        .gap_0p5()
                        .h_8()
                        .rounded(t::sidebar_row_radius())
                        .hover(|s| s.bg(t::hover()))
                        .child(
                            Button::new(SharedString::from(format!("folder-{key}")))
                                .custom(quiet_row(cx))
                                .small()
                                .compact()
                                .flex_1()
                                .min_w_0()
                                .px_0()
                                .border_0()
                                .tooltip("Folder without Git")
                                .child(
                                    row()
                                        .w_full()
                                        .gap_1p5()
                                        .text_sm()
                                        .text_color(t::secondary())
                                        .child(
                                            Icon::new(IconName::Folder)
                                                .size_3p5()
                                                .text_color(t::faint()),
                                        )
                                        .child(message),
                                )
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    s.local_selection(cx);
                                    s.projects.update(cx, |p, cx| p.select(id, cx))
                                })),
                        )
                        .child(reveal_on_hover(
                            &format!("folder-{key}"),
                            &group,
                            false,
                            row()
                                .gap_0p5()
                                .child(
                                    Button::new(SharedString::from(format!("terminal-{key}")))
                                        .ghost()
                                        .small()
                                        .compact()
                                        .size_6()
                                        .p_0()
                                        .icon(
                                            Icon::default()
                                                .path("canopy/square-terminal.svg")
                                                .size_3p5(),
                                        )
                                        .accessibility_label("Open folder terminal")
                                        .tooltip("Open folder terminal")
                                        .disabled(blocked)
                                        .on_click(cx.listener(move |s, _, _, cx| {
                                            s.projects.update(cx, |p, cx| p.show_terminal(id, cx))
                                        })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("chat-{key}")))
                                        .ghost()
                                        .small()
                                        .compact()
                                        .size_6()
                                        .p_0()
                                        .icon(IconName::Plus)
                                        .accessibility_label("New chat in this folder")
                                        .tooltip("New chat in this folder")
                                        .disabled(blocked)
                                        .on_click(cx.listener(move |s, _, _, cx| {
                                            s.projects.update(cx, |p, cx| p.create_chat_in(id, cx))
                                        })),
                                ),
                        ))
                        .context_menu(move |menu, window, cx| items(menu, window, cx))
                        .into_any_element(),
                );
                rows.push(self.workspace_chats(id, cx).into_any_element());
            }
            trees
                .entry(identity)
                .or_insert_with(|| (header, vec![]))
                .1
                .extend(rows);
        }
        self.remote_projects(&mut trees, now, cx);
        let mut groups: Vec<_> = trees
            .into_iter()
            .filter_map(|(key, (header, rows))| {
                let disclosure = self.project_groups.get(&key)?;
                Some(
                    column()
                        .child(header)
                        .child(disclosure.measured_body(
                            SharedString::from(format!("project-body-{key}")),
                            column().gap_px().children(rows),
                            now,
                        ))
                        .into_any_element(),
                )
            })
            .collect();
        groups.extend(self.remote_connection_messages(cx));
        let error = git.error.as_ref().or(state.error.as_ref()).or(self
            .terminals
            .read(cx)
            .error
            .as_ref());
        if let Some(error) = error {
            groups.push(
                column()
                    .gap_1()
                    .py_2()
                    .child(div().text_xs().text_color(t::red()).child(error.clone()))
                    .into_any_element(),
            );
        }
        if let Some(error) = &git.watch_error {
            groups.push(
                row()
                    .py_2()
                    .gap_2()
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(t::yellow())
                                    .child("Git updates paused"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(t::secondary())
                                    .child("Refresh to update branches."),
                            ),
                    )
                    .child(
                        Button::new("refresh-git-warning")
                            .ghost()
                            .small()
                            .compact()
                            .icon(IconName::RotateCw)
                            .tooltip(error.clone())
                            .accessibility_label("Refresh Git metadata")
                            .on_click(
                                cx.listener(|s, _, _, cx| s.git.update(cx, |g, cx| g.refresh(cx))),
                            ),
                    )
                    .into_any_element(),
            );
        }
        column().gap_1p5().children(groups)
    }
}

/// Row buttons draw no fill of their own: the whole row carries hover.
fn quiet_row(cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .foreground(t::text())
        .hover(transparent_black())
        .active(transparent_black())
}

/// Secondary row actions: shown on hover (always for `pinned`), and also in the
/// row's context menu.
fn reveal_on_hover(
    key: &str,
    group: &SharedString,
    pinned: bool,
    actions: impl IntoElement,
) -> Stateful<Div> {
    aside(&format!("actions-{key}"), actions).when(!pinned, |d| {
        d.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.))
    })
}

/// A part of a row with tooltips of its own: the row's hover card steps aside.
fn aside(key: &str, child: impl IntoElement) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("aside-{key}")))
        .flex_shrink_0()
        .hides_hover_card()
        .child(child)
}

pub(super) fn tag(label: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .px_1()
        .rounded(rems(0.25))
        .border_1()
        .border_color(t::border())
        .text_size(rems(0.65625))
        .line_height(rems(0.9375))
        .text_color(t::faint())
        .child(label)
}

/// Keep the distinguishing suffix visible even when a branch shares a long prefix.
fn branch_label(label: &str) -> Div {
    div()
        .w_full()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_middle()
        .child(label.to_owned())
}
