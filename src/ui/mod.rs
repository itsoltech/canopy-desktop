pub mod assets;
#[cfg(target_os = "macos")]
mod attachment_native;
mod attachment_preview;
mod changes_panel;
pub(crate) mod chat;
mod chat_recovery;
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
pub(crate) mod keymap;
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
mod pull_request;
mod quick_open;
mod remote_chat;
pub mod remote_client;
mod resume_palette;
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
#[cfg(test)]
mod visual_tests;
pub(crate) mod window_geometry;
#[cfg(target_os = "windows")]
mod windows_settings;
mod workspace_layout;
mod workspace_panes;
mod workspace_tabs;
mod worktree_dialog;
mod worktree_overview;
pub(crate) mod youtrack;
use workspace_panes::{Placement, SplitResize};
pub mod theme;

use crate::app_state::AppState;
use canopy_desktop::motion::{self, Presence, presets};
use canopy_desktop::state::workspace::{Axis, PaneId, WorkspaceId};
use components::file_tree::{FileTree, FileTreeEvent, FileTreeModel, FileTreeRequest};
use components::*;
use gpui_kit::base::{Disableable, FocusTrapElement};
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
        ShowTerminals,
        ShowFiles,
        ShowTasks,
        ShowPreferences,
        DismissNavigation,
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
    /// `/resume`: the closed chats to bring back.
    resume: Option<Entity<resume_palette::ResumePalette>>,
    resume_events: Option<Subscription>,
    _editor_observers: Vec<Subscription>,
    screen_id: Option<WorkspaceId>,
    screen_reveal: Presence,
    sidebar: Entity<sidebar::Sidebar>,
    remote: Option<Entity<remote_client::RemoteWorkspace>>,
    remote_tools: Option<(
        String,
        canopy_desktop::state::workspace::ToolPage,
        Entity<remote_client::RemoteWorkspace>,
    )>,
    _remote_navigation: Subscription,
    _remote_chat_navigation: Option<Subscription>,
    sidebar_open: bool,
    sidebar_overlay: bool,
    sidebar_overlay_focus: FocusHandle,
    sidebar_return_focus: Option<FocusHandle>,
    navigation_selection: Option<PaneId>,
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
    _pull_request_requests: Subscription,
    /// The create or details dialog of a pull request.
    pull_request_modal: Option<AnyView>,
    pull_request_modal_events: Option<Subscription>,
    recovery_modal: Option<Entity<chat_recovery::ChatRecoveryDialog>>,
    recovery_modal_events: Option<Subscription>,
    modal: Option<Entity<TextPrompt>>,
    modal_events: Option<Subscription>,
    tab_scroll: workspace_tabs::TabScroll,
    hovered_tab: Option<canopy_desktop::state::workspace::TabId>,
    drop_hint: Option<(PaneId, Placement)>,
    hover_cards: Entity<components::HoverCards>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<AppState>().clone();
        #[cfg(target_os = "windows")]
        let windows_settings = {
            let executor = cx.foreground_executor().clone();
            let async_cx = cx.to_async();
            match windows_settings::SettingsMonitor::install(window, move || {
                let async_cx = async_cx.clone();
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
            cx.observe_window_bounds(window, |this, window, cx| {
                if this.sidebar_overlay
                    && f32::from(window.viewport_size().width) >= f32::from(window.rem_size()) * 43.
                {
                    this.dismiss_navigation(window, cx);
                }
            }),
            cx.observe(&state.diffs, |_, _, cx| cx.notify()),
            // The Changes tab counts the worktree's changed files.
            cx.observe(&state.changes, |_, _, cx| cx.notify()),
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
                // The layout stays mounted for projects and "No project" alike; switching
                // spaces only resets transient view state, without an entrance animation.
                if this.screen_id != id {
                    this.remote = None;
                    this.sidebar
                        .update(cx, |sidebar, cx| sidebar.local_selection(cx));
                    this.screen_id = id;
                    this.opened_file = None;
                    this.hovered_tab = None;
                    this.drop_hint = None;
                    // The focused pane belonged to the space left: actions and shortcuts
                    // (New chat, ⌘N) would go to the window's root and be lost until
                    // something here took focus. A pane shown here takes it after this.
                    this.focus.focus(window, cx);
                }
                cx.notify();
            }),
            cx.observe_in(&state.workspace, window, |this, _, _, cx| {
                let selected = this.state.workspace.read(cx).selected_pane().map(|p| p.id);
                if selected != this.navigation_selection {
                    this.remote = None;
                    this.sidebar
                        .update(cx, |sidebar, cx| sidebar.local_selection(cx));
                    this.sidebar_overlay = false;
                    this.sidebar_return_focus = None;
                }
                this.navigation_selection = selected;
                cx.notify();
            }),
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
        // Commands and their (possibly custom) shortcuts: Settings → Shortcuts.
        keymap::init(cx);
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
                this.sync_remote_tools(window, cx);
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
            // Catalog chats without a pane (created remotely) appear in the sidebar.
            cx.observe_in(&state.settings, window, |this, _, window, cx| {
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
            cx.observe_in(&state.chats, window, |this, _, window, cx| {
                this.sync_remote_tools(window, cx);
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
            // A conversation kept without its agent and account: the user picks them.
            cx.subscribe_in(
                &state.chats,
                window,
                |this, _, request: &crate::ui::chat::ClientFileRequested, window, cx| {
                    if this.focused_remote(cx).as_ref() != Some(&request.runtime) {
                        return;
                    }
                    this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Files, cx);
                    this.sync_remote_tools(window, cx);
                    if let Some((_, _, view)) = &this.remote_tools {
                        view.update(cx, |view, cx| view.open_file(request.file.clone(), cx));
                    }
                },
            ),
            cx.subscribe_in(
                &state.chats,
                window,
                |this, _, request: &crate::ui::chat::ClientForkRequested, window, cx| {
                    this.open_chat_palette(
                        new_chat_palette::Purpose::RemoteFork(request.clone()),
                        window,
                        cx,
                    )
                },
            ),
            cx.subscribe_in(
                &state.chats,
                window,
                |this, _, request: &crate::app_state::ChatRecoveryRequested, window, cx| {
                    if this.recovery_modal.is_some()
                        || this.upstream_modal.is_some()
                        || this.pull_request_modal.is_some()
                    {
                        return;
                    }
                    let chat = request.0.clone();
                    let dialog = cx.new(|cx| {
                        chat_recovery::ChatRecoveryDialog::new(chat, this.focus.clone(), window, cx)
                    });
                    this.recovery_modal_events =
                        Some(cx.subscribe(&dialog, |this, _, _: &ModalDismissed, cx| {
                            this.recovery_modal = None;
                            this.recovery_modal_events = None;
                            cx.notify();
                        }));
                    this.recovery_modal = Some(dialog);
                    cx.notify();
                },
            ),
            // `/resume` in a chat: the closed chats to bring back.
            cx.subscribe_in(
                &state.chats,
                window,
                |this, _, request: &crate::app_state::ChatResumeRequested, window, cx| {
                    this.open_resume(request.replace, window, cx)
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
        let remote_navigation = cx.subscribe_in(
            &sidebar,
            window,
            |this, _, event: &sidebar::Navigation, window, cx| {
                match event {
                    sidebar::Navigation::Local => this.remote = None,
                    sidebar::Navigation::Remote { host, resource } => {
                        if let Some(canopy_desktop::remote::proto::resource_ref::Resource::Chat(
                            chat,
                        )) = &resource.resource
                        {
                            this.remote = None;
                            this.open_remote_chat(host.clone(), chat.clone(), cx);
                            return;
                        }
                        if let Some(view) = &this.remote {
                            view.update(cx, |view, cx| {
                                view.open(host.clone(), resource.clone(), cx)
                            });
                        } else {
                            this.remote = Some(cx.new(|cx| {
                                remote_client::RemoteWorkspace::new(
                                    host.clone(),
                                    resource.clone(),
                                    window,
                                    cx,
                                )
                            }));
                        }
                    }
                }
                this.sidebar_overlay = false;
                cx.notify();
            },
        );
        let remote_chat_navigation = remote_client::connections(cx).ok().map(|connections| {
            cx.subscribe_in(
                &connections,
                window,
                |this, _, event: &remote_client::OpenChat, window, cx| {
                    this.remote = None;
                    this.open_remote_chat(event.host.clone(), event.chat.clone(), cx);
                    window.activate_window();
                },
            )
        });
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
                    || this.pull_request_modal.is_some()
                    || this.recovery_modal.is_some()
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
        let pull_request_requests = cx.subscribe_in(
            &state.pull_requests,
            window,
            |this, _, request: &crate::app_state::PullRequestDialog, window, cx| {
                if this.modal.is_some()
                    || this.task_detail_modal.is_some()
                    || this.task_editor_modal.is_some()
                    || this.confirmation.is_some()
                    || this.worktree_modal.is_some()
                    || this.upstream_modal.is_some()
                    || this.pull_request_modal.is_some()
                    || this.recovery_modal.is_some()
                {
                    return;
                }
                let focus = window.focused(cx).unwrap_or_else(|| this.focus.clone());
                fn dismissed(this: &mut Workspace, cx: &mut Context<Workspace>) {
                    this.pull_request_modal = None;
                    this.pull_request_modal_events = None;
                    cx.notify();
                }
                let (view, events): (AnyView, _) = match request {
                    crate::app_state::PullRequestDialog::Create(branch) => {
                        let dialog = cx.new(|cx| {
                            pull_request::CreatePullRequest::new(branch.clone(), focus, window, cx)
                        });
                        let events = cx.subscribe(&dialog, |this, _, _: &ModalDismissed, cx| {
                            dismissed(this, cx)
                        });
                        (dialog.into(), events)
                    }
                    crate::app_state::PullRequestDialog::Details(pull) => {
                        let dialog = cx.new(|cx| {
                            pull_request::PullRequestDetails::new(pull.clone(), focus, window, cx)
                        });
                        let events = cx.subscribe(&dialog, |this, _, _: &ModalDismissed, cx| {
                            dismissed(this, cx)
                        });
                        (dialog.into(), events)
                    }
                };
                this.pull_request_modal = Some(view);
                this.pull_request_modal_events = Some(events);
                cx.notify();
            },
        );
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
                    || this.pull_request_modal.is_some()
                    || this.recovery_modal.is_some()
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
            resume: None,
            resume_events: None,
            _editor_observers: editor_observers,
            screen_id: None,
            screen_reveal: Presence::new(true, presets::CONTENT_REVEAL, Instant::now()),
            sidebar,
            remote: None,
            remote_tools: None,
            _remote_navigation: remote_navigation,
            _remote_chat_navigation: remote_chat_navigation,
            sidebar_open: true,
            sidebar_overlay: false,
            sidebar_overlay_focus: cx.focus_handle(),
            sidebar_return_focus: None,
            navigation_selection: state.workspace.read(cx).selected_pane().map(|p| p.id),
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
            _pull_request_requests: pull_request_requests,
            pull_request_modal: None,
            pull_request_modal_events: None,
            recovery_modal: None,
            recovery_modal_events: None,
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
            hover_cards: components::HoverCards::install(window, cx),
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
            || self.pull_request_modal.is_some()
            || self.recovery_modal.is_some()
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
            || self.pull_request_modal.is_some()
            || self.recovery_modal.is_some()
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
            || self.pull_request_modal.is_some()
            || self.recovery_modal.is_some()
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
            let id = state.tabs().get(index).map(|t| t.id);
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
                let page = match state.tool_panel().page() {
                    canopy_desktop::state::workspace::ToolPage::Tabs => {
                        canopy_desktop::state::workspace::ToolPage::Agent
                    }
                    page => page,
                };
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
    fn dismiss_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_overlay = false;
        if let Some(focus) = self.sidebar_return_focus.take() {
            focus.focus(window, cx);
        }
        cx.notify();
    }
    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if f32::from(window.viewport_size().width) < f32::from(window.rem_size()) * 43. {
            if self.sidebar_overlay {
                self.dismiss_navigation(window, cx);
            } else {
                self.sidebar_return_focus = window.focused(cx);
                self.sidebar_overlay = true;
                self.sidebar_overlay_focus.focus(window, cx);
                cx.notify();
            }
            return;
        }
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
        let hidden = self.remote.is_some()
            || self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || self.confirmation.is_some()
            || self.quick_open.is_some()
            || self.new_chat.is_some()
            || self.resume.is_some()
            || self.worktree_modal.is_some()
            || self.upstream_modal.is_some()
            || self.pull_request_modal.is_some()
            || self.recovery_modal.is_some();
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
        let viewport_width = f32::from(window.viewport_size().width);
        let rem = f32::from(window.rem_size());
        let narrow_sidebar = viewport_width < rem * 43.;
        let sidebar_width = widths
            .left
            .max(rem * 15.)
            .min((viewport_width - rem * 28.).max(rem * 15.));
        let sidebar_width = if narrow_sidebar {
            0.
        } else {
            sidebar_width * sidebar_progress
        };
        let sidebar_visible = sidebar_width > 0.;
        let attention = if sidebar_visible {
            0
        } else {
            self.state
                .projects
                .read(cx)
                .runtime_workspaces(cx)
                .iter()
                .flat_map(|w| w.all_panes())
                .filter(|p| {
                    matches!(
                        sidebar::navigation_status(p.id, cx),
                        "Waiting" | "Needs attention" | "Error"
                    )
                })
                .count()
        };

        motion::request_frame(window, sidebar_animating);
        let inspector_open = self.state.workspace.read(cx).tools_open();
        let main = if let Some(remote) = &self.remote {
            remote.clone().into_any_element()
        } else {
            self.render_work_area(
                sidebar_width,
                (viewport_width - sidebar_width).max(0.),
                (f32::from(window.viewport_size().height) - t::TITLEBAR).max(0.),
                window,
                cx,
            )
            .into_any_element()
        };
        // The window background (Settings → Appearance) spans the work area,
        // behind tabs, panes and chats; panels keep their theme surfaces.
        let main = components::background::window_surface(
            div().relative().flex_1().min_w_0().h_full(),
            cx,
        )
        .child(main);
        column()
            .id("workspace")
            .key_context(
                if self.modal.is_some()
                    || self.task_detail_modal.is_some()
                    || self.task_editor_modal.is_some()
                    || self.upstream_modal.is_some()

                    || self.pull_request_modal.is_some()
            || self.recovery_modal.is_some()
                    || self.worktree_modal.is_some()
                    || self.confirmation.is_some()
                    // Its ⌘1…⌘9 pick projects instead of switching tabs.
                    || self.new_chat.is_some()
                    || self.resume.is_some()
                {
                    "WorkspaceModal"
                } else {
                    "Workspace"
                },
            )
            .on_action(cx.listener(|this, _: &OpenFolder, window, cx| {
                this.remote = None;
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.local_selection(cx));
                this.state
                    .projects
                    .update(cx, |state, cx| state.open_folder(window, cx));
            }))
            .on_action(cx.listener(|this, _: &CloseProject, window, cx| {
                if this.focused_remote(cx).is_some() {
                    this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Files, cx);
                    this.sync_remote_tools(window, cx);
                    if let Some((_, _, view)) = &this.remote_tools {
                        view.update(cx, |v, cx| v.close_project(cx));
                    }
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.close_project(cx));
                    return;
                }
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
                if this.remote.is_some() {
                    return;
                }
                this.set_inspector_open(!this.state.workspace.read(cx).tools_open(), window, cx)
            }))
            .on_action(cx.listener(|this, _: &DismissNavigation, window, cx| {
                if this.sidebar_overlay {
                    this.dismiss_navigation(window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ShowTerminals, _, cx| {
                if this.focused_remote(cx).is_some() {
                    this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Agent, cx);
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.show(remote_client::Page::Terminals, cx));
                    return;
                }
                this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Tabs, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowFiles, _, cx| {
                if this.focused_remote(cx).is_some() {
                    this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Files, cx);
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.show(remote_client::Page::Files, cx));
                    return;
                }
                if this.state.projects.read(cx).catalog.active.is_some() {
                    this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Files, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ShowTasks, _, cx| {
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.show(remote_client::Page::Tasks, cx));
                    return;
                }
                this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Tasks, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowPreferences, _, cx| {
                if let Some(handle) = this.preferences
                    && handle
                        .update(cx, |_, window, _| window.activate_window())
                        .is_ok()
                {
                    return;
                }
                match preferences::open(cx) {
                    Ok(handle) => this.preferences = Some(handle),
                    Err(error) => eprintln!("Failed to open Preferences: {error:#}"),
                }
            }))
            .on_action(cx.listener(|this, _: &ShowChanges, window, cx| {
                if this.focused_remote(cx).is_some() {
                    this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Changes, cx);
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.show(remote_client::Page::Git, cx));
                    return;
                }
                if this.state.projects.read(cx).catalog.active.is_none() {
                    return;
                }
                this.select_inspector(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowSession, window, cx| {
                if this.remote.is_some() {
                    return;
                }
                this.show_session_inspector(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShellTab, _, cx| {
                if this.remote.is_some() {
                    return;
                }
                this.select_tab(0, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CodexTab, _, cx| {
                if this.remote.is_some() {
                    return;
                }
                this.select_tab(1, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewFile, window, cx| {
                if this.focused_remote(cx).is_some() {
                    this.select_tool_page(canopy_desktop::state::workspace::ToolPage::Files, cx);
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.show(remote_client::Page::Files, cx));
                } else {
                    this.open_new_file(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewTab, _, cx| {
                if this.new_chat_on_active_host(cx) {
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.new_chat(cx));
                } else {
                    this.new_tab(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewChat, _, cx| {
                if this.new_chat_on_active_host(cx) {
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.new_chat(cx));
                    return;
                }
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
                this.remote = None;
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.local_selection(cx));
                this.open_chat_palette(new_chat_palette::Purpose::NewChat, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewChatNoProject, _, cx| {
                this.remote = None;
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.local_selection(cx));
                if this.modal.is_none() && this.new_chat.is_none() {
                    this.state.chats.update(cx, |chats, cx| {
                        chats.open_new(crate::app_state::ChatTarget::NoProject, cx)
                    });
                }
            }))
            .on_action(cx.listener(|this, _: &QuickOpen, window, cx| {
                if this.remote.is_some() {
                    return;
                }
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
                if this.focused_remote(cx).is_some() {
                    if let Some((_, _, view)) = &this.remote_tools {
                        view.update(cx, |view, cx| view.save(cx));
                    }
                    return;
                }
                if let Some(remote) = &this.remote {
                    remote.update(cx, |view, cx| view.save(cx));
                    return;
                }
                let pane = this.state.workspace.read(cx).active().map(|t| t.focused);
                if let Some(view) =
                    pane.and_then(|p| this.state.editors.read(cx).views.get(&p).cloned())
                {
                    view.update(cx, |view, cx| view.save(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
                if this.remote.take().is_some() {
                    this.sidebar
                        .update(cx, |sidebar, cx| sidebar.local_selection(cx));
                    cx.notify();
                    return;
                }
                let w = this.state.workspace.read(cx);
                if w.tools_open() && w.tool_panel().is_focused() {
                    this.state.workspace.update(cx, |w, cx| {
                        w.hide_tools();
                        cx.notify();
                    });
                    return;
                }
                if let Some(tab) = this.state.workspace.read(cx).active().map(|t| t.id) {
                    this.close_workspace_tab(this.state.workspace.read(cx).id, tab, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SplitHorizontal, _, cx| {
                if this.remote.is_none() {
                    this.split(Axis::Horizontal, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SplitVertical, _, cx| {
                if this.remote.is_none() {
                    this.split(Axis::Vertical, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ClosePane, _, cx| {
                if this.remote.is_some() {
                    return;
                }
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
                    .pl(px(titlebar_leading_inset()
                        + 32.
                        + if attention > 0 { 28. } else { 0. }))
                    .pr(px(titlebar_trailing_inset() + 44.))
                    .bg(t::sidebar())
                    .border_b_1()
                    .border_color(t::border())
                    .text_color(t::secondary())
                    .text_sm()
                    .child({
                        let workspace = self.state.workspace.read(cx);
                        let title = self
                            .remote
                            .as_ref()
                            .map(|remote| remote.read(cx).label(cx))
                            .or_else(|| {
                                workspace.selected_pane().map(|pane| {
                                    workspace
                                        .pane_title(pane.id)
                                        .unwrap_or_else(|| {
                                            pane.metadata.title.as_deref().unwrap_or(&pane.tool)
                                        })
                                        .to_owned()
                                })
                            });
                        let project = active
                            .as_ref()
                            .map(|p| {
                                let root = p.repository_path.as_ref().unwrap_or(&p.path);
                                let name = root.file_name().unwrap_or_default().to_string_lossy();
                                let branch = self
                                    .state
                                    .git
                                    .read(cx)
                                    .repository(root)
                                    .and_then(|r| r.worktrees.iter().find(|wt| wt.path == p.path))
                                    .map(|wt| wt.label.as_str());
                                branch.map_or_else(
                                    || name.to_string(),
                                    |branch| format!("{name} / {branch}"),
                                )
                            })
                            .unwrap_or_else(|| "Private chats".into());
                        let show_context = self.remote.is_none()
                            && (title.is_none() || viewport_width >= rem * 52.);
                        let project = if viewport_width < rem * 64. && title.is_some() {
                            active
                                .as_ref()
                                .and_then(|p| {
                                    self.state
                                        .git
                                        .read(cx)
                                        .repository(p.repository_path.as_ref().unwrap_or(&p.path))
                                        .and_then(|r| {
                                            r.worktrees.iter().find(|wt| wt.path == p.path)
                                        })
                                        .map(|wt| wt.label.clone())
                                })
                                .unwrap_or(project)
                        } else {
                            project
                        };
                        row()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .gap_2()
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(MouseButton::Left, |event, window, _| {
                                #[cfg(target_os = "macos")]
                                if event.click_count == 2 {
                                    window.titlebar_double_click();
                                }
                                #[cfg(not(target_os = "macos"))]
                                let _ = (event, window);
                            })
                            .children(show_context.then(|| {
                                div()
                                    .max_w(relative(0.4))
                                    .text_color(t::muted())
                                    .truncate()
                                    .child(project)
                            }))
                            .children(title.map(|title| {
                                row()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_2()
                                    .children(
                                        show_context
                                            .then(|| div().text_color(t::faint()).child("/")),
                                    )
                                    .child(div().truncate().text_color(t::text()).child(title))
                            }))
                    })
                    .children(dev_tools_button(window, cx))
                    .child({
                        row()
                            .gap_1()
                            .absolute()
                            .left(px(titlebar_leading_inset()))
                            .top(px(8.))
                            .child(
                                icon_button(
                                    "toggle-sidebar",
                                    if sidebar_visible || self.sidebar_overlay {
                                        IconName::PanelLeftClose
                                    } else {
                                        IconName::PanelLeftOpen
                                    },
                                    if sidebar_visible || self.sidebar_overlay {
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
                            .children((attention > 0).then(|| {
                                div()
                                    .px_1()
                                    .rounded_sm()
                                    .bg(t::yellow().opacity(0.12))
                                    .text_xs()
                                    .text_color(t::yellow())
                                    .child(attention.to_string())
                            }))
                    })
                    .children(self.remote.is_none().then(|| {
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
                    })),
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
                            .w(px(sidebar_width))
                            .h_full()
                            .flex_shrink_0()
                            .overflow_hidden()
                            .children(
                                (sidebar_visible || (sidebar_animating && !narrow_sidebar)).then(
                                    || {
                                        div()
                                            .w(px(sidebar_width))
                                            .h_full()
                                            .opacity(sidebar_progress)
                                            .child(self.sidebar.clone())
                                    },
                                ),
                            ),
                    )
                    .child(main)
                    .children((sidebar_visible && !sidebar_animating).then(|| {
                        pane_divider("resize-left", PanelSide::Left).left(px(sidebar_width - 4.))
                    }))
            })
            .children((narrow_sidebar && self.sidebar_overlay).then(|| {
                div()
                    .absolute()
                    .top(px(t::TITLEBAR))
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .bg(t::navigation_overlay())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, _, window, cx| s.dismiss_navigation(window, cx)),
                    )
                    .child(
                        div()
                            .h_full()
                            .w(rems(18.))
                            .max_w(relative(0.8))
                            .shadow(vec![t::navigation_shadow()])
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(self.sidebar.clone())
                            .focus_trap("sidebar-overlay", &self.sidebar_overlay_focus),
                    )
            }))
            .child(self.state.toasts.clone())
            .children(self.confirmation.clone())
            .children(self.upstream_modal.clone())
            .children(self.pull_request_modal.clone())
            .children(self.recovery_modal.clone())
            .children(self.modal.clone())
            .children(self.quick_open.clone())
            .children(self.new_chat.clone())
            .children(self.resume.clone())
            .children(self.worktree_modal.clone())
            .children(self.task_detail_modal.clone())
            .children(self.task_editor_modal.clone())
            .child(self.hover_cards.clone())
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
            || self.resume.is_some()
            || self.quick_open.is_some()
            || self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
            || (!matches!(&purpose, new_chat_palette::Purpose::RemoteFork(_))
                && crate::app_state::Chats::unavailable(cx).is_some())
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

    fn open_resume(
        &mut self,
        replace: Option<canopy_desktop::state::workspace::PaneId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.resume.is_some()
            || self.new_chat.is_some()
            || self.quick_open.is_some()
            || self.modal.is_some()
            || self.task_detail_modal.is_some()
            || self.task_editor_modal.is_some()
        {
            return;
        }
        let view = cx
            .new(|cx| resume_palette::ResumePalette::new(replace, self.focus.clone(), window, cx));
        self.resume_events = Some(cx.subscribe(&view, |this, _, _: &ModalDismissed, cx| {
            this.resume = None;
            this.resume_events = None;
            cx.notify();
        }));
        self.resume = Some(view);
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
