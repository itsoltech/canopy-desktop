//! Remote resource tools, including the inspector beside a normal chat pane.
//! Chat navigation is forwarded to the shared ChatView and local tab layout.
use super::*;

pub struct RemoteWorkspace {
    view: Entity<RemoteWindow>,
}
impl RemoteWorkspace {
    pub(in crate::ui) fn tools(
        host: Arc<HostClient>,
        workspace: Option<p::WorkspaceRef>,
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
        resource: p::ResourceRef,
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
    pub fn open(
        &mut self,
        host: Arc<HostClient>,
        resource: p::ResourceRef,
        cx: &mut Context<Self>,
    ) {
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
            view.discover(0, cx);
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
                    match resource.resource {
                        Some(p::resource_ref::Resource::Workspace(workspace))
                            if workspace.host_id == host_id =>
                        {
                            view.workspace = Some(workspace);
                            view.navigate(Page::Files, cx);
                            view.fields["path"]
                                .update(cx, |field, cx| field.set_value("", window, cx));
                            if view.ready() {
                                view.load_directory(cx);
                            }
                        }
                        Some(p::resource_ref::Resource::Chat(chat)) if chat.host_id == host_id => {
                            view.workspace = host.shared_catalog().and_then(|catalog| {
                                catalog
                                    .chats
                                    .iter()
                                    .find(|c| c.chat.as_ref() == Some(&chat))
                                    .and_then(|c| c.workspace.clone())
                            });
                            view.open_chat(chat, window, cx);
                        }
                        Some(p::resource_ref::Resource::Terminal(terminal))
                            if terminal.host_id == host_id =>
                        {
                            view.workspace = host.shared_catalog().and_then(|catalog| {
                                catalog
                                    .terminals
                                    .iter()
                                    .find(|t| t.terminal.as_ref() == Some(&terminal))
                                    .and_then(|t| t.workspace.clone())
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
    pub(super) fn embedded_navigation(&self, cx: &Context<Self>) -> impl IntoElement {
        row().gap_1().px_3().pb_2().flex_wrap().children(
            [
                ("new-remote-chat", "New chat", Page::Catalog),
                ("remote-files", "Files", Page::Files),
                ("remote-changes", "Changes", Page::Git),
                ("remote-terminals", "Terminals", Page::Terminals),
                ("remote-notes", "Notes", Page::Notes),
                ("remote-tasks", "Tasks", Page::Tasks),
            ]
            .map(|(id, label, page)| {
                button(id, label).on_click(cx.listener(move |view, _, _, cx| {
                    view.navigate(page, cx);
                    match page {
                        Page::Files => view.load_directory(cx),
                        Page::Git => view.load_git(cx),
                        Page::Notes => view.open_notes(cx),
                        Page::Tasks => view.load_tasks(cx),
                        Page::Catalog => view.discover(0, cx),
                        _ => {}
                    }
                }))
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
        let title = host.shared_catalog().and_then(|catalog| {
            if self.page == Page::Chat {
                self.chat
                    .as_ref()
                    .and_then(|reference| {
                        catalog
                            .chats
                            .iter()
                            .find(|chat| chat.chat.as_ref() == Some(reference))
                    })
                    .map(|chat| chat.title.clone())
            } else {
                self.workspace
                    .as_ref()
                    .and_then(|reference| {
                        catalog
                            .projects
                            .iter()
                            .flat_map(|p| &p.checkouts)
                            .find(|w| w.workspace.as_ref() == Some(reference))
                    })
                    .map(|w| w.branch.clone().unwrap_or_else(|| w.name.clone()))
            }
        });
        format!(
            "{} · {}",
            title.unwrap_or_else(|| "Private chats".into()),
            host.name()
        )
    }
}
impl RemoteWorkspace {
    pub(in crate::ui) fn open_file(&mut self, file: p::FileRef, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| {
            view.navigate(Page::Files, cx);
            view.open_remote_file(file, cx);
        });
    }
    pub(in crate::ui) fn show(&mut self, page: Page, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| {
            view.navigate(page, cx);
            match page {
                Page::Files => view.load_directory(cx),
                Page::Git => view.load_git(cx),
                Page::Tasks => view.load_tasks(cx),
                Page::Catalog => view.discover(0, cx),
                _ => {}
            }
        });
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
    pub(super) fn render_terminal_picker(&self, cx: &Context<Self>) -> AnyElement {
        let mut body = column().gap_2();
        if let Some(host) = &self.host
            && let Some(catalog) = host.shared_catalog()
        {
            for summary in catalog
                .terminals
                .iter()
                .filter(|t| t.workspace == self.workspace)
            {
                if let Some(terminal) = summary.terminal.clone() {
                    body = body.child(
                        button(
                            SharedString::from(format!("open-terminal:{}", terminal.terminal_id)),
                            summary.title.clone(),
                        )
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
        }
        body.child(
            button("new-workspace-terminal", "New terminal")
                .disabled(!self.ready() || self.workspace.is_none())
                .on_click(cx.listener(|view, _, _, cx| {
                    let Some(host) = view.host.clone() else {
                        return;
                    };
                    let workspace = view.workspace.clone();
                    view.run(
                        async move {
                            host.mutate(None, move |channel, command| async move {
                                p::terminal_service_client::TerminalServiceClient::new(channel)
                                    .open_terminal(p::OpenTerminalRequest {
                                        command: Some(command),
                                        workspace,
                                        tool_id: "shell".into(),
                                        profile_id: None,
                                    })
                                    .await
                            })
                            .await?;
                            Ok(Update::Done("Terminal created on this computer.".into()))
                        },
                        cx,
                    );
                })),
        )
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
            ensure_global(cx).unwrap();
            cx.global::<ClientGlobal>()
                .registry
                .set(registry)
                .unwrap_or_else(|_| panic!("new registry"));
        });
        let resource = |index: usize| p::ResourceRef {
            resource: Some(p::resource_ref::Resource::Workspace(p::WorkspaceRef {
                host_id: hosts[index].id().as_str().into(),
                workspace_id: format!("tree-{index}"),
            })),
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
                .is_some_and(|w| w.workspace_id == "tree-1")
        })
        .await;
        cx.update(|cx| {
            let remote = view.read(cx).view.read(cx);
            assert_eq!(remote.host.as_ref().unwrap().id(), hosts[1].id());
            assert_eq!(
                remote.workspace.as_ref().unwrap().host_id,
                hosts[1].id().as_str()
            );
            assert!(remote.embedded);
        });
    }
}
