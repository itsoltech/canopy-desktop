//! Flat conversation navigation with the project, checkout and agent visible in each row.
mod context;
use super::*;
use crate::app_state::{AppState, ProjectsState};
use canopy_desktop::chat::provider::Provider;
use canopy_desktop::state::workspace::{PaneId, PaneKind, WorkspaceId};
use context::ChatContext;
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

fn chat_text(
    pane: PaneId,
    part: &'static str,
    text: impl Into<SharedString>,
) -> impl IntoElement + Styled {
    let text = text.into();
    let label = div()
        .id(SharedString::from(format!("chat-{part}-{pane:?}")))
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

impl Sidebar {
    pub(super) fn chats_content(&self, now: Instant, cx: &mut Context<Self>) -> Div {
        let app = cx.global::<AppState>().clone();
        let projects = self.projects.read(cx);
        let scratch = projects.scratch_workspace(cx);
        let enabled = projects.ready && !projects.busy && self.sections[3].open;
        let focused = app.workspace.read(cx).active().map(|tab| tab.focused);
        let chats = app.chats.read(cx);
        let agents = app.agents.read(cx);
        let git = self.git.read(cx);
        let mut rows = vec![];
        for workspace in projects.runtime_workspaces(cx) {
            let context = ChatContext::new(
                projects
                    .catalog
                    .items
                    .iter()
                    .find(|p| p.workspace == workspace.id),
                git,
            );
            let workspace_id = workspace.id;
            for tab in workspace.tabs() {
                for pane in tab
                    .root
                    .panes()
                    .into_iter()
                    .filter(|p| p.metadata.kind == PaneKind::Chat)
                {
                    let id = pane.id;
                    let title = workspace.chat_title(id).unwrap_or("New chat").to_owned();
                    let selected = focused == Some(id);
                    let provider = Provider::from_tool(&pane.tool);
                    let agent = match provider {
                        Some(provider) => provider.name(),
                        None => &pane.tool,
                    };
                    let mut status = chats
                        .views
                        .get(&id)
                        .map_or("Not started", |v| v.read(cx).navigation_status());
                    if !matches!(status, "Error" | "Loading" | "Stopping")
                        && let Some(session) = agents.sessions.get(&id)
                    {
                        status = session.status_label();
                    }
                    let status_color = match status {
                        "Working" => t::accent(),
                        "Goal" => t::goal(),
                        "Needs attention" | "Waiting" => t::yellow(),
                        "Error" => t::red(),
                        _ => t::secondary(),
                    };
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
                        .w_full()
                        .min_w_0()
                        .px_2()
                        .border_0()
                        .rounded(t::control_radius(cx))
                        .disabled(!enabled)
                        .accessibility_label(format!(
                            "{}, {title}, {}, {agent}, {status}{}",
                            context.project,
                            context.checkout,
                            if selected { ", selected" } else { "" }
                        ))
                        .tooltip(format!(
                            "{title}\n{}\nAgent: {agent}\nStatus: {status}",
                            context.detail
                        ))
                        .child(
                            column()
                                .w_full()
                                .items_start()
                                .gap_0p5()
                                .text_left()
                                .child(
                                    row()
                                        .w_full()
                                        .pr_6()
                                        .gap_2()
                                        .text_xs()
                                        .text_color(t::secondary())
                                        .child(
                                            chat_text(id, "project", context.project.clone())
                                                .flex_1(),
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_color(status_color)
                                                .child(status),
                                        ),
                                )
                                .child(
                                    chat_text(id, "title", title.clone())
                                        .w_full()
                                        .text_sm()
                                        .text_color(t::text()),
                                )
                                .child(
                                    row()
                                        .w_full()
                                        .gap_2()
                                        .text_xs()
                                        .text_color(t::secondary())
                                        .child(
                                            row()
                                                .flex_1()
                                                .gap_1()
                                                .child(if context.git {
                                                    custom_icon("git-branch")
                                                        .size_3()
                                                        .text_color(t::secondary())
                                                        .into_any_element()
                                                } else {
                                                    Icon::new(IconName::Folder)
                                                        .size_3()
                                                        .into_any_element()
                                                })
                                                .child(
                                                    chat_text(
                                                        id,
                                                        "checkout",
                                                        context.checkout.clone(),
                                                    )
                                                    .flex_1(),
                                                ),
                                        )
                                        .child(
                                            row()
                                                .flex_shrink_0()
                                                .gap_1()
                                                .child(match provider {
                                                    Some(Provider::Claude) => custom_icon("claude")
                                                        .size_3()
                                                        .text_color(t::secondary())
                                                        .into_any_element(),
                                                    Some(Provider::Codex) => custom_icon("openai")
                                                        .size_3()
                                                        .text_color(t::secondary())
                                                        .into_any_element(),
                                                    None => Icon::new(IconName::Bot)
                                                        .size_3()
                                                        .into_any_element(),
                                                })
                                                .child(chat_text(id, "agent", agent.to_owned())),
                                        ),
                                ),
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
                        });
                    rows.push(
                        div()
                            .relative()
                            .h(t::chat_row_height())
                            .flex_shrink_0()
                            .child(action)
                            .child(div().absolute().top_1().right_1().child(menu))
                            .into_any_element(),
                    );
                }
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
                column().gap_1().children(rows),
                now,
            ))
    }
}
