//! Remote resource tools, including the inspector beside a normal chat pane.
//! Chat navigation is forwarded to the shared ChatView and local tab layout.
use super::*;

pub struct RemoteWorkspace {
    view: Entity<RemoteWindow>,
}
impl RemoteWorkspace {
    pub(in crate::ui) fn tools(
        host: Arc<HostClient>,
        workspace: Option<p::ProjectCheckout>,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = cx.new(|cx| {
            let mut view = RemoteWindow::new(window, cx);
            view.embedded = true;
            view.compact_tools = true;
            view.host = Some(host);
            view.workspace = workspace;
            view.page = page;
            view.connect_on_open = true;
            view
        });
        Self { view }
    }
    pub fn new(
        host: Arc<HostClient>,
        resource: ResourceView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = cx.new(|cx| {
            let mut view = RemoteWindow::new(window, cx);
            view.embedded = true;
            view.host = Some(host);
            view.requested_resource = Some(resource);
            view.connect_on_open = true;
            view
        });
        Self { view }
    }
    pub fn open(&mut self, host: Arc<HostClient>, resource: ResourceView, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| {
            if view.registry.is_none() {
                view.host = Some(host);
                view.requested_resource = Some(resource);
                view.connect_on_open = true;
                return;
            }
            if view
                .host
                .as_ref()
                .is_none_or(|current| current.id() != host.id())
            {
                view.select_host(host.clone(), cx);
            }
            view.requested_resource = Some(resource);
            view.apply_requested_resource(cx);
            view.connect_embedded(host, cx);
        });
    }
    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| {
            view.navigate(Page::Catalog, cx);
            view.discover(cx);
        });
    }
    pub fn close_project(&mut self, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| {
            if let Some(workspace) = view.workspace.clone() {
                view.close_project(workspace, cx);
            }
        });
    }
}
impl Render for RemoteWorkspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

impl RemoteWindow {
    pub(super) fn apply_requested_resource(&mut self, cx: &mut Context<Self>) {
        if self.registry.is_none() {
            return;
        }
        let Some(resource) = self.requested_resource.clone() else {
            return;
        };
        let owner = cx.entity().downgrade();
        let window = self.window;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = owner.update(cx, |view, cx| {
                    if view.requested_resource.as_ref() != Some(&resource) {
                        return;
                    }
                    let Some(host) = view.host.clone() else {
                        return;
                    };
                    let host_id = host.id().as_str();
                    match resource {
                        ResourceView::Project(workspace)
                            if workspace
                                .project
                                .as_ref()
                                .is_some_and(|p| p.host_id == host_id) =>
                        {
                            view.workspace = Some(workspace);
                            view.navigate(Page::Files, cx);
                            view.fields["path"].update(cx, |f, cx| f.set_value("", window, cx));
                            if view.ready() {
                                view.load_directory(cx);
                            }
                        }
                        ResourceView::Thread(chat) if chat.host == host_id => {
                            view.workspace = host.shared_catalog().and_then(|c| {
                                let id = c
                                    .threads
                                    .iter()
                                    .find(|t| t.catalog_id == chat.chat)?
                                    .project_catalog_id
                                    .as_ref()?;
                                checkout(&c, id)
                            });
                            view.open_chat(chat.chat, window, cx);
                        }
                        ResourceView::Terminal(terminal) if terminal.host_id == host_id => {
                            view.workspace = host.shared_catalog().and_then(|c| {
                                let id = &c
                                    .terminals
                                    .iter()
                                    .find(|t| t.terminal.as_ref() == Some(&terminal))?
                                    .project_catalog_id;
                                checkout(&c, id)
                            });
                            view.navigate(Page::Terminal, cx);
                            view.terminal = Some(terminal);
                            if view.ready() {
                                view.open_terminal(cx);
                            }
                        }
                        _ => view.error = Some("This resource belongs to another computer.".into()),
                    }
                    cx.notify();
                });
            });
        });
    }
}

impl RemoteWindow {
    /// Tab strip of the embedded workspace; selection mirrors the visible page.
    pub(super) fn embedded_navigation(&self, inset: f32, cx: &Context<Self>) -> Div {
        use gpui_kit::component::IconName;
        let current = match self.page {
            Page::Terminal => Page::Terminals,
            Page::Chat => Page::Catalog,
            page => page,
        };
        row()
            .flex_shrink_0()
            .gap(px(4.))
            .px(px(inset))
            .py(px(8.))
            .border_b_1()
            .border_color(t::border())
            .flex_wrap()
            .children(
                [
                    (
                        "new-remote-chat",
                        "New chat",
                        icon(IconName::Plus),
                        Page::Catalog,
                    ),
                    ("remote-files", "Files", icon(IconName::Folder), Page::Files),
                    (
                        "remote-changes",
                        "Changes",
                        custom_icon("git-branch"),
                        Page::Git,
                    ),
                    (
                        "remote-terminals",
                        "Terminals",
                        custom_icon("terminal"),
                        Page::Terminals,
                    ),
                    (
                        "remote-notes",
                        "Notes",
                        icon(IconName::FileText),
                        Page::Notes,
                    ),
                    ("remote-tasks", "Tasks", custom_icon("issue"), Page::Tasks),
                ]
                .map(|(id, label, glyph, page)| {
                    let selected = current == page;
                    selection_button(id, label, selected)
                        .border_0()
                        .icon(glyph.text_color(if selected { t::accent() } else { t::muted() }))
                        .text_color(if selected { t::text() } else { t::secondary() })
                        .on_click(cx.listener(move |view, _, _, cx| view.show_page(page, cx)))
                }),
            )
    }
}

impl RemoteWindow {
    pub(super) fn connect_embedded(&mut self, host: Arc<HostClient>, cx: &mut Context<Self>) {
        if host.phase() != Phase::Offline {
            return;
        }
        let id = host.id().clone();
        let ready = prepare_connection(cx);
        let task = self.global.runtime.spawn(async move {
            ready.await?;
            host.connect().await
        });
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|_| Err("Connection task stopped.".into()));
            let _ = this.update(cx, |this, cx| {
                if this.host.as_ref().is_none_or(|host| host.id() != &id) {
                    return;
                }
                match result {
                    Ok(()) => this.refresh_connection(cx),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn selection_label(&self) -> String {
        let Some(host) = &self.host else {
            return "Remote computer".into();
        };
        let title = self.workspace_label();
        format!(
            "{} · {}",
            title.unwrap_or_else(|| "Private chats".into()),
            host.name()
        )
    }
}
impl RemoteWorkspace {
    pub(in crate::ui) fn open_thread_file(
        &mut self,
        file: &crate::app_state::ThreadFile,
        cx: &mut Context<Self>,
    ) {
        let Some((host, workspace, path)) = file.environment_address() else {
            return;
        };
        let project = self
            .view
            .read(cx)
            .host
            .as_ref()
            .filter(|h| h.id().as_str() == host)
            .and_then(|h| h.shared_catalog())
            .and_then(|c| checkout(&c, workspace))
            .and_then(|c| c.project);
        let Some(project) = project else {
            return;
        };
        self.open_file(
            p::FileRef {
                target: Some(p::ResourceTarget {
                    resource: Some(p::resource_target::Resource::Project(project)),
                }),
                relative_path: path.to_string_lossy().into_owned(),
            },
            cx,
        );
    }

    pub(in crate::ui) fn open_file(&mut self, file: p::FileRef, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| {
            view.navigate(Page::Files, cx);
            view.open_remote_file(file, cx);
        });
    }
    pub(in crate::ui) fn show(&mut self, page: Page, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| view.show_page(page, cx));
    }
    pub fn save(&mut self, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| {
            if view.page == Page::Files {
                view.save_file(cx);
            }
        });
    }
    pub fn label(&self, cx: &App) -> String {
        self.view.read(cx).selection_label()
    }
}

impl RemoteWindow {
    /// Starts a shell in the selected checkout. Loading shows on controls that check
    /// `pending == Some("new-terminal")`; the result is `notice("new-terminal")`.
    pub(super) fn new_shell_terminal(&mut self, cx: &mut Context<Self>) {
        self.new_terminal("shell".into(), None, cx);
    }
    pub(super) fn render_terminal_picker(&self, cx: &Context<Self>) -> AnyElement {
        use gpui_kit::component::IconName;
        let ready = self.ready();
        let mut list = column()
            .gap(px(4.))
            .child(chrome::section_title("TERMINALS"));
        let terminals = self
            .host
            .as_ref()
            .and_then(|host| host.shared_catalog())
            .map(|catalog| {
                catalog
                    .terminals
                    .iter()
                    .filter(|t| {
                        self.workspace
                            .as_ref()
                            .is_some_and(|w| w.catalog_id == t.project_catalog_id)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if terminals.is_empty() {
            list = list.child(
                div()
                    .py(px(4.))
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("No terminals in this checkout yet."),
            );
        }
        for summary in terminals {
            if let Some(terminal) = summary.terminal.clone() {
                list = list.child(
                    chrome::list_row(
                        SharedString::from(format!("open-terminal:{}", terminal.terminal_id)),
                        format!("Open terminal {}", summary.title),
                        self.terminal.as_ref() == Some(&terminal),
                    )
                    .child(custom_icon("terminal").flex_shrink_0())
                    .child(chrome::row_text(
                        summary.title.clone(),
                        if summary.running { "Running" } else { "Exited" },
                    ))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.navigate(Page::Terminal, cx);
                        view.terminal = Some(terminal.clone());
                        view.terminal_frame = None;
                        if view.ready() {
                            view.open_terminal(cx);
                        }
                    })),
                );
            }
        }
        let list = list
            .child(
                row().pt(px(8.)).child(
                    button("new-workspace-terminal", "New terminal")
                        .icon(icon(IconName::Plus))
                        .loading(self.pending == Some("new-terminal"))
                        .disabled(!ready || self.workspace.is_none())
                        .on_click(cx.listener(|view, _, _, cx| view.new_shell_terminal(cx))),
                ),
            )
            .children(self.notice("new-terminal"));
        // The shell gives this page its full area; it supplies its own inset and scroll.
        let inset = if self.compact_tools {
            chrome::COMPACT_X
        } else if self.embedded {
            20.
        } else {
            chrome::CONTENT_X
        };
        div()
            .id("remote-terminals-scroll")
            .size_full()
            .overflow_y_scroll()
            .child(list.px(px(inset)).py(px(16.)))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientGlobal, ClientRegistry, RemoteWorkspace, Root, ensure_global, p};
    use canopy_desktop::remote::client::store::{CredentialStore, HostRecord};
    use gpui_kit::{AppContext, TestAppContext, px, size, test::TestAppContextExt};
    use std::{sync::Arc, time::Duration};

    struct Offline;
    impl CredentialStore for Offline {
        fn load(&self, _: &str) -> Result<String, String> {
            Err("No credentials in this test".into())
        }
        fn save(&self, _: &str, _: &str) -> Result<(), String> {
            panic!("no pairing")
        }
        fn remove(&self, _: &str) -> Result<(), String> {
            panic!("no revoke")
        }
    }

    #[gpui_kit::test]
    async fn selecting_another_computer_while_the_registry_loads_keeps_the_requested_destination(
        cx: &mut TestAppContext,
    ) {
        cx.background_executor.allow_parking();
        let directory = tempfile::tempdir().unwrap();
        let registry =
            ClientRegistry::open(directory.path().join("clients.json"), Arc::new(Offline)).unwrap();
        let mut hosts = vec![];
        for name in ["first", "second"] {
            let id = uuid::Uuid::new_v4().to_string();
            let record: HostRecord = serde_json::from_value(serde_json::json!({
                "id":id,"name":name,"endpoint":"https://127.0.0.1:1","server_name":"paired-host","certificate":[],"fingerprint":[],"device_id":"device","credential_ref":"no-credentials","ssh":null,"last_seen":null,"epoch":null,"closing_epoch":null,"catalog":[],"outbox":[],"mutations":[],"views":{},"uploads":[],"chats":{}
            })).unwrap();
            hosts.push(registry.insert(record).unwrap());
        }
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::ui::theme::init(cx);
            crate::app_state::AppState::install_test(
                canopy_desktop::state::workspace::Workspace::new(),
                cx,
            );
            ensure_global(cx).unwrap();
            cx.global::<ClientGlobal>()
                .registry
                .set(registry)
                .unwrap_or_else(|_| panic!("new registry"));
        });
        let resource = |index: usize| {
            super::super::ResourceView::Project(p::ProjectCheckout {
                catalog_id: format!("tree-{index}"),
                project: Some(p::ProjectRef {
                    host_id: hosts[index].id().as_str().into(),
                    project_id: format!("project-{index}"),
                    worktree_id: None,
                }),
                ..Default::default()
            })
        };
        let mut view = None;
        let window = cx.open_window(size(px(900.), px(700.)), |window, cx| {
            let entity =
                cx.new(|cx| RemoteWorkspace::new(hosts[0].clone(), resource(0), window, cx));
            entity.update(cx, |view, cx| view.open(hosts[1].clone(), resource(1), cx));
            view = Some(entity.clone());
            Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        cx.wait_for(window.into(), Duration::from_secs(2), |_, cx| {
            let remote = view.read(cx).view.read(cx);
            remote
                .workspace
                .as_ref()
                .is_some_and(|w| w.catalog_id == "tree-1")
        })
        .await;
        cx.update(|cx| {
            let remote = view.read(cx).view.read(cx);
            assert_eq!(remote.host.as_ref().unwrap().id(), hosts[1].id());
            assert_eq!(
                remote
                    .workspace
                    .as_ref()
                    .unwrap()
                    .project
                    .as_ref()
                    .unwrap()
                    .host_id,
                hosts[1].id().as_str()
            );
            assert!(remote.embedded);
        });
    }
}
