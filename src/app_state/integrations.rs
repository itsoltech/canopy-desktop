mod mutations;
use super::AppState;
use canopy_desktop::{
    integrations::{credentials, *},
    settings::SettingsClient,
};
use gpui_kit::*;
pub use mutations::WriteFinished;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
#[derive(Clone)]
struct Cached {
    items: Arc<Vec<TaskItem>>,
    next_cursor: Option<String>,
    total_count: Option<usize>,
}
pub struct IntegrationsState {
    pub config: Config,
    pub ready: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub connection: String,
    pub path: Option<PathBuf>,
    pub repository: Option<RepositoryContext>,
    pub target: Option<ProjectTarget>,
    pub items: Arc<Vec<TaskItem>>,
    pub loading: bool,
    pub task_error: Option<String>,
    pub more: bool,
    pub total_count: Option<usize>,
    pub filter: TaskState,
    pub query: String,
    jira_reconcile: Vec<(ProjectTarget, String, u64)>,
    deleted_tasks: Vec<TaskRef>,
    client: Option<SettingsClient>,
    write: Option<Task<()>>,
    read: Option<Task<()>>,
    origin: Option<Task<()>>,
    detail: Option<Task<()>>,
    observers: Vec<Subscription>,
    visible: bool,
    next_cursor: Option<String>,
    cache: HashMap<String, Cached>,
    quitting: bool,
    mutation_id: u64,
    read_generation: u64,
}
impl EventEmitter<TaskItem> for IntegrationsState {}
impl IntegrationsState {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
            ready: false,
            busy: false,
            error: None,
            connection: "Not connected".into(),
            path: None,
            repository: None,
            target: None,
            items: Arc::new(vec![]),
            loading: false,
            task_error: None,
            more: false,
            total_count: None,
            filter: TaskState::Open,
            query: String::new(),
            jira_reconcile: vec![],
            deleted_tasks: vec![],
            client: None,
            write: None,
            read: None,
            origin: None,
            detail: None,
            observers: vec![],
            visible: false,
            next_cursor: None,
            cache: HashMap::new(),
            quitting: false,
            mutation_id: 0,
            read_generation: 0,
        }
    }
    pub fn bind(&mut self, cx: &mut Context<Self>) {
        let app = cx.global::<AppState>().clone();
        self.observers = vec![
            cx.observe(&app.projects, |this, _, cx| this.resolve(cx)),
            cx.observe(&app.git, |this, _, cx| this.resolve(cx)),
            cx.observe(&app.layout, |this, layout, cx| {
                let layout = layout.read(cx);
                let visible = layout.inspector_open && layout.inspector_tasks;
                if this.visible != visible {
                    this.visible = visible;
                    if visible {
                        this.refresh(false, cx);
                    } else {
                        this.read = None;
                        this.loading = false;
                    }
                    cx.notify();
                }
            }),
        ];
    }
    pub fn initialize(&mut self, client: SettingsClient, cx: &mut Context<Self>) {
        self.client = Some(client.clone());
        self.write = Some(cx.spawn(async move |this, cx| {
            let result = client.load_integrations().await;
            let _ = this.update(cx, |this, cx| {
                this.ready = true;
                match result {
                    Ok(config) => {
                        this.connection = if !config.accounts.is_empty() {
                            "Saved connection — not checked".into()
                        } else {
                            "Not connected".into()
                        };
                        this.config = config;
                        this.resolve(cx);
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        }));
    }
    fn resolve(&mut self, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        let app = cx.global::<AppState>().clone();
        let path = app
            .projects
            .read(cx)
            .catalog
            .current()
            .map(|p| p.path.clone());
        if path != self.path {
            self.path = path.clone();
            self.query.clear();
            self.repository = None;
            self.target = None;
            self.read = None;
            self.detail = None;
            self.items = Arc::new(vec![]);
            self.loading = false;
            self.more = false;
            self.next_cursor = None;
            self.total_count = None;
            self.task_error = None;
            cx.notify();
        }
        self.origin = None;
        let (Some(path), Some(client)) = (path, app.git.read(cx).client()) else {
            return;
        };
        self.origin = Some(cx.spawn(async move |this, cx| {
            let result = client.task_repository(path).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(repo) => {
                        let target = repo.as_ref().and_then(|r| this.config.task_project(r));
                        let changed = target != this.target;
                        this.repository = repo;
                        this.target = target;
                        if changed {
                            this.query.clear();
                            this.read = None;
                            this.detail = None;
                            this.task_error = None;
                            this.loading = false;
                            this.items = Arc::new(vec![]);
                            this.next_cursor = None;
                            this.total_count = None;
                            this.more = false;
                            if this.visible {
                                this.refresh(false, cx);
                            }
                        } else if this.visible
                            && this.items.is_empty()
                            && !this.loading
                            && this.task_error.is_none()
                        {
                            this.refresh(false, cx);
                        }
                    }
                    Err(error) => this.task_error = Some(error),
                }
                cx.notify();
            });
        }));
    }
    pub fn connect(
        &mut self,
        replace: Option<String>,
        scope: AccountScope,
        token: String,
        cx: &mut Context<Self>,
    ) {
        if cx.global::<AppState>().git.read(cx).busy {
            return;
        }
        if self.busy || !self.ready || self.quitting {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        if let Some(id) = &replace
            && !self.config.accounts.iter().any(|a| &a.id == id)
        {
            self.error = Some("This connection no longer exists.".into());
            cx.notify();
            return;
        }
        let credential = uuid::Uuid::new_v4().to_string();
        let id = replace.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let candidate = Account {
            id: id.clone(),
            provider: match scope {
                AccountScope::Jira { .. } => Provider::Jira,
                AccountScope::Youtrack { .. } => Provider::Youtrack,
                AccountScope::Default | AccountScope::Owner(_) => Provider::Github,
            },
            login: "pending".into(),
            credential: credential.clone(),
            scope: scope.clone(),
        };
        let mut config = match self.config.with_account(candidate) {
            Ok(config) => config,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let http = cx.http_client();
        self.busy = true;
        self.error = None;
        self.connection = "Checking connection…".into();
        self.detail = None;
        self.read = None;
        self.loading = false;
        cx.notify();
        self.write = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let candidate = config.accounts.iter().find(|a| a.id == id).unwrap();
                let login = canopy_desktop::integrations::client(http, candidate, token.clone())?
                    .verify()
                    .await?;
                config
                    .accounts
                    .iter_mut()
                    .find(|a| a.id == id)
                    .unwrap()
                    .login = login;
                let (key, value) = (credential.clone(), token);
                cx.background_executor()
                    .spawn(async move { credentials::store(&key, &value) })
                    .await?;
                if let Err(error) = client.save_integrations(config.clone()).await {
                    let key = credential.clone();
                    let _ = cx
                        .background_executor()
                        .spawn(async move { credentials::remove(&key) })
                        .await;
                    return Err(error.to_string());
                }
                Ok(clean_retired(config, client, cx.background_executor().clone()).await)
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((config, warning)) => {
                        this.config = config;
                        this.connection = format!("{} · Token verified", scope.label());
                        this.error = warning;
                        this.cache.clear();
                        this.items = Arc::new(vec![]);
                        this.more = false;
                        this.next_cursor = None;
                        this.total_count = None;
                        this.task_error = None;
                        this.resolve(cx);
                        if this.visible {
                            this.refresh(true, cx);
                        }
                    }
                    Err(error) => {
                        this.connection = "Connection failed".into();
                        this.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }
    pub fn verify(&mut self, id: &str, cx: &mut Context<Self>) {
        if cx.global::<AppState>().git.read(cx).busy {
            return;
        }
        if self.busy || self.quitting {
            return;
        }
        let Some(account) = self.config.accounts.iter().find(|a| a.id == id).cloned() else {
            return;
        };
        let http = cx.http_client();
        self.busy = true;
        self.error = None;
        self.connection = "Checking connection…".into();
        cx.notify();
        self.write = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let key = account.credential.clone();
                let token = cx
                    .background_executor()
                    .spawn(async move { credentials::load(&key) })
                    .await?;
                canopy_desktop::integrations::client(http, &account, token)?
                    .verify()
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(login) => {
                        this.connection =
                            format!("{} · Token verified as {login}", account.scope.label());
                        if this.visible && !this.loading {
                            this.refresh(false, cx);
                        }
                    }
                    Err(error) => {
                        this.connection = "Connection failed".into();
                        this.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }
    pub fn disconnect(&mut self, id: &str, cx: &mut Context<Self>) {
        if cx.global::<AppState>().git.read(cx).busy {
            return;
        }
        if self.busy || self.quitting {
            return;
        }
        let Some(account) = self.config.accounts.iter().find(|a| a.id == id).cloned() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let mut config = self.config.clone();
        config.accounts.retain(|a| a.id != account.id);
        config.retired_credentials.push(account.credential.clone());
        self.busy = true;
        self.detail = None;
        self.read = None;
        self.loading = false;
        self.error = None;
        cx.notify();
        self.write = Some(cx.spawn(async move |this, cx| {
            let result = client.save_integrations(config.clone()).await;
            let (config, warning) = if result.is_ok() {
                clean_retired(config, client, cx.background_executor().clone()).await
            } else {
                (config, None)
            };
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        this.config = config;
                        this.connection = format!("{} · Disconnected", account.scope.label());
                        this.error = warning;
                        this.cache.clear();
                        this.items = Arc::new(vec![]);
                        this.more = false;
                        this.next_cursor = None;
                        this.total_count = None;
                        this.task_error = None;
                        this.resolve(cx);
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        }));
    }
    pub fn cleanup_credentials(&mut self, cx: &mut Context<Self>) {
        if cx.global::<AppState>().git.read(cx).busy {
            return;
        }
        if self.busy || self.quitting {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let config = self.config.clone();
        self.busy = true;
        cx.notify();
        self.write = Some(cx.spawn(async move |this, cx| {
            let (config, error) =
                clean_retired(config, client, cx.background_executor().clone()).await;
            let _ = this.update(cx, |this, cx| {
                this.config = config;
                this.error = error;
                this.busy = false;
                cx.notify();
            });
        }));
    }
    pub fn active_account(&self) -> Option<&Account> {
        self.config.account_for(self.target.as_ref()?)
    }
    pub fn tasks_visible(&self) -> bool {
        self.visible
    }
    fn cache_key(&self) -> Option<String> {
        Some(
            serde_json::json!([
                self.active_account()?.credential,
                self.target,
                format!("{:?}", self.filter),
                self.query,
                self.target
                    .as_ref()
                    .and_then(|t| self.config.selected_filter(t))
                    .map(|f| f.expression)
            ])
            .to_string(),
        )
    }
    pub fn refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.quitting || self.busy || !self.ready {
            return;
        }
        let (Some(account), Some(target)) = (self.active_account().cloned(), self.target.clone())
        else {
            return;
        };
        let key = self.cache_key().unwrap();
        if !force && let Some(cached) = self.cache.get(&key) {
            self.read = None;
            self.loading = false;
            self.items = cached.items.clone();
            self.next_cursor = cached.next_cursor.clone();
            self.total_count = cached.total_count;
            self.more = self.next_cursor.is_some();
            self.task_error = None;
            cx.notify();
            return;
        }
        self.load_page(account, target, None, key, cx);
    }
    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        if !self.more || self.loading || self.busy {
            return;
        }
        if let (Some(account), Some(target), Some(key), Some(cursor)) = (
            self.active_account().cloned(),
            self.target.clone(),
            self.cache_key(),
            self.next_cursor.clone(),
        ) {
            self.load_page(account, target, Some(cursor), key, cx);
        }
    }
    fn load_page(
        &mut self,
        account: Account,
        target: ProjectTarget,
        cursor: Option<String>,
        key: String,
        cx: &mut Context<Self>,
    ) {
        self.read_generation = self.read_generation.wrapping_add(1);
        let read_generation = self.read_generation;
        let http = cx.http_client();
        let filter = self.filter;
        let query = self.query.clone();
        let expression = self.config.selected_filter(&target).map(|f| f.expression);
        let reconcile = self
            .jira_reconcile
            .iter()
            .filter(|(t, k, _)| *t == target && *k == account.credential)
            .map(|(_, _, n)| *n)
            .collect::<Vec<_>>();
        let context = TaskQuery {
            text: query,
            filter: expression,
            reconcile,
        };
        let first_page = cursor.is_none();
        self.loading = true;
        self.task_error = None;
        cx.notify();
        self.read = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let token = cx
                    .background_executor()
                    .spawn({
                        let credential = account.credential.clone();
                        async move { credentials::load(&credential) }
                    })
                    .await?;
                canopy_desktop::integrations::client(http, &account, token)?
                    .list_context(&target, filter, cursor.as_deref(), &context)
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.read_generation != read_generation {
                    return;
                }
                if this.cache_key().as_deref() != Some(key.as_str()) {
                    this.loading = false;
                    return;
                }
                this.loading = false;
                match result {
                    Ok(result) => {
                        let mut items = if first_page {
                            vec![]
                        } else {
                            this.items.as_ref().clone()
                        };
                        for task in result.tasks {
                            if this
                                .deleted_tasks
                                .iter()
                                .any(|t| t.same_task(&task.reference))
                            {
                                continue;
                            }
                            if let Some(old) = items
                                .iter_mut()
                                .find(|old| old.reference.id == task.reference.id)
                            {
                                *old = task;
                            } else {
                                items.push(task);
                            }
                        }
                        let at_limit = items.len() >= 1000;
                        items.truncate(1000);
                        this.items = Arc::new(items);
                        this.total_count = result.total_count;
                        this.next_cursor = if at_limit {
                            None
                        } else {
                            result.next_cursor.clone()
                        };
                        this.more = this.next_cursor.is_some();
                        if at_limit && result.next_cursor.is_some() {
                            this.task_error = Some(
                                "Loaded 1,000 issues. Narrow your filter to browse older tasks."
                                    .into(),
                            );
                        }
                        if this.cache.len() >= 8 {
                            this.cache.clear();
                        }
                        this.cache.insert(
                            key,
                            Cached {
                                items: this.items.clone(),
                                next_cursor: this.next_cursor.clone(),
                                total_count: this.total_count,
                            },
                        );
                    }
                    Err(error) => this.task_error = Some(error),
                }
                cx.notify();
            });
        }));
    }
    pub fn open_linked(&mut self, task: TaskRef, cx: &mut Context<Self>) {
        if self.busy || !self.ready || self.quitting {
            return;
        }
        let Some(account) = self.config.account_for(&task.project).cloned() else {
            self.task_error = Some(format!(
                "No connection for {}. Add this provider account in Preferences → Integrations.",
                task.project.key
            ));
            cx.notify();
            return;
        };
        let http = cx.http_client();
        self.task_error = None;
        self.detail = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let token = cx
                    .background_executor()
                    .spawn({
                        let credential = account.credential.clone();
                        async move { credentials::load(&credential) }
                    })
                    .await?;
                canopy_desktop::integrations::client(http, &account, token)?
                    .task(&task)
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(task) => cx.emit(task),
                    Err(error) => this.task_error = Some(error),
                }
                cx.notify();
            });
        }));
    }
    pub fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.busy || self.quitting {
            return;
        }
        self.query = query;
        self.items = Arc::new(vec![]);
        self.more = false;
        self.next_cursor = None;
        self.total_count = None;
        self.read = None;
        self.loading = false;
        self.refresh(true, cx);
        cx.notify();
    }
    pub fn set_filter(&mut self, filter: TaskState, cx: &mut Context<Self>) {
        if self.filter == filter {
            return;
        }
        self.filter = filter;
        self.read = None;
        self.items = Arc::new(vec![]);
        self.more = false;
        self.next_cursor = None;
        self.total_count = None;
        self.refresh(false, cx);
        cx.notify();
    }
    fn save_config(
        &mut self,
        config: Config,
        operation: &'static str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.busy || !self.ready || self.quitting {
            return Err("Integration configuration is busy.".into());
        }
        let client = self.client.clone().ok_or("Settings unavailable")?;
        self.busy = true;
        self.error = None;
        cx.notify();
        self.write = Some(cx.spawn(async move |this, cx| {
            let result = client.save_integrations(config.clone()).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        let old_key = this.cache_key();
                        let old_target = this.target.clone();
                        this.config = config;
                        // Repository identity is already cached: switching Jira projects must not
                        // wait behind unrelated work on the libgit2 worker.
                        this.target = this
                            .repository
                            .as_ref()
                            .and_then(|r| this.config.task_project(r));
                        if old_target != this.target {
                            this.query.clear();
                            this.detail = None;
                        }
                        if old_key != this.cache_key() {
                            this.read = None;
                            this.items = Arc::new(vec![]);
                            this.loading = false;
                            this.more = false;
                            this.next_cursor = None;
                            this.total_count = None;
                            if this.visible {
                                this.refresh(true, cx);
                            }
                        }
                        this.resolve(cx);
                    }
                    Err(e) => this.error = Some(format!("{operation} could not be saved: {e}")),
                }
                cx.notify();
            });
        }));
        Ok(())
    }
    pub fn choose_task_filter(&mut self, id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let target = self.target.as_ref().ok_or("Choose a task project first.")?;
        let config = self.config.select_filter(target, id)?;
        self.save_config(config, "Task filter selection", cx)
    }
    pub fn save_task_filter(
        &mut self,
        filter: filters::TaskFilter,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let config = self.config.save_filter(filter)?;
        self.save_config(config, "Task filter", cx)
    }
    pub fn delete_task_filter(&mut self, id: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let config = self.config.remove_filter(id);
        self.save_config(config, "Task filter", cx)
    }
    pub fn override_repository(
        &mut self,
        value: Option<ProjectTarget>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let repo = self.repository.as_ref().ok_or("Select a Git project.")?;
        let mut config = self.config.clone();
        if let Some(value) = value {
            config.overrides.insert(repo.common.clone(), value);
        } else {
            config.overrides.remove(&repo.common);
        }
        self.save_config(config, "Repository override", cx)
    }
    pub fn link(
        &mut self,
        path: PathBuf,
        task: TaskRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let mut config = self.config.clone();
        let links = config.links.entry(path).or_default();
        if let Some(existing) = links.iter_mut().find(|t| t.same_task(&task)) {
            *existing = task;
        } else {
            links.push(task);
        }
        self.save_config(config, "Task link", cx)
    }
    pub fn unlink(
        &mut self,
        path: PathBuf,
        task: &TaskRef,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let mut config = self.config.clone();
        if let Some(links) = config.links.get_mut(&path) {
            links.retain(|t| !t.same_task(task));
        }
        self.save_config(config, "Task unlink", cx)
    }
    pub fn begin_quit(&mut self) -> Option<Task<()>> {
        self.quitting = true;
        self.read = None;
        self.origin = None;
        self.detail = None;
        self.write.take()
    }
    pub fn cancel_quit(&mut self, cx: &mut Context<Self>) {
        self.quitting = false;
        self.resolve(cx);
    }
}

async fn clean_retired(
    config: Config,
    client: SettingsClient,
    executor: BackgroundExecutor,
) -> (Config, Option<String>) {
    if config.retired_credentials.is_empty() {
        return (config, None);
    }
    let keys = config.retired_credentials.clone();
    let failed = executor
        .spawn(async move {
            keys.into_iter()
                .filter(|key| credentials::remove(key).is_err())
                .collect::<Vec<_>>()
        })
        .await;
    let warning = (!failed.is_empty()).then(|| {
        "Connection saved, but some old credentials could not be removed. Retry cleanup below."
            .to_owned()
    });
    let mut cleaned = config.clone();
    cleaned.retired_credentials = failed;
    if let Err(error) = client.save_integrations(cleaned.clone()).await {
        return (
            config,
            Some(format!("Could not save credential cleanup status: {error}")),
        );
    }
    (cleaned, warning)
}
