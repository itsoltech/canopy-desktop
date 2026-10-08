use super::*;
#[derive(Clone)]
pub struct WriteFinished {
    pub id: u64,
    pub command: TaskWrite,
    pub result: Result<WriteReceipt, WriteError>,
}
impl EventEmitter<WriteFinished> for IntegrationsState {}
impl IntegrationsState {
    /// A write lives in the global owner, survives view closure and is awaited on quit.
    pub fn submit(
        &mut self,
        command: TaskWrite,
        draft: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        if self.busy || !self.ready || self.quitting {
            return Err("Another integration operation is still running.".into());
        }
        let account = self
            .config
            .account_for(command.project())
            .cloned()
            .ok_or("Connect this repository in Preferences → Integrations.")?;
        self.mutation_id = self.mutation_id.wrapping_add(1);
        let id = self.mutation_id;
        let origin = self.path.clone();
        let http = cx.http_client();
        let credential = account.credential.clone();
        self.busy = true;
        self.read = None;
        self.detail = None;
        self.loading = false;
        cx.notify();
        self.write =
            Some(cx.spawn(async move |this, cx| {
                let mut result = async {
                    let token = cx
                        .background_executor()
                        .spawn({
                            let credential = account.credential.clone();
                            async move { credentials::load(&credential) }
                        })
                        .await
                        .map_err(WriteError::Rejected)?;
                    client(http, &account, token)?.write(&command).await
                }
                .await;
                let mut config_update = None;
                if let Ok(receipt) = &mut result
                    && let Some(deleted) = &receipt.deleted_task
                    && let Ok((mut config, Some(client))) =
                        this.read_with(cx, |s, _| (s.config.clone(), s.client.clone()))
                {
                    for links in config.links.values_mut() {
                        links.retain(|t| !t.same_task(deleted));
                    }
                    match client.save_integrations(config.clone()).await {
                        Ok(()) => config_update = Some(config),
                        Err(_) => receipt.notice = Some(
                            "Issue deleted in the task provider, but its local task links could not be saved."
                                .into(),
                        ),
                    }
                }
                let _ = this.update(cx, |this, cx| {
                    if let Some(config) = config_update {
                        this.config = config;
                    }
                    this.busy = false;
                    if let Ok(receipt) = &result {
                        if let Some(deleted) = &receipt.deleted_task {
                            this.task_error = receipt.notice.clone();
                            this.total_count = None;
                            this.deleted_tasks.push(deleted.clone());
                            if this.deleted_tasks.len() > 128 {
                                this.deleted_tasks.remove(0);
                            }
                            this.items = Arc::new(
                                this.items
                                    .iter()
                                    .filter(|t| !t.reference.same_task(deleted))
                                    .cloned()
                                    .collect(),
                            );
                            cx.global::<AppState>().toasts.clone().update(cx, |s, cx| {
                                s.show(format!("Deleted {}", deleted.label()), cx)
                            });
                        }
                        if let Some(key) = &draft {
                            cx.global::<AppState>()
                                .task_drafts
                                .clone()
                                .update(cx, |s, cx| s.remove(key, cx));
                        }
                        this.cache.clear();
                        if let Some(task) = &receipt.task {
                            if let Some(id) = task.jira.as_ref().and_then(|d| d.internal_id) {
                                this.jira_reconcile.retain(|(t, k, n)| {
                                    !(t == &task.reference.project && k == &credential && *n == id)
                                });
                                this.jira_reconcile.insert(
                                    0,
                                    (task.reference.project.clone(), credential.clone(), id),
                                );
                                this.jira_reconcile.truncate(32);
                            }
                            if this.target.as_ref() == Some(&task.reference.project) {
                                if command.is_create() && task.reference.project.provider == Provider::Jira {
                                    this.filter = TaskState::Open;
                                }
                                let mut items = this.items.as_ref().clone();
                                if matches!(task.reference.project.provider, Provider::Jira | Provider::Youtrack) {
                                    // Membership is decided by the selected JQL, not the GitHub Open/Closed filter.
                                    if let Some(old) = items
                                        .iter_mut()
                                        .find(|old| old.reference.same_task(&task.reference))
                                    {
                                        *old = task.clone();
                                    }
                                } else {
                                    items.retain(|old| !old.reference.same_task(&task.reference));
                                    if task.state == this.filter {
                                        items.insert(0, task.clone());
                                    }
                                }
                                this.items = Arc::new(items);
                                this.total_count = None;
                            }
                            if this.path == origin && !this.quitting {
                                cx.emit(task.clone());
                            }
                        }
                        if command.is_create()
                            && let Some(task) = &receipt.task
                        {
                            cx.global::<AppState>()
                                .toasts
                                .clone()
                                .update(cx, |toasts, cx| {
                                    toasts.show(format!("Created {}", task.reference.label()), cx)
                                });
                        }
                        if this.visible
                            && this.target.as_ref() == Some(command.project())
                            && !this.quitting
                        {
                            this.refresh(true, cx);
                        }
                    }
                    if let Err(error) = &result {
                        if let Some(key) = &draft {
                            cx.global::<AppState>()
                                .task_drafts
                                .clone()
                                .update(cx, |s, cx| s.record_error(key, error.to_string(), cx));
                        }
                        if this.path == origin {
                            this.task_error = Some(error.to_string());
                        }
                    }
                    cx.emit(WriteFinished {
                        id,
                        command,
                        result,
                    });
                    cx.notify();
                });
            }));
        Ok(id)
    }
    pub fn draft_key(&self, project: &ProjectTarget, kind: &str) -> Option<String> {
        self.config
            .account_for(project)
            .map(|account| format!("{}:{}:{}:{kind}", account.id, account.login, project.key))
    }
}
