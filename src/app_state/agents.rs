use super::AppState;
use canopy_desktop::{
    agents::{
        Event, Status,
        relay::{Registration, Relay},
    },
    state::workspace::{Pane, PaneId},
};
use gpui_kit::*;
use std::collections::{HashMap, VecDeque};
#[derive(Clone)]
pub struct AgentSession {
    pub pane: Pane,
    pub run: String,
    pub active: bool,
    pub unseen: bool,
    pub session: Option<String>,
    confirmed: bool,
    retired: VecDeque<String>,
    pub status: Status,
    pub model: Option<String>,
    pub permission: Option<String>,
    pub tool: Option<String>,
    pub response: Option<String>,
    pub question: Option<String>,
    attention: canopy_desktop::agents::Attention,
    pub recent: VecDeque<String>,
    pub error: Option<String>,
    pub event_count: u64,
    pub last_event: Option<String>,
    pub awaiting_events: bool,
}
pub struct AgentsState {
    pub ready: bool,
    pub notification_revision: u64,
    main_active: bool,
    window_obscured: bool,
    pub error: Option<String>,
    pub sessions: HashMap<PaneId, AgentSession>,
    relay: Option<Relay>,
    events: Option<Task<()>>,
    observers: Vec<Subscription>,
    questions: HashMap<(PaneId, String), Task<()>>,
    handshakes: HashMap<PaneId, Task<()>>,
}
impl AgentsState {
    pub fn new() -> Self {
        Self {
            ready: false,
            notification_revision: 0,
            main_active: false,
            window_obscured: false,
            error: None,
            sessions: HashMap::new(),
            relay: None,
            events: None,
            observers: vec![],
            questions: HashMap::new(),
            handshakes: HashMap::new(),
        }
    }
    #[cfg(test)]
    pub fn insert_test_session(&mut self, pane: Pane, status: Status) {
        self.sessions.insert(
            pane.id,
            AgentSession {
                pane,
                run: "test-run".into(),
                active: true,
                unseen: false,
                session: None,
                confirmed: false,
                retired: VecDeque::new(),
                status,
                model: None,
                permission: None,
                tool: None,
                response: None,
                question: None,
                attention: Default::default(),
                recent: VecDeque::new(),
                error: None,
                event_count: 0,
                last_event: None,
                awaiting_events: false,
            },
        );
    }
    pub fn bind(&mut self, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        self.observers = vec![
            cx.observe(&app.workspace, |this, _, cx| this.prune(cx)),
            cx.observe(&app.projects, |this, _, cx| this.prune(cx)),
        ];
        self.events = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { Relay::start() })
                .await;
            let relay = match result {
                Ok(relay) => relay,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.error = Some(error);
                        this.ready = true;
                        cx.notify();
                    });
                    return;
                }
            };
            let events = relay.events.clone();
            let _ = this.update(cx, |this, cx| {
                this.relay = Some(relay);
                this.ready = true;
                cx.notify();
            });
            while let Ok(event) = events.recv().await {
                let _ = this.update(cx, |this, cx| this.apply(event, cx));
            }
            let _ = this.update(cx, |this, cx| {
                this.error =
                    Some("Agent event connection stopped. Restart Canopy to reconnect.".into());
                for session in this.sessions.values_mut().filter(|s| s.active) {
                    session.error = this.error.clone();
                }
                this.handshakes.clear();
                cx.notify();
            });
        }));
    }
    pub fn register(&mut self, pane: Pane, cx: &mut Context<Self>) -> Result<Registration, String> {
        let relay = self.relay.as_ref().ok_or_else(|| {
            self.error
                .clone()
                .unwrap_or("Agent integration is starting.".into())
        })?;
        if let Some(previous) = self.sessions.get(&pane.id) {
            relay.remove(&previous.run);
        }
        self.questions.retain(|(id, _), _| *id != pane.id);
        self.handshakes.remove(&pane.id);
        let pane_id = pane.id;
        let registration = relay.register();
        self.sessions.insert(
            pane.id,
            AgentSession {
                session: pane.metadata.resume_id.clone(),
                confirmed: false,
                retired: VecDeque::new(),
                pane,
                run: registration.run.clone(),
                active: true,
                unseen: false,
                status: Status::Starting,
                model: None,
                permission: None,
                tool: None,
                response: None,
                question: None,
                attention: Default::default(),
                recent: VecDeque::new(),
                error: None,
                event_count: 0,
                last_event: None,
                awaiting_events: false,
            },
        );
        let run = registration.run.clone();
        self.handshakes.insert(
            pane_id,
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(12))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(session) = this
                        .sessions
                        .get_mut(&pane_id)
                        .filter(|s| s.active && s.run == run && s.event_count == 0)
                    {
                        session.awaiting_events = true;
                        cx.notify();
                    }
                });
            }),
        );
        cx.notify();
        Ok(registration)
    }
    fn visible(&self, pane: PaneId, cx: &App) -> bool {
        let app = cx.global::<AppState>();
        let workspace = app.workspace.read(cx);
        canopy_desktop::agents::pane_visible(
            self.main_active && !self.window_obscured,
            app.projects.read(cx).catalog.active,
            workspace,
            pane,
        )
    }
    pub fn window_obscured(&mut self, obscured: bool, cx: &mut Context<Self>) {
        if self.window_obscured == obscured {
            return;
        }
        self.window_obscured = obscured;
        self.acknowledge_visible(cx);
    }
    pub fn window_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.main_active = active;
        self.acknowledge_visible(cx);
    }
    fn acknowledge_visible(&mut self, cx: &mut Context<Self>) {
        let seen: Vec<_> = self
            .sessions
            .keys()
            .copied()
            .filter(|pane| self.visible(*pane, cx))
            .collect();
        let mut changed = false;
        for pane in seen {
            if let Some(session) = self.sessions.get_mut(&pane)
                && session.unseen
            {
                session.unseen = false;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }
    fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        let visible = self
            .sessions
            .values()
            .find(|s| s.run == event.run)
            .is_some_and(|s| self.visible(s.pane.id, cx))
            || cx.global::<AppState>().settings.read(cx).quitting;
        let Some(session) = self
            .sessions
            .values_mut()
            .find(|s| s.run == event.run && s.active)
        else {
            return;
        };
        if let Some(id) = &event.session
            && !event.subagent
        {
            if session.retired.contains(id) {
                return;
            }
            let changed = session.session.as_ref() != Some(id);
            if changed && session.session.is_some() {
                if !session.confirmed || event.name != "SessionStart" {
                    session.error = Some(
                        "Agent reported a different session than the saved resume target.".into(),
                    );
                    if canopy_desktop::agents::should_notify(
                        session.status,
                        Status::Failed,
                        visible,
                    ) {
                        session.unseen = true;
                        self.notification_revision += 1;
                    }
                    session.status = Status::Failed;
                    cx.notify();
                    return;
                }
                if let Some(previous) = session.session.clone() {
                    if session.retired.len() == 64 {
                        session.retired.pop_front();
                    }
                    session.retired.push_back(previous);
                }
            }
            if changed {
                let pane = session.pane.clone();
                cx.global::<AppState>()
                    .projects
                    .clone()
                    .update(cx, |projects, cx| {
                        projects.record_agent_session(&pane, id, cx)
                    });
                session.session = Some(id.clone());
                session.pane.metadata.resume_id = Some(id.clone());
            }
            session.confirmed = true;
        }
        if !event.subagent && !event.name.is_empty() {
            session.event_count = session.event_count.saturating_add(1);
            session.last_event = Some(event.name.clone());
            session.awaiting_events = false;
            self.handshakes.remove(&session.pane.id);
        }
        let watch = (session.pane.tool == "claude"
            && !event.subagent
            && event.name == "PreToolUse"
            && event.is_question_tool())
        .then(|| (session.pane.id, event.clone()));
        session.attention.apply(&event);
        let previous = session.status;
        session.status = if session.attention.waiting() {
            Status::Waiting
        } else {
            event.status(session.status)
        };
        if canopy_desktop::agents::should_notify(previous, session.status, visible) {
            session.unseen = true;
            self.notification_revision += 1;
        } else if session.status == Status::Working || event.name == "SessionStart" || visible {
            session.unseen = false;
        }
        session.question = session.attention.question();
        if event.model.is_some() {
            session.model = event.model;
        }
        if event.permission.is_some() {
            session.permission = event.permission;
        }
        if event.tool.is_some() {
            session.tool = event.tool;
        }
        if event.response.is_some() {
            session.response = event.response;
        }
        if session.recent.len() == 20 {
            session.recent.pop_front();
        }
        session.recent.push_back(event.name);
        self.prune_questions();
        if let Some((pane, event)) = watch {
            self.watch_question(pane, event, cx);
        }
        cx.notify();
    }
    fn prune_questions(&mut self) {
        self.questions.retain(|(pane, call), _| {
            self.sessions
                .get(pane)
                .is_some_and(|session| session.active && session.attention.contains(call))
        });
    }
    fn watch_question(&mut self, pane: PaneId, event: Event, cx: &mut Context<Self>) {
        let (Some(path), Some(session), Some(call)) = (
            event.transcript.clone(),
            event.session.clone(),
            event.tool_use_id.clone(),
        ) else {
            return;
        };
        if !path.is_absolute()
            || path.file_name().and_then(|v| v.to_str()) != Some(&format!("{session}.jsonl"))
        {
            return;
        }
        let key = (pane, call.clone());
        if self.questions.contains_key(&key) {
            return;
        }
        self.questions.insert(
            key,
            cx.spawn(async move |this, cx| {
                use canopy_desktop::agents::transcript::{QuestionWatch, read_rejection};
                let watch_path = path.clone();
                let watcher = cx
                    .background_executor()
                    .spawn(async move { QuestionWatch::new(&watch_path) })
                    .await;
                let watcher = match watcher {
                    Ok(watcher) => watcher,
                    Err(_) => {
                        let _ = this.update(cx, |this, cx| {
                            if let Some(s) =
                                this.sessions.get_mut(&pane).filter(|s| s.run == event.run)
                            {
                                s.error =
                                    Some("Could not watch Claude question cancellation.".into());
                                cx.notify();
                            }
                        });
                        return;
                    }
                };
                loop {
                    let (path, session, read_call) = (path.clone(), session.clone(), call.clone());
                    let rejected = cx
                        .background_executor()
                        .spawn(async move { read_rejection(&path, &session, &read_call) })
                        .await;
                    if matches!(rejected, Ok(true)) {
                        let _ =
                            this.update(cx, |this, cx| {
                                if !this.sessions.get(&pane).is_some_and(|s| {
                                    s.run == event.run && s.attention.contains(&call)
                                }) {
                                    return;
                                }
                                let mut resolved = event.clone();
                                resolved.name = "PostToolUseFailure".into();
                                resolved.interrupted = true;
                                resolved.question = None;
                                this.apply(resolved, cx);
                            });
                        break;
                    }
                    if !watcher.changed().await {
                        break;
                    }
                }
            }),
        );
    }
    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        let events = self.relay.as_ref().map(|relay| relay.events.clone());
        if let Some(events) = events {
            for _ in 0..128 {
                let Ok(event) = events.try_recv() else {
                    break;
                };
                self.apply(event, cx);
            }
        }
    }
    pub fn finish(
        &mut self,
        pane: PaneId,
        run: &str,
        failed: bool,
        natural: bool,
        cx: &mut Context<Self>,
    ) {
        self.flush(cx);
        if let Some(relay) = &self.relay {
            relay.remove(run);
        }
        let visible = self.visible(pane, cx) || cx.global::<AppState>().settings.read(cx).quitting;
        if let Some(session) = self.sessions.get_mut(&pane)
            && session.run == run
        {
            let next = if failed {
                Status::Failed
            } else {
                Status::Exited
            };
            if natural
                && canopy_desktop::agents::should_notify(session.status, next, visible)
                && !session.unseen
            {
                session.unseen = true;
                self.notification_revision += 1;
            }
            if !natural {
                session.unseen = false;
            }
            self.handshakes.remove(&pane);
            self.questions.retain(|(id, _), _| *id != pane);
            session.active = false;
            session.status = if failed {
                Status::Failed
            } else {
                Status::Exited
            };
            cx.notify();
        }
    }
    pub fn focus(&self, pane: PaneId, cx: &mut App) {
        let app = cx.global::<AppState>().clone();
        app.projects
            .update(cx, |projects, cx| projects.focus_agent(pane, cx));
    }

    fn prune(&mut self, cx: &mut Context<Self>) {
        let projects = cx.global::<AppState>().projects.clone();
        if !projects.read(cx).ready {
            return;
        }
        let alive: std::collections::HashSet<_> = projects
            .read(cx)
            .runtime_workspaces(cx)
            .into_iter()
            .flat_map(|w| w.all_panes())
            .map(|p| p.id)
            .collect();
        let before = self.sessions.len();
        self.sessions.retain(|pane, session| {
            if alive.contains(pane) {
                true
            } else {
                if let Some(relay) = &self.relay {
                    relay.remove(&session.run);
                }
                false
            }
        });
        self.handshakes.retain(|id, _| alive.contains(id));
        self.prune_questions();
        self.acknowledge_visible(cx);
        if before != self.sessions.len() {
            cx.notify();
        }
    }
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<()> {
        self.questions.clear();
        self.handshakes.clear();
        self.events = None;
        let relay = self.relay.take();
        cx.background_executor().spawn(async move {
            if let Some(relay) = relay {
                relay.shutdown();
            }
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IntegrationHealth {
    Starting,
    Receiving,
    AwaitingEvents,
    Issue,
    Stopped,
}
impl AgentSession {
    pub fn integration_health(&self) -> IntegrationHealth {
        if self.error.is_some() {
            IntegrationHealth::Issue
        } else if !self.active {
            IntegrationHealth::Stopped
        } else if self.event_count > 0 {
            IntegrationHealth::Receiving
        } else if self.awaiting_events {
            IntegrationHealth::AwaitingEvents
        } else {
            IntegrationHealth::Starting
        }
    }
}
