use super::search::{Entry, SettingsSearch};
use super::*;
use gpui_kit::{component::scroll::ScrollableElement, prelude::FluentBuilder};

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
/// Entries without working settings yet. They stay in the list, greyed out,
/// and open a page that says so (see docs/settings.md for the evidence).
/// Their settings (from Electron) stay searchable.
const UNAVAILABLE: [&str; 4] = ["Updates", "Privacy", "Skills", "Git"];
pub(super) fn unavailable(page: &str) -> bool {
    UNAVAILABLE.contains(&page)
}
fn not_available_tooltip(window: &mut Window, cx: &mut App) -> AnyView {
    gpui_kit::component::tooltip::Tooltip::new(NOT_AVAILABLE).build(window, cx)
}

/// A Settings page: its place in the list and what search finds on it.
pub(super) struct NavPage {
    pub name: &'static str,
    icon: IconName,
    pub description: &'static str,
    pub keywords: &'static str,
    /// The page's settings, declared next to its rendering.
    pub entries: fn() -> Vec<Entry>,
}
const fn page(
    name: &'static str,
    icon: IconName,
    description: &'static str,
    keywords: &'static str,
    entries: fn() -> Vec<Entry>,
) -> NavPage {
    NavPage {
        name,
        icon,
        description,
        keywords,
        entries,
    }
}

/// Every Settings page, in sidebar order. Search indexes exactly these.
pub(super) const GROUPS: &[(&str, &[NavPage])] = &[
    (
        "GENERAL",
        &[
            page(
                "General",
                IconName::Settings2,
                "Startup behavior, default tools, and resource usage",
                "reopen workspace startup launch new tab worktree shell cpu ram memory performance",
                || super::general::SEARCH.to_vec(),
            ),
            page(
                "Updates",
                IconName::RotateCw,
                "Auto-update channel and check frequency",
                "auto update upgrade channel stable next version",
                || UPDATES.to_vec(),
            ),
            page(
                "Privacy",
                IconName::EyeOff,
                "Telemetry and diagnostics",
                "telemetry diagnostics privacy analytics",
                || PRIVACY.to_vec(),
            ),
            page(
                "Shortcuts",
                IconName::SquareTerminal,
                "Keyboard shortcuts for every command",
                "shortcuts hotkeys keybindings keyboard keys reset",
                super::shortcuts::entries,
            ),
        ],
    ),
    (
        "FEATURES",
        &[page(
            "Notch",
            IconName::Bell,
            "Session status at the top of the screen",
            "notch overlay status indicator session",
            || super::notch::SEARCH.to_vec(),
        )],
    ),
    (
        "APPEARANCE",
        &[page(
            "Appearance",
            IconName::Palette,
            "Color theme, window and sidebar background, and terminal font",
            "theme colors dark light mode background wallpaper gradient font",
            super::appearance::entries,
        )],
    ),
    (
        "AI AGENTS",
        &[
            page(
                "Claude",
                IconName::Star,
                "Claude Code profiles",
                "claude anthropic",
                || super::agent::search(canopy_desktop::state::agent_settings::Agent::Claude),
            ),
            page(
                "Gemini",
                IconName::Star,
                "Gemini CLI profiles",
                "gemini google",
                || super::agent::search(canopy_desktop::state::agent_settings::Agent::Gemini),
            ),
            page(
                "OpenCode",
                IconName::SquareTerminal,
                "OpenCode profiles",
                "opencode",
                || super::agent::search(canopy_desktop::state::agent_settings::Agent::OpenCode),
            ),
            page(
                "Codex",
                IconName::Bot,
                "Codex profiles",
                "codex openai",
                || super::agent::search(canopy_desktop::state::agent_settings::Agent::Codex),
            ),
            page(
                "Skills",
                IconName::Settings,
                "Available skills",
                "skills agents",
                || SKILLS.to_vec(),
            ),
        ],
    ),
    (
        "DEV TOOLS",
        &[
            page(
                "Terminal",
                IconName::SquareTerminal,
                "The shell started in terminal tabs",
                "terminal shell zsh bash fish powershell login",
                || super::tools::SEARCH.to_vec(),
            ),
            page(
                "Tools",
                IconName::Settings,
                "Tools in the sidebar and their profiles",
                "tools custom command program",
                || super::tools::SEARCH.to_vec(),
            ),
            page(
                "Git",
                IconName::Network,
                "Pull strategy and worktree setup",
                "git pull",
                || GIT.to_vec(),
            ),
            page(
                "Integrations",
                IconName::Network,
                "Bring your team's issues into Canopy",
                "integrations issues tracker connections accounts",
                || super::integrations::SEARCH.to_vec(),
            ),
            page(
                "Task filters",
                IconName::Settings2,
                "Saved queries for Tasks",
                "task filters queries tasks issues",
                super::task_filters::entries,
            ),
        ],
    ),
    // Electron keeps remote access under Security; Globe is unique in this list.
    (
        "SECURITY",
        &[page(
            "Connections",
            IconName::Globe,
            "Use this computer from your other devices, and open other computers here",
            "remote access connections phone mobile devices computers",
            || super::remote::SEARCH.to_vec(),
        )],
    ),
];

// Settings of the greyed pages, as Electron names them.
const UPDATES: &[Entry] = &[
    Entry::new(
        "auto-install",
        "AUTOMATIC UPDATES",
        "Download and install automatically",
        "Updates are fetched in the background and applied on the next restart",
        "auto download install background",
    ),
    Entry::new(
        "channel",
        "AUTOMATIC UPDATES",
        "Update channel",
        "Stable receives tested releases. Pre-release includes the newest features but may have bugs.",
        "stable next pre-release beta",
    ),
    Entry::new(
        "frequency",
        "AUTOMATIC UPDATES",
        "Check frequency",
        "How often the app checks for new versions in the background",
        "hourly daily weekly never manual",
    ),
    Entry::new(
        "check",
        "MANUAL CHECK",
        "Check for updates",
        "Run a one-off check against the current channel",
        "check now manual",
    ),
];
const PRIVACY: &[Entry] = &[Entry::new(
    "telemetry",
    "TELEMETRY",
    "Send minimal telemetry",
    "One daily ping helps count active users. It contains only screen resolution, locale, app version, OS, and architecture.",
    "diagnostics ping analytics privacy",
)];
const SKILLS: &[Entry] = &[
    Entry::new(
        "installed",
        "INSTALLED SKILLS",
        "Installed skills",
        "Skills registered with the Canopy skill store",
        "",
    ),
    Entry::new("install", "INSTALL", "Install a skill", "", "add"),
];
const GIT: &[Entry] = &[
    Entry::new(
        "rebase",
        "PULL STRATEGY",
        "Rebase",
        "Replay local commits on top of upstream — keeps history linear",
        "git pull rebase linear history",
    ),
    Entry::new(
        "merge",
        "PULL STRATEGY",
        "Merge",
        "Create a merge commit when histories diverge",
        "git pull merge commit",
    ),
    Entry::new(
        "base-directory",
        "WORKTREES",
        "Base directory",
        "Where new worktrees are created",
        "worktree directory path pattern",
    ),
    Entry::new(
        "base-branch",
        "WORKTREES",
        "Default base branch",
        "Preselected as the base when creating a worktree",
        "worktree default base branch develop main",
    ),
    Entry::new(
        "setup",
        "WORKTREE SETUP",
        "Worktree setup",
        "Bootstrap commands and file copies that run after git worktree add",
        "worktree setup actions copy command bootstrap",
    ),
];

fn navigation(page: &str, search: &SettingsSearch, cx: &Context<PreferencesNavigation>) -> Div {
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
                .children(GROUPS.iter().enumerate().map(|(index, (label, items))| {
                    column()
                        .flex_shrink_0()
                        .mt(px(if index == 0 { 0. } else { 12. }))
                        .child(caption(*label).px(px(8.)).pb(px(4.)).line_height(px(16.)))
                        .child(column().gap(px(1.)).children(items.iter().map(|item| {
                            let name = item.name;
                            let active = name == page;
                            let inert = unavailable(name);
                            let (dimmed, count) = search.navigation(name, active);
                            row()
                                .id(SharedString::from(format!("prefs-nav-{name}")))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.page.update(cx, |page, cx| {
                                        *page = name.to_owned();
                                        cx.notify();
                                    });
                                }))
                                .when(inert, |row| row.tooltip(not_available_tooltip))
                                .cursor_pointer()
                                .relative()
                                .h(px(33.))
                                .flex_shrink_0()
                                .pl(px(12.))
                                .pr(px(8.))
                                .gap(px(8.))
                                .rounded(px(4.))
                                .when(dimmed, |row| row.opacity(t::SEARCH_DIMMED))
                                .bg(if active {
                                    t::selected()
                                } else {
                                    transparent_black()
                                })
                                .text_size(px(13.))
                                .text_color(match (inert, active) {
                                    (true, true) => t::muted(),
                                    (true, false) => t::faint(),
                                    (false, true) => t::text(),
                                    (false, false) => t::secondary(),
                                })
                                .child(nav_icon(name, item.icon.clone()).text_color(
                                    match (inert, active) {
                                        (true, _) => t::faint(),
                                        (false, true) => t::accent(),
                                        (false, false) => t::muted(),
                                    },
                                ))
                                .child(div().flex_1().min_w_0().truncate().child(name))
                                .children((count > 0).then(|| {
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(11.))
                                        .text_color(if inert { t::faint() } else { t::muted() })
                                        .child(count.to_string())
                                }))
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
                        })))
                })),
        )
        .child(
            // Settings export/import is not implemented yet.
            row()
                .id("prefs-backup")
                .tooltip(not_available_tooltip)
                .h(px(44.))
                .px(px(12.))
                .gap(px(12.))
                .border_t_1()
                .border_color(t::border())
                .child(caption("BACKUP").font_weight(FontWeight::NORMAL))
                .child(div().flex_1())
                .child(custom_icon("download").text_color(t::faint()))
                .child(custom_icon("upload").text_color(t::faint())),
        )
}
fn transparent_black() -> Hsla {
    rgba(0).into()
}

pub(super) struct PreferencesNavigation {
    page: Entity<String>,
    search: Entity<SettingsSearch>,
    _observers: [Subscription; 2],
}
impl Render for PreferencesNavigation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        navigation(self.page.read(cx), self.search.read(cx), cx)
    }
}

impl PreferencesNavigation {
    pub(super) fn new(
        page: Entity<String>,
        search: Entity<SettingsSearch>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            _observers: [
                cx.observe(&page, |_, _, cx| cx.notify()),
                cx.observe(&search, |_, _, cx| cx.notify()),
            ],
            page,
            search,
        }
    }
}
