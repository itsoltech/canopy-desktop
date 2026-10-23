pub mod assets;
#[cfg(target_os = "macos")]
mod attachment_native;
mod attachment_preview;
mod changes_panel;
pub(crate) mod chat;
pub mod components;
mod conversations;
#[cfg(any(debug_assertions, feature = "dev-inspector"))]
pub mod dev_inspector;
pub(crate) mod diff;
pub(crate) mod editor;
pub(crate) mod file_icons;
mod file_nodes;
pub(crate) mod font_preview;
mod history;
mod html_native;
pub(crate) mod image_preview;
mod inspector;
mod jira;
mod markdown;
mod new_chat_palette;
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
mod workspace_layout;
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
        NewChat,
        NewFile,
        NewChatNoProject,
        NewChatIn,
        SplitHorizontal,
        SplitVertical,
        ClosePane
    ]
);

pub struct Workspace {
    focus: FocusHandle,
    _activation_observer: Subscription,
    _files_observer: Subscription,
    _chat_requests: Subscription,
    quick_open: Option<Entity<quick_open::QuickOpen>>,
    quick_open_events: Option<Subscription>,
    new_chat: Option<Entity<new_chat_palette::NewChatPalette>>,
    new_chat_events: Option<Subscription>,
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
            cx.observe_in(&state.projects, window, |this, entity, _, cx| {
                let id = entity.read(cx).catalog.active;
                // The layout stays mounted for projects and "No project" alike; switching
                // spaces only resets transient view state, without an entrance animation.
                if this.screen_id != id {
                    this.screen_id = id;
                    this.opened_file = None;
                    this.hovered_tab = None;
                    this.drop_hint = None;
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
                let now = Instant::now();
                this.sidebar_reveal
                    .set_open(this.sidebar_open, now, motion::policy(cx));
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
            KeyBinding::new("cmd-n", NewChat, Some("Workspace")),
            KeyBinding::new("cmd-alt-n", NewChatNoProject, Some("Workspace")),
            KeyBinding::new("cmd-shift-n", NewChatIn, Some("Workspace")),
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
            KeyBinding::new("ctrl-shift-n", NewChat, Some("Workspace")),
            KeyBinding::new("ctrl-alt-shift-n", NewChatNoProject, Some("Workspace")),
            KeyBinding::new("ctrl-alt-n", NewChatIn, Some("Workspace")),
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
            // Chat views own a composer, so they are created with this window.
            cx.observe_in(&state.workspace, window, |this, _, window, cx| {
                this.state
                    .chats
                    .update(cx, |chats, cx| chats.sync(window, cx));
            }),
            cx.observe_in(&state.projects, window, |this, _, window, cx| {
                this.state
                    .chats
                    .update(cx, |chats, cx| chats.sync(window, cx));
            }),
            cx.observe_in(&state.tools, window, |this, _, window, cx| {
                this.state
                    .chats
                    .update(cx, |chats, cx| chats.sync(window, cx));
            }),
            cx.observe_in(&state.git, window, |this, _, window, cx| {
                this.state
                    .chats
                    .update(cx, |chats, cx| chats.sync(window, cx));
            }),
            cx.observe_in(&state.agents, window, |this, _, window, cx| {
                this.state
                    .chats
                    .update(cx, |chats, cx| chats.agents_changed(window, cx));
            }),
            cx.observe_in(&state.chats, window, |_, _, window, cx| {
                window.refresh();
                cx.notify();
            }),
            cx.subscribe_in(
                &state.editors,
                window,
                |_, _, request: &crate::app_state::GuardRequest, window, cx| {
                    crate::app_state::Editors::confirm(request, window, cx)
                },
            ),
            cx.subscribe_in(
                &state.chats,
                window,
                |_, _, request: &crate::app_state::ChatGuardRequest, window, cx| {
                    crate::app_state::Chats::confirm(request, window, cx)
                },
            ),
            // Fork under a reply: the same choice of project as "New chat in".
            cx.subscribe_in(
                &state.chats,
                window,
                |this, _, request: &crate::app_state::ChatForkRequested, window, cx| {
                    let purpose = new_chat_palette::Purpose::Fork(Box::new(request.0.clone()));
                    this.open_chat_palette(purpose, window, cx)
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
        let sidebar = cx.new(sidebar::Sidebar::new);
        let chat_requests = cx.subscribe_in(
            &sidebar,
            window,
            |this, _, request: &sidebar::RenameChat, window, cx| {
                this.rename_chat(request.workspace, request.pane, window, cx);
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
        let inspector = cx.new(|cx| inspector::Inspector::new(files, window, cx));
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
            _chat_requests: chat_requests,
            quick_open: None,
            quick_open_events: None,
            new_chat: None,
            new_chat_events: None,
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
        if self.state.projects.read(cx).catalog.active.is_none() {
            self.state.chats.update(cx, |chats, cx| {
                chats.open_new(crate::app_state::ChatTarget::NoProject, cx)
            });
            return;
        }
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
            .update(cx, |tools, cx| tools.launch(&id, None, cx));
    }
    fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.state.workspace.update(cx, |state, cx| {
            let id = state.tool_tabs().nth(index).map(|t| t.id);
            if let Some(id) = id {
                let _ = state.activate(id);
                cx.notify();
            }
        });
    }
    fn split(&mut self, axis: Axis, cx: &mut Context<Self>) {
        self.split_terminal(axis, cx);
    }
    fn set_inspector_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.state.workspace.update(cx, |state, cx| {
            if open {
                let page = state.tool_panel().page();
                state.show_tools(page);
            } else {
                state.hide_tools();
            }
            cx.notify();
        });
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
            || self.new_chat.is_some()
            || self.worktree_modal.is_some()
            || self.upstream_modal.is_some();
        self.state
            .editors
            .update(cx, |editors, cx| editors.obscure_media(hidden, cx));
        let active = self.state.projects.read(cx).catalog.current().cloned();
        let now = Instant::now();
        let screen_progress = self.screen_reveal.progress(now);
        motion::request_frame(window, self.screen_reveal.is_animating(now));
        let sidebar_animating = self.sidebar_reveal.is_animating(now);
        let sidebar_progress = self.sidebar_reveal.progress(now);
        let widths = self.state.layout.read(cx).widths;
        let layout = widths.layout(
            f32::from(window.viewport_size().width),
            sidebar_progress,
            0.,
        );
        motion::request_frame(window, sidebar_animating);
        let inspector_open = self.state.workspace.read(cx).tools_open();
        let main = self.render_work_area(
            layout.left,
            layout.terminal,
            (f32::from(window.viewport_size().height) - t::TITLEBAR - t::STATUS).max(0.),
            window,
            cx,
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
                    // Its ⌘1…⌘9 pick projects instead of switching tabs.
                    || self.new_chat.is_some()
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
            .on_drag_move::<workspace_layout::ToolResize>(cx.listener(
                |this, event: &DragMoveEvent<workspace_layout::ToolResize>, _, cx| {
                    let drag = *event.drag(cx);
                    let size = if drag.bottom {
                        drag.height - f32::from(event.event.position.y - drag.origin.y)
                    } else {
                        drag.width - f32::from(event.event.position.x - drag.origin.x)
                    };
                    this.state.workspace.update(cx, |w, cx| {
                        w.tool_panel_mut().resize(size);
                        cx.notify();
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
                this.set_inspector_open(!this.state.workspace.read(cx).tools_open(), window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowChanges, window, cx| {
                if this.state.projects.read(cx).catalog.active.is_none() {
                    return;
                }
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
            .on_action(cx.listener(|this, _: &NewFile, window, cx| this.open_new_file(window, cx)))
            .on_action(cx.listener(|this, _: &NewTab, _, cx| this.new_tab(cx)))
            .on_action(cx.listener(|this, _: &NewChat, _, cx| {
                if this.modal.is_some()
                    || this.new_chat.is_some()
                    || crate::app_state::Chats::unavailable(cx).is_some()
                {
                    return;
                }
                this.state
                    .chats
                    .update(cx, |chats, cx| chats.open_current(cx));
            }))
            .on_action(cx.listener(|this, _: &NewChatIn, window, cx| {
                this.open_chat_palette(new_chat_palette::Purpose::NewChat, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewChatNoProject, _, cx| {
                if this.modal.is_none() && this.new_chat.is_none() {
                    this.state.chats.update(cx, |chats, cx| {
                        chats.open_new(crate::app_state::ChatTarget::NoProject, cx)
                    });
                }
            }))
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
                let w = this.state.workspace.read(cx);
                if w.tools_open()
                    && w.tool_panel().is_focused()
                    && w.tool_panel().page() != canopy_desktop::state::workspace::ToolPage::Tabs
                {
                    this.state.workspace.update(cx, |w, cx| {
                        w.show_tools(canopy_desktop::state::workspace::ToolPage::Tabs);
                        cx.notify();
                    });
                    return;
                }
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
                    .h(px(t::TITLEBAR))
                    .flex_shrink_0()
                    .justify_start()
                    .pl(px(titlebar_leading_inset() + 32.))
                    .pr(px(titlebar_trailing_inset() + 44.))
                    .bg(t::sidebar())
                    .border_b_1()
                    .border_color(t::border())
                    .text_color(t::secondary())
                    .text_sm()
                    .child({
                        let workspace = self.state.workspace.read(cx);
                        let title = if workspace.tool_panel().is_maximized() {
                            workspace
                                .tool_tab()
                                .map(|tab| match workspace.tool_panel().page() {
                                    canopy_desktop::state::workspace::ToolPage::Tabs => {
                                        format!("Terminal · {}", tab.title)
                                    }
                                    canopy_desktop::state::workspace::ToolPage::Files => {
                                        "Files".into()
                                    }
                                    canopy_desktop::state::workspace::ToolPage::Changes => {
                                        "Changes".into()
                                    }
                                    canopy_desktop::state::workspace::ToolPage::Tasks => {
                                        "Tasks".into()
                                    }
                                    canopy_desktop::state::workspace::ToolPage::Agent => {
                                        "Agent details".into()
                                    }
                                })
                        } else {
                            workspace.primary_tab().map(|tab| {
                                format!(
                                    "Chat · {}",
                                    workspace.chat_title(tab.focused).unwrap_or(&tab.title)
                                )
                            })
                        }
                        .unwrap_or_else(|| "Canopy".into());
                        let label = active
                            .as_ref()
                            .map(|p| {
                                let branch = self
                                    .state
                                    .git
                                    .read(cx)
                                    .repository(&p.path)
                                    .and_then(|r| r.worktrees.iter().find(|wt| wt.path == p.path))
                                    .map(|wt| wt.label.as_str());
                                match branch {
                                    Some(branch) => format!("{} / {} · {title}", p.name, branch),
                                    None => format!("{} · {title}", p.name),
                                }
                            })
                            .unwrap_or(title);
                        div().flex_1().min_w_0().truncate().child(label)
                    })
                    .child({
                        div()
                            .absolute()
                            .left(px(titlebar_leading_inset()))
                            .top(px(8.))
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
                    })
                    .child({
                        div()
                            .absolute()
                            .right(px(t::SPACING_UNIT * 3. + titlebar_trailing_inset()))
                            .top(px(8.))
                            .child(
                                icon_button(
                                    "toggle-inspector",
                                    if inspector_open {
                                        IconName::PanelRightClose
                                    } else {
                                        IconName::PanelRightOpen
                                    },
                                    if inspector_open {
                                        "Hide tools"
                                    } else {
                                        "Show tools"
                                    },
                                )
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.set_inspector_open(
                                            !this.state.workspace.read(cx).tools_open(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                    }),
            )
            .child({
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
            })
            .child({
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
                    .children(
                        self.state
                            .notch_status
                            .read(cx)
                            .error
                            .clone()
                            .map(|error| div().text_color(t::red()).child(error)),
                    )
                    .child(div().flex_1())
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
                    .children(dev_tools_button(window, cx))
            })
            .child(self.state.toasts.clone())
            .children(self.confirmation.clone())
            .children(self.upstream_modal.clone())
            .children(self.modal.clone())
            .children(self.quick_open.clone())
            .children(self.new_chat.clone())
            .children(self.worktree_modal.clone())
            .children(self.task_detail_modal.clone())
            .children(self.task_editor_modal.clone())
    }
}

impl Workspace {
    /// "New chat in" / "Fork into": the projects to put the chat in.
    fn open_chat_palette(
        &mut self,
        purpose: new_chat_palette::Purpose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_chat.is_some()
            || self.quick_open.is_some()
            || self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || crate::app_state::Chats::unavailable(cx).is_some()
        {
            return;
        }
        let view = cx.new(|cx| {
            new_chat_palette::NewChatPalette::new(purpose, self.focus.clone(), window, cx)
        });
        self.new_chat_events = Some(cx.subscribe(&view, |this, _, _: &ModalDismissed, cx| {
            this.new_chat = None;
            this.new_chat_events = None;
            cx.notify();
        }));
        self.new_chat = Some(view);
        cx.notify();
    }
}

/// GPUI's layout inspector (⌥⌘I: pick an element, see and edit its box and styles), in
/// development builds only; release builds don't have it (`debug_assertions`, or the
/// `dev-inspector` feature on purpose).
#[cfg(any(debug_assertions, feature = "dev-inspector"))]
fn dev_tools_button(window: &Window, cx: &App) -> Option<AnyElement> {
    let picking = window.is_inspector_picking(cx);
    Some(
        icon_button_base("dev-tools", "Layout inspector (⌥⌘I)")
            .icon(custom_icon("braces").text_color(if picking { t::accent() } else { t::muted() }))
            .tooltip("Layout inspector (⌥⌘I) — development builds only")
            .on_click(|_, window, cx| window.toggle_inspector(cx))
            .into_any_element(),
    )
}

#[cfg(not(any(debug_assertions, feature = "dev-inspector")))]
fn dev_tools_button(_: &Window, _: &App) -> Option<AnyElement> {
    None
}
