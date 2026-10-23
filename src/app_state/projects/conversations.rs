//! Addressed conversation commands, independent of the workspace currently shown.
use super::*;
use canopy_desktop::state::workspace::PaneKind;

impl ProjectsState {
    pub(crate) fn conversation_workspace(
        &self,
        workspace: WorkspaceId,
        cx: &App,
    ) -> Option<Workspace> {
        let active = cx.global::<AppState>().workspace.read(cx);
        if active.id == workspace {
            Some(active.clone())
        } else if self.scratch.id == workspace {
            Some(self.scratch.clone())
        } else {
            self.sessions.get(&workspace).cloned()
        }
    }

    /// Mutates the authoritative workspace without changing the visible project.
    fn update_chat_workspace(
        &mut self,
        workspace: WorkspaceId,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Workspace) -> Result<(), SharedString>,
    ) -> Result<(), SharedString> {
        if !self.available() {
            return Err("Projects are busy. Try again when the current operation finishes.".into());
        }
        let active = cx.global::<AppState>().workspace.clone();
        if active.read(cx).id == workspace {
            active.update(cx, |state, cx| {
                update(state)?;
                cx.notify();
                Ok::<_, SharedString>(())
            })?;
        } else if self.scratch.id == workspace {
            update(&mut self.scratch)?;
        } else {
            update(
                self.sessions.get_mut(&workspace).ok_or_else(|| {
                    SharedString::from("The conversation is no longer available.")
                })?,
            )?;
        }
        self.enabled = true;
        self.changed(cx);
        cx.notify();
        Ok(())
    }

    pub fn rename_chat(
        &mut self,
        workspace: WorkspaceId,
        pane: PaneId,
        title: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), SharedString> {
        if !self.chat_exists(workspace, pane, cx) {
            return Err("The conversation is no longer available.".into());
        }
        self.update_chat_workspace(workspace, cx, |state| {
            state
                .rename_chat(pane, title)
                .map_err(|_| SharedString::from("Enter a non-empty name without line breaks."))
        })?;
        self.sync_chat_name(pane, cx);
        Ok(())
    }

    pub(super) fn sync_chat_name(&self, pane: PaneId, cx: &mut Context<Self>) {
        let name = self.runtime_workspaces(cx).iter().find_map(|workspace| {
            let title = workspace.chat_title(pane)?.to_owned();
            let state = workspace
                .all_panes()
                .iter()
                .find(|p| p.id == pane)?
                .metadata
                .chat_title_state;
            Some((title, state))
        });
        let Some((name, state)) = name else {
            return;
        };
        // A first message can call this from inside ChatView's update.
        cx.defer(move |cx| Self::apply_chat_name(pane, &name, state, cx));
    }

    pub(super) fn apply_chat_name(
        pane: PaneId,
        name: &str,
        state: Option<canopy_desktop::state::workspace::ChatTitleState>,
        cx: &mut App,
    ) {
        let app = cx.global::<AppState>().clone();
        if let Some(view) = app.chats.read(cx).views.get(&pane).cloned() {
            view.update(cx, |view, cx| view.set_title(name, state, cx));
        }
        app.agents
            .update(cx, |agents, cx| agents.rename(pane, name, cx));
    }

    pub fn request_delete_chat(workspace: WorkspaceId, pane: PaneId, cx: &mut App) {
        let projects = cx.global::<AppState>().projects.clone();
        if !projects.read(cx).chat_exists(workspace, pane, cx) {
            return;
        }
        super::super::Editors::guard_closing(
            vec![pane],
            move |cx| {
                let app = cx.global::<AppState>().clone();
                if !app.projects.read(cx).chat_exists(workspace, pane, cx) {
                    return;
                }
                if let Some(error) = app.chats.read(cx).deletion_blocker(&[pane], cx) {
                    app.chats.update(cx, |chats, cx| {
                        chats.error = Some(error);
                        cx.notify();
                    });
                    return;
                }
                app.projects.update(cx, |projects, cx| {
                    if !projects.chat_exists(workspace, pane, cx) {
                        return;
                    }
                    if let Err(error) = projects.update_chat_workspace(workspace, cx, |state| {
                        let tab = state
                            .tabs()
                            .iter()
                            .find(|t| t.root.find(pane).is_some())
                            .map(|t| t.id)
                            .ok_or_else(|| {
                                SharedString::from("The conversation is no longer available.")
                            })?;
                        state.close_pane(tab, pane).map_err(|_| {
                            SharedString::from("The conversation could not be deleted.")
                        })
                    }) {
                        app.chats.update(cx, |chats, cx| {
                            chats.error = Some(error.to_string());
                            cx.notify();
                        });
                    }
                });
            },
            cx,
        );
    }

    fn chat_exists(&self, workspace: WorkspaceId, pane: PaneId, cx: &App) -> bool {
        self.available()
            && self.conversation_workspace(workspace, cx).is_some_and(|w| {
                w.all_panes()
                    .iter()
                    .any(|p| p.id == pane && p.metadata.kind == PaneKind::Chat)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::ChatGuardRequest;
    use canopy_desktop::state::workspace::{Axis, PaneMetadata};
    use core::prelude::v1::test;
    use gpui_kit::TestAppContext;

    struct Probe {
        request: Option<ChatGuardRequest>,
        _events: Subscription,
    }

    #[gpui_kit::test]
    fn standalone_commands_keep_project_and_resolve_the_panes_current_tab(cx: &mut TestAppContext) {
        let (app, workspace, pane, neighbor, destination, target, probe, project) =
            cx.update(|cx| {
                let mut scratch = Workspace::new();
                let tab = scratch.open("Saved conversation", "claude");
                let pane = scratch.active().unwrap().focused;
                scratch
                    .set_pane_metadata(
                        tab,
                        pane,
                        PaneMetadata {
                            kind: PaneKind::Chat,
                            title: Some("New chat".into()),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                let neighbor = scratch.split(tab, pane, Axis::Horizontal, "shell").unwrap();
                let destination = scratch.open("Preview", "editor");
                let target = scratch.active().unwrap().focused;
                let workspace = scratch.id;
                let app = AppState::install_test(scratch, cx);
                let project = app.projects.update(cx, |projects, cx| {
                    let mut catalog = projects.catalog.clone();
                    let id = catalog.open("/project".into());
                    projects.ready = true;
                    projects.publish(catalog, cx);
                    id
                });
                let chats = app.chats.clone();
                let probe = cx.new(|cx: &mut Context<Probe>| Probe {
                    request: None,
                    _events: cx.subscribe(&chats, |this, _, request: &ChatGuardRequest, _| {
                        this.request = Some(request.clone());
                    }),
                });
                (
                    app,
                    workspace,
                    pane,
                    neighbor,
                    destination,
                    target,
                    probe,
                    project,
                )
            });
        cx.update(|cx| {
            app.projects.update(cx, |projects, cx| {
                projects
                    .rename_chat(workspace, pane, "Renamed conversation", cx)
                    .unwrap();
            });
            assert_eq!(app.projects.read(cx).catalog.active, Some(project));
            assert_eq!(
                app.projects.read(cx).scratch.chat_title(pane),
                Some("Renamed conversation")
            );
            app.projects
                .update(cx, |projects, cx| projects.focus_agent(pane, cx));
            assert_eq!(app.projects.read(cx).catalog.active, None);
            assert_eq!(app.workspace.read(cx).active().unwrap().focused, pane);
            app.projects.update(cx, |projects, cx| {
                let mut catalog = projects.catalog.clone();
                catalog.select(project);
                projects.publish(catalog, cx);
            });
            ProjectsState::request_delete_chat(workspace, pane, cx);
        });
        cx.run_until_parked();
        let action = cx.update(|cx| {
            let request = probe
                .read(cx)
                .request
                .as_ref()
                .expect("a dormant saved chat must ask before deletion")
                .clone();
            assert_eq!(request.ids, [pane]);
            assert!(request.deleting);
            assert!(app.projects.read(cx).scratch.chat_title(pane).is_some());
            // Dock while the confirmation is pending. Consent still targets just PaneId.
            app.projects.update(cx, |projects, _| {
                let source = projects
                    .scratch
                    .tabs()
                    .iter()
                    .find(|t| t.root.find(pane).is_some())
                    .unwrap()
                    .id;
                projects
                    .scratch
                    .dock_pane(source, pane, destination, target, Axis::Horizontal, false)
                    .unwrap();
            });
            request.action.clone()
        });
        cx.update(|cx| {
            action(cx);
            let projects = app.projects.read(cx);
            assert_eq!(projects.catalog.active, Some(project));
            assert_eq!(app.workspace.read(cx).id, project);
            assert!(projects.scratch.chat_title(pane).is_none());
            let remaining = projects.scratch.all_panes();
            assert!(remaining.iter().any(|p| p.id == neighbor));
            assert!(remaining.iter().any(|p| p.id == target));
        });
    }

    #[gpui_kit::test]
    fn inactive_project_chat_commands_preserve_the_visible_project_and_neighbor(
        cx: &mut TestAppContext,
    ) {
        let (app, source, pane, neighbor, visible, probe) = cx.update(|cx| {
            let app = AppState::install_test(Workspace::new(), cx);
            let (source, pane, neighbor, visible) = app.projects.update(cx, |projects, cx| {
                projects.ready = true;
                projects.open_worktree("/repo".into(), "/worktree".into(), cx);
                let source = projects.catalog.active.unwrap();
                projects
                    .open_chat(
                        Some(source),
                        "Project conversation",
                        "claude",
                        PaneMetadata {
                            kind: PaneKind::Chat,
                            title: Some("Project conversation".into()),
                            cwd: Some("/worktree".into()),
                            ..Default::default()
                        },
                        cx,
                    )
                    .unwrap();
                let (pane, neighbor) = app.workspace.update(cx, |workspace, _| {
                    let tab = workspace.active().unwrap().id;
                    let pane = workspace.active().unwrap().focused;
                    let neighbor = workspace
                        .split(tab, pane, Axis::Horizontal, "shell")
                        .unwrap();
                    (pane, neighbor)
                });
                projects.open_worktree("/other".into(), "/other".into(), cx);
                let visible = projects.catalog.active.unwrap();
                projects
                    .rename_chat(source, pane, "Named project conversation", cx)
                    .unwrap();
                (source, pane, neighbor, visible)
            });
            let chats = app.chats.clone();
            let probe = cx.new(|cx: &mut Context<Probe>| Probe {
                request: None,
                _events: cx.subscribe(&chats, |this, _, request: &ChatGuardRequest, _| {
                    this.request = Some(request.clone());
                }),
            });
            assert_eq!(app.workspace.read(cx).id, visible);
            assert_eq!(
                app.projects
                    .read(cx)
                    .conversation_workspace(source, cx)
                    .unwrap()
                    .chat_title(pane),
                Some("Named project conversation")
            );
            ProjectsState::request_delete_chat(source, pane, cx);
            (app, source, pane, neighbor, visible, probe)
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let request = probe
                .read(cx)
                .request
                .as_ref()
                .expect("saved project chat asks before deleting")
                .clone();
            assert_eq!(request.ids, [pane]);
            (request.action)(cx);
            let projects = app.projects.read(cx);
            let remaining = projects.conversation_workspace(source, cx).unwrap();
            assert!(remaining.chat_title(pane).is_none());
            assert!(remaining.all_panes().iter().any(|p| p.id == neighbor));
            assert_eq!(projects.catalog.active, Some(visible));
            assert_eq!(app.workspace.read(cx).id, visible);
        });
    }

    #[gpui_kit::test]
    fn dependent_pane_blocks_delete_without_switching_projects(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut scratch = Workspace::new();
            let tab = scratch.open("Conversation", "claude");
            let pane = scratch.active().unwrap().focused;
            scratch
                .set_pane_metadata(
                    tab,
                    pane,
                    PaneMetadata {
                        kind: PaneKind::Chat,
                        cwd: Some("/chat-folder".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            scratch.split(tab, pane, Axis::Horizontal, "shell").unwrap();
            let workspace = scratch.id;
            let app = AppState::install_test(scratch, cx);
            app.projects.update(cx, |p, _| p.ready = true);
            ProjectsState::request_delete_chat(workspace, pane, cx);
            assert!(
                app.chats
                    .read(cx)
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("uses the chat's folder")
            );
            assert_eq!(app.workspace.read(cx).all_panes().len(), 2);
            assert_eq!(app.projects.read(cx).catalog.active, None);
        });
    }
}
