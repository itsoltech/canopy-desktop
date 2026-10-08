//! Shared presentation for provider context and integration feedback.
use super::{super::theme as t, column, custom_icon, icon, row};
use canopy_desktop::integrations::Provider;
use gpui_kit::{component::IconName, *};

pub fn provider_icon(provider: Provider) -> gpui_kit::component::Icon {
    let mark = match provider {
        Provider::Github => icon(IconName::Github).text_color(t::text()),
        Provider::Jira => custom_icon("jira").text_color(t::jira_brand()),
        Provider::Youtrack => custom_icon("issue").text_color(t::accent()),
    };
    mark.size(px(16.)).flex_shrink_0()
}
pub fn github_heading(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Div {
    provider_heading(Provider::Github, title, detail)
}
pub fn provider_heading(
    provider: Provider,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
) -> Div {
    column()
        .gap(px(2.))
        .child(
            row()
                .h(px(20.))
                .flex_shrink_0()
                .gap(px(8.))
                .child(provider_icon(provider))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.into()),
                ),
        )
        .child(
            div()
                .pl(px(24.))
                .truncate()
                .text_size(px(11.))
                .line_height(px(16.))
                .text_color(t::muted())
                .child(detail.into()),
        )
}

pub fn integration_message(message: impl Into<SharedString>, error: bool) -> Div {
    row()
        .items_start()
        .gap(px(8.))
        .p(px(10.))
        .rounded(px(4.))
        .bg(t::hover())
        .child(
            icon(if error {
                IconName::TriangleAlert
            } else {
                IconName::Info
            })
            .flex_shrink_0()
            .text_color(if error { t::red() } else { t::muted() }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(if error { t::red() } else { t::secondary() })
                .child(message.into()),
        )
}
