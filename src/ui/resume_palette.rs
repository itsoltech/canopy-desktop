//! `/resume` (DEC-094): closed chats of this project, latest first, like "New chat in".
//! Tab shows every project's; Enter opens one again, the bin deletes it for good.
use super::components::files::{SEARCH_ROW_HEIGHT, SEARCH_VIEWPORT_HEIGHT, file_message};
use super::{components::modal::*, components::*, theme as t};
use crate::app_state::AppState;
use canopy_desktop::chat::closed::ClosedChat;
use canopy_desktop::motion::{self, Presence, presets};
use canopy_desktop::state::workspace::PaneId;
use gpui_kit::{
    component::{
        IconName,
        input::{InputEvent, InputState},
    },
    *,
};
use std::path::PathBuf;
use std::time::Instant;

pub struct ResumePalette {
    input: Entity<InputState>,
    /// Closed chats and when they closed; `None` while they load.
    chats: Option<Vec<(ClosedChat, i64)>>,
    error: Option<String>,
    /// The project the list starts with (`None`: "No project").
    here: Option<PathBuf>,
    everywhere: bool,
    visible: Vec<usize>,
    selected: usize,
    /// The empty chat `/resume` was typed in.
    replace: Option<PaneId>,
    scroll: ScrollHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    _load: Task<()>,
    _events: Vec<Subscription>,
}
impl EventEmitter<ModalDismissed> for ResumePalette {}

impl ResumePalette {
    pub fn new(
        replace: Option<PaneId>,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search closed chats…"));
        let events = vec![
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.filter(cx),
                InputEvent::PressEnter { .. } => this.open(this.selected, cx),
                _ => {}
            }),
        ];
        let focus = input.clone();
        window.on_next_frame(move |window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
        let mut presence = Presence::new(false, presets::POPOVER, Instant::now());
        presence.set_open(true, Instant::now(), motion::policy(cx));
        let app = cx.global::<AppState>().clone();
        let here = {
            let catalog = &app.projects.read(cx).catalog;
            catalog.active.and_then(|id| {
                catalog
                    .items
                    .iter()
                    .find(|project| project.workspace == id)
                    .map(|project| {
                        project
                            .worktree_path
                            .clone()
                            .unwrap_or(project.path.clone())
                    })
            })
        };
        let load = app
            .chat_history
            .update(cx, |history, cx| history.closed(cx));
        let load = cx.spawn(async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(chats) => this.chats = Some(chats),
                    Err(error) => {
                        this.chats = Some(vec![]);
                        this.error = Some(error);
                    }
                }
                this.filter(cx);
            });
        });
        Self {
            input,
            chats: None,
            error: None,
            here,
            everywhere: false,
            visible: vec![],
            selected: 0,
            replace,
            scroll: ScrollHandle::new(),
            return_focus,
            presence,
            closing: false,
            scheduled: false,
            _load: load,
            _events: events,
        }
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_string();
        let here = self.here.clone();
        let everywhere = self.everywhere;
        self.visible = self
            .chats
            .iter()
            .flatten()
            .enumerate()
            .filter(|(_, (chat, _))| everywhere || chat.in_project(here.as_deref()))
            .filter(|(_, (chat, _))| chat.matches(&query))
            .map(|(index, _)| index)
            .collect();
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }

    /// `position` indexes the visible list.
    fn open(&mut self, position: usize, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let Some(chat) = self
            .visible
            .get(position)
            .and_then(|&index| self.chats.as_ref()?.get(index))
            .map(|(chat, _)| chat.clone())
        else {
            return;
        };
        self.close(cx);
        let replace = self.replace;
        cx.global::<AppState>()
            .chats
            .clone()
            .update(cx, |chats, cx| chats.reopen(chat, replace, cx));
    }

    fn forget(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(chats) = self.chats.as_mut() else {
            return;
        };
        if index >= chats.len() {
            return;
        }
        let (chat, _) = chats.remove(index);
        cx.global::<AppState>()
            .chats
            .clone()
            .update(cx, |chats, cx| chats.forget(&chat, cx));
        self.filter(cx);
    }

    fn keystroke(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => self.close(cx),
            "down" => {
                self.selected = (self.selected + 1).min(self.visible.len().saturating_sub(1));
                cx.notify();
            }
            "up" => {
                self.selected = self.selected.saturating_sub(1);
                cx.notify();
            }
            "tab" => {
                self.everywhere = !self.everywhere;
                self.selected = 0;
                self.filter(cx);
            }
            _ => return false,
        }
        true
    }
}

impl Render for ResumePalette {
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
        let empty = match (&self.chats, &self.error) {
            (None, _) => Some("Loading closed chats…".to_owned()),
            (_, Some(error)) => Some(error.clone()),
            _ if self.visible.is_empty() => Some(if self.everywhere {
                "No closed chats".to_owned()
            } else {
                "No closed chats here — Tab shows all projects".to_owned()
            }),
            _ => None,
        };
        let rows: Vec<AnyElement> = self
            .visible
            .iter()
            .enumerate()
            .filter_map(|(position, &index)| {
                let (chat, closed_at) = self.chats.as_ref()?.get(index)?;
                Some(
                    closed_row(
                        chat,
                        *closed_at,
                        position == self.selected,
                        index,
                        self.everywhere,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open(position, cx)))
                    .child(
                        icon_button(("resume-forget", index), IconName::Delete, "Delete chat")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.forget(index, cx);
                            })),
                    )
                    .into_any_element(),
                )
            })
            .collect();
        div()
            .absolute()
            .inset_0()
            .child(modal_backdrop(progress))
            .child(
                modal_surface("resume-palette")
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
                            .child(if self.everywhere {
                                "Resume a closed chat · all projects"
                            } else {
                                "Resume a closed chat · this project"
                            }),
                    )
                    .child(
                        column()
                            .id("resume-choices")
                            .max_h(px(SEARCH_VIEWPORT_HEIGHT))
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(
                                empty.map(|message| file_message(message, t::muted()).p(px(12.))),
                            )
                            .children(rows),
                    )
                    .child(
                        row()
                            .gap(px(12.))
                            .px(px(8.))
                            .pt(px(6.))
                            .text_size(px(11.))
                            .text_color(t::faint())
                            .child("↑↓ Navigate")
                            .child("Enter Resume")
                            .child(if self.everywhere {
                                "Tab This project"
                            } else {
                                "Tab All projects"
                            })
                            .child("Esc Close"),
                    ),
            )
    }
}

fn closed_row(
    chat: &ClosedChat,
    closed_at: i64,
    selected: bool,
    index: usize,
    everywhere: bool,
) -> Stateful<Div> {
    let agent = canopy_desktop::chat::provider::Provider::from_tool(&chat.tool)
        .map_or("Agent", |provider| provider.name());
    let place = everywhere.then(|| {
        chat.project
            .as_ref()
            .map_or("No project".to_owned(), |project| project.name.clone())
    });
    let detail = [
        Some(agent.to_owned()),
        place,
        Some(super::components::history::age(closed_at)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    row()
        .id(("resume-choice", index))
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
        .child(icon(IconName::Bot))
        .child(
            column()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(t::text())
                        .truncate()
                        .child(chat.title.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(t::muted())
                        .truncate()
                        .child(detail),
                ),
        )
}
