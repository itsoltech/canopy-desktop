use super::{components::*, theme as t};
use crate::app_state::{AppState, ChangesState};
use canopy_desktop::git::{
    changes::{Edit, FileChange},
    network::Operation,
};
use canopy_desktop::motion::{self, Transition, presets};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName,
        input::{InputEvent, InputState, TextareaState},
    },
    prelude::FluentBuilder,
    *,
};
use std::time::Instant;
use std::{collections::HashMap, path::PathBuf};
#[derive(Clone)]
pub struct DiscardRequest {
    pub path: PathBuf,
    pub file: FileChange,
}
enum ChangeRow {
    Header(bool, usize),
    File(usize),
}
pub struct ChangesPanel {
    commit_loading: ButtonLoading,
    pull_loading: ButtonLoading,
    push_loading: ButtonLoading,
    history: Entity<super::history::HistoryView>,
    show_history: bool,
    content: Transition,
    rows: Vec<ChangeRow>,
    state: Entity<ChangesState>,
    filter: Entity<InputState>,
    message: Entity<TextareaState>,
    path: Option<PathBuf>,
    drafts: HashMap<PathBuf, String>,
    hovered: Option<String>,
    kind: Option<String>,
    error: Option<String>,
    seen_commit: Option<(PathBuf, String)>,
    seen_network: u64,
    show_hook_output: bool,
    _events: Vec<Subscription>,
}
impl EventEmitter<DiscardRequest> for ChangesPanel {}
impl ChangesPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let history = cx.new(|_| super::history::HistoryView::new());
        let state = cx.global::<AppState>().changes.clone();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files…"));
        let message = cx.new(|cx| TextareaState::new(window, cx).placeholder("Commit message"));
        let pulls = cx.global::<AppState>().pull_requests.clone();
        let events = vec![
            cx.observe(&pulls, |_, _, cx| cx.notify()),
            cx.observe(&history, |_, _, cx| cx.notify()),
            cx.observe_in(&state, window, |this, state, window, cx| {
                let next = state.read(cx).path.clone();
                let current = state.read(cx);
                this.commit_loading.set(current.committing, cx);
                let operation = current
                    .network_operation
                    .as_ref()
                    .filter(|(path, _)| Some(path) == current.path.as_ref())
                    .map(|(_, op)| *op);
                this.pull_loading.set(
                    operation == Some(canopy_desktop::git::network::Operation::Pull),
                    cx,
                );
                this.push_loading.set(
                    operation == Some(canopy_desktop::git::network::Operation::Push),
                    cx,
                );
                if this.path != next {
                    if let Some(old) = this.path.take() {
                        this.drafts
                            .insert(old, this.message.read(cx).value().to_string());
                    }
                    this.path = next;
                    let draft = this
                        .path
                        .as_ref()
                        .and_then(|p| this.drafts.get(p))
                        .cloned()
                        .unwrap_or_default();
                    this.message
                        .update(cx, |m, cx| m.set_value(draft, window, cx));
                    this.error = None;
                }
                let commit = state.read(cx).last_commit.clone();
                if commit != this.seen_commit
                    && let Some((path, _)) = &commit
                {
                    this.drafts.remove(path);
                    if this.path.as_ref() == Some(path) {
                        this.message.update(cx, |m, cx| m.set_value("", window, cx));
                    }
                }
                if this.show_history && !this.inactive(cx) {
                    let path = this.path.clone();
                    this.history.update(cx, |view, cx| view.set_path(path, cx));
                    if commit != this.seen_commit
                        || state.read(cx).network_revision != this.seen_network
                    {
                        this.history.update(cx, |view, cx| view.refresh(cx));
                    }
                }
                this.seen_commit = commit;
                this.seen_network = state.read(cx).network_revision;
                this.rebuild(cx);
                cx.notify();
            }),
            cx.subscribe(&filter, |this, _, _: &InputEvent, cx| {
                this.rebuild(cx);
                cx.notify();
            }),
            cx.subscribe(&message, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        let mut panel = Self {
            commit_loading: ButtonLoading::default(),
            pull_loading: ButtonLoading::default(),
            push_loading: ButtonLoading::default(),
            history,
            show_history: false,
            content: Transition::new(1., Instant::now()),
            path: state.read(cx).path.clone(),
            state,
            filter,
            message,
            drafts: HashMap::new(),
            hovered: None,
            kind: None,
            error: None,
            seen_commit: None,
            seen_network: 0,
            show_hook_output: false,
            _events: events,
            rows: vec![],
        };
        panel.rebuild(cx);
        panel
    }
    fn inactive(&self, cx: &App) -> bool {
        let layout = cx.global::<AppState>().layout.read(cx);
        !layout.inspector_changes || !layout.inspector_open
    }
    fn rebuild(&mut self, cx: &App) {
        self.rows.clear();
        let query = self.filter.read(cx).value().to_lowercase();
        if let Some(data) = &self.state.read(cx).data {
            for staged in [true, false] {
                let count = data.files.iter().filter(|f| f.staged == staged).count();
                if count == 0 {
                    continue;
                }
                self.rows.push(ChangeRow::Header(staged, count));
                self.rows.extend(
                    data.files
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| {
                            f.staged == staged
                                && f.path.to_string_lossy().to_lowercase().contains(&query)
                                && self.kind.as_ref().is_none_or(|k| k == &f.kind)
                        })
                        .map(|(i, _)| ChangeRow::File(i)),
                );
            }
        }
    }
    fn list_row(&self, index: usize, cx: &Context<Self>) -> AnyElement {
        let disabled = self.inactive(cx)
            || self.state.read(cx).busy
            || cx.global::<AppState>().git.read(cx).busy;
        match self.rows.get(index) {
            Some(ChangeRow::Header(staged, count)) => {
                let staged = *staged;
                let origin = self.state.read(cx).path.clone();
                git::change_group(staged, *count)
                    .child(
                        git::group_action(
                            if staged { "unstage-all" } else { "stage-all" },
                            if staged { "Unstage all" } else { "Stage all" },
                        )
                        .disabled(disabled || *count == 0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit(
                                origin.clone(),
                                if staged {
                                    Edit::Unstage(None)
                                } else {
                                    Edit::Stage(None)
                                },
                                cx,
                            )
                        })),
                    )
                    .into_any_element()
            }
            Some(ChangeRow::File(i)) => {
                let data = self.state.read(cx).data.clone();
                data.as_ref()
                    .and_then(|d| Some((d.files.get(*i)?, d.lines.get(*i).copied().flatten())))
                    .map(|(f, lines)| self.file_row(f, lines, disabled, cx).into_any_element())
                    .unwrap_or_else(|| div().into_any_element())
            }
            None => div().into_any_element(),
        }
    }
    fn edit(&mut self, origin: Option<PathBuf>, edit: Edit, cx: &mut Context<Self>) {
        if origin != self.state.read(cx).path {
            return;
        }
        if let Some(path) = origin {
            self.error = self
                .state
                .update(cx, |state, cx| state.edit(path, edit, cx))
                .err()
                .map(|e| e.to_string());
            cx.notify();
        }
    }
    fn file_row(
        &self,
        file: &FileChange,
        lines: Option<(usize, usize)>,
        disabled: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let key = format!("{}:{}", file.staged, file.path.display());
        let hover = key.clone();
        let hovered = self.hovered.as_ref() == Some(&key);
        let diff = file.clone();
        let edit = file.clone();
        let discard = file.clone();
        let path = self.state.read(cx).path.clone();
        let edit_path = path.clone();
        let diff_path = path.clone();
        let tooltip = file
            .old_path
            .as_ref()
            .map(|old| format!("{} → {}", old.display(), file.path.display()))
            .unwrap_or_else(|| file.path.display().to_string());
        git::change_file_row(SharedString::from(key.clone()), &file.kind, hovered)
            .on_hover(cx.listener(move |this, over, _, cx| {
                if *over {
                    this.hovered = Some(hover.clone());
                } else if this.hovered.as_ref() == Some(&hover) {
                    this.hovered = None;
                }
                cx.notify();
            }))
            .child(
                git::change_file_label(
                    SharedString::from(format!("diff-{key}")),
                    &file.path,
                    hovered,
                )
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .when(!self.inactive(cx), |label| {
                    label.on_click(cx.listener(move |this, _, _, cx| {
                        if this.state.read(cx).path == diff_path {
                            this.state.update(cx, |s, cx| s.open_diff(diff.clone(), cx));
                        }
                    }))
                }),
            )
            .children(
                lines
                    .filter(|_| !hovered)
                    .map(|(added, deleted)| line_counts(added, deleted)),
            )
            .children((hovered && !file.staged).then(|| {
                git::change_action(
                    SharedString::from(format!("discard-{key}")),
                    IconName::Undo2,
                    "Discard working tree changes",
                )
                .disabled(disabled)
                .on_click(cx.listener(move |_, _, _, cx| {
                    if let Some(path) = &path {
                        cx.emit(DiscardRequest {
                            path: path.clone(),
                            file: discard.clone(),
                        });
                    }
                }))
            }))
            .children(hovered.then(|| {
                git::change_action(
                    SharedString::from(format!("stage-{key}")),
                    if file.staged {
                        IconName::Minus
                    } else {
                        IconName::Plus
                    },
                    if file.staged {
                        "Unstage file"
                    } else {
                        "Stage file"
                    },
                )
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit(
                        edit_path.clone(),
                        if edit.staged {
                            Edit::Unstage(Some(edit.clone()))
                        } else {
                            Edit::Stage(Some(edit.clone()))
                        },
                        cx,
                    )
                }))
            }))
    }
    fn select_view(&mut self, history: bool, cx: &mut Context<Self>) {
        if self.show_history == history {
            return;
        }
        self.show_history = history;
        let now = Instant::now();
        self.content = Transition::new(0., now);
        self.content
            .retarget(1., presets::STATE_CHANGE, now, motion::policy(cx));
        if history {
            let path = self.state.read(cx).path.clone();
            self.history.update(cx, |view, cx| view.open(path, cx));
        }
        cx.notify();
    }
    /// Branch, the Changes/History switch and the refresh of the shown view.
    fn header(&self, disabled: bool, cx: &mut Context<Self>) -> Div {
        let branch = cx
            .global::<AppState>()
            .pull_requests
            .read(cx)
            .current
            .as_ref()
            .map(|branch| branch.name.clone());
        let this = cx.entity().downgrade();
        let refresh = if self.show_history {
            let history = self.history.read(cx);
            loading_icon_button(
                "refresh-history",
                IconName::RotateCw,
                "Refresh history",
                Some(history.refresh_loading()),
            )
            .disabled(history.loading() && !history.refresh_loading().active())
            .on_click(
                cx.listener(|this, _, _, cx| this.history.update(cx, |view, cx| view.refresh(cx))),
            )
        } else {
            icon_button("refresh-changes", IconName::RotateCw, "Refresh changes")
                .disabled(disabled)
                .on_click(
                    cx.listener(|this, _, _, cx| this.state.update(cx, |s, cx| s.refresh(cx))),
                )
        };
        panel_header()
            .children(branch.map(branch_label))
            .child(div().flex_1())
            .child(segmented_control(
                "changes-view",
                "Changes view",
                &[(false, "Changes"), (true, "History")],
                self.show_history,
                move |history, _, cx| {
                    let _ = this.update(cx, |this, cx| this.select_view(history, cx));
                },
            ))
            .child(refresh)
    }
}
impl Render for ChangesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let opacity = self.content.value(now);
        motion::request_frame(window, self.content.is_animating(now));
        let state = self.state.read(cx);
        let data = state.data.clone();
        let disabled = self.inactive(cx) || state.busy || cx.global::<AppState>().git.read(cx).busy;
        let files = data
            .as_ref()
            .map(|s| s.files.as_slice())
            .unwrap_or_default();
        let commit_path = state.path.clone();
        let staged = files.iter().filter(|f| f.staged).count();
        let tracking = data.as_ref().and_then(|d| d.upstream.clone());
        let signing = data
            .as_ref()
            .map(|d| {
                if d.signing.enabled {
                    format!("Signed · {}", d.signing.format)
                } else {
                    "Unsigned".into()
                }
            })
            .unwrap_or_default();
        let header = self.header(disabled, cx);
        let state = self.state.read(cx);

        let composer = (!self.show_history).then(|| {
            column()
                .w_full()
                .gap(px(6.))
                .p(px(8.))
                .pb(px(6.))
                .rounded(px(8.))
                .border_1()
                .border_color(t::border())
                .bg(t::input_bg())
                .child(
                    textarea(&self.message)
                        .appearance(false)
                        .h(px(44.))
                        .w_full()
                        .p_0()
                        .disabled(disabled),
                )
                .child(
                    row()
                        .gap(px(6.))
                        .child(
                            row()
                                .flex_1()
                                .min_w_0()
                                .gap(px(4.))
                                .text_size(px(11.))
                                .text_color(t::faint())
                                .children((!state.committing).then(|| {
                                    custom_icon("shield")
                                        .size(px(12.))
                                        .text_color(t::faint())
                                        .flex_shrink_0()
                                }))
                                .child(div().min_w_0().truncate().child(if state.committing {
                                    state
                                        .commit_phase
                                        .clone()
                                        .unwrap_or_else(|| "Committing…".into())
                                } else {
                                    signing
                                })),
                        )
                        .children(state.committing.then(|| {
                            button("cancel-commit", "Cancel").h(px(24.)).on_click(
                                cx.listener(|this, _, _, cx| this.state.read(cx).cancel()),
                            )
                        }))
                        .child(
                            primary_loading_button(
                                "commit-staged",
                                match staged {
                                    0 => "Commit".to_owned(),
                                    1 => "Commit 1 file".to_owned(),
                                    n => format!("Commit {n} files"),
                                },
                                &self.commit_loading,
                            )
                            .h(px(24.))
                            .disabled(
                                (disabled
                                    || staged == 0
                                    || self.message.read(cx).value().trim().is_empty())
                                    && !self.commit_loading.active(),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if this.state.read(cx).path != commit_path {
                                        return;
                                    }
                                    let message = this.message.read(cx).value().to_string();
                                    this.error = this
                                        .state
                                        .update(cx, |s, cx| s.commit(message, cx))
                                        .err()
                                        .map(|e| e.to_string());
                                    cx.notify();
                                },
                            )),
                        ),
                )
        });
        let sync = row()
            .gap(px(6.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(t::MONO)
                    .text_size(px(11.))
                    .text_color(t::faint())
                    .child(
                        tracking
                            .as_ref()
                            .map(|tracking| tracking.name.clone())
                            .unwrap_or_else(|| "No upstream".into()),
                    ),
            )
            .children([Operation::Pull, Operation::Push].map(|operation| {
                let feedback = match operation {
                    Operation::Pull => &self.pull_loading,
                    Operation::Push => &self.push_loading,
                };
                let pending = tracking.as_ref().map_or(0, |tracking| match operation {
                    Operation::Pull => tracking.behind,
                    Operation::Push => tracking.ahead,
                });
                git_network::transfer_button(operation, feedback, pending)
                    .h(px(24.))
                    .disabled((disabled || state.path.is_none()) && !feedback.active())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.error = this
                            .state
                            .update(cx, |state, cx| state.network(operation, None, cx))
                            .err()
                            .map(|e| e.to_string());
                        cx.notify();
                    }))
            }));
        let footer = panel_footer()
            .children(composer)
            .child(sync)
            .children(super::pull_request::strip(cx))
            .children(self.error.clone().or_else(|| state.error.clone()).map(|e| {
                div()
                    .id("changes-operation-error")
                    .max_h(px(120.))
                    .overflow_y_scroll()
                    .text_size(px(11.))
                    .text_color(t::red())
                    .child(e)
            }))
            .children(
                state
                    .commit_warning
                    .as_ref()
                    .filter(|(path, _)| Some(path) == state.path.as_ref())
                    .map(|(_, warning)| {
                        div()
                            .id("commit-hook-warning")
                            .max_h(px(120.))
                            .overflow_y_scroll()
                            .text_size(px(11.))
                            .text_color(t::yellow())
                            .child(warning.clone())
                    }),
            )
            .children(
                state
                    .commit_output
                    .as_ref()
                    .filter(|(path, _)| Some(path) == state.path.as_ref())
                    .map(|(_, output)| {
                        column()
                            .gap(px(6.))
                            .child(
                                button(
                                    "commit-hook-output",
                                    if self.show_hook_output {
                                        "Hide hook output"
                                    } else {
                                        "Show hook output"
                                    },
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.show_hook_output = !this.show_hook_output;
                                        cx.notify();
                                    },
                                )),
                            )
                            .children(self.show_hook_output.then(|| {
                                div()
                                    .id("hook-output-scroll")
                                    .max_h(px(120.))
                                    .overflow_y_scroll()
                                    .text_size(px(11.))
                                    .font_family(t::MONO)
                                    .child(output.clone())
                            }))
                    }),
            );
        if self.show_history {
            return column()
                .size_full()
                .child(header)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .opacity(opacity)
                        .child(self.history.clone()),
                )
                .child(footer)
                .into_any_element();
        }
        let (added, deleted) = data
            .as_ref()
            .map(|d| {
                d.lines
                    .iter()
                    .flatten()
                    .fold((0, 0), |(a, d), (added, deleted)| (a + added, d + deleted))
            })
            .unwrap_or_default();
        let repository = state
            .path
            .as_ref()
            .is_some_and(|p| cx.global::<AppState>().git.read(cx).repository(p).is_some());
        let body: AnyElement = if data.is_none() {
            panel_empty(
                if repository {
                    "Loading changes…"
                } else {
                    "No Git repository selected"
                },
                if repository {
                    "Reading the working tree."
                } else {
                    "Open a project that is a Git repository to see its changes."
                },
            )
            .into_any_element()
        } else if files.is_empty() {
            panel_empty(
                "No uncommitted changes",
                "Edits in this worktree will show up here.",
            )
            .into_any_element()
        } else {
            column()
                .flex_1()
                .min_h_0()
                .child(
                    row()
                        .flex_shrink_0()
                        .gap(px(8.))
                        .px(px(PANEL_INSET))
                        .pb(px(8.))
                        .child(
                            row()
                                .flex_shrink_0()
                                .gap(px(4.))
                                .text_size(px(11.))
                                .text_color(t::faint())
                                .child(format!(
                                    "{} file{}",
                                    files.len(),
                                    if files.len() == 1 { "" } else { "s" }
                                ))
                                .children(
                                    (added + deleted > 0).then(|| line_counts(added, deleted)),
                                ),
                        )
                        .children((added + deleted > 0).then(|| line_bar(added, deleted))),
                )
                .child(
                    row()
                        .flex_shrink_0()
                        .gap(px(6.))
                        .px(px(PANEL_INSET))
                        .pb(px(6.))
                        .child(
                            input(&self.filter)
                                .h(px(26.))
                                .text_size(px(12.))
                                .flex_1()
                                .disabled(self.inactive(cx)),
                        )
                        .child(row().flex_shrink_0().gap(px(3.)).children(
                            ["A", "M", "D", "R"].into_iter().map(|kind| {
                                let selected = self.kind.as_deref() == Some(kind);
                                filter_chip(
                                    SharedString::from(format!("changes-kind-{kind}")),
                                    kind,
                                    selected,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.kind = (!selected).then(|| kind.into());
                                        this.rebuild(cx);
                                        cx.notify();
                                    },
                                ))
                            }),
                        )),
                )
                .child(
                    uniform_list(
                        "changed-files-scroll",
                        self.rows.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            range.map(|i| this.list_row(i, cx)).collect::<Vec<_>>()
                        }),
                    )
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .px(px(LIST_INSET)),
                )
                .into_any_element()
        };
        column()
            .size_full()
            .child(header)
            .child(column().flex_1().min_h_0().opacity(opacity).child(body))
            .child(footer)
            .into_any_element()
    }
}
