use super::super::{ButtonLoading, button, icon_button, primary_loading_button};
use super::*;
use canopy_desktop::motion::{self, Presence, presets};
use gpui_kit::{base::Disableable, component::IconName};
use std::time::Instant;
type Confirm = Box<dyn Fn(&mut App) -> Result<(), SharedString>>;
type Completion = Box<dyn Fn(&App) -> (bool, Option<String>)>;
pub struct Confirmation {
    title: SharedString,
    message: SharedString,
    label: SharedString,
    confirm: Confirm,
    focus: FocusHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    error: Option<SharedString>,
    submitted: bool,
    loading: ButtonLoading,
    completion: Option<Completion>,
    completion_events: Option<Subscription>,
}
impl EventEmitter<ModalDismissed> for Confirmation {}
impl Confirmation {
    pub fn new(
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        label: impl Into<SharedString>,
        return_focus: FocusHandle,
        confirm: impl Fn(&mut App) -> Result<(), SharedString> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let initial = focus.clone();
        window.on_next_frame(move |window, cx| initial.focus(window, cx));
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        Self {
            title: title.into(),
            message: message.into(),
            label: label.into(),
            confirm: Box::new(confirm),
            focus,
            return_focus,
            presence,
            closing: false,
            scheduled: false,
            error: None,
            submitted: false,
            loading: ButtonLoading::default(),
            completion: None,
            completion_events: None,
        }
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.submitted {
            return;
        }
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.submitted || self.presence.is_animating(Instant::now()) {
            return;
        }
        match (self.confirm)(cx) {
            Ok(()) => {
                self.submitted = true;
                self.sync_completion(cx);
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }
    /// Keep a confirmation open while an accepted background action is running.
    /// The operation owns its task; the dialog only observes feedback/results.
    pub fn wait_for<T: 'static>(
        &mut self,
        state: &Entity<T>,
        result: impl Fn(&T) -> (bool, Option<String>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.completion_events = Some(cx.observe(state, |this, _, cx| this.sync_completion(cx)));
        let state = state.clone();
        self.completion = Some(Box::new(move |cx| result(state.read(cx))));
    }
    fn sync_completion(&mut self, cx: &mut Context<Self>) {
        if !self.submitted {
            return;
        }
        let (busy, error) = self
            .completion
            .as_ref()
            .map(|result| result(cx))
            .unwrap_or((false, None));
        self.loading.set(busy, cx);
        if !busy {
            self.submitted = false;
            self.error = error.map(Into::into);
            if self.error.is_none() {
                self.close(cx);
            }
        }
        cx.notify();
    }
}
impl Render for Confirmation {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.presence.progress(now);
        let animated = self.presence.is_animating(now);
        motion::request_frame(window, animated);
        if self.closing && !animated && !self.scheduled {
            self.scheduled = true;
            let owner = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.return_focus.focus(window, cx);
                    cx.emit(ModalDismissed);
                });
            });
        }
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(0., 440.);
        let disabled = self.closing || animated || self.submitted;
        let panel = modal_surface("confirm-action")
            .absolute()
            .w(px(width))
            .left((window.viewport_size().width - px(width)) / 2.)
            .top(px(96. + motion::distance::BASE * (1. - progress)))
            .opacity(progress)
            .child(modal_header(
                self.title.clone(),
                icon_button("close-confirm", IconName::Close, "Close dialog")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ))
            .child(self.message.clone())
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_color(t::red()).child(e)),
            )
            .child(modal_actions(
                button("cancel-confirm", "Cancel")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                primary_loading_button("accept-confirm", self.label.clone(), &self.loading)
                    .disabled(disabled && !self.loading.active())
                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
            ));
        let ok = cx.listener(|this, _, _, cx| this.submit(cx));
        let cancel = cx.listener(|this, _, _, cx| this.close(cx));
        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.focus.clone())
            .close_on_backdrop_press(false)
            .on_ok(move |e, w, cx| {
                ok(e, w, cx);
                false
            })
            .on_cancel(move |e, w, cx| {
                cancel(e, w, cx);
                false
            })
            .backdrop(modal_backdrop(progress))
            .popup(panel)
    }
}
