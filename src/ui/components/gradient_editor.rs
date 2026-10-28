//! Gradient editor for custom backgrounds: a bar with draggable color stops, the
//! selected stop's color and position, a direction dial and presets.
//!
//! The editor owns only the draft and its interaction state. It emits
//! `Preview` while a stop or the dial is dragged and `Change` for every settled
//! edit (mouse up, a committed field, a key press, a preset); the owner shows
//! previews and persists changes, coalescing bursts.
use super::super::theme::{
    self as t, GRADIENT_MAX_STOPS, GRADIENT_MIN_STOPS, Gradient, GradientStop, HexColor,
    ThemeVariant,
};
use super::background::{SurfacePaint, gradient_fills, paint_canvas};
use super::{button, icon, icon_button, input, row};
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{InputEvent, InputState},
};
use gpui_kit::{base::Disableable, prelude::FluentBuilder, *};
use std::{cell::Cell, rc::Rc};

/// A gradient being edited: angle, stops in the user's order and the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientModel {
    angle: u16,
    stops: Vec<GradientStop>,
    selected: usize,
}
impl GradientModel {
    pub fn new(gradient: &Gradient) -> Self {
        Self {
            angle: gradient.angle(),
            stops: gradient.stops().to_vec(),
            selected: 0,
        }
    }
    pub fn gradient(&self) -> Gradient {
        Gradient::new(self.angle, self.stops.clone()).expect("the editor keeps gradients valid")
    }
    pub fn angle(&self) -> u16 {
        self.angle
    }
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn selected_stop(&self) -> GradientStop {
        self.stops[self.selected]
    }
    pub fn select(&mut self, index: usize) {
        if index < self.stops.len() {
            self.selected = index;
        }
    }
    pub fn can_add(&self) -> bool {
        self.stops.len() < GRADIENT_MAX_STOPS
    }
    pub fn can_remove(&self) -> bool {
        self.stops.len() > GRADIENT_MIN_STOPS
    }
    /// The gradient's color at `position` (0–100), interpolated in sRGB.
    pub fn color_at(&self, position: f32) -> HexColor {
        let mut stops = self.stops.clone();
        stops.sort_by_key(|stop| stop.position);
        let first = stops[0];
        if position <= first.position as f32 {
            return first.color;
        }
        for pair in stops.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if position <= b.position as f32 {
                let span = (b.position as f32 - a.position as f32).max(1.);
                return mix(a.color, b.color, (position - a.position as f32) / span);
            }
        }
        stops[stops.len() - 1].color
    }
    /// Adds a stop at `position` with the color the gradient has there and
    /// selects it. `false` when the gradient has the most stops allowed.
    pub fn add_at(&mut self, position: f32) -> bool {
        if !self.can_add() {
            return false;
        }
        let position = position.round().clamp(0., 100.);
        self.stops.push(GradientStop {
            color: self.color_at(position),
            position: position as u8,
        });
        self.selected = self.stops.len() - 1;
        true
    }
    /// Adds a stop in the middle of the widest gap between neighbouring stops.
    pub fn add_in_largest_gap(&mut self) -> bool {
        let mut positions: Vec<u8> = self.stops.iter().map(|stop| stop.position).collect();
        positions.sort_unstable();
        let (start, end) = positions
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            // The first of equally wide gaps wins.
            .fold((0, 0), |best, gap| {
                if gap.1 - gap.0 > best.1 - best.0 {
                    gap
                } else {
                    best
                }
            });
        self.add_at((start as f32 + end as f32) / 2.)
    }
    /// Removes the selected stop and selects the stop nearest to it. `false`
    /// when only the fewest stops allowed remain.
    pub fn remove_selected(&mut self) -> bool {
        if !self.can_remove() {
            return false;
        }
        let removed = self.stops.remove(self.selected);
        self.selected = (0..self.stops.len())
            .min_by_key(|&index| self.stops[index].position.abs_diff(removed.position))
            .unwrap_or(0);
        true
    }
    /// Moves the selected stop to `position`, clamped to 0–100.
    pub fn move_selected(&mut self, position: f32) {
        if position.is_finite() {
            self.stops[self.selected].position = position.round().clamp(0., 100.) as u8;
        }
    }
    pub fn nudge_selected(&mut self, delta: i16) {
        let position = self.stops[self.selected].position as i16 + delta;
        self.move_selected(position as f32);
    }
    pub fn set_selected_color(&mut self, color: HexColor) {
        self.stops[self.selected].color = color;
    }
    /// Sets the angle in degrees, wrapped into 0–359.
    pub fn set_angle(&mut self, angle: i32) {
        self.angle = angle.rem_euclid(360) as u16;
    }
    pub fn apply(&mut self, preset: &Preset) {
        self.angle = preset.angle;
        self.stops = preset
            .stops
            .iter()
            .map(|&(color, position)| GradientStop {
                color: HexColor(color),
                position,
            })
            .collect();
        self.selected = 0;
    }
}

fn channels(color: HexColor) -> [f32; 3] {
    [16, 8, 0].map(|shift| ((color.0 >> shift) & 0xff) as f32)
}
fn mix(a: HexColor, b: HexColor, t: f32) -> HexColor {
    let (a, b) = (channels(a), channels(b));
    let t = t.clamp(0., 1.);
    let channel = |i: usize| (a[i] + (b[i] - a[i]) * t).round() as u32;
    HexColor((channel(0) << 16) | (channel(1) << 8) | channel(2))
}
fn hex_of(color: Hsla) -> HexColor {
    let color: Rgba = color.into();
    let channel = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    HexColor((channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b))
}
fn hsla_of(color: HexColor) -> Hsla {
    rgb(color.0).into()
}

/// The angle (CSS degrees, 0 = up, clockwise) pointing from `center` to
/// `point`; `snap` rounds to 15° steps.
pub fn angle_towards(center: Point<Pixels>, point: Point<Pixels>, snap: bool) -> u16 {
    let delta = point - center;
    let degrees = f32::from(delta.y).atan2(f32::from(delta.x)).to_degrees() + 90.;
    let mut angle = degrees.rem_euclid(360.).round();
    if snap {
        angle = (angle / 15.).round() * 15.;
    }
    (angle as u16) % 360
}

/// WCAG relative luminance of an sRGB color.
fn luminance(color: HexColor) -> f32 {
    let linear = |v: f32| {
        let v = v / 255.;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b] = channels(color).map(linear);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}
/// A warning when a background color is too light for a dark variant (or too
/// dark for a light one) to keep the theme's text readable on it.
pub fn readability_warning(
    colors: impl IntoIterator<Item = HexColor>,
    variant: ThemeVariant,
) -> Option<&'static str> {
    let mut colors = colors.into_iter().map(luminance);
    match variant {
        ThemeVariant::Dark => colors
            .any(|luminance| luminance > 0.25)
            .then_some("Light colors under a dark theme make text hard to read."),
        ThemeVariant::Light => colors
            .any(|luminance| luminance < 0.35)
            .then_some("Dark colors under a light theme make text hard to read."),
    }
}

/// A starting gradient: CSS angle and (`0xRRGGBB`, position) stops.
pub struct Preset {
    pub name: &'static str,
    pub angle: u16,
    pub stops: &'static [(u32, u8)],
}
const DARK_PRESETS: [Preset; 5] = [
    Preset {
        name: "Aurora",
        angle: 135,
        stops: &[(0x0f2027, 0), (0x203a43, 55), (0x2c5364, 100)],
    },
    Preset {
        name: "Dusk",
        angle: 160,
        stops: &[(0x1f1c2c, 0), (0x3a2a4d, 60), (0x5b3a5e, 100)],
    },
    Preset {
        name: "Ocean",
        angle: 180,
        stops: &[(0x0b1d2e, 0), (0x103a4f, 100)],
    },
    Preset {
        name: "Forest",
        angle: 150,
        stops: &[(0x0f1f17, 0), (0x1d3b2a, 100)],
    },
    Preset {
        name: "Ember",
        angle: 120,
        stops: &[(0x1c1414, 0), (0x3b1d1a, 60), (0x55251c, 100)],
    },
];
const LIGHT_PRESETS: [Preset; 5] = [
    Preset {
        name: "Mist",
        angle: 135,
        stops: &[(0xf3f6fa, 0), (0xe6ecf4, 55), (0xdde5ef, 100)],
    },
    Preset {
        name: "Dawn",
        angle: 160,
        stops: &[(0xfdf3ea, 0), (0xf8e5dc, 60), (0xf2dae0, 100)],
    },
    Preset {
        name: "Lagoon",
        angle: 180,
        stops: &[(0xedf7f8, 0), (0xd6ecf0, 100)],
    },
    Preset {
        name: "Sage",
        angle: 150,
        stops: &[(0xf1f5ee, 0), (0xdde9d9, 100)],
    },
    Preset {
        name: "Lilac",
        angle: 120,
        stops: &[(0xf6f2fb, 0), (0xebe3f5, 60), (0xe2d8f1, 100)],
    },
];
/// Presets that suit `variant`.
pub fn presets(variant: ThemeVariant) -> &'static [Preset] {
    match variant {
        ThemeVariant::Dark => &DARK_PRESETS,
        ThemeVariant::Light => &LIGHT_PRESETS,
    }
}

/// Eight directions for the buttons: CSS angle and accessible name.
const DIRECTIONS: [(u16, &str); 8] = [
    (315, "Towards top left"),
    (0, "Towards top"),
    (45, "Towards top right"),
    (90, "Towards right"),
    (270, "Towards left"),
    (225, "Towards bottom left"),
    (180, "Towards bottom"),
    (135, "Towards bottom right"),
];
const DIAL: f32 = 56.;
const KNOB_RADIUS: f32 = 20.;
const LABEL_WIDTH: f32 = 88.;

pub enum GradientEditorEvent {
    /// A drag moved: show this gradient, do not persist it yet.
    Preview(Gradient),
    /// A settled edit to persist.
    Change(Gradient),
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Stop,
    Dial,
}

pub struct GradientEditor {
    model: GradientModel,
    variant: ThemeVariant,
    drag: Option<Drag>,
    /// The gradient when the drag started; mouse up persists only a change.
    drag_start: Option<Gradient>,
    bar: Rc<Cell<Bounds<Pixels>>>,
    dial: Rc<Cell<Bounds<Pixels>>>,
    picker: Entity<ColorPickerState>,
    hex: Entity<InputState>,
    position: Entity<InputState>,
    angle: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<GradientEditorEvent> for GradientEditor {}

impl GradientEditor {
    pub fn new(
        gradient: &Gradient,
        variant: ThemeVariant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let model = GradientModel::new(gradient);
        let stop = model.selected_stop();
        let picker =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla_of(stop.color)));
        let digits = |limit: usize| {
            move |text: &str, _: &mut App| {
                text.len() <= limit && text.chars().all(|c| c.is_ascii_digit())
            }
        };
        let hex = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(stop.color.to_hex())
                .validate(|text, _| {
                    let hex = text.strip_prefix('#').unwrap_or(text);
                    hex.len() <= 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
                })
        });
        let position = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(stop.position.to_string())
                .validate(digits(3))
        });
        let angle = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(model.angle().to_string())
                .validate(digits(3))
        });
        let commit =
            |event: &InputEvent| matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur);
        let subscriptions = vec![
            cx.subscribe_in(&picker, window, |this, _, event, window, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                let color = hex_of(*color);
                if this.model.selected_stop().color != color {
                    this.model.set_selected_color(color);
                    this.changed(window, cx);
                }
            }),
            cx.subscribe_in(&hex, window, move |this, input, event, window, cx| {
                if !commit(event) {
                    return;
                }
                let text = input.read(cx).value();
                let text = text.trim();
                let parsed = HexColor::parse(&format!("#{}", text.trim_start_matches('#')));
                match parsed {
                    Some(color) if color != this.model.selected_stop().color => {
                        this.model.set_selected_color(color);
                        this.changed(window, cx);
                    }
                    _ => this.sync_fields(true, window, cx),
                }
            }),
            cx.subscribe_in(&position, window, move |this, input, event, window, cx| {
                if !commit(event) {
                    return;
                }
                match input.read(cx).value().trim().parse::<u16>() {
                    Ok(value) if value.min(100) as u8 != this.model.selected_stop().position => {
                        this.model.move_selected(value.min(100) as f32);
                        this.changed(window, cx);
                    }
                    _ => this.sync_fields(true, window, cx),
                }
            }),
            cx.subscribe_in(&angle, window, move |this, input, event, window, cx| {
                if !commit(event) {
                    return;
                }
                match input.read(cx).value().trim().parse::<i32>() {
                    Ok(value) if value.rem_euclid(360) as u16 != this.model.angle() => {
                        this.model.set_angle(value);
                        this.changed(window, cx);
                    }
                    _ => this.sync_fields(true, window, cx),
                }
            }),
        ];
        Self {
            model,
            variant,
            drag: None,
            drag_start: None,
            bar: Rc::default(),
            dial: Rc::default(),
            picker,
            hex,
            position,
            angle,
            _subscriptions: subscriptions,
        }
    }

    /// Loads `gradient` (another surface or appearance, or a committed value)
    /// unless it is already the draft or a drag is in progress.
    pub fn set_gradient(
        &mut self,
        gradient: &Gradient,
        variant: ThemeVariant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.variant != variant {
            self.variant = variant;
            cx.notify();
        }
        if self.drag.is_some() || self.model.gradient() == *gradient {
            return;
        }
        let selected = self.model.selected();
        self.model = GradientModel::new(gradient);
        self.model.select(selected);
        self.sync_fields(false, window, cx);
        cx.notify();
    }

    /// Writes the selected stop and angle into the fields; a field being typed
    /// in keeps its text unless `force`.
    fn sync_fields(&mut self, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let stop = self.model.selected_stop();
        for (field, text) in [
            (&self.hex, stop.color.to_hex()),
            (&self.position, stop.position.to_string()),
            (&self.angle, self.model.angle().to_string()),
        ] {
            let focused = field.read(cx).focus_handle(cx).is_focused(window);
            if (force || !focused) && field.read(cx).value().as_ref() != text {
                field.update(cx, |field, cx| field.set_value(text, window, cx));
            }
        }
        let color = hsla_of(stop.color);
        if self.picker.read(cx).value().map(hex_of) != Some(stop.color) {
            self.picker
                .update(cx, |picker, cx| picker.set_value(color, window, cx));
        }
    }

    /// A settled edit: refresh the fields and hand the gradient to the owner.
    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_fields(false, window, cx);
        cx.emit(GradientEditorEvent::Change(self.model.gradient()));
        cx.notify();
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.selected() != index {
            self.model.select(index);
            self.sync_fields(true, window, cx);
            cx.notify();
        }
    }

    fn begin_drag(&mut self, drag: Drag, start: Gradient) {
        self.drag = Some(drag);
        self.drag_start = Some(start);
    }

    fn drag_to(
        &mut self,
        position: Point<Pixels>,
        snap: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.model.gradient();
        match self.drag {
            Some(Drag::Stop) => {
                let bar = self.bar.get();
                if bar.size.width > px(0.) {
                    let x = f32::from(position.x - bar.left()) / f32::from(bar.size.width);
                    self.model.move_selected(x * 100.);
                }
            }
            Some(Drag::Dial) => {
                let dial = self.dial.get();
                self.model
                    .set_angle(angle_towards(dial.center(), position, snap) as i32);
            }
            None => return,
        }
        if self.model.gradient() != before {
            self.sync_fields(false, window, cx);
            cx.emit(GradientEditorEvent::Preview(self.model.gradient()));
            cx.notify();
        }
    }

    fn end_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.drag.take().is_none() {
            return;
        }
        if self.drag_start.take().as_ref() != Some(&self.model.gradient()) {
            self.changed(window, cx);
        } else {
            cx.notify();
        }
    }

    /// Arrow keys move the focused stop (Shift: 10%), Home/End to the ends,
    /// Delete or Backspace removes it.
    fn stop_key(
        &mut self,
        index: usize,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = if event.keystroke.modifiers.shift {
            10
        } else {
            1
        };
        self.select(index, window, cx);
        match event.keystroke.key.as_str() {
            "left" | "down" => self.model.nudge_selected(-step),
            "right" | "up" => self.model.nudge_selected(step),
            "home" => self.model.move_selected(0.),
            "end" => self.model.move_selected(100.),
            "delete" | "backspace" => {
                if !self.model.remove_selected() {
                    return;
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        self.changed(window, cx);
    }

    /// Arrow keys turn the dial by 1° (Shift: 15°).
    fn dial_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let step = if event.keystroke.modifiers.shift {
            15
        } else {
            1
        };
        let angle = self.model.angle() as i32;
        let angle = match event.keystroke.key.as_str() {
            "left" | "down" => angle - step,
            "right" | "up" => angle + step,
            _ => return,
        };
        cx.stop_propagation();
        self.model.set_angle(angle);
        self.changed(window, cx);
    }

    fn render_bar(&self, cx: &mut Context<Self>) -> Div {
        let mut stops = self.model.gradient().sorted_stops();
        let colors: Vec<_> = stops
            .drain(..)
            .map(|stop| (hsla_of(stop.color), stop.position as f32 / 100.))
            .collect();
        let bar_bounds = self.bar.clone();
        let mut order: Vec<usize> = (0..self.model.stops().len()).collect();
        order.sort_by_key(|&index| self.model.stops()[index].position);
        let count = self.model.stops().len();
        div()
            .relative()
            .pb(px(14.))
            .child(
                div()
                    .id("gradient-bar")
                    .relative()
                    .h(px(28.))
                    .rounded(px(6.))
                    .cursor(CursorStyle::Crosshair)
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new("Click to add a color")
                            .build(window, cx)
                    })
                    .child(
                        paint_canvas(SurfacePaint::Gradient(gradient_fills(90., &colors)), px(6.))
                            .absolute()
                            .inset_0(),
                    )
                    .child(
                        canvas(move |bounds, _, _| bar_bounds.set(bounds), |_, _, _, _| {})
                            .absolute()
                            .inset_0(),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(t::gradient_bar_edge()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            let bar = this.bar.get();
                            if !this.model.can_add() || bar.size.width <= px(0.) {
                                return;
                            }
                            let start = this.model.gradient();
                            let x = f32::from(event.position.x - bar.left())
                                / f32::from(bar.size.width);
                            this.model.add_at(x * 100.);
                            this.begin_drag(Drag::Stop, start);
                            this.sync_fields(true, window, cx);
                            cx.emit(GradientEditorEvent::Preview(this.model.gradient()));
                            cx.notify();
                        }),
                    ),
            )
            .children(order.into_iter().map(|index| {
                let stop = self.model.stops()[index];
                let selected = index == self.model.selected();
                let size = if selected { 20. } else { 16. };
                div()
                    .id(("gradient-stop", index))
                    .absolute()
                    .top(px(if selected { 16. } else { 18. }))
                    .left(relative(stop.position as f32 / 100.))
                    .ml(px(-size / 2.))
                    .size(px(size))
                    .rounded_full()
                    .bg(hsla_of(stop.color))
                    .border_2()
                    .border_color(t::gradient_handle_ring())
                    .shadow(vec![BoxShadow {
                        inset: false,
                        color: t::gradient_handle_shadow(),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(3.),
                        spread_radius: px(0.),
                    }])
                    .when(selected, |handle| {
                        handle.shadow(vec![
                            BoxShadow {
                                inset: false,
                                color: t::accent(),
                                offset: point(px(0.), px(0.)),
                                blur_radius: px(0.),
                                spread_radius: px(2.),
                            },
                            BoxShadow {
                                inset: false,
                                color: t::gradient_handle_shadow(),
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(3.),
                                spread_radius: px(0.),
                            },
                        ])
                    })
                    .cursor(if self.drag == Some(Drag::Stop) {
                        CursorStyle::ClosedHand
                    } else {
                        CursorStyle::OpenHand
                    })
                    .role(Role::Slider)
                    .aria_label(format!(
                        "Color {} of {count}, {}",
                        index + 1,
                        stop.color.to_hex()
                    ))
                    .aria_numeric_value(stop.position as f64)
                    .aria_min_numeric_value(0.)
                    .aria_max_numeric_value(100.)
                    .tab_index(0)
                    .focus_visible(|style| style.border_color(t::accent()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.select(index, window, cx);
                            this.begin_drag(Drag::Stop, this.model.gradient());
                            cx.notify();
                        }),
                    )
                    .on_key_down(cx.listener(move |this, event, window, cx| {
                        this.stop_key(index, event, window, cx)
                    }))
            }))
    }

    fn render_stop_row(&self, cx: &mut Context<Self>) -> Div {
        row()
            .flex_wrap()
            .gap(px(8.))
            .child(label("Selected color"))
            .child(
                ColorPicker::new(&self.picker)
                    .accessibility_label("Color of the selected stop")
                    .small(),
            )
            .child(
                input(&self.hex)
                    .w(px(90.))
                    .font_family(t::MONO)
                    .text_size(px(12.)),
            )
            .child(
                row()
                    .gap(px(4.))
                    .child(
                        input(&self.position)
                            .w(px(56.))
                            .font_family(t::MONO)
                            .text_size(px(12.)),
                    )
                    .child(unit("%")),
            )
            .child(
                icon_button("gradient-remove-stop", IconName::Delete, "Remove color")
                    .size(px(28.))
                    .min_w(px(28.))
                    .max_w(px(28.))
                    .min_h(px(28.))
                    .max_h(px(28.))
                    .disabled(!self.model.can_remove())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.model.remove_selected() {
                            this.sync_fields(true, window, cx);
                            this.changed(window, cx);
                        }
                    })),
            )
            .child(div().flex_1())
            .child(
                button("gradient-add-stop", "Add color")
                    .icon(icon(IconName::Plus))
                    .disabled(!self.model.can_add())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.model.add_in_largest_gap() {
                            this.sync_fields(true, window, cx);
                            this.changed(window, cx);
                        }
                    })),
            )
    }

    fn render_direction(&self, cx: &mut Context<Self>) -> Div {
        let angle = self.model.angle();
        let radians = (angle as f32 - 90.).to_radians();
        let knob = point(
            DIAL / 2. + KNOB_RADIUS * radians.cos(),
            DIAL / 2. + KNOB_RADIUS * radians.sin(),
        );
        let dial_bounds = self.dial.clone();
        row()
            .flex_wrap()
            .gap(px(14.))
            .child(label("Direction"))
            .child(
                div()
                    .id("gradient-dial")
                    .relative()
                    .flex_shrink_0()
                    .size(px(DIAL))
                    .rounded_full()
                    .border_1()
                    .border_color(t::control_border())
                    .bg(t::input_bg())
                    .role(Role::Slider)
                    .aria_label("Gradient angle")
                    .aria_numeric_value(angle as f64)
                    .aria_min_numeric_value(0.)
                    .aria_max_numeric_value(359.)
                    .tab_index(0)
                    .focus_visible(|style| style.border_color(t::accent()))
                    .child(
                        canvas(move |bounds, _, _| dial_bounds.set(bounds), |_, _, _, _| {})
                            .absolute()
                            .inset_0(),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(DIAL / 2. - 3.))
                            .top(px(DIAL / 2. - 3.))
                            .size(px(6.))
                            .rounded_full()
                            .bg(t::control_border()),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(knob.x - 5.))
                            .top(px(knob.y - 5.))
                            .size(px(10.))
                            .rounded_full()
                            .bg(t::accent())
                            .shadow(vec![BoxShadow {
                                inset: false,
                                color: t::accent().opacity(0.2),
                                offset: point(px(0.), px(0.)),
                                blur_radius: px(0.),
                                spread_radius: px(3.),
                            }]),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.begin_drag(Drag::Dial, this.model.gradient());
                            this.drag_to(event.position, event.modifiers.shift, window, cx);
                        }),
                    )
                    .on_key_down(
                        cx.listener(|this, event, window, cx| this.dial_key(event, window, cx)),
                    ),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(4)
                    .gap(px(4.))
                    .children(DIRECTIONS.iter().map(|&(direction, name)| {
                        let active = direction == angle;
                        Button::new(("gradient-direction", direction as usize))
                            .ghost()
                            .small()
                            .compact()
                            .size(px(26.))
                            .p_0()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(if active { t::accent() } else { t::border() })
                            .when(active, |button| button.bg(t::accent().opacity(0.08)))
                            .accessibility_label(format!("{name} ({direction}°)"))
                            .tooltip(format!("{direction}°"))
                            .child(
                                icon(IconName::ArrowUp)
                                    .text_color(if active { t::accent() } else { t::muted() })
                                    .rotate(radians_of(direction)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.model.angle() != direction {
                                    this.model.set_angle(direction as i32);
                                    this.changed(window, cx);
                                }
                            }))
                    })),
            )
            .child(
                row()
                    .gap(px(4.))
                    .child(
                        input(&self.angle)
                            .w(px(56.))
                            .font_family(t::MONO)
                            .text_size(px(12.)),
                    )
                    .child(unit("°")),
            )
    }

    fn render_presets(&self, cx: &mut Context<Self>) -> Div {
        row()
            .flex_wrap()
            .gap(px(8.))
            .child(label("Presets"))
            .children(
                presets(self.variant)
                    .iter()
                    .enumerate()
                    .map(|(index, preset)| {
                        let stops: Vec<_> = preset
                            .stops
                            .iter()
                            .map(|&(color, position)| (rgb(color).into(), position as f32 / 100.))
                            .collect();
                        Button::new(("gradient-preset", index))
                            .ghost()
                            .small()
                            .compact()
                            .h(px(26.))
                            .pl(px(4.))
                            .pr(px(8.))
                            .rounded(px(13.))
                            .border_1()
                            .border_color(t::border())
                            .text_size(px(12.))
                            .text_color(t::secondary())
                            .child(
                                row()
                                    .gap(px(6.))
                                    .child(
                                        paint_canvas(
                                            SurfacePaint::Gradient(gradient_fills(
                                                preset.angle as f32,
                                                &stops,
                                            )),
                                            px(9.),
                                        )
                                        .size(px(18.))
                                        .flex_shrink_0(),
                                    )
                                    .child(preset.name),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.model.apply(&presets(this.variant)[index]);
                                this.sync_fields(true, window, cx);
                                this.changed(window, cx);
                            }))
                    }),
            )
    }
}

fn radians_of(degrees: u16) -> Radians {
    gpui_kit::radians((degrees as f32).to_radians())
}
fn label(text: &'static str) -> Div {
    div()
        .w(px(LABEL_WIDTH))
        .flex_shrink_0()
        .text_size(px(11.))
        .text_color(t::muted())
        .child(text)
}
fn unit(text: &'static str) -> Div {
    div().text_size(px(12.)).text_color(t::muted()).child(text)
}

impl Render for GradientEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let warning = readability_warning(
            self.model.stops().iter().map(|stop| stop.color),
            self.variant,
        );
        let editor = cx.entity().downgrade();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(14.))
            .rounded(px(8.))
            .border_1()
            .border_color(t::border())
            .bg(t::panel())
            .child(self.render_bar(cx))
            .child(self.render_stop_row(cx))
            .child(self.render_direction(cx))
            .child(self.render_presets(cx))
            .children(warning.map(|warning| {
                row()
                    .items_start()
                    .gap(px(8.))
                    .text_size(px(11.))
                    .line_height(px(15.))
                    .text_color(t::yellow())
                    .child(
                        icon(IconName::TriangleAlert)
                            .size(px(13.))
                            .text_color(t::yellow()),
                    )
                    .child(warning)
            }))
            // While dragging, follow the pointer anywhere in the window, as a
            // pointer capture would, until the button is released.
            .when(self.drag.is_some(), |element| {
                element.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let moved = editor.clone();
                            window.on_mouse_event(
                                move |event: &MouseMoveEvent, phase, window, cx| {
                                    if phase != DispatchPhase::Bubble {
                                        return;
                                    }
                                    let _ = moved.update(cx, |this, cx| {
                                        if event.pressed_button == Some(MouseButton::Left) {
                                            this.drag_to(
                                                event.position,
                                                event.modifiers.shift,
                                                window,
                                                cx,
                                            );
                                        } else {
                                            this.end_drag(window, cx);
                                        }
                                    });
                                },
                            );
                            let released = editor.clone();
                            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                                if phase == DispatchPhase::Bubble {
                                    let _ =
                                        released.update(cx, |this, cx| this.end_drag(window, cx));
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` brings GPUI's `test` attribute; these are plain tests.
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext};

    fn model(stops: &[(u32, u8)]) -> GradientModel {
        GradientModel::new(
            &Gradient::new(
                90,
                stops
                    .iter()
                    .map(|&(color, position)| GradientStop {
                        color: HexColor(color),
                        position,
                    })
                    .collect(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn stops_are_added_with_the_interpolated_color_up_to_the_limit() {
        let mut m = model(&[(0x000000, 0), (0xfefefe, 100)]);
        assert!(m.add_at(50.));
        assert_eq!(m.selected(), 2);
        assert_eq!(m.selected_stop().color, HexColor(0x7f7f7f));
        assert_eq!(m.selected_stop().position, 50);
        // The widest gap is split in its middle.
        assert!(m.add_in_largest_gap());
        assert_eq!(m.selected_stop().position, 25);
        assert!(m.add_at(140.));
        assert_eq!(m.selected_stop().position, 100);
        assert!(m.add_at(-5.));
        assert_eq!(m.selected_stop().position, 0);
        assert_eq!(m.stops().len(), GRADIENT_MAX_STOPS);
        assert!(!m.can_add() && !m.add_at(10.) && !m.add_in_largest_gap());
        assert_eq!(m.stops().len(), GRADIENT_MAX_STOPS);
        // The draft always converts to a valid stored gradient.
        assert_eq!(m.gradient().stops().len(), GRADIENT_MAX_STOPS);
    }

    #[test]
    fn removal_keeps_two_stops_and_selects_the_nearest() {
        let mut m = model(&[(0x111111, 0), (0x222222, 40), (0x333333, 100)]);
        m.select(1);
        assert!(m.remove_selected());
        // 0 is nearer to the removed 40 than 100 is.
        assert_eq!(m.selected_stop().position, 0);
        assert!(!m.can_remove() && !m.remove_selected());
        assert_eq!(m.stops().len(), GRADIENT_MIN_STOPS);
    }

    #[test]
    fn dragging_and_keys_clamp_the_selected_stop() {
        let mut m = model(&[(0x111111, 10), (0x222222, 90)]);
        m.select(0);
        m.move_selected(-30.);
        assert_eq!(m.selected_stop().position, 0);
        m.move_selected(42.6);
        assert_eq!(m.selected_stop().position, 43);
        m.move_selected(250.);
        assert_eq!(m.selected_stop().position, 100);
        m.move_selected(f32::NAN);
        assert_eq!(m.selected_stop().position, 100);
        m.nudge_selected(1);
        assert_eq!(m.selected_stop().position, 100);
        m.nudge_selected(-10);
        assert_eq!(m.selected_stop().position, 90);
        m.move_selected(5.);
        m.nudge_selected(-10);
        assert_eq!(m.selected_stop().position, 0);
        // A stop may cross another; painting sorts, the user's order is kept.
        m.move_selected(95.);
        assert_eq!(m.stops()[0].position, 95);
        assert_eq!(m.gradient().sorted_stops()[0].position, 90);
    }

    #[test]
    fn angles_wrap_and_the_dial_points_and_snaps() {
        let mut m = model(&[(0x111111, 0), (0x222222, 100)]);
        m.set_angle(-15);
        assert_eq!(m.angle(), 345);
        m.set_angle(725);
        assert_eq!(m.angle(), 5);
        let center = point(px(28.), px(28.));
        assert_eq!(angle_towards(center, point(px(28.), px(0.)), false), 0);
        assert_eq!(angle_towards(center, point(px(56.), px(28.)), false), 90);
        assert_eq!(angle_towards(center, point(px(28.), px(56.)), false), 180);
        assert_eq!(angle_towards(center, point(px(0.), px(28.)), false), 270);
        // 34° snaps to 30°, 353° to 0°.
        let at = |degrees: f32| {
            let r = (degrees - 90.).to_radians();
            point(px(28. + 20. * r.cos()), px(28. + 20. * r.sin()))
        };
        assert_eq!(angle_towards(center, at(34.), false), 34);
        assert_eq!(angle_towards(center, at(34.), true), 30);
        assert_eq!(angle_towards(center, at(353.), true), 0);
    }

    #[gpui_kit::test]
    fn the_keyboard_moves_and_removes_the_focused_stop(cx: &mut TestAppContext) {
        let gradient = model(&[(0x111111, 0), (0x222222, 50), (0x333333, 100)]).gradient();
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::super::theme::init(cx);
            cx.open_window(Default::default(), |window, cx| {
                cx.new(|cx| GradientEditor::new(&gradient, ThemeVariant::Dark, window, cx))
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let editor = window.root(&mut cx).unwrap();
        let changes = Rc::new(Cell::new(0));
        let seen = changes.clone();
        cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event, _| {
                if matches!(event, GradientEditorEvent::Change(_)) {
                    seen.set(seen.get() + 1);
                }
            })
            .detach()
        });
        cx.run_until_parked();
        // Tab reaches the stop handles first, in position order.
        cx.update(|window, cx| window.focus_next(cx));
        cx.simulate_keystrokes("right shift-right");
        let stops = |cx: &mut VisualTestContext| {
            editor.read_with(cx, |editor, _| {
                editor
                    .model
                    .stops()
                    .iter()
                    .map(|stop| stop.position)
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(stops(&mut cx), [11, 50, 100]);
        assert_eq!(changes.get(), 2);
        cx.simulate_keystrokes("shift-left shift-left");
        assert_eq!(stops(&mut cx)[0], 0, "clamped at the start");
        cx.simulate_keystrokes("delete");
        assert_eq!(stops(&mut cx).len(), 2);
        // Two stops remain: Delete does nothing and persists nothing.
        let before = changes.get();
        cx.simulate_keystrokes("delete");
        assert_eq!(stops(&mut cx).len(), 2);
        assert_eq!(changes.get(), before);
    }

    #[test]
    fn readability_flags_colors_that_fight_the_variant() {
        let dark = [HexColor(0x141a26), HexColor(0x2a2240)];
        assert_eq!(readability_warning(dark, ThemeVariant::Dark), None);
        assert!(readability_warning(dark, ThemeVariant::Light).is_some());
        let light = [HexColor(0xf6f8fc), HexColor(0xdde5ef)];
        assert_eq!(readability_warning(light, ThemeVariant::Light), None);
        assert!(readability_warning(light, ThemeVariant::Dark).is_some());
        // One bad stop is enough.
        assert!(readability_warning([dark[0], HexColor(0xffffff)], ThemeVariant::Dark).is_some());
        // Presets suit the appearance they are offered for.
        for variant in ThemeVariant::ALL {
            for preset in presets(variant) {
                let colors = preset.stops.iter().map(|&(color, _)| HexColor(color));
                assert_eq!(
                    readability_warning(colors, variant),
                    None,
                    "{}",
                    preset.name
                );
                let mut m = model(&[(0, 0), (0, 100)]);
                m.apply(preset);
                assert_eq!(m.angle(), preset.angle);
            }
        }
    }
}
