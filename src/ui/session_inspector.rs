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
    component::{IconName, scroll::ScrollableElement, tooltip::Tooltip},
    prelude::FluentBuilder,
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
    provider: String,
    status: Option<Status>,
    status_label: &'static str,
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
        let pane = workspace.selected_pane();
        let agents = app.agents.read(cx);
        let session = pane.and_then(|p| agents.sessions.get(&p.id));
        let mut out = Self {
            pane: pane.map(|p| p.id),
            run: session.map(|s| s.run.clone()),
            title: String::new(),
            provider: String::new(),
            status: None,
            status_label: "",
            empty: String::new(),
            info: vec![],
            health: None,
            focus_terminal: false,
            question: None,
            response: None,
            recent: vec![],
        };
        out.title = pane
            .map(|p| {
                workspace
                    .pane_title(p.id)
                    .unwrap_or_else(|| p.metadata.title.as_deref().unwrap_or(&p.tool))
                    .to_owned()
            })
            .unwrap_or_default();
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
        out.title = workspace
            .pane_title(session.pane.id)
            .unwrap_or_else(|| {
                session
                    .pane
                    .metadata
                    .title
                    .as_deref()
                    .unwrap_or(&session.pane.tool)
            })
            .to_owned();
        out.provider = provider;
        out.status = Some(session.status);
        out.status_label = session.status_label();
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
    title: SharedString,
    provider: String,
    status: Status,
    status_label: &'static str,
    selected: bool,
}

fn status_color(status: Status) -> Hsla {
    match status {
        Status::Working => t::accent(),
        Status::Waiting => t::yellow(),
        Status::Failed => t::red(),
        Status::Starting | Status::Exited => t::muted(),
        Status::Idle => t::green(),
    }
}

fn session_choices(cx: &App) -> Vec<SessionChoice> {
    let app = cx.global::<AppState>();
    let selected = app.workspace.read(cx).selected_pane().map(|pane| pane.id);
    let mut sessions: Vec<_> = app
        .agents
        .read(cx)
        .sessions
        .values()
        .filter(|session| session.active || session.unseen)
        .map(|session| {
            let title = session
                .pane
                .metadata
                .title
                .as_deref()
                .unwrap_or(&session.pane.tool)
                .to_owned();
            SessionChoice {
                pane: session.pane.id,
                label: format!("{title} — {}", session.status_label()).into(),
                title: title.into(),
                provider: app
                    .tools
                    .read(cx)
                    .catalog
                    .get(&session.pane.tool)
                    .map(|tool| tool.name.clone())
                    .unwrap_or_else(|| tool_label(&session.pane.tool)),
                status: session.status,
                status_label: session.status_label(),
                selected: selected == Some(session.pane.id),
            }
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
impl SessionInspector {
    fn session_list(&self, keyboard_active: bool) -> Div {
        column()
            .flex_shrink_0()
            .px(px(LIST_INSET))
            .pb(px(6.))
            .children(self.sessions.iter().map(|session| {
                let pane = session.pane;
                let highlighted = (keyboard_active && self.keyboard_selection == Some(pane))
                    || (!keyboard_active && session.selected);
                row()
                    .id(SharedString::from(format!("agent-session-{pane:?}")))
                    .w_full()
                    .h(px(LIST_ROW))
                    .px(px(6.))
                    .gap(px(7.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .keyboard(session.label.clone())
                    .bg(if highlighted {
                        t::selected()
                    } else {
                        rgba(0).into()
                    })
                    .when(!highlighted, |row| row.hover(|style| style.bg(t::hover())))
                    .text_size(px(12.))
                    .text_color(if highlighted {
                        t::text()
                    } else {
                        t::secondary()
                    })
                    .child(tool_icon(&session.provider, IconName::SquareTerminal))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(session.title.clone()),
                    )
                    .child(status_badge(
                        session.status_label,
                        status_color(session.status),
                    ))
                    .on_click(move |_, _, cx| {
                        cx.global::<AppState>()
                            .agents
                            .clone()
                            .update(cx, |agents, cx| agents.focus(pane, cx))
                    })
            }))
    }

    /// The selected pane's session: who runs it and in which state.
    fn session_card(&self) -> Div {
        let s = &self.content;
        row()
            .flex_shrink_0()
            .gap(px(8.))
            .child(
                row()
                    .size(px(26.))
                    .flex_shrink_0()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(t::elevated())
                    .child(tool_icon(&s.provider, IconName::SquareTerminal)),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t::text())
                            .child(s.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(t::faint())
                            .child(s.provider.clone()),
                    ),
            )
            .children(s.status.map(|status| {
                row()
                    .flex_shrink_0()
                    .gap(px(5.))
                    .h(px(20.))
                    .px(px(7.))
                    .rounded(px(5.))
                    .bg(status_color(status).opacity(0.12))
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(status_color(status))
                    .child(dot().size(px(6.)).bg(status_color(status)))
                    .child(s.status_label)
            }))
    }
}
impl Render for SessionInspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let keyboard_active = self.focus.is_focused(window);
        let s = self.content.clone();
        let focus_button = |pane: PaneId, active: bool| {
            button("focus-agent-terminal", "Focus terminal")
                .h(px(24.))
                .disabled(!active)
                .on_click(move |_, _, cx| {
                    cx.global::<AppState>()
                        .agents
                        .clone()
                        .update(cx, |s, cx| s.focus(pane, cx))
                })
        };
        let mut body = column()
            .w_full()
            .flex_shrink_0()
            .gap(px(12.))
            .pt(px(12.))
            .pb(px(20.))
            .px(px(PANEL_INSET));
        if s.status.is_some() {
            body = body.child(self.session_card());
        } else if self.sessions.is_empty() || !s.empty.is_empty() {
            body = body.child(
                column()
                    .flex_shrink_0()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t::text())
                            .child(if s.title.is_empty() {
                                "No pane selected".to_owned()
                            } else {
                                s.title.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t::faint())
                            .child(s.empty.clone()),
                    ),
            );
        }
        // What the agent asks is the first thing to act on.
        if s.question.is_some() {
            body = body.child(
                column()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t::yellow().opacity(0.25))
                    .bg(t::yellow().opacity(0.07))
                    .child(panel_caption("Waiting for you").text_color(t::yellow()))
                    .child(self.question.clone())
                    .children(
                        s.pane
                            .map(|pane| row().pt(px(2.)).child(focus_button(pane, self.active))),
                    ),
            );
        }
        if !s.info.is_empty() || s.health.is_some() {
            body = body.child(
                column()
                    .flex_shrink_0()
                    .gap(px(5.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t::border())
                    .bg(t::panel())
                    .children(s.info.iter().map(|row| {
                        session_info_detail(row.label, row.value.clone(), row.detail.clone())
                    }))
                    .children(s.health.as_ref().map(|(label, color, detail)| {
                        integration_health(label.clone(), *color, detail.clone())
                    })),
            );
        }
        if s.question.is_none()
            && let Some(pane) = s.pane.filter(|_| s.focus_terminal)
        {
            body = body.child(row().child(focus_button(pane, self.active)));
        }
        body = body
            .children(s.response.as_ref().map(|_| {
                session_section("Last response")
                    .gap(px(6.))
                    .child(self.response.clone())
            }))
            .children((!s.recent.is_empty()).then(|| {
                session_section("Recent activity")
                    .gap(px(2.))
                    .children(s.recent.iter().map(|(id, label, raw)| {
                        let raw = raw.clone();
                        row()
                            .id(SharedString::from(format!(
                                "session-activity-{}-{id}",
                                s.run.as_deref().unwrap_or("")
                            )))
                            .flex_shrink_0()
                            .gap(px(8.))
                            .min_h(px(20.))
                            .text_size(px(11.5))
                            .text_color(t::secondary())
                            .child(dot().size(px(4.)).bg(t::faint()))
                            .child(div().flex_1().min_w_0().child(label.clone()))
                            .tooltip(move |w, cx| Tooltip::new(raw.clone()).build(w, cx))
                    }))
            }));
        let sessions = self.sessions.len();
        let list = (sessions > 0).then(|| self.session_list(keyboard_active));
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
                    .size_full()
                    .child(
                        panel_header()
                            .h(px(32.))
                            .child(panel_caption("Sessions"))
                            .child(div().flex_1())
                            .children((sessions > 0).then(|| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::faint())
                                    .child(sessions.to_string())
                            })),
                    )
                    .children(list)
                    .children((sessions == 0).then(|| {
                        div()
                            .flex_shrink_0()
                            .px(px(PANEL_INSET))
                            .pb(px(10.))
                            .text_size(px(11.5))
                            .text_color(t::faint())
                            .child("No active or unread agent sessions.")
                    }))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .border_t_1()
                            .border_color(t::border())
                            .child(
                                column()
                                    .id("agent-inspector-scroll")
                                    .size_full()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll)
                                    .child(body),
                            )
                            .vertical_scrollbar(&self.scroll),
                    ),
            )
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
