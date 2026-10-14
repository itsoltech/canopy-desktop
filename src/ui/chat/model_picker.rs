//! The composer's model and reasoning picker, after ChatGPT's: one popover whose first
//! page sets the reasoning effort on a slider. Its model name turns the page into the
//! account's models (and the other accounts); choosing one glides back to the slider.
use super::{composer::provider_icon, view::ChatView};
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::config::ModelInfo;
use canopy_desktop::motion::{self, Transition, presets};
use gpui_kit::base::Disableable;
use gpui_kit::component::{
    IconName, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    popover::Popover,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{cell::Cell, rc::Rc, time::Instant};

const WIDTH: f32 = 272.;
const PAD: f32 = 12.;
const KNOB: f32 = 28.;
const TRACK: f32 = 24.;
const DOT: f32 = 4.;
/// The page slides this far as it changes.
const SHIFT: f32 = 14.;
/// The effort's name moves this far as it changes.
const LABEL_SHIFT: f32 = 8.;
const LABEL_HEIGHT: f32 = 20.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Effort,
    Models,
}

pub(super) struct ModelPicker {
    pub open: bool,
    page: Page,
    /// Eased 0..1 since the page changed; the panel's height goes from `from` to the
    /// new page's measured height and the page slides in (from the right when going to
    /// the models).
    turn: Transition,
    forward: bool,
    from: Pixels,
    measured: Rc<Cell<Pixels>>,
    /// The stop under the knob while the slider is held; set on release.
    dragging: Option<usize>,
    /// The knob's eased position, in stops, and where it is heading.
    knob: Transition,
    knob_target: f32,
    /// The knob and fill fade in once an effort is known (none is before the first
    /// choice when the agent doesn't say its default).
    shown: Transition,
    shown_target: f32,
    track: Rc<Cell<Bounds<Pixels>>>,
    /// The effort's name: the new one rises in as the old one leaves (falls when the
    /// effort goes down).
    label: SharedString,
    label_stop: Option<usize>,
    label_from: Option<SharedString>,
    label_turn: Transition,
    label_up: bool,
    /// Clock of the light travelling through the filled track.
    epoch: Instant,
    /// The panel takes focus when the popover opens, for ←/→ on the slider.
    focus: FocusHandle,
}

impl ModelPicker {
    pub fn new(now: Instant, cx: &mut App) -> Self {
        Self {
            focus: cx.focus_handle(),
            open: false,
            page: Page::Effort,
            turn: Transition::new(1., now),
            forward: true,
            from: px(0.),
            measured: Default::default(),
            dragging: None,
            knob: Transition::new(0., now),
            knob_target: 0.,
            shown: Transition::new(0., now),
            shown_target: 0.,
            track: Default::default(),
            label: SharedString::default(),
            label_stop: None,
            label_from: None,
            label_turn: Transition::new(1., now),
            label_up: true,
            epoch: now,
        }
    }

    fn height(&self, now: Instant) -> Option<Pixels> {
        if !self.turn.is_animating(now) {
            return None;
        }
        let to = self.measured.get();
        Some(self.from + (to - self.from) * self.turn.value(now))
    }

    /// Glides the knob to `stop`; one appearing starts there instead.
    fn move_knob(&mut self, stop: Option<usize>, policy: motion::MotionPolicy, now: Instant) {
        let shown = if stop.is_some() { 1. } else { 0. };
        if let Some(stop) = stop.map(|stop| stop as f32) {
            if self.shown_target == 0. {
                self.knob = Transition::new(stop, now);
                self.knob_target = stop;
            } else if self.knob_target != stop {
                self.knob_target = stop;
                self.knob.retarget(stop, presets::STATE_CHANGE, now, policy);
            }
        }
        if self.shown_target != shown {
            self.shown_target = shown;
            self.shown
                .retarget(shown, presets::STATE_CHANGE, now, policy);
        }
    }

    /// Puts the knob, its fill and the name at `stop` at once, as the popover opens.
    fn place(&mut self, stop: Option<usize>, now: Instant) {
        let at = stop.unwrap_or_default() as f32;
        let shown = if stop.is_some() { 1. } else { 0. };
        self.knob = Transition::new(at, now);
        self.knob_target = at;
        self.shown = Transition::new(shown, now);
        self.shown_target = shown;
        self.label = SharedString::default();
        self.label_from = None;
        self.label_turn = Transition::new(1., now);
    }

    /// Starts the name's change when it differs from the one shown.
    fn show_label(
        &mut self,
        label: SharedString,
        stop: Option<usize>,
        policy: motion::MotionPolicy,
        now: Instant,
    ) {
        if self.label == label {
            return;
        }
        let old = std::mem::replace(&mut self.label, label);
        let from = std::mem::replace(&mut self.label_stop, stop);
        if old.is_empty() {
            return;
        }
        self.label_up = stop.unwrap_or_default() >= from.unwrap_or_default();
        self.label_from = Some(old);
        self.label_turn = Transition::new(0., now);
        self.label_turn
            .retarget(1., presets::STATE_CHANGE, now, policy);
    }
}

/// The level the agent uses now: the chosen one, else its default when it says it.
fn active_stop(
    model: &ModelInfo,
    chosen: Option<canopy_desktop::chat::config::Effort>,
) -> Option<usize> {
    let effort = chosen.or(model.default_effort)?;
    model
        .effort_levels
        .iter()
        .position(|level| *level == effort)
}

/// What the model list says under "Default": the model it stands for.
fn default_note(label: &str) -> String {
    match label
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
    {
        Some(model) => format!("Recommended · {model}"),
        None => "The account's recommended model".to_owned(),
    }
}

impl ChatView {
    /// The composer's trigger: the model (and effort) named as `density` allows.
    pub(super) fn model_picker(
        &self,
        density: super::composer::Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use super::composer::Density;
        let (account, model) = self.selection_label(cx);
        let effort = self.effort_label();
        let name = match &effort {
            Some(effort) => format!("{account} · {model} · {effort}"),
            None => format!("{account} · {model}"),
        };
        let open = self.model_picker.open;
        let foreground = t::muted();
        let trigger = Button::new("chat-model")
            .custom(
                ButtonCustomVariant::new(cx)
                    .foreground(foreground)
                    .hover(t::hover())
                    .active(t::hover()),
            )
            .xsmall()
            .compact()
            .h(px(26.))
            .px(px(8.))
            .rounded(px(13.))
            .border_0()
            .when(open, |button| button.bg(t::hover()))
            .accessibility_label(name.clone())
            .when(density != Density::Full || !open, |button| {
                button.tooltip(if density == Density::Full {
                    "Model and reasoning".to_owned()
                } else {
                    name.clone()
                })
            })
            .child(
                row()
                    .gap(px(5.))
                    .text_size(px(12.))
                    .child(provider_icon(self.provider()))
                    .when(density != Density::Icons, |row| {
                        row.child(div().text_color(t::secondary()).child(model))
                    })
                    .when(density == Density::Full, |row| {
                        row.children(effort.map(|e| div().text_color(foreground).child(e)))
                    })
                    .child(
                        icon(IconName::ChevronDown)
                            .size(px(10.))
                            .text_color(foreground),
                    ),
            );
        let view = cx.entity().downgrade();
        let content = view.clone();
        Popover::new("chat-model-picker")
            .anchor(Anchor::BottomLeft)
            .offset(px(8.))
            .open(open)
            .on_open_change(move |open, _, cx| {
                let _ = view.update(cx, |this, cx| this.set_model_picker_open(*open, cx));
            })
            .trigger(trigger)
            .track_focus(&self.model_picker.focus)
            .p_0()
            .rounded(px(16.))
            .bg(t::elevated())
            .border_color(t::control_border())
            .content(move |_, window, cx| {
                content
                    .upgrade()
                    .map(|view| view.update(cx, |this, cx| this.model_picker_panel(window, cx)))
                    .unwrap_or_else(|| div().into_any_element())
            })
    }

    fn set_model_picker_open(&mut self, open: bool, cx: &mut Context<Self>) {
        let now = Instant::now();
        let effort = self.current_model().filter(ModelInfo::supports_effort);
        let stop = effort
            .as_ref()
            .and_then(|model| active_stop(model, self.settings.effort));
        let picker = &mut self.model_picker;
        picker.open = open;
        if open {
            picker.page = if effort.is_some() {
                Page::Effort
            } else {
                Page::Models
            };
            picker.turn = Transition::new(1., now);
            picker.dragging = None;
            picker.place(stop, now);
        }
        cx.notify();
    }

    fn turn_page(&mut self, page: Page, cx: &mut Context<Self>) {
        let now = Instant::now();
        let picker = &mut self.model_picker;
        if picker.page == page {
            return;
        }
        picker.from = picker.height(now).unwrap_or(picker.measured.get());
        picker.forward = page == Page::Models;
        picker.page = page;
        picker.turn = Transition::new(0., now);
        picker
            .turn
            .retarget(1., presets::RESIZE, now, motion::policy(cx));
        cx.notify();
    }

    /// A model from the list: back to its reasoning, or closed when it has none.
    fn pick_model(&mut self, value: Option<String>, cx: &mut Context<Self>) {
        self.set_model(value, cx);
        match self.current_model().filter(ModelInfo::supports_effort) {
            Some(model) => {
                let stop = active_stop(&model, self.settings.effort);
                let policy = motion::policy(cx);
                self.model_picker.move_knob(stop, policy, Instant::now());
                self.turn_page(Page::Effort, cx);
            }
            None => self.set_model_picker_open(false, cx),
        }
    }

    fn model_picker_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let now = Instant::now();
        let picker = &self.model_picker;
        let sheen = picker.page == Page::Effort
            && picker.shown_target > 0.
            && motion::policy(cx) == motion::MotionPolicy::Full;
        let animating = sheen
            || picker.turn.is_animating(now)
            || picker.knob.is_animating(now)
            || picker.shown.is_animating(now)
            || picker.label_turn.is_animating(now);
        motion::request_frame(window, animating);
        let progress = picker.turn.value(now);
        let height = picker.height(now);
        let shift = SHIFT * (1. - progress) * if picker.forward { 1. } else { -1. };
        let measured = picker.measured.clone();
        let page = match (picker.page, self.current_model()) {
            (Page::Effort, Some(model)) if model.supports_effort() => {
                self.effort_page(&model, cx).into_any_element()
            }
            _ => self.models_page(cx).into_any_element(),
        };
        div()
            .id("chat-model-picker-panel")
            .track_focus(&self.model_picker.focus)
            .w(px(WIDTH))
            .flex()
            .flex_col()
            .when_some(height, |panel, height| panel.h(height).overflow_hidden())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let step = match event.keystroke.key.as_str() {
                    "left" => -1,
                    "right" => 1,
                    _ => return,
                };
                if this.model_picker.page == Page::Effort {
                    this.step_effort(step, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex_shrink_0()
                    .left(px(shift))
                    .opacity(progress)
                    .child(page)
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, _, _| measured.set(bounds.size.height),
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
            .into_any_element()
    }

    /// The effort's name, the model to change, Reset, and the slider.
    fn effort_page(&mut self, model: &ModelInfo, cx: &mut Context<Self>) -> Div {
        let now = Instant::now();
        let policy = motion::policy(cx);
        let active = active_stop(model, self.settings.effort);
        let shown = self.model_picker.dragging.or(active);
        if self.model_picker.dragging.is_none() {
            self.model_picker.move_knob(active, policy, now);
        }
        let label: SharedString = match shown {
            Some(stop) => model.effort_levels[stop].label().into(),
            None => "Default".into(),
        };
        self.model_picker.show_label(label, shown, policy, now);
        let last = model.effort_levels.len().saturating_sub(1).max(1) as f32;
        let intensity = self.model_picker.knob.value(now) / last;
        column()
            .w_full()
            .p(px(PAD))
            .gap(px(12.))
            .child(
                row()
                    .items_start()
                    .child(
                        div()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(provider_icon(self.provider())),
                    )
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(px(1.))
                            .child(self.effort_name(intensity, now))
                            .child(
                                row()
                                    .id("chat-model-picker-models")
                                    .max_w_full()
                                    .gap(px(2.))
                                    .px(px(6.))
                                    .rounded(px(6.))
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .text_color(t::secondary())
                                    .hover(|style| style.text_color(t::text()))
                                    .child(div().min_w_0().truncate().child(model.label()))
                                    .child(icon(IconName::ChevronRight).size(px(11.)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.turn_page(Page::Models, cx)
                                    })),
                            ),
                    )
                    .child(
                        Button::new("chat-effort-reset")
                            .ghost()
                            .compact()
                            .size(px(24.))
                            .min_w(px(24.))
                            .max_w(px(24.))
                            .p_0()
                            .rounded(px(12.))
                            .border_0()
                            .accessibility_label("Reset to the default")
                            .tooltip("Reset to the default")
                            .disabled(self.settings.effort.is_none())
                            .child(custom_icon("rotate-ccw").size(px(14.)))
                            .on_click(cx.listener(|this, _, _, cx| this.set_effort(None, cx))),
                    ),
            )
            .child(self.effort_slider(model.effort_levels.len(), cx))
    }

    /// The effort's name, tinted like the fill; a new one replaces the old in motion.
    fn effort_name(&self, intensity: f32, now: Instant) -> Div {
        let picker = &self.model_picker;
        let progress = picker.label_turn.value(now);
        let rise = if picker.label_up { 1. } else { -1. } * LABEL_SHIFT;
        let shown = picker.shown.value(now);
        let color = t::secondary().blend(t::effort(intensity).opacity(shown));
        let line = |text: SharedString, offset: f32, opacity: f32| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(offset))
                .h(px(LABEL_HEIGHT))
                .flex()
                .justify_center()
                .items_center()
                .opacity(opacity)
                .child(text)
        };
        div()
            .relative()
            .w_full()
            .h(px(LABEL_HEIGHT))
            .text_size(px(15.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .children(
                picker
                    .label_from
                    .clone()
                    .filter(|_| progress < 1.)
                    .map(|old| line(old, -rise * progress, 1. - progress)),
            )
            .child(line(picker.label.clone(), rise * (1. - progress), progress))
    }

    /// Press, drag and release anywhere along the track; ←/→ step.
    fn effort_slider(&self, count: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let picker = &self.model_picker;
        let now = Instant::now();
        let span = WIDTH - 2. * PAD - KNOB;
        let last = count.saturating_sub(1).max(1) as f32;
        let at = |stop: f32| KNOB / 2. + span * stop / last;
        let knob = picker.knob.value(now).clamp(0., last);
        let shown = picker.shown.value(now);
        let intensity = knob / last;
        let track = picker.track.clone();
        let dragging = picker.dragging.is_some();
        let view = cx.entity().downgrade();
        let inset = (KNOB - TRACK) / 2.;
        let clock = if motion::policy(cx) == motion::MotionPolicy::Full {
            now.saturating_duration_since(picker.epoch).as_secs_f64()
        } else {
            0.
        };
        let fill = at(knob) + TRACK / 2.;
        div()
            .id("chat-effort-slider")
            .relative()
            .w_full()
            .h(px(KNOB))
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(inset))
                    .h(px(TRACK))
                    .rounded_full()
                    .bg(t::selected()),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(inset))
                    .h(px(TRACK))
                    .w(px(fill))
                    .rounded_full()
                    .opacity(shown)
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(t::effort(0.), 0.),
                        linear_color_stop(t::effort(intensity), 1.),
                    ))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                paint_sheen(bounds, intensity, clock, window)
                            },
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
            .children((0..count).map(|stop| {
                let filled = shown > 0. && (stop as f32) < knob;
                div()
                    .absolute()
                    .left(px(at(stop as f32) - DOT / 2.))
                    .top(px((KNOB - DOT) / 2.))
                    .size(px(DOT))
                    .rounded_full()
                    .bg(if filled {
                        gpui_kit::white().opacity(0.6)
                    } else {
                        t::text().opacity(0.25)
                    })
            }))
            .child(
                div()
                    .absolute()
                    .left(px(at(knob) - KNOB / 2.))
                    .top_0()
                    .size(px(KNOB))
                    .rounded_full()
                    .opacity(shown)
                    .bg(gpui_kit::white())
                    .shadow_md(),
            )
            .child(
                canvas(
                    move |bounds, _, _| track.set(bounds),
                    move |_, _, window, _| {
                        // While held, the whole window follows the pointer.
                        if !dragging {
                            return;
                        }
                        let moved = view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                let _ =
                                    moved.update(cx, |this, cx| this.slider_at(event.position, cx));
                            }
                        });
                        let released = view.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                let _ = released.update(cx, |this, cx| this.slider_release(cx));
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.model_picker.dragging = Some(0);
                    this.slider_at(event.position, cx);
                }),
            )
            // A click released before the held slider repainted.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.slider_release(cx)),
            )
    }

    /// The stop nearest to `position` while the slider is held.
    fn slider_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(model) = self.current_model().filter(ModelInfo::supports_effort) else {
            return;
        };
        if self.model_picker.dragging.is_none() {
            return;
        }
        let count = model.effort_levels.len();
        let bounds = self.model_picker.track.get();
        let span = f32::from(bounds.size.width) - KNOB;
        let x = f32::from(position.x - bounds.left()) - KNOB / 2.;
        let last = count.saturating_sub(1).max(1) as f32;
        let stop = (x / span.max(1.) * last).round().clamp(0., last) as usize;
        let policy = motion::policy(cx);
        self.model_picker.dragging = Some(stop);
        self.model_picker
            .move_knob(Some(stop), policy, Instant::now());
        cx.notify();
    }

    fn slider_release(&mut self, cx: &mut Context<Self>) {
        let Some(stop) = self.model_picker.dragging.take() else {
            return;
        };
        if let Some(effort) = self
            .current_model()
            .and_then(|model| model.effort_levels.get(stop).copied())
            && Some(effort) != self.settings.effort
        {
            self.set_effort(Some(effort), cx);
        }
        cx.notify();
    }

    /// ←/→: the next level; from the unknown default, → starts at the lowest and ←
    /// at the highest.
    fn step_effort(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(model) = self.current_model().filter(ModelInfo::supports_effort) else {
            return;
        };
        let last = model.effort_levels.len() as isize - 1;
        let stop = match active_stop(&model, self.settings.effort) {
            Some(stop) => (stop as isize + step).clamp(0, last),
            None if step > 0 => 0,
            None => last,
        };
        self.set_effort(Some(model.effort_levels[stop as usize]), cx);
    }

    /// The account's models, Default first, then the other accounts before the first
    /// message.
    fn models_page(&self, cx: &mut Context<Self>) -> Div {
        let (accounts, current) = self.accounts(cx);
        let models = self.models();
        let chosen = self.settings.model.clone();
        let account = accounts
            .iter()
            .find(|account| account.id == current)
            .map(|account| account.name.clone())
            .unwrap_or_else(|| self.provider().name().into());
        let back = self
            .current_model()
            .is_some_and(|model| model.supports_effort());
        // The agent is chosen before the first message only (DEC-087).
        let locked = self.account_locked();
        let mut list = column().w_full().child(model_row(
            "chat-model-default",
            "Default".to_owned(),
            Some(default_note(&Self::default_model_label(&models))),
            chosen.is_none(),
            cx.listener(|this, _, _, cx| this.pick_model(None, cx)),
        ));
        for (index, info) in models.iter().filter(|m| m.value != "default").enumerate() {
            let value = info.value.clone();
            list = list.child(model_row(
                ("chat-model-option", index),
                info.label(),
                None,
                chosen.as_deref() == Some(info.value.as_str()),
                cx.listener(move |this, _, _, cx| this.pick_model(Some(value.clone()), cx)),
            ));
        }
        if models.is_empty() {
            list = list.child(
                div()
                    .px(px(10.))
                    .py(px(8.))
                    .text_size(px(13.))
                    .text_color(t::muted())
                    .child("The agent lists its models once it has started."),
            );
        }
        let others: Vec<_> = accounts.iter().filter(|a| a.id != current).collect();
        if !locked && !others.is_empty() {
            list = list
                .child(div().mx(px(10.)).my(px(6.)).h(px(1.)).bg(t::border()))
                .child(section_label("Accounts"));
            for (index, other) in others.into_iter().enumerate() {
                let (provider, id) = (other.provider, other.id.clone());
                list = list.child(model_row(
                    ("chat-model-account", index),
                    format!("Use {} (restarts this chat)", other.name),
                    None,
                    false,
                    cx.listener(move |this, _, _, cx| {
                        this.set_model_picker_open(false, cx);
                        this.set_account(provider, id.clone(), cx);
                    }),
                ));
            }
        }
        column()
            .w_full()
            .p(px(6.))
            .child(
                row()
                    .h(px(32.))
                    .px(px(4.))
                    .gap(px(4.))
                    .children(back.then(|| {
                        icon_button("chat-model-picker-back", IconName::ChevronLeft, "Back")
                            .on_click(
                                cx.listener(|this, _, _, cx| this.turn_page(Page::Effort, cx)),
                            )
                    }))
                    .child(
                        div()
                            .px(px(if back { 0. } else { 6. }))
                            .text_size(px(13.))
                            .text_color(t::muted())
                            .truncate()
                            .child(format!("{account} models")),
                    ),
            )
            .child(
                div()
                    .id("chat-model-list")
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .child(list),
            )
    }
}

fn section_label(text: &'static str) -> Div {
    div()
        .px(px(10.))
        .py(px(4.))
        .text_size(px(12.))
        .text_color(t::muted())
        .child(text)
}

/// One choice of the model list: its name, an optional note under it, ✓ when chosen.
fn model_row(
    id: impl Into<ElementId>,
    title: String,
    note: Option<String>,
    chosen: bool,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    row()
        .id(id)
        .w_full()
        .min_h(px(36.))
        .px(px(10.))
        .py(px(6.))
        .gap(px(8.))
        .rounded(px(8.))
        .cursor_pointer()
        .hover(|style| style.bg(t::hover()))
        .on_click(click)
        .child(
            column()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(t::text())
                        .truncate()
                        .child(title),
                )
                .children(note.map(|note| {
                    div()
                        .text_size(px(12.))
                        .text_color(t::muted())
                        .truncate()
                        .child(note)
                })),
        )
        .children(chosen.then(|| icon(IconName::Check).size(px(15.)).text_color(t::text())))
}

/// Broad, evenly spaced bands of light travelling right through the fill, like light
/// within the material: brighter as the effort rises. They keep to the straight middle
/// of the track and fade out before its rounded ends. The fill's own opacity fades them
/// with it; `clock` stands still under reduced motion.
fn paint_sheen(bounds: Bounds<Pixels>, intensity: f32, clock: f64, window: &mut Window) {
    const BAND: f32 = 56.;
    const PITCH: f32 = 88.;
    /// Steady at every level, so a change of effort never makes the bands jump.
    const SPEED: f64 = 12.;
    const SEGMENT: f32 = 6.;
    const EDGE_FADE: f32 = 12.;
    let inset = TRACK / 2. + 1.;
    let room = f32::from(bounds.size.width) - 2. * inset;
    if room <= 0. {
        return;
    }
    let peak = 0.05 + 0.13 * intensity.clamp(0., 1.);
    let light = t::effort_sheen();
    let phase = (clock * SPEED).rem_euclid(f64::from(PITCH)) as f32;
    let alpha_at = |x: f32| {
        let u = (x - phase).rem_euclid(PITCH);
        // A raised cosine: value and slope reach zero at the band's ends.
        let wave = if u < BAND {
            0.5 - 0.5 * (std::f32::consts::TAU * u / BAND).cos()
        } else {
            0.
        };
        let edge = (x.min(room - x) / EDGE_FADE).clamp(0., 1.);
        peak * wave * edge * edge * (3. - 2. * edge)
    };
    // Side-by-side two-stop gradients sharing their end alphas: a smooth band, no seams.
    let mut x = 0.;
    let mut left = alpha_at(x);
    while x < room {
        let next = (x + SEGMENT).min(room);
        let right = alpha_at(next);
        if left > 0. || right > 0. {
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + point(px(inset + x), px(0.)),
                    size(px(next - x), bounds.size.height),
                ),
                linear_gradient(
                    90.,
                    linear_color_stop(light.opacity(left), 0.),
                    linear_color_stop(light.opacity(right), 1.),
                ),
            ));
        }
        x = next;
        left = right;
    }
}
