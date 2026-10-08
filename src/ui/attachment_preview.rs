#[cfg(target_os = "macos")]
use super::attachment_native::NativePreview;
use crate::{
    app_state::{AppState, WriteFinished},
    ui::{components::integrations::integration_message, components::*, theme as t},
};
use canopy_desktop::{
    integrations::{
        AccountScope, TaskRef, TaskWrite,
        attachments::PreviewFile,
        credentials,
        jira::{Jira, JiraWrite},
        youtrack::Youtrack,
    },
    motion::{self, Presence, presets},
};
use gpui_kit::{
    base::Disableable,
    component::{IconName, Root},
    *,
};
use std::{path::PathBuf, sync::Arc, time::Instant};
pub fn open(
    task: TaskRef,
    id: String,
    name: String,
    directory: PathBuf,
    cx: &mut App,
) -> Result<WindowHandle<Root>> {
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(900.), px(680.)),
                cx,
            ))),
            window_min_size: Some(size(px(480.), px(320.))),
            titlebar: Some(titlebar_options(format!("{} — {name}", task.label()))),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| AttachmentPreview::new(task, id, name, directory, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
}
struct AttachmentPreview {
    focus: FocusHandle,
    task: TaskRef,
    id: String,
    name: String,
    directory: PathBuf,
    credential: Option<String>,
    file: Option<Arc<PreviewFile>>,
    #[cfg(target_os = "macos")]
    native: Option<std::rc::Rc<NativePreview>>,
    close_sender: async_channel::Sender<()>,
    loading: bool,
    saving: bool,
    save_loading: ButtonLoading,
    error: Option<String>,
    notice: Option<String>,
    reveal: Presence,
    read: Option<Task<()>>,
    save: Option<Task<()>>,
    _close: Task<()>,
    _events: Vec<Subscription>,
}
impl AttachmentPreview {
    fn new(
        task: TaskRef,
        id: String,
        name: String,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let initial = focus.clone();
        window.on_next_frame(move |w, cx| initial.focus(w, cx));
        let (close_sender, close_receiver) = async_channel::bounded(1);
        let close_task = cx.spawn_in(window, async move |_, cx| {
            if close_receiver.recv().await.is_ok() {
                let _ = cx.update(|window, _| window.remove_window());
            }
        });
        let state = cx.global::<AppState>().integrations.clone();
        let credential = state
            .read(cx)
            .config
            .account_for(&task.project)
            .map(|a| a.credential.clone());
        let events = vec![
            cx.observe_in(
                &cx.global::<AppState>().settings.clone(),
                window,
                |_, state, window, cx| {
                    if state.read(cx).quitting {
                        window.remove_window();
                    }
                },
            ),
            cx.observe_in(&state, window, |s, state, window, cx| {
                if state
                    .read(cx)
                    .config
                    .account_for(&s.task.project)
                    .map(|a| &a.credential)
                    != s.credential.as_ref()
                {
                    window.remove_window();
                }
            }),
            cx.subscribe_in(&state, window, |s, _, e: &WriteFinished, window, _| {
                let removed = match &e.command {
                    TaskWrite::Jira {
                        action: JiraWrite::DeleteIssue { .. },
                        ..
                    } => true,
                    TaskWrite::Jira {
                        action: JiraWrite::DeleteAttachment { id },
                        ..
                    } => id == &s.id,
                    TaskWrite::Youtrack {
                        action: canopy_desktop::integrations::youtrack::YoutrackWrite::DeleteIssue { .. },
                        ..
                    } => true,
                    TaskWrite::Youtrack {
                        action: canopy_desktop::integrations::youtrack::YoutrackWrite::DeleteAttachment { id },
                        ..
                    } => id == &s.id,
                    _ => false,
                };
                if e.result.is_ok()
                    && removed
                    && e.command.task().is_some_and(|t| t.same_task(&s.task))
                {
                    window.remove_window();
                }
            }),
        ];
        let mut this = Self {
            focus,
            task,
            id,
            name,
            directory,
            credential,
            file: None,
            #[cfg(target_os = "macos")]
            native: None,
            close_sender,
            loading: false,
            saving: false,
            save_loading: ButtonLoading::default(),
            error: None,
            notice: None,
            reveal: Presence::new(false, presets::CONTENT_REVEAL, Instant::now()),
            read: None,
            save: None,
            _close: close_task,
            _events: events,
        };
        this.load(window, cx);
        this
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(account) = cx
            .global::<AppState>()
            .integrations
            .read(cx)
            .config
            .account_for(&self.task.project)
            .cloned()
        else {
            self.error = Some("Connect this task provider first.".into());
            return;
        };
        let task = self.task.clone();
        let id = self.id.clone();
        let name = self.name.clone();
        let http = cx.http_client();
        self.loading = true;
        self.error = None;
        self.read=Some(cx.spawn_in(window,async move|this,cx|{
            let result=async {let key=account.credential.clone();let token=cx.background_executor().spawn(async move{credentials::load(&key)}).await?;
                let bytes = match &account.scope {
                    AccountScope::Jira {site,email,cloud_id} => Jira::new(http.clone(),site,email,cloud_id.as_deref(),token,account.login)?.download_attachment(&task,&id).await?,
                    AccountScope::Youtrack { service } => Youtrack::new(http.clone(),service,token,account.login)?.download_attachment(&task,&id).await?,
                    AccountScope::Default | AccountScope::Owner(_) => return Err("Attachment preview requires a task tracker connection.".into()),
                };
                cx.background_executor().spawn(async move{PreviewFile::create(&name,&bytes).map(Arc::new)}).await
            }.await;
            let _=this.update_in(cx,|s,window,cx|{s.loading=false;match result{Ok(file)=>{
                #[cfg(target_os="macos")] match NativePreview::new(file.clone(),s.close_sender.clone(),window){Ok(preview)=>s.native=Some(std::rc::Rc::new(preview)),Err(e)=>s.error=Some(e)}
                s.file=Some(file);s.reveal.set_open(true,Instant::now(),motion::policy(cx));
                #[cfg(not(target_os="macos"))] {s.error=Some("System attachment preview is available on macOS. You can still save this file.".into());}
            },Err(e)=>s.error=Some(e)}cx.notify();});
        }));
        cx.notify();
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(file) = self.file.clone() else {
            return;
        };
        let name = canopy_desktop::integrations::attachments::preview_name(&self.name);
        let picker = cx.prompt_for_new_path(&self.directory, Some(&name));
        self.saving = true;
        self.save_loading.set(true, cx);
        self.notice = None;
        self.save = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let path = picker
                    .await
                    .map_err(|_| "Save dialog closed.")?
                    .map_err(|e| e.to_string())?;
                let Some(path) = path else {
                    return Ok(false);
                };
                cx.background_executor()
                    .spawn(async move { file.save_to(&path) })
                    .await?;
                Ok::<_, String>(true)
            }
            .await;
            let _ = this.update(cx, |s, cx| {
                s.saving = false;
                s.save_loading.set(false, cx);
                match result {
                    Ok(true) => s.notice = Some("Attachment saved.".into()),
                    Ok(false) => {}
                    Err(e) => s.error = Some(e),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
impl Render for AttachmentPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let progress = self.reveal.progress(now);
        motion::request_frame(window, self.reveal.is_animating(now));
        #[cfg(target_os = "macos")]
        let native = self.native.clone();
        column()
            .id("attachment-preview")
            .track_focus(&self.focus)
            .on_key_down(|event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    || event.keystroke.modifiers.platform && event.keystroke.key == "w"
                {
                    cx.stop_propagation();
                    window.remove_window();
                }
            })
            .size_full()
            .bg(t::bg())
            .text_color(t::text())
            .text_size(px(13.))
            .child(
                window_titlebar("attachment-titlebar", window)
                    .h(px(40.))
                    .flex_shrink_0()
                    .justify_center()
                    .child("Canopy — Attachment"),
            )
            .child(
                row()
                    .h(px(56.))
                    .px(px(20.))
                    .gap(px(12.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(t::border())
                    .child(
                        column()
                            .flex_1()
                            .gap(px(3.))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(self.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t::muted())
                                    .child(format!(
                                        "{} · {}",
                                        self.task.label(),
                                        self.task.project.site.as_deref().unwrap_or("")
                                    )),
                            ),
                    )
                    .child(
                        loading_button_with_icon(
                            "save-attachment",
                            "Save as…",
                            custom_icon("download"),
                            &self.save_loading,
                        )
                        .disabled(
                            (self.file.is_none() || self.saving) && !self.save_loading.active(),
                        )
                        .on_click(cx.listener(|s, _, _, cx| s.save(cx))),
                    )
                    .child(
                        icon_button("close-attachment", IconName::Close, "Close preview (Esc)")
                            .on_click(|_, w, _| w.remove_window()),
                    ),
            )
            .children(
                self.error
                    .clone()
                    .map(|e| div().p(px(12.)).child(integration_message(e, true))),
            )
            .children(
                self.notice
                    .clone()
                    .map(|e| div().p(px(12.)).child(integration_message(e, false))),
            )
            .child(
                column()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .children(self.loading.then(|| {
                        column()
                            .w(px(220.))
                            .gap(px(12.))
                            .child(skeleton_bar().h(px(100.)).w_full())
                            .child(
                                div()
                                    .text_color(t::secondary())
                                    .child("Loading attachment…"),
                            )
                    }))
                    .children((!self.loading && self.file.is_none()).then(|| {
                        button("retry-preview", "Retry")
                            .on_click(cx.listener(|s, _, w, cx| s.load(w, cx)))
                    }))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, _, _| {
                                #[cfg(target_os = "macos")]
                                if let Some(native) = &native {
                                    native.frame(bounds, progress);
                                }
                            },
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
    }
}
