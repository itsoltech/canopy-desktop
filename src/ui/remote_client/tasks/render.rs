//! Remote task browser and task details, composed from the local task components.
use super::*;
use crate::ui::components::{
    integrations::{integration_message, provider_heading, provider_icon},
    task_detail as view,
    tasks::{TASK_ROW_HEIGHT, task_empty_state, task_row, task_skeleton},
};
use gpui_kit::component::{IconName, scroll::ScrollableElement};

/// Width at which task details gain the separate metadata column.
const WIDE: f32 = 1000.;
/// Below this width the browser and the details share one column.
const NARROW: f32 = 720.;
const BROWSER_WIDTH: f32 = 320.;

impl Render for TasksView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        // Layout follows this view's own width (embedded panes are narrower than the window).
        let measure = canvas(
            move |bounds, _, cx| {
                let width = f32::from(bounds.size.width);
                let _ = owner.update(cx, |s, cx| {
                    if (s.width - width).abs() > 0.5 {
                        s.width = width;
                        cx.notify();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let open = self.task.is_some() || self.mode == FormMode::Create;
        let narrow = self.width > 0. && self.width < NARROW;
        let wide = self.width == 0. || self.width >= WIDE;
        row()
            .relative()
            .size_full()
            .min_h(px(520.))
            .items_stretch()
            .text_color(t::text())
            .child(measure)
            .when(!narrow || !open, |r| {
                r.child(self.render_browser(narrow, cx))
            })
            .when(!narrow || open, |r| {
                r.child(self.render_detail_area(wide, narrow, cx))
            })
    }
}

impl TasksView {
    fn project_label(&self) -> String {
        self.project
            .as_ref()
            .map(|p| p.label.clone())
            .unwrap_or_default()
    }
    pub(super) fn local_provider(&self) -> canopy_desktop::integrations::Provider {
        adapter::provider(self.provider())
    }
    fn render_browser(&self, narrow: bool, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let open = self.task.is_some() || self.mode == FormMode::Create;
        let heading = match &self.integration {
            Some(integration) => provider_heading(
                self.local_provider(),
                self.project
                    .as_ref()
                    .map(|p| p.label.clone())
                    .unwrap_or_else(|| "Tasks".into()),
                integration.name.clone(),
            )
            .flex_1(),
            None => column()
                .flex_1()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Tasks"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .line_height(px(16.))
                        .text_color(t::muted())
                        .child("Browse the tasks of this computer’s accounts."),
                ),
        };
        let header = row()
            .gap(px(8.))
            .py(px(12.))
            .flex_shrink_0()
            .child(heading)
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .child(
                        icon_button("create-task", IconName::Plus, "New task")
                            .disabled(disabled || self.project.is_none())
                            .on_click(cx.listener(|s, _, w, cx| s.new_task(w, cx))),
                    )
                    .child(
                        loading_icon_button(
                            "refresh-tasks",
                            IconName::RotateCw,
                            "Refresh tasks",
                            Some(&self.refresh_loading),
                        )
                        .disabled(
                            (disabled || self.project.is_none()) && !self.refresh_loading.active(),
                        )
                        .on_click(cx.listener(|s, _, _, cx| s.search(false, cx))),
                    ),
            );
        let unconfigured = self.integrations.iter().filter(|i| !i.configured).count();
        let accounts = (!self.integrations.is_empty()).then(|| {
            column()
                .gap(px(4.))
                .flex_shrink_0()
                .child(
                    row()
                        .gap(px(6.))
                        .child(
                            dropdown(&self.integration_picker)
                                .placeholder("Choose account")
                                .flex_1()
                                .min_w_0()
                                .disabled(disabled),
                        )
                        .child(
                            icon_button("refresh-accounts", IconName::RotateCw, "Refresh accounts")
                                .disabled(disabled)
                                .on_click(cx.listener(|s, _, _, cx| s.load_integrations(cx))),
                        ),
                )
                .children((unconfigured > 0).then(|| {
                    div()
                        .text_size(px(10.))
                        .text_color(t::muted())
                        .child(if unconfigured == 1 {
                            "1 account needs setup in Host settings.".to_owned()
                        } else {
                            format!("{unconfigured} accounts need setup in Host settings.")
                        })
                }))
        });
        let projects = self.integration.is_some().then(|| {
            column()
                .gap(px(6.))
                .flex_shrink_0()
                .child(
                    row()
                        .gap(px(6.))
                        .child(
                            dropdown(&self.project_picker)
                                .placeholder(if self.loading == Loading::Projects {
                                    "Loading projects…"
                                } else {
                                    "Choose project"
                                })
                                .flex_1()
                                .min_w_0()
                                .disabled(disabled),
                        )
                        .child(
                            loading_icon_button(
                                "refresh-projects",
                                IconName::RotateCw,
                                "Refresh projects",
                                Some(&self.projects_loading),
                            )
                            .disabled(disabled && !self.projects_loading.active())
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.load_choices(Choices::Projects, false, cx)
                            })),
                        ),
                )
                .children(self.projects_next.as_ref().map(|_| {
                    button("more-projects", "Load more projects")
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|s, _, _, cx| s.load_choices(Choices::Projects, true, cx)),
                        )
                }))
        });
        let tracker = self.provider() != "github";
        let filters = (self.project.is_some() && tracker).then(|| self.render_filters(cx));
        let search = self.project.is_some().then(|| {
            column()
                .gap(px(8.))
                .flex_shrink_0()
                .child(
                    row()
                        .gap(px(6.))
                        .child(input(&self.query).flex_1().disabled(disabled))
                        .child(
                            loading_button("task-search", "Search", &self.refresh_loading)
                                .disabled(disabled && !self.refresh_loading.active())
                                .on_click(cx.listener(|s, _, _, cx| s.search(false, cx))),
                        ),
                )
                .child(
                    row()
                        .gap(px(8.))
                        .child(
                            row()
                                .gap(px(2.))
                                .p(px(2.))
                                .rounded(px(6.))
                                .bg(t::hover())
                                .children(["Open", "Closed"].into_iter().enumerate().map(
                                    |(ix, label)| {
                                        selection_button(
                                            SharedString::from(format!("task-state-{ix}")),
                                            label,
                                            self.state == ix,
                                        )
                                        .border_0()
                                        .disabled(disabled)
                                        .on_click(
                                            cx.listener(move |s, _, _, cx| {
                                                if s.state != ix {
                                                    s.state = ix;
                                                    s.search(false, cx);
                                                }
                                            }),
                                        )
                                    },
                                )),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(t::muted())
                                .child(format!("{} loaded", self.tasks.len())),
                        ),
                )
        });
        let list = if self.integrations.is_empty() {
            if self.busy {
                task_skeleton().flex_1().into_any_element()
            } else {
                centered(task_empty_state(
                    "Connect a task provider",
                    "Set up GitHub, Jira or YouTrack in Host settings to browse tasks here.",
                ))
            }
        } else if self.integration.is_none() {
            centered(task_empty_state(
                "Choose an account",
                "Pick one of this computer’s task accounts to browse its projects.",
            ))
        } else if self.project.is_none() {
            if self.loading == Loading::Projects && self.projects.is_empty() {
                task_skeleton().flex_1().into_any_element()
            } else {
                centered(task_empty_state(
                    "Choose a project",
                    "Pick a project to browse and search its tasks.",
                ))
            }
        } else if self.tasks.is_empty() {
            if self.loading == Loading::Tasks || self.loading == Loading::Projects {
                task_skeleton().flex_1().into_any_element()
            } else {
                centered(task_empty_state(
                    "No matching tasks",
                    "Change the search or filter, then search again.",
                ))
            }
        } else {
            self.render_task_list(cx).into_any_element()
        };
        column()
            .h_full()
            .min_h_0()
            .gap(px(10.))
            .px(px(12.))
            .pb(px(12.))
            .when(narrow, |c| c.flex_1().min_w_0())
            .when(!narrow, |c| {
                c.w(px(BROWSER_WIDTH))
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(t::border())
            })
            .child(header)
            .children(
                (!open)
                    .then(|| self.error.clone().map(|e| integration_message(e, true)))
                    .flatten(),
            )
            .children(accounts)
            .children(projects)
            .children(filters)
            .children(search)
            .child(list)
    }
    fn render_filters(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        column()
            .gap(px(8.))
            .flex_shrink_0()
            .child(
                row()
                    .gap(px(6.))
                    .child(
                        dropdown(&self.filter_picker)
                            .placeholder("No saved filter")
                            .flex_1()
                            .min_w_0()
                            .disabled(disabled),
                    )
                    .child(
                        icon_button(
                            "manage-filters",
                            if self.manage_filters {
                                IconName::ChevronUp
                            } else {
                                IconName::Settings
                            },
                            if self.manage_filters {
                                "Hide filter settings"
                            } else {
                                "Edit filter"
                            },
                        )
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.manage_filters = !s.manage_filters;
                            cx.notify();
                        })),
                    ),
            )
            .children(self.manage_filters.then(|| {
                column()
                    .gap(px(10.))
                    .p(px(10.))
                    .rounded(px(6.))
                    .bg(t::hover())
                    .child(form_field(
                        "Condition",
                        "Applies to the next search. Leave empty to show every task.",
                        input(&self.filter_expression).disabled(disabled),
                    ))
                    .child(form_field(
                        "Name",
                        "",
                        input(&self.filter_name).disabled(disabled),
                    ))
                    .child(
                        row()
                            .gap(px(6.))
                            .justify_end()
                            .child(
                                button("remove-filter", "Delete filter")
                                    .text_color(t::red())
                                    .disabled(
                                        disabled || self.filter.as_ref().is_none_or(|f| f.builtin),
                                    )
                                    .on_click(cx.listener(|s, _, _, cx| s.remove_filter(cx))),
                            )
                            .child(
                                button("save-filter", "Save filter")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|s, _, _, cx| s.save_filter(cx))),
                            ),
                    )
            }))
    }
    fn render_task_list(&self, cx: &mut Context<Self>) -> Div {
        let more = usize::from(self.next.is_some());
        let list = uniform_list(
            "task-results",
            self.tasks.len() + more,
            cx.processor(|s, range: std::ops::Range<usize>, _, cx| {
                let provider = s.local_provider();
                let project = s.project_label();
                range
                    .map(|ix| {
                        if ix >= s.tasks.len() {
                            return row()
                                .h(px(TASK_ROW_HEIGHT))
                                .justify_center()
                                .child(
                                    loading_button(
                                        "more-tasks",
                                        "Load more tasks",
                                        &s.more_loading,
                                    )
                                    .disabled(!s.ready() && !s.more_loading.active())
                                    .on_click(cx.listener(|s, _, _, cx| s.search(true, cx))),
                                )
                                .into_any_element();
                        }
                        let task = &s.tasks[ix];
                        let item = adapter::item(task, provider, &project, &s.names);
                        let reference = task.task.clone();
                        let selected = s.task.as_ref().and_then(|t| t.task.as_ref())
                            == reference.as_ref()
                            && reference.is_some();
                        task_row(&item, selected)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                if let Some(reference) = reference.clone()
                                    && s.ready()
                                {
                                    s.open_task(reference, cx);
                                }
                            }))
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .size_full()
        .track_scroll(&self.task_list);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(list)
            .vertical_scrollbar(&self.task_list)
    }
    fn render_detail_area(&self, wide: bool, narrow: bool, cx: &mut Context<Self>) -> Div {
        let content = if self.mode == FormMode::Create {
            self.render_create(narrow, cx).into_any_element()
        } else if let Some(task) = &self.task {
            self.render_task(task, wide, narrow, cx).into_any_element()
        } else if self.loading == Loading::Task {
            detail_skeleton().into_any_element()
        } else {
            centered(task_empty_state(
                "No task selected",
                "Choose a task from the list to read and update it here.",
            ))
        };
        column()
            .flex_1()
            .min_w_0()
            .h_full()
            .min_h_0()
            .gap(px(12.))
            .px(px(16.))
            .pb(px(12.))
            .children(self.render_upload_banner(cx))
            .child(content)
    }
    fn render_task(&self, task: &p::Task, wide: bool, narrow: bool, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let provider = self.local_provider();
        let reference = adapter::reference(task, provider, &self.project_label());
        let url = task.url.clone();
        let actions = row()
            .gap(px(8.))
            .child(
                icon_button("edit-task", IconName::Replace, "Edit title and description")
                    .disabled(disabled || self.editing || self.mode != FormMode::Detail)
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.editing = true;
                        s.tab = DetailTab::Description;
                        cx.notify();
                    })),
            )
            .child(
                loading_icon_button("refresh-task", IconName::RotateCw, "Refresh task", None)
                    .disabled(disabled)
                    .on_click(cx.listener(|s, _, _, cx| {
                        if let Ok(reference) = s.task_ref() {
                            s.open_task(reference, cx);
                        }
                    })),
            )
            .children((!url.is_empty()).then(|| {
                icon_button(
                    "open-task-url",
                    IconName::ExternalLink,
                    "Open task in browser",
                )
                .on_click(move |_, _, cx| cx.open_url(&url))
            }))
            .child(
                icon_button(
                    "close-task",
                    if narrow {
                        IconName::ArrowLeft
                    } else {
                        IconName::Close
                    },
                    "Back to task list",
                )
                .on_click(cx.listener(|s, _, _, cx| s.close_task(cx))),
            );
        let comments = self.comments_loaded.then_some(self.comments.len());
        let tabs = view::tab_bar(DetailTab::ALL.map(|tab| {
            view::tab_button(tab, self.tab, comments)
                .on_click(cx.listener(move |s, _, _, cx| s.select_tab(tab, cx)))
        }));
        let page = match self.tab {
            DetailTab::Description => {
                let main = if matches!(self.mode, FormMode::Fields | FormMode::Transition) {
                    self.render_fields_form(cx).into_any_element()
                } else if self.editing {
                    self.render_editor(cx).into_any_element()
                } else if task.description.trim().is_empty() {
                    div()
                        .text_size(px(12.))
                        .text_color(t::muted())
                        .child("No description")
                        .into_any_element()
                } else {
                    self.description.clone().into_any_element()
                };
                view::description(
                    (!wide).then(|| {
                        self.render_metadata(task, cx)
                            .pb(px(16.))
                            .border_b_1()
                            .border_color(t::border())
                            .into_any_element()
                    }),
                    main,
                )
                .into_any_element()
            }
            DetailTab::Comments => self.render_comments(cx).into_any_element(),
        };
        let body = row()
            .items_stretch()
            .flex_1()
            .min_h_0()
            .gap(px(24.))
            .child(
                column().flex_1().min_w_0().gap(px(12.)).child(tabs).child(
                    div()
                        .id(SharedString::from(format!(
                            "remote-task-page-{}-{}",
                            reference.id,
                            self.tab.position()
                        )))
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .child(page),
                ),
            )
            .children(wide.then(|| {
                div()
                    .id("remote-task-metadata")
                    .w(px(240.))
                    .flex_shrink_0()
                    .pl(px(20.))
                    .border_l_1()
                    .border_color(t::border())
                    .overflow_y_scrollbar()
                    .child(self.render_metadata(task, cx).pb(px(16.)))
            }));
        column()
            .flex_1()
            .min_h_0()
            .gap(px(16.))
            .pt(px(12.))
            .child(view::header(&reference, false, actions))
            .child(view::title(task.title.clone()))
            .children(self.error.clone().map(|e| integration_message(e, true)))
            .child(body)
            .child(self.render_footer(task, cx))
    }
    fn render_footer(&self, task: &p::Task, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let scoped = task
            .task
            .as_ref()
            .and_then(|t| t.project.as_ref())
            .and_then(|p| p.resource.as_ref())
            .is_some_and(
                |r| matches!(r, p::task_project::Resource::Project(p) if p.worktree_id.is_some()),
            );
        let busy_form = self.editing
            || self.mode != FormMode::Detail
            || self.action.is_some()
            || self.tab == DetailTab::Comments;
        let create = if busy_form {
            button("worktree", "New worktree")
        } else {
            primary_button("worktree", "New worktree")
        };
        row()
            .flex_shrink_0()
            .gap(px(8.))
            .pt(px(16.))
            .border_t_1()
            .border_color(t::border())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.))
                    .text_color(t::muted())
                    .child(self.notice.clone()),
            )
            .children(
                self.returned_workspace
                    .as_ref()
                    .and_then(|id| self.host.shared_catalog().and_then(|c| checkout(&c, id)))
                    .map(|workspace| {
                        button("open-created-worktree", "Open worktree")
                            .disabled(disabled)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(OpenWorkspace(workspace.clone()))
                            }))
                    }),
            )
            .child(
                input(&self.branch)
                    .w(px(180.))
                    .disabled(disabled || !scoped),
            )
            .child(
                create
                    .when(!scoped, |b| {
                        b.tooltip("Open this task from a project workspace to create a worktree.")
                    })
                    .disabled(disabled || !scoped)
                    .on_click(cx.listener(|s, _, _, cx| s.task_worktree(cx))),
            )
    }
    fn render_editor(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        column()
            .gap(px(12.))
            .pb(px(16.))
            .child(form_field(
                "Title",
                "",
                input(&self.title).disabled(disabled),
            ))
            .child(form_field("Description", "Markdown", self.body.clone()))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        button("save-draft", "Save draft")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| s.save_draft(cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        button("cancel-edit", "Cancel")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, w, cx| s.cancel_edit(w, cx))),
                    )
                    .child(
                        primary_loading_button("save-task", "Save", &self.save_loading)
                            .disabled(disabled && !self.save_loading.active())
                            .on_click(cx.listener(|s, _, _, cx| s.save_task(cx))),
                    ),
            )
    }
    fn render_fields_form(&self, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let transition = self.mode == FormMode::Transition;
        let mut form = column().gap(px(12.)).pb(px(16.)).child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .child(if transition {
                    "Change status"
                } else {
                    "Edit fields"
                }),
        );
        if transition {
            form = form.child(
                column()
                    .gap(px(4.))
                    .child(caption("TRANSITION"))
                    .children(self.schema.transitions.iter().map(|t| {
                        let choice = t.clone();
                        selection_button(
                            SharedString::from(format!("transition:{}", t.id)),
                            format!("{} → {}", t.name, t.destination),
                            self.transition
                                .as_ref()
                                .is_some_and(|v| v.choice_id == t.id),
                        )
                        .w_full()
                        .disabled(disabled)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.choose_transition(choice.clone(), w, cx)
                        }))
                    }))
                    .children(self.schema.transitions.is_empty().then(|| {
                        div()
                            .text_size(px(12.))
                            .text_color(t::muted())
                            .child("No transitions are available for this task.")
                    })),
            );
        }
        form = form.children(self.fields.iter().map(|f| f.render()));
        if transition {
            form = form.child(form_field("Comment", "Optional", self.comment.clone()));
        }
        form.child(
            row()
                .gap(px(8.))
                .justify_end()
                .child(
                    button("cancel-fields", "Cancel")
                        .disabled(disabled)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.mode = FormMode::Detail;
                            s.fields.clear();
                            s.field_events.clear();
                            s.transition = None;
                            cx.notify();
                        })),
                )
                .child(
                    primary_loading_button(
                        "save-fields",
                        if transition {
                            "Change status"
                        } else {
                            "Save fields"
                        },
                        &self.save_loading,
                    )
                    .disabled(
                        (disabled || (transition && self.transition.is_none()))
                            && !self.save_loading.active(),
                    )
                    .on_click(cx.listener(|s, _, _, cx| s.save_fields(cx))),
                ),
        )
    }
    fn render_create(&self, narrow: bool, cx: &mut Context<Self>) -> Div {
        let disabled = !self.ready();
        let jira = self.provider() == "jira";
        let mut form = column().gap(px(12.)).pb(px(16.));
        if jira {
            form = form.child(
                column()
                    .gap(px(6.))
                    .child(caption("ISSUE TYPE"))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .text_color(if self.issue_type.is_some() {
                                        t::text()
                                    } else {
                                        t::muted()
                                    })
                                    .child(
                                        self.issue_type
                                            .as_ref()
                                            .map(|t| t.label.clone())
                                            .unwrap_or_else(|| "Not chosen".into()),
                                    ),
                            )
                            .child(
                                button("issue-types", "Choose type…")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.load_choices(Choices::Types, false, cx)
                                    })),
                            ),
                    )
                    .children(
                        (self.choice_kind == Some(Choices::Types))
                            .then(|| self.render_picker("Issue type", cx)),
                    ),
            );
        }
        form = form
            .child(form_field(
                "Title",
                "",
                input(&self.title).disabled(disabled),
            ))
            .child(form_field("Description", "Markdown", self.body.clone()))
            .children(self.fields.iter().map(|f| f.render()));
        if self.provider() == "github" {
            form = form.child(self.render_option_field("label", "Labels", cx));
            form = form.child(self.render_option_field("assignee", "Assignees", cx));
            form = form.child(self.render_option_field("milestone", "Milestone", cx));
        }
        let header = row()
            .flex_shrink_0()
            .gap(px(8.))
            .child(provider_icon(self.local_provider()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(t::secondary())
                    .child(format!("{}  /  New task", self.project_label())),
            )
            .child(
                icon_button(
                    "cancel-create",
                    if narrow {
                        IconName::ArrowLeft
                    } else {
                        IconName::Close
                    },
                    "Discard new task",
                )
                .on_click(cx.listener(|s, _, _, cx| {
                    s.mode = FormMode::Detail;
                    s.fields.clear();
                    s.field_events.clear();
                    s.choice_kind = None;
                    cx.notify();
                })),
            );
        column()
            .flex_1()
            .min_h_0()
            .gap(px(16.))
            .pt(px(12.))
            .child(header)
            .child(view::title("New task"))
            .children(self.error.clone().map(|e| integration_message(e, true)))
            .child(
                div()
                    .id("remote-new-task")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(form),
            )
            .child(
                row()
                    .flex_shrink_0()
                    .gap(px(8.))
                    .pt(px(16.))
                    .border_t_1()
                    .border_color(t::border())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(t::muted())
                            .child(self.notice.clone()),
                    )
                    .child(
                        button("save-draft", "Save draft")
                            .disabled(disabled)
                            .on_click(cx.listener(|s, _, _, cx| s.save_draft(cx))),
                    )
                    .child(
                        primary_loading_button("save-task", "Create task", &self.save_loading)
                            .disabled(
                                (disabled || (jira && self.issue_type.is_none()))
                                    && !self.save_loading.active(),
                            )
                            .on_click(cx.listener(|s, _, _, cx| s.save_task(cx))),
                    ),
            )
    }
    fn render_upload_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (_, task, name) = self.pending_upload.as_ref()?;
        // Shown inline in the attachments of its own task; elsewhere as a banner.
        if self.task.as_ref().and_then(|t| t.task.as_ref()) == Some(task) {
            return None;
        }
        let target = task.clone();
        Some(
            row()
                .flex_shrink_0()
                .mt(px(12.))
                .gap(px(8.))
                .child(
                    integration_message(
                        format!("“{name}” is ready to upload to {}.", task.task_id),
                        false,
                    )
                    .flex_1(),
                )
                .child(
                    button("open-upload-task", "Open task")
                        .disabled(!self.ready())
                        .on_click(cx.listener(move |s, _, _, cx| s.open_task(target.clone(), cx))),
                ),
        )
    }
    fn close_task(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.editing = false;
        self.action = None;
        self.choice_kind = None;
        if matches!(self.mode, FormMode::Fields | FormMode::Transition) {
            self.mode = FormMode::Detail;
            self.fields.clear();
            self.field_events.clear();
        }
        self.sync_description(cx);
        cx.notify();
    }
    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.task.clone() else {
            return;
        };
        self.title
            .update(cx, |v, cx| v.set_value(task.title.clone(), window, cx));
        self.body
            .update(cx, |v, cx| v.set_value(&task.description, window, cx));
        self.editing = false;
        cx.notify();
    }
}

fn centered(content: Div) -> AnyElement {
    column()
        .flex_1()
        .justify_center()
        .child(content)
        .into_any_element()
}

fn detail_skeleton() -> Div {
    column()
        .flex_1()
        .gap(px(16.))
        .pt(px(16.))
        .child(skeleton_bar().w(px(160.)).h(px(12.)))
        .child(skeleton_bar().w(px(320.)).h(px(20.)))
        .child(skeleton_bar().w_full().h(px(10.)))
        .child(skeleton_bar().w_full().h(px(10.)))
        .child(skeleton_bar().w(px(240.)).h(px(10.)))
}
