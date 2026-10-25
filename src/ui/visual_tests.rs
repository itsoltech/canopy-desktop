//! Native review fixture: production views, synthetic data, no accounts or agent processes.
use super::Workspace;
use crate::app_state::AppState;
use canopy_desktop::{
    agents::Status,
    git::{RepositoryInfo, WorktreeHead, WorktreeInfo},
    state::workspace::{PaneKind, PaneMetadata, Workspace as Model},
};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, WindowBounds, WindowOptions, point, px, size};
use std::sync::Arc;

/// Run explicitly under a graphical display. CANOPY_UI_SCENE=empty|chat|cli,
/// CANOPY_UI_WIDTH/HEIGHT/FONT select the geometry. The fixture quits after 30s.
#[test]
#[ignore = "native visual review requires a display; never runs in the unit suite"]
fn native_visual_fixture() {
    let number = |name: &str, default: f32| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let width = number("CANOPY_UI_WIDTH", 1440.);
    let height = number("CANOPY_UI_HEIGHT", 900.);
    let font = number("CANOPY_UI_FONT", 16.);
    let scene = std::env::var("CANOPY_UI_SCENE").unwrap_or_else(|_| "chat".into());
    gpui_kit::application().with_assets(super::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        super::theme::init(cx);
        super::terminal::init(cx);
        super::chat::init(cx);
        super::editor::init(cx);
        super::components::modal::init_prompts(cx);
        gpui_kit::component::Theme::global_mut(cx).font_size = px(font);
        gpui_kit::component::Theme::sync_base(cx);
        cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"))]).unwrap();
        let mut scratch = Model::new();
        scratch.open_conversation("Ideas for the next release", "claude");
        scratch.open_conversation("Research and notes", "codex");
        let app = AppState::install_test(scratch, cx);
        app.projects.update(cx, |p, _| p.ready = true);
        app.tools.update(cx, |tools, _| {
            tools.ready = true;
            tools.environment = Some(Ok(Arc::new(canopy_desktop::terminal::environment::ShellEnvironment {
                shell: "/bin/sh".into(), vars: Default::default(),
            })));
        });
        let mut selected = None;
        let mut cli = None;
        for (project_name, names) in [
            ("billing-api", ["main", "feature/retries"]),
            ("design-system", ["main", "feature/accessible-menus"]),
            ("canopy-desktop", ["main", "feature/workspace-navigation"]),
        ] {
            let root = std::path::PathBuf::from(format!("/visual-fixture/{project_name}"));
            let worktrees: Vec<_> = names.iter().enumerate().map(|(i, branch)| WorktreeInfo {
                path: if i == 0 { root.clone() } else { root.join("worktrees").join(branch.replace('/', "-")) },
                name: (i > 0).then(|| branch.replace('/', "-")), head: WorktreeHead::Branch((*branch).into()),
                label: (*branch).into(), locked: false, available: true, missing: false, registered_branch: None,
            }).collect();
            for wt in &worktrees {
                app.projects.update(cx, |p, cx| p.open_worktree(root.clone(), wt.path.clone(), cx));
                let (chat, terminal) = app.workspace.update(cx, |w, _| {
                    let chat = w.open_conversation(if project_name == "canopy-desktop" { "Workspace navigation and layout" } else { "Review pending changes" }, "claude");
                    let chat_pane = w.active().unwrap().focused;
                    w.set_pane_metadata(chat, chat_pane, PaneMetadata { kind: PaneKind::Chat, title: Some(w.tabs().last().unwrap().title.clone()), cwd: Some(wt.path.clone()), ..Default::default() }).unwrap();
                    w.open("Codex · implementation review", "codex");
                    let terminal = w.active().unwrap().focused;
                    w.open("Terminal 1", "shell");
                    w.open("Terminal 2", "shell");
                    if scene == "overflow" { for i in 3..9 { w.open(format!("Terminal {i}"), "shell"); } }
                    w.activate(chat).unwrap();
                    (chat_pane, terminal)
                });
                selected = Some(chat);
                cli = Some(terminal);
                let panes = app.workspace.read(cx).all_panes();
                app.agents.update(cx, |a, _| {
                    for pane in panes.iter().filter(|p| p.tool != "shell") {
                        a.insert_test_session(pane.clone(), if pane.id == chat { Status::Idle } else { Status::Working });
                    }
                });
            }
            app.git.update(cx, |g, _| { g.entries.insert(root.clone(), Ok(Some(Arc::new(RepositoryInfo { origin: None, common: root.join(".git"), root, branches: vec![], worktrees })))); });
        }
        app.layout.update(cx, |l, _| l.widths.left = 288.);
        app.workspace.update(cx, |w, _| {
            if scene == "empty" { let tabs: Vec<_> = w.tabs().iter().map(|t| t.id).collect(); for tab in tabs { w.close(tab).unwrap(); } }
            else if scene == "cli" { w.select_session(cli.unwrap()).unwrap(); }
            else { w.select_session(selected.unwrap()).unwrap(); w.show_tools(canopy_desktop::state::workspace::ToolPage::Tabs); }
        });
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(width), px(height))))),
            titlebar: Some(super::components::titlebar_options("Canopy · visual review")), ..Default::default()
        }, |window, cx| {
            let panes = app.workspace.read(cx).all_panes();
            for pane in panes {
                if pane.metadata.kind == PaneKind::Chat {
                    let id = pane.id;
                    let view = cx.new(|cx| super::chat::ChatView::new(pane, window, cx));
                    app.chats.update(cx, |c, _| { c.views.insert(id, view); });
                } else {
                    let id = pane.id;
                    let view = cx.new(|cx| super::terminal::TerminalView::visual_fixture(pane,
                        "$ cargo test --locked --test workspace\n\n  Running workspace navigation tests\n\n  test selection_preserves_terminal ... ok\n  test restore_keeps_worktree_context ... ok\n  test resize_preserves_preferences ... ok\n\n  test result: ok. 36 passed; 0 failed\n\n$ ", cx));
                    app.terminals.update(cx, |t, _| { t.views.insert(id, view); });
                }
            }
            let view = cx.new(|cx| Workspace::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        }).expect("open native review window");
        let _ = window.update(cx, |_, window, _| window.activate_window());
        cx.activate(true);
        if let Ok(path) = std::env::var("CANOPY_UI_PROBE") {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(std::time::Duration::from_millis(700)).await;
                let rectangles = window.update(cx, |_, window, cx| {
                    let ids = ["sidebar-new-chat-menu", "tool-panel-menu", "all-tool-tabs", "tool-page-Files", "tool-page-Agent", "toggle-sidebar", "new-terminal", "split-terminal", "maximize-tools", "return-to-work", "menu-project-/visual-fixture/canopy-desktop"];
                    let mut rectangles = serde_json::Map::new();
                    for id in ids {
                        if let Some(element) = window.try_find(id) {
                            let b = element.bounds();
                            rectangles.insert(id.into(), serde_json::json!({"x": f32::from(b.center().x), "y": f32::from(b.center().y), "left": f32::from(b.left()), "right": f32::from(b.right())}));
                        }
                    }
                    let pane = cx.global::<AppState>().workspace.read(cx).selected_pane().map(|p| p.id);
                    if let Some(pane) = pane && let Some(element) = window.try_find(gpui_kit::SharedString::from(format!("chat-menu-{pane:?}"))) {
                            let b = element.bounds();
                            rectangles.insert("chat-menu".into(), serde_json::json!({"x": f32::from(b.center().x), "y": f32::from(b.center().y)}));
                    }
                    rectangles
                }).unwrap();
                std::fs::write(path, serde_json::to_vec_pretty(&rectangles).unwrap()).unwrap();
            }).detach();
        }
        cx.spawn(async |cx| {
            cx.background_executor().timer(std::time::Duration::from_secs(30)).await;
            cx.update(|cx| cx.quit());
        }).detach();
    });
}
