//! One scroll viewport per Session page; prepared content and Markdown states have an owner.
use super::{components::session::*, components::*, markdown::MarkdownView, theme as t};
use crate::app_state::{AgentSession, AppState, IntegrationHealth};
use canopy_desktop::{
    agents::{
        Status,
        presentation::{event_label, mode_label, tool_label},
    },
    state::workspace::PaneId,
};
use gpui_kit::{
    base::Disableable,
    component::{scroll::ScrollableElement, tooltip::Tooltip},
    *,
};
use std::sync::Arc;

actions!(
    session_inspector,
    [PreviousSession, NextSession, OpenSession, ReturnToTerminal]
);
#[derive(Clone, PartialEq)]
struct Metadata {
    label: &'static str,
    value: String,
    detail: String,
}
#[derive(Clone, PartialEq)]
struct SessionContent {
    pane: Option<PaneId>,
    run: Option<String>,
    title: String,
    status: Option<Status>,
    empty: String,
    info: Vec<Metadata>,
    health: Option<(String, Hsla, String)>,
    focus_terminal: bool,
    question: Option<Arc<str>>,
    response: Option<Arc<str>>,
    recent: Vec<(u64, String, String)>,
}
impl SessionContent {
    fn read(cx: &App) -> Self {
        let app = cx.global::<AppState>();
        let workspace = app.workspace.read(cx);
        let pane = workspace.active().and_then(|t| t.root.find(t.focused));
        let agents = app.agents.read(cx);
        let session = pane.and_then(|p| agents.sessions.get(&p.id));
        let mut out = Self {
            pane: pane.map(|p| p.id),
            run: session.map(|s| s.run.clone()),
            title: String::new(),
            status: None,
            empty: String::new(),
            info: vec![],
            health: None,
            focus_terminal: false,
            question: None,
            response: None,
            recent: vec![],
        };
        let Some(session) = session else {
            out.empty = match pane.map(|p| p.tool.as_str()) {
                Some("claude" | "codex") => "Agent session has not started yet.",
                Some(_) => "This pane has no agent integration.",
                None => "Select a pane to inspect its session.",
            }
            .into();
            if pane.is_some_and(|p| matches!(p.tool.as_str(), "claude" | "codex"))
                && let Some(error) = &agents.error
            {
                out.health = Some(("Unavailable".into(), t::red(), error.clone()));
            }
            return out;
        };
        let catalog = &app.tools.read(cx).catalog;
        let tool = catalog.get(&session.pane.tool);
        let provider = tool
            .map(|t| t.name.clone())
            .unwrap_or_else(|| tool_label(&session.pane.tool));
        out.title = format!("{provider} session");
        out.status = Some(session.status);
        out.health = Some(health(session));
        out.focus_terminal = matches!(
            session.integration_health(),
            IntegrationHealth::AwaitingEvents | IntegrationHealth::Issue
        );
        let mut add = |label, value: String, detail: Option<String>| {
            out.info.push(Metadata {
                label,
                detail: detail.unwrap_or_else(|| value.clone()),
                value,
            })
        };
        add(
            "Session",
            session
                .session
                .clone()
                .unwrap_or_else(|| "Waiting for first hook".into()),
            None,
        );
        add(
            "Model",
            session.model.clone().unwrap_or_else(|| "—".into()),
            None,
        );
        add(
            "Mode",
            session
                .permission
                .as_deref()
                .map(mode_label)
                .unwrap_or_else(|| "—".into()),
            session.permission.clone(),
        );
        let profile_id = session.pane.metadata.profile_id.as_deref();
        let profile = profile_id
            .and_then(|id| tool?.profiles.iter().find(|p| p.id == id))
            .map(|p| p.name.clone());
        add(
            "Profile",
            profile.unwrap_or_else(|| {
                if profile_id.is_some() {
                    "Unavailable profile".into()
                } else {
                    "Default".into()
                }
            }),
            profile_id.map(str::to_owned),
        );
        add(
            "Tool",
            session
                .tool
                .as_deref()
                .map(tool_label)
                .unwrap_or_else(|| "—".into()),
            session.tool.clone(),
        );
        add(
            "Folder",
            session
                .pane
                .metadata
                .cwd
                .as_ref()
                .map(|p| canopy_desktop::platform::paths::display(p))
                .unwrap_or_else(|| "—".into()),
            None,
        );
        out.question = session
            .question
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(Arc::from);
        out.response = session
            .response
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(Arc::from);
        out.recent = session
            .recent
            .iter()
            .rev()
            .take(8)
            .enumerate()
            .map(|(i, event)| {
                (
                    session.event_count.saturating_sub(i as u64),
                    event_label(event),
                    event.clone(),
                )
            })
            .collect();
        out
    }
}
pub struct SessionInspector {
    content: SessionContent,
    sessions: Vec<SessionChoice>,
    keyboard_selection: Option<PaneId>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    active: bool,
    scroll: ScrollHandle,
    response: Entity<MarkdownView>,
    question: Entity<MarkdownView>,
    _observers: Vec<Subscription>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionChoice {
    pane: canopy_desktop::state::workspace::PaneId,
    label: SharedString,
    selected: bool,
}

fn session_choices(cx: &App) -> Vec<SessionChoice> {
    let app = cx.global::<AppState>();
    let selected = app.workspace.read(cx).active().map(|tab| tab.focused);
    let mut sessions: Vec<_> = app
        .agents
        .read(cx)
        .sessions
        .values()
        .filter(|session| session.active || session.unseen)
        .map(|session| SessionChoice {
            pane: session.pane.id,
            label: format!(
                "{} — {}",
                session
                    .pane
                    .metadata
                    .title
                    .as_deref()
                    .unwrap_or(&session.pane.tool),
                session.status.label()
            )
            .into(),
            selected: selected == Some(session.pane.id),
        })
        .collect();
    sessions.sort_by_key(|session| (session.label.to_string(), format!("{:?}", session.pane)));
    sessions
}
impl SessionInspector {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            KeyBinding::new("up", PreviousSession, Some("SessionInspector")),
            KeyBinding::new("down", NextSession, Some("SessionInspector")),
            KeyBinding::new("enter", OpenSession, Some("SessionInspector")),
            KeyBinding::new("escape", ReturnToTerminal, Some("SessionInspector")),
        ]);
        let app = cx.global::<AppState>().clone();
        let mut s = Self {
            content: SessionContent::read(cx),
            sessions: session_choices(cx),
            keyboard_selection: None,
            focus: cx.focus_handle(),
            return_focus: None,
            active: false,
            scroll: ScrollHandle::new(),
            response: cx.new(MarkdownView::live),
            question: cx.new(MarkdownView::live),
            _observers: vec![
                cx.observe(&app.agents, |s, _, cx| s.sync(cx)),
                cx.observe(&app.workspace, |s, _, cx| s.sync(cx)),
                cx.observe(&app.tools, |s, _, cx| s.sync(cx)),
                cx.observe(&app.layout, |s, _, cx| s.sync(cx)),
            ],
        };
        s.sync(cx);
        s
    }
    pub fn focus_sessions(
        &mut self,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.return_focus = Some(return_focus);
        self.keyboard_selection = preferred_session(&self.sessions);
        self.focus.focus(window, cx);
        cx.notify();
    }
    #[cfg(test)]
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.keyboard_selection = move_session(&self.sessions, self.keyboard_selection, delta);
        cx.notify();
    }
    fn open_selection(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self.keyboard_selection else {
            return;
        };
        cx.global::<AppState>()
            .agents
            .clone()
            .update(cx, |agents, cx| agents.focus(pane, cx));
    }
    fn return_to_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.return_focus.take() {
            focus.focus(window, cx);
            cx.notify();
        }
    }
    fn sync(&mut self, cx: &mut Context<Self>) {
        let content = SessionContent::read(cx);
        let sessions = session_choices(cx);
        let active = {
            let layout = cx.global::<AppState>().layout.read(cx);
            layout.inspector_open && !layout.inspector_changes && !layout.inspector_tasks
        };
        if self.content.pane != content.pane || self.content.run != content.run {
            self.scroll.set_offset(point(px(0.), px(0.)));
            for view in [&self.response, &self.question] {
                view.update(cx, |v, cx| {
                    v.set_content(Arc::from(""), String::new(), false, cx)
                });
            }
        }
        self.response.update(cx, |view, cx| {
            view.set_content(
                content.response.clone().unwrap_or_else(|| Arc::from("")),
                String::new(),
                active,
                cx,
            )
        });
        self.question.update(cx, |view, cx| {
            view.set_content(
                content.question.clone().unwrap_or_else(|| Arc::from("")),
                String::new(),
                active,
                cx,
            )
        });
        let selection_missing = self
            .keyboard_selection
            .is_some_and(|pane| !sessions.iter().any(|session| session.pane == pane));
        let sessions_arrived_while_focused = self.keyboard_selection.is_none()
            && self.return_focus.is_some()
            && !sessions.is_empty();
        if selection_missing || sessions_arrived_while_focused {
            self.keyboard_selection = preferred_session(&sessions);
        }
        if self.content != content || self.sessions != sessions || self.active != active {
            self.active = active;
            self.content = content;
            self.sessions = sessions;
            cx.notify();
        }
    }
}
impl Render for SessionInspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = &self.content;
        let keyboard_active = self.focus.is_focused(window);
        let mut body = column()
            .w_full()
            .flex_shrink_0()
            .gap(px(16.))
            .pt(px(12.))
            .pb(px(20.))
            .pr(px(8.));
        if self.sessions.is_empty() {
            body = body.child(
                session_section("AGENT SESSIONS").child(
                    div()
                        .text_color(t::muted())
                        .child("No active or unread agent sessions."),
                ),
            );
        } else {
            body = body.child(session_section("AGENT SESSIONS").gap(px(4.)).children(
                self.sessions.iter().map(|session| {
                    let pane = session.pane;
                    let button = list_button(
                        SharedString::from(format!("agent-session-{pane:?}")),
                        session.label.clone(),
                        false,
                        cx,
                    )
                    .w_full();
                    let button = if (keyboard_active
                        && self.keyboard_selection == Some(session.pane))
                        || (!keyboard_active && session.selected)
                    {
                        button.bg(t::selected())
                    } else {
                        button
                    };
                    button.on_click(move |_, _, cx| {
                        cx.global::<AppState>()
                            .agents
                            .clone()
                            .update(cx, |agents, cx| agents.focus(pane, cx))
                    })
                }),
            ));
        }
        if let Some(status) = s.status {
            let color = match status {
                Status::Working => t::accent(),
                Status::Waiting => t::yellow(),
                Status::Failed => t::red(),
                Status::Starting | Status::Exited => t::muted(),
                Status::Idle => t::green(),
            };
            body = body
                .child(caption(s.title.clone()).flex_shrink_0())
                .child(session_status(status.label(), color));
        } else {
            body = body.child(
                div()
                    .flex_shrink_0()
                    .text_color(t::muted())
                    .child(s.empty.clone()),
            );
        }
        if let Some((label, color, detail)) = &s.health {
            body = body.child(integration_health(label.clone(), *color, detail.clone()));
        }
        body = body
            .children((!s.info.is_empty()).then(|| {
                column()
                    .w_full()
                    .flex_shrink_0()
                    .gap(px(8.))
                    .children(s.info.iter().map(|row| {
                        session_info_detail(row.label, row.value.clone(), row.detail.clone())
                    }))
            }))
            .children(s.question.as_ref().map(|_| {
                session_section("WAITING FOR YOU")
                    .gap(px(8.))
                    .child(self.question.clone())
            }))
            .children(s.response.as_ref().map(|_| {
                session_section("LAST RESPONSE")
                    .gap(px(8.))
                    .child(self.response.clone())
            }))
            .children((!s.recent.is_empty()).then(|| {
                session_section("RECENT ACTIVITY")
                    .gap(px(6.))
                    .children(s.recent.iter().map(|(id, label, raw)| {
                        let raw = raw.clone();
                        div()
                            .id(SharedString::from(format!(
                                "session-activity-{}-{id}",
                                s.run.as_deref().unwrap_or("")
                            )))
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .line_height(px(17.))
                            .text_color(t::secondary())
                            .child(label.clone())
                            .tooltip(move |w, cx| Tooltip::new(raw.clone()).build(w, cx))
                    }))
            }))
            .children(s.pane.filter(|_| s.focus_terminal).map(|pane| {
                button("focus-agent-terminal", "Focus terminal")
                    .disabled(!self.active)
                    .flex_shrink_0()
                    .on_click(move |_, _, cx| {
                        cx.global::<AppState>()
                            .agents
                            .clone()
                            .update(cx, |s, cx| s.focus(pane, cx))
                    })
            }));
        div()
            .id("session-inspector")
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .key_context("SessionInspector")
            .on_action(cx.listener(|this, _: &PreviousSession, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &NextSession, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &OpenSession, _, cx| this.open_selection(cx)))
            .on_action(cx.listener(|this, _: &ReturnToTerminal, window, cx| {
                this.return_to_terminal(window, cx)
            }))
            .child(
                column()
                    .id("agent-inspector-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(body),
            )
            .vertical_scrollbar(&self.scroll)
    }
}

fn preferred_session(sessions: &[SessionChoice]) -> Option<PaneId> {
    sessions
        .iter()
        .find(|session| session.selected)
        .or_else(|| sessions.first())
        .map(|session| session.pane)
}

fn move_session(
    sessions: &[SessionChoice],
    selected: Option<PaneId>,
    delta: isize,
) -> Option<PaneId> {
    if sessions.is_empty() {
        return None;
    }
    let current = sessions
        .iter()
        .position(|session| Some(session.pane) == selected);
    let index = current
        .map(|index| {
            index
                .saturating_add_signed(delta)
                .min(sessions.len().saturating_sub(1))
        })
        .unwrap_or(0);
    Some(sessions[index].pane)
}
fn health(session: &AgentSession) -> (String, Hsla, String) {
    use IntegrationHealth as H;
    let (label,color,detail)=match session.integration_health(){
        H::Starting=>("Connecting",t::muted(),"Waiting for the first agent event.".to_owned()),
        H::Receiving=>("Events received",t::green(),format!("{} events · Last: {}",session.event_count,session.last_event.as_deref().map(event_label).unwrap_or_else(||"—".into()))),
        H::AwaitingEvents=>("No events received",t::yellow(),if session.pane.tool=="codex"{"Check the Codex terminal for hook approval or startup errors. Use /hooks to review and enable the Canopy helper. Waiting alone does not confirm that approval is required."}else{"Check the Claude terminal for startup errors and whether hooks are enabled."}.into()),
        H::Issue=>("Integration issue",t::red(),session.error.clone().unwrap_or_default()),
        H::Stopped=>("Stopped",t::muted(),if session.event_count==0{"The process ended without sending agent events.".into()}else{format!("{} events received before the process ended.",session.event_count)}),
    };
    (label.into(), color, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app_state::AppState, ui::terminal::TerminalView};
    use canopy_desktop::{
        agents::Status,
        state::{projects::Projects, workspace::Axis},
        terminal::environment::ShellEnvironment,
    };
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext};
    use std::sync::Arc;

    struct Fixture {
        second: PaneId,
        first_terminal: FocusHandle,
        second_terminal: FocusHandle,
    }

    fn install_fixture(with_sessions: bool, cx: &mut App) -> Fixture {
        let mut projects = Projects::default();
        let workspace_id = projects.open(std::env::current_dir().unwrap());
        let mut workspace = canopy_desktop::state::workspace::Workspace::empty(workspace_id);
        let tab = workspace.open("Agents", "claude");
        let first = workspace.active().unwrap().focused;
        let second = workspace
            .split(tab, first, Axis::Horizontal, "codex")
            .unwrap();
        workspace.focus(tab, first).unwrap();
        let panes = workspace.all_panes();

        let app = AppState::install_test(workspace, cx);
        app.projects.update(cx, |state, _| {
            state.catalog = projects;
            state.ready = true;
        });
        app.tools.update(cx, |state, _| state.ready = true);

        let first_view = cx.new(|cx| {
            TerminalView::new(
                panes.iter().find(|pane| pane.id == first).unwrap().clone(),
                Err::<Arc<ShellEnvironment>, _>("test terminal".into()),
                cx,
            )
        });
        let second_view = cx.new(|cx| {
            TerminalView::new(
                panes.iter().find(|pane| pane.id == second).unwrap().clone(),
                Err::<Arc<ShellEnvironment>, _>("test terminal".into()),
                cx,
            )
        });
        app.terminals.update(cx, |state, _| {
            state.views.insert(first, first_view.clone());
            state.views.insert(second, second_view.clone());
        });
        if with_sessions {
            app.agents.update(cx, |state, _| {
                for pane in panes {
                    state.insert_test_session(pane, Status::Working);
                }
            });
        }
        Fixture {
            second,
            first_terminal: first_view.read(cx).focus_handle(),
            second_terminal: second_view.read(cx).focus_handle(),
        }
    }

    fn session_shortcut() -> &'static str {
        if cfg!(target_os = "windows") {
            "ctrl-shift-a"
        } else {
            "cmd-shift-s"
        }
    }

    #[gpui_kit::test]
    fn terminal_shortcut_selects_another_session_using_only_the_keyboard(cx: &mut TestAppContext) {
        let (window, fixture) = cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::theme::init(cx);
            let fixture = install_fixture(true, cx);
            let focus = fixture.first_terminal.clone();
            let window = cx
                .open_window(Default::default(), |window, cx| {
                    let workspace = cx.new(|cx| super::super::Workspace::new(window, cx));
                    focus.focus(window, cx);
                    workspace
                })
                .unwrap();
            (window, fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = window.root(&mut cx).unwrap();

        assert!(cx.update(|window, _| fixture.first_terminal.is_focused(window)));
        cx.simulate_keystrokes(session_shortcut());
        assert!(cx.update(|window, cx| {
            workspace
                .read(cx)
                .inspector
                .read(cx)
                .session_focus_handle(cx)
                .is_focused(window)
        }));
        cx.simulate_keystrokes("down enter");
        // TerminalView applies request_focus from AgentsState on its next frame.
        assert!(cx.update(|window, cx| window.simulate_next_frame(cx)) > 0);

        cx.update(|window, cx| {
            assert_eq!(
                cx.global::<AppState>()
                    .workspace
                    .read(cx)
                    .active()
                    .unwrap()
                    .focused,
                fixture.second
            );
            assert!(fixture.second_terminal.is_focused(window));
        });
    }

    #[gpui_kit::test]
    fn empty_session_list_keeps_escape_return_to_terminal(cx: &mut TestAppContext) {
        let (window, fixture) = cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::theme::init(cx);
            let fixture = install_fixture(false, cx);
            let focus = fixture.first_terminal.clone();
            let window = cx
                .open_window(Default::default(), |window, cx| {
                    let workspace = cx.new(|cx| super::super::Workspace::new(window, cx));
                    focus.focus(window, cx);
                    workspace
                })
                .unwrap();
            (window, fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let workspace = window.root(&mut cx).unwrap();

        cx.simulate_keystrokes(session_shortcut());
        assert!(cx.update(|window, cx| {
            workspace
                .read(cx)
                .inspector
                .read(cx)
                .session_focus_handle(cx)
                .is_focused(window)
        }));
        cx.simulate_keystrokes("escape");
        assert!(cx.update(|window, _| fixture.first_terminal.is_focused(window)));
    }
}
