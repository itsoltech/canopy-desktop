//! Panel above the composer for requests waiting on the user (DEC-040): tool
//! permissions, the agent's questions and plan approval. One request at a time, oldest
//! first, in the place and style of the queue tray; the composer's text is the user's
//! own answer, a note when denying, or feedback on a plan.
use super::{render::centered, tools::diff_line, view::ChatView};
use crate::ui::{components::*, markdown::MarkdownView, theme as t};
use canopy_desktop::{
    chat::{
        acp::PermissionOption,
        presentation::{self, Question},
        provider::Provider,
        session::Response,
        tools::{self, Proposal},
        transcript::{Body, ItemId, Permission, PermissionKind, Remember},
    },
    motion::{
        self, Easing, MotionPolicy, Presence, PresenceSpec, Transition, TransitionSpec, duration,
        presets,
    },
};
use gpui_kit::component::{IconName, tooltip::Tooltip};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::Instant};

/// The panel opens and closes like a panel; its content swaps like text.
const SPEC: PresenceSpec = presets::PANEL;
const SWAP: TransitionSpec = TransitionSpec::new(duration::QUICK, Easing::EaseInOut);
/// Diff and plan previews scroll inside the panel beyond these heights.
const PREVIEW_HEIGHT: f32 = 220.;
const PLAN_HEIGHT: f32 = 320.;
/// The panel's content scrolls beyond this, keeping the conversation in view.
const PANEL_HEIGHT: f32 = 440.;
/// Lines of a proposed change shown at most.
const PREVIEW_LINES: usize = 300;
/// A new request ignores input this long, so a double click or a key meant for the
/// previous one never answers it unseen.
const ARM: std::time::Duration = std::time::Duration::from_millis(400);
/// The empty field names its shortcuts: commands, files, skills, pull requests, shell.
pub(super) const COMPOSER_PLACEHOLDER: &str =
    "Ask anything — / commands, @ files, $ skills, # pull requests, ! shell";

pub(super) struct Request {
    pub item: ItemId,
    pub permission: Permission,
    /// The subagent that asks, by title.
    pub asker: Option<String>,
    pub questions: Vec<Question>,
    pub step: usize,
    /// Picked options per question.
    pub chosen: Vec<BTreeSet<usize>>,
    /// Own answers per question, kept while moving between questions.
    pub typed: Vec<String>,
    pub plan: Option<Entity<MarkdownView>>,
    pub proposal: Option<Proposal>,
    /// Write: whether the file already exists (checked off the UI thread).
    pub replaces: Option<bool>,
    /// When the request was shown; input before `ARM` has passed is ignored.
    pub shown: Instant,
    /// An ACP agent's own options, ids and kinds as it sent them (AGT-11); `None` for
    /// Claude's and Codex's requests.
    pub offered: Option<Vec<PermissionOption>>,
}
impl Request {
    fn armed(&self) -> bool {
        self.shown.elapsed() >= ARM
    }
}

/// Composer text and placeholder to apply once a window is at hand.
pub(super) struct ComposerChange {
    pub text: Option<gpui_kit::component::input::InputContent>,
    pub placeholder: &'static str,
}

pub(super) struct DecisionPanel {
    pub request: Option<Request>,
    /// 1-based position of the request among those waiting, and their count.
    pub position: (usize, usize),
    presence: Option<Presence>,
    leaving: bool,
    /// Open while expanded; the header stays when folded.
    fold: Disclosure,
    /// Content of a new request or question coming in.
    swap: Transition,
    /// Composer text from before the first request, restored once none waits.
    draft: Option<String>,
    pub composer: Option<ComposerChange>,
}

impl Default for DecisionPanel {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            request: None,
            position: (0, 0),
            presence: None,
            leaving: false,
            fold: Disclosure::new(true, now),
            swap: Transition::new(1., now),
            draft: None,
            composer: None,
        }
    }
}

impl DecisionPanel {
    /// The request the user can answer now (not one on its way out).
    pub fn active(&self) -> Option<&Request> {
        self.request.as_ref().filter(|_| !self.leaving)
    }

    pub fn active_mut(&mut self) -> Option<&mut Request> {
        self.request.as_mut().filter(|_| !self.leaving)
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.presence.as_ref().is_some_and(|p| p.is_animating(now))
            || self.fold.active(now)
            || self.swap.is_animating(now)
    }

    pub fn collapsed(&self) -> bool {
        !self.fold.open
    }

    fn toggle_fold(&mut self, cx: &App) {
        self.fold.toggle(Instant::now(), cx);
    }

    /// New content fades and rises into place.
    fn swap_in(&mut self, now: Instant, policy: MotionPolicy) {
        self.swap = Transition::new(0., now);
        self.swap.retarget(1., SWAP, now, policy);
    }

    /// Unmounts the request once its exit has finished.
    fn prune(&mut self, now: Instant) {
        if self.leaving && !self.animating(now) {
            self.request = None;
            self.presence = None;
            self.leaving = false;
            // Its restore is already on its way to the composer.
            self.draft = None;
        }
    }
}

/// One option row and what choosing it does.
struct Choice {
    label: String,
    description: String,
    action: Action,
}

#[derive(Clone)]
enum Action {
    Allow,
    AllowForChat,
    Deny,
    Approve {
        accept_edits: bool,
    },
    KeepPlanning,
    /// Pick (single choice) or toggle (several) an option of the current question.
    Option(usize),
    /// The ACP agent's option with this id.
    Offered(String),
}

fn choices(request: &Request) -> Vec<Choice> {
    let choice = |label: &str, description: String, action| Choice {
        label: label.to_owned(),
        description,
        action,
    };
    if let Some(offered) = &request.offered {
        // Named as the agent names them; what "always" covers is the agent's to say.
        return offered
            .iter()
            .map(|option| Choice {
                label: option.name.clone(),
                description: match option.kind.as_str() {
                    "allow_once" | "reject_once" => "Only this time".into(),
                    "allow_always" | "reject_always" => "The agent remembers this choice".into(),
                    _ => String::new(),
                },
                action: Action::Offered(option.option_id.clone()),
            })
            .collect();
    }
    match request.permission.kind {
        PermissionKind::Tool => {
            let mut list = vec![choice("Allow", "Only this time".into(), Action::Allow)];
            match &request.permission.remember {
                Some(Remember::Rules(rules)) => list.push(choice(
                    "Always allow in this chat",
                    rules
                        .iter()
                        .map(|rule| rule.text())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Action::AllowForChat,
                )),
                Some(Remember::AcceptEdits) => list.push(choice(
                    "Accept edits in this chat",
                    "File edits no longer ask in this chat".into(),
                    Action::AllowForChat,
                )),
                Some(Remember::Session) => list.push(choice(
                    "Allow for this session",
                    "The same request no longer asks until the chat restarts".into(),
                    Action::AllowForChat,
                )),
                None => {}
            }
            list.push(choice(
                "Deny",
                "Or write what to do instead and press Enter".into(),
                Action::Deny,
            ));
            list
        }
        PermissionKind::Plan => vec![
            choice(
                "Approve",
                "Carry out the plan, asking before changes".into(),
                Action::Approve {
                    accept_edits: false,
                },
            ),
            choice(
                "Approve and accept edits",
                "Carry out the plan; file edits don't ask".into(),
                Action::Approve { accept_edits: true },
            ),
            choice(
                "Keep planning",
                "Or write what to change and press Enter".into(),
                Action::KeepPlanning,
            ),
        ],
        PermissionKind::Question => request
            .questions
            .get(request.step)
            .map(|question| {
                question
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, (label, description))| Choice {
                        label: label.clone(),
                        description: description.clone(),
                        action: Action::Option(index),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// One answer per question: picked labels joined with ", ", then the typed text (for a
/// single choice the typed text replaces the pick).
fn answers(request: &Request) -> Vec<String> {
    request
        .questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let typed = request.typed[index].trim();
            let picked: Vec<&str> = request.chosen[index]
                .iter()
                .filter_map(|&option| question.options.get(option).map(|(l, _)| l.as_str()))
                .collect();
            if question.multi {
                picked
                    .into_iter()
                    .chain((!typed.is_empty()).then_some(typed))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else if !typed.is_empty() {
                typed.to_owned()
            } else {
                picked.join(", ")
            }
        })
        .collect()
}

/// The ACP agent's own rejection: for this time, else for good; `None` withdraws
/// the request.
fn reject_option(offered: &[PermissionOption]) -> Option<&PermissionOption> {
    ["reject_once", "reject_always"]
        .iter()
        .find_map(|kind| offered.iter().find(|option| option.kind == *kind))
}

fn placeholder(kind: PermissionKind, provider: Option<Provider>) -> &'static str {
    let Some(provider) = provider else {
        return "Choose an option, or write a reply to reject";
    };
    match (kind, provider) {
        (PermissionKind::Tool, Provider::Claude) => "Tell Claude what to do instead",
        (PermissionKind::Tool, Provider::Codex) => "Tell Codex what to do instead",
        (PermissionKind::Question, _) => "Type your own answer",
        (PermissionKind::Plan, Provider::Claude) => "Tell Claude what to change in the plan",
        (PermissionKind::Plan, Provider::Codex) => "Tell Codex what to change in the plan",
    }
}

impl ChatView {
    /// Follows the session's oldest waiting request. Returns a file whose existence
    /// decides how a proposed Write is described.
    pub(super) fn sync_decision(&mut self, policy: MotionPolicy, cx: &mut Context<Self>) {
        let now = Instant::now();
        let runtime = self.rt(cx);
        let Some(transcript) = runtime.transcript() else {
            return;
        };
        let pending = runtime.pending_decisions();
        let first = pending.first().copied();
        let current = self.decision.active().map(|request| request.item);
        self.decision.position = (usize::from(first.is_some()), pending.len());
        if first == current {
            return;
        }
        let Some(item) = first else {
            // Nothing waits any more: the panel leaves and the draft comes back.
            self.decision.leaving = true;
            if let Some(presence) = &mut self.decision.presence {
                presence.set_open(false, now, policy);
            }
            // The draft stays until the panel is gone: a request arriving meanwhile
            // must not mistake the last answer for it.
            self.decision.composer = Some(ComposerChange {
                text: Some(self.decision.draft.clone().unwrap_or_default().into()),
                placeholder: COMPOSER_PLACEHOLDER,
            });
            return;
        };
        let offered = runtime.decision_options(item);
        let Some(permission) = transcript.items().iter().find_map(|i| match &i.body {
            Body::Permission(p) if i.id == item => Some(p.clone()),
            _ => None,
        }) else {
            return;
        };
        if self.decision.request.is_none() && self.decision.draft.is_none() {
            let draft = self.composer.read(cx).value().to_string();
            self.decision.draft = Some(draft);
        }
        let questions = presentation::questions(&permission);
        let plan = (permission.kind == PermissionKind::Plan).then(|| {
            let text: Arc<str> = permission.input["plan"].as_str().unwrap_or_default().into();
            let cwd = self.rows.cwd.clone();
            cx.new(|cx| {
                let mut view = MarkdownView::document(cx);
                view.stream(text, cx);
                view.finish(cwd, cx);
                view
            })
        });
        let proposal = (permission.kind == PermissionKind::Tool)
            .then(|| tools::proposal(&permission.tool, &permission.input));
        let check = match &proposal {
            Some(Proposal::Change(change)) if change.created => Some(change.path.clone()),
            _ => None,
        };
        let asker = self.asker(&permission, cx);
        self.decision.composer = Some(ComposerChange {
            text: Some(String::new().into()),
            placeholder: placeholder(permission.kind, self.provider(cx)),
        });
        self.decision.request = Some(Request {
            item,
            asker,
            chosen: vec![BTreeSet::new(); questions.len()],
            typed: vec![String::new(); questions.len()],
            questions,
            step: 0,
            plan,
            proposal,
            replaces: None,
            shown: now,
            permission,
            offered,
        });
        if self.decision.collapsed() {
            self.decision.toggle_fold(cx);
        }
        self.decision.swap_in(now, policy);
        if self.decision.leaving || self.decision.presence.is_none() {
            let mut presence = Presence::new(false, SPEC, now);
            presence.set_open(true, now, policy);
            self.decision.presence = Some(presence);
        }
        self.decision.leaving = false;
        if let Some(path) = check {
            self.check_replaces(item, path, cx);
        }
    }

    fn check_replaces(&mut self, item: ItemId, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let exists = cx
                .background_executor()
                .spawn(async move { std::fs::symlink_metadata(&path).is_ok() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(request) = this.decision.active_mut()
                    && request.item == item
                {
                    request.replaces = Some(exists);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The subagent that asks, when it is not the main agent.
    fn asker(&self, permission: &Permission, cx: &App) -> Option<String> {
        let call = self.rows.permission_thread(permission)?;
        let index = self.rows.calls.get(call)?.index;
        match &self.transcript(cx)?.get(index)?.body {
            Body::Tool(tool) => Some(presentation::subagent_title(tool)),
            _ => None,
        }
    }

    /// Applies a pending composer change; needs the window, so it runs after render.
    pub(super) fn apply_composer_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(change) = self.decision.composer.take() else {
            return;
        };
        cx.defer_in(window, move |this, window, cx| {
            if let Some(text) = change.text {
                this.set_composer_content(text, window, cx);
            }
            this.composer.update(cx, |state, cx| {
                state.set_placeholder(change.placeholder, window, cx);
            });
        });
    }

    fn respond(&mut self, item: ItemId, response: Response, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.respond(item, response, cx));
    }

    fn typed(&self, cx: &App) -> Option<String> {
        let text = self.composer.read(cx).value().trim().to_owned();
        (!text.is_empty()).then_some(text)
    }

    /// Option `index` of the request: clicked, or its number pressed.
    /// Option `index` of request `item` (a click or key meant for another request, or
    /// arriving right after this one appeared, is ignored).
    pub(super) fn choose(
        &mut self,
        item: ItemId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self
            .decision
            .active()
            .filter(|r| r.item == item && r.armed())
        else {
            return;
        };
        let Some(action) = choices(request).get(index).map(|c| c.action.clone()) else {
            return;
        };
        let note = self.typed(cx);
        match action {
            Action::Allow => self.respond(item, Response::Allow, cx),
            Action::AllowForChat => self.respond(item, Response::AllowForChat, cx),
            Action::Deny => self.respond(item, Response::Deny(note), cx),
            Action::Approve { accept_edits } => {
                self.respond(item, Response::ApprovePlan { accept_edits }, cx)
            }
            Action::KeepPlanning => self.respond(item, Response::KeepPlanning(note), cx),
            Action::Offered(option) => self.runtime.update(cx, |runtime, cx| {
                runtime.respond_option(item, Some(option), cx)
            }),
            Action::Option(option) => {
                let Some(request) = self.decision.active_mut() else {
                    return;
                };
                let step = request.step;
                let multi = request.questions.get(step).is_some_and(|q| q.multi);
                let chosen = &mut request.chosen[step];
                if multi {
                    if !chosen.remove(&option) {
                        chosen.insert(option);
                    }
                    cx.notify();
                } else {
                    *chosen = BTreeSet::from([option]);
                    // A picked option replaces a typed answer.
                    request.typed[step].clear();
                    self.composer
                        .update(cx, |state, cx| state.set_value("", window, cx));
                    self.decision_next(window, cx);
                }
            }
        }
    }

    /// Next question, or Submit on the last one.
    pub(super) fn decision_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.composer.read(cx).value().trim().to_owned();
        let Some(request) = self.decision.active_mut() else {
            return;
        };
        if request.permission.kind != PermissionKind::Question {
            return;
        }
        let step = request.step;
        if let Some(slot) = request.typed.get_mut(step) {
            *slot = typed;
        }
        let answered = |request: &Request, step: usize| {
            !request.typed[step].is_empty() || !request.chosen[step].is_empty()
        };
        if request.questions.is_empty() || !answered(request, step) {
            return;
        }
        if step + 1 < request.questions.len() {
            request.step += 1;
            // The next question gets the same guard as a new request.
            request.shown = Instant::now();
            self.decision.swap_in(Instant::now(), motion::policy(cx));
            let Some(request) = self.decision.active_mut() else {
                return;
            };
            let text = request.typed[request.step].clone();
            self.composer
                .update(cx, |state, cx| state.set_value(text, window, cx));
            cx.notify();
            return;
        }
        let answers = answers(request);
        let item = request.item;
        self.respond(item, Response::Answer(answers), cx);
    }

    fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |state, cx| state.focus(window, cx));
    }

    pub(super) fn decision_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.composer.read(cx).value().trim().to_owned();
        let Some(request) = self.decision.active_mut() else {
            return;
        };
        if request.step == 0 {
            return;
        }
        request.typed[request.step] = typed;
        request.step -= 1;
        request.shown = Instant::now();
        let text = request.typed[request.step].clone();
        self.decision.swap_in(Instant::now(), motion::policy(cx));
        self.composer
            .update(cx, |state, cx| state.set_value(text, window, cx));
        cx.notify();
    }

    /// Enter in the composer while a request waits.
    pub(super) fn decision_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.decision.active() else {
            return;
        };
        let item = request.item;
        match request.permission.kind {
            PermissionKind::Question => self.decision_next(window, cx),
            // An ACP agent takes only its own options, no note: typed text rejects, then
            // goes as the next message.
            PermissionKind::Tool if request.offered.is_some() => {
                if self.typed(cx).is_some() {
                    self.reject_and_send(item, window, cx);
                }
            }
            PermissionKind::Tool => {
                if let Some(note) = self.typed(cx) {
                    self.respond(item, Response::Deny(Some(note)), cx);
                }
            }
            PermissionKind::Plan => {
                if let Some(note) = self.typed(cx) {
                    self.respond(item, Response::KeepPlanning(Some(note)), cx);
                }
            }
        }
    }

    /// An ACP agent's request answered with text: the agent's rejection (it has no
    /// room for a note), then the text as an ordinary message, queued while the agent
    /// works.
    fn reject_and_send(&mut self, item: ItemId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self
            .decision
            .active()
            .filter(|r| r.item == item && r.armed())
        else {
            return;
        };
        // Nothing is rejected when the text couldn't go after it.
        if self.attachments_blocked(cx) || !self.rt(cx).accepts_messages() {
            return;
        }
        let reject = request
            .offered
            .as_deref()
            .and_then(reject_option)
            .map(|option| option.option_id.clone());
        self.runtime
            .update(cx, |runtime, cx| runtime.respond_option(item, reject, cx));
        self.submit(window, cx);
    }

    /// A folded panel opens first, so nothing is answered unseen.
    fn unfold(&mut self, cx: &mut Context<Self>) -> bool {
        if self.decision.active().is_some() && self.decision.collapsed() {
            self.decision.toggle_fold(cx);
            cx.notify();
            return true;
        }
        false
    }

    /// ⌘⏎: the first option (Allow, Approve), or the next question.
    pub(super) fn decision_primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.unfold(cx) {
            return;
        }
        match self.decision.active().map(|r| r.permission.kind) {
            Some(PermissionKind::Question) => self.decision_next(window, cx),
            // A typed note means the user is answering otherwise: Enter sends it.
            Some(_) if self.typed(cx).is_some() => {}
            Some(_) => {
                let item = self.decision.active().map(|r| r.item).expect("request");
                self.choose(item, 0, window, cx)
            }
            None => {}
        }
    }

    /// ⌘⌫ / Esc: deny, skip the questions, or keep planning.
    pub(super) fn decision_dismiss(&mut self, cx: &mut Context<Self>) {
        if self.unfold(cx) {
            return;
        }
        let Some(request) = self.decision.active() else {
            return;
        };
        let item = request.item;
        if let Some(offered) = &request.offered {
            let reject = reject_option(offered).map(|option| option.option_id.clone());
            self.runtime
                .update(cx, |runtime, cx| runtime.respond_option(item, reject, cx));
            return;
        }
        let note = self.typed(cx);
        let response = match request.permission.kind {
            PermissionKind::Plan => Response::KeepPlanning(note),
            PermissionKind::Question => Response::Deny(None),
            PermissionKind::Tool => Response::Deny(note),
        };
        self.respond(item, response, cx);
    }

    /// Digits 1–9 pick options while the message field is empty.
    pub(super) fn decision_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.decision.active().is_none() || self.decision.collapsed() {
            return false;
        }
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        // In a non-empty field ⌘⌫ keeps deleting text, and digits are typed.
        if !self.composer.read(cx).value().is_empty() {
            return false;
        }
        if modifiers.platform && keystroke.key == "backspace" {
            self.decision_dismiss(cx);
            return true;
        }
        if modifiers.modified() {
            return false;
        }
        let Some((item, options)) = self
            .decision
            .active()
            .map(|request| (request.item, choices(request).len()))
        else {
            return false;
        };
        // Without such an option the digit is typed as usual.
        let Some(digit) = keystroke
            .key
            .parse::<usize>()
            .ok()
            .filter(|d| (1..=9).contains(d) && *d <= options)
        else {
            return false;
        };
        self.choose(item, digit - 1, window, cx);
        true
    }

    pub(super) fn decision_bar(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let now = Instant::now();
        self.decision.prune(now);
        let request = self.decision.request.as_ref()?;
        let progress = self
            .decision
            .presence
            .as_ref()
            .map_or(1., |p| p.progress(now));
        let active = !self.decision.leaving;
        let collapsed = self.decision.collapsed();
        let kind = request.permission.kind;
        let key = request.item.0;
        let (label, title) = match kind {
            PermissionKind::Tool => (
                "Permission".to_owned(),
                request
                    .permission
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Allow {}?", request.permission.tool)),
            ),
            PermissionKind::Plan => (
                "Plan".to_owned(),
                "Ready to carry out this plan?".to_owned(),
            ),
            PermissionKind::Question => {
                let question = request.questions.get(request.step);
                (
                    match question.map(|q| q.header.as_str()) {
                        Some(header) if !header.is_empty() => format!("Question · {header}"),
                        _ => "Question".to_owned(),
                    },
                    question.map_or_else(
                        || format!("{} has a question", self.agent_name(cx)),
                        |q| q.question.clone(),
                    ),
                )
            }
        };
        let questions = (kind == PermissionKind::Question && request.questions.len() > 1)
            .then(|| format!("{}/{}", request.step + 1, request.questions.len()));
        let waiting = (self.decision.position.1 > 1)
            .then(|| format!("+{} waiting", self.decision.position.1 - 1));
        let counter = match (questions, waiting) {
            (Some(questions), Some(waiting)) => Some(format!("{questions} · {waiting}")),
            (questions, waiting) => questions.or(waiting),
        };
        let header = row()
            .w_full()
            .gap(px(8.))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child(label),
            )
            .children(request.asker.clone().map(|asker| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(t::faint())
                    .child(format!("Subagent · {asker}"))
            }))
            .child(div().flex_1())
            .children(counter.map(|counter| {
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child(counter)
            }))
            .children((kind == PermissionKind::Question).then(|| {
                text_action(("chat-decision-skip", key), "Skip")
                    .tooltip(|window, cx| Tooltip::new("Skip all questions (⌘⌫)").build(window, cx))
                    .when(active, |skip| {
                        skip.on_click(cx.listener(|this, _, _, cx| this.decision_dismiss(cx)))
                    })
            }))
            .child(
                div()
                    .id(("chat-decision-fold", key))
                    .keyboard(if collapsed {
                        "Expand the request"
                    } else {
                        "Collapse the request"
                    })
                    .flex_shrink_0()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|style| style.bg(t::hover()))
                    .child(
                        icon(if collapsed {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .size(px(13.))
                        .text_color(t::muted()),
                    )
                    .tooltip(move |window, cx| {
                        Tooltip::new(if collapsed { "Expand" } else { "Collapse" })
                            .build(window, cx)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.decision.toggle_fold(cx);
                        this.focus_composer(window, cx);
                        cx.notify();
                    })),
            );
        let title = div()
            .w_full()
            .text_size(px(14.))
            .line_height(px(20.))
            .text_color(t::text())
            .child(title);
        let description = (kind == PermissionKind::Tool)
            .then(|| {
                request
                    .permission
                    .description
                    .clone()
                    .filter(|d| Some(d) != request.permission.title.as_ref())
            })
            .flatten()
            .map(|description| {
                div()
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child(description)
            });
        // A new question or request swaps in place (text swap: quick, small rise).
        let swap = self.decision.swap.value(now);
        let content = column()
            .gap(px(6.))
            .pt(px(6.))
            .relative()
            .top(px(motion::distance::MICRO * (1. - swap)))
            .opacity(swap)
            .child(title)
            .children(description)
            .child(
                column()
                    .id(("chat-decision-body", key))
                    .max_h(px(PANEL_HEIGHT))
                    .overflow_y_scroll()
                    .gap(px(6.))
                    .children(self.preview(request, key, cx))
                    .child(self.options(request, active, cx))
                    .nested_scroll(),
            );
        let panel = column()
            .id("chat-decision")
            .w_full()
            .px(px(14.))
            .pt(px(10.))
            .pb(px(8.))
            .rounded_t(px(12.))
            .border_1()
            .border_b_0()
            .border_color(t::border())
            // The composer's surface: the panel reads as part of the message field.
            .bg(t::composer_surface())
            .child(header)
            // The request's body lies over the conversation; its header keeps room.
            .child(super::render::unfolds_over(
                &self.overlay_unfolded,
                px(0.),
                self.decision.fold.measured_body(
                    ("chat-decision-fold-body", key),
                    // Another question or preview size eases the panel's height.
                    AnimatedHeight::new(
                        ("chat-decision-height", key),
                        motion::policy(cx) == MotionPolicy::Full,
                        content,
                    ),
                    now,
                ),
            ));
        // Panel reveal: the height opens with a short rise and fade, and closes the same
        // way, a little quicker (PANEL).
        let reveal = gpui_kit::base::MotionReveal::new(
            ("chat-decision-reveal", key),
            progress,
            div()
                .w_full()
                .relative()
                .top(px(motion::distance::BASE * (1. - progress)))
                .opacity(progress)
                .child(panel)
                .into_any_element(),
        );
        Some(centered(div().mx(px(14.)).child(reveal)).mb(px(-4.)))
    }

    fn preview(&self, request: &Request, key: u64, cx: &App) -> Option<AnyElement> {
        if let Some(plan) = &request.plan {
            return Some(
                div()
                    .id(("chat-decision-plan", key))
                    .max_h(px(PLAN_HEIGHT))
                    .overflow_y_scroll()
                    .px(px(18.))
                    .py(px(14.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(t::control_border())
                    .bg(t::elevated())
                    .text_color(t::text())
                    .child(plan.clone())
                    .nested_scroll()
                    .into_any_element(),
            );
        }
        let cwd = self.pane(cx).metadata.cwd.clone().unwrap_or_default();
        Some(match request.proposal.as_ref()? {
            Proposal::Command { command, .. } => div()
                .id(("chat-decision-command", key))
                .max_h(px(PREVIEW_HEIGHT))
                .overflow_y_scroll()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .bg(t::elevated())
                .font_family(t::MONO)
                .text_size(px(12.))
                .text_color(t::text())
                .child(command.clone())
                .nested_scroll()
                .into_any_element(),
            Proposal::Input(input) => div()
                .id(("chat-decision-input", key))
                .max_h(px(PREVIEW_HEIGHT))
                .overflow_y_scroll()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .bg(t::elevated())
                .font_family(t::MONO)
                .text_size(px(12.))
                .text_color(t::secondary())
                .child(input.clone())
                .nested_scroll()
                .into_any_element(),
            Proposal::Change(change) => {
                let note = if change.created {
                    match request.replaces {
                        Some(true) => "Replaces the existing file",
                        Some(false) => "New file",
                        None => "",
                    }
                } else {
                    ""
                };
                let numbered = change.lines.iter().any(|l| l.new.or(l.old).is_some());
                let gutter = if numbered {
                    let digits = change.lines.len().max(1).to_string().len() as f32;
                    18. + 8. * digits
                } else {
                    12.
                };
                column()
                    .gap(px(4.))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(crate::ui::markdown::file_pill::pill(
                                ("chat-decision-file", key),
                                &cwd,
                                &canopy_desktop::chat::mentions::Target {
                                    path: change.path.clone(),
                                    line: None,
                                },
                                px(12.),
                                px(20.),
                            ))
                            .child(div().text_size(px(12.)).text_color(t::muted()).child(note)),
                    )
                    .child(
                        div()
                            .id(("chat-decision-diff", key))
                            .max_h(px(PREVIEW_HEIGHT))
                            .overflow_y_scroll()
                            .rounded(px(8.))
                            .bg(t::elevated())
                            .py(px(4.))
                            .children(
                                change
                                    .lines
                                    .iter()
                                    .take(PREVIEW_LINES)
                                    .map(|line| diff_line(line, gutter)),
                            )
                            .children((change.lines.len() > PREVIEW_LINES).then(|| {
                                div()
                                    .px(px(10.))
                                    .py(px(4.))
                                    .text_size(px(12.))
                                    .text_color(t::muted())
                                    .child(format!(
                                        "{} more lines not shown",
                                        change.lines.len() - PREVIEW_LINES
                                    ))
                            }))
                            .nested_scroll(),
                    )
                    .into_any_element()
            }
        })
    }

    fn options(&self, request: &Request, active: bool, cx: &mut Context<Self>) -> Div {
        let key = request.item.0;
        let item = request.item;
        if choices(request).is_empty() {
            return column().child(
                div()
                    .px(px(10.))
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("Type your answer in the message field"),
            );
        }
        let chosen = request.chosen.get(request.step);
        let multi = request.permission.kind == PermissionKind::Question
            && request.questions.get(request.step).is_some_and(|q| q.multi);
        // With text typed for an ACP agent, Enter picks its rejection and sends the text.
        let enter = request
            .offered
            .as_deref()
            .and_then(reject_option)
            .filter(|_| self.typed(cx).is_some())
            .map(|option| option.option_id.clone());
        column().gap(px(2.)).children(
            choices(request)
                .into_iter()
                .enumerate()
                .map(|(index, mut choice)| {
                    let rejects = matches!(&choice.action, Action::Offered(id) if Some(id) == enter.as_ref());
                    if rejects {
                        choice.description = if choice.description.is_empty() {
                            "Enter sends your text after it".into()
                        } else {
                            format!("{} · Enter sends your text after it", choice.description)
                        };
                    }
                    let selected = rejects
                        || matches!(choice.action, Action::Option(o) if chosen.is_some_and(|c| c.contains(&o)));
                    row()
                        .id(ElementId::NamedInteger(
                            format!("chat-decision-option-{key}").into(),
                            index as u64,
                        ))
                        .w_full()
                        .gap(px(10.))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .when(selected, |row| row.bg(t::hover()))
                        .when(active, |row| {
                            row.keyboard(format!("Option {}: {}", index + 1, choice.label))
                                .cursor_pointer()
                                .hover(|style| style.bg(t::hover()))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.choose(item, index, window, cx);
                                    this.focus_composer(window, cx);
                                }))
                        })
                        .children(multi.then(|| checkbox(selected)))
                        .child(
                            column()
                                .flex_1()
                                .min_w_0()
                                .gap(px(1.))
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .text_color(t::text())
                                        .child(choice.label),
                                )
                                .children((!choice.description.is_empty()).then(|| {
                                    div()
                                        .text_size(px(12.))
                                        .text_color(t::muted())
                                        .child(choice.description)
                                })),
                        )
                        .children((index < 9).then(|| {
                            div()
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .text_color(t::faint())
                                .child((index + 1).to_string())
                        }))
                }),
        )
    }
}

pub(super) fn text_action(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .keyboard(label)
        .flex_shrink_0()
        .px(px(6.))
        .h(px(20.))
        .flex()
        .items_center()
        .rounded(px(5.))
        .cursor_pointer()
        .text_size(px(12.))
        .text_color(t::secondary())
        .hover(|style| style.bg(t::hover()).text_color(t::text()))
        .child(label)
}

/// Marks a picked option of a question that allows several.
pub(super) fn checkbox(checked: bool) -> Div {
    div()
        .flex_shrink_0()
        .size(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .border_1()
        .border_color(if checked { t::accent() } else { t::muted() })
        .when(checked, |mark| {
            mark.bg(t::accent())
                .child(icon(IconName::Check).size(px(10.)).text_color(t::bg()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_desktop::chat::transcript::{Decision, Rule};
    use core::prelude::v1::test;
    use serde_json::json;

    fn request(
        kind: PermissionKind,
        remember: Option<Remember>,
        input: serde_json::Value,
    ) -> Request {
        let permission = Permission {
            request_id: "r".into(),
            call_id: None,
            tool: "Bash".into(),
            input,
            title: None,
            description: None,
            kind,
            decision: Decision::Pending,
            remember,
            resolution: None,
        };
        let questions = presentation::questions(&permission);
        Request {
            item: ItemId(1),
            asker: None,
            chosen: vec![BTreeSet::new(); questions.len()],
            typed: vec![String::new(); questions.len()],
            questions,
            step: 0,
            plan: None,
            proposal: None,
            replaces: None,
            shown: Instant::now(),
            offered: None,
            permission,
        }
    }

    #[test]
    fn several_picks_keep_a_typed_addition() {
        let mut request = request(
            PermissionKind::Question,
            None,
            json!({"questions": [
                {"question": "A?", "options": [{"label": "x"}, {"label": "y"}]},
                {"question": "B?", "options": [{"label": "z"}, {"label": "w"}], "multiSelect": true}
            ]}),
        );
        request.chosen[0].insert(1);
        request.typed[0] = "own".into();
        request.chosen[1].extend([0, 1]);
        request.typed[1] = "also v".into();
        assert_eq!(answers(&request), ["own", "z, w, also v"]);
        request.typed[0].clear();
        assert_eq!(answers(&request)[0], "y");
        assert!(!request.armed(), "a request just shown ignores input");
    }

    fn labels(request: &Request) -> Vec<String> {
        choices(request).into_iter().map(|c| c.label).collect()
    }

    #[test]
    fn options_follow_the_kind_of_request() {
        let rule = Remember::Rules(vec![Rule {
            tool: "Bash".into(),
            content: Some("npm test *".into()),
        }]);
        let tool = request(PermissionKind::Tool, Some(rule), json!({}));
        assert_eq!(
            labels(&tool),
            ["Allow", "Always allow in this chat", "Deny"]
        );
        assert_eq!(choices(&tool)[1].description, "Bash(npm test *)");
        let plain = request(PermissionKind::Tool, None, json!({}));
        assert_eq!(labels(&plain), ["Allow", "Deny"], "nothing to remember");
        let edits = request(PermissionKind::Tool, Some(Remember::AcceptEdits), json!({}));
        assert_eq!(labels(&edits)[1], "Accept edits in this chat");
        let plan = request(PermissionKind::Plan, None, json!({"plan": "# P"}));
        assert_eq!(
            labels(&plan),
            ["Approve", "Approve and accept edits", "Keep planning"]
        );
        let mut questions = request(
            PermissionKind::Question,
            None,
            json!({"questions": [
                {"question": "A?", "options": [{"label": "x"}, {"label": "y"}]},
                {"question": "B?", "options": [{"label": "z"}], "multiSelect": true}
            ]}),
        );
        assert_eq!(labels(&questions), ["x", "y"]);
        questions.step = 1;
        assert_eq!(labels(&questions), ["z"]);
    }

    #[test]
    fn typed_text_rejects_with_the_agents_own_option_first() {
        let option =
            |id: &str, kind: &str| PermissionOption::new(id.into(), id.into(), kind.into());
        let offered = vec![
            option("always", "reject_always"),
            option("allow", "allow_once"),
            option("once", "reject_once"),
        ];
        let id = |offered: &[PermissionOption]| reject_option(offered).map(|o| o.option_id.clone());
        assert_eq!(
            id(&offered).as_deref(),
            Some("once"),
            "this time before for good"
        );
        assert_eq!(id(&offered[..2]).as_deref(), Some("always"));
        assert_eq!(
            id(&offered[1..2]),
            None,
            "without a rejection the request is withdrawn"
        );
    }
}
