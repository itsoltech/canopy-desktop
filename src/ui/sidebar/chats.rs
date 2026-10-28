//! Conversations are nested under their worktree; private conversations retain context labels.
pub(super) mod context;
use super::activity::{self, Activity};
use super::*;
use crate::app_state::{AppState, ProjectsState};
use canopy_desktop::chat::provider::Provider;
use canopy_desktop::state::workspace::{PaneId, PaneKind, WorkspaceId};
use context::ChatContext;
use gpui_kit::{
    base::{Disableable, Placement},
    component::{
        Selectable, Sizable,
        button::{ButtonCustomVariant, ButtonVariants},
        menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem},
    },
};
use std::rc::Rc;

pub struct RenameChat {
    pub workspace: WorkspaceId,
    pub pane: PaneId,
}
impl EventEmitter<RenameChat> for Sidebar {}

fn chat_text(
    key: &str,
    part: &'static str,
    text: impl Into<SharedString>,
) -> impl IntoElement + Styled {
    let text = text.into();
    let label = div()
        .id(SharedString::from(format!("chat-{part}-{key}")))
        .aria_label(text.clone())
        .min_w_0()
        .truncate()
        .child(text);
    #[cfg(test)]
    let label = {
        use gpui_kit::test::TestSupportExt;
        label.test_support()
    };
    label
}

/// Builds a row's actions menu; the same items open from its context menu.
pub(super) type MenuItems = dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu;
/// What clicking a row (or one of its buttons) does.
pub(super) type RowAction = dyn Fn(&mut Sidebar, &mut Context<Sidebar>);

/// One agent session in the tree, local or on another computer: both draw the
/// same row and differ only in where state and actions come from.
pub(super) struct SessionRow {
    /// Unique among sessions; names the row's elements and animations.
    pub key: String,
    pub title: String,
    pub mark: (Hsla, &'static str),
    pub activity: Activity,
    pub status: (&'static str, Hsla),
    /// The model, or the computer of a remote session.
    pub detail: Option<String>,
    /// A CLI agent in a terminal tab.
    pub cli: bool,
    /// The session runs on another computer.
    pub host: bool,
    pub selected: bool,
    pub enabled: bool,
    /// Position in its list, for the staggered reveal.
    pub index: usize,
    pub label: String,
    pub card: Rc<super::cards::SessionCard>,
    pub on_click: Rc<RowAction>,
    pub menu: Option<Rc<MenuItems>>,
}

impl Sidebar {
    pub(super) fn chat_feedback(&self, cx: &Context<Self>) -> Div {
        let app = cx.global::<AppState>();
        let chats = app.chats.read(cx);
        let mut rows = Vec::new();
        for error in chats
            .error
            .iter()
            .chain(app.chat_history.read(cx).error.iter())
            .map(String::as_str)
            .chain(app.notch_status.read(cx).error.iter().map(|s| s.as_ref()))
        {
            rows.push(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(t::red())
                    .child(error.to_owned())
                    .into_any_element(),
            );
        }
        for (id, error) in chats.cleanup_errors() {
            let chats = app.chats.clone();
            rows.push(
                column()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .child(div().text_xs().text_color(t::red()).child(error))
                    .child(
                        Button::new(SharedString::from(format!("retry-chat-{id:?}")))
                            .ghost()
                            .small()
                            .label("Retry cleanup")
                            .on_click(move |_, _, cx| {
                                chats.update(cx, |chats, cx| chats.retry_cleanup(id, cx));
                            }),
                    )
                    .into_any_element(),
            );
        }
        // Conversations kept without their agent and account wait for the user.
        let settings = app.settings.read(cx);
        let mut unresolved: Vec<_> = settings.unresolved_chats().collect();
        unresolved.sort_by(|a, b| a.id().cmp(b.id()));
        for record in unresolved {
            let chat = record.id().to_owned();
            let title = record
                .metadata()
                .title
                .clone()
                .unwrap_or_else(|| format!("Conversation {}", &chat[..chat.len().min(8)]));
            let chats = app.chats.clone();
            rows.push(
                column()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(t::secondary())
                            .child(format!("“{title}” needs an agent and account.")),
                    )
                    .child(
                        Button::new(SharedString::from(format!("restore-chat-{chat}")))
                            .ghost()
                            .small()
                            .label("Restore chat…")
                            .on_click(move |_, _, cx| {
                                let chat = chat.clone();
                                chats.update(cx, |_, cx| {
                                    cx.emit(crate::app_state::ChatRecoveryRequested(chat))
                                });
                            }),
                    )
                    .into_any_element(),
            );
        }
        column().px_3().children(rows)
    }
    pub(super) fn session_row(&self, spec: SessionRow, cx: &Context<Self>) -> AnyElement {
        let SessionRow {
            key,
            title,
            mark,
            activity,
            status,
            detail,
            cli,
            host,
            selected,
            enabled,
            index,
            label,
            card,
            on_click,
            menu,
        } = spec;
        let group = SharedString::from(format!("session-{key}"));
        let action = Button::new(SharedString::from(if cli {
            format!("agent-terminal-{key}")
        } else {
            format!("chat-{key}")
        }))
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(t::text())
                .hover(t::chat_hover())
                .active(t::chat_selected()),
        )
        .small()
        .compact()
        .selected(selected)
        .h(t::chat_row_height())
        .w_full()
        .min_w_0()
        .px_0()
        .border_0()
        .rounded(t::sidebar_row_radius())
        .disabled(!enabled)
        .accessibility_label(label)
        .child(
            // Padding lives inside the button's clipping label so
            // the activity edge can extend past the tile.
            row()
                .w_full()
                .min_w_0()
                .pl_2()
                .pr_1p5()
                .gap_2p5()
                .child(activity::agent_avatar(
                    &key,
                    mark,
                    activity,
                    &group,
                    selected,
                    self.reduced,
                ))
                .child(
                    column()
                        .flex_1()
                        .min_w_0()
                        .items_start()
                        .gap_1()
                        .text_left()
                        .child(
                            row()
                                .w_full()
                                .min_w_0()
                                .gap_1p5()
                                .when(menu.is_some(), |d| d.pr_6())
                                .child(
                                    chat_text(&key, "title", title.clone())
                                        .text_sm()
                                        .line_height(t::chat_title_line_height())
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t::text()),
                                )
                                .when(cli, |d| d.child(title_glyph("canopy/square-terminal.svg")))
                                .when(host, |d| d.child(title_glyph("canopy/monitor.svg"))),
                        )
                        .child(
                            row()
                                .w_full()
                                .min_w_0()
                                .gap_1()
                                .text_xs()
                                .line_height(t::chat_metadata_line_height())
                                .child(
                                    chat_text(&key, "agent", status.0)
                                        .flex_shrink_0()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(status.1),
                                )
                                .children(detail.map(|detail| {
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(t::muted())
                                        .child(format!("· {detail}"))
                                })),
                        ),
                ),
        )
        .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)));
        let menu = menu.filter(|_| enabled);
        let session = div()
            .id(SharedString::from(format!("session-row-{key}")))
            .relative()
            .group(group.clone())
            .h(t::chat_row_height())
            .flex_shrink_0()
            // Button tooltips are text-only; the card needs its own host.
            .child(
                div()
                    .id(SharedString::from(format!("session-card-{key}")))
                    .size_full()
                    .hover_card(format!("session-{key}"), Placement::Right, move |_, cx| {
                        card.render(cx).into_any_element()
                    })
                    .child(action),
            )
            .when(activity == Activity::Attention && !self.reduced, |row| {
                row.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(t::sidebar_row_radius())
                        .bg(t::attention_flash())
                        .with_animation(
                            SharedString::from(format!("attention-sweep-{key}")),
                            Animation::new(canopy_desktop::motion::duration::SWEEP),
                            |sweep, progress| {
                                sweep.opacity(
                                    1. - canopy_desktop::motion::Easing::SmoothOut.sample(progress),
                                )
                            },
                        ),
                )
            })
            .children(menu.clone().map(|items| {
                let button = Button::new(SharedString::from(format!("chat-menu-{key}")))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .foreground(t::secondary())
                            .hover(t::hover())
                            .active(t::selected()),
                    )
                    .small()
                    .compact()
                    .size_6()
                    .p_0()
                    .border_0()
                    .rounded(t::control_radius(cx))
                    .accessibility_label(format!("Actions for {title}"))
                    .tooltip("Chat actions")
                    .icon(IconName::Ellipsis)
                    .dropdown_menu(move |menu, window, cx| items(menu, window, cx));
                div()
                    .id(SharedString::from(format!("session-menu-{key}")))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_1p5()
                    .hides_hover_card()
                    .flex()
                    .items_center()
                    .when(!selected, |d| {
                        d.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.))
                    })
                    .child(button)
            }));
        let session = match menu {
            Some(items) => session
                .context_menu(move |menu, window, cx| items(menu, window, cx))
                .into_any_element(),
            None => session.into_any_element(),
        };
        reveal(&key, index, session, self.reduced)
    }
    pub(super) fn chats_content(&self, now: Instant, cx: &Context<Self>) -> Div {
        let id = self.projects.read(cx).scratch_id(cx);
        self.chat_section(id, false, now, cx)
    }
    pub(super) fn workspace_chats(&self, id: WorkspaceId, cx: &Context<Self>) -> Div {
        self.chat_section(id, true, Instant::now(), cx)
    }
    fn chat_section(
        &self,
        workspace_id: WorkspaceId,
        nested: bool,
        now: Instant,
        cx: &Context<Self>,
    ) -> Div {
        let app = cx.global::<AppState>().clone();
        let projects = self.projects.read(cx);
        let scratch = projects.scratch_workspace(cx);
        let enabled = projects.ready && !projects.busy && (nested || self.sections[1].open);
        let git = self.git.read(cx);
        let mut rows = vec![];
        for workspace in projects
            .runtime_workspaces(cx)
            .into_iter()
            .filter(|w| w.id == workspace_id)
        {
            let project = projects
                .catalog
                .items
                .iter()
                .find(|p| p.workspace == workspace.id);
            let context = ChatContext::new(project, git);

            let workspace_id = workspace.id;
            // Panes of the layout, then catalog chats no pane shows (created
            // remotely): those open a tab here when picked.
            let mut entries: Vec<(PaneId, String, String, bool, Option<String>)> = vec![];
            for tab in workspace.tabs() {
                for pane in tab.root.panes().into_iter().filter(is_session) {
                    let cli = pane.metadata.kind == PaneKind::Terminal;
                    let title = if cli {
                        if matches!(
                            &tab.root,
                            canopy_desktop::state::workspace::SplitTree::Leaf(_)
                        ) {
                            &tab.title
                        } else {
                            pane.metadata.title.as_deref().unwrap_or(&tab.title)
                        }
                    } else {
                        workspace.chat_title(pane.id).unwrap_or("New chat")
                    }
                    .to_owned();
                    entries.push((pane.id, title, pane.tool.clone(), cli, None));
                }
            }
            entries.extend(
                app.chats
                    .read(cx)
                    .detached()
                    .filter(|chat| chat.workspace == workspace_id)
                    .map(|chat| {
                        (
                            chat.id,
                            chat.title.clone(),
                            chat.tool.clone(),
                            false,
                            Some(chat.chat.clone()),
                        )
                    }),
            );
            {
                for (id, title, tool, cli, detached) in entries {
                    let live = app.workspace.read(cx);
                    let selected = self.remote_selection.is_none()
                        && live.id == workspace.id
                        && live.selected_pane().is_some_and(|p| p.id == id);
                    let provider = Provider::from_tool(&tool);
                    let agent = match provider {
                        Some(provider) => provider.name(),
                        // An ACP agent or other tool: its name in Preferences.
                        None => app
                            .tools
                            .read(cx)
                            .catalog
                            .get(&tool)
                            .map_or(tool.as_str(), |tool| tool.name.as_str()),
                    };
                    let activity = Activity::of(id, cx);
                    let model = model_label(id, cli, cx);
                    let key = format!("{id:?}");
                    let card = Rc::new(super::cards::SessionCard {
                        id: Some(id),
                        activity,
                        title: title.clone(),
                        mark: activity::provider_mark(provider),
                        agent: agent.to_owned(),
                        model: model.clone(),
                        cli,
                        folder: super::cards::Folder::new(
                            context.folder.clone().unwrap_or_default(),
                        ),
                        context: context.clone(),
                        host: None,
                    });
                    let on_click: Rc<RowAction> = {
                        let detached = detached.clone();
                        Rc::new(move |this, cx| {
                            this.local_selection(cx);
                            match &detached {
                                Some(chat) => {
                                    let chat = chat.clone();
                                    cx.global::<AppState>()
                                        .chats
                                        .clone()
                                        .update(cx, |chats, cx| chats.open_detached(&chat, cx));
                                }
                                None => this.projects.update(cx, |p, cx| {
                                    if cli {
                                        p.focus_agent_terminal(id, cx)
                                    } else {
                                        p.focus_agent(id, cx)
                                    }
                                }),
                            }
                        })
                    };
                    let owner = cx.entity().downgrade();
                    let menu: Rc<MenuItems> = Rc::new(
                        move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                            if let Some(chat) = detached.clone() {
                                // Named in its tab once open; deleting asks like a tab.
                                return menu.item(PopupMenuItem::new("Delete chat…").on_click(
                                    move |_, _, cx| {
                                        crate::app_state::Chats::request_delete_detached(
                                            chat.clone(),
                                            cx,
                                        )
                                    },
                                ));
                            }
                            let rename = owner.clone();
                            menu.item(PopupMenuItem::new("Rename chat…").on_click(
                                move |_, _, cx| {
                                    let _ = rename.update(cx, |_, cx| {
                                        cx.emit(RenameChat {
                                            workspace: workspace_id,
                                            pane: id,
                                        })
                                    });
                                },
                            ))
                            .separator()
                            .item(
                                PopupMenuItem::new("Delete chat…").on_click(move |_, _, cx| {
                                    ProjectsState::request_delete_chat(workspace_id, id, cx);
                                }),
                            )
                        },
                    );
                    let index = rows.len();
                    rows.push(self.session_row(
                        SessionRow {
                            key,
                            title: title.clone(),
                            mark: activity::provider_mark(provider),
                            activity,
                            status: (activity.label(), activity.color()),
                            detail: model,
                            cli,
                            host: false,
                            selected,
                            enabled,
                            index,
                            label: format!(
                                "{}, {title}, {}, {agent}, {}{}{}",
                                context.project,
                                context.checkout,
                                activity.label(),
                                if cli { ", CLI terminal" } else { "" },
                                if selected { ", selected" } else { "" }
                            ),
                            card,
                            on_click,
                            menu: (!cli).then_some(menu),
                        },
                        cx,
                    ));
                }
            }
        }
        if nested {
            return session_rail(rows);
        }
        let remote = self.remote_private_chats(rows.len(), cx);
        rows.extend(remote);
        if rows.is_empty() {
            rows.push(
                div()
                    .px_2()
                    .py_2()
                    .text_xs()
                    .text_color(t::muted())
                    .child(if projects.ready {
                        "No private chats"
                    } else {
                        "Restoring chats…"
                    })
                    .into_any_element(),
            );
        }
        let other: Vec<_> = scratch
            .tabs()
            .iter()
            .filter(|t| !t.root.panes().iter().any(is_session))
            .map(|t| (t.focused, t.title.clone()))
            .collect();
        if !other.is_empty() {
            let projects = self.projects.clone();
            rows.push(
                Button::new("standalone-open-tabs")
                    .ghost()
                    .small()
                    .compact()
                    .label("Open tabs…")
                    .disabled(!enabled)
                    .dropdown_menu(move |menu, _, _| {
                        other.iter().fold(menu, |menu, (pane, title)| {
                            let pane = *pane;
                            let projects = projects.clone();
                            menu.item(PopupMenuItem::new(title.clone()).on_click(
                                move |_, _, cx| {
                                    projects.update(cx, |p, cx| p.focus_agent(pane, cx));
                                },
                            ))
                        })
                    })
                    .into_any_element(),
            );
        }
        let _ = now;
        column().gap_px().children(rows)
    }
}

/// Shared by rows and collapsed parent summaries; an absent view is not an idle agent.
pub(in crate::ui) fn navigation_status(id: PaneId, cx: &App) -> &'static str {
    let app = cx.global::<AppState>();
    // The runtime says, with or without a view (a hidden, remote or sleeping chat).
    let status = app
        .chats
        .read(cx)
        .runtime(id)
        .map_or("Not started", |runtime| {
            runtime.read(cx).navigation_status()
        });
    if !matches!(status, "Error" | "Loading" | "Stopping")
        && let Some(session) = app.agents.read(cx).sessions.get(&id)
    {
        session.status_label()
    } else {
        status
    }
}

/// A worktree's sessions, hung from a rail under its row.
pub(super) fn session_rail(mut rows: Vec<AnyElement>) -> Div {
    if rows.is_empty() {
        rows.push(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(t::faint())
                .child("No sessions yet")
                .into_any_element(),
        );
    }
    column()
        .ml(rems(1.25))
        .pl_1()
        .border_l_1()
        .border_color(t::border())
        .gap_px()
        .children(rows)
}

/// Chats and CLI agents: the panes the sidebar lists as sessions.
pub(super) fn is_session(pane: &canopy_desktop::state::workspace::Pane) -> bool {
    pane.metadata.remote.is_none()
        && (pane.metadata.kind == PaneKind::Chat
            || (pane.metadata.kind == PaneKind::Terminal
                && Provider::from_tool(&pane.tool).is_some()))
}

/// The chat's chosen model, else the model a CLI agent reported.
fn model_label(id: PaneId, cli: bool, cx: &App) -> Option<String> {
    let app = cx.global::<AppState>();
    if !cli
        && let Some(runtime) = app.chats.read(cx).runtime(id)
        && let Some(label) = runtime.read(cx).model_label(cx)
    {
        return Some(label);
    }
    app.agents.read(cx).sessions.get(&id)?.model.clone()
}

/// A small mark after a session's title: a CLI terminal, another computer.
fn title_glyph(path: &'static str) -> Icon {
    Icon::default()
        .path(path)
        .size_3()
        .flex_shrink_0()
        .text_color(t::faint())
}

/// Rows rise into place, staggered, when their group opens or a session appears.
fn reveal(key: &str, index: usize, row: AnyElement, reduced: bool) -> AnyElement {
    use canopy_desktop::motion::{Easing, distance, duration};
    if reduced {
        return row;
    }
    let delay = duration::STAGGER * index.min(5) as u32;
    let total = duration::FAST + delay;
    let delay = delay.as_secs_f32() / total.as_secs_f32();
    div()
        .relative()
        .child(row)
        .with_animation(
            SharedString::from(format!("session-reveal-{key}")),
            Animation::new(total),
            move |row, progress| {
                let p = Easing::SmoothOut.sample(((progress - delay) / (1. - delay)).max(0.));
                row.top(px(distance::MICRO * (1. - p))).opacity(p)
            },
        )
        .into_any_element()
}

pub(in crate::ui) fn status_color(status: &str) -> Hsla {
    match status {
        "Needs attention" | "Waiting" => t::yellow(),
        "Error" => t::red(),
        "Goal" => t::goal(),
        _ => t::secondary(),
    }
}
