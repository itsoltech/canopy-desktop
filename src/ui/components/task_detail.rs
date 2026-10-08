//! Stateless task-reader pieces. Owners supply actions, entities and motion progress.
use super::{super::theme as t, badge, caption, column, icon, modal, row, selection_button};
use canopy_desktop::{
    integrations::{Provider, TaskComment, TaskRef},
    motion,
};
use gpui_kit::{
    component::{IconName, button::Button},
    *,
};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Description,
    Comments,
}
impl DetailTab {
    pub const ALL: [Self; 2] = [Self::Description, Self::Comments];
    pub fn position(self) -> f32 {
        match self {
            Self::Description => 0.,
            Self::Comments => 1.,
        }
    }
}
pub fn tab_button(tab: DetailTab, selected: DetailTab, comments: Option<usize>) -> Button {
    let (id, label) = match tab {
        DetailTab::Description => ("task-detail-tab-0", "Description".to_owned()),
        DetailTab::Comments => (
            "task-detail-tab-1",
            comments
                .map(|n| format!("Comments · {n}"))
                .unwrap_or_else(|| "Comments".into()),
        ),
    };
    selection_button(id, label, selected == tab).border_0()
}
pub fn tab_bar(tabs: impl IntoIterator<Item = Button>) -> Div {
    row()
        .gap(px(4.))
        .flex_shrink_0()
        .border_b_1()
        .border_color(t::border())
        .pb(px(8.))
        .children(tabs)
}
pub struct DetailLayout {
    viewport: Size<Pixels>,
    pub width: f32,
    pub height: f32,
}
impl DetailLayout {
    pub fn new(viewport: Size<Pixels>) -> Self {
        Self {
            width: (f32::from(viewport.width) - 64.).clamp(0., 1040.),
            height: (f32::from(viewport.height) - 80.).clamp(0., 840.),
            viewport,
        }
    }
    pub fn wide(&self) -> bool {
        self.width >= 760.
    }
    pub fn surface(&self, progress: f32) -> Stateful<Div> {
        modal::modal_surface("expanded-task-dialog")
            .absolute()
            .w(px(self.width))
            .h(px(self.height))
            .p(px(24.))
            .gap(px(16.))
            .left((self.viewport.width - px(self.width)) / 2.)
            .top(
                (self.viewport.height - px(self.height)) / 2.
                    + px(motion::distance::BASE * (1. - progress)),
            )
            .opacity(progress)
    }
}
pub fn header(reference: &TaskRef, linked: bool, actions: impl IntoElement) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(8.))
        .child(super::integrations::provider_icon(
            reference.project.provider,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.))
                .text_color(t::secondary())
                .child(format!(
                    "{}  /  {}",
                    reference
                        .project
                        .site
                        .as_deref()
                        .unwrap_or(&reference.project.key),
                    reference.label()
                )),
        )
        .children(linked.then(|| badge("Linked").text_color(t::accent())))
        .child(actions)
}
pub fn title(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(20.))
        .line_height(px(26.))
        .font_weight(FontWeight::MEDIUM)
        .line_clamp(2)
        .child(text.into())
}
fn metadata_field(label: &'static str, content: impl IntoElement) -> Div {
    column().gap(px(6.)).child(caption(label)).child(content)
}
pub fn metadata(task: &TaskRef, path: &Path, editor: impl IntoElement) -> Div {
    column()
        .gap(px(20.))
        .child(metadata_field("DETAILS", editor).gap(px(8.)))
        .child(metadata_field(
            if task.project.site.is_some() {
                "PROJECT"
            } else {
                "REPOSITORY"
            },
            div()
                .text_size(px(12.))
                .line_clamp(2)
                .child(task.project.key.clone()),
        ))
        .children(task.project.site.as_ref().map(|site| {
            metadata_field(
                match task.project.provider {
                    Provider::Jira => "JIRA SITE",
                    Provider::Youtrack => "YOUTRACK SERVICE",
                    Provider::Github => "SITE",
                },
                div().text_size(px(11.)).line_clamp(2).child(site.clone()),
            )
        }))
        .child(metadata_field(
            "WORKTREE",
            div().text_size(px(12.)).line_clamp(2).child(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
        ))
}
pub fn description(metadata: Option<AnyElement>, content: impl IntoElement) -> Div {
    column()
        .w_full()
        .gap(px(20.))
        .pb(px(16.))
        .children(metadata)
        .child(content)
}
/// No timer or state mutation: this only paints the owner's current transition.
pub fn pages(progress: f32, description: impl IntoElement, comments: impl IntoElement) -> Div {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .children((progress < 1.).then(|| {
            div()
                .absolute()
                .inset_0()
                .left(px(-motion::distance::BASE * progress))
                .opacity(1. - progress)
                .child(description)
        }))
        .children((progress > 0.).then(|| {
            div()
                .absolute()
                .inset_0()
                .left(px(motion::distance::BASE * (1. - progress)))
                .opacity(progress)
                .child(comments)
        }))
}
pub fn footer(link: impl IntoElement, create_worktree: impl IntoElement) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(8.))
        .pt(px(16.))
        .border_t_1()
        .border_color(t::border())
        .child(
            div()
                .flex_1()
                .text_size(px(11.))
                .text_color(t::muted())
                .child("Esc to return to the task list"),
        )
        .child(link)
        .child(create_worktree)
}
pub fn comment_card(comment: &TaskComment, body: impl IntoElement, open: impl IntoElement) -> Div {
    let date = comment
        .created_at
        .get(..16)
        .map(|s| format!("{} UTC", s.replace('T', " ")))
        .unwrap_or_else(|| "Date unavailable".into());
    column()
        .w_full()
        .py(px(16.))
        .gap(px(12.))
        .border_b_1()
        .border_color(t::border())
        .child(
            row()
                .gap(px(8.))
                .child(icon(IconName::User).flex_shrink_0())
                .child(
                    column()
                        .flex_1()
                        .gap(px(2.))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(comment.author.clone()),
                        )
                        .child(div().text_size(px(10.)).text_color(t::muted()).child(date)),
                )
                .child(open),
        )
        .child(body)
}
pub fn comments_toolbar(loaded: usize, total: Option<usize>, refresh: impl IntoElement) -> Div {
    row()
        .flex_shrink_0()
        .gap(px(8.))
        .h(px(28.))
        .child(
            div()
                .flex_1()
                .text_size(px(11.))
                .text_color(t::muted())
                .child(
                    total
                        .map(|total| format!("{loaded} of {total} comments"))
                        .unwrap_or_else(|| "Discussion".into()),
                ),
        )
        .child(refresh)
}
pub fn comments_loading() -> Div {
    column()
        .gap(px(16.))
        .child(super::skeleton_bar().h(px(12.)).w(px(110.)))
        .child(super::skeleton_bar().h(px(56.)).w_full())
}
pub fn comments_empty() -> Div {
    column()
        .items_center()
        .gap(px(8.))
        .child(icon(IconName::Inbox))
        .child(div().text_color(t::secondary()).child("No comments yet"))
}
