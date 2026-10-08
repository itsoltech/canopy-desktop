use super::{super::theme as t, badge, column, icon, row, skeleton_bar};
use canopy_desktop::integrations::{TaskItem, TaskState};
use gpui_kit::{component::IconName, *};

pub const TASK_ROW_HEIGHT: f32 = 56.;

pub fn task_state_mark(state: TaskState) -> Div {
    row()
        .size(px(14.))
        .flex_shrink_0()
        .justify_center()
        .child(match state {
            TaskState::Open => div()
                .size(px(10.))
                .rounded_full()
                .border_1()
                .border_color(t::green())
                .into_any_element(),
            TaskState::Closed => icon(IconName::CircleCheck)
                .text_color(t::muted())
                .into_any_element(),
        })
}

pub fn task_row(task: &TaskItem, selected: bool) -> Stateful<Div> {
    row()
        .id(SharedString::from(format!(
            "task-{}-{}",
            task.reference.project.key, task.reference.id
        )))
        .h(px(TASK_ROW_HEIGHT))
        .w_full()
        .px(px(8.))
        .gap(px(8.))
        .rounded(px(4.))
        .cursor_pointer()
        .bg(if selected {
            t::selected()
        } else {
            rgba(0).into()
        })
        .hover(|s| s.bg(if selected { t::selected() } else { t::hover() }))
        .child(
            column()
                .h_full()
                .pt(px(11.))
                .child(task_state_mark(task.state)),
        )
        .child(
            column()
                .flex_1()
                .gap(px(4.))
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(t::text())
                        .child(task.reference.title.clone()),
                )
                .child(
                    row()
                        .gap(px(6.))
                        .text_size(px(10.))
                        .text_color(t::muted())
                        .child(
                            div()
                                .font_family(t::MONO)
                                .flex_shrink_0()
                                .child(task.reference.label()),
                        )
                        .children(
                            task.jira
                                .as_ref()
                                .map(|j| badge(j.status.clone()).max_w(px(90.)).truncate()),
                        )
                        .children(
                            task.labels
                                .first()
                                .map(|label| badge(label.clone()).max_w(px(110.)).truncate()),
                        )
                        .children((task.labels.len() > 1).then(|| {
                            div()
                                .flex_shrink_0()
                                .child(format!("+{}", task.labels.len() - 1))
                        }))
                        .children(task.assignees.first().map(|assignee| {
                            div().flex_1().truncate().child(if task.jira.is_some() {
                                assignee.clone()
                            } else {
                                format!("@{assignee}")
                            })
                        })),
                ),
        )
}

pub fn task_details(task: &TaskItem, description: impl IntoElement) -> Div {
    column()
        .gap(px(12.))
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .child(task.reference.title.clone()),
        )
        .child(task_metadata(task))
        .child(description)
}

pub fn task_metadata(task: &TaskItem) -> Div {
    column()
        .gap(px(12.))
        .child(
            row()
                .gap(px(6.))
                .flex_wrap()
                .child(
                    badge(if let Some(jira) = &task.jira {
                        jira.status.as_str()
                    } else if task.state == TaskState::Open {
                        "Open"
                    } else {
                        "Closed"
                    })
                    .text_color(if task.state == TaskState::Open {
                        t::green()
                    } else {
                        t::secondary()
                    }),
                )
                .children(
                    task.labels
                        .iter()
                        .map(|label| badge(label.clone()).max_w(px(180.)).truncate()),
                ),
        )
        .children((!task.assignees.is_empty()).then(|| {
            row()
                .items_start()
                .gap(px(8.))
                .child(icon(IconName::User).flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.))
                        .text_color(t::secondary())
                        .child(
                            task.assignees
                                .iter()
                                .map(|name| {
                                    if task.jira.is_some() {
                                        name.clone()
                                    } else {
                                        format!("@{name}")
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                )
        }))
}

pub fn task_empty_state(title: &str, description: &str) -> Div {
    column()
        .items_center()
        .gap(px(8.))
        .px(px(16.))
        .py(px(28.))
        .text_center()
        .child(icon(IconName::Inbox).size(px(16.)).text_color(t::muted()))
        .child(
            div()
                .text_size(px(13.))
                .text_color(t::secondary())
                .child(title.to_owned()),
        )
        .child(
            div()
                .max_w(px(260.))
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(t::muted())
                .child(description.to_owned()),
        )
}

pub fn task_skeleton() -> Div {
    column().children((0..6).map(|_| {
        row()
            .h(px(TASK_ROW_HEIGHT))
            .gap(px(8.))
            .px(px(8.))
            .child(skeleton_bar().size(px(12.)).flex_shrink_0())
            .child(
                column()
                    .flex_1()
                    .gap(px(8.))
                    .child(skeleton_bar().w_full().h(px(10.)))
                    .child(skeleton_bar().w(px(88.)).h(px(8.))),
            )
    }))
}
