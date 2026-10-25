//! "New chat in…" palette: choose an open project or "No project", like Cmd+P.
use super::components::files::{SEARCH_ROW_HEIGHT, SEARCH_VIEWPORT_HEIGHT, file_message};
use super::{components::modal::*, components::*, theme as t};
use crate::app_state::{AppState, ChatTarget, ForkRequest};
use canopy_desktop::motion::{self, Presence, presets};
use gpui_kit::{
    component::{
        IconName,
        input::{InputEvent, InputState},
    },
    *,
};
use std::time::Instant;

/// One destination; `shortcut` is fixed by the unfiltered order.
struct Choice {
    target: ChatTarget,
    name: String,
    path: Option<String>,
    shortcut: Option<&'static str>,
    remote: Option<canopy_desktop::remote::proto::WorkspaceRef>,
}

/// What the chosen project gets: a new chat, or a fork of a conversation.
pub enum Purpose {
    NewChat,
    Fork(Box<ForkRequest>),
    RemoteFork(super::chat::ClientForkRequested),
}

pub struct NewChatPalette {
    purpose: Purpose,
    input: Entity<InputState>,
    choices: Vec<Choice>,
    visible: Vec<usize>,
    selected: usize,
    scroll: ScrollHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    _events: Vec<Subscription>,
}
impl EventEmitter<ModalDismissed> for NewChatPalette {}

const PROJECT_SHORTCUTS: [&str; 9] = ["⌘1", "⌘2", "⌘3", "⌘4", "⌘5", "⌘6", "⌘7", "⌘8", "⌘9"];

impl NewChatPalette {
    pub fn new(
        purpose: Purpose,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects…"));
        let events = vec![
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.filter(cx),
                InputEvent::PressEnter { .. } => this.choose(this.selected, cx),
                _ => {}
            }),
        ];
        let focus = input.clone();
        window.on_next_frame(move |window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
        let mut presence = Presence::new(false, presets::POPOVER, Instant::now());
        presence.set_open(true, Instant::now(), motion::policy(cx));
        let projects = &cx.global::<AppState>().projects.read(cx).catalog;
        let active = projects.active;
        let mut choices = vec![Choice {
            target: ChatTarget::NoProject,
            name: "Without a project".into(),
            path: None,
            shortcut: Some("⌥⌘N"),
            remote: None,
        }];
        choices.extend(projects.items.iter().enumerate().map(|(index, project)| {
            Choice {
                target: ChatTarget::Project(project.workspace),
                name: project.name.clone(),
                path: Some(
                    project
                        .worktree_path
                        .as_ref()
                        .unwrap_or(&project.path)
                        .display()
                        .to_string(),
                ),
                shortcut: PROJECT_SHORTCUTS.get(index).copied(),
                remote: None,
            }
        }));
        if let Purpose::RemoteFork(request) = &purpose {
            choices.truncate(1);
            choices[0].name = "Without a project".into();
            if let Some(host) = request.runtime.read(cx).remote_host()
                && let Some(catalog) = host.shared_catalog()
            {
                for project in &catalog.projects {
                    for checkout in &project.checkouts {
                        if checkout.closing == Some(true) {
                            continue;
                        }
                        choices.push(Choice {
                            target: ChatTarget::NoProject,
                            name: format!("{} · {}", project.name, checkout.name),
                            path: Some(checkout.path.clone()),
                            shortcut: PROJECT_SHORTCUTS.get(choices.len() - 1).copied(),
                            remote: checkout.workspace.clone(),
                        });
                    }
                }
            }
        }
        // Start on the space the user is in: the likeliest destination.
        let selected = match active {
            Some(id) => choices
                .iter()
                .position(|choice| choice.target == ChatTarget::Project(id))
                .unwrap_or(0),
            None => 0,
        };
        Self {
            purpose,
            input,
            visible: (0..choices.len()).collect(),
            choices,
            selected,
            scroll: ScrollHandle::new(),
            return_focus,
            presence,
            closing: false,
            scheduled: false,
            _events: events,
        }
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_lowercase();
        let query = query.trim();
        self.visible = self
            .choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| {
                query.is_empty()
                    || choice.name.to_lowercase().contains(query)
                    || choice
                        .path
                        .as_ref()
                        .is_some_and(|path| path.to_lowercase().contains(query))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }

    /// `position` indexes the visible list.
    fn choose(&mut self, position: usize, cx: &mut Context<Self>) {
        let Some(&index) = self.visible.get(position) else {
            return;
        };
        self.open(index, cx);
    }

    fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let Some(choice) = self.choices.get(index) else {
            return;
        };
        let target = choice.target;
        let remote = choice.remote.clone();
        self.close(cx);
        let chats = cx.global::<AppState>().chats.clone();
        match &self.purpose {
            Purpose::NewChat => chats.update(cx, |chats, cx| chats.open_new(target, cx)),
            Purpose::Fork(request) => {
                let request = (**request).clone();
                chats.update(cx, |chats, cx| chats.fork(target, request, cx))
            }
            Purpose::RemoteFork(request) => request
                .runtime
                .update(cx, |r, cx| r.client_fork(request.item, remote, cx)),
        }
    }

    fn keystroke(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let keystroke = &event.keystroke;
        let command = keystroke.modifiers.platform;
        match keystroke.key.as_str() {
            "escape" => self.close(cx),
            "down" => {
                self.selected = (self.selected + 1).min(self.visible.len().saturating_sub(1));
                cx.notify();
            }
            "up" => {
                self.selected = self.selected.saturating_sub(1);
                cx.notify();
            }
            "n" if command && keystroke.modifiers.alt => self.open(0, cx),
            digit if command && digit.len() == 1 => {
                let Some(number) = digit.chars().next().and_then(|c| c.to_digit(10)) else {
                    return false;
                };
                if number == 0 {
                    return false;
                }
                // ⌘1 is the first project; "No project" is choice 0.
                self.open(number as usize, cx);
            }
            _ => return false,
        }
        true
    }
}

impl Render for NewChatPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animated = self.presence.is_animating(now);
        motion::request_frame(window, animated);
        if self.closing && !animated && !self.scheduled {
            self.scheduled = true;
            let target = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = target.update(cx, |this, cx| {
                    this.return_focus.focus(window, cx);
                    cx.emit(ModalDismissed);
                });
            });
        }
        let width = (f32::from(window.viewport_size().width) - 40.).clamp(0., 560.);
        div()
            .absolute()
            .inset_0()
            .child(modal_backdrop(progress))
            .child(
                modal_surface("new-chat-palette")
                    .absolute()
                    .w(px(width))
                    .left((window.viewport_size().width - px(width)) / 2.)
                    .top(px(72. + 8. * (1. - progress)))
                    .opacity(progress)
                    .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if !this.keystroke(event, cx) {
                            return;
                        }
                        let row = 40.;
                        let offset = -f32::from(this.scroll.offset().y);
                        let top = this.selected as f32 * row;
                        let next = if top < offset {
                            top
                        } else if top + row > offset + SEARCH_VIEWPORT_HEIGHT {
                            top + row - SEARCH_VIEWPORT_HEIGHT
                        } else {
                            offset
                        };
                        this.scroll.set_offset(point(px(0.), px(-next)));
                        cx.stop_propagation();
                    }))
                    .child(input(&self.input).w_full())
                    .child(
                        div()
                            .px(px(8.))
                            .pt(px(4.))
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child(match self.purpose {
                                Purpose::NewChat => "New chat in",
                                Purpose::Fork(_) | Purpose::RemoteFork(_) => "Fork into",
                            }),
                    )
                    .child(
                        column()
                            .id("new-chat-choices")
                            .max_h(px(SEARCH_VIEWPORT_HEIGHT))
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(self.visible.is_empty().then(|| {
                                file_message("No matching projects", t::muted()).p(px(12.))
                            }))
                            .children(self.visible.iter().enumerate().map(|(position, &index)| {
                                let choice = &self.choices[index];
                                choice_row(choice, position == self.selected, index).on_click(
                                    cx.listener(move |this, _, _, cx| this.open(index, cx)),
                                )
                            })),
                    )
                    .child(
                        row()
                            .gap(px(12.))
                            .px(px(8.))
                            .pt(px(6.))
                            .text_size(px(11.))
                            .text_color(t::faint())
                            .child("↑↓ Navigate")
                            .child("Enter Select")
                            .child("Esc Close"),
                    ),
            )
    }
}

fn choice_row(choice: &Choice, selected: bool, index: usize) -> Stateful<Div> {
    row()
        .id(("new-chat-choice", index))
        .min_h(px(SEARCH_ROW_HEIGHT.max(40.)))
        .flex_shrink_0()
        .gap(px(10.))
        .px(px(8.))
        .rounded(px(6.))
        .bg(if selected {
            t::hover()
        } else {
            hsla(0., 0., 0., 0.)
        })
        .cursor_pointer()
        .child(icon(match choice.target {
            ChatTarget::NoProject => IconName::Bot,
            ChatTarget::Project(_) => IconName::Folder,
        }))
        .child(
            column()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t::text())
                        .truncate()
                        .child(choice.name.clone()),
                )
                .children(choice.path.clone().map(|path| {
                    div()
                        .text_size(px(11.))
                        .text_color(t::muted())
                        .truncate()
                        .child(path)
                })),
        )
        .children(choice.shortcut.map(|shortcut| {
            div()
                .flex_shrink_0()
                .text_size(px(11.))
                .text_color(t::faint())
                .child(shortcut)
        }))
}
