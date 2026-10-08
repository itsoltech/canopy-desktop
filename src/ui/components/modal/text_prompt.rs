//! Shared modal surface and a single-field prompt. No domain state is owned here.
use super::{modal_actions, modal_backdrop, modal_field, modal_header, modal_surface};
use crate::ui::components::{button, icon_button, input, primary_button};
use crate::ui::theme as t;
use canopy_desktop::motion::{self, Presence, presets};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName,
        input::{InputEvent, InputState},
    },
    *,
};
use std::time::Instant;

pub struct ModalDismissed;
type Submit = Box<dyn Fn(&str, &mut App) -> Result<(), SharedString>>;
pub struct TextPrompt {
    title: SharedString,
    label: SharedString,
    confirm: SharedString,
    field: Entity<InputState>,
    focus: FocusHandle,
    return_focus: FocusHandle,
    submit: Submit,
    error: Option<SharedString>,
    presence: Presence,
    closing: bool,
    dismiss_scheduled: bool,
    _input_events: Subscription,
}
impl EventEmitter<ModalDismissed> for TextPrompt {}
impl TextPrompt {
    pub fn new(
        title: impl Into<SharedString>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        return_focus: FocusHandle,
        submit: impl Fn(&str, &mut App) -> Result<(), SharedString> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| InputState::new(window, cx).default_value(value));
        let input_events = cx.subscribe(&field, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.error = None;
                cx.notify();
            }
        });
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        let input_focus = field.clone();
        window.on_next_frame(move |window, cx| {
            input_focus.read(cx).focus_handle(cx).focus(window, cx)
        });
        Self {
            title: title.into(),
            label: label.into(),
            confirm: "Save".into(),
            field,
            focus: cx.focus_handle(),
            return_focus,
            submit: Box::new(submit),
            error: None,
            presence,
            closing: false,
            dismiss_scheduled: false,
            _input_events: input_events,
        }
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let value = self.field.read(cx).value();
        match (self.submit)(&value, cx) {
            Ok(()) => self.close(cx),
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }
}
impl Render for TextPrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animating = self.presence.is_animating(now);
        motion::request_frame(window, animating);
        if self.closing && !animating && !self.dismiss_scheduled {
            self.dismiss_scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.return_focus.focus(window, cx);
                    cx.emit(ModalDismissed);
                });
            });
        }
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(0., 420.);
        let panel = modal_surface("text-prompt")
            .absolute()
            .left((window.viewport_size().width - px(width)) / 2.)
            .top(px(96. + motion::distance::BASE * (1. - progress)))
            .w(px(width))
            .opacity(progress)
            .child(modal_header(
                self.title.clone(),
                icon_button("modal-close", IconName::Close, "Close dialog")
                    .disabled(self.closing)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ))
            .child(modal_field(
                self.label.clone(),
                input(&self.field)
                    .bg(t::input_bg())
                    .border_color(t::control_border())
                    .disabled(self.closing),
                self.error.clone(),
            ))
            .child(modal_actions(
                button("modal-cancel", "Cancel")
                    .disabled(self.closing)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                primary_button("modal-save", self.confirm.clone())
                    .disabled(self.closing)
                    .on_click(cx.listener(|this, _, _, cx| this.confirm(cx))),
            ));
        // Base Dialog supplies modal focus trapping and backdrop event isolation;
        // actual focus and motion are retained by this entity.
        let confirm = cx.listener(|this, _, _, cx| this.confirm(cx));
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(false)
            .on_ok(move |event, window, cx| {
                confirm(event, window, cx);
                false
            })
            .on_cancel(move |event, window, cx| {
                cancel(event, window, cx);
                false
            })
            .backdrop(modal_backdrop(progress))
            .popup(panel)
    }
}
