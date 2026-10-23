//! `!` commands (DEC-072): the composer's shell mode, running a command in the chat's
//! folder, its card in the conversation, and its result going to the agent.
use super::{rows::Row, view::ChatView};
use crate::app_state::AppState;
use crate::ui::{components::*, theme as t};
use canopy_desktop::chat::{
    shell::{Running, ShellEvent},
    transcript::{ItemId, ShellRun},
};
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
    spinner::Spinner,
};
use gpui_kit::{prelude::FluentBuilder, *};

/// Output lines shown before "Show more".
const PREVIEW_LINES: usize = 12;

/// A command running for this chat.
pub(super) struct Shell {
    pub(super) running: Running,
    _pump: Task<()>,
}

impl ChatView {
    /// The composer runs shell commands: Enter runs its text.
    pub(super) fn shell_mode(&self, _: &App) -> bool {
        self.shell
    }

    /// Turns shell mode on or off; the field says what Enter does.
    pub(super) fn set_shell(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.shell == on {
            return;
        }
        self.shell = on;
        self.shell_error = None;
        let placeholder = if on {
            "Run a shell command"
        } else {
            super::decision::COMPOSER_PLACEHOLDER
        };
        self.composer.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx)
        });
        cx.notify();
    }

    /// `!` typed into an empty field turns shell mode on and isn't kept as text.
    pub(super) fn composer_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.composer.read(cx).value().to_string();
        let was_empty = std::mem::replace(&mut self.composer_was_empty, value.is_empty());
        self.goal_typed(&value, window, cx);
        if self.shell
            || self.goal_mode
            || !was_empty
            || self.has_code_blocks()
            || !value.starts_with('!')
            || self.editing.is_some()
            || self.decision.active().is_some()
            || !self.threads.is_empty()
        {
            return;
        }
        let rest = value[1..].to_owned();
        self.set_shell(true, window, cx);
        // After the field's own update: it reports this change from inside it.
        let composer = self.composer.clone();
        cx.defer_in(window, move |this, window, cx| {
            composer.update(cx, |state, cx| state.set_value(rest.clone(), window, cx));
            this.composer_was_empty = rest.is_empty();
        });
    }

    /// Backspace or Esc in an empty field leaves shell (or goal) mode.
    pub(super) fn leave_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !(self.shell || self.goal_mode) || !self.composer.read(cx).value().is_empty() {
            return false;
        }
        self.set_shell(false, window, cx);
        self.set_goal_mode(false, window, cx);
        true
    }

    /// Runs `command` in the chat's folder; its card shows in the conversation and its
    /// result goes to the agent when it ends.
    pub(super) fn run_shell(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let command = command.trim();
        if command.is_empty() {
            return Err("Type a command to run.".into());
        }
        let cwd = self
            .cwd()
            .ok_or("This chat has no folder to run commands in.")?;
        let env = cx
            .global::<AppState>()
            .tools
            .read(cx)
            .environment
            .clone()
            .ok_or("The shell environment is still loading.")??;
        let session = self
            .session
            .as_mut()
            .filter(|session| !session.status().ended())
            .ok_or("The agent isn't running.")?;
        let (running, events) = Running::start(command, &cwd, &env.shell, &env.vars)?;
        let id = session.begin_shell(command);
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let ended = matches!(event, ShellEvent::Exit { .. });
                let alive = this
                    .update(cx, |this, cx| this.on_shell(id, event, cx))
                    .is_ok();
                if ended || !alive {
                    break;
                }
            }
        });
        self.shells.insert(
            id,
            Shell {
                running,
                _pump: pump,
            },
        );
        self.after_change(cx);
        Ok(())
    }

    fn on_shell(&mut self, id: ItemId, event: ShellEvent, cx: &mut Context<Self>) {
        let Some(session) = &mut self.session else {
            return;
        };
        match event {
            ShellEvent::Output(chunk) => session.shell_output(id, &chunk),
            ShellEvent::Exit { code, elapsed } => {
                let stopped = self.stopped_shells.remove(&id);
                if let Some(shell) = self.shells.remove(&id) {
                    // Exit describes the command, not proof that its entire group
                    // released cwd. Keep the handle for deletion/quit as well.
                    self.shell_resources.push(shell.running);
                }
                let session = self.session.as_mut().expect("session");
                let lines = session.end_shell(
                    id,
                    code,
                    stopped,
                    elapsed.as_millis().min(u64::MAX as u128) as u64,
                );
                self.write(lines);
            }
        }
        self.after_change(cx);
    }

    /// Stop on a running command's card: it ends, and the agent hears it was stopped.
    pub(super) fn stop_shell(&mut self, id: ItemId, cx: &mut Context<Self>) {
        if let Some(shell) = self.shells.get(&id) {
            self.stopped_shells.insert(id);
            shell.running.stop();
            cx.notify();
        }
    }

    /// The chat closes or its agent restarts: running commands end with it.
    pub(super) fn stop_shells(&mut self) {
        for (id, shell) in &self.shells {
            self.stopped_shells.insert(*id);
            shell.running.stop();
        }
    }
}

/// The card of a `!` command: the command, its output, how it ended and whether the
/// agent got it.
pub(super) fn shell_card(
    view: &Entity<ChatView>,
    this: &ChatView,
    entry: &Row,
    id: ItemId,
    run: &ShellRun,
    live: bool,
) -> Div {
    let key = entry.key;
    let running = run.running && this.shells.contains_key(&id) && live;
    let interrupted = run.running && !running;
    let lines: Vec<&str> = run.output.trim_end().lines().collect();
    let shown = if entry.extra.expanded {
        lines.len()
    } else {
        lines.len().min(PREVIEW_LINES)
    };
    let status: AnyElement = if running {
        row()
            .gap(px(6.))
            .child(Spinner::new())
            .child("Running")
            .into_any_element()
    } else if interrupted {
        div().child("Interrupted").into_any_element()
    } else if run.stopped {
        div().child("Stopped").into_any_element()
    } else {
        let code = run.exit;
        div()
            .text_color(if code == Some(0) {
                t::green()
            } else {
                t::red()
            })
            .child(match code {
                Some(code) => format!("Exit {code}"),
                None => "Killed".to_owned(),
            })
            .into_any_element()
    };
    let elapsed = run
        .elapsed_ms
        .map(|ms| super::tools::elapsed(ms as f32 / 1000.));
    let agent = this.provider().name();
    column()
        .pt(px(16.))
        .items_end()
        .child(
            column()
                .w(relative(0.85))
                .rounded(px(10.))
                .border_1()
                .border_color(t::shell().opacity(0.5))
                .bg(t::shell().opacity(0.08))
                .overflow_hidden()
                .child(
                    row()
                        .h(px(34.))
                        .px(px(12.))
                        .gap(px(8.))
                        .child(
                            custom_icon("square-terminal")
                                .size(px(14.))
                                .text_color(t::shell()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(t::MONO)
                                .text_size(px(12.5))
                                .text_color(t::text())
                                .child(format!("$ {}", run.command)),
                        )
                        .child(
                            row()
                                .flex_shrink_0()
                                .gap(px(8.))
                                .text_size(px(12.))
                                .text_color(t::muted())
                                .children(elapsed)
                                .child(status)
                                .when(running, |row| {
                                    let view = view.downgrade();
                                    row.child(
                                        Button::new(("chat-shell-stop", key))
                                            .xsmall()
                                            .ghost()
                                            .label("Stop")
                                            .on_click(move |_, _, cx| {
                                                let _ = view
                                                    .update(cx, |this, cx| this.stop_shell(id, cx));
                                            }),
                                    )
                                }),
                        ),
                )
                .when(!lines.is_empty(), |card| {
                    card.child(
                        div()
                            .id(("chat-shell-output", key))
                            .border_t_1()
                            .border_color(t::shell().opacity(0.25))
                            .px(px(12.))
                            .py(px(8.))
                            .overflow_x_scroll()
                            .font_family(t::MONO)
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .text_color(t::secondary())
                            .children(
                                lines[..shown]
                                    .iter()
                                    .map(|line| div().whitespace_nowrap().child(line.to_string())),
                            ),
                    )
                    .when(lines.len() > PREVIEW_LINES, |card| {
                        let view = view.downgrade();
                        let expanded = entry.extra.expanded;
                        card.child(
                            Button::new(("chat-shell-more", key))
                                .ghost()
                                .xsmall()
                                .compact()
                                .w_full()
                                .h(px(26.))
                                .rounded(px(0.))
                                .border_t_1()
                                .border_color(t::shell().opacity(0.25))
                                .text_size(px(12.))
                                .text_color(t::muted())
                                .label(if expanded {
                                    "Show less".to_owned()
                                } else {
                                    format!("Show {} more lines", lines.len() - shown)
                                })
                                .on_click(move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.shown_rows_mut().toggle_expanded(key, cx);
                                        cx.notify();
                                    });
                                }),
                        )
                    })
                }),
        )
        .when(run.reported, |column| {
            column.child(
                div()
                    .pt(px(4.))
                    .text_size(px(11.))
                    .text_color(t::faint())
                    .child(format!("Sent to {agent}")),
            )
        })
}

/// "Shell" over the composer while it is in shell mode.
pub(super) fn shell_label() -> Div {
    row()
        .px(px(4.))
        .gap(px(6.))
        .text_size(px(12.))
        .text_color(t::shell())
        .child(custom_icon("square-terminal").size(px(13.)))
        .child("Shell")
        .child(
            div()
                .text_color(t::muted())
                .child("— runs on your machine as you; the agent's mode doesn't apply"),
        )
}
