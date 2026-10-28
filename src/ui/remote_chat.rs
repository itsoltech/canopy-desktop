//! A host-owned chat opens a normal pane; tab and split ids are local.
use super::*;
use canopy_desktop::{
    remote::{client::HostClient, proto},
    state::workspace::{PaneKind, PaneMetadata, RemoteChat},
};
use std::sync::Arc;
impl Workspace {
    pub(super) fn focused_remote(&self, cx: &App) -> Option<Entity<crate::app_state::ChatRuntime>> {
        let pane = self.state.workspace.read(cx).active()?.focused;
        self.state
            .chats
            .read(cx)
            .runtime(pane)
            .filter(|r| r.read(cx).is_remote())
            .cloned()
    }
    pub(super) fn sync_remote_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use canopy_desktop::state::workspace::ToolPage;
        if !self.state.workspace.read(cx).tools_open() {
            return;
        }
        let Some(source) = self.focused_remote(cx) else {
            self.remote_tools = None;
            return;
        };
        let runtime = source.read(cx);
        let Some(client) = runtime.remote_source() else {
            return;
        };
        let Some(host) = client.host.clone() else {
            return;
        };
        let workspace = client.context().and_then(|c| c.workspace);
        let key = format!(
            "{}:{}",
            host.id().as_str(),
            workspace
                .as_ref()
                .map(|w| w.workspace_id.as_str())
                .unwrap_or("private")
        );
        let page = self.state.workspace.read(cx).tool_panel().page();
        let remote_page = match page {
            ToolPage::Files => remote_client::Page::Files,
            ToolPage::Changes => remote_client::Page::Git,
            ToolPage::Tasks => remote_client::Page::Tasks,
            _ => remote_client::Page::Terminals,
        };
        if let Some((current, last, view)) = &mut self.remote_tools
            && *current == key
        {
            if *last != page {
                view.update(cx, |v, cx| v.show(remote_page, cx));
                *last = page;
            }
            return;
        }
        let view = cx.new(|cx| {
            remote_client::RemoteWorkspace::tools(host, workspace, remote_page, window, cx)
        });
        self.remote_tools = Some((key, page, view));
    }
    pub(super) fn new_chat_on_active_host(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self
            .state
            .workspace
            .read(cx)
            .active()
            .and_then(|t| t.root.find(t.focused))
            .filter(|p| p.metadata.remote.is_some())
            .cloned()
        else {
            return false;
        };
        if let Some(runtime) = self.state.chats.read(cx).runtime(pane.id).cloned() {
            runtime.update(cx, |r, cx| r.client_new_chat(cx));
        }
        true
    }
    pub(super) fn open_remote_chat(
        &mut self,
        host: Arc<HostClient>,
        chat: proto::ChatRef,
        cx: &mut Context<Self>,
    ) {
        let Some(reference) = RemoteChat::new(chat.host_id.clone(), chat.chat_id.clone()) else {
            return;
        };
        if host.id().as_str() != reference.host {
            return;
        }
        let existing = self
            .state
            .projects
            .read(cx)
            .runtime_workspaces(cx)
            .iter()
            .flat_map(|w| w.all_panes())
            .find(|p| p.metadata.remote.as_deref() == Some(&reference));
        if let Some(pane) = existing {
            self.state
                .projects
                .update(cx, |p, cx| p.focus_agent(pane.id, cx));
        } else {
            let summary = host.shared_catalog().and_then(|c| {
                c.chats
                    .iter()
                    .find(|s| s.chat.as_ref() == Some(&chat))
                    .cloned()
            });
            let title = summary
                .as_ref()
                .map(|s| s.title.clone())
                .unwrap_or_else(|| "Chat".into());
            let tool = summary
                .as_ref()
                .map(|s| s.agent_id.clone())
                .unwrap_or_default();
            let target = self.state.projects.read(cx).catalog.active;
            let result = self.state.projects.update(cx, |p, cx| {
                p.open_chat(
                    target,
                    &title,
                    &tool,
                    PaneMetadata {
                        kind: PaneKind::Chat,
                        title: Some(title.clone()),
                        remote: Some(Box::new(reference)),
                        ..Default::default()
                    },
                    cx,
                )
            });
            if let Err(error) = result {
                self.state.chats.update(cx, |chats, cx| {
                    chats.error = Some(error);
                    cx.notify();
                });
            }
        }
        self.sidebar_overlay = false;
        cx.notify();
    }
}

/// Starts a chat on another computer, in `workspace` or as a private chat, and
/// opens it here. The agent is the one a new chat here would use when the host
/// offers it, else the host's first.
pub(crate) fn new_chat_on(
    host: Arc<HostClient>,
    workspace: Option<proto::WorkspaceRef>,
    cx: &mut App,
) {
    let app = cx.global::<crate::app_state::AppState>().clone();
    let preferred = crate::app_state::Chats::provider(cx).tool().to_owned();
    let executor = match remote_client::executor(cx) {
        Ok(executor) => executor,
        Err(error) => {
            app.chats.update(cx, |chats, cx| {
                chats.error = Some(error.to_string());
                cx.notify();
            });
            return;
        }
    };
    let task = executor.spawn(async move {
        let mut client = proto::catalog_service_client::CatalogServiceClient::new(host.channel()?);
        let agents = client
            .list_agents(proto::AgentDiscoveryRequest {
                host_id: host.id().as_str().into(),
                workspace: workspace.clone(),
                page_token: None,
            })
            .await
            .map_err(canopy_desktop::remote::client::rpc_error)?
            .into_inner()
            .agents;
        let agent = agents
            .iter()
            .find(|a| a.agent_id == preferred)
            .or(agents.first())
            .map(|a| a.agent_id.clone())
            .ok_or_else(|| format!("{} has no agents set up.", host.name()))?;
        let receipt = host
            .mutate(None, |channel, command| {
                let agent = agent.clone();
                async move {
                    proto::catalog_service_client::CatalogServiceClient::new(channel)
                        .create_chat(proto::CreateChatRequest {
                            command: Some(command),
                            workspace,
                            agent_id: agent,
                            profile_id: None,
                        })
                        .await
                }
            })
            .await?;
        let chat = RemoteChat::new(host.id().as_str().into(), receipt.resource_id)
            .ok_or_else(|| "The computer returned an invalid chat.".to_owned())?;
        Ok::<_, String>((chat, agent))
    });
    cx.spawn(async move |cx| {
        let result = task.await.unwrap_or_else(|e| Err(e.to_string()));
        cx.update(|cx| {
            let opened = result.and_then(|(chat, agent)| {
                let target = app.projects.read(cx).catalog.active;
                app.projects.update(cx, |p, cx| {
                    p.open_chat(
                        target,
                        "New chat",
                        &agent,
                        PaneMetadata {
                            kind: PaneKind::Chat,
                            title: Some("New chat".into()),
                            remote: Some(Box::new(chat)),
                            ..Default::default()
                        },
                        cx,
                    )
                })
            });
            if let Err(error) = opened {
                app.chats.update(cx, |chats, cx| {
                    chats.error = Some(error);
                    cx.notify();
                });
            }
        })
    })
    .detach();
}
