use super::{
    controls::{labeled, text, value},
    *,
};
use crate::ui::task_edit::composer::{ComposerChanged, MarkdownComposer};
use gpui_kit::component::{WindowExt, radio::RadioGroup};
use p::task_mutation_request::Action as Mutation;
mod attachments;
mod choices;
mod fields;
mod operations;
mod render;
use fields::FieldView;

type Client = p::tasks_service_client::TasksServiceClient<tonic::transport::Channel>;
#[derive(Clone, Copy, PartialEq)]
enum FormMode {
    Detail,
    Create,
    Fields,
    Transition,
}
#[derive(Clone)]
enum Choices {
    Projects,
    Metadata(&'static str),
    Types,
    Boards,
    Sprints,
    Links,
}
#[expect(
    clippy::large_enum_variant,
    reason = "The busy gate permits one pending task operation result, including its receipt and updated task DTO."
)]
enum ResultView {
    Integrations(p::TaskIntegrations, Option<p::WorkspaceRef>),
    Choices(Choices, p::TaskChoicePage, bool),
    Tasks(p::TaskPage, bool),
    Detail(p::Task),
    Schema(p::TaskSchema, FormMode),
    Comments(p::TaskCommentPage, bool),
    Receipt(p::CommandReceipt),
    Preview(
        Arc<canopy_desktop::integrations::attachments::PreviewFile>,
        String,
    ),
    UploadPrepared(String, p::TaskRef, String),
    UploadAttached(p::CommandReceipt),
    UploadDiscarded,
    Activity(p::TaskSchema, bool),
    Changed(p::CommandReceipt, Option<p::Task>, Option<String>),
}
pub(super) struct OpenWorkspace(pub(super) p::WorkspaceRef);
impl EventEmitter<OpenWorkspace> for TasksView {}
pub(super) struct TasksView {
    host: Arc<HostClient>,
    runtime: Arc<tokio::runtime::Runtime>,
    window: AnyWindowHandle,
    workspace: Option<p::WorkspaceRef>,
    integrations: Vec<p::TaskIntegration>,
    discovery_pending: bool,
    integration: Option<p::TaskIntegration>,
    project: Option<p::TaskChoice>,
    query: Entity<InputState>,
    state: usize,
    filter: Option<p::TaskFilter>,
    filter_name: Entity<InputState>,
    filter_expression: Entity<InputState>,
    tasks: Vec<p::Task>,
    next: Option<String>,
    filters: Vec<p::TaskFilter>,
    task: Option<p::Task>,
    choice_kind: Option<Choices>,
    choices: Vec<p::TaskChoice>,
    choice_next: Option<String>,
    choice_query: Entity<InputState>,
    schema: p::TaskSchema,
    fields: Vec<FieldView>,
    mode: FormMode,
    issue_type: Option<p::TaskChoice>,
    create_labels: Vec<String>,
    create_assignees: Vec<String>,
    create_milestone: Option<String>,
    board: Option<p::TaskChoice>,
    transition: Option<p::TaskStatusChoice>,
    title: Entity<InputState>,
    body: Entity<MarkdownComposer>,
    comment: Entity<MarkdownComposer>,
    comment_id: Option<String>,
    comment_rich: Option<FieldView>,
    activity: Vec<String>,
    activity_next: Option<String>,
    returned_workspace: Option<String>,
    comments: Vec<p::TaskComment>,
    comments_next: Option<String>,
    branch: Entity<InputState>,
    link_target: Entity<InputState>,
    link_outward: bool,
    work_seconds: Entity<InputState>,
    work_started: Entity<InputState>,
    attachment_path: Entity<InputState>,
    pending_upload: Option<(String, p::TaskRef, String)>,
    upload_attempted: bool,
    busy: bool,
    error: Option<String>,
    notice: String,
    pending: Option<Task<()>>,
    abort: Option<tokio::task::AbortHandle>,
    subscriptions: Vec<Subscription>,
    field_events: Vec<Subscription>,
    restoring: bool,
}
impl TasksView {
    pub(super) fn new(
        host: Arc<HostClient>,
        runtime: Arc<tokio::runtime::Runtime>,
        workspace: Option<p::WorkspaceRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut s = Self {
            host,
            runtime,
            workspace,
            window: window.window_handle(),
            integrations: vec![],
            discovery_pending: false,
            integration: None,
            project: None,
            query: text("", false, window, cx),
            state: 0,
            filter: None,
            filter_name: text("", false, window, cx),
            filter_expression: text("", false, window, cx),
            tasks: vec![],
            next: None,
            filters: vec![],
            task: None,
            choice_kind: None,
            choices: vec![],
            choice_next: None,
            choice_query: text("", false, window, cx),
            schema: Default::default(),
            fields: vec![],
            mode: FormMode::Detail,
            issue_type: None,
            create_labels: vec![],
            create_assignees: vec![],
            create_milestone: None,
            board: None,
            transition: None,
            title: text("", false, window, cx),
            body: cx.new(|cx| MarkdownComposer::new(String::new(), 180., window, cx)),
            comment: cx.new(|cx| MarkdownComposer::new(String::new(), 120., window, cx)),
            comment_id: None,
            comment_rich: None,
            activity: vec![],
            activity_next: None,
            returned_workspace: None,
            comments: vec![],
            comments_next: None,
            branch: text("", false, window, cx),
            link_target: text("", false, window, cx),
            link_outward: true,
            work_seconds: text("", false, window, cx),
            work_started: text("", false, window, cx),
            attachment_path: text("", false, window, cx),
            pending_upload: None,
            upload_attempted: false,
            busy: false,
            error: None,
            notice: String::new(),
            pending: None,
            abort: None,
            subscriptions: vec![],
            field_events: vec![],
            restoring: false,
        };
        s.subscriptions
            .push(cx.observe(&s.choice_query, |_, _, cx| cx.notify()));
        s.subscriptions
            .push(cx.subscribe(&s.title, |s, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    s.persist_draft(cx);
                }
            }));
        s.subscriptions
            .push(cx.subscribe(&s.body, |s, _, _: &ComposerChanged, cx| s.persist_draft(cx)));
        s.subscriptions
            .push(cx.subscribe(&s.comment, |s, _, _: &ComposerChanged, cx| {
                s.persist_draft(cx)
            }));
        s.restore_upload();
        s.load_integrations(cx);
        s
    }
    pub(super) fn set_workspace(
        &mut self,
        workspace: Option<p::WorkspaceRef>,
        cx: &mut Context<Self>,
    ) {
        // An open task keeps the workspace with which it was fetched. A new
        // project selection captures the navigation scope for subsequent reads.
        if self.workspace != workspace {
            self.workspace = workspace;
            self.integrations.clear();
            self.discovery_pending = true;
            if self.ready() {
                self.load_integrations(cx);
            }
            self.notice="Workspace changed. The open task retains its original scope; select a project to query the new scope.".into();
            cx.notify();
        }
    }
    fn ready(&self) -> bool {
        !self.busy && self.host.phase() == Phase::Ready
    }
    fn project_ref(&self) -> Result<p::TaskProject, String> {
        Ok(p::TaskProject {
            host_id: self.host.id().as_str().into(),
            integration_id: self
                .integration
                .as_ref()
                .ok_or("Choose an integration.")?
                .id
                .clone(),
            project_id: self.project.as_ref().ok_or("Choose a project.")?.id.clone(),
            workspace: self.workspace.clone(),
        })
    }
    fn task_ref(&self) -> Result<p::TaskRef, String> {
        self.task
            .as_ref()
            .and_then(|t| t.task.clone())
            .ok_or_else(|| "Open a task first.".into())
    }
    fn provider(&self) -> &str {
        self.integration
            .as_ref()
            .map(|i| i.provider.as_str())
            .unwrap_or("")
    }
    fn supported(&self, operation: &str) -> bool {
        self.task.as_ref().is_some_and(|t| {
            t.capabilities
                .iter()
                .any(|c| c.operation == operation && c.supported)
        })
    }
    fn run<F>(&mut self, future: F, cx: &mut Context<Self>)
    where
        F: std::future::Future<Output = Result<ResultView, String>> + Send + 'static,
    {
        if !self.ready() {
            return;
        }
        self.busy = true;
        self.error = None;
        self.body.update(cx, |v, cx| v.enable(false, cx));
        self.comment.update(cx, |v, cx| v.enable(false, cx));
        for f in &self.fields {
            f.enable(false, cx);
        }
        if let Some(f) = &self.comment_rich {
            f.enable(false, cx);
        }
        let job = self.runtime.spawn(future);
        self.abort = Some(job.abort_handle());
        self.pending = Some(cx.spawn(async move |this, cx| {
            let result = job.await.map_err(|e| format!("Task operation interrupted: {e}. Query operation results before retrying a write.")).and_then(|v| v);
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                s.abort = None;
                s.body.update(cx, |v, cx| v.enable(true, cx));
                s.comment.update(cx, |v, cx| v.enable(true, cx));
                for f in &s.fields { f.enable(true, cx); }
                if let Some(f) = &s.comment_rich { f.enable(true, cx); }
                match result {
                    Ok(r) => s.install(r, cx),
                    Err(e) => s.error = Some(e),
                }
                if s.discovery_pending && s.ready() { s.load_integrations(cx); }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn install(&mut self, result: ResultView, cx: &mut Context<Self>) {
        match result {
            ResultView::Integrations(s, workspace) => {
                if workspace != self.workspace {
                    return;
                }
                self.integrations = s.integrations;
            }
            ResultView::Choices(kind, page, more) => {
                self.choice_kind = Some(kind);
                if !more {
                    self.choices.clear();
                }
                for c in page.choices {
                    if !self.choices.iter().any(|v| v.id == c.id) {
                        self.choices.push(c);
                    }
                }
                self.choice_next = page.next_page_token;
            }
            ResultView::Tasks(page, more) => {
                if !more {
                    self.tasks.clear();
                }
                for t in page.tasks {
                    if let Some(old) = self.tasks.iter_mut().find(|v| v.task == t.task) {
                        *old = t;
                    } else {
                        self.tasks.push(t);
                    }
                }
                self.next = page.next_page_token;
                self.filters = page.filters;
                self.notice = format!("{} tasks loaded", self.tasks.len());
            }
            ResultView::Detail(task) => {
                self.task = Some(task.clone());
                self.mode = FormMode::Detail;
                self.fields.clear();
                self.comments.clear();
                self.comments_next = None;
                self.comment_id = None;
                self.comment_rich = None;
                self.activity.clear();
                self.activity_next = None;
                self.schema = Default::default();
                let title = self.title.clone();
                let body = self.body.clone();
                let comment = self.comment.clone();
                let saved = self
                    .host
                    .snapshot()
                    .record
                    .view(&self.draft_key())
                    .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok());
                self.restoring = true;
                let _ = self.window.update(cx, |_, w, cx| {
                    title.update(cx, |v, cx| {
                        v.set_value(
                            saved
                                .as_ref()
                                .and_then(|v| v["title"].as_str())
                                .unwrap_or(&task.title)
                                .to_owned(),
                            w,
                            cx,
                        )
                    });
                    body.update(cx, |v, cx| {
                        v.set_value(
                            saved
                                .as_ref()
                                .and_then(|v| v["body"].as_str())
                                .unwrap_or(&task.description),
                            w,
                            cx,
                        )
                    });
                    comment.update(cx, |v, cx| v.set_value("", w, cx));
                });
                self.restoring = false;
            }
            ResultView::Schema(schema, mode) => {
                self.mode = mode;
                self.schema = schema.clone();
                let provider = self.provider().to_owned();
                let current = self.task.clone();
                let fields = schema
                    .fields
                    .into_iter()
                    .map(|mut f| {
                        if f.current_value.is_none() {
                            f.current_value = current
                                .as_ref()
                                .and_then(|t| t.fields.iter().find(|v| v.id == f.id))
                                .and_then(|v| v.current_value.clone());
                        }
                        f
                    })
                    .collect::<Vec<_>>();
                if let Ok(fields) = self.window.update(cx, |_, w, cx| {
                    fields
                        .into_iter()
                        .map(|f| FieldView::new(f, &provider, mode == FormMode::Create, w, cx))
                        .collect()
                }) {
                    self.fields = fields;
                    self.field_events =
                        self.fields.iter().filter_map(|f| f.subscribe(cx)).collect();
                    let saved = self
                        .host
                        .snapshot()
                        .record
                        .view(&self.draft_key())
                        .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok());
                    if let Some(saved) = saved {
                        self.restoring = true;
                        let _ = self.window.update(cx, |_, w, cx| {
                            for f in &self.fields {
                                if let Some(value) = saved["fields"].get(f.id(cx)) {
                                    f.restore(value, w, cx);
                                }
                            }
                        });
                        self.restoring = false;
                    }
                }
            }
            ResultView::Comments(page, more) => {
                if !more {
                    self.comments.clear();
                }
                for c in page.comments {
                    if let Some(old) = self.comments.iter_mut().find(|v| v.id == c.id) {
                        *old = c;
                    } else {
                        self.comments.push(c);
                    }
                }
                self.comments_next = page.next_page_token;
            }
            ResultView::Receipt(receipt) => {
                let result = receipt
                    .operation_output
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
                self.returned_workspace = result
                    .as_ref()
                    .and_then(|r| r["workspace_id"].as_str())
                    .map(str::to_owned);
                self.notice = result
                    .as_ref()
                    .and_then(|r| r["notice"].as_str())
                    .unwrap_or("Change applied.")
                    .to_owned();
                if let Some(id) = &self.returned_workspace {
                    self.notice = format!(
                        "Worktree created: {id}. Its original task and workspace scope were preserved."
                    );
                }
                if let Some(id) = result.as_ref().and_then(|r| r["deleted_task_id"].as_str()) {
                    self.tasks
                        .retain(|t| t.task.as_ref().is_none_or(|t| t.task_id != id));
                    self.task = None;
                }
                if let Some(id) = result
                    .as_ref()
                    .and_then(|r| r["deleted_comment_id"].as_str())
                {
                    self.comments.retain(|c| c.id != id);
                }
                if let Some(id) = result.as_ref().and_then(|r| r["task_id"].as_str()) {
                    self.notice.push_str(&format!(" Task: {id}."));
                }
            }
            ResultView::Preview(file, media) => {
                if let Err(e) = super::super::preview::open(file, media, cx) {
                    self.error = Some(e.to_string());
                }
            }
            ResultView::UploadPrepared(id, task, name) => {
                self.pending_upload = Some((id, task, name));
                self.upload_attempted = false;
                self.notice="File prepared on this device. Upload and attach to the original task when ready.".into();
            }
            ResultView::Activity(schema, more) => {
                if !more {
                    self.activity.clear();
                }
                self.activity.extend(schema.activity);
                self.activity_next = schema.next_page_token;
            }
            ResultView::Changed(receipt, task, error) => {
                self.install(ResultView::Receipt(receipt), cx);
                if let Some(task) = task {
                    if let Some(row) = self.tasks.iter_mut().find(|t| t.task == task.task) {
                        *row = task.clone();
                    }
                    self.task = Some(task);
                    if self.mode == FormMode::Create {
                        self.mode = FormMode::Detail;
                        self.fields.clear();
                    }
                }
                if let Some(e) = error {
                    self.notice.push_str(&format!(" Refresh failed: {e}. The write is confirmed; refresh without repeating it."));
                }
            }
            ResultView::UploadDiscarded => {
                self.pending_upload = None;
                self.upload_attempted = false;
                self.notice = "Local attachment removed; no task mutation was replayed.".into();
            }
            ResultView::UploadAttached(receipt) => {
                self.pending_upload = None;
                self.upload_attempted = false;
                self.install(ResultView::Receipt(receipt), cx);
            }
        }
    }
    fn fail(&mut self, e: impl Into<String>, cx: &mut Context<Self>) {
        self.error = Some(e.into());
        cx.notify();
    }
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        self.persist_draft(cx);
    }
    fn draft_key(&self) -> String {
        if let Some(task) = self.task.as_ref().and_then(|t| t.task.as_ref()) {
            format!(
                "task-draft:{}:{}:{}:{}",
                task.integration_id,
                task.project_id,
                task.workspace
                    .as_ref()
                    .map(|w| w.workspace_id.as_str())
                    .unwrap_or("global"),
                task.task_id
            )
        } else {
            format!(
                "task-draft:new:{}:{}:{}",
                self.integration
                    .as_ref()
                    .map(|i| i.id.as_str())
                    .unwrap_or(""),
                self.project.as_ref().map(|p| p.id.as_str()).unwrap_or(""),
                self.workspace
                    .as_ref()
                    .map(|w| w.workspace_id.as_str())
                    .unwrap_or("global")
            )
        }
    }
    fn persist_draft(&mut self, cx: &mut Context<Self>) {
        if self.restoring || (self.task.is_none() && self.mode != FormMode::Create) {
            return;
        }
        let fields: serde_json::Map<String, serde_json::Value> = self
            .fields
            .iter()
            .filter(|f| !matches!(f, FieldView::ReadOnly(_)))
            .map(|f| (f.id(cx), f.draft(cx)))
            .collect();
        let draft=serde_json::json!({"title":value(&self.title,cx),"body":self.body.read(cx).value(cx),"fields":fields,"comment_id":self.comment_id,"comment":self.comment.read(cx).value(cx),"comment_document":self.comment_rich.as_ref().map(|f|f.draft(cx))}).to_string();
        let host = self.host.clone();
        let key = self.draft_key();
        let revision = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let job = self
            .runtime
            .spawn_blocking(move || host.save_view_version(key, draft, revision));
        cx.spawn(async move |this, cx| {
            let result = job.await.map_err(|e| e.to_string()).and_then(|v| v);
            let _ = this.update(cx, |s, cx| {
                match result {
                    Ok(()) => s.notice = "Draft saved on this device.".into(),
                    Err(e) => s.error = Some(format!("Could not save draft: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
impl Drop for TasksView {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
    }
}
