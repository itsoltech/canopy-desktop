//! Process tasks and coalesced frame updates, owned by TerminalView.
use super::*;

fn wait_for_cleanup(
    completion: async_channel::Receiver<()>,
    result: Arc<std::sync::Mutex<Option<Result<(), String>>>>,
    cx: &mut Context<TerminalView>,
) -> Task<Result<(), String>> {
    cx.spawn(async move |_, _| {
        let _ = completion.recv().await;
        let result = result.lock().unwrap().clone();
        result.ok_or("Terminal cleanup result was lost.".to_owned())?
    })
}

fn cleanup_is_current(
    operation_generation: u64,
    session_generation: u64,
    result: Option<&Result<(), String>>,
) -> bool {
    operation_generation == session_generation && result.is_none_or(Result::is_ok)
}

impl TerminalView {
    pub fn start(&mut self, cx: &mut Context<Self>) {
        if cx
            .global::<crate::app_state::AppState>()
            .settings
            .read(cx)
            .quitting
            || cx.global::<crate::app_state::AppState>().git.read(cx).busy
        {
            return;
        }
        if self.stopping
            || self.starting
            || self
                .session
                .as_ref()
                .is_some_and(|s| matches!(s.status(), Status::Running))
        {
            return;
        }
        if let Some(error) = self.cleanup_error() {
            self.error = Some(format!(
                "Retry terminal cleanup before starting another process: {error}"
            ));
            cx.notify();
            return;
        }
        if self.cleanup_result().is_some_and(|result| result.is_ok()) {
            self.cleanup = None;
        }
        self.generation += 1;
        let generation = self.generation;
        self.pump = None;
        self.session = None;
        self.starting = true;
        self.needs_focus = true;
        self.error = None;
        self.input_error = None;
        self.status = None;
        self.frame = None;
        self.cursor = Default::default();
        self.cursor_task = None;
        let app = cx.global::<crate::app_state::AppState>().clone();
        let mut catalog = app.tools.read(cx).catalog.clone();
        let env = app
            .tools
            .read(cx)
            .environment
            .clone()
            .and_then(Result::ok)
            .or_else(|| self.env.clone());
        let mut pane = app
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .into_iter()
            .flat_map(|w| w.all_panes())
            .find(|p| p.id == self.pane.id)
            .unwrap_or_else(|| self.pane.clone());
        let registration = if matches!(pane.tool.as_str(), "claude" | "codex") {
            match app
                .agents
                .update(cx, |agents, cx| agents.register(pane.clone(), cx))
            {
                Ok(registration) => {
                    self.agent_run = Some(registration.run.clone());
                    Some(registration)
                }
                Err(error) => {
                    self.starting = false;
                    self.error = Some(error);
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let size = self.columns;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let env = match env {
                        Some(env) => env,
                        None => Arc::new(ShellEnvironment::load()?),
                    };
                    if registration.is_some() {
                        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                        let helper = canopy_desktop::agents::launch::helper_path(&executable);
                        canopy_desktop::agents::launch::validate_helper(&helper)?;
                        canopy_desktop::agents::launch::augment(&mut catalog, &mut pane, &helper)?;
                    }
                    let mut prepared =
                        canopy_desktop::terminal::agent_config::prepare(&catalog, &pane, &env)?;
                    if let Some(registration) = &registration {
                        canopy_desktop::agents::launch::environment(
                            &mut prepared.environment,
                            registration,
                        );
                    }
                    let session = Session::start_with_config(
                        prepared.spec,
                        &prepared.environment,
                        size,
                        prepared.files,
                    )?;
                    Ok::<_, String>((env, session))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    if let Ok((_, session)) = result {
                        session.stop();
                        this.session = Some(Arc::new(session));
                    }
                    return;
                }
                this.starting = false;
                match result {
                    Ok((env, session)) => {
                        let notifications = session.notifications.clone();
                        this.env = Some(env);
                        this.session = Some(Arc::new(session));
                        this.refresh(cx);
                        this.pump = Some(cx.spawn(async move |this, cx| {
                            while notifications.recv().await.is_ok() {
                                // One coalesced wake slot: output cannot accumulate UI tasks.
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(8))
                                    .await;
                                let alive = this
                                    .update(cx, |this, cx| {
                                        if this.generation != generation {
                                            return false;
                                        }
                                        this.refresh(cx);
                                        this.session.is_some()
                                    })
                                    .unwrap_or(false);
                                if !alive {
                                    break;
                                }
                            }
                        }));
                    }
                    Err(error) => {
                        this.error = Some(error);
                        if let Some(run) = &this.agent_run {
                            cx.global::<crate::app_state::AppState>()
                                .agents
                                .clone()
                                .update(cx, |agents, cx| {
                                    agents.finish(this.pane.id, run, true, true, cx)
                                });
                        }
                    }
                }
                cx.emit(TerminalStateChanged);
                if this.visible {
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }
    pub fn request_focus(&mut self, cx: &mut Context<Self>) {
        self.needs_focus = true;
        cx.notify();
    }
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
    pub fn set_active(&mut self, visible: bool, focused: bool, cx: &mut Context<Self>) {
        if focused && !self.focused {
            self.needs_focus = true;
        }
        let changed = self.visible != visible || self.focused != focused;
        self.visible = visible;
        self.focused = focused;
        if changed && visible {
            self.refresh(cx);
            cx.notify();
        }
    }
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        if let Some(cleanup) = &self.cleanup {
            let result = cleanup.result.lock().unwrap().clone();
            if cleanup_is_current(cleanup.generation, self.generation, result.as_ref()) {
                return wait_for_cleanup(cleanup.completion.clone(), cleanup.result.clone(), cx);
            }
            self.cleanup = None;
        }
        self.stopping = true;
        self.generation += 1;
        self.visible = false;
        if let Some(run) = self.agent_run.take() {
            cx.global::<crate::app_state::AppState>()
                .agents
                .clone()
                .update(cx, |agents, cx| {
                    agents.finish(self.pane.id, &run, false, false, cx)
                });
        }
        self.pump = None;
        self.settle_task = None;
        self.cursor_task = None;
        let start = self.task.take();
        let current = self.session.clone();
        if let Some(session) = &current {
            session.stop();
        }
        let owner = cx.entity();
        let (finished, completion) = async_channel::bounded::<()>(1);
        let result = Arc::new(std::sync::Mutex::new(None));
        let shared_result = result.clone();
        let task = cx.spawn(async move |_, cx| {
            if let Some(start) = start {
                start.await;
            }
            let late = owner.update(cx, |this, _| this.session.clone());
            let mut cleanup = Ok(());
            let mut sessions = current.into_iter().chain(late).collect::<Vec<_>>();
            sessions.dedup_by(|left, right| Arc::ptr_eq(left, right));
            for session in sessions {
                session.stop();
                let session_cleanup = if matches!(session.cleanup_result(), Some(Err(_))) {
                    session.retry_cleanup().await
                } else {
                    session.wait_closed().await
                };
                if let Err(error) = session_cleanup
                    && cleanup.is_ok()
                {
                    cleanup = Err(error);
                }
            }
            *shared_result.lock().unwrap() = Some(cleanup);
            drop(finished);
        });
        self.cleanup = Some(CleanupOperation {
            generation: self.generation,
            _task: task,
            completion: completion.clone(),
            result: result.clone(),
        });
        wait_for_cleanup(completion, result, cx)
    }
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            let status = Some(session.status());
            if self.status != status {
                if !matches!(status, Some(Status::Running))
                    && let Some(run) = self.agent_run.take()
                {
                    cx.global::<crate::app_state::AppState>()
                        .agents
                        .clone()
                        .update(cx, |agents, cx| {
                            agents.finish(
                                self.pane.id,
                                &run,
                                matches!(status, Some(Status::Failed(_))) || matches!(status, Some(Status::Exited {code: Some(code), ..}) if code != 0),
                                true,
                                cx,
                            )
                        });
                }
                self.status = status;
                cx.emit(TerminalStateChanged);
            }
            if self.visible {
                if let Some(frame) = session.frame() {
                    self.set_frame(frame, cx);
                }
                cx.notify();
            }
        }
        self.paste_task_prompt(cx);
    }

    pub fn paste_task_prompt(&mut self, cx: &mut Context<Self>) {
        if self.starting || self.stopping || self.pane.metadata.task_prompt.is_none() {
            return;
        }
        let app = cx.global::<crate::app_state::AppState>().clone();
        let text = self
            .pane
            .metadata
            .task_prompt
            .as_ref()
            .expect("pending draft");
        let Some(bytes) = canopy_desktop::terminal::input::task_prompt_bytes(text) else {
            return;
        };
        // No Enter. Sending directly avoids recursive refresh -> send -> refresh.
        if self.session.as_ref().is_some_and(|s| s.input(bytes)) {
            self.pane.metadata.task_prompt = None;
            app.projects.update(cx, |projects, cx| {
                projects.task_prompt_pasted(self.pane.id, cx)
            });
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cleanup_is_current;

    #[test]
    fn restart_generation_never_reuses_the_previous_cleanup_result() {
        assert!(cleanup_is_current(2, 2, Some(&Ok(()))));
        assert!(!cleanup_is_current(2, 3, Some(&Ok(()))));
        assert!(!cleanup_is_current(2, 2, Some(&Err("retry".into()))));
    }
}
