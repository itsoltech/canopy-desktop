//! A host-owned chat opens a normal pane; tab and split ids are local.
use super::*;
use canopy_desktop::{
    api::proto,
    remote::client::HostClient,
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
            .filter(|r| r.read(cx).project_tools_context().is_some())
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
        let Some((host, workspace)) = runtime.project_tools_context() else {
            return;
        };
        let workspace = workspace.and_then(|id| {
            host.shared_catalog()
                .and_then(|c| remote_client::checkout(&c, &id))
        });
        let key = format!(
            "{}:{}",
            host.id().as_str(),
            workspace
                .as_ref()
                .map(|w| w.catalog_id.as_str())
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
            runtime.update(cx, |r, cx| r.create_sibling(cx));
        }
        true
    }

    pub(super) fn open_remote_chat(
        &mut self,
        host: Arc<HostClient>,
        chat: String,
        cx: &mut Context<Self>,
    ) {
        let Some(reference) = RemoteChat::new(host.id().as_str().into(), chat.clone()) else {
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
                c.threads
                    .iter()
                    .find(|s| s.catalog_id == chat)
                    .and_then(|s| s.summary.clone())
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
    workspace: Option<proto::ProjectCheckout>,
    cx: &mut App,
) {
    crate::app_state::ChatRuntime::create_on_environment(
        host,
        workspace.map(|workspace| workspace.catalog_id),
        cx,
    );
}
