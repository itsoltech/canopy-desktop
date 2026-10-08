use super::*;
use gpui_kit::component::scroll::ScrollableElement;

fn nav_icon(name: &str, fallback: IconName) -> gpui_kit::component::Icon {
    match name {
        "Privacy" => custom_icon("shield"),
        "Shortcuts" => custom_icon("keyboard"),
        "Claude" => custom_icon("sparkles"),
        "Gemini" => custom_icon("diamond"),
        "OpenCode" => custom_icon("code"),
        "Codex" => custom_icon("braces"),
        "Skills" => custom_icon("wrench"),
        _ => icon(fallback),
    }
}
fn navigation(page: &str, cx: &Context<PreferencesNavigation>) -> Div {
    let groups = [
        (
            "GENERAL",
            vec![
                ("General", IconName::Settings2),
                ("Updates", IconName::RotateCw),
                ("Privacy", IconName::EyeOff),
                ("Shortcuts", IconName::SquareTerminal),
            ],
        ),
        (
            "FEATURES",
            vec![("Notch", IconName::Bell), ("Misc", IconName::Ellipsis)],
        ),
        (
            "APPEARANCE",
            vec![
                ("Appearance", IconName::Palette),
                ("Sidebar", IconName::PanelLeft),
            ],
        ),
        (
            "AI AGENTS",
            vec![
                ("Claude", IconName::Star),
                ("Gemini", IconName::Star),
                ("OpenCode", IconName::SquareTerminal),
                ("Codex", IconName::Bot),
                ("Skills", IconName::Settings),
            ],
        ),
        (
            "DEV TOOLS",
            vec![
                ("Terminal", IconName::SquareTerminal),
                ("Tools", IconName::Settings),
                ("Git", IconName::Network),
                ("Integrations", IconName::Network),
                ("Task filters", IconName::Settings2),
                ("File Watcher", IconName::Folder),
            ],
        ),
    ];
    column()
        .w(px(SIDEBAR))
        .flex_shrink_0()
        .h_full()
        .border_r_1()
        .border_color(t::border())
        .child(
            column()
                .id("prefs-navigation")
                .flex_1()
                .overflow_y_scrollbar()
                .py(px(12.))
                .px(px(8.))
                .children(
                    groups
                        .into_iter()
                        .enumerate()
                        .map(|(index, (label, items))| {
                            column()
                                .flex_shrink_0()
                                .mt(px(if index == 0 { 0. } else { 12. }))
                                .child(caption(label).px(px(8.)).pb(px(4.)).line_height(px(16.)))
                                .child(column().gap(px(1.)).children(items.into_iter().map(
                                    |(name, glyph)| {
                                        let active = name == page;
                                        row()
                                            .id(SharedString::from(format!("prefs-nav-{name}")))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if matches!(
                                                    name,
                                                    "General"
                                                        | "Tools"
                                                        | "Claude"
                                                        | "Codex"
                                                        | "Gemini"
                                                        | "OpenCode"
                                                        | "Terminal"
                                                        | "Integrations"
                                                        | "Task filters"
                                                ) {
                                                    this.page.update(cx, |page, cx| {
                                                        *page = name.to_owned();
                                                        cx.notify();
                                                    });
                                                }
                                            }))
                                            .cursor_pointer()
                                            .relative()
                                            .h(px(33.))
                                            .flex_shrink_0()
                                            .pl(px(12.))
                                            .pr(px(8.))
                                            .gap(px(8.))
                                            .rounded(px(4.))
                                            .bg(if active {
                                                t::selected()
                                            } else {
                                                transparent_black()
                                            })
                                            .text_size(px(13.))
                                            .text_color(if active {
                                                t::text()
                                            } else {
                                                t::secondary()
                                            })
                                            .child(nav_icon(name, glyph).text_color(if active {
                                                t::accent()
                                            } else {
                                                t::muted()
                                            }))
                                            .child(name)
                                            .children(active.then(|| {
                                                div()
                                                    .absolute()
                                                    .left_0()
                                                    .top(px(8.5))
                                                    .w(px(2.))
                                                    .h(px(16.))
                                                    .rounded(px(3.))
                                                    .bg(t::accent())
                                            }))
                                    },
                                )))
                        }),
                ),
        )
        .child(
            row()
                .h(px(44.))
                .px(px(12.))
                .gap(px(12.))
                .border_t_1()
                .border_color(t::border())
                .child(caption("BACKUP").font_weight(FontWeight::NORMAL))
                .child(div().flex_1())
                .child(custom_icon("download"))
                .child(custom_icon("upload")),
        )
}
fn transparent_black() -> Hsla {
    rgba(0).into()
}

pub(super) struct PreferencesNavigation {
    page: Entity<String>,
    _observer: Subscription,
}
impl Render for PreferencesNavigation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        navigation(self.page.read(cx), cx)
    }
}

impl PreferencesNavigation {
    pub(super) fn new(page: Entity<String>, cx: &mut Context<Self>) -> Self {
        let observer = cx.observe(&page, |_, _, cx| cx.notify());
        Self {
            page,
            _observer: observer,
        }
    }
}
