//! Agent-session notch shared by native macOS and Windows overlay adapters.
use super::notch_geometry::{InputRegion, NativeEvent};
#[cfg(target_os = "macos")]
use super::notch_macos::PointerMonitor;
use super::notch_motion::{CONTENT_TRAVEL, Motion, Values};
#[cfg(target_os = "windows")]
use super::notch_windows::PointerMonitor;
use super::{components::*, theme as t};
use gpui_kit::component::Root;
use gpui_kit::*;
use std::time::Instant;

const EXPANDED_WIDTH: f32 = 480.;
const ROW_HEIGHT: f32 = NOTCH_ROW_HEIGHT;
const CONTENT_PADDING: f32 = 6.;
const MAX_ROWS: usize = 8;
const FILTER_HEIGHT: f32 = 32.;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum StatusFilter {
    #[default]
    All,
    Idle,
    Working,
    Waiting,
    Failed,
}
impl StatusFilter {
    const ITEMS: [(Self, &'static str); 5] = [
        (Self::All, "All"),
        (Self::Idle, "Idle"),
        (Self::Working, "Working"),
        (Self::Waiting, "Needs attention"),
        (Self::Failed, "Error"),
    ];
    fn matches(self, status: canopy_desktop::agents::Status) -> bool {
        use canopy_desktop::agents::Status;
        match self {
            Self::All => true,
            Self::Idle => matches!(status, Status::Idle | Status::Exited),
            Self::Working => matches!(status, Status::Starting | Status::Working),
            Self::Waiting => status == Status::Waiting,
            Self::Failed => status == Status::Failed,
        }
    }
}
fn show_session(session: &NotchSession, overview: bool, filter: StatusFilter) -> bool {
    if overview {
        filter.matches(session.state)
    } else {
        session.unseen
    }
}
fn content_height(count: usize, overview: bool) -> f32 {
    ROW_HEIGHT * count.clamp(1, MAX_ROWS) as f32
        + CONTENT_PADDING
        + if overview { FILTER_HEIGHT } else { 0. }
}

#[derive(Clone, Copy)]
struct Geometry {
    header: f32,
    collapsed_width: f32,
    content_height: f32,
}
impl Geometry {
    fn new(menu_height: f32) -> Self {
        let header = if menu_height > 0. {
            menu_height.clamp(24., 48.)
        } else {
            37.
        };
        Self {
            header,
            content_height: ROW_HEIGHT + CONTENT_PADDING,
            collapsed_width: ((header * 5.5).round() + 80.).min(EXPANDED_WIDTH),
        }
    }
    fn region(self, values: Values) -> InputRegion {
        InputRegion {
            canvas_width: EXPANDED_WIDTH,
            width: self.collapsed_width + (EXPANDED_WIDTH - self.collapsed_width) * values.width,
            height: self.header + self.content_height * values.height,
            radius: 16. + 8. * values.height,
        }
    }
    #[cfg(test)]
    fn contains_pointer(self, values: Values, position: Point<Pixels>) -> bool {
        self.region(values).contains(position)
    }
    #[cfg(test)]
    fn expanded_height(self) -> f32 {
        self.header + self.content_height
    }
}

fn project_sessions(cx: &App) -> Vec<NotchSession> {
    let app = cx.global::<crate::app_state::AppState>();
    let agents = app.agents.read(cx);
    let mut sessions: Vec<_> = agents
        .sessions
        .values()
        .filter(|s| s.unseen || (s.active && s.status != canopy_desktop::agents::Status::Exited))
        .collect();
    sessions.sort_by_key(|s| {
        (
            !s.unseen,
            match s.status {
                canopy_desktop::agents::Status::Waiting => 0,
                canopy_desktop::agents::Status::Failed => 1,
                canopy_desktop::agents::Status::Working => 2,
                _ => 3,
            },
            s.run.clone(),
        )
    });
    sessions
        .into_iter()
        .map(|session| {
            let path = session.pane.metadata.cwd.as_ref();
            let project = app
                .projects
                .read(cx)
                .catalog
                .items
                .iter()
                .find(|p| Some(&p.path) == path);
            NotchSession {
                unseen: session.unseen,
                state: session.status,
                pane: session.pane.id,
                workspace: project
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| session.pane.tool.clone())
                    .into(),
                context: format!(
                    "{} · {}",
                    session.pane.tool,
                    session.model.as_deref().unwrap_or("session")
                )
                .into(),
                status: session
                    .question
                    .as_ref()
                    .map(|q| format!("Question: {q}"))
                    .unwrap_or_else(|| {
                        if session.integration_health()
                            == crate::app_state::IntegrationHealth::AwaitingEvents
                        {
                            return "No agent events".into();
                        }
                        if session.unseen
                            && matches!(
                                session.status,
                                canopy_desktop::agents::Status::Idle
                                    | canopy_desktop::agents::Status::Exited
                            )
                        {
                            "Finished".into()
                        } else {
                            session.status.label().into()
                        }
                    })
                    .into(),
                status_color: match session.status {
                    canopy_desktop::agents::Status::Starting if session.awaiting_events => {
                        t::yellow()
                    }
                    canopy_desktop::agents::Status::Waiting => t::yellow(),
                    canopy_desktop::agents::Status::Failed => t::red(),
                    canopy_desktop::agents::Status::Working => t::accent(),
                    _ => t::green(),
                },
            }
        })
        .collect()
}

pub fn open(main: WindowHandle<Root>, cx: &mut App) -> Result<WindowHandle<Root>> {
    let display = cx
        .primary_display()
        .ok_or_else(|| std::io::Error::other("No primary display"))?;
    let bounds = display.bounds();
    let visible = display.visible_bounds();
    let geometry = Geometry::new(f32::from(visible.origin.y - bounds.origin.y));
    let origin = point(
        bounds.origin.x + (bounds.size.width - px(EXPANDED_WIDTH)) / 2.,
        bounds.origin.y,
    );
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                origin,
                size(
                    px(EXPANDED_WIDTH),
                    px(geometry.header + content_height(MAX_ROWS, true)),
                ),
            ))),
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: false,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id: Some(display.id()),
            window_background: WindowBackgroundAppearance::Transparent,
            inactive_frame_interval: None,
            ..Default::default()
        },
        move |window, cx| {
            let notch = cx.new(|cx| {
                let target = cx.entity().downgrade();
                let async_cx = cx.to_async();
                let executor = cx.foreground_executor().clone();
                let sessions = project_sessions(cx);
                let mut initial_region = geometry.region(Values::default());
                if sessions.is_empty() {
                    initial_region.width = 0.;
                    initial_region.height = 0.;
                }
                let monitor = PointerMonitor::install(window, initial_region, move |event| {
                    let target = target.clone();
                    let mut async_cx = async_cx.clone();
                    executor
                        .spawn(async move {
                            let _ = target.update(&mut async_cx, |this: &mut Notch, cx| {
                                this.handle_native_event(event, cx)
                            });
                        })
                        .detach();
                });
                let monitor = match monitor {
                    Ok(monitor) => Some(monitor),
                    Err(error) => {
                        eprintln!("Notch mouse monitor failed: {error:#}");
                        let toasts = cx.global::<crate::app_state::AppState>().toasts.clone();
                        toasts.update(cx, |toasts, cx| {
                            toasts.show("The session notch is unavailable.", cx)
                        });
                        let status = cx
                            .global::<crate::app_state::AppState>()
                            .notch_status
                            .clone();
                        status.update(cx, |status, cx| {
                            status.fail("Session notch unavailable. Restart Canopy to retry.", cx)
                        });
                        let _ = main.update(cx, |_, window, cx| {
                            cx.activate(true);
                            window.activate_window();
                        });
                        window.remove_window();
                        None
                    }
                };
                let projects = cx.global::<crate::app_state::AppState>().agents.clone();
                let observer = cx.observe(&projects, |this, state, cx| {
                    this.sessions = project_sessions(cx);
                    let revision = state.read(cx).notification_revision;
                    let unseen = state
                        .read(cx)
                        .sessions
                        .values()
                        .any(|session| session.unseen);
                    if revision != this.notice_revision {
                        this.notice_revision = revision;
                        if unseen {
                            this.show_notification(cx);
                        }
                    }
                    if !unseen {
                        this.notice_open = false;
                        this.notice_timer = None;
                        this.sync_motion(cx);
                    }
                    if this.sessions.is_empty() {
                        this.set_pointer_inside(false, cx);
                    }
                    this.sync_motion(cx);
                });
                Notch {
                    _projects_observer: observer,
                    overview: false,
                    filter: StatusFilter::All,
                    scroll: ScrollHandle::new(),
                    notice_open: false,
                    notice_revision: 0,
                    notice_timer: None,
                    #[cfg(target_os = "windows")]
                    native_retry: None,
                    monitor,
                    main,
                    geometry,
                    sessions,
                    rows: canopy_desktop::motion::Transition::new(
                        ROW_HEIGHT + CONTENT_PADDING,
                        Instant::now(),
                    ),
                    pointer: PointerState::default(),
                    motion: Motion::new(Instant::now()),
                }
            });
            cx.new(|cx| Root::new(notch, window, cx).bg(rgba(0)).bordered(false))
        },
    )
}

struct Notch {
    monitor: Option<PointerMonitor>,
    main: WindowHandle<Root>,
    geometry: Geometry,
    sessions: Vec<NotchSession>,
    rows: canopy_desktop::motion::Transition,
    _projects_observer: Subscription,
    overview: bool,
    filter: StatusFilter,
    scroll: ScrollHandle,
    notice_open: bool,
    notice_revision: u64,
    notice_timer: Option<Task<()>>,
    #[cfg(target_os = "windows")]
    native_retry: Option<Task<()>>,
    pointer: PointerState,
    motion: Motion,
}
#[derive(Default)]
struct PointerState {
    inside: bool,
}
impl PointerState {
    fn update(&mut self, inside: bool) -> bool {
        let changed = self.inside != inside;
        self.inside = inside;
        changed
    }
}

// Entering an existing notification (including its closing animation) keeps its mode.
fn hover_opens_overview(notice_open: bool, values: Values) -> bool {
    !notice_open && values == Values::default()
}

impl Notch {
    fn handle_native_event(&mut self, event: NativeEvent, cx: &mut Context<Self>) {
        match event {
            NativeEvent::Pointer(inside) => self.set_pointer_inside(inside, cx),
            #[cfg(target_os = "windows")]
            NativeEvent::Failed(message) => {
                self.set_pointer_inside(false, cx);
                let status = cx
                    .global::<crate::app_state::AppState>()
                    .notch_status
                    .clone();
                status.update(cx, |status, cx| status.fail(message, cx));
                self.schedule_native_retry(cx);
            }
            #[cfg(target_os = "windows")]
            NativeEvent::Recovered => {
                self.native_retry = None;
                let status = cx
                    .global::<crate::app_state::AppState>()
                    .notch_status
                    .clone();
                status.update(cx, |status, cx| status.clear(cx));
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn schedule_native_retry(&mut self, cx: &mut Context<Self>) {
        if self.native_retry.is_some() {
            return;
        }
        self.native_retry = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(1))
                .await;
            let _ = this.update(cx, |this, _| {
                this.native_retry = None;
                if let Some(monitor) = &this.monitor {
                    let _ = monitor.retry();
                }
            });
        }));
    }

    fn sync_motion(&mut self, cx: &mut Context<Self>) {
        let count = self
            .sessions
            .iter()
            .filter(|s| show_session(s, self.overview, self.filter))
            .count();
        self.rows.retarget(
            content_height(count, self.overview),
            canopy_desktop::motion::presets::RESIZE,
            Instant::now(),
            canopy_desktop::motion::policy(cx),
        );
        self.motion.retarget(
            !self.sessions.is_empty() && (self.pointer.inside || self.notice_open),
            Instant::now(),
            canopy_desktop::motion::policy(cx),
        );
        cx.notify();
    }
    fn show_notification(&mut self, cx: &mut Context<Self>) {
        self.scroll.set_offset(point(px(0.), px(0.)));
        if !self.pointer.inside {
            self.overview = false;
        }
        self.notice_open = true;
        self.sync_motion(cx);
        let revision = self.notice_revision;
        self.notice_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(4))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.notice_revision == revision {
                    this.notice_open = false;
                    this.sync_motion(cx);
                }
            });
        }));
    }
    fn set_pointer_inside(&mut self, inside: bool, cx: &mut Context<Self>) {
        let inside = inside && !self.sessions.is_empty();
        if !self.pointer.update(inside) {
            return;
        }
        if inside && hover_opens_overview(self.notice_open, self.motion.values(Instant::now())) {
            self.scroll.set_offset(point(px(0.), px(0.)));
            self.overview = true;
        }
        self.sync_motion(cx);
    }
}

impl Render for Notch {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let values = self.motion.values(now);
        self.geometry.content_height = self.rows.value(now);
        canopy_desktop::motion::request_frame(
            window,
            self.motion.is_animating(now) || self.rows.is_animating(now),
        );
        let width = self.geometry.collapsed_width
            + (EXPANDED_WIDTH - self.geometry.collapsed_width) * values.width;
        let height = self.geometry.header + self.geometry.content_height * values.height;
        let main = self.main;
        let visible: Vec<_> = self
            .sessions
            .iter()
            .filter(|s| show_session(s, self.overview, self.filter))
            .collect();
        if let Some(monitor) = &self.monitor {
            let mut region = self.geometry.region(values);
            if self.sessions.is_empty() {
                region.width = 0.;
                region.height = 0.;
            }
            monitor.set_region(region);
        }
        row()
            .opacity(if !self.sessions.is_empty() { 1. } else { 0. })
            .relative()
            .size_full()
            .items_start()
            .justify_center()
            .child(
                notch_surface(
                    "notch-island",
                    size(px(width), px(height)),
                    px(16. + 8. * values.height),
                )
                .child(notch_header(
                    px(self.geometry.header),
                    self.sessions
                        .first()
                        .map(|s| s.status_color)
                        .unwrap_or_else(t::notch_idle),
                ))
                .child(
                    column()
                        .relative()
                        .top(px(CONTENT_TRAVEL * (1. - values.content)))
                        .opacity(values.content)
                        .px(px(CONTENT_PADDING))
                        .pb(px(CONTENT_PADDING))
                        .flex_shrink_0()
                        .children(self.overview.then(|| {
                            row()
                                .h(px(FILTER_HEIGHT))
                                .flex_shrink_0()
                                .gap(px(4.))
                                .children(StatusFilter::ITEMS.into_iter().map(|(filter, label)| {
                                    notch_filter(label, self.filter == filter).on_click(
                                        cx.listener(move |this, _, _, cx| {
                                            this.filter = filter;
                                            this.scroll.set_offset(point(px(0.), px(0.)));
                                            this.sync_motion(cx);
                                        }),
                                    )
                                }))
                        }))
                        .child(
                            column()
                                .id(if self.overview {
                                    "notch-overview"
                                } else {
                                    "notch-notifications"
                                })
                                .max_h(px((self.geometry.content_height
                                    - CONTENT_PADDING
                                    - if self.overview { FILTER_HEIGHT } else { 0. })
                                .max(0.)))
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .children((visible.is_empty()).then(|| {
                                    row()
                                        .h(px(ROW_HEIGHT))
                                        .px(px(12.))
                                        .text_size(px(11.))
                                        .text_color(t::muted())
                                        .child("No agents with this status")
                                }))
                                .children(visible.into_iter().map(|session| {
                                    let pane = session.pane;
                                    notch_session_row(
                                        SharedString::from(format!("notch-{pane:?}")),
                                        session,
                                    )
                                    .on_click(
                                        move |_, _, cx| {
                                            let agents = cx
                                                .global::<crate::app_state::AppState>()
                                                .agents
                                                .clone();
                                            agents.update(cx, |agents, cx| agents.focus(pane, cx));
                                            let _ = main.update(cx, |_, window, cx| {
                                                cx.activate(true);
                                                window.activate_window();
                                            });
                                        },
                                    )
                                })),
                        ),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Geometry, PointerState, Values};
    use gpui_kit::{point, px};
    #[test]
    fn automatic_notices_ignore_overview_filters_and_exclude_unchanged_agents() {
        use super::{NotchSession, StatusFilter, show_session};
        use canopy_desktop::{agents::Status, state::workspace::PaneId};
        let mut session = NotchSession {
            pane: PaneId::new(),
            unseen: true,
            state: Status::Waiting,
            workspace: "project".into(),
            context: "codex".into(),
            status: "Needs attention".into(),
            status_color: super::t::yellow(),
        };
        assert!(show_session(&session, false, StatusFilter::Working));
        assert!(!show_session(&session, true, StatusFilter::Working));
        assert!(show_session(&session, true, StatusFilter::Waiting));
        session.unseen = false;
        assert!(!show_session(&session, false, StatusFilter::All));
        assert!(show_session(&session, true, StatusFilter::All));
    }
    #[test]
    fn viewport_caps_at_eight_rows_and_keeps_filter_space_separate() {
        use super::{CONTENT_PADDING, FILTER_HEIGHT, ROW_HEIGHT, content_height};
        assert_eq!(
            content_height(8, true),
            ROW_HEIGHT * 8. + CONTENT_PADDING + FILTER_HEIGHT
        );
        assert_eq!(content_height(80, true), content_height(8, true));
        assert_eq!(content_height(1, false), ROW_HEIGHT + CONTENT_PADDING);
        assert_eq!(content_height(0, true), content_height(1, true));
    }
    #[test]
    fn filters_cover_all_agent_states() {
        use super::StatusFilter;
        use canopy_desktop::agents::Status;
        for (status, expected) in [
            (Status::Starting, StatusFilter::Working),
            (Status::Working, StatusFilter::Working),
            (Status::Waiting, StatusFilter::Waiting),
            (Status::Failed, StatusFilter::Failed),
            (Status::Idle, StatusFilter::Idle),
            (Status::Exited, StatusFilter::Idle),
        ] {
            assert!(StatusFilter::All.matches(status));
            for (filter, _) in StatusFilter::ITEMS.into_iter().skip(1) {
                assert_eq!(filter.matches(status), filter == expected);
            }
        }
    }
    #[test]
    fn notification_hover_stays_in_notification_mode_until_fully_collapsed() {
        use super::hover_opens_overview;
        assert!(!hover_opens_overview(true, Values::default()));
        for progress in [0.01, 0.5, 1.] {
            let values = Values {
                width: progress,
                height: progress,
                content: progress,
            };
            assert!(!hover_opens_overview(true, values));
            // Timeout while hovered and re-entry during closing still preserve the notice.
            assert!(!hover_opens_overview(false, values));
        }
        assert!(hover_opens_overview(false, Values::default()));
    }
    #[test]
    fn geometry_matches_electron_reference() {
        let g = Geometry::new(39.);
        assert_eq!(g.header, 39.);
        assert_eq!(g.collapsed_width, 295.);
        assert_eq!(g.expanded_height(), 93.);
    }
    #[test]
    fn auto_hidden_menu_has_bounded_fallback() {
        let g = Geometry::new(0.);
        assert_eq!(g.header, 37.);
        assert!(g.collapsed_width < 480.);
        assert_eq!(Geometry::new(200.).header, 48.);
    }
    #[test]
    fn cursor_outside_stays_outside_through_collapse() {
        let geometry = Geometry::new(39.);
        for step in 0..=100 {
            assert!(!geometry.contains_pointer(
                Values {
                    width: step as f32 / 100.,
                    height: step as f32 / 100.,
                    content: 0.
                },
                point(px(200.), px(110.))
            ));
        }
        assert!(geometry.contains_pointer(
            Values {
                width: 1.,
                height: 1.,
                content: 1.
            },
            point(px(200.), px(65.))
        ));
        assert!(!geometry.contains_pointer(Values::default(), point(px(200.), px(65.))));
    }
    #[test]
    fn repeated_move_does_not_restart_transition() {
        let mut pointer = PointerState::default();
        assert!(pointer.update(true));
        assert!(!pointer.update(true)); // header -> session remains one enter
        assert!(pointer.update(false));
        assert!(!pointer.update(false)); // repeated outside moves do not restart the close
        assert!(pointer.update(true)); // actual re-entry may reopen
    }
}
