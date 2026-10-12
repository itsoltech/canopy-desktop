//! Switching agents mid-conversation (DEC-061…064): the conversation goes on with
//! another account or provider; the agent switched to gets the conversation so far
//! (built by `chat::handoff`) with the next message, or resumes its own earlier session
//! and gets only what happened since.
use super::{render::centered, view::ChatView};
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::{
    chat::{
        handoff::{self, Part},
        history::{AgentRef, Boundary},
        provider::Provider,
        transcript::{Body, Item, NoticeKind, Outcome, Transcript},
    },
    motion,
};
use gpui_kit::*;
use std::time::Instant;

/// The conversation so far, waiting for the next message to the agent switched to.
pub(super) struct Handoff {
    pub agent: &'static str,
    pub text: String,
    pub fold: Disclosure,
}
impl Handoff {
    fn new(agent: &'static str, text: String) -> Self {
        Self {
            agent,
            text,
            fold: Disclosure::new(false, Instant::now()),
        }
    }
}

fn name(agent: Option<&AgentRef>) -> &'static str {
    agent
        .and_then(|agent| Provider::from_tool(&agent.tool))
        .map_or("Agent", Provider::name)
}

/// "4.2 KB"
pub(super) fn size_label(text: &str) -> String {
    format!("{:.1} KB", text.len() as f32 / 1024.)
}

impl ChatView {
    /// Who runs the current session, as saved with it (DEC-065).
    pub(super) fn agent_ref(&self) -> AgentRef {
        let mut settings = self.settings.clone();
        // The chat's id and rules stay with the chat, not with one agent.
        settings.id = None;
        settings.allowed.clear();
        AgentRef {
            tool: self.pane.tool.clone(),
            profile: self.pane.metadata.profile_id.clone(),
            session: self.pane.metadata.resume_id.clone(),
            settings: Some(settings),
        }
    }

    /// Saves the running session's agent again once its session id or choices change.
    pub(super) fn record_meta(&mut self, cx: &mut Context<Self>) {
        if self.history_off
            || self
                .session
                .as_ref()
                .is_none_or(|session| session.status().ended())
        {
            return;
        }
        let agent = Some(self.agent_ref());
        let goal = self.session.as_ref().and_then(|s| s.goal()).cloned();
        let previous = self.goal_row.previous.clone();
        if self.epoch_meta.agent == agent
            && self.epoch_meta.goal == goal
            && self.epoch_meta.previous_goal == previous
        {
            return;
        }
        self.epoch_meta.agent = agent;
        self.epoch_meta.goal = goal;
        self.epoch_meta.previous_goal = previous;
        let Some(chat) = self.settings.id.clone() else {
            return;
        };
        let (epoch, meta) = (self.epoch, self.epoch_meta.clone());
        cx.global::<AppState>()
            .chat_history
            .clone()
            .update(cx, |history, cx| {
                history.record(&chat, epoch, Some(meta), [], false, cx)
            });
    }

    /// Every part of the conversation with its agent, oldest first; the running
    /// session last.
    fn parts(&self) -> Vec<(Option<&AgentRef>, &Transcript)> {
        let mut parts: Vec<_> = self
            .rows
            .archived
            .iter()
            .map(|archived| (archived.agent.as_ref(), &archived.transcript))
            .collect();
        if let Some(session) = &self.session {
            parts.push((self.epoch_meta.agent.as_ref(), session.transcript()));
        }
        parts
    }

    /// What `profile` of `provider` gets on a switch, and its last part here: since that
    /// part when its session can go on, otherwise the whole conversation.
    fn handoff_for(&self, provider: Provider, profile: &str) -> (Option<String>, Option<AgentRef>) {
        let tool = provider.tool();
        let parts = self.parts();
        let own = parts.iter().rposition(|(agent, _)| {
            agent.is_some_and(|agent| {
                agent.tool == tool && agent.profile.as_deref() == Some(profile)
            })
        });
        let earlier = own.and_then(|index| parts[index].0.cloned());
        let resumes = earlier
            .as_ref()
            .is_some_and(|agent| agent.session.is_some());
        let since = match own {
            Some(index) if resumes => index + 1,
            _ => 0,
        };
        let summary = handoff::summary(
            &parts[since..]
                .iter()
                .map(|(agent, transcript)| Part {
                    agent: name(*agent),
                    transcript,
                })
                .collect::<Vec<_>>(),
            provider.name(),
            resumes,
        );
        (summary, earlier)
    }

    /// Restored right after a switch, before its first message: the context the switch
    /// prepared is prepared again (it isn't saved).
    pub(super) fn restore_handoff(&mut self) {
        let switched = self.restored.as_ref().is_some_and(|last| {
            matches!(
                last.meta.before,
                Some(Boundary::SwitchedToClaude | Boundary::SwitchedToCodex)
            ) && !last
                .items
                .iter()
                .any(|item| matches!(item.body, Body::User(_)))
        });
        let Some(profile) = self.pane.metadata.profile_id.clone().filter(|_| switched) else {
            return;
        };
        let provider = self.provider();
        let (summary, _) = self.handoff_for(provider, &profile);
        self.handoff = summary.map(|text| Handoff::new(provider.name(), text));
    }

    /// Nothing runs, waits or is being edited: the agent can change.
    pub(super) fn can_switch_agent(&self) -> bool {
        !self.closed
            && !self.switching
            && !self.busy()
            && !self.is_working()
            && self.decision.active().is_none()
            && self.editing.is_none()
    }

    /// An account picked in the composer: before the first message a plain restart
    /// with it, afterwards a switch that carries the conversation over.
    pub(super) fn choose_account(
        &mut self,
        provider: Provider,
        profile: String,
        cx: &mut Context<Self>,
    ) {
        if self.account_locked() {
            self.switch_agent(provider, profile, cx);
        } else {
            self.set_account(provider, profile, cx);
        }
    }

    /// Goes on with `profile` of `provider`: its earlier session when it had one here
    /// (with what happened since), otherwise a new one with the whole conversation.
    pub(super) fn switch_agent(
        &mut self,
        provider: Provider,
        profile: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let tool = provider.tool();
        if !self.can_switch_agent()
            || (self.pane.tool == tool
                && self.pane.metadata.profile_id.as_deref() == Some(profile.as_str()))
        {
            return false;
        }
        // The goal belongs to the agent pursuing it (DEC-082): it ends here and is
        // offered to the next one.
        let goal = self.session.as_mut().and_then(|session| {
            let objective = session.goal()?.objective.clone();
            session.drop_goal(canopy_desktop::chat::goal::EndReason::ClearedOnSwitch);
            Some(objective)
        });
        if goal.is_some() {
            self.goal_row.previous = goal.clone();
            self.after_change(cx);
        }
        let (summary, earlier) = self.handoff_for(provider, &profile);
        let summary = match (summary, goal) {
            (summary, Some(goal)) => Some(format!(
                "{}The previous agent was working toward this goal when the user switched; \
                 it was cleared, and the user may set it for you again: {goal}",
                summary.map(|s| format!("{s}\n\n")).unwrap_or_default()
            )),
            (summary, None) => summary,
        };
        let resume = earlier.as_ref().and_then(|agent| agent.session.clone());
        // Its own choices from before, else the account's defaults in the same mode.
        let mut settings = match earlier.and_then(|agent| agent.settings) {
            Some(settings) => settings,
            None => {
                let mut settings = self.settings.clone();
                settings.mode = match provider {
                    Provider::Claude => settings.mode.for_claude(),
                    Provider::Codex => settings.mode.for_codex(),
                };
                settings.effort = None;
                settings.model = None;
                settings
            }
        };
        settings.id = self.settings.id.clone();
        settings.allowed = self.settings.allowed.clone();
        self.switching = true;
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            let _ = stop.await;
            let _ = this.update(cx, |this, cx| {
                this.switching = false;
                if this.closed {
                    return;
                }
                this.pane.tool = tool.to_owned();
                this.pane.metadata.profile_id = Some(profile.clone());
                this.pane.metadata.resume_id = resume.clone();
                this.settings = settings;
                let pane = this.pane.id;
                cx.global::<AppState>()
                    .projects
                    .clone()
                    .update(cx, |projects, cx| {
                        projects.switch_chat_agent(pane, tool, &profile, resume.as_deref(), cx)
                    });
                this.persist(None, cx);
                this.remember(cx);
                this.boundary = Boundary::switched_to(provider);
                this.handoff = summary.map(|text| Handoff::new(provider.name(), text));
                this.resuming_switch = resume.is_some();
                this.start(cx);
            });
        })
        .detach();
        cx.notify();
        true
    }

    /// The agent switched back to couldn't continue its earlier session: it starts a
    /// new one with the whole conversation, and the first message goes again.
    pub(super) fn check_resumed_switch(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_ref().filter(|_| self.resuming_switch) else {
            return;
        };
        if !session.status().ended() {
            // A turn completed: the session continued fine.
            if session.transcript().items().iter().any(|item| {
                matches!(&item.body, Body::TurnEnd(end) if end.outcome == Outcome::Completed)
            }) {
                self.resuming_switch = false;
            }
            return;
        }
        self.resuming_switch = false;
        let provider = self.provider();
        let message = session.retry_draft();
        // The failed attempt holds nothing the summary needs beyond the message.
        let summary = handoff::summary(
            &self
                .rows
                .archived
                .iter()
                .map(|archived| Part {
                    agent: name(archived.agent.as_ref()),
                    transcript: &archived.transcript,
                })
                .collect::<Vec<_>>(),
            provider.name(),
            false,
        );
        self.handoff = summary.map(|text| Handoff::new(provider.name(), text));
        self.history_note = Some(format!(
            "{} couldn't continue its earlier session here; it starts a new one with a summary \
             of the conversation.",
            provider.name()
        ));
        self.pane.metadata.resume_id = None;
        let (pane, tool) = (self.pane.id, self.pane.tool.clone());
        let profile = self.pane.metadata.profile_id.clone().unwrap_or_default();
        cx.global::<AppState>()
            .projects
            .clone()
            .update(cx, |projects, cx| {
                projects.switch_chat_agent(pane, &tool, &profile, None, cx)
            });
        self.switching = true;
        let stop = self.shutdown(cx);
        cx.spawn(async move |this, cx| {
            let _ = stop.await;
            let _ = this.update(cx, |this, cx| {
                this.switching = false;
                if this.closed {
                    return;
                }
                this.boundary = Boundary::Restarted;
                match message {
                    Some(mut draft) => {
                        // The new summary goes with it instead of the delta it had.
                        draft.context = None;
                        this.with_images(draft, cx, |this, draft, cx| {
                            this.send_on_start = Some(draft);
                            this.start(cx);
                        });
                    }
                    None => this.start(cx),
                }
            });
        })
        .detach();
    }

    /// Forgets the waiting context once a message took it.
    pub(super) fn sync_handoff(&mut self) {
        if self.handoff.is_some()
            && self
                .session
                .as_ref()
                .is_some_and(|session| session.handoff().is_none())
        {
            self.handoff = None;
        }
    }

    /// The other provider's default account, when it can run.
    pub(super) fn other_agent(&self, cx: &App) -> Option<(Provider, String)> {
        let provider = Provider::ALL
            .into_iter()
            .find(|provider| *provider != self.provider())?;
        let tools = cx.global::<AppState>().tools.read(cx);
        let tool = tools
            .catalog
            .get(provider.tool())
            .filter(|tool| tool.enabled)?;
        if matches!(tools.availability.get(provider.tool()), Some(Err(_))) {
            return None;
        }
        let profile = tool
            .default_profile
            .clone()
            .filter(|id| tool.profiles.iter().any(|p| &p.id == id))
            .or_else(|| tool.profiles.first().map(|p| p.id.clone()))?;
        Some((provider, profile))
    }

    /// A limit notice of the last, unfinished turn offers the other provider.
    pub(super) fn limit_switch(&self, item: &Item, cx: &App) -> Option<Provider> {
        let Body::Notice(notice) = &item.body else {
            return None;
        };
        if notice.kind != NoticeKind::RateLimit || !self.settled() {
            return None;
        }
        let transcript = self.session.as_ref()?.transcript();
        let last = transcript
            .items()
            .iter()
            .rfind(|i| i.parent.is_none())?
            .turn;
        let failed = transcript.items().iter().any(|i| {
            i.turn == item.turn
                && matches!(&i.body, Body::TurnEnd(end) if end.outcome != Outcome::Completed)
        });
        (item.turn == last && failed)
            .then(|| self.other_agent(cx))
            .flatten()
            .map(|(provider, _)| provider)
    }

    /// "Continue with …" on a limit: the other provider takes the last message again.
    pub(super) fn continue_elsewhere(&mut self, cx: &mut Context<Self>) {
        let Some((provider, profile)) = self.other_agent(cx) else {
            return;
        };
        let Some(mut draft) = self.session.as_ref().and_then(|s| s.retry_draft()) else {
            return;
        };
        // The context for the new agent goes with it, not an earlier one.
        draft.context = None;
        self.with_images(draft, cx, move |this, draft, cx| {
            if this.switch_agent(provider, profile, cx) {
                this.send_on_start = Some(draft);
            }
        });
    }

    /// Above the composer after a switch: what the next message brings the agent.
    pub(super) fn handoff_bar(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let handoff = self.handoff.as_ref()?;
        let now = Instant::now();
        let animate = motion::policy(cx) == motion::MotionPolicy::Full;
        let header = row()
            .id("chat-handoff-toggle")
            .keyboard(format!(
                "{} the context for {}",
                if handoff.fold.open { "Hide" } else { "Show" },
                handoff.agent
            ))
            .gap(px(8.))
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(t::muted())
            .hover(|style| style.text_color(t::secondary()))
            .child(custom_icon("arrow-right-left").size(px(13.)))
            .child(div().flex_1().min_w_0().truncate().child(format!(
                "Context for {} · {} — sent with your next message",
                handoff.agent,
                size_label(&handoff.text)
            )))
            .child(handoff.fold.chevron(now).text_color(t::faint()))
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(handoff) = &mut this.handoff {
                    handoff.fold.toggle(Instant::now(), cx);
                    cx.notify();
                }
            }));
        let body = context_text("chat-handoff-text", handoff.text.clone());
        Some(
            centered(
                column()
                    .mx(px(14.))
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(10.))
                    .gap(px(6.))
                    .rounded_t(px(12.))
                    .border_1()
                    .border_b_0()
                    .border_color(t::border())
                    .bg(t::composer_surface())
                    .child(header)
                    .child(handoff.fold.measured_body(
                        "chat-handoff-body",
                        AnimatedHeight::new("chat-handoff-height", animate, body),
                        now,
                    )),
            )
            .mb(px(-4.)),
        )
    }

    pub(super) fn handoff_animating(&self, now: Instant) -> bool {
        self.handoff
            .as_ref()
            .is_some_and(|handoff| handoff.fold.active(now))
    }
}

/// The context exactly as the agent gets it, scrollable.
pub(super) fn context_text(id: impl Into<ElementId>, text: String) -> NestedScroller {
    div()
        .id(id)
        .w_full()
        .max_h(px(240.))
        .overflow_y_scroll()
        .p(px(8.))
        .rounded(px(8.))
        .bg(t::bg())
        .text_size(px(12.))
        .text_color(t::secondary())
        .child(text)
        .nested_scroll()
}
