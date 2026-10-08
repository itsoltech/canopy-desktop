use super::*;
use crate::ui::worktree_dialog::WorktreeRequest;
use canopy_desktop::git::{RemovalKind, RemoveWorktree};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::{
    base::Disableable,
    component::button::{ButtonCustomVariant, ButtonVariants},
};
impl EventEmitter<WorktreeRequest> for Sidebar {}
impl Sidebar {
    fn workspace_stop_button(
        &self,
        id: canopy_desktop::state::workspace::WorkspaceId,
        key: &str,
        disabled: bool,
        cx: &Context<Self>,
    ) -> Button {
        let feedback = self.stop_feedback.get(&id);
        let button = if let Some(feedback) = feedback {
            loading_stop_button(SharedString::from(format!("stop-{key}")), feedback, cx)
        } else {
            stop_processes_button(SharedString::from(format!("stop-{key}")), cx)
        };
        button
            .disabled(
                (disabled || self.terminals.read(cx).workspace_stopping(id))
                    && !feedback.is_some_and(ButtonLoading::active),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.terminals
                    .update(cx, |terminals, cx| terminals.request_stop_workspace(id, cx));
            }))
    }

    pub(super) fn sync_stop_feedback(&mut self, cx: &mut Context<Self>) {
        let ids: std::collections::HashSet<_> = self
            .projects
            .read(cx)
            .catalog
            .items
            .iter()
            .map(|p| p.workspace)
            .collect();
        self.stop_feedback.retain(|id, _| ids.contains(id));
        for id in ids {
            self.stop_feedback
                .entry(id)
                .or_default()
                .set(self.terminals.read(cx).workspace_stopping(id), cx);
        }
    }

    pub(super) fn sync_project_groups(&mut self, cx: &mut Context<Self>) {
        self.sync_stop_feedback(cx);
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
        cx.notify();
    }
    fn project_hover(&mut self, key: &str, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.hovered_project_item = Some(key.into());
        } else if self.hovered_project_item.as_deref() == Some(key) {
            self.hovered_project_item = None;
        } else {
            return;
        }
        cx.notify();
    }
    pub(super) fn projects_content(&self, now: Instant, cx: &Context<Self>) -> (Div, f32) {
        let state = self.projects.read(cx);
        let git = self.git.read(cx);
        let terminals = self.terminals.read(cx);
        let running = terminals.running_workspaces(cx);
        let terminal_error = terminals.error.clone();
        let mut seen = std::collections::HashSet::new();
        let mut groups = vec![];
        let mut height = 0.;
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
            let hover_key = key.clone();
            let toggle_root = root.clone();
            let close_root = root.clone();
            let refresh = self.git.clone();
            let hovered = self.hovered_project_item.as_deref() == Some(&key);
            let foreground = if hovered { t::text() } else { t::secondary() };
            let trigger = Button::new(SharedString::from(key.clone()))
                .custom(ButtonCustomVariant::new(cx).foreground(foreground))
                .compact()
                .h(px(t::ROW))
                .w_full()
                .p_0()
                .border_0()
                .cursor_pointer()
                .accessibility_label(format!(
                    "{} {label}",
                    if disclosure.open {
                        "Collapse"
                    } else {
                        "Expand"
                    }
                ))
                .tooltip(canopy_desktop::platform::paths::display(&root))
                .disabled(!self.sections[0].open)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(group) = this.project_groups.get_mut(&toggle_root) {
                        group.toggle(Instant::now(), cx);
                        cx.notify();
                    }
                }))
                .child(
                    row()
                        .w_full()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(foreground)
                        .child(disclosure.chevron(now).text_color(foreground))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_left()
                                .text_ellipsis()
                                .child(label),
                        ),
                );
            let header = row()
                .h(px(t::ROW))
                .gap(px(8.))
                .child(
                    hover_action(
                        SharedString::from(format!("hover-{key}")),
                        trigger,
                        cx.listener(move |this, value, _, cx| {
                            this.project_hover(&hover_key, *value, cx)
                        }),
                    )
                    .flex_1()
                    .min_w_0()
                    .context_menu(move |menu, _, _| {
                        let refresh = refresh.clone();
                        menu.item(PopupMenuItem::new("Refresh Git metadata").on_click(
                            move |_, _, cx| refresh.update(cx, |git, cx| git.refresh(cx)),
                        ))
                    }),
                )
                .children(info.clone().map(|repo| {
                    list_button(SharedString::from(format!("new-{key}")), "+ new", false, cx)
                        .px_0()
                        .w(px(36.))
                        .disabled(git.busy || state.busy || !self.sections[0].open)
                        .accessibility_label("Create worktree")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(WorktreeRequest::Create(repo.clone()))
                        }))
                }))
                .child(
                    quiet_button(
                        SharedString::from(format!("close-{key}")),
                        "Close project",
                        false,
                        cx,
                    )
                    .w(px(16.))
                    .child(icon(IconName::Close).size(px(12.)))
                    .disabled(git.busy || !self.sections[0].open)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.projects
                            .update(cx, |p, cx| p.close_repository(&close_root, cx))
                    })),
                );
            let mut rows = vec![];
            if let Some(info) = info {
                for wt in &info.worktrees {
                    let path = wt.path.clone();
                    let repository = info.root.clone();
                    let remove_path = path.clone();
                    let remove_root = repository.clone();
                    let selected = state.catalog.current().is_some_and(|p| p.path == path);
                    let workspace = state
                        .catalog
                        .items
                        .iter()
                        .find(|p| p.path == path)
                        .map(|p| p.workspace);
                    let key = format!("worktree-{}", path.display());
                    let hover_key = key.clone();
                    let hovered = self.hovered_project_item.as_deref() == Some(&key);
                    let name = if wt.locked {
                        format!("{} · locked", wt.label)
                    } else {
                        wt.label.clone()
                    };
                    let action = list_button(SharedString::from(key.clone()), name, hovered, cx)
                        .px_0()
                        .w_full()
                        .disabled(
                            git.busy
                                || state.busy
                                || !wt.available
                                || !disclosure.open
                                || !self.sections[0].open,
                        )
                        .tooltip(wt.tooltip())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.git
                                .update(cx, |g, cx| g.open(repository.clone(), path.clone(), cx))
                        }));
                    let entry = row()
                        .id(SharedString::from(format!("hover-{key}")))
                        .on_hover(cx.listener(move |this, value, _, cx| {
                            this.project_hover(&hover_key, *value, cx)
                        }))
                        .h(px(t::ROW))
                        .flex_shrink_0()
                        .pl(px(14.))
                        .gap(px(6.))
                        .child(
                            div()
                                .w(px(10.))
                                .text_color(t::secondary())
                                .child(if selected { "*" } else { "" }),
                        )
                        .child(div().flex_1().min_w_0().child(action))
                        .children(workspace.filter(|id| running.contains(id)).map(|id| {
                            self.workspace_stop_button(
                                id,
                                &key,
                                git.busy || !disclosure.open || !self.sections[0].open,
                                cx,
                            )
                        }))
                        .children(wt.name.as_ref().map(|name| {
                            let request = RemoveWorktree {
                                repository: remove_root.clone(),
                                path: remove_path.clone(),
                                kind: if wt.missing {
                                    RemovalKind::MissingRegistration { name: name.clone() }
                                } else {
                                    RemovalKind::Directory
                                },
                            };
                            remove_worktree_button(SharedString::from(format!("remove-{key}")), cx)
                                .accessibility_label(if wt.missing {
                                    "Remove stale worktree entry"
                                } else {
                                    "Remove worktree"
                                })
                                .tooltip(if wt.missing {
                                    "Remove stale worktree entry"
                                } else {
                                    "Remove worktree"
                                })
                                .opacity(if hovered { 1. } else { 0. })
                                .disabled(
                                    !hovered
                                        || git.busy
                                        || wt.locked
                                        || (!wt.available && !wt.missing)
                                        || !disclosure.open
                                        || !self.sections[0].open,
                                )
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.emit(WorktreeRequest::Remove(request.clone()))
                                }))
                        }));
                    rows.push(entry.into_any_element());
                }
            } else {
                let message = match git.entries.get(&project.path) {
                    Some(Ok(None)) => "Folder without Git".into(),
                    Some(Err(error)) => error.clone(),
                    None => "Reading Git metadata…".into(),
                    _ => String::new(),
                };
                // A non-Git folder still has a selectable workspace.
                let id = project.workspace;
                rows.push(
                    row()
                        .h(px(t::ROW))
                        .flex_shrink_0()
                        .gap(px(6.))
                        .child(
                            list_button(
                                SharedString::from(format!("folder-{key}")),
                                message,
                                false,
                                cx,
                            )
                            .flex_1()
                            .min_w_0()
                            .px(px(14.))
                            .disabled(!disclosure.open || !self.sections[0].open || git.busy)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.projects.update(cx, |p, cx| p.select(id, cx))
                                },
                            )),
                        )
                        .children(running.contains(&id).then(|| {
                            self.workspace_stop_button(
                                id,
                                &key,
                                git.busy || !disclosure.open || !self.sections[0].open,
                                cx,
                            )
                        }))
                        .into_any_element(),
                );
            }
            let body_height = rows.len() as f32 * t::ROW;
            height += t::ROW + disclosure.height(body_height, now);
            groups.push(
                column()
                    .child(header)
                    .child(disclosure.body(body_height, column().children(rows), now))
                    .into_any_element(),
            );
        }
        if let Some(error) = git
            .error
            .as_ref()
            .or(git.watch_error.as_ref())
            .or(terminal_error.as_ref())
        {
            groups.push(
                div()
                    .text_size(px(11.))
                    .text_color(t::red())
                    .child(error.clone())
                    .into_any_element(),
            );
            height += t::ROW * 3.;
        }
        (column().children(groups), height)
    }
}
