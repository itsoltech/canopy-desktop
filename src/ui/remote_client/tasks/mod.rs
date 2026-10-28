use super::{controls::value, *};
use crate::ui::{
    components::task_detail::DetailTab,
    markdown::MarkdownView,
    task_edit::composer::{ComposerChanged, MarkdownComposer},
};
use gpui_kit::component::{
    WindowExt,
    select::{SelectEvent, SelectState},
};
use p::task_mutation_request::Action as Mutation;
mod adapter;
mod attachments;
mod choices;
mod fields;
mod operations;
mod render;
mod sections;
use fields::FieldView;

type Client = p::tasks_service_client::TasksServiceClient<tonic::transport::Channel>;
#[derive(Clone, Copy, PartialEq)]
enum FormMode {
    Detail,
    Create,
    Fields,
    Transition,
}
#[derive(Clone, PartialEq)]
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
/// The request currently occupying the busy gate, used only for its presentation.
#[derive(Clone, Copy, PartialEq)]
enum Loading {
    None,
    Tasks,
    MoreTasks,
    Task,
    Comments,
    Projects,
    Save,
    Other,
}
/// Inline Jira forms shown in the task details column. One is open at a time.
#[derive(Clone, Copy, PartialEq)]
enum DetailAction {
    LogWork,
    Link,
}
type Picker = Entity<SelectState<Vec<SelectOption>>>;
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
    integration_picker: Picker,
    project: Option<p::TaskChoice>,
    projects: Vec<p::TaskChoice>,
    projects_next: Option<String>,
    project_picker: Picker,
    filter_picker: Picker,
    manage_filters: bool,
    query: Entity<InputState>,
    state: usize,
    filter: Option<p::TaskFilter>,
    filter_name: Entity<InputState>,
    filter_expression: Entity<InputState>,
    tasks: Vec<p::Task>,
    task_list: UniformListScrollHandle,
    next: Option<String>,
    filters: Vec<p::TaskFilter>,
    task: Option<p::Task>,
    description: Entity<MarkdownView>,
    tab: DetailTab,
    editing: bool,
    action: Option<DetailAction>,
    width: f32,
    choice_kind: Option<Choices>,
    choices: Vec<p::TaskChoice>,
    choice_next: Option<String>,
    choice_query: Entity<InputState>,
    /// Display names learned from option lists, keyed by opaque provider id.
    names: BTreeMap<String, String>,
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
    comment_views: BTreeMap<String, Entity<MarkdownView>>,
    comments_loaded: bool,
    comments_next: Option<String>,
    branch: Entity<InputState>,
    link_target: Entity<InputState>,
    link_outward: bool,
    work_duration: Entity<InputState>,
    work_started: Entity<InputState>,
    work_comment: Entity<InputState>,
    pending_upload: Option<(String, p::TaskRef, String)>,
    upload_attempted: bool,
    busy: bool,
    loading: Loading,
    refresh_loading: ButtonLoading,
    projects_loading: ButtonLoading,
    more_loading: ButtonLoading,
    save_loading: ButtonLoading,
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
        let picker = |window: &mut Window, cx: &mut Context<Self>, searchable: bool| {
            cx.new(|cx| {
                SelectState::new(Vec::<SelectOption>::new(), None, window, cx)
                    .searchable(searchable)
            })
        };
        let integration_picker = picker(window, cx, false);
        let project_picker = picker(window, cx, true);
        let filter_picker = picker(window, cx, true);
        let mut s = Self {
            host,
            runtime,
            workspace,
            window: window.window_handle(),
            integrations: vec![],
            discovery_pending: false,
            integration: None,
            integration_picker,
            project: None,
            projects: vec![],
            projects_next: None,
            project_picker,
            filter_picker,
            manage_filters: false,
            query: placeholder("Search tasks…", window, cx),
            state: 0,
            filter: None,
            filter_name: placeholder("Filter name", window, cx),
            filter_expression: placeholder("Condition", window, cx),
            tasks: vec![],
            task_list: UniformListScrollHandle::new(),
            next: None,
            filters: vec![],
            task: None,
            description: cx.new(MarkdownView::reading),
            tab: DetailTab::Description,
            editing: false,
            action: None,
            width: 0.,
            choice_kind: None,
            choices: vec![],
            choice_next: None,
            choice_query: placeholder("Filter available values…", window, cx),
            names: BTreeMap::new(),
            schema: Default::default(),
            fields: vec![],
            mode: FormMode::Detail,
            issue_type: None,
            create_labels: vec![],
            create_assignees: vec![],
            create_milestone: None,
            board: None,
            transition: None,
            title: placeholder("Title", window, cx),
            body: cx.new(|cx| MarkdownComposer::new(String::new(), 180., window, cx)),
            comment: cx.new(|cx| MarkdownComposer::new(String::new(), 120., window, cx)),
            comment_id: None,
            comment_rich: None,
            activity: vec![],
            activity_next: None,
            returned_workspace: None,
            comments: vec![],
            comment_views: BTreeMap::new(),
            comments_loaded: false,
            comments_next: None,
            branch: placeholder("Branch name", window, cx),
            link_target: placeholder("Issue key, for example CAN-42", window, cx),
            link_outward: true,
            work_duration: placeholder("1h 30m", window, cx),
            work_started: placeholder("YYYY-MM-DD HH:MM", window, cx),
            work_comment: placeholder("What did you work on?", window, cx),
            pending_upload: None,
            upload_attempted: false,
            busy: false,
            loading: Loading::None,
            refresh_loading: ButtonLoading::default(),
            projects_loading: ButtonLoading::default(),
            more_loading: ButtonLoading::default(),
            save_loading: ButtonLoading::default(),
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
            .push(cx.subscribe(&s.query, |s, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) && s.project.is_some() {
                    s.search(false, cx);
                }
            }));
        s.subscriptions.push(cx.subscribe(
            &s.integration_picker,
            |s, _, event: &SelectEvent<Vec<SelectOption>>, cx| {
                if let SelectEvent::Confirm(Some(id)) = event {
                    s.pick_integration(id.as_ref(), cx);
                }
            },
        ));
        s.subscriptions.push(cx.subscribe(
            &s.project_picker,
            |s, _, event: &SelectEvent<Vec<SelectOption>>, cx| {
                if let SelectEvent::Confirm(Some(id)) = event {
                    s.pick_project(id.as_ref(), cx);
                }
            },
        ));
        s.subscriptions.push(cx.subscribe_in(
            &s.filter_picker,
            window,
            |s, _, event: &SelectEvent<Vec<SelectOption>>, window, cx| {
                if let SelectEvent::Confirm(Some(id)) = event {
                    s.choose_filter(id.as_ref(), window, cx);
                }
            },
        ));
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
            self.notice = "Workspace changed. The open task keeps its original workspace; choose a project to browse this one.".into();
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
        self.run_as(Loading::Other, future, cx);
    }
    fn run_as<F>(&mut self, loading: Loading, future: F, cx: &mut Context<Self>)
    where
        F: std::future::Future<Output = Result<ResultView, String>> + Send + 'static,
    {
        if !self.ready() {
            return;
        }
        self.busy = true;
        self.loading = loading;
        self.refresh_loading.set(loading == Loading::Tasks, cx);
        self.projects_loading.set(loading == Loading::Projects, cx);
        self.more_loading.set(loading == Loading::MoreTasks, cx);
        self.save_loading.set(loading == Loading::Save, cx);
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
            let result = job
                .await
                .map_err(|_| {
                    "The request was interrupted. Refresh the task before you retry a change."
                        .to_owned()
                })
                .and_then(|v| v);
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                s.abort = None;
                s.loading = Loading::None;
                s.refresh_loading.set(false, cx);
                s.projects_loading.set(false, cx);
                s.more_loading.set(false, cx);
                s.save_loading.set(false, cx);
                s.body.update(cx, |v, cx| v.enable(true, cx));
                s.comment.update(cx, |v, cx| v.enable(true, cx));
                for f in &s.fields {
                    f.enable(true, cx);
                }
                if let Some(f) = &s.comment_rich {
                    f.enable(true, cx);
                }
                match result {
                    Ok(r) => s.install(r, cx),
                    Err(e) => s.error = Some(e),
                }
                if s.discovery_pending && s.ready() {
                    s.load_integrations(cx);
                }
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
                self.sync_integration_picker(cx);
                // A single configured account needs no choice; its projects load directly.
                let mut configured = self.integrations.iter().filter(|i| i.configured);
                if self.integration.is_none()
                    && let (Some(only), None) = (configured.next(), configured.next())
                {
                    let only = only.clone();
                    self.choose_integration(only, cx);
                }
            }
            ResultView::Choices(Choices::Projects, page, more) => {
                if !more {
                    self.projects.clear();
                }
                for c in page.choices {
                    if let Some(old) = self.projects.iter_mut().find(|v| v.id == c.id) {
                        *old = c;
                    } else {
                        self.projects.push(c);
                    }
                }
                self.projects_next = page.next_page_token;
                self.sync_project_picker(cx);
            }
            ResultView::Choices(kind, page, more) => {
                if !more && self.choice_kind.as_ref() != Some(&kind) {
                    let query = self.choice_query.clone();
                    let _ = self.window.update(cx, |_, w, cx| {
                        query.update(cx, |v, cx| v.set_value("", w, cx))
                    });
                }
                if matches!(kind, Choices::Metadata(_)) {
                    for c in &page.choices {
                        self.names.insert(c.id.clone(), c.label.clone());
                    }
                }
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
                if self.filters != page.filters {
                    self.filters = page.filters;
                    self.sync_filter_picker(cx);
                }
            }
            ResultView::Detail(task) => {
                let same = self.task.as_ref().and_then(|t| t.task.as_ref()) == task.task.as_ref();
                self.task = Some(task.clone());
                self.sync_description(cx);
                self.mode = FormMode::Detail;
                self.action = None;
                if !same {
                    self.tab = DetailTab::Description;
                    self.choice_kind = None;
                }
                if let Some(row) = self.tasks.iter_mut().find(|t| t.task == task.task) {
                    *row = task.clone();
                }
                self.fields.clear();
                self.comments.clear();
                self.comment_views.clear();
                self.comments_loaded = false;
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
                // A restored local draft stays visible in the editor instead of
                // silently differing from the provider's description.
                self.editing = saved.as_ref().is_some_and(|v| {
                    v["title"].as_str().is_some_and(|t| t != task.title)
                        || v["body"].as_str().is_some_and(|b| b != task.description)
                });
                if self.tab == DetailTab::Comments {
                    self.load_comments(false, cx);
                }
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
                self.comments_loaded = true;
                self.sync_comment_views(cx);
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
                if self.returned_workspace.is_some() {
                    self.notice = "Worktree created.".into();
                }
                if let Some(id) = result.as_ref().and_then(|r| r["deleted_task_id"].as_str()) {
                    self.tasks
                        .retain(|t| t.task.as_ref().is_none_or(|t| t.task_id != id));
                    self.task = None;
                    self.editing = false;
                    self.action = None;
                }
                if let Some(id) = result
                    .as_ref()
                    .and_then(|r| r["deleted_comment_id"].as_str())
                {
                    self.comments.retain(|c| c.id != id);
                    self.comment_views.remove(id);
                    if self.comment_id.as_deref() == Some(id) {
                        self.comment_id = None;
                        self.comment_rich = None;
                    }
                }
                if let Some(id) = result.as_ref().and_then(|r| r["task_id"].as_str()) {
                    self.notice.push_str(&format!(" Task {id}."));
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
                self.notice = format!(
                    "“{}” is ready to upload.",
                    self.pending_upload
                        .as_ref()
                        .map(|u| u.2.as_str())
                        .unwrap_or("File")
                );
            }
            ResultView::Activity(schema, more) => {
                if !more {
                    self.activity.clear();
                }
                self.activity.extend(schema.activity);
                self.activity_next = schema.next_page_token;
            }
            ResultView::Changed(receipt, task, error) => {
                let commented = receipt
                    .operation_output
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .is_some_and(|r| r["comment_id"].is_string());
                self.install(ResultView::Receipt(receipt), cx);
                if let Some(task) = task {
                    if let Some(row) = self.tasks.iter_mut().find(|t| t.task == task.task) {
                        *row = task.clone();
                    }
                    let same =
                        self.task.as_ref().and_then(|t| t.task.as_ref()) == task.task.as_ref();
                    self.task = Some(task);
                    self.sync_description(cx);
                    if !same {
                        self.tab = DetailTab::Description;
                        self.comments.clear();
                        self.comment_views.clear();
                        self.comments_loaded = false;
                        self.comments_next = None;
                    }
                    if self.mode == FormMode::Create {
                        self.mode = FormMode::Detail;
                        self.fields.clear();
                    }
                    if matches!(self.mode, FormMode::Fields | FormMode::Transition) {
                        self.mode = FormMode::Detail;
                        self.fields.clear();
                        self.transition = None;
                    }
                    if let Some(task) = &self.task
                        && value(&self.title, cx) == task.title
                        && self.body.read(cx).value(cx) == task.description
                    {
                        self.editing = false;
                    }
                    self.action = None;
                    if commented {
                        self.comment_id = None;
                        self.comment_rich = None;
                        let comment = self.comment.clone();
                        let _ = self.window.update(cx, |_, w, cx| {
                            comment.update(cx, |v, cx| v.set_value("", w, cx))
                        });
                    }
                    if self.tab == DetailTab::Comments {
                        self.load_comments(false, cx);
                    }
                }
                if let Some(e) = error {
                    self.notice.push_str(&format!(
                        " The change was saved, but the task couldn’t be refreshed: {e}"
                    ));
                }
            }
            ResultView::UploadDiscarded => {
                self.pending_upload = None;
                self.upload_attempted = false;
                self.notice = "Prepared file removed.".into();
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
                    Err(e) => s.error = Some(format!("Couldn’t save the draft: {e}")),
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
fn placeholder(text: &'static str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).placeholder(text))
}
