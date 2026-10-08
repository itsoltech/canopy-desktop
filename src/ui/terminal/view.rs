//! Declarative terminal view and interaction wiring.
use super::*;
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_focus {
            self.needs_focus = false;
            let focus = self.focus.clone();
            window.on_next_frame(move |window, cx| focus.focus(window, cx));
        }
        let finished = self.error.is_some()
            || self
                .status
                .as_ref()
                .is_some_and(|s| !matches!(s, Status::Running));
        let label = self.error.clone().or_else(|| match &self.status {
            Some(Status::Exited {
                code: Some(code), ..
            }) => Some(format!("Process exited with code {code}")),
            Some(Status::Exited {
                signal: Some(signal),
                ..
            }) => Some(format!("Process terminated by signal {signal}")),
            Some(Status::Exited { .. }) => Some("Process exited".into()),
            Some(Status::Failed(error)) => Some(error.clone()),
            _ => None,
        });
        let owner = cx.entity();
        let paint_owner = owner.clone();
        let surface = canvas(
            move |bounds, _, cx| {
                owner.update(cx, |this, cx| this.resize(bounds, cx));
            },
            move |bounds, _, window, cx| {
                paint_owner.update(cx, |this, cx| {
                    painter::paint(this, bounds, window, cx);
                });
            },
        )
        .size_full();
        let drop_owner = cx.entity();
        terminal_surface("terminal")
            .key_context("Terminal")
            .track_focus(&self.focus)
            .can_drop(move |drag, _, cx| {
                drag.is::<ExternalPaths>() && drop_owner.read(cx).can_accept_file_drop()
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_files(paths.paths(), window, cx)
            }))
            .on_action(cx.listener(|this, _: &CopyTerminal, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &PasteTerminal, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &TerminalTab, window, cx| {
                if this.focus.is_focused(window) {
                    if let Some(bytes) = this.encoded_key("tab", false, false, false) {
                        this.send(bytes, cx);
                    }
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &TerminalBacktab, window, cx| {
                if this.focus.is_focused(window) {
                    if let Some(bytes) = this.encoded_key("tab", false, false, true) {
                        this.send(bytes, cx);
                    }
                } else {
                    window.focus_prev(cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &TerminalShiftEnter, window, cx| {
                if this.focus.is_focused(window) {
                    if let Some(bytes) = this.encoded_key("enter", false, false, true) {
                        this.send(bytes, cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event, _, cx| this.key(event, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.activate(window, cx);
                    this.mouse(event.position, 0, true, event.modifiers.shift, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    this.mouse(event.position, 32, false, event.modifiers.shift, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.mouse(event.position, 3, false, event.modifiers.shift, cx)
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                this.wheel += f32::from(event.delta.pixel_delta(px(painter::CELL_HEIGHT)).y)
                    / painter::CELL_HEIGHT;
                let lines = this.wheel as i32;
                this.wheel -= lines as f32;
                if lines != 0 {
                    this.scroll(lines, event.position, event.modifiers.shift, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                terminal_viewport(surface)
                    .children(self.starting.then(|| {
                        terminal_message("Starting…")
                            .absolute()
                            .left(px(12.))
                            .top(px(12.))
                    }))
                    .children(self.input_error.as_ref().map(|message| {
                        terminal_message(message.clone())
                            .absolute()
                            .left(px(12.))
                            .bottom(px(12.))
                            .max_w(px(360.))
                            .px(px(8.))
                            .py(px(6.))
                            .rounded(px(4.))
                            .bg(t::elevated())
                            .border_1()
                            .border_color(t::border())
                    })),
            )
            .children(
                (!finished && self.pane.metadata.task_prompt.is_some()).then(|| {
                    process_status_bar(
                        "Task prompt pending — waiting for the terminal. It will not be sent.",
                        [button("paste-task-prompt", "Paste task prompt")
                            .on_click(cx.listener(|this, _, _, cx| this.paste_task_prompt(cx)))
                            .into_any_element()],
                    )
                }),
            )
            .children(finished.then(|| {
                process_status_bar(
                    label.unwrap_or_default(),
                    [
                        button("restart-terminal", "Restart")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.activate(window, cx);
                                this.start(cx)
                            }))
                            .into_any_element(),
                        button("close-terminal", "Close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let pane = this.pane.id;
                                let app = cx.global::<AppState>().clone();
                                app.workspace.update(cx, |state, cx| {
                                    if let Some(tab) = state
                                        .tabs()
                                        .iter()
                                        .find(|t| t.root.find(pane).is_some())
                                        .map(|t| t.id)
                                    {
                                        let _ = state.close_pane(tab, pane);
                                        cx.notify();
                                    }
                                });
                            }))
                            .into_any_element(),
                    ],
                )
                .children(
                    matches!(self.pane.tool.as_str(), "claude" | "codex").then(|| {
                        button("new-agent-session", "New session").on_click(cx.listener(
                            |this, _, window, cx| {
                                cx.global::<AppState>()
                                    .projects
                                    .clone()
                                    .update(cx, |projects, cx| {
                                        projects.clear_agent_session(this.pane.id, cx)
                                    });
                                this.activate(window, cx);
                                this.start(cx);
                            },
                        ))
                    }),
                )
            }))
    }
}
