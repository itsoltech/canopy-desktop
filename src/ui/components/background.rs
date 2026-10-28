//! Custom window and sidebar backgrounds (Settings → Appearance): what a surface
//! paints for a stored `Background`, and multi-stop gradients for GPUI, whose
//! linear gradient has only two stops.
//!
//! A gradient of n stops is painted as n − 1 two-stop layers over the same
//! bounds and angle: layer 0 runs opaque c0@p0 → c1@p1, layer i runs
//! transparent c(i+1)@p(i) → c(i+1)@p(i+1). Past p(i) the layers below already
//! show c(i), so blending layer i over them by its alpha is exactly the linear
//! interpolation between c(i) and c(i+1); before p(i) it is transparent and past
//! p(i+1) it covers everything below with c(i+1).
use super::super::theme::{self as t, Background as Stored, BackgroundMode, BackgroundTarget};
use gpui_kit::*;
use std::collections::HashMap;

/// Shortest span between two layer stops; coincident stops make a hard edge.
const MIN_SPAN: f32 = 0.0001;

/// What a surface paints.
#[derive(Clone, Debug)]
pub enum SurfacePaint {
    /// The theme's own surface color: nothing custom is painted.
    Theme(Hsla),
    Solid(Hsla),
    /// Two-stop layers, painted bottom to top over the same bounds.
    Gradient(Vec<Background>),
}
impl SurfacePaint {
    /// The paint of a stored background; `theme` is the surface's theme color.
    pub fn of(background: &Stored, theme: Hsla) -> Self {
        match background.mode {
            BackgroundMode::Theme => Self::Theme(theme),
            BackgroundMode::Solid => Self::Solid(rgb(background.solid.0).into()),
            BackgroundMode::Gradient => {
                let stops: Vec<_> = background
                    .gradient
                    .sorted_stops()
                    .into_iter()
                    .map(|stop| (rgb(stop.color.0).into(), stop.position as f32 / 100.))
                    .collect();
                Self::Gradient(gradient_fills(background.gradient.angle() as f32, &stops))
            }
        }
    }
    /// A custom color or gradient replaces the theme surface.
    pub fn is_custom(&self) -> bool {
        !matches!(self, Self::Theme(_))
    }
    /// Paints into `bounds` with rounded corners.
    pub fn paint(
        &self,
        bounds: Bounds<Pixels>,
        corners: impl Into<Corners<Pixels>>,
        window: &mut Window,
    ) {
        let corners = corners.into();
        match self {
            Self::Theme(color) | Self::Solid(color) => {
                window.paint_quad(fill(bounds, *color).corner_radii(corners))
            }
            Self::Gradient(layers) => {
                for layer in layers {
                    window.paint_quad(fill(bounds, *layer).corner_radii(corners));
                }
            }
        }
    }
    /// Paints the surface laid out over `surface`, visible only inside `clip`,
    /// at `opacity`: a backdrop that matches the surface behind it.
    pub fn paint_clipped(
        &self,
        surface: Bounds<Pixels>,
        clip: Bounds<Pixels>,
        opacity: f32,
        window: &mut Window,
    ) {
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| match self {
            Self::Theme(color) | Self::Solid(color) => {
                window.paint_quad(fill(clip, color.opacity(opacity)))
            }
            Self::Gradient(layers) => {
                for layer in layers {
                    window.paint_quad(fill(surface, layer.opacity(opacity)));
                }
            }
        });
    }
}

/// Two-stop layers (color, position 0–1) whose stack paints the gradient of
/// `stops`, sorted by position. See the module documentation.
pub fn gradient_layers(stops: &[(Hsla, f32)]) -> Vec<[(Hsla, f32); 2]> {
    let span = |from: f32, to: f32| {
        if to - from >= MIN_SPAN {
            (from, to)
        } else if to >= MIN_SPAN {
            (to - MIN_SPAN, to)
        } else {
            (from, from + MIN_SPAN)
        }
    };
    match stops {
        [] => Vec::new(),
        [(color, _)] => vec![[(*color, 0.), (*color, 1.)]],
        _ => stops
            .windows(2)
            .enumerate()
            .map(|(index, pair)| {
                let ((from_color, from), (to_color, to)) = (pair[0], pair[1]);
                let (from, to) = span(from, to);
                let start = if index == 0 {
                    from_color
                } else {
                    to_color.opacity(0.)
                };
                [(start, from), (to_color, to)]
            })
            .collect(),
    }
}

/// GPUI fills for `gradient_layers` at a CSS angle (0 = towards the top).
pub fn gradient_fills(angle: f32, stops: &[(Hsla, f32)]) -> Vec<Background> {
    gradient_layers(stops)
        .into_iter()
        .map(|[(from, start), (to, end)]| {
            linear_gradient(
                angle,
                linear_color_stop(from, start),
                linear_color_stop(to, end),
            )
        })
        .collect()
}

/// An element that paints `paint` over its bounds with rounded corners (rounded
/// parents do not clip their children in GPUI); size it like any element.
pub fn paint_canvas(paint: SurfacePaint, corners: impl Into<Corners<Pixels>>) -> Canvas<()> {
    let corners = corners.into();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| paint.paint(bounds, corners, window),
    )
}

/// The window shows a custom color or gradient instead of the theme surface.
pub fn window_is_custom(cx: &App) -> bool {
    t::background(BackgroundTarget::Window, cx).mode != BackgroundMode::Theme
}
/// Fill of a region that lies on the window surface (tab bar, pane header, chat):
/// its theme color, or none over a custom window background, which shows through.
pub fn window_fill(theme: Hsla, cx: &App) -> Hsla {
    if window_is_custom(cx) {
        transparent_black()
    } else {
        theme
    }
}

/// The window background (behind tabs, panes and chats) as shown now.
pub fn window_paint(cx: &App) -> SurfacePaint {
    SurfacePaint::of(&t::background(BackgroundTarget::Window, cx), t::bg())
}
/// The sidebar background as shown now.
pub fn sidebar_paint(cx: &App) -> SurfacePaint {
    SurfacePaint::of(&t::background(BackgroundTarget::Sidebar, cx), t::sidebar())
}

/// Where each window painted its window background in the last frame, for
/// backdrops that must match it. Written only by `window_surface`.
#[derive(Default)]
struct WindowSurfaces(HashMap<WindowId, Bounds<Pixels>>);
impl Global for WindowSurfaces {}

/// Paints the window background behind `element`'s children. Call before
/// adding children: a gradient is painted by the first child.
pub fn window_surface(element: Div, cx: &App) -> Div {
    surface(element, window_paint(cx), true)
}
/// Paints the sidebar background behind `element`'s children; see `window_surface`.
pub fn sidebar_surface(element: Div, cx: &App) -> Div {
    surface(element, sidebar_paint(cx), false)
}
/// Paints `paint` behind `element`'s children: a color as its background, a
/// gradient by a first, absolutely placed child (no hit target).
fn surface(element: Div, paint: SurfacePaint, record: bool) -> Div {
    match paint {
        SurfacePaint::Theme(color) | SurfacePaint::Solid(color) => element.bg(color),
        SurfacePaint::Gradient(layers) => element.child(
            canvas(
                move |bounds, window, cx| {
                    if record {
                        cx.default_global::<WindowSurfaces>()
                            .0
                            .insert(window.window_handle().window_id(), bounds);
                    }
                },
                move |bounds, _, window, _| {
                    for layer in &layers {
                        window.paint_quad(fill(bounds, *layer));
                    }
                },
            )
            .absolute()
            .inset_0(),
        ),
    }
}

/// A backdrop over the window background, e.g. under an overlay that hides
/// content scrolled beneath it: the same paint, clipped to the backdrop, its
/// opacity rising from `from` at the top to `to` at the bottom. `None` when the
/// window shows the theme color, which callers paint as before.
pub fn window_backdrop(from: f32, to: f32, cx: &App) -> Option<impl IntoElement + use<>> {
    let paint = window_paint(cx);
    paint.is_custom().then(|| {
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let surface = cx
                    .try_global::<WindowSurfaces>()
                    .and_then(|surfaces| surfaces.0.get(&window.window_handle().window_id()))
                    .copied()
                    .unwrap_or(bounds);
                // Thin bands approximate the opacity ramp; GPUI has no alpha masks.
                let bands = if (to - from).abs() < f32::EPSILON {
                    1
                } else {
                    (f32::from(bounds.size.height) / 2.).clamp(1., 12.) as usize
                };
                let height = bounds.size.height / bands as f32;
                for band in 0..bands {
                    let clip = Bounds::new(
                        bounds.origin + point(px(0.), height * band as f32),
                        size(bounds.size.width, height),
                    );
                    let opacity = from + (to - from) * (band as f32 + 0.5) / bands as f32;
                    paint.paint_clipped(surface, clip, opacity, window);
                }
            },
        )
        .absolute()
        .inset_0()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` brings GPUI's `test` attribute; these are plain tests.
    use core::prelude::v1::test;

    fn color(hex: u32) -> Hsla {
        rgb(hex).into()
    }
    /// What GPUI's shader paints at `t`: each layer interpolates its two stops
    /// (alpha included) and is blended over the layers below.
    fn composite(layers: &[[(Hsla, f32); 2]], t: f32) -> Rgba {
        let mut out = Rgba {
            r: 0.,
            g: 0.,
            b: 0.,
            a: 1.,
        };
        for [(from, start), (to, end)] in layers {
            let u = ((t - start) / (end - start)).clamp(0., 1.);
            let (a, b): (Rgba, Rgba) = ((*from).into(), (*to).into());
            let lerp = |x: f32, y: f32| x + (y - x) * u;
            let alpha = lerp(a.a, b.a);
            out = Rgba {
                r: lerp(a.r, b.r) * alpha + out.r * (1. - alpha),
                g: lerp(a.g, b.g) * alpha + out.g * (1. - alpha),
                b: lerp(a.b, b.b) * alpha + out.b * (1. - alpha),
                a: 1.,
            };
        }
        out
    }
    /// CSS semantics: the first color before the first stop, the last after the
    /// last, linear interpolation between neighbours.
    fn reference(stops: &[(Hsla, f32)], t: f32) -> Rgba {
        let rgba = |c: Hsla| -> Rgba { c.into() };
        if t <= stops[0].1 {
            return rgba(stops[0].0);
        }
        for pair in stops.windows(2) {
            let ((a, pa), (b, pb)) = (pair[0], pair[1]);
            if t <= pb {
                let u = if pb > pa { (t - pa) / (pb - pa) } else { 1. };
                let (a, b) = (rgba(a), rgba(b));
                let lerp = |x: f32, y: f32| x + (y - x) * u;
                return Rgba {
                    r: lerp(a.r, b.r),
                    g: lerp(a.g, b.g),
                    b: lerp(a.b, b.b),
                    a: 1.,
                };
            }
        }
        rgba(stops[stops.len() - 1].0)
    }

    #[test]
    fn layered_two_stop_gradients_reproduce_every_multi_stop_gradient() {
        let cases: Vec<Vec<(Hsla, f32)>> = vec![
            vec![
                (color(0x141a26), 0.),
                (color(0x1d2a3f), 0.55),
                (color(0x2a2240), 1.),
            ],
            vec![
                (color(0xff0000), 0.2),
                (color(0x00ff00), 0.4),
                (color(0x0000ff), 0.9),
            ],
            vec![
                (color(0x000000), 0.),
                (color(0xffffff), 0.2),
                (color(0x336699), 0.4),
                (color(0xcc8844), 0.6),
                (color(0x22aa55), 0.8),
                (color(0x101010), 1.),
            ],
            vec![(color(0x0b1d2e), 0.), (color(0x103a4f), 1.)],
        ];
        for stops in &cases {
            let layers = gradient_layers(stops);
            assert_eq!(layers.len(), stops.len() - 1);
            // Only the first layer is opaque at its start.
            assert_eq!(layers[0][0].0.a, 1.);
            assert!(layers[1..].iter().all(|layer| layer[0].0.a == 0.));
            for step in 0..=200 {
                let t = step as f32 / 200.;
                let (got, want) = (composite(&layers, t), reference(stops, t));
                for (g, w) in [(got.r, want.r), (got.g, want.g), (got.b, want.b)] {
                    assert!((g - w).abs() < 1e-4, "t={t} got {got:?} want {want:?}");
                }
            }
        }
    }

    #[test]
    fn coincident_stops_make_a_hard_edge_without_a_zero_span() {
        let stops = [
            (color(0xff0000), 0.),
            (color(0x00ff00), 0.5),
            (color(0x0000ff), 0.5),
            (color(0xffffff), 1.),
        ];
        let layers = gradient_layers(&stops);
        assert!(
            layers
                .iter()
                .all(|[(_, a), (_, b)]| b - a >= MIN_SPAN * 0.5)
        );
        let before: Rgba = composite(&layers, 0.49);
        let after: Rgba = composite(&layers, 0.501);
        assert!(before.g > 0.9 && before.b < 0.1, "{before:?}");
        assert!(after.b > 0.9 && after.g < 0.1, "{after:?}");
        // Both stops at an edge still give a finite span.
        let edge = gradient_layers(&[(color(0x000000), 1.), (color(0xffffff), 1.)]);
        assert!(edge[0][1].1 - edge[0][0].1 >= MIN_SPAN * 0.5);
        let start = gradient_layers(&[(color(0x000000), 0.), (color(0xffffff), 0.)]);
        assert!(start[0][1].1 > start[0][0].1);
    }
}
