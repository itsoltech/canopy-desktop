//! Projects: the computer's checkouts, chats and terminals, plus opening projects.
use super::*;
use chrome::*;
use gpui_kit::component::{IconName, button::ButtonVariants};

impl RemoteWindow {
    pub(super) fn open_project(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let host_path = self.value("project_path", cx);
        if !self.ready() || host_path.trim().is_empty() {
            return;
        }
        self.run_in(
            "open-project",
            async move {
                host.open_project(host_path).await?;
                Ok(Update::Done("Project opened on the other computer.".into()))
            },
            cx,
        );
    }
    fn select_workspace(&mut self, reference: p::ProjectCheckout, cx: &mut Context<Self>) {
        self.navigate(Page::Files, cx);
        self.workspace = Some(reference);
        self.agents = None;
        self.discovery_revision += 1;
        self.discovery_busy = false;
        self.file = None;
        self.file_hash.clear();
        self.file_dirty = false;
        self.directory = None;
        self.git = None;
        self.git_history = None;
        self.git_refs = None;
        self.git_diff = None;
        self.save_layout(cx);
        self.load_directory(cx);
    }
    pub(super) fn select_terminal(&mut self, terminal: api::TerminalRef, cx: &mut Context<Self>) {
        self.navigate(Page::Terminal, cx);
        self.terminal = Some(terminal);
        self.terminal_frame = None;
        self.terminal_history = None;
        self.terminal_history_offset = 0;
        self.terminal_run.clear();
        self.terminal_selection = None;
        self.save_layout(cx);
        self.open_terminal(cx);
    }
    pub(super) fn new_terminal(
        &mut self,
        tool_id: String,
        profile_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let workspace = self.workspace.clone();
        self.run_in(
            "new-terminal",
            async move {
                host.open_terminal(
                    workspace.ok_or("Project is missing.")?.catalog_id,
                    tool_id,
                    profile_id,
                )
                .await?;
                Ok(Update::Done(
                    "Terminal started. It appears in the list when the computer reports it.".into(),
                ))
            },
            cx,
        );
    }
    pub(super) fn render_catalog(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.embedded {
            return self.render_agent_discovery(cx);
        }
        let Some(host) = self.host.clone() else {
            return empty(
                "No computer selected",
                "Add a computer or choose one on the left.",
            )
            .into_any_element();
        };
        let ready = self.ready();
        let open = column()
            .gap(px(8.))
            .child(section_title("OPEN PROJECT"))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        input(&self.fields["project_path"])
                            .flex_1()
                            .disabled(!ready && self.pending != Some("open-project")),
                    )
                    .child(
                        button("open-project", "Open")
                            .loading(self.pending == Some("open-project"))
                            .disabled(!ready || self.value("project_path", cx).trim().is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.open_project(cx))),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child("A folder path on the other computer. Canopy opens it there."),
            )
            .children(self.notice("open-project"));
        let Some(catalog) = host.catalog() else {
            let body = if host.phase() == Phase::Connecting {
                column().child(loading("Loading projects…"))
            } else {
                empty(
                    "Nothing loaded yet",
                    "Connect to see projects, chats and terminals on this computer.",
                )
                .child(
                    button("catalog-connect", "Connect")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                )
            };
            return column()
                .gap(px(24.))
                .child(body)
                .child(open)
                .into_any_element();
        };
        let mut projects = column().gap(px(4.)).child(section_title("PROJECTS"));
        if catalog.projects.is_empty() {
            projects = projects.child(
                div()
                    .py(px(4.))
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("No open projects are shared with this device."),
            );
        }
        for project in catalog.projects {
            let closing = project.checkouts.iter().any(|w| w.closing);
            let close_target = project.checkouts.first().cloned();
            projects = projects.child(
                row()
                    .h(px(32.))
                    .gap(px(8.))
                    .px(px(8.))
                    .child(icon(IconName::Folder))
                    .child(row_text(project.name.clone(), project.path.clone()))
                    .children(close_target.map(|workspace| {
                        button(
                            SharedString::from(format!("close-project:{}", project.path)),
                            if closing {
                                "Retry cleanup…"
                            } else {
                                "Close project…"
                            },
                        )
                        .ghost()
                        .border_0()
                        .text_color(t::muted())
                        .loading(self.pending == Some("close-project"))
                        .disabled(!ready)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project(workspace.clone(), cx)
                        }))
                    })),
            );
            for workspace in project.checkouts {
                let Some(_) = workspace.project.as_ref() else {
                    continue;
                };
                let reference = workspace.clone();
                let closing = workspace.closing;
                let selected = self
                    .workspace
                    .as_ref()
                    .is_some_and(|w| w.project == reference.project);
                let detail = workspace
                    .branch
                    .clone()
                    .filter(|branch| branch != &workspace.name)
                    .unwrap_or_default();
                projects = projects.child(
                    list_row(
                        SharedString::from(format!("workspace:{}", reference.catalog_id)),
                        format!("Open {}", workspace.name),
                        selected,
                    )
                    .pl(px(30.))
                    .when(closing, |row| row.opacity(0.6).cursor_default())
                    .child(custom_icon("git-branch").flex_shrink_0())
                    .child(row_text(workspace.name.clone(), detail))
                    .children(closing.then(|| badge("Closing")))
                    .when(!closing, |row| {
                        row.on_click(cx.listener(move |this, _, _, cx| {
                            this.select_workspace(reference.clone(), cx)
                        }))
                    }),
                );
            }
        }
        projects = projects.children(self.notice("close-project"));
        let mut chats = column().gap(px(4.)).child(section_title("CHATS"));
        if catalog.threads.is_empty() {
            chats = chats.child(
                div()
                    .py(px(4.))
                    .text_size(px(12.))
                    .text_color(t::muted())
                    .child("No chats yet. Start one below."),
            );
        }
        for thread in catalog.threads {
            if let Some(summary) = thread.summary {
                let chat = thread.catalog_id;
                chats = chats.child(
                    list_row(
                        SharedString::from(format!("chat:{chat}")),
                        format!("Open chat {}", summary.title),
                        false,
                    )
                    .child(icon(IconName::Bot).flex_shrink_0())
                    .child(row_text(summary.title, summary.agent_id))
                    .child(
                        icon(IconName::ArrowRight)
                            .size(px(12.))
                            .text_color(t::faint()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_chat(chat.clone(), window, cx)
                    })),
                );
            }
        }
        let mut terminals = column().gap(px(4.)).child(section_title("TERMINALS"));
        for summary in catalog.terminals {
            if let Some(terminal) = summary.terminal {
                terminals =
                    terminals.child(
                        list_row(
                            SharedString::from(format!("terminal:{}", terminal.terminal_id)),
                            format!("Open terminal {}", summary.title),
                            self.terminal.as_ref() == Some(&terminal),
                        )
                        .child(custom_icon("terminal").flex_shrink_0())
                        .child(row_text(
                            summary.title,
                            if summary.running { "Running" } else { "Exited" },
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_terminal(terminal.clone(), cx)
                        })),
                    );
            }
        }
        terminals = terminals
            .child(
                row()
                    .pt(px(8.))
                    .gap(px(8.))
                    .child(
                        input(&self.fields["tool"])
                            .w(px(160.))
                            .disabled(!ready && self.pending != Some("new-terminal")),
                    )
                    .child(
                        button("new-terminal", "New terminal")
                            .icon(icon(IconName::Plus))
                            .loading(self.pending == Some("new-terminal"))
                            .disabled(!ready || self.workspace.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let tool = this.value("tool", cx);
                                let profile = nonempty(this.value("profile", cx));
                                this.new_terminal(tool, profile, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child(if self.workspace.is_some() {
                        "Tool ID from the other computer, for example shell. Opens in the selected checkout."
                    } else {
                        "Select a checkout above to start a terminal in it."
                    }),
            )
            .children(self.notice("new-terminal"));
        column()
            .gap(px(24.))
            .child(projects)
            .child(chats)
            .child(terminals)
            .child(
                column()
                    .gap(px(8.))
                    .child(section_title("NEW CHAT"))
                    .child(self.render_agent_discovery(cx)),
            )
            .child(open)
            .into_any_element()
    }
}
