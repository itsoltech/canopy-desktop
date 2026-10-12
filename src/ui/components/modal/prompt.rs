//! In-window renderer for `Window::prompt` where the platform has no native alert
//! (Linux). GPUI's fallback is unstyled, so prompts reuse the Canopy modal surface.
use super::super::{button, primary_button};
use super::{modal_backdrop, modal_surface};
use crate::ui::theme as t;
use canopy_desktop::motion::{self, Presence, presets};
use gpui_kit::{base::Disableable, *};
use std::time::Instant;

pub fn init(cx: &mut App) {
    if cfg!(not(any(target_os = "macos", target_os = "windows"))) {
        cx.set_prompt_builder(|_, message, detail, answers, handle, window, cx| {
            let view = cx.new(|cx| PromptDialog::new(message, detail, answers, window, cx));
            handle.with_view(view, window, cx)
        });
    }
}

struct PromptDialog {
    message: SharedString,
    detail: Option<SharedString>,
    answers: Vec<PromptButton>,
    focus: FocusHandle,
    presence: Presence,
    /// Answer picked by the user; sent once the exit animation ends.
    chosen: Option<usize>,
    sent: bool,
}
impl EventEmitter<PromptResponse> for PromptDialog {}
impl Focusable for PromptDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl PromptDialog {
    fn new(
        message: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        let focus = cx.focus_handle();
        let initial = focus.clone();
        window.on_next_frame(move |window, cx| initial.focus(window, cx));
        Self {
            message: message.to_owned().into(),
            detail: detail.map(|d| d.to_owned().into()),
            answers: answers.to_vec(),
            focus,
            presence,
            chosen: None,
            sent: false,
        }
    }
    fn choose(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.chosen.is_some() {
            return;
        }
        self.chosen = Some(ix);
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn cancel_index(&self) -> usize {
        self.answers
            .iter()
            .position(PromptButton::is_cancel)
            .unwrap_or(self.answers.len().saturating_sub(1))
    }
    fn ok_index(&self) -> Option<usize> {
        self.answers
            .iter()
            .position(|a| matches!(a, PromptButton::Ok(_)))
    }
}
impl Render for PromptDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animated = self.presence.is_animating(now);
        motion::request_frame(window, animated);
        if let Some(ix) = self.chosen.filter(|_| !animated && !self.sent) {
            self.sent = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let _ = owner.update(cx, |_, cx| cx.emit(PromptResponse(ix)));
            });
        }
        // Cancel first, extra choices next, the default answer last and accented.
        let mut order: Vec<usize> = (0..self.answers.len()).collect();
        order.sort_by_key(|&ix| match &self.answers[ix] {
            PromptButton::Cancel(_) => 0,
            PromptButton::Other(_) => 1,
            PromptButton::Ok(_) => 2,
        });
        let disabled = self.chosen.is_some();
        let actions = order.into_iter().map(|ix| {
            let answer = &self.answers[ix];
            let id = ("prompt-answer", ix);
            match answer {
                PromptButton::Ok(label) => primary_button(id, label.clone()),
                _ => button(id, answer.label().clone()),
            }
            .disabled(disabled)
            .on_click(cx.listener(move |this, _, _, cx| this.choose(ix, cx)))
        });
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(0., 440.);
        let panel = modal_surface("window-prompt")
            .w(px(width))
            .mt(px(96. + motion::distance::BASE * (1. - progress)))
            .opacity(progress)
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.message.clone()),
            )
            .children(
                self.detail
                    .clone()
                    .map(|d| div().text_color(t::secondary()).child(d)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.))
                    .children(actions),
            );
        div()
            .id("window-prompt-host")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let modifiers = &event.keystroke.modifiers;
                if modifiers.modified() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "escape" => this.choose(this.cancel_index(), cx),
                    "enter" => {
                        if let Some(ix) = this.ok_index() {
                            this.choose(ix, cx)
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(modal_backdrop(progress))
            .child(panel)
    }
}
