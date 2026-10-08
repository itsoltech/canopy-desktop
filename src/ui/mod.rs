pub mod assets;
#[cfg(target_os = "macos")]
mod attachment_native;
mod attachment_preview;
mod changes_panel;
pub mod components;
pub(crate) mod diff;
pub(crate) mod editor;
mod file_nodes;
pub(crate) mod font_preview;
mod history;
pub(crate) mod image_preview;
mod inspector;
mod jira;
mod markdown;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod notch;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod notch_geometry;
#[cfg(target_os = "macos")]
mod notch_macos;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod notch_motion;
#[cfg(target_os = "windows")]
mod notch_windows;
mod pane_layout;
mod platform_ui;
mod preferences;
mod quick_open;
mod session_inspector;
mod sidebar;
mod task_controls;
mod task_detail;
pub(crate) mod task_edit;
mod task_source;
mod tasks_panel;
pub(crate) mod terminal;
pub(crate) mod toasts;
mod upstream_dialog;
#[cfg(target_os = "macos")]
mod video_native;
#[cfg(target_os = "macos")]
pub(crate) mod video_preview;
#[cfg(target_os = "windows")]
mod windows_settings;
mod workspace_panes;
mod workspace_tabs;
mod worktree_dialog;
pub(crate) mod youtrack;
use workspace_panes::{Placement, SplitResize};
pub mod theme;

use crate::app_state::AppState;
use canopy_desktop::motion::{self, Presence, presets};
use canopy_desktop::state::workspace::{Axis, PaneId, WorkspaceId};
use components::file_tree::{FileTree, FileTreeEvent, FileTreeModel, FileTreeRequest};
use components::*;
use gpui_kit::base::Disableable;
use gpui_kit::component::{IconName, Root};
use gpui_kit::*;
use pane_layout::PanelSide;
use std::time::Instant;
use theme as t;

actions!(
    workspace,
    [
        OpenFolder,
        CloseProject,
        ToggleInspector,
        ToggleSidebar,
        ShowChanges,
        ShowSession,
        ShellTab,
        CodexTab,
        NewTab,
        CloseTab,
        SaveFile,
        QuickOpen,
        SplitHorizontal,
        SplitVertical,
        ClosePane
    ]
);

pub struct Workspace {
    focus: FocusHandle,
    _activation_observer: Subscription,
    _files_observer: Subscription,
    _new_file_events: Subscription,
    quick_open: Option<Entity<quick_open::QuickOpen>>,
    quick_open_events: Option<Subscription>,
    _editor_observers: Vec<Subscription>,
    screen_id: Option<WorkspaceId>,
    screen_reveal: Presence,
    sidebar: Entity<sidebar::Sidebar>,
    sidebar_open: bool,
    sidebar_reveal: Presence,
    state: AppState,
    _state_observers: Vec<Subscription>,
    #[cfg(target_os = "windows")]
    _windows_settings: Option<windows_settings::SettingsMonitor>,
    inspector: Entity<inspector::Inspector>,
    inspector_open: bool,
    inspector_reveal: Presence,
    _file_events: Subscription,
    _file_directory_events: Subscription,
    opened_file: Option<SharedString>,
    preferences: Option<WindowHandle<Root>>,
    worktree_modal: Option<Entity<worktree_dialog::WorktreeDialog>>,
    worktree_modal_events: Option<Subscription>,
    _worktree_requests: Subscription,
    _task_worktree_requests: Subscription,
    _task_detail_requests: Subscription,
    _task_edit_requests: Subscription,
    task_editor_modal: Option<Entity<task_edit::TaskEditorDialog>>,
    task_editor_events: Option<Subscription>,
    task_detail_modal: Option<Entity<task_detail::TaskDetailDialog>>,
    task_detail_events: Option<Subscription>,
    confirmation: Option<Entity<modal::Confirmation>>,
    confirmation_events: Option<Subscription>,
    _discard_events: Subscription,
    _upstream_events: Subscription,
    upstream_modal: Option<Entity<upstream_dialog::UpstreamDialog>>,
    upstream_modal_events: Option<Subscription>,
    modal: Option<Entity<TextPrompt>>,
    modal_events: Option<Subscription>,
    tab_scroll: workspace_tabs::TabScroll,
    hovered_tab: Option<canopy_desktop::state::workspace::TabId>,
    drop_hint: Option<(PaneId, Placement)>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().clone();
        #[cfg(target_os = "windows")]
        let windows_settings = {
            let executor = cx.foreground_executor().clone();
            let async_cx = cx.to_async();
            match windows_settings::SettingsMonitor::install(window, move || {
                let mut async_cx = async_cx.clone();
                executor
                    .spawn(async move {
                        let _ = async_cx.update(|cx| {
                            if canopy_desktop::motion::refresh_system_policy() {
                                cx.refresh_windows();
                            }
                        });
                    })
                    .detach();
            }) {
                Ok(monitor) => Some(monitor),
                Err(error) => {
                    eprintln!("Windows animation settings monitor failed: {error:#}");
                    None
                }
            }
        };
        let activation_observer = cx.observe_window_activation(window, |this, window, cx| {
            this.state.agents.update(cx, |agents, cx| {
                agents.window_active(window.is_window_active(), cx)
            });
        });
        state.agents.update(cx, |agents, cx| {
            agents.window_active(window.is_window_active(), cx)
        });
        let observers = vec![
            cx.observe(&state.diffs, |_, _, cx| cx.notify()),
            cx.observe(&state.git, |_, _, cx| cx.notify()),
            cx.observe_in(&state.tools, window, |_, _, window, cx| {
                window.refresh();
                cx.notify();
            }),
            cx.observe_in(&state.terminals, window, |_, _, window, cx| {
                window.refresh();
                cx.notify();
            }),
            cx.observe_in(&state.projects, window, |this, entity, window, cx| {
                let id = entity.read(cx).catalog.active;
                let mounting_layout = this.screen_id.is_some() != id.is_some();
                if this.screen_id != id {
                    this.screen_id = id;
                    this.opened_file = None;
                    this.hovered_tab = None;
                    this.drop_hint = None;
                    let now = Instant::now();
                    // Switching between populated workspace contexts is not a page entrance.
                    // Settle any previous entrance too, including rapid worktree switches.
                    this.screen_reveal =
                        Presence::new(!mounting_layout, presets::CONTENT_REVEAL, now);
                    if mounting_layout {
                        this.screen_reveal.set_open(true, now, motion::policy(cx));
                    }
                }
                if mounting_layout {
                    window.refresh();
                    window.on_next_frame(|window, _| window.refresh());
                }
                cx.notify();
            }),
            cx.observe_in(&state.workspace, window, |_, _, _, cx| cx.notify()),
            cx.observe_in(&state.notch_status, window, |_, _, window, cx| {
                window.refresh();
                cx.notify();
            }),
            cx.observe_in(&state.settings, window, |_, _, window, cx| {
                window.refresh();
                window.on_next_frame(|window, _| window.refresh());
                cx.notify();
            }),
            cx.observe_in(&state.layout, window, |this, entity, _, cx| {
                let layout = entity.read(cx);
                this.sidebar_open = layout.sidebar_open;
                this.inspector_open = layout.inspector_open;
                let now = Instant::now();
                this.sidebar_reveal
                    .set_open(this.sidebar_open, now, motion::policy(cx));
                this.inspector_reveal
                    .set_open(this.inspector_open, now, motion::policy(cx));
                cx.notify();
            }),
        ];
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        #[cfg(not(target_os = "windows"))]
        cx.bind_keys([
            KeyBinding::new("cmd-o", OpenFolder, Some("Workspace")),
            KeyBinding::new("cmd-shift-w", CloseProject, Some("Workspace")),
            KeyBinding::new("cmd-b", ToggleInspector, Some("Workspace")),
            KeyBinding::new("cmd-alt-b", ToggleSidebar, Some("Workspace")),
            KeyBinding::new("cmd-shift-g", ShowChanges, Some("Workspace")),
            KeyBinding::new("cmd-shift-s", ShowSession, Some("Workspace")),
            KeyBinding::new("cmd-t", NewTab, Some("Workspace")),
            KeyBinding::new("cmd-w", CloseTab, Some("Workspace")),
            KeyBinding::new("cmd-s", SaveFile, Some("Workspace")),
            KeyBinding::new("cmd-p", QuickOpen, Some("Workspace")),
            KeyBinding::new("cmd-d", SplitHorizontal, Some("Workspace")),
            KeyBinding::new("cmd-shift-d", SplitVertical, Some("Workspace")),
            KeyBinding::new("cmd-alt-w", ClosePane, Some("Workspace")),
            KeyBinding::new("cmd-1", ShellTab, Some("Workspace")),
            KeyBinding::new("cmd-2", CodexTab, Some("Workspace")),
        ]);
        #[cfg(target_os = "windows")]
        cx.bind_keys([
            KeyBinding::new("ctrl-shift-o", OpenFolder, Some("Workspace")),
            KeyBinding::new("ctrl-shift-f4", CloseProject, Some("Workspace")),
            KeyBinding::new("ctrl-shift-i", ToggleInspector, Some("Workspace")),
            KeyBinding::new("ctrl-shift-b", ToggleSidebar, Some("Workspace")),
            KeyBinding::new("ctrl-shift-g", ShowChanges, Some("Workspace")),
            KeyBinding::new("ctrl-shift-a", ShowSession, Some("Workspace")),
            KeyBinding::new("ctrl-shift-t", NewTab, Some("Workspace")),
            KeyBinding::new("ctrl-shift-w", CloseTab, Some("Workspace")),
            KeyBinding::new("ctrl-shift-s", SaveFile, Some("Workspace")),
            KeyBinding::new("ctrl-shift-p", QuickOpen, Some("Workspace")),
            KeyBinding::new("ctrl-shift-d", SplitHorizontal, Some("Workspace")),
            KeyBinding::new("ctrl-shift-e", SplitVertical, Some("Workspace")),
            KeyBinding::new("ctrl-shift-x", ClosePane, Some("Workspace")),
            KeyBinding::new("ctrl-shift-1", ShellTab, Some("Workspace")),
            KeyBinding::new("ctrl-shift-2", CodexTab, Some("Workspace")),
        ]);
        let editor_observers = vec![
            cx.observe_in(&state.workspace, window, |this, _, window, cx| {
                this.state
                    .editors
                    .update(cx, |editors, cx| editors.sync(window, cx));
            }),
            cx.observe_in(&state.projects, window, |this, _, window, cx| {
                this.state
                    .editors
                    .update(cx, |editors, cx| editors.sync(window, cx));
            }),
            cx.observe(&state.editors, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &state.editors,
                window,
                |_, _, request: &crate::app_state::GuardRequest, window, cx| {
                    crate::app_state::Editors::confirm(request, window, cx)
                },
            ),
        ];
        let files =
            cx.new(|cx| FileTree::new(FileTreeModel::new(vec![], []).expect("empty tree"), cx));
        let files_view = files.clone();
        let mut files_root = None;
        let files_observer = cx.observe(&state.files, move |_, state, cx| {
            let state = state.read(cx);
            let reset = files_root != state.root;
            files_root = state.root.clone();
            let nodes = file_nodes::nodes(&state.index);
            files_view.update(cx, |view, cx| view.replace(nodes, reset, cx));
        });
        let file_events = cx.subscribe(&files, |this, _, event: &FileTreeEvent, cx| {
            this.state.files.update(cx, |files, cx| {
                files.open(std::path::Path::new(event.id.as_str()), cx)
            });
        });
        let file_directory_events = cx.subscribe(&files, |this, _, event: &FileTreeRequest, cx| {
            this.state.files.update(cx, |files, cx| match event {
                FileTreeRequest::Expanded(ids) => files.expanded(
                    ids.iter()
                        .map(|id| std::path::PathBuf::from(id.as_str()))
                        .collect(),
                    cx,
                ),
                FileTreeRequest::Refresh => files.refresh(cx),
            });
        });
        let sidebar = cx.new(|cx| sidebar::Sidebar::new(files, cx));
        let new_file_events = cx.subscribe_in(
            &sidebar,
            window,
            |this, _, _: &sidebar::NewFile, window, cx| {
                if this.modal.is_some()
                    || this.task_detail_modal.is_some()
                    || this.task_editor_modal.is_some()
                    || this.state.files.read(cx).root.is_none()
                {
                    return;
                }
                let files = this.state.files.clone();
                let root = files.read(cx).root.clone();
                let modal = cx.new(|cx| {
                    TextPrompt::new(
                        "New file",
                        "Path relative to worktree",
                        "",
                        this.focus.clone(),
                        move |name, cx| {
                            if name.trim().is_empty() {
                                return Err("Enter a file name.".into());
                            }
                            if files.read(cx).root != root {
                                return Err("The worktree changed.".into());
                            }
                            files.update(cx, |files, cx| {
                                files.create_file(std::path::PathBuf::from(name), cx)
                            });
                            Ok(())
                        },
                        window,
                        cx,
                    )
                });
                this.modal_events =
                    Some(cx.subscribe(&modal, |this, _, _: &ModalDismissed, cx| {
                        this.modal = None;
                        this.modal_events = None;
                        cx.notify();
                    }));
                this.modal = Some(modal);
                cx.notify();
            },
        );
        let worktree_requests = cx.subscribe_in(
            &sidebar,
            window,
            |this, _, request: &worktree_dialog::WorktreeRequest, window, cx| {
                if this.modal.is_some()
                    || this.task_detail_modal.is_some()
                    || this.task_editor_modal.is_some()
                    || this.worktree_modal.is_some()
                    || this.upstream_modal.is_some()
                {
                    return;
                }
                let modal = cx.new(|cx| {
                    worktree_dialog::WorktreeDialog::new(
                        request.clone(),
                        this.focus.clone(),
                        window,
                        cx,
                    )
                });
                this.worktree_modal_events =
                    Some(cx.subscribe(&modal, |this, _, _: &ModalDismissed, cx| {
                        this.worktree_modal = None;
                        this.worktree_modal_events = None;
                        cx.notify();
                    }));
                this.worktree_modal = Some(modal);
                cx.notify();
            },
        );
        let inspector = cx.new(|cx| inspector::Inspector::new(window, cx));
        let task_worktree_requests = cx.subscribe_in(
            &inspector,
            window,
            |this, _, request: &tasks_panel::TaskWorktreeRequest, window, cx| {
                this.open_task_worktree(request.clone(), window, cx)
            },
        );
        let task_detail_requests = cx.subscribe_in(
            &inspector,
            window,
            |this, _, request: &task_detail::TaskDetailRequest, window, cx| {
                this.open_task_reader(request.clone(), None, window, cx)
            },
        );
        let task_edit_requests = cx.subscribe_in(
            &inspector,
            window,
            |this, _, request: &task_edit::TaskEditRequest, window, cx| {
                this.open_task_editor(request.clone(), window, cx)
            },
        );
        let discard_events=cx.subscribe_in(&inspector,window,|this,_,request:&changes_panel::DiscardRequest,window,cx|{
            if this.confirmation.is_some()||this.task_detail_modal.is_some(){return;}
            let path=request.path.clone();let file=request.file.clone();let target=this.state.changes.clone();
            let progress = target.clone();
            let modal=cx.new(|cx| {
                let mut modal = modal::Confirmation::new("Discard changes?",format!("Discard working tree changes to {}? Untracked files will be deleted. This cannot be undone.",file.path.display()),"Discard",this.focus.clone(),move|cx|target.update(cx,|state,cx|state.edit(path.clone(),canopy_desktop::git::changes::Edit::Discard(file.clone()),cx)),window,cx);
                modal.wait_for(&progress, |state| (state.busy, state.error.clone()), cx);
                modal
            });
            this.confirmation_events=Some(cx.subscribe(&modal,|this,_,_:&ModalDismissed,cx|{this.confirmation=None;this.confirmation_events=None;cx.notify();}));this.confirmation=Some(modal);cx.notify();
        });
        let upstream_events = cx.subscribe_in(
            &state.changes,
            window,
            |this, _, request: &crate::app_state::UpstreamRequest, window, cx| {
                if this.modal.is_some()
                    || this.task_detail_modal.is_some()
                    || this.task_editor_modal.is_some()
                    || this.confirmation.is_some()
                    || this.worktree_modal.is_some()
                    || this.upstream_modal.is_some()
                {
                    return;
                }
                let dialog = cx.new(|cx| {
                    upstream_dialog::UpstreamDialog::new(
                        request.clone(),
                        this.focus.clone(),
                        window,
                        cx,
                    )
                });
                this.upstream_modal_events =
                    Some(cx.subscribe(&dialog, |this, _, _: &ModalDismissed, cx| {
                        this.upstream_modal = None;
                        this.upstream_modal_events = None;
                        cx.notify();
                    }));
                this.upstream_modal = Some(dialog);
                cx.notify();
            },
        );
        Self {
            _activation_observer: activation_observer,
            _files_observer: files_observer,
            _new_file_events: new_file_events,
            quick_open: None,
            quick_open_events: None,
            _editor_observers: editor_observers,
            screen_id: None,
            screen_reveal: Presence::new(true, presets::CONTENT_REVEAL, Instant::now()),
            sidebar,
            sidebar_open: true,
            sidebar_reveal: Presence::new(true, presets::PANEL, Instant::now()),
            state,
            _state_observers: observers,
            #[cfg(target_os = "windows")]
            _windows_settings: windows_settings,
            inspector,
            confirmation: None,
            confirmation_events: None,
            _discard_events: discard_events,
            _upstream_events: upstream_events,
            upstream_modal: None,
            upstream_modal_events: None,
            inspector_open: true,
            inspector_reveal: Presence::new(true, presets::PANEL, Instant::now()),
            _file_events: file_events,
            _file_directory_events: file_directory_events,
            opened_file: None,
            focus,
            preferences: None,
            worktree_modal: None,
            worktree_modal_events: None,
            _worktree_requests: worktree_requests,
            _task_worktree_requests: task_worktree_requests,
            _task_detail_requests: task_detail_requests,
            _task_edit_requests: task_edit_requests,
            task_editor_modal: None,
            task_editor_events: None,
            task_detail_modal: None,
            task_detail_events: None,
            modal: None,
            modal_events: None,
            hovered_tab: None,
            tab_scroll: workspace_tabs::TabScroll::new(),
            drop_hint: None,
        }
    }

    fn open_task_reader(
        &mut self,
        request: task_detail::TaskDetailRequest,
        notice: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || self.worktree_modal.is_some()
            || self.confirmation.is_some()
            || self.upstream_modal.is_some()
            || self.state.settings.read(cx).quitting
        {
            return;
        }
        let focus = window.focused(cx).unwrap_or_else(|| self.focus.clone());
        let dialog = cx.new(|cx| {
            let mut dialog = task_detail::TaskDetailDialog::new(request, focus, window, cx);
            dialog.notice = notice;
            dialog
        });
        self.task_detail_events = Some(cx.subscribe_in(
            &dialog,
            window,
            |this, dialog, _: &ModalDismissed, window, cx| {
                let worktree = dialog.read(cx).pending_worktree.clone();
                let edit = dialog.read(cx).pending_edit.clone();
                let reader = dialog.read(cx).pending_reader.clone();
                this.task_detail_modal = None;
                this.task_detail_events = None;
                if let Some(request) = edit {
                    this.open_task_editor(request, window, cx);
                } else if let Some(request) = reader {
                    this.open_task_reader(request, None, window, cx);
                } else if let Some(request) = worktree {
                    this.open_task_worktree(request, window, cx);
                }
                let hidden = this.task_detail_modal.is_some() || this.task_editor_modal.is_some();
                this.state
                    .agents
                    .update(cx, |s, cx| s.window_obscured(hidden, cx));
                cx.notify();
            },
        ));
        self.task_detail_modal = Some(dialog);
        self.state
            .agents
            .update(cx, |s, cx| s.window_obscured(true, cx));
        cx.notify();
    }
    fn open_task_editor(
        &mut self,
        request: task_edit::TaskEditRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || self.worktree_modal.is_some()
            || self.confirmation.is_some()
            || self.upstream_modal.is_some()
            || self.state.settings.read(cx).quitting
        {
            return;
        }
        let focus = window.focused(cx).unwrap_or_else(|| self.focus.clone());
        let dialog = cx.new(|cx| task_edit::TaskEditorDialog::new(request, focus, window, cx));
        self.task_editor_events = Some(cx.subscribe_in(
            &dialog,
            window,
            |this, dialog, _: &ModalDismissed, window, cx| {
                let form = dialog.read(cx);
                let task = if form.return_to_task {
                    form.saved.clone().or_else(|| form.request.task().cloned())
                } else {
                    None
                };
                let path = form.request.path().clone();
                let notice = form.notice.clone();
                this.task_editor_modal = None;
                this.task_editor_events = None;
                if this.state.integrations.read(cx).path.as_ref() == Some(&path)
                    && !this.state.settings.read(cx).quitting
                    && let Some(task) = task
                {
                    this.open_task_reader(
                        task_detail::TaskDetailRequest { task, path },
                        notice,
                        window,
                        cx,
                    );
                }
                let hidden = this.task_detail_modal.is_some() || this.task_editor_modal.is_some();
                this.state
                    .agents
                    .update(cx, |s, cx| s.window_obscured(hidden, cx));
                cx.notify();
            },
        ));
        self.task_editor_modal = Some(dialog);
        self.state
            .agents
            .update(cx, |s, cx| s.window_obscured(true, cx));
        cx.notify();
    }
    fn open_task_worktree(
        &mut self,
        request: tasks_panel::TaskWorktreeRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || self.worktree_modal.is_some()
            || self.confirmation.is_some()
            || self.upstream_modal.is_some()
        {
            return;
        }
        let dialog = cx.new(|cx| {
            let mut dialog = worktree_dialog::WorktreeDialog::new(
                worktree_dialog::WorktreeRequest::Create(request.repository.clone()),
                self.focus.clone(),
                window,
                cx,
            );
            dialog.attach_task(request.task, window, cx);
            dialog
        });
        self.worktree_modal_events =
            Some(cx.subscribe(&dialog, |this, _, _: &ModalDismissed, cx| {
                this.worktree_modal = None;
                this.worktree_modal_events = None;
                cx.notify();
            }));
        self.worktree_modal = Some(dialog);
        cx.notify();
    }
    fn new_tab(&mut self, cx: &mut Context<Self>) {
        let id = self
            .state
            .settings
            .read(cx)
            .values
            .new_tab_tool
            .as_str()
            .to_owned();
        self.state
            .tools
            .update(cx, |state, cx| state.launch(&id, None, cx));
    }
    fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.state.workspace.update(cx, |state, cx| {
            if let Some(tab) = state.tabs().get(index) {
                let id = tab.id;
                let _ = state.activate(id);
                cx.notify();
            }
        });
    }
    fn split(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let tool = self
            .state
            .settings
            .read(cx)
            .values
            .new_tab_tool
            .as_str()
            .to_owned();
        self.state.workspace.update(cx, |state, cx| {
            if let Some(t) = state.active() {
                let (id, pane) = (t.id, t.focused);
                if state.split(id, pane, axis, tool).is_ok() {
                    cx.global::<AppState>()
                        .tools
                        .read(cx)
                        .catalog
                        .bind_default(state);
                    cx.notify();
                }
            }
        });
    }
    fn set_inspector_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.state.layout.update(cx, |state, cx| {
            state.inspector_open = open;
            cx.notify();
        });
        self.inspector_open = open;
        self.inspector_reveal
            .set_open(open, Instant::now(), motion::policy(cx));
        if !open {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
    fn select_inspector(&mut self, changes: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.inspector
            .update(cx, |view, cx| view.select(changes, cx));
        self.set_inspector_open(true, window, cx);
    }
    fn show_session_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let return_focus = self
            .state
            .workspace
            .read(cx)
            .active()
            .and_then(|tab| self.state.terminals.read(cx).views.get(&tab.focused))
            .map(|view| view.read(cx).focus_handle())
            .unwrap_or_else(|| self.focus.clone());
        self.select_inspector(false, window, cx);
        self.inspector.update(cx, |inspector, cx| {
            inspector.focus_sessions(return_focus, window, cx)
        });
    }
    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        self.state.layout.update(cx, |state, cx| {
            state.sidebar_open = self.sidebar_open;
            cx.notify();
        });
        self.sidebar_reveal
            .set_open(self.sidebar_open, Instant::now(), motion::policy(cx));
        if !self.sidebar_open {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
}
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hidden = self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || self.confirmation.is_some()
            || self.quick_open.is_some()
            || self.worktree_modal.is_some()
            || self.upstream_modal.is_some();
        self.state
            .editors
            .update(cx, |editors, cx| editors.obscure_media(hidden, cx));
        let active = self.state.projects.read(cx).catalog.current().cloned();
        let has_project = active.is_some();
        let now = Instant::now();
        let screen_progress = self.screen_reveal.progress(now);
        motion::request_frame(window, self.screen_reveal.is_animating(now));
        let sidebar_animating = self.sidebar_reveal.is_animating(now);
        let inspector_animating = self.inspector_reveal.is_animating(now);
        let sidebar_progress = self.sidebar_reveal.progress(now);
        let inspector_progress = self.inspector_reveal.progress(now);
        let widths = self.state.layout.read(cx).widths;
        let layout = widths.layout(
            f32::from(window.viewport_size().width),
            sidebar_progress,
            inspector_progress,
        );
        motion::request_frame(window, sidebar_animating || inspector_animating);
        let tabs = self.render_tabs(
            f32::from(window.viewport_size().width) - layout.left,
            window,
            cx,
        );
        let main = column().flex_1().min_w_0().h_full().child(tabs).child(
            row()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .child(
                    div()
                        .w(px(layout.terminal))
                        .h_full()
                        .min_w_0()
                        .children(
                            (has_project && self.state.workspace.read(cx).tabs().is_empty()).then(
                                || {
                                    let tools = self.state.tools.read(cx);
                                    let shell_enabled =
                                        tools.catalog.get("shell").is_some_and(|tool| tool.enabled);
                                    empty_state(
                                        primary_button("open-empty-shell", "Open shell")
                                            .disabled(
                                                !tools.ready
                                                    || !shell_enabled
                                                    || self.state.git.read(cx).busy
                                                    || self.state.projects.read(cx).busy,
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if this.state.workspace.read(cx).tabs().is_empty() {
                                                    this.state.tools.update(cx, |tools, cx| {
                                                        tools.launch("shell", None, cx)
                                                    });
                                                }
                                            })),
                                        (!shell_enabled).then(|| {
                                            "Enable Shell in Preferences to use this action.".into()
                                        }),
                                        None,
                                    )
                                    .size_full()
                                },
                            ),
                        )
                        .children(self.state.workspace.read(cx).active().cloned().map(|tab| {
                            self.render_tree(
                                &tab.root,
                                tab.id,
                                tab.focused,
                                Bounds::new(
                                    point(px(layout.left), px(40. + t::TABS)),
                                    size(
                                        px(layout.terminal),
                                        window.viewport_size().height
                                            - px(40. + t::TABS + t::STATUS),
                                    ),
                                ),
                                cx,
                            )
                        })),
                )
                .child(
                    div()
                        .relative()
                        .w(px(layout.right))
                        .h_full()
                        .flex_shrink_0()
                        .overflow_hidden()
                        .children((self.inspector_open || inspector_animating).then(|| {
                            div()
                                .absolute()
                                .top_0()
                                .right_0()
                                .w(px(layout.panels.right))
                                .h_full()
                                .opacity(inspector_progress)
                                .child(self.inspector.clone())
                        })),
                ),
        );
        column()
            .id("workspace")
            .key_context(
                if self.modal.is_some()
                    || self.task_detail_modal.is_some()
                    || self.task_editor_modal.is_some()
                    || self.upstream_modal.is_some()
                    || self.worktree_modal.is_some()
                    || self.confirmation.is_some()
                {
                    "WorkspaceModal"
                } else {
                    "Workspace"
                },
            )
            .on_action(cx.listener(|this, _: &OpenFolder, window, cx| {
                this.state
                    .projects
                    .update(cx, |state, cx| state.open_folder(window, cx));
            }))
            .on_action(cx.listener(|this, _: &CloseProject, _, cx| {
                if this.state.git.read(cx).busy {
                    return;
                }
                this.state.projects.update(cx, |state, cx| {
                    if let Some(project) = state.catalog.current() {
                        let root = project
                            .repository_path
                            .as_ref()
                            .unwrap_or(&project.path)
                            .clone();
                        state.close_repository(&root, cx);
                    }
                });
            }))
            .on_drag_move::<SplitResize>(cx.listener(
                |this, event: &DragMoveEvent<SplitResize>, _, cx| {
                    this.drop_hint = None;
                    let drag = *event.drag(cx);
                    let ratio = match drag.axis {
                        Axis::Horizontal => {
                            f32::from(event.event.position.x - drag.bounds.origin.x)
                                / f32::from(drag.bounds.size.width).max(1.)
                        }
                        Axis::Vertical => {
                            f32::from(event.event.position.y - drag.bounds.origin.y)
                                / f32::from(drag.bounds.size.height).max(1.)
                        }
                    };
                    this.state.workspace.update(cx, |state, cx| {
                        if state.set_ratio(drag.tab, drag.split, ratio).is_ok() {
                            cx.notify();
                        }
                    });
                },
            ))
            .on_drag_move::<PanelSide>(cx.listener(
                |this, event: &DragMoveEvent<PanelSide>, window, cx| {
                    this.drop_hint = None;
                    let side = *event.drag(cx);
                    let width = match side {
                        PanelSide::Left => f32::from(event.event.position.x),
                        PanelSide::Right => {
                            f32::from(window.viewport_size().width - event.event.position.x)
                        }
                    };
                    this.state.layout.update(cx, |state, cx| {
                        state.resize(side, width, f32::from(window.viewport_size().width));
                        cx.notify();
                    });
                    cx.notify();
                },
            ))
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &ToggleSidebar, window, cx| this.toggle_sidebar(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleInspector, window, cx| {
                this.set_inspector_open(!this.inspector_open, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowChanges, window, cx| {
                this.select_inspector(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowSession, window, cx| {
                this.show_session_inspector(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShellTab, _, cx| {
                this.select_tab(0, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CodexTab, _, cx| {
                this.select_tab(1, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewTab, _, cx| this.new_tab(cx)))
            .on_action(cx.listener(|this, _: &QuickOpen, window, cx| {
                if this.quick_open.is_some()
                    || this.modal.is_some()
                    || this.task_detail_modal.is_some()
                    || this.task_editor_modal.is_some()
                    || this.state.files.read(cx).root.is_none()
                {
                    return;
                }
                let view = cx.new(|cx| quick_open::QuickOpen::new(this.focus.clone(), window, cx));
                this.quick_open_events =
                    Some(cx.subscribe(&view, |this, _, _: &ModalDismissed, cx| {
                        this.quick_open = None;
                        this.quick_open_events = None;
                        cx.notify();
                    }));
                this.quick_open = Some(view);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SaveFile, _, cx| {
                let pane = this.state.workspace.read(cx).active().map(|t| t.focused);
                if let Some(view) =
                    pane.and_then(|p| this.state.editors.read(cx).views.get(&p).cloned())
                {
                    view.update(cx, |view, cx| view.save(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
                if let Some(tab) = this.state.workspace.read(cx).active().map(|t| t.id) {
                    this.close_workspace_tab(this.state.workspace.read(cx).id, tab, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &SplitHorizontal, _, cx| this.split(Axis::Horizontal, cx)),
            )
            .on_action(cx.listener(|this, _: &SplitVertical, _, cx| this.split(Axis::Vertical, cx)))
            .on_action(cx.listener(|this, _: &ClosePane, _, cx| {
                let target = this
                    .state
                    .workspace
                    .read(cx)
                    .active()
                    .map(|t| (t.id, t.focused));
                if let Some((tab, pane)) = target {
                    this.close_editor_pane(tab, pane, cx);
                }
            }))
            .relative()
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .text_size(px(12.))
            .child(
                workspace_titlebar("workspace-titlebar", window)
                    .relative()
                    .h(px(40.))
                    .flex_shrink_0()
                    .justify_center()
                    .bg(t::sidebar())
                    .border_b_1()
                    .border_color(t::border())
                    .text_color(t::muted())
                    .text_size(px(12.))
                    .child(
                        active
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "Canopy".into()),
                    )
                    .children(has_project.then(|| {
                        div()
                            .absolute()
                            .left(px(titlebar_leading_inset()))
                            .top(px(10.))
                            .child(
                                icon_button(
                                    "toggle-sidebar",
                                    if self.sidebar_open {
                                        IconName::PanelLeftClose
                                    } else {
                                        IconName::PanelLeftOpen
                                    },
                                    if self.sidebar_open {
                                        "Hide sidebar"
                                    } else {
                                        "Show sidebar"
                                    },
                                )
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.toggle_sidebar(window, cx)
                                    },
                                )),
                            )
                    }))
                    .children(has_project.then(|| {
                        div()
                            .absolute()
                            .right(px(t::SPACING_UNIT * 3. + titlebar_trailing_inset()))
                            .top(px(10.))
                            .child(
                                icon_button(
                                    "toggle-inspector",
                                    if self.inspector_open {
                                        IconName::PanelRightClose
                                    } else {
                                        IconName::PanelRightOpen
                                    },
                                    if self.inspector_open {
                                        "Hide inspector"
                                    } else {
                                        "Show inspector"
                                    },
                                )
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.set_inspector_open(!this.inspector_open, window, cx);
                                    },
                                )),
                            )
                    })),
            )
            .children((!has_project).then(|| {
                let project_state = self.state.projects.read(cx);
                let settings = self.state.settings.read(cx);
                let error = project_state
                    .error
                    .clone()
                    .or_else(|| settings.error.clone())
                    .or_else(|| self.state.tools.read(cx).error.clone());
                let error = error
                    .map(SharedString::from)
                    .or_else(|| self.state.notch_status.read(cx).error.clone());
                empty_state(
                    button("open-folder", "Open folder")
                        .disabled(!project_state.ready || project_state.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.state
                                .projects
                                .update(cx, |state, cx| state.open_folder(window, cx))
                        })),
                    (project_state.busy || !project_state.ready)
                        .then(|| "Restoring workspace…".into()),
                    error,
                )
                .relative()
                .top(px(motion::distance::BASE * (1. - screen_progress)))
                .opacity(screen_progress)
            }))
            .children(has_project.then(|| {
                row()
                    .relative()
                    .opacity(screen_progress)
                    .top(px(motion::distance::BASE * (1. - screen_progress)))
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(
                        div()
                            .w(px(layout.left))
                            .h_full()
                            .flex_shrink_0()
                            .overflow_hidden()
                            .children((self.sidebar_open || sidebar_animating).then(|| {
                                div()
                                    .w(px(layout.panels.left))
                                    .h_full()
                                    .opacity(sidebar_progress)
                                    .child(self.sidebar.clone())
                            })),
                    )
                    .child(main)
                    .children((self.sidebar_open && !sidebar_animating).then(|| {
                        pane_divider("resize-left", PanelSide::Left).left(px(layout.left - 4.))
                    }))
                    .children((self.inspector_open && !inspector_animating).then(|| {
                        pane_divider("resize-right", PanelSide::Right)
                            .left(px(layout.left + layout.terminal - 4.))
                            .top(px(t::TABS))
                    }))
            }))
            .children(has_project.then(|| {
                row()
                    .h(px(t::STATUS))
                    .flex_shrink_0()
                    .px(px(12.))
                    .gap(px(14.))
                    .border_t_1()
                    .border_color(t::border())
                    .bg(t::sidebar())
                    .text_size(px(11.))
                    .text_color(t::faint())
                    .child(
                        div()
                            .text_color(t::secondary())
                            .child(self.opened_file.clone().unwrap_or_else(|| "Idle".into())),
                    )
                    .children(
                        self.state
                            .projects
                            .read(cx)
                            .error
                            .clone()
                            .map(|e| div().text_color(t::red()).child(e)),
                    )
                    .children(
                        self.state
                            .notch_status
                            .read(cx)
                            .error
                            .clone()
                            .map(|error| div().text_color(t::red()).child(error)),
                    )
                    .child(div().flex_1())
                    .child(custom_icon("git-branch"))
                    .child(
                        self.state
                            .projects
                            .read(cx)
                            .catalog
                            .current()
                            .map(|p| {
                                self.state
                                    .git
                                    .read(cx)
                                    .repository(&p.path)
                                    .and_then(|r| r.worktrees.iter().find(|wt| wt.path == p.path))
                                    .map(|wt| wt.head.description())
                                    .unwrap_or_else(|| "No Git repository".into())
                            })
                            .unwrap_or_default(),
                    )
                    .children(
                        self.state
                            .settings
                            .read(cx)
                            .values
                            .resource_usage
                            .then(|| badge("0% · 725 MB")),
                    )
                    .child(dot())
                    .child(badge("1"))
                    .child(
                        icon_button("settings", IconName::Settings, "Settings").on_click(
                            cx.listener(|this, _, _, cx| {
                                if let Some(handle) = this.preferences
                                    && handle
                                        .update(cx, |_, window, _| window.activate_window())
                                        .is_ok()
                                {
                                    return;
                                }
                                match preferences::open(cx) {
                                    Ok(handle) => this.preferences = Some(handle),
                                    Err(error) => {
                                        eprintln!("Failed to open Preferences: {error:#}")
                                    }
                                }
                            }),
                        ),
                    )
            }))
            .child(self.state.toasts.clone())
            .children(
                Root::render_sheet_layer(window, cx)
                    .map(|layer| div().absolute().inset_0().child(layer)),
            )
            .children(
                Root::render_dialog_layer(window, cx)
                    .map(|layer| div().absolute().inset_0().child(layer)),
            )
            .children(Root::render_notification_layer(window, cx))
            .children(self.confirmation.clone())
            .children(self.upstream_modal.clone())
            .children(self.modal.clone())
            .children(self.quick_open.clone())
            .children(self.worktree_modal.clone())
            .children(self.task_detail_modal.clone())
            .children(self.task_editor_modal.clone())
    }
}
