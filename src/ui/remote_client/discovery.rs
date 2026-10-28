//! New chat: destination, agent/account choice and creation. Agent discovery is a
//! READ-scoped refresh that never waits for, or blocks, the mutation gate.
use super::*;
use canopy_desktop::remote::client::rpc_error;
use chrome::*;
use gpui_kit::component::{IconName, button::ButtonVariants};

impl RemoteWindow {
    pub(super) fn discover(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        if self.discovery_busy {
            return;
        }
        let workspace = self.workspace.clone();
        self.discovery_revision += 1;
        let revision = self.discovery_revision;
        let generation = self.generation;
        self.discovery_busy = true;
        if self.scope == Some("discover") {
            self.error = None;
        }
        let task = self.global.runtime.spawn(async move {
            tokio::time::timeout(std::time::Duration::from_secs(30), async move {
                let mut client =
                    p::catalog_service_client::CatalogServiceClient::new(host.channel()?);
                let mut agents = Vec::new();
                let mut token = None;
                let mut seen = std::collections::HashSet::new();
                loop {
                    let page = client
                        .list_agents(p::AgentDiscoveryRequest {
                            host_id: host.id().as_str().into(),
                            workspace: workspace.clone(),
                            page_token: token,
                        })
                        .await
                        .map_err(rpc_error)?
                        .into_inner();
                    agents.extend(page.agents);
                    token = page.next_page_token;
                    if let Some(next) = &token {
                        if !seen.insert(next.clone()) || seen.len() > 256 {
                            return Err("Agent catalog pagination did not advance.".into());
                        }
                    } else {
                        break;
                    }
                }
                Ok::<_, String>(p::AgentDiscovery {
                    agents,
                    next_page_token: None,
                })
            })
            .await
            .map_err(|_| "Loading agents timed out. Try again.".to_owned())?
        });
        self.discovery_abort = Some(task.abort_handle());
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|_| Err("Catalog worker stopped.".into()));
            let _ = this.update(cx, |this, cx| {
                if this.discovery_revision != revision {
                    return;
                }
                this.discovery_busy = false;
                this.discovery_abort = None;
                if this.generation != generation {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(value) => this.agents = Some(value),
                    Err(error) => {
                        this.error = Some(error);
                        this.scope = Some("discover");
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn create_chat(&mut self, cx: &mut Context<Self>) {
        if self.workspace_closing() {
            self.error =
                Some("This workspace is closing. Finish cleanup before creating a chat.".into());
            self.scope = Some("new-chat");
            cx.notify();
            return;
        }
        let Some(host) = self.host.clone() else {
            return;
        };
        let workspace = self.workspace.clone();
        let agent_id = self.value("agent", cx);
        let profile_id = Some(self.value("profile", cx));
        self.run_in(
            "new-chat",
            async move {
                host.mutate(None, move |channel, command| async move {
                    p::catalog_service_client::CatalogServiceClient::new(channel)
                        .create_chat(p::CreateChatRequest {
                            command: Some(command),
                            workspace,
                            agent_id,
                            profile_id,
                        })
                        .await
                })
                .await?;
                Ok(Update::Done("Chat created. Open it from Projects.".into()))
            },
            cx,
        );
    }
    pub(super) fn render_agent_discovery(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let ready = self.ready();
        let agent = self.value("agent", cx);
        let profile = self.value("profile", cx);
        let destination = column()
            .gap(px(8.))
            .child(section_title("DESTINATION"))
            .child(
                row()
                    .h(px(32.))
                    .gap(px(8.))
                    .px(px(8.))
                    .rounded(px(4.))
                    .bg(t::hover())
                    .child(match &self.workspace {
                        Some(_) => custom_icon("git-branch"),
                        None => icon(IconName::Inbox),
                    })
                    .child(row_text(
                        self.workspace_label()
                            .unwrap_or_else(|| "Private chats".into()),
                        if self.workspace.is_some() {
                            "Checkout"
                        } else {
                            "Not tied to a project"
                        },
                    ))
                    .children(self.workspace.is_some().then(|| {
                        button("private-chat-destination", "Use Private chats")
                            .ghost()
                            .border_0()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace = None;
                                this.agents = None;
                                this.discovery_revision += 1;
                                this.discovery_busy = false;
                                this.save_layout(cx);
                                cx.notify();
                            }))
                    })),
            );
        let mut agents = column()
            .gap(px(4.))
            .child(
                section_title("AGENT AND ACCOUNT").children(self.agents.is_some().then(|| {
                    icon_button("refresh-agents", IconName::RotateCw, "Refresh agents")
                        .loading(self.discovery_busy)
                        .disabled(!ready || self.discovery_busy)
                        .on_click(cx.listener(|this, _, _, cx| this.discover(cx)))
                })),
            );
        match &self.agents {
            None if self.discovery_busy => agents = agents.child(loading("Loading agents…")),
            None => {
                agents = agents.child(
                    row().child(
                        button("discover-agents", "Choose agent and account")
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, _, cx| this.discover(cx))),
                    ),
                )
            }
            Some(discovery) if discovery.agents.is_empty() => {
                agents = agents.child(
                    div()
                        .py(px(4.))
                        .text_size(px(12.))
                        .text_color(t::muted())
                        .child("No agents are available in this workspace."),
                )
            }
            Some(discovery) => {
                for available in &discovery.agents {
                    for account in &available.accounts {
                        let agent_id = available.agent_id.clone();
                        let account_id = account.account_id.clone();
                        let selected = agent == agent_id && profile == account_id;
                        agents = agents.child(
                            list_row(
                                SharedString::from(format!("agent:{agent_id}:{account_id}")),
                                format!("{} · {}", available.name, account.label),
                                selected,
                            )
                            .child(icon(IconName::Bot).flex_shrink_0())
                            .child(row_text(available.name.clone(), account.label.clone()))
                            .children(account.native_default.then(|| badge("Default account")))
                            .children(selected.then(|| {
                                icon(IconName::Check).size(px(12.)).text_color(t::accent())
                            }))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.fields["agent"].update(cx, |s, cx| {
                                        s.set_value(agent_id.clone(), window, cx)
                                    });
                                    this.fields["profile"].update(cx, |s, cx| {
                                        s.set_value(account_id.clone(), window, cx)
                                    });
                                    cx.notify();
                                },
                            )),
                        );
                    }
                }
            }
        }
        agents = agents.children(self.notice("discover"));
        let selected = self.agents.as_ref().is_some_and(|d| {
            d.agents
                .iter()
                .any(|a| a.agent_id == agent && a.accounts.iter().any(|p| p.account_id == profile))
        });
        column()
            .gap(px(20.))
            .child(destination)
            .child(agents)
            .children(self.notice("new-chat"))
            .child(
                row().justify_end().child(
                    primary_button("new-chat", "Create chat")
                        .loading(self.pending == Some("new-chat"))
                        .disabled(!ready || !selected || self.workspace_closing())
                        .on_click(cx.listener(|this, _, _, cx| this.create_chat(cx))),
                ),
            )
            .into_any_element()
    }
    fn workspace_closing(&self) -> bool {
        let Some(workspace) = &self.workspace else {
            return false;
        };
        self.host
            .as_ref()
            .and_then(|h| h.catalog())
            .is_some_and(|catalog| {
                catalog
                    .projects
                    .iter()
                    .flat_map(|p| &p.checkouts)
                    .any(|w| w.workspace.as_ref() == Some(workspace) && w.closing.unwrap_or(false))
            })
    }
}
