use super::{components::*, theme as t};
use crate::app_state::{AppState, UpstreamRequest};
use canopy_desktop::{
    git::network::{Operation, Upstream},
    motion::{self, Presence, presets},
};
use gpui_kit::{
    base::Disableable,
    component::{IconName, IndexPath, input::InputState, select::SelectState},
    *,
};
use std::time::Instant;
pub struct UpstreamDialog {
    request: UpstreamRequest,
    remote: Entity<SelectState<Vec<SelectOption>>>,
    branch: Entity<InputState>,
    focus: FocusHandle,
    return_focus: FocusHandle,
    presence: Presence,
    closing: bool,
    scheduled: bool,
    error: Option<SharedString>,
}
impl EventEmitter<ModalDismissed> for UpstreamDialog {}
impl UpstreamDialog {
    pub fn new(
        request: UpstreamRequest,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let options: Vec<_> = request
            .plan
            .remotes
            .iter()
            .map(|name| SelectOption::new(name.clone(), name.clone()))
            .collect();
        let remote = cx.new(|cx| SelectState::new(options, Some(IndexPath::new(0)), window, cx));
        let branch = cx.new(|cx| InputState::new(window, cx).placeholder("Remote branch name"));
        branch.update(cx, |input, cx| {
            input.set_value(request.plan.branch.clone(), window, cx)
        });
        let now = Instant::now();
        let mut presence = Presence::new(false, presets::POPOVER, now);
        presence.set_open(true, now, motion::policy(cx));
        let focus = cx.focus_handle();
        let initial = focus.clone();
        window.on_next_frame(move |window, cx| initial.focus(window, cx));
        Self {
            request,
            remote,
            branch,
            focus,
            return_focus,
            presence,
            closing: false,
            scheduled: false,
            error: None,
        }
    }
    fn close(&mut self, cx: &mut Context<Self>) {
        self.closing = true;
        self.presence
            .set_open(false, Instant::now(), motion::policy(cx));
        cx.notify();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.presence.is_animating(Instant::now()) {
            return;
        }
        let Some(remote) = self.remote.read(cx).selected_value().cloned() else {
            return;
        };
        let branch = self.branch.read(cx).value().trim().to_owned();
        if branch.is_empty() || !git2::Reference::is_valid_name(&format!("refs/heads/{branch}")) {
            self.error = Some("Enter a valid remote branch name.".into());
            cx.notify();
            return;
        }
        let target = Upstream {
            remote: remote.to_string(),
            branch,
        };
        let result = cx
            .global::<AppState>()
            .changes
            .clone()
            .update(cx, |state, cx| {
                state.network(
                    self.request.operation,
                    Some((self.request.plan.clone(), target)),
                    cx,
                )
            });
        match result {
            Ok(()) => self.close(cx),
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }
}
impl Render for UpstreamDialog {
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
        let disabled = self.closing || animated;
        let push = self.request.operation == Operation::Push;
        let panel = modal::modal_surface("upstream-dialog")
            .absolute()
            .w(px(width))
            .left((window.viewport_size().width - px(width)) / 2.)
            .top(px(96. + motion::distance::BASE * (1. - progress)))
            .opacity(progress)
            .child(modal::modal_header(
                if push {
                    "Publish branch"
                } else {
                    "Choose upstream"
                },
                icon_button("close-upstream", IconName::Close, "Close dialog")
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            ))
            .child(git_network::upstream_form(
                &self.request.plan.branch,
                self.request.operation,
                &self.remote,
                &self.branch,
                disabled,
                button("same-branch-name", "Use local branch name")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.branch.update(cx, |input, cx| {
                            input.set_value(this.request.plan.branch.clone(), window, cx)
                        });
                    })),
            ))
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(t::red()).child(error)),
            )
            .child(modal::modal_actions(
                button("cancel-upstream", "Cancel")
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                primary_button(
                    "confirm-upstream",
                    if push {
                        "Publish branch"
                    } else {
                        "Set upstream and pull"
                    },
                )
                .disabled(disabled)
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
            .backdrop(modal::modal_backdrop(progress))
            .popup(panel)
    }
}
