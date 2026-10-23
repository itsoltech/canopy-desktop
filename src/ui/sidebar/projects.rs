use super::*;
use crate::ui::worktree_dialog::WorktreeRequest;
use canopy_desktop::git::{RemovalKind, RemoveWorktree};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    Sizable,
    button::ButtonVariants,
    menu::{DropdownMenu, PopupMenuItem},
};
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
        let roots: std::collections::HashSet<_> = self
            .projects
            .read(cx)
            .catalog
            .items
            .iter()
            .map(|p| p.repository_path.as_ref().unwrap_or(&p.path).clone())
            .collect();
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
                self.collapsed_worktrees.remove(&project.path);
                let root = project.repository_path.as_ref().unwrap_or(&project.path);
                if let Some(group) = self.project_groups.get_mut(root)
                    && !group.open
                {
                    group.toggle(Instant::now(), cx);
                }
            }
        }
        cx.notify();
    }
    fn activity(
        &self,
        ids: &[canopy_desktop::state::workspace::WorkspaceId],
        cx: &App,
    ) -> Option<Div> {
        use canopy_desktop::state::workspace::PaneKind;
        let app = cx.global::<crate::app_state::AppState>();
        let mut working = 0;
        let mut attention = 0;
        for pane in app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .filter(|w| ids.contains(&w.id))
            .flat_map(|w| w.all_panes())
            .filter(|p| {
                p.metadata.kind == PaneKind::Chat
                    || (p.metadata.kind == PaneKind::Terminal
                        && canopy_desktop::chat::provider::Provider::from_tool(&p.tool).is_some())
            })
        {
            match super::chats::navigation_status(pane.id, cx) {
                "Working" | "Goal" => working += 1,
                "Waiting" | "Needs attention" | "Error" => attention += 1,
                _ => {}
            }
        }
        if working + attention == 0 {
            return None;
        }
        let text = match (working, attention) {
            (0, n) => format!("{n} need attention"),
            (n, 0) => format!("{n} working"),
            (w, a) => format!("{w} working · {a} need attention"),
        };
        Some(
            div()
                .text_xs()
                .line_height(rems(1.))
                .text_color(if attention > 0 {
                    t::yellow()
                } else {
                    t::secondary()
                })
                .child(text),
        )
    }

    pub(super) fn projects_content(&self, now: Instant, cx: &Context<Self>) -> Div {
        let state = self.projects.read(cx);
        let git = self.git.read(cx);
        let app = cx.global::<crate::app_state::AppState>();
        let running = app.running_workspaces(cx);
        let mut seen = std::collections::HashSet::new();
        let mut groups = vec![];
        for project in &state.catalog.items {
            let root = project
                .repository_path
                .as_ref()
                .unwrap_or(&project.path)
                .clone();
            if !seen.insert(root.clone()) {
                continue;
            }
            let Some(disclosure) = self.project_groups.get(&root) else {
                continue;
            };
            let info = git.repository(&root).cloned();
            let label = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let key = format!("project-{}", root.display());
            let toggle_root = root.clone();
            let close_root = root.clone();
            let ids: Vec<_> = state
                .catalog
                .items
                .iter()
                .filter(|p| p.repository_path.as_ref().unwrap_or(&p.path) == &root)
                .map(|p| p.workspace)
                .collect();
            let activity = (!disclosure.open)
                .then(|| self.activity(&ids, cx))
                .flatten();
            let owner = cx.entity().downgrade();
            let repo = info.clone();
            let blocked = git.busy || state.busy || !self.sections[0].open;
            let project_menu = Button::new(SharedString::from(format!("menu-{key}")))
                .ghost()
                .small()
                .compact()
                .size_6()
                .p_0()
                .icon(IconName::Ellipsis)
                .tooltip("Project actions")
                .disabled(blocked)
                .dropdown_menu(move |menu, _, _| {
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
                });
            let header = row()
                .gap_1()
                .min_h(t::navigation_height())
                .child(
                    Button::new(SharedString::from(key.clone()))
                        .ghost()
                        .small()
                        .compact()
                        .min_h(t::navigation_height())
                        .flex_1()
                        .min_w_0()
                        .px_0()
                        .accessibility_label(format!(
                            "{} {label}",
                            if disclosure.open {
                                "Collapse"
                            } else {
                                "Expand"
                            }
                        ))
                        .tooltip(canopy_desktop::platform::paths::display(&root))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if let Some(group) = s.project_groups.get_mut(&toggle_root) {
                                group.toggle(Instant::now(), cx);
                                cx.notify();
                            }
                        }))
                        .child(
                            row().w_full().gap_2().child(disclosure.chevron(now)).child(
                                row()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(t::text())
                                            .truncate()
                                            .child(label),
                                    )
                                    .children(activity),
                            ),
                        ),
                )
                .child(project_menu);
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
                    let selected = workspace.is_some() && workspace == state.catalog.active;
                    let expanded = !self.collapsed_worktrees.contains(&path);
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
                    let menu = Button::new(SharedString::from(format!("menu-{key}")))
                        .ghost()
                        .small()
                        .compact()
                        .size_6()
                        .p_0()
                        .icon(IconName::Ellipsis)
                        .tooltip("Worktree actions")
                        .dropdown_menu(move |menu, _, _| {
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
                        });
                    let action = Button::new(SharedString::from(key))
                        .ghost()
                        .small()
                        .compact()
                        .h_8()
                        .flex_1()
                        .min_w_0()
                        .px_0()
                        .disabled(disabled)
                        .tooltip(wt.tooltip())
                        .accessibility_label(format!(
                            "{}{}",
                            wt.label,
                            if selected { ", current worktree" } else { "" }
                        ))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.git.update(cx, |g, cx| {
                                g.open(open_root.clone(), open_path.clone(), cx)
                            })
                        }))
                        .child(
                            div()
                                .w_full()
                                .text_left()
                                .truncate()
                                .text_sm()
                                .text_color(if selected {
                                    t::accent()
                                } else {
                                    t::secondary()
                                })
                                .child(branch_label(&wt.label)),
                        );
                    let row = row()
                        .pl_3()
                        .gap_1()
                        .h_8()
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
                            .icon(if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .accessibility_label(format!(
                                "{} {}",
                                if expanded { "Collapse" } else { "Expand" },
                                wt.label
                            ))
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    if !s.collapsed_worktrees.remove(&collapse_path) {
                                        s.collapsed_worktrees.insert(collapse_path.clone());
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(action)
                        .children(
                            workspace
                                .filter(|_| !expanded)
                                .and_then(|id| self.activity(&[id], cx)),
                        )
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
                            .icon(IconName::SquareTerminal)
                            .tooltip("Open worktree terminal")
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.git.update(cx, |g, cx| {
                                        g.open_terminal(
                                            terminal_root.clone(),
                                            terminal_path.clone(),
                                            cx,
                                        )
                                    })
                                },
                            )),
                        )
                        .child(menu);
                    rows.push(
                        column()
                            .child(row)
                            .children(
                                workspace
                                    .filter(|_| expanded)
                                    .map(|id| self.workspace_chats(id, cx)),
                            )
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
                rows.push(
                    row()
                        .pl_2()
                        .gap_1()
                        .h_8()
                        .child(
                            Button::new(SharedString::from(format!("folder-{key}")))
                                .ghost()
                                .small()
                                .compact()
                                .flex_1()
                                .min_w_0()
                                .px_0()
                                .label(message)
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    s.projects.update(cx, |p, cx| p.select(id, cx))
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("terminal-{key}")))
                                .ghost()
                                .small()
                                .compact()
                                .size_6()
                                .p_0()
                                .icon(IconName::SquareTerminal)
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
                                .tooltip("New chat in this folder")
                                .disabled(blocked)
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    s.projects.update(cx, |p, cx| p.create_chat_in(id, cx))
                                })),
                        )
                        .into_any_element(),
                );
                rows.push(self.workspace_chats(id, cx).into_any_element());
            }
            groups.push(
                column()
                    .child(header)
                    .child(disclosure.measured_body(
                        SharedString::from(format!("project-body-{}", root.display())),
                        column().children(rows),
                        now,
                    ))
                    .into_any_element(),
            );
        }
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
        column().gap_1().children(groups)
    }
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
