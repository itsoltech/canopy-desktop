//! The work area of a worktree without open tabs: which checkout it is, the chats
//! waiting in it and the next actions. Reads the project catalogue, the Git watcher
//! and the chat registry that already hold this state; no I/O.
use super::*;
use crate::app_state::{Chats, GitState};
use canopy_desktop::chat::provider::Provider;
use canopy_desktop::state::{
    projects::Project,
    workspace::{ToolPage, WorkspaceId},
};
use gpui_kit::component::{
    Icon, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A project checkout named as the sidebar names it.
pub(in crate::ui) struct Checkout {
    /// `owner/repository` from the origin, else the repository folder's name.
    pub project: String,
    /// The checked-out HEAD, or why Git can't name it.
    pub head: Result<String, &'static str>,
    /// The repository's own checkout (the sidebar's `default` tag).
    pub main: bool,
    pub locked: bool,
    pub path: PathBuf,
}

impl Checkout {
    pub fn of(project: &Project, git: &GitState) -> Self {
        let path = project.worktree_path.as_ref().unwrap_or(&project.path);
        let root = project.repository_path.as_ref().unwrap_or(&project.path);
        let info = git.repository(root).or_else(|| git.repository(path));
        let name_root = info.map_or(root, |r| &r.root);
        let label = info
            .and_then(|r| r.origin.as_ref())
            .map(|o| o.path().to_owned())
            .unwrap_or_else(|| {
                name_root
                    .file_name()
                    .unwrap_or(name_root.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            });
        let worktree = info.and_then(|r| r.worktrees.iter().find(|w| w.path == *path));
        let head = match (worktree, git.entries.get(path)) {
            (Some(worktree), _) => Ok(worktree.head.description()),
            (None, Some(Ok(None))) => Err("Folder without Git"),
            (None, Some(Err(_))) => Err("Git unavailable"),
            (None, Some(Ok(Some(_)))) => Err("Git HEAD unavailable"),
            (None, None) if info.is_some() => Err("Git HEAD unavailable"),
            (None, None) => Err("Reading Git…"),
        };
        Self {
            project: label,
            head,
            main: info.is_some_and(|r| r.root == *path),
            locked: worktree.is_some_and(|w| w.locked),
            path: path.clone(),
        }
    }
}

/// `~/…` for folders in the home directory, like the sidebar's cards. The home
/// directory is looked up once per process, not per frame.
pub(in crate::ui) fn home_relative(path: &Path) -> String {
    static HOME: OnceLock<Option<PathBuf>> = OnceLock::new();
    let home = HOME.get_or_init(|| canopy_desktop::platform::directories::user_home().ok());
    home.as_ref()
        .and_then(|home| {
            let rest = path.strip_prefix(home).ok()?;
            Some(if rest.as_os_str().is_empty() {
                "~".to_owned()
            } else {
                format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display())
            })
        })
        .unwrap_or_else(|| canopy_desktop::platform::paths::display(path))
}

/// A path whose end names the worktree: elide its middle, never its tail.
pub(in crate::ui) fn path_label(text: String) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_middle()
        .child(text)
}

/// The mark of the agent behind a chat: Claude's, Codex's, else its tool's.
pub(in crate::ui) fn agent_mark(provider: Option<Provider>, agent: &str) -> AnyElement {
    match provider {
        Some(Provider::Claude) => tool_icon("Claude Code", IconName::Bot),
        Some(Provider::Codex) => custom_icon("openai")
            .text_color(t::text())
            .into_any_element(),
        None => tool_icon(agent, IconName::Bot),
    }
}

/// An icon-led line; the icon stands in for the fact's label.
fn fact(icon: Icon) -> Div {
    row().w_full().min_w_0().gap_2().child(
        div()
            .flex_shrink_0()
            .text_color(t::faint())
            .child(icon.size_3p5()),
    )
}

/// A chat of this workspace that no tab shows (made on another device or left
/// running): picking it opens its tab here.
struct WaitingChat {
    chat: String,
    key: String,
    title: String,
    agent: String,
    provider: Option<Provider>,
    status: &'static str,
}

impl Workspace {
    fn waiting_chats(&self, workspace: WorkspaceId, cx: &App) -> Vec<WaitingChat> {
        let tools = self.state.tools.read(cx);
        let mut chats: Vec<_> = self
            .state
            .chats
            .read(cx)
            .detached()
            .filter(|chat| chat.workspace == workspace)
            .map(|chat| {
                let provider = Provider::from_tool(&chat.tool);
                WaitingChat {
                    chat: chat.chat.clone(),
                    key: format!("{:?}", chat.id),
                    title: chat.title.clone(),
                    agent: provider.map_or_else(
                        || {
                            tools
                                .catalog
                                .get(&chat.tool)
                                .map_or(chat.tool.clone(), |tool| tool.name.clone())
                        },
                        |p| p.name().to_owned(),
                    ),
                    provider,
                    status: super::sidebar::navigation_status(chat.id, cx),
                }
            })
            .collect();
        chats.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.chat.cmp(&b.chat)));
        chats
    }

    /// Shown in place of the tabs' content while the selected worktree (or the
    /// Private chats space) has no tab open.
    pub(super) fn worktree_overview(&self, cx: &mut Context<Self>) -> AnyElement {
        let reason = Chats::unavailable(cx);
        let project = self.state.projects.read(cx).catalog.current().cloned();
        let checkout = project
            .as_ref()
            .map(|p| Checkout::of(p, self.state.git.read(cx)));
        let workspace = self.state.workspace.read(cx).id;
        let waiting = self.waiting_chats(workspace, cx);

        let header = match &checkout {
            Some(checkout) => row()
                .gap_3()
                .child(
                    project_tile(&checkout.project)
                        .size_9()
                        .rounded_md()
                        .text_sm(),
                )
                .child(
                    column()
                        .flex_1()
                        .gap_0p5()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t::text())
                                .truncate()
                                .child(checkout.project.clone()),
                        )
                        .child(
                            row()
                                .gap_1p5()
                                .text_sm()
                                .child(
                                    match &checkout.head {
                                        Ok(_) => Icon::default().path("canopy/git-branch.svg"),
                                        Err(_) => Icon::new(IconName::Folder),
                                    }
                                    .size_3p5()
                                    .flex_shrink_0()
                                    .text_color(
                                        if checkout.head.is_ok() {
                                            t::accent()
                                        } else {
                                            t::faint()
                                        },
                                    ),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(if checkout.head.is_ok() {
                                            t::secondary()
                                        } else {
                                            t::muted()
                                        })
                                        .child(match &checkout.head {
                                            Ok(head) => head.clone(),
                                            Err(reason) => (*reason).to_owned(),
                                        }),
                                )
                                .when(checkout.main, |row| row.child(checkout_tag("default")))
                                .when(checkout.locked, |row| row.child(checkout_tag("locked"))),
                        ),
                ),
            None => column()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t::text())
                        .child("Private chats"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(t::secondary())
                        .child("Chats without a project, each in a private folder of its own."),
                ),
        };

        let facts = checkout.as_ref().map(|checkout| {
            let copied = checkout.path.display().to_string();
            column()
                .gap_1p5()
                .text_sm()
                .text_color(t::secondary())
                .child(
                    fact(Icon::new(IconName::Folder))
                        .child(path_label(home_relative(&checkout.path)))
                        .child(
                            icon_button("overview-copy-path", IconName::Copy, "Copy path")
                                .flex_shrink_0()
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                                }),
                        ),
                )
                .children(
                    project
                        .as_ref()
                        .and_then(|p| p.worktree_base.as_ref())
                        .filter(|_| !checkout.main)
                        .map(|base| {
                            fact(Icon::default().path("canopy/git-merge.svg")).child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .child(format!("Created from {}", base.reference)),
                            )
                        }),
                )
        });

        let actions = column()
            .gap_2()
            .child(
                row()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        primary_button("open-empty-chat", "New chat")
                            .disabled(reason.is_some())
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.state.chats.update(cx, |chats, cx| chats.open_current(cx))
                            })),
                    )
                    .when(checkout.is_some(), |row| {
                        row.child(
                            button("open-empty-terminal", "Open terminal")
                                .on_click(cx.listener(|s, _, _, cx| s.new_terminal(cx))),
                        )
                    })
                    .child(
                        button("resume-empty-chat", "Resume chat…").on_click(
                            cx.listener(|s, _, window, cx| s.open_resume(None, window, cx)),
                        ),
                    ),
            )
            .children(reason.map(|reason| div().text_xs().text_color(t::muted()).child(reason)));

        let chats = (!waiting.is_empty()).then(|| {
            column()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t::muted())
                        .child("Chats"),
                )
                .child(
                    column()
                        .gap_px()
                        .children(waiting.into_iter().map(|chat| self.waiting_row(chat, cx))),
                )
        });

        let tools = checkout.is_some().then(|| {
            row()
                .gap_1()
                .pt_4()
                .border_t_1()
                .border_color(t::border())
                .child(self.tool_entry(ToolPage::Files, "Files", &ShowFiles, cx))
                .child(self.tool_entry(ToolPage::Changes, "Changes", &ShowChanges, cx))
                .child(self.tool_entry(ToolPage::Tasks, "Tasks", &ShowTasks, cx))
        });

        // Auto margins centre the content and, unlike `justify_center`, let a short
        // window scroll to its top instead of clipping it.
        let content = column()
            .id("worktree-overview")
            .flex_shrink_0()
            .w_full()
            .max_w(rems(30.))
            .mx_auto()
            .my_auto()
            .px_6()
            .py_8()
            .gap_6()
            .child(column().gap_4().child(header).children(facts))
            .child(actions)
            .children(chats)
            .children(tools);
        #[cfg(test)]
        let content = {
            use gpui_kit::TestSupportExt;
            content.test_support()
        };
        let area = div()
            .id("empty-worktree-content")
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(content);
        #[cfg(test)]
        let area = {
            use gpui_kit::TestSupportExt;
            area.test_support()
        };
        area.into_any_element()
    }

    fn waiting_row(&self, chat: WaitingChat, cx: &mut Context<Self>) -> Button {
        let WaitingChat {
            chat,
            key,
            title,
            agent,
            provider,
            status,
        } = chat;
        Button::new(SharedString::from(format!("overview-chat-{key}")))
            .ghost()
            .small()
            .w_full()
            .h_8()
            .px_2()
            .accessibility_label(format!("{title}, {agent}, {status}"))
            .child(
                row()
                    .w_full()
                    .gap_2()
                    .child(agent_mark(provider, &agent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .text_sm()
                            .text_color(t::text())
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(super::sidebar::status_color(status))
                            .child(status),
                    ),
            )
            .on_click(cx.listener(move |s, _, _, cx| {
                s.state
                    .chats
                    .update(cx, |chats, cx| chats.open_detached(&chat, cx))
            }))
    }

    fn tool_entry(
        &self,
        page: ToolPage,
        label: &'static str,
        action: &dyn Action,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(SharedString::from(format!("overview-tool-{page:?}")))
            .ghost()
            .small()
            .label(label)
            .tooltip_with_action(format!("Show {label}"), action, Some("Workspace"))
            .on_click(cx.listener(move |s, _, _, cx| s.select_tool_page(page, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Workspace;
    use crate::app_state::AppState;
    use canopy_desktop::state::workspace::Workspace as Model;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, component::Root, px, size};

    #[gpui_kit::test]
    fn a_worktree_without_tabs_shows_its_overview_centred(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let app = cx.update(|cx| {
            crate::ui::theme::init(cx);
            let app = AppState::install_test(Model::new(), cx);
            app.projects.update(cx, |p, cx| {
                p.ready = true;
                p.install_test_worktree("/workspace/repo".into(), "/workspace/repo".into(), cx);
            });
            app.tools.update(cx, |tools, _| tools.ready = true);
            app
        });
        let window = cx.open_window(size(px(1100.), px(740.)), |window, cx| {
            Root::new(cx.new(|cx| Workspace::new(window, cx)), window, cx)
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(app.workspace.read(cx).tabs().is_empty());
            // Centred in the work area, not pinned to its top edge.
            let area = window.find("empty-worktree-content").bounds();
            let content = window.find("worktree-overview").bounds();
            let offset = (content.center().y - area.center().y).abs();
            assert!(offset <= px(1.), "area {area:?}, content {content:?}");
            assert!(window.try_find("overview-copy-path").is_some());
            assert!(window.try_find("overview-tool-Changes").is_some());
            window.click("open-empty-terminal", cx);
            assert_eq!(app.workspace.read(cx).tabs().len(), 1);
            window.render_frame(cx);
            assert!(window.try_find("empty-worktree-content").is_none());
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            app.projects.update(cx, |p, cx| p.select_none(cx));
            window.render_frame(cx);
            // Private chats: no project folder, terminal or project tools.
            assert!(window.try_find("open-empty-chat").is_some());
            assert!(window.try_find("resume-empty-chat").is_some());
            assert!(window.try_find("open-empty-terminal").is_none());
            assert!(window.try_find("overview-copy-path").is_none());
        })
        .unwrap();
    }
}
