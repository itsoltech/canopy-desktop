//! Agent identity and activity in the sidebar: the provider tile, the edge traced
//! around it while the agent works, status badges and working / needs-you tallies.
use super::*;
use crate::app_state::AppState;
use canopy_desktop::chat::provider::Provider;
use canopy_desktop::motion::{Easing, duration};
use canopy_desktop::state::workspace::PaneId;
use gpui_kit::component::tooltip::Tooltip;
use std::f32::consts::{FRAC_PI_2, PI};

/// What a session row shows; derived from the shared navigation status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Activity {
    Working,
    Goal,
    Attention,
    /// Idle and finished since the user last looked (`AgentSession::unseen`).
    Done,
    Idle,
    Error,
    Starting,
    NoStatus,
    Loading,
    Stopping,
    Exited,
    NotStarted,
}
impl Activity {
    pub fn of(id: PaneId, cx: &App) -> Self {
        let unseen = cx
            .global::<AppState>()
            .agents
            .read(cx)
            .sessions
            .get(&id)
            .is_some_and(|s| s.unseen);
        match navigation_status(id, cx) {
            "Working" => Self::Working,
            "Goal" => Self::Goal,
            "Needs attention" | "Waiting" => Self::Attention,
            "Idle" if unseen => Self::Done,
            "Idle" => Self::Idle,
            "Error" => Self::Error,
            "Starting" => Self::Starting,
            "No status" => Self::NoStatus,
            "Loading" => Self::Loading,
            "Stopping" => Self::Stopping,
            "Exited" => Self::Exited,
            _ => Self::NotStarted,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Goal => "Goal",
            Self::Attention => "Needs you",
            Self::Done => "Done",
            Self::Idle => "Idle",
            Self::Error => "Error",
            Self::Starting => "Starting…",
            Self::NoStatus => "No status",
            Self::Loading => "Loading…",
            Self::Stopping => "Stopping…",
            Self::Exited => "Exited",
            Self::NotStarted => "Not started",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Self::Working => "The agent is working",
            Self::Goal => "Working toward a /goal across turns",
            Self::Attention => "Waiting for your answer or approval",
            Self::Done => "Finished since you last looked",
            Self::Idle => "Waiting for your next message",
            Self::Error => "Stopped with an error",
            Self::Starting => "The agent process is starting",
            Self::NoStatus => "Started, but no status reported yet",
            Self::Loading => "Loading the conversation",
            Self::Stopping => "Stopping the agent",
            Self::Exited => "The agent process has exited",
            Self::NotStarted => "Starts with your first message",
        }
    }
    pub fn color(self) -> Hsla {
        match self {
            Self::Working => t::secondary(),
            Self::Goal => t::goal(),
            Self::Attention => t::yellow(),
            Self::Done => t::green(),
            Self::Error => t::red(),
            Self::Exited | Self::NotStarted => t::faint(),
            _ => t::muted(),
        }
    }
    pub fn working(self) -> bool {
        matches!(self, Self::Working | Self::Goal)
    }
    pub fn needs_you(self) -> bool {
        matches!(self, Self::Attention | Self::Error)
    }
}

pub(super) fn provider_mark(provider: Option<Provider>) -> (Hsla, &'static str) {
    match provider {
        Some(Provider::Claude) => (t::claude_mark(), "canopy/claude.svg"),
        _ => (t::codex_mark(), "canopy/openai.svg"),
    }
}

/// The agent's provider tile with its activity edge and status badge.
/// `group` is the row's hover group: the badge's cut-out follows the row fill.
pub(super) fn agent_avatar(
    pane: PaneId,
    provider: Option<Provider>,
    activity: Activity,
    group: &SharedString,
    selected: bool,
    reduced: bool,
) -> Div {
    let (mark, path) = provider_mark(provider);
    let (fill, edge, ink) = match activity {
        Activity::NotStarted => (transparent_black(), t::control_border(), t::muted()),
        Activity::Exited => (t::hover(), t::border(), t::faint()),
        _ => (t::agent_tile(mark), t::agent_tile_edge(mark), mark),
    };
    div()
        .relative()
        .flex_shrink_0()
        .size(rems(1.625))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(t::agent_tile_size())
                .flex()
                .items_center()
                .justify_center()
                .rounded(t::agent_tile_radius())
                .bg(fill)
                .border_1()
                .border_color(edge)
                .child(Icon::default().path(path).size_3p5().text_color(ink)),
        )
        .children(activity_edge(pane, activity, reduced))
        .children(badge(pane, activity, group, selected, reduced))
}

#[derive(Clone, Copy, PartialEq)]
enum Edge {
    /// A light segment travelling along the tile's edge.
    Trace,
    /// Dashes crawling slowly: the agent is starting or stopping.
    Crawl,
    /// Still dashes: started, but nothing reported yet.
    Dashes,
}

fn activity_edge(pane: PaneId, activity: Activity, reduced: bool) -> Option<AnyElement> {
    let (edge, color) = match activity {
        Activity::Working => (Edge::Trace, t::accent()),
        Activity::Goal => (Edge::Trace, t::goal()),
        Activity::Starting | Activity::Loading | Activity::Stopping => (Edge::Crawl, t::muted()),
        Activity::NoStatus => (Edge::Dashes, t::muted()),
        _ => return None,
    };
    // Concentric with the tile, just outside it.
    let frame = div()
        .absolute()
        .top(rems(-0.1875))
        .left(rems(-0.1875))
        .size(rems(2.));
    if reduced || edge == Edge::Dashes {
        return Some(frame.child(edge_canvas(edge, color, 0.)).into_any_element());
    }
    let period = if edge == Edge::Trace {
        duration::ORBIT
    } else {
        duration::CRAWL
    };
    Some(
        frame
            .with_animation(
                SharedString::from(format!("activity-edge-{pane:?}")),
                Animation::new(period).repeat_synced(),
                move |frame, phase| frame.child(edge_canvas(edge, color, phase)),
            )
            .into_any_element(),
    )
}

fn edge_canvas(edge: Edge, color: Hsla, phase: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            // Geometry in the frame's 32-unit space: a 1.5-unit stroke 1.75 units
            // outside the 24-unit tile, radius = tile radius + gap.
            let unit = f32::from(bounds.size.width) / 32.;
            let outline = Outline::new(bounds, unit * 1.5, unit * 9.5);
            let width = unit * 1.5;
            match edge {
                Edge::Trace => {
                    outline.stroke(0., 1., width, color.opacity(0.16), window);
                    outline.stroke(phase, 0.24, width, color, window);
                }
                Edge::Crawl | Edge::Dashes => {
                    let (shift, opacity) = if edge == Edge::Crawl {
                        (phase, 0.8)
                    } else {
                        (0., 0.55)
                    };
                    for dash in 0..14 {
                        let start = (dash as f32 + shift) / 14.;
                        outline.stroke(start, 0.03, width, color.opacity(opacity), window);
                    }
                }
            }
        },
    )
    .size_full()
}

/// A rounded square sampled as a closed polyline, measured along its length.
struct Outline {
    points: Vec<(f32, f32)>,
    lengths: Vec<f32>,
}
impl Outline {
    fn new(bounds: Bounds<Pixels>, inset: f32, radius: f32) -> Self {
        let left = f32::from(bounds.left()) + inset;
        let top = f32::from(bounds.top()) + inset;
        let right = f32::from(bounds.right()) - inset;
        let bottom = f32::from(bounds.bottom()) - inset;
        let r = radius.min((right - left) / 2.).min((bottom - top) / 2.);
        // Clockwise from the top edge, like an SVG rect.
        let corners = [
            ((right - r, top + r), -FRAC_PI_2),
            ((right - r, bottom - r), 0.),
            ((left + r, bottom - r), FRAC_PI_2),
            ((left + r, top + r), PI),
        ];
        let mut points = vec![(left + r, top)];
        for ((x, y), start) in corners {
            for step in 0..=8 {
                let angle = start + FRAC_PI_2 * step as f32 / 8.;
                points.push((x + r * angle.cos(), y + r * angle.sin()));
            }
        }
        points.push((left + r, top));
        let mut lengths = vec![0.];
        for pair in points.windows(2) {
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            lengths.push(lengths.last().unwrap() + (dx * dx + dy * dy).sqrt());
        }
        Self { points, lengths }
    }
    /// Strokes `length` of the outline (both as fractions of it) from `start`, wrapping.
    fn stroke(&self, start: f32, length: f32, width: f32, color: Hsla, window: &mut Window) {
        let total = *self.lengths.last().unwrap();
        let from = start.rem_euclid(1.) * total;
        let to = from + length.min(1.) * total;
        let mut path = PathBuilder::stroke(px(width));
        let mut open = false;
        for lap in 0..2 {
            let offset = lap as f32 * total;
            for i in 1..self.points.len() {
                let (a, b) = (self.lengths[i - 1] + offset, self.lengths[i] + offset);
                if b <= a || b < from || a >= to {
                    continue;
                }
                let at = |d: f32| {
                    let k = ((d - a) / (b - a)).clamp(0., 1.);
                    let (p, q) = (self.points[i - 1], self.points[i]);
                    point(px(p.0 + (q.0 - p.0) * k), px(p.1 + (q.1 - p.1) * k))
                };
                if !open {
                    path.move_to(at(from));
                    open = true;
                }
                path.line_to(at(to));
            }
        }
        if open && let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}

fn badge(
    pane: PaneId,
    activity: Activity,
    group: &SharedString,
    selected: bool,
    reduced: bool,
) -> Vec<AnyElement> {
    let (color, glyph): (_, Option<fn() -> Icon>) = match activity {
        Activity::Attention => (t::yellow(), None),
        Activity::Error => (t::red(), Some(|| Icon::default().path("canopy/bang.svg"))),
        Activity::Done => (t::green(), Some(|| Icon::new(IconName::Check))),
        _ => return vec![],
    };
    let group = group.clone();
    // A 0.75rem dot inside a 2px cut-out in the row's own fill.
    let dot = move |scale: f32| {
        let size = 0.6 + 0.4 * scale;
        let offset = -0.3125 + (1. - size) / 2.;
        div()
            .absolute()
            .right(rems(offset))
            .bottom(rems(offset))
            .size(rems(size))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(color)
            .border_2()
            .border_color(if selected {
                t::chat_selected()
            } else {
                t::sidebar()
            })
            .children(glyph.map(|glyph| glyph().size_2().text_color(t::badge_ink())))
    };
    let dot = move |scale: f32| {
        let dot = dot(scale);
        if selected {
            dot
        } else {
            dot.group_hover(group.clone(), |s| s.border_color(t::chat_hover()))
        }
    };
    if reduced {
        return vec![dot(1.).into_any_element()];
    }
    let pop = Easing::SmoothOut;
    let mut layers = vec![];
    if activity == Activity::Attention {
        // Three pulses, then still: no perpetual blinking.
        layers.push(
            div()
                .absolute()
                .with_animation(
                    SharedString::from(format!("attention-halo-{pane:?}")),
                    Animation::new(duration::PULSE * 3),
                    move |halo, progress| {
                        let phase = 1. - (1. - (progress * 3.).fract()).powi(2);
                        let size = 0.75 * (1. + 1.6 * phase);
                        let center = 1.4375;
                        halo.left(rems(center - size / 2.))
                            .top(rems(center - size / 2.))
                            .size(rems(size))
                            .rounded_full()
                            .bg(t::yellow())
                            .opacity(if progress >= 1. {
                                0.
                            } else {
                                0.55 * (1. - phase)
                            })
                    },
                )
                .into_any_element(),
        );
    }
    layers.push(
        div()
            .with_animation(
                SharedString::from(format!("status-badge-{pane:?}-{}", activity.label())),
                Animation::new(duration::QUICK),
                move |_, progress| {
                    let scale = pop.sample(progress);
                    dot(scale).opacity(scale)
                },
            )
            .into_any_element(),
    );
    layers
}

/// Agent sessions of a group, and how many of them work or need the user.
#[derive(Clone, Copy, Default)]
pub(super) struct Tally {
    pub sessions: usize,
    pub working: usize,
    pub needs_you: usize,
}
impl Tally {
    pub fn of(panes: impl IntoIterator<Item = PaneId>, cx: &App) -> Self {
        panes.into_iter().fold(Self::default(), |mut tally, pane| {
            let activity = Activity::of(pane, cx);
            tally.sessions += 1;
            tally.working += activity.working() as usize;
            tally.needs_you += activity.needs_you() as usize;
            tally
        })
    }
    pub fn is_empty(self) -> bool {
        self.working + self.needs_you == 0
    }
    /// Colored pips with counts; `long` adds the words (the footer).
    pub fn pips(self, id: &str, long: bool) -> Option<Div> {
        if self.is_empty() {
            return None;
        }
        let pip = |part: &str, count: usize, color: Hsla, word: &str, tip: String| {
            div()
                .id(SharedString::from(format!("{id}-{part}")))
                .flex()
                .items_center()
                .gap_1()
                .child(div().size(rems(0.4375)).rounded_full().bg(color))
                .child(if long {
                    format!("{count} {word}")
                } else {
                    count.to_string()
                })
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        };
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .text_xs()
                .text_color(t::secondary())
                .when(self.working > 0, |d| {
                    d.child(pip(
                        "working",
                        self.working,
                        t::accent(),
                        "working",
                        format!("{} working", self.working),
                    ))
                })
                .when(self.needs_you > 0, |d| {
                    d.child(pip(
                        "needs-you",
                        self.needs_you,
                        t::yellow(),
                        "need you",
                        format!(
                            "{} {} your attention",
                            self.needs_you,
                            if self.needs_you == 1 { "needs" } else { "need" }
                        ),
                    ))
                }),
        )
    }
}

/// A project's monogram on its identity hue.
pub(super) fn project_tile(name: &str) -> Div {
    let hue = t::project_hue(name);
    let initials: String = name
        .split(['-', '_', ' ', '.'])
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    div()
        .size(t::project_tile_size())
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(rems(0.3125))
        .bg(t::project_tile(hue))
        .text_color(t::project_tile_text(hue))
        .text_size(rems(0.6875))
        .font_weight(FontWeight::BOLD)
        .child(initials)
}
