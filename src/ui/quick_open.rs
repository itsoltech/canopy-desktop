use super::components::files::{
    SEARCH_ROW_HEIGHT, SEARCH_VIEWPORT_HEIGHT, file_message, file_search_row,
};
use super::{components::modal::*, components::*, theme as t};
use crate::app_state::AppState;
use canopy_desktop::motion::{self, Presence, presets};
use gpui_kit::{
    component::input::{InputEvent, InputState},
    *,
};
use std::path::PathBuf;
use std::time::Instant;
pub struct QuickOpen {
    root: Option<PathBuf>,
    input: Entity<InputState>,
    results: Vec<PathBuf>,
    selected: usize,
    _events: Vec<Subscription>,
    scroll: ScrollHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    search_task: Option<Task<()>>,
    cancel: Option<canopy_desktop::files::CancelGuard>,
    warning: Option<String>,
    searching: bool,
    open_when_ready: bool,
}
impl EventEmitter<ModalDismissed> for QuickOpen {}
impl QuickOpen {
    pub fn new(return_focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search files…"));
        let files = cx.global::<AppState>().files.clone();
        let root = files.read(cx).root.clone();
        let events = vec![
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.search(cx),
                InputEvent::PressEnter { .. } => this.open(cx),
                _ => {}
            }),
            cx.observe(&files, |this, files, cx| {
                if files.read(cx).root != this.root {
                    this.close(cx);
                }
            }),
        ];
        let focus = input.clone();
        window.on_next_frame(move |window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
        let mut presence = Presence::new(false, presets::POPOVER, Instant::now());
        presence.set_open(true, Instant::now(), motion::policy(cx));
        let mut this = Self {
            root,
            input,
            results: vec![],
            selected: 0,
            _events: events,
            scroll: ScrollHandle::new(),
            return_focus,
            presence,
            closing: false,
            scheduled: false,
            search_task: None,
            cancel: None,
            warning: None,
            searching: false,
            open_when_ready: false,
        };
        this.search(cx);
        this
    }
    fn search(&mut self, cx: &mut Context<Self>) {
        let files = cx.global::<AppState>().files.read(cx);
        if files.root != self.root {
            self.close(cx);
            return;
        }
        let query = self.input.read(cx).value().to_lowercase();
        if self.closing {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        self.search_task = None;
        let cancel = canopy_desktop::files::CancelGuard::default();
        let signal = cancel.0.clone();
        self.cancel = Some(cancel);
        self.searching = true;
        self.open_when_ready = false;
        self.results.clear();
        self.warning = None;
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
            let result = cx
                .background_executor()
                .spawn(async move { canopy_desktop::files::search::find(&root, &query, &signal) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.closing {
                    return;
                }
                s.searching = false;
                match result {
                    Ok(result) => {
                        s.results = result.paths;
                        s.warning = result.warning;
                    }
                    Err(e) => s.warning = Some(e),
                }
                s.selected = 0;
                s.scroll.set_offset(point(px(0.), px(0.)));
                if s.open_when_ready {
                    s.open_when_ready = false;
                    s.open(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        self.closing = true;
        self.search_task = None;
        self.cancel = None;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn open(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        if self.searching {
            self.open_when_ready = true;
            return;
        }
        if let Some(path) = self.results.get(self.selected).cloned() {
            let files = cx.global::<AppState>().files.clone();
            if files.read(cx).root == self.root {
                files.update(cx, |files, cx| files.open(&path, cx));
            }
            self.close(cx);
        }
    }
}
impl Render for QuickOpen {
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
        let width = (f32::from(window.viewport_size().width) - 40.).clamp(0., 640.);
        div()
            .absolute()
            .inset_0()
            .child(modal_backdrop(progress))
            .child(
                modal_surface("quick-open")
                    .absolute()
                    .w(px(width))
                    .left((window.viewport_size().width - px(width)) / 2.)
                    .top(px(72. + 8. * (1. - progress)))
                    .opacity(progress)
                    .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        match event.keystroke.key.as_str() {
                            "escape" => this.close(cx),
                            "down" => {
                                this.selected =
                                    (this.selected + 1).min(this.results.len().saturating_sub(1));
                                cx.notify();
                            }
                            "up" => {
                                this.selected = this.selected.saturating_sub(1);
                                cx.notify();
                            }
                            _ => return,
                        }
                        let offset = -f32::from(this.scroll.offset().y);
                        let top = this.selected as f32 * SEARCH_ROW_HEIGHT;
                        let next = if top < offset {
                            top
                        } else if top + SEARCH_ROW_HEIGHT > offset + SEARCH_VIEWPORT_HEIGHT {
                            top + SEARCH_ROW_HEIGHT - SEARCH_VIEWPORT_HEIGHT
                        } else {
                            offset
                        };
                        this.scroll.set_offset(point(px(0.), px(-next)));
                        cx.stop_propagation();
                    }))
                    .child(input(&self.input).w_full())
                    .children(
                        self.warning.clone().map(|warning| {
                            file_message(warning, t::secondary()).text_size(px(11.))
                        }),
                    )
                    .child(
                        column()
                            .id("quick-open-results")
                            .max_h(px(SEARCH_VIEWPORT_HEIGHT))
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(self.results.is_empty().then(|| {
                                file_message(
                                    if self.searching {
                                        "Searching…"
                                    } else {
                                        "No matching files"
                                    },
                                    t::muted(),
                                )
                                .p(px(12.))
                            }))
                            .children(self.results.iter().enumerate().map(|(i, path)| {
                                file_search_row(
                                    SharedString::from(path.to_string_lossy().to_string()),
                                    path.to_string_lossy().to_string(),
                                    i == self.selected,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.selected = i;
                                        this.open(cx);
                                    },
                                ))
                            })),
                    ),
            )
    }
}
