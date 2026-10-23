//! Direct navigation to standalone conversations; project/worktree rows stay separate.
use super::*;
use crate::app_state::{AppState, ProjectsState};
use canopy_desktop::state::workspace::{PaneId, PaneKind, WorkspaceId};
use gpui_kit::{
    base::Disableable,
    component::{
        Selectable, Sizable,
        button::{ButtonCustomVariant, ButtonVariants},
        menu::{DropdownMenu, PopupMenuItem},
    },
};

pub struct RenameChat {
    pub workspace: WorkspaceId,
    pub pane: PaneId,
}
impl EventEmitter<RenameChat> for Sidebar {}

impl Sidebar {
    pub(super) fn chats_content(&self, now: Instant, cx: &mut Context<Self>) -> Div {
        let app = cx.global::<AppState>().clone();
        let projects = self.projects.read(cx);
        let scratch = projects.scratch_workspace(cx);
        let workspace = scratch.id;
        let enabled = projects.ready && !projects.busy && self.sections[3].open;
        let active = projects.catalog.active.is_none();
        let focused = active
            .then(|| scratch.active().map(|tab| tab.focused))
            .flatten();
        let chats = app.chats.read(cx);
        let agents = app.agents.read(cx);
        let mut rows = vec![];
        for tab in scratch.tabs() {
            for pane in tab
                .root
                .panes()
                .into_iter()
                .filter(|p| p.metadata.kind == PaneKind::Chat)
            {
                let id = pane.id;
                let title = scratch.chat_title(id).unwrap_or("New chat").to_owned();
                let selected = focused == Some(id);
                let mut status = chats
                    .views
                    .get(&id)
                    .map_or("Not started", |v| v.read(cx).navigation_status());
                if !matches!(status, "Error" | "Loading" | "Stopping")
                    && let Some(session) = agents.sessions.get(&id)
                {
                    status = session.status_label();
                }
                let action = Button::new(SharedString::from(format!("chat-{id:?}")))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .foreground(t::text())
                            .hover(t::hover())
                            .active(t::selected()),
                    )
                    .small()
                    .compact()
                    .selected(selected)
                    .h(t::chat_row_height())
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .border_0()
                    .rounded(t::control_radius(cx))
                    .disabled(!enabled)
                    .accessibility_label(format!(
                        "{title}, {status}{}",
                        if selected { ", selected" } else { "" }
                    ))
                    .tooltip(format!("{title} — {status}"))
                    .child(
                        column()
                            .w_full()
                            .items_start()
                            .text_left()
                            .child(div().w_full().truncate().text_sm().child(title.clone()))
                            .child(div().text_xs().text_color(t::muted()).child(status)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.projects.update(cx, |p, cx| p.focus_agent(id, cx));
                    }));
                let owner = cx.entity().downgrade();
                let menu = Button::new(SharedString::from(format!("chat-menu-{id:?}")))
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
                    .disabled(!enabled)
                    .dropdown_menu(move |menu, _, _| {
                        let rename = owner.clone();
                        menu.item(
                            PopupMenuItem::new("Rename chat…").on_click(move |_, _, cx| {
                                let _ = rename.update(cx, |_, cx| {
                                    cx.emit(RenameChat {
                                        workspace,
                                        pane: id,
                                    })
                                });
                            }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Delete chat…").on_click(move |_, _, cx| {
                                ProjectsState::request_delete_chat(workspace, id, cx);
                            }),
                        )
                    });
                rows.push(
                    row()
                        .h(t::chat_row_height())
                        .flex_shrink_0()
                        .gap_1()
                        .child(action)
                        .child(menu)
                        .into_any_element(),
                );
            }
        }
        if rows.is_empty() {
            rows.push(
                div()
                    .px_2()
                    .py_2()
                    .text_xs()
                    .text_color(t::muted())
                    .child(if projects.ready {
                        "No chats yet"
                    } else {
                        "Restoring chats…"
                    })
                    .into_any_element(),
            );
        }
        let other: Vec<_> = scratch
            .tabs()
            .iter()
            .filter(|t| {
                !t.root
                    .panes()
                    .iter()
                    .any(|p| p.metadata.kind == PaneKind::Chat)
            })
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
        for error in chats
            .error
            .iter()
            .chain(app.chat_history.read(cx).error.iter())
        {
            rows.push(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(t::red())
                    .child(error.clone())
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
        column()
            .flex_shrink_0()
            .px_3()
            .pt_2()
            .pb_2()
            .child(self.header(3, "CHATS", now, cx))
            .child(self.sections[3].measured_body(
                "sidebar-chats-body",
                column().children(rows),
                now,
            ))
    }
}
