//! Read-only scoped catalogs refresh independently from the Send mutation gate.
use super::*;
use canopy_desktop::remote::client::rpc_error;
enum Discovery {
    Agents(p::AgentDiscovery),
    Catalog(p::AgentCatalog),
    Pulls(p::PullRequests),
    Files(p::DirectoryEntries),
}
impl RemoteWindow {
    fn discover(&mut self, kind: u8, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        if self.discovery_busy {
            return;
        }
        let workspace = if self.page == Page::Chat {
            self.projection
                .summary
                .as_ref()
                .and_then(|s| s.workspace.clone())
        } else {
            self.workspace.clone()
        };
        let (agent_id, account_id) = if self.page == Page::Chat {
            self.projection
                .summary
                .as_ref()
                .map(|s| {
                    (
                        s.agent_id.clone(),
                        s.profile_id.clone().unwrap_or_else(|| s.agent_id.clone()),
                    )
                })
                .unwrap_or_default()
        } else {
            (self.value("agent", cx), self.value("profile", cx))
        };
        let query = self.value("query", cx);
        let token = (kind == 3)
            .then(|| self.pulls.as_ref().and_then(|p| p.next_page_token.clone()))
            .flatten();
        self.discovery_revision += 1;
        let revision = self.discovery_revision;
        let generation = self.generation;
        self.discovery_busy = true;
        let task = self.global.runtime.spawn(async move {
            tokio::time::timeout(std::time::Duration::from_secs(30), async move {
                let mut client =
                    p::catalog_service_client::CatalogServiceClient::new(host.channel()?);
                Ok::<_, String>(match kind {
                    0 => {
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
                        Discovery::Agents(p::AgentDiscovery {
                            agents,
                            next_page_token: None,
                        })
                    }
                    1 => Discovery::Catalog(
                        client
                            .get_agent_catalog(p::AgentCatalogRequest {
                                host_id: host.id().as_str().into(),
                                workspace,
                                agent_id,
                                account_id,
                                refresh: true,
                            })
                            .await
                            .map_err(rpc_error)?
                            .into_inner(),
                    ),
                    2 | 3 => Discovery::Pulls(
                        client
                            .list_pull_requests(p::PullRequestsRequest {
                                workspace,
                                query,
                                page_token: token,
                            })
                            .await
                            .map_err(rpc_error)?
                            .into_inner(),
                    ),
                    _ => Discovery::Files(
                        p::files_service_client::FilesServiceClient::new(host.channel()?)
                            .search_files(p::SearchFilesRequest {
                                workspace,
                                query,
                                limit: 100,
                            })
                            .await
                            .map_err(rpc_error)?
                            .into_inner(),
                    ),
                })
            })
            .await
            .map_err(|_| "Catalog request timed out. Retry the refresh.".to_owned())?
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
                    Ok(Discovery::Agents(value)) => this.agents = Some(value),
                    Ok(Discovery::Catalog(value)) => this.agent_catalog = Some(value),
                    Ok(Discovery::Pulls(value)) => this.pulls = Some(value),
                    Ok(Discovery::Files(value)) => this.mentions = Some(value),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn render_agent_discovery(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = column().gap_2().child(
            button(
                "discover-agents",
                if self.discovery_busy {
                    "Loading agents…"
                } else {
                    "Choose agent and account"
                },
            )
            .disabled(!self.ready() || self.discovery_busy)
            .on_click(cx.listener(|this, _, _, cx| this.discover(0, cx))),
        );
        body = body
            .child(match &self.workspace {
                Some(workspace) => format!("New chat destination: {}", workspace.workspace_id),
                None => "New chat destination: Private chats".into(),
            })
            .child(
                button("private-chat-destination", "Use Private chats").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.workspace = None;
                        this.agents = None;
                        this.agent_catalog = None;
                        this.discovery_revision += 1;
                        this.discovery_busy = false;
                        this.save_layout(cx);
                        cx.notify();
                    },
                )),
            );
        if let Some(discovery) = &self.agents {
            for agent in &discovery.agents {
                for account in &agent.accounts {
                    let agent_id = agent.agent_id.clone();
                    let account_id = account.account_id.clone();
                    let selected = self.value("agent", cx) == agent_id
                        && self.value("profile", cx) == account_id;
                    body = body.child(
                        button(
                            SharedString::from(format!("agent:{agent_id}:{account_id}")),
                            format!(
                                "{}{} · {}{}",
                                if selected { "✓ " } else { "" },
                                agent.name,
                                account.label,
                                if account.native_default {
                                    " (native account)"
                                } else {
                                    ""
                                }
                            ),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.fields["agent"]
                                    .update(cx, |s, cx| s.set_value(agent_id.clone(), window, cx));
                                this.fields["profile"].update(cx, |s, cx| {
                                    s.set_value(account_id.clone(), window, cx)
                                });
                                this.agent_catalog = None;
                                cx.notify();
                            },
                        )),
                    );
                }
            }
            if discovery.agents.is_empty() {
                body = body.child("No agents are available in this workspace.");
            }
        }
        let selected = self.agents.as_ref().is_some_and(|d| {
            d.agents.iter().any(|a| {
                a.agent_id == self.value("agent", cx)
                    && a.accounts
                        .iter()
                        .any(|p| p.account_id == self.value("profile", cx))
            })
        });
        body.child(
            button("new-chat", "New chat")
                .disabled(!self.ready() || !selected || self.workspace_closing())
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.workspace_closing() {
                        this.error = Some(
                            "This workspace is closing. Finish cleanup before creating a chat."
                                .into(),
                        );
                        cx.notify();
                        return;
                    }
                    let host = this.host.clone().unwrap();
                    let workspace = this.workspace.clone();
                    let agent_id = this.value("agent", cx);
                    let profile_id = Some(this.value("profile", cx));
                    this.run(
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
                })),
        )
        .into_any_element()
    }
    pub(super) fn render_composer_catalog(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = column()
            .gap_2()
            .child(
                button("discover-commands", "Refresh models, commands and skills")
                    .disabled(
                        self.discovery_busy
                            || self.host.as_ref().is_none_or(|h| h.phase() != Phase::Ready),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.discover(1, cx))),
            )
            .child(self.field("query", "Search commands, skills, files or pull requests"));
        let query = self.value("query", cx).to_lowercase();
        if let Some(catalog) = &self.agent_catalog {
            if let Some(error) = &catalog.error {
                body = body.child(error.clone());
            }
            if catalog.refreshing {
                body = body.child("Catalog refresh in progress. Send remains available.");
            }
            for command in catalog
                .commands
                .iter()
                .filter(|c| query.is_empty() || c.name.to_lowercase().contains(&query))
                .take(50)
            {
                let command = command.clone();
                body = body.child(
                    button(
                        SharedString::from(format!("command:{}", command.id)),
                        format!(
                            "{}{} · {}",
                            if command.skill { "$" } else { "/" },
                            command.name,
                            command.description
                        ),
                    )
                    .disabled(!self.supports("embedded_context"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.attachments.push(p::Attachment {
                            label: command.name.clone(),
                            content: Some(p::attachment::Content::Command(p::CommandAttachment {
                                name: command.name.clone(),
                                skill: command.skill,
                                path: command.path.clone(),
                                description: Some(command.description.clone()),
                            })),
                        });
                        this.save_context_attachments(cx);
                        cx.notify();
                    })),
                );
            }
            if catalog.commands.is_empty() {
                body = body.child("No commands or skills were returned for this account.");
            }
        }
        body = body.child(
            row()
                .gap_2()
                .child(
                    button("find-mentions", "Find files")
                        .disabled(self.discovery_busy || !self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.discover(4, cx))),
                )
                .child(
                    button("find-pulls", "Find pull requests")
                        .disabled(self.discovery_busy || !self.ready())
                        .on_click(cx.listener(|this, _, _, cx| this.discover(2, cx))),
                ),
        );
        if let Some(files) = &self.mentions {
            for entry in files.entries.iter().take(100).filter(|e| !e.directory) {
                let file = p::FileRef {
                    workspace: files.workspace.clone(),
                    relative_path: entry.relative_path.clone(),
                };
                body = body.child(
                    button(
                        SharedString::from(format!("mention:{}", entry.relative_path)),
                        entry.relative_path.clone(),
                    )
                    .disabled(!self.supports("embedded_context"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.attachments.push(p::Attachment {
                            label: file.relative_path.clone(),
                            content: Some(p::attachment::Content::Path(file.clone())),
                        });
                        this.save_context_attachments(cx);
                        cx.notify();
                    })),
                );
            }
            if files.incomplete {
                body = body.child("More matches exist. Refine the file search.");
            }
        }
        if let Some(pulls) = &self.pulls {
            for choice in &pulls.pulls {
                if let Some(mut pull) = choice.pull.clone() {
                    pull.context_id = Some(choice.context_id.clone());
                    body = body.child(
                        button(
                            SharedString::from(format!("pull:{}", choice.context_id)),
                            format!("#{} {} · {}", pull.number, pull.title, choice.author),
                        )
                        .disabled(!self.supports("embedded_context"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.attachments.push(p::Attachment {
                                label: format!("#{} {}", pull.number, pull.title),
                                content: Some(p::attachment::Content::Pull(pull.clone())),
                            });
                            this.save_context_attachments(cx);
                            cx.notify();
                        })),
                    );
                }
            }
            if pulls.next_page_token.is_some() {
                body = body.child(
                    button("more-pulls", "Next pull requests")
                        .disabled(self.discovery_busy)
                        .on_click(cx.listener(|this, _, _, cx| this.discover(3, cx))),
                );
            }
        }
        body.into_any_element()
    }
}

impl RemoteWindow {
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
